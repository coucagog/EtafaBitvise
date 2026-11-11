//! Clés d'hôte connues, au format `known_hosts` d'OpenSSH, dans le dossier de l'application.

use std::io;
use std::path::PathBuf;

use russh::keys::{parse_public_key_base64, HashAlg, PublicKey};
use serde::Serialize;

use crate::fichiers::ecrire_atomique;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Empreinte {
    pub algorithme: String,
    pub empreinte: String,
}

pub fn empreinte(cle: &PublicKey) -> Empreinte {
    Empreinte {
        algorithme: cle.algorithm().as_str().to_string(),
        empreinte: cle.fingerprint(HashAlg::Sha256).to_string(),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verification {
    Connue,
    Inconnue,
    /// L'hôte a des clés enregistrées et aucune ne correspond.
    Modifiee(Vec<Empreinte>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoteConnu {
    pub motif: String,
    pub hote: String,
    pub port: u16,
    pub algorithme: String,
    pub empreinte: String,
}

pub struct MagasinHotes {
    chemin: PathBuf,
}

impl MagasinHotes {
    pub fn new(chemin: PathBuf) -> Self {
        Self { chemin }
    }

    fn motif(hote: &str, port: u16) -> String {
        if port == 22 {
            hote.to_string()
        } else {
            format!("[{hote}]:{port}")
        }
    }

    fn lire(&self) -> io::Result<String> {
        match std::fs::read_to_string(&self.chemin) {
            Ok(texte) => Ok(texte),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e),
        }
    }

    fn cles_de(texte: &str, motif: &str) -> Vec<PublicKey> {
        texte
            .lines()
            .filter_map(|ligne| {
                let mut champs = ligne.split_whitespace();
                let hotes = champs.next()?;
                let _algorithme = champs.next()?;
                let base64 = champs.next()?;
                hotes.split(',').any(|h| h == motif).then(|| parse_public_key_base64(base64).ok())?
            })
            .collect()
    }

    /// Une erreur de lecture remonte à l'appelant, qui doit refuser la connexion.
    pub fn verifier(&self, hote: &str, port: u16, cle: &PublicKey) -> io::Result<Verification> {
        let enregistrees = Self::cles_de(&self.lire()?, &Self::motif(hote, port));
        Ok(if enregistrees.is_empty() {
            Verification::Inconnue
        } else if enregistrees.iter().any(|e| e.key_data() == cle.key_data()) {
            Verification::Connue
        } else {
            Verification::Modifiee(enregistrees.iter().map(empreinte).collect())
        })
    }

    fn ligne(motif: &str, cle: &PublicKey) -> io::Result<String> {
        let openssh = cle.to_openssh().map_err(|e| io::Error::other(e.to_string()))?;
        Ok(format!("{motif} {}\n", openssh.trim_end()))
    }

    pub fn enregistrer(&self, hote: &str, port: u16, cle: &PublicKey) -> io::Result<()> {
        let mut texte = self.lire()?;
        if !texte.is_empty() && !texte.ends_with('\n') {
            texte.push('\n');
        }
        texte.push_str(&Self::ligne(&Self::motif(hote, port), cle)?);
        ecrire_atomique(&self.chemin, texte.as_bytes())
    }

    /// Retire toutes les clés de cet hôte puis enregistre la nouvelle.
    pub fn remplacer(&self, hote: &str, port: u16, cle: &PublicKey) -> io::Result<()> {
        let motif = Self::motif(hote, port);
        let mut texte: String = self
            .lire()?
            .lines()
            .filter(|l| l.split_whitespace().next() != Some(motif.as_str()))
            .flat_map(|l| [l, "\n"])
            .collect();
        texte.push_str(&Self::ligne(&motif, cle)?);
        ecrire_atomique(&self.chemin, texte.as_bytes())
    }
}

fn lire_ligne(ligne: &str) -> Option<(&str, PublicKey)> {
    let mut champs = ligne.split_whitespace();
    let motif = champs.next()?;
    let _algorithme = champs.next()?;
    Some((motif, parse_public_key_base64(champs.next()?).ok()?))
}

fn separer_motif(motif: &str) -> (String, u16) {
    motif
        .strip_prefix('[')
        .and_then(|m| m.rsplit_once("]:"))
        .and_then(|(hote, port)| Some((hote.to_string(), port.parse().ok()?)))
        .unwrap_or_else(|| (motif.to_string(), 22))
}

impl MagasinHotes {
    pub fn lister(&self) -> io::Result<Vec<HoteConnu>> {
        let mut hotes: Vec<HoteConnu> = self
            .lire()?
            .lines()
            .filter_map(lire_ligne)
            .map(|(motif, cle)| {
                let (hote, port) = separer_motif(motif);
                let e = empreinte(&cle);
                HoteConnu { motif: motif.to_string(), hote, port, algorithme: e.algorithme, empreinte: e.empreinte }
            })
            .collect();
        hotes.sort_by(|a, b| (&a.hote, a.port).cmp(&(&b.hote, b.port)));
        Ok(hotes)
    }

    /// Ne retire que la clé désignée ; les autres lignes, même illisibles, sont conservées.
    pub fn supprimer(&self, motif: &str, empreinte_cle: &str) -> io::Result<bool> {
        let texte = self.lire()?;
        let mut retiree = false;
        let mut sortie = String::with_capacity(texte.len());
        for ligne in texte.lines() {
            if !retiree && lire_ligne(ligne).is_some_and(|(m, c)| m == motif && empreinte(&c).empreinte == empreinte_cle) {
                retiree = true;
            } else {
                sortie.push_str(ligne);
                sortie.push('\n');
            }
        }
        if retiree {
            ecrire_atomique(&self.chemin, sortie.as_bytes())?;
        }
        Ok(retiree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::{Algorithm, PrivateKey};

    fn cle() -> PublicKey {
        PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap().public_key().clone()
    }

    fn magasin(nom: &str) -> MagasinHotes {
        let chemin = std::env::temp_dir().join(format!("mc-hotes-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_file(&chemin);
        MagasinHotes::new(chemin)
    }

    #[test]
    fn cycle_inconnue_connue_modifiee_remplacee() {
        let m = magasin("cycle");
        let (a, b) = (cle(), cle());
        assert_eq!(m.verifier("h", 22, &a).unwrap(), Verification::Inconnue);
        m.enregistrer("h", 22, &a).unwrap();
        assert_eq!(m.verifier("h", 22, &a).unwrap(), Verification::Connue);
        assert_eq!(m.verifier("h", 22, &b).unwrap(), Verification::Modifiee(vec![empreinte(&a)]));
        m.remplacer("h", 22, &b).unwrap();
        assert_eq!(m.verifier("h", 22, &b).unwrap(), Verification::Connue);
        assert_eq!(m.verifier("h", 22, &a).unwrap(), Verification::Modifiee(vec![empreinte(&b)]));
    }

    #[test]
    fn le_port_distingue_les_hotes() {
        let m = magasin("port");
        let a = cle();
        m.enregistrer("h", 2222, &a).unwrap();
        assert_eq!(m.verifier("h", 22, &a).unwrap(), Verification::Inconnue);
        assert_eq!(m.verifier("h", 2222, &a).unwrap(), Verification::Connue);
    }

    #[test]
    fn remplacer_garde_les_autres_hotes() {
        let m = magasin("autres");
        let (a, b, c) = (cle(), cle(), cle());
        m.enregistrer("h1", 22, &a).unwrap();
        m.enregistrer("h2", 22, &b).unwrap();
        m.remplacer("h1", 22, &c).unwrap();
        assert_eq!(m.verifier("h2", 22, &b).unwrap(), Verification::Connue);
    }

    #[test]
    fn liste_avec_ports_et_oubli_cible() {
        let m = magasin("liste");
        let (a, b, c) = (cle(), cle(), cle());
        m.enregistrer("h1", 22, &a).unwrap();
        m.enregistrer("::1", 2222, &b).unwrap();
        m.enregistrer("h1", 22, &c).unwrap();
        let liste = m.lister().unwrap();
        assert_eq!(liste.len(), 3);
        let v6 = liste.iter().find(|h| h.port == 2222).unwrap();
        assert_eq!((v6.hote.as_str(), v6.motif.as_str()), ("::1", "[::1]:2222"));

        assert!(m.supprimer("h1", &empreinte(&a).empreinte).unwrap());
        assert_eq!(m.lister().unwrap().len(), 2);
        assert_eq!(m.verifier("h1", 22, &c).unwrap(), Verification::Connue);
        assert!(!m.supprimer("h1", &empreinte(&a).empreinte).unwrap(), "déjà retirée");
    }

    #[test]
    fn oubli_garde_les_lignes_illisibles() {
        let m = magasin("illisible");
        let a = cle();
        m.enregistrer("h", 22, &a).unwrap();
        let mut texte = std::fs::read_to_string(&m.chemin).unwrap();
        texte.push_str("# note de l'utilisateur\n");
        std::fs::write(&m.chemin, &texte).unwrap();
        assert!(m.supprimer("h", &empreinte(&a).empreinte).unwrap());
        assert_eq!(std::fs::read_to_string(&m.chemin).unwrap(), "# note de l'utilisateur\n");
    }
}
