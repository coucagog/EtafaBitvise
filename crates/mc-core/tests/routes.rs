//! Saisie interactive, proxy et rebond, contre un serveur SSH local.

use std::borrow::Cow;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_core::connexion::{connecter, connecter_par, CodeErreur, Decision, Decisions, ErreurConnexion, Evenement, Invite, Rappel, Session};
use mc_core::hotes::MagasinHotes;
use mc_core::profils::{GenreProxy, MethodeAuth, Profil, Proxy};
use mc_core::terminal::Sortie;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{self, Auth, Response};
use russh::{Channel, ChannelId};
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

#[derive(Default)]
struct Compteurs {
    connexions: AtomicUsize,
    cibles: Mutex<Vec<String>>,
}

struct Serveur {
    compteurs: Arc<Compteurs>,
    canaux: HashMap<ChannelId, Channel<server::Msg>>,
    etape: u8,
}

fn invites() -> Auth {
    Auth::Partial {
        name: Cow::Borrowed("Vérification"),
        instructions: Cow::Borrowed("Saisissez\u{1b}[31m votre code"),
        prompts: Cow::Owned(vec![(Cow::Borrowed("Code : "), false), (Cow::Borrowed("Équipe : "), true)]),
    }
}

impl server::Handler for Serveur {
    type Error = russh::Error;

    async fn auth_password(&mut self, _utilisateur: &str, _mot_de_passe: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _utilisateur: &str,
        _sous_methodes: &str,
        reponse: Option<Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        let Some(reponse) = reponse else {
            return Ok(invites());
        };
        let saisies: Vec<String> = reponse.map(|o| String::from_utf8_lossy(&o).into_owned()).collect();
        self.etape += 1;
        match (self.etape, saisies.as_slice()) {
            (1, [code, equipe]) if code == "123456" && equipe == "ops" => Ok(Auth::Partial {
                name: Cow::Borrowed(""),
                instructions: Cow::Borrowed(""),
                prompts: Cow::Owned(vec![]),
            }),
            (2, []) => Ok(Auth::Accept),
            _ => Ok(Auth::reject()),
        }
    }

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
        _terme: &str,
        _colonnes: u32,
        _lignes: u32,
        _largeur: u32,
        _hauteur: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(canal)?;
        Ok(())
    }

    async fn shell_request(&mut self, canal: ChannelId, session: &mut server::Session) -> Result<(), Self::Error> {
        session.channel_success(canal)?;
        session.data(canal, b"BONJOUR".to_vec())?;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        canal: Channel<server::Msg>,
        hote: &str,
        port: u32,
        _origine: &str,
        _port_origine: u32,
        reponse: server::ChannelOpenHandle,
        _session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        let adresse = format!("{hote}:{port}");
        self.compteurs.cibles.lock().unwrap().push(adresse.clone());
        reponse.accept().await;
        tokio::spawn(async move {
            if let Ok(mut tcp) = TcpStream::connect(adresse).await {
                let _ = copy_bidirectional(&mut canal.into_stream(), &mut tcp).await;
            }
        });
        Ok(())
    }
}

struct Banc {
    adresse: SocketAddr,
    compteurs: Arc<Compteurs>,
}

async fn serveur() -> Banc {
    let config = Arc::new(server::Config {
        auth_rejection_time: Duration::from_millis(5),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap()],
        ..Default::default()
    });
    let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = ecoute.local_addr().unwrap();
    let compteurs = Arc::new(Compteurs::default());
    let partages = compteurs.clone();
    tokio::spawn(async move {
        while let Ok((flux, _)) = ecoute.accept().await {
            partages.connexions.fetch_add(1, Ordering::SeqCst);
            let (config, compteurs) = (config.clone(), partages.clone());
            tokio::spawn(async move {
                if let Ok(session) = server::run_stream(config, flux, Serveur { compteurs, canaux: HashMap::new(), etape: 0 }).await {
                    let _ = session.await;
                }
            });
        }
    });
    Banc { adresse, compteurs }
}

fn profil(adresse: SocketAddr, methode: MethodeAuth) -> Profil {
    Profil { hote: adresse.ip().to_string(), port: adresse.port(), utilisateur: "alice".into(), methode, ..Default::default() }
}

fn magasin(nom: &str) -> Arc<MagasinHotes> {
    let chemin = std::env::temp_dir().join(format!("mc-routes-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_file(&chemin);
    Arc::new(MagasinHotes::new(chemin))
}

type Questions = Arc<Mutex<Vec<(String, Vec<Invite>)>>>;

/// Accepte les clés d'hôte et note les hôtes interrogés ; répond aux saisies avec `reponses`.
fn repondeur(decisions: Decisions, reponses: Option<Vec<String>>) -> (Rappel, Questions, Arc<Mutex<Vec<String>>>) {
    let questions = Questions::default();
    let hotes = Arc::new(Mutex::new(Vec::new()));
    let (notes, hotes_vus) = (questions.clone(), hotes.clone());
    let rappel: Rappel = Arc::new(move |e| match e {
        Evenement::CleInconnue { port, .. } => {
            hotes_vus.lock().unwrap().push(port.to_string());
            decisions.repondre(Decision::AccepterUneFois);
        }
        Evenement::QuestionClavier { instructions, invites, .. } => {
            notes.lock().unwrap().push((instructions, invites));
            decisions.repondre_clavier(reponses.clone());
        }
        _ => {}
    });
    (rappel, questions, hotes)
}

async fn clavier(reponses: Option<Vec<String>>, nom: &str) -> (Result<Session, ErreurConnexion>, Questions) {
    let b = serveur().await;
    let decisions = Decisions::default();
    let (rappel, questions, _) = repondeur(decisions.clone(), reponses);
    let r = connecter(&profil(b.adresse, MethodeAuth::Clavier), None, magasin(nom), decisions, rappel).await;
    (r, questions)
}

#[tokio::test]
async fn saisie_interactive_reussie_en_plusieurs_echanges() {
    let (r, questions) = clavier(Some(vec!["123456".into(), "ops".into()]), "ki-ok").await;
    assert!(r.is_ok(), "{:?}", r.err());
    let questions = questions.lock().unwrap();
    assert_eq!(questions.len(), 1, "un échange sans invite ne doit rien demander");
    let (instructions, invites) = &questions[0];
    assert!(!instructions.contains('\u{1b}'), "les séquences de contrôle du serveur sont retirées");
    assert_eq!(invites.len(), 2);
    assert!(invites[0].masque);
    assert!(!invites[1].masque);
}

#[tokio::test]
async fn saisie_interactive_fausse_refusee() {
    let (r, _) = clavier(Some(vec!["000000".into(), "ops".into()]), "ki-faux").await;
    assert_eq!(r.err().unwrap().code, CodeErreur::Authentification);
}

#[tokio::test]
async fn saisie_interactive_abandonnee() {
    let (r, _) = clavier(None, "ki-abandon").await;
    assert_eq!(r.err().unwrap().code, CodeErreur::Annulee);
}

#[tokio::test]
async fn saisie_interactive_nombre_de_reponses_incorrect() {
    let (r, _) = clavier(Some(vec!["123456".into()]), "ki-nombre").await;
    assert_eq!(r.err().unwrap().code, CodeErreur::Annulee);
}

/// Proxy minimal : relaie vers la cible demandée, et note ce qu'il a reçu.
async fn proxy(http: bool, refus: bool) -> (SocketAddr, Arc<Mutex<String>>) {
    let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = ecoute.local_addr().unwrap();
    let vu = Arc::new(Mutex::new(String::new()));
    let note = vu.clone();
    tokio::spawn(async move {
        let (mut flux, _) = ecoute.accept().await.unwrap();
        let cible = if http {
            let mut entete = Vec::new();
            while !entete.ends_with(b"\r\n\r\n") {
                entete.push(flux.read_u8().await.unwrap());
            }
            let texte = String::from_utf8_lossy(&entete).into_owned();
            if refus {
                flux.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await.unwrap();
                return;
            }
            flux.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
            texte.split_whitespace().nth(1).unwrap().to_string()
        } else {
            let mut salut = [0; 3];
            flux.read_exact(&mut salut).await.unwrap();
            flux.write_all(&[5, 0]).await.unwrap();
            let mut tete = [0; 5];
            flux.read_exact(&mut tete).await.unwrap();
            let mut nom = vec![0; usize::from(tete[4])];
            flux.read_exact(&mut nom).await.unwrap();
            let port = flux.read_u16().await.unwrap();
            if refus {
                flux.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap();
                return;
            }
            flux.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap();
            format!("{}:{port}", String::from_utf8(nom).unwrap())
        };
        *note.lock().unwrap() = cible.clone();
        let mut sortie = TcpStream::connect(cible).await.unwrap();
        let _ = copy_bidirectional(&mut flux, &mut sortie).await;
    });
    (adresse, vu)
}

async fn par_proxy(genre: GenreProxy, refus: bool, nom: &str) -> (Result<Session, ErreurConnexion>, SocketAddr, Arc<Mutex<String>>) {
    let b = serveur().await;
    let (adresse_proxy, vu) = proxy(genre == GenreProxy::Http, refus).await;
    let mut p = profil(b.adresse, MethodeAuth::MotDePasse);
    p.proxy = Proxy { genre, hote: "127.0.0.1".into(), port: adresse_proxy.port() };
    let decisions = Decisions::default();
    let (rappel, _, _) = repondeur(decisions.clone(), None);
    let r = connecter(&p, Some("x".into()), magasin(nom), decisions, rappel).await;
    (r, b.adresse, vu)
}

#[tokio::test]
async fn connexion_par_socks5() {
    let (r, adresse, vu) = par_proxy(GenreProxy::Socks5, false, "socks").await;
    assert!(r.unwrap().est_ouverte());
    assert_eq!(*vu.lock().unwrap(), adresse.to_string());
}

#[tokio::test]
async fn connexion_par_http() {
    let (r, adresse, vu) = par_proxy(GenreProxy::Http, false, "http").await;
    assert!(r.unwrap().est_ouverte());
    assert_eq!(*vu.lock().unwrap(), adresse.to_string());
}

#[tokio::test]
async fn proxy_qui_refuse_signale() {
    for (genre, nom) in [(GenreProxy::Socks5, "socks-refus"), (GenreProxy::Http, "http-refus")] {
        let (r, _, _) = par_proxy(genre, true, nom).await;
        let erreur = r.err().unwrap();
        assert_eq!(erreur.code, CodeErreur::Injoignable);
        assert!(erreur.detail.unwrap().contains("proxy"));
    }
}

fn collecteur() -> (Arc<Mutex<Vec<u8>>>, Sortie) {
    let recu: Arc<Mutex<Vec<u8>>> = Arc::default();
    let copie = recu.clone();
    (recu, Box::new(move |d| copie.lock().unwrap().extend_from_slice(&d)))
}

#[tokio::test]
async fn rebond_verifie_les_deux_cles_et_ouvre_la_session() {
    let (intermediaire, cible) = (serveur().await, serveur().await);
    let decisions = Decisions::default();
    let (rappel, _, hotes_vus) = repondeur(decisions.clone(), None);
    let rebond = (profil(intermediaire.adresse, MethodeAuth::MotDePasse), Some("r".into()));
    let session = connecter_par(&profil(cible.adresse, MethodeAuth::MotDePasse), Some("c".into()), Some(rebond), magasin("rebond"), decisions, rappel)
        .await
        .unwrap();

    assert_eq!(*hotes_vus.lock().unwrap(), vec![intermediaire.adresse.port().to_string(), cible.adresse.port().to_string()]);
    assert_eq!(*intermediaire.compteurs.cibles.lock().unwrap(), vec![cible.adresse.to_string()]);
    assert_eq!(cible.compteurs.connexions.load(Ordering::SeqCst), 1);

    // La session passe réellement par le rebond : un terminal doit s'y ouvrir.
    let (recu, sortie) = collecteur();
    let ouverture = session.ouvrir_terminal("xterm", 80, 24, sortie, Box::new(|_| {}));
    let _terminal = timeout(Duration::from_secs(5), ouverture).await.expect("réponse attendue à travers le rebond").unwrap();
    for _ in 0..100 {
        if recu.lock().unwrap().as_slice() == b"BONJOUR" {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(recu.lock().unwrap().as_slice(), b"BONJOUR");
    session.fermer().await;
    sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn rebond_injoignable_nomme_le_rebond() {
    let cible = serveur().await;
    let eteint = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
    let decisions = Decisions::default();
    let (rappel, _, _) = repondeur(decisions.clone(), None);
    let mut p = profil(eteint, MethodeAuth::MotDePasse);
    p.delai_connexion = 2;
    let r = connecter_par(&profil(cible.adresse, MethodeAuth::MotDePasse), Some("c".into()), Some((p, Some("r".into()))), magasin("rebond-hs"), decisions, rappel).await;
    let erreur = r.err().unwrap();
    assert!(erreur.titre.starts_with("Rebond"), "{}", erreur.titre);
    assert_eq!(cible.compteurs.connexions.load(Ordering::SeqCst), 0, "la cible ne doit recevoir aucune connexion directe");
}
