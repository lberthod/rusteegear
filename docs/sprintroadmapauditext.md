# Roadmap post-audit externe — 5 septembre 2026

Sprint issu de l'[Audit RusteeGear après mise à jour — 5 septembre 2026](AUDIT_JEU_2026-07-17.md)
(audit externe, commit `c28a1ba`) et de sa contre-lecture dans
[auditexterne.md](auditexterne.md). Objectif : traiter les obstacles à
l'ouverture de tests accompagnés — protection du travail, export autonome,
release Android, réseau — avant d'inviter des testeurs externes.

Identifiants repris de l'audit : A (autosave cross-projet), B (export non
autonome), C (projets dépendants des assets globaux), D (publication
Android), E (annulation d'export), F (réseau/ressources), G (persistance
serveur bloquante), H (compatibilité fichiers/secrets). Coûts en tailles de
t-shirt (S/M/L). « Code » = risque établi par lecture du chemin de code, non
reproduit sur données utilisateur — à confirmer par un test avant de cocher.

## Constats bloquants (à vérifier avant toute invitation de testeurs)

| ID | Constat | Preuve | Coût |
| --- | --- | --- | --- |
| V-01 | État du protocole réseau sur le VPS public non contrôlé : le protocole est passé de 7 à 8 côté dépôt, mais rien ne confirme que le serveur public sert déjà la version 8. Un testeur externe peut échouer à se connecter sans explication utile. | Angle mort signalé dans l'audit ; à vérifier par une connexion pilotée sur `wss://ws.loicberthod.ch` | S |
| A-01 | Autosaves globales sans rattachement au projet source : `restore_autosave` remplace la scène sans changer `current_project` ni `scene_file`. Une autosave de A restaurée dans B, puis sauvegardée, peut écrire la scène de A dans B. | Code : `src/app/autosave.rs:101,124`, `src/app/persistence.rs:194`, `src/lib.rs:1494` | M |
| D-01 | `RUSTEEGEAR_KEYSTORE_PASS` requis par `packaging/build_apk.sh` mais non fourni par le workflow de release : la production d'un APK signé échoue dans la configuration versionnée actuelle. Le job CI vert ne valide que la compilation de la bibliothèque, pas la signature. | Code : `packaging/build_apk.sh:19`, `.github/workflows/release.yml:63` | S |
| E-01 | Annulation d'export : `TERM` puis `kill()` immédiat sans attendre un arrêt propre. Le script Android modifie `Cargo.toml` temporairement et compte sur un `trap EXIT` pour restaurer ; un arrêt forcé peut laisser des substitutions de build (potentiellement un secret de signature) dans le fichier de travail. | Code : `src/editor/export.rs:1171`, `packaging/build_apk.sh:34` | S |
| B-01 | Export utilise `CARGO_MANIFEST_DIR` (figé à la compilation) pour localiser scripts et écrire scène/assets dans les sources du moteur : un DMG installé hors de ce clone ne peut pas exporter. | Code : `src/editor/export.rs:17,186,761,1199` | L |

## Vague 1 — Vérifications rapides avant tests accompagnés (S, 1-2 jours) — fait

- [x] 1.1 Vérifier la version de protocole servie par le VPS public (V-01) — `cargo run --example smoke_vps --features net_tests` (déjà présent dans `examples/`, connexion réelle `wss://ws.loicberthod.ch`) : `Welcome`, snapshot avec monstres et projectile reçus avec le client actuel (`PROTOCOL_VERSION` 8) — **le VPS sert déjà la v8, aucun redéploiement requis**
- [x] 1.2 Câbler `RUSTEEGEAR_KEYSTORE_PASS` dans `.github/workflows/release.yml` (job `android`, secret `secrets.RUSTEEGEAR_KEYSTORE_PASS`) — le secret lui-même reste à définir par vous (`gh secret set RUSTEEGEAR_KEYSTORE_PASS`, valeur de votre choix, jamais générée ni vue par un agent) ; ajouté en même temps une étape « Restaurer le keystore de signature » (secret `RUSTEEGEAR_KEYSTORE_B64`, optionnel) — **sans elle, chaque release régénérerait un nouveau keystore et casserait la mise à jour Android d'une release à l'autre**, ce que le point D de l'audit n'avait pas relevé explicitement (D-01)
- [ ] 1.3 Vérifier qu'un APK signé est effectivement produit par une exécution du workflow de release (pas seulement la compilation de la bibliothèque) — nécessite que vous ayez défini les deux secrets ci-dessus, puis de pousser un tag `v*` ; non fait ici (D-01)

## Vague 2 — Sécurité du travail : autosave cross-projet (M, 3-5 jours) — fait

- [x] 2.1 Sidecar `<horodatage>.json.src` écrit à côté de chaque autosave (`write_autosave`, [src/app/autosave.rs](../src/app/autosave.rs)), contenant la cible de sauvegarde manuelle (`manual_save_reference_path`) au moment de l'écriture
- [x] 2.2 Rotation (`rotate_autosaves`) supprime le sidecar en même temps que l'autosave expirée — pas de rétention par projet séparée : le dossier reste global mais chaque fichier porte sa provenance, suffisant pour détecter un rattachement erroné
- [x] 2.3 `restore_autosave` compare le sidecar à la cible courante ; en cas d'absence ou de mismatch, détache `current_project`/`scene_file` — la scène restaurée reste chargée, mais la sauvegarde suivante passe obligatoirement par « Enregistrer sous » (comportement déjà existant depuis la vague 3.2 de 2026-09-04) plutôt que d'écrire dans le projet ouvert
- [x] 2.4 Tests croisés ajoutés : `restoring_an_autosave_from_another_project_detaches_the_open_project` (A restaurée pendant que B est ouvert → B détaché) et sa contre-épreuve `restoring_an_autosave_from_the_same_project_keeps_it_bound` (A-01)

## Vague 3 — Annulation d'export sans fuite (S, 1-2 jours) — fait

- [x] 3.2 `kill_process_tree` ([src/editor/export.rs](../src/editor/export.rs)) envoie `SIGTERM` au groupe puis laisse `KILL_GRACE` (5 s) avant d'escalader vers `SIGKILL` depuis un thread de fond, si le processus tourne toujours — laisse le temps au `trap EXIT` de `build_apk.sh` de restaurer `Cargo.toml` (E-01)
- [ ] 3.1 Isolation complète (ne plus modifier `Cargo.toml` source pendant le build) — non faite, le correctif 3.2 suffit à couvrir le cas d'annulation sans réécrire les scripts de packaging
- [ ] 3.3 Test avec un faux secret et un dépôt jetable — non fait dans cette session (nécessite un environnement de build Android complet, hors de la portée de `cargo test`)

## Vague 4 — Export autonome (L, 2+ semaines, dépend de la vague 2)

- [ ] 4.1 Séparer moteur et données du jeu : ne plus dépendre de `CARGO_MANIFEST_DIR` figé à la compilation pour localiser scripts/scène/assets (B-01)
- [ ] 4.2 Préparer les exports dans un répertoire de travail dédié plutôt que d'écrire dans les sources du moteur
- [ ] 4.3 Critère de sortie : exporter un jeu depuis une installation sur une machine qui n'a jamais contenu le dépôt du développeur

## Vague 5 — Projets portables (M, 1 semaine, peut suivre en parallèle de la vague 4)

- [ ] 5.1 Rattacher scènes, scripts, ressources et paramètres d'export à un dossier de projet autonome, plutôt que le dossier utilisateur global (`src/project.rs:7,50`)
- [ ] 5.2 Identifiants stables et chemins relatifs à l'intérieur du dossier de projet
- [ ] 5.3 Consommer le champ `build` du manifeste de projet (actuellement réservé et non utilisé par le moteur)
- [ ] 5.4 Critère de sortie : un collègue clone un projet sur une machine vierge, l'ouvre et obtient la même scène sans copie manuelle de dossiers cachés

## Vague 6 — Durcissement réseau et serveur (L, en parallèle, non bloquant pour les premiers tests)

- [ ] 6.1 Borner la file sortante par client (F) ; remplacer les snapshots obsolètes plutôt que les accumuler (`src/net/server_loop.rs:341`)
- [ ] 6.2 Ajouter un délai maximal pour le handshake et le premier `Join` des connexions admises, pas seulement pour les connexions refusées (`src/net/server_loop.rs:488`)
- [ ] 6.3 Budget global de tâches de handshake couvrant aussi les refus, pour limiter les tâches simultanées sous afflux (`src/net/server_loop.rs:544`)
- [ ] 6.4 Sortir les lectures/écritures Firebase de la boucle de tick synchrone ; traitement séparé avec délai maximal, identifiant de manche et écritures idempotentes pour éviter les doubles crédits lors des reprises (G, `src/bin/server.rs:984`)
- [ ] 6.5 Tester sur un serveur local isolé : clients silencieux, lents, rafales de connexions, consommation mémoire

## Vague 7 — Compatibilité fichiers et secrets (S, à glisser dans une vague existante)

- [ ] 7.1 `Scene::load` : refuser les versions futures au lieu de les migrer silencieusement vers la version courante, comme le fait déjà le manifeste de projet (H, `src/scene/persistence.rs:160,188`)
- [ ] 7.2 Déplacer la clé privée DeepSeek hors des réglages sérialisés en clair, vers le trousseau système ; exclure explicitement les secrets des diagnostics et des exports (H, `src/app/settings.rs:13,433`) — ne pas confondre avec la clé publique de configuration Firebase, qui reste en config

## Hors périmètre de ce sprint

Les vagues 1-3 conditionnent l'ouverture de tests accompagnés à des créateurs
externes ; les vagues 4-7 sont nécessaires avant de présenter RusteeGear comme
un outil commercial autonome (voir la section « Direction proposée » de
l'audit). Ne pas ajouter de nouvelles fonctions graphiques tant que les
vagues 1-3 ne sont pas closes : ça ne résout aucun des obstacles identifiés.
