# Analyse de l'audit externe RusteeGear — 5 septembre 2026

Ce document est une contre-lecture de l'[Audit RusteeGear après mise à jour — 5 septembre 2026](AUDIT_JEU_2026-07-17.md) (auteur externe, non listé dans ce dépôt sous ce nom — voir le texte source fourni en amont). Objectif : dire ce qui est bien établi, ce qui doit être challengé, et quoi vérifier avant d'ouvrir des tests accompagnés.

## Verdict sur l'audit lui-même

L'audit est solide et bien calibré. La conclusion (« base crédible pour préversion accompagnée, pas encore outil commercial autonome ») colle aux preuves citées : 926 tests + CI verte sont réels, mais systématiquement encadrés par des limites explicites (pas de test manuel, pas de charge hostile, pas de reproduction des risques statiques). C'est le bon niveau de prudence pour ce type de document.

## Points confirmés comme bien fondés

- **Risque A (autosave cross-projet)** — le plus grave et le mieux justifié. Chemin de code précis (`src/app/autosave.rs:101,124`, `src/app/persistence.rs:194`, `src/lib.rs:1494`), scénario A/B concret et vérifiable par un test, pas une simple hypothèse. Priorité haute justifiée.
- **Point D (Android, secret de signature manquant)** — fait vérifiable en quelques minutes : `packaging/build_apk.sh` exige `RUSTEEGEAR_KEYSTORE_PASS`, absent de `.github/workflows/release.yml:63`. Le job CI vert ne prouve que la compilation de la bibliothèque, pas la production d'un APK signé. C'est un blocage binaire, pas un risque probabiliste.
- **Distinction vérifié / supposé** — globalement bien tenue tout au long du document ; les risques B, C, E, F, G, H sont présentés comme des lectures de code, pas des incidents reproduits, ce qui est honnête.

## Points à challenger

1. **Priorisation A vs D dans la roadmap** — l'audit met A en « priorité haute » et D en « blocage de release », mais la roadmap proposée (section « Direction proposée ») place la distribution (étape 4) après protection du travail, portabilité du projet et une expérience complète (étapes 1–3). Si un objectif court terme inclut une release Android testable, D doit être traité en parallèle de A, pas après — c'est un échec garanti à la compilation de la release, pas un risque probabiliste comme A.

2. **Risque E sous-priorisé** — classé « risque nouveau » mais pas mis au même niveau que D dans le texte, alors qu'il touche à une fuite potentielle de secret (mot de passe de signature laissé dans `Cargo.toml` après une annulation d'export forcée). Même si non reproduit, un risque de fuite de secret mérite d'être traité avec la même urgence que D, pas seulement listé en dessous.

3. **État du protocole réseau sur le VPS public — angle mort à combler avant tout test externe.** L'audit signale explicitement : « le protocole réseau est passé de 7 à 8 [...] l'état réel du VPS n'a pas été contrôlé pendant cet audit ». Concrètement, cela veut dire qu'on ne sait pas si le serveur public actuel accepte les clients en v8. Si ce n'est pas vérifié avant d'inviter des testeurs accompagnés, le risque concret est que la première chose que ces testeurs rencontrent soit un refus de connexion silencieux ou un mismatch de protocole — pas le contenu du jeu qu'on veut faire tester.

## Actions concrètes avant d'ouvrir des tests accompagnés

Dans l'ordre de rentabilité (rapide + bloquant) :

1. **Vérifier la version de protocole exposée par le VPS public** et, si elle est en v7, planifier la mise à jour ou geler l'invitation de testeurs tant que ce n'est pas fait.
2. **Écrire le test croisé A/B pour l'autosave** décrit dans l'audit (autosave la plus récente appartenant au projet A, projet B rouvert au démarrage avec une sauvegarde plus ancienne, restauration puis sauvegarde ne doit jamais écrire dans B le contenu de A).
3. **Fournir le secret `RUSTEEGEAR_KEYSTORE_PASS`** au workflow de release et documenter la gestion de la clé de signature entre versions (ne pas régénérer une identité à chaque release).
4. **Traiter le risque E en même temps que D** : ne pas modifier `Cargo.toml` en place pendant le build Android, ou a minima garantir la restauration via un `trap` déclenché depuis le processus parent plutôt qu'un `kill` immédiat.

## Ce qui n'est pas remis en cause

Les recommandations structurelles de l'audit (protéger le travail, rendre le projet portable, terminer une expérience unique, valider distribution/réseau, tester l'intérêt commercial) restent une séquence raisonnable. Le désaccord porte uniquement sur l'ordonnancement à très court terme (A/D/E/VPS doivent être vérifiés avant, pas après, l'ouverture à des testeurs externes), pas sur la direction produit proposée.
