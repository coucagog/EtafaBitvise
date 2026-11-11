# Périmètre fonctionnel — Bitvise SSH Client 9.66

Référence observée : version 9.66 (20 juillet 2026), Windows uniquement.
« Compatible » signifie ici : compatible avec les serveurs SSH standard (OpenSSH, Bitvise SSH
Server en SSH standard). Les extensions propriétaires sont traitées à part.

## 1. Avant connexion (fenêtre principale)

Colonne gauche : Load profile, Save profile, Save profile as, New profile, Close profile.
Onglets : Login, Options, Terminal, RDP, SFTP, Services, C2S, S2C, SSH, Notes, About.
Journal horodaté sous les onglets. Boutons Log in et Exit.

Onglet Login : hôte, port, obfuscation et mot-clé, Kerberos (SPN, GSS key exchange, délégation,
gssapi-keyex), utilisateur, méthode initiale, clé client, passphrase, élévation.
Liens : Proxy settings, Host key manager, Client key manager, Help.

## 2. Après connexion

La colonne gauche change. Libellés confirmés dans `BvSsh.exe` et les captures officielles :

| Action | Effet |
|---|---|
| Save profile / Save profile as | Restent disponibles |
| Bitvise SSH Server Control Panel | Administration d'un Bitvise SSH Server |
| New terminal console | Nouvelle console (plusieurs possibles) |
| New SFTP window | Nouvelle fenêtre de transfert |
| New Remote Desktop | Lance MSTSC à travers le tunnel, avec profil temporaire |
| Log out | Fermeture de session, avec invite si de l'activité est en cours |

Par défaut, un terminal et une fenêtre SFTP s'ouvrent à la connexion (réglable dans Options).

**Fenêtre SFTP** : menus Window, Local, Remote, Upload queue, Download queue, Log. Vues Browse,
Upload queue, Download queue, Log. Deux panneaux Local files et Remote files avec filtre, chemin,
précédent/suivant, rafraîchir, dossier personnel. Colonnes Name, Size, Type, Date Modified.
Barre basse de chaque panneau : Upload ou Download, Auto start, mode Binary, règle « Ask if file
exists », Pause.

Files d'attente : démarrer, mettre en pause, déplacer, retirer, suivre le transfert actif, mode de
transfert, règles si le fichier existe (écraser, reprendre, échouer, demander, renommer).
Miroir local vers distant et distant vers local, avec suppression optionnelle des fichiers absents
et écrasement. Gestion des permissions et renommage.

**Terminal** : xterm, VT100, bvterm ; ligatures, palette xterm, thème sombre.

**Session** : reconnexion automatique avec délai, keep-alive, détection de coupure, déconnexion à la
fin de l'activité, règles C2S/S2C modifiables, compteurs d'octets envoyés et reçus.

## 3. Services

Proxy SOCKS4/4A/5 et HTTP CONNECT, pont FTP vers SFTP, lecteur SFTP (WinFsp, UNC configurable),
redirections C2S et S2C dont sockets Unix côté serveur, rebond (jump proxy).

## 4. Authentification et cryptographie

Mot de passe (dont changement à la connexion), clé publique Ed25519/ECDSA/RSA, keyboard-interactive,
Kerberos/NTLM, agents PuTTY et OpenSSH, 2FA. KEX post-quantique `mlkem768x25519-sha256` et
`mlkem1024nistp384-sha384`, kex strict (Terrapin), mode FIPS.

## 5. Outils en ligne de commande

`sftpc`, `sexec`, `stermc`, `stnlc`, `spksc`, `bvtermc`, `totermc`, `BvSshCtrl`, `log`, `retry`.

## 6. Décisions de périmètre

| Fonction | Windows | Linux | macOS | Android | Décision |
|---|---|---|---|---|---|
| Profils, gestion hôtes et clés | Oui | Oui | Oui | Oui | **Inclus**, format JSON propre |
| Terminal xterm | Oui | Oui | Oui | Oui, barre de touches à prévoir | **Inclus** |
| SFTP, files, reprise, miroir | Oui | Oui | Oui | Oui, accès fichiers limité par Android, vue un panneau | **Inclus** |
| C2S, S2C, SOCKS5, HTTP CONNECT | Oui | Oui | Oui | Service au premier plan requis | **Inclus** |
| Rebond, reconnexion automatique | Oui | Oui | Oui | Oui | **Inclus** |
| Pont FTP vers SFTP | Oui | Oui | Oui | Faible intérêt | Phase ultérieure, à écrire |
| Bureau à distance | MSTSC | Client externe | Client externe | Non | Desktop seulement |
| Lecteur SFTP | WinFsp | FUSE | macFUSE, tiers | Non | Phase ultérieure |
| Kerberos | SSPI | GSSAPI | GSSAPI | Non prévu | **À tester**, du code gssapi existe dans russh |
| Sockets Unix distants | Oui | Oui | Oui | Oui | Phase ultérieure, `streamlocal` existe dans russh |

Limites de compatibilité :

- **`.tlp`** : aucun format publié trouvé. Pas d'import prévu ; pas de rétro-ingénierie.
- **Obfuscation Bitvise** : aucune spécification publique trouvée. Compatibilité non garantie.
- **bvterm** : spécification publique de 2010, utile seulement avec Bitvise SSH Server. Basse priorité.
- **Panneau de contrôle serveur, règles centralisées** : propres à Bitvise SSH Server. Exclus.
- **FIPS** : hors périmètre initial.
- **`mlkem1024nistp384-sha384`, `sntrup761x25519`** : absents de russh 0.65.1.
  `mlkem768x25519-sha256` couvre le besoin post-quantique avec OpenSSH et Bitvise.

Reprise autorisée : les **fonctions**. Interdit : réutiliser icônes, textes, code ou apparence de Bitvise.
