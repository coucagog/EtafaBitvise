//! Accès d'un agent (assistant d'IA, script) à un profil : niveaux, règles, commande unique.
//!
//! Les règles ci-dessous filtrent ce que l'agent demande. Elles ne remplacent pas les droits du
//! compte sur le serveur, seule vraie limite : voir `Doc-Ref/08-acces-agent.md`.

use std::time::{Duration, Instant};

use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::time::timeout;

use crate::connexion::Session;
use crate::sftp::Sftp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Niveau {
    /// Profil invisible pour un agent.
    #[default]
    Aucun,
    /// 0 : commandes de consultation d'une liste fermée, lecture de fichiers dans des dossiers choisis.
    Lecture,
    /// 1 : comme la lecture, avec les commandes ajoutées par l'utilisateur (redémarrer un service…).
    Exploitation,
    /// 2 : toute commande, chacune approuvée par l'utilisateur. Pas encore disponible.
    Encadre,
    /// 3 : toute commande et tout fichier, sans approbation. Pour des serveurs d'essai.
    Complet,
}

impl Niveau {
    pub fn numero(self) -> Option<u8> {
        match self {
            Niveau::Aucun => None,
            Niveau::Lecture => Some(0),
            Niveau::Exploitation => Some(1),
            Niveau::Encadre => Some(2),
            Niveau::Complet => Some(3),
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Niveau::Aucun => "aucun accès",
            Niveau::Lecture => "0 · lecture",
            Niveau::Exploitation => "1 · exploitation",
            Niveau::Encadre => "2 · shell encadré",
            Niveau::Complet => "3 · accès complet",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AccesAgent {
    pub niveau: Niveau,
    /// Commandes ajoutées à la liste de consultation, une par entrée : `systemctl restart nginx`.
    /// Une entrée autorise aussi ses arguments supplémentaires.
    pub commandes: Vec<String>,
    /// Dossiers où l'agent peut lister et lire. Vide : aucun, sauf au niveau 3.
    pub dossiers: Vec<String>,
}

/// Consultation seule, sans effet sur le serveur pour un compte sans droits d'administration.
/// Une entrée terminée par ` =` n'admet aucun argument de plus. Les fichiers se lisent par les outils
/// de lecture, limités aux dossiers ouverts : aucune commande de la liste ne prend un chemin de fichier.
pub const COMMANDES_LECTURE: &[&str] = &[
    "uptime",
    "hostname =",
    "uname",
    "whoami =",
    "id =",
    "date =",
    "df",
    "free",
    "lsblk",
    "nproc =",
    "lscpu =",
    "cat /etc/os-release =",
    "ip -brief address =",
    "ip -brief link =",
    "ip route =",
    "ss -tln =",
    "ss -uln =",
    "ps",
    "pgrep",
    "systemctl status",
    "systemctl is-active",
    "systemctl is-enabled",
    "systemctl is-failed",
    "systemctl list-units",
    "systemctl list-timers",
    "systemctl --failed =",
    "journalctl",
    "last",
    "who =",
    "w =",
];

/// Arguments qui donneraient un effet à une commande de consultation.
const ARGUMENTS_INTERDITS: &[&str] = &[
    "-f", "--follow", "-F", "--vacuum-size", "--vacuum-time", "--vacuum-files", "--rotate", "--flush", "--sync",
    "--relinquish-var", "--smart-relinquish-var", "--setup-keys", "--update-catalog", "-w", "--wtmp",
];

/// Tout ce qui permettrait d'enchaîner, de rediriger ou de substituer une commande dans le shell distant.
const CARACTERES_INTERDITS: &[char] = &[
    ';', '|', '&', '$', '`', '<', '>', '(', ')', '{', '}', '\\', '\'', '"', '*', '?', '[', ']', '~', '!', '#', '\n', '\r', '\t', '\0',
];

pub const TAILLE_SORTIE_MAX: usize = 64 * 1024;
pub const TAILLE_LECTURE_MAX: u64 = 256 * 1024;
pub const DELAI_DEFAUT: Duration = Duration::from_secs(30);
pub const DELAI_MAX: Duration = Duration::from_secs(600);

fn mots(texte: &str) -> Vec<&str> {
    texte.split(' ').filter(|m| !m.is_empty()).collect()
}

impl AccesAgent {
    pub fn visible(&self) -> bool {
        self.niveau != Niveau::Aucun
    }

    /// Commandes de départ admises, dans l'ordre : liste de consultation puis ajouts de l'utilisateur.
    pub fn commandes_admises(&self) -> Vec<String> {
        match self.niveau {
            Niveau::Lecture => COMMANDES_LECTURE.iter().map(|c| c.trim_end_matches(" =").to_string()).collect(),
            Niveau::Exploitation => COMMANDES_LECTURE
                .iter()
                .map(|c| c.trim_end_matches(" =").to_string())
                .chain(self.commandes.iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// `Ok(())` si la commande peut être envoyée au serveur, sinon la raison du refus.
    pub fn verifier_commande(&self, commande: &str) -> Result<(), String> {
        let commande = commande.trim();
        if commande.is_empty() {
            return Err("Commande vide.".into());
        }
        if commande.len() > 4096 {
            return Err("Commande trop longue (4 096 caractères au plus).".into());
        }
        match self.niveau {
            Niveau::Aucun => return Err("Ce profil n'est pas ouvert aux agents.".into()),
            Niveau::Encadre => {
                return Err("Le niveau 2 (approbation de chaque commande) n'est pas encore disponible.".into())
            }
            Niveau::Complet => return Ok(()),
            Niveau::Lecture | Niveau::Exploitation => {}
        }
        if let Some(c) = commande.chars().find(|c| CARACTERES_INTERDITS.contains(c) || c.is_control()) {
            return Err(format!(
                "Caractère « {} » refusé à ce niveau : une seule commande, sans redirection, enchaînement ni guillemets.",
                c.escape_default()
            ));
        }
        let demandes = mots(commande);
        // Les ajouts de l'utilisateur gardent leurs arguments ; la liste de consultation, non.
        let ajouts: Vec<String> =
            if self.niveau == Niveau::Exploitation { self.commandes.iter().map(|c| c.trim().to_string()).collect() } else { Vec::new() };
        let correspond = |modele: &str| {
            let (modele, exacte) = match modele.strip_suffix(" =") {
                Some(m) => (m, true),
                None => (modele, false),
            };
            let m = mots(modele);
            !m.is_empty()
                && demandes.len() >= m.len()
                && demandes[..m.len()] == m[..]
                && (!exacte || demandes.len() == m.len())
        };
        if ajouts.iter().any(|a| correspond(a)) {
            return Ok(());
        }
        if !COMMANDES_LECTURE.iter().any(|c| correspond(c)) {
            return Err(format!(
                "« {} » ne fait pas partie des commandes admises à ce niveau.",
                demandes.first().copied().unwrap_or_default()
            ));
        }
        if let Some(a) = demandes.iter().find(|m| {
            ARGUMENTS_INTERDITS.iter().any(|i| m == &i || m.starts_with(&format!("{i}=")))
        }) {
            return Err(format!("L'argument « {a} » est refusé à ce niveau."));
        }
        Ok(())
    }

    /// `chemin` doit être la forme canonique donnée par le serveur (liens et `..` résolus).
    pub fn verifier_chemin(&self, chemin: &str) -> Result<(), String> {
        match self.niveau {
            Niveau::Aucun => return Err("Ce profil n'est pas ouvert aux agents.".into()),
            Niveau::Complet => return Ok(()),
            _ => {}
        }
        if !chemin.starts_with('/') || chemin.split('/').any(|p| p == "..") {
            return Err("Chemin non canonique.".into());
        }
        let admis = self.dossiers.iter().map(|d| d.trim().trim_end_matches('/')).filter(|d| d.starts_with('/')).any(|d| {
            d.is_empty() || chemin == d || chemin.starts_with(&format!("{d}/"))
        });
        if admis {
            Ok(())
        } else if self.dossiers.is_empty() {
            Err("Aucun dossier n'est ouvert à l'agent sur ce profil.".into())
        } else {
            Err(format!("{chemin} est hors des dossiers ouverts à l'agent : {}.", self.dossiers.join(", ")))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resultat {
    pub code: Option<u32>,
    pub sortie: String,
    pub erreurs: String,
    /// La sortie a dépassé la taille maximale ; la suite a été ignorée.
    pub tronquee: bool,
    /// Le délai a expiré : le canal a été fermé avant la fin de la commande.
    pub expiree: bool,
    pub duree_ms: u64,
}

/// Texte affichable : caractères de contrôle et séquences d'échappement du terminal retirés.
pub fn texte_propre(octets: &[u8]) -> String {
    let brut = String::from_utf8_lossy(octets);
    let mut propre = String::with_capacity(brut.len());
    let mut caracteres = brut.chars().peekable();
    while let Some(c) = caracteres.next() {
        if c == '\u{1b}' {
            // CSI : ESC [ … lettre finale ; autres séquences : un caractère après ESC.
            if caracteres.peek() == Some(&'[') {
                caracteres.next();
                for s in caracteres.by_ref() {
                    if ('@'..='~').contains(&s) {
                        break;
                    }
                }
            } else {
                caracteres.next();
            }
        } else if !c.is_control() || c == '\n' || c == '\t' {
            propre.push(c);
        }
    }
    propre
}

fn ajouter(tampon: &mut Vec<u8>, donnees: &[u8], tronquee: &mut bool) {
    let place = TAILLE_SORTIE_MAX.saturating_sub(tampon.len());
    if donnees.len() > place {
        *tronquee = true;
    }
    tampon.extend_from_slice(&donnees[..donnees.len().min(place)]);
}

impl Session {
    /// Exécute une commande unique, sans terminal, et rend sa sortie bornée en taille et en durée.
    pub async fn executer(&self, commande: &str, delai: Duration) -> Result<Resultat, String> {
        // OpenSSH coupe toute la connexion sur un caractère nul dans la commande.
        if commande.contains('\0') {
            return Err("Caractère nul refusé dans une commande.".into());
        }
        let debut = Instant::now();
        let delai = delai.min(DELAI_MAX);
        let mut canal = match timeout(Duration::from_secs(15), self.poignee.channel_open_session()).await {
            Ok(Ok(canal)) => canal,
            Ok(Err(e)) => return Err(format!("Ouverture du canal impossible : {e}.")),
            Err(_) => return Err("Le serveur n'a pas répondu à l'ouverture du canal.".into()),
        };
        canal.exec(true, commande.as_bytes()).await.map_err(|e| format!("Envoi de la commande impossible : {e}."))?;
        let (mut sortie, mut erreurs) = (Vec::new(), Vec::new());
        let (mut code, mut tronquee, mut refusee) = (None, false, false);
        let lecture = async {
            while let Some(message) = canal.wait().await {
                match message {
                    ChannelMsg::Failure => {
                        refusee = true;
                        break;
                    }
                    ChannelMsg::Data { data } => ajouter(&mut sortie, &data, &mut tronquee),
                    ChannelMsg::ExtendedData { data, .. } => ajouter(&mut erreurs, &data, &mut tronquee),
                    ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
        };
        let expiree = timeout(delai, lecture).await.is_err();
        let _ = canal.close().await;
        if refusee {
            return Err("Le serveur a refusé l'exécution de la commande.".into());
        }
        Ok(Resultat {
            code,
            sortie: texte_propre(&sortie),
            erreurs: texte_propre(&erreurs),
            tronquee,
            expiree,
            duree_ms: debut.elapsed().as_millis() as u64,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Extrait {
    pub chemin: String,
    pub taille: u64,
    pub debut: u64,
    pub texte: String,
    /// Le fichier continue après cet extrait.
    pub suite: bool,
}

impl Sftp {
    /// Forme canonique d'un chemin distant : liens et `..` résolus par le serveur.
    pub async fn canonique(&self, chemin: &str) -> Result<String, String> {
        self.s.canonicalize(chemin).await.map_err(|e| e.to_string())
    }

    /// Lit au plus `taille` octets à partir de `debut`.
    pub async fn lire_extrait(&self, chemin: &str, debut: u64, taille: u64) -> Result<Extrait, String> {
        let taille = taille.clamp(1, TAILLE_LECTURE_MAX);
        let meta = self.s.metadata(chemin).await.map_err(|e| e.to_string())?;
        if meta.is_dir() {
            return Err(format!("{chemin} est un dossier."));
        }
        let total = meta.size.unwrap_or(0);
        let mut fichier = self.s.open(chemin).await.map_err(|e| e.to_string())?;
        if debut > 0 {
            fichier.seek(std::io::SeekFrom::Start(debut)).await.map_err(|e| e.to_string())?;
        }
        let mut octets = Vec::new();
        (&mut fichier).take(taille).read_to_end(&mut octets).await.map_err(|e| e.to_string())?;
        let lu = octets.len() as u64;
        Ok(Extrait { chemin: chemin.into(), taille: total, debut, texte: texte_propre(&octets), suite: debut + lu < total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acces(niveau: Niveau) -> AccesAgent {
        AccesAgent { niveau, commandes: vec!["systemctl restart nginx".into()], dossiers: vec!["/var/log".into(), "/etc/nginx/".into()] }
    }

    #[test]
    fn lecture_admet_la_consultation() {
        let a = acces(Niveau::Lecture);
        for c in ["uptime", "df -h", "systemctl status nginx", "journalctl -u ssh -n 50 --no-pager", "cat /etc/os-release", "ip -brief address"] {
            assert_eq!(a.verifier_commande(c), Ok(()), "{c}");
        }
    }

    #[test]
    fn lecture_refuse_le_reste() {
        let a = acces(Niveau::Lecture);
        for c in [
            "rm -rf /tmp/x",
            "uptime; reboot",
            "df && reboot",
            "ls $(id)",
            "ls `id`",
            "cat /etc/shadow",
            "cat /etc/os-release /etc/shadow",
            "systemctl restart nginx",
            "systemctl stop nginx",
            "journalctl --vacuum-size=1M",
            "journalctl -f",
            "tail -f /var/log/syslog",
            "echo x > /tmp/y",
            "ls\nreboot",
            "find / -delete",
            "ls /root",
            "head /etc/passwd",
            "hostname pirate",
            "ip route add 10.0.0.0/8 dev eth0",
            "last -f /var/log/btmp",
            "",
            "uptimex",
        ] {
            assert!(a.verifier_commande(c).is_err(), "{c:?} aurait dû être refusée");
        }
    }

    #[test]
    fn exploitation_ajoute_les_commandes_choisies() {
        let a = acces(Niveau::Exploitation);
        assert_eq!(a.verifier_commande("systemctl restart nginx"), Ok(()));
        assert!(a.verifier_commande("systemctl restart ssh").is_err());
        assert!(a.verifier_commande("systemctl restart nginx; reboot").is_err());
        assert!(acces(Niveau::Lecture).verifier_commande("systemctl restart nginx").is_err());
    }

    #[test]
    fn niveaux_sans_filtre_ou_sans_acces() {
        assert_eq!(acces(Niveau::Complet).verifier_commande("rm -rf /tmp/x && echo $HOME"), Ok(()));
        assert!(acces(Niveau::Aucun).verifier_commande("uptime").is_err());
        assert!(acces(Niveau::Encadre).verifier_commande("uptime").is_err());
    }

    #[test]
    fn chemins() {
        let a = acces(Niveau::Lecture);
        assert_eq!(a.verifier_chemin("/var/log"), Ok(()));
        assert_eq!(a.verifier_chemin("/var/log/syslog"), Ok(()));
        assert_eq!(a.verifier_chemin("/etc/nginx/nginx.conf"), Ok(()));
        assert!(a.verifier_chemin("/var/logs").is_err());
        assert!(a.verifier_chemin("/etc/shadow").is_err());
        assert!(a.verifier_chemin("/var/log/../../etc/shadow").is_err());
        assert!(a.verifier_chemin("var/log/x").is_err());
        assert!(AccesAgent { niveau: Niveau::Lecture, ..Default::default() }.verifier_chemin("/var/log").is_err());
        assert_eq!(acces(Niveau::Complet).verifier_chemin("/etc/shadow"), Ok(()));
    }

    #[test]
    fn texte_sans_echappements() {
        assert_eq!(texte_propre(b"\x1b[31mrouge\x1b[0m\r\nfin\x07"), "rouge\nfin");
    }

    #[test]
    fn profil_ancien_sans_acces() {
        let p: crate::profils::Profil = serde_json::from_str(r#"{"id":"a"}"#).unwrap();
        assert_eq!(p.agent.niveau, Niveau::Aucun);
        let p: crate::profils::Profil =
            serde_json::from_str(r#"{"id":"a","agent":{"niveau":"complet","dossiers":["/"]}}"#).unwrap();
        assert_eq!(p.agent.niveau, Niveau::Complet);
    }
}
