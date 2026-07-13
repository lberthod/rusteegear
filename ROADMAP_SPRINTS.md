# RusteeGear — Plan de sprints d'exécution (post-MVP)

> Feuille de route **étape par étape** pour faire évoluer le moteur du MVP actuel
> jusqu'au mobile (iOS / Android).
> Chaque sprint a : **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**.
> Convention : un sprint ≈ 1 à 3 jours. On ne démarre un sprint que si le précédent
> est « vert » (livrable validé).

État de départ : **MVP complet**.

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
| **G — Éditeur produit orienté Android** | 33 → 37+ | Boucle produit sans ligne de commande |
| **H — Jouabilité mobile sans script** | — | Objet jouable au doigt, rendu zéro-alloc |
| **I — Robustesse & découplage** | 45 → 49 | Pas fixe, init sans panic, capteurs, signature |
| *(Multijoueur en ligne)* | 50 → 79 (+ 80, 82 réseau) | Voir section **🌐 Multijoueur en ligne** ci-dessous |
| **K — Filet de sécurité** | 80 → 83 | Golden tests rendu, temps maîtrisé (time scale/step), console dev, debug drawing |
| **L — Animation squelettale** | 84 → 88 | Skinning glTF → blending → réplication réseau |
| **M — Image** | 89 → 92 | Ciel/fog, HDR + tone mapping, bloom, mipmaps |
| **N — Chaîne gameplay** | 93 → 99 | Événements → prefabs → spawn/destroy Lua → save |
| **O — Physique & feel** | 100 → 103 | Exposer rapier (trimesh, CCD, couches), character controller |
| **P — Audio, HUD & confort** | 104 → 110 | Bus/panning, widgets HUD, manettes, hot-reload, profiler GPU |
| **Q — Web (ex-« pistes Phase J »)** | 111 → 114 | WASM/WebGPU, multijoueur navigateur, vitrine publique |
| **R — WebXR** | 115 → 117 | Casque dans le navigateur (spike isolé → rendu stéréo → tests IWE) |
| **S — Extensions quasi-gratuites** | 118 → 127 | Suites peu coûteuses de K/L/M/N/O/P (audio confort, post-effets HDR, SSAO, pipeline assets, outillage éditeur…) pour dépasser 100/200 sur la grille des 200 fonctionnalités |

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
> la distribution. Lire **[README.md](README.md)** et
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

### Sprint 47 — Tests étendus & skip-rebuild ✅
**Objectif** : élargir la couverture ; sauter le travail inutile au repos.
- [x] Tests : **saut du contrôleur** (s'élève), collision sur mur, **round-trip JSON** des composants
      (input_receiver, jump, tap_action, visible), défauts rétro-compat (`visible=true`).
- [x] **Skip-rebuild par hash** : `render_input_hash` couvre caméra + transforms/couleurs/matériau/
      surbrillance/mesh/texture/visibilité. Hash identique ⇒ sortie identique ⇒ pas de reconstruction
      (matrices, inverse-transposée, upload d'instances). **Sûr par construction** (tout changement
      modifie le hash → pas de frame périmée).
- **Fichiers** : `src/runtime/physics.rs`, `src/scene/mod.rs`, `src/gfx/renderer.rs`.
- **Sprint 47 : livré (tests + skip-rebuild par hash, sans risque d'affichage figé).**

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

> **Pistes long terme (ex-Phase J)** : WebGPU/WASM (→ désormais planifié en **Phase Q**),
> ECS léger, LOD / occlusion culling / fusion de meshes statiques, éditeur tournant sur mobile.

---

## 🌐 Multijoueur en ligne (50 → 79, + 80/82 réseau) — VPS, Firebase, jeu en ligne

> Numérotation **indépendante** du tronc solo ci-dessus (continue à 50 là où le solo
> s'arrêtait à 49) ; détail complet des sprints dans **[SPRINT_MMORPG.md](SPRINT_MMORPG.md)**
> (50-65, puis 80/82) et **[SPRINTNETWORK.md](SPRINTNETWORK.md)** (66-79, suite directe de
> [AUDIT_LATENCE_MULTIJOUEUR.md](AUDIT_LATENCE_MULTIJOUEUR.md)). Cette section résume ce qui
> est **réalisé** ; elle ne remplace pas les documents source.
>
> **Scope verrouillé (2026-07-07)** : petit multi en ligne, **2–16 joueurs par salon**
> (pas de monde persistant partagé) ; serveur de jeu Rust **autoritaire** (WebSocket) pour
> le temps réel (position/combat) ; **Firebase Realtime Database** en backend annexe
> seulement (comptes, progression, chat, classement, présence) — RTDB n'a pas d'autorité
> serveur, il ne transporte jamais la simulation temps réel.
>
> ⚠️ **Collision de numérotation** : les sprints réseau **80** et **82** ci-dessous
> (« Vie individualisée… » et « Multi-salons ») portent les **mêmes numéros** que les
> sprints **80** et **82** du tronc solo (PHASE K, « Golden tests de rendu » et « Console
> développeur », ci-dessous) — deux chantiers distincts numérotés en parallèle par erreur
> (sessions concurrentes, cf. mémoire `concurrent-sessions-hazard`). Assumé tel quel dans
> les documents source plutôt que renuméroté rétroactivement ; ne pas confondre les deux
> en lisant « Sprint 80 »/« Sprint 82 » sans préciser le tronc.

### PHASE M-net — Préparation (50)

#### Sprint 50 — Extraire le gameplay combat de `app/mod.rs` ✅ FAIT
Isolé attaque/manches/IA dans `src/app/combat.rs`, point d'extension pour le serveur
réseau, refactor pur (aucun changement de comportement, 83/83 tests verts).

### PHASE N-net — Serveur & protocole (51 → 53)

#### Sprint 51 — Serveur de jeu headless ✅ FAIT
Binaire `src/bin/server.rs` (aucun appel `gfx`/`editor`/`winit`), boucle à tick 20 Hz
découplée du pas fixe physique 60 Hz. `cargo run --bin server` fait tourner une manche
sans fenêtre.

#### Sprint 52 — Protocole réseau & sérialisation ✅ FAIT
`src/net/protocol.rs` : `ClientMsg`/`ServerMsg`/`Snapshot`/`EntityDelta` en `bincode`.
10 tests de round-trip ; snapshot de 20 entités mesuré à 536 octets (~27 octets/entité).

#### Sprint 53 — Transport WebSocket + connexion client ✅ FAIT
`tokio` + `tokio-tungstenite` (desktop/Android uniquement, `ws://` sans TLS) ;
`NetServer`/`NetClient` exposent des canaux `mpsc` synchrones au reste du programme.
95/95 tests verts, serveur réel écoutant sur `127.0.0.1:7777`.

### PHASE O-net — Client réseau (54 → 55)

#### Sprint 54 — Prédiction client & interpolation 🟢 (cœur livré, câblage réel au Sprint 63)
`src/net/interpolation.rs` : historique borné, interpolation position/yaw (chemin
court), réconciliation à seuil (`SNAP_THRESHOLD` 0,5 m). 102/102 tests verts.

#### Sprint 55 — Salons multijoueurs (lobby, join/leave) 🟢 (cœur serveur fait, UI câblée au Sprint 63)
`src/app/multiplayer.rs` : un joueur réseau = objet indépendant avec son propre
`NetworkInput`, routé dans `sim_step` sans changer le comportement solo. Test
bout-en-bout à travers un vrai socket. 108 tests lib + 1 bin verts.

### PHASE P-net — Firebase Realtime Database, backend annexe (56 → 59)

#### Sprint 56 — Comptes & authentification 🟢 (client REST fait, écran câblé plus tard)
`src/net/firebase.rs` : `sign_up`/`sign_in`/`set_profile_name`/`get_profile_name` (API
REST Firebase Auth/RTDB). Clé API + URL RTDB dans les Paramètres. 114 tests verts.

#### Sprint 57 — Inventaire & progression persistante 🟢 (câblage serveur fait, non vérifié en réel)
`PlayerProgress { level, xp }` ↔ `/users/{uid}/progress` ; seul le **serveur** (compte
Firebase dédié) écrit la progression, jamais le client (anti-triche). 117 tests verts.

#### Sprint 58 — Chat de salon & présence 🟢 (REST fait, SSE temps réel non retenu)
`ChatMessage`/`Presence` sur RTDB via polling REST (SSE écarté : incompatible avec la
boucle `winit` sans thread dédié supplémentaire, pas justifié à cette échelle).
123 tests verts.

#### Sprint 59 — Classement (leaderboard) 🟢 (backend + câblage serveur faits, UI câblée au Sprint 65)
`LeaderboardEntry` sur `/leaderboard`, écrit par le serveur en fin de manche, lu par
polling public. 126 tests verts. Risque documenté : pas de purge, à corriger avant
usage soutenu.

### PHASE Q-net — Robustesse & mise en production (60 → 62)

#### Sprint 60 — Durcissement réseau & anti-triche de base ✅ FAIT
**Bug réel corrigé** : un `NaN` reçu du réseau traversait `f32::clamp` sans filtre et
corrompait la position simulée — `sanitize_network_input` le rejette désormais.
Ajouté : cooldown d'attaque réseau validé serveur, timeout client (10 s). 130 tests
lib + 2 bin verts.

#### Sprint 61 — Tests de charge & optimisation bande passante ✅ FAIT
`examples/load_test_client.rs`, 16 bots réels à 20 Hz : traitement serveur ~0,4 ms/tick
(1 % du budget à 20 Hz), 6,76 Ko/s/joueur descendant — largement sous l'objectif, aucune
optimisation nécessaire à cette échelle (décision mesurée, pas anticipée).

#### Sprint 62 — Déploiement serveur ⬜
VPS simple, packaging du binaire serveur (cross-compile Linux), `wss://` obligatoire en
prod. **Seul sprint réseau du plan initial resté non fait.**

### Hors plan initial, demandés directement par l'utilisateur (63 → 65)

#### Sprint 63 — Client réseau desktop & fenêtre Multijoueur ✅ FAIT
`src/app/network_client.rs` + fenêtre **🌐 Multijoueur** dans l'éditeur : câble enfin
`NetClient` dans la boucle réelle (envoi d'`Input`, fantômes distants interpolés,
connexion Firebase optionnelle). **Bug critique trouvé et corrigé en testant réellement** :
le serveur perdait la manche tout seul avant qu'un joueur ne rejoigne (heuristique
`player_index` désignant un monstre ou le sol). 136 tests lib + 2 bin verts.

#### Sprint 64 — Chat en jeu ✅ FAIT
Fenêtre Multijoueur → section Chat, branchée sur le backend Firebase du Sprint 58.
137 tests lib + 2 bin verts.

#### Sprint 65 — Classement en jeu ✅ FAIT
Fenêtre Multijoueur → section Classement, branchée sur le backend du Sprint 59.
138 tests lib + 2 bin verts.

### Suite directe : latence & qualité du mode en ligne (66 → 79)

> Après [AUDIT_LATENCE_MULTIJOUEUR.md](AUDIT_LATENCE_MULTIJOUEUR.md) (2026-07-12) : chaque
> sprint corrige un symptôme **mesuré en jeu réel** (vidéo, VPS), pas anticipé.

#### Sprint 66 — Lissage de la réconciliation du joueur local ✅ FAIT
Correction dure (« snap ») remplacée par un lissage progressif — fin de la
téléportation visible en cas de désaccord serveur/client.

#### Sprint 67 — Délai d'interpolation pour les fantômes distants ✅ FAIT
Supprime les gels/saccades des joueurs distants affichés.

#### Sprint 68 — Plafonnement du débit d'`Input` client ✅ FAIT
N'envoie plus un `Input` à chaque frame de rendu, seulement au rythme utile.

#### Sprint 69 — Vérification géographique du serveur de test ⬜ (infra, pas code)
Établir si les 150-250 ms de RTT mesurés viennent de la distance géographique du VPS —
nécessite un accès réel non disponible en environnement de dev.

#### Sprint 70 — Cohérence doc/code du `Snapshot` ✅ FAIT
Lève une divergence entre la documentation et le comportement réel du `Snapshot`.

#### Sprint 71 — Transport non-TCP ⬜ (conditionnel, non déclenché)
Prévu seulement si les Sprints 66-68 ne suffisent pas — jugé non nécessaire.

#### Sprint 72 — Interpolation de rendu à pas fixe ✅ FAIT (`0aa0b5d`)
**Cause du « judder »** : simulation à pas fixe 1/60 s, rendu affichant la dernière pose
brute. **Correctif** : `blend_render_poses` mélange les deux derniers pas pondérés par
l'accumulateur ; téléportations exemptées (`TELEPORT_SNAP_PER_STEP`).

#### Sprint 73 — Game feel du déplacement ✅ FAIT (`e7695fe`)
Constantes documentées (`BRAKE_FACTOR`, `AIR_CONTROL`, `FALL_GRAVITY_FACTOR`),
rotation en amorti exponentiel indépendant du framerate.

#### Sprint 74 — Réconciliation par trajectoire récente ✅ FAIT (`718fb1d`)
**Cause du tremblement mesuré** : la position serveur, toujours en retard d'une
latence + un tick, dépassait `SNAP_THRESHOLD` en continu à vitesse constante.
**Correctif** : historique 1 s de la trajectoire prédite — une position serveur proche
d'un point récent n'est plus corrigée (« en phase, juste en retard »).

#### Sprint 75 — Convention d'axes de la poussée W/S ✅ FAIT (`04c0cc6`)
**Bug de signe trouvé** : le client envoyait W/S en Z monde, le serveur attendait la
convention joystick — corrigé, test de bout en bout par yaw ajouté.

#### Sprint 76 — Boutons tactiles/gyro dans l'`Input` réseau ✅ FAIT (`62cf640`, `619b5a6`)
Saut/attaque tactiles et gyroscope désormais transmis au serveur (invisibles avant) ;
croix directionnelle tactile devenue pavé tank W/A/S/D (glyphes ▲▼ absents sur Android).

#### Sprint 77 — Rattrapage doux à l'arrêt + serveur VPS aligné ✅ FAIT (`1f00598`)
**Cause du décalage mesuré** (positions différentes selon l'appareil à l'arrêt) :
serveur et client sur des versions de physique différentes. **Correctif** : convergence
douce (5 %/frame) sous 3 cm d'écart à l'arrêt + **VPS recompilé sur le même commit**
que les clients.

#### Sprint 78 — Boule de feu + monstres sur la carte multijoueur ✅ FAIT
Première attaque à distance (`src/app/fireball.rs`), recharge validée **côté serveur**
(anti-spam), 5 monstres avec PV sur la carte embarquée, diffusés dans le `Snapshot`
(`player_id: None`).

#### Sprint 79 — Visée réelle, multi-armes et changement d'arme ✅ FAIT
**Audit du Sprint 78, trois trous trouvés** : orientation des joueurs réseau jamais
appliquée côté serveur (`aim_yaw` ajouté au protocole), une seule arme (table
`RANGED_WEAPONS` : Boule de feu/Éclair/Boulet), pas de changement d'arme (clavier 1/2/3
+ bouton tactile, borné côté serveur).

### Sprints réseau numérotés 80 et 82 (⚠️ collision avec PHASE K solo, voir avertissement ci-dessus)

#### Sprint 80 (réseau) — Vie individualisée, IA multi-cibles, soin coopératif ✅ FAIT
`src/app/health.rs` : vie par joueur (`HashMap<PlayerId, f32>`), mort = spectateur sans
fin de manche pour les autres (`is_room_lost`) ; IA multi-cibles (chaque `AiChaser`
poursuit le joueur vivant visible le plus proche) ; soin coopératif touche **H**/bouton
tactile, validé serveur. 209 tests verts. Incident de session concurrente géré sans
perte (cf. `concurrent-sessions-hazard`).

#### Sprint 82 (réseau) — Multi-salons ✅ FAIT
`src/bin/server.rs` : `HashMap<String, Room>` remplace l'`AppState` unique ; salon
choisi/créé via `ClientMsg::Join::lobby`, `DEFAULT_LOBBY` préservé pour aucune
régression client. Une manche décidée ne coupe plus tout le process
(`Room::restart()` isolé par salon). 220 tests lib + 4 bin verts.

### Reste ouvert côté réseau

- **Sprint 62** — déploiement VPS proprement documenté/scripté (`packaging/deploy_server.sh`).
- **Sprint 69** — vérification géographique du VPS de test (infra).
- **Sprint 71** — transport non-TCP (seulement si nécessaire).
- Écrans UI reportés faute d'affichage graphique vérifiable en environnement de dev :
  lobby, connexion Firebase, reconnexion avec la même identité, sélection de salon
  dans l'éditeur — tous fonctionnels côté backend/serveur, jamais vus tourner ici.

---

## 🚀 Phases K → Q — Vers un moteur pertinent (sprints 80 → 114)

> Issues de l'**audit comparatif à 200 fonctionnalités** (Godot / Unity / Unreal / RusteeGear,
> 2026-07-13) : RusteeGear couvre ~27 % de la grille, avec un profil vertical (noyau, physique
> rapier, éditeur, réseau prédit/réconcilié tiennent la comparaison ; l'animation, le rendu
> d'image et l'audio avancé sont les continents manquants). Ces 7 phases exécutent les
> chantiers retenus 🟢 — beaucoup débloqué pour peu de code lisible, souvent en exposant ce que
> `rapier3d`/`kira` savent déjà faire — et **aucun** des refus assumés (pas de boîte noire, pas
> d'ECS/render graph/plugins/réflexion, pas de GI/Nanite, pas de consoles sous NDA, pas de
> télémétrie automatique).
>
> Logique d'ordre : **K d'abord** (L et M réécrivent le pipeline de rendu — sans golden tests,
> chaque sprint shader est un saut sans filet) ; **L avant M** (le skinning impose le système de
> variants WGSL dont l'HDR profitera) ; **N strictement ordonnée** (chaque sprint consomme le
> précédent) ; **O après N** (les requêtes physiques émettent des événements) ; **P** = réservoir
> de sprints tampons insérables après K ; **Q ferme la boucle** quand il y a quelque chose à
> montrer. Les sprints 50 → 78 étant pris par le multijoueur, on démarre à 80 (79 = tampon).

### PHASE K — Filet de sécurité (80 → 83) ✅

> **Phase K — Filet de sécurité : terminée** (Sprints 80→83). Golden tests headless,
> simulation maîtrisée (time scale + pas unique), console de commandes, debug drawing
> (Rust + Lua) et sélecteur de vue normales/profondeur. La Phase L (animation squelettale)
> peut commencer sans sprint de rendu à l'aveugle.

#### Sprint 80 — Golden tests de rendu 🟢
**Objectif** : ne plus jamais toucher un shader sans filet.
- [x] Rendu **headless** wgpu (`Renderer::new_headless` + `render_scene_headless`, sans fenêtre/
      surface/UI, mêmes shaders/pipelines que `render()`) — 1 scène de référence livrée
      (primitives + lumières + ombre) ; glTF+ombres et démo contrôleur restent à ajouter au même
      harnais (`tests/golden_render.rs`).
- [x] Comparaison aux images « golden » avec **seuil de tolérance** par canal (`tests/golden/`).
- [x] Commande de re-génération documentée (`UPDATE_GOLDEN=1 cargo test --test golden_render`).
- [x] CI (`ubuntu-latest`, sans GPU) : le test **saute proprement** au lieu d'échouer en permanence.
- **Fichiers** : `src/gfx/renderer.rs`, `tests/golden_render.rs`, `tests/golden/`.
- **Livrable** : vérifié en conditions réelles — golden régénéré sur GPU Metal, puis une régression
  injectée dans `main.wgsl` a fait échouer le test avant d'être révertée.
- **Risque** : différences GPU CI/local → absorbé pour l'instant en sautant le test sans GPU plutôt
  qu'en installant un rasteriseur logiciel (lavapipe) en CI.

#### Sprint 81 — Temps maîtrisé (time scale, step frame) 🟢
**Objectif** : rendre la simulation reproductible et inspectable.
- [ ] ~~RNG seedé par partie~~ — écarté : aucun `rand`/`thread_rng` dans le dépôt à ce jour, pas
      de consommateur actuel. À reprendre quand un besoin réel apparaîtra (loot, variation IA,
      particules…).
- [x] `AppState::time_scale` multipliant le `dt` **simulé** (physique/scripts) avant
      `fixed_substeps` — jamais le `dt` du compteur FPS ni `FIXED_DT` lui-même. Toolbar :
      préréglages ¼×/½×/1×/2×.
- [x] Bouton **« ⏭ »** en pause : `AppState::request_step` force exactement un pas fixe (accumulateur
      à 0 + dt forcé = `FIXED_DT`), consommé automatiquement, sans rattrapage.
- **Fichiers** : `src/app/mod.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs`.
- **Livrable** : testé bout en bout avec `AppState` réel et `dt` contrôlé (`last_frame` déplacé) —
  une frame gelée sans demande n'avance pas `self.time`, une frame avec demande avance de
  exactement 1/60 s, la frame gelée suivante n'avance plus.

#### Sprint 82 — Console développeur (cvars) 🟢
**Objectif** : multiplier la vitesse de debug de tout le reste.
- [x] Champ de saisie dans la fenêtre Console existante + registre de commandes :
      `timescale <v>` (clampé 0..8), `pause`/`play`/`stop`/`step` (recoupent la toolbar,
      accessibles au clavier), `tp <x> <y> <z>`, `net_stats`. ~~`give`~~/~~`seed`~~ retirés
      de la liste indicative : pas de système d'inventaire, pas de RNG à seeder (cf.
      Sprint 81) — ajouter des commandes le jour où ces systèmes existeront.
- **Fichiers** : `src/app/mod.rs` (`AppState::run_console_command`), `src/editor/mod.rs`.
- **Livrable** : 6 commandes testées bout en bout (`AppState` réel) — jamais de panique sur
  une saisie invalide, toujours un message de retour (usage ou erreur explicite).

#### Sprint 83 — Debug drawing + vues buffers 🟢
**Objectif** : voir ce que le moteur calcule.
- [x] `AppState::debug_line/debug_box/debug_sphere` **côté Rust**, sur le pipeline gizmo
      (buffer dédié, redimensionné au doublement), vidé après chaque frame de rendu — branché
      sur `render()` et `render_scene_headless`. Vérifié visuellement (exemple jetable).
- [x] Exposition **Lua** : `debug.line(x1,y1,z1,x2,y2,z2,r,g,b)`, même mécanique que
      `vibrate()`/`set_health()` (table accumulatrice relue après `func.call`). Les 16 sites
      d'appel de `run_script` (1 production + 15 tests) mis à jour ; `debug.box`/`debug.sphere`
      restent Rust-only (un script peut composer des lignes lui-même au besoin).
- [x] Sélecteur de vue (`DebugView` : Éclairé/Normales/Profondeur) dans la toolbar, encodé
      dans un canal inutilisé de l'uniform d'éclairage existant (`ambient.y`) plutôt que
      d'agrandir l'uniform. Profondeur linéarisée sur une échelle visuelle de 20 m (le
      near/far réel de la caméra, 0.1..100, écraserait toute scène compacte dans le même
      blanc) — ajusté après un premier essai peu lisible, vérifié visuellement.
- **Fichiers** : `src/app/mod.rs` (`run_script`, API Rust + Lua, `DebugView`),
  `src/gfx/renderer.rs` (pipeline, uniform), `src/gfx/shaders/main.wgsl` (branches de vue),
  `src/editor/mod.rs` (toolbar).
- **Livrable** : le clic de sélection en mode édition visualise le rayon de picking (ligne
  jaune) ; un script trace sa propre trajectoire avec `debug.line()` ; la toolbar bascule
  entre rendu éclairé, normales et profondeur sur la scène affichée.

### PHASE L — Animation squelettale (84 → 88)

#### Sprint 84 — Données de squelette 🟢
**Objectif** : lire `joints`/`weights`/bind poses du glTF — données pures, sans rendu.
- [x] `scene::import::load_gltf_skeleton(path)` : hiérarchie de joints (nom, parent,
      transform de liaison, matrice inverse de liaison) + poids de peau par sommet
      (`JOINTS_0`/`WEIGHTS_0`, jusqu'à 4 os). `Vertex`/`MeshData` inchangés — le rendu
      (skinning GPU) arrive au Sprint 86.
- **Fichiers** : `src/scene/import.rs` (`load_gltf` existant intact, juste factorisé via
  `read_document`, partagé avec la nouvelle fonction).
- **Livrable** : round-trip testé sur un `.glb` minimal construit à la main (2 joints en
  chaîne parent/enfant, poids non triviaux) — pas de fixture Mixamo réelle dans le dépôt,
  mais 5 tests couvrant hiérarchie/noms/poses de liaison/poids, dont 2 qui exercent l'API
  publique de bout en bout (fichier temporaire réel, avec et sans skin).

#### Sprint 85 — Échantillonnage de clips 🟢
**Objectif** : jouer un clip (keyframes, interpolation, boucle) côté CPU.
- [x] `scene::import::load_gltf_clips(path)` + `Clip::sample_joint(joint, time)` : canaux
      translation/rotation/scale, interpolation Linear (nlerp pour les rotations, conforme
      à la spec glTF) ou Step ; bouclage automatique (`rem_euclid`, robuste aux temps
      négatifs) ; `CubicSpline` ignoré (non géré) plutôt que mal interpolé en silence.
- [ ] Intégration visuelle (cube parenté à un bone dans l'éditeur) — reportée : nécessite
      la hiérarchie parent/enfant de transforms (item du noyau encore ◐, cf. audit 200
      fonctionnalités) et touche `app/mod.rs`. Le CPU pur est testé et fonctionnel
      indépendamment de cette intégration.
- **Fichiers** : `src/scene/import.rs` seul (`app/mod.rs` pas encore touché — voir ci-dessus).
- **Livrable** : vitesse d'interpolation vérifiée à des timestamps précis (test), bouclage
  et palier (step) vérifiés. Un vrai bug de fixture de test (race condition sur un chemin
  de fichier temporaire partagé entre threads de test) attrapé et corrigé avant qu'il ne
  devienne un échec intermittent en CI.

#### Sprint 86 — Skinning GPU 🟢
**Objectif** : le vertex shader déforme le mesh.
- [x] `SkinnedVertex` (type séparé de `Vertex` — les meshes statiques restent inchangés,
      cf. `src/gfx/mesh.rs`), `shaders/skinned.wgsl` (vertex de skinning, **fragment
      partagée** avec `main.wgsl` — aucune duplication de l'éclairage), palette de
      matrices en storage buffer (groupe 4, capacité 128 par instance, tronque plutôt
      que déborder).
- [x] `scene::import::compute_joint_matrices(skeleton, clip, time)` : matrice par joint
      côté CPU, robuste à un ordre `Skeleton::joints` sans garantie parent-avant-enfant.
- [x] **Vérifié visuellement** (pas juste compilé) : planche à charnière pondérée moitié
      joint 0 fixe / moitié joint 1 pivotant, rendue à 0°/45°/-90° — courbe lisse
      obtenue (preuve que le mélange de poids par sommet fonctionne, pas qu'un joint «
      gagne »), transformée en golden test permanent ; bug injecté puis reverté pour
      confirmer la détection (5.51 % de pixels divergents avant correctif).
- [x] **Intégration éditeur** (Sprint 87) : `SceneObject.animation`, lecture en Play,
      dessin réel dans `render()`/`render_scene_headless()` — cf. Sprint 87 ci-dessous.
- **Fichiers** : `src/gfx/mesh.rs`, `src/gfx/renderer.rs`, `src/gfx/shaders/skinned.wgsl`,
  `src/scene/import.rs`, `src/scene/mod.rs`, `src/app/mod.rs`, `tests/golden_skinning.rs`.
- **Risque (confirmé, mais maîtrisé)** : les layouts de bind groups ont bien été le point
  délicat annoncé — résolu en réutilisant le module `main.wgsl` pour l'étage fragment
  (wgpu autorise des modules vertex/fragment distincts si `VsOut` correspond exactement),
  évitant de dupliquer tout le code d'éclairage dans `skinned.wgsl`.

#### Sprint 87 — Intégration Play + blending + state machine ✅ FAIT
**Objectif** : des transitions douces pilotables en Lua.
- [x] **Intégration Play/rendu** (reportée du Sprint 86) : `SceneObject.animation`
      (clip + temps + vitesse), `sim_step` avance le temps (compatible `time_scale`,
      Sprint 81), `Renderer` dessine chaque objet skinné individuellement (`joint_buf`
      à créneaux + offset dynamique, `MAX_SKINNED_INSTANCES = 8` — plusieurs
      personnages animés distincts par frame, chacun sa propre palette de joints).
      Vérifié par un test d'intégration bout en bout (rend la même scène à deux temps,
      vérifie que les pixels diffèrent réellement) — pas seulement les briques isolées.
- [x] **Fondu enchaîné (crossfade)** : `AnimationState::set_clip()` démarre une
      transition (`prev_clip`/`prev_time`/`blend`, durée fixe `CROSSFADE_SECONDS = 0.2s`,
      le clip quitté continue de jouer pendant le fondu) ; `compute_joint_matrices_blended`
      mélange les deux clips au niveau des poses **locales** par joint (lerp
      translation/échelle, nlerp rotation) avant de composer la hiérarchie une seule
      fois — mélanger des matrices monde aurait été faux pour la rotation. Vérifié par
      3 tests CPU (extrémités, milieu, clamp) + 1 test de rendu bout en bout.
- [x] **Exposition Lua** (`obj.anim = "run"`) : `run_script` reçoit désormais
      `anim: &mut Option<AnimationState>` (signature + 16 sites d'appel, même geste que
      `debug.line()` au Sprint 83) ; le champ `obj.anim` est lu en écriture après
      l'appel Lua et route vers `AnimationState::set_clip()` — absent (`""`) ou
      inchangé ⇒ aucun redémarrage de fondu. N'existe que pour les objets skinnés
      (`obj.animation.is_some()`), silencieux sinon. 2 tests dédiés (démarre un fondu ;
      ne redémarre pas le clip courant si le script n'y touche pas) + 253 tests lib/4
      tests bin verts au total.
- **Fichiers** : `src/scene/mod.rs` (`AnimationState`), `src/app/mod.rs` (`sim_step`,
  `run_script`), `src/gfx/renderer.rs` (`draw_plan_skinned`, `prepare_skinned_draws`,
  `draw_skinned_objects`), `src/scene/import.rs` (`ImportedMesh::skinned_mesh_data`,
  `compute_joint_matrices_blended`).
- **Livrable restant, hors scope de ce sprint** : la démo mobile (`Scene::mobile_demo`)
  n'a pas de personnage skinné (capsule statique) — un joueur qui court/s'arrête/saute
  « sans à-coup, piloté par le script » demande un asset skinné avec clips
  idle/run/jump nommés, qui n'existe pas encore dans le dépôt (seul le golden test de
  skinning a un rig synthétique). Mécanisme Lua complet et testé ; contenu de démo à
  faire dans un sprint dédié plutôt qu'anticipé ici.

#### Sprint 88 — Animation répliquée ✅ FAIT
**Objectif** : les joueurs réseau s'animent aussi.
- [x] **`EntityDelta::anim_clip`** (`src/net/protocol.rs`) : nom du clip joué (vide =
      non skinné/pose de liaison), rempli par `AppState::network_snapshot`
      (`src/app/multiplayer.rs`) pour joueurs *et* monstres réseau. **Pas de temps
      de lecture répliqué** (écart avec le plan initial « clip + phase ») : chaque
      client avance déjà localement le temps de tout `AnimationState` à chaque pas
      fixe (`sim_step`, y compris pour un fantôme réseau ou un monstre — la boucle
      qui fait ça ne distingue pas l'origine de l'objet), donc seul le *choix* du
      clip a besoin d'être répliqué ; envoyer une phase en plus aurait été de la
      synchronisation non justifiée par un symptôme mesuré (cf. Sprint 61).
- [x] **Application côté client** (`src/app/network_client.rs`) : `poll_network`
      pousse `RemoteEntity::latest_anim_clip()` dans `AnimationState::set_clip()`
      du fantôme (fondu enchaîné inclus, Sprint 87) ; même geste pour les monstres
      réseau dans `handle_server_msg`. `EntityDelta` a perdu `Copy` (le nouveau
      `String` ne l'est pas) : `Timed<T>` (`interpolation.rs`) est passé à
      `Clone` seul, sans changement de comportement.
- [x] **Tests** : 2 dans `interpolation.rs` (`latest_anim_clip` suit le dernier
      snapshot, pas le premier), 2 dans `multiplayer.rs` (clip répliqué si
      `AnimationState` présent, vide sinon). 257 tests lib + 4 tests bin verts.
- **Fichiers** : `src/net/protocol.rs`, `src/net/interpolation.rs`,
  `src/app/multiplayer.rs`, `src/app/network_client.rs`.
- **Livrable restant, hors scope de ce sprint** : « deux clients voient le même
  perso courir » demande un joueur réseau avec un mesh skinné réel dans une
  scène jouée en ligne — aucune scène du dépôt n'en a un aujourd'hui (même
  constat que le livrable restant du Sprint 87) ; mécanisme complet et testé,
  contenu de démo à faire dans un sprint dédié.

### PHASE M — Image (89 → 92)

#### Sprint 89 — Ciel + brouillard ✅ FAIT
- [x] **Ciel** (`src/gfx/shaders/sky.wgsl`, nouveau) : dégradé horizon/zénith, dessiné en
      premier dans la passe principale via un triangle plein écran sans vertex buffer
      (pas de cube inversé — inutile ici), profondeur `Always`/pas d'écriture pour ne
      jamais l'emporter sur la géométrie réelle. La direction de vue est reconstruite à
      partir de `Camera::inv_view_proj` (nouveau champ, calculé une fois par frame dans
      `write_uniforms`) plutôt qu'un dégradé fixe en espace écran : sinon le ciel resterait
      immobile pendant qu'on oriente la caméra, un défaut visible immédiatement en testant.
- [x] **Brouillard exponentiel** dans `main.wgsl` (`fs_main`) : `1 - exp(-distance *
      density)`, mélangé vers `fog_color` juste avant le retour final.
- [x] **`scene::Sky`** (`src/scene/mod.rs`) : `horizon_color`/`zenith_color`/`fog_color`/
      `fog_density` sur `Scene`, `#[serde(default)]`. Par défaut, `horizon_color ==
      zenith_color == [0.07, 0.08, 0.1]` (l'ancienne couleur de clear fixe) et
      `fog_density = 0.0` : aucune scène existante ne change d'aspect tant que
      l'inspecteur n'y touche pas — le golden `primitives_lights.png` passe sans
      régénération.
- [x] **Inspecteur** (`src/editor/mod.rs`) : section « 🌫 Ciel & brouillard » sous
      l'éclairage, 3 couleurs + un curseur de densité.
- [x] **Goldens** : nouveau `tests/golden/sky_and_fog.png` (ciel/brouillard nettement
      réglés) + `sky_and_fog_settings_change_the_render` (garde-fou : vérifie que ce
      réglage change bien >20 % des pixels par rapport à la scène de référence, pour
      détecter un uniform mal câblé qui laisserait le rendu inchangé malgré des valeurs
      différentes). 257 tests lib + 4 tests bin (inchangés, ce sprint n'ajoute aucun test
      unitaire, seulement des golden) + 3 golden render + 1 golden skinning verts.
- **Fichiers** : `src/gfx/shaders/sky.wgsl` (nouveau), `src/gfx/shaders/main.wgsl`,
  `src/gfx/renderer.rs`, `src/scene/mod.rs`, `src/editor/mod.rs`, `tests/golden_render.rs`.

#### Sprint 90 — Cible HDR + tone mapping ✅ FAIT
- [x] **Cible HDR** (`HDR_FORMAT = Rgba16Float`) : les 5 pipelines de la passe
      principale (`pipeline`, `sky_pipeline`, `grid_pipeline`, `gizmo_pipeline`,
      `skinned_pipeline`) dessinent désormais dans une texture intermédiaire
      (`hdr_view`, persistante et redimensionnée dans `resize()` comme `depth_view`)
      plutôt que directement dans le format d'affichage — sans ça, une valeur > 1
      (émissif, spéculaire fort) serait écrêtée *avant* même d'atteindre un
      éventuel tone mapping.
- [x] **Tone mapping ACES** (`src/gfx/shaders/tonemap.wgsl`, nouveau) : passe plein
      écran (même technique que `sky.wgsl`) qui échantillonne `hdr_view` et
      applique l'approximation filmique de Narkowicz (2015) avant d'écrire dans le
      format final — partagée par `render()`/`render_scene_headless()`/
      `render_skinned_test()` via un unique helper `Renderer::tonemap()`.
- [x] **Tests** : golden `overbright_emissive_keeps_its_hue_instead_of_clipping_to_white`
      — un émissif dont le canal rouge dépasse largement 1.0 doit rester teinté
      (rouge dominant, pas blanc pur), la preuve concrète du livrable annoncé.
      Les 3 goldens existants (`primitives_lights`, `sky_and_fog`, `skinned_hinge_
      bent90`) régénérés (`UPDATE_GOLDEN=1`) : la courbe ACES modifie le contraste
      de **toute** l'image, y compris en dessous de 1.0, donc même les scènes sans
      surexposition changent visuellement (vérifié à l'œil avant régénération —
      toujours la même scène, juste un contraste différent, aucune régression).
- **Fichiers** : `src/gfx/shaders/tonemap.wgsl` (nouveau), `src/gfx/renderer.rs`,
  `tests/golden_render.rs`.

#### Sprint 91 — Bloom + réglages ✅ FAIT
- [x] **Chaîne de mips down/upsample** (`src/gfx/shaders/bloom.wgsl`, nouveau) : seuil
      (extrait les pixels dont la radiance HDR dépasse 1.0) → `BLOOM_MIP_LEVELS = 4`
      niveaux de descente (remplace) → remontée (additionne) — une texture à
      plusieurs mips (`Renderer::bloom_mip_views`, une vue par niveau), 3 pipelines
      partageant le même shader/layout (seul le blend state change downsample vs
      upsample). `mip_views[0]` (résultat final, moitié résolution HDR) composé dans
      `tonemap.wgsl` — le filtrage bilinéaire du sampler fait le dernier upsample
      vers la pleine résolution au passage.
- [x] **Réglages** : `scene::Sky::bloom_intensity` (curseur scène, section « 🌫 Ciel &
      brouillard » de l'inspecteur) **et** `BuildConfig::bloom` (case à cocher,
      panneau Export, comme `msaa`) — les deux doivent être vrais, en plus de
      `RenderQuality::bloom_enabled()`, pour que le renderer calcule le halo
      (`AppState::bloom_enabled`, relu comme `render_quality`).
- [x] **Opt-out mobile documenté** : `RenderQuality::bloom_enabled()` coupe le bloom
      sur qualité « Basse » (les passes GPU sont **sautées**, pas juste neutralisées
      côté shader — vrai gain de perf) ; le préréglage « ⚡ Performance » du panneau
      Export coche cet opt-out (`bloom = false`).
- [x] **Tests** : golden `bloom.png` (halo net et visible) +
      `bloom_intensity_visibly_spreads_light_around_the_bright_object` (garde-fou :
      le halo doit déborder du contour de l'objet, pas seulement changer ses pixels
      déjà brillants — ce que le tone mapping seul ferait) + 2 tests
      `RenderQuality::bloom_enabled`/rétrocompatibilité JSON de `BuildConfig::bloom`.
      Les 3 scènes de référence existantes (`primitives_lights`, `sky_and_fog`,
      l'émissif surexposé du Sprint 90) désactivent explicitement le bloom
      (`bloom_intensity: 0.0`) pour rester des goldens à une seule variable —
      **aucune régénération nécessaire**. 261 tests lib + 4 tests bin + 6 golden
      render + 1 golden skinning verts.
- **Fichiers** : `src/gfx/shaders/bloom.wgsl` (nouveau), `src/gfx/shaders/tonemap.wgsl`,
  `src/gfx/renderer.rs`, `src/scene/mod.rs` (`Sky::bloom_intensity`),
  `src/app/build_config.rs` (`BuildConfig::bloom`, `RenderQuality::bloom_enabled`),
  `src/app/mod.rs` (`AppState::bloom_enabled`), `src/editor/mod.rs` (curseur scène),
  `src/editor/export.rs` (case à cocher build), `tests/golden_render.rs`.
- **Livrable restant, hors scope de ce sprint** : même constat que les Sprints 87-90 —
  « la boule de feu rayonne » en jeu réel demande de vérifier `app::fireball` avec
  un émissif réglé en pratique (mécanisme prêt, contenu/tuning à faire séparément).

#### Sprint 92 — Mipmaps + tangentes ✅ FAIT
- [x] **Mips générés à l'import** (`src/gfx/shaders/mipgen.wgsl`, nouveau) :
      `make_texture` calcule `mip_count_for(width, height)` (formule standard, `1 +
      log2(plus grande dimension)`) et crée la texture avec toute la chaîne, puis
      **blits chaînés** — un niveau à la fois, chacun un simple échantillonnage
      bilinéaire du niveau précédent (même triangle plein écran que `sky.wgsl`/
      `bloom.wgsl`, dans un module séparé pour ne pas coupler la génération de mips,
      une fois par texture, au pipeline de bloom, par frame). `tex_sampler` gagne
      `mipmap_filter: Linear` — sans lui, le sampler resterait bloqué sur le mip 0
      quelle que soit la chaîne générée.
- [x] **Tangentes** (`import::compute_tangents`, `src/scene/import.rs`) : méthode de
      Lengyel (tangente par triangle depuis les dérivées position/UV, accumulée par
      sommet, orthogonalisée contre la normale par Gram-Schmidt, signe de bitangente
      déduit du triangle) — **pas** l'algorithme de référence mikktspace (Blender),
      plus complexe ; équivalent fonctionnel largement utilisé sous ce nom dans
      d'autres moteurs, documenté comme tel dans le code plutôt que de prétendre à
      une conformité qu'il n'a pas. Calculées pour **tout** mesh importé (skinné ou
      non) dans `ImportedMesh::load_skinning()`, stockées à part
      (`ImportedMesh::tangents`, donnée dérivée non sérialisée, même statut que
      `skeleton`/`clips`/`vertex_skins`) — **pas encore branchées sur le GPU** (aucun
      normal mapping ce sprint, cf. livrable restant).
- [x] **Tests** : `mip_count_for` (formule vérifiée contre des puissances de deux
      connues), 4 tests de `compute_tangents` (tangente attendue sur UV aligné axes,
      orthogonalité à la normale, inversion de signe sur UV en miroir, robustesse sur
      triangle dégénéré), 1 test bout-en-bout (`load_skinning` peuple bien
      `tangents`), 1 nouveau golden (damier haute fréquence sur un plan en
      perspective — le cas d'école de l'aliasing lointain, sert de filet si la chaîne
      de blits de `make_texture` venait à casser). 267 tests lib + 4 tests bin + 7
      golden render + 1 golden skinning verts ; aucun golden existant n'a dû être
      régénéré (aucune scène de référence n'utilise de texture).
- **Fichiers** : `src/gfx/shaders/mipgen.wgsl` (nouveau), `src/gfx/renderer.rs`
  (`make_texture`, `mip_count_for`), `src/scene/import.rs` (`compute_tangents`),
  `src/scene/mod.rs` (`ImportedMesh::tangents`), `tests/golden_render.rs`.
- **Livrable restant, hors scope de ce sprint** : le « avant/après » a été vérifié
  visuellement (golden `textured_ground_mipmaps.png` : le damier se lisse
  proprement vers l'horizon plutôt que de scintiller) plutôt que via une capture
  d'écran manuelle dédiée — équivalent en pratique, versionné et reproductible. Le
  normal mapping lui-même (consommer `tangents` dans un shader) reste un sprint à
  part, non planifié dans cette section.

### PHASE N — Chaîne gameplay (93 → 99)

#### Sprint 93 — Événements ✅ FAIT
- [x] **File d'événements** (`AppState::game_events`, `Vec<String>` — des noms plutôt
      qu'un enum `GameEvent` : les émetteurs/auditeurs principaux sont des scripts Lua,
      un enum Rust fermé les brimerait) : drainée au début de chaque tick fixe
      (`sim_step`), les événements émis pendant un tick sont **délivrés au suivant**
      puis jetés — le décalage rend l'ordre des objets dans la boucle des scripts
      indifférent (pas de « l'auditeur doit venir après l'émetteur »), et la
      consommation en un tick borne la file.
- [x] **Lua** : `emit("nom")` (accumulateur relu après l'appel, même patron que
      `vibrate`/`debug.line`) et `on_event("nom") -> bool` (ensemble des événements
      reçus ce tick). +2 paramètres à `run_script`, 18 sites d'appel mis à jour.
- [x] **Événements moteur** : `score:N` émis à chaque point marqué, via un nouveau
      point de passage unique `AppState::add_score(n)` qui remplace les 6 sites
      `self.score += …` de production (pièces, arme, attaque au contact, attaque de
      zone, boule de feu, zone mortelle) — un `score:N` par valeur **traversée**, pas
      seulement la valeur finale (2 pièces le même tick ne font pas sauter `score:3`).
- [x] **Livrable vérifié** : test bout-en-bout `a_door_opens_on_score_3_without_direct_
      coupling` — une porte scriptée `if on_event('score:3') then obj.y = 10 end`
      s'ouvre quand le joueur ramasse 3 pièces, sans référencer ni pièces ni joueur.
      **Trouvé en l'écrivant** : le jeu gèle une fois gagné (`advance_play`), donc un
      événement émis au tick de la victoire n'est jamais délivré — cas documenté dans
      le test (la porte du livrable s'ouvre *en cours de partie*). +1 test unitaire
      `run_script` (emit → events_out ; on_event vrai/faux ; pas de livraison
      intra-tick). 270 tests lib + 4 bin verts.
- **Fichiers** : `src/app/mod.rs` (file, `add_score`, `run_script`),
  `src/app/combat.rs`, `src/app/fireball.rs` (routage par `add_score`) — pas
  `src/runtime/mod.rs` (la table Lua vit dans `run_script`, pas dans `runtime`).

#### Sprint 94 — Cycle de vie + handles générationnels ⬜
**Objectif** : créer/détruire des objets en Play sans invalider de références.
- [ ] File de commandes spawn/despawn appliquée en **fin de tick**.
- [ ] `scene.objects` migré vers des handles générationnels (`slotmap`), module par module.
- **Fichiers** : `src/scene/mod.rs`, `src/app/*`, `src/net/*`.
- **Livrable** : détruire un objet en Play n'invalide aucune référence (tests).
- **Risque** : le refactor le plus délicat du plan — il touche les indices du réseau et de l'undo.

#### Sprint 95 — GUID d'assets + versioning de scènes ✅ FAIT
- [x] **Manifeste `uuid → nom de fichier`** (`src/assets.rs`, `AssetManifest`, persisté
      dans `assets_dir()/manifest.json`) : nouveau schéma `asset-id://<uuid>`, résolu par
      `resolve_asset_id` avant les schémas existants dans `read_bytes`. `import_to_assets`
      délivre désormais ce schéma pour tout nouvel import (`register_asset`, idempotent
      par nom) ; les scènes déjà écrites avec un `asset://<nom>` en dur ne sont **pas**
      migrées rétroactivement — un asset doit être ré-importé/enregistré pour devenir
      rename-safe (documenté, pas un oubli).
  - `is_known_scheme()` centralise ce qui était un `starts_with(SCHEME) ||
    starts_with(ASSET_SCHEME)` répété à 4 endroits (`scene/import.rs`, `runtime/audio.rs`,
    `editor/readiness.rs`, `AppState::collect_assets`) — chacun aurait dû être mis à jour
    séparément pour reconnaître `asset-id://` sans ce point de passage unique.
  - `rename_asset(id, new_name)` renomme le fichier **et** met à jour le manifeste,
    gardant l'uuid stable — c'est le mécanisme qui rend le renommage transparent.
  - Logique testable sans toucher `~/.motor3derust/assets/` ni l'environnement global :
    `register_asset`/`resolve_asset_id`/`rename_asset` délèguent à des variantes `_at(dir,
    …)` paramétrées par répertoire, exercées avec un dossier temporaire par test.
- [x] **`Scene::version`** (`#[serde(default)]`, 0 = legacy) + `Scene::migrate()`,
      appelée par `Scene::load` après désérialisation. **Aucune migration réelle
      n'existe encore** dans ce projet (rien n'a encore changé de forme au point de
      dépasser un simple `#[serde(default)]`, documenté ainsi dans le code plutôt que
      d'inventer un historique) : le seul correctif appliqué à `version == 0` —
      dédoublonner `groups` — est une vraie correction d'hygiène pour un JSON
      ancien/modifié à la main, pas une migration de façade. Idempotente : une scène
      déjà à `CURRENT_VERSION` n'est pas retouchée, même avec des doublons volontaires.
- [x] **Tests** : 5 dans `assets.rs` (idempotence par nom, résolution après renommage,
      uuid/renommage inconnus, `is_known_scheme`) + 2 dans `scene/mod.rs` (une scène
      legacy sans champ `version` du tout se charge à `CURRENT_VERSION` avec `groups`
      dédoublonnés ; `migrate` laisse une scène déjà à jour intacte). 277 tests lib + 4
      bin + 8 golden verts.
- **Fichiers** : `src/assets.rs`, `src/scene/mod.rs`, `src/scene/import.rs`,
  `src/runtime/audio.rs`, `src/editor/readiness.rs`, `src/app/mod.rs`
  (`collect_assets`, `optimized_path`).
- **Livrable restant, hors scope de ce sprint** : pas d'UI de renommage dans le
  navigateur d'assets de l'éditeur — `rename_asset` existe et est testé côté moteur,
  reste à câbler un bouton/champ dans `src/editor/mod.rs`.

#### Sprint 96 — Prefabs 🟢 (mécanisme moteur fait, UI éditeur restante)
- [x] **`SceneObject::prefab: Option<PrefabInstance>`** (`asset_id` = référence stable
      `asset-id://<uuid>` du Sprint 95, `overrides: Vec<String>` = noms des champs
      JSON explicitement modifiés sur cette instance).
- [x] **`Scene::save_prefab(obj, name)`** : sérialise l'objet dans
      `assets_dir()/prefabs/<name>.json`, enregistré dans le manifeste d'assets — un
      renommage ultérieur du fichier prefab ne casse aucune instance (même mécanisme
      que le Sprint 95).
- [x] **`Scene::instantiate_prefab(asset_id, name, at)`** : nouvelle instance, avec
      `transform`/`name` surchargés d'office (chaque instance a naturellement sa
      propre position et un nom distinct dans la hiérarchie), le reste suivant le
      template.
- [x] **`Scene::sync_prefab_instances()`** : fusion au niveau JSON
      (`serde_json::Value`) plutôt que champ Rust par champ — `SceneObject` a des
      dizaines de champs, une fusion générique évite d'étendre cette fonction à
      chaque champ futur. Copie chaque champ du template **non listé** dans
      `overrides` ; `prefab` lui-même n'est jamais copié (préserverait sinon le lien
      et les surcharges de l'instance). Un prefab introuvable (fichier supprimé/
      déplacé) laisse l'instance telle quelle, sans erreur bruyante.
- [x] **Livrable vérifié** : test `modifying_a_prefab_updates_its_instances_except_
      overrides` — 20 instances d'un prefab « gemme », une couleur surchargée à la
      main sur l'instance #5 ; le prefab change de couleur (jaune → vert) et se
      resynchronise : les 19 autres suivent la nouvelle couleur, l'instance #5 garde
      la sienne, et `transform`/`name` restent propres à chacune. +2 tests (objet
      sans prefab inchangé ; prefab introuvable = no-op sans panique). 280 tests lib
      + 4 bin + 8 golden verts.
- **Fichiers** : `src/scene/mod.rs`.
- **Livrable restant, hors scope de ce sprint** : pas d'instanciation depuis le
  navigateur d'assets ni de bouton « créer un prefab depuis la sélection » dans
  `src/editor/mod.rs` — mécanisme moteur complet et testé, câblage UI à faire dans
  un sprint dédié (même situation que le renommage d'assets du Sprint 95).

#### Sprint 97 — API Lua de scène ✅ FAIT
> Fait **sans** attendre le Sprint 94 (sauté pour l'instant, refactor à handles
> générationnels jugé trop risqué en présence d'une autre session active ce jour-là) :
> `spawn` n'a besoin que d'un ajout en fin de tableau (jamais d'insertion/retrait ailleurs
> ⇒ indices existants intacts), et `obj:destroy()` réutilise le `visible = false` déjà
> établi partout dans ce moteur (monstres vaincus, collectibles ramassés) plutôt qu'un
> vrai retrait de `scene.objects`. Un vrai retrait/réutilisation de slots reste le
> Sprint 94, toujours ouvert.
- [x] **`spawn(prefab_ref, x, y, z)`** : accumulé pendant la boucle des scripts
      (`AppState::sim_step`), appliqué **après** — `scene.objects` est emprunté mutable
      pendant la boucle, on ne peut pas y pousser un objet à ce moment-là. Instancie le
      prefab (`Scene::instantiate_prefab`, Sprint 96) et reconstruit la physique une
      seule fois si au moins un spawn a eu lieu (même garde-fou que
      `spawn_network_player`).
- [x] **`obj:destroy()`** : suppression douce (`visible = false`), pas de retrait —
      accumulé en `bool` par script (comme `set_health`), appliqué après l'appel Lua.
- [x] **`find_tag("nom")`** : nouveau champ `SceneObject::tag`, instantané
      `Vec<(String, Vec3)>` pris **avant** la boucle des scripts (positions des objets
      visibles tagués), exposé en Lua comme une table de `{x,y,z}`. Un objet
      spawné/détruit ce tick n'y apparaît donc pas encore/plus — disponible au tick
      suivant seulement.
- [x] **Coroutines Lua natives** : vérifiées, pas câblées — `mlua::Lua::new()` charge
      déjà la stdlib complète. Testé plutôt que supposé (`lua_coroutines_work_out_of_
      the_box`, `coroutine.create`/`resume`/`yield` réels).
- [x] **Tests** : `obj:destroy()` masque sans retirer ; `find_tag` isole les tags
      demandés parmi plusieurs ; bout-en-bout `spawn` — un script fait apparaître un
      ennemi depuis un prefab, retrouvable par tag. 284 tests lib + 4 bin + 8 golden verts.
- **Fichiers** : `src/app/mod.rs` (`run_script`, `sim_step`), `src/scene/mod.rs`
  (`SceneObject::tag`) — pas `src/runtime/mod.rs` (la table Lua vit dans `run_script`,
  cf. la même note aux Sprints 87/93).
- **Livrable restant, hors scope de ce sprint** : pas de démo « vagues d'ennemis »
  dédiée dans l'éditeur — mécanisme complet et testé (spawn/destroy/find_tag/
  coroutines), contenu de démo à faire séparément (même situation que les Sprints
  95/96 : mécanisme moteur avant contenu/UI).

#### Sprint 98 — user:// + sauvegarde de partie ✅ FAIT (Android non vérifié sur appareil)
- [x] **Schéma `user://`** (`src/assets.rs`) : **sans** la crate `dirs` (pas de nouvelle
      dépendance — même choix qu'au Sprint 95 avec `uuid`) — desktop réutilise
      `$HOME` (`~/.motor3derust/save/`, à côté de `assets/` mais distinct : données
      **écrites** par le jeu, pas importées par l'éditeur) ; Android n'a pas de
      `$HOME`, donc `assets::set_android_data_dir` pose une fois le chemin fourni par
      `android_app.internal_data_path()` (`android-activity`), appelé dans
      `lib.rs::android_main` avant la boucle d'événements.
- [x] **`SaveGame`** (`src/runtime/savegame.rs`, nouveau) : `version`, `score`,
      `positions` (une par objet de `scene.objects`, dans l'ordre), `lua_vars`.
      Versionnée comme les scènes (Sprint 95) — `version` toujours forcée à
      `CURRENT_VERSION` à l'écriture. **Pas de `seed`** : aucun RNG seedable
      n'existe dans ce moteur à ce jour (écarté explicitement au Sprint 80, cf. plus
      haut dans ce fichier) — rien à sauvegarder de ce côté, un champ inventé aurait
      été un mensonge de plus qu'à corriger plus tard.
- [x] **`save.get("clé")`/`save.set("clé", valeur)` en Lua** : nouveau
      `AppState::lua_vars` (persistant, contrairement à `game_events` du Sprint 93 qui
      se vide chaque tick), lu/écrit séquentiellement par les scripts dans l'ordre de
      `sim_step` — pas de décalage d'un tick nécessaire ici (contrairement aux
      événements) puisque l'ordre d'exécution est déjà déterministe et accepté tel
      quel.
- [x] **`AppState::save_game`/`load_game`** : points d'entrée haut niveau
      (`capture_save`/`apply_save` + écriture/lecture `user://`) — pas encore de
      bouton dans l'éditeur/le player, cf. le livrable restant.
- [x] **Livrable vérifié sur desktop** : test bout en bout `saving_and_loading_a_game_
      restores_score_position_and_lua_vars` — score, position d'objet et variable Lua
      remis à zéro puis restaurés après un aller-retour disque réel par
      `save_game`/`load_game`. 288 tests lib + 4 bin + 8 golden verts.
- **Fichiers** : `src/assets.rs`, `src/runtime/savegame.rs` (nouveau),
  `src/runtime/mod.rs`, `src/app/mod.rs`, `src/lib.rs` (`android_main`).
- **Livrable restant, hors scope de ce sprint** : « **et** Android » non vérifié —
  code écrit contre l'API réelle d'`android-activity` (`internal_data_path`), mais
  jamais tourné sur un appareil (aucun matériel disponible dans cet environnement,
  même situation que le Sprint 48 « Capteurs & assets mobiles »). Pas de bouton
  Sauvegarder/Charger dans l'éditeur/le player — mécanisme complet et testé, UI à
  faire séparément (même situation que les Sprints 95/96/97).

#### Sprint 99 — Anim notifies ✅ FAIT (démo de combat animée non câblée)
- [x] **`ImportedMesh::notifies: HashMap<clip, Vec<(temps, nom)>>`** (`src/scene/mod.rs`)
      — **sérialisé**, contrairement à `clips`/`skeleton` (entièrement rederivés du
      glTF à chaque chargement) : un marqueur est authored à la main, le format glTF
      n'a pas de notion standard de marqueur, donc rien à en dériver.
- [x] **`notifies_crossed(markers, prev_time, cur_time, duration)`** (`src/scene/
      mod.rs`) : fonction pure, gère le bouclage de fin de clip (un pas qui traverse
      la fin ne doit pas manquer un marqueur proche de `duration`) et le temps figé
      (vitesse nulle ⇒ rien ne se déclenche en boucle). 5 tests unitaires.
- [x] **Câblage `sim_step`** : la boucle d'avance d'animation (Sprint 87) calcule les
      marqueurs franchis et les injecte dans `events_in` **ce même tick** (pas de
      décalage d'un tick comme les événements du Sprint 93 : cette boucle s'exécute
      entièrement avant qu'aucun script ne tourne, donc aucune ambiguïté d'ordre à
      éviter) — un événement `anim:<nom>` par marqueur franchi, lisible via
      `on_event` (Sprint 93).
- [x] **Livrable vérifié** : test bout-en-bout `an_anim_notify_gates_the_combat_hit_
      window` — un objet animé avec deux marqueurs (`hit_open`/`hit_close`) sur un
      clip synthétique, script qui n'ouvre la fenêtre (`save.set`, Sprint 98) qu'entre
      les deux. 294 tests lib + 4 bin + 8 golden verts.
- **Fichiers** : `src/scene/mod.rs`, `src/scene/import.rs` (`Clip::without_tracks`,
  constructeur de test), `src/app/mod.rs` (`sim_step`) — pas `src/app/combat.rs`/
  `src/runtime/mod.rs` : le système de combat mêlée (`attack_windup`) reste sur
  timer fixe, aucune scène du dépôt n'a de personnage animé à retimer dessus (même
  constat que les Sprints 87-89 : aucune démo skinnée n'existe encore).
- **Livrable restant, hors scope de ce sprint** : pas de retimage du mode combat réel
  sur des marqueurs — mécanisme complet et testé (marqueurs → événements → scripts),
  contenu skinné/démo à faire séparément (même situation que les Sprints 95-98).

### PHASE O — Physique & feel (100 → 103c)

#### Sprint 100 — Trimesh + convexe ✅ FAIT
- [x] **`ColliderShape::TriMesh`/`ConvexHull`** (`src/runtime/physics.rs`) : construits
      depuis les vertices bruts de `ImportedMesh::data` (mis à l'échelle de l'objet,
      comme les demi-dimensions des primitives) — `SharedShape::trimesh`/
      `convex_hull` de rapier. `TriMesh` réservé au décor **statique** (pas de
      propriétés de masse définies) : demandé sur un corps dynamique, repli
      automatique sur `ConvexHull` (`log::warn!`) plutôt que de planter ou de laisser
      un objet traverser le décor sans jamais entrer en collision.
- [x] **Choix dans l'inspecteur** (`src/editor/mod.rs`) : « Enveloppe convexe »/
      « Silhouette exacte », visibles seulement pour un objet `MeshKind::Imported`
      (n'ont pas de sens pour une primitive Cube/Sphère/...).
- [x] **Livrable vérifié** par 4 tests bout-en-bout (physique réelle, pas une
      assertion sur la forme construite) : une boule tombe sur un décor triangulaire
      dont la boîte englobante couvre un coin **vide** de sa silhouette réelle — avec
      `TriMesh`, elle s'arrête sur le triangle et traverse le coin vide ; avec `Auto`
      (contre-épreuve), la boîte englobante bloque à tort ce même coin. Un rocher
      importé (tétraèdre) tombe sur un sol en `ConvexHull` dynamique et s'y arrête ;
      la même scène en `TriMesh` (dynamique) se replie sur `ConvexHull` sans jamais
      tomber indéfiniment. **Trouvé en écrivant les tests** : un `TriMesh` n'a pas
      d'épaisseur — une boule lâchée de trop haut le traversait par tunneling (aucun
      contact détecté en un seul pas de simulation, symptôme identique à l'absence
      de collider) ; corrigé en lâchant les boules de test d'assez bas pour ne pas
      tunneliser, plutôt que d'anticiper la CCD par objet du **Sprint 101**, hors
      scope ici.
- **Fichiers** : `src/runtime/physics.rs`, `src/editor/mod.rs`. 298 tests lib + 4
  bin + 8 golden verts.

#### Sprint 101 — CCD + couches de collision ✅ FAIT
- [x] **`SceneObject::ccd: bool`** (`src/scene/mod.rs`) : active
      `RigidBodyBuilder::ccd_enabled` (`src/runtime/physics.rs`) — désactivé par
      défaut (coûteux), réservé aux objets qui en ont réellement besoin (missiles).
- [x] **`collision_layer`/`collision_mask: u32`** (bits, `Group`/`InteractionGroups`
      de rapier) : toutes les couches par défaut (`u32::MAX`) — aucune scène
      existante ne change de comportement tant que ces champs ne sont pas réglés.
      `Group::from_bits_truncate` plutôt qu'une conversion qui pourrait paniquer :
      un JSON de scène ancien/corrompu ne doit pas faire planter l'entrée en Play.
- [x] **Inspecteur** (`src/editor/mod.rs`) : case CCD + deux champs hexadécimaux
      (couches/masque), affichés à côté du choix de forme de collider.
- [x] **`Physics::set_velocity`** (nouveau, même famille que `set_position`) : impose
      une vitesse initiale à un corps dynamique — nécessaire pour tester un
      projectile qui doit partir vite dès sa création, sans passer par `control`
      (pensé pour un joueur piloté, pas un missile).
- [x] **Livrable vérifié** par 4 tests bout-en-bout (physique réelle) : un missile à
      200 m/s traverse un mur fin de 5 cm par tunneling sans `ccd`, et s'arrête avec
      `ccd` activé. Un missile dont `collision_mask` exclut la couche du mur le
      traverse à vitesse modeste (sans avoir besoin de CCD) ; sans ce réglage, il est
      bloqué normalement. **Trouvé en écrivant les tests** : un premier essai des
      tests de masque semblait indiquer un bug de filtrage (le missile traversait le
      mur même *sans* masque) — en réalité la gravité faisait tomber le missile sous
      un mur de hauteur normale avant qu'il n'ait eu le temps de parcourir la
      distance à vitesse modeste (aucun lien avec les couches de collision) ;
      corrigé en agrandissant le mur des deux tests concernés, pas en touchant au
      code de production.
- **Fichiers** : `src/scene/mod.rs`, `src/runtime/physics.rs`, `src/editor/mod.rs`.
  302 tests lib + 4 bin + 8 golden verts.

#### Sprint 102 — Requêtes gameplay + trigger exit ✅ FAIT
- [x] **`Physics::raycast`/`overlap_sphere`** (`src/runtime/physics.rs`) : requêtes
      spatiales via le `QueryPipeline` de rapier — `raycast` renvoie le premier
      collider touché (point d'impact, distance, index d'objet via un nouveau
      `collider_owner: HashMap<ColliderHandle, usize>` qui couvre **tous** les
      colliders, statiques inclus, contrairement à `dynamic`/`controlled`) ;
      `overlap_sphere` renvoie les index dans un rayon. Même filtrage par couche que
      `collision_layer`/`collision_mask` (Sprint 101, bits partagés). Reconstruisent
      une broad-phase **jetable** à chaque appel plutôt que de réutiliser celle de
      `step` : **trouvé en écrivant les tests** — peupler directement la BVH
      incrémentale de la simulation cassait la physique réelle (chasseurs/joueur
      téléportés, cf. les tests d'IA qui ont viré au rouge) en perturbant son suivi
      interne des colliders modifiés entre deux pas.
- [x] **`raycast()`/`overlap_sphere()` côté Lua** (`src/app/mod.rs`, `run_script`) :
      fermetures **scopées** (`lua.scope`, pas `lua.create_function`) — seules du
      fichier à emprunter `&Physics` au lieu de ne capturer que des valeurs
      possédées/clonées comme le reste de l'API Lua. `raycast` renvoie une table
      `{x,y,z,dist}` ou `nil` ; `overlap_sphere` un compte (pas une liste d'index —
      un script n'a de toute façon pas de handle direct sur un autre objet, cf.
      `find_tag`, Sprint 97). `physics` vaut `None` hors mode Play : les deux
      fonctions renvoient alors « rien touché » sans planter.
- [x] **`obj.exited`** (`AppState::trigger_prev`, `sim_step`) : symétrique de
      `obj.triggered` — vrai le tick où le contact avec une zone `trigger` vient de
      cesser (différence entre l'ensemble déclenché du tick précédent et celui de ce
      tick), pas seulement « pas en contact ».
- [x] **Livrable vérifié** par 10 tests bout-en-bout (5 `runtime::physics`, physique
      réelle ; 5 `app`, au niveau Lua) : capteur de sol (`raycast` vers le bas,
      distance/point d'impact lus dans le script puis visualisés via `debug.line`,
      Sprint 83) et cône de vision (`overlap_sphere` pour la détection de proximité,
      brique du test d'angle/ligne de vue qu'un script ferait ensuite avec
      `find_tag` + `raycast`).
- **Fichiers** : `src/runtime/physics.rs`, `src/app/mod.rs` (`run_script`, pas prévu
  au départ — nécessaire pour exposer `raycast`/`overlap_sphere`/`obj.exited` aux
  scripts, même schéma que les autres API Lua ajoutées aux sprints précédents,
  ex. Sprint 97/99).
- **Livrable** : capteur de sol et cône de vision scriptés en Lua, visualisés au debug drawing (Sprint 83).
  312 tests lib + 4 bin + 8 golden verts.

#### Sprint 103a — Maintenabilité : découpage des gros modules & AppState ⬜
**Objectif** : réduire le risque des futurs changements gameplay/UI/réseau/script en cassant les fichiers-mastodontes avant d'y ajouter la physique de contrôleur (cf. `AUDIT.md` §7.4).
- [ ] `app/mod.rs` (7180 lignes) : `AppState` porte trop de responsabilités (gameplay, scripts Lua, réseau, sauvegarde, combat, animation, UI indirecte) — extraire ces sous-systèmes dans des modules dédiés, réduire `AppState`/`app/mod.rs` à l'orchestration (state machine, boucle principale).
- [ ] `src/editor/mod.rs` (3948 lignes) et `src/scene/mod.rs` (4810 lignes) : même traitement (découpage par responsabilité) ; scinder en sprint(s) séparé(s) si le périmètre est trop large pour tenir ici.
- [ ] Commentaires trop volumineux qui documentent l'historique de sprint plutôt que le comportement actuel : déplacer l'historique vers `docs/audits` (à créer), garder dans le code uniquement les invariants importants.
- **Fichiers** : `src/app/mod.rs`, `src/app/combat.rs`, `src/editor/mod.rs`, `src/scene/mod.rs`, nouveaux modules extraits, `docs/audits/`.
- **Livrable** : taille des trois fichiers réduite significativement ; comportement inchangé ; tests existants verts ; commentaires du code recentrés sur les invariants, historique déplacé en doc.
- **Risque** : refactor pur, sans nouvelle fonctionnalité — le faire **seul**, pas de sprint gameplay en parallèle, pour limiter les conflits de merge. Si le périmètre (3 fichiers + AppState + commentaires) s'avère trop large pour un seul sprint, scinder en 103a-1 (`app/mod.rs`/`AppState`), 103a-2 (`editor`+`scene`), 103a-3 (commentaires).

#### Sprint 103b — Character controller kinématique ⬜
**Objectif** : marches, pentes, snap au sol — **sans casser le multijoueur**.
- [ ] Migration vers `KinematicCharacterController` de rapier.
- **Fichiers** : `src/runtime/physics.rs`, `src/app/multiplayer.rs`, `src/bin/server.rs`.
- **Livrable** : escalier montable en solo et en ligne.
- **Risque** : seul sprint qui menace l'acquis multijoueur — le faire **seul**, pas groupé.

#### Sprint 103c — Audit complet de la prédiction réseau ⬜
**Objectif** : revalider la prédiction/réconciliation réseau après la migration du contrôleur, façon sprints 72–77.
- [ ] Vérifier la réconciliation client/serveur sur le nouveau contrôleur cinématique.
- [ ] Tests de non-régression rubber-banding à latence simulée.
- **Fichiers** : `src/app/network_client.rs`, `src/net/interpolation.rs`, `src/bin/server.rs`.
- **Livrable** : tests de réconciliation verts ; aucun rubber-banding à 100 ms simulées.

### PHASE P — Audio, HUD & confort (104 → 110, sprints tampons insérables après K)

#### Sprint 104 — Audio : bus + panning + streaming ⬜
- [ ] Tracks kira musique/SFX (volumes persistés) ; panning stéréo caméra→source ; `StreamingSoundData` pour les musiques.
- **Fichiers** : `src/runtime/audio.rs`, `src/app/settings.rs`.
- **Livrable** : réglages M/SFX dans les paramètres ; musique longue sans pic mémoire (profiler).

#### Sprint 105 — Audio : randomisation ⬜
- [ ] ± pitch/volume par déclenchement (RNG du Sprint 81).
- **Fichiers** : `src/runtime/sfx.rs`.
- **Livrable** : dix pas d'affilée ne sonnent plus identiques.

#### Sprint 106 — Widgets de HUD déclaratifs ⬜
- [ ] 5 widgets sérialisés dans la scène (texte, image, jauge, bouton, ancres) au-dessus de la 3D — la safe area existe déjà.
- **Fichiers** : `src/scene/mod.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs`.
- **Livrable** : le HUD de la démo contrôleur reconstruit en widgets, zéro code en dur.

#### Sprint 107 — Manettes + remapping ⬜
- [ ] Crate `gilrs` ; table actions→touches persistée, éditable dans les paramètres.
- **Fichiers** : `src/app/input.rs`, `src/app/settings.rs`.
- **Livrable** : démo jouable à la manette Bluetooth sur desktop et Android.

#### Sprint 108 — Hot-reload (assets + Lua) ⬜
- [ ] `notify` sur le dossier assets → réimport async ; invalidation des chunks Lua modifiés en cours de Play.
- **Fichiers** : `src/assets.rs`, `src/runtime/mod.rs`.
- **Livrable** : retoucher une texture ou un script se voit sans redémarrer.

#### Sprint 109 — Éditeur : snapping + profiler GPU ⬜
- [ ] Snap position/rotation au pas (touche modificatrice) ; timestamp queries wgpu par passe + compteur de draw calls.
- **Fichiers** : `src/gfx/`, `src/editor/mod.rs`.
- **Livrable** : coût des passes ombre/scène/HDR/bloom lisible dans le profiler.

#### Sprint 110 — Production : crash log + rustdoc ⬜
- [ ] `panic::set_hook` → fichier dans `user://` + écran d'envoi **volontaire** (pas de télémétrie automatique, par principe).
- [ ] `cargo doc` publié en CI (GitHub Pages) ; semver des releases.
- **Fichiers** : `src/main.rs`, `.github/workflows/`.
- **Livrable** : un panic Android laisse une trace exploitable ; doc API en ligne.

### PHASE Q — Web, la vitrine (111 → 114)

#### Sprint 111 — Build wasm32 ⬜
- [ ] Cible `wasm32-unknown-unknown`, winit→canvas, wgpu→WebGPU ; une scène statique s'affiche.
- **Fichiers** : `src/lib.rs`, `src/gfx/renderer.rs`, `packaging/`.
- **Livrable** : la démo mobile tourne dans Chrome, page servie par la CI.
- **Risque** : API bloquantes (fichiers, threads) — sprint de défrichage.

#### Sprint 112 — Assets & audio web ⬜
- [ ] Assets par fetch async (le chemin async existant aide) ; contexte audio web pour kira.
- **Livrable** : démo contrôleur complète, jouable au clavier dans le navigateur.

#### Sprint 113 — Multijoueur navigateur ⬜
- [ ] Client WebSocket compilé en WASM (déjà en WebSocket — avantage décisif) ; passage en `wss://`.
- **Fichiers** : `src/net/client.rs`, `examples/smoke_vps.rs`.
- **Livrable** : un joueur navigateur et un joueur desktop se voient bouger sur le VPS (smoke test étendu).

#### Sprint 114 — Vitrine publique ⬜
- [ ] Page de démo déployée en CI ; lien README ; « rejoindre un salon » via les lobbies Firebase existants.
- **Livrable** : n'importe qui teste le multijoueur en un clic — le meilleur README possible.

> **Définition de « terminé » K→Q** : voir section suivante. Au Sprint 114, le moteur a des
> personnages animés, une image moderne (HDR/bloom/ciel), un gameplay scriptable de bout en bout
> (événements → prefabs → spawn → save), une physique fidèle, un audio vivant, et il tourne dans
> un navigateur — sans avoir trahi un seul refus assumé.

### PHASE R — WebXR, le casque dans le navigateur (115 → 117, dépend de PHASE Q)

> Le Sprint 111 livre un canvas WebGPU classique en 2D plat, **pas** une session
> WebXR — c'est un chantier séparé, à ne démarrer qu'une fois PHASE Q acquise.

#### Sprint 115 — Spike : session WebXR isolée ⬜
- [ ] `cargo build --target wasm32-unknown-unknown --lib` sur le crate actuel (sans
  rendu) pour lister précisément les dépendances bloquantes (`mlua` vendored en C,
  `tokio`/`tokio-tungstenite`) avant d'y toucher.
- [ ] Exemple isolé (hors moteur) `wgpu` + `winit` + `wasm-bindgen` : triangle dans
  un `<canvas>`, puis `navigator.xr.requestSession("immersive-vr")` + rendu stéréo
  trivial (deux triangles colorés, un par œil).
- **Fichiers** : `examples/` (nouveau, isolé du moteur).
- **Livrable** : session WebXR minimale testable dans Chrome avec **Immersive Web
  Emulator** (casque/contrôleurs simulés, sans matériel).
- **Risque** : `mlua` vendored (C) et `tokio` incompatibles wasm32 nu — à
  contourner ou différer avant toute intégration moteur.

#### Sprint 116 — Intégration moteur : rendu stéréo + poses ⬜
- [ ] `XRWebGLLayer`/`XRProjectionLayer` branché sur la surface wgpu du moteur ;
  boucle `XRFrame` (deux vues caméra) cohabitant avec la boucle `winit` existante.
- [ ] Poses tête + contrôleurs/mains (`XRInputSource`) injectées dans `src/app/`.
- **Fichiers** : `src/gfx/renderer.rs`, `src/app/mod.rs`.
- **Livrable** : une scène RusteeGear s'affiche en stéréo dans un casque simulé
  (IWE) ou réel (Quest via navigateur).

#### Sprint 117 — Tests XR automatisés + polish ⬜
- [ ] Scénarios IWE scriptés (déplacement contrôleur, gâchette, préhension d'objet)
  rejoués après chaque changement, via le pont MCP d'IWE si un agent est disponible.
- **Livrable** : une checklist d'interactions XR de base (viser, saisir, téléporter)
  validée sans casque physique à chaque itération.

> **Hors scope confirmé** : performance réelle sur casque autonome (Quest, Pico),
> confort/nausées, hand tracking physique imparfait — un émulateur écran ne les
> mesure pas ; à valider sur matériel réel avant toute publication XR.

---

### PHASE S — Extensions quasi-gratuites (118 → 127)

> Issue du même **audit comparatif à 200 fonctionnalités** que les phases K→Q
> (Godot / Unity / Unreal / RusteeGear, 2026-07-13, re-vérifié dans le code le
> 13 juillet après les sprints 80→99) : une fois K, L, M et N livrées, le score
> RusteeGear sur la grille remonte à ~82–85 / 200, encore loin de la barre
> symbolique de 100. Plutôt que d'inventer de nouveaux chantiers, ces 10 sprints
> activent des items déjà catalogués dans l'audit comme « quasi gratuits » ou
> « une petite marche » une fois un prérequis précis posé — et ce prérequis
> (bus audio, cible HDR, manifeste GUID, skinning GPU, triggers) est justement
> livré par K/L/M/N/O. Avec S, la projection franchit **~101–104 / 200** — une
> projection de lecture de grille, pas une mesure, tant que ces sprints ne sont
> pas livrés. **Aucun refus assumé (🔴) n'est reconsidéré** : pas de boîte noire
> (FMOD/Wwise), pas de GI/Nanite, pas de consoles. Sprints insérables n'importe
> où après leurs prérequis respectifs — même logique de réservoir que P.

#### Sprint 118 — Audio confort (DSP, reverb, ducking, musique adaptative) ⬜
**Objectif** : transformer le bus musique/SFX (Sprint 104) en mixeur complet.
- [ ] Reverb/EQ/limiteur natifs à `kira` sur le bus SFX.
- [ ] Zones de réverbération : triggers (Sprint 89) qui changent le send.
- [ ] Ducking : automation de volume du bus musique quand le SFX joue.
- [ ] Musique adaptative : 2 layers en crossfade (même mécanique que le crossfade d'animation, Sprint 87).
- **Fichiers** : `src/runtime/audio.rs`.
- **Livrable** : une zone de danger assourdit la musique ; les pas d'un combat font baisser la musique puis remonter (ducking) ; 2 layers de musique se croisent sans coupure.
- **Prérequis livré** : bus musique/SFX + panning (Sprint 104).

#### Sprint 119 — Post-effets HDR (exposition auto, grading, vignette) ⬜
**Objectif** : finir la chaîne HDR (Sprint 90) avec ses effets quasi gratuits.
- [ ] Exposition auto : histogramme compute sur la cible HDR.
- [ ] Color grading : LUT 3D appliquée dans la passe finale.
- [ ] Vignette : ~3 lignes dans la passe finale.
- **Fichiers** : `src/gfx/renderer.rs`, `src/gfx/shaders/`.
- **Livrable** : une scène très sombre puis très claire s'expose automatiquement ; une LUT de test change visiblement l'ambiance ; vignette activable/désactivable.
- **Prérequis livré** : cible HDR + tone mapping (Sprint 90).

#### Sprint 120 — SSAO ⬜
- [ ] Occlusion ambiante hémisphère + blur, branchée sur la cible HDR.
- **Fichiers** : `src/gfx/renderer.rs`, `src/gfx/shaders/`.
- **Livrable** : les coins et recoins d'une scène de test s'assombrissent visiblement par rapport au rendu sans SSAO (comparaison avant/après).
- **Prérequis livré** : cible HDR (Sprint 90).

#### Sprint 121 — Variants de shaders + cache ⬜
- [ ] Quelques `#ifdef` maison (ombres on/off, skinning) assemblés à la compilation des pipelines.
- **Fichiers** : `src/gfx/renderer.rs`, `src/gfx/shaders/`.
- **Livrable** : un objet non skinné et un objet skinné cohabitent dans la même scène sans repli sur un unique pipeline monolithique.
- **Prérequis livré** : skinning GPU (Sprint 86).

#### Sprint 122 — Forces de zone (vent, buoyancy) ⬜
- [ ] Force appliquée aux corps rapier dans un trigger.
- **Fichiers** : `src/runtime/physics.rs`, `src/app/mod.rs`.
- **Livrable** : un objet dynamique traversant une zone de vent est visiblement poussé ; retrouve son comportement normal en sortant.
- **Prérequis livré** : triggers + événement exit (Sprint 102).

#### Sprint 123 — Pipeline assets, extensions ⬜
- [ ] Presets qualité par plateforme (généralisation de la réduction mobile existante).
- [ ] Graphe de dépendances d'assets depuis le manifeste GUID (Sprint 95).
- [ ] Règles de budget (polycount, tailles) dans le contrôle qualité APK existant.
- [ ] Normalisation loudness à l'import audio.
- **Fichiers** : `src/assets.rs`.
- **Livrable** : renommer/déplacer un asset référencé ailleurs le signale avant l'export ; un import audio trop fort est normalisé.
- **Prérequis livré** : manifeste GUID (Sprint 95).

#### Sprint 124 — Compression Zstd des packs embarqués ⬜
- [ ] Crate `zstd` sur le blob d'assets embarqué dans le player.
- **Fichiers** : `src/assets.rs`, `packaging/`.
- **Livrable** : taille du `.apk`/`.dmg` mesurée avant/après, réduction documentée.

#### Sprint 125 — Outillage éditeur (recherche de références, profilers, breakpoints Lua) ⬜
- [ ] Graphe de références sur le manifeste GUID (« qui utilise cet asset ? »).
- [ ] Profiler CPU : vue timeline par-dessus les spans `tracing` existants.
- [ ] Profiler mémoire : compteurs par sous-système (au lieu du seul total global).
- [ ] Hooks de debug `mlua` pour des breakpoints Lua basiques.
- **Fichiers** : `src/editor/mod.rs`, `src/app/mod.rs`.
- **Livrable** : supprimer un asset référencé ailleurs est signalé avant coup ; un script Lua peut être mis en pause à une ligne donnée.

#### Sprint 126 — Terrain sculpté + placement assisté ⬜
- [ ] Brosse de hauteur (raycast → heightmap → re-upload de la texture de terrain).
- [ ] Scatter aléatoire d'instances.
- [ ] Drop physique : laisser rapier poser les objets scattérés au sol.
- **Fichiers** : `src/scene/mod.rs`, `src/editor/mod.rs`.
- **Livrable** : une brosse en mode édition creuse/soulève le terrain visiblement ; un scatter de rochers tombe et se stabilise au sol sans intervention manuelle.
- **Prérequis livré** : raycast Lua/éditeur (Sprint 102).

#### Sprint 127 — Localisation + abilities généralisées ⬜
- [ ] Table de clés FR/EN pour le texte runtime (pas l'éditeur). RTL : hors scope, assumé.
- [ ] `combat.rs` (homing, knockback, manches) généralisé en données déclaratives (coût, cooldown, effets à durée) — sans viser l'abstraction complète d'un GAS façon Unreal.
- **Fichiers** : `src/app/combat.rs`, `src/assets.rs`.
- **Livrable** : la démo contrôleur passe en anglais par un réglage ; une nouvelle capacité de combat s'ajoute par des données, sans nouveau code Rust.

> **Définition de « terminé » S** : dix chantiers 🟠 déjà documentés comme peu coûteux
> sont livrés, sans qu'aucun refus assumé (🔴) n'ait été reconsidéré — mixeur audio
> complet, chaîne HDR finie (expo/grading/vignette/SSAO), pipeline assets et
> outillage éditeur étoffés, terrain sculptable, moteur utilisable en anglais.
> Projection : ~101–104 / 200 sur la grille des 200 fonctionnalités.

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
- **K** : plus aucun sprint de rendu sans filet — goldens en CI, simulation reproductible (seed),
  step frame, console dev, debug drawing.
- **L** : un personnage Mixamo **court, s'arrête et saute** dans l'éditeur et **en ligne**, sans à-coup.
- **M** : ciel + fog + HDR/tone mapping + bloom + mipmaps — l'allure de toutes les démos transformée.
- **N** : un jeu à vagues d'ennemis **entièrement scripté en Lua** (spawn/destroy/find, prefabs,
  événements) avec **sauvegarde** restaurée sur mobile.
- **O** : décors importés fidèles (trimesh), projectiles fiables (CCD, couches), escaliers montables
  (controller kinématique) — **prédiction réseau re-validée**.
- **P** : audio mixé/spatialisé/varié, HUD en widgets, manettes, hot-reload, profiler GPU, crash log
  local, doc API publiée.
- **Q** : la démo multijoueur jouable **dans le navigateur**, lien public dans le README.
- **R** : une scène RusteeGear s'affiche **en stéréo dans un casque** (simulé via
  Immersive Web Emulator ou réel via navigateur), poses tête/contrôleurs prises en
  compte, interactions XR de base validées par des scénarios rejouables.
- **S** : mixeur audio complet (DSP, ducking, musique adaptative), chaîne HDR finie
  (exposition auto, grading, vignette, SSAO), pipeline assets et outillage éditeur
  étoffés, terrain sculptable, moteur localisable — score projeté ~101–104 / 200
  sur la grille des 200 fonctionnalités, sans qu'un seul refus assumé (🔴) n'ait
  été reconsidéré.

## 📌 Conseils d'exécution
1. **Faire le Sprint 7 en premier** : sans le refactor, chaque portage dupliquerait du code.
2. **Garder le mode Player (Sprint 14) comme cible de test** mobile — pas l'éditeur complet.
3. **Tester sur device tôt** (Sprint 16) : les surprises GPU/cycle de vie viennent du matériel réel.
4. Avancer **une plateforme à la fois**.
