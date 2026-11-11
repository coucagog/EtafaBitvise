//! Parcours de fichiers et transferts SFTP.

use std::fmt::Display;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::watch;

use crate::connexion::Session;

const TAILLE_TAMPON: usize = 128 * 1024;
const INTERVALLE_PROGRES: Duration = Duration::from_millis(100);
pub const ANNULE: &str = "Annulé";

fn txt(e: impl Display) -> String {
    e.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Genre {
    Dossier,
    Fichier,
    LienDossier,
    Lien,
    Autre,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entree {
    pub nom: String,
    pub chemin: String,
    pub genre: Genre,
    pub taille: u64,
    pub modifie: Option<u64>,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Conflit {
    Ecraser,
    Reprendre,
    Ignorer,
    Renommer,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progres {
    pub fait: u64,
    pub total: u64,
    pub fichier: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Bilan {
    pub fichiers: u32,
    pub ignores: u32,
    pub erreurs: Vec<String>,
}

pub type SurProgres = Box<dyn Fn(Progres) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ordre {
    Marche,
    Pause,
    Annule,
}

/// Commande un transfert en cours depuis une autre tâche.
pub struct Controle(watch::Sender<Ordre>);

impl Default for Controle {
    fn default() -> Self {
        Self(watch::channel(Ordre::Marche).0)
    }
}

impl Controle {
    pub fn pause(&self, active: bool) {
        self.0.send_modify(|o| {
            if *o != Ordre::Annule {
                *o = if active { Ordre::Pause } else { Ordre::Marche };
            }
        });
    }

    pub fn annuler(&self) {
        self.0.send_replace(Ordre::Annule);
    }
}

enum Echec {
    Annule,
    Autre(String),
}

async fn point_de_controle(ordres: &mut watch::Receiver<Ordre>) -> Result<(), Echec> {
    match ordres.wait_for(|o| *o != Ordre::Pause).await.map(|o| *o) {
        Ok(Ordre::Marche) => Ok(()),
        _ => Err(Echec::Annule),
    }
}

/// Rejette les noms reçus d'un serveur qui sortiraient du dossier de destination.
pub fn nom_sur(nom: &str) -> bool {
    if nom.is_empty() || nom == "." || nom == ".." || nom.contains(['/', '\\', '\0']) {
        return false;
    }
    if !cfg!(windows) {
        return true;
    }
    let base = nom.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserve = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.ends_with(|c: char| c.is_ascii_digit()));
    !reserve && !nom.contains(':') && !nom.ends_with([' ', '.'])
}

fn joindre(dossier: &str, nom: &str) -> String {
    if dossier.is_empty() || dossier.ends_with('/') {
        format!("{dossier}{nom}")
    } else {
        format!("{dossier}/{nom}")
    }
}

/// « a.txt » devient « a (1).txt » ; un nom caché comme « .profile » garde son point initial.
fn nom_numerote(nom: &str, n: u32) -> String {
    match nom.rfind('.') {
        Some(i) if i > 0 => format!("{} ({n}){}", &nom[..i], &nom[i..]),
        _ => format!("{nom} ({n})"),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Ignorer,
    Copier(u64),
}

fn action(conflit: Conflit, existant: Option<u64>, taille: u64) -> Action {
    match (existant, conflit) {
        (None, _) => Action::Copier(0),
        (Some(_), Conflit::Ignorer) => Action::Ignorer,
        (Some(n), Conflit::Reprendre) if n == taille => Action::Ignorer,
        (Some(n), Conflit::Reprendre) if n < taille => Action::Copier(n),
        _ => Action::Copier(0),
    }
}

struct Suivi<'a> {
    fait: u64,
    total: u64,
    dernier: Instant,
    fichier: String,
    rappel: &'a SurProgres,
}

impl<'a> Suivi<'a> {
    fn new(total: u64, rappel: &'a SurProgres) -> Self {
        Self { fait: 0, total, dernier: Instant::now(), fichier: String::new(), rappel }
    }

    fn emettre(&mut self) {
        self.dernier = Instant::now();
        (self.rappel)(Progres { fait: self.fait, total: self.total, fichier: self.fichier.clone() });
    }

    fn ajouter(&mut self, octets: u64) {
        self.fait += octets;
        if self.dernier.elapsed() >= INTERVALLE_PROGRES {
            self.emettre();
        }
    }
}

#[derive(Default)]
struct Arbre {
    dossiers: Vec<String>,
    /// Chemin relatif à la racine, vide si la source est un fichier seul.
    fichiers: Vec<(String, u64)>,
    ignores: u32,
    erreurs: Vec<String>,
}

impl Arbre {
    fn total(&self) -> u64 {
        self.fichiers.iter().map(|(_, t)| t).sum()
    }
}

fn relatif(racine: &str, nom: &str) -> String {
    if racine.is_empty() {
        nom.to_string()
    } else {
        format!("{racine}/{nom}")
    }
}

pub struct Sftp {
    pub(crate) s: SftpSession,
}

impl Session {
    pub async fn ouvrir_sftp(&self) -> Result<Sftp, String> {
        let canal = self.poignee.channel_open_session().await.map_err(txt)?;
        canal.request_subsystem(true, "sftp").await.map_err(txt)?;
        let s = SftpSession::new(canal.into_stream()).await.map_err(txt)?;
        s.set_timeout(30);
        Ok(Sftp { s })
    }
}

impl Sftp {
    pub async fn accueil(&self) -> Result<String, String> {
        self.s.canonicalize(".").await.map_err(txt)
    }

    pub async fn lister(&self, chemin: &str) -> Result<Vec<Entree>, String> {
        let mut entrees = Vec::new();
        for e in self.s.read_dir(chemin).await.map_err(txt)? {
            let nom = e.file_name();
            if nom == "." || nom == ".." {
                continue;
            }
            let chemin_entree = joindre(chemin, &nom);
            let meta = e.metadata();
            let type_fichier = e.file_type();
            let genre = if type_fichier.is_dir() {
                Genre::Dossier
            } else if type_fichier.is_symlink() {
                match self.s.metadata(chemin_entree.as_str()).await {
                    Ok(cible) if cible.file_type().is_dir() => Genre::LienDossier,
                    _ => Genre::Lien,
                }
            } else if type_fichier.is_file() {
                Genre::Fichier
            } else {
                Genre::Autre
            };
            entrees.push(Entree {
                nom,
                chemin: chemin_entree,
                genre,
                taille: meta.size.unwrap_or(0),
                modifie: meta.mtime.map(u64::from),
                mode: meta.permissions.map(|p| p & 0o7777),
            });
        }
        Ok(entrees)
    }

    pub async fn creer_dossier(&self, chemin: &str) -> Result<(), String> {
        self.s.create_dir(chemin).await.map_err(txt)
    }

    pub async fn renommer(&self, ancien: &str, nouveau: &str) -> Result<(), String> {
        self.s.rename(ancien, nouveau).await.map_err(txt)
    }

    pub async fn changer_mode(&self, chemin: &str, mode: u32) -> Result<(), String> {
        let attributs = FileAttributes { permissions: Some(mode & 0o7777), ..FileAttributes::empty() };
        self.s.set_metadata(chemin, attributs).await.map_err(txt)
    }

    /// Un lien symbolique est retiré sans toucher à sa cible.
    pub async fn supprimer(&self, chemin: &str) -> Result<(), String> {
        let meta = self.s.symlink_metadata(chemin).await.map_err(txt)?;
        if !meta.file_type().is_dir() {
            return self.s.remove_file(chemin).await.map_err(txt);
        }
        let mut a_visiter = vec![chemin.to_string()];
        let mut dossiers = Vec::new();
        while let Some(dossier) = a_visiter.pop() {
            for e in self.s.read_dir(dossier.as_str()).await.map_err(txt)? {
                let nom = e.file_name();
                if nom == "." || nom == ".." {
                    continue;
                }
                let c = joindre(&dossier, &nom);
                if e.file_type().is_dir() {
                    a_visiter.push(c);
                } else {
                    self.s.remove_file(c.as_str()).await.map_err(|e| format!("{c} : {e}"))?;
                }
            }
            dossiers.push(dossier);
        }
        for d in dossiers.iter().rev() {
            self.s.remove_dir(d.as_str()).await.map_err(|e| format!("{d} : {e}"))?;
        }
        Ok(())
    }

    async fn existe(&self, chemin: &str) -> bool {
        self.s.symlink_metadata(chemin).await.is_ok()
    }

    async fn taille_existante(&self, chemin: &str) -> Option<u64> {
        self.s.metadata(chemin).await.ok().map(|m| m.size.unwrap_or(0))
    }

    async fn nom_libre(&self, dossier: &str, nom: &str) -> Result<String, String> {
        for n in 1..1000 {
            let candidat = nom_numerote(nom, n);
            if !self.existe(&joindre(dossier, &candidat)).await {
                return Ok(candidat);
            }
        }
        Err(format!("Aucun nom libre pour {nom}."))
    }

    pub async fn envoyer(
        &self,
        source: &Path,
        dossier: &str,
        conflit: Conflit,
        controle: &Controle,
        rappel: &SurProgres,
    ) -> Result<Bilan, String> {
        let mut nom = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("Nom de fichier inutilisable : {}", source.display()))?
            .to_string();
        if self.existe(&joindre(dossier, &nom)).await {
            match conflit {
                Conflit::Ignorer => return Ok(Bilan { ignores: 1, ..Default::default() }),
                Conflit::Renommer => nom = self.nom_libre(dossier, &nom).await?,
                _ => {}
            }
        }
        let racine = joindre(dossier, &nom);
        let arbre = arbre_local(source).await?;
        let mut bilan = Bilan { ignores: arbre.ignores, erreurs: arbre.erreurs.clone(), ..Default::default() };
        let mut suivi = Suivi::new(arbre.total(), rappel);
        let mut ordres = controle.0.subscribe();

        for rel in &arbre.dossiers {
            // Un dossier déjà présent n'est pas une erreur : un vrai refus ressort à l'écriture des fichiers.
            let _ = self.s.create_dir(joindre(&racine, rel).trim_end_matches('/')).await;
        }
        for (rel, taille) in &arbre.fichiers {
            let cible = if rel.is_empty() { racine.clone() } else { joindre(&racine, rel) };
            let origine = if rel.is_empty() { source.to_path_buf() } else { source.join(rel) };
            suivi.fichier = if rel.is_empty() { nom.clone() } else { rel.clone() };
            suivi.emettre();
            match action(conflit, self.taille_existante(&cible).await, *taille) {
                Action::Ignorer => {
                    suivi.fait += taille;
                    bilan.ignores += 1;
                }
                Action::Copier(decalage) => {
                    suivi.fait += decalage;
                    match self.copier_vers_serveur(&origine, &cible, decalage, &mut suivi, &mut ordres).await {
                        Ok(()) => bilan.fichiers += 1,
                        Err(Echec::Annule) => return Err(ANNULE.into()),
                        Err(Echec::Autre(e)) => bilan.erreurs.push(format!("{} : {e}", suivi.fichier)),
                    }
                }
            }
        }
        suivi.emettre();
        Ok(bilan)
    }

    async fn copier_vers_serveur(
        &self,
        origine: &Path,
        cible: &str,
        decalage: u64,
        suivi: &mut Suivi<'_>,
        ordres: &mut watch::Receiver<Ordre>,
    ) -> Result<(), Echec> {
        let autre = |e: String| Echec::Autre(e);
        let mut lecture = tokio::fs::File::open(origine).await.map_err(|e| autre(txt(e)))?;
        let mut ecriture = if decalage > 0 {
            let mut f = self.s.open_with_flags(cible, OpenFlags::WRITE).await.map_err(|e| autre(txt(e)))?;
            f.seek(SeekFrom::Start(decalage)).await.map_err(|e| autre(txt(e)))?;
            lecture.seek(SeekFrom::Start(decalage)).await.map_err(|e| autre(txt(e)))?;
            f
        } else {
            self.s.create(cible).await.map_err(|e| autre(txt(e)))?
        };
        let mut tampon = vec![0; TAILLE_TAMPON];
        loop {
            point_de_controle(ordres).await?;
            let n = lecture.read(&mut tampon).await.map_err(|e| autre(txt(e)))?;
            if n == 0 {
                break;
            }
            ecriture.write_all(&tampon[..n]).await.map_err(|e| autre(txt(e)))?;
            suivi.ajouter(n as u64);
        }
        ecriture.shutdown().await.map_err(|e| autre(txt(e)))
    }

    pub async fn telecharger(
        &self,
        source: &str,
        dossier: &Path,
        conflit: Conflit,
        controle: &Controle,
        rappel: &SurProgres,
    ) -> Result<Bilan, String> {
        let mut nom = source.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
        if !nom_sur(&nom) {
            return Err(format!("Nom refusé pour ce système : {nom}"));
        }
        if tokio::fs::symlink_metadata(dossier.join(&nom)).await.is_ok() {
            match conflit {
                Conflit::Ignorer => return Ok(Bilan { ignores: 1, ..Default::default() }),
                Conflit::Renommer => nom = nom_libre_local(dossier, &nom).await?,
                _ => {}
            }
        }
        let racine = dossier.join(&nom);
        let arbre = self.arbre_distant(source).await?;
        let mut bilan = Bilan { ignores: arbre.ignores, erreurs: arbre.erreurs.clone(), ..Default::default() };
        let mut suivi = Suivi::new(arbre.total(), rappel);
        let mut ordres = controle.0.subscribe();

        for rel in &arbre.dossiers {
            let chemin = if rel.is_empty() { racine.clone() } else { racine.join(rel) };
            tokio::fs::create_dir_all(&chemin).await.map_err(|e| format!("{} : {e}", chemin.display()))?;
        }
        for (rel, taille) in &arbre.fichiers {
            let origine = if rel.is_empty() { source.to_string() } else { joindre(source, rel) };
            let cible = if rel.is_empty() { racine.clone() } else { racine.join(rel) };
            suivi.fichier = if rel.is_empty() { nom.clone() } else { rel.clone() };
            suivi.emettre();
            let existant = tokio::fs::metadata(&cible).await.ok().map(|m| m.len());
            match action(conflit, existant, *taille) {
                Action::Ignorer => {
                    suivi.fait += taille;
                    bilan.ignores += 1;
                }
                Action::Copier(decalage) => {
                    suivi.fait += decalage;
                    match self.copier_depuis_serveur(&origine, &cible, decalage, &mut suivi, &mut ordres).await {
                        Ok(()) => bilan.fichiers += 1,
                        Err(Echec::Annule) => return Err(ANNULE.into()),
                        Err(Echec::Autre(e)) => bilan.erreurs.push(format!("{} : {e}", suivi.fichier)),
                    }
                }
            }
        }
        suivi.emettre();
        Ok(bilan)
    }

    async fn copier_depuis_serveur(
        &self,
        origine: &str,
        cible: &Path,
        decalage: u64,
        suivi: &mut Suivi<'_>,
        ordres: &mut watch::Receiver<Ordre>,
    ) -> Result<(), Echec> {
        let autre = |e: String| Echec::Autre(e);
        let mut lecture = self.s.open(origine).await.map_err(|e| autre(txt(e)))?;
        let mut ecriture = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(decalage == 0)
            .open(cible)
            .await
            .map_err(|e| autre(txt(e)))?;
        if decalage > 0 {
            lecture.seek(SeekFrom::Start(decalage)).await.map_err(|e| autre(txt(e)))?;
            ecriture.seek(SeekFrom::Start(decalage)).await.map_err(|e| autre(txt(e)))?;
        }
        let mut tampon = vec![0; TAILLE_TAMPON];
        loop {
            point_de_controle(ordres).await?;
            let n = lecture.read(&mut tampon).await.map_err(|e| autre(txt(e)))?;
            if n == 0 {
                break;
            }
            ecriture.write_all(&tampon[..n]).await.map_err(|e| autre(txt(e)))?;
            suivi.ajouter(n as u64);
        }
        ecriture.flush().await.map_err(|e| autre(txt(e)))
    }

    async fn arbre_distant(&self, source: &str) -> Result<Arbre, String> {
        let meta = self.s.metadata(source).await.map_err(txt)?;
        let mut arbre = Arbre::default();
        if !meta.file_type().is_dir() {
            arbre.fichiers.push((String::new(), meta.size.unwrap_or(0)));
            return Ok(arbre);
        }
        arbre.dossiers.push(String::new());
        let mut a_visiter = vec![String::new()];
        while let Some(rel) = a_visiter.pop() {
            let dossier = if rel.is_empty() { source.to_string() } else { joindre(source, &rel) };
            for e in self.s.read_dir(dossier.as_str()).await.map_err(txt)? {
                let nom = e.file_name();
                if nom == "." || nom == ".." {
                    continue;
                }
                if !nom_sur(&nom) {
                    arbre.erreurs.push(format!("Nom refusé pour ce système : {nom}"));
                    continue;
                }
                let chemin_rel = relatif(&rel, &nom);
                let genre = e.file_type();
                if genre.is_dir() {
                    arbre.dossiers.push(chemin_rel.clone());
                    a_visiter.push(chemin_rel);
                } else if genre.is_file() {
                    arbre.fichiers.push((chemin_rel, e.metadata().size.unwrap_or(0)));
                } else {
                    arbre.ignores += 1;
                }
            }
        }
        Ok(arbre)
    }
}

async fn nom_libre_local(dossier: &Path, nom: &str) -> Result<String, String> {
    for n in 1..1000 {
        let candidat = nom_numerote(nom, n);
        if tokio::fs::symlink_metadata(dossier.join(&candidat)).await.is_err() {
            return Ok(candidat);
        }
    }
    Err(format!("Aucun nom libre pour {nom}."))
}

/// Les liens symboliques ne sont pas suivis : ils peuvent boucler ou sortir du dossier.
async fn arbre_local(source: &Path) -> Result<Arbre, String> {
    let meta = tokio::fs::metadata(source).await.map_err(txt)?;
    let mut arbre = Arbre::default();
    if !meta.is_dir() {
        arbre.fichiers.push((String::new(), meta.len()));
        return Ok(arbre);
    }
    arbre.dossiers.push(String::new());
    let mut a_visiter = vec![String::new()];
    while let Some(rel) = a_visiter.pop() {
        let dossier = if rel.is_empty() { source.to_path_buf() } else { source.join(&rel) };
        let mut lecture = tokio::fs::read_dir(&dossier).await.map_err(txt)?;
        while let Some(e) = lecture.next_entry().await.map_err(txt)? {
            let Some(nom) = e.file_name().to_str().map(str::to_string) else {
                arbre.erreurs.push(format!("Nom non UTF-8 dans {}", dossier.display()));
                continue;
            };
            let genre = e.file_type().await.map_err(txt)?;
            let chemin_rel = relatif(&rel, &nom);
            if genre.is_dir() {
                arbre.dossiers.push(chemin_rel.clone());
                a_visiter.push(chemin_rel);
            } else if genre.is_file() {
                arbre.fichiers.push((chemin_rel, e.metadata().await.map_err(txt)?.len()));
            } else {
                arbre.ignores += 1;
            }
        }
    }
    Ok(arbre)
}

pub fn accueil_local() -> String {
    std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_else(|_| "/".into())
}

fn lecteurs() -> Vec<Entree> {
    (b'A'..=b'Z')
        .map(|l| format!("{}:\\", l as char))
        .filter(|c| Path::new(c).exists())
        .map(|c| Entree { nom: c.clone(), chemin: c, genre: Genre::Dossier, taille: 0, modifie: None, mode: None })
        .collect()
}

/// Sous Windows, le chemin vide liste les lecteurs.
pub async fn lister_local(chemin: &str) -> Result<Vec<Entree>, String> {
    if chemin.is_empty() && cfg!(windows) {
        return Ok(lecteurs());
    }
    let mut lecture = tokio::fs::read_dir(chemin).await.map_err(txt)?;
    let mut entrees = Vec::new();
    while let Some(e) = lecture.next_entry().await.map_err(txt)? {
        let chemin_entree: PathBuf = e.path();
        // Un fichier verrouillé par le système ne doit pas masquer tout le dossier.
        let Ok(meta) = tokio::fs::symlink_metadata(&chemin_entree).await else { continue };
        let genre = if meta.is_dir() {
            Genre::Dossier
        } else if meta.is_symlink() {
            match tokio::fs::metadata(&chemin_entree).await {
                Ok(cible) if cible.is_dir() => Genre::LienDossier,
                _ => Genre::Lien,
            }
        } else if meta.is_file() {
            Genre::Fichier
        } else {
            Genre::Autre
        };
        entrees.push(Entree {
            nom: e.file_name().to_string_lossy().into_owned(),
            chemin: chemin_entree.to_string_lossy().into_owned(),
            genre,
            taille: meta.len(),
            modifie: meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()),
            mode: None,
        });
    }
    Ok(entrees)
}

pub async fn creer_dossier_local(chemin: &str) -> Result<(), String> {
    tokio::fs::create_dir(chemin).await.map_err(txt)
}

pub async fn renommer_local(ancien: &str, nouveau: &str) -> Result<(), String> {
    tokio::fs::rename(ancien, nouveau).await.map_err(txt)
}

pub async fn supprimer_local(chemin: &str) -> Result<(), String> {
    let meta = tokio::fs::symlink_metadata(chemin).await.map_err(txt)?;
    if meta.is_dir() {
        tokio::fs::remove_dir_all(chemin).await.map_err(txt)
    } else {
        tokio::fs::remove_file(chemin).await.map_err(txt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_dangereux_refuses() {
        for nom in ["", ".", "..", "../x", "a/b", "a\\b", "a\0b"] {
            assert!(!nom_sur(nom), "{nom:?}");
        }
        assert!(nom_sur("rapport final.txt"));
        assert!(nom_sur(".profile"));
    }

    #[cfg(windows)]
    #[test]
    fn noms_interdits_par_windows_refuses() {
        for nom in ["CON", "nul.txt", "COM1", "lpt9.log", "a:b", "flux:cache", "fin.", "fin "] {
            assert!(!nom_sur(nom), "{nom:?}");
        }
        assert!(nom_sur("console.txt"));
        assert!(nom_sur("COMPTE"));
    }

    #[test]
    fn numerotation_garde_l_extension_et_les_noms_caches() {
        assert_eq!(nom_numerote("a.tar.gz", 2), "a.tar (2).gz");
        assert_eq!(nom_numerote("dossier", 1), "dossier (1)");
        assert_eq!(nom_numerote(".profile", 1), ".profile (1)");
    }

    #[test]
    fn regles_de_conflit() {
        assert_eq!(action(Conflit::Ignorer, None, 10), Action::Copier(0));
        assert_eq!(action(Conflit::Ignorer, Some(3), 10), Action::Ignorer);
        assert_eq!(action(Conflit::Ecraser, Some(30), 10), Action::Copier(0));
        assert_eq!(action(Conflit::Reprendre, Some(3), 10), Action::Copier(3));
        assert_eq!(action(Conflit::Reprendre, Some(10), 10), Action::Ignorer);
        // Destination plus grande que la source : reprendre la corromprait.
        assert_eq!(action(Conflit::Reprendre, Some(30), 10), Action::Copier(0));
    }

    #[test]
    fn pause_ne_leve_pas_une_annulation() {
        let c = Controle::default();
        c.annuler();
        c.pause(false);
        assert_eq!(*c.0.borrow(), Ordre::Annule);
    }
}
