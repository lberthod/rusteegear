# Passation — RusteeGear

Document de reprise pour le développeur qui prend la suite. Lire aussi
**[README.md](README.md)** (vision/archi), **[SPRINTS.md](SPRINTS.md)** (récap + logique
des prochains sprints), **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)** (détail sprint)
et **[packaging/EXPORT.md](packaging/EXPORT.md)**.

## État au moment de la passation (13 juillet 2026)

- Phases **A→N** en place (moteur solo + éditeur + mobile + multijoueur en ligne) ;
  **~310 tests** (302 dans `src/`, 8 dans `tests/`, cf. `ROADMAP_SPRINTS.md` pour le
  détail lib/bin/golden), `clippy --all-targets` propre, `fmt` OK, CI active.
- L'éditeur couvre la **boucle produit mobile sans ligne de commande** : créer une scène →
  marquer un objet **pilotable** (joystick + saut + collisions, sans script) → Build Panel Android → APK.
- Un **serveur multijoueur headless** (`src/bin/server.rs`, WebSocket + bincode) existe,
  **serveur autoritaire** : prédiction client, réconciliation, lobbies/salons, mouvement,
  combat (boule de feu, IA, vie, changement d'arme), soin coopératif, sauvegarde,
  animations répliquées, comptes/classement Firebase (backend annexe) — voir la section
  dédiée dans [README.md](README.md#-multijoueur-en-ligne-chantier-en-cours),
  `SPRINT_MMORPG.md`/`SPRINTNETWORK.md` et `AUDIT_LATENCE_MULTIJOUEUR.md`.
- **Animation squelettale** (skinning GPU, blending, state machine, réplication réseau,
  anim notifies Sprint 99) et **image** (ciel/fog, HDR/tone mapping, bloom, mipmaps) sont
  livrées (Phases L et M).
- **Chaîne gameplay Lua** (Phase N) : événements, GUID d'assets/versioning, prefabs
  (mécanisme moteur fait, UI éditeur restante — Sprint 96), API scène
  spawn/destroy/find_tag, sauvegarde `user://` (Sprint 98, non vérifiée sur device
  Android réel).
- **Physique** (Phase O, en cours) : colliders `TriMesh`/`ConvexHull` sur les décors
  importés livrés (Sprint 100) ; restent CCD/couches de collision (101), requêtes
  gameplay (102), character controller kinématique (103b, cf. plus bas).
- Le **chemin de rendu** est sans allocation par frame (culling lumières, tampons
  réutilisés, plan de dessin par index, re-tri paresseux) et la simulation tourne à
  **pas fixe** (accumulateur, `AppState::fixed_substeps`), découplée du framerate.
- ⚠️ Le **gyroscope** est simulé au clavier sur desktop mais **pas branché au capteur
  Android** (Sprint 48, toujours ⬜). La signature *distribution* store reste à faire
  (Sprint 49, ⬜, secrets requis). Ces deux points sont toujours ouverts.
- ⚠️ **Sprint 94** (cycle de vie + handles générationnels, `slotmap`) reste **⬜**, sauté
  dans l'enchaînement 93→99 — c'est le refactor le plus délicat annoncé (indices réseau +
  undo) ; ne pas le découvrir tardivement, il conditionne la robustesse du
  spawn/destroy Lua du Sprint 97. Plus on construit dessus (prefabs, physique), plus il
  coûtera cher à faire a posteriori.
- ⚠️ Les **golden tests de rendu** (`tests/golden_render.rs`, `tests/golden_skinning.rs`)
  sont *skippés* en CI (pas de GPU headless sur `ubuntu-latest`) — ils ne protègent que
  si tu les lances en local avant de merger un changement de rendu.
- **Prochaine étape = Phase O — Physique & feel** (sprints 100→103c, Sprint 100 fait) :
  avant le character controller kinématique (**103b**, qui menace la prédiction réseau
  acquise 72→77, à faire **seul**), traiter la maintenabilité (**103a** — voir « Par où
  commencer » ci-dessous). Voir [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md).

## Commandes clés

```bash
cargo run                      # éditeur desktop (mode édition)
cargo run -- --player          # mode player (scène plein écran)
cargo run --profile dev-fast   # itération rapide (compile bien plus vite que release)
cargo run --bin server         # serveur multijoueur headless (WebSocket)

cargo test                     # tests unitaires
cargo clippy --all-targets -- -D warnings   # lint (doit rester vert)
cargo fmt --all                # formatage (la CI vérifie --check)
cargo build --release          # build optimisé (LTO ; lent)
```

Toolchain : Rust stable, édition 2024. Composants : `rustup component add clippy rustfmt`.

## Exports / packaging

Depuis l'éditeur : bouton **📦 Export** (config, presets, install device, log). En ligne de commande :

```bash
./packaging/build_dmg.sh       # macOS .dmg (cargo install cargo-bundle)
./packaging/build_apk.sh       # Android .apk (NDK + cargo install cargo-apk)
./packaging/build_ios.sh       # iOS .ipa (Xcode + brew install xcodegen)
./packaging/install_ios_device.sh   # build + signe + installe sur iPhone branché
```

Variables pilotées par le panneau Export : `OUTPUT_NAME`, `BUNDLE_ID`, `APP_VERSION`,
`BUILD_NUMBER`, `PLAYER_BUILD=1`, `INSTALL_DEVICE`, et pour iOS `TEAM_ID`/`IDENTITY`/`PROFILE`.

## Architecture (carte mentale)

- `src/lib.rs` — boucle winit, `run()` desktop, `android_main`, resume mobile.
- `src/app/` — **logique sans GPU** : `AppState` (scène, sélection, picking, Play, scripts
  Lua, réseau, sauvegarde…), `build_config`, `input`, `settings`, `ai` (IA ennemis),
  `combat`/`fireball`/`health` (gameplay solo/réseau, réutilisés par le serveur),
  `multiplayer` (joueurs réseau), `network_client`.
  ⚠️ `AppState`/`app/mod.rs` (≈7000 lignes) cumule trop de responsabilités — découpage
  prévu au Sprint 103a, cf. `AUDIT.md` §7.4.
- `src/gfx/` — rendu `wgpu` : `renderer`, `mesh`, `camera`, shaders WGSL (skinning, HDR, bloom…).
- `src/scene/` — `Scene`/`SceneObject` (groupes, couleur, lumière, prefabs, GUID
  d'assets), import glTF, sérialisation, versioning. Également volumineux (`mod.rs`
  ≈4800 lignes), à découper au Sprint 103a.
- `src/runtime/` — mode Play : `physics` (rapier3d), `audio` (kira), `sfx`, `savegame` (`user://`).
- `src/net/` — protocole réseau (`protocol`, bincode), transport WebSocket (`client`,
  `server_loop`, desktop only), interpolation (lissage malgré la latence), backend
  annexe (`firebase`).
- `src/bin/server.rs` — serveur multijoueur headless (partage `src/net` et `src/app`,
  fait tourner `AppState` sans GPU/UI).
- `src/editor/` — UI egui : panneaux + `export.rs` (Build & Export), `readiness.rs`
  (Readiness Check). Aussi volumineux (`mod.rs` ≈3900 lignes), à découper au Sprint 103a.
- `src/assets.rs` — assets embarqués (`include_dir`, schémas `bundle://` / `asset-id://`)
  pour le player exporté.

Règle d'or : **la logique (`app`) ne dépend pas du GPU**. Tout ce qui touche `wgpu` reste dans `gfx`.
C'est ce qui rend le portage mobile direct — ne pas la casser. C'est aussi ce qui permet à
`src/bin/server.rs` de réutiliser `AppState` tel quel, en headless.

## Pièges connus / conventions

- **Player vs éditeur** : mobile = mode player auto ; desktop éditeur, sauf `--player` ou feature `player_build`.
  L'export desktop utilise `PLAYER_BUILD=1` pour produire un player jouable (scène + assets embarqués).
- **Sélection** : invariant `selection` (primaire) ⊆ `selected` (ensemble), maintenu **à la main** via
  `select_single` / `clear_selection` / `toggle_select`. Ne jamais réassigner `selection` directement.
- **Scène embarquée** : `assets/player_scene.json` (réécrit à l'export) doit exister pour compiler
  (`include_str!`). `assets/bundle/.gitkeep` est requis par `include_dir!`.
- **`.gltf` à références externes** ne s'embarque pas → préférer `.glb` (autonome).
- **GPU au repos** : la boucle throttle quand rien ne bouge (`AppState::is_active`). Garder `Poll` en Play/interaction.
- **Rendu** : ne jamais committer une modif GPU « à l'aveugle » — lancer l'app et regarder.

## Git / CI

- Travailler sur une branche, PR vers `main`. La CI (`.github/workflows/ci.yml`) exige
  `fmt --check` + `clippy -D warnings` + `cargo test` + cross-build Android/iOS.
- Release : pousser un tag `v*` déclenche `.github/workflows/release.yml` (artefacts macOS/Android).

## Par où commencer

**Sprint 103a — maintenabilité : découpage des gros modules & `AppState`** (cf.
[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md), Phase O) : `app/mod.rs` (≈7000 lignes),
`editor/mod.rs` (≈3900) et `scene/mod.rs` (≈4800) sont devenus risqués à modifier —
`AppState` en particulier cumule gameplay, scripts Lua, réseau, sauvegarde, combat,
animation et pilotage indirect de l'UI. Extraire ces sous-systèmes en modules dédiés
avant d'ajouter la physique du character controller (Sprint 103b), pour limiter les
conflits et les régressions. Refactor pur, sans nouvelle fonctionnalité, à faire seul
(pas de sprint gameplay en parallèle) — bon premier chantier pour se familiariser avec
tout le cœur du moteur sans risquer de casser du gameplay en même temps.

Alternative à ne **pas** reporter indéfiniment : **Sprint 94** (cycle de vie + handles
générationnels, `slotmap`) — resté ⬜ dans la Phase N alors que le reste (93, 95→99)
est fait ; c'est le refactor le plus délicat annoncé au plan (indices réseau + undo à
ne pas casser), mais plus on construit dessus (prefabs, spawn/destroy Lua, et bientôt
la physique de la Phase O), plus il coûtera cher à faire a posteriori.

Alternative plus douce pour s'imprégner du code : lire `AUDIT.md`, `AUDIT_MMORPG.md` et
`AUDIT_LATENCE_MULTIJOUEUR.md` (audits existants, encore largement à jour) avant de
toucher au code.
