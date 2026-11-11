//! Authentification par agent SSH, contre un serveur local.

mod commun;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use mc_core::connexion::{connecter, CodeErreur, Decision, Decisions, ErreurConnexion, Evenement, Rappel, Session};
use mc_core::hotes::MagasinHotes;
use mc_core::profils::{MethodeAuth, Profil};
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{self, Auth};
use tokio::net::TcpListener;

struct Serveur {
    acceptee: PublicKey,
}

impl server::Handler for Serveur {
    type Error = russh::Error;

    async fn auth_publickey(&mut self, _utilisateur: &str, cle: server::VerifiedPublicKey<'_>) -> Result<Auth, Self::Error> {
        Ok(if cle.public_key.key_data() == self.acceptee.key_data() { Auth::Accept } else { Auth::reject() })
    }
}

fn cle() -> PrivateKey {
    PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap()
}

async fn serveur(acceptee: PublicKey) -> SocketAddr {
    let config = Arc::new(server::Config {
        auth_rejection_time: Duration::from_millis(5),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![cle()],
        ..Default::default()
    });
    let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = ecoute.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((flux, _)) = ecoute.accept().await {
            let (config, acceptee) = (config.clone(), acceptee.clone());
            tokio::spawn(async move {
                if let Ok(session) = server::run_stream(config, flux, Serveur { acceptee }).await {
                    let _ = session.await;
                }
            });
        }
    });
    adresse
}

async fn essayer(adresse: SocketAddr, nom: &str) -> Result<Session, ErreurConnexion> {
    let chemin = std::env::temp_dir().join(format!("mc-agent-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_file(&chemin);
    let decisions = Decisions::default();
    let reponses = decisions.clone();
    let rappel: Rappel = Arc::new(move |e| {
        if matches!(e, Evenement::CleInconnue { .. }) {
            reponses.repondre(Decision::AccepterUneFois);
        }
    });
    let profil = Profil {
        hote: adresse.ip().to_string(),
        port: adresse.port(),
        utilisateur: "alice".into(),
        methode: MethodeAuth::Agent,
        ..Default::default()
    };
    connecter(&profil, None, Arc::new(MagasinHotes::new(chemin)), decisions, rappel).await
}

/// Un seul test : `SSH_AUTH_SOCK` est propre au processus.
#[tokio::test]
async fn agent_vide_refuse_puis_seconde_identite() {
    let (chemin, mut agent) = commun::agent_local("mc-essai-agent").await;
    std::env::set_var("SSH_AUTH_SOCK", &chemin);
    let (autre, bonne) = (cle(), cle());
    let adresse = serveur(bonne.public_key().clone()).await;

    let erreur = essayer(adresse, "vide").await.err().expect("agent vide");
    assert_eq!(erreur.code, CodeErreur::CleLocale);
    assert!(erreur.texte.contains("aucune clé"), "{}", erreur.texte);

    agent.add_identity(&autre, &[]).await.unwrap();
    let erreur = essayer(adresse, "refus").await.err().expect("clé non autorisée");
    assert_eq!(erreur.code, CodeErreur::Authentification);

    // La bonne clé n'est pas la première proposée.
    agent.add_identity(&bonne, &[]).await.unwrap();
    let session = essayer(adresse, "ok").await.map_err(|e| e.titre).unwrap();
    assert!(session.est_ouverte());
    session.fermer().await;

    std::env::set_var("SSH_AUTH_SOCK", if cfg!(windows) { r"\\.\pipe\mc-essai-inexistant" } else { "/nonexistent/agent" });
    assert!(essayer(adresse, "absent").await.is_err());
}
