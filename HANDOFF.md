# Passation — RusteeGear

Document de reprise pour le développeur qui prend la suite. Lire aussi
**[README.md](README.md)** (vision/archi), **[SPRINTS.md](SPRINTS.md)** (récap + logique
des prochains sprints), **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)** (détail sprint)
et **[packaging/EXPORT.md](packaging/EXPORT.md)**.

## État au moment de la passation (13 juillet 2026)

- Phases **A→N** en place (moteur solo + multijoueur + animation + image + chaîne gameplay) ;
  **~298 tests** (`grep -rn '#\[test\]' src | wc -l`), CI active (`fmt --check` + `clippy -D
  warnings` + `cargo test` + cross-build Android/iOS). ⚠️ **Au moment précis de cette passation,
  une autre session a du travail non commité en cours sur `src/editor/mod.rs`** (glisser-déposer
  des overlays HUD, panneau 👁 Aperçu HUD) qui **casse temporairement la compilation**
  (`Scene { .. }` sans le champ `hud_layout`, signature de `weapon_hud`/`crosshair` non propagée
  partout) — ne pas juger le vert CI sur cet état transitoire, vérifier `cargo build` après que
  cette session ait fini/committé.
- L'éditeur couvre la **boucle produit mobile sans ligne de commande** : créer une scène →
  marquer un objet **pilotable** (joystick + saut + collisions, sans script) → Build Panel Android → APK.
- Un **serveur multijoueur headless** (`src/bin/server.rs`, WebSocket) existe : prédiction client,
  réconciliation, lobbies, chat, classement, comptes Firebase, combat (boule de feu, IA, vie,
  changement d'arme) — voir `SPRINTNETWORK.md` et `AUDIT_LATENCE_MULTIJOUEUR.md`.
- **Animation squelettale** (skinning GPU, blending, state machine, réplication réseau, anim
  notifies Sprint 99) et **image** (ciel/fog, HDR/tone mapping, bloom, mipmaps) sont livrées.
- **Chaîne gameplay Lua** (Phase N) : événements, GUID d'assets/versioning, prefabs (mécanisme
  fait, UI éditeur restante — Sprint 96), API scène spawn/destroy/find_tag, sauvegarde `user://`
  (Sprint 98, non vérifiée sur device Android réel).
- Le **chemin de rendu** est sans allocation par frame (culling lumières, tampons réutilisés,
  plan de dessin par index, re-tri paresseux).
- ⚠️ Le **gyroscope** est simulé au clavier sur desktop mais **pas branché au capteur Android**
  (Sprint 48, toujours ⬜). La signature *distribution* store reste à faire (Sprint 49, ⬜,
  secrets requis).
- ⚠️ **Sprint 94** (cycle de vie + handles générationnels, `slotmap`) reste **⬜**, sauté dans
  l'enchaînement 93→99 — c'est le refactor le plus délicat annoncé (indices réseau + undo) ;
  ne pas le découvrir tardivement, il conditionne la robustesse du spawn/destroy Lua du Sprint 97.
- **Prochaine étape formelle = Phase O — Physique & feel** (sprints 100→103 : trimesh/convexe,
  CCD, requêtes gameplay, character controller kinématique), avec un avertissement du plan :
  faire le **Sprint 103 seul**, il menace la prédiction réseau acquise (72→77). Voir
  [SPRINTS.md](SPRINTS.md) et [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md).

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
- `src/app/` — **logique sans GPU** : `AppState` (scène, sélection, picking, Play), `build_config`,
  `input`, `settings`, `ai` (IA ennemis), `combat`/`fireball`/`health` (gameplay solo/réseau),
  `multiplayer` (client réseau côté app), `network_client`.
- `src/gfx/` — rendu `wgpu` : `renderer`, `mesh`, `camera`, shaders WGSL (skinning, HDR, bloom…).
- `src/scene/` — `Scene`/`SceneObject` (groupes, couleur, lumière, prefabs, GUID d'assets),
  import glTF, sérialisation, versioning.
- `src/runtime/` — mode Play : `physics` (rapier3d), `audio` (kira), `sfx`, `savegame` (`user://`).
- `src/net/` — protocole réseau (`protocol`), transport WebSocket (`client`, `server_loop`),
  interpolation (`interpolation`), backend annexe (`firebase`).
- `src/bin/server.rs` — serveur multijoueur headless (partage `src/net` et `src/app`).
- `src/editor/` — UI egui : panneaux + `export.rs` (Build & Export), `readiness.rs` (Readiness Check).
- `src/assets.rs` — assets embarqués (`include_dir`, schémas `bundle://` / `asset-id://`) pour le player exporté.

Règle d'or : **la logique (`app`) ne dépend pas du GPU**. Tout ce qui touche `wgpu` reste dans `gfx`.
C'est ce qui rend le portage mobile direct — ne pas la casser.

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

**Sprint 100 — Trimesh + convexe** (cf. [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md), Phase O) :
ouvrir la physique rapier aux décors importés (trimesh) et aux formes convexes, préalable aux
sprints 101-103 (CCD, requêtes gameplay, character controller kinématique).

Alternative plus douce, à ne **pas** reporter indéfiniment : **Sprint 94** (cycle de vie +
handles générationnels, `slotmap`) — resté ⬜ dans la Phase N alors que le reste (93, 95→99)
est fait ; c'est le refactor le plus délicat annoncé au plan (indices réseau + undo à ne pas
casser), mais plus on construit dessus (prefabs, spawn/destroy Lua), plus il coûtera cher à
faire a posteriori.

⚠️ Avant de commencer quoi que ce soit : vérifier `git status` / `cargo build` — au moment de
cette passation, `src/editor/mod.rs` avait un chantier non commité d'une autre session
(glisser-déposer HUD) qui cassait la compilation (champ `hud_layout` manquant dans plusieurs
constructions de `Scene`). Attendre que ce chantier soit committé ou stashé avant de bâtir dessus.
