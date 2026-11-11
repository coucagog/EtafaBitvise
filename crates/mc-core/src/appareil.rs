//! Fichiers de l'appareil désignés par le sélecteur Android (URI `content://`).
//! L'accès passe par le ContentResolver, côté Kotlin (`FichiersAndroid`). Appels bloquants.

use std::path::{Path, PathBuf};

const LONGUEUR_NOM_MAX: usize = 200;

pub fn est_uri(chemin: &str) -> bool {
    chemin.starts_with("content://")
}

/// Le nom vient de l'application qui fournit le document : il ne doit pas pouvoir désigner un autre dossier.
pub fn nom_sur(nom: &str) -> String {
    let dernier = nom.rsplit(['/', '\\']).next().unwrap_or_default();
    let filtre: String = dernier.chars().filter(|c| !c.is_control()).collect();
    let propre = filtre.trim();
    if propre.is_empty() || propre == "." || propre == ".." {
        return "fichier".into();
    }
    let mut fin = propre.len().min(LONGUEUR_NOM_MAX);
    while !propre.is_char_boundary(fin) {
        fin -= 1;
    }
    propre[..fin].to_string()
}

/// Un chemin libre dans `dossier` : l'import n'écrase jamais un fichier existant.
pub fn chemin_libre(dossier: &Path, nom: &str) -> Option<PathBuf> {
    let (base, extension) = match nom.rsplit_once('.') {
        Some((b, e)) if !b.is_empty() => (b, format!(".{e}")),
        _ => (nom, String::new()),
    };
    (0..1000)
        .map(|i| if i == 0 { nom.to_string() } else { format!("{base} ({i}){extension}") })
        .map(|n| dossier.join(n))
        .find(|c| !c.exists())
}

#[cfg(target_os = "android")]
pub(crate) mod jvm {
    use jni::objects::{JClass, JObject};
    use jni::{JNIEnv, JavaVM};

    /// Appelle une classe de l'application ; le message d'une exception ne quitte pas la JVM, il pourrait citer une donnée.
    pub fn avec_classe<T>(
        nom: &str,
        erreur: &str,
        action: impl for<'a, 'b> FnOnce(&'b mut JNIEnv<'a>, &'b JClass<'a>, &'b JObject<'a>) -> jni::errors::Result<T>,
    ) -> Result<T, String> {
        let contexte_ndk = ndk_context::android_context();
        // SAFETY : tao initialise ces pointeurs au démarrage, avec une référence globale qui vit autant que le processus.
        let vm = unsafe { JavaVM::from_raw(contexte_ndk.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let contexte = unsafe { JObject::from_raw(contexte_ndk.context().cast()) };
        let resultat = (|| {
            // Depuis un fil natif, FindClass ne voit pas les classes de l'application.
            let chargeur = env.call_method(&contexte, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
            let nom = env.new_string(nom)?;
            let classe = env.call_method(&chargeur, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[(&nom).into()])?.l()?;
            let classe = JClass::from(classe);
            action(&mut env, &classe, &contexte)
        })();
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return Err(erreur.into());
        }
        resultat.map_err(|_| erreur.to_string())
    }
}

#[cfg(target_os = "android")]
mod imp {
    use std::path::Path;

    use jni::objects::{JString, JValue};

    use super::jvm::avec_classe;

    const CLASSE: &str = "app.mlsconnect.desktop.FichiersAndroid";
    const ERREUR: &str = "Accès au fichier de l'appareil impossible.";
    const SIGNATURE_COPIE: &str = "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)V";

    fn texte(chemin: &Path) -> Result<&str, String> {
        chemin.to_str().ok_or_else(|| ERREUR.to_string())
    }

    pub fn nom(uri: &str) -> Result<Option<String>, String> {
        avec_classe(CLASSE, ERREUR, |env, classe, contexte| {
            let uri = env.new_string(uri)?;
            let args: [JValue; 2] = [contexte.into(), (&uri).into()];
            let valeur = env.call_static_method(classe, "nom", "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;", &args)?.l()?;
            if valeur.is_null() {
                return Ok(None);
            }
            let valeur = JString::from(valeur);
            let nom: String = env.get_string(&valeur)?.into();
            Ok(Some(nom))
        })
    }

    pub fn copier_vers(uri: &str, destination: &Path) -> Result<(), String> {
        let destination = texte(destination)?;
        avec_classe(CLASSE, ERREUR, |env, classe, contexte| {
            let uri = env.new_string(uri)?;
            let destination = env.new_string(destination)?;
            let args: [JValue; 3] = [contexte.into(), (&uri).into(), (&destination).into()];
            env.call_static_method(classe, "lire", SIGNATURE_COPIE, &args)?;
            Ok(())
        })
    }

    pub fn copier_depuis(source: &Path, uri: &str) -> Result<(), String> {
        let source = texte(source)?;
        avec_classe(CLASSE, ERREUR, |env, classe, contexte| {
            let source = env.new_string(source)?;
            let uri = env.new_string(uri)?;
            let args: [JValue; 3] = [contexte.into(), (&source).into(), (&uri).into()];
            env.call_static_method(classe, "ecrire", SIGNATURE_COPIE, &args)?;
            Ok(())
        })
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use std::path::Path;

    const ABSENT: &str = "Les URI content:// n'existent que sur Android.";

    pub fn nom(_uri: &str) -> Result<Option<String>, String> {
        Err(ABSENT.into())
    }

    pub fn copier_vers(_uri: &str, _destination: &Path) -> Result<(), String> {
        Err(ABSENT.into())
    }

    pub fn copier_depuis(_source: &Path, _uri: &str) -> Result<(), String> {
        Err(ABSENT.into())
    }
}

pub use imp::{copier_depuis, copier_vers, nom};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_nom_fourni_ne_sort_pas_du_dossier() {
        assert_eq!(nom_sur("../../shared_prefs/mc_trousseau.xml"), "mc_trousseau.xml");
        assert_eq!(nom_sur("..\\..\\clé.pem"), "clé.pem");
        assert_eq!(nom_sur(".."), "fichier");
        assert_eq!(nom_sur("a/"), "fichier");
        assert_eq!(nom_sur("  \n"), "fichier");
        assert_eq!(nom_sur("ligne\r\nsuite.txt"), "lignesuite.txt");
    }

    #[test]
    fn un_nom_long_est_coupe_sur_un_caractere_entier() {
        let long = "é".repeat(150);
        let coupe = nom_sur(&long);
        assert!(coupe.len() <= LONGUEUR_NOM_MAX);
        assert_eq!(coupe.chars().count(), 100);
    }

    #[test]
    fn un_fichier_existant_n_est_pas_ecrase() {
        let dossier = std::env::temp_dir().join(format!("mc-appareil-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        assert_eq!(chemin_libre(&dossier, "a.txt").unwrap(), dossier.join("a.txt"));
        std::fs::write(dossier.join("a.txt"), "x").unwrap();
        assert_eq!(chemin_libre(&dossier, "a.txt").unwrap(), dossier.join("a (1).txt"));
        std::fs::write(dossier.join(".cache"), "x").unwrap();
        assert_eq!(chemin_libre(&dossier, ".cache").unwrap(), dossier.join(".cache (1)"));
        std::fs::remove_dir_all(&dossier).unwrap();
    }
}
