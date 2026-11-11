//! Terminal contre un vrai serveur OpenSSH. Lancé à la main :
//! MC_REEL_HOTE, MC_REEL_PORT, MC_REEL_UTILISATEUR, MC_REEL_CLE (clé sans passphrase),
//! puis `cargo test -p mc-core --test openssh_reel -- --ignored --nocapture --test-threads=1`.

mod commun;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_core::connexion::{connecter, connecter_par, Decision, Decisions, Evenement, Rappel, Session};
use mc_core::hotes::MagasinHotes;
use mc_core::profils::{GenreProxy, MethodeAuth, Profil, Proxy};
use mc_core::sftp::{lister_local, Conflit, Controle, Genre, Progres, SurProgres, ANNULE};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

type Recu = Arc<Mutex<Vec<u8>>>;

fn profil() -> Profil {
    let var = |nom: &str| std::env::var(nom).unwrap_or_else(|_| panic!("{nom} manquante"));
    Profil {
        hote: var("MC_REEL_HOTE"),
        port: var("MC_REEL_PORT").parse().unwrap(),
        utilisateur: var("MC_REEL_UTILISATEUR"),
        methode: MethodeAuth::Cle,
        chemin_cle: var("MC_REEL_CLE"),
        delai_connexion: 20,
        keepalive: true,
        intervalle_keepalive: 15,
        ..Default::default()
    }
}

async fn ouvrir(nom: &str) -> (Session, Arc<Mutex<Vec<String>>>) {
    let chemin = std::env::temp_dir().join(format!("mc-reel-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_file(&chemin);
    let decisions = Decisions::default();
    let reponses = decisions.clone();
    let pertes = Arc::new(Mutex::new(Vec::new()));
    let notes = pertes.clone();
    let rappel: Rappel = Arc::new(move |e| match e {
        Evenement::CleInconnue { .. } => reponses.repondre(Decision::AccepterUneFois),
        Evenement::SessionPerdue { raison } => notes.lock().unwrap().push(raison),
        _ => {}
    });
    let session = connecter(&profil(), None, Arc::new(MagasinHotes::new(chemin)), decisions, rappel)
        .await
        .map_err(|e| format!("{} : {:?}", e.titre, e.detail))
        .unwrap();
    (session, pertes)
}

fn texte(recu: &Recu) -> String {
    String::from_utf8_lossy(&recu.lock().unwrap()).into_owned()
}

async fn attendre(recu: &Recu, motif: &str, secondes: u64) {
    let attente = async {
        while !texte(recu).contains(motif) {
            sleep(Duration::from_millis(10)).await;
        }
    };
    if timeout(Duration::from_secs(secondes), attente).await.is_err() {
        let tout = texte(recu);
        let debut = tout.char_indices().rev().nth(400).map_or(0, |(i, _)| i);
        panic!("« {motif} » absent ; fin de sortie : {}", &tout[debut..]);
    }
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn shell_reel_complet() {
    let (session, pertes) = ouvrir("shell").await;
    let recu = Recu::default();
    let copie = recu.clone();
    let (emetteur, fin) = oneshot::channel();
    let terminal = session
        .ouvrir_terminal(
            "xterm-256color",
            100,
            30,
            Box::new(move |o| copie.lock().unwrap().extend(o)),
            Box::new(move |code| drop(emetteur.send(code))),
        )
        .await
        .unwrap();

    // Les marqueurs sont calculés par le shell : l'écho de la commande ne peut pas les satisfaire.
    terminal.ecrire(b"echo T=$TERM S=$(stty size) M=$((40+2))\n".to_vec());
    attendre(&recu, "T=xterm-256color S=30 100 M=42", 15).await;
    println!("pty et shell : ok");

    terminal.redimensionner(132, 43);
    sleep(Duration::from_millis(300)).await;
    terminal.ecrire(b"echo R=$(stty size)\n".to_vec());
    attendre(&recu, "R=43 132", 10).await;
    println!("redimensionnement : ok");

    terminal.ecrire("printf 'U=\\303\\251\\342\\202\\254\\n'\n".as_bytes().to_vec());
    attendre(&recu, "U=é€", 10).await;
    println!("UTF-8 : ok");

    // Flux entrant abondant pendant que des frappes partent : le cas qui gelait la session.
    let debut = std::time::Instant::now();
    terminal.ecrire(b"seq 1 400000; echo D=$((1000+1))\n".to_vec());
    for _ in 0..500 {
        terminal.ecrire(b"#".to_vec());
    }
    terminal.ecrire(b"\n".to_vec());
    attendre(&recu, "D=1001", 120).await;
    println!("duplex : {} octets en {:?}", recu.lock().unwrap().len(), debut.elapsed());

    terminal.ecrire(b"exit 3\n".to_vec());
    assert_eq!(timeout(Duration::from_secs(10), fin).await.unwrap().unwrap(), Some(3));
    println!("code de sortie : ok");
    assert!(session.est_ouverte());
    assert!(pertes.lock().unwrap().is_empty());
    session.fermer().await;
}

fn sans_suivi() -> SurProgres {
    Box::new(|_| ())
}

fn donnees(taille: usize) -> Vec<u8> {
    (0..taille).map(|i| (i * 31 % 251) as u8).collect()
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn sftp_reel_complet() {
    let (session, _) = ouvrir("sftp").await;
    let sftp = session.ouvrir_sftp().await.unwrap();
    let accueil = sftp.accueil().await.unwrap();
    let base = format!("{accueil}/mc-essai-sftp");
    let _ = sftp.supprimer(&base).await;
    sftp.creer_dossier(&base).await.unwrap();

    let local = std::env::temp_dir().join(format!("mc-sftp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&local);
    let source = local.join("arbre");
    std::fs::create_dir_all(source.join("sous/profond")).unwrap();
    std::fs::write(source.join("gros.bin"), donnees(8 * 1024 * 1024)).unwrap();
    std::fs::write(source.join("sous/profond/é€.txt"), "accentué").unwrap();
    std::fs::write(source.join("vide.txt"), "").unwrap();
    let c = Controle::default();

    // Aller-retour d'un dossier complet.
    let debut = std::time::Instant::now();
    let bilan = sftp.envoyer(&source, &base, Conflit::Ecraser, &c, &sans_suivi()).await.unwrap();
    assert_eq!((bilan.fichiers, bilan.erreurs.len()), (3, 0), "{bilan:?}");
    println!("envoi 8 Mio : {:?}", debut.elapsed());
    let retour = local.join("retour");
    std::fs::create_dir_all(&retour).unwrap();
    let debut = std::time::Instant::now();
    let bilan = sftp.telecharger(&format!("{base}/arbre"), &retour, Conflit::Ecraser, &c, &sans_suivi()).await.unwrap();
    assert_eq!((bilan.fichiers, bilan.erreurs.len()), (3, 0), "{bilan:?}");
    println!("réception 8 Mio : {:?}", debut.elapsed());
    assert_eq!(std::fs::read(retour.join("arbre/gros.bin")).unwrap(), donnees(8 * 1024 * 1024));
    assert_eq!(std::fs::read_to_string(retour.join("arbre/sous/profond/é€.txt")).unwrap(), "accentué");
    assert_eq!(std::fs::metadata(retour.join("arbre/vide.txt")).unwrap().len(), 0);

    // Reprise : la suite d'un fichier tronqué, sans réécrire le début.
    let partiel = retour.join("arbre/gros.bin");
    let tronque = donnees(3 * 1024 * 1024);
    std::fs::write(&partiel, &tronque).unwrap();
    let progres = Arc::new(Mutex::new(Vec::new()));
    let notes = progres.clone();
    let suivi: SurProgres = Box::new(move |p: Progres| notes.lock().unwrap().push(p.fait));
    let bilan = sftp.telecharger(&format!("{base}/arbre/gros.bin"), &retour.join("arbre"), Conflit::Reprendre, &c, &suivi).await.unwrap();
    assert_eq!(bilan.fichiers, 1);
    assert_eq!(std::fs::read(&partiel).unwrap(), donnees(8 * 1024 * 1024));
    assert!(progres.lock().unwrap()[0] >= tronque.len() as u64 || progres.lock().unwrap()[1] >= tronque.len() as u64);
    println!("reprise téléchargement : ok");

    let distant = format!("{base}/arbre/gros.bin");
    sftp.supprimer(&distant).await.unwrap();
    let debut_fichier = local.join("debut");
    std::fs::create_dir_all(&debut_fichier).unwrap();
    std::fs::write(debut_fichier.join("gros.bin"), &tronque).unwrap();
    sftp.envoyer(&debut_fichier.join("gros.bin"), &format!("{base}/arbre"), Conflit::Ecraser, &c, &sans_suivi()).await.unwrap();
    let complet = local.join("complet");
    std::fs::create_dir_all(&complet).unwrap();
    std::fs::write(complet.join("gros.bin"), donnees(8 * 1024 * 1024)).unwrap();
    let bilan = sftp.envoyer(&complet.join("gros.bin"), &format!("{base}/arbre"), Conflit::Reprendre, &c, &sans_suivi()).await.unwrap();
    assert_eq!(bilan.fichiers, 1);
    let verif = local.join("verif");
    std::fs::create_dir_all(&verif).unwrap();
    sftp.telecharger(&distant, &verif, Conflit::Ecraser, &c, &sans_suivi()).await.unwrap();
    assert_eq!(std::fs::read(verif.join("gros.bin")).unwrap(), donnees(8 * 1024 * 1024));
    println!("reprise envoi : ok");

    // Conflits.
    let b = sftp.envoyer(&source.join("vide.txt"), &format!("{base}/arbre"), Conflit::Ignorer, &c, &sans_suivi()).await.unwrap();
    assert_eq!((b.fichiers, b.ignores), (0, 1));
    let b = sftp.envoyer(&source.join("vide.txt"), &format!("{base}/arbre"), Conflit::Renommer, &c, &sans_suivi()).await.unwrap();
    assert_eq!(b.fichiers, 1);
    let noms: Vec<_> = sftp.lister(&format!("{base}/arbre")).await.unwrap().into_iter().map(|e| e.nom).collect();
    assert!(noms.iter().any(|n| n == "vide (1).txt"), "{noms:?}");
    println!("conflits : ok");

    // Pause puis reprise, et annulation : le transfert doit s'arrêter pour de vrai.
    let controle = Arc::new(Controle::default());
    controle.pause(true);
    let sftp = Arc::new(sftp);
    let tache = {
        let (s, c, d, f) = (sftp.clone(), controle.clone(), format!("{base}/arbre"), complet.join("gros.bin"));
        tokio::spawn(async move { s.envoyer(&f, &d, Conflit::Ecraser, &c, &sans_suivi()).await.map(|b| b.fichiers) })
    };
    sleep(Duration::from_millis(500)).await;
    assert!(!tache.is_finished(), "la pause n'a pas retenu le transfert");
    controle.pause(false);
    assert_eq!(timeout(Duration::from_secs(60), tache).await.unwrap().unwrap(), Ok(1));
    let controle = Arc::new(Controle::default());
    controle.pause(true);
    let tache = {
        let (s, c, d, f) = (sftp.clone(), controle.clone(), format!("{base}/arbre"), complet.join("gros.bin"));
        tokio::spawn(async move { s.envoyer(&f, &d, Conflit::Ecraser, &c, &sans_suivi()).await.map(|b| b.fichiers) })
    };
    sleep(Duration::from_millis(300)).await;
    controle.annuler();
    assert_eq!(timeout(Duration::from_secs(10), tache).await.unwrap().unwrap(), Err(ANNULE.to_string()));
    println!("pause et annulation : ok");

    // Permissions, renommage, suppression sans suivre un lien.
    let cible = format!("{base}/arbre/vide.txt");
    sftp.changer_mode(&cible, 0o640).await.unwrap();
    let e = sftp.lister(&format!("{base}/arbre")).await.unwrap();
    assert_eq!(e.iter().find(|x| x.nom == "vide.txt").unwrap().mode, Some(0o640));
    sftp.renommer(&cible, &format!("{base}/arbre/renomme.txt")).await.unwrap();
    assert!(sftp.lister(&format!("{base}/arbre")).await.unwrap().iter().any(|x| x.nom == "renomme.txt"));
    sftp.supprimer(&base).await.unwrap();
    assert!(!sftp.lister(&accueil).await.unwrap().iter().any(|x| x.nom == "mc-essai-sftp"));
    println!("permissions, renommage, suppression : ok");

    let liste = lister_local(local.to_str().unwrap()).await.unwrap();
    assert!(liste.iter().any(|x| x.nom == "arbre" && x.genre == Genre::Dossier));
    std::fs::remove_dir_all(&local).unwrap();
    session.fermer().await;
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn coupure_reelle_puis_nouvelle_session() {
    let (session, pertes) = ouvrir("coupure").await;
    let recu = Recu::default();
    let copie = recu.clone();
    let (emetteur, fin) = oneshot::channel();
    let terminal = session
        .ouvrir_terminal("xterm", 80, 24, Box::new(move |o| copie.lock().unwrap().extend(o)), Box::new(move |c| drop(emetteur.send(c))))
        .await
        .unwrap();
    terminal.ecrire(b"echo P=$((1+1))\n".to_vec());
    attendre(&recu, "P=2", 15).await;

    // Tue le processus sshd de cette session : le serveur disparaît sans fermer proprement le canal.
    terminal.ecrire(b"kill -9 $PPID\n".to_vec());
    assert_eq!(timeout(Duration::from_secs(20), fin).await.unwrap().unwrap(), None);
    sleep(Duration::from_millis(200)).await;
    let notes = pertes.lock().unwrap().clone();
    println!("perte signalée : {notes:?}");
    assert_eq!(notes.len(), 1);
    assert!(!session.est_ouverte());

    let (nouvelle, _) = ouvrir("reprise").await;
    assert!(nouvelle.est_ouverte());
    let recu = Recu::default();
    let copie = recu.clone();
    let t = nouvelle.ouvrir_terminal("xterm", 80, 24, Box::new(move |o| copie.lock().unwrap().extend(o)), Box::new(|_| ())).await.unwrap();
    t.ecrire(b"echo N=$((2+2))\n".to_vec());
    attendre(&recu, "N=4", 15).await;
    println!("nouvelle session : ok");
    nouvelle.fermer().await;
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn tunnels_reels() {
    use mc_core::tunnels::{CodeTunnel, Genre, Regle};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    const VOLUME: u64 = 8 * 1024 * 1024;
    const PORT_VOLUME: u16 = 29871;
    const PORT_ECHO: u16 = 29872;

    async fn port_libre() -> u16 {
        TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port()
    }
    fn regle(genre: Genre, port: u16, cible_port: u16) -> Regle {
        Regle { id: "reel".into(), genre, ecoute_hote: "127.0.0.1".into(), ecoute_port: port, cible_hote: "127.0.0.1".into(), cible_port, ..Default::default() }
    }
    async fn banniere(flux: &mut TcpStream) -> String {
        let mut recu = Vec::new();
        let mut tampon = [0; 128];
        while !recu.contains(&b'\n') {
            let n = timeout(Duration::from_secs(10), flux.read(&mut tampon)).await.expect("bannière attendue").unwrap();
            assert!(n > 0, "connexion fermée avant la bannière");
            recu.extend_from_slice(&tampon[..n]);
        }
        String::from_utf8_lossy(&recu).into_owned()
    }

    let (session, pertes) = ouvrir("tunnels").await;
    let session = Arc::new(session);

    // Client vers serveur : la bannière du sshd de la VM, atteinte depuis elle-même.
    let port = port_libre().await;
    let tunnel = session.ouvrir_tunnel(&regle(Genre::Local, port, 22)).await.unwrap();
    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    assert!(banniere(&mut flux).await.starts_with("SSH-2.0-OpenSSH"));
    assert_eq!(tunnel.mesure().actives, 1);
    drop(flux);
    tunnel.fermer().await;
    println!("local : ok");

    // Destination refusée : la connexion se ferme, la session reste.
    let port = port_libre().await;
    let tunnel = session.ouvrir_tunnel(&regle(Genre::Local, port, 1)).await.unwrap();
    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut tampon = [0; 8];
    let lu = timeout(Duration::from_secs(10), flux.read(&mut tampon)).await.expect("fermeture attendue");
    assert!(matches!(lu, Ok(0) | Err(_)));
    assert!(session.est_ouverte());
    println!("destination refusée : {:?}", tunnel.mesure().derniere_erreur);
    tunnel.fermer().await;

    // Proxy SOCKS5.
    let port = port_libre().await;
    let tunnel = session.ouvrir_tunnel(&regle(Genre::Socks, port, 0)).await.unwrap();
    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    flux.write_all(&[5, 1, 0]).await.unwrap();
    let mut methode = [0; 2];
    flux.read_exact(&mut methode).await.unwrap();
    assert_eq!(methode, [5, 0]);
    flux.write_all(&[5, 1, 0, 3, 9]).await.unwrap();
    flux.write_all(b"localhost").await.unwrap();
    flux.write_all(&22u16.to_be_bytes()).await.unwrap();
    let mut reponse = [0; 10];
    flux.read_exact(&mut reponse).await.unwrap();
    assert_eq!(reponse[1], 0);
    assert!(banniere(&mut flux).await.starts_with("SSH-2.0-OpenSSH"));
    drop(flux);
    tunnel.fermer().await;
    println!("socks : ok");

    // Proxy HTTP CONNECT.
    let port = port_libre().await;
    let tunnel = session.ouvrir_tunnel(&regle(Genre::Http, port, 0)).await.unwrap();
    let mut flux = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    flux.write_all(b"CONNECT 127.0.0.1:22 HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
    let attendu = b"HTTP/1.1 200 Connection established\r\n\r\n";
    let mut reponse = vec![0; attendu.len()];
    flux.read_exact(&mut reponse).await.unwrap();
    assert_eq!(reponse, attendu);
    assert!(banniere(&mut flux).await.starts_with("SSH-2.0-OpenSSH"));
    drop(flux);
    tunnel.fermer().await;
    println!("http connect : ok");

    // Port privilégié : un compte ordinaire doit être refusé par le serveur.
    let refus = session.ouvrir_tunnel(&regle(Genre::Distant, 80, 22)).await.err().unwrap();
    assert_eq!(refus.code, CodeTunnel::Refuse);
    println!("port privilégié : {}", refus.message);

    // Serveur vers client : le shell distant écrit dans le port redirigé.
    let recu = Recu::default();
    let copie = recu.clone();
    let terminal = session
        .ouvrir_terminal("xterm", 200, 40, Box::new(move |d| copie.lock().unwrap().extend_from_slice(&d)), Box::new(|_| {}))
        .await
        .unwrap();

    let puits = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port_puits = puits.local_addr().unwrap().port();
    let tache = tokio::spawn(async move {
        let (mut flux, _) = puits.accept().await.unwrap();
        let mut total = 0u64;
        let mut tampon = vec![0; 64 * 1024];
        loop {
            match flux.read(&mut tampon).await.unwrap() {
                0 => return total,
                n => total += n as u64,
            }
        }
    });
    let tunnel = session.ouvrir_tunnel(&regle(Genre::Distant, PORT_VOLUME, port_puits)).await.unwrap();
    let debut = std::time::Instant::now();
    terminal.ecrire(format!("head -c {VOLUME} /dev/urandom > /dev/tcp/127.0.0.1/{PORT_VOLUME}; echo FIN-$?\n").into_bytes());
    let total = timeout(Duration::from_secs(120), tache).await.expect("volume attendu").unwrap();
    assert_eq!(total, VOLUME);
    attendre(&recu, "FIN-0", 20).await;
    println!("distant : {} Mio en {:?}", VOLUME / 1024 / 1024, debut.elapsed());
    tunnel.fermer().await;

    // Le port doit être rendu par le serveur : on peut le redemander aussitôt.
    let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port_echo = echo.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut flux, _) = echo.accept().await.unwrap();
        let (mut l, mut e) = flux.split();
        let _ = tokio::io::copy(&mut l, &mut e).await;
    });
    let tunnel = session.ouvrir_tunnel(&regle(Genre::Distant, PORT_VOLUME, port_echo)).await.unwrap();
    println!("port distant libéré et réutilisé : ok");
    let tunnel2 = session.ouvrir_tunnel(&regle(Genre::Distant, PORT_ECHO, port_echo)).await.unwrap();
    terminal.ecrire(format!("exec 3<>/dev/tcp/127.0.0.1/{PORT_VOLUME}; printf bonjour-distant >&3; head -c 15 <&3; echo; exec 3>&-\n").into_bytes());
    attendre(&recu, "bonjour-distant\r\n", 20).await;
    let attente = async {
        while tunnel.mesure().descendu != 15 || tunnel.mesure().monte != 15 {
            sleep(Duration::from_millis(20)).await;
        }
    };
    timeout(Duration::from_secs(5), attente).await.expect("compteurs à 15 octets");
    println!("distant duplex : ok");
    tunnel.fermer().await;
    tunnel2.fermer().await;
    drop(terminal);

    assert!(pertes.lock().unwrap().is_empty(), "aucune perte attendue");
    session.fermer().await;
}

fn magasin(nom: &str) -> Arc<MagasinHotes> {
    let chemin = std::env::temp_dir().join(format!("mc-reel-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_file(&chemin);
    Arc::new(MagasinHotes::new(chemin))
}

/// Accepte les clés d'hôte et répond `reponse` à chaque invite de saisie interactive.
fn repondeur(decisions: &Decisions, reponse: Option<String>, invites: Arc<Mutex<Vec<String>>>) -> Rappel {
    let reponses = decisions.clone();
    Arc::new(move |e| match e {
        Evenement::CleInconnue { .. } => reponses.repondre(Decision::AccepterUneFois),
        Evenement::QuestionClavier { invites: questions, .. } => {
            invites.lock().unwrap().extend(questions.iter().map(|q| q.texte.clone()));
            reponses.repondre_clavier(reponse.as_ref().map(|r| vec![r.clone(); questions.len()]));
        }
        _ => {}
    })
}

/// Lance `commande` dans un vrai shell de la session et attend `motif`.
async fn verifier_shell(session: &Session, commande: &str, motif: &str) {
    let recu = Recu::default();
    let copie = recu.clone();
    let terminal = session
        .ouvrir_terminal("xterm", 80, 24, Box::new(move |o| copie.lock().unwrap().extend(o)), Box::new(|_| {}))
        .await
        .unwrap();
    terminal.ecrire(format!("{commande}\n").into_bytes());
    attendre(&recu, motif, 20).await;
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn saisie_interactive_reelle() {
    if std::env::var("MC_REEL_PORT_KI").is_err() {
        println!("ignoré : pas de sshd à saisie interactive");
        return;
    }
    let var = |nom: &str| std::env::var(nom).unwrap_or_else(|_| panic!("{nom} manquante"));
    let mut cible = profil();
    cible.port = var("MC_REEL_PORT_KI").parse().unwrap();
    cible.methode = MethodeAuth::Clavier;

    let invites = Arc::new(Mutex::new(Vec::new()));
    let decisions = Decisions::default();
    let rappel = repondeur(&decisions, Some(var("MC_REEL_MDP")), invites.clone());
    let session = connecter(&cible, None, magasin("ki"), decisions, rappel)
        .await
        .map_err(|e| format!("{} : {:?}", e.titre, e.detail))
        .unwrap();
    assert!(!invites.lock().unwrap().is_empty(), "le serveur devait poser une question");
    println!("invites : {:?}", invites.lock().unwrap());
    verifier_shell(&session, "echo K=$((40+2))", "K=42").await;
    session.fermer().await;

    let decisions = Decisions::default();
    let rappel = repondeur(&decisions, Some("mauvais-code".into()), Arc::default());
    let erreur = connecter(&cible, None, magasin("ki-faux"), decisions, rappel).await.err().expect("refus attendu");
    println!("réponse fausse : {}", erreur.titre);
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn rebond_reel() {
    // La VM se rejoint elle-même : la seconde session vient donc de 127.0.0.1 vu par son sshd.
    let intermediaire = profil();
    let cible = Profil { hote: "127.0.0.1".into(), port: 22, ..profil() };
    let decisions = Decisions::default();
    let rappel = repondeur(&decisions, None, Arc::default());
    let session = connecter_par(&cible, None, Some((intermediaire, None)), magasin("rebond"), decisions, rappel)
        .await
        .map_err(|e| format!("{} : {:?}", e.titre, e.detail))
        .unwrap();
    verifier_shell(&session, "echo C=${SSH_CONNECTION%% *} P=$((1+1))", "C=127.0.0.1 P=2").await;
    session.fermer().await;
    println!("rebond : ok");
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn proxy_socks_reel() {
    use mc_core::tunnels::{Genre as GenreTunnel, Regle};

    // Le proxy est un vrai tunnel SOCKS de l'application, ouvert sur une première session.
    let (premiere, _) = ouvrir("proxy-1").await;
    let premiere = Arc::new(premiere);
    let libre = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port();
    let regle = Regle { id: "s".into(), genre: GenreTunnel::Socks, ecoute_hote: "127.0.0.1".into(), ecoute_port: libre, ..Default::default() };
    let tunnel = premiere.ouvrir_tunnel(&regle).await.unwrap();

    let cible = Profil {
        hote: "127.0.0.1".into(),
        port: 22,
        proxy: Proxy { genre: GenreProxy::Socks5, hote: "127.0.0.1".into(), port: tunnel.port },
        ..profil()
    };
    let decisions = Decisions::default();
    let rappel = repondeur(&decisions, None, Arc::default());
    let session = connecter(&cible, None, magasin("proxy-2"), decisions, rappel)
        .await
        .map_err(|e| format!("{} : {:?}", e.titre, e.detail))
        .unwrap();
    verifier_shell(&session, "echo C=${SSH_CONNECTION%% *} P=$((2+1))", "C=127.0.0.1 P=3").await;
    session.fermer().await;
    tunnel.fermer().await;
    premiere.fermer().await;
    println!("proxy SOCKS : ok");
}

async fn agent_avec_cle(nom: &str) -> String {
    let (chemin, mut agent) = commun::agent_local(nom).await;
    let cle = russh::keys::load_secret_key(std::env::var("MC_REEL_CLE").unwrap(), None).unwrap();
    agent.add_identity(&cle, &[]).await.unwrap();
    chemin
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn agent_reel() {
    std::env::set_var("SSH_AUTH_SOCK", agent_avec_cle("mc-essai-reel").await);
    let cible = Profil { methode: MethodeAuth::Agent, chemin_cle: String::new(), ..profil() };
    let decisions = Decisions::default();
    let rappel = repondeur(&decisions, None, Arc::default());
    let session = connecter(&cible, None, magasin("agent"), decisions, rappel)
        .await
        .map_err(|e| format!("{} : {:?}", e.titre, e.detail))
        .unwrap();
    verifier_shell(&session, "echo G=$((5+5))", "G=10").await;
    session.fermer().await;
    println!("agent : ok");
}

/// Agent à disposition du parcours graphique, le temps de l'essai.
#[tokio::test]
#[ignore = "lancé à la main pour le parcours graphique"]
async fn agent_pour_interface() {
    if std::env::var("MC_REEL_INTERFACE").is_err() {
        return;
    }
    println!("TUBE {}", agent_avec_cle("mc-essai-gui").await);
    sleep(Duration::from_secs(150)).await;
}

use mc_core::ftp::Passerelle;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

struct ClientFtp {
    lecture: BufReader<OwnedReadHalf>,
    ecriture: OwnedWriteHalf,
}

impl ClientFtp {
    async fn connecter(port: u16) -> Self {
        let (lecture, ecriture) = TcpStream::connect(("127.0.0.1", port)).await.unwrap().into_split();
        let mut client = Self { lecture: BufReader::new(lecture), ecriture };
        assert_eq!(client.reponse().await.0, 220);
        client
    }

    async fn reponse(&mut self) -> (u16, String) {
        let mut tout = String::new();
        loop {
            let mut ligne = String::new();
            timeout(Duration::from_secs(20), self.lecture.read_line(&mut ligne)).await.unwrap().unwrap();
            assert!(!ligne.is_empty(), "connexion fermée après : {tout}");
            tout.push_str(&ligne);
            let b = ligne.as_bytes();
            if b.len() > 3 && b[3] == b' ' && b[..3].iter().all(u8::is_ascii_digit) {
                return (ligne[..3].parse().unwrap(), tout);
            }
        }
    }

    async fn envoyer(&mut self, commande: &str) -> (u16, String) {
        self.ecriture.write_all(format!("{commande}\r\n").as_bytes()).await.unwrap();
        self.reponse().await
    }

    async fn code(&mut self, commande: &str) -> u16 {
        self.envoyer(commande).await.0
    }

    async fn passif(&mut self) -> TcpStream {
        let (code, texte) = self.envoyer("EPSV").await;
        assert_eq!(code, 229, "{texte}");
        let port: u16 = texte.split("|||").nth(1).unwrap().split('|').next().unwrap().parse().unwrap();
        TcpStream::connect(("127.0.0.1", port)).await.unwrap()
    }

    async fn lire(&mut self, commande: &str) -> Vec<u8> {
        let mut flux = self.passif().await;
        let (code, texte) = self.envoyer(commande).await;
        assert_eq!(code, 150, "{commande} : {texte}");
        let mut contenu = Vec::new();
        flux.read_to_end(&mut contenu).await.unwrap();
        assert_eq!(self.reponse().await.0, 226);
        contenu
    }

    async fn ecrire(&mut self, commande: &str, contenu: &[u8]) {
        let mut flux = self.passif().await;
        let (code, texte) = self.envoyer(commande).await;
        assert_eq!(code, 150, "{commande} : {texte}");
        flux.write_all(contenu).await.unwrap();
        flux.shutdown().await.unwrap();
        drop(flux);
        let (code, texte) = self.reponse().await;
        assert_eq!(code, 226, "{commande} : {texte}");
    }
}

#[tokio::test]
#[ignore = "demande un serveur OpenSSH réel"]
async fn passerelle_ftp_reelle() {
    let (session, _) = ouvrir("ftp").await;
    let sftp = Arc::new(session.ouvrir_sftp().await.unwrap());
    let accueil = sftp.accueil().await.unwrap();
    let base = format!("{accueil}/mc-essai-ftp");
    let _ = sftp.supprimer(&base).await;

    let passerelle = Passerelle::ouvrir(sftp.clone(), 0).await.unwrap();
    let acces = passerelle.acces.clone();
    assert_eq!(acces.mot_de_passe.len(), 32);

    let mut c = ClientFtp::connecter(acces.port).await;
    assert_eq!(c.code("PWD").await, 530, "rien sans authentification");
    assert_eq!(c.code("RETR /etc/passwd").await, 530);
    assert_eq!(c.code(&format!("USER {}", acces.utilisateur)).await, 331);
    assert_eq!(c.code("PASS mauvais").await, 530);
    assert_eq!(c.code(&format!("USER {}", acces.utilisateur)).await, 331);
    assert_eq!(c.code(&format!("PASS {}", acces.mot_de_passe)).await, 230);

    assert!(c.envoyer("FEAT").await.1.contains("MLST"));
    let (code, texte) = c.envoyer("PWD").await;
    assert_eq!(code, 257);
    assert!(texte.contains(&format!("\"{accueil}\"")), "{texte}");
    assert_eq!(c.code("PORT 127,0,0,1,4,1").await, 502, "mode actif refusé");
    assert_eq!(c.code("EPRT |1|127.0.0.1|1025|").await, 502);
    let (code, texte) = c.envoyer("PASV").await;
    assert_eq!(code, 227);
    assert!(texte.contains("(127,0,0,1,"), "{texte}");

    assert_eq!(c.code("MKD mc-essai-ftp").await, 257);
    assert_eq!(c.code("CWD mc-essai-ftp").await, 250);
    assert_eq!(c.code("TYPE I").await, 200);

    let contenu = donnees(1_000_000);
    c.ecrire("STOR gros été.bin", &contenu).await;
    assert_eq!(c.envoyer("SIZE gros été.bin").await.1.trim(), "213 1000000");
    assert_eq!(c.lire("RETR gros été.bin").await, contenu);
    assert_eq!(c.code("REST 999000").await, 350);
    assert_eq!(c.lire("RETR gros été.bin").await, &contenu[999_000..]);
    assert_eq!(c.lire("RETR gros été.bin").await.len(), 1_000_000, "REST ne vaut qu'une fois");
    assert_eq!(c.code("RETR absent").await, 550);

    c.ecrire("STOR suite.txt", b"debut").await;
    c.ecrire("APPE suite.txt", b"-fin").await;
    assert_eq!(c.lire("RETR suite.txt").await, b"debut-fin");
    assert_eq!(c.code("REST 5").await, 350);
    c.ecrire("STOR suite.txt", b"+REPRIS").await;
    assert_eq!(c.lire("RETR suite.txt").await, b"debut+REPRIS");

    let liste = String::from_utf8(c.lire("LIST -la").await).unwrap();
    assert!(liste.lines().any(|l| l.starts_with('-') && l.ends_with(" gros été.bin") && l.contains("1000000")), "{liste}");
    let noms = String::from_utf8(c.lire("NLST").await).unwrap();
    assert!(noms.lines().any(|l| l == "suite.txt"), "{noms}");
    let mlsd = String::from_utf8(c.lire("MLSD").await).unwrap();
    assert!(mlsd.lines().any(|l| l.starts_with("type=file;size=1000000;modify=") && l.ends_with("; gros été.bin")), "{mlsd}");
    assert!(c.envoyer("MLST suite.txt").await.1.contains("type=file;size=12;"));
    assert!(c.envoyer("MDTM suite.txt").await.1.starts_with("213 20"));

    assert_eq!(c.code("RNTO sans-rnfr").await, 503);
    assert_eq!(c.code("RNFR suite.txt").await, 350);
    assert_eq!(c.code("RNTO renomme.txt").await, 250);
    assert_eq!(c.code("MKD sous").await, 257);
    assert_eq!(c.code("CWD sous/../sous").await, 250);
    assert_eq!(c.code("CDUP").await, 250);
    assert_eq!(c.code("RETR sous").await, 550, "pas de RETR sur un dossier");
    assert_eq!(c.code("CWD renomme.txt").await, 550);
    assert_eq!(c.code("RMD sous").await, 250);
    assert_eq!(c.code("DELE renomme.txt").await, 250);
    assert_eq!(c.code("DELE gros été.bin").await, 250);
    assert_eq!(c.code("QUIT").await, 221);

    // La fermeture coupe l'écoute et les clients déjà connectés.
    let mut suivant = ClientFtp::connecter(acces.port).await;
    drop(passerelle);
    sleep(Duration::from_millis(300)).await;
    let mut reste = Vec::new();
    // Une fin de flux ou une erreur conviennent : seule une attente sans fin serait un défaut.
    let _ = timeout(Duration::from_secs(5), suivant.lecture.read_to_end(&mut reste)).await.expect("client encore connecté");
    assert!(TcpStream::connect(("127.0.0.1", acces.port)).await.is_err(), "port encore ouvert");

    sftp.supprimer(&base).await.unwrap();
    session.fermer().await;
}
