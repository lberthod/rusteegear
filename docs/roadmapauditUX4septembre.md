# Roadmap post-audit UX — 4 septembre 2026

Audit UX / ergonomie / fonctionnel (hors code, hors sécurité) livré le
2026-09-04 sur le commit `4e78bfa`. Méthode : éditeur compilé et piloté
(`--pilot`, captures), démo web ouverte dans Chrome 148, workflows GitHub
vérifiés job par job, lecture des surfaces UI et des documents d'entrée.

Identifiants des constats : W (web), O (onboarding), E (éditeur), P (joueur),
M (mobile), N (réseau), A (accessibilité). Coût en tailles de t-shirt, même
format que `roadmapaudit29aout.md` et `roadmapaudit3septembre.md`.

## Constats bloquants (à traiter avant tout playtest)

| ID | Constat | Preuve | Coût |
| --- | --- | --- | --- |
| W-01 | Démo web publique = écran noir. `WebAssembly.instantiateStreaming(): invalid value type 0x0` dans Chrome 148 (WebGPU actif). Workflow Pages en échec sur les 27 pushes depuis `d759158` (3 sept.) : `rust-toolchain.toml` épingle 1.98.0, toolchain distincte de `stable` où l'action installe wasm32 → `can't find crate for core`. Le `.wasm` en ligne (3 sept. 13:34, 23 Mo) échoue lui-même, probablement `wasm-opt --all-features`. | `gh run list --workflow pages.yml`, console navigateur | S |
| E-01 | 💾 Enregistrer écrit toujours `~/motor3derust_scene.json`, même projet ouvert. | `src/app/persistence.rs:177-180`, `src/gfx/renderer/frame.rs:825` | S |
| E-02 | Aucun toast/bandeau : sauvegarde, import GLB, erreurs Lua (répétées chaque frame) partent dans la Console, fermée par défaut. | `src/editor/mod.rs:268`, `src/app/simulation.rs:1714,1750` | M |
| M-01 | Tactile : stick verrouillé sur l'axe vertical, Touch bruts ignorés → impossible de tourner sur Android/iOS. | `src/editor/hud.rs:1228`, `src/lib.rs:559` | M |

## Vague 0 — Rallumer la vitrine (S, 1 jour) — fait `93b7f48`

- [x] 0.1 `targets = ["wasm32-unknown-unknown"]` dans `rust-toolchain.toml` ; Pages vert (W-01)
- [x] 0.2 wasm-opt avec features explicites (bulk-memory, sign-ext, mutable-globals, nontrapping-fptoint) ; étape de test de chargement du `.wasm` dans `pages.yml` (W-01)
- [x] 0.3 `packaging/web/index.html` : barre de chargement, test `navigator.gpu`, message d'erreur, lien `.dmg` (W-02) ; bonus : plein écran et rendu qui suit la fenêtre (W-03, KNOWN_LIMITATIONS mis à jour)

## Vague 1 — Un éditeur qui parle (M, 1 semaine) — fait `2a84436`

- [x] 1.1 Enregistrer vise le projet ouvert ; raccourcis Cmd+S / Cmd+Maj+S / Cmd+O / Cmd+N (E-01, O-03)
- [x] 1.2 Toasts + compteur d'erreurs cliquable dans la barre d'état, alimentés par `log_buffer` (E-02)
- [x] 1.3 Badge d'erreur Lua sur l'objet (hiérarchie) et sous le champ Script (inspecteur), dédupliqué (E-02)
- [x] 1.4 Titre de fenêtre « Projet — scène • » + chemin dans la barre d'état (E-03)
- [x] 1.5 Garde « modifications non sauvegardées » sur Ouvrir, Ouvrir un projet, Démos, Nouveau projet (E-04)
- [x] 1.6 Inspecteur teinté pendant Play + mention « non conservé après Stop » (E-07)
- [x] 1.7 Démarrage sur le dernier projet ouvert (ou écran d'accueil Récents / Premier jeu / Hameau / Nouveau) (O-05)
- [x] 1.8 Modale d'autosave : date + delta au lieu du chemin brut (O-05)
- [x] 1.9 (découvert en route) `examples/broken_scene` faisait planter l'éditeur (« buffer slices can not be empty ») ; `local_aabb` inondait la Console — corrigés dans le même commit

## Vague 2 — Un joueur qui sait où il est (L, 2 semaines) — fait `473e3e9`

- [x] 2.1 Écran d'accueil joueur : pseudo mémorisé, « Jouer en ligne » / « Jouer seul », salon + classe, serveur repliable ; écran de chargement (P-01, N-01)
- [x] 2.2 Pastille réseau permanente + ping dans le HUD, bannières perte / reconnexion, Tab maintenu = roster (P-02) — partiel : pastille et bannières faites ; pas de ping (le protocole n'a pas de mesure de latence, changement serveur) ni de Tab = roster
- [x] 2.3 Pause : Reprendre / Paramètres / Se déconnecter / Quitter ; libellé « La partie continue » en ligne (P-03)
- [x] 2.4 Tactile : orbite au glissé sur la moitié droite, stick gauche 2 axes relatif caméra (M-01)
- [x] 2.5 Tactile : boutons Pause et Carte, ⚙ dans la pause, champs Firebase sortis de l'écran joueur (M-02)
- [x] 2.6 `safe_area` activé par défaut avec vraies marges système, écran maintenu allumé (M-03) — partiel : safe_area activé dans la scène livrée (marge en %, pas les insets système), wake lock sur le web seulement (Android/iOS restent à faire)
- [x] 2.7 En Play : touches d'outils Q/T/Y/F désactivées, glissé souris 2 axes, sensibilité (P-04)

## Vague 3 — Playtest (M, 3 jours + testeurs)

- [x] 3.1 Démos ▸ Premier jeu embarqué dans le binaire (`project::first_game_dir`) ; export qui détecte l'absence du dépôt (O-06) — `4b2f126` ; le `.dmg` reste à construire sur la machine de test
- [ ] 3.2 3 à 5 testeurs sur `docs/TEST_SCENARIO.md`, dont un non-développeur, sans aide (O-04)
- [ ] 3.3 Résultats dans `docs/playtests/2026-09-XX.md` (protocole et format : `docs/playtests/README.md`) ; reprioriser la vague 5

## Vague 4 — Une seule vérité documentaire (S, 2 jours) — fait `0c19f8c`

- [x] 4.1 Table unique des bindings dans le code → fenêtre Raccourcis complète + `docs/CONTROLS.md` (O-03)
- [x] 4.2 QUICKSTART : budget « ~30 min la première fois », sections pilotage et pré-push déplacées vers `docs/PILOT.md` / `CONTRIBUTING.md` ; `doctor.sh` vérifie la toolchain épinglée (O-01)
- [x] 4.3 README aligné sur les libellés réels, `docs/guide-createur` supprimé ou réécrit, KNOWN_LIMITATIONS rafraîchi (O-02)

## Vague 5 — Profondeur (XL, après playtest) — lots A `5a0c519` et B (voir hachages ci-dessous)

- [x] 5.1 Undo des champs de l'Inspecteur (état d'avant l'UI capturé par frame, une entrée par rafale), sélection conservée après undo (E-05) — `5a0c519`
- [x] 5.2 Éditeur de script dédié (monospace, erreur en ligne) ; liste des clips d'animation dans l'inspecteur (E-06) — `5a0c519`
- [x] 5.3 Remapping clavier (saut/attaque/tir/soin/pause/carte), palette daltonienne, échelle de l'interface egui (A-01) — `5a0c519` ; la souris garde un seul réglage (sensibilité, 2.7)
- [x] 5.4 Vocabulaire FR unifié (Aimanter, Compiler l'APK, Lancer sur l'appareil, Journal ADB), tutoiement partout, fenêtres flottantes mémorisées entre deux lancements, menu contextuel de la vue 3D (E-08) — lot B ; « Play/Stop » conservés (convention des éditeurs de jeu)
- [x] 5.5 Aide en jeu (F1 / bouton ?, contrôles et objectif depuis la table unique), journal de crash affiché aussi en mode joueur (P-06) — lot B
- [ ] 5.6 Spectateur actif ou réapparition, minuteur d'attente (P-05) — décision de game design (mort définitive par manche assumée dans le GDD), à trancher au playtest

## À conserver tel quel

Manette remappable ; panneau Build & Export (modèle de feedback) ; modale
de fermeture et récupération d'autosave ; assistant Nouveau projet et projets
récents ; HUD de jeu (vagues, bannières, mini-carte) ; pont de pilotage ;
`examples/first_game` et `examples/broken_scene` ; reconnexion automatique.
