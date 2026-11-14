//! Profils de connexion. Le type ne contient aucun champ de secret : un mot de passe ou une
//! phrase secrète ne peut donc pas finir dans `profiles.json`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::fichiers::ecrire_atomique;
use crate::agent::AccesAgent;
use crate::tunnels::Regle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MethodeAuth {
    MotDePasse,
    #[default]
    Cle,
    Clavier,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GenreProxy {
    #[default]
    Aucun,
    Socks5,
    Http,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Proxy {
    pub genre: GenreProxy,
    pub hote: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profil {
    pub id: String,
    pub nom: String,
    pub groupe: String,
    pub hote: String,
    pub port: u16,
    pub utilisateur: String,
    pub methode: MethodeAuth,
    pub chemin_cle: String,
    pub delai_connexion: u32,
    pub keepalive: bool,
    pub intervalle_keepalive: u32,
    pub notes: String,
    pub derniere_connexion: Option<String>,
    pub tunnels: Vec<Regle>,
    pub proxy: Proxy,
    /// Identifiant du profil par lequel passer. Vide : connexion directe.
    pub rebond: String,
    /// Le secret saisi est conservé dans le trousseau du système.
    pub memoriser: bool,
    /// Ce qu'un agent peut faire avec ce profil. Absent des anciens fichiers : aucun accès.
    pub agent: AccesAgent,
}

impl Default for Profil {
    fn default() -> Self {
        Self {
            id: String::new(),
            nom: String::new(),
            groupe: String::new(),
            hote: String::new(),
            port: 22,
            utilisateur: String::new(),
            methode: MethodeAuth::default(),
            chemin_cle: String::new(),
            delai_connexion: 15,
            keepalive: true,
            intervalle_keepalive: 30,
            notes: String::new(),
            derniere_connexion: None,
            tunnels: Vec::new(),
            proxy: Proxy::default(),
            rebond: String::new(),
            memoriser: false,
            agent: AccesAgent::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Fichier {
    version: u32,
    profils: Vec<Profil>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErreurProfils {
    pub message: String,
    pub detail: Option<String>,
}

pub struct MagasinProfils {
    chemin: PathBuf,
}

impl MagasinProfils {
    pub fn new(chemin: PathBuf) -> Self {
        Self { chemin }
    }

    pub fn chemin(&self) -> &std::path::Path {
        &self.chemin
    }

    /// Un fichier absent donne une liste vide. Un fichier illisible renvoie une erreur et n'est pas modifié.
    pub fn charger(&self) -> Result<Vec<Profil>, ErreurProfils> {
        let texte = match std::fs::read_to_string(&self.chemin) {
            Ok(texte) => texte,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(ErreurProfils {
                    message: "Le fichier de profils n'a pas pu être lu.".into(),
                    detail: Some(e.to_string()),
                })
            }
        };
        serde_json::from_str::<Fichier>(&texte).map(|f| f.profils).map_err(|e| ErreurProfils {
            message: "Le fichier de profils est illisible. Il n'a pas été modifié.".into(),
            detail: Some(format!("profiles.json, ligne {} : {e}", e.line())),
        })
    }

    /// Garde la version précédente, si elle est valide, dans `profiles.json.bak`.
    pub fn enregistrer(&self, profils: &[Profil]) -> Result<(), ErreurProfils> {
        let erreur = |e: String| ErreurProfils {
            message: "Les profils n'ont pas pu être enregistrés.".into(),
            detail: Some(e),
        };
        if let Ok(ancien) = std::fs::read_to_string(&self.chemin) {
            if serde_json::from_str::<Fichier>(&ancien).is_ok() {
                let mut sauvegarde = self.chemin.as_os_str().to_owned();
                sauvegarde.push(".bak");
                ecrire_atomique(&PathBuf::from(sauvegarde), ancien.as_bytes())
                    .map_err(|e| erreur(e.to_string()))?;
            }
        }
        let fichier = Fichier { version: 1, profils: profils.to_vec() };
        let texte = serde_json::to_vec_pretty(&fichier).map_err(|e| erreur(e.to_string()))?;
        ecrire_atomique(&self.chemin, &texte).map_err(|e| erreur(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mc-profils-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn absent_donne_vide_puis_aller_retour() {
        let d = dossier("aller-retour");
        let m = MagasinProfils::new(d.join("profiles.json"));
        assert!(m.charger().unwrap().is_empty());
        let p = Profil { id: "a".into(), nom: "prod".into(), hote: "10.0.0.1".into(), ..Default::default() };
        m.enregistrer(std::slice::from_ref(&p)).unwrap();
        assert_eq!(m.charger().unwrap(), vec![p]);
    }

    #[test]
    fn fichier_corrompu_signale_et_conserve() {
        let d = dossier("corrompu");
        std::fs::create_dir_all(&d).unwrap();
        let chemin = d.join("profiles.json");
        std::fs::write(&chemin, "{ pas du json").unwrap();
        let erreur = MagasinProfils::new(chemin.clone()).charger().unwrap_err();
        assert!(erreur.detail.unwrap().contains("ligne 1"));
        assert_eq!(std::fs::read_to_string(chemin).unwrap(), "{ pas du json");
    }

    #[test]
    fn un_champ_secret_est_ignore_et_jamais_reecrit() {
        let p: Profil = serde_json::from_str(r#"{"id":"a","motDePasse":"s3cret"}"#).unwrap();
        assert!(!serde_json::to_string(&p).unwrap().contains("s3cret"));
    }
}
