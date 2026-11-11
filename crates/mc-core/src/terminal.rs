//! Terminaux interactifs d'une session.
//!
//! Chaque canal est lu par une tâche qui ne fait rien d'autre, et écrit par une seconde tâche :
//! une file russh pleine bloque toute la session (voir `Doc-Ref/02-choix-techniques.md`).

use std::time::Duration;

use russh::{client, Channel, ChannelMsg, Pty};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::connexion::Session;

const DELAI_OUVERTURE: Duration = Duration::from_secs(15);
const MODES: [(Pty, u32); 2] = [(Pty::VERASE, 127), (Pty::IUTF8, 1)];

/// Ne doit pas attendre : elle est appelée depuis la tâche de lecture du canal.
pub type Sortie = Box<dyn Fn(Vec<u8>) + Send + Sync>;
/// Reçoit le code de sortie, s'il est connu, à la fermeture du canal.
pub type Fin = Box<dyn FnOnce(Option<u32>) + Send>;

enum Commande {
    Donnees(Vec<u8>),
    Taille { colonnes: u32, lignes: u32 },
}

/// Lâcher cette valeur ferme le canal.
pub struct Terminal {
    commandes: mpsc::UnboundedSender<Commande>,
}

impl Terminal {
    pub fn ecrire(&self, donnees: Vec<u8>) {
        let _ = self.commandes.send(Commande::Donnees(donnees));
    }

    pub fn redimensionner(&self, colonnes: u32, lignes: u32) {
        let _ = self.commandes.send(Commande::Taille { colonnes, lignes });
    }
}

async fn attendre_reponse(canal: &mut Channel<client::Msg>, sortie: &Sortie, demande: &str) -> Result<(), String> {
    let attente = async {
        while let Some(message) = canal.wait().await {
            match message {
                ChannelMsg::Success => return Ok(()),
                ChannelMsg::Failure => return Err(format!("Le serveur a refusé la demande de {demande}.")),
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => sortie(data.to_vec()),
                _ => {}
            }
        }
        Err(format!("Le canal s'est fermé avant la réponse à la demande de {demande}."))
    };
    timeout(DELAI_OUVERTURE, attente)
        .await
        .unwrap_or_else(|_| Err(format!("Le serveur n'a pas répondu à la demande de {demande}.")))
}

impl Session {
    pub async fn ouvrir_terminal(
        &self,
        terme: &str,
        colonnes: u32,
        lignes: u32,
        sortie: Sortie,
        fin: Fin,
    ) -> Result<Terminal, String> {
        let mut canal = match timeout(DELAI_OUVERTURE, self.poignee.channel_open_session()).await {
            Ok(Ok(canal)) => canal,
            Ok(Err(e)) => {
                if !self.est_ouverte() {
                    self.perte.signaler("La connexion au serveur a été perdue.".into());
                }
                return Err(format!("Ouverture du canal impossible : {e}."));
            }
            Err(_) => return Err("Le serveur n'a pas répondu à l'ouverture du canal.".into()),
        };

        canal
            .request_pty(true, terme, colonnes, lignes, 0, 0, &MODES)
            .await
            .map_err(|e| format!("Demande de pseudo-terminal impossible : {e}."))?;
        attendre_reponse(&mut canal, &sortie, "pseudo-terminal").await?;
        canal
            .request_shell(true)
            .await
            .map_err(|e| format!("Demande de shell impossible : {e}."))?;
        attendre_reponse(&mut canal, &sortie, "shell").await?;

        let (mut lecture, ecriture) = canal.split();
        let perte = self.perte.clone();
        tokio::spawn(async move {
            let mut code = None;
            let mut ferme = false;
            while let Some(message) = lecture.wait().await {
                match message {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => sortie(data.to_vec()),
                    ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
                    ChannelMsg::Close => {
                        ferme = true;
                        break;
                    }
                    _ => {}
                }
            }
            // Sans message de fermeture, c'est la session entière qui est tombée.
            if !ferme {
                perte.signaler("La connexion au serveur a été perdue.".into());
            }
            fin(code);
        });

        let (commandes, mut recepteur) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(commande) = recepteur.recv().await {
                let resultat = match commande {
                    Commande::Donnees(donnees) => ecriture.data_bytes(donnees).await,
                    Commande::Taille { colonnes, lignes } => ecriture.window_change(colonnes, lignes, 0, 0).await,
                };
                if resultat.is_err() {
                    break;
                }
            }
            let _ = ecriture.close().await;
        });
        Ok(Terminal { commandes })
    }
}
