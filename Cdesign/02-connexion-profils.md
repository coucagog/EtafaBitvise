# Prompt 02 — Connexion et profils

Conçois l'écran affiché avant connexion, avec une liste de profils à gauche et le détail du profil sélectionné.

Actions de profil : nouveau, enregistrer, enregistrer sous, importer, exporter, supprimer.

Sections du détail : Connexion, Authentification, Options, Terminal, Transferts, Tunnels, Algorithmes SSH, Notes.

Connexion : hôte, port, utilisateur, proxy, rebond par serveur intermédiaire.
Authentification : méthode (mot de passe, clé, keyboard-interactive, agent), choix de clé, passphrase.
Options : ouvrir terminal et SFTP à la connexion, reconnexion automatique, délai, keep-alive.
Bouton principal : Se connecter. Sous le formulaire : journal horodaté repliable.

Conçois aussi :
1. Le gestionnaire de clés client : liste, génération, import, export de la clé publique.
2. Le gestionnaire de clés d'hôte : liste, empreinte, suppression.
3. La vérification de clé d'hôte inconnue : empreinte SHA-256 bien lisible, Accepter une fois, Enregistrer et accepter, Annuler.
4. Le cas grave où la clé a changé : alerte rouge, comparaison ancienne et nouvelle empreinte, pas d'acceptation par défaut.
5. L'état de connexion en cours, avec les étapes et l'erreur d'authentification.
