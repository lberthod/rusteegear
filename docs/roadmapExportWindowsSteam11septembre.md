# Roadmap — export Windows et mise en ligne Steam (11 septembre 2026)

Suite à la question « les jeux exportés par RusteeGear peuvent-ils être listés
sur Steam ? ». Réponse courte : oui, Steam ignore le moteur et n'exige qu'un
exécutable natif par plateforme. Ce qui manque, c'est un **player Windows**
(l'essentiel des joueurs Steam), puis le tuyau SteamPipe. Cette roadmap suit
la convention des précédentes
([roadmapAnalyseComparative4septembre.md](roadmapAnalyseComparative4septembre.md)) :
chaque sprint dit ce qui est livré, où, comment c'est vérifié, et ce qui reste
volontairement hors périmètre.

| # | Sprint | Durée | État |
|---|---|---|---|
| 0 | Portabilité Windows du player (chemins, console, icône, CRT) | ½ j | ⬜ |
| 1 | `packaging/build_windows.sh` + job Windows dans `release.yml` | ½ j | ⬜ |
| 2 | Cible « Windows · .zip » dans le panneau Export (local + distant via CI) | 1 j | ⬜ |
| 3 | Premier lancement réel sur GPU Windows (pilot + capture en CI) | ½ j | ⬜ |
| 4 | Steamworks : compte, dépôts, `steam-deploy` sur tag | ½ j + délais Valve | ⬜ |
| 5 | Optionnel : player Linux natif, SDK Steamworks depuis Lua | — | ⬜ |

## État des lieux (ce qui rend le chantier court)

- **Pipeline existant** : le panneau Export (`src/editor/export.rs::start`)
  écrit `assets/player_scene.json` et `assets/bundle/` (assets zstd), puis
  lance `packaging/build_*.sh` avec `PLAYER_BUILD=1` → `cargo build --release
  --features player_build`. La feature force le mode joueur
  (`src/lib.rs`, `run()`), la scène est figée dans le binaire par
  `include_str!`/`include_dir!` (`build.rs` surveille les deux chemins).
  Un player Windows = la même commande sur un hôte Windows.
- **Le code compile et teste déjà sur Windows** : job CI `editor-windows`
  (build des 4 binaires + `cargo test --lib`) vert sur les runs
  34577096294 et 34534076697 de `main`. Toujours en `continue-on-error`.
- **Aucune DLL à livrer** : `mlua` vendored, `zstd-sys` statique, wgpu charge
  DX12/Vulkan au runtime, `kira`/cpal → WASAPI, `gilrs` → XInput. Le zip Steam
  contient un seul `.exe`.
- **Scène et bundle sont versionnés** (`assets/player_scene.json`,
  `assets/bundle/` ne sont pas dans `.gitignore`) : un export peut donc être
  committé sur une branche et construit par la CI, ce qui règle le problème
  de l'hôte (on développe sur macOS, Windows se compile sur Windows).
- **Cross-compilation depuis macOS écartée** : `cargo-xwin` ou zig vers
  `x86_64-pc-windows-gnu` sont possibles mais fragiles avec `lua-src` et
  `zstd-sys` (compilation C) ; le runner Windows existe déjà et marche.

## Sprint 0 — portabilité Windows du player

Correctifs sans lesquels le premier `.exe` se comporte mal, même s'il compile.

- **Dossier utilisateur** : neuf usages de `std::env::var("HOME")`
  (`src/app/build_config.rs`, `src/app/settings.rs` via `assets.rs`,
  `src/log_buffer.rs`, `src/app/mod.rs`, `src/editor/export.rs`). `HOME`
  n'est pas défini sous Windows hors Git Bash : réglages, journaux et config
  d'export tombent silencieusement sur `None`/`.`. Introduire un
  `crate::paths::home_dir()` (HOME, sinon USERPROFILE) et remplacer les neuf
  appels. Test unitaire : `home_dir()` non vide quand seul USERPROFILE est
  posé.
- **Pas de console noire** : dans `src/main.rs`,
  `#![cfg_attr(all(windows, feature = "player_build"), windows_subsystem = "windows")]`.
  L'éditeur garde sa console (logs `env_logger` visibles). Vérifier que le
  pont `--pilot` et `log_buffer` n'écrivent pas sur un stdout fermé (le
  `println!` sur un handle absent panique sous `windows_subsystem`).
- **Icône et version dans l'exe** : `winresource` en `[build-dependencies]`
  (cible windows uniquement), `assets/icon/icon.ico` généré depuis
  `icon_256.png` (ImageMagick `convert` ou `png2ico`), `build.rs` pose
  l'icône, `FileVersion`/`ProductName` depuis `APP_VERSION`/`APP_NAME` (mêmes
  variables que `build_apk.sh`).
- **Runtime C statique** : `.cargo/config.toml`,
  `[target.x86_64-pc-windows-msvc] rustflags = ["-C", "target-feature=+crt-static"]`
  — sinon le joueur doit avoir le redistribuable VC++ (Steam sait l'installer
  via « Install Scripts », mais autant ne pas en dépendre).
- **Annulation d'export** (`export.rs::kill_process_tree`, branche
  `not(unix)`) : ne tue que `bash`, `cargo` continue. Utiliser
  `taskkill /PID <pid> /T /F`. Pertinent seulement pour l'export local depuis
  un éditeur Windows (Sprint 2), pas bloquant.
- **Hors périmètre** : installateur MSI/NSIS (Steam n'en veut pas), signature
  Authenticode (SmartScreen se tait pour les exes lancés par Steam).

## Sprint 1 — `build_windows.sh` et job de release

- **`packaging/build_windows.sh`**, même contrat que `build_dmg.sh` :
  `RUSTEEGEAR_COMMIT`, `OUTPUT_NAME`, `PLAYER_BUILD`, `APP_VERSION`,
  `BUILD_NUMBER`. Fait `cargo build --release --bin motor3derust
  [--features player_build]`, copie `target/release/motor3derust.exe` vers
  `target/export/<OUTPUT_NAME>-windows/<OUTPUT_NAME>.exe`, puis zip
  (`7z a` ou `powershell Compress-Archive`, les deux présents sur les
  runners). Tourne sous Git Bash localement et `shell: bash` en CI.
- **`release.yml`** : job `windows` (`runs-on: windows-latest`, LFS comme
  `editor-windows`) qui produit `RusteeGear-Editor-<tag>-windows.zip` et
  l'attache à la Release. Bonus immédiat : un éditeur Windows téléchargeable
  pour les testeurs, trou n° 1 de l'analyse comparative du 4 septembre.
- **Vérifié** : release de test sur un tag `v0.x.y-rc`, zip présent, taille
  cohérente avec le .dmg, `strings` de l'exe montre le commit.
- **Documentation** : ligne Windows dans le tableau « Pré-requis par
  plateforme » de `packaging/EXPORT.md` et dans `KNOWN_LIMITATIONS.md`.

## Sprint 2 — cible Windows dans le panneau Export

- **`Target::Windows`** dans `src/editor/export.rs` : label
  « Windows · .zip », script `packaging/build_windows.sh`, ajout dans
  `prereqs`, `targets` de « Tout exporter », `install` = jamais.
- **Détection (`detect`)** :
  - hôte Windows → `cargo` présent, `rust_target_installed("x86_64-pc-windows-msvc")`
    (déjà la cible hôte), Build Tools Visual Studio détectés via `cl.exe` ou
    `vswhere`. Export local, identique à macOS aujourd'hui.
  - hôte macOS/Linux → « export distant (CI) », prérequis : `gh` authentifié
    (`gh auth status`), dépôt propre ou seulement `assets/player_scene.json`
    + `assets/bundle/` modifiés.
- **Export distant** (`export_remote.rs`, nouveau, même canal `LogMsg`) :
  1. `git checkout -B export/<safe_name>-<build_number>` depuis HEAD,
     `git add assets/player_scene.json assets/bundle`, commit « Export
     <nom> build N », `git push -u origin`.
  2. Nouveau workflow `.github/workflows/export.yml` sur
     `push: branches: ["export/**"]` : job Windows (et Linux, même prix)
     lançant `build_windows.sh` avec `PLAYER_BUILD=1` et les variables
     tirées du nom de branche ou d'un `export.env` committé avec la scène ;
     artefact `<nom>-windows.zip`.
  3. Le panneau suit avec `gh run watch --exit-status` (sortie streamée dans
     le journal), puis `gh run download -n <nom>-windows -D target/export/`.
  4. Retour sur la branche de départ ; la branche `export/**` est gardée
     (traçabilité build ↔ commit) et nettoyée par un `gh api` après 30 jours
     ou à la main.
- **Vérifié** : export distant depuis macOS de la scène Hameau, zip
  téléchargé dans `target/export/`, journal montrant les étapes ; test
  unitaire du parseur de sortie `gh run watch` ; test que le dépôt revient
  sur la branche d'origine même si le push échoue.
- **Hors périmètre** : file d'attente multi-exports simultanés, export
  distant macOS (le .dmg reste local).

## Sprint 3 — premier lancement réel sur GPU Windows

Le player Windows n'a jamais été *exécuté*, seulement compilé. Même preuve que
le job `editor-linux` :

- Dans `export.yml` (ou `editor-windows` de `ci.yml`), lancer
  `motor3derust.exe --pilot` avec `WGPU_BACKEND=dx12` (fenêtre réelle sur le
  runner, pas de Xvfb nécessaire ; à défaut, `vulkan` + SwiftShader ou
  `WARP`), boucler sur `pilot state`, `pilot screenshot editor-windows.png`,
  `console play`/`stop`, publier la capture. `timeout` équivalent :
  `Start-Process` + `Wait-Process -Timeout`.
- **Points à observer** : création du device DX12, presse-papiers, chemins
  `\` dans `assets.rs` (le job Windows passe déjà `cargo test --lib`, donc
  probablement sains), gamepad XInput, audio WASAPI sans périphérique (le
  runner n'a pas de carte son : `kira` doit démarrer sans paniquer, comme
  sous Xvfb).
- Après ce sprint et 15 runs verts : retirer `continue-on-error` du job
  `editor-windows` (règle du dépôt, cf. `golden`).

## Sprint 4 — Steamworks

Prérequis côté compte (hors code, délais Valve) :

1. Steam Direct : 100 USD par application (rendus après 1 000 USD de ventes
   nettes), identité, coordonnées fiscales et bancaires. Validation
   quelques jours.
2. Créer l'app → App ID, deux dépôts (Windows 64 bits, macOS), une branche
   de build `beta` privée avec mot de passe.
3. Page boutique (captures, description, tags, capsule) et build soumis à
   revue, chacune 2 à 5 jours ouvrés ; « Coming soon » possible avant.

Côté dépôt :

- **`packaging/steam/app_build.vdf`** + `depot_windows.vdf`,
  `depot_macos.vdf` : `ContentRoot` = `target/export/<nom>-windows/` et le
  contenu du `.app` (pas le .dmg). Launch options : `<nom>.exe` (Windows),
  `<nom>.app` (macOS). Un `steam_appid.txt` **n'est pas** nécessaire tant
  que le SDK n'est pas intégré.
- **`release.yml`** : après les jobs `windows` et `macos`, job `steam` avec
  `game-ci/steam-deploy@v3` (`appId`, `buildDescription = tag`,
  `depot1Path`/`depot2Path`, `releaseBranch: beta`). Secrets
  `STEAM_USERNAME`, `STEAM_CONFIG_VDF` (issu de `steamcmd` avec Steam Guard
  validé une fois en local, procédure documentée dans `packaging/EXPORT.md`).
  Même précaution que les secrets Android
  (`project-android-release-secrets-pending`) : le job saute si le secret
  est absent, il ne casse pas la release.
- **Vérifié** : build visible dans Steamworks › SteamPipe, installé via le
  client Steam sur un PC Windows et un Mac, lancement depuis la
  bibliothèque, overlay Shift+Tab (fonctionne sans SDK sur DX12/Vulkan ;
  sous Metal l'overlay est absent, connu et accepté).
- **macOS** : Steam n'exige ni signature ni notarisation ; Gatekeeper ne
  met pas en quarantaine un fichier écrit par le client Steam. Le
  `build_dmg.sh` actuel suffit.
- **Steam Deck** : le build Windows tourne via Proton ; viser « Playable »
  demande la manette (déjà `gilrs`) et une résolution 1280×800 lisible
  (HUD egui à vérifier), pas de Linux natif.
- **Hors périmètre** : succès, cloud saves, classements, DRM Steam.

## Sprint 5 — optionnel

- **Player Linux natif** : `build_linux.sh` (tar.gz, `AppImage` inutile pour
  Steam), job `ubuntu-latest` avec les `apt` du job `editor-linux`, dépôt
  Linux dans Steamworks. Utile surtout pour la revue « Deck Verified » et
  les joueurs Linux natifs (≈ 2 %).
- **SDK Steamworks depuis Lua** : crate `steamworks` (bindings Rust du
  SDK, redistribuable `steam_api64.dll`/`libsteam_api.dylib` à livrer à côté
  de l'exe, `steam_appid.txt` en dev). Pont `steam.unlock_achievement(id)`,
  `steam.set_stat`, `steam.user_name` dans `src/app/scripting.rs`, no-op sur
  web/mobile et hors Steam, comme les autres ponts plateforme
  (`docs/LUA_API.md`). Feature Cargo `steam` désactivée par défaut pour ne
  pas alourdir la CI.

## Ordre conseillé

Sprints 0 et 1 d'abord (une journée, aucune dépendance externe, livre un
éditeur Windows aux testeurs au passage). Ouvrir le compte Steam Direct en
parallèle : c'est le chemin critique (validation Valve). Sprint 3 avant
d'envoyer quoi que ce soit à Steam. Sprint 2 peut attendre : tant que
l'export Windows se déclenche par tag ou branche `export/**` à la main, il
est déjà utilisable.
