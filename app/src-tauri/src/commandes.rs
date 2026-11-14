use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as MutexSync, MutexGuard};

use mc_core::{appareil, trousseau};
use mc_core::connexion::{connecter_par as ouvrir, CodeErreur, AGENT_DISPONIBLE, Decision, Decisions, ErreurConnexion, Evenement, Rappel, Session};
use mc_core::cles::{CodeCle, ErreurCle, GenreCle, InfoCle, MagasinCles};
use mc_core::echange::{self, Apercu, BilanExport, BilanImport, CodeEchange, Doublon, ErreurEchange};
use mc_core::hotes::{HoteConnu, MagasinHotes};
use mc_core::profils::{ErreurProfils, MagasinProfils, Profil};
use mc_core::ftp::{AccesFtp, Passerelle};
use mc_core::sftp::{self, Bilan, Conflit, Controle, Entree, Progres, Sftp, SurProgres};
use mc_core::terminal::{Fin, Sortie, Terminal};
use mc_core::tunnels::{CodeTunnel, ErreurTunnel, Mesure, Regle, Tunnel};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};
use tokio::sync::{Mutex, Notify};

#[derive(Clone)]
pub struct Identifiants {
    profil: Profil,
    secret: Option<String>,
    secret_rebond: Option<String>,
}

pub struct Etat {
    pub cles: Arc<MagasinCles>,
    pub profils: MagasinProfils,
    pub hotes: Arc<MagasinHotes>,
    pub decisions: Decisions,
    pub en_cours: AtomicBool,
    pub annulation: Notify,
    pub session: Mutex<Option<Arc<Session>>>,
    /// Conservés en mémoire seulement, le temps de la session, pour la reconnexion.
    pub identifiants: Mutex<Option<Identifiants>>,
    pub terminaux: MutexSync<HashMap<u64, Terminal>>,
    pub prochain_terminal: AtomicU64,
    pub sftp: Mutex<Option<Arc<Sftp>>>,
    pub transferts: MutexSync<HashMap<u64, Arc<Controle>>>,
    pub tunnels: Mutex<HashMap<String, Tunnel>>,
    pub ftp: Mutex<Option<Passerelle>>,
}

impl Etat {
    pub async fn session_active(&self) -> bool {
        self.session.lock().await.as_ref().is_some_and(|s| s.est_ouverte())
    }

    fn terminaux(&self) -> MutexGuard<'_, HashMap<u64, Terminal>> {
        self.terminaux.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn transferts(&self) -> MutexGuard<'_, HashMap<u64, Arc<Controle>>> {
        self.transferts.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Le canal SFTP s'ouvre au premier usage et sert toute la session.
    async fn canal_sftp(&self) -> Result<Arc<Sftp>, String> {
        let mut canal = self.sftp.lock().await;
        if let Some(sftp) = canal.as_ref() {
            return Ok(sftp.clone());
        }
        let session = self.session.lock().await.clone().ok_or("Aucune session ouverte.")?;
        let sftp = Arc::new(session.ouvrir_sftp().await?);
        *canal = Some(sftp.clone());
        Ok(sftp)
    }

    /// Ferme la session et ses terminaux, sans oublier les identifiants.
    async fn fermer_session(&self) {
        self.terminaux().clear();
        self.tunnels.lock().await.clear();
        *self.ftp.lock().await = None;
        for controle in self.transferts().values() {
            controle.annuler();
        }
        *self.sftp.lock().await = None;
        if let Some(session) = self.session.lock().await.take() {
            session.fermer().await;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilsCharges {
    profils: Vec<Profil>,
    chemin: String,
}

#[tauri::command]
pub fn profils_charger(etat: State<'_, Arc<Etat>>) -> Result<ProfilsCharges, ErreurProfils> {
    Ok(ProfilsCharges {
        profils: etat.profils.charger()?,
        chemin: etat.profils.chemin().display().to_string(),
    })
}

#[tauri::command]
pub fn profils_enregistrer(etat: State<'_, Arc<Etat>>, profils: Vec<Profil>) -> Result<(), ErreurProfils> {
    etat.profils.enregistrer(&profils)
}

fn erreur_echange(message: String) -> ErreurEchange {
    ErreurEchange { code: CodeEchange::Disque, message }
}

fn erreur_profils(e: ErreurProfils) -> ErreurEchange {
    erreur_echange(format!("{} {}", e.message, e.detail.unwrap_or_default()).trim().to_string())
}

/// Texte d'un export, depuis un chemin ou un document de l'appareil.
fn lire_export(app: &AppHandle, source: &str) -> Result<String, ErreurEchange> {
    const TAILLE_MAX: u64 = 8 * 1024 * 1024;
    let lire = |chemin: &Path| -> Result<String, ErreurEchange> {
        if std::fs::metadata(chemin)?.len() > TAILLE_MAX {
            return Err(ErreurEchange { code: CodeEchange::Format, message: "Ce fichier est trop grand pour un export de profils.".into() });
        }
        std::fs::read_to_string(chemin)
            .map_err(|_| ErreurEchange { code: CodeEchange::Format, message: "Ce fichier n'est pas un export de profils lisible.".into() })
    };
    if !appareil::est_uri(source) {
        return lire(Path::new(source));
    }
    avec_temporaire(app, "profils.mcprofils", erreur_echange, |temporaire| {
        appareil::copier_vers(source, temporaire).map_err(erreur_echange)?;
        lire(temporaire)
    })
}

/// Exporte les profils choisis (et leurs rebonds). `phrase` : clés privées incluses, chiffrées.
#[tauri::command]
pub async fn profils_exporter(
    app: AppHandle,
    etat: State<'_, Arc<Etat>>,
    ids: Vec<String>,
    destination: String,
    phrase: Option<String>,
) -> Result<BilanExport, ErreurEchange> {
    let etat = etat.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let profils = etat.profils.charger().map_err(erreur_profils)?;
        let (texte, bilan) = echange::exporter(&profils, &ids, phrase.as_deref(), &etat.hotes)?;
        if appareil::est_uri(&destination) {
            avec_temporaire(&app, "profils.mcprofils", erreur_echange, |temporaire| {
                std::fs::write(temporaire, &texte)?;
                appareil::copier_depuis(temporaire, &destination).map_err(erreur_echange)
            })?;
        } else {
            let chemin = Path::new(&destination);
            if chemin.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() != Some(echange::EXTENSION) {
                return Err(ErreurEchange { code: CodeEchange::Format, message: "Choisissez un fichier .mcprofils.".into() });
            }
            std::fs::write(chemin, &texte)?;
        }
        Ok(bilan)
    })
    .await
    .map_err(|e| erreur_echange(e.to_string()))?
}

#[tauri::command]
pub async fn profils_apercu(app: AppHandle, etat: State<'_, Arc<Etat>>, source: String) -> Result<Apercu, ErreurEchange> {
    let etat = etat.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let texte = lire_export(&app, &source)?;
        echange::apercu(&texte, &etat.profils.charger().map_err(erreur_profils)?)
    })
    .await
    .map_err(|e| erreur_echange(e.to_string()))?
}

/// Fusionne un export avec les profils locaux, puis enregistre la liste obtenue.
#[tauri::command]
pub async fn profils_importer(
    app: AppHandle,
    etat: State<'_, Arc<Etat>>,
    source: String,
    phrase: Option<String>,
    doublon: Doublon,
) -> Result<BilanImport, ErreurEchange> {
    let etat = etat.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let texte = lire_export(&app, &source)?;
        let locaux = etat.profils.charger().map_err(erreur_profils)?;
        let (liste, bilan) = echange::importer(&texte, phrase.as_deref(), doublon, &locaux, &etat.cles, &etat.hotes)?;
        etat.profils.enregistrer(&liste).map_err(erreur_profils)?;
        Ok(bilan)
    })
    .await
    .map_err(|e| erreur_echange(e.to_string()))?
}

/// Remet le drapeau à zéro quelle que soit l'issue, y compris si la commande est abandonnée.
struct FinConnexion<'a>(&'a AtomicBool);

impl Drop for FinConnexion<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn occupe(titre: &str, texte: &str) -> ErreurConnexion {
    ErreurConnexion::simple(CodeErreur::Occupe, titre, texte.into())
}

/// Le rebond désigne un profil enregistré, à un seul niveau.
fn trouver_rebond(etat: &Etat, profil: &Profil) -> Result<Option<Profil>, ErreurConnexion> {
    if profil.rebond.is_empty() {
        return Ok(None);
    }
    let erreur = |texte: &str| ErreurConnexion::simple(CodeErreur::Injoignable, "Rebond impossible", texte.into());
    if profil.rebond == profil.id {
        return Err(erreur("Un profil ne peut pas être son propre rebond."));
    }
    let profils = etat.profils.charger().map_err(|e| erreur(&e.message))?;
    let rebond = profils.into_iter().find(|p| p.id == profil.rebond).ok_or_else(|| erreur("Le profil de rebond n'existe plus."))?;
    Ok(Some(Profil { rebond: String::new(), ..rebond }))
}

/// Les appels au trousseau sont bloquants, et imbriquent un exécuteur sous Linux.
async fn lire_trousseau(id: &str) -> Option<String> {
    let id = id.to_string();
    tauri::async_runtime::spawn_blocking(move || trousseau::lire(&id)).await.ok()?.unwrap_or_else(|e| {
        log::warn!("lecture du trousseau impossible : {e}");
        None
    })
}

async fn etablir(
    etat: &Etat,
    profil: &Profil,
    secret: Option<String>,
    secret_rebond: Option<String>,
    canal: Channel<Evenement>,
) -> Result<Arc<Session>, ErreurConnexion> {
    let rebond = match trouver_rebond(etat, profil)? {
        Some(p) => {
            let saisi = secret_rebond.filter(|s| !s.is_empty());
            let secret = if saisi.is_none() && p.memoriser { lire_trousseau(&p.id).await } else { saisi };
            Some((p, secret))
        }
        None => None,
    };    let rappel: Rappel = Arc::new(move |e| {
        let _ = canal.send(e);
    });
    let tentative = ouvrir(profil, secret, rebond, etat.hotes.clone(), etat.decisions.clone(), rappel);
    tokio::select! {
        resultat = tentative => resultat.map(Arc::new),
        _ = etat.annulation.notified() => {
            Err(ErreurConnexion::simple(CodeErreur::Annulee, "Connexion annulée", "La connexion a été annulée.".into()))
        }
    }
}

#[tauri::command]
pub async fn connecter(
    app: AppHandle,
    etat: State<'_, Arc<Etat>>,
    profil: Profil,
    secret: Option<String>,
    secret_rebond: Option<String>,
    canal: Channel<Evenement>,
) -> Result<Option<String>, ErreurConnexion> {
    if etat.en_cours.swap(true, Ordering::SeqCst) {
        return Err(occupe("Connexion déjà en cours", "Patientez ou annulez-la."));
    }
    let _fin = FinConnexion(&etat.en_cours);
    if etat.session_active().await {
        return Err(occupe("Session déjà ouverte", "Déconnectez-vous d'abord."));
    }
    let saisi = secret.filter(|s| !s.is_empty());
    let secret = match &saisi {
        None if profil.memoriser => lire_trousseau(&profil.id).await,
        _ => saisi.clone(),
    };
    let session = etablir(&etat, &profil, secret.clone(), secret_rebond.clone(), canal).await?;    *etat.session.lock().await = Some(session);
    // Après une connexion réussie seulement : un secret refusé n'est pas enregistré.
    let avertissement = match saisi.filter(|_| profil.memoriser) {
        Some(valeur) => {
            let id = profil.id.clone();
            tauri::async_runtime::spawn_blocking(move || trousseau::ecrire(&id, &valeur))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r)
                .err()
                .map(|e| format!("Secret non enregistré dans le trousseau : {e}."))
        }
        None => None,
    };
    *etat.identifiants.lock().await = Some(Identifiants { profil, secret, secret_rebond });
    #[cfg(desktop)]
    crate::barre::libelle_connexion(&app, true);
    let _ = &app;
    Ok(avertissement)
}

/// Rétablit une session perdue. Les vérifications de clé d'hôte s'appliquent comme au premier essai.
#[tauri::command]
pub async fn reconnecter(etat: State<'_, Arc<Etat>>, canal: Channel<Evenement>) -> Result<(), ErreurConnexion> {
    if etat.en_cours.swap(true, Ordering::SeqCst) {
        return Err(occupe("Connexion déjà en cours", "Patientez."));
    }
    let _fin = FinConnexion(&etat.en_cours);
    let Some(Identifiants { profil, secret, secret_rebond }) = etat.identifiants.lock().await.clone() else {
        return Err(occupe("Reconnexion impossible", "Aucune session à rétablir."));
    };
    etat.fermer_session().await;
    let session = etablir(&etat, &profil, secret, secret_rebond, canal).await?;
    *etat.session.lock().await = Some(session);
    Ok(())
}

#[tauri::command]
pub fn annuler_connexion(etat: State<'_, Arc<Etat>>) {
    if etat.en_cours.load(Ordering::SeqCst) {
        etat.annulation.notify_one();
    }
}

#[tauri::command]
pub fn decider(etat: State<'_, Arc<Etat>>, decision: Decision) {
    etat.decisions.repondre(decision);
}

#[tauri::command]
pub async fn deconnecter(app: AppHandle, etat: State<'_, Arc<Etat>>) -> Result<(), ()> {
    etat.fermer_session().await;
    *etat.identifiants.lock().await = None;
    #[cfg(desktop)]
    crate::barre::libelle_connexion(&app, false);
    let _ = &app;
    Ok(())
}

#[tauri::command]
pub async fn terminal_ouvrir(
    etat: State<'_, Arc<Etat>>,
    terme: String,
    cols: u32,
    rows: u32,
    sortie: Channel<InvokeResponseBody>,
    fin: Channel<Option<u32>>,
) -> Result<u64, String> {
    // Copie de l'Arc : le verrou n'est pas tenu pendant l'ouverture du canal.
    let session = etat.session.lock().await.clone().ok_or("Aucune session ouverte.")?;
    let sur_sortie: Sortie = Box::new(move |octets| {
        let _ = sortie.send(InvokeResponseBody::Raw(octets));
    });
    let sur_fin: Fin = Box::new(move |code| {
        let _ = fin.send(code);
    });
    let terminal = session.ouvrir_terminal(&terme, cols, rows, sur_sortie, sur_fin).await?;
    let id = etat.prochain_terminal.fetch_add(1, Ordering::Relaxed);
    etat.terminaux().insert(id, terminal);
    Ok(id)
}

// Commandes synchrones : traitées dans l'ordre d'arrivée, ce qui préserve l'ordre des frappes.
#[tauri::command]
pub fn terminal_ecrire(etat: State<'_, Arc<Etat>>, id: u64, donnees: Vec<u8>) {
    if let Some(terminal) = etat.terminaux().get(&id) {
        terminal.ecrire(donnees);
    }
}

#[tauri::command]
pub fn terminal_redimensionner(etat: State<'_, Arc<Etat>>, id: u64, cols: u32, rows: u32) {
    if let Some(terminal) = etat.terminaux().get(&id) {
        terminal.redimensionner(cols, rows);
    }
}

#[tauri::command]
pub fn terminal_fermer(etat: State<'_, Arc<Etat>>, id: u64) {
    etat.terminaux().remove(&id);
}

#[derive(Serialize)]
pub struct Accueils {
    local: String,
    distant: String,
}

fn dossier_fichiers(app: &AppHandle) -> Result<PathBuf, String> {
    let dossier = app.path().app_data_dir().map_err(|e| e.to_string())?.join("fichiers");
    std::fs::create_dir_all(&dossier).map_err(|e| e.to_string())?;
    Ok(dossier)
}

/// Les échanges avec l'appareil restent limités au dossier de fichiers de l'application.
fn dans_fichiers(app: &AppHandle, chemin: &str) -> Result<PathBuf, String> {
    let racine = dossier_fichiers(app)?.canonicalize().map_err(|e| e.to_string())?;
    let cible = Path::new(chemin).canonicalize().map_err(|e| e.to_string())?;
    if cible.starts_with(&racine) {
        Ok(cible)
    } else {
        Err("Ce chemin est hors du dossier de l'application.".into())
    }
}

/// Exécute `action` sur un fichier du cache privé, effacé ensuite quel que soit le résultat.
fn avec_temporaire<T, E>(
    app: &AppHandle,
    nom: &str,
    erreur: impl Fn(String) -> E,
    action: impl FnOnce(&Path) -> Result<T, E>,
) -> Result<T, E> {
    let horloge = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let dossier = app.path().app_cache_dir().map_err(|e| erreur(e.to_string()))?.join(format!("mc-{horloge}"));
    std::fs::create_dir_all(&dossier).map_err(|e| erreur(e.to_string()))?;
    let resultat = action(&dossier.join(appareil::nom_sur(nom)));
    let _ = std::fs::remove_dir_all(&dossier);
    resultat
}

/// Copie des documents choisis dans le sélecteur système ; renvoie le nombre de fichiers importés.
#[tauri::command]
pub async fn appareil_importer(app: AppHandle, uris: Vec<String>, dossier: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dossier = dans_fichiers(&app, &dossier)?;
        for uri in &uris {
            if !appareil::est_uri(uri) {
                return Err("Document de l'appareil attendu.".to_string());
            }
            let nom = appareil::nom_sur(&appareil::nom(uri)?.unwrap_or_default());
            let cible = appareil::chemin_libre(&dossier, &nom).ok_or("Trop de fichiers portent déjà ce nom.")?;
            let partiel = cible.with_file_name(format!(".{nom}.mc-partiel"));
            let resultat = appareil::copier_vers(uri, &partiel).and_then(|_| std::fs::rename(&partiel, &cible).map_err(|e| e.to_string()));
            if let Err(e) = resultat {
                let _ = std::fs::remove_file(&partiel);
                return Err(e);
            }
        }
        Ok(uris.len())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn appareil_exporter(app: AppHandle, source: String, uri: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = dans_fichiers(&app, &source)?;
        if !appareil::est_uri(&uri) {
            return Err("Document de l'appareil attendu.".to_string());
        }
        appareil::copier_depuis(&source, &uri)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn sftp_accueils(app: AppHandle, etat: State<'_, Arc<Etat>>) -> Result<Accueils, String> {
    // Android n'expose que le dossier privé de l'application sans permission de stockage.
    let local = if cfg!(mobile) {
        dossier_fichiers(&app)?.to_string_lossy().into_owned()
    } else {
        sftp::accueil_local()
    };
    Ok(Accueils { local, distant: etat.canal_sftp().await?.accueil().await? })
}

#[tauri::command]
pub async fn local_lister(chemin: String) -> Result<Vec<Entree>, String> {
    sftp::lister_local(&chemin).await
}

#[tauri::command]
pub async fn local_creer_dossier(chemin: String) -> Result<(), String> {
    sftp::creer_dossier_local(&chemin).await
}

#[tauri::command]
pub async fn local_renommer(ancien: String, nouveau: String) -> Result<(), String> {
    sftp::renommer_local(&ancien, &nouveau).await
}

#[tauri::command]
pub async fn local_supprimer(chemin: String) -> Result<(), String> {
    sftp::supprimer_local(&chemin).await
}

#[tauri::command]
pub async fn sftp_lister(etat: State<'_, Arc<Etat>>, chemin: String) -> Result<Vec<Entree>, String> {
    etat.canal_sftp().await?.lister(&chemin).await
}

#[tauri::command]
pub async fn sftp_creer_dossier(etat: State<'_, Arc<Etat>>, chemin: String) -> Result<(), String> {
    etat.canal_sftp().await?.creer_dossier(&chemin).await
}

#[tauri::command]
pub async fn sftp_renommer(etat: State<'_, Arc<Etat>>, ancien: String, nouveau: String) -> Result<(), String> {
    etat.canal_sftp().await?.renommer(&ancien, &nouveau).await
}

#[tauri::command]
pub async fn sftp_supprimer(etat: State<'_, Arc<Etat>>, chemin: String) -> Result<(), String> {
    etat.canal_sftp().await?.supprimer(&chemin).await
}

#[tauri::command]
pub async fn sftp_permissions(etat: State<'_, Arc<Etat>>, chemin: String, mode: u32) -> Result<(), String> {
    etat.canal_sftp().await?.changer_mode(&chemin, mode).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn sftp_transferer(
    etat: State<'_, Arc<Etat>>,
    id: u64,
    envoi: bool,
    source: String,
    dossier: String,
    conflit: Conflit,
    canal: Channel<Progres>,
) -> Result<Bilan, String> {
    let sftp = etat.canal_sftp().await?;
    let controle = Arc::new(Controle::default());
    etat.transferts().insert(id, controle.clone());
    let rappel: SurProgres = Box::new(move |p| {
        let _ = canal.send(p);
    });
    let resultat = if envoi {
        sftp.envoyer(Path::new(&source), &dossier, conflit, &controle, &rappel).await
    } else {
        sftp.telecharger(&source, Path::new(&dossier), conflit, &controle, &rappel).await
    };
    etat.transferts().remove(&id);
    resultat
}

#[tauri::command]
pub fn transfert_pause(etat: State<'_, Arc<Etat>>, id: u64, active: bool) {
    if let Some(controle) = etat.transferts().get(&id) {
        controle.pause(active);
    }
}

#[tauri::command]
pub fn transfert_annuler(etat: State<'_, Arc<Etat>>, id: u64) {
    if let Some(controle) = etat.transferts().get(&id) {
        controle.annuler();
    }
}

#[tauri::command]
pub fn afficher_fenetre(app: AppHandle) {
    afficher(&app);
}

#[tauri::command]
pub async fn quitter(app: AppHandle, etat: State<'_, Arc<Etat>>) -> Result<(), ()> {
    etat.fermer_session().await;
    app.exit(0);
    Ok(())
}

pub fn afficher(app: &AppHandle) {
    if let Some(fenetre) = app.get_webview_window("main") {
        let _ = fenetre.show();
        #[cfg(desktop)]
        let _ = fenetre.unminimize();
        let _ = fenetre.set_focus();
    }
}

/// Remplace une écoute déjà ouverte pour cette règle : une modification s'applique pendant la session.
/// Une passerelle déjà ouverte est conservée, avec ses accès.
#[tauri::command]
pub async fn ftp_ouvrir(etat: State<'_, Arc<Etat>>, port: u16) -> Result<AccesFtp, String> {
    let mut ftp = etat.ftp.lock().await;
    if let Some(passerelle) = ftp.as_ref() {
        return Ok(passerelle.acces.clone());
    }
    let sftp = etat.canal_sftp().await?;
    let passerelle = Passerelle::ouvrir(sftp, port).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::AddrInUse => format!("Le port {port} est déjà utilisé."),
        _ => e.to_string(),
    })?;
    let acces = passerelle.acces.clone();
    *ftp = Some(passerelle);
    Ok(acces)
}

#[tauri::command]
pub async fn ftp_etat(etat: State<'_, Arc<Etat>>) -> Result<Option<AccesFtp>, ()> {
    Ok(etat.ftp.lock().await.as_ref().map(|p| p.acces.clone()))
}

#[tauri::command]
pub async fn ftp_fermer(etat: State<'_, Arc<Etat>>) -> Result<(), ()> {
    *etat.ftp.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn tunnel_ouvrir(etat: State<'_, Arc<Etat>>, regle: Regle) -> Result<u16, ErreurTunnel> {
    let session = etat.session.lock().await.clone().ok_or(ErreurTunnel { code: CodeTunnel::Session, message: "Aucune session ouverte.".into() })?;
    let mut tunnels = etat.tunnels.lock().await;
    if let Some(ancien) = tunnels.remove(&regle.id) {
        ancien.fermer().await;
    }
    let tunnel = session.ouvrir_tunnel(&regle).await?;
    let port = tunnel.port;
    tunnels.insert(regle.id, tunnel);
    Ok(port)
}

#[tauri::command]
pub async fn tunnel_fermer(etat: State<'_, Arc<Etat>>, id: String) -> Result<(), ()> {
    let tunnel = etat.tunnels.lock().await.remove(&id);
    if let Some(tunnel) = tunnel {
        tunnel.fermer().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn tunnel_mesures(etat: State<'_, Arc<Etat>>) -> Result<HashMap<String, Mesure>, ()> {
    Ok(etat.tunnels.lock().await.iter().map(|(id, t)| (id.clone(), t.mesure())).collect())
}

/// N'écrit que les règles : les modifications non enregistrées du formulaire restent en brouillon.
#[tauri::command]
pub fn profil_tunnels(etat: State<'_, Arc<Etat>>, id: String, regles: Vec<Regle>) -> Result<bool, ErreurProfils> {
    let mut profils = etat.profils.charger()?;
    let Some(profil) = profils.iter_mut().find(|p| p.id == id) else {
        return Ok(false);
    };
    profil.tunnels = regles;
    etat.profils.enregistrer(&profils)?;
    Ok(true)
}

#[tauri::command]
pub fn repondre_clavier(etat: State<'_, Arc<Etat>>, reponses: Option<Vec<String>>) {
    etat.decisions.repondre_clavier(reponses);
}

#[tauri::command]
pub fn cles_lister(etat: State<'_, Arc<Etat>>) -> Result<Vec<InfoCle>, ErreurCle> {
    etat.cles.lister()
}

/// La génération RSA dure plusieurs secondes : hors du fil de l'interface.
#[tauri::command]
pub async fn cles_generer(
    etat: State<'_, Arc<Etat>>,
    nom: String,
    genre: GenreCle,
    commentaire: String,
    phrase: Option<String>,
) -> Result<InfoCle, ErreurCle> {
    let cles = etat.cles.clone();
    tauri::async_runtime::spawn_blocking(move || cles.generer(&nom, genre, &commentaire, phrase.as_deref()))
        .await
        .map_err(|e| ErreurCle { code: CodeCle::Disque, message: e.to_string() })?
}

#[tauri::command]
pub async fn cles_importer(app: AppHandle, etat: State<'_, Arc<Etat>>, chemin: String, phrase: Option<String>) -> Result<InfoCle, ErreurCle> {
    let etat = etat.inner().clone();
    let erreur = |message: String| ErreurCle { code: CodeCle::Format, message };
    tauri::async_runtime::spawn_blocking(move || {
        if !appareil::est_uri(&chemin) {
            return etat.cles.importer(Path::new(&chemin), phrase.as_deref());
        }
        let nom = appareil::nom(&chemin).map_err(erreur)?.unwrap_or_else(|| "cle".into());
        avec_temporaire(&app, &nom, erreur, |temporaire| {
            appareil::copier_vers(&chemin, temporaire).map_err(erreur)?;
            etat.cles.importer(temporaire, phrase.as_deref())
        })
    })
    .await
    .map_err(|e| erreur(e.to_string()))?
}

#[tauri::command]
pub fn cles_supprimer(etat: State<'_, Arc<Etat>>, nom: String) -> Result<(), ErreurCle> {
    etat.cles.supprimer(&nom)
}

#[tauri::command]
pub fn cle_exporter_publique(app: AppHandle, etat: State<'_, Arc<Etat>>, nom: String, destination: String) -> Result<(), ErreurCle> {
    if !appareil::est_uri(&destination) {
        return etat.cles.exporter_publique(&nom, Path::new(&destination));
    }
    let erreur = |message: String| ErreurCle { code: CodeCle::Format, message };
    avec_temporaire(&app, "cle.pub", erreur, |temporaire| {
        etat.cles.exporter_publique(&nom, temporaire)?;
        appareil::copier_depuis(temporaire, &destination).map_err(erreur)
    })
}

/// Écrit le journal choisi par l'utilisateur ; seules les extensions de texte sont acceptées.
#[tauri::command]
pub fn journal_exporter(app: AppHandle, destination: String, contenu: String) -> Result<(), String> {
    if appareil::est_uri(&destination) {
        return avec_temporaire(&app, "journal.log", |e| e, |temporaire| {
            std::fs::write(temporaire, contenu).map_err(|e| e.to_string())?;
            appareil::copier_depuis(temporaire, &destination)
        });
    }
    let chemin = Path::new(&destination);
    let extension = chemin.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    if extension != "log" && extension != "txt" {
        return Err("Choisissez un fichier .log ou .txt.".into());
    }
    std::fs::write(chemin, contenu).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn hotes_lister(etat: State<'_, Arc<Etat>>) -> Result<Vec<HoteConnu>, ErreurCle> {
    Ok(etat.hotes.lister()?)
}

#[tauri::command]
pub fn hotes_supprimer(etat: State<'_, Arc<Etat>>, motif: String, empreinte: String) -> Result<bool, ErreurCle> {
    Ok(etat.hotes.supprimer(&motif, &empreinte)?)
}

#[derive(Serialize)]
pub struct Capacites {
    mobile: bool,
    agent: bool,
    trousseau: bool,
}

#[tauri::command]
pub fn capacites() -> Capacites {
    Capacites { mobile: cfg!(mobile), agent: AGENT_DISPONIBLE, trousseau: trousseau::DISPONIBLE }
}

/// Parmi ces profils, ceux dont un secret est présent. Le secret lui-même ne quitte pas Rust.
#[tauri::command]
pub async fn trousseau_presents(ids: Vec<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ids.into_iter().filter(|id| matches!(trousseau::lire(id), Ok(Some(_)))).collect()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trousseau_oublier(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || trousseau::effacer(&id)).await.map_err(|e| e.to_string())?
}
