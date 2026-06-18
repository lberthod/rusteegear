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

### Sprint 12 — Physique (collisions) ✅ FAIT
**Objectif** : gravité et collisions réelles en mode Play.
- [x] **`rapier3d`** : un `RigidBody` + `Collider` (cuboid/ball depuis l'AABB×échelle) par objet physique.
- [x] Step de simulation en Play (dt clampé) ; recopie des poses rapier → `Transform`.
- [x] Inspecteur : type de corps **Aucune / Statique / Dynamique**.
- [x] Bonus : **Stop restaure la scène** (Play = aperçu réinitialisable, snapshot à l'entrée).
- [x] rapier 0.33 utilise glam (parry/glamx) → conversion via composants f32.
- **Fichiers** : `src/runtime/physics.rs`, `runtime/mod.rs`, `scene/mod.rs`, `app/mod.rs`, `editor/mod.rs`.
- **Livrable** : en Play, la sphère tombe et rebondit sur le sol. ✅

### Sprint 13 — Audio ✅ FAIT
**Objectif** : sons et ambiance.
- [x] Crate **`kira`** ; champs `audio_clip` + `audio_autoplay` sur `SceneObject`.
- [x] Autoplay au lancement de Play, bouton « Tester », **stop des sons au Stop**.
- [x] Décodage audio **en thread de fond** + **cache** (pas de re-décodage).
- **Fichiers** : `src/runtime/audio.rs`, `scene/mod.rs`, `app/mod.rs`, `editor/mod.rs`.
- **Livrable** : un objet joue un son au lancement du mode Play. ✅

> **Phase B — Runtime de jeu : terminée** (Sprints 11→13).

### Optimisations performance ✅ FAIT
- [x] **Import glTF asynchrone** (thread de fond + canal) : plus de gel pendant le chargement.
- [x] **Audio asynchrone + cache** : décodage hors du thread de rendu.
- [x] **Présentation vsync (Fifo)** : rendu calé sur l'écran, fluide et peu gourmand.
- [x] Rappel : toujours tester en `--release` (debug = stack non optimisée, très lente).

---

## PHASE C — Portage mobile

> Pré-requis : Phase A (au moins Sprint 7, le refactor) terminée — l'abstraction
> plateforme évite de dupliquer le code. `wgpu` + `winit` supportent déjà iOS/Android :
> l'effort est packaging + entrées tactiles + cycle de vie.

### Sprint 14 — Mode « Player » plein écran ✅ FAIT
**Objectif** : un mode sans panneaux éditeur (ce qui tournera sur mobile/casque).
- [x] Flag `--player` (et auto-activé sur iOS/Android) : démarre directement en Play, sans egui.
- [x] Rendu plein écran sans panneaux ni gizmo ; caméra toujours contrôlable.
- **Fichiers** : `src/main.rs`, `app/mod.rs`, `gfx/renderer.rs`.
- **Livrable** : `cargo run -- --player` lance la scène animée en plein écran. ✅

### Sprint 15 — Entrées tactiles ✅ FAIT
**Objectif** : piloter la caméra/jeu au doigt.
- [x] `WindowEvent::Touch` géré : **1 doigt = orbit**, **2 doigts = pinch-zoom** (suivi des IDs).
- [x] Traduit vers les `InputEvent` agnostiques (Sprint 7) → réutilise la logique caméra souris.
- [x] Compile desktop **et** iOS ; souris desktop inchangée.
- **Fichiers** : `src/main.rs`.
- **Livrable** : gestes tactiles → caméra (validé matériellement sur device iOS au Sprint 16+). ✅
- _Note : les events Touch ne se déclenchent que sur écran tactile ; test final sur iPhone._

### Sprint 16 — Build & déploiement iOS 🟡 PARTIEL (compile + .ipa non signé)
**Objectif** : un `.ipa` qui tourne sur iPhone/iPad.
- [x] Cibles Rust `aarch64-apple-ios` (+ `-sim`) ajoutées.
- [x] **Cross-compilation complète réussie** : wgpu, winit, egui, rapier, mlua (Lua C), kira → iOS arm64. ✅
- [x] `rfd` rendu desktop-only (`[target.'cfg(not(ios/android))']`) — seul blocage de compilation.
- [x] `packaging/build_ios.sh` : assemble `.app` + Info.plist → **`.ipa` (non signé)** (~6 Mo).
- [ ] **Bloqué ici** : signature/provisioning (compte développeur Apple requis) pour installer sur device.
- [ ] À faire ensuite : projet Xcode via **`cargo-mobile2`**, cycle de vie iOS (recréer la surface wgpu au resume), entrées tactiles (Sprint 15), mode Player (Sprint 14).
- **Pré-requis build** : Xcode complet (`export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`).
- **Fichiers** : `packaging/build_ios.sh`, `Cargo.toml`, `editor/mod.rs` (gates cfg).
- **État** : la preuve technique est faite (le moteur compile et se package pour iOS) ; reste la signature Apple + l'intégration UIKit pour un lancement réel.

### Sprint 17 — Build Android 🟡 PARTIEL (préparé, bloqué sur le NDK)
**Objectif** : un `.apk` Android (backend Vulkan).
- [x] Cible Rust `aarch64-linux-android` ajoutée.
- [x] `winit` configuré avec la feature `android-native-activity` (ciblée Android).
- [x] Mode Player auto-activé sur Android ; desktop inchangé.
- [x] `packaging/build_android.md` : marche à suivre complète.
- [ ] **Bloqué ici** : NDK non installé (`aarch64-linux-android-clang` manquant → Lua/`mlua` + linker).
- [ ] À faire ensuite : NDK, `cargo-ndk`/`cargo-apk`, point d'entrée `android_main` + `cdylib`.
- **Fichiers** : `Cargo.toml`, `packaging/build_android.md`.
- **État** : logique identique à iOS ; reste l'environnement (NDK) + l'entrée `android_main`.

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
