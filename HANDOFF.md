# Passation — RusteeGear

Document de reprise pour le développeur qui prend la suite. Lire aussi
**[README.md](README.md)** (vision/archi), **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)**
(historique + Phase F à venir), **[AUDIT.md](AUDIT.md)** et **[packaging/EXPORT.md](packaging/EXPORT.md)**.

## État au moment de la passation

- Phases **A→E** en place (cœur) ; **11 tests verts**, `clippy -D warnings` propre, CI active.
- ⚠️ Plusieurs nouveautés (assets embarqués, matériaux/lumière, resume mobile) sont **vertes en CI
  mais jamais exécutées de bout en bout**. **Priorité = Sprint 28** (validation réelle) avant d'ajouter des features.

## Commandes clés

```bash
cargo run                      # éditeur desktop (mode édition)
cargo run -- --player          # mode player (scène plein écran)
cargo run --profile dev-fast   # itération rapide (compile bien plus vite que release)

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
- `src/app/` — **logique sans GPU** : `AppState` (scène, sélection, picking, Play), `build_config`, `input`.
- `src/gfx/` — rendu `wgpu` : `renderer`, `mesh`, `camera`, shaders WGSL.
- `src/scene/` — `Scene`/`SceneObject` (groupes, couleur, lumière), import glTF, sérialisation.
- `src/runtime/` — mode Play : `physics` (rapier3d), `audio` (kira).
- `src/editor/` — UI egui : panneaux + `export.rs` (panneau Build & Export).
- `src/assets.rs` — assets embarqués (`include_dir`, schéma `bundle://`) pour le player exporté.

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

**Sprint 28** (cf. ROADMAP_SPRINTS.md) : lancer un export réel, valider sur device, ajouter les tests
manquants (invariant de sélection, résolution `bundle://`), réduire les `unwrap()`. C'est l'onboarding idéal.
