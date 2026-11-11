//! Redirections de ports d'une session : local, distant et proxy dynamique.
//!
//! russh rouvre la fenêtre SSH dès la réception, sans tenir compte du client local. Le canal est donc
//! lu en continu (voir `Doc-Ref/02-choix-techniques.md`), et une connexion qui ne consomme pas assez
//! vite est fermée au-delà de `PLAFOND_ATTENTE` : ni la session ni la mémoire ne dépendent d'elle.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::{client, Channel, ChannelMsg};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep, timeout};

use crate::connexion::Session;

const DELAI_OUVERTURE: Duration = Duration::from_secs(15);
const DELAI_NEGOCIATION: Duration = Duration::from_secs(15);
const PLAFOND_ATTENTE: usize = 64 * 1024 * 1024;
const TAILLE_MAX_ENTETE: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Genre {
    #[default]
    Local,
    Distant,
    Socks,
    Http,
}

/// Enregistrée dans le profil : ne contient aucun secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Regle {
    pub id: String,
    pub nom: String,
    pub genre: Genre,
    pub ecoute_hote: String,
    pub ecoute_port: u16,
    pub cible_hote: String,
    pub cible_port: u16,
    pub auto: bool,
}

impl Regle {
    /// Une interface non renseignée reste limitée à ce poste, jamais ouverte au réseau.
    fn hote_ecoute(&self) -> &str {
        match self.ecoute_hote.trim() {
            "" => "127.0.0.1",
            hote => hote,
        }
    }

    fn valider(&self) -> Result<(), ErreurTunnel> {
        if self.ecoute_port == 0 {
            return Err(ErreurTunnel::new(CodeTunnel::Invalide, "Le port d'écoute doit être compris entre 1 et 65535."));
        }
        let a_cible = matches!(self.genre, Genre::Local | Genre::Distant);
        if a_cible && (self.cible_hote.trim().is_empty() || self.cible_port == 0) {
            return Err(ErreurTunnel::new(CodeTunnel::Invalide, "La destination doit indiquer un hôte et un port."));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeTunnel {
    Invalide,
    PortOccupe,
    Refuse,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErreurTunnel {
    pub code: CodeTunnel,
    pub message: String,
}

impl ErreurTunnel {
    fn new(code: CodeTunnel, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

#[derive(Default)]
struct Compteurs {
    actives: AtomicU32,
    descendu: AtomicU64,
    monte: AtomicU64,
    derniere_erreur: Mutex<Option<String>>,
}

impl Compteurs {
    fn noter(&self, erreur: String) {
        *self.derniere_erreur.lock().unwrap_or_else(|p| p.into_inner()) = Some(erreur);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mesure {
    pub actives: u32,
    pub descendu: u64,
    pub monte: u64,
    pub derniere_erreur: Option<String>,
}

struct Renvoi {
    cible_hote: String,
    cible_port: u16,
    compteurs: Arc<Compteurs>,
    arret: watch::Receiver<bool>,
}

/// Redirections distantes demandées par cette session, par port d'écoute. Un canal entrant
/// sur un autre port n'a été demandé par personne : il est refusé.
#[derive(Clone, Default)]
pub(crate) struct Renvois(Arc<Mutex<HashMap<u32, Arc<Renvoi>>>>);

impl Renvois {
    fn table(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Arc<Renvoi>>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// `None` si cette session n'a rien demandé sur ce port ; sinon, de quoi servir le canal une fois accepté.
    pub(crate) fn servir(&self, canal: Channel<client::Msg>, port: u32) -> Option<impl FnOnce() + Send> {
        let renvoi = self.table().get(&port)?.clone();
        Some(move || {
            tokio::spawn(async move {
                let cible = (renvoi.cible_hote.as_str(), renvoi.cible_port);
                match timeout(DELAI_OUVERTURE, TcpStream::connect(cible)).await {
                    Ok(Ok(flux)) => {
                        let _ = flux.set_nodelay(true);
                        relayer(canal, flux, renvoi.compteurs.clone(), renvoi.arret.clone()).await;
                    }
                    Ok(Err(e)) => {
                        renvoi.compteurs.noter(format!("Destination {}:{} injoignable : {e}.", renvoi.cible_hote, renvoi.cible_port));
                        let _ = canal.close().await;
                    }
                    Err(_) => {
                        renvoi.compteurs.noter(format!("Destination {}:{} : aucune réponse.", renvoi.cible_hote, renvoi.cible_port));
                        let _ = canal.close().await;
                    }
                }
            });
        })
    }
}

/// Lâcher cette valeur ferme l'écoute et les connexions en cours.
pub struct Tunnel {
    pub port: u16,
    compteurs: Arc<Compteurs>,
    arret: watch::Sender<bool>,
    distant: Option<(Arc<Session>, String)>,
}

impl Tunnel {
    pub fn mesure(&self) -> Mesure {
        Mesure {
            actives: self.compteurs.actives.load(Ordering::Relaxed),
            descendu: self.compteurs.descendu.load(Ordering::Relaxed),
            monte: self.compteurs.monte.load(Ordering::Relaxed),
            derniere_erreur: self.compteurs.derniere_erreur.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        }
    }

    /// Demande en plus au serveur de libérer son port, pour une redirection distante.
    pub async fn fermer(self) {
        if let Some((session, hote)) = &self.distant {
            let _ = timeout(Duration::from_secs(5), session.poignee.cancel_tcpip_forward(hote.clone(), u32::from(self.port))).await;
        }
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.arret.send_replace(true);
        if let Some((session, _)) = &self.distant {
            session.renvois.table().remove(&u32::from(self.port));
        }
    }
}

fn erreur_ecoute(hote: &str, port: u16, e: std::io::Error) -> ErreurTunnel {
    use std::io::ErrorKind::{AddrInUse, PermissionDenied};
    match e.kind() {
        AddrInUse => ErreurTunnel::new(CodeTunnel::PortOccupe, format!("Le port {port} est déjà utilisé sur {hote}.")),
        PermissionDenied => ErreurTunnel::new(CodeTunnel::PortOccupe, format!("Le système refuse l'écoute sur {hote}:{port}.")),
        _ => ErreurTunnel::new(CodeTunnel::Invalide, format!("Écoute impossible sur {hote}:{port} : {e}.")),
    }
}

impl Session {
    pub async fn ouvrir_tunnel(self: &Arc<Self>, regle: &Regle) -> Result<Tunnel, ErreurTunnel> {
        regle.valider()?;
        if !self.est_ouverte() {
            return Err(ErreurTunnel::new(CodeTunnel::Session, "La session n'est plus ouverte."));
        }
        let compteurs = Arc::new(Compteurs::default());
        let (arret, attente) = watch::channel(false);
        if regle.genre == Genre::Distant {
            return self.ouvrir_distant(regle, compteurs, arret, attente).await;
        }
        let hote = regle.hote_ecoute();
        let ecoute = TcpListener::bind((hote, regle.ecoute_port)).await.map_err(|e| erreur_ecoute(hote, regle.ecoute_port, e))?;
        let port = ecoute.local_addr().map_or(regle.ecoute_port, |a| a.port());
        tokio::spawn(ecouter(self.clone(), ecoute, regle.clone(), compteurs.clone(), attente));
        Ok(Tunnel { port, compteurs, arret, distant: None })
    }

    async fn ouvrir_distant(
        self: &Arc<Self>,
        regle: &Regle,
        compteurs: Arc<Compteurs>,
        arret: watch::Sender<bool>,
        attente: watch::Receiver<bool>,
    ) -> Result<Tunnel, ErreurTunnel> {
        let port = u32::from(regle.ecoute_port);
        let hote = regle.hote_ecoute().to_string();
        {
            let mut table = self.renvois.table();
            if table.contains_key(&port) {
                return Err(ErreurTunnel::new(CodeTunnel::PortOccupe, format!("Le port distant {port} est déjà redirigé par cette session.")));
            }
            // Enregistré avant la demande : le serveur peut ouvrir un canal aussitôt après sa réponse.
            table.insert(
                port,
                Arc::new(Renvoi { cible_hote: regle.cible_hote.clone(), cible_port: regle.cible_port, compteurs: compteurs.clone(), arret: attente }),
            );
        }
        let refus = match timeout(DELAI_OUVERTURE, self.poignee.tcpip_forward(hote.clone(), port)).await {
            Ok(Ok(_)) => None,
            Ok(Err(e)) => Some(ErreurTunnel::new(CodeTunnel::Refuse, format!("Le serveur a refusé d'écouter sur {hote}:{port} ({e})."))),
            Err(_) => Some(ErreurTunnel::new(CodeTunnel::Refuse, "Le serveur n'a pas répondu à la demande de redirection.")),
        };
        if let Some(erreur) = refus {
            self.renvois.table().remove(&port);
            return Err(erreur);
        }
        Ok(Tunnel { port: regle.ecoute_port, compteurs, arret, distant: Some((self.clone(), hote)) })
    }
}

/// Ne rend rien : la garde renvoyée par `wait_for` ne peut pas traverser un `await`.
async fn arrete(arret: &mut watch::Receiver<bool>) {
    let _ = arret.wait_for(|a| *a).await;
}

async fn ecouter(session: Arc<Session>, ecoute: TcpListener, regle: Regle, compteurs: Arc<Compteurs>, arret: watch::Receiver<bool>) {
    let mut fin = arret.clone();
    loop {
        tokio::select! {
            _ = arrete(&mut fin) => break,
            accepte = ecoute.accept() => match accepte {
                Ok((flux, origine)) => {
                    let _ = flux.set_nodelay(true);
                    tokio::spawn(servir(session.clone(), flux, origine, regle.clone(), compteurs.clone(), arret.clone()));
                }
                // Par exemple, plus de descripteurs libres : on laisse le système respirer.
                Err(_) => sleep(Duration::from_millis(200)).await,
            },
        }
    }
}

async fn servir(session: Arc<Session>, mut flux: TcpStream, origine: SocketAddr, regle: Regle, compteurs: Arc<Compteurs>, arret: watch::Receiver<bool>) {
    let cible = match regle.genre {
        Genre::Local => Ok((regle.cible_hote.clone(), regle.cible_port)),
        Genre::Socks => borne(negocier_socks(&mut flux)).await,
        Genre::Http => borne(negocier_http(&mut flux)).await,
        Genre::Distant => return,
    };
    let (hote, port) = match cible {
        Ok(c) => c,
        Err(e) => {
            compteurs.noter(e);
            return;
        }
    };
    let ouverture = session.poignee.channel_open_direct_tcpip(hote.as_str(), u32::from(port), origine.ip().to_string(), u32::from(origine.port()));
    let raison = match timeout(DELAI_OUVERTURE, ouverture).await {
        Ok(Ok(canal)) => {
            if repondre(&mut flux, regle.genre, true).await.is_ok() {
                relayer(canal, flux, compteurs, arret).await;
            } else {
                let _ = canal.close().await;
            }
            return;
        }
        Ok(Err(e)) => e.to_string(),
        Err(_) => "aucune réponse du serveur".to_string(),
    };
    let _ = repondre(&mut flux, regle.genre, false).await;
    compteurs.noter(format!("Destination {hote}:{port} refusée : {raison}."));
    if !session.est_ouverte() {
        session.perte.signaler("La connexion au serveur a été perdue.".into());
    }
}

async fn borne<F: std::future::Future<Output = Result<(String, u16), String>>>(negociation: F) -> Result<(String, u16), String> {
    timeout(DELAI_NEGOCIATION, negociation).await.unwrap_or_else(|_| Err("Client proxy trop lent à négocier.".into()))
}

async fn negocier_socks<S: AsyncRead + AsyncWrite + Unpin>(flux: &mut S) -> Result<(String, u16), String> {
    let lecture = |e: std::io::Error| format!("Négociation SOCKS interrompue : {e}.");
    let [version, nombre] = lire::<2, _>(flux).await.map_err(lecture)?;
    if version != 5 {
        return Err("Seul SOCKS5 est pris en charge.".into());
    }
    let mut methodes = vec![0; usize::from(nombre)];
    flux.read_exact(&mut methodes).await.map_err(lecture)?;
    if !methodes.contains(&0) {
        let _ = flux.write_all(&[5, 0xff]).await;
        return Err("Le client SOCKS exige une authentification non prise en charge.".into());
    }
    flux.write_all(&[5, 0]).await.map_err(lecture)?;

    let [_, commande, _, adresse] = lire::<4, _>(flux).await.map_err(lecture)?;
    if commande != 1 {
        let _ = flux.write_all(&reponse_socks(7)).await;
        return Err("Seule la commande CONNECT est prise en charge.".into());
    }
    let hote = match adresse {
        1 => std::net::Ipv4Addr::from(lire::<4, _>(flux).await.map_err(lecture)?).to_string(),
        4 => std::net::Ipv6Addr::from(lire::<16, _>(flux).await.map_err(lecture)?).to_string(),
        3 => {
            let [taille] = lire::<1, _>(flux).await.map_err(lecture)?;
            let mut nom = vec![0; usize::from(taille)];
            flux.read_exact(&mut nom).await.map_err(lecture)?;
            String::from_utf8(nom).map_err(|_| "Nom d'hôte SOCKS invalide.".to_string())?
        }
        _ => {
            let _ = flux.write_all(&reponse_socks(8)).await;
            return Err("Type d'adresse SOCKS inconnu.".into());
        }
    };
    let port = u16::from_be_bytes(lire::<2, _>(flux).await.map_err(lecture)?);
    Ok((hote, port))
}

async fn lire<const N: usize, S: AsyncRead + Unpin>(flux: &mut S) -> std::io::Result<[u8; N]> {
    let mut tampon = [0; N];
    flux.read_exact(&mut tampon).await?;
    Ok(tampon)
}

fn reponse_socks(code: u8) -> [u8; 10] {
    [5, code, 0, 1, 0, 0, 0, 0, 0, 0]
}

async fn negocier_http<S: AsyncRead + AsyncWrite + Unpin>(flux: &mut S) -> Result<(String, u16), String> {
    // Octet par octet : ce qui suit l'en-tête appartient au tunnel.
    let mut entete = Vec::with_capacity(256);
    while !entete.ends_with(b"\r\n\r\n") {
        if entete.len() >= TAILLE_MAX_ENTETE {
            let _ = flux.write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\n\r\n").await;
            return Err("En-tête HTTP trop long.".into());
        }
        let mut octet = [0];
        flux.read_exact(&mut octet).await.map_err(|e| format!("Négociation HTTP interrompue : {e}."))?;
        entete.push(octet[0]);
    }
    let texte = String::from_utf8_lossy(&entete);
    let mut mots = texte.lines().next().unwrap_or_default().split_ascii_whitespace();
    let (methode, cible) = (mots.next().unwrap_or_default(), mots.next().unwrap_or_default());
    if !methode.eq_ignore_ascii_case("CONNECT") {
        let _ = flux.write_all(b"HTTP/1.1 405 Method Not Allowed\r\nAllow: CONNECT\r\nContent-Length: 0\r\n\r\n").await;
        return Err("Seule la méthode CONNECT est prise en charge.".into());
    }
    let decoupe = cible.rsplit_once(':').and_then(|(hote, port)| Some((hote.trim_matches(['[', ']']).to_string(), port.parse::<u16>().ok()?)));
    match decoupe {
        Some((hote, port)) if !hote.is_empty() && port != 0 => Ok((hote, port)),
        _ => {
            let _ = flux.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n").await;
            Err("Destination HTTP CONNECT invalide.".into())
        }
    }
}

async fn repondre<S: AsyncWrite + Unpin>(flux: &mut S, genre: Genre, reussi: bool) -> std::io::Result<()> {
    match (genre, reussi) {
        (Genre::Socks, true) => flux.write_all(&reponse_socks(0)).await,
        (Genre::Socks, false) => flux.write_all(&reponse_socks(5)).await,
        (Genre::Http, true) => flux.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await,
        (Genre::Http, false) => flux.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n").await,
        _ => Ok(()),
    }
}

struct Active(Arc<Compteurs>);

impl Drop for Active {
    fn drop(&mut self) {
        self.0.actives.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn relayer(canal: Channel<client::Msg>, flux: TcpStream, compteurs: Arc<Compteurs>, mut arret: watch::Receiver<bool>) {
    compteurs.actives.fetch_add(1, Ordering::Relaxed);
    let _active = Active(compteurs.clone());
    let (mut lecture, ecriture) = canal.split();
    let (mut tcp_lecture, mut tcp_ecriture) = flux.into_split();
    let (emetteur, mut recepteur) = mpsc::unbounded_channel::<Vec<u8>>();
    let en_attente = AtomicUsize::new(0);
    {
        let depuis_ssh = async {
            let mut emetteur = Some(emetteur);
            while let Some(message) = lecture.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        let Some(sortie) = &emetteur else { continue };
                        if en_attente.fetch_add(data.len(), Ordering::Relaxed) > PLAFOND_ATTENTE {
                            compteurs.noter("Connexion fermée : le client local ne consomme pas assez vite.".into());
                            return;
                        }
                        if sortie.send(data.to_vec()).is_err() {
                            return;
                        }
                    }
                    ChannelMsg::Eof => emetteur = None,
                    ChannelMsg::Close => return,
                    _ => {}
                }
            }
        };
        let vers_tcp = async {
            while let Some(morceau) = recepteur.recv().await {
                en_attente.fetch_sub(morceau.len(), Ordering::Relaxed);
                if tcp_ecriture.write_all(&morceau).await.is_err() {
                    break;
                }
                compteurs.descendu.fetch_add(morceau.len() as u64, Ordering::Relaxed);
            }
            let _ = tcp_ecriture.shutdown().await;
        };
        let depuis_tcp = async {
            let mut tampon = vec![0; 32 * 1024];
            loop {
                match tcp_lecture.read(&mut tampon).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if ecriture.data(&tampon[..n]).await.is_err() {
                            break;
                        }
                        compteurs.monte.fetch_add(n as u64, Ordering::Relaxed);
                    }
                }
            }
            let _ = ecriture.eof().await;
            // La connexion dure tant que le canal n'est pas fermé de l'autre côté.
            std::future::pending::<()>().await;
        };
        tokio::select! {
            _ = async { tokio::join!(depuis_ssh, vers_tcp) } => {}
            _ = depuis_tcp => {}
            _ = arrete(&mut arret) => {}
        }
    }
    let _ = ecriture.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn socks_nom_de_domaine() {
        let (mut client, mut proxy) = duplex(256);
        let negociation = tokio::spawn(async move { negocier_socks(&mut proxy).await });
        client.write_all(&[5, 1, 0]).await.unwrap();
        let mut reponse = [0; 2];
        client.read_exact(&mut reponse).await.unwrap();
        assert_eq!(reponse, [5, 0]);
        client.write_all(&[5, 1, 0, 3, 7]).await.unwrap();
        client.write_all(b"exemple").await.unwrap();
        client.write_all(&443u16.to_be_bytes()).await.unwrap();
        assert_eq!(negociation.await.unwrap().unwrap(), ("exemple".to_string(), 443));
    }

    #[tokio::test]
    async fn socks_refuse_sans_methode_anonyme_et_bind() {
        let (mut client, mut proxy) = duplex(256);
        let negociation = tokio::spawn(async move { negocier_socks(&mut proxy).await });
        client.write_all(&[5, 1, 2]).await.unwrap();
        let mut reponse = [0; 2];
        client.read_exact(&mut reponse).await.unwrap();
        assert_eq!(reponse, [5, 0xff]);
        assert!(negociation.await.unwrap().is_err());

        let (mut client, mut proxy) = duplex(256);
        let negociation = tokio::spawn(async move { negocier_socks(&mut proxy).await });
        client.write_all(&[5, 1, 0, 5, 2, 0, 1]).await.unwrap();
        let mut reponse = [0; 12];
        client.read_exact(&mut reponse).await.unwrap();
        assert_eq!(reponse[3], 7);
        assert!(negociation.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn http_connect_et_ipv6() {
        let (mut client, mut proxy) = duplex(1024);
        client.write_all(b"CONNECT [::1]:8443 HTTP/1.1\r\nHost: x\r\n\r\nSUITE").await.unwrap();
        assert_eq!(negocier_http(&mut proxy).await.unwrap(), ("::1".to_string(), 8443));
        let mut suite = [0; 5];
        proxy.read_exact(&mut suite).await.unwrap();
        assert_eq!(&suite, b"SUITE", "les octets après l'en-tête appartiennent au tunnel");
    }

    #[tokio::test]
    async fn http_refuse_autre_methode_et_cible_invalide() {
        for requete in ["GET / HTTP/1.1\r\n\r\n", "CONNECT sansport HTTP/1.1\r\n\r\n", "CONNECT hote:0 HTTP/1.1\r\n\r\n"] {
            let (mut client, mut proxy) = duplex(1024);
            client.write_all(requete.as_bytes()).await.unwrap();
            assert!(negocier_http(&mut proxy).await.is_err(), "{requete}");
        }
    }

    #[test]
    fn interface_vide_reste_sur_ce_poste() {
        assert_eq!(Regle::default().hote_ecoute(), "127.0.0.1");
    }

    #[test]
    fn regles_incompletes_refusees() {
        let vide = Regle { genre: Genre::Local, ecoute_port: 8080, ..Default::default() };
        assert_eq!(vide.valider().unwrap_err().code, CodeTunnel::Invalide);
        let proxy = Regle { genre: Genre::Socks, ecoute_port: 1080, ..Default::default() };
        assert!(proxy.valider().is_ok());
    }
}
