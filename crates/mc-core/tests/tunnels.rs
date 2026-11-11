//! Redirections de bout en bout contre un serveur SSH local.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_core::connexion::{connecter, Decision, Decisions, Evenement, Rappel, Session};
use mc_core::hotes::MagasinHotes;
use mc_core::profils::{MethodeAuth, Profil};
use mc_core::tunnels::{CodeTunnel, Genre, Regle};
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{self, Auth, Handle};
use russh::Channel;
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

type Poignee = Arc<Mutex<Option<Handle>>>;

struct Serveur {
    poignee: Poignee,
}

impl server::Handler for Serveur {
    type Error = russh::Error;

    async fn auth_succeeded(&mut self, session: &mut server::Session) -> Result<(), Self::Error> {
        *self.poignee.lock().unwrap() = Some(session.handle());
        Ok(())
    }

    async fn auth_password(&mut self, _utilisateur: &str, _mot_de_passe: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
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
        if hote == "interdit" {
            reponse.reject(russh::ChannelOpenFailure::AdministrativelyProhibited).await;
            return Ok(());
        }
        reponse.accept().await;
        let adresse = format!("{hote}:{port}");
        tokio::spawn(async move {
            if let Ok(mut tcp) = TcpStream::connect(adresse).await {
                let _ = copy_bidirectional(&mut canal.into_stream(), &mut tcp).await;
            }
        });
        Ok(())
    }

    async fn tcpip_forward(&mut self, adresse: &str, port: &mut u32, session: &mut server::Session) -> Result<bool, Self::Error> {
        if adresse == "refuse" {
            return Ok(false);
        }
        let (adresse, port) = (adresse.to_string(), *port);
        let Ok(ecoute) = TcpListener::bind((adresse.as_str(), port as u16)).await else {
            return Ok(false);
        };
        let poignee = session.handle();
        tokio::spawn(async move {
            while let Ok((mut tcp, _)) = ecoute.accept().await {
                let (poignee, adresse) = (poignee.clone(), adresse.clone());
                tokio::spawn(async move {
                    if let Ok(canal) = poignee.channel_open_forwarded_tcpip(adresse, port, "127.0.0.1", 1).await {
                        let _ = copy_bidirectional(&mut canal.into_stream(), &mut tcp).await;
                    }
                });
            }
        });
        Ok(true)
    }
}

struct Banc {
    session: Arc<Session>,
    poignee: Poignee,
}

async fn banc(nom: &str) -> Banc {
    let config = Arc::new(server::Config {
        auth_rejection_time: Duration::from_millis(5),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap()],
        ..Default::default()
    });
    let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = ecoute.local_addr().unwrap();
    let poignee: Poignee = Arc::default();
    let partagee = poignee.clone();
    tokio::spawn(async move {
        let (flux, _) = ecoute.accept().await.unwrap();
        if let Ok(session) = server::run_stream(config, flux, Serveur { poignee: partagee }).await {
            let _ = session.await;
        }
    });

    let chemin = std::env::temp_dir().join(format!("mc-tunnels-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_file(&chemin);
    let profil = Profil {
        hote: adresse.ip().to_string(),
        port: adresse.port(),
        utilisateur: "alice".into(),
        methode: MethodeAuth::MotDePasse,
        ..Default::default()
    };
    let decisions = Decisions::default();
    let repondre = decisions.clone();
    let rappel: Rappel = Arc::new(move |e| {
        if matches!(e, Evenement::CleInconnue { .. }) {
            repondre.repondre(Decision::AccepterUneFois);
        }
    });
    let session = connecter(&profil, Some("bon".into()), Arc::new(MagasinHotes::new(chemin)), decisions, rappel).await.unwrap();
    Banc { session: Arc::new(session), poignee }
}

async fn echo() -> SocketAddr {
    let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = ecoute.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut flux, _)) = ecoute.accept().await {
            tokio::spawn(async move {
                let (mut l, mut e) = flux.split();
                let _ = tokio::io::copy(&mut l, &mut e).await;
            });
        }
    });
    adresse
}

async fn port_libre() -> u16 {
    TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port()
}

async fn aller_retour(flux: &mut TcpStream, message: &[u8]) {
    flux.write_all(message).await.unwrap();
    let mut recu = vec![0; message.len()];
    timeout(Duration::from_secs(10), flux.read_exact(&mut recu)).await.expect("délai dépassé").unwrap();
    assert_eq!(recu, message);
}

fn regle(genre: Genre, port: u16, cible: SocketAddr) -> Regle {
    Regle {
        id: "t".into(),
        genre,
        ecoute_hote: "127.0.0.1".into(),
        ecoute_port: port,
        cible_hote: cible.ip().to_string(),
        cible_port: cible.port(),
        ..Default::default()
    }
}

#[tokio::test]
async fn local_relaie_compte_et_se_ferme() {
    let b = banc("local").await;
    let cible = echo().await;
    let port = port_libre().await;
    let tunnel = b.session.ouvrir_tunnel(&regle(Genre::Local, port, cible)).await.unwrap();

    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    aller_retour(&mut flux, b"bonjour tunnel").await;
    let mesure = tunnel.mesure();
    assert_eq!(mesure.actives, 1);
    assert_eq!(mesure.monte, 14);
    assert_eq!(mesure.descendu, 14);

    drop(flux);
    timeout(Duration::from_secs(5), async {
        while tunnel.mesure().actives != 0 {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("la connexion terminée doit sortir du compte");

    drop(tunnel);
    sleep(Duration::from_millis(100)).await;
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err(), "le port doit être libéré");
}

#[tokio::test]
async fn fermer_le_tunnel_coupe_les_connexions_en_cours() {
    let b = banc("coupe").await;
    let port = port_libre().await;
    let tunnel = b.session.ouvrir_tunnel(&regle(Genre::Local, port, echo().await)).await.unwrap();
    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    aller_retour(&mut flux, b"avant").await;
    tunnel.fermer().await;
    let mut tampon = [0; 8];
    let lu = timeout(Duration::from_secs(5), flux.read(&mut tampon)).await.expect("la connexion doit se fermer");
    assert!(matches!(lu, Ok(0) | Err(_)));
}

#[tokio::test]
async fn port_local_occupe_signale() {
    let b = banc("occupe").await;
    let occupe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupe.local_addr().unwrap().port();
    let erreur = b.session.ouvrir_tunnel(&regle(Genre::Local, port, echo().await)).await.err().unwrap();
    assert_eq!(erreur.code, CodeTunnel::PortOccupe);
}

#[tokio::test]
async fn destination_refusee_notee_sans_tomber() {
    let b = banc("refuse-cible").await;
    let port = port_libre().await;
    let mut r = regle(Genre::Local, port, echo().await);
    r.cible_hote = "interdit".into();
    let tunnel = b.session.ouvrir_tunnel(&r).await.unwrap();
    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut tampon = [0; 8];
    let lu = timeout(Duration::from_secs(5), flux.read(&mut tampon)).await.expect("la connexion doit se fermer");
    assert!(matches!(lu, Ok(0) | Err(_)));
    assert!(tunnel.mesure().derniere_erreur.unwrap().contains("interdit"));
    assert!(b.session.est_ouverte());
}

#[tokio::test]
async fn distant_relaie_vers_ce_poste() {
    let b = banc("distant").await;
    let port = port_libre().await;
    let tunnel = b.session.ouvrir_tunnel(&regle(Genre::Distant, port, echo().await)).await.unwrap();
    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    aller_retour(&mut flux, b"depuis le serveur").await;
    assert_eq!(tunnel.mesure().actives, 1);
}

#[tokio::test]
async fn distant_refuse_par_le_serveur() {
    let b = banc("distant-refus").await;
    let mut r = regle(Genre::Distant, 4040, echo().await);
    r.ecoute_hote = "refuse".into();
    let erreur = b.session.ouvrir_tunnel(&r).await.err().unwrap();
    assert_eq!(erreur.code, CodeTunnel::Refuse);

    // Après un refus, le port doit rester disponible pour une nouvelle règle.
    let port = port_libre().await;
    r.ecoute_hote = "127.0.0.1".into();
    r.ecoute_port = port;
    b.session.ouvrir_tunnel(&r).await.unwrap();
}

#[tokio::test]
async fn canal_entrant_non_demande_refuse() {
    let b = banc("non-demande").await;
    let poignee = b.poignee.lock().unwrap().clone().unwrap();
    let ouverture = poignee.channel_open_forwarded_tcpip("127.0.0.1", 9999, "10.0.0.1", 1);
    let reponse = timeout(Duration::from_secs(5), ouverture).await.expect("le client doit répondre");
    assert!(reponse.is_err(), "un canal que personne n'a demandé ne doit pas aboutir");
    assert!(b.session.est_ouverte());
}

#[tokio::test]
async fn socks_relaie() {
    let b = banc("socks").await;
    let cible = echo().await;
    let port = port_libre().await;
    let _tunnel = b.session.ouvrir_tunnel(&regle(Genre::Socks, port, cible)).await.unwrap();
    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    flux.write_all(&[5, 1, 0]).await.unwrap();
    let mut methode = [0; 2];
    flux.read_exact(&mut methode).await.unwrap();
    assert_eq!(methode, [5, 0]);
    let mut demande = vec![5, 1, 0, 1, 127, 0, 0, 1];
    demande.extend(cible.port().to_be_bytes());
    flux.write_all(&demande).await.unwrap();
    let mut reponse = [0; 10];
    flux.read_exact(&mut reponse).await.unwrap();
    assert_eq!(reponse[1], 0);
    aller_retour(&mut flux, b"par socks").await;
}

#[tokio::test]
async fn http_connect_relaie_et_signale_le_refus() {
    let b = banc("http").await;
    let cible = echo().await;
    let port = port_libre().await;
    let _tunnel = b.session.ouvrir_tunnel(&regle(Genre::Http, port, cible)).await.unwrap();

    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    flux.write_all(format!("CONNECT {cible} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).await.unwrap();
    let attendu = b"HTTP/1.1 200 Connection established\r\n\r\n";
    let mut reponse = vec![0; attendu.len()];
    flux.read_exact(&mut reponse).await.unwrap();
    assert_eq!(reponse, attendu);
    aller_retour(&mut flux, b"par http").await;

    let mut refuse = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    refuse.write_all(b"CONNECT interdit:80 HTTP/1.1\r\n\r\n").await.unwrap();
    let mut texte = String::new();
    timeout(Duration::from_secs(5), refuse.read_to_string(&mut texte)).await.unwrap().unwrap();
    assert!(texte.starts_with("HTTP/1.1 502"), "{texte}");
}
