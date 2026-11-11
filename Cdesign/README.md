# Cdesign — prompts pour Claude Design

## Recommandation

**Oui pour les maquettes, non pour le code de production.**

- YawREC fournit de bons fondamentaux, mais seulement pour une fenêtre fixe de 800 × 240. Il faut inventer les écrans
  denses (terminal, SFTP à deux panneaux, files, tunnels) et une version mobile : c'est là que Claude Design est utile.
- L'intégration reste à faire dans Vite par Claude Code, car elle dépend des états réels du cœur Rust.

Claude Design autonome (claude.ai/design) ferme au plus tard le 14 décembre 2026 et ses fonctions passent dans Claude.
Si l'adresse n'est plus disponible, utilisez ces prompts dans Claude directement.

## Utilisation

1. Joindre `yawrec/src/style.css`, `yawrec/docs/icon-source.svg` et `Doc-Ref/05-design-system.md`.
2. Envoyer `00-brief-commun.md`, puis les prompts dans l'ordre, un par échange.
3. Déposer les exports dans `Cdesign/exports/`.

| Ordre | Prompt | Résultat attendu |
|---|---|---|
| 0 | [00-brief-commun.md](00-brief-commun.md) | Cadre commun |
| 1 | [01-design-system.md](01-design-system.md) | Composants étendus |
| 2 | [02-connexion-profils.md](02-connexion-profils.md) | Avant connexion |
| 3 | [03-session-terminal.md](03-session-terminal.md) | Après connexion, terminal |
| 4 | [04-sftp.md](04-sftp.md) | Transferts |
| 5 | [05-tunnels-journal.md](05-tunnels-journal.md) | Tunnels et journal |
| 6 | [06-mobile.md](06-mobile.md) | Android |
| 7 | [07-icone-barre-des-taches.md](07-icone-barre-des-taches.md) | Icône et menu |
