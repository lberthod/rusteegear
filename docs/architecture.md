# Architecture

État des lieux courant du moteur — où chercher, pas comment on y est arrivé
(pour l'historique par sprint, voir `docs/audits/` et `ROADMAP_SPRINTS.md`).
Sept sections, chacune un point d'entrée avec les fichiers/types clés, pas
une resucée du code.

## Boucle principale

`src/lib.rs::App` implémente `winit::application::ApplicationHandler` :
`resumed()` crée fenêtre + `Renderer` ; `window_event()` reçoit
`RedrawRequested` et appelle `renderer.render(&mut self.state)` ; `about_to_
wait()` réarme `request_redraw()` et choisit `ControlFlow::Poll` tant que
`AppState::is_active()` (Play qui tourne, ou interaction en cours) — sinon
l'app throttle. Point d'entrée : `run()` (`src/lib.rs`).

Toute la logique de jeu (scène, sélection, mode Play, script Lua, réseau)
vit dans `AppState` (`src/app/mod.rs` + ses sous-modules), **sans**
dépendance GPU — le `Renderer` ne fait que consommer cet état pour dessiner,
jamais l'inverse.

## Pipeline simulation

`AppState::advance_play` (`src/app/simulation.rs`, extrait de `app/mod.rs`
au Sprint 105a-1) : lit le dt réel, gère les transitions Édition ↔ Play,
puis avance la simulation à **pas fixe** (`FIXED_DT = 1/60`, accumulateur
borné contre la spirale de la mort, `fixed_substeps`) — indépendant du
framerate d'affichage. Chaque pas fixe (`sim_step`) exécute les scripts Lua
de chaque objet (`app/scripting.rs::run_script`), les actions au tap, puis
pilote la physique (`runtime::physics::Physics::control`/`step`, corps
kinématique pour le joueur depuis le Sprint 103b). Entre deux pas fixes,
`blend_render_poses` interpole la pose affichée (prev→curr) pour un rendu
lisse même quand le pas de simulation ne tombe pas pile sur une frame ;
`restore_sim_poses` annule ce mélange juste avant de resimuler, sauf si le
transform a été modifié de l'extérieur depuis (réconciliation réseau, effet
d'attaque…).

## Pipeline rendu

`src/gfx/renderer.rs::Renderer` — couche de rendu pure (wgpu), sans état
métier propre : `render(&mut self, app: &mut AppState)` est l'appel
principal par frame, `resize()`/`on_ui_event()` (interception egui) les
autres points d'entrée notables. Structures GPU au sommet du fichier :
`CameraUniform`, `ModelUniform`, `PointLightU`/`SceneUniform`/
`BloomUniform`. Shaders sous `src/gfx/shaders/`. La caméra (orbite,
yaw/pitch/distance) est séparée dans `src/gfx/camera.rs::OrbitCamera`
(`eye()`, `view_proj()` — NDC wgpu, z ∈ [0,1]).

## Modèle scène/assets

`src/scene/mod.rs` — **pas d'ECS** : `Scene` est un `Vec<SceneObject>`, où
chaque `SceneObject` agrège directement ses composants optionnels
(`Transform`, `MeshKind`, `Controller`, `Combat`, `AudioSource`, etc.). Sous-
modules `demos`/`import`/`persistence`/`prefab`/`queries`.

`src/assets.rs` — trois schémas d'URI, dispatchés par `is_known_scheme` :
- `bundle://` : embarqué au binaire (`include_dir!`), pour le player exporté.
- `asset://` : dossier de projet (`~/.motor3derust/assets/`, édition
  desktop), avec repli sur le bundle si absent.
- `asset-id://` : référence stable (uuid → nom de fichier via un manifeste),
  survit à un renommage — délivrée par les nouveaux imports.

Toute fonction qui joint un nom fourni par l'appelant à un dossier de base
passe par `safe_join` (Sprint 105a-2) — garde-fou de traversée de
répertoire par analyse de composants de chemin.

## Modèle réseau

`src/net/protocol.rs` — `ClientMsg`/`ServerMsg`, codec `bincode` (compact,
plusieurs fois/seconde/joueur). Champs `Join` validés par `valid_join_fields`
(Sprint 105a-2, longueur + charset) avant toute inscription côté serveur.

`src/net/server_loop.rs::NetServer` (thread tokio dédié, `WebSocketConfig`
resserré à 64 Kio) / `src/net/client.rs::NetClient` — transport, un canal
`std::sync::mpsc` synchrone de chaque côté (le reste du programme n'a jamais
besoin de connaître tokio). `src/bin/server.rs` est le binaire serveur
headless (multi-salons, une `AppState` par salon).

Côté client, `src/app/network_client.rs` intègre la connexion à `AppState` :
prédiction locale inchangée (`sim_step` tourne pareil en solo/réseau),
réconciliation douce (`apply_local_network_position`, `interpolation::
reconcile`, `SNAP_THRESHOLD`) plutôt qu'un `snap` brutal sur la position
serveur. `src/net/firebase.rs` est un backend annexe (comptes, progression,
chat, classement) via l'API REST Firebase — jamais le gameplay temps réel.

**Tests réseau** (socket réel) : derrière la feature Cargo `net_tests`
(désactivée par défaut, Sprint 105a-3) — `cargo test` reste rapide et
indépendant d'un environnement CI qui restreint parfois le bind loopback.
Couverture complète : `cargo test --features net_tests`.

## Modèle scripting Lua

`src/app/scripting.rs::run_script` — exécute le chunk Lua déjà compilé d'un
objet (cache par hash de source, `script_key`), expose `obj` (position,
rotation, échelle, couleur, `tapped`/`triggered`/`exited`, `anim`), `dt`,
`time`, `input`, et une API : `emit`/`on_event` (événements de gameplay,
décalés d'un tick), `spawn`/`obj:destroy()` (accumulés, appliqués après la
boucle des scripts), `save.get`/`save.set` (état persistant, cf. section
suivante), `find_tag` (instantané pris avant la boucle), `raycast`/
`overlap_sphere` (`runtime::physics::Physics`, fermetures scopées).
`mlua` (Lua 5.4 vendored), VM unique par `AppState` (champ `lua`).

## Règles de sauvegarde/export

`src/runtime/savegame.rs::SaveGame` — versionné (`CURRENT_VERSION`, migré
comme les scènes), slots nommés (`user://save_<slot>.json`), noms de slot
validés (`valid_slot`, Sprint 105a-2 — charset restreint, redondant avec
`assets::safe_join` par conception, message d'erreur plus clair). Variantes
`_at(..., dir: &Path)` (Sprint 105a-3) pour isoler les tests du vrai
`$HOME` — même patron que `assets::register_asset_at`/`read_user_bytes_at`.

Export desktop/iOS : `src/editor/export.rs` (build/export pipeline) et
`src/app/build_config.rs::BuildConfig` (qualité de rendu/bloom visés,
persistée, relue à l'entrée en Play sans redémarrer l'app).
