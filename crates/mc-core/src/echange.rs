//! Export et import de profils vers un fichier `.mcprofils`, pour les reprendre sur une autre machine
//! ou sur l'application mobile.
//!
//! Le fichier contient les profils, la partie publique de leurs clés et les clés d'hôte connues.
//! En option, les clés privées voyagent dans un coffre chiffré par une phrase choisie à l'export :
//! Argon2id pour dériver la clé, AES-256-GCM pour chiffrer et authentifier.
//! Les mots de passe et passphrases du trousseau ne sont jamais exportés.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit};
use argon2::{Algorithm, Argon2, Params, Version};
use russh::keys::{decode_secret_key, PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};

use crate::agent::AccesAgent;
use crate::cles::{nom_propre, CodeCle, MagasinCles};
use crate::hotes::{empreinte, Ajout, MagasinHotes};
use crate::profils::{MethodeAuth, Profil};

pub const EXTENSION: &str = "mcprofils";
const FORMAT: &str = "mls-connect-profils";
const VERSION: u32 = 1;
/// Lie le coffre à ce format : un coffre déplacé dans un autre contexte ne se déchiffre pas.
const DONNEES_ASSOCIEES: &[u8] = b"mls-connect-profils/1";
pub const PHRASE_MIN: usize = 8;
const TAILLE_FICHIER_MAX: usize = 8 * 1024 * 1024;
const PROFILS_MAX: usize = 2000;
const TAILLE_CLE_MAX: u64 = 64 * 1024;
// Argon2id : 64 Mio, 3 passes. À l'import, des paramètres démesurés sont refusés.
const MEMOIRE_KIO: u32 = 64 * 1024;
const ITERATIONS: u32 = 3;
const MEMOIRE_MAX_KIO: u32 = 512 * 1024;
const ITERATIONS_MAX: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeEchange {
    Format,
    PhraseCourte,
    PhraseIncorrecte,
    Disque,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErreurEchange {
    pub code: CodeEchange,
    pub message: String,
}

fn erreur(code: CodeEchange, message: impl Into<String>) -> ErreurEchange {
    ErreurEchange { code, message: message.into() }
}

fn format(message: impl Into<String>) -> ErreurEchange {
    erreur(CodeEchange::Format, message)
}

impl From<std::io::Error> for ErreurEchange {
    fn from(e: std::io::Error) -> Self {
        erreur(CodeEchange::Disque, format!("Accès au disque impossible : {e}."))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fichier {
    format: String,
    version: u32,
    profils: Vec<Entree>,
    #[serde(default)]
    cles: Vec<ClePublique>,
    #[serde(default)]
    hotes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    coffre: Option<Coffre>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entree {
    profil: Profil,
    /// Empreinte de la clé du profil, si elle en a une.
    #[serde(default)]
    cle: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClePublique {
    nom: String,
    empreinte: String,
    publique: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Coffre {
    kdf: String,
    memoire: u32,
    iterations: u32,
    parallelisme: u32,
    sel: String,
    nonce: String,
    donnees: String,
}

#[derive(Serialize, Deserialize)]
struct Secrets {
    cles: Vec<ClePrivee>,
}

#[derive(Serialize, Deserialize)]
struct ClePrivee {
    empreinte: String,
    privee: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BilanExport {
    pub profils: usize,
    pub cles: usize,
    pub cles_privees: usize,
    pub hotes: usize,
    /// Profils dont la clé n'a pas pu être lue : ils partent sans clé.
    pub cles_illisibles: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApercuProfil {
    pub id: String,
    pub nom: String,
    pub serveur: String,
    /// Un profil local porte déjà cet identifiant.
    pub existe: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Apercu {
    pub profils: Vec<ApercuProfil>,
    pub cles: usize,
    pub cles_privees: bool,
    pub hotes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Doublon {
    /// Le profil importé remplace le profil local de même identifiant.
    Remplacer,
    /// Le profil importé est ajouté à côté, sous un nouvel identifiant.
    Copier,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BilanImport {
    pub ajoutes: usize,
    pub remplaces: usize,
    pub cles_importees: usize,
    pub cles_presentes: usize,
    /// Profils à clé dont la clé reste à choisir sur cette machine.
    pub cles_a_choisir: Vec<String>,
    pub hotes_ajoutes: usize,
    pub hotes_presents: usize,
    /// Hôtes dont la clé importée diffère de celle déjà enregistrée : la clé locale est gardée.
    pub hotes_differents: Vec<String>,
}

fn hex(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

fn depuis_hex(texte: &str) -> Option<Vec<u8>> {
    if texte.len() % 2 != 0 {
        return None;
    }
    (0..texte.len()).step_by(2).map(|i| u8::from_str_radix(texte.get(i..i + 2)?, 16).ok()).collect()
}

fn deriver(phrase: &str, sel: &[u8], memoire: u32, iterations: u32, parallelisme: u32) -> Result<[u8; 32], ErreurEchange> {
    let params = Params::new(memoire, iterations, parallelisme, Some(32)).map_err(|e| format(format!("Paramètres de chiffrement invalides : {e}.")))?;
    let mut cle = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(phrase.as_bytes(), sel, &mut cle)
        .map_err(|e| format(format!("Dérivation impossible : {e}.")))?;
    Ok(cle)
}

fn chiffrer(phrase: &str, clair: &[u8]) -> Result<Coffre, ErreurEchange> {
    let sel: [u8; 16] = rand::random();
    let nonce: [u8; 12] = rand::random();
    let mut cle = deriver(phrase, &sel, MEMOIRE_KIO, ITERATIONS, 1)?;
    let chiffre = Aes256Gcm::new_from_slice(&cle)
        .map_err(|_| format("Clé de chiffrement invalide."))?
        .encrypt(&nonce.into(), Payload { msg: clair, aad: DONNEES_ASSOCIEES })
        .map_err(|_| format("Chiffrement impossible."));
    cle.fill(0);
    Ok(Coffre {
        kdf: "argon2id".into(),
        memoire: MEMOIRE_KIO,
        iterations: ITERATIONS,
        parallelisme: 1,
        sel: hex(&sel),
        nonce: hex(&nonce),
        donnees: hex(&chiffre?),
    })
}

fn dechiffrer(phrase: &str, coffre: &Coffre) -> Result<Vec<u8>, ErreurEchange> {
    if coffre.kdf != "argon2id" {
        return Err(format(format!("Méthode de chiffrement inconnue : {}.", coffre.kdf)));
    }
    if coffre.memoire > MEMOIRE_MAX_KIO || coffre.iterations > ITERATIONS_MAX || !(1..=4).contains(&coffre.parallelisme) {
        return Err(format("Paramètres de chiffrement hors limites : fichier refusé."));
    }
    let (Some(sel), Some(nonce), Some(donnees)) = (depuis_hex(&coffre.sel), depuis_hex(&coffre.nonce), depuis_hex(&coffre.donnees)) else {
        return Err(format("Coffre des clés illisible."));
    };
    let nonce: [u8; 12] = nonce.try_into().map_err(|_| format("Coffre des clés illisible."))?;
    let mut cle = deriver(phrase, &sel, coffre.memoire, coffre.iterations, coffre.parallelisme)?;
    let clair = Aes256Gcm::new_from_slice(&cle)
        .map_err(|_| format("Clé de chiffrement invalide."))?
        .decrypt(&nonce.into(), Payload { msg: &donnees, aad: DONNEES_ASSOCIEES })
        .map_err(|_| erreur(CodeEchange::PhraseIncorrecte, "Phrase incorrecte, ou fichier modifié depuis l'export."));
    cle.fill(0);
    clair
}

/// Partie publique d'une clé privée : lue dans le fichier OpenSSH, même chiffré, ou dans le `.pub` voisin.
fn publique_de(chemin: &Path, texte: &str) -> Option<PublicKey> {
    if let Ok(cle) = PrivateKey::from_openssh(texte) {
        return Some(cle.public_key().clone());
    }
    let mut voisin = chemin.as_os_str().to_owned();
    voisin.push(".pub");
    if let Some(p) = std::fs::read_to_string(voisin).ok().and_then(|t| PublicKey::from_openssh(t.trim()).ok()) {
        return Some(p);
    }
    decode_secret_key(texte, None).ok().map(|c| c.public_key().clone())
}

/// Profils choisis, plus ceux qui leur servent de rebond.
fn avec_rebonds<'a>(tous: &'a [Profil], ids: &[String]) -> Vec<&'a Profil> {
    let mut retenus: Vec<&Profil> = Vec::new();
    let mut a_voir: Vec<String> = ids.to_vec();
    while let Some(id) = a_voir.pop() {
        if retenus.iter().any(|p| p.id == id) {
            continue;
        }
        if let Some(p) = tous.iter().find(|p| p.id == id) {
            if !p.rebond.is_empty() {
                a_voir.push(p.rebond.clone());
            }
            retenus.push(p);
        }
    }
    // Ordre d'origine, pour un import fidèle.
    tous.iter().filter(|p| retenus.iter().any(|r| r.id == p.id)).collect()
}

/// Construit le fichier d'export. `phrase` : clés privées incluses, chiffrées par cette phrase.
pub fn exporter(
    tous: &[Profil],
    ids: &[String],
    phrase: Option<&str>,
    hotes: &MagasinHotes,
) -> Result<(String, BilanExport), ErreurEchange> {
    let phrase = phrase.filter(|p| !p.is_empty());
    if phrase.is_some_and(|p| p.chars().count() < PHRASE_MIN) {
        return Err(erreur(CodeEchange::PhraseCourte, format!("La phrase doit compter au moins {PHRASE_MIN} caractères.")));
    }
    let retenus = avec_rebonds(tous, ids);
    if retenus.is_empty() {
        return Err(format("Aucun profil à exporter."));
    }
    let mut bilan = BilanExport { profils: retenus.len(), ..Default::default() };
    let mut entrees = Vec::new();
    let mut cles: Vec<ClePublique> = Vec::new();
    let mut privees: Vec<ClePrivee> = Vec::new();
    let mut lignes: Vec<String> = Vec::new();

    for p in &retenus {
        let mut cle = None;
        if p.methode == MethodeAuth::Cle && !p.chemin_cle.trim().is_empty() {
            let chemin = Path::new(&p.chemin_cle);
            let lue = std::fs::metadata(chemin)
                .ok()
                .filter(|m| m.len() <= TAILLE_CLE_MAX)
                .and_then(|_| std::fs::read_to_string(chemin).ok());
            match lue.as_deref().and_then(|t| publique_de(chemin, t).map(|pk| (t, pk))) {
                Some((texte, publique)) => {
                    let e = empreinte(&publique).empreinte;
                    if !cles.iter().any(|c| c.empreinte == e) {
                        let nom = nom_propre(&chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                        let publique = publique.to_openssh().map_err(|e| format(e.to_string()))?;
                        cles.push(ClePublique { nom, empreinte: e.clone(), publique });
                        if phrase.is_some() {
                            privees.push(ClePrivee { empreinte: e.clone(), privee: texte.to_string() });
                        }
                    }
                    cle = Some(e);
                }
                None => bilan.cles_illisibles.push(p.nom.clone()),
            }
        }
        for l in hotes.lignes(&p.hote, p.port)? {
            if !lignes.contains(&l) {
                lignes.push(l);
            }
        }
        // Le chemin de la clé ne vaut que sur cette machine ; l'accès d'un agent se règle sur chaque machine.
        let profil = Profil { chemin_cle: String::new(), derniere_connexion: None, agent: AccesAgent::default(), ..(*p).clone() };
        entrees.push(Entree { profil, cle });
    }

    bilan.cles = cles.len();
    bilan.cles_privees = privees.len();
    bilan.hotes = lignes.len();
    let coffre = match phrase {
        Some(phrase) => {
            let clair = serde_json::to_vec(&Secrets { cles: privees }).map_err(|e| format(e.to_string()))?;
            Some(chiffrer(phrase, &clair)?)
        }
        None => None,
    };
    let fichier = Fichier { format: FORMAT.into(), version: VERSION, profils: entrees, cles, hotes: lignes, coffre };
    let texte = serde_json::to_string_pretty(&fichier).map_err(|e| format(e.to_string()))?;
    Ok((texte, bilan))
}

fn lire(texte: &str) -> Result<Fichier, ErreurEchange> {
    if texte.len() > TAILLE_FICHIER_MAX {
        return Err(format("Ce fichier est trop grand pour un export de profils."));
    }
    let fichier: Fichier = serde_json::from_str(texte.trim_start_matches('\u{feff}'))
        .map_err(|e| format(format!("Ce fichier n'est pas un export de profils MLS Connect (ligne {}).", e.line())))?;
    if fichier.format != FORMAT {
        return Err(format("Ce fichier n'est pas un export de profils MLS Connect."));
    }
    if fichier.version > VERSION {
        return Err(format("Cet export vient d'une version plus récente de MLS Connect : mettez l'application à jour."));
    }
    if fichier.profils.len() > PROFILS_MAX {
        return Err(format("Trop de profils dans ce fichier."));
    }
    Ok(fichier)
}

pub fn apercu(texte: &str, locaux: &[Profil]) -> Result<Apercu, ErreurEchange> {
    let f = lire(texte)?;
    Ok(Apercu {
        profils: f
            .profils
            .iter()
            .map(|e| ApercuProfil {
                id: e.profil.id.clone(),
                nom: e.profil.nom.clone(),
                serveur: format!("{}@{}:{}", e.profil.utilisateur, e.profil.hote, e.profil.port),
                existe: locaux.iter().any(|l| l.id == e.profil.id),
            })
            .collect(),
        cles: f.cles.len(),
        cles_privees: f.coffre.is_some(),
        hotes: f.hotes.len(),
    })
}

fn nouvel_id() -> String {
    let n: u128 = rand::random();
    let h = format!("{n:032x}");
    format!("{}-{}-4{}-a{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}

/// Fusionne l'export avec les profils locaux et rend la nouvelle liste, à enregistrer par l'appelant.
/// Sans `phrase`, les clés privées du fichier sont ignorées ; les clés déjà présentes servent quand même.
pub fn importer(
    texte: &str,
    phrase: Option<&str>,
    doublon: Doublon,
    locaux: &[Profil],
    cles: &MagasinCles,
    hotes: &MagasinHotes,
) -> Result<(Vec<Profil>, BilanImport), ErreurEchange> {
    let f = lire(texte)?;
    let secrets: HashMap<String, String> = match (&f.coffre, phrase.filter(|p| !p.is_empty())) {
        (Some(coffre), Some(phrase)) => {
            let clair = dechiffrer(phrase, coffre)?;
            let s: Secrets = serde_json::from_slice(&clair).map_err(|_| format("Coffre des clés illisible."))?;
            s.cles.into_iter().map(|c| (c.empreinte, c.privee)).collect()
        }
        _ => HashMap::new(),
    };
    let mut bilan = BilanImport::default();
    let erreur_cle = |e: crate::cles::ErreurCle| erreur(CodeEchange::Disque, e.message);

    // Clés : une clé déjà présente (même empreinte) est réutilisée, sinon celle du coffre est importée.
    let mut chemins: HashMap<String, String> = HashMap::new();
    for c in &f.cles {
        if let Some(locale) = cles.par_empreinte(&c.empreinte).map_err(erreur_cle)? {
            bilan.cles_presentes += 1;
            chemins.insert(c.empreinte.clone(), locale.chemin);
            continue;
        }
        let Some(privee) = secrets.get(&c.empreinte) else { continue };
        match cles.importer_contenu(&c.nom, privee, None) {
            // Le coffre est authentifié ; on vérifie tout de même que la clé est bien celle annoncée.
            Ok(info) if info.empreinte == c.empreinte => {
                bilan.cles_importees += 1;
                chemins.insert(c.empreinte.clone(), info.chemin);
            }
            Ok(info) => {
                let _ = cles.supprimer(&info.nom);
            }
            Err(e) if e.code == CodeCle::Existe => {
                if let Some(locale) = cles.par_empreinte(&c.empreinte).map_err(erreur_cle)? {
                    chemins.insert(c.empreinte.clone(), locale.chemin);
                }
            }
            // Clé dans un format protégé par sa propre phrase (PuTTY, PEM) : à importer à la main.
            Err(_) => {}
        }
    }

    // Profils.
    let mut liste: Vec<Profil> = locaux.to_vec();
    let ids_locaux: HashSet<String> = locaux.iter().map(|p| p.id.clone()).collect();
    let mut correspondance: HashMap<String, String> = HashMap::new();
    let mut importes: Vec<String> = Vec::new();
    for e in &f.profils {
        let mut p = e.profil.clone();
        let ancien = p.id.clone();
        let existe = ids_locaux.contains(&ancien);
        if p.id.trim().is_empty() || (existe && doublon == Doublon::Copier) || correspondance.contains_key(&ancien) {
            p.id = nouvel_id();
            if existe {
                p.nom = format!("{} (importé)", p.nom);
            }
        }
        correspondance.insert(ancien, p.id.clone());
        p.derniere_connexion = None;
        p.agent = AccesAgent::default();
        p.chemin_cle = e.cle.as_ref().and_then(|c| chemins.get(c)).cloned().unwrap_or_default();
        if p.methode == MethodeAuth::Cle && p.chemin_cle.is_empty() {
            bilan.cles_a_choisir.push(p.nom.clone());
        }
        importes.push(p.id.clone());
        match liste.iter_mut().find(|l| l.id == p.id) {
            Some(local) => {
                *local = p;
                bilan.remplaces += 1;
            }
            None => {
                liste.push(p);
                bilan.ajoutes += 1;
            }
        }
    }
    // Rebonds : vers le profil importé correspondant, ou vers un profil local existant, sinon retirés.
    let ids: HashSet<String> = liste.iter().map(|p| p.id.clone()).collect();
    for p in liste.iter_mut().filter(|p| importes.contains(&p.id)) {
        if let Some(nouveau) = correspondance.get(&p.rebond) {
            p.rebond = nouveau.clone();
        }
        if !p.rebond.is_empty() && (!ids.contains(&p.rebond) || p.rebond == p.id) {
            p.rebond.clear();
        }
    }

    // Clés d'hôte.
    for l in &f.hotes {
        match hotes.ajouter_ligne(l)? {
            Ajout::Ajoutee => bilan.hotes_ajoutes += 1,
            Ajout::Presente => bilan.hotes_presents += 1,
            Ajout::Differente => {
                let motif = l.split_whitespace().next().unwrap_or_default().to_string();
                if !bilan.hotes_differents.contains(&motif) {
                    bilan.hotes_differents.push(motif);
                }
            }
            Ajout::Illisible => {}
        }
    }
    Ok((liste, bilan))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Machine {
        dossier: PathBuf,
        cles: MagasinCles,
        hotes: MagasinHotes,
    }

    fn machine(nom: &str) -> Machine {
        let dossier = std::env::temp_dir().join(format!("mc-echange-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).unwrap();
        Machine { cles: MagasinCles::new(dossier.join("keys")), hotes: MagasinHotes::new(dossier.join("known_hosts")), dossier }
    }

    fn cle_hote() -> PublicKey {
        PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap().public_key().clone()
    }

    /// Deux profils : « cible » passe par « rebond » ; la clé de « cible » est protégée par sa propre phrase.
    fn depart(m: &Machine) -> (Vec<Profil>, String) {
        let info = m.cles.generer("perso", crate::cles::GenreCle::Ed25519, "moi@poste", Some("phrase-de-la-cle")).unwrap();
        let rebond = Profil { id: "r".into(), nom: "rebond".into(), hote: "10.0.0.1".into(), utilisateur: "u".into(), methode: MethodeAuth::Agent, ..Default::default() };
        let cible = Profil {
            id: "c".into(),
            nom: "cible".into(),
            hote: "10.0.0.2".into(),
            port: 2222,
            utilisateur: "u".into(),
            chemin_cle: info.chemin.clone(),
            rebond: "r".into(),
            memoriser: true,
            agent: AccesAgent { niveau: crate::agent::Niveau::Complet, ..Default::default() },
            ..Default::default()
        };
        m.hotes.enregistrer("10.0.0.1", 22, &cle_hote()).unwrap();
        m.hotes.enregistrer("10.0.0.2", 2222, &cle_hote()).unwrap();
        m.hotes.enregistrer("10.9.9.9", 22, &cle_hote()).unwrap();
        (vec![rebond, cible], info.empreinte)
    }

    #[test]
    fn aller_retour_avec_cles_privees() {
        let a = machine("a1");
        let (profils, empreinte_cle) = depart(&a);
        // Seule « cible » est choisie : son rebond suit.
        let (texte, bilan) = exporter(&profils, &["c".into()], Some("phrase longue"), &a.hotes).unwrap();
        assert_eq!((bilan.profils, bilan.cles, bilan.cles_privees, bilan.hotes), (2, 1, 1, 2));
        assert!(!texte.contains(&a.dossier.display().to_string().replace('\\', "\\\\")), "chemin local exporté");
        assert!(!texte.contains("OPENSSH PRIVATE KEY"), "clé privée en clair");

        let b = machine("b1");
        let apercu = apercu(&texte, &[]).unwrap();
        assert!(apercu.cles_privees && apercu.profils.len() == 2 && !apercu.profils[0].existe);
        let (liste, bilan) = importer(&texte, Some("phrase longue"), Doublon::Copier, &[], &b.cles, &b.hotes).unwrap();
        assert_eq!((bilan.ajoutes, bilan.cles_importees, bilan.hotes_ajoutes), (2, 1, 2));
        assert!(bilan.cles_a_choisir.is_empty());
        let cible = liste.iter().find(|p| p.nom == "cible").unwrap();
        assert_eq!(cible.rebond, "r");
        assert!(cible.memoriser && cible.agent.niveau == crate::agent::Niveau::Aucun);
        let locale = b.cles.par_empreinte(&empreinte_cle).unwrap().unwrap();
        assert_eq!(cible.chemin_cle, locale.chemin);
        assert!(locale.protegee, "la clé garde sa propre phrase");
        assert!(b.hotes.lignes("10.0.0.2", 2222).unwrap().len() == 1 && b.hotes.lignes("10.9.9.9", 22).unwrap().is_empty());
    }

    #[test]
    fn sans_phrase_ni_coffre() {
        let a = machine("a2");
        let (profils, _) = depart(&a);
        let (texte, bilan) = exporter(&profils, &["c".into(), "r".into()], None, &a.hotes).unwrap();
        assert_eq!(bilan.cles_privees, 0);
        assert!(!texte.contains("coffre"));
        let b = machine("b2");
        let (liste, bilan) = importer(&texte, None, Doublon::Copier, &[], &b.cles, &b.hotes).unwrap();
        assert_eq!(liste.len(), 2);
        assert_eq!(bilan.cles_a_choisir, vec!["cible".to_string()]);
        // Sur la machine d'origine, la clé est retrouvée à son empreinte.
        let (_, bilan) = importer(&texte, None, Doublon::Remplacer, &profils, &a.cles, &a.hotes).unwrap();
        assert_eq!((bilan.remplaces, bilan.cles_presentes, bilan.hotes_presents), (2, 1, 2));
        assert!(bilan.cles_a_choisir.is_empty());
    }

    #[test]
    fn phrase_courte_mauvaise_ou_fichier_modifie() {
        let a = machine("a3");
        let (profils, _) = depart(&a);
        assert_eq!(exporter(&profils, &["c".into()], Some("court"), &a.hotes).unwrap_err().code, CodeEchange::PhraseCourte);
        let (texte, _) = exporter(&profils, &["c".into()], Some("phrase longue"), &a.hotes).unwrap();
        let b = machine("b3");
        let e = importer(&texte, Some("autre phrase"), Doublon::Copier, &[], &b.cles, &b.hotes).unwrap_err();
        assert_eq!(e.code, CodeEchange::PhraseIncorrecte);
        let mut f: serde_json::Value = serde_json::from_str(&texte).unwrap();
        let donnees = f["coffre"]["donnees"].as_str().unwrap().to_string();
        let altere = format!("{}{}", if donnees.starts_with('0') { "1" } else { "0" }, &donnees[1..]);
        f["coffre"]["donnees"] = serde_json::Value::String(altere);
        let e = importer(&f.to_string(), Some("phrase longue"), Doublon::Copier, &[], &b.cles, &b.hotes).unwrap_err();
        assert_eq!(e.code, CodeEchange::PhraseIncorrecte);
        f["coffre"]["memoire"] = serde_json::json!(4_000_000);
        assert_eq!(importer(&f.to_string(), Some("phrase longue"), Doublon::Copier, &[], &b.cles, &b.hotes).unwrap_err().code, CodeEchange::Format);
        // Le fichier reste lisible sans la phrase : profils seuls.
        assert_eq!(importer(&texte, None, Doublon::Copier, &[], &b.cles, &b.hotes).unwrap().0.len(), 2);
        assert!(lire("{}").is_err() && lire(r#"{"format":"autre","version":1,"profils":[]}"#).is_err());
    }

    #[test]
    fn copie_des_doublons_et_cle_d_hote_differente() {
        let a = machine("a4");
        let (profils, _) = depart(&a);
        let (texte, _) = exporter(&profils, &["c".into()], None, &a.hotes).unwrap();
        let b = machine("b4");
        // La machine B connaît déjà 10.0.0.1 sous une autre clé.
        b.hotes.enregistrer("10.0.0.1", 22, &cle_hote()).unwrap();
        let avant = b.hotes.lignes("10.0.0.1", 22).unwrap();
        let (liste, bilan) = importer(&texte, None, Doublon::Copier, &profils, &b.cles, &b.hotes).unwrap();
        assert_eq!((liste.len(), bilan.ajoutes), (4, 2));
        assert_eq!(bilan.hotes_differents, vec!["10.0.0.1".to_string()]);
        assert_eq!(b.hotes.lignes("10.0.0.1", 22).unwrap(), avant, "clé locale conservée");
        let copie = liste.iter().find(|p| p.nom == "cible (importé)").unwrap();
        let rebond = liste.iter().find(|p| p.nom == "rebond (importé)").unwrap();
        assert_ne!(copie.id, "c");
        assert_eq!(copie.rebond, rebond.id, "le rebond suit la copie");
    }

    #[test]
    fn hex_aller_retour() {
        let o: Vec<u8> = (0..=255).collect();
        assert_eq!(depuis_hex(&hex(&o)).unwrap(), o);
        assert!(depuis_hex("abc").is_none() && depuis_hex("zz").is_none());
    }
}
