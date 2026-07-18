# Serveur MCP « RustyGear » — proposition d'intégration future

## Contexte

Le projet dispose déjà du socle technique nécessaire pour qu'un LLM crée/édite des scènes et importe des assets Blender : `Scene::from_ai_json` + `SCENE_SYSTEM_PROMPT` (`src/app/ai.rs`) génèrent déjà des scènes depuis du texte via une API IA externe, et `src/scene/import.rs` sait déjà charger des `.glb`. Ce qui manque, c'est un pont natif entre Claude Code et ces briques : aujourd'hui l'édition se fait à la main (fichiers, `cargo build`/`cargo run` manuels), sans moyen de vérifier visuellement une scène sans ouvrir le jeu.

Objectif : un binaire serveur MCP (`src/bin/rustygear_mcp.rs`) exposé en stdio à Claude Code, avec des tools pour créer/éditer des scènes, importer automatiquement des exports Blender via un dossier surveillé, builder/lancer/arrêter le jeu, gérer les prefabs, et capturer un screenshot headless pour vérification visuelle.

## Infrastructure existante à réutiliser (vérifiée)

- **Scènes** : `Scene`/`SceneObject` (`src/scene/mod.rs`), `Scene::save`/`Scene::load`/`Scene::from_ai_json` (`src/scene/persistence.rs`), schéma JSON contraint déjà documenté dans `src/app/ai.rs::SCENE_SYSTEM_PROMPT`, exemples dans `assets/player_scene.json` et `assets/examples/*.json`.
- **Import glTF/GLB** : `import::load_gltf`, `load_gltf_skeleton`, `load_gltf_clips` (`src/scene/import.rs`) — seul format supporté, aligné avec l'export Blender.
- **Prefabs** : `Scene::save_prefab`/`instantiate_prefab`/`sync_prefab_instances` (`src/scene/prefab.rs`), `PrefabScope::{General, Scene(String)}`, `list_prefabs`/`delete_prefab`/`register_asset`/`assets_dir()` (`src/assets.rs`).
- **Watcher de dossier** : `src/lib.rs` (~ligne 600) utilise déjà `notify::RecommendedWatcher` en mode `NonRecursive` pour le hot-reload de textures — `notify = "8.2.0"` est déjà une dépendance desktop, pas de nouvelle dépendance à ajouter pour ça.
- **Rendu headless** : `Renderer::new_headless` + `render_scene_headless` (`src/gfx/renderer.rs`), déjà utilisé par `tests/golden_render.rs`. Ne capture pas le HUD egui. Non fiable sur CI (pas de GPU) — usage local uniquement, comme le test existant.
- **Binaire headless de référence** : `src/bin/server.rs` — patron à suivre pour créer un nouveau binaire sans dépendre de winit/egui (sauf pour `gfx`, nécessaire ici pour le screenshot).
- Pas de `[workspace]` : tout binaire sous `src/bin/*.rs` est auto-découvert par Cargo et hérite des mêmes dépendances (`tokio`, `notify`, `image`, `ureq`, `rfd` déjà disponibles côté desktop).

## Décisions de scope

- **Périmètre** : scènes, import Blender via watcher, build/run/test, prefabs — le plus complet possible pour maximiser l'autonomie de Claude.
- **Workflow Blender** : un tool démarre la surveillance d'un dossier d'export ; les nouveaux `.glb` y sont importés automatiquement (pas d'export manuel déposé à la main).
- **Langue** : noms de tools en anglais (convention MCP standard), descriptions/commentaires en français (cohérent avec `ai.rs`/`server.rs`).
- **Screenshot** : usage local uniquement, pas d'exigence CI (même limite que `golden_render.rs`).

## Architecture

- Nouveau binaire `src/bin/rustygear_mcp.rs` dans le crate existant (pas de nouveau crate/workspace) — même `cargo build`/`clippy` que le reste, réutilise directement les types serde existants.
- SDK : `rmcp` (SDK MCP officiel Rust), ajouté sous la table `[target.'cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))'.dependencies]` du `Cargo.toml`, aux côtés de `rfd`/`ureq`.
- Transport : stdio (lancé en sous-processus par Claude Code, pas de port à gérer).
- `tokio` (déjà dépendance) comme runtime async pour `rmcp`.
- État interne du process, protégé par `tokio::sync::Mutex` :
  ```rust
  struct McpState {
      app: AppState,
      current_scene_path: Option<PathBuf>,
      watcher: Option<(notify::RecommendedWatcher, mpsc::Receiver<notify::Result<notify::Event>>)>,
      watch_dir: Option<PathBuf>,
      game_process: Option<std::process::Child>,
  }
  ```

## Tools MCP

**Scènes**
- `create_scene(objects_json, name?) -> {path, summary}` — via `Scene::from_ai_json` + `Scene::save`.
- `edit_scene(path, patch)` — charge/sauvegarde via `Scene::load`/`Scene::save`, patch au format JSON Patch (RFC 6902) sur le JSON complet de `Scene`.
- `load_scene(path) -> Scene JSON`
- `list_scenes(dir?) -> [path]`

**Import Blender**
- `import_asset(path, register=true) -> {asset_id, aabb_min, aabb_max, has_skeleton, has_animations}` — via `import::load_gltf` (+ skeleton/clips) + `assets::register_asset`.
- `watch_blender_exports(dir) -> {watching}` — démarre un `notify::recommended_watcher` (même pattern que `lib.rs`), filtré sur `.glb`/`.gltf`.
- `poll_import_events() -> [{path, asset_id, imported_at}]` — vide la queue, importe automatiquement chaque nouveau fichier détecté. (Notifications MCP push en option ultérieure si `rmcp` le supporte simplement — non bloquant pour la v1.)
- `stop_watching() -> {ok}`

**Prefabs**
- `list_prefabs(scope)`, `create_prefab(object_json, name, scope)`, `instantiate_prefab(asset_id, name, position, scene_path)`, `sync_prefab_instances(scene_path)`
- `delete_prefab(scope, name, confirm: bool)` — `confirm` obligatoire (erreur explicite si absent/false), car il n'y a pas d'UI de popup côté MCP.

**Build / Run / Test**
- `build(profile="dev-fast") -> {ok, warnings, errors}` — `cargo build --profile <profile> --message-format=json`, parsing structuré des diagnostics.
- `run_demo() -> {pid, log_tail}` — **point à vérifier en premier** : `src/lib.rs::run()` (ligne 659) ne lit que `--player` via `std::env::args()`, aucun flag de sélection de scène/démo n'existe. Solution retenue : le tool sauvegarde la scène désirée au chemin par défaut chargé au démarrage (à identifier en lisant `make_app`, ligne 625, et le chemin de chargement initial de scène), puis lance `cargo run` sans argument spécial — pas de modification du cœur du moteur.
- `stop_game() -> {ok}`
- `capture_screenshot(scene_path?, width=1280, height=720) -> {png_base64}` — tourne dans le process serveur lui-même via `Renderer::new_headless`/`render_scene_headless`, encodage PNG via `image` (déjà dépendance). Local uniquement.
- `run_tests(filter?) -> {ok, passed, failed, output}` — `cargo test [filter]`.

## Fichiers à créer/modifier

- `src/bin/rustygear_mcp.rs` (nouveau — le binaire serveur)
- `Cargo.toml` (ajout `rmcp` en dépendance desktop-only)
- `src/assets.rs` (petit refactor : extraire le helper `watch_dir(dir) -> Option<(RecommendedWatcher, Receiver<...>)>` déjà écrit dans `lib.rs`, pour le partager entre le hot-reload existant et le nouveau watcher Blender)
- Documentation de configuration Claude Code (`.mcp.json` ou équivalent) pointant vers le binaire compilé

## Ordre d'implémentation suggéré

1. Lire `src/lib.rs::make_app` (ligne 625) en entier pour confirmer le chemin de scène chargé par défaut au démarrage (bloquant pour `run_demo`).
2. Squelette du binaire + `rmcp` en dépendance, un seul tool trivial (`list_scenes`) pour valider la boucle stdio de bout en bout (test via `@modelcontextprotocol/inspector` ou client de test avant de brancher Claude Code).
3. Tools scène (`create_scene`, `load_scene`, `edit_scene`) — mapping direct sur le code existant.
4. Tools prefab (`list_prefabs`, `create_prefab`, `delete_prefab` avec confirm, `instantiate_prefab`, `sync_prefab_instances`).
5. Import + watcher : extraire `watch_dir` en helper partagé, puis `import_asset`, `watch_blender_exports`, `poll_import_events`.
6. `build`, `run_tests` (les plus simples).
7. `run_demo`/`stop_game` (dépend du point 1).
8. `capture_screenshot` (le plus délicat — init GPU headless dans le process serveur), en dernier car il valide visuellement tout le reste.
9. Config Claude Code (`.mcp.json`, transport stdio, chemin vers `target/debug/rustygear_mcp`).

## Vérification

- `cargo fmt` + `cargo clippy` doivent passer sur le nouveau binaire (CI stricte du projet).
- Séquence de bout en bout depuis Claude Code une fois configuré : `create_scene` → `capture_screenshot` (inspection visuelle du PNG retourné) → `build` → `run_demo` → `stop_game`. Cette séquence sert elle-même de test-preuve du serveur MCP (cohérent avec le pattern « scènes/démos jouables comme preuve d'implémentation » du projet).
- Tests inline `#[cfg(test)]` pour les tools touchant `assets_dir()`, en réutilisant le pattern `temp_assets_dir` déjà présent dans `src/scene/prefab.rs` (isolation, cf. pollution connue de `~/.motor3derust/assets/prefabs/`).

## Points ouverts (à trancher en tout début d'implémentation, pas de blocage sur le reste du plan)

- Format exact du chemin de scène par défaut chargé au démarrage du jeu (nécessaire pour `run_demo`).
- Support des notifications MCP push côté `rmcp` en stdio (sinon `poll_import_events` reste en mode polling, ce qui fonctionne déjà).
- Verrouillage de fichiers : `Scene::save`/`assets.rs` n'ont aujourd'hui aucun verrou — si une session éditeur est ouverte en parallèle du serveur MCP, il y a un risque d'écrasement mutuel (sessions concurrentes sur ce dépôt). Hors scope de cette proposition, à documenter comme limitation connue.
