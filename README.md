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

## 🎯 Quel besoin Motor3DeRust adresse-t-il ?

Les moteurs grand public (Unity, Unreal, Godot) sont extraordinairement complets,
mais ce sont des **boîtes noires** : des millions de lignes, un runtime opaque, un
modèle de licence et de télémétrie que l'on subit, et une courbe d'apprentissage qui
porte sur *l'outil* plus que sur *les concepts*. Quand on veut **comprendre comment
un moteur fonctionne réellement** — comment un vertex part d'un `Vec<f32>` pour finir
en pixel à l'écran, comment un raycast sélectionne un objet, comment une boucle de
simulation reste stable — ces moteurs cachent justement ce qui est intéressant.

Motor3DeRust répond à un besoin précis :

- **Pédagogique & maîtrise totale.** Chaque étage du pipeline (fenêtre → événements →
  état → rendu GPU → UI) est écrit à la main, lisible en une après-midi, sans
  abstraction magique. C'est un moteur que l'on peut **tenir entièrement dans sa tête**.
- **Hackable et minimal.** ~2 500 lignes de Rust au total. Ajouter une primitive, un
  type de collider ou une variable de script se fait en quelques lignes, sans se battre
  contre un framework.
- **Portable par conception.** La logique (scène, caméra, entrées, picking) ne dépend
  **pas** du GPU ; seule la couche `gfx/` parle à `wgpu`. C'est ce découpage qui a permis
  de porter l'app sur **iOS et Android** sans réécrire le cœur.
- **Sans dépendance lourde ni runtime caché.** Pas de garbage collector, pas de moteur
  embarqué, pas de licence à négocier — un seul binaire natif.

Ce n'est **pas** un concurrent d'Unity. C'est un **socle compréhensible** pour
apprendre, prototyper et expérimenter le rendu temps réel et l'architecture moteur.

---

## 🦀 Pourquoi Rust ?

Un moteur de jeu cumule les contraintes que Rust adresse le mieux :

- **Performance native, prévisible.** Le rendu temps réel et la physique exigent un
  contrôle fin de la mémoire et zéro pause GC. Rust offre les performances du C/C++
  (pas de runtime, pas de ramasse-miettes, *zero-cost abstractions*) tout en restant
  expressif.
- **Sécurité mémoire sans coût à l'exécution.** Le *borrow checker* élimine à la
  compilation les classes de bugs qui hantent les moteurs C++ (use-after-free, data
  races, pointeurs pendants). Sur un système concurrent (rendu + chargements async +
  audio + physique), c'est décisif : ici, l'import glTF, le décodage audio et le
  chargement de scène tournent **sur des threads de fond** en toute sûreté, garantie
  par le type system (`Send`/`Sync`).
- **Un écosystème graphique de premier plan.** L'essentiel de la stack est écrit *en*
  Rust et de grande qualité : `wgpu` (abstraction GPU moderne : Metal / Vulkan / DX12 /
  WebGPU), `winit` (fenêtrage multiplateforme), `egui` (UI immédiate), `glam` (maths
  SIMD), `rapier3d` (physique), `kira` (audio). On bénéficie d'un alignement rare entre
  le langage et ses bibliothèques.
- **Portabilité réelle.** Un même cœur compile vers macOS, un `.so` Android (`cdylib` +
  `android_main`) et un binaire iOS — `cargo` et l'abstraction `wgpu` font le gros du
  travail.
- **Outillage moderne.** `cargo` (build, dépendances, tests), `clippy` (lints),
  `rustfmt` (format) et un système de modules clair rendent un projet de cette taille
  agréable à maintenir — et faciles à valider en CI.

En résumé : Rust permet d'écrire un moteur **bas niveau et performant** tout en gardant
la **fiabilité** et le **confort de développement** qu'on attendrait d'un langage de
plus haut niveau.

---

## ⚖️ From scratch sur Rust — et pas sur Bevy ?

[Bevy](https://bevyengine.org/) est l'excellent moteur de jeu de l'écosystème Rust :
ECS complet, ordonnanceur de systèmes, rendu PBR, plugins… Si l'objectif était de
**produire un jeu** le plus vite possible, Bevy (ou Godot, Fyrox) serait un choix
parfaitement légitime — et probablement supérieur.

Mais l'objectif de Motor3DeRust est exactement l'inverse : **comprendre et maîtriser le
moteur lui-même**. Or s'appuyer sur Bevy reviendrait à remplacer une boîte noire
(Unity) par une autre, certes en Rust. On hériterait de son ECS, de son ordonnanceur,
de son pipeline de rendu et de ses choix d'architecture — c'est-à-dire de précisément
ce que ce projet cherche à écrire à la main pour l'apprendre.

| Critère | Motor3DeRust (from scratch) | Bevy |
|---|---|---|
| **Objectif** | Comprendre/maîtriser un moteur | Produire des jeux efficacement |
| **Taille du cœur** | ~2 500 lignes, lisible d'un bout à l'autre | Très large, nombreux sous-systèmes |
| **Architecture** | Scène = `Vec<SceneObject>`, explicite | ECS complet + ordonnanceur de systèmes |
| **Rendu** | Pipeline `wgpu`/WGSL écrit à la main | Moteur de rendu intégré (PBR, etc.) |
| **Courbe d'apprentissage** | On apprend *les concepts* | On apprend *le framework* |
| **Contrôle** | Total (chaque ligne est à soi) | Cadré par les conventions du moteur |
| **Productivité jeu** | Faible (tout est à construire) | Élevée |
| **Boîte noire** | Aucune | Le moteur lui-même |

Concrètement, Motor3DeRust ne s'appuie **que** sur des briques *ciblées et
remplaçables* (`winit` pour la fenêtre, `wgpu` pour le GPU, `egui` pour l'UI,
`rapier3d`/`kira`/`mlua` pour le runtime) et **assemble lui-même** la boucle
d'événements, le pipeline de rendu, le picking, les gizmos, la sérialisation et le
mode Play. C'est ce qui rend la **comparaison pertinente** : on choisit la dépendance
pour *un problème précis et bien délimité*, jamais pour la structure générale du
moteur — qui, elle, reste l'objet même de l'apprentissage.

> En une phrase : **Bevy est un moteur ; Motor3DeRust est l'exercice consistant à en
> écrire un.** Les deux sont en Rust ; seul le second t'apprend ce qu'il y a dedans.

---

## 🎮 Fonctionnalités (disponibles aujourd'hui)

**Rendu & édition**
- **Rendu 3D** temps réel via `wgpu` (Metal sur macOS), shaders WGSL, depth buffer, éclairage Lambert paramétré.
- **Matériaux** : teinte (albédo) par objet + **éclairage de scène éditable** (direction, couleur, ambiante).
- **Caméra orbitale** (clic-glisser / molette) ; présentation **vsync** + cadence adaptative (throttle CPU au repos).
- **Primitives** cube / sphère / plan **+ import de modèles glTF / GLB** (chargement asynchrone).
- **Éditeur `egui`** : toolbar · hiérarchie · inspecteur · bandeau d'état (FPS, objets, mode, backend GPU).
- **Hiérarchie ergonomique** : **groupes définis par l'utilisateur** (glisser-déposer), filtre de recherche, icônes & badges (physique/script/audio).
- **Sélection** par hiérarchie ou clic 3D ; **multi-sélection** (Cmd/Maj+clic), **renommage inline** (double-clic).
- **Gizmos** translate / rotate / scale (**W / E / R**), manipulation à la souris.
- **Undo / Redo** (Cmd+Z / Cmd+Shift+Z), **copier/coller** (Cmd+C/V), **dupliquer** (Cmd+D), **supprimer** (Suppr) — en lot.
- **Sérialisation** de la scène en JSON (Save / Load asynchrone).

**Runtime de jeu** (mode Play ▶/⏹, aperçu réinitialisable)
- **Scripting Lua** par objet (`mlua`, chunks compilés en cache) : `obj.x/y/z`, `obj.rx/ry/rz`, `obj.sx/sy/sz`, `dt`, `time`.
- **Physique** `rapier3d` : corps Statique / Dynamique, gravité, collisions, rebond.
- **Audio** `kira` : son par objet, autoplay au Play, décodage asynchrone + cache.

**Build & Export 1-clic** (panneau 📦 dans l'éditeur)
- Exporte un **player jouable du jeu créé** en `.dmg` / `.apk` / `.ipa` — **scène et assets embarqués** dans le binaire.
- **Config persistée** (nom, bundle id, version, build #) + **préréglages** ; identité appliquée au bundle macOS.
- **Pré-vol** des toolchains (cargo-bundle / cargo-apk+NDK / Xcode), **install sur device** (adb / devicectl), log streamé, « Tout exporter ».

**Plateformes**
- **macOS** (éditeur, `.dmg`), **Android** (`.apk`), **iOS** (sur iPhone) — mode player tactile sur mobile (avec resume).

---

## 🗓️ Historique & avancement

| Phase | Sprints | État |
|---|---|---|
| **MVP** — moteur + éditeur + `.dmg` | 0 → 6 | ✅ |
| **A** — Fondations éditeur (refactor, gizmos, glTF, undo/dup) | 7 → 10 | ✅ |
| **B** — Runtime de jeu (Lua, physique, audio) + optimisations | 11 → 13 | ✅ |
| **C** — Portage mobile (Player, tactile, iOS, Android) | 14 → 17 | ✅ |
| **D** — App de dev & exports 1-clic (perf, panneau Export, config, presets, CI) | 18 → 23 | ✅ |
| **E** — Player complet & maturité (assets embarqués, multi-sélection, matériaux, resume) | 24 → 27 | 🟢 cœur |

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
├── lib.rs         # event loop winit + run() (desktop) + android_main (cdylib) + resume mobile
├── main.rs        # entrée desktop → motor3derust::run()
├── assets.rs      # assets embarqués (include_dir, schéma bundle://) pour le player exporté
├── app/           # logique sans GPU : AppState, picking, sélection, build_config
├── gfx/           # couche rendu wgpu (renderer, mesh, camera, gizmo, shaders WGSL)
├── scene/         # Transform, MeshKind, Scene, groupes, lumière, import glTF, sérialisation
├── runtime/       # mode Play : physics (rapier3d), audio (kira)
└── editor/        # UI egui (toolbar, hiérarchie, inspecteur, panneau export) — desktop
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

### ✅ 🛠️ v2.5 — App de dev & exports 1-clic _(Phase D, sprints 18→23)_
- [x] **Optimisations app** : profils Cargo (LTO), bandeau d'état FPS/GPU, cadence adaptative.
- [x] **Panneau « Build & Export »** : `.dmg`/`.apk`/`.ipa` depuis des boutons, log streamé, pré-vol.
- [x] **Config persistée** (nom, bundle id, version, build #) + **préréglages** + install device.
- [x] **CI de release** : tag `v*` → artefacts macOS/Android attachés à la Release.

### 🟢 🎮 v3 — Player complet & maturité _(Phase E, sprints 24→27)_
- [x] **Assets embarqués** dans le player (glTF + sons) → jouable hors développement.
- [x] **Édition** : multi-sélection (Cmd/Maj), copier/coller en lot, renommage inline.
- [x] **Matériaux** : couleur par objet + éclairage de scène éditable.
- [x] **Resume mobile** (recréation de surface) + **identité de bundle macOS**.
- [ ] Reste : textures & **ombres** (shadow mapping), multi-sélection au clic 3D, sous-groupes.

### ⬜ Pistes futures
- [ ] Textures / matériaux PBR, **ombres** (shadow mapping).
- [ ] Multi-sélection au clic 3D, réordonnancement & sous-groupes dans la hiérarchie.
- [ ] Override d'identité Android, **IPA signé en CI** (secrets), signature *distribution* store.

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
| Assets embarqués (player) | `include_dir` |
| Sélecteur de fichiers (desktop) | `rfd` |
| Packaging | `cargo-bundle` (macOS) · `cargo-apk` (Android) · `xcodegen`+Xcode (iOS) |

> Export depuis l'éditeur : voir **[packaging/EXPORT.md](packaging/EXPORT.md)**.

---

## 📄 Licence

MIT — voir [LICENSE](LICENSE). Fais-en ce que tu veux. 🦀
