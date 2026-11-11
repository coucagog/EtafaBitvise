//! Parcours de connexion réels contre un serveur SSH local.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_core::connexion::{connecter, CodeErreur, Decision, Decisions, ErreurConnexion, Evenement, Rappel, Session};
use mc_core::hotes::MagasinHotes;
use mc_core::profils::{MethodeAuth, Profil};
use mc_core::terminal::Sortie;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{self, Auth};
use russh::{Channel, ChannelId, Disconnect};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

struct Serveur {
    tentatives: Arc<AtomicUsize>,
    canaux: HashMap<ChannelId, Channel<server::Msg>>,
}

impl server::Handler for Serveur {
    type Error = russh::Error;

    async fn channel_open_session(
        &mut self,
        canal: Channel<server::Msg>,
        reponse: server::ChannelOpenHandle,
        _session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.canaux.insert(canal.id(), canal);
        reponse.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        canal: ChannelId,
        terme: &str,
        colonnes: u32,
        lignes: u32,
        _largeur: u32,
        _hauteur: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(canal)?;
        session.data(canal, format!("PTY {terme} {colonnes}x{lignes}\n").into_bytes())?;
        Ok(())
    }

    async fn shell_request(&mut self, canal: ChannelId, session: &mut server::Session) -> Result<(), Self::Error> {
        session.channel_success(canal)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        canal: ChannelId,
        colonnes: u32,
        lignes: u32,
        _largeur: u32,
        _hauteur: u32,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.data(canal, format!("TAILLE {colonnes}x{lignes}\n").into_bytes())?;
        Ok(())
    }

    async fn data(&mut self, canal: ChannelId, donnees: &[u8], session: &mut server::Session) -> Result<(), Self::Error> {
        match donnees {
            b"quitter" => {
                session.exit_status_request(canal, 7)?;
                session.close(canal)?;
            }
            b"couper" => session.disconnect(Disconnect::ByApplication, "arret", "fr")?,
            _ => session.data(canal, donnees.to_vec())?,
        }
        Ok(())
    }

    async fn auth_password(&mut self, _utilisateur: &str, mot_de_passe: &str) -> Result<Auth, Self::Error> {
        self.tentatives.fetch_add(1, Ordering::SeqCst);
        Ok(if mot_de_passe == "bon" { Auth::Accept } else { Auth::reject() })
    }
}

fn config_neuve() -> Arc<server::Config> {
    Arc::new(server::Config {
        auth_rejection_time: Duration::from_millis(5),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap()],
        ..Default::default()
    })
}

struct Banc {
    adresse: SocketAddr,
    tentatives: Arc<AtomicUsize>,
    config: Arc<Mutex<Arc<server::Config>>>,
}

impl Banc {
    /// Même adresse, nouvelle clé : comme un serveur réinstallé ou usurpé.
    fn changer_cle(&self) {
        *self.config.lock().unwrap() = config_neuve();
        self.tentatives.store(0, Ordering::SeqCst);
    }

    fn profil(&self) -> Profil {
        Profil {
            hote: self.adresse.ip().to_string(),
            port: self.adresse.port(),
            utilisateur: "alice".into(),
            methode: MethodeAuth::MotDePasse,
            ..Default::default()
        }
    }
}

async fn serveur() -> Banc {
    let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = ecoute.local_addr().unwrap();
    let tentatives = Arc::new(AtomicUsize::new(0));
    let config = Arc::new(Mutex::new(config_neuve()));
    let (compteur, partagee) = (tentatives.clone(), config.clone());
    tokio::spawn(async move {
        while let Ok((flux, _)) = ecoute.accept().await {
            let cfg = partagee.lock().unwrap().clone();
            let tentatives = compteur.clone();
            tokio::spawn(async move {
                if let Ok(session) = server::run_stream(cfg, flux, Serveur { tentatives, canaux: HashMap::new() }).await {
                    let _ = session.await;
                }
            });
        }
    });
    Banc { adresse, tentatives, config }
}

fn magasin(nom: &str) -> (Arc<MagasinHotes>, PathBuf) {
    let chemin = std::env::temp_dir().join(format!("mc-connexion-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_file(&chemin);
    (Arc::new(MagasinHotes::new(chemin.clone())), chemin)
}

/// Répond à la question de clé d'hôte, et note les questions posées.
fn repondeur(decisions: Decisions, reponse: Decision) -> (Rappel, Arc<Mutex<Vec<&'static str>>>) {
    let questions = Arc::new(Mutex::new(Vec::new()));
    let notes = questions.clone();
    let rappel: Rappel = Arc::new(move |e| {
        let nom = match e {
            Evenement::CleInconnue { .. } => "inconnue",
            Evenement::CleModifiee { .. } => "modifiee",
            Evenement::SessionPerdue { .. } => {
                notes.lock().unwrap().push("perdue");
                return;
            }
            _ => return,
        };
        notes.lock().unwrap().push(nom);
        decisions.repondre(reponse);
    });
    (rappel, questions)
}

async fn essai(
    p: &Profil,
    secret: &str,
    hotes: &Arc<MagasinHotes>,
    reponse: Decision,
) -> (Result<Session, ErreurConnexion>, Vec<&'static str>) {
    let decisions = Decisions::default();
    let (rappel, questions) = repondeur(decisions.clone(), reponse);
    let r = connecter(p, Some(secret.into()), hotes.clone(), decisions, rappel).await;
    let q = questions.lock().unwrap().clone();
    (r, q)
}

#[tokio::test]
async fn cle_inconnue_annulee_n_envoie_rien_et_n_enregistre_rien() {
    let b = serveur().await;
    let (hotes, chemin) = magasin("annulee");
    let (r, q) = essai(&b.profil(), "bon", &hotes, Decision::Annuler).await;
    assert_eq!(r.err().unwrap().code, CodeErreur::CleRefusee);
    assert_eq!(q, ["inconnue"]);
    assert_eq!(b.tentatives.load(Ordering::SeqCst), 0);
    assert!(!chemin.exists());
}

#[tokio::test]
async fn cle_enregistree_ne_redemande_pas() {
    let b = serveur().await;
    let (hotes, _) = magasin("enregistree");
    let (r, q) = essai(&b.profil(), "bon", &hotes, Decision::EnregistrerEtAccepter).await;
    assert!(r.map_err(|e| e.titre).unwrap().est_ouverte());
    assert_eq!(q, ["inconnue"]);
    let (r, q) = essai(&b.profil(), "bon", &hotes, Decision::Annuler).await;
    assert!(r.is_ok());
    assert!(q.is_empty());
}

#[tokio::test]
async fn accepter_une_fois_ne_memorise_pas() {
    let b = serveur().await;
    let (hotes, _) = magasin("une-fois");
    assert!(essai(&b.profil(), "bon", &hotes, Decision::AccepterUneFois).await.0.is_ok());
    let (r, q) = essai(&b.profil(), "bon", &hotes, Decision::Annuler).await;
    assert!(r.is_err());
    assert_eq!(q, ["inconnue"]);
}

#[tokio::test]
async fn cle_modifiee_bloque_avant_authentification() {
    let b = serveur().await;
    let (hotes, _) = magasin("modifiee");
    assert!(essai(&b.profil(), "bon", &hotes, Decision::EnregistrerEtAccepter).await.0.is_ok());
    b.changer_cle();
    // « Accepter une fois » ne doit jamais suffire pour une clé modifiée.
    let (r, q) = essai(&b.profil(), "bon", &hotes, Decision::AccepterUneFois).await;
    assert_eq!(r.err().unwrap().code, CodeErreur::CleModifiee);
    assert_eq!(q, ["modifiee"]);
    assert_eq!(b.tentatives.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cle_modifiee_remplacee_sur_demande_explicite() {
    let b = serveur().await;
    let (hotes, _) = magasin("remplacee");
    assert!(essai(&b.profil(), "bon", &hotes, Decision::EnregistrerEtAccepter).await.0.is_ok());
    b.changer_cle();
    assert!(essai(&b.profil(), "bon", &hotes, Decision::Remplacer).await.0.is_ok());
    let (r, q) = essai(&b.profil(), "bon", &hotes, Decision::Annuler).await;
    assert!(r.is_ok());
    assert!(q.is_empty());
}

#[tokio::test]
async fn mauvais_mot_de_passe_refuse() {
    let b = serveur().await;
    let (hotes, _) = magasin("mdp");
    let (r, _) = essai(&b.profil(), "faux", &hotes, Decision::AccepterUneFois).await;
    assert_eq!(r.err().unwrap().code, CodeErreur::Authentification);
    assert_eq!(b.tentatives.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cle_privee_absente_signalee() {
    let b = serveur().await;
    let (hotes, _) = magasin("cle-absente");
    let mut p = b.profil();
    p.methode = MethodeAuth::Cle;
    p.chemin_cle = "fichier-inexistant".into();
    let (r, _) = essai(&p, "", &hotes, Decision::AccepterUneFois).await;
    assert_eq!(r.err().unwrap().code, CodeErreur::CleLocale);
}

#[tokio::test]
async fn hote_injoignable_signale() {
    let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = ecoute.local_addr().unwrap().port();
    drop(ecoute);
    let (hotes, _) = magasin("injoignable");
    let p = Profil { hote: "127.0.0.1".into(), port, delai_connexion: 2, ..Default::default() };
    let (r, _) = essai(&p, "", &hotes, Decision::Annuler).await;
    assert_eq!(r.err().unwrap().code, CodeErreur::Injoignable);
}

type Recu = Arc<Mutex<Vec<u8>>>;

fn collecteur() -> (Recu, Sortie) {
    let recu = Recu::default();
    let copie = recu.clone();
    (recu, Box::new(move |octets| copie.lock().unwrap().extend(octets)))
}

async fn attendre_texte(recu: &Recu, motif: &str) {
    let attente = async {
        while !String::from_utf8_lossy(&recu.lock().unwrap()).contains(motif) {
            sleep(Duration::from_millis(5)).await;
        }
    };
    timeout(Duration::from_secs(5), attente).await.unwrap_or_else(|_| panic!("« {motif} » jamais reçu"));
}

async fn attendre_notes(notes: &Arc<Mutex<Vec<&'static str>>>, attendu: &[&str]) {
    let attente = async {
        while notes.lock().unwrap().as_slice() != attendu {
            sleep(Duration::from_millis(5)).await;
        }
    };
    timeout(Duration::from_secs(5), attente).await.unwrap_or_else(|_| panic!("notes : {:?}", notes.lock().unwrap()));
}

async fn session_suivie(nom: &str) -> (Session, Arc<Mutex<Vec<&'static str>>>, Banc) {
    let b = serveur().await;
    let (hotes, _) = magasin(nom);
    let decisions = Decisions::default();
    let (rappel, notes) = repondeur(decisions.clone(), Decision::AccepterUneFois);
    let session = connecter(&b.profil(), Some("bon".into()), hotes, decisions, rappel).await.map_err(|e| e.titre).unwrap();
    (session, notes, b)
}

#[tokio::test]
async fn terminal_echange_redimensionne_et_rend_le_code_de_sortie() {
    let (session, notes, _banc) = session_suivie("terminal").await;
    let (recu, sortie) = collecteur();
    let (emetteur, fin) = oneshot::channel();
    let terminal = session
        .ouvrir_terminal("xterm-256color", 100, 30, sortie, Box::new(move |code| drop(emetteur.send(code))))
        .await
        .unwrap();
    attendre_texte(&recu, "PTY xterm-256color 100x30").await;
    terminal.ecrire(b"bonjour".to_vec());
    attendre_texte(&recu, "bonjour").await;
    terminal.redimensionner(120, 40);
    attendre_texte(&recu, "TAILLE 120x40").await;
    terminal.ecrire(b"quitter".to_vec());
    assert_eq!(timeout(Duration::from_secs(5), fin).await.unwrap().unwrap(), Some(7));
    // La fin d'un terminal ne vaut pas perte de la session.
    assert!(session.est_ouverte());
    assert_eq!(*notes.lock().unwrap(), ["inconnue"]);
}

#[tokio::test]
async fn deux_terminaux_sont_independants() {
    let (session, _notes, _banc) = session_suivie("deux").await;
    let (recu_a, sortie_a) = collecteur();
    let (recu_b, sortie_b) = collecteur();
    let a = session.ouvrir_terminal("xterm", 80, 24, sortie_a, Box::new(|_| ())).await.unwrap();
    let b = session.ouvrir_terminal("xterm", 80, 24, sortie_b, Box::new(|_| ())).await.unwrap();
    a.ecrire(b"pour-a".to_vec());
    b.ecrire(b"pour-b".to_vec());
    attendre_texte(&recu_a, "pour-a").await;
    attendre_texte(&recu_b, "pour-b").await;
    assert!(!String::from_utf8_lossy(&recu_a.lock().unwrap()).contains("pour-b"));
}

#[tokio::test]
async fn perte_de_session_signalee_une_seule_fois() {
    let (session, notes, _banc) = session_suivie("perte").await;
    let (_recu, sortie) = collecteur();
    let (emetteur, fin) = oneshot::channel();
    let terminal = session.ouvrir_terminal("xterm", 80, 24, sortie, Box::new(move |code| drop(emetteur.send(code)))).await.unwrap();
    terminal.ecrire(b"couper".to_vec());
    assert_eq!(timeout(Duration::from_secs(5), fin).await.unwrap().unwrap(), None);
    attendre_notes(&notes, &["inconnue", "perdue"]).await;
    sleep(Duration::from_millis(100)).await;
    assert_eq!(*notes.lock().unwrap(), ["inconnue", "perdue"]);
}

#[tokio::test]
async fn deconnexion_volontaire_ne_signale_aucune_perte() {
    let (session, notes, _banc) = session_suivie("volontaire").await;
    let (_recu, sortie) = collecteur();
    let _terminal = session.ouvrir_terminal("xterm", 80, 24, sortie, Box::new(|_| ())).await.unwrap();
    session.fermer().await;
    sleep(Duration::from_millis(200)).await;
    assert_eq!(*notes.lock().unwrap(), ["inconnue"]);
}
