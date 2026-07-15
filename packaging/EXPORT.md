# Exporter un jeu — panneau « Build & Export »

L'éditeur RusteeGear (desktop) exporte le **jeu créé** sous forme de *player*
jouable pour chaque plateforme : un `.dmg`, `.apk` ou `.ipa` qui démarre directement
sur ta scène en mode joueur. La scène **et ses assets** (modèles glTF, sons) sont
**embarqués** dans le binaire à l'export (copiés dans `assets/bundle/`, inclus à la
compilation) : le player tourne donc sur un autre poste/appareil sans fichiers externes.
Un asset introuvable au moment de l'export est signalé dans le journal (⚠).

Ouvre le panneau via le bouton **📦 Export** de la barre d'outils.

## Configuration (persistée dans `~/.motor3derust/build_config.json`)

| Champ | Rôle |
|---|---|
| **Nom de l'app** | nom du fichier de sortie (`target/export/<nom>.<ext>`) |
| **Bundle id** | identifiant inversé (ex. `com.exemple.monjeu`) — validé en direct |
| **Version** | version marketing (CFBundleShortVersionString / versionName) |
| **Build #** | numéro interne, **auto-incrémenté** à chaque export |
| **Signature iOS** | Team ID, identité, profil `.mobileprovision` (vides = défauts du script) |

Les **préréglages** (combo « Préréglage ») enregistrent une configuration nommée
dans `~/.motor3derust/presets/` — pratique pour « Démo », « Interne », « Store ».

## Pré-requis par plateforme

| Cible | Outils | Installation |
|---|---|---|
| **macOS** (.dmg) | `cargo-bundle` | `cargo install cargo-bundle` |
| **Android** (.apk) | `cargo-apk` + NDK + cible Rust | `cargo install cargo-apk` · NDK via Android Studio · `rustup target add aarch64-linux-android` |
| **iOS** (.ipa) | Xcode + `xcodegen` + cible Rust + identité de signature | `brew install xcodegen` · `rustup target add aarch64-apple-ios` · certificat « Apple Development » dans le trousseau |
| **Web** (.zip) | cible wasm32 + `wasm-bindgen-cli` (version exacte du lockfile, vérifiée par le panneau) | `rustup target add wasm32-unknown-unknown` · `cargo install wasm-bindgen-cli --version <version du Cargo.lock>` |

Le panneau affiche **✓ prêt** ou **⚠ + ce qui manque** par cible (détecté au lancement).

> **Web** : produit `target/export/<nom>-web.zip` (index.html + wasm/JS, scène et
> assets embarqués) — décompresser et servir en HTTP statique (ex. `python3 -m
> http.server`), ouvrir dans Chrome (WebGPU). Scripts Lua et meshes à animation
> squelettale fonctionnels depuis le Sprint 137. Limite actuelle du player web
> (ROADMAP_SPRINTS.md, Sprints 114-116) : musique en flux absente.

## Installation sur appareil

- **Android** : coche « Installer sur l'appareil (adb) » → `adb install -r` après le build (appareil branché, débogage USB activé).
- **iOS** : coche « Installer sur l'iPhone branché (devicectl) » → build + signature + installation/lancement via `install_ios_device.sh` (nécessite un appareil enregistré et un certificat).

Après un export réussi : **📂 Révéler le dossier** ouvre `target/export/`.

## Tout exporter

Le bouton **🚀 Tout exporter** enfile les cibles **prêtes** et les construit l'une
après l'autre (le log streame chaque build).

## Variables transmises aux scripts

Le panneau pilote `packaging/build_*.sh` via l'environnement :
`OUTPUT_NAME`, `BUNDLE_ID`, `APP_VERSION`, `BUILD_NUMBER`, `PLAYER_BUILD=1`,
`INSTALL_DEVICE`, et pour iOS `TEAM_ID` / `IDENTITY` / `PROFILE`.

> **Android (Sprint 36)** : `build_apk.sh` injecte désormais `BUNDLE_ID` (package),
> `APP_NAME` (label) et `APP_VERSION` (versionName) dans `Cargo.toml` avant le build,
> puis **restaure** le fichier (trap). L'identité de l'APK reflète donc le panneau Export.
> **macOS** : seul le nom de fichier est appliqué ; le patch du `.app` reste à faire.

## Validation sur appareil (Android)

Checklist de la boucle complète, à exécuter une fois sur un appareil réel :
1. Brancher le téléphone (USB, débogage activé) → `adb devices` le liste.
2. Panneau Export → Android, cocher *Installer sur l'appareil* → Build APK.
3. Au lancement : vérifier le **joystick** + **boutons** (la démo mobile bouge le personnage).
4. Mettre l'app en arrière-plan puis revenir : le rendu **reprend** sans crash.
5. `adb logcat | grep RusteeGear` pour les logs natifs en cas de souci.

## Distribution signée (à fournir en *GitHub Secrets*)

- **Android** : `release.keystore` est déjà signé (clé de dev) ; pour le Play Store,
  remplacer par une clé d'upload et stocker `KEYSTORE_BASE64` / mots de passe en secrets.
- **iOS** : certificat `.p12` + profil `.mobileprovision` en secrets, puis activer un job
  iOS dans `release.yml` (non activé : nécessite un compte développeur Apple).
- **macOS** : notarisation (`xcrun notarytool`) avec un Apple ID en secrets.

## CI de release

Pousser un tag `v*` déclenche `.github/workflows/release.yml` : build des artefacts
macOS et Android, attachés à la Release GitHub. L'IPA signé n'est pas produit en CI
(certificat + profil à fournir en *GitHub Secrets*).
