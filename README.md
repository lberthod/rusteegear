<div align="center">

# 🦀 Motor3DeRust

**Un moteur / éditeur de jeu 3D minimaliste « à la Unity », écrit _from scratch_ en Rust.**

winit · wgpu · egui — aucun moteur tiers.

![langage](https://img.shields.io/badge/Rust-1.95-orange?logo=rust)
![plateformes](https://img.shields.io/badge/macOS%20·%20Android%20·%20iOS-qui%20tournent-success?logo=apple)
![rendu](https://img.shields.io/badge/wgpu-Metal%20%7C%20Vulkan-blue)
![licence](https://img.shields.io/badge/licence-MIT-green)

**Tourne réellement sur les 3 plateformes** : éditeur complet sur macOS,
mode « player » tactile sur iPhone et Android.

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

**Plateformes**
- **macOS** (éditeur, `.dmg`), **Android** (`.apk`), **iOS** (sur iPhone) — mode player tactile sur mobile.

---

## 🗓️ Historique & avancement

| Phase | Sprints | État |
|---|---|---|
| **MVP** — moteur + éditeur + `.dmg` | 0 → 6 | ✅ |
| **A** — Fondations éditeur (refactor, gizmos, glTF, undo/dup) | 7 → 10 | ✅ |
| **B** — Runtime de jeu (Lua, physique, audio) + optimisations | 11 → 13 | ✅ |
| **C** — Portage mobile (Player, tactile, iOS, Android) | 14 → 17 | ✅ |

> Détail sprint par sprint : voir **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)**.

### Plateformes — état honnête

| Cible | Livrable | Statut |
|---|---|---|
| **macOS** | `.dmg` (éditeur complet) | ✅ fonctionne — non signé (clic droit ▸ Ouvrir) |
| **Android** | `.apk` signé (arm64-v8a) | ✅ s'installe (`adb install`) et tourne en mode player |
| **iOS** | app signée, installée sur iPhone | ✅ tourne (scène animée + tactile) — signature développeur **personnelle** (pas App Store) |

L'**éditeur** (panneaux egui, gizmos, inspecteur) est **desktop**. Sur mobile, l'app
démarre en **mode player** : la scène jouable plein écran, caméra au doigt (1 doigt =
orbite, 2 doigts = zoom). iOS/Android ne sont pas signés pour une distribution store.

---

## 🚀 Démarrage rapide

```bash
cargo run                       # éditeur desktop
cargo run -- --player           # mode player (scène plein écran)
```

### Builds par plateforme
```bash
# macOS (.dmg) — cargo install cargo-bundle
./packaging/build_dmg.sh        # → target/release/bundle/dmg/Motor3DeRust.dmg

# Android (.apk) — NDK + cargo install cargo-apk (voir packaging/build_android.md)
./packaging/build_apk.sh        # → target/release/apk/motor3derust.apk

# iPhone branché — Xcode + brew install xcodegen (voir packaging/build_ios.md)
./packaging/install_ios_device.sh   # build + signature auto + install + lancement
```

> ⚠️ Aucune cible n'est signée pour distribution store. Le `.dmg` n'est pas signé
> (clic droit ▸ Ouvrir) ; l'`.apk` est signé clé debug ; l'iOS utilise votre certificat
> de développement personnel (installe sur un appareil enregistré).

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
├── lib.rs         # event loop winit + run() (desktop) + android_main (cdylib)
├── main.rs        # entrée desktop → motor3derust::run()
├── app/           # logique sans GPU : AppState, entrées agnostiques, picking
├── gfx/           # couche rendu wgpu (renderer, mesh, camera, gizmo, shaders WGSL)
├── scene/         # Transform, MeshKind, Scene, import glTF, sérialisation
├── runtime/       # mode Play : physics (rapier3d), audio (kira)
└── editor/        # UI egui (toolbar, hiérarchie, inspecteur) — desktop
```

Séparation nette **logique (`app`) / rendu (`gfx`)** : l'état (scène, caméra, entrées)
ne dépend pas du GPU, ce qui a rendu le portage mobile direct. Le rendu repose sur
`wgpu` (Metal / Vulkan / DX12 / WebGPU) — la clé de la portabilité.
Détails et journal : **[PLAN.md](PLAN.md)** · **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)**.

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

### ✅ 📱 v2 — Portage mobile _(Phase C, sprints 14→17)_
- [x] **Mode « player »** plein écran sans panneaux (auto sur mobile).
- [x] **Entrées tactiles** : orbit à un doigt, pinch-to-zoom à deux doigts.
- [x] **iOS** : cross-compilation, signature développeur, **installé et lancé sur iPhone**.
- [x] **Android** : NDK, `cdylib` + `android_main`, **`.apk` signé** via `cargo-apk`.
- [ ] Reste : signature *distribution* (App Store / Play Store), icônes, écran de lancement.

### ⬜ Pistes futures
- [ ] Multi-sélection, copier/coller.
- [ ] Textures / matériaux PBR, ombres (shadow mapping).
- [ ] Recréation de surface au resume mobile (suspend/resume).

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
| Packaging | `cargo-bundle` (macOS) · `cargo-apk` (Android) · `xcodegen`+Xcode (iOS) |

---

## 📄 Licence

MIT — voir [LICENSE](LICENSE). Fais-en ce que tu veux. 🦀
