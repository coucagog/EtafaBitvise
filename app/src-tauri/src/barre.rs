//! Barre des tâches, icônes cachées et raccourcis globaux, sur le modèle de YawREC.

use std::sync::{Arc, Mutex};

use mc_core::raccourcis::catalogue;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::commandes::{afficher, Etat};

pub const EVENEMENT_RACCOURCI: &str = "mc://raccourci";
pub const EVENEMENT_QUITTER: &str = "mc://quitter";

struct EntreeConnexion(Mutex<Option<MenuItem<Wry>>>);

fn raccourcis() -> Vec<(String, String, Shortcut)> {
    catalogue()
        .globaux
        .into_iter()
        .filter_map(|g| match g.touches.parse::<Shortcut>() {
            Ok(s) => Some((g.id, g.touches, s)),
            Err(e) => {
                log::warn!("raccourci {} illisible : {e}", g.touches);
                None
            }
        })
        .collect()
}

/// Seuls `Ctrl+Shift+F7` et `Ctrl+Shift+F8` sont déclarés au système.
pub fn plugin_raccourcis() -> tauri::plugin::TauriPlugin<Wry> {
    let liste = raccourcis();
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |app, raccourci, evenement| {
            if evenement.state() != ShortcutState::Pressed {
                return;
            }
            if let Some((id, _, _)) = liste.iter().find(|(_, _, s)| s == raccourci) {
                let _ = app.emit(EVENEMENT_RACCOURCI, id);
            }
        })
        .build()
}

/// Un raccourci déjà pris par une autre application est journalisé, sans bloquer le démarrage.
pub fn enregistrer_raccourcis(app: &AppHandle) {
    for (_, touches, raccourci) in raccourcis() {
        match app.global_shortcut().register(raccourci) {
            Ok(()) => log::info!("{touches} actif"),
            Err(e) => log::warn!("raccourci {touches} impossible : {e}"),
        }
    }
}

fn texte_connexion(connecte: bool) -> String {
    let action = if connecte { "Se déconnecter" } else { "Se connecter" };
    format!("{action} (Ctrl+Shift+F7)")
}

pub fn libelle_connexion(app: &AppHandle, connecte: bool) {
    if let Some(entree) = app.state::<EntreeConnexion>().0.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        let _ = entree.set_text(texte_connexion(connecte));
    }
}

async fn demander_sortie(app: AppHandle) {
    if app.state::<Arc<Etat>>().session_active().await {
        afficher(&app);
        let _ = app.emit(EVENEMENT_QUITTER, ());
    } else {
        app.exit(0);
    }
}

pub fn installer(app: &AppHandle) -> tauri::Result<()> {
    let afficher_entree = MenuItem::with_id(app, "barre-afficher", "Afficher MLS Connect", true, None::<&str>)?;
    let connexion = MenuItem::with_id(app, "barre-connexion", texte_connexion(false), true, None::<&str>)?;
    let separateur = PredefinedMenuItem::separator(app)?;
    let quitter = MenuItem::with_id(app, "barre-quitter", "Quitter MLS Connect", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&afficher_entree, &connexion, &separateur, &quitter])?;
    app.manage(EntreeConnexion(Mutex::new(Some(connexion))));

    TrayIconBuilder::with_id("mc-barre")
        .icon(Image::from_bytes(include_bytes!("../icons/barre.png"))?)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("MLS Connect")
        .on_menu_event(|app, evenement| match evenement.id.as_ref() {
            "barre-afficher" => afficher(app),
            "barre-connexion" => {
                let _ = app.emit(EVENEMENT_RACCOURCI, "connexion");
            }
            "barre-quitter" => {
                tauri::async_runtime::spawn(demander_sortie(app.clone()));
            }
            _ => {}
        })
        .on_tray_icon_event(|icone, evenement| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = evenement {
                afficher(icone.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
