# Faisabilité VNC dans MLS Connect

Note du 2026-11-09. Étude sur papier : aucun prototype n'a été construit ni essayé. Les
caractéristiques de noVNC ci-dessous viennent de mes connaissances et sont à confirmer au prototype.

## Conclusion

Faisable sur les quatre systèmes, y compris Android, en affichant le bureau dans la WebView avec
noVNC et en faisant passer le protocole par un canal SSH de la session déjà ouverte. Aucun port local
n'est nécessaire. Sujet à traiter comme une fonction à part entière, après les essais tablette.

## Pourquoi cette voie

| Voie | Windows, Linux, macOS | Android | Limite |
|---|---|---|---|
| noVNC dans la WebView, canal SSH direct | Oui | Oui | Performances d'un rendu JavaScript |
| Client RFB écrit en Rust, rendu dans un canvas | Oui | Oui | Protocole, encodages et clavier à écrire et maintenir |
| Lancer un client VNC installé, à travers un tunnel local | Oui, s'il est présent | Peu praticable | Dépend d'un logiciel tiers |

Le tunnel local existant permet déjà d'utiliser un client VNC externe : ce besoin est couvert sans
développement.

## Principe retenu

1. Rust ouvre un canal `direct-tcpip` vers `127.0.0.1:5900` (hôte et port réglables) côté serveur.
2. Les octets passent entre ce canal et la WebView par un `Channel` Tauri, déjà employé pour le terminal.
3. noVNC reçoit un objet au comportement de WebSocket au lieu d'une URL, ce qui évite d'ouvrir un
   port local et de modifier la CSP stricte.

## Sécurité

- Le serveur VNC doit écouter sur sa boucle locale : seul le canal SSH le rend joignable.
- L'authentification VNC classique est faible (DES, 8 caractères). Elle ne doit servir que dans le canal SSH.
- Le mot de passe VNC reste en mémoire le temps de la saisie ; il suit les règles du trousseau s'il est mémorisé.
- noVNC est sous licence MPL-2.0 : à livrer dans l'application, sans chargement depuis un CDN.

## À vérifier au prototype

- Fluidité sur la tablette RGPH-5 (800×1280, WebView système), le point le plus incertain.
- Débit du passage par `Channel` à une résolution de bureau courante.
- Clavier tactile : touches Ctrl, Alt, Échap ; réutilisation de la barre du terminal Android.
- Pas d'interférence avec les raccourcis globaux et ceux de YawREC.
