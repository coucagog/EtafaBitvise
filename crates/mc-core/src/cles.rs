//! Clés privées de l'application : un fichier OpenSSH par clé, avec sa clé publique à côté.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use russh::keys::ssh_key::LineEnding;
use russh::keys::{decode_secret_key, Algorithm, PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};

use crate::fichiers::{ecrire_atomique, ecrire_prive};
use crate::hotes::empreinte;

const NOM_MAX: usize = 64;
const COMMENTAIRE_MAX: usize = 128;
const TAILLE_CLE_MAX: u64 = 64 * 1024;
const RESERVES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3",
    "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeCle {
    NomInvalide,
    Existe,
    PhraseRequise,
    Format,
    Absente,
    Disque,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErreurCle {
    pub code: CodeCle,
    pub message: String,
}

impl ErreurCle {
    fn new(code: CodeCle, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl From<io::Error> for ErreurCle {
    fn from(e: io::Error) -> Self {
        Self::new(CodeCle::Disque, format!("Accès au disque impossible : {e}."))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GenreCle {
    Ed25519,
    Rsa,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoCle {
    pub nom: String,
    pub chemin: String,
    pub algorithme: String,
    pub empreinte: String,
    pub commentaire: String,
    pub protegee: bool,
    pub publique: String,
}

fn nom_valide(nom: &str) -> bool {
    let radical = nom.split('.').next().unwrap_or_default().to_ascii_uppercase();
    !nom.is_empty()
        && nom.len() <= NOM_MAX
        && !nom.starts_with('.')
        && !nom.ends_with('.')
        && !nom.ends_with(".pub")
        && !nom.ends_with(".tmp")
        && nom.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !RESERVES.contains(&radical.as_str())
}

fn verifier_nom(nom: &str) -> Result<(), ErreurCle> {
    if nom_valide(nom) {
        Ok(())
    } else {
        Err(ErreurCle::new(CodeCle::NomInvalide, "Le nom ne peut contenir que des lettres, des chiffres, « - », « _ » et « . »."))
    }
}

fn nom_depuis_fichier(source: &Path) -> String {
    nom_propre(&source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
}

/// Nom de clé valide tiré d'un nom quelconque.
pub fn nom_propre(brut: &str) -> String {
    let propre: String = brut
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') { c } else { '-' })
        .take(NOM_MAX - 4)
        .collect();
    if nom_valide(&propre) { propre } else { "cle-importee".to_string() }
}

fn format_erreur(e: impl std::fmt::Display) -> ErreurCle {
    ErreurCle::new(CodeCle::Format, format!("Opération sur la clé impossible : {e}."))
}

pub struct MagasinCles {
    dossier: PathBuf,
}

impl MagasinCles {
    pub fn new(dossier: PathBuf) -> Self {
        Self { dossier }
    }

    fn chemin(&self, nom: &str) -> PathBuf {
        self.dossier.join(nom)
    }

    pub fn dossier(&self) -> &Path {
        &self.dossier
    }

    /// Clé déjà présente, reconnue à son empreinte.
    pub fn par_empreinte(&self, empreinte_cle: &str) -> Result<Option<InfoCle>, ErreurCle> {
        Ok(self.lister()?.into_iter().find(|c| c.empreinte == empreinte_cle))
    }

    fn chemin_public(&self, nom: &str) -> PathBuf {
        self.dossier.join(format!("{nom}.pub"))
    }

    fn info(&self, nom: &str, cle: &PrivateKey) -> Result<InfoCle, ErreurCle> {
        // Le commentaire d'une clé chiffrée n'est lisible que dans son fichier public.
        let enregistree = fs::read_to_string(self.chemin_public(nom))
            .ok()
            .and_then(|t| PublicKey::from_openssh(t.trim()).ok())
            .filter(|p| p.key_data() == cle.public_key().key_data());
        let publique = enregistree.as_ref().unwrap_or(cle.public_key());
        let e = empreinte(cle.public_key());
        Ok(InfoCle {
            nom: nom.to_string(),
            chemin: self.chemin(nom).display().to_string(),
            algorithme: e.algorithme,
            empreinte: e.empreinte,
            commentaire: publique.comment().to_string(),
            protegee: cle.is_encrypted(),
            publique: publique.to_openssh().map_err(format_erreur)?,
        })
    }

    /// Une clé illisible n'empêche pas d'afficher les autres.
    pub fn lister(&self) -> Result<Vec<InfoCle>, ErreurCle> {
        let entrees = match fs::read_dir(&self.dossier) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut cles = Vec::new();
        for entree in entrees {
            let nom = entree?.file_name().to_string_lossy().into_owned();
            if !nom_valide(&nom) {
                continue;
            }
            let Ok(texte) = fs::read_to_string(self.chemin(&nom)) else { continue };
            if let Ok(cle) = PrivateKey::from_openssh(&texte) {
                cles.push(self.info(&nom, &cle)?);
            }
        }
        cles.sort_by(|a, b| a.nom.cmp(&b.nom));
        Ok(cles)
    }

    fn deposer(&self, nom: &str, cle: &PrivateKey, texte: &str) -> Result<InfoCle, ErreurCle> {
        ecrire_prive(&self.chemin(nom), texte.as_bytes())?;
        let publique = cle.public_key().to_openssh().map_err(format_erreur)?;
        ecrire_atomique(&self.chemin_public(nom), format!("{publique}\n").as_bytes())?;
        self.info(nom, cle)
    }

    /// Une phrase vide laisse la clé en clair : l'interface prévient l'utilisateur.
    pub fn generer(&self, nom: &str, genre: GenreCle, commentaire: &str, phrase: Option<&str>) -> Result<InfoCle, ErreurCle> {
        verifier_nom(nom)?;
        if self.chemin(nom).exists() {
            return Err(ErreurCle::new(CodeCle::Existe, format!("Une clé « {nom} » existe déjà.")));
        }
        let algorithme = match genre {
            GenreCle::Ed25519 => Algorithm::Ed25519,
            GenreCle::Rsa => Algorithm::Rsa { hash: None },
        };
        let mut cle = PrivateKey::random(&mut rand::rng(), algorithme).map_err(format_erreur)?;
        let commentaire: String = commentaire.chars().filter(|c| !c.is_control()).take(COMMENTAIRE_MAX).collect();
        cle.set_comment(commentaire);
        if let Some(phrase) = phrase.filter(|p| !p.is_empty()) {
            let chiffree = cle.encrypt(&mut rand::rng(), phrase).map_err(format_erreur)?;
            let texte = chiffree.to_openssh(LineEnding::LF).map_err(format_erreur)?;
            // Le fichier public est écrit depuis la clé en clair, seule à porter le commentaire.
            ecrire_prive(&self.chemin(nom), texte.as_bytes())?;
            let publique = cle.public_key().to_openssh().map_err(format_erreur)?;
            ecrire_atomique(&self.chemin_public(nom), format!("{publique}\n").as_bytes())?;
            return self.info(nom, &chiffree);
        }
        let texte = cle.to_openssh(LineEnding::LF).map_err(format_erreur)?;
        self.deposer(nom, &cle, &texte)
    }

    /// Conserve tel quel un fichier OpenSSH ; convertit un PEM, PKCS#8 ou PuTTY au format OpenSSH.
    pub fn importer(&self, source: &Path, phrase: Option<&str>) -> Result<InfoCle, ErreurCle> {
        if fs::metadata(source)?.len() > TAILLE_CLE_MAX {
            return Err(ErreurCle::new(CodeCle::Format, "Ce fichier est trop grand pour être une clé privée."));
        }
        let texte = fs::read_to_string(source)
            .map_err(|_| ErreurCle::new(CodeCle::Format, "Ce fichier n'est pas une clé privée lisible."))?;
        self.importer_contenu(&nom_depuis_fichier(source), &texte, phrase)
    }

    /// Comme `importer`, depuis le texte de la clé ; `base` donne le nom, rendu unique.
    pub fn importer_contenu(&self, base: &str, texte: &str, phrase: Option<&str>) -> Result<InfoCle, ErreurCle> {
        if texte.len() as u64 > TAILLE_CLE_MAX {
            return Err(ErreurCle::new(CodeCle::Format, "Ce texte est trop grand pour être une clé privée."));
        }
        let texte = texte.to_string();
        let phrase = phrase.filter(|p| !p.is_empty());
        if phrase.is_none() && ppk_protegee(&texte) {
            return Err(ErreurCle::new(CodeCle::PhraseRequise, "Cette clé est protégée : saisissez sa passphrase."));
        }
        let (cle, contenu) = match PrivateKey::from_openssh(&texte) {
            Ok(cle) => (cle, texte),
            Err(_) => {
                let claire = decode_secret_key(&texte, phrase).map_err(|e| match (&e, phrase) {
                    (russh::keys::Error::KeyIsEncrypted, None) => {
                        ErreurCle::new(CodeCle::PhraseRequise, "Cette clé est protégée : saisissez sa passphrase.")
                    }
                    _ => ErreurCle::new(CodeCle::Format, "Format non pris en charge, ou passphrase incorrecte. Formats acceptés : OpenSSH, PEM, PKCS#8, PuTTY (.ppk)."),
                })?;
                let sortie = match phrase {
                    Some(p) => claire.encrypt(&mut rand::rng(), p).map_err(format_erreur)?,
                    None => claire,
                };
                let contenu = sortie.to_openssh(LineEnding::LF).map_err(format_erreur)?.to_string();
                (sortie, contenu)
            }
        };

        let existantes = self.lister()?;
        let empreinte_cle = empreinte(cle.public_key()).empreinte;
        if let Some(doublon) = existantes.iter().find(|c| c.empreinte == empreinte_cle) {
            return Err(ErreurCle::new(CodeCle::Existe, format!("Cette clé est déjà présente sous le nom « {} ».", doublon.nom)));
        }
        let base = nom_propre(base);
        let nom = (1..1000)
            .map(|i| if i == 1 { base.clone() } else { format!("{base}-{i}") })
            .find(|n| !self.chemin(n).exists())
            .ok_or_else(|| ErreurCle::new(CodeCle::Existe, "Trop de clés portent déjà ce nom."))?;
        self.deposer(&nom, &cle, &contenu)
    }

    pub fn supprimer(&self, nom: &str) -> Result<(), ErreurCle> {
        verifier_nom(nom)?;
        match fs::remove_file(self.chemin(nom)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(ErreurCle::new(CodeCle::Absente, format!("La clé « {nom} » n'existe plus.")));
            }
            Err(e) => return Err(e.into()),
        }
        match fs::remove_file(self.chemin_public(nom)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    pub fn exporter_publique(&self, nom: &str, destination: &Path) -> Result<(), ErreurCle> {
        verifier_nom(nom)?;
        let cle = self
            .lister()?
            .into_iter()
            .find(|c| c.nom == nom)
            .ok_or_else(|| ErreurCle::new(CodeCle::Absente, format!("La clé « {nom} » n'existe plus.")))?;
        ecrire_atomique(destination, format!("{}\n", cle.publique.trim_end()).as_bytes())?;
        Ok(())
    }
}

/// Un fichier PuTTY annonce son chiffrement en clair, dans son en-tête.
fn ppk_protegee(texte: &str) -> bool {
    texte.trim_start().starts_with("PuTTY-User-Key-File-")
        && texte.lines().any(|l| l.strip_prefix("Encryption:").is_some_and(|v| v.trim() != "none"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Clés d'exemple publiques, reprises des tests de la bibliothèque ssh-key.
    const PHRASE_EXEMPLE: &str = "123";

    fn exemple(nom: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("donnees").join(nom)
    }

    #[test]
    fn importe_une_cle_putty_claire_v3_et_v2() {
        let m = magasin("ppk");
        let ed = m.importer(&exemple("id_ed25519.ppk"), None).unwrap();
        assert_eq!(ed.algorithme, "ssh-ed25519");
        assert!(!ed.protegee);
        assert!(decode_secret_key(&fs::read_to_string(&ed.chemin).unwrap(), None).is_ok());
        let rsa = m.importer(&exemple("id_rsa_3072.ppk2"), None).unwrap();
        assert!(rsa.algorithme.contains("rsa"), "{}", rsa.algorithme);
    }

    #[test]
    fn une_cle_putty_protegee_demande_sa_phrase_et_le_reste() {
        let m = magasin("ppk-enc");
        let source = exemple("id_ed25519_enc.ppk");
        assert_eq!(m.importer(&source, None).unwrap_err().code, CodeCle::PhraseRequise);
        assert_eq!(m.importer(&source, Some("mauvaise")).unwrap_err().code, CodeCle::Format);
        let info = m.importer(&source, Some(PHRASE_EXEMPLE)).unwrap();
        assert!(info.protegee);
        assert_eq!(info.algorithme, "ssh-ed25519");
        let texte = fs::read_to_string(&info.chemin).unwrap();
        assert!(!texte.contains("PuTTY"));
        assert!(decode_secret_key(&texte, Some(PHRASE_EXEMPLE)).is_ok());
    }

    fn magasin(nom: &str) -> MagasinCles {
        let dossier = std::env::temp_dir().join(format!("mc-cles-{}-{nom}", std::process::id()));
        let _ = fs::remove_dir_all(&dossier);
        MagasinCles::new(dossier)
    }

    #[test]
    fn genere_liste_et_retrouve_le_commentaire_d_une_cle_protegee() {
        let m = magasin("gen");
        let info = m.generer("perso", GenreCle::Ed25519, "moi@poste", Some("phrase")).unwrap();
        assert!(info.protegee);
        assert_eq!(info.algorithme, "ssh-ed25519");
        let liste = m.lister().unwrap();
        assert_eq!(liste, vec![info.clone()]);
        assert_eq!(liste[0].commentaire, "moi@poste");
        assert!(liste[0].publique.starts_with("ssh-ed25519 "));

        let texte = fs::read_to_string(&info.chemin).unwrap();
        assert!(decode_secret_key(&texte, Some("mauvaise")).is_err());
        let claire = decode_secret_key(&texte, Some("phrase")).unwrap();
        assert_eq!(empreinte(claire.public_key()).empreinte, info.empreinte);
    }

    #[test]
    fn cle_sans_phrase_utilisable_directement() {
        let m = magasin("clair");
        let info = m.generer("k", GenreCle::Ed25519, "", None).unwrap();
        assert!(!info.protegee);
        assert!(decode_secret_key(&fs::read_to_string(&info.chemin).unwrap(), None).is_ok());
    }

    #[test]
    fn noms_dangereux_refuses() {
        let m = magasin("noms");
        for nom in ["", "../x", "a/b", "a\\b", ".cache", "x.pub", "x.tmp", "NUL", "com1.txt", "é", "fin."] {
            assert_eq!(m.generer(nom, GenreCle::Ed25519, "", None).unwrap_err().code, CodeCle::NomInvalide, "{nom}");
        }
        assert_eq!(m.supprimer("../x").unwrap_err().code, CodeCle::NomInvalide);
    }

    #[test]
    fn un_nom_existant_n_est_jamais_ecrase() {
        let m = magasin("existe");
        let premiere = m.generer("k", GenreCle::Ed25519, "", None).unwrap();
        assert_eq!(m.generer("k", GenreCle::Ed25519, "", None).unwrap_err().code, CodeCle::Existe);
        assert_eq!(m.lister().unwrap(), vec![premiere]);
    }

    #[test]
    fn import_openssh_conserve_le_chiffrement_et_refuse_le_doublon() {
        let (source, cible) = (magasin("src"), magasin("dst"));
        let info = source.generer("id_test", GenreCle::Ed25519, "c", Some("p")).unwrap();
        let importee = cible.importer(Path::new(&info.chemin), None).unwrap();
        assert_eq!(importee.nom, "id_test");
        assert_eq!(importee.empreinte, info.empreinte);
        assert!(importee.protegee);
        assert_eq!(fs::read_to_string(&importee.chemin).unwrap(), fs::read_to_string(&info.chemin).unwrap());
        assert_eq!(cible.importer(Path::new(&info.chemin), None).unwrap_err().code, CodeCle::Existe);
    }

    #[test]
    fn import_prend_un_nom_libre() {
        let (a, b, cible) = (magasin("a"), magasin("b"), magasin("c2"));
        let (x, y) = (a.generer("k", GenreCle::Ed25519, "", None).unwrap(), b.generer("k", GenreCle::Ed25519, "", None).unwrap());
        assert_eq!(cible.importer(Path::new(&x.chemin), None).unwrap().nom, "k");
        assert_eq!(cible.importer(Path::new(&y.chemin), None).unwrap().nom, "k-2");
    }

    #[test]
    fn import_refuse_ce_qui_n_est_pas_une_cle() {
        let m = magasin("refus");
        let f = std::env::temp_dir().join(format!("mc-pas-une-cle-{}", std::process::id()));
        fs::write(&f, "bonjour").unwrap();
        assert_eq!(m.importer(&f, None).unwrap_err().code, CodeCle::Format);
        assert!(m.lister().unwrap().is_empty());
    }

    #[test]
    fn supprimer_retire_les_deux_fichiers_et_exporter_ecrit_la_publique() {
        let m = magasin("sup");
        let info = m.generer("k", GenreCle::Ed25519, "c", None).unwrap();
        let export = std::env::temp_dir().join(format!("mc-export-{}.pub", std::process::id()));
        m.exporter_publique("k", &export).unwrap();
        let texte = fs::read_to_string(&export).unwrap();
        assert!(texte.starts_with("ssh-ed25519 ") && !texte.contains("PRIVATE"));
        m.supprimer("k").unwrap();
        assert!(!Path::new(&info.chemin).exists());
        assert!(!m.chemin_public("k").exists());
        assert_eq!(m.supprimer("k").unwrap_err().code, CodeCle::Absente);
    }

    #[cfg(unix)]
    #[test]
    fn la_cle_privee_n_est_lisible_que_par_son_proprietaire() {
        use std::os::unix::fs::PermissionsExt;
        let m = magasin("droits");
        let info = m.generer("k", GenreCle::Ed25519, "", None).unwrap();
        assert_eq!(fs::metadata(&info.chemin).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
