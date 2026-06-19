# Motor3DeRust — Plan de sprints d'exécution (post-MVP)

> Feuille de route **étape par étape** pour faire évoluer le moteur du MVP actuel
> jusqu'au mobile (iOS / Android).
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
| **D — App de dev & exports 1-clic** | 18 → 23 | Optimiser l'app desktop (.dmg) et exporter APK/IPA depuis des boutons configurables |
| **E — Player complet & maturité éditeur** | 24 → 27 | Embarquer les assets, enrichir l'édition, monter en qualité de rendu, durcir |

> Phases A et B améliorent le cœur **partagé** par toutes les plateformes.
> Les faire avant C évite de réécrire des features sur plusieurs cibles.

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

### Sprint 16 — Build & signature iOS 🟢 .ipa signé (reste le profil device)
**Objectif** : un `.ipa` qui tourne sur iPhone/iPad.
- [x] Cross-compilation `aarch64-apple-ios` complète (wgpu, winit, egui, rapier, mlua, kira). ✅
- [x] `rfd` desktop-only ; mode Player auto sur iOS.
- [x] `packaging/build_ios.sh` : assemble `.app` + Info.plist → `.ipa` (~6 Mo).
- [x] **`.ipa` signé** avec le certificat `Apple Development: lberthod@gmail.com` (Team `N668CK695Q`). ✅
- [ ] Reste pour installer sur device : un **profil de provisioning** (Xcode auto / portail) — cf. `build_ios.md`.
- **Fichiers** : `packaging/build_ios.sh`, `packaging/build_ios.md`, `Cargo.toml`.
- **État** : compile, package **et se signe** pour iOS ; dernière étape = profil device (compte Apple présent).

### Sprint 17 — Build Android ✅ FAIT (APK signé)
**Objectif** : un `.apk` Android (backend Vulkan).
- [x] Cible Rust `aarch64-linux-android` ; `winit` feature `android-native-activity` (ciblée).
- [x] Crate restructuré en **lib + bin** ; `src/lib.rs` expose `run()` (desktop) + `android_main` (cdylib).
- [x] Mode Player auto-activé sur Android ; desktop inchangé.
- [x] **NDK 28.2.13676358** installé via Android Studio (`sdkmanager`) ; API 26 (AAudio).
- [x] **APK release signé** via `cargo-apk` : `target/release/apk/motor3derust.apk` (~6.6 Mo, arm64-v8a). ✅
- [x] Scripts `packaging/build_apk.sh` + `android_env.sh` ; doc `build_android.md`.
- **Fichiers** : `src/lib.rs`, `src/main.rs`, `Cargo.toml`, `packaging/*`.
- **Livrable** : APK installable (`adb install`) lançant la scène en mode Player. ✅

---

## PHASE D — App de dev & exports 1-clic

> **Objectif global.** Faire du `.dmg` macOS l'**atelier central** : on conçoit la
> scène/le jeu dans l'app de dev, puis on **exporte un APK ou un IPA depuis un bouton**,
> sans retoucher la ligne de commande. Toute la config de build (identité, certificat,
> SDK, version) devient **éditable dans l'UI** et persistée. En parallèle, on professionnalise
> l'app desktop (perf, profils de build, diagnostics) pour qu'elle serve de poste de travail.
>
> Pré-requis : Phase C terminée (les 3 plateformes compilent et tournent). Les scripts
> `packaging/build_*.sh` existants servent de socle ; cette phase les **pilote depuis l'app**.

### Sprint 18 — Profils de build & app desktop « de dev » ✅ FAIT
**Objectif** : séparer proprement *dev* et *release*, et optimiser l'app desktop.
- [x] Profils Cargo dédiés : `[profile.release]` avec `lto = "thin"`, `codegen-units = 1`,
      `panic = "abort"`, `strip = true` ; profil `dev-fast` (`opt-level = 1`) pour itérer vite.
- [x] Bandeau d'état dans l'éditeur (panneau bas) : FPS lissé, nombre d'objets, mode (Edit/Play),
      backend GPU (Metal/Vulkan) — `StatusInfo` passé à `Editor::run`.
- [x] Cadence adaptative : `ControlFlow::Poll` en Play ou pendant une interaction
      (`AppState::is_active()`), throttle `wait_duration(60 ms)` au repos (CPU ≈ 0 %).
- [ ] Feature flags `editor` / `player` pour ne compiler que le nécessaire → **reporté**
      (gating complet d'egui risqué pour les builds mobiles ; à faire dans un sprint dédié).
- **Fichiers** : `Cargo.toml`, `src/app/mod.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs`, `src/lib.rs`.
- **Livrable** : `.dmg` plus léger/rapide ; bandeau FPS visible ; CPU au repos chute (throttle 16 fps). ✅
- **Risques** : `Wait` mal réglé fige l'animation → `Poll` conservé dès que `is_active()` (Play/interaction).

### Sprint 19 — Panneau « Build & Export » dans l'éditeur ✅ FAIT
**Objectif** : un onglet/fenêtre egui d'où l'on lance les exports.
- [x] Fenêtre flottante **« 📦 Build & Export »** (bouton toolbar) : 3 cartes — macOS (.dmg), Android (.apk), iOS (.ipa).
- [x] Chaque carte affiche : cible, statut (✓ prêt / ⚠ config manquante + aide), bouton **Exporter**.
- [x] Détection des pré-requis (`cargo-bundle`, `cargo-apk`, `xcodegen`, plateforme) au démarrage.
- [x] L'export s'exécute en **thread de fond** (`std::process::Command` sur les scripts `packaging/*.sh`)
      avec **log streamé** (stdout+stderr) dans une zone défilante, spinner pendant l'exécution.
- [ ] Bouton « Annuler » + dossier de sortie configurable → reporté aux Sprints 20-21.
- **Fichiers** : nouveau `src/editor/export.rs`, `src/editor/mod.rs`.
- **Livrable** : cliquer « Exporter » lance le packaging sans quitter l'app, log visible en direct. ✅
- **Risques** : build long depuis l'UI → async (un seul à la fois), bouton désactivé pendant l'exécution.

### Sprint 20 — Config de build persistée & éditable ✅ FAIT
**Objectif** : rendre l'identité/version **configurables dans l'app**.
- [x] Struct `BuildConfig` (serde) : `app_name`, `bundle_id`, `version`, `build_number`.
- [x] Persistance dans `~/.motor3derust/build_config.json` (`load`/`save`) ; formulaire egui (grille) dans le panneau Export.
- [x] Validation en direct (bundle id : segments alphanumériques séparés par points ; nom/version non vides) affichée sous le formulaire.
- [x] Numéro de build **auto-incrémenté et persisté** à chaque export.
- [x] Scripts `packaging/*.sh` reçoivent `OUTPUT_NAME` / `BUNDLE_ID` / `APP_VERSION` / `BUILD_NUMBER` via env.
      **iOS** : Info.plist entièrement piloté (id, version, build, nom). **macOS/Android** : nom de fichier renommé.
- [ ] Chemins SDK/NDK, équipe Apple, keystore éditables → reporté (Sprint 23, avec presets/secrets).
- **Fichiers** : `src/app/build_config.rs`, `src/editor/export.rs`, `packaging/build_ios.sh`.
- **Livrable** : changer nom/bundle id/version dans l'UI se reflète dans l'`.ipa` exporté ; config persistée entre sessions. ✅
- **Note** : l'override du bundle id/version interne sur **macOS** (cargo-bundle) et **Android** (cargo-apk)
  est limité — ils lisent `Cargo.toml` ; seul le nom de fichier est renommé. Override complet = Sprint 23.

### Sprint 21 — Export APK 1-clic ✅ FAIT
**Objectif** : bouton « Exporter Android » fiable et configurable.
- [x] `export.rs` invoque `build_apk.sh` (cargo-apk) avec l'environnement issu de `BuildConfig`.
- [x] **Pré-vol** Android : `cargo-apk` présent, **NDK localisé** (env ou `~/Library/Android/sdk/ndk/*`),
      cible `aarch64-linux-android` installée (`rustup target list`) → message d'aide précis sinon.
- [x] APK rangé dans `target/export/<nom>.apk` ; bouton **« 📂 Révéler le dossier »** après succès.
- [x] Option **« Installer sur l'appareil (adb) »** (case Android, grisée si `adb` absent) → `adb install -r`.
- **Fichiers** : `src/editor/export.rs`, `packaging/build_apk.sh`.
- **Livrable** : un clic → `.apk` signé dans `target/export/` (et installé sur device si coché). ✅
- **Risques** : env Android fragile → détection centralisée (`find_ndk`, `rust_target_installed`), log streamé.

### Sprint 22 — Export IPA 1-clic ✅ FAIT
**Objectif** : bouton « Exporter iOS » avec signature configurable.
- [x] `export.rs` pilote `build_ios.sh` (fichier `.ipa`) / `install_ios_device.sh` (device) via `BuildConfig`
      (Team ID, identité « Apple Development », profil `.mobileprovision`), surchargés seulement si renseignés.
- [x] Pré-vol iOS : `xcodegen`, cible `aarch64-apple-ios`, **identité de signature** présente (`security find-identity`).
- [x] Deux modes selon la case : **Exporter `.ipa`** (build_ios.sh) ou **Installer sur iPhone** (`devicectl`).
- [x] Sélecteur de profil de provisioning (`.mobileprovision`) via rfd dans la section « Signature iOS ».
- **Fichiers** : `src/editor/export.rs`, `src/app/build_config.rs`, `packaging/build_ios.sh`.
- **Livrable** : un clic → `.ipa` signé dans `target/export/`, ou app installée/lancée sur l'iPhone branché. ✅
- **Risques** : signature Apple capricieuse → log Xcode brut streamé dans l'UI.

### Sprint 23 — Finition, presets & CI de release ✅ FAIT
**Objectif** : rendre les exports reproductibles et partageables.
- [x] **Presets** d'export sauvegardables = `BuildConfig` nommés dans `~/.motor3derust/presets/`
      (combo de chargement + bouton 💾 d'enregistrement).
- [x] Incrément auto du `build_number` à chaque export (Sprint 20).
- [x] Bouton **« 🚀 Tout exporter »** : enfile les cibles prêtes, jouées une par une.
- [x] Workflow CI `release.yml` : sur tag `v*`, build macOS (.dmg) + Android (.apk) attachés à la Release.
- [x] Doc `packaging/EXPORT.md` : pré-requis par plateforme, config, install device, variables.
- [ ] IPA signé en CI (certificat + profil en secrets) → laissé en option (documenté).
- **Fichiers** : `src/editor/export.rs`, `src/app/build_config.rs`, `.github/workflows/release.yml`, `packaging/EXPORT.md`.
- **Livrable** : pousser un tag `v*` produit les artefacts en Release ; presets réutilisables dans l'app. ✅
- **Risques** : signatures en CI (secrets) → certificats/keystore en *GitHub Secrets*, jamais dans le repo.

---

## PHASE E — Player complet & maturité éditeur

> Issue de l'audit post-Phase D. Objectif : que le **jeu exporté tourne réellement
> partout** (assets compris), que l'**édition** rivalise avec un éditeur sérieux, et
> que le rendu et la robustesse montent d'un cran.

### Sprint 24 — Assets embarqués dans le player ⬜
**Objectif** : un `.dmg`/`.apk`/`.ipa` qui contient **tout le jeu** (modèles + sons), jouable hors développement.
- [ ] Bundle d'assets : copier les fichiers glTF/sons référencés dans `assets/bundle/` + réécrire les chemins de la scène en chemins relatifs au bundle.
- [ ] Player : résoudre les assets depuis le bundle (`include_dir!` ou dossier `Resources` du `.app`/APK) au lieu de chemins disque absolus.
- [ ] Décodage glTF/sons depuis mémoire (octets) et plus seulement depuis un chemin.
- [ ] Le panneau Export embarque la scène **et** ses assets ; avertir si un asset est introuvable.
- **Fichiers** : `src/scene/import.rs`, `src/runtime/audio.rs`, `src/editor/export.rs`, `packaging/*.sh`.
- **Livrable** : exporter une scène avec un modèle importé + un son → le player les joue sur un autre poste/appareil. ✅
- **Risques** : tailles d'APK/IPA ; chemins relatifs cross-platform → tests sur device.

### Sprint 25 — Édition avancée & hiérarchie ⬜
**Objectif** : multi-sélection, copier/coller, renommage et réorganisation.
- [ ] **Multi-sélection** (Cmd/Maj+clic) ; gizmo et inspecteur agissant sur la sélection multiple.
- [ ] **Copier/Coller/Dupliquer** un ensemble (Cmd+C/V), avec historique undo.
- [ ] **Renommage inline** dans la hiérarchie (double-clic) ; **réordonnancement** par glisser-déposer.
- [ ] **Sous-groupes** (groupes imbriqués) + repli mémorisé.
- **Fichiers** : `src/app/mod.rs`, `src/editor/mod.rs`.
- **Livrable** : sélectionner 3 objets, les grouper, les déplacer ensemble, renommer un groupe. ✅
- **Risques** : invariants d'index lors des suppressions multiples → travailler par identifiants stables.

### Sprint 26 — Rendu : matériaux & ombres ⬜
**Objectif** : sortir du Lambert uni — texture/couleur par objet et ombres.
- [ ] **Matériau par objet** : couleur albédo éditable dans l'inspecteur (+ métallique/rugosité simples).
- [ ] **Textures** : charger une image et l'échantillonner (UV des primitives + glTF).
- [ ] **Ombres** : shadow mapping directionnel (depth pass + comparaison).
- [ ] Réglages de scène : direction/couleur de lumière, couleur ambiante.
- **Fichiers** : `src/gfx/*`, `shaders/*.wgsl`, `src/scene/mod.rs`, `src/editor/mod.rs`.
- **Livrable** : un objet texturé projette une ombre sur le sol ; couleur éditable en direct. ✅
- **Risques** : coût GPU mobile (carte d'ombre) → résolution adaptative, limites `wgpu`.

### Sprint 27 — Identité, cycle de vie mobile & durcissement ⬜
**Objectif** : finir l'override d'identité, gérer le resume mobile, durcir/tester.
- [ ] **Override bundle id/version** macOS (patch Info.plist du `.app`) et Android (manifest cargo-apk).
- [ ] **Resume mobile** : recréation de la surface `wgpu` sur `suspended`/`resumed` (évite l'écran noir au retour d'app).
- [ ] **IPA signé en CI** (certificat + profil en *GitHub Secrets*) ; artefact attaché à la Release.
- [ ] **Tests d'intégration** : round-trip scène avec groupes/assets, sérialisation des nouveaux champs ; réduire les `unwrap()` restants.
- **Fichiers** : `packaging/*.sh`, `src/lib.rs`, `.github/workflows/release.yml`, tests.
- **Livrable** : un tag `v*` produit `.dmg`+`.apk`+`.ipa` signés à la bonne identité ; l'app mobile survit au passage en arrière-plan. ✅
- **Risques** : secrets CI → jamais dans le repo ; signature Apple capricieuse → logs bruts.

---

## ✅ Définition de « terminé » par phase

- **A** : éditeur confortable — gizmos, import glTF, undo, duplication fonctionnent.
- **B** : une scène devient un mini-jeu — script + physique + audio en mode Play.
- **C** : la même scène tourne en mode Player sur iOS (et Android).
- **D** ✅ : depuis l'app de dev (.dmg), on exporte un **player** du jeu créé en `.dmg` / `.apk` / `.ipa`
  **en un clic** (scène embarquée), avec config éditable/persistée, presets, install device et CI de release.
- **E** : le jeu exporté tourne partout **assets compris** ; édition avancée (multi-sélection, sous-groupes,
  renommage) ; rendu avec matériaux/ombres ; identité de bundle et cycle de vie mobile durcis.

## 📌 Conseils d'exécution
1. **Faire le Sprint 7 en premier** : sans le refactor, chaque portage dupliquerait du code.
2. **Garder le mode Player (Sprint 14) comme cible de test** mobile — pas l'éditeur complet.
3. **Tester sur device tôt** (Sprint 16) : les surprises GPU/cycle de vie viennent du matériel réel.
4. Avancer **une plateforme à la fois**.
