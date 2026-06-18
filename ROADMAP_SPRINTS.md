# Motor3DeRust — Plan de sprints d'exécution (post-MVP)

> Feuille de route **étape par étape** pour faire évoluer le moteur du MVP actuel
> jusqu'au mobile (iOS/Android) et à la VR (Oculus/Quest).
> Chaque sprint a : **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**.
> Convention : un sprint ≈ 1 à 3 jours. On ne démarre un sprint que si le précédent
> est « vert » (livrable validé).

État de départ : **MVP complet** (Sprints 0→6, voir [PLAN.md](PLAN.md)).

---

## 🧭 Vue d'ensemble des phases

| Phase | Sprints | But |
|---|---|---|
| **A — Fondations éditeur** | 7 → 10 | Rendre l'éditeur réellement utilisable (gizmos, glTF, undo, multi-objets) |
| **B — Runtime de jeu** | 11 → 13 | Transformer la scène en « jeu » (scripting, physique, audio) |
| **C — Portage mobile** | 14 → 17 | iOS d'abord, puis Android |
| **D — Réalité virtuelle** | 18 → 21 | OpenXR, rendu stéréo, contrôleurs, cible Quest |

> Phases A et B améliorent le cœur **partagé** par toutes les plateformes.
> Les faire avant C/D évite de réécrire des features sur 3 cibles.

---

## PHASE A — Fondations éditeur

### Sprint 7 — Refactor : séparer `App`, `Renderer` et `Scene` ✅ FAIT
**Objectif** : isoler la logique GPU de l'état applicatif pour préparer le multi-plateforme.
- [x] `AppState` (scène, sélection, mode Play, caméra, interaction, picking) dans `src/app/mod.rs`, sans GPU.
- [x] `Renderer` (renommé depuis `State`) ne porte que le GPU + egui ; `render(&mut AppState)`.
- [x] `InputEvent` agnostique (`src/app/input.rs`) ; `main.rs` traduit winit → `InputEvent`.
- **Fichiers** : `src/gfx/renderer.rs`, `src/app/mod.rs`, `src/app/input.rs`, `src/main.rs`.
- **Livrable** : comportement identique au MVP, build sans warning, 3 démarrages OK. ✅

### Sprint 8 — Gizmo de manipulation à la souris ✅ FAIT (complet)
**Objectif** : déplacer / tourner / redimensionner un objet directement dans la vue 3D.
- [x] 3 axes X/Y/Z (lignes colorées), pipeline dédié sans depth-test ; anneaux pour la rotation.
- [x] Picking écran (~10 px) des axes (lignes) et des anneaux (polyligne projetée).
- [x] **Translate** : drag le long de l'axe (plan face-caméra).
- [x] **Rotate** : drag sur l'anneau → angle dans le plan perpendiculaire à l'axe.
- [x] **Scale** : drag le long de l'axe → composante d'échelle (min 0.05).
- [x] Bascule de mode : touches **W / E / R** + boutons toolbar ; inspecteur live.
- **Fichiers** : `src/gfx/shaders/gizmo.wgsl`, `src/gfx/renderer.rs`, `src/app/mod.rs`, `src/editor/mod.rs`, `src/main.rs`.
- **Livrable** : manipulation complète translate/rotate/scale au gizmo. ✅

### Sprint 9 — Import de modèles glTF ✅ FAIT
**Objectif** : charger de vrais assets 3D.
- [x] Crate `gltf` ; lecture positions/normales/indices, toutes primitives fusionnées → `MeshData`.
- [x] Indices passés en `u32` (modèles > 65535 sommets) ; `MeshKind::Imported(u32)` + registre `Scene::imported`.
- [x] Bouton toolbar « 📥 Importer glTF » via dialogue `rfd`.
- [x] Recadrage auto (centré à l'origine, mis à l'échelle ~2 u) ; rechargement depuis le chemin au Load.
- [x] Message d'erreur explicite pour les `.gltf` sans leurs fichiers compagnons (préférer `.glb`).
- **Fichiers** : `src/scene/import.rs`, `scene/mod.rs`, `gfx/mesh.rs`, `gfx/renderer.rs`, `app/mod.rs`, `editor/mod.rs`.
- **Livrable** : importer un `.glb` l'affiche, recadré et éditable au gizmo. ✅

### Sprint 10 — Undo/Redo + duplication ✅ FAIT
**Objectif** : ergonomie d'édition de base.
- [x] Historique par snapshots de la liste d'objets (pile undo/redo, 50 niveaux).
- [x] Couvre : add / delete / duplicate + déplacement-gizmo (1 snapshot par drag).
- [x] Raccourcis **Cmd/Ctrl+Z**, **Cmd/Ctrl+Shift+Z**, **Cmd/Ctrl+D** + boutons toolbar.
- [x] Actions d'édition centralisées dans `AppState` (passent par l'historique).
- [ ] Reporté à un sprint dédié : multi-sélection (Shift+clic) ; undo des éditions inspecteur.
- **Fichiers** : `src/app/mod.rs`, `editor/mod.rs`, `gfx/renderer.rs`, `main.rs`, `scene/mod.rs`.
- **Livrable** : annuler/refaire + dupliquer, au clavier et à la souris. ✅

> **Phase A — Fondations éditeur : terminée** (Sprints 7→10).

---

## PHASE B — Runtime de jeu

### Sprint 11 — Scripting Lua ✅ FAIT
**Objectif** : attacher du comportement aux objets.
- [x] Crate **`mlua`** (Lua 5.4 vendored, aucune dépendance système).
- [x] Champ `script: String` sur `SceneObject` ; runtime `Lua` dans `AppState`.
- [x] API exposée : `obj.x/y/z`, `obj.rx/ry/rz` (°), `obj.sx/sy/sz`, `dt`, `time` + stdlib `math`.
- [x] Exécution par objet en mode Play ; erreurs capturées et loguées (pas de crash).
- [x] Éditeur de script (multiligne) dans l'inspecteur ; cube de démo scripté.
- **Fichiers** : `src/app/mod.rs`, `scene/mod.rs`, `editor/mod.rs`.
- **Livrable** : un cube tourne via script Lua, éditable en direct. ✅

### Sprint 12 — Physique (collisions)
**Objectif** : gravité et collisions réelles en mode Play.
- [ ] Intégrer **`rapier3d`** : un `RigidBody` + `Collider` par objet (box/sphère).
- [ ] Step de simulation en Play ; recopier les poses rapier → `Transform`.
- [ ] Inspecteur : type de corps (statique/dynamique), masse.
- **Fichiers** : `src/runtime/physics.rs` (nouveau), `runtime/mod.rs`.
- **Livrable** : en Play, une sphère tombe et rebondit sur le plan-sol.
- **Risque** : conversion repères/échelle entre rapier et le moteur ; mapping AABB→collider.

### Sprint 13 — Audio
**Objectif** : sons et ambiance.
- [ ] Crate **`kira`** (ou `rodio`). Composant `AudioSource { clip, autoplay }`.
- [ ] Lecture déclenchée en Play / par script.
- **Fichiers** : `src/runtime/audio.rs` (nouveau).
- **Livrable** : un objet joue un son au lancement du mode Play.
- **Risque** : latence/threads audio ; garder l'API minimale.

---

## PHASE C — Portage mobile

> Pré-requis : Phase A (au moins Sprint 7, le refactor) terminée — l'abstraction
> plateforme évite de dupliquer le code. `wgpu` + `winit` supportent déjà iOS/Android :
> l'effort est packaging + entrées tactiles + cycle de vie.

### Sprint 14 — Mode « Player » plein écran
**Objectif** : un mode sans panneaux éditeur (ce qui tournera sur mobile/casque).
- [ ] Flag `--player` / build feature `player` : démarre directement en Play, sans egui.
- [ ] Charger une scène figée (`scene.json` embarqué via `include_str!` ou asset).
- **Fichiers** : `src/main.rs`, `app/mod.rs`, `Cargo.toml` (feature).
- **Livrable** : sur desktop, `cargo run --features player` lance la scène en plein écran jouable.
- **Risque** : faible ; surtout de l'aiguillage de configuration.

### Sprint 15 — Entrées tactiles
**Objectif** : piloter la caméra/jeu au doigt.
- [ ] Gérer `WindowEvent::Touch` de winit : 1 doigt = orbit, 2 doigts = pinch-zoom + pan.
- [ ] Abstraire derrière le trait `InputSource` du Sprint 7 (souris ⇆ tactile).
- **Fichiers** : `src/app/input.rs`, `gfx/camera.rs`.
- **Livrable** : sur desktop avec trackpad/simulateur, les gestes contrôlent la caméra.
- **Risque** : gestion multi-touch (suivi des IDs de doigts).

### Sprint 16 — Build & déploiement iOS
**Objectif** : un `.ipa` qui tourne sur iPhone/iPad.
- [ ] Cible `aarch64-apple-ios` (+ `aarch64-apple-ios-sim`). Outil : **`cargo-mobile2`** (génère le projet Xcode).
- [ ] Boucle de rendu via `CADisplayLink` ; gérer suspend/resume → **recréer la surface wgpu** au retour au premier plan.
- [ ] Icône + `Info.plist` iOS + signature (compte développeur Apple requis).
- **Fichiers** : `gen/apple/` (généré), `app/mod.rs` (hooks cycle de vie).
- **Livrable** : la scène en mode Player tourne sur un appareil iOS (ou simulateur).
- **Risque** : ⚠️ le plus dur de la phase — perte de contexte GPU au background ; signature Apple.

### Sprint 17 — Build Android (parallèle d'iOS)
**Objectif** : un `.apk` Android (backend Vulkan).
- [ ] Cible `aarch64-linux-android` via `cargo-apk` / `cargo-mobile2` ; `winit` en mode `android-activity`.
- [ ] Gérer `Resumed`/`Suspended` (recréation surface), permissions, `AndroidManifest`.
- **Fichiers** : `gen/android/` (généré), `app/mod.rs`.
- **Livrable** : `.apk` installable lançant la scène.
- **Risque** : cycle de vie Android encore plus strict (surface détruite au pause).

---

## PHASE D — Réalité virtuelle (Oculus / Meta Quest)

> La VR passe par **OpenXR** (standard supporté par Meta Quest et PCVR/SteamVR).
> On réutilise tout le moteur de rendu ; on remplace la caméra par les poses XR
> et on rend **deux vues** (une par œil).

### Sprint 18 — Bootstrap OpenXR (desktop d'abord)
**Objectif** : ouvrir une session XR et présenter une image (même fixe).
- [ ] Crate **`openxr`** : instance, system, session liée à `wgpu` (interop via `wgpu-hal`/`ash`).
- [ ] Créer les **swapchains** XR ; boucle `xrWaitFrame`/`xrBeginFrame`/`xrEndFrame`.
- **Fichiers** : `src/xr/mod.rs` (nouveau), `gfx/renderer.rs` (cible de rendu = textures XR).
- **Livrable** : sur un casque PCVR (ou simulateur OpenXR), on voit un fond coloré stable.
- **Risque** : ⚠️ interop wgpu↔OpenXR (récupérer le device Vulkan partagé) = point le plus technique du projet.

### Sprint 19 — Rendu stéréo de la scène
**Objectif** : voir la scène 3D en relief.
- [ ] Récupérer du runtime XR les 2 poses + projections (par œil) → 2 matrices view/proj.
- [ ] Rendre la scène une fois par œil dans la swapchain correspondante (ou multiview).
- **Fichiers** : `xr/mod.rs`, `gfx/renderer.rs`, `gfx/camera.rs` (caméra pilotée par XR).
- **Livrable** : la scène (sol + cube + sphère) apparaît en VR, suit les mouvements de tête.
- **Risque** : justesse des matrices (sinon nausée) ; performances (2× le rendu).

### Sprint 20 — Contrôleurs & interaction VR
**Objectif** : sélectionner/déplacer des objets en VR.
- [ ] Action sets OpenXR : poses des mains + boutons (gâchette, grip).
- [ ] Rayon de pointage depuis la main → picking (réutilise ray/AABB existant) ; grab pour déplacer.
- **Fichiers** : `xr/input.rs` (nouveau), `app/mod.rs`.
- **Livrable** : pointer un objet et appuyer sur la gâchette le saisit et le déplace.
- **Risque** : mapping des espaces (main → monde) ; ergonomie.

### Sprint 21 — Cible Meta Quest (standalone)
**Objectif** : APK VR autonome sur Quest.
- [ ] Build Android + **OpenXR loader Oculus** ; manifeste VR (`com.oculus.intent.category.VR`).
- [ ] Confort : téléportation, vignettage anti-nausée, plancher (stage space).
- **Fichiers** : `gen/android/` (config VR), `xr/`.
- **Livrable** : `.apk` installé sur un Quest, scène explorable en standalone.
- **Risque** : combine les difficultés Android (Sprint 17) + VR (Sprints 18-20).

---

## ✅ Définition de « terminé » par phase

- **A** : éditeur confortable — gizmos, import glTF, undo, multi-sélection fonctionnent.
- **B** : une scène devient un mini-jeu — script + physique + audio en mode Play.
- **C** : la même scène tourne en mode Player sur iOS (et Android).
- **D** : la même scène est explorable en VR sur Meta Quest, manettes en main.

## 📌 Conseils d'exécution
1. **Faire le Sprint 7 en premier** : sans le refactor, chaque portage dupliquerait du code.
2. **Garder le mode Player (Sprint 14) comme cible de test** mobile/VR — pas l'éditeur complet.
3. **Tester sur device tôt** (Sprints 16/18) : les surprises GPU/cycle de vie viennent du matériel réel.
4. Avancer **une plateforme à la fois** ; ne pas ouvrir C et D en parallèle.
