//! Secrets confiés au trousseau du système, sur demande explicite, un par profil.
//! Les appels sont bloquants : à exécuter hors des tâches asynchrones.

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod imp {
    use keyring::{Entry, Error};

    const SERVICE: &str = "MLS Connect";

    fn entree(id: &str) -> Result<Entry, String> {
        Entry::new(SERVICE, &format!("profil-{id}")).map_err(|e| e.to_string())
    }

    pub fn lire(id: &str) -> Result<Option<String>, String> {
        match entree(id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn ecrire(id: &str, secret: &str) -> Result<(), String> {
        entree(id)?.set_password(secret).map_err(|e| e.to_string())
    }

    pub fn effacer(id: &str) -> Result<(), String> {
        match entree(id)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub const DISPONIBLE: bool = true;
}

/// Android : le chiffrement est confié au Keystore, côté Kotlin (`CoffreAndroid`).
#[cfg(target_os = "android")]
mod imp {
    use jni::objects::{JClass, JObject, JString, JValue};
    use jni::JNIEnv;

    use crate::appareil::jvm::avec_classe;

    const CLASSE: &str = "app.mlsconnect.desktop.CoffreAndroid";
    const ARGS_ID: &str = "(Landroid/content/Context;Ljava/lang/String;)";
    const INDISPONIBLE: &str = "coffre Android indisponible";

    fn avec_coffre<T>(action: impl for<'a, 'b> FnOnce(&'b mut JNIEnv<'a>, &'b JClass<'a>, &'b JObject<'a>) -> jni::errors::Result<T>) -> Result<T, String> {
        avec_classe(CLASSE, INDISPONIBLE, action)
    }

    pub fn lire(id: &str) -> Result<Option<String>, String> {
        avec_coffre(|env, classe, contexte| {
            let id = env.new_string(id)?;
            let signature = format!("{ARGS_ID}Ljava/lang/String;");
            let valeur = env.call_static_method(classe, "lire", signature, &[contexte.into(), (&id).into()])?.l()?;
            if valeur.is_null() {
                return Ok(None);
            }
            let texte = JString::from(valeur);
            let secret: String = env.get_string(&texte)?.into();
            Ok(Some(secret))
        })
    }

    pub fn ecrire(id: &str, secret: &str) -> Result<(), String> {
        avec_coffre(|env, classe, contexte| {
            let id = env.new_string(id)?;
            let secret = env.new_string(secret)?;
            let args: [JValue; 3] = [contexte.into(), (&id).into(), (&secret).into()];
            env.call_static_method(classe, "ecrire", "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)V", &args)?;
            Ok(())
        })
    }

    pub fn effacer(id: &str) -> Result<(), String> {
        avec_coffre(|env, classe, contexte| {
            let id = env.new_string(id)?;
            env.call_static_method(classe, "effacer", format!("{ARGS_ID}V"), &[contexte.into(), (&id).into()])?;
            Ok(())
        })
    }

    pub const DISPONIBLE: bool = true;
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux", target_os = "android")))]
mod imp {
    const ABSENT: &str = "trousseau non disponible sur ce système";

    pub fn lire(_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    pub fn ecrire(_id: &str, _secret: &str) -> Result<(), String> {
        Err(ABSENT.into())
    }

    pub fn effacer(_id: &str) -> Result<(), String> {
        Ok(())
    }

    pub const DISPONIBLE: bool = false;
}

pub use imp::{effacer, ecrire, lire, DISPONIBLE};
