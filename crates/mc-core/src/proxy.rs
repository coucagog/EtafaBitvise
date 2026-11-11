//! Côté client d'un proxy : SOCKS5 et HTTP CONNECT, sans authentification.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const TAILLE_MAX_REPONSE: usize = 8 * 1024;

pub(crate) async fn par_socks5<S: AsyncRead + AsyncWrite + Unpin>(flux: &mut S, hote: &str, port: u16) -> Result<(), String> {
    let taille = u8::try_from(hote.len()).map_err(|_| "nom d'hôte trop long pour SOCKS5".to_string())?;
    let e = |e: std::io::Error| format!("échange interrompu ({e})");
    flux.write_all(&[5, 1, 0]).await.map_err(e)?;
    let mut methode = [0; 2];
    flux.read_exact(&mut methode).await.map_err(e)?;
    if methode != [5, 0] {
        return Err("le proxy exige une authentification non prise en charge".into());
    }
    let mut demande = vec![5, 1, 0, 3, taille];
    demande.extend_from_slice(hote.as_bytes());
    demande.extend_from_slice(&port.to_be_bytes());
    flux.write_all(&demande).await.map_err(e)?;

    let mut tete = [0; 4];
    flux.read_exact(&mut tete).await.map_err(e)?;
    if tete[1] != 0 {
        return Err(format!("refus du proxy, code {}", tete[1]));
    }
    let reste = match tete[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut longueur = [0];
            flux.read_exact(&mut longueur).await.map_err(e)?;
            usize::from(longueur[0]) + 2
        }
        autre => return Err(format!("réponse SOCKS inconnue ({autre})")),
    };
    let mut adresse_liee = vec![0; reste];
    flux.read_exact(&mut adresse_liee).await.map_err(e)?;
    Ok(())
}

pub(crate) async fn par_http<S: AsyncRead + AsyncWrite + Unpin>(flux: &mut S, hote: &str, port: u16) -> Result<(), String> {
    let e = |e: std::io::Error| format!("échange interrompu ({e})");
    let cible = if hote.contains(':') { format!("[{hote}]:{port}") } else { format!("{hote}:{port}") };
    flux.write_all(format!("CONNECT {cible} HTTP/1.1\r\nHost: {cible}\r\n\r\n").as_bytes()).await.map_err(e)?;

    // Octet par octet : la bannière SSH peut suivre l'en-tête dans le même paquet.
    let mut reponse = Vec::with_capacity(128);
    while !reponse.ends_with(b"\r\n\r\n") {
        if reponse.len() >= TAILLE_MAX_REPONSE {
            return Err("réponse HTTP trop longue".into());
        }
        let mut octet = [0];
        flux.read_exact(&mut octet).await.map_err(e)?;
        reponse.push(octet[0]);
    }
    let texte = String::from_utf8_lossy(&reponse);
    let ligne = texte.lines().next().unwrap_or_default();
    match ligne.split_ascii_whitespace().nth(1) {
        Some(code) if code.starts_with('2') => Ok(()),
        Some("407") => Err("le proxy exige une authentification non prise en charge".into()),
        _ => Err(format!("refus du proxy : {ligne}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn socks5_envoie_le_nom_et_lit_toute_la_reponse() {
        let (mut client, mut proxy) = duplex(512);
        let tache = tokio::spawn(async move {
            let mut salut = [0; 3];
            proxy.read_exact(&mut salut).await.unwrap();
            proxy.write_all(&[5, 0]).await.unwrap();
            let mut demande = [0; 5 + 7 + 2];
            proxy.read_exact(&mut demande).await.unwrap();
            assert_eq!(&demande[5..12], b"serveur");
            assert_eq!(u16::from_be_bytes([demande[12], demande[13]]), 22);
            proxy.write_all(&[5, 0, 0, 1, 1, 2, 3, 4, 0, 80]).await.unwrap();
            proxy.write_all(b"SSH").await.unwrap();
        });
        par_socks5(&mut client, "serveur", 22).await.unwrap();
        let mut suite = [0; 3];
        client.read_exact(&mut suite).await.unwrap();
        assert_eq!(&suite, b"SSH");
        tache.await.unwrap();
    }

    #[tokio::test]
    async fn socks5_signale_le_refus() {
        let (mut client, mut proxy) = duplex(512);
        proxy.write_all(&[5, 0, 5, 5, 0, 1]).await.unwrap();
        assert!(par_socks5(&mut client, "serveur", 22).await.unwrap_err().contains('5'));
    }

    #[tokio::test]
    async fn socks5_refuse_une_authentification_exigee() {
        let (mut client, mut proxy) = duplex(512);
        proxy.write_all(&[5, 0xff]).await.unwrap();
        assert!(par_socks5(&mut client, "serveur", 22).await.unwrap_err().contains("authentification"));
    }

    #[tokio::test]
    async fn http_garde_les_octets_qui_suivent() {
        let (mut client, mut proxy) = duplex(512);
        proxy.write_all(b"HTTP/1.1 200 OK\r\nX: y\r\n\r\nSSH-2.0").await.unwrap();
        par_http(&mut client, "serveur", 22).await.unwrap();
        let mut suite = [0; 7];
        client.read_exact(&mut suite).await.unwrap();
        assert_eq!(&suite, b"SSH-2.0");
    }

    #[tokio::test]
    async fn http_signale_refus_et_authentification() {
        for (reponse, motif) in [("HTTP/1.1 403 Forbidden\r\n\r\n", "403"), ("HTTP/1.1 407 Auth\r\n\r\n", "authentification")] {
            let (mut client, mut proxy) = duplex(512);
            proxy.write_all(reponse.as_bytes()).await.unwrap();
            assert!(par_http(&mut client, "serveur", 22).await.unwrap_err().contains(motif));
        }
    }
}
