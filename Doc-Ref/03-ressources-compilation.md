# Règles obligatoires avant toute opération lourde

Machine de développement partagée avec d'autres travaux : i5-7500T, 4 cœurs, 15,9 Go de RAM.

Sont concernés : `cargo build/check/test/run`, `tauri build`, `npm install` important, Gradle,
téléchargements de plusieurs Go (NDK).

1. Lancer `scripts/garde-ressources.ps1`. Si le verdict est REFUS : ne rien lancer.
2. Conditions : aucune autre compilation en cours, au moins 4 Go de RAM libre, 20 Go de disque, CPU à 60 % maximum.
3. Exécuter via `-Commande` : priorité basse et un seul job (`CARGO_BUILD_JOBS=1`).
4. Ne jamais arrêter ni ralentir un processus qui n'appartient pas à ce projet.
5. Compilation complète multi-cibles et builds Android/macOS/Linux : en CI de préférence.
6. `NDK_HOME` est défini par le garde-fou pour la commande lancée seulement. Ne pas le définir dans les variables Windows.
7. Noter dans `journal-dev/` chaque opération lourde et son résultat.

`aws-lc-sys` compile une grande quantité de C : c'est l'étape la plus coûteuse du cœur SSH.
