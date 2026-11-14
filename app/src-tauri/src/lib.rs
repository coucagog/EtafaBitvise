mod approbation;
mod commandes;
#[cfg(desktop)]
mod barre;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

use commandes::Etat;
use mc_core::cles::MagasinCles;
use mc_core::connexion::Decisions;
use mc_core::hotes::MagasinHotes;
use mc_core::profils::MagasinProfils;
use tauri::Manager;
use tokio::sync::{Mutex, Notify};

fn journal_fichier() {
    let mut journal = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    // Fichier distinct de `yawrec.log`, en ajout : une seconde instance n'efface pas la première.
    let chemin = std::env::temp_dir().join("mlsconnect.log");
    if let Ok(fichier) = std::fs::OpenOptions::new().create(true).append(true).open(chemin) {
        journal.target(env_logger::Target::Pipe(Box::new(fichier)));
    }
    let _ = journal.try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    journal_fichier();
    let constructeur = tauri::Builder::default();

    #[cfg(desktop)]
    let constructeur = constructeur
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| commandes::afficher(app)))
        .plugin(barre::plugin_raccourcis());

    constructeur
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            commandes::profils_charger,
            commandes::profils_enregistrer,
            commandes::profils_exporter,
            commandes::profils_apercu,
            commandes::profils_importer,
            commandes::connecter,
            commandes::reconnecter,
            commandes::annuler_connexion,
            commandes::decider,
            commandes::deconnecter,
            commandes::terminal_ouvrir,
            commandes::terminal_ecrire,
            commandes::terminal_redimensionner,
            commandes::terminal_fermer,
            commandes::appareil_importer,
            commandes::appareil_exporter,
            commandes::sftp_accueils,
            commandes::local_lister,
            commandes::local_creer_dossier,
            commandes::local_renommer,
            commandes::local_supprimer,
            commandes::sftp_lister,
            commandes::sftp_creer_dossier,
            commandes::sftp_renommer,
            commandes::sftp_supprimer,
            commandes::sftp_permissions,
            commandes::tunnel_ouvrir,
            commandes::tunnel_fermer,
            commandes::tunnel_mesures,
            commandes::profil_tunnels,
            commandes::ftp_ouvrir,
            commandes::ftp_etat,
            commandes::ftp_fermer,
            commandes::repondre_clavier,
            commandes::capacites,
            commandes::trousseau_presents,
            commandes::trousseau_oublier,
            commandes::cles_lister,
            commandes::cles_generer,
            commandes::cles_importer,
            commandes::cles_supprimer,
            commandes::cle_exporter_publique,
            commandes::journal_exporter,
            commandes::hotes_lister,
            commandes::hotes_supprimer,
            commandes::sftp_transferer,
            commandes::transfert_pause,
            commandes::transfert_annuler,
            approbation::agent_demandes,
            approbation::agent_repondre,
            commandes::afficher_fenetre,
            commandes::quitter,
        ])
        .setup(|app| {
            let dossier = app.path().app_config_dir()?;
            app.manage(Arc::new(Etat {
                cles: Arc::new(MagasinCles::new(dossier.join("keys"))),
                profils: MagasinProfils::new(dossier.join("profiles.json")),
                hotes: Arc::new(MagasinHotes::new(dossier.join("known_hosts"))),
                decisions: Decisions::default(),
                en_cours: AtomicBool::new(false),
                annulation: Notify::new(),
                session: Mutex::new(None),
                identifiants: Mutex::new(None),
                terminaux: std::sync::Mutex::new(HashMap::new()),
                prochain_terminal: AtomicU64::new(1),
                sftp: Mutex::new(None),
                transferts: std::sync::Mutex::new(HashMap::new()),
                tunnels: tokio::sync::Mutex::new(HashMap::new()),
                ftp: Mutex::new(None),
            }));
            app.manage(Arc::new(approbation::Attentes::default()));
            #[cfg(desktop)]
            {
                approbation::demarrer(app.handle(), dossier.clone());
                barre::enregistrer_raccourcis(app.handle());
                if let Err(e) = barre::installer(app.handle()) {
                    log::warn!("icône de la barre désactivée : {e}");
                }
            }
            Ok(())
        })
        .on_window_event(|fenetre, evenement| {
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = evenement {
                use tauri::Emitter;
                let app = fenetre.app_handle();
                let active = tauri::async_runtime::block_on(app.state::<Arc<Etat>>().session_active());
                if active {
                    api.prevent_close();
                    let _ = app.emit(barre::EVENEMENT_QUITTER, ());
                }
            }
            #[cfg(mobile)]
            let _ = (fenetre, evenement);
        })
        .build(tauri::generate_context!())
        .expect("MLS Connect : erreur au lancement")
        .run(|app, evenement| {
            // `exit` termine le processus sans libérer le guichet : son annonce est retirée ici.
            if let tauri::RunEvent::Exit = evenement {
                if let Ok(dossier) = app.path().app_config_dir() {
                    let _ = std::fs::remove_file(dossier.join(mc_core::approbation::FICHIER));
                }
            }
        });
}
