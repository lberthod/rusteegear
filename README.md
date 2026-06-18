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
boîte noire. Le projet est pensé pour grandir vers le **mobile (iOS / Android)**
grâce à la portabilité de `wgpu`.

---

## 🎮 Fonctionnalités (disponibles aujourd'hui)

**Rendu & édition**
- **Rendu 3D** temps réel via `wgpu` (Metal sur macOS), shaders WGSL, depth buffer, éclairage Lambert.
- **Caméra orbitale** (clic-glisser / molette) ; présentation **vsync** (fluide).
- **Primitives** cube / sphère / plan **+ import de modèles glTF / GLB** (chargement asynchrone).
- **Éditeur `egui`** à 3 panneaux : toolbar · hiérarchie · inspecteur.
- **Sélection** par hiérarchie ou clic 3D (raycast ray/AABB), surbrillance.
- **Gizmos** translate / rotate / scale (**W / E / R**), manipulation à la souris.
- **Undo / Redo** (Cmd+Z / Cmd+Shift+Z) et **duplication** (Cmd+D).
- **Sérialisation** de la scène en JSON (Save / Load).

**Runtime de jeu** (mode Play ▶/⏹, aperçu réinitialisable)
- **Scripting Lua** par objet (`mlua`) : `obj.x/y/z`, `obj.rx/ry/rz`, `obj.sx/sy/sz`, `dt`, `time`.
- **Physique** `rapier3d` : corps Statique / Dynamique, gravité, collisions, rebond.
- **Audio** `kira` : son par objet, autoplay au Play, décodage asynchrone + cache.

**Distribution**
- **Packaging macOS** : `.dmg` distribuable.

---

## 🗓️ Historique & avancement

| Phase | Sprints | État |
|---|---|---|
| **MVP** — moteur + éditeur + `.dmg` | 0 → 6 | ✅ |
| **A** — Fondations éditeur (refactor, gizmos, glTF, undo/dup) | 7 → 10 | ✅ |
| **B** — Runtime de jeu (Lua, physique, audio) + optimisations | 11 → 13 | ✅ |
| **C** — Portage mobile (Player, tactile, iOS, Android) | 14 → 17 | ⏳ en cours |

> Détail sprint par sprint : voir **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)**.

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
WebGPU** — c'est la clé qui rend les portages mobile ci-dessous réalistes
sans réécrire le moteur. Détails et journal des sprints : voir **[PLAN.md](PLAN.md)**.

---

## 🗺️ Roadmap

> Légende : ✅ fait · 🟡 partiel · ⬜ à venir. Détail par sprint : [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md).

### ✅ v1.1 — Confort éditeur _(Phase A, sprints 7→10)_
- [x] **Gizmos translate / rotate / scale** manipulables à la souris (touches W/E/R).
- [x] **Import de modèles glTF / GLB** (crate `gltf`, chargement asynchrone + recadrage auto).
- [x] **Undo/redo** (Cmd+Z / Cmd+Shift+Z) et **duplication** (Cmd+D).
- [ ] Multi-sélection, copier/coller (reporté à un sprint dédié).
- [ ] Textures et matériaux PBR de base, ombres (shadow mapping).

### ✅ v1.2 — Runtime & scripting _(Phase B, sprints 11→13)_
- [x] **Scripting Lua** par objet (`mlua`) : `obj.x/y/z`, `obj.rx/ry/rz`, `obj.sx/sy/sz`, `dt`, `time`.
- [x] **Physique** `rapier3d` : corps Statique / Dynamique, gravité, collisions, rebond.
- [x] **Système audio** `kira` : son par objet, autoplay, décodage asynchrone + cache.
- [x] _Bonus_ : optimisations — chargement asynchrone, présentation vsync.

### 🟡 📱 v2 — Portage iOS _(Phase C, sprints 14→17 — en cours)_

> `wgpu` tourne sur Metal-iOS et `winit` sait créer une surface iOS : le moteur de
> rendu est déjà compatible. Le travail porte sur le _packaging_ et l'entrée tactile.

- [x] **Cible `aarch64-apple-ios`** : **cross-compilation complète réussie** (wgpu, winit, egui, rapier, mlua/Lua, kira → arm64).
- [x] **Packaging `.ipa`** via `packaging/build_ios.sh` (assemble `.app` + `Info.plist`) — **non signé** pour l'instant.
- [ ] Projet Xcode via `cargo-mobile2` + **signature/provisioning** (compte développeur Apple) pour installer sur device.
- [ ] **Mode « player »** plein écran sans panneaux (un jeu, pas un éditeur, sur mobile).
- [ ] **Entrées tactiles** : orbit à un doigt, pinch-to-zoom à deux doigts (events `Touch` de winit).
- [ ] Cycle de vie iOS (lancement UIKit, suspend/resume → recréation de la surface wgpu).
- _Note : « APK » concerne Android ; l'équivalent iOS est un `.ipa`. Un portage **Android**
  (`aarch64-linux-android`, backend Vulkan) suit exactement la même logique et est prévu en parallèle._

---

## 🛠️ Stack technique

| Besoin | Crate |
|---|---|
| Fenêtre / événements | `winit` |
| Rendu GPU | `wgpu` (WGSL) |
| Maths | `glam` |
| UI éditeur | `egui` + `egui-wgpu` + `egui-winit` |
| Sérialisation | `serde` + `serde_json` |
| Import 3D | `gltf` |
| Scripting | `mlua` (Lua 5.4) |
| Physique | `rapier3d` |
| Audio | `kira` |
| Sélecteur de fichiers (desktop) | `rfd` |
| Packaging macOS | `cargo-bundle` |

---

## 📄 Licence

MIT — voir [LICENSE](LICENSE). Fais-en ce que tu veux. 🦀
