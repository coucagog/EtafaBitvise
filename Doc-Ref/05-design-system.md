# Design system — dérivé de YawREC

Source : `C:\Users\hp\Desktop\My_Projects\YawREC\yawrec\src\style.css`.

## Repris tel quel

```css
--rec-500:#ef4444; --rec-600:#dc2626; --rec-700:#b91c1c; --rec-glow:rgba(239,68,68,.35);
--ok-500:#4ade80; --accent-blue:#60a5fa; --close-red:#c42b1c;
--text-primary:#f0f0f0; --text-secondary:#d8d8d8; --text-muted:#999; --text-faint:#888;
--font-sans:"Segoe UI Variable","Segoe UI",system-ui,sans-serif;
--font-mono:ui-monospace,"Cascadia Mono","Consolas",monospace;
```

- Fenêtre : dégradé `rgba(32,32,32,.92)` vers `rgba(24,24,24,.92)`, flou 40 px, saturation 180 %, rayon 8 px, bordure blanche à 8 %.
- Barre de titre 32 px, boutons système 46 × 32 px, survol de fermeture `--close-red`.
- Pied de page 28 px, fond noir à 30 %, texte 11 px.
- Contrôle segmenté, `.kbd`, séparateurs fins à 4-6 % de blanc, transitions de 120 à 240 ms.
- Popovers : fond `rgba(14,14,16,.97)`, en-têtes 11 px en capitales.

## Signification des couleurs

Dans YawREC, le rouge signifie « enregistrement ». Ici :

| Couleur | Usage |
|---|---|
| `--accent-blue` | Action principale, sélection, onglet actif |
| `--ok-500` | Connecté, transfert terminé, tunnel actif |
| `#fbbf24` (jaune du VU-mètre YawREC) | Reconnexion, avertissement, clé d'hôte inconnue |
| `--rec-500` | Erreur, déconnexion, suppression |

Le liseré rouge `.recording` devient un liseré vert discret quand une session est active.

## À créer (absent de YawREC)

YawREC est une fenêtre fixe de 800 × 240 ; il manque : onglets, formulaires complets, tableaux, panneaux redimensionnables,
files de transfert, journal, boîtes de dialogue, version mobile, thème clair éventuel, polices de terminal.

## Portabilité

La transparence et `backdrop-filter` dépendent du système : prévoir un fond opaque de repli sous Linux et Android.
« Segoe UI » n'existe que sous Windows : la pile se replie sur `system-ui`.

## Maquettes

Confiées à Claude Design, intégration par Claude Code : voir `Cdesign/README.md`.
Exports reçus le 2026-11-08 dans `Cdesign/exports/` (7 archives). Audit : `rapport-conformite-design.html`.

Les maquettes sont la référence visuelle. Leur `support.js` et leurs styles en ligne ne vont pas dans l'application.
Les tokens de la planche 01 sont la référence, complétés par ceux de la maquette Android.

## Logo retenu

Piste C, Crochets (choix de l'utilisateur, 2026-11-08). Sources dans `Cdesign/retenu/` :

- `icone-source-1024.svg` : 32 px et plus
- `icone-source-petit.svg` : 24 px et moins (tuile 992, trait 120, point agrandi, sans halo)

## Corrections obligatoires à l'intégration

1. Aucune ressource externe : icônes Lucide (MIT) en local, plus de `unpkg.com` ni de Google Fonts, pour respecter la CSP stricte.
2. JetBrains Mono et IBM Plex Mono : les embarquer en local ou retirer ces deux choix du terminal.
3. Jamais de texte sous 11 px : les six `.kbd` de l'écran Connexion passent de 10 à 11 px.
4. Clé d'hôte inconnue : focus initial sur Annuler, pour qu'Entrée ne fasse pas confiance à une clé non lue.
5. Android : 48 px minimum pour toute cible (`--touch-min`). Cette valeur remplace `--control-h-touch` (44 px). 14 boutons sont à corriger.
6. Android : marges réelles avec `env(safe-area-inset-*)`. Les barres système dessinées dans la maquette ne sont pas à reproduire.
7. Sur surface surélevée ou champ : `--text-muted` plutôt que `--text-faint`, et `--danger-text` plutôt que `--rec-500` pour le texte.
8. Respecter `prefers-reduced-motion`.
9. La liste des algorithmes affichée doit venir de ce que `russh` prend réellement en charge.
