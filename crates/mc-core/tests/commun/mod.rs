//! Agent SSH de test, sur un tube nommé (Windows) ou une socket (Unix).

use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::agent::server::serve;

pub type Client = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

#[cfg(windows)]
pub async fn agent_local(nom: &str) -> (String, Client) {
    use tokio::net::windows::named_pipe::ServerOptions;

    let chemin = format!(r"\\.\pipe\{nom}-{}", std::process::id());
    // La première instance existe avant le retour : un client peut s'y connecter aussitôt.
    let premiere = ServerOptions::new().first_pipe_instance(true).create(&chemin).unwrap();
    let flux = Box::pin(futures::stream::unfold((chemin.clone(), premiere), |(tube, serveur)| async move {
        serveur.connect().await.ok()?;
        let suivante = ServerOptions::new().create(&tube).ok()?;
        Some((Ok(serveur), (tube, suivante)))
    }));
    tokio::spawn(async move {
        let _ = serve(flux, ()).await;
    });
    let client = AgentClient::connect_named_pipe(&chemin).await.unwrap().dynamic();
    (chemin, client)
}

#[cfg(unix)]
pub async fn agent_local(nom: &str) -> (String, Client) {
    use tokio::net::UnixListener;

    let chemin = std::env::temp_dir().join(format!("{nom}-{}.sock", std::process::id())).display().to_string();
    let _ = std::fs::remove_file(&chemin);
    let ecoute = UnixListener::bind(&chemin).unwrap();
    let flux = Box::pin(futures::stream::unfold(ecoute, |ecoute| async move {
        let accepte = ecoute.accept().await.map(|(f, _)| f);
        Some((accepte, ecoute))
    }));
    tokio::spawn(async move {
        let _ = serve(flux, ()).await;
    });
    let client = AgentClient::connect_uds(&chemin).await.unwrap().dynamic();
    (chemin, client)
}
