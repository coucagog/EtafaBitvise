use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Écrit dans un fichier voisin puis le renomme : une coupure ne laisse jamais un fichier tronqué.
fn remplacer(chemin: &Path, contenu: &[u8], ouvrir: impl FnOnce(&Path) -> io::Result<File>) -> io::Result<()> {
    if let Some(parent) = chemin.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut temporaire = chemin.as_os_str().to_owned();
    temporaire.push(".tmp");
    let temporaire = std::path::PathBuf::from(temporaire);
    {
        let mut fichier = ouvrir(&temporaire)?;
        fichier.write_all(contenu)?;
        fichier.sync_all()?;
    }
    std::fs::rename(&temporaire, chemin)
}

pub(crate) fn ecrire_atomique(chemin: &Path, contenu: &[u8]) -> io::Result<()> {
    remplacer(chemin, contenu, |t| File::create(t))
}

/// Pour une clé privée : sous Unix, lisible par son seul propriétaire dès sa création.
/// Sous Windows, le fichier hérite des droits du profil de l'utilisateur.
pub(crate) fn ecrire_prive(chemin: &Path, contenu: &[u8]) -> io::Result<()> {
    remplacer(chemin, contenu, |t| {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(t)
    })
}
