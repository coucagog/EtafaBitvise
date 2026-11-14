//! Niveau 2 de l'accès agent : chaque commande demandée par `mc-mcp` est montrée à l'utilisateur,
//! qui l'approuve ou la refuse. L'application n'exécute rien : elle répond seulement.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use mc_core::approbation::{Demande, Guichet, Verdict, DELAI_REPONSE};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;

pub const EVENEMENT_DEMANDE: &str = "mc://agent-demande";
pub const EVENEMENT_FIN: &str = "mc://agent-demande-fin";
/// Au-delà, les demandes sont refusées sans être affichées.
const ATTENTES_MAX: usize = 8;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeAffichee {
    id: u64,
    profil: String,
    serveur: String,
    commande: String,
    delai_s: u64,
}

#[derive(Default)]
pub struct Attentes {
    prochaine: AtomicU64,
    en_cours: Mutex<HashMap<u64, (DemandeAffichee, oneshot::Sender<bool>)>>,
}

impl Attentes {
    fn en_cours(&self) -> MutexGuard<'_, HashMap<u64, (DemandeAffichee, oneshot::Sender<bool>)>> {
        self.en_cours.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Retire la demande de l'écran quand elle se termine : réponse, délai ou abandon du demandeur.
struct Retrait {
    app: AppHandle,
    attentes: Arc<Attentes>,
    id: u64,
}

impl Drop for Retrait {
    fn drop(&mut self) {
        self.attentes.en_cours().remove(&self.id);
        let _ = self.app.emit(EVENEMENT_FIN, self.id);
    }
}

async fn decider(app: AppHandle, demande: Demande) -> Verdict {
    let attentes = app.state::<Arc<Attentes>>().inner().clone();
    let id = attentes.prochaine.fetch_add(1, Ordering::Relaxed) + 1;
    let affichee = DemandeAffichee {
        id,
        profil: demande.profil,
        serveur: demande.serveur,
        commande: demande.commande,
        delai_s: DELAI_REPONSE.as_secs(),
    };
    let (reponse, attente) = oneshot::channel();
    {
        let mut en_cours = attentes.en_cours();
        if en_cours.len() >= ATTENTES_MAX {
            return Verdict::Refusee;
        }
        en_cours.insert(id, (affichee.clone(), reponse));
    }
    let _retrait = Retrait { app: app.clone(), attentes, id };
    crate::commandes::afficher(&app);
    let _ = app.emit(EVENEMENT_DEMANDE, &affichee);
    match tokio::time::timeout(DELAI_REPONSE, attente).await {
        Ok(Ok(true)) => Verdict::Approuvee,
        Ok(_) => Verdict::Refusee,
        Err(_) => Verdict::Expiree,
    }
}

/// Ouvre l'écoute locale et l'annonce dans le dossier de données. Sans elle, le niveau 2 est refusé.
pub fn demarrer(app: &AppHandle, dossier: PathBuf) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match Guichet::ouvrir(&dossier).await {
            Ok(guichet) => guichet.servir(move |demande| decider(app.clone(), demande)).await,
            Err(e) => log::warn!("approbation des commandes d'agent désactivée : {e}"),
        }
    });
}

/// Demandes en attente, pour une interface qui vient de se (re)charger.
#[tauri::command]
pub fn agent_demandes(attentes: State<'_, Arc<Attentes>>) -> Vec<DemandeAffichee> {
    let mut demandes: Vec<DemandeAffichee> = attentes.en_cours().values().map(|(d, _)| d.clone()).collect();
    demandes.sort_by_key(|d| d.id);
    demandes
}

#[tauri::command]
pub fn agent_repondre(attentes: State<'_, Arc<Attentes>>, id: u64, accepte: bool) {
    if let Some((_, reponse)) = attentes.en_cours().remove(&id) {
        let _ = reponse.send(accepte);
    }
}
