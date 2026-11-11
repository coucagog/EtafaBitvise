# Barre des tâches et icônes cachées — comportement repris de YawREC

Sources : `yawrec/src-tauri/src/lib.rs` (`setup_tray`, `show_main_window`, `.run`),
`yawrec/src/main.js`, `yawrec/src-tauri/tauri.conf.json`.

## Comportement de YawREC, à reproduire

| Élément | Comportement |
|---|---|
| Barre des tâches | L'application y apparaît (`skipTaskbar: false`) |
| Icônes cachées | Une icône, celle de l'application, infobulle = nom de l'application |
| Clic gauche sur l'icône | Ramène la fenêtre au premier plan (`show`, `unminimize`, `set_focus`) ; pas de menu |
| Clic droit sur l'icône | Menu : Afficher, action principale avec raccourci, séparateur, Quitter |
| Bouton réduire | Réduit dans la barre des tâches (pas dans les icônes cachées) |
| Bouton fermer | Ferme réellement l'application, sans masquage dans les icônes cachées |
| Sortie pendant une activité | Sortie retenue le temps de terminer proprement, puis `app.exit(0)` |
| Seconde ouverture | Instance unique, ramène la fenêtre existante |
| Raccourcis globaux | Action principale et action secondaire |
| Fenêtre | Sans décoration système, barre de titre personnalisée et zone de déplacement |
| Échec de l'icône | Journalisé, l'application continue |

## Adaptation à l'application SSH

| YawREC | Application SSH |
|---|---|
| « Afficher YawREC » | « Afficher {NomApp} » |
| « Démarrer / Arrêter (Ctrl+Shift+F9) » | « Se connecter / Se déconnecter » (dernier profil), avec raccourci |
| « Quitter YawREC » | « Quitter {NomApp} » |
| Finaliser le MP4 avant sortie | Si sessions ou transferts actifs : demander confirmation, fermer proprement, puis quitter |
| Pause `Ctrl+Shift+F10` | Pause / reprise des transferts |

**Différences assumées** : fenêtre redimensionnable et sans `alwaysOnTop`, car l'application affiche terminal et fichiers
dans une grande fenêtre.

## Raccourcis (décidé le 2026-11-08)

Source unique : `app/src/lib/raccourcis.json`. Les tests de `crates/mc-core/src/raccourcis.rs` vérifient l'absence de conflit.

| Touches | Portée | Action |
|---|---|---|
| `Ctrl+Shift+F7` | Globale | Se connecter / Se déconnecter |
| `Ctrl+Shift+F8` | Globale | Pause / reprise des transferts |
| `Ctrl+Shift+P` | Terminal, dans la fenêtre | Se reconnecter après une déconnexion |
| `Alt+C` | Terminal, dans la fenêtre | Copier |
| `Alt+V` | Terminal, dans la fenêtre | Coller |

**Règles de non-interférence avec YawREC** :

1. YawREC détient `Ctrl+Shift+F9` et `Ctrl+Shift+F10` en global : MLS Connect ne les utilise jamais.
2. Seuls F7 et F8 sont déclarés au système. `Ctrl+Shift+P` reste interne à la fenêtre : déclaré en global, il volerait la palette
   de VS Code, raison pour laquelle YawREC l'a abandonné.
3. Identifiant `app.mlsconnect.desktop` (YawREC : `app.yawrec.desktop`) : l'instance unique et les données restent séparées.
4. Icône de barre `mc-barre`, journal `%TEMP%\mlsconnect.log`, port de développement 5183 (YawREC : 5173).
5. Un raccourci déjà pris par une autre application est journalisé, sans bloquer le démarrage.

`Alt+C` et `Alt+V` ne parviennent plus au programme distant (dans bash, `Alt+C` met un mot en majuscule initiale) : conséquence acceptée.

Le menu de YawREC est statique et son icône ne change pas d'état. Ajouter un état « connecté » serait une extension, à valider.

## Par plateforme

| Plateforme | Équivalent |
|---|---|
| Windows | Identique à YawREC |
| macOS | Icône dans la barre de menus |
| Linux | Icône et menu au clic droit. **Le clic gauche n'émet pas d'événement** (doc Tauri) : « Afficher » passe par le menu |
| Android | Pas de zone équivalente : notification du service au premier plan avec actions Afficher et Déconnecter |
