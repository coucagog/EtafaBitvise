# Prompt 00 — Brief commun

Tu conçois l'interface d'un client SSH de bureau et mobile (Windows, Linux, macOS, Android), pour administrateurs système.
Il regroupe connexion par profils, terminal, transfert de fichiers SFTP, et tunnels de ports.

Nom provisoire : {NomApp}. N'utilise aucun logo, icône ni texte d'un produit existant.

Pars du design system joint, issu de notre application YawREC : thème sombre façon verre dépoli Windows 11, tokens CSS de `style.css`,
fenêtre sans décoration avec barre de titre personnalisée de 32 px, pied de page de 28 px.

Signification stricte des couleurs :
- bleu `#60a5fa` : action principale, sélection, onglet actif
- vert `#4ade80` : connecté, succès, tunnel actif
- jaune `#fbbf24` : reconnexion, avertissement, clé d'hôte inconnue
- rouge `#ef4444` : erreur, déconnexion, suppression

Contraintes :
- Interface dense et professionnelle, lisible en usage prolongé ; texte courant en 12-13 px, jamais sous 11 px.
- Contraste AA au minimum. Navigation clavier et états de focus visibles.
- Chaque écran montre ses états : vide, chargement, erreur, actif.
- Fenêtre redimensionnable, largeur minimale 960 px, largeur de référence 1280 px.
- Prévois un fond opaque de repli quand le flou n'est pas disponible.
- Textes de l'interface en français.
- Rendu en HTML/CSS réutilisant les noms de variables CSS fournis. Pas de dégradés décoratifs supplémentaires, pas d'illustrations.

Réponds d'abord par une courte reformulation et tes questions éventuelles, sans générer d'écran.
