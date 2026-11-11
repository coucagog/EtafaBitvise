//! Passerelle FTP locale vers la session SFTP, pour les logiciels qui ne parlent que FTP.
//!
//! FTP circule en clair : la passerelle n'écoute que sur 127.0.0.1, exige un mot de passe tiré
//! au sort à chaque ouverture et n'accepte que le mode passif (sans PORT, pas de rebond FTP).

use std::fmt::Display;
use std::io::{self, SeekFrom};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use russh_sftp::protocol::OpenFlags;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep, timeout};

use crate::sftp::{Entree, Genre, Sftp};

const UTILISATEUR: &str = "mc";
const LIGNE_MAX: u64 = 4096;
const CLIENTS_MAX: usize = 8;
const ECHECS_MAX: u8 = 3;
const TAILLE_TAMPON: usize = 128 * 1024;
const DELAI_INACTIF: Duration = Duration::from_secs(600);
const DELAI_DONNEES: Duration = Duration::from_secs(20);
const MOIS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccesFtp {
    pub port: u16,
    pub utilisateur: String,
    pub mot_de_passe: String,
}

/// La passerelle et ses clients s'arrêtent à la destruction de cette valeur.
pub struct Passerelle {
    pub acces: AccesFtp,
    tache: JoinHandle<()>,
}

impl Drop for Passerelle {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

impl Passerelle {
    /// `port` à 0 laisse le système choisir.
    pub async fn ouvrir(sftp: Arc<Sftp>, port: u16) -> io::Result<Self> {
        let ecoute = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
        let acces = AccesFtp {
            port: ecoute.local_addr()?.port(),
            utilisateur: UTILISATEUR.into(),
            mot_de_passe: format!("{:032x}", rand::random::<u128>()),
        };
        let secret = acces.mot_de_passe.clone();
        let tache = tokio::spawn(accepter(ecoute, sftp, secret));
        Ok(Self { acces, tache })
    }
}

async fn accepter(ecoute: TcpListener, sftp: Arc<Sftp>, secret: String) {
    let mut clients = JoinSet::new();
    loop {
        let Ok((flux, _)) = ecoute.accept().await else {
            sleep(Duration::from_millis(100)).await;
            continue;
        };
        while clients.try_join_next().is_some() {}
        if clients.len() < CLIENTS_MAX {
            clients.spawn(servir(flux, sftp.clone(), secret.clone()));
        }
    }
}

async fn servir(flux: TcpStream, sftp: Arc<Sftp>, secret: String) {
    let _ = flux.set_nodelay(true);
    let (lecture, ecriture) = flux.into_split();
    let mut lecture = BufReader::new(lecture);
    let dossier = sftp.accueil().await.map(|d| resoudre("/", &d)).unwrap_or_else(|_| "/".into());
    let mut client = Client {
        sftp,
        secret,
        ecriture,
        dossier,
        utilisateur_ok: false,
        authentifie: false,
        echecs: 0,
        passif: None,
        reprise: 0,
        a_renommer: None,
    };
    if client.repondre(220, "MLS Connect, passerelle FTP").await.is_err() {
        return;
    }
    while let Some(ligne) = lire_ligne(&mut lecture).await {
        let Ok(texte) = std::str::from_utf8(&ligne) else {
            if client.repondre(501, "Texte UTF-8 attendu.").await.is_err() {
                return;
            }
            continue;
        };
        let (commande, arg) = texte.split_once(' ').unwrap_or((texte, ""));
        if !matches!(client.traiter(&commande.to_ascii_uppercase(), arg).await, Ok(true)) {
            return;
        }
    }
}

/// Retire la fin de ligne, et les octets Telnet que certains clients placent devant ABOR.
async fn lire_ligne(lecture: &mut BufReader<OwnedReadHalf>) -> Option<Vec<u8>> {
    let mut ligne = Vec::new();
    timeout(DELAI_INACTIF, (&mut *lecture).take(LIGNE_MAX).read_until(b'\n', &mut ligne)).await.ok()?.ok()?;
    if ligne.last() != Some(&b'\n') {
        return None;
    }
    while matches!(ligne.last(), Some(b'\n' | b'\r')) {
        ligne.pop();
    }
    let debut = ligne.iter().position(u8::is_ascii).unwrap_or(ligne.len());
    ligne.drain(..debut);
    Some(ligne)
}

struct Client {
    sftp: Arc<Sftp>,
    secret: String,
    ecriture: OwnedWriteHalf,
    dossier: String,
    utilisateur_ok: bool,
    authentifie: bool,
    echecs: u8,
    passif: Option<TcpListener>,
    reprise: u64,
    a_renommer: Option<String>,
}

impl Client {
    async fn repondre(&mut self, code: u16, texte: &str) -> io::Result<()> {
        self.ecriture.write_all(format!("{code} {}\r\n", sans_controle(texte)).as_bytes()).await
    }

    async fn conclure(&mut self, resultat: Result<(), impl Display>, code: u16, texte: &str) -> io::Result<()> {
        match resultat {
            Ok(()) => self.repondre(code, texte).await,
            Err(e) => self.repondre(550, &e.to_string()).await,
        }
    }

    async fn ouvrir_passif(&mut self) -> io::Result<Option<u16>> {
        match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await {
            Ok(ecoute) => {
                let port = ecoute.local_addr()?.port();
                self.passif = Some(ecoute);
                Ok(Some(port))
            }
            Err(_) => {
                self.repondre(425, "Canal de données impossible à ouvrir.").await?;
                Ok(None)
            }
        }
    }

    async fn flux_donnees(&mut self) -> io::Result<Option<TcpStream>> {
        let Some(ecoute) = self.passif.take() else {
            self.repondre(425, "Utilisez PASV ou EPSV d'abord.").await?;
            return Ok(None);
        };
        self.repondre(150, "Canal de données ouvert.").await?;
        match timeout(DELAI_DONNEES, ecoute.accept()).await {
            Ok(Ok((flux, adresse))) if adresse.ip().is_loopback() => Ok(Some(flux)),
            _ => {
                self.repondre(425, "Aucune connexion de données reçue.").await?;
                Ok(None)
            }
        }
    }

    async fn envoyer_texte(&mut self, corps: &str) -> io::Result<()> {
        let Some(mut donnees) = self.flux_donnees().await? else { return Ok(()) };
        let envoi = async {
            donnees.write_all(corps.as_bytes()).await?;
            donnees.shutdown().await
        };
        self.fin_transfert(envoi.await).await
    }

    async fn fin_transfert(&mut self, resultat: io::Result<()>) -> io::Result<()> {
        match resultat {
            Ok(()) => self.repondre(226, "Transfert terminé.").await,
            Err(e) => self.repondre(451, &format!("Transfert interrompu : {e}")).await,
        }
    }

    async fn telecharger(&mut self, chemin: &str, reprise: u64) -> io::Result<()> {
        match decrire(&self.sftp, chemin).await {
            Ok(e) if matches!(e.genre, Genre::Dossier) => return self.repondre(550, "C'est un dossier.").await,
            Err(e) => return self.repondre(550, &e).await,
            Ok(_) => {}
        }
        let mut fichier = match self.sftp.s.open(chemin).await {
            Ok(f) => f,
            Err(e) => return self.repondre(550, &e.to_string()).await,
        };
        if reprise > 0 {
            if let Err(e) = fichier.seek(SeekFrom::Start(reprise)).await {
                return self.repondre(550, &e.to_string()).await;
            }
        }
        let Some(mut donnees) = self.flux_donnees().await? else { return Ok(()) };
        let resultat = copier(&mut fichier, &mut donnees).await;
        self.fin_transfert(resultat).await
    }

    async fn deposer(&mut self, chemin: &str, ajout: bool, reprise: u64) -> io::Result<()> {
        let ouverture = if ajout {
            self.sftp.s.open_with_flags(chemin, OpenFlags::WRITE | OpenFlags::APPEND | OpenFlags::CREATE).await
        } else if reprise > 0 {
            self.sftp.s.open_with_flags(chemin, OpenFlags::WRITE).await
        } else {
            self.sftp.s.create(chemin).await
        };
        let mut fichier = match ouverture {
            Ok(f) => f,
            Err(e) => return self.repondre(550, &e.to_string()).await,
        };
        if reprise > 0 && !ajout {
            if let Err(e) = fichier.seek(SeekFrom::Start(reprise)).await {
                return self.repondre(550, &e.to_string()).await;
            }
        }
        let Some(mut donnees) = self.flux_donnees().await? else { return Ok(()) };
        let resultat = copier(&mut donnees, &mut fichier).await;
        self.fin_transfert(resultat).await
    }

    /// Renvoie `false` quand la connexion doit se terminer.
    async fn traiter(&mut self, commande: &str, arg: &str) -> io::Result<bool> {
        // REST et RNFR ne valent que pour la commande qui les suit ;
        // certains clients placent toutefois PASV ou TYPE entre REST et le transfert.
        let reprise = if matches!(commande, "PASV" | "EPSV" | "TYPE") { self.reprise } else { std::mem::take(&mut self.reprise) };
        let origine = self.a_renommer.take();
        if arg.contains('\0') {
            self.repondre(501, "Argument invalide.").await?;
            return Ok(true);
        }
        if !self.authentifie && !matches!(commande, "USER" | "PASS" | "QUIT" | "FEAT" | "AUTH") {
            self.repondre(530, "Authentification requise.").await?;
            return Ok(true);
        }
        let sftp = self.sftp.clone();
        let chemin = resoudre(&self.dossier, arg);
        match commande {
            "USER" => {
                self.utilisateur_ok = arg == UTILISATEUR;
                self.authentifie = false;
                self.repondre(331, "Mot de passe requis.").await?;
            }
            "PASS" => {
                if self.utilisateur_ok && egal_constant(arg, &self.secret) {
                    self.authentifie = true;
                    self.repondre(230, "Connecté.").await?;
                } else {
                    self.echecs += 1;
                    sleep(Duration::from_secs(1)).await;
                    if self.echecs >= ECHECS_MAX {
                        self.repondre(421, "Trop d'échecs.").await?;
                        return Ok(false);
                    }
                    self.repondre(530, "Identifiants incorrects.").await?;
                }
            }
            "QUIT" => {
                self.repondre(221, "Au revoir.").await?;
                return Ok(false);
            }
            "FEAT" => {
                self.ecriture
                    .write_all(b"211-Extensions\r\n UTF8\r\n EPSV\r\n PASV\r\n REST STREAM\r\n SIZE\r\n MDTM\r\n MLST type*;size*;modify*;\r\n211 Fin\r\n")
                    .await?;
            }
            "SYST" => self.repondre(215, "UNIX Type: L8").await?,
            "NOOP" => self.repondre(200, "OK").await?,
            "ABOR" => self.repondre(225, "Aucun transfert en cours.").await?,
            "ALLO" => self.repondre(202, "Inutile ici.").await?,
            "OPTS" => self.repondre(200, "OK").await?,
            // Les fichiers sont transmis sans conversion, y compris en TYPE A.
            "TYPE" if matches!(arg.chars().next().map(|c| c.to_ascii_uppercase()), Some('A' | 'I')) => self.repondre(200, "OK").await?,
            "MODE" if arg.eq_ignore_ascii_case("S") => self.repondre(200, "OK").await?,
            "STRU" if arg.eq_ignore_ascii_case("F") => self.repondre(200, "OK").await?,
            "PWD" | "XPWD" => {
                let texte = format!("\"{}\"", self.dossier.replace('"', "\"\""));
                self.repondre(257, &texte).await?;
            }
            "CWD" | "XCWD" | "CDUP" | "XCUP" => {
                let cible = if commande.ends_with("UP") { resoudre(&self.dossier, "..") } else { chemin };
                match decrire(&sftp, &cible).await {
                    Ok(e) if matches!(e.genre, Genre::Dossier) => {
                        self.dossier = cible;
                        self.repondre(250, "Dossier changé.").await?;
                    }
                    Ok(_) => self.repondre(550, "Ce n'est pas un dossier.").await?,
                    Err(e) => self.repondre(550, &e).await?,
                }
            }
            "PASV" => {
                if let Some(port) = self.ouvrir_passif().await? {
                    self.repondre(227, &format!("Mode passif (127,0,0,1,{},{}).", port >> 8, port & 0xff)).await?;
                }
            }
            "EPSV" if arg.eq_ignore_ascii_case("ALL") => self.repondre(200, "OK").await?,
            "EPSV" if !arg.is_empty() && arg != "1" => self.repondre(522, "IPv4 seulement (1).").await?,
            "EPSV" => {
                if let Some(port) = self.ouvrir_passif().await? {
                    self.repondre(229, &format!("Mode passif (|||{port}|)")).await?;
                }
            }
            "REST" => match arg.parse() {
                Ok(n) => {
                    self.reprise = n;
                    self.repondre(350, "Reprise enregistrée.").await?;
                }
                Err(_) => self.repondre(501, "Position invalide.").await?,
            },
            "LIST" | "NLST" | "MLSD" => {
                let cible = if commande == "MLSD" { chemin } else { resoudre(&self.dossier, sans_options(arg)) };
                match sftp.lister(&cible).await {
                    Ok(entrees) => self.envoyer_texte(&corps_liste(commande, &entrees)).await?,
                    Err(e) => self.repondre(550, &e).await?,
                }
            }
            "MLST" => match decrire(&sftp, &chemin).await {
                Ok(e) => {
                    let texte = format!("250-Détails\r\n {} {}\r\n250 Fin\r\n", faits(&e), sans_controle(&chemin));
                    self.ecriture.write_all(texte.as_bytes()).await?;
                }
                Err(e) => self.repondre(550, &e).await?,
            },
            "SIZE" => match decrire(&sftp, &chemin).await {
                Ok(e) if matches!(e.genre, Genre::Dossier) => self.repondre(550, "C'est un dossier.").await?,
                Ok(e) => self.repondre(213, &e.taille.to_string()).await?,
                Err(e) => self.repondre(550, &e).await?,
            },
            "MDTM" => match decrire(&sftp, &chemin).await.map(|e| e.modifie) {
                Ok(Some(t)) => self.repondre(213, &date_mlsd(t)).await?,
                Ok(None) => self.repondre(550, "Date inconnue.").await?,
                Err(e) => self.repondre(550, &e).await?,
            },
            "RETR" => self.telecharger(&chemin, reprise).await?,
            "STOR" => self.deposer(&chemin, false, reprise).await?,
            "APPE" => self.deposer(&chemin, true, 0).await?,
            "DELE" => self.conclure(sftp.s.remove_file(chemin).await, 250, "Fichier supprimé.").await?,
            "RMD" | "XRMD" => self.conclure(sftp.s.remove_dir(chemin).await, 250, "Dossier supprimé.").await?,
            "MKD" | "XMKD" => {
                let texte = format!("\"{}\" créé.", chemin.replace('"', "\"\""));
                self.conclure(sftp.s.create_dir(chemin).await, 257, &texte).await?;
            }
            "RNFR" => match decrire(&sftp, &chemin).await {
                Ok(_) => {
                    self.a_renommer = Some(chemin);
                    self.repondre(350, "Indiquez le nouveau nom.").await?;
                }
                Err(e) => self.repondre(550, &e).await?,
            },
            "RNTO" => match origine {
                Some(ancien) => self.conclure(sftp.s.rename(ancien, chemin).await, 250, "Renommé.").await?,
                None => self.repondre(503, "RNFR doit précéder RNTO.").await?,
            },
            // PORT et EPRT feraient se connecter la passerelle à une adresse choisie par le client.
            "PORT" | "EPRT" => self.repondre(502, "Mode actif refusé : utilisez le mode passif.").await?,
            _ => self.repondre(502, "Commande non prise en charge.").await?,
        }
        Ok(true)
    }
}

async fn copier<L, E>(source: &mut L, cible: &mut E) -> io::Result<()>
where
    L: AsyncRead + Unpin,
    E: AsyncWrite + Unpin,
{
    let mut tampon = vec![0; TAILLE_TAMPON];
    loop {
        let n = source.read(&mut tampon).await?;
        if n == 0 {
            return cible.shutdown().await;
        }
        cible.write_all(&tampon[..n]).await?;
    }
}

/// Suit les liens, comme un client FTP s'y attend pour CWD, SIZE ou RETR.
async fn decrire(sftp: &Sftp, chemin: &str) -> Result<Entree, String> {
    let meta = sftp.s.metadata(chemin).await.map_err(|e| e.to_string())?;
    let type_fichier = meta.file_type();
    let genre = if type_fichier.is_dir() {
        Genre::Dossier
    } else if type_fichier.is_file() {
        Genre::Fichier
    } else {
        Genre::Autre
    };
    Ok(Entree {
        nom: chemin.rsplit('/').next().unwrap_or(chemin).to_string(),
        chemin: chemin.to_string(),
        genre,
        taille: meta.size.unwrap_or(0),
        modifie: meta.mtime.map(u64::from),
        mode: meta.permissions.map(|p| p & 0o7777),
    })
}

fn sans_controle(texte: &str) -> String {
    texte.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

fn egal_constant(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |ecart, (x, y)| ecart | (x ^ y)) == 0
}

/// Résolution textuelle : le résultat est toujours absolu et sans `.` ni `..`.
fn resoudre(dossier: &str, arg: &str) -> String {
    let brut = if arg.starts_with('/') { arg.to_string() } else { format!("{dossier}/{arg}") };
    let mut morceaux = Vec::new();
    for morceau in brut.split('/') {
        match morceau {
            "" | "." => {}
            ".." => {
                morceaux.pop();
            }
            m => morceaux.push(m),
        }
    }
    format!("/{}", morceaux.join("/"))
}

/// `LIST -la dossier` : les clients ajoutent des options de `ls`.
fn sans_options(arg: &str) -> &str {
    let mut reste = arg.trim_start();
    while reste.starts_with('-') {
        reste = reste.split_once(' ').map_or("", |(_, r)| r).trim_start();
    }
    reste
}

fn est_dossier(e: &Entree) -> bool {
    matches!(e.genre, Genre::Dossier | Genre::LienDossier)
}

fn corps_liste(commande: &str, entrees: &[Entree]) -> String {
    // Un nom contenant un saut de ligne fabriquerait de fausses lignes de liste.
    entrees
        .iter()
        .filter(|e| !matches!(e.genre, Genre::Autre) && !e.nom.chars().any(char::is_control))
        .map(|e| match commande {
            "NLST" => format!("{}\r\n", e.nom),
            "MLSD" => format!("{} {}\r\n", faits(e), e.nom),
            _ => format!("{}\r\n", ligne_ls(e)),
        })
        .collect()
}

fn faits(e: &Entree) -> String {
    let genre = if est_dossier(e) { "dir" } else { "file" };
    let date = e.modifie.map(|t| format!("modify={};", date_mlsd(t))).unwrap_or_default();
    format!("type={genre};size={};{date}", e.taille)
}

fn ligne_ls(e: &Entree) -> String {
    let dossier = est_dossier(e);
    let mode = e.mode.unwrap_or(if dossier { 0o755 } else { 0o644 });
    let droits: String = (0..9)
        .rev()
        .map(|i| if mode >> i & 1 == 1 { ['x', 'w', 'r'][i % 3] } else { '-' })
        .collect();
    let (annee, mois, jour, ..) = civil(e.modifie.unwrap_or(0));
    format!(
        "{}{droits} 1 ftp ftp {:>12} {} {jour:02}  {annee} {}",
        if dossier { 'd' } else { '-' },
        e.taille,
        MOIS[mois as usize - 1],
        e.nom
    )
}

fn date_mlsd(secondes: u64) -> String {
    let (a, mo, j, h, mi, s) = civil(secondes);
    format!("{a:04}{mo:02}{j:02}{h:02}{mi:02}{s:02}")
}

/// Date UTC depuis un temps Unix (algorithme « civil from days »).
fn civil(secondes: u64) -> (u64, u64, u64, u64, u64, u64) {
    let jours = secondes / 86_400 + 719_468;
    let reste = secondes % 86_400;
    let ere = jours / 146_097;
    let jour_ere = jours % 146_097;
    let annee_ere = (jour_ere - jour_ere / 1460 + jour_ere / 36_524 - jour_ere / 146_096) / 365;
    let jour_annee = jour_ere - (365 * annee_ere + annee_ere / 4 - annee_ere / 100);
    let mp = (5 * jour_annee + 2) / 153;
    let jour = jour_annee - (153 * mp + 2) / 5 + 1;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 };
    let annee = annee_ere + ere * 400 + u64::from(mois <= 2);
    (annee, mois, jour, reste / 3600, reste % 3600 / 60, reste % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entree(nom: &str, genre: Genre) -> Entree {
        Entree { nom: nom.into(), chemin: format!("/{nom}"), genre, taille: 42, modifie: Some(1_794_225_600), mode: Some(0o750) }
    }

    #[test]
    fn resolution_sans_sortie_de_la_racine() {
        assert_eq!(resoudre("/home/a", ""), "/home/a");
        assert_eq!(resoudre("/home/a", "b/./c"), "/home/a/b/c");
        assert_eq!(resoudre("/home/a", "../b"), "/home/b");
        assert_eq!(resoudre("/home/a", "/etc/"), "/etc");
        assert_eq!(resoudre("/", "../../.."), "/");
    }

    #[test]
    fn options_de_ls_retirees() {
        assert_eq!(sans_options("-la"), "");
        assert_eq!(sans_options("-l -a /srv/x"), "/srv/x");
        assert_eq!(sans_options("mon dossier"), "mon dossier");
    }

    #[test]
    fn dates_utc() {
        assert_eq!(date_mlsd(0), "19700101000000");
        assert_eq!(date_mlsd(951_782_400), "20000229000000");
        assert_eq!(date_mlsd(1_794_225_600), "20261109120000");
    }

    #[test]
    fn lignes_de_liste() {
        assert_eq!(ligne_ls(&entree("a.txt", Genre::Fichier)), format!("-rwxr-x--- 1 ftp ftp {:>12} Nov 09  2026 a.txt", 42));
        assert!(ligne_ls(&entree("d", Genre::LienDossier)).starts_with('d'));
        assert_eq!(faits(&entree("d", Genre::Dossier)), "type=dir;size=42;modify=20261109120000;");
    }

    #[test]
    fn un_nom_avec_saut_de_ligne_n_entre_pas_dans_la_liste() {
        let entrees = [entree("ok", Genre::Fichier), entree("x\r\nfaux", Genre::Fichier), entree("tube", Genre::Autre)];
        assert_eq!(corps_liste("NLST", &entrees), "ok\r\n");
    }

    #[test]
    fn comparaison_du_secret() {
        assert!(egal_constant("abc", "abc"));
        assert!(!egal_constant("abc", "abd"));
        assert!(!egal_constant("abc", "abcd"));
    }
}
