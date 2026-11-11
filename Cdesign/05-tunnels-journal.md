# Prompt 05 — Tunnels et journal

Conçois la gestion des tunnels :

- Client vers serveur : interface et port d'écoute, destination
- Serveur vers client : interface et port d'écoute distants, destination locale
- Proxy dynamique SOCKS5 et HTTP CONNECT : interface, port
- Pont FTP vers SFTP : port, mode passif

Liste commune : type, écoute, destination, état, connexions actives, débit, interrupteur, modifier, supprimer.
Les règles sont modifiables pendant la session. Montre : actif, port déjà utilisé, refusé par le serveur.
Avertis si l'écoute est ouverte sur toutes les interfaces.

Conçois aussi le journal de session : horodatage, niveau, source (connexion, terminal, SFTP, tunnel), filtre, recherche, copier, exporter. Il ne doit pas afficher de secrets.
