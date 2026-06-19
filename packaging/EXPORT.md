# Exporter un jeu — panneau « Build & Export »

L'éditeur Motor3DeRust (desktop) exporte le **jeu créé** sous forme de *player*
jouable pour chaque plateforme : un `.dmg`, `.apk` ou `.ipa` qui démarre directement
sur ta scène en mode joueur (ta scène est **embarquée** dans le binaire à l'export).

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

Le panneau affiche **✓ prêt** ou **⚠ + ce qui manque** par cible (détecté au lancement).

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

> **macOS / Android** : le bundle id et la version *internes* proviennent encore de
> `Cargo.toml` (cargo-bundle / cargo-apk les y lisent) ; seul le **nom de fichier** est
> appliqué. L'override complet (patch du `.app` / manifest) reste à faire.

## CI de release

Pousser un tag `v*` déclenche `.github/workflows/release.yml` : build des artefacts
macOS et Android, attachés à la Release GitHub. L'IPA signé n'est pas produit en CI
(certificat + profil à fournir en *GitHub Secrets*).
