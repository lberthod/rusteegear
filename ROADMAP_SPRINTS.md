# RusteeGear — Plan de sprints d'exécution (post-MVP)

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
| **F — Reprise, finitions & distribution** | 28 → 31 | Onboarding/validation, finir l'édition & le rendu, distribuer proprement |

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

### Sprint 24 — Assets embarqués dans le player ✅ FAIT
**Objectif** : un `.dmg`/`.apk`/`.ipa` qui contient **tout le jeu** (modèles + sons), jouable hors développement.
- [x] Bundle d'assets : à l'export, copie des fichiers glTF/sons référencés dans `assets/bundle/`
      + réécriture des chemins de la scène en `bundle://<clé>`.
- [x] Player : assets embarqués à la compilation via `include_dir!` (`src/assets.rs`), résolus par clé.
- [x] Décodage **depuis mémoire** : glTF (`gltf::import_slice`) et sons (`StaticSoundData::from_cursor`).
- [x] Le panneau Export embarque la scène **et** ses assets ; **avertit** si un asset est introuvable.
- **Fichiers** : `src/assets.rs`, `src/scene/import.rs`, `src/runtime/audio.rs`, `src/editor/export.rs`, `Cargo.toml`.
- **Livrable** : exporter une scène avec un modèle importé + un son → le player les joue sur un autre poste/appareil. ✅
- **Risques** : tailles d'APK/IPA ; `.gltf` à références externes → préférer `.glb` (autonome).

### Sprint 25 — Édition avancée & hiérarchie 🟢 (cœur fait)
**Objectif** : multi-sélection, copier/coller, renommage et réorganisation.
- [x] **Multi-sélection** dans la hiérarchie (Cmd/Maj+clic) ; surbrillance primaire (1.0) + autres (0.55) ;
      gizmo/inspecteur sur la primaire.
- [x] **Copier/Coller** (Cmd+C/V) + **Dupliquer** (Cmd+D) en lot ; **Suppr/Backspace** supprime la sélection ; undo.
- [x] **Renommage inline** dans la hiérarchie (double-clic → champ, validation à la perte de focus).
- [ ] Multi-sélection au clic 3D (modificateurs via `InputEvent`) → reporté.
- [ ] **Réordonnancement** par glisser-déposer et **sous-groupes** imbriqués → reportés (Sprint dédié).
- **Fichiers** : `src/app/mod.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs`, `src/lib.rs`.
- **Livrable** : Cmd+clic pour sélectionner plusieurs objets, Cmd+C/V/D et Suppr en lot, double-clic pour renommer. ✅
- **Risques** : invariants d'index sur suppressions multiples → suppression par indices décroissants.

### Sprint 26 — Rendu : matériaux & ombres 🟢 (matériaux + lumière faits)
**Objectif** : sortir du Lambert uni — texture/couleur par objet et ombres.
- [x] **Couleur (teinte/albédo) par objet** éditable dans l'inspecteur (color picker), via `ModelUniform`.
- [x] **Éclairage de scène** éditable (direction, couleur, ambiante) via `SceneUniform` (groupe 0, binding 1) ;
      shader Lambert paramétré ; persisté dans la scène (`Scene::light`).
- [ ] **Textures** (chargement image + UV) → reporté (nécessite UV des primitives + bind group texture).
- [ ] **Ombres** (shadow mapping directionnel) → reporté (depth pass + sampler comparaison, à itérer sur GPU).
- **Fichiers** : `src/gfx/renderer.rs`, `src/gfx/shaders/main.wgsl`, `src/scene/mod.rs`, `src/editor/mod.rs`.
- **Livrable** : couleur d'objet et éclairage modifiables en direct, persistés au save. ✅
- **Risques** : textures/ombres → coût GPU mobile, à valider visuellement → passe dédiée.

### Sprint 27 — Identité, cycle de vie mobile & durcissement 🟢 (cœur fait)
**Objectif** : finir l'override d'identité, gérer le resume mobile, durcir/tester.
- [x] **Override d'identité macOS** : à l'export, patch de l'Info.plist du `.app` (id/nom/version/build)
      via PlistBuddy puis `.dmg` recréé avec `hdiutil`.
- [x] **Resume mobile** : `suspended` lâche le renderer (surface invalide), `resumed` le reconstruit ;
      l'état applicatif (scène, sélection) est préservé.
- [x] **Tests d'intégration** : round-trip scène avec groupes/couleur/lumière + compat ascendante
      (anciennes scènes sans les nouveaux champs).
- [ ] Override id/version **Android** (cargo-apk lit `Cargo.toml`) → reporté.
- [ ] **IPA signé en CI** (certificat + profil en *GitHub Secrets*) → reporté (secrets à fournir).
- **Fichiers** : `packaging/build_dmg.sh`, `src/lib.rs`, `src/scene/mod.rs` (tests).
- **Livrable** : `.dmg` exporté à la bonne identité ; l'app mobile survit au passage en arrière-plan ; 11 tests verts. ✅
- **Risques** : secrets CI → jamais dans le repo ; signature Apple capricieuse → logs bruts.

---

## PHASE F — Reprise, finitions & distribution

> **Contexte de reprise.** Projet transmis à un nouveau développeur. Les Phases A→E
> sont en place (cœur), mais plusieurs nouveautés n'ont **jamais été exécutées de bout
> en bout** (cf. audit Phase E) et certains demi-sprints restent à finir. Cette phase
> sécurise d'abord la reprise (validation + tests), puis termine l'édition, le rendu et
> la distribution. Lire **[README.md](README.md)**, **[PLAN.md](PLAN.md)** et
> **[packaging/EXPORT.md](packaging/EXPORT.md)** avant de démarrer.

### Sprint 28 — Prise en main & validation de bout en bout 🟢 (validé desktop)
**Objectif** : exécuter réellement ce qui a été codé « en vert » et poser des filets.
- [x] **Export `.dmg` player réel** validé : `OUTPUT_NAME/BUNDLE_ID/APP_VERSION … ./packaging/build_dmg.sh`
      → `target/export/DemoJeu.dmg` (5,5 Mo) avec identité appliquée (id/nom/version vérifiés dans l'Info.plist).
- [x] **Tests bon marché** ajoutés : invariant de sélection (`selection` ⊆ `selected`), niveaux de surbrillance,
      résolution `bundle://` (`strip_scheme`/`bundle_bytes`). **15 tests verts**.
- [x] `unwrap()` du tactile (`lib.rs`) durcis en `let … else` (plus aucun `unwrap` hors tests).
- [ ] **Test sur device mobile** (`.apk` + resume arrière-plan) → à faire avec un appareil (hors CI/headless).
- **Fichiers** : `src/app/mod.rs`, `src/assets.rs`, `src/lib.rs`, tests.
- **Livrable** : player `.dmg` produit à la bonne identité ; tests verts élargis. ✅ (reste : run GUI + device).
- **Risques** : surprises runtime (alignement uniformes GPU mobile, chemins) → corriger dès observation.

### Sprint 29 — Édition complète (reporté du Sprint 25) 🟢 (cœur fait)
**Objectif** : finir la sélection et la hiérarchie pour un vrai confort d'édition.
- [x] **Multi-sélection au clic 3D** (Cmd/Maj) : `App::set_additive` depuis les modificateurs winit,
      `toggle_select` vs `select_single` dans `handle_input`.
- [x] **Gizmo multi-objets (translate)** : déplace toute la sélection en bloc (positions d'origine mémorisées).
- [x] **Réordonnancement** : boutons ▲/▼ dans l'inspecteur (`move_selected_in_list`, avec undo).
- [ ] Gizmo multi en **rotate/scale** (pivot commun) → reporté.
- [ ] **Réordonnancement par glisser-déposer** et **sous-groupes** imbriqués → reportés.
- **Fichiers** : `src/app/mod.rs`, `src/lib.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs`.
- **Livrable** : Cmd+clic 3D multi-sélectionne, le gizmo déplace le groupe, ▲/▼ réordonnent. ✅
- **Risques** : invariants d'index → couverts par les tests du Sprint 28.

### Sprint 30 — Rendu : ombres & textures (reporté du Sprint 26) 🟢 (ombres + textures faites)
**Objectif** : passer d'un rendu plat à un rendu crédible. **Itérer visuellement** (lancer l'app souvent).
- [x] **Shadow mapping** directionnel : passe de profondeur 1024² (`shadow.wgsl`), PCF 3×3,
      biais + cull faces avant (anti-acné). **Validé à l'écran** (ombres nettes, sans acné).
- [x] **Textures** albédo par objet : UV sur primitives (cube/sphère/plan) + glTF (`read_tex_coords`),
      `Vertex.uv`, bind group 3 (texture+sampler), texture blanche par défaut, décodage `image`
      (disque ou `bundle://`), embarquées à l'export. **Validé à l'écran** (cube texturé + ombre).
- [ ] Matériau étendu (métallique/rugosité) → étape suivante (PBR).
- [ ] Réglage du coût mobile (résolution d'ombre, repli `wgpu`) → à valider sur device.
- **Fichiers** : `src/gfx/{renderer.rs,mesh.rs,shaders/*}`, `src/scene/{mod,import}.rs`, `src/editor/{mod,export}.rs`.
- **Livrable** : objet texturé qui projette une ombre, rendu desktop (validé). Reste : PBR + validation mobile. 🟢
- **Risques** : non vérifiable sans GPU réel → itéré à l'écran (captures utilisateur).

### Sprint 31 — Distribution complète ⬜
**Objectif** : livrables signés et reproductibles pour les stores.
- [ ] **Override d'identité Android** (bundle id/version) : injecter dans `cargo-apk` (génération du manifeste).
- [ ] **IPA signé en CI** : certificat + profil en *GitHub Secrets*, job iOS dans `release.yml`.
- [ ] **Signature de distribution** : notarisation macOS, App Store / Play Store (icônes, écran de lancement).
- [ ] Versionnement : tag `v*` → 3 artefacts signés attachés à la Release, `build_number` cohérent.
- **Fichiers** : `packaging/*.sh`, `.github/workflows/release.yml`, `Cargo.toml`, `packaging/EXPORT.md`.
- **Livrable** : un tag produit `.dmg` notarisé + `.apk` + `.ipa` signés, prêts à distribuer. ✅
- **Risques** : secrets → *GitHub Secrets* uniquement, jamais dans le repo ; comptes développeur requis.

### Sprint 32 — Outils produit & barre de menus pro 🟢 (cœur fait)
**Objectif** : transformer l'éditeur en *produit* orienté export Android, pas seulement
un démonstrateur technique. Barre de menus complète + outils de contrôle qualité.
- [x] **Barre de menus repensée** (`src/editor/mod.rs`) : Fichier (Nouveau projet, Quitter…),
      Édition (Aligner au sol, Réinitialiser transform), Ajouter (sous-menu « Objet 3D » +
      catégories à venir grisées), Outils, Aide.
- [x] **Actions d'édition 3D** (`src/app/mod.rs`) : `new_scene`, `align_to_ground` (base sur y=0
      via AABB×échelle), `reset_transform` (rotation/échelle), `request_quit`.
- [x] **Toolbar** : Dupliquer/Supprimer rapides + bouton **🤖 Build APK** mis en avant à droite.
- [x] **Console intégrée** (`src/log_buffer.rs`) : logger qui *tee* vers `env_logger` + tampon
      circulaire (500 lignes) affiché dans une fenêtre, avec « Effacer ».
- [x] **Profiler FPS** : sparkline 120 frames (vert/orange/rouge), min/moy/max + nb d'objets.
- [x] **Contrôle qualité APK / APK Readiness Check** (`src/editor/readiness.rs`) : analyse réelle
      de la scène + config de build (scène vide, sol, lumière, scripts, colliders manquants,
      textures introuvables/ > 4096 px, nom/package id/version) → verdict ✅/⚠/❌ + « prêt à exporter ».
- [x] **Diagnostic système** : Rust, `ANDROID_HOME`/`ANDROID_NDK_HOME`, backend graphique.
- [x] **Aide** : raccourcis clavier, guide export APK, à propos.
- [ ] **Optimisation mobile** (compresser/réduire textures, fusion meshes, LOD, occlusion) → reporté
      (nécessite un pipeline d'assets).
- [x] **Primitives Cylindre & Capsule** (`src/gfx/mesh.rs`, `src/scene/mod.rs`) : meshes générés,
      AABB, colliders dédiés (`cylinder`/`capsule_y` dans `physics.rs`), entrées de menu + catégories.
- [x] **Contrôles tactiles mobiles** (`Scene::mobile` = joystick + boutons nommés, sérialisés) :
      overlay egui (joystick draggable bas-gauche, boutons ronds bas-droite) en mode **Play** *et*
      **Player** (exporté), routage d'évènements adapté en player. Exposés aux scripts Lua via
      `input.jx`, `input.jy`, `input.btn.<nom>` (test `script_reads_mobile_input`). Configurés
      depuis Ajouter → Contrôles mobiles.
- [ ] **Boucle complète vérifiée sur device** (joystick → script → APK) → à faire sur appareil réel.
- [ ] **Terrain**, **Lumières/Caméras comme objets**, **Gyroscope/Vibration** → reporté
      (nouveaux sous-systèmes : ECS, capteurs natifs).
- **Fichiers** : `src/editor/{mod,readiness,export}.rs`, `src/log_buffer.rs`, `src/app/{mod,build_config}.rs`,
  `src/gfx/renderer.rs`, `src/lib.rs`.
- **Livrable** : menus complets + Console / Profiler / Contrôle qualité APK fonctionnels, branchés sur
  l'état réel ; build + clippy + tests verts. 🟢
- **Risques** : items reportés = vrais sous-systèmes, pas de l'UI → à planifier en sprints dédiés.

---

## 🔭 Audit (2026-06-19) & sprints proposés 33–37

**État.** ~7000 lignes Rust, architecture saine (état métier `app/` sans GPU, rendu `gfx/`,
runtime `runtime/`, UI `editor/`). 20 tests, CI clippy-clean. Acquis récents : éditeur complet,
contrôles tactiles + scripts Lua, aperçu mobile jouable, génération IA (script + scène) via DeepSeek.

**Dette / manques principaux** :
- Rendu plat (pas de PBR, une seule lumière globale, pas d'instanciation/culling).
- Modèle de scène limité : lumière unique, pas de caméras/lumières comme objets.
- Pas de pipeline d'assets mobile (chargement fichiers désactivé sur mobile = P10, pas de compression textures).
- Distribution non finalisée (identité Android non surchargée, IPA non signé en CI, pas de validation device).
- Confort d'édition incomplet (pas de glisser-déposer, sous-groupes, gizmo multi rotate/scale).

### Sprint 33 — Matériaux PBR & rendu avancé ⬜
**Objectif** : passer d'un rendu plat à un rendu crédible, sans casser le coût mobile.
- [x] Matériau par objet : metallic, roughness, emissive (champs SceneObject + sliders inspecteur).
- [x] Shader PBR léger (diffuse atténuée métal + spéculaire Blinn-Phong piloté rugosité + émission).
- [x] **Frustum culling** CPU (plans Gribb-Hartmann + test AABB monde) avant la passe caméra.
- [ ] Shader PBR (BRDF Cook-Torrance simplifié) + uniforms matériau (bind group dédié).
- [x] Rendu instancié : buffer storage d'instances (groupe 1) indexé par instance_index,
      draws groupés par mesh+texture (1 draw par lot, scindé en plages visibles).
- [x] **Sprint 33 terminé.**
- [ ] **Frustum culling** CPU (AABB monde déjà calculées) avant soumission.
- **Fichiers** : `src/gfx/{renderer,shaders/*}.rs`, `src/scene/mod.rs`, `src/editor/mod.rs`.
- **Livrable** : objets métal/plastique réalistes, draw calls réduits, FPS stable sur grosses scènes.
- **Risques** : coût GPU mobile → garder un repli « unlit » (toggle qualité, lié au panneau Optimisation).

### Sprint 34 — Scène étendue : lumières & caméras comme objets ⬜
**Objectif** : sortir du modèle « une lumière globale » vers une vraie hiérarchie d'entités.
- [ ] `SceneObject` typé (enum `kind`: Mesh / Light(point|dir|spot) / Camera) ou composants.
- [x] Plusieurs lumières ponctuelles (Scene.point_lights, max 8) : uniform std140 + shader,
      marqueurs en croix colorés dans l'éditeur, ajout via menu + édition inspecteur.
- [x] Caméra de jeu (Scene.game_camera) : « Ajouter → Caméra principale » fige la vue, appliquée
      à l'entrée en Play, marqueur cyan dans l'éditeur, édition/suppression dans l'inspecteur.
- [x] Migration JSON : tous les champs additifs sont #[serde(default)] (anciennes scènes OK).
- **Sprint 34 terminé** (lumières multiples + caméra de jeu ; entités pleinement typées = évolution future).
- [ ] (évolution) ECS/entités typées Light/Camera dans la liste d'objets, sélection 3D dédiée → ultérieur.
- [ ] Caméra de jeu comme objet : la vue Play utilise la caméra active (pas l'orbite éditeur).
- [ ] Migration JSON rétro-compatible (anciennes scènes : lumière globale → objet lumière).
- **Fichiers** : `src/scene/mod.rs`, `src/gfx/renderer.rs`, `src/app/mod.rs`, `src/editor/mod.rs`.
- **Livrable** : ajouter/déplacer lumières et caméras depuis le menu Ajouter ; rendu multi-lumières.
- **Risques** : refactor du modèle de scène → faire la migration JSON et étendre les tests d'abord.

### Sprint 35 — Pipeline d'assets & optimisation mobile ⬜
**Objectif** : rendre les assets portables et le panneau « Optimisation mobile » réel (différenciateur APK).
- [ ] Gestionnaire d'assets : copie/normalisation dans un dossier projet, chemins relatifs (`asset://`).
- [ ] Chargement de fichiers sur mobile (P10) : remplaçant de `rfd` (picker natif ou navigateur d'assets intégré).
- [ ] Compression/réduction de textures à l'export (taille max, mipmaps, formats GPU mobiles).
- [x] Panneau « Optimisation mobile » câblé : réduction réelle des textures (resize Lanczos3
      → copies _optN.png, objets mis à jour, undo) + limite de lumières.
- [x] Gestionnaire d'assets : schéma asset:// (~/.motor3derust/assets), read_bytes central
      (bundle:// / asset:// / disque), « Rassembler les assets » (copie + réécriture).
- [x] Chargement mobile (P10) : navigateur d'assets intégré (liste projet + embarqués,
      assignation de texture), fonctionne sans rfd.
- [~] Fusion des meshes statiques : descopée — l'instanciation du Sprint 33 collapse déjà
      les objets même mesh+matériau en un draw call (gain redondant).
- **Sprint 35 terminé.**
- **Fichiers** : `src/assets.rs`, `src/scene/{mod,import}.rs`, `src/editor/{export,mod}.rs`, `packaging/*`.
- **Livrable** : un projet s'exporte avec ses assets optimisés ; le Readiness Check reflète les gains.
- **Risques** : formats de texture GPU mobiles (ASTC/ETC2) → commencer par redimensionnement + PNG/mipmaps.

### Sprint 36 — Distribution signée & validation device ⬜
**Objectif** : finir la chaîne de livraison (reprise du Sprint 31) et valider la boucle sur appareil réel.
- [x] Override d'identité Android : build_apk.sh injecte BUNDLE_ID/APP_NAME/APP_VERSION
      dans Cargo.toml (puis restaure) ; release.yml fixe le versionName depuis le tag v*.
- [x] Validation device : checklist documentée (packaging/EXPORT.md).
- [ ] IPA signé en CI + notarisation macOS : dépendent de secrets/comptes (documentés, non activés).
- **Sprint 36 : cœur livré (identité Android + validation) ; signature stores = secrets à fournir.**
- [ ] **IPA signé en CI** (certificat + profil en *GitHub Secrets*), job iOS dans `release.yml`.
- [ ] Notarisation macOS ; tag `v*` → 3 artefacts signés attachés à la Release, `build_number` cohérent.
- [ ] **Validation device** : checklist + procédure (joystick → script → APK installé → resume arrière-plan).
- **Fichiers** : `packaging/*.sh`, `.github/workflows/release.yml`, `Cargo.toml`, `src/editor/export.rs`.
- **Livrable** : un tag produit `.dmg` notarisé + `.apk` + `.ipa` signés ; boucle mobile validée sur 1 appareil.
- **Risques** : secrets/comptes développeur requis → *GitHub Secrets* uniquement ; tester tôt sur device.

### Sprint 37 — IA avancée & confort d'édition ⬜
**Objectif** : capitaliser sur l'IA et combler les manques d'ergonomie de l'éditeur.
- [x] IA « Ajouter à la scène » (vs remplacer) : objets + lumières générés ajoutés à la scène.
- [x] Historique des prompts IA (8 récents, ré-exécution en un clic).
- [x] Gizmo multi-objets en rotate/scale autour d'un pivot commun (centroïde de la sélection).
- [x] Glisser-déposer hiérarchie : déposer un objet sur un groupe le range, sur un autre objet le réordonne (passe par l'historique).
- [ ] (évolution) Sous-groupes imbriqués → ultérieur (DnD egui).
- **Sprint 37 : livré (IA itérative + historique + gizmo multi + DnD réordonner/regrouper) ; sous-groupes = évolution.**
- [ ] **Historique des prompts** par projet ; ré-exécution en un clic.
- [ ] Hiérarchie : **glisser-déposer** pour réordonner/regrouper, **sous-groupes** imbriqués.
- [ ] Gizmo **multi-objets en rotate/scale** (pivot commun) ; raccourcis d'alignement/distribution.
- **Fichiers** : `src/app/{ai,mod}.rs`, `src/editor/mod.rs`, `src/scene/mod.rs`.
- **Livrable** : workflow IA itératif + édition multi confortable proche d'un éditeur pro.
- **Risques** : fusion IA↔scène existante (indices/sélection) → passer par des opérations validées + undo.

---

## PHASE G — Éditeur produit orienté Android

> **But.** Transformer l'éditeur d'un démonstrateur technique en un **produit lisible
> avec une promesse claire** : *créer une scène 3D en Rust → ajouter des contrôles mobiles
> → exporter un APK → tester sur téléphone*. Stack confirmée : **Rust + winit
> (`android-activity`) + wgpu (Vulkan)** + `cargo-apk`. La boucle d'usage cible :
> `Créer scène → Ajouter objets → Ajouter caméra → Ajouter joystick mobile → Build APK → Installer sur Android`.
>
> Une partie de cette vision est **déjà en place** (Sprint 32 : barre de menus, console,
> profiler FPS, **APK Readiness Check**, contrôles tactiles ; Sprints 33–35 : PBR,
> lumières multiples, caméra de jeu, optimisation mobile). La Phase G **complète** les
> menus, le panneau de build et les composants mobiles pour atteindre l'UI cible.

### Sprint 38 — Menus complets & barre du haut « produit » 🟢
**Objectif** : compléter la barre de menus et la toolbar pour couvrir la boucle d'usage.
- [x] **Fichier** : Nouveau projet, Ouvrir…, Enregistrer, **Enregistrer sous…**,
      Importer glTF, Build & Export (= Exporter APK, ouvre le Build Panel),
      **Paramètres projet…** (ouvre Build Panel + fenêtre Paramètres), Quitter.
- [x] Édition : Couper (Cmd+X), Copier, Coller, Tout sélectionner (Cmd+A), Grouper, Dégrouper (menu + raccourcis),
      Préférences (les autres existent déjà : Annuler/Rétablir/Dupliquer/Supprimer/Aligner au sol/Reset transform).
- [x] **Barre du haut** : `▶ Play | ⏸ Pause | ■ Stop | Déplacer | Tourner | Redim. |
      Snap | Grid | Aperçu mobile | Suivi caméra | Build APK | 📲 Run Device`.
- [x] **Aide** : Raccourcis clavier, **Guide export APK**, dépôt GitHub,
      À propos, **Diagnostic système** (Rust/Cargo/SDK/NDK/backend GPU).
- [ ] (évolution) bascule **2D/3D** + repère gizmo **Local/Global** → ultérieur (caméra ortho + état d'espace).
- **Fichiers** : `src/editor/mod.rs`, `src/editor/export.rs`, `src/app/mod.rs`, `src/gfx/renderer.rs`.
- **Livrable** : tous les menus de l'UI cible présents et branchés ; toolbar avec Pause/Stop/Snap/Grid/Build APK/Run Device.
- **Sprint 38 : livré (menus Fichier/Édition/Ajouter/Outils/Aide + toolbar + Run Device) ; 2D-3D / Local-Global = évolution.**
- **Risques** : Snap/Grid/2D-3D = vraie logique d'édition (pas que de l'UI) → prévoir l'état associé.

### Sprint 39 — Build Panel Android (fenêtre dédiée) 🟢
**Objectif** : remplacer un simple « Export APK » par un **panneau de build professionnel**.
- [x] Fenêtre **Build Android** structurée en sections repliables :
  - **Application** : nom, package id, version, build #, orientation (Auto/Portrait/Paysage),
    Min SDK, Target SDK, **icône PNG**, **splash PNG**.
  - **Rendu** : backend Vulkan (info), qualité (Low/Medium/High), FPS cible (slider 30–120),
    ombres On/Off, MSAA (Off/2×/4×).
  - **Assets** : récapitulatif modèles/textures/sons (embarqués au build via `bundle://`).
  - **Signature** : iOS (Team/Identité/Profil) ; Android = keystore release du script.
  - **Actions** : Exporter (par cible) · Installer sur appareil · 📲 Run · 📋 Logs ADB · Tout exporter.
- [x] **APK Readiness Check** enrichi : SDK min>target, min SDK bas (Vulkan), icône manquante/introuvable,
      récap orientation/FPS/MSAA/ombres (en plus des checks scène/textures/identité existants).
- [x] Câblage build : orientation + min/target SDK injectés dans `Cargo.toml` par `build_apk.sh` ;
      FPS/ombres/MSAA persistés et transmis au build (env). Icône/splash : sélection + readiness.
- [ ] (évolution) Génération automatique des **mipmaps d'icône** + splash natif ; compression assets in-panel.
- **Fichiers** : `src/editor/{export,readiness}.rs`, `src/app/build_config.rs`, `packaging/build_apk.sh`.
- **Sprint 39 : livré (panneau structuré Application/Rendu/Assets/Signature/Actions + Logs ADB + readiness enrichi) ; mipmaps icône = évolution.**
- **Risques** : icône/splash → injection dans `cargo-apk` (métadonnées Android) à valider.

### Sprint 40 — Menu Ajouter complet (objets, lumières, caméras, physique, audio, UI mobile) 🟢
**Objectif** : structurer le menu Ajouter façon Unity, en priorisant le mobile.
- [x] Objet 3D : Cube, Sphère, Plan, Cylindre, Capsule, Terrain (sol subdivisé à relief doux).
- [x] Lumière (sous-menu) : Ponctuelle, **Spot (cône)**, Directionnelle (réinitialiser), Ambiante +0,1.
- [x] **Caméra** (sous-menu) : Principale (vue actuelle) + **Caméra de suivi (mobile)**.
- [x] **Physique (sélection)** : Corps statique, Rigidbody (dynamique), Trigger, Aucune — appliqué à l'objet sélectionné.
- [x] **Audio (sélection)** : choisir une source sonore pour l'objet sélectionné (son spatialisé géré dans l'inspecteur).
- [x] **UI mobile** (sous-menu) : Joystick, Bouton tactile, **Zone tactile plein écran** (input.btn.touch), **Barre de vie HUD**.
- [ ] (évolution) Ambient/Listener audio comme entités dédiées ; Texte/Bouton UI libres éditables dans l'overlay.
- **Fichiers** : `src/scene/mod.rs`, `src/editor/mod.rs`.
- **Sprint 40 : livré (menu Ajouter structuré façon Unity : Objet 3D / Lumière / Caméra / Physique / Audio / UI mobile) ; entités audio/UI libres = évolution.**
- **Risques** : Terrain, Spot Light, Trigger Zone = nouveaux sous-systèmes → MVP minimal d'abord.

### Sprint 41 — Composants d'inspecteur mobiles 🟢
**Objectif** : exposer dans l'inspecteur les composants par objet, dont les composants mobiles.
- [x] Composants standards édités dans l'inspecteur : Transform, Mesh Renderer (type de mesh),
      Material (metallic/roughness/emissive), Collider, Rigidbody, Script Lua, Audio Source, Touch Area.
- [x] Section **🧩 Composants mobiles (Android)** par objet :
  - **Input Receiver** : déplacement piloté par le joystick (plan X/Z, vitesse réglable) — câblé dans la boucle Play.
  - **Gyroscope Controller** : déplacement piloté par l'inclinaison (tilt.x/y) — câblé dans la boucle Play.
  - **Vibration Feedback** : retour haptique au tap, durée réglable (20–400 ms, défaut 80) — `vibrate()` natif/no-op desktop.
- [x] **Screen Safe Area** : rentre contrôles + HUD dans une marge sûre (encoche/bords) — flag `MobileControls.safe_area`.
- [x] Touch Button / Virtual Joystick / Gyroscope (tilt) : existants au niveau scène, exposés aux scripts.
- [ ] (évolution) Intensité de vibration légère/moyenne/forte + déclencheur collision ; capteurs réels via JNI complet.
- **Fichiers** : `src/scene/mod.rs`, `src/editor/mod.rs`, `src/app/mod.rs`.
- **Sprint 41 : livré (composants mobiles par objet : Input Receiver / Gyroscope / Vibration + Screen Safe Area) ; intensités/déclencheurs = évolution.**
- **Risques** : gyroscope/vibration = **capteurs natifs** Android (JNI/`android-activity`) → repli no-op sur desktop.

### Sprint 42 — Menu Outils & optimisation mobile complète 🟢
**Objectif** : faire du menu Outils le centre de contrôle qualité/perf mobile (différenciateur).
- [x] Outils : Gestionnaire d'assets, Console, Profiler FPS + mémoire (objets/meshes/textures),
      Build Android, **Gestionnaire de scripts Lua** (liste + aperçu + sélection), Optimisation mobile,
      Contrôle qualité APK.
- [x] **Bake lighting** : fige les lumières ponctuelles en émission statique (selon distance/portée) puis les supprime — réduit les lumières dynamiques (annulable).
- [x] **Convertisseur textures** : redimensionne aux **puissances de 2** (mip-mapping/compression GPU) — copies `…_pot.png` (annulable).
- [x] **Optimisation mobile** étendue : réduire textures (fait), limiter lumières (fait), **Mode performance Android** (textures ≤ 1024 + ≤ 4 lumières en un clic) ; préréglage performance du rendu (qualité basse/ombres off/MSAA off) dans le Build Panel.
- [ ] (évolution) **Fusion meshes statiques**, **LOD**, **occlusion culling** : grisés dans le panneau (vrais sous-systèmes de rendu).
- **Fichiers** : `src/editor/{mod,export}.rs`, `src/app/mod.rs`, `src/gfx/renderer.rs`.
- **Sprint 42 : livré (gestionnaire scripts Lua, bake lighting, convertisseur POT, mode performance Android) ; fusion meshes / LOD / occlusion = évolution.**
- **Risques** : LOD / occlusion culling = vrais sous-systèmes de rendu → planifier en incréments.

> **Définition de « terminé » Phase G** : la boucle produit complète est réalisable
> *sans ligne de commande* — créer une scène, ajouter un joystick mobile et une caméra,
> ouvrir le Build Panel Android, passer le Readiness Check, builder, installer et lancer
> l'APK sur un téléphone connecté.

---

## PHASE H — Jouabilité mobile sans script & performance

### Sprint 43 — Contrôleur de personnage sans script ✅
**Objectif** : rendre un objet jouable au doigt sans écrire de Lua.
- [x] **Input Receiver** : un objet « pilotable » devient un corps **dynamique** rapier
      (rotations bloquées), piloté en **vitesse** par le joystick → **collisions** avec le décor statique.
- [x] **Saut** sur bouton tactile (impulsion verticale quand au sol), vitesse + hauteur réglables.
- [x] **Caméra qui suit** l'objet pilotable (`player_position` priorise `input_receiver`).
- [x] **Actions au tap sans script** : `TapAction` = Changer couleur / Masquer (ramasser, champ `visible`).
- [x] **Démo contrôleur** (Fichier ›) + **JSON pré-généré** (`assets/examples/demo_controleur.json`,
      via `examples/gen_controller_demo.rs`) ; **récap « scène embarquée »** dans le Build Panel.
- **Fichiers** : `src/scene/mod.rs`, `src/runtime/physics.rs`, `src/app/mod.rs`, `src/editor/{mod,export}.rs`.
- **Tests** : déplacement au joystick + collision sur mur (rapier headless), `hue_to_rgb`.

### Sprint 44 — Optimisations rendu ✅
**Objectif** : alléger le chemin de rendu par frame.
- [x] **Culling/LOD des lumières** : seules les 8 plus proches de la caméra envoyées au shader.
- [x] **Zéro allocation par frame** : tampons d'ordre + d'uniformes réutilisés.
- [x] **Re-tri d'ordre paresseux** : tri (groupage mesh/texture) seulement quand le nb d'objets change.
- [x] **Plan de dessin par index** : mesh/texture relus depuis `scene.objects` au draw (0 clone/frame).
- **Fichiers** : `src/gfx/renderer.rs`, `src/scene/mod.rs`.

---

## PHASE I — Robustesse & découplage (à venir)

> Passer d'un **éditeur-produit jouable** à une **base robuste et distribuable**.

### Sprint 45 — Découpler simulation & rendu 🟢
**Objectif** : la simulation ne doit plus suivre la cadence de rendu.
- [x] Boucle de mise à jour à **pas fixe** (1/60 s, accumulateur) pour physique + scripts, indépendante du FPS.
- [x] Garde-fous : **cap** de sous-pas (anti « spirale de la mort »), reliquat jeté, reset à Play/Pause.
- [x] `sim_step(dt)` isolé + `fixed_substeps()` pure et **testée** (30/60/120 FPS, gel borné).
- [ ] (évolution) **Interpolation** de rendu entre deux pas (fluidité à FPS très variable) → ultérieur.
- **Fichiers** : `src/app/mod.rs` (`advance_play`/`sim_step`/`fixed_substeps`).
- **Sprint 45 : livré (pas fixe + cap + test framerate-indépendant) ; interpolation de rendu = évolution.**

### Sprint 46 — Durcir l'initialisation 🟢
**Objectif** : éviter les crashs froids, surtout sur mobile.
- [x] Init GPU/fenêtre/resume entièrement sur `Result` + `match` + `log::error!` + `exit` (déjà en place, vérifié).
- [x] Caps de surface vides (`formats`/`alpha_modes`) → erreur propre au lieu d'indexer `[0]` (panic).
- [x] Audit : **0 `unwrap()`/`expect()` en code de production** (tous confinés aux tests) ; lookup texture par défaut sûr.
- **Fichiers** : `src/gfx/renderer.rs`, `src/lib.rs`. **Réf.** : Audit P4.
- **Sprint 46 : livré (init sans panic, code de prod sans unwrap).**

### Sprint 47 — Tests étendus (dirty-tracking reporté) 🟢
**Objectif** : élargir la couverture ; sauter le travail inutile au repos.
- [x] Tests : **saut du contrôleur** (s'élève), collision sur mur, **round-trip JSON** des composants
      (input_receiver, jump, tap_action, visible), défauts rétro-compat (`visible=true`).
- [ ] (reporté) **Compteur de révision** de scène → sauter rebuild models/draw plan au repos.
      Raison : la boucle **throttle déjà à 16 Hz** au repos (gain marginal) ; un mauvais critère
      « dirty » figerait l'affichage sur édition d'inspecteur → à faire **avec validation visuelle**.
- **Fichiers** : `src/runtime/physics.rs`, `src/scene/mod.rs`.
- **Sprint 47 : tests livrés ; skip-rebuild reporté (gain marginal vs risque non vérifiable).**

### Sprint 48 — Capteurs & assets mobiles ⬜
**Objectif** : brancher le matériel Android réel.
- [ ] **Gyroscope natif** (NDK `ASensorManager`) → alimente `input.tilt` (repli no-op desktop).
- [ ] **Vibration native** Android (au lieu du log desktop).
- [ ] **Import d'assets sur mobile** (lever P10 : `rfd` désactivé sans remplacement).
- **Fichiers** : `src/lib.rs` (`android_main`), `src/runtime/`, `src/app/input.rs`.
- **Risque** : code plateforme **à valider sur appareil** (pas de repli testable en CI).

### Sprint 49 — Distribution signée ⬜
**Objectif** : livrables prêts pour les stores.
- [ ] **IPA signé en CI** (certificat + profil en *GitHub Secrets*), job iOS dans `release.yml`.
- [ ] **Notarisation macOS** ; signature *distribution* Android (clé release dédiée).
- **Fichiers** : `.github/workflows/release.yml`, `packaging/*`. **Risque** : comptes/secrets requis.

> **Pistes long terme (Phase J)** : WebGPU/WASM, ECS léger, LOD / occlusion culling /
> fusion de meshes statiques, éditeur tournant sur mobile.

---

## ✅ Définition de « terminé » par phase

- **A** : éditeur confortable — gizmos, import glTF, undo, duplication fonctionnent.
- **B** : une scène devient un mini-jeu — script + physique + audio en mode Play.
- **C** : la même scène tourne en mode Player sur iOS (et Android).
- **D** ✅ : depuis l'app de dev (.dmg), on exporte un **player** du jeu créé en `.dmg` / `.apk` / `.ipa`
  **en un clic** (scène embarquée), avec config éditable/persistée, presets, install device et CI de release.
- **E** : le jeu exporté tourne partout **assets compris** ; édition avancée (multi-sélection, sous-groupes,
  renommage) ; rendu avec matériaux/ombres ; identité de bundle et cycle de vie mobile durcis.
- **F** : reprise sécurisée (exports/devices validés, tests élargis) ; édition et rendu terminés
  (multi-3D, sous-groupes, ombres, textures) ; livrables **signés** prêts pour les stores.
- **G** ✅ : boucle produit **sans ligne de commande** — menus/toolbar complets, Build Panel Android,
  menu Ajouter façon Unity, composants mobiles, outils & optimisation.
- **H** ✅ : un objet est **jouable au doigt sans script** (joystick + saut + collisions + caméra suivi,
  actions au tap) ; le chemin de rendu est **sans allocation par frame**.
- **I** : base **robuste & distribuable** — simulation à pas fixe, init sans panic, capteurs mobiles
  natifs, et livrables **signés** pour les stores.

## 📌 Conseils d'exécution
1. **Faire le Sprint 7 en premier** : sans le refactor, chaque portage dupliquerait du code.
2. **Garder le mode Player (Sprint 14) comme cible de test** mobile — pas l'éditeur complet.
3. **Tester sur device tôt** (Sprint 16) : les surprises GPU/cycle de vie viennent du matériel réel.
4. Avancer **une plateforme à la fois**.
