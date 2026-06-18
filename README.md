<div align="center">

# 🦀 Motor3DeRust

**Un moteur / éditeur de jeu 3D minimaliste « à la Unity », écrit _from scratch_ en Rust.**

winit · wgpu · egui — aucun moteur tiers.

![langage](https://img.shields.io/badge/Rust-1.95-orange?logo=rust)
![plateforme](https://img.shields.io/badge/macOS-.dmg-black?logo=apple)
![rendu](https://img.shields.io/badge/wgpu-Metal%20%7C%20Vulkan%20%7C%20Metal--iOS-blue)
![statut](https://img.shields.io/badge/MVP-complet-success)
![licence](https://img.shields.io/badge/licence-MIT-green)

</div>

---

## ✨ Vision

Motor3DeRust est un éditeur de jeu 3D léger et hackable. L'objectif n'est pas de
remplacer Unity, mais d'offrir une base **comprenable de bout en bout** : chaque
ligne du pipeline de rendu, de l'ECS-léger et de l'UI est écrite à la main, sans
boîte noire. Le projet est pensé pour grandir vers le **mobile (iOS)** et la
**réalité virtuelle (Oculus / Meta Quest)** grâce à la portabilité de `wgpu`.

---

## 🎮 Fonctionnalités (MVP — disponible aujourd'hui)

- **Rendu 3D** temps réel via `wgpu` (Metal sur macOS), shaders WGSL, depth buffer,
  éclairage Lambert (directionnel + ambiant).
- **Caméra orbitale** : clic-glisser pour tourner, molette pour zoomer.
- **Primitives** générées par code : cube, sphère (UV), plan.
- **Éditeur `egui`** à 3 panneaux : toolbar · hiérarchie · inspecteur.
- **Sélection** par la hiérarchie **ou par clic direct dans la vue 3D** (raycast ray/AABB).
- **Édition** complète du `Transform` : position / rotation / échelle, renommage, suppression.
- **Mode Play** (▶ / ⏹) : anime la scène en _delta-time_.
- **Sérialisation** de la scène en JSON (Save / Load).
- **Packaging macOS** : génération d'un `.dmg` distribuable.

---

## 🚀 Démarrage rapide

```bash
# Lancer en développement
cargo run

# Produire le .dmg macOS
cargo install cargo-bundle      # une seule fois
./packaging/build_dmg.sh
# → target/release/bundle/dmg/Motor3DeRust.dmg
```

> ⚠️ Le `.dmg` n'est pas signé. Premier lancement : **clic droit ▸ Ouvrir** (Gatekeeper).

### Commandes dans l'éditeur

| Action | Commande |
|---|---|
| Tourner la caméra | clic gauche + glisser (sur la vue 3D) |
| Zoomer | molette |
| Sélectionner un objet | clic sur l'objet, ou dans la hiérarchie |
| Ajouter un objet | boutons Cube / Sphère / Plan |
| Éditer / supprimer | panneau Inspecteur (droite) |
| Lancer / arrêter l'animation | ▶ Play / ⏹ Stop |
| Sauver / charger | 💾 Save / 📂 Load (`~/motor3derust_scene.json`) |

---

## 🧱 Architecture

```
src/
├── main.rs        # event loop winit + routage des événements
├── gfx/           # couche rendu wgpu
│   ├── renderer.rs    # surface, pipeline, depth, passes, picking, mode Play
│   ├── mesh.rs        # Vertex, GpuMesh, génération cube/sphère/plan
│   ├── camera.rs      # caméra orbitale (matrices view/proj)
│   └── shaders/main.wgsl
├── scene/         # modèle de scène sans ECS lourd
│   └── mod.rs         # Transform, MeshKind, SceneObject, Scene + sérialisation
└── editor/        # UI egui (toolbar, hiérarchie, inspecteur)
```

Le rendu repose entièrement sur `wgpu`, qui cible **Metal, Vulkan, DX12, OpenGL et
WebGPU** — c'est la clé qui rend les portages mobile et VR ci-dessous réalistes
sans réécrire le moteur. Détails et journal des sprints : voir **[PLAN.md](PLAN.md)**.

---

## 🗺️ Roadmap

### v1.1 — Confort éditeur
- [ ] Gizmos de translation/rotation manipulables à la souris dans la vue 3D.
- [ ] Import de modèles **glTF** (crate `gltf`).
- [ ] Multi-sélection, copier/coller, undo/redo.
- [ ] Textures et matériaux PBR de base, ombres (shadow mapping).

### v1.2 — Runtime & scripting
- [ ] Système de composants / scripts (WASM ou Lua via `mlua`).
- [ ] Physique simple (collisions AABB → intégration `rapier3d`).
- [ ] Système audio (`kira` / `rodio`).

### 📱 v2 — Portage iOS

> `wgpu` tourne sur Metal-iOS et `winit` sait créer une surface iOS : le moteur de
> rendu est déjà compatible. Le travail porte sur le _packaging_ et l'entrée tactile.

- [ ] Cible `aarch64-apple-ios` + `cargo-xcodebuild` / `cargo-mobile2` pour générer le projet Xcode.
- [ ] Adapter l'éditeur : l'UI `egui` reste, mais prévoir un **mode « player »** plein écran sans panneaux (un jeu, pas un éditeur, sur mobile).
- [ ] **Entrées tactiles** : orbit à un doigt, pinch-to-zoom à deux doigts (events `Touch` de winit).
- [ ] Boucle de rendu pilotée par `CADisplayLink`, gestion du cycle de vie iOS (suspend/resume → recréation de la surface wgpu).
- [ ] Signature & déploiement via Xcode (compte développeur Apple requis).
- _Note : « APK » concerne Android ; l'équivalent iOS est un `.ipa`. Un portage **Android**
  (`aarch64-linux-android`, backend Vulkan) suit exactement la même logique et est prévu en parallèle._

### 🥽 v2 — Réalité virtuelle (Oculus / Meta Quest)

> La VR se branche via **OpenXR**, le standard ouvert que supportent les casques Meta Quest.
> `wgpu` peut partager ses textures avec OpenXR (interop via `wgpu-hal` / `ash`).

- [ ] Intégrer **OpenXR** (crate `openxr`) : création de session, espaces de référence, swapchains.
- [ ] **Rendu stéréo** : une vue par œil, deux matrices view/projection fournies par le runtime XR (remplace la caméra orbitale).
- [ ] Boucle XR : `xrWaitFrame` / `xrBeginFrame` / `xrEndFrame`, synchronisation avec la boucle wgpu.
- [ ] Suivi des **contrôleurs** (poses + boutons) pour sélectionner/déplacer les objets en VR.
- [ ] Cible **Meta Quest** (Android + OpenXR loader Oculus) ; cible PCVR via SteamVR/OpenXR sur desktop.
- [ ] Confort VR : téléportation, vignettage anti-nausée.

---

## 🛠️ Stack technique

| Besoin | Crate |
|---|---|
| Fenêtre / événements | `winit` |
| Rendu GPU | `wgpu` (WGSL) |
| Maths | `glam` |
| UI éditeur | `egui` + `egui-wgpu` + `egui-winit` |
| Sérialisation | `serde` + `serde_json` |
| Packaging macOS | `cargo-bundle` |
| _(prévu)_ Import 3D · Physique · VR | `gltf` · `rapier3d` · `openxr` |

---

## 📄 Licence

MIT — voir [LICENSE](LICENSE). Fais-en ce que tu veux. 🦀
