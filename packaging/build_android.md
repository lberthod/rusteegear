# Build Android (`.apk`)

État : **la cross-compilation Android fonctionne** ✅ (NDK installé). Reste à
empaqueter en `.apk` (cdylib + `android_main` + cargo-apk).

## Ce qui fonctionne
- Cible Rust `aarch64-linux-android` ✅
- `Cargo.toml` : winit avec la feature `android-native-activity` (ciblée Android) ✅
- Mode **Player** auto-activé sur Android ✅
- **NDK 28.2.13676358** installé via les command-line tools d'Android Studio ✅
- **Compilation + linking complets** du moteur pour Android (Lua C, kira, rapier, wgpu, winit). ✅

## Installation du NDK (faite, pour mémoire)
```bash
# command-line tools (fournissent sdkmanager) dans ~/Library/Android/sdk/cmdline-tools/latest
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
SM=~/Library/Android/sdk/cmdline-tools/latest/bin/sdkmanager
yes | "$SM" --licenses
yes | "$SM" --install "ndk;28.2.13676358" "platform-tools"
```
> Alternative GUI : Android Studio ▸ SDK Manager ▸ SDK Tools ▸ cocher **NDK (Side by side)**.

## Compiler le moteur pour Android
```bash
source packaging/android_env.sh
cargo build --release --target aarch64-linux-android
```
> ⚠️ API **26 minimum** (variable `..android26-clang`) : `libaaudio` (audio) n'existe
> qu'à partir de l'API 26. Lier en API 24 échoue avec `unable to find library -laaudio`.

## Reste pour un `.apk` installable
Android lance une bibliothèque native (`.so`) via `NativeActivity`, pas un binaire :
- exposer le crate en `cdylib` (derrière `#[cfg(target_os = "android")]`) ;
- fournir `android_main(app: AndroidApp)` → `EventLoopBuilder::with_android_app(app)` ;
- empaqueter avec `cargo install cargo-apk` puis `cargo apk build --release`.

Aucun blocage côté moteur : il compile et se lie pour Android.
