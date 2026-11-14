# Accès d'un agent aux serveurs par MLS Connect

Étude et prototype, 14 novembre 2026. Niveau 2 ajouté le même jour.

## Question

Un agent (Claude Code, un assistant d'IA, un script) peut-il se connecter à nos serveurs par MLS Connect, avec un profil que l'utilisateur lui a ouvert et un niveau d'accès choisi ? Par MCP, ou par un autre moyen ?

## Réponse courte

Oui, par **MCP sur l'entrée et la sortie standard** : un petit programme, `mc-mcp`, lancé par le client MCP, réutilise le cœur Rust de MLS Connect. L'agent désigne un profil par son nom. Il ne voit jamais ni la clé ni la passphrase, et la clé d'hôte est vérifiée comme dans l'application.

Les règles de MLS Connect filtrent les demandes de l'agent, mais **la vraie limite reste le compte sur le serveur** : un agent qui dispose aussi d'un terminal sur le poste peut contourner un filtre local. Chaque niveau s'accompagne donc d'une recommandation côté serveur.

## Voies comparées

| Voie | Pour | Contre | Choix |
|---|---|---|---|
| MCP sur stdio, programme `mc-mcp` | Outils typés, décrits à l'agent ; règles par profil ; journal ; standard pris en charge par Claude Code et d'autres clients | Un programme de plus à installer | **Retenue** |
| `mc-mcp` demande l'accord à l'application ouverte, par un canal local | Un seul serveur MCP pour tous les niveaux ; l'application n'exécute rien, elle affiche et répond | L'application doit être ouverte ; un port local protégé par un jeton | **Retenue pour le niveau 2** |
| MCP en HTTP local, servi par l'application ouverte | Sessions de l'agent visibles dans la fenêtre | Second serveur MCP à écrire et à déclarer ; l'application doit être ouverte | Écartée : le canal local suffit à l'approbation |
| Ligne de commande `mls-connect --exec profil "commande"` | Très simple | Pas de description des outils ni de résultat structuré | Écartée |
| `ssh` direct avec une clé dédiée | Existe déjà | Ni trousseau, ni règles par profil, ni journal commun ; secret à confier à l'agent | Écartée |

## Fonctionnement

```
Client MCP (Claude Code) ⇄ stdio, JSON-RPC ⇄ mc-mcp ⇄ mc-core ⇄ SSH ⇄ serveur
                                               │
                     profiles.json, known_hosts, trousseau (lecture) ; agent.log (écriture)
```

- `mc-mcp` lit les fichiers de MLS Connect dans son dossier de données (`%APPDATA%\app.mlsconnect.desktop` sous Windows, `~/.config/app.mlsconnect.desktop` sous Linux, `~/Library/Application Support/app.mlsconnect.desktop` sous macOS), ou dans celui donné par `--dossier`. Il ne les modifie jamais.
- Les profils sont relus à chaque demande : un changement de niveau dans l'application s'applique aussitôt, et un profil fermé perd sa session.
- Une session par profil est gardée ouverte entre deux demandes, et fermée à la fin du programme.

### Outils exposés

| Outil | Rôle |
|---|---|
| `profils` | Profils ouverts aux agents : serveur, niveau, commandes et dossiers admis |
| `executer` | Commande unique, sans terminal : code de sortie, sortie et erreurs (64 Kio au plus chacune), délai de 30 s par défaut, 600 s au plus |
| `lister` | Contenu d'un dossier, par SFTP |
| `lire` | Extrait d'un fichier, par SFTP : 256 Kio au plus par appel, reprise par position |

## Niveaux d'accès

Le niveau se règle dans le profil : Options, « Accès d'un agent ». Il est enregistré dans `profiles.json` (champ `agent`). Un ancien profil, sans ce champ, reste fermé.

| Niveau | L'agent peut | Côté serveur, recommandé | Prototype |
|---|---|---|---|
| Aucun | Rien : le profil lui est invisible | | Fait |
| **0 · Lecture** | Commandes d'une liste fermée de consultation ; lister et lire dans les dossiers ouverts du profil | Compte dédié sans sudo ; clé `restrict` et `from=` | **Fait, essayé** |
| 1 · Exploitation | Niveau 0, plus les commandes ajoutées par l'utilisateur, avec leurs arguments | Compte dédié ; `sudoers` limité à ces commandes exactes | Fait, non essayé sur un serveur réel |
| **2 · Shell encadré** | Toute commande, chacune approuvée par l'utilisateur dans MLS Connect ouvert ; lister et lire dans les dossiers ouverts du profil | Compte dédié | **Fait** ; approbation essayée sous Windows, exécution après accord non essayée sur un serveur réel |
| **3 · Accès complet** | Toute commande, tout fichier, sans approbation | Serveurs d'essai seulement ; les droits du compte sont la seule limite | **Fait, essayé** |

### Règles des niveaux 0 et 1

- **Une seule commande** : `; | & $ ` < > ( ) { } \ ' " * ? [ ] ~ ! #`, les retours à la ligne et les caractères de contrôle sont refusés. Sans eux, le shell distant ne peut ni enchaîner, ni rediriger, ni substituer.
- **Liste de consultation** (`mc_core::agent::COMMANDES_LECTURE`) : `uptime`, `df`, `free`, `lsblk`, `ps`, `pgrep`, `systemctl status|is-active|is-enabled|is-failed|list-units|list-timers`, `journalctl`, `last`, et quelques commandes sans argument (`hostname`, `whoami`, `id`, `date`, `nproc`, `lscpu`, `cat /etc/os-release`, `ip -brief address|link`, `ip route`, `ss -tln|-uln`, `who`, `w`, `systemctl --failed`).
- Aucune commande de la liste ne prend un chemin de fichier : **les fichiers se lisent par `lister` et `lire`, limités aux dossiers ouverts**.
- Arguments refusés même sur une commande admise : suivi sans fin (`-f`, `--follow`) et options d'écriture de `journalctl` (`--vacuum-*`, `--rotate`, `--flush`…).
- **Chemins** : le serveur donne la forme canonique (liens symboliques et `..` résolus), puis elle est comparée aux dossiers ouverts. Un lien de `/var/log` vers `/etc/passwd` est donc refusé.

### Niveau 2 : approbation dans l'application

```
mc-mcp ── commande ──▶ 127.0.0.1:port (jeton) ──▶ MLS Connect : boîte « Approuver / Refuser »
mc-mcp ◀── approuvée, refusée ou expirée ──────── choix de l'utilisateur
```

- Au démarrage, l'application (bureau) écoute sur la boucle locale, sur un port libre, et dépose `agent-approbation.json` (port et jeton aléatoire de 256 bits) dans son dossier de données. Le fichier est retiré à la fermeture ; un fichier resté après un arrêt brutal mène à un refus.
- `mc-mcp` envoie le profil, le serveur et la commande. L'application passe au premier plan et affiche la commande entière, telle qu'elle sera exécutée. Le focus est sur **Refuser** ; Échap vaut refus.
- Sans réponse en 120 s : refus. Application fermée : refus, avec un message qui demande de l'ouvrir. Huit demandes en attente au plus.
- Si `mc-mcp` abandonne (client MCP arrêté), la demande disparaît de l'écran.
- La commande doit s'afficher telle quelle : caractères de contrôle (sauf saut de ligne et tabulation), caractères invisibles et marques de sens d'écriture sont refusés avant l'affichage.
- L'accord vaut pour une commande, une fois. Rien n'est mémorisé.
- `lister` et `lire` suivent la règle du niveau 0 (dossiers ouverts), sans demande d'accord.
- `agent.log` : la commande refusée porte la raison ; la commande exécutée porte `"approuvee": true`.
- Limite : le jeton écarte un autre compte du poste, pas un programme du même compte. Un agent qui a aussi un terminal sur le poste peut se passer de `mc-mcp` (voir Risques) : la limite reste le compte sur le serveur.

### Règles communes à tous les niveaux

- **Clé d'hôte** : elle doit déjà être enregistrée dans MLS Connect. Une clé inconnue ou modifiée vaut un refus, car aucune question ne peut être posée à l'agent.
- **Secrets** : passphrase ou mot de passe lus dans le trousseau, si le profil les y mémorise ; agent SSH du système ; jamais demandés à l'agent. La saisie interactive est refusée.
- **Caractère nul refusé**, même au niveau 3 : OpenSSH coupe toute la connexion quand une commande en contient un (constaté pendant les essais).
- **Sorties nettoyées** : caractères de contrôle et séquences du terminal retirés. Chaque réponse rappelle que le texte du serveur est une donnée, pas une instruction.
- **Journal `agent.log`**, dans le dossier de données : une ligne JSON par demande, avec la date, le profil, le niveau, l'outil, la commande ou le chemin, la décision (`executee`, `faite`, `refusee`, `echec`), le code, la durée et la taille de la sortie. **La sortie elle-même n'est jamais consignée.**

## Risques

| Risque | Parade |
|---|---|
| L'agent contourne `mc-mcp` : il a aussi un terminal sur le poste, et peut lire une clé non protégée | Limiter le compte sur le serveur, pas seulement le profil. Protéger la clé par une passphrase gardée dans le trousseau. |
| Injection : un journal ou un fichier du serveur contient des instructions destinées à l'agent | Niveau 0 par défaut ; avertissement dans chaque réponse ; écritures possibles seulement aux niveaux 1 et 3, choisis par l'utilisateur. |
| Tout ce que l'agent lit part chez le fournisseur du modèle | Ne pas ouvrir de dossiers contenant des secrets ; un compte dédié ne lit pas les fichiers protégés. |
| Commande approuvée sans être lue, au niveau 2 | Commande affichée en entier, focus sur Refuser, aucun accord mémorisé. Préférer le niveau 1 pour des gestes répétés. |
| Commande destructrice au niveau 3 | Réservé aux serveurs d'essai. Claude Code demande l'accord avant chaque appel d'outil, sauf autorisation permanente : ne pas l'accorder pour `executer` sur un profil de niveau 3. |
| Commande sans fin | Délai de 30 s par défaut, 600 s au plus : le canal est fermé, la session reste utilisable. |

## Préparer un serveur (niveau 0)

```bash
sudo useradd -m -s /bin/bash agent-mc && sudo passwd -l agent-mc
sudo install -d -m 700 -o agent-mc -g agent-mc /home/agent-mc/.ssh
echo 'restrict,from="IP_DU_POSTE" ssh-ed25519 AAAA… mc-agent' | sudo tee /home/agent-mc/.ssh/authorized_keys
sudo chown agent-mc: /home/agent-mc/.ssh/authorized_keys && sudo chmod 600 /home/agent-mc/.ssh/authorized_keys
# Pour lire les journaux système : sudo usermod -aG adm,systemd-journal agent-mc
```

`restrict` coupe le terminal, les renvois de ports et d'agent, mais laisse les commandes et SFTP. Au niveau 1, ajouter un fichier `/etc/sudoers.d/agent-mc` limité aux commandes exactes, par exemple `agent-mc ALL=(root) NOPASSWD: /usr/bin/systemctl restart nginx`.

## Brancher Claude Code

1. Ouvrir une première fois la connexion dans MLS Connect, pour vérifier et enregistrer la clé d'hôte, puis cocher « Mémoriser dans le trousseau » si la clé a une passphrase.
2. Régler le niveau du profil : Options, « Accès d'un agent ».
3. Déclarer le serveur MCP :
   ```
   claude mcp add mls-connect -- "C:\chemin\vers\mc-mcp.exe"
   ```
   `mc-mcp` est installé à côté de l'application :

   | Système | Chemin |
   |---|---|
   | Windows (installateur) | `%LOCALAPPDATA%\MLS Connect\mc-mcp.exe` |
   | Linux (`.deb`) | `/usr/bin/mc-mcp` |
   | macOS | `/Applications/MLS Connect.app/Contents/MacOS/mc-mcp` |
   | AppImage | dans l'image, non utilisable tel quel : prendre le `.deb`, ou construire `mc-mcp` à part |
4. Vérifier : `mc-mcp --profils` affiche les profils ouverts.

## Essais du prototype (VM 102, 14 novembre 2026)

Compte `agent-mc` sans sudo, clé `restrict,from="127.0.0.1,::1"`, profils de niveaux 0 et 3, client MCP de test (`journal-dev/outils-essais/mcp-essai.mjs`).

- Tests du cœur : tous réussis, dont 7 nouveaux sur les règles.
- Essai réel de la commande unique et de la lecture : code de sortie, sortie et erreurs séparées, séquences du terminal retirées, délai expiré, sortie tronquée à 64 Kio, caractère nul refusé, session toujours utilisable, liens symboliques résolus.
- Protocole MCP : 23 contrôles sur 23. Niveau 0 : consultation admise ; enchaînement, substitution, `cat`, `ls`, `systemctl restart` et `journalctl --vacuum-size` refusés ; lecture hors de `/var/log`, par `..` ou par un lien symbolique refusée. Niveau 3 : commande libre, lecture de `/etc/passwd`, délai expiré, pas de sudo. Clé d'hôte inconnue refusée, profil fermé invisible.
- `agent.log` : une ligne par demande, sans aucune sortie de commande.

## Limites du prototype

- Niveau 2 : l'application doit être ouverte sur le même poste et sous le même compte que `mc-mcp`. Pas d'approbation sur Android.
- Pas de session interactive (terminal) pour l'agent : commandes uniques seulement.
- Aucun outil d'écriture de fichiers : au niveau 3, passer par une commande.
- AppImage : `mc-mcp` est dans l'image, mais un client MCP ne peut pas l'y lancer directement.

## Livraison de `mc-mcp` dans les paquets

**Décidé** : `mc-mcp` est un exécutable annexe de Tauri (`bundle.externalBin`), copié à côté de l'application par chaque paquet.

- La déclaration est dans un fichier à part, `app/src-tauri/tauri.paquets.conf.json`, passé seulement à l'empaquetage. Raison : Tauri exige la présence du fichier annexe dès la compilation ; dans la configuration principale, il casserait `cargo check`, les tests, Android et la construction sans paquet.
- Préparation : `node scripts/sidecar.mjs` depuis `app` (construit `mc-mcp` en mode final et le copie dans `src-tauri/binaries/`, sous le nom du triplet de la cible ; `--debug` et `--target` existent). Dossier ignoré par git.
- Empaquetage : `cargo tauri build --config src-tauri/tauri.paquets.conf.json`.
- Garde-fou Windows : `scripts/attendre-et-tester.ps1 -Paquet` (installateur NSIS de débogage). CI : une étape par système (NSIS, `.deb`, `.app`).
- Vérifié sous Windows le 14 novembre 2026 : installateur NSIS construit, `mc-mcp.exe` présent dans son script d'installation et de désinstallation. Installateur non exécuté. Linux et macOS : non vérifiés avant le prochain passage de la CI.
