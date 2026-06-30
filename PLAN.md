# RusteeGear — Moteur de jeu 3D (style Unity, très basique) en Rust

> ⚠️ **ARCHIVE — cadrage MVP initial (Sprints 0→6, daté 2026-06-18).**
> Ce document fige la vision et le périmètre *de départ*. Plusieurs « hors périmètre v1 »
> (import glTF, scripting Lua, physique, audio, ombres, mobile) ont **depuis été réalisés**.
> Pour l'état **actuel** + la suite : **[README.md](README.md)**, **[SPRINTS.md](SPRINTS.md)**,
> **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)**. Conservé comme trace historique.

> Document de cadrage : analyse, architecture, plan de sprints.
> Cible : application macOS distribuable en `.dmg`.
> Date : 2026-06-18.

---

## 1. Analyse & objectifs

### Vision
Un **éditeur de jeu 3D minimaliste** à la Unity : une fenêtre avec un viewport 3D,
une hiérarchie de scène, un inspecteur de propriétés, et la possibilité de placer
des objets 3D simples, les déplacer, et lancer une « simulation » (mode Play).

### Périmètre v1 (MVP) — ce qu'on fait
- Fenêtre native macOS + rendu 3D temps réel.
- Caméra orbitale (rotation/zoom/pan à la souris).
- Primitives 3D : cube, sphère, plan.
- Scène = arbre d'entités avec `Transform` (position, rotation, échelle).
- Éditeur : panneau Hiérarchie, panneau Inspecteur, viewport central.
- Sélection d'objet + gizmo de déplacement basique.
- Éclairage simple (1 lumière directionnelle + ambiante).
- Sauvegarde/chargement de scène en JSON.
- Mode Play : appliquer une gravité/rotation simple aux objets scriptés.
- Packaging `.dmg`.

### Hors périmètre v1 — ce qu'on NE fait PAS (volontairement)
- Pas d'import de modèles (glTF/FBX) → repoussé v2.
- Pas de physique réaliste (moteur de collisions complet).
- Pas de système de scripting utilisateur (Lua/WASM) → v2.
- Pas de PBR/ombres avancées.
- Pas d'audio, pas de réseau, pas de build de jeu autonome.

### Risques & contraintes
| Risque | Impact | Mitigation |
|---|---|---|
| Courbe wgpu/rendu | Élevé | S'appuyer sur **Bevy** (ECS + rendu prêts) plutôt que tout coder |
| Gizmos/picking 3D | Moyen | Crate `bevy_mod_picking` / raycast simple |
| Signature/notarisation macOS | Moyen | DMG non signé pour dev ; doc pour signature plus tard |
| Scope creep « comme Unity » | Élevé | MVP strict ci-dessus, le reste en backlog |

---

## 2. Choix techniques — **FROM SCRATCH (winit + wgpu)**

> Décision : on code le moteur à la main, sans Bevy. Plus long mais formateur ;
> contrôle total du rendu. On s'appuie quand même sur des crates fondamentales.

| Besoin | Choix | Pourquoi |
|---|---|---|
| Langage | Rust 1.95 | Demandé |
| Fenêtre / event loop | **winit** | Standard Rust, multiplateforme, natif macOS |
| Rendu GPU | **wgpu** | API moderne (Metal sous macOS), shaders WGSL |
| Maths (vecteurs/matrices) | **glam** | Mat4, Vec3, quaternions — base caméra/transform |
| UI éditeur | **egui** + `egui-wgpu` + `egui-winit` | Panneaux immédiats sans dépendre de Bevy |
| Sérialisation scène | `serde` + `serde_json` | Standard Rust |
| Chargement textures | `image` | PNG/JPEG si besoin |
| Logs | `env_logger` + `log` | Debug wgpu |
| Packaging macOS | `cargo-bundle` puis `hdiutil` | `.app` → `.dmg` |

### Ce qu'il faut coder soi-même (le vrai travail)
- **Pipeline de rendu wgpu** : surface, swapchain, depth buffer, render pass.
- **Shaders WGSL** : vertex (MVP matrix) + fragment (éclairage Lambert simple).
- **Mini-ECS maison** ou simple `Vec<Entity>` (pas besoin d'un ECS complet pour le MVP).
- **Génération de meshes** : cube/sphère/plan (vertices + indices à la main).
- **Caméra** : matrices view/projection, contrôle orbital.
- **Uniform buffers / bind groups** : envoi des matrices et lumières au GPU.
- **Picking** : raycast souris → objet (test ray/AABB).

---

## 3. Architecture

```
motor3derust/
├── Cargo.toml
├── PLAN.md
├── assets/                 # icônes, futurs modèles
├── packaging/
│   ├── Info.plist
│   ├── icon.icns
│   └── make_dmg.sh         # build .app -> .dmg
└── src/
    ├── main.rs             # event loop winit, init wgpu, boucle principale
    ├── gfx/                # COUCHE RENDU (wgpu)
    │   ├── mod.rs
    │   ├── renderer.rs     # surface, device/queue, render pass, depth
    │   ├── pipeline.rs     # render pipeline + bind groups
    │   ├── mesh.rs         # Vertex, GpuMesh (vertex/index buffers)
    │   ├── camera.rs       # matrices view/proj + OrbitCamera
    │   └── shaders/main.wgsl  # vertex + fragment (Lambert)
    ├── scene/
    │   ├── mod.rs          # Scene { objects: Vec<SceneObject> }
    │   ├── object.rs       # SceneObject { name, transform, mesh_kind }
    │   ├── transform.rs    # Transform (pos/rot/scale) -> Mat4
    │   ├── primitives.rs   # mesh data cube/sphère/plan (CPU)
    │   └── serialization.rs# save/load JSON (serde)
    ├── editor/             # UI egui
    │   ├── mod.rs          # intégration egui-wgpu / egui-winit
    │   ├── hierarchy.rs    # liste des objets
    │   ├── inspector.rs    # édition du Transform sélectionné
    │   └── toolbar.rs      # Add / Play / Stop / Save / Load
    ├── input/
    │   ├── mod.rs
    │   └── picking.rs      # raycast souris -> objet (ray/AABB)
    └── runtime/
        ├── mod.rs          # mode Play
        └── behaviors.rs    # gravité / rotation simples
```

### Modèle de données (cœur, sans ECS)
```rust
struct Transform { position: Vec3, rotation: Quat, scale: Vec3 } // -> Mat4
enum MeshKind { Cube, Sphere, Plane }
struct SceneObject { name: String, transform: Transform, mesh: MeshKind }
struct Scene { objects: Vec<SceneObject> }     // sérialisable JSON
struct AppState { mode: Edit | Play, selection: Option<usize> }
```
> Pas besoin d'un ECS complet pour le MVP : un `Vec<SceneObject>` indexé suffit.
> Le GPU garde une `GpuMesh` par `MeshKind`, dessinée via une matrice modèle par objet.

### Flux (boucle principale winit)
```
winit event ─┬─▶ input::picking ─▶ AppState.selection
             ├─▶ OrbitCamera (drag / zoom)
             └─▶ egui (Hierarchy/Inspector/Toolbar) lit/écrit Scene

RedrawRequested ─▶ renderer (render pass)
                     ├─ par objet : upload matrice MVP -> draw GpuMesh
                     └─ egui render pass par-dessus
Toolbar Play ─▶ AppState.mode = Play ─▶ runtime::behaviors mute les Transform
Toolbar Save ─▶ serialization::save(&scene, "scene.json")
```

---

## 4. Plan de sprints (from scratch — 7 sprints)

> Le rendu manuel ajoute ~2 sprints vs une approche Bevy. Sprints 1–2 = le cœur du défi.

### Sprint 0 — Fenêtre & boucle (½ j) ✅ FAIT
- [x] `cargo init`, dépendances : `winit`, `wgpu`, `glam`, `pollster`, `env_logger`.
- [x] Fenêtre winit + event loop, surface wgpu effacée à une couleur.
- **Livrable** : fenêtre qui s'ouvre et se redimensionne sans crash.

### Sprint 1 — Premier cube à l'écran ✅ FAIT
- [x] Render pipeline + shader WGSL (`gfx/shaders/main.wgsl`), vertex/index buffers (`gfx/mesh.rs`).
- [x] Depth buffer (Depth32Float), matrices via uniform buffers + bind groups.
- [x] Génération mesh cube (24 verts / 36 indices, normales par face).
- **Livrable** : un cube 3D coloré orange, en perspective, qui tourne. ✅

### Sprint 2 — Caméra, primitives & lumière ✅ FAIT
- [x] `OrbitCamera` : drag souris = orbit (yaw/pitch), molette = zoom (distance clampée).
- [x] Génération sphère (UV) + plan ; module `scene` (Transform, MeshKind, Scene).
- [x] Dessin de N objets : un `GpuMesh` par type, une matrice modèle par objet.
- [x] Éclairage Lambert (directionnelle + ambiante) déjà en place depuis le Sprint 1.
- **Livrable** : scène de démo (sol + cube + sphère), navigable à la souris. ✅

### Sprint 3 — Intégration egui ✅ FAIT
- [x] Brancher `egui-winit` + `egui-wgpu` (passe egui après la passe scène).
- [x] Layout : toolbar (haut), hiérarchie (gauche), inspecteur (droite).
- [x] Boutons Add Cube/Sphere/Plane → ajoutent dans `Scene.objects` (+ sync GPU).
- [x] Bonus (avance sur Sprint 4) : sélection via hiérarchie + surbrillance, inspecteur
      éditant position/rotation/échelle, renommage, suppression.
- **Livrable** : éditeur complet à 3 panneaux, ajout/édition/suppression d'objets. ✅
- Note : warnings de dépréciation egui 0.34 (ancienne API panneaux) — à migrer plus tard.

### Sprint 4 — Sélection & édition ✅ FAIT
- [x] Picking : raycast depuis la souris (unproject NDC), test ray/AABB → `selection`.
- [x] Surbrillance jaune de l'objet sélectionné (param dans l'uniform modèle).
- [x] Inspecteur (édition transform/rename/delete) déjà fait au Sprint 3.
- **Livrable** : cliquer un objet dans la vue 3D le sélectionne. ✅

### Sprint 5 — Sérialisation & mode Play ✅ FAIT
- [x] `serde` + `serde_json` : `Scene::save`/`load` (scene.json) + boutons toolbar.
- [x] Mode Play (toggle ▶/⏹) + `advance_play` (rotation des objets) avec delta-time.
- **Livrable** : scène persistante + bouton Play animant la scène. ✅
- Note technique : bug corrigé — acquérir la surface avant de lancer egui, sinon le
  `textures_delta` (atlas de police) est jeté → panic intermittente d'egui-wgpu.

### Sprint 6 — Packaging macOS ✅ FAIT
- [x] `cargo-bundle` + métadonnées `[package.metadata.bundle]` → `.app` + `Info.plist`.
- [x] `packaging/build_dmg.sh` → `RusteeGear.dmg` (4.5 Mo).
- [x] README utilisateur (commandes + contournement Gatekeeper).
- [x] Correctif : save/load vers `~/motor3derust_scene.json` (cwd = `/` en mode .app).
- **Livrable** : `target/release/bundle/dmg/RusteeGear.dmg`, `.app` lancée et vérifiée. ✅
- (Icône `.icns` non fournie — optionnelle ; signature/notarisation = étape ultérieure.)

---

## 5. Packaging `.dmg` (procédure cible)
```bash
cargo install cargo-bundle           # une fois
cargo bundle --release               # produit target/release/bundle/osx/RusteeGear.app
./packaging/make_dmg.sh              # produit RusteeGear.dmg
```
> Note : un `.dmg` non signé déclenchera Gatekeeper (clic droit ▸ Ouvrir).
> Signature + notarisation = étape ultérieure (compte développeur Apple requis).

---

## 6. Definition of Done (MVP)
- L'app s'ouvre depuis le `.dmg` sur macOS (Apple Silicon).
- On ajoute/sélectionne/déplace/supprime des objets.
- On sauvegarde puis recharge une scène.
- Le mode Play anime visiblement la scène.
- `cargo clippy` sans warning bloquant.
