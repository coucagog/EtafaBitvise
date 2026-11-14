//! Approbation, dans l'application ouverte, d'une commande demandée par un agent (niveau 2).
//!
//! L'application écoute sur la boucle locale et dépose son port et un jeton dans son dossier de
//! données. `mc-mcp` y envoie la commande, et attend le choix de l'utilisateur. La commande n'est
//! exécutée que par `mc-mcp`, après un accord : l'application ne fait qu'afficher et répondre.
//! Le jeton écarte les demandes d'un autre compte du poste. Voir `Doc-Ref/08-acces-agent.md`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

pub const FICHIER: &str = "agent-approbation.json";
/// Temps laissé à l'utilisateur pour répondre.
pub const DELAI_REPONSE: Duration = Duration::from_secs(120);
const TAILLE_DEMANDE_MAX: u64 = 16 * 1024;
const FERMEE: &str = "MLS Connect doit être ouvert pour approuver une commande de niveau 2 : ouvrez l'application, puis redemandez.";

#[derive(Serialize, Deserialize)]
struct RendezVous {
    port: u16,
    jeton: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Demande {
    pub profil: String,
    pub serveur: String,
    pub commande: String,
}

#[derive(Serialize, Deserialize)]
struct Message {
    jeton: String,
    #[serde(flatten)]
    demande: Demande,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Approuvee,
    Refusee,
    /// L'utilisateur n'a pas répondu à temps.
    Expiree,
}

#[derive(Serialize, Deserialize)]
struct Reponse {
    verdict: Verdict,
}

/// Demande l'accord de l'utilisateur à l'application ouverte. `Err` : aucune réponse possible.
pub async fn demander(dossier: &Path, demande: &Demande) -> Result<Verdict, String> {
    let texte = tokio::fs::read_to_string(dossier.join(FICHIER)).await.map_err(|_| FERMEE.to_string())?;
    let rv: RendezVous = serde_json::from_str(&texte).map_err(|_| FERMEE.to_string())?;
    let flux = match timeout(Duration::from_secs(3), TcpStream::connect(("127.0.0.1", rv.port))).await {
        Ok(Ok(flux)) => flux,
        _ => return Err(FERMEE.into()),
    };
    // La moitié d'écriture reste ouverte jusqu'à la réponse : sa fermeture vaut un abandon.
    let (lecture, mut ecriture) = flux.into_split();
    let mut ligne = serde_json::to_string(&Message { jeton: rv.jeton, demande: demande.clone() }).map_err(|e| e.to_string())?;
    ligne.push('\n');
    ecriture.write_all(ligne.as_bytes()).await.map_err(|_| FERMEE.to_string())?;
    let mut reponse = String::new();
    let lu = timeout(DELAI_REPONSE + Duration::from_secs(10), BufReader::new(lecture.take(1024)).read_line(&mut reponse)).await;
    match lu {
        Err(_) => Ok(Verdict::Expiree),
        Ok(Ok(n)) if n > 0 => serde_json::from_str::<Reponse>(&reponse)
            .map(|r| r.verdict)
            .map_err(|_| "Réponse de MLS Connect illisible.".to_string()),
        Ok(_) => Err("MLS Connect a fermé la demande sans répondre : application fermée, ou trop de demandes en attente.".into()),
    }
}

/// Côté application : écoute locale, annoncée dans le dossier de données.
pub struct Guichet {
    ecoute: TcpListener,
    jeton: String,
    fichier: PathBuf,
}

fn ecrire_prive(chemin: &Path, texte: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(chemin)?.write_all(texte.as_bytes())
}

/// Comparaison sans sortie anticipée.
fn egaux(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

impl Guichet {
    pub async fn ouvrir(dossier: &Path) -> std::io::Result<Self> {
        let ecoute = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = ecoute.local_addr()?.port();
        let jeton = format!("{:032x}{:032x}", rand::random::<u128>(), rand::random::<u128>());
        std::fs::create_dir_all(dossier)?;
        let fichier = dossier.join(FICHIER);
        let _ = std::fs::remove_file(&fichier);
        ecrire_prive(&fichier, &serde_json::to_string(&RendezVous { port, jeton: jeton.clone() }).unwrap_or_default())?;
        Ok(Self { ecoute, jeton, fichier })
    }

    /// Sert les demandes sans fin. `decider` rend le choix de l'utilisateur ; son exécution est
    /// abandonnée si le demandeur ferme la connexion avant la réponse.
    pub async fn servir<F, Fut>(self, decider: F)
    where
        F: Fn(Demande) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Verdict> + Send + 'static,
    {
        let decider = Arc::new(decider);
        let jeton: Arc<str> = Arc::from(self.jeton.as_str());
        loop {
            match self.ecoute.accept().await {
                Ok((flux, _)) => {
                    let (decider, jeton) = (decider.clone(), jeton.clone());
                    tokio::spawn(async move { traiter(flux, &jeton, &*decider).await });
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    }
}

impl Drop for Guichet {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.fichier);
    }
}

async fn traiter<F, Fut>(flux: TcpStream, jeton: &str, decider: &F)
where
    F: Fn(Demande) -> Fut,
    Fut: Future<Output = Verdict>,
{
    let (lecture, mut ecriture) = flux.into_split();
    let mut lecture = BufReader::new(lecture.take(TAILLE_DEMANDE_MAX));
    let mut ligne = String::new();
    match timeout(Duration::from_secs(5), lecture.read_line(&mut ligne)).await {
        Ok(Ok(n)) if n > 0 && ligne.ends_with('\n') => {}
        _ => return,
    }
    let Ok(message) = serde_json::from_str::<Message>(&ligne) else { return };
    if !egaux(message.jeton.as_bytes(), jeton.as_bytes()) {
        return;
    }
    // Tout octet de plus, ou la fin du flux, vaut un abandon du demandeur.
    let mut suite = lecture.into_inner().into_inner();
    let mut octet = [0u8; 1];
    let verdict = tokio::select! {
        v = decider(message.demande) => v,
        _ = suite.read(&mut octet) => return,
    };
    if let Ok(mut reponse) = serde_json::to_string(&Reponse { verdict }) {
        reponse.push('\n');
        let _ = ecriture.write_all(reponse.as_bytes()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mc-approbation-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn demande(commande: &str) -> Demande {
        Demande { profil: "essai".into(), serveur: "u@10.0.0.1:22".into(), commande: commande.into() }
    }

    #[tokio::test]
    async fn accord_et_refus() {
        let d = dossier("accord");
        let guichet = Guichet::ouvrir(&d).await.unwrap();
        let service = tokio::spawn(guichet.servir(|dem: Demande| async move {
            assert_eq!(dem.profil, "essai");
            if dem.commande == "uptime" {
                Verdict::Approuvee
            } else {
                Verdict::Refusee
            }
        }));
        assert_eq!(demander(&d, &demande("uptime")).await, Ok(Verdict::Approuvee));
        assert_eq!(demander(&d, &demande("rm -rf /")).await, Ok(Verdict::Refusee));
        service.abort();
        let _ = service.await;
        // Le guichet fermé retire son annonce.
        assert!(!d.join(FICHIER).exists());
        assert!(demander(&d, &demande("uptime")).await.is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[tokio::test]
    async fn sans_application_ou_mauvais_jeton() {
        let d = dossier("jeton");
        assert!(demander(&d, &demande("uptime")).await.is_err());
        let guichet = Guichet::ouvrir(&d).await.unwrap();
        let service = tokio::spawn(guichet.servir(|_| async { Verdict::Approuvee }));
        // Annonce falsifiée : même port, autre jeton.
        let mut rv: RendezVous = serde_json::from_str(&std::fs::read_to_string(d.join(FICHIER)).unwrap()).unwrap();
        assert_eq!(rv.jeton.len(), 64);
        rv.jeton = "0".repeat(64);
        std::fs::write(d.join(FICHIER), serde_json::to_string(&rv).unwrap()).unwrap();
        assert!(demander(&d, &demande("uptime")).await.is_err());
        service.abort();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[tokio::test]
    async fn abandon_du_demandeur() {
        struct Temoin(Arc<AtomicBool>);
        impl Drop for Temoin {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let d = dossier("abandon");
        let abandonnee = Arc::new(AtomicBool::new(false));
        let recue = Arc::new(tokio::sync::Notify::new());
        let (a, r) = (abandonnee.clone(), recue.clone());
        let guichet = Guichet::ouvrir(&d).await.unwrap();
        let service = tokio::spawn(guichet.servir(move |_| {
            let (temoin, r) = (Temoin(a.clone()), r.clone());
            async move {
                let _temoin = temoin;
                r.notify_one();
                std::future::pending::<Verdict>().await
            }
        }));
        let dd = d.clone();
        let client = tokio::spawn(async move { demander(&dd, &demande("uptime")).await });
        recue.notified().await;
        client.abort();
        let _ = client.await;
        for _ in 0..100 {
            if abandonnee.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(abandonnee.load(Ordering::SeqCst), "la demande abandonnée n'a pas été retirée");
        service.abort();
        let _ = std::fs::remove_dir_all(&d);
    }
}
