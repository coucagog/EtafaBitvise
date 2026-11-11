# Choix techniques

| Sujet | Choix | Statut |
|---|---|---|
| Cœur SSH | Rust, `russh` 0.65 (pas de version antérieure à 0.63 : CVE-2026-102824) | Retenu pour le client. Contre un vrai OpenSSH 10.0 : 33 essais sur 33 en duplex intense. Blocages limités à l'appariement avec un serveur russh (banc local). À ne pas utiliser comme serveur de redirection sans revoir ce point |
| SFTP | `russh-sftp` 3.0 | Proposé. Opérations, intégrité et reprise testées sous Windows |
| Runtime | `tokio` | Décidé |
| Application | Tauri 2, comme YawREC, pour les 4 plateformes | Proposé |
| Front | Vite, TypeScript, Svelte | Proposé |
| Terminal | `@xterm/xterm` 6 | Proposé |
| Installeurs Windows | NSIS + MSI, comme YawREC | Proposé |
| Crypto | `aws-lc-rs` (défaut russh), `ring` comme alternative si Android bloque | **À tester** |
| Secrets | Trousseau de l'OS (Credential Manager, Keychain, Secret Service, Keystore) | Proposé |

Pourquoi Svelte plutôt que le JavaScript sans framework de YawREC : YawREC est une mini-fenêtre,
alors qu'ici il faut des listes de fichiers virtualisées, des files de transfert et des onglets
multiples. Le code reste compilé et léger pour la WebView Android.

## Architecture

```
crates/mc-core      SSH, SFTP, tunnels, profils, sans dépendance à Tauri
crates/mc-cli       équivalents sexec / sftpc / stnlc (phase ultérieure)
app/src-tauri       commandes, barre des tâches, plateforme
app/src             interface Vite
```

## Règles de sécurité à tenir

1. Toute la logique SSH et les clés restent côté Rust. Le front ne voit jamais une clé ni un mot de passe.
2. Vérification de la clé d'hôte obligatoire : pas de mode « tout accepter ».
3. Aucun secret dans les profils JSON ni dans les journaux.
   Le mot de passe ou la passphrase saisis restent en mémoire côté Rust jusqu'à la déconnexion, pour `Ctrl+Shift+P` ;
   ils ne sont jamais écrits sur disque. Le trousseau du système les remplacera quand il sera réalisé.
4. CSP stricte, sur le modèle de `tauri.conf.json` de YawREC. Seule souplesse : `style-src 'unsafe-inline'` sans nonce
   (`dangerousDisableAssetCspModification: ["style-src"]`), car xterm.js crée ses styles à l'exécution. `script-src` reste `'self'`.
5. Listes d'algorithmes modernes par défaut ; algorithmes anciens activables explicitement, avec avertissement.
6. `cargo deny` en CI (déjà installé ici).

## Règle d'usage de russh

La boucle de session russh s'arrête quand la file d'un canal est pleine : tous les canaux de la connexion sont alors
gelés. Chaque canal doit être lu en continu par une tâche indépendante de son écriture. Aucune attente sur le réseau,
le disque ou l'interface ne doit retarder cette lecture.

Application dans `crates/mc-core/src/terminal.rs` : une tâche de lecture et une tâche d'écriture par canal, reliées à
l'interface par une file non bornée côté écriture. `terminal_ecrire` est une commande Tauri synchrone pour garder l'ordre des frappes.

## Contraintes par plateforme

- **Android** : NDK r30 (`30.0.16248370`). Cœur testé sur tablette ARM64 (17/17). Sous Windows sans mode développeur, Tauri ne peut pas créer son lien symbolique : copier la bibliothèque dans `jniLibs` puis lancer Gradle (`scripts/attendre-et-tester.ps1 -Gradle`). Les tunnels en arrière-plan exigent un service au premier plan
  avec notification permanente. Le plugin `tauri-plugin-background-service` est tiers : à évaluer avant adoption.
- **macOS et iOS** : la signature et les builds finaux exigent un Mac ou une CI macOS. iOS hors périmètre demandé.
- **Linux** : compilation native à faire sur Linux ou en CI (pas de distribution WSL ici).

## Nom du produit

**MLS Connect**, abrégé `{MC}` (décidé le 2026-11-08). `EtafaBitvise` reste le nom du dossier interne, jamais celui du produit.
Dans les maquettes, `{NomApp}` se remplace par « MLS Connect ».

Note d'installation : TypeScript reste en 5.9, la version 7 n'étant pas utilisée avec `svelte-check`.

## Tunnels

- Écoute par défaut sur 127.0.0.1. Toute autre interface est signalée dans la liste et dans le formulaire.
- Les proxys SOCKS5 et HTTP CONNECT n'ont pas d'authentification : l'écoute sur ce poste en est la seule protection.
- russh rouvre la fenêtre SSH dès la réception, sans tenir compte du client local. Chaque canal est donc lu en continu, et une connexion
  est fermée au-delà de 64 Mio en attente. Une connexion lente ne peut ainsi ni geler la session ni saturer la mémoire.
- Un canal `forwarded-tcpip` entrant sur un port que la session n'a pas demandé est refusé.
- Les règles vivent dans le profil, sans secret. Elles ne sont écrites que par `profil_tunnels`, qui ne touche pas au reste du profil.

## Connexion avancée

- La route réseau (directe, proxy, rebond) produit un flux ; une seule négociation SSH s'applique à tout flux (`client::connect_stream`).
- Saisie interactive : réponses jamais conservées, redemandées à chaque reconnexion. Texte du serveur filtré (caractères de contrôle) et borné ; 16 échanges au plus.
- Proxy : SOCKS5 et HTTP CONNECT, sans authentification en v1. Le nom du serveur est résolu par le proxy, pas localement.
- Rebond : un profil enregistré, un seul niveau, jamais lui-même. La clé d'hôte du rebond et celle de la cible sont vérifiées l'une et l'autre.
- Le profil de rebond est relu sur le disque à la connexion : un brouillon non enregistré n'est pas utilisable.
- Décision de l'utilisateur (9 novembre) : le mot de passe ou la passphrase de la session et du rebond restent en mémoire Rust jusqu'à la déconnexion, pour que `Ctrl+Shift+P` reconnecte sans redemander. Jamais sur disque ni dans le journal.

## Clés et secrets locaux

- Magasin de clés : un fichier OpenSSH par clé dans le dossier de configuration (`keys/`), avec son `.pub`. Noms validés (pas de traversée, ni noms réservés Windows), jamais d'écrasement.
- Clé privée : écrite en 0600 sous Unix dès sa création ; sous Windows, droits hérités du profil.
- Agent SSH : `SSH_AUTH_SOCK` s'il désigne un tube nommé, puis agent OpenSSH, puis Pageant sous Windows ; socket `SSH_AUTH_SOCK` sous Unix. 5 identités essayées au plus (`MaxAuthTries`). Absent sur Android.
- Trousseau : facultatif, par profil, service « MLS Connect ». Écrit seulement après une connexion réussie, effacé au décochage et à la suppression du profil. La valeur ne quitte jamais Rust ; le front n'apprend que sa présence. Absent sur Android pour l'instant.

## Écran Android

- Une seule base Svelte : l'adaptation passe par `styles/tactile.css` et trois classes sur `<html>` (`tactile`, `etroit`, `android`), sans seconde application.
- Touches du terminal : action sur `pointerdown` annulé, pour garder le focus et le clavier virtuel ; `click` seulement pour l'activation au clavier (`detail === 0`).
- Ctrl et Alt de la rangée valent pour la touche suivante, puis se relâchent.
- Les actions sur les fichiers restent accessibles par un bouton : l'appui long ne doit pas être le seul chemin.
- Fichiers locaux sur Android : dossier privé de l'application ; échanges avec le reste de l'appareil par le sélecteur système (voir plus bas).
- Construction sous Windows sans mode développeur : `-ApkApp` puis `-GradleApp`, qui copie la bibliothèque dans `jniLibs` au lieu du lien symbolique.

## Miroir SFTP et journal de session (2026-11-09)

- Le miroir compare par taille puis date (tolérance 2 s) et montre toujours un aperçu avant d'agir.
- Une lecture en échec arrête toute la comparaison : un dossier illisible ne doit pas faire passer ses fichiers pour absents.
- La suppression des éléments absents de la source est décochée par défaut, signalée comme destructive, exécutée après la réussite de toutes les copies, et refusée sur une racine.
- Les liens symboliques ne sont ni suivis ni supprimés. Un nom renvoyé par le serveur contenant un séparateur est refusé.
- Les signets ne contiennent aucun secret : ils restent dans le stockage de la WebView.
- Le journal de session regroupe connexion, terminal, tunnels et SFTP par `app.consigner` ; aucun secret ne doit y être placé. L'export n'accepte que .log et .txt.

## Android : essais et limites (2026-11-09)

- `enableEdgeToEdge` empêche le système de réduire la fenêtre : `MainActivity.kt` applique l'encart du clavier en marge basse. Ce fichier est dans `gen/android` : à conserver si le projet Android est régénéré.
- Ne pas utiliser `am force-stop` pour relancer l'application pendant les essais : la WebView ne redémarre plus son moteur de rendu (constaté aussi sur le prototype, cause non identifiée).

## Passerelle FTP (2026-11-09)

- Écoute sur 127.0.0.1 uniquement, jamais d'option d'exposition réseau : FTP circule en clair.
- Mot de passe de 128 bits tiré à chaque ouverture, gardé en mémoire, jamais journalisé ni enregistré. Trois échecs ferment la connexion.
- Mode passif seul : `PORT` et `EPRT` sont refusés pour exclure le rebond FTP.
- Inactive par défaut, fermée avec la session. Non proposée sur Android, où son intérêt est faible.
- Les noms contenant un caractère de contrôle sont retirés des listes, pour ne pas fabriquer de fausses lignes.

## Clés PuTTY (2026-11-09)

- Un `.ppk` est converti au format OpenSSH à l'import, et reste protégé par sa passphrase s'il en a une.

- Saisie du terminal sur Android : hors composition, `TerminalVue.svelte` lit l'évènement `input` et n'utilise plus la comparaison différée de xterm, qui dupliquait des caractères.
- Avant toute installation d'un APK : supprimer la sortie Gradle et `intermediates/incremental/packageArm64Debug*`, sinon l'empaquetage incrémental double la taille du fichier.

## Linux : construction (2026-11-09)

- Paquets Debian 13 : `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libsecret-1-dev patchelf`.
- `@tauri-apps/cli` n'est pas une dépendance du projet : `cargo tauri` (installé à part), ou `npm run build` puis `cargo build --manifest-path app/src-tauri/Cargo.toml --features tauri/custom-protocol`. Le binaire de cargo se nomme `mc-connect`.
- Avec moins de 4 Go de mémoire : ajouter un swap, limiter à 3 tâches et compiler sans informations de débogage (`CARGO_PROFILE_DEV_DEBUG=0`).

- Premier dessin incomplet sous WebKitGTK (fenêtre transparente) : `main.ts` change le fond de la page un instant, 300 ms puis 1,5 s après le chargement, pour forcer un second dessin. Vérifié sur Xvfb, avec et sans compositeur.
- Paquets : `cargo tauri build --bundles deb,appimage` avec `NO_STRIP=true` et `APPIMAGE_EXTRACT_AND_RUN=1`. Sur Debian 13, `/tmp` est en mémoire : pointer `TMPDIR` vers le disque. Dépendances du `.deb` : `libayatana-appindicator3-1`, `libwebkit2gtk-4.1-0`, `libgtk-3-0`.

## macOS (2026-11-10)

- `tauri.macos.conf.json`, fusionné par Tauri à la construction sur macOS seulement : barre de titre native superposée (`titleBarStyle: Overlay`, titre masqué), fenêtre opaque, paquets `app` et `dmg`, macOS 11 au minimum. Les tableaux remplacent ceux de `tauri.conf.json` : la fenêtre y est décrite en entier.
- Pas de fenêtre transparente : elle exigerait `macOSPrivateApi`, une API privée qu'Apple refuse sur le Mac App Store.
- `main.ts` ajoute la classe `mac` et le fond opaque. `base.css` masque alors nos boutons de fenêtre et laisse 78 px aux trois boutons du système.
- Raccourcis : ceux de la fenêtre restent sur Ctrl (Cmd n'est pas intercepté). Alt+C et Alt+V se reconnaissent à la touche physique, donc Option+C fonctionne malgré le caractère « ç » produit.
- Trousseau : Keychain (`keyring`, `apple-native`). Agent SSH : `SSH_AUTH_SOCK`.
- Construction : il faut un Mac ou la CI GitHub (`macos-latest`). Le SDK de Xcode n'est licencié que sur matériel Apple : pas de compilation croisée depuis Linux.
- Signature et notarisation : un certificat Developer ID est nécessaire pour distribuer hors du Mac App Store sans alerte Gatekeeper. Sans lui, l'application s'ouvre par clic droit puis « Ouvrir ».

## Android : session en arrière-plan (2026-11-09)

- Service au premier plan `SessionService.kt`, type `specialUse`, démarré par le pont JavaScript `McAndroid` tant que `app.phase` vaut `connecte`. La notification est privée sur l'écran verrouillé (« Session SSH ouverte ») et porte l'action « Se déconnecter ».
- Fermer l'application depuis les tâches récentes termine le processus : aucune session ni aucun secret ne reste en mémoire sans écran pour les fermer.
- Pas de verrou de réveil : la veille profonde peut couper une session longue, écran éteint. Choix assumé pour la batterie ; à revoir si l'usage le demande.

## Android : secrets et fichiers de l'appareil (2026-11-09)

- Secrets des profils : `CoffreAndroid.kt`, AES-256-GCM avec une clé du Keystore Android non exportable, identifiant du profil en donnée authentifiée, stockage dans des préférences privées. Sauvegarde Android désactivée (`allowBackup=false`) : une sauvegarde restaurée sur un autre appareil serait de toute façon indéchiffrable.
- Le secret transite par une chaîne Java le temps de l'appel JNI : elle ne peut pas être effacée explicitement. La clé SSH, elle, reste côté Rust.
- Documents de l'appareil : sélecteur système du plugin dialog (URI `content://`), lus et écrits par `FichiersAndroid.kt`. Pas de permission de stockage demandée. Le nom fourni par le document est assaini (`appareil::nom_sur`), l'import n'écrase rien, et les échanges restent limités au dossier `fichiers` de l'application.
- Import de clé : copie dans le cache privé le temps de la validation, puis effacement dans tous les cas.
- Ces fichiers Kotlin et le manifeste sont dans `gen/android` : à conserver si ce dossier est régénéré.
