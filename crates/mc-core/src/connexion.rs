//! Établit une session SSH et rend compte de chaque étape.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, AuthResult, DisconnectReason, KeyboardInteractiveAuthResponse};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelOpenFailure, Disconnect, MethodKind};
use serde::{Deserialize, Serialize};
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::agent::AgentIdentity;
use russh::keys::Algorithm;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio::time::timeout;

use crate::hotes::{empreinte, Empreinte, MagasinHotes, Verification};
use crate::profils::{GenreProxy, MethodeAuth, Profil};
use crate::proxy;
use crate::tunnels::Renvois;

pub const NOMBRE_ETAPES: usize = 6;
const ETAPE_NOM: usize = 0;
const ETAPE_TCP: usize = 1;
const ETAPE_ALGOS: usize = 2;
const ETAPE_CLE: usize = 3;
const ETAPE_AUTH: usize = 4;
const ETAPE_SESSION: usize = 5;
/// Un serveur ne doit pas pouvoir faire tourner la saisie interactive sans fin.
const ECHANGES_CLAVIER_MAX: usize = 16;
const TAILLE_TEXTE_SERVEUR_MAX: usize = 512;
/// Au-delà, sshd coupe la connexion (MaxAuthTries vaut 6 par défaut).
const TENTATIVES_AGENT_MAX: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EtatEtape {
    EnCours,
    Fait,
    Attente,
    Erreur,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Evenement {
    Etape { index: usize, etat: EtatEtape, detail: String },
    CleInconnue { hote: String, port: u16, presentee: Empreinte },
    CleModifiee { hote: String, port: u16, presentee: Empreinte, enregistrees: Vec<Empreinte> },
    SessionPerdue { raison: String },
    QuestionClavier { nom: String, instructions: String, invites: Vec<Invite> },
}

#[derive(Debug, Clone, Serialize)]
pub struct Invite {
    pub texte: String,
    pub masque: bool,
}

pub type Rappel = Arc<dyn Fn(Evenement) + Send + Sync>;

/// Annonce au plus une fois la perte d'une session établie. Muette pendant la négociation
/// et après une déconnexion demandée par l'utilisateur.
#[derive(Clone)]
pub(crate) struct Perte {
    rappel: Rappel,
    muette: Arc<AtomicBool>,
}

impl Perte {
    fn new(rappel: Rappel) -> Self {
        Self { rappel, muette: Arc::new(AtomicBool::new(true)) }
    }

    fn armer(&self) {
        self.muette.store(false, Ordering::SeqCst);
    }

    fn taire(&self) {
        self.muette.store(true, Ordering::SeqCst);
    }

    pub(crate) fn signaler(&self, raison: String) {
        if !self.muette.swap(true, Ordering::SeqCst) {
            (self.rappel)(Evenement::SessionPerdue { raison });
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decision {
    Annuler,
    AccepterUneFois,
    EnregistrerEtAccepter,
    Remplacer,
}

/// Point de rencontre entre la question posée pendant la négociation et la réponse de l'utilisateur.
#[derive(Clone, Default)]
pub struct Decisions {
    decision: Arc<Mutex<Option<oneshot::Sender<Decision>>>>,
    clavier: Arc<Mutex<Option<oneshot::Sender<Vec<String>>>>>,
}

impl Decisions {
    pub fn repondre(&self, decision: Decision) {
        if let Some(emetteur) = self.decision.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = emetteur.send(decision);
        }
    }

    /// Une question abandonnée vaut un refus.
    async fn attendre(&self, poser: impl FnOnce()) -> Decision {
        let (emetteur, recepteur) = oneshot::channel();
        *self.decision.lock().unwrap_or_else(|p| p.into_inner()) = Some(emetteur);
        poser();
        recepteur.await.unwrap_or(Decision::Annuler)
    }

    /// `None` abandonne la saisie interactive en cours.
    pub fn repondre_clavier(&self, reponses: Option<Vec<String>>) {
        let emetteur = self.clavier.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let (Some(emetteur), Some(reponses)) = (emetteur, reponses) {
            let _ = emetteur.send(reponses);
        }
    }

    async fn attendre_clavier(&self, poser: impl FnOnce()) -> Option<Vec<String>> {
        let (emetteur, recepteur) = oneshot::channel();
        *self.clavier.lock().unwrap_or_else(|p| p.into_inner()) = Some(emetteur);
        poser();
        recepteur.await.ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeErreur {
    Occupe,
    Annulee,
    Injoignable,
    CleRefusee,
    CleModifiee,
    Negociation,
    CleLocale,
    Authentification,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErreurConnexion {
    pub code: CodeErreur,
    pub titre: String,
    pub texte: String,
    pub detail: Option<String>,
    pub conseil: Option<String>,
}

impl ErreurConnexion {
    pub fn simple(code: CodeErreur, titre: &str, texte: String) -> Self {
        Self { code, titre: titre.into(), texte, detail: None, conseil: None }
    }

    fn avec_conseil(mut self, conseil: &str) -> Self {
        self.conseil = Some(conseil.into());
        self
    }

    fn avec_detail(mut self, detail: String) -> Self {
        self.detail = Some(detail);
        self
    }
}

pub(crate) struct Client {
    hote: String,
    port: u16,
    hotes: Arc<MagasinHotes>,
    decisions: Decisions,
    rappel: Rappel,
    refus: Arc<Mutex<Option<ErreurConnexion>>>,
    perte: Perte,
    renvois: Renvois,
}

impl Client {
    fn etape(&self, index: usize, etat: EtatEtape, detail: String) {
        (self.rappel)(Evenement::Etape { index, etat, detail });
    }

    fn refuser(&self, erreur: ErreurConnexion) -> bool {
        self.etape(ETAPE_CLE, EtatEtape::Erreur, erreur.titre.clone());
        *self.refus.lock().unwrap_or_else(|p| p.into_inner()) = Some(erreur);
        false
    }

    fn annulee(&self) -> bool {
        self.refuser(ErreurConnexion::simple(
            CodeErreur::CleRefusee,
            "Clé d'hôte refusée",
            "La connexion a été interrompue avant toute authentification.".into(),
        ))
    }

    fn erreur_magasin(&self, e: std::io::Error) -> bool {
        self.refuser(
            ErreurConnexion::simple(
                CodeErreur::CleRefusee,
                "Clés d'hôte inaccessibles",
                "La clé du serveur n'a pas pu être vérifiée : la connexion est interrompue.".into(),
            )
            .avec_detail(e.to_string()),
        )
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        canal: Channel<client::Msg>,
        _adresse: &str,
        port: u32,
        _origine: &str,
        _port_origine: u32,
        reponse: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        match self.renvois.servir(canal, port) {
            Some(servir) => {
                reponse.accept().await;
                servir();
            }
            None => reponse.reject(ChannelOpenFailure::AdministrativelyProhibited).await,
        }
        Ok(())
    }

    async fn disconnected(&mut self, raison: DisconnectReason<Self::Error>) -> Result<(), Self::Error> {
        self.perte.signaler(match raison {
            DisconnectReason::ReceivedDisconnect(_) => "Le serveur a mis fin à la connexion.".into(),
            DisconnectReason::Error(e) => format!("La connexion a été perdue : {e}."),
        });
        Ok(())
    }

    async fn check_server_key(&mut self, cle: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let cle = cle.public_key();
        let presentee = empreinte(&cle);
        self.etape(ETAPE_ALGOS, EtatEtape::Fait, String::new());
        self.etape(ETAPE_CLE, EtatEtape::EnCours, presentee.algorithme.clone());

        let verification = match self.hotes.verifier(&self.hote, self.port, &cle) {
            Ok(v) => v,
            Err(e) => return Ok(self.erreur_magasin(e)),
        };
        match verification {
            Verification::Connue => {
                self.etape(
                    ETAPE_CLE,
                    EtatEtape::Fait,
                    format!("{} · conforme à la clé enregistrée", presentee.algorithme),
                );
                Ok(true)
            }
            Verification::Inconnue => {
                self.etape(
                    ETAPE_CLE,
                    EtatEtape::Attente,
                    format!("{} · inconnue, en attente de votre décision", presentee.algorithme),
                );
                let evenement = Evenement::CleInconnue {
                    hote: self.hote.clone(),
                    port: self.port,
                    presentee: presentee.clone(),
                };
                let rappel = self.rappel.clone();
                match self.decisions.attendre(move || rappel(evenement)).await {
                    Decision::AccepterUneFois => {
                        self.etape(ETAPE_CLE, EtatEtape::Fait, format!("{} · acceptée une fois", presentee.algorithme));
                        Ok(true)
                    }
                    Decision::EnregistrerEtAccepter => match self.hotes.enregistrer(&self.hote, self.port, &cle) {
                        Ok(()) => {
                            self.etape(ETAPE_CLE, EtatEtape::Fait, format!("{} · enregistrée", presentee.algorithme));
                            Ok(true)
                        }
                        Err(e) => Ok(self.erreur_magasin(e)),
                    },
                    _ => Ok(self.annulee()),
                }
            }
            Verification::Modifiee(enregistrees) => {
                self.etape(
                    ETAPE_CLE,
                    EtatEtape::Erreur,
                    format!("{} présentée, différente de la clé enregistrée", presentee.algorithme),
                );
                let evenement = Evenement::CleModifiee {
                    hote: self.hote.clone(),
                    port: self.port,
                    presentee: presentee.clone(),
                    enregistrees,
                };
                let rappel = self.rappel.clone();
                // Seul un remplacement explicite poursuit : « accepter une fois » n'a pas cours ici.
                match self.decisions.attendre(move || rappel(evenement)).await {
                    Decision::Remplacer => match self.hotes.remplacer(&self.hote, self.port, &cle) {
                        Ok(()) => {
                            self.etape(
                                ETAPE_CLE,
                                EtatEtape::Fait,
                                format!("{} · nouvelle clé enregistrée", presentee.algorithme),
                            );
                            Ok(true)
                        }
                        Err(e) => Ok(self.erreur_magasin(e)),
                    },
                    _ => Ok(self.refuser(
                        ErreurConnexion::simple(
                            CodeErreur::CleModifiee,
                            "Connexion bloquée : la clé d'hôte a changé",
                            "Aucune donnée d'authentification n'a été envoyée au serveur.".into(),
                        )
                        .avec_conseil(
                            "Confirmez la nouvelle empreinte avec l'administrateur du serveur avant de remplacer la clé.",
                        ),
                    )),
                }
            }
        }
    }
}

pub struct Session {
    pub(crate) poignee: client::Handle<Client>,
    pub(crate) perte: Perte,
    pub(crate) renvois: Renvois,
    /// Reste ouverte tant que cette session passe par elle.
    rebond: Option<Box<Session>>,
}

impl Session {
    pub fn est_ouverte(&self) -> bool {
        !self.poignee.is_closed()
    }

    pub async fn fermer(&self) {
        self.perte.taire();
        let _ = self.poignee.disconnect(Disconnect::ByApplication, "", "fr").await;
        if let Some(rebond) = &self.rebond {
            Box::pin(rebond.fermer()).await;
        }
    }
}

fn nom_methode(m: &MethodKind) -> String {
    let nom: &str = m.into();
    nom.to_string()
}

/// Le texte d'un serveur est affiché tel quel : ni caractères de contrôle, ni longueur démesurée.
fn propre(texte: &str) -> String {
    texte.chars().filter(|c| !c.is_control() || *c == '\n').take(TAILLE_TEXTE_SERVEUR_MAX).collect()
}

fn saisie_abandonnee() -> ErreurConnexion {
    ErreurConnexion::simple(CodeErreur::Annulee, "Connexion annulée", "La saisie interactive a été abandonnée.".into())
}

async fn authentifier_clavier(
    poignee: &mut client::Handle<Client>,
    utilisateur: &str,
    decisions: &Decisions,
    rappel: &Rappel,
) -> Result<Result<AuthResult, russh::Error>, ErreurConnexion> {
    let mut reponse = match poignee.authenticate_keyboard_interactive_start(utilisateur, None::<String>).await {
        Ok(r) => r,
        Err(e) => return Ok(Err(e)),
    };
    for _ in 0..ECHANGES_CLAVIER_MAX {
        match reponse {
            KeyboardInteractiveAuthResponse::Success => return Ok(Ok(AuthResult::Success)),
            KeyboardInteractiveAuthResponse::Failure { remaining_methods, partial_success } => {
                return Ok(Ok(AuthResult::Failure { remaining_methods, partial_success }));
            }
            KeyboardInteractiveAuthResponse::InfoRequest { name, instructions, prompts } => {
                let reponses = if prompts.is_empty() {
                    Vec::new()
                } else {
                    let evenement = Evenement::QuestionClavier {
                        nom: propre(&name),
                        instructions: propre(&instructions),
                        invites: prompts.iter().map(|p| Invite { texte: propre(&p.prompt), masque: !p.echo }).collect(),
                    };
                    let rappel = rappel.clone();
                    let saisies = decisions.attendre_clavier(move || rappel(evenement)).await.ok_or_else(saisie_abandonnee)?;
                    if saisies.len() != prompts.len() {
                        return Err(saisie_abandonnee());
                    }
                    saisies
                };
                reponse = match poignee.authenticate_keyboard_interactive_respond(reponses).await {
                    Ok(r) => r,
                    Err(e) => return Ok(Err(e)),
                };
            }
        }
    }
    Err(ErreurConnexion::simple(
        CodeErreur::Authentification,
        "Authentification interrompue",
        "Le serveur demande trop de saisies successives.".into(),
    ))
}

type ClientAgent = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

/// Sous Windows : `SSH_AUTH_SOCK` s'il désigne un tube nommé, puis l'agent OpenSSH, puis Pageant.
#[cfg(windows)]
async fn ouvrir_agent() -> Result<ClientAgent, String> {
    const TUBE_OPENSSH: &str = r"\\.\pipe\openssh-ssh-agent";
    let tube = std::env::var("SSH_AUTH_SOCK").ok().filter(|v| v.starts_with(r"\\")).unwrap_or_else(|| TUBE_OPENSSH.into());
    match AgentClient::connect_named_pipe(&tube).await {
        Ok(client) => Ok(client.dynamic()),
        Err(e) => AgentClient::connect_pageant()
            .await
            .map(AgentClient::dynamic)
            .map_err(|_| format!("ni l'agent OpenSSH ({e}) ni Pageant ne répondent")),
    }
}

#[cfg(all(unix, not(target_os = "android")))]
async fn ouvrir_agent() -> Result<ClientAgent, String> {
    AgentClient::connect_env().await.map(AgentClient::dynamic).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
async fn ouvrir_agent() -> Result<ClientAgent, String> {
    Err("aucun agent SSH n'existe sur Android".into())
}

pub const AGENT_DISPONIBLE: bool = cfg!(not(target_os = "android"));

fn erreur_agent(texte: String) -> ErreurConnexion {
    ErreurConnexion::simple(CodeErreur::CleLocale, "Agent SSH indisponible", texte)
}

async fn authentifier_agent(
    poignee: &mut client::Handle<Client>,
    utilisateur: &str,
) -> Result<Result<AuthResult, russh::Error>, ErreurConnexion> {
    let mut agent = timeout(Duration::from_secs(5), ouvrir_agent())
        .await
        .map_err(|_| "l'agent ne répond pas".to_string())
        .and_then(|r| r)
        .map_err(|e| {
            erreur_agent(format!("Connexion à l'agent impossible : {e}."))
                .avec_conseil("Démarrez l'agent OpenSSH ou Pageant, puis réessayez.")
        })?;
    let identites = agent.request_identities().await.map_err(|e| erreur_agent(format!("L'agent n'a pas répondu : {e}.")))?;
    if identites.is_empty() {
        return Err(erreur_agent("L'agent est joignable mais ne contient aucune clé.".into()).avec_conseil("Ajoutez une clé avec ssh-add, puis réessayez."));
    }
    let hachage = poignee.best_supported_rsa_hash().await.ok().flatten().flatten();
    let mut refus = None;
    for identite in identites.into_iter().take(TENTATIVES_AGENT_MAX) {
        let AgentIdentity::PublicKey { key, .. } = identite else { continue };
        let hachage_cle = if matches!(key.algorithm(), Algorithm::Rsa { .. }) { hachage } else { None };
        match poignee.authenticate_publickey_with(utilisateur, key, hachage_cle, &mut agent).await {
            Ok(AuthResult::Success) => return Ok(Ok(AuthResult::Success)),
            Ok(echec) => refus = Some(echec),
            Err(e) => return Err(erreur_agent(format!("L'agent n'a pas pu signer : {e}."))),
        }
    }
    refus.map(|r| Ok(Ok(r))).unwrap_or_else(|| Err(erreur_agent("L'agent ne contient que des certificats, non pris en charge.".into())))
}

pub async fn connecter(    profil: &Profil,
    secret: Option<String>,
    hotes: Arc<MagasinHotes>,
    decisions: Decisions,
    rappel: Rappel,
) -> Result<Session, ErreurConnexion> {
    connecter_par(profil, secret, None, hotes, decisions, rappel).await
}

/// `rebond` : profil et secret de la machine intermédiaire. Sa clé d'hôte est vérifiée comme les autres.
pub async fn connecter_par(
    profil: &Profil,
    secret: Option<String>,
    rebond: Option<(Profil, Option<String>)>,
    hotes: Arc<MagasinHotes>,
    decisions: Decisions,
    rappel: Rappel,
) -> Result<Session, ErreurConnexion> {
    let etape = |index, etat, detail: String| rappel(Evenement::Etape { index, etat, detail });
    let adresse = format!("{}:{}", profil.hote, profil.port);
    let delai = Duration::from_secs(u64::from(profil.delai_connexion.max(1)));
    let injoignable = |index: usize, detail: String| {
        etape(index, EtatEtape::Erreur, detail.clone());
        ErreurConnexion::simple(
            CodeErreur::Injoignable,
            "Serveur injoignable",
            format!("La connexion à {adresse} a échoué."),
        )
        .avec_detail(detail)
        .avec_conseil("Vérifiez l'adresse, le port et votre réseau, puis réessayez.")
    };

    if let Some((profil_rebond, secret_rebond)) = rebond {
        etape(ETAPE_NOM, EtatEtape::Fait, format!("résolu par le rebond {}", profil_rebond.hote));
        etape(ETAPE_TCP, EtatEtape::EnCours, format!("{adresse}, par {}", profil_rebond.hote));
        // Les étapes du rebond ne remplacent pas celles du serveur visé ; ses questions, si.
        let transmis = rappel.clone();
        let rappel_rebond: Rappel = Arc::new(move |e| {
            if !matches!(e, Evenement::Etape { .. }) {
                transmis(e);
            }
        });
        let attente = connecter_par(&profil_rebond, secret_rebond, None, hotes.clone(), decisions.clone(), rappel_rebond);
        let intermediaire = Box::pin(attente).await.map_err(|mut e| {
            e.titre = format!("Rebond {} : {}", profil_rebond.hote, e.titre);
            etape(ETAPE_TCP, EtatEtape::Erreur, e.titre.clone());
            e
        })?;
        let ouverture = intermediaire.poignee.channel_open_direct_tcpip(profil.hote.as_str(), u32::from(profil.port), "127.0.0.1", 0);
        let canal = match timeout(delai, ouverture).await {
            Ok(Ok(canal)) => canal,
            Ok(Err(e)) => return Err(injoignable(ETAPE_TCP, format!("le rebond n'a pas pu joindre {adresse} : {e}"))),
            Err(_) => return Err(injoignable(ETAPE_TCP, format!("le rebond ne répond pas pour {adresse}"))),
        };
        etape(ETAPE_TCP, EtatEtape::Fait, format!("{adresse}, par {}", profil_rebond.hote));
        let mut session = negocier(profil, secret, canal.into_stream(), hotes, decisions, rappel.clone()).await?;
        session.rebond = Some(Box::new(intermediaire));
        return Ok(session);
    }

    let par_proxy = profil.proxy.genre != GenreProxy::Aucun;
    let adresse_tcp = if par_proxy { format!("{}:{}", profil.proxy.hote, profil.proxy.port) } else { adresse.clone() };
    let hote_tcp = if par_proxy { profil.proxy.hote.as_str() } else { profil.hote.as_str() };

    etape(ETAPE_NOM, EtatEtape::EnCours, hote_tcp.to_string());
    let adresses: Vec<_> = match timeout(delai, tokio::net::lookup_host(adresse_tcp.as_str())).await {
        Ok(Ok(a)) => a.collect(),
        Ok(Err(e)) => return Err(injoignable(ETAPE_NOM, e.to_string())),
        Err(_) => return Err(injoignable(ETAPE_NOM, "résolution trop longue".into())),
    };
    etape(ETAPE_NOM, EtatEtape::Fait, hote_tcp.to_string());

    etape(ETAPE_TCP, EtatEtape::EnCours, adresse_tcp.clone());
    let mut flux = match timeout(delai, TcpStream::connect(adresses.as_slice())).await {
        Ok(Ok(f)) => f,
        Ok(Err(e)) => return Err(injoignable(ETAPE_TCP, e.to_string())),
        Err(_) => return Err(injoignable(ETAPE_TCP, format!("aucune réponse en {} s", delai.as_secs()))),
    };
    let _ = flux.set_nodelay(true);
    if par_proxy {
        let negociation = async {
            match profil.proxy.genre {
                GenreProxy::Socks5 => proxy::par_socks5(&mut flux, &profil.hote, profil.port).await,
                _ => proxy::par_http(&mut flux, &profil.hote, profil.port).await,
            }
        };
        match timeout(delai, negociation).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(injoignable(ETAPE_TCP, format!("proxy {adresse_tcp} : {e}"))),
            Err(_) => return Err(injoignable(ETAPE_TCP, format!("proxy {adresse_tcp} : aucune réponse"))),
        }
        etape(ETAPE_TCP, EtatEtape::Fait, format!("{adresse}, par le proxy {adresse_tcp}"));
    } else {
        etape(ETAPE_TCP, EtatEtape::Fait, adresse_tcp.clone());
    }
    negocier(profil, secret, flux, hotes, decisions, rappel.clone()).await
}

async fn negocier<S>(
    profil: &Profil,
    secret: Option<String>,
    flux: S,
    hotes: Arc<MagasinHotes>,
    decisions: Decisions,
    rappel: Rappel,
) -> Result<Session, ErreurConnexion>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let etape = |index, etat, detail: String| rappel(Evenement::Etape { index, etat, detail });
    let adresse = format!("{}:{}", profil.hote, profil.port);
    etape(ETAPE_ALGOS, EtatEtape::EnCours, String::new());
    let config = client::Config {
        keepalive_interval: profil
            .keepalive
            .then(|| Duration::from_secs(u64::from(profil.intervalle_keepalive.max(1)))),
        ..Default::default()
    };
    let refus = Arc::new(Mutex::new(None));
    let perte = Perte::new(rappel.clone());
    let renvois = Renvois::default();
    let gestionnaire = Client {
        perte: perte.clone(),
        renvois: renvois.clone(),
        hote: profil.hote.clone(),
        port: profil.port,
        hotes,
        decisions: decisions.clone(),
        rappel: rappel.clone(),
        refus: refus.clone(),
    };
    let mut poignee = match client::connect_stream(Arc::new(config), flux, gestionnaire).await {
        Ok(p) => p,
        Err(e) => {
            if let Some(erreur) = refus.lock().unwrap_or_else(|p| p.into_inner()).take() {
                return Err(erreur);
            }
            etape(ETAPE_ALGOS, EtatEtape::Erreur, e.to_string());
            return Err(ErreurConnexion::simple(
                CodeErreur::Negociation,
                "Échec de la négociation SSH",
                format!("Le serveur {adresse} n'a pas pu établir de connexion sécurisée."),
            )
            .avec_detail(e.to_string()));
        }
    };

    etape(ETAPE_AUTH, EtatEtape::EnCours, profil.utilisateur.clone());
    let erreur_cle = |texte: String, detail: String| {
        etape(ETAPE_AUTH, EtatEtape::Erreur, detail.clone());
        ErreurConnexion::simple(CodeErreur::CleLocale, "Clé privée inutilisable", texte).avec_detail(detail)
    };
    let (resultat, libelle) = match profil.methode {
        MethodeAuth::MotDePasse => (
            poignee.authenticate_password(profil.utilisateur.as_str(), secret.unwrap_or_default()).await,
            "mot de passe".to_string(),
        ),
        MethodeAuth::Agent => (authentifier_agent(&mut poignee, &profil.utilisateur).await?, "agent".to_string()),
        MethodeAuth::Clavier => (
            authentifier_clavier(&mut poignee, &profil.utilisateur, &decisions, &rappel).await?,
            "saisie interactive".to_string(),
        ),
        MethodeAuth::Cle => {
            let phrase = secret.filter(|s| !s.is_empty());
            let cle = load_secret_key(&profil.chemin_cle, phrase.as_deref()).map_err(|e| {
                let texte = if phrase.is_none() && matches!(e, russh::keys::Error::KeyIsEncrypted) {
                    "Cette clé est protégée : saisissez sa passphrase.".to_string()
                } else {
                    format!("La clé {} n'a pas pu être chargée.", profil.chemin_cle)
                };
                erreur_cle(texte, e.to_string())
            })?;
            let hachage = poignee.best_supported_rsa_hash().await.ok().flatten().flatten();
            (
                poignee
                    .authenticate_publickey(
                        profil.utilisateur.as_str(),
                        PrivateKeyWithHashAlg::new(Arc::new(cle), hachage),
                    )
                    .await,
                "clé".to_string(),
            )
        }
    };
    match resultat {
        Ok(AuthResult::Success) => {}
        Ok(AuthResult::Failure { remaining_methods, .. }) => {
            let methodes: Vec<String> = remaining_methods.iter().map(nom_methode).collect();
            etape(ETAPE_AUTH, EtatEtape::Erreur, format!("{libelle} refusé(e)"));
            let mut erreur = ErreurConnexion::simple(
                CodeErreur::Authentification,
                "Authentification refusée",
                format!("Le serveur a refusé : {libelle}, utilisateur « {} ».", profil.utilisateur),
            );
            if !methodes.is_empty() {
                erreur = erreur.avec_detail(format!("Méthodes acceptées par le serveur : {}", methodes.join(", ")));
            }
            return Err(erreur);
        }
        Err(e) => {
            etape(ETAPE_AUTH, EtatEtape::Erreur, e.to_string());
            return Err(ErreurConnexion::simple(
                CodeErreur::Authentification,
                "Authentification interrompue",
                "La connexion a été perdue pendant l'authentification.".into(),
            )
            .avec_detail(e.to_string()));
        }
    }
    etape(ETAPE_AUTH, EtatEtape::Fait, libelle);
    etape(ETAPE_SESSION, EtatEtape::Fait, "session prête".into());
    perte.armer();
    Ok(Session { poignee, perte, renvois, rebond: None })
}
