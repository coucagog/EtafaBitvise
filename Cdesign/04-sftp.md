# Prompt 04 — Transferts SFTP

Conçois une vue à deux panneaux, Fichiers locaux et Fichiers distants, séparés par une barre déplaçable.

Vues : Parcourir, File d'envoi, File de téléchargement, Journal, avec compteur sur chaque file.

Chaque panneau : filtre, chemin éditable, précédent, suivant, dossier parent, actualiser, dossier personnel, signets.
Colonnes : Nom, Taille, Type, Modifié, Permissions (côté distant).
Barre basse : Envoyer ou Télécharger, Démarrage auto, Binaire ou Texte, règle si le fichier existe, Pause.

Interactions à montrer : glisser-déposer, sélection multiple, menu contextuel (renommer, supprimer, permissions, nouveau dossier).

Files : fichier, progression, débit, temps restant, état ; démarrer, pause, retirer, réordonner, suivre le transfert actif.
Une erreur de transfert doit être visible sans bloquer la file.

Dialogues : fichier existant (écraser, reprendre, ignorer, renommer, appliquer à tous), permissions, et miroir d'un dossier avec aperçu des changements.
Pour le miroir, l'option « Supprimer les fichiers absents de la source » doit être signalée comme destructive.
