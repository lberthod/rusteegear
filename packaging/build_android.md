# Build Android (`.apk`)

État : **préparé, bloqué sur le NDK**. La cible Rust et le backend winit Android
sont configurés, mais produire l'`.apk` nécessite le NDK Android (non installé ici)
+ une dernière intégration `android-activity`.

## Ce qui est déjà fait
- Cible Rust : `rustup target add aarch64-linux-android` ✅
- `Cargo.toml` : winit avec la feature `android-native-activity` (ciblée Android) ✅
- Mode **Player** auto-activé sur Android (cf. `src/main.rs`) ✅

## Ce qu'il reste (prérequis lourds)

### 1. Installer le NDK
Via Android Studio ▸ SDK Manager ▸ onglet *SDK Tools* ▸ cocher **NDK (Side by side)**.
Ou en ligne de commande (sdkmanager) :
```bash
sdkmanager "ndk;27.0.12077973"
export ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/27.0.12077973
```
> Sans NDK : erreur `aarch64-linux-android-clang: No such file` (Lua/`mlua` ne peut
> pas compiler son C, et le linker natif manque).

### 2. Outils de build
```bash
cargo install cargo-ndk      # ou cargo-apk
```

### 3. Intégration `android-activity` (code)
Android lance une bibliothèque native (`.so`) via `NativeActivity`, pas un binaire.
Il faut donc, derrière `#[cfg(target_os = "android")]` :
- exposer le crate en `cdylib` ;
- fournir le point d'entrée `android_main(app: AndroidApp)` qui crée l'`EventLoop`
  via `EventLoopBuilder::with_android_app(app)` (winit `platform::android`).

### 4. Construire l'APK
```bash
cargo ndk -t arm64-v8a -o ./jniLibs build --release
# puis empaqueter via cargo-apk / Gradle + AndroidManifest
```

## Résumé
La logique de portage est identique à iOS (mode Player + tactile déjà faits, moteur
portable via wgpu/Vulkan). Le travail restant est **environnemental** (NDK) +
l'entrée `android_main`. Aucun blocage côté moteur lui-même.
