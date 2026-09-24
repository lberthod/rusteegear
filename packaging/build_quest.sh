#!/usr/bin/env bash
# Construit l'APK VR Meta Quest (feature `vr`, session OpenXR — roadmap
# docs/roadmapExportVRQuest24septembre.md). APK distinct de build_apk.sh : autre
# identifiant (com.berthod.rusteegear.vr), s'installe à côté de l'APK téléphone.
#
#   ./packaging/build_quest.sh              # APK de test (profil dev-fast, clé debug)
#   INSTALL=1 ./packaging/build_quest.sh    # + installe et lance sur le casque (adb)
#   RUSTEEGEAR_KEYSTORE_PASS=… ./packaging/build_quest.sh --release
#   VR_SCENE=cubes ./packaging/build_quest.sh   # scène de test de la phase 0 (défaut : Rivière)
#   VR_HZ=90 VR_RENDER_SCALE=0.8 ./packaging/build_quest.sh   # fréquence, résolution de rendu
#   VR_TARGET=pico ./packaging/build_quest.sh     # autre famille de casques (quest, pico, androidxr)
#
# Prérequis : NDK 28.2 (sdkmanager), cargo-apk, casque en mode développeur.
set -euo pipefail
cd "$(dirname "$0")/.."

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$ANDROID_HOME/ndk/28.2.13676358}"
if [ -z "${JAVA_HOME:-}" ]; then
    for jh in "/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
              "/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home"; do
        [ -x "$jh/bin/java" ] && export JAVA_HOME="$jh" && break
    done
fi
[ -n "${JAVA_HOME:-}" ] || { echo "❌ JDK introuvable (JAVA_HOME)"; exit 1; }
ADB="$ANDROID_HOME/platform-tools/adb"

RELEASE=0
[ "${1:-}" = "--release" ] && RELEASE=1

# Lancé par le panneau Export de l'éditeur (PLAYER_BUILD=1, même contrat que
# build_apk.sh) : scène du projet embarquée, identifiant propre au casque
# (« <bundle>.vr », l'APK téléphone du même projet reste installable à côté),
# installation demandée par la case « Installer et lancer sur le casque ».
if [ "${PLAYER_BUILD:-0}" = "1" ]; then
    VR_SCENE="${VR_SCENE:-embedded}"
    if [ -n "${BUNDLE_ID:-}" ] && [ "${BUNDLE_ID%.vr}" = "$BUNDLE_ID" ]; then
        BUNDLE_ID="$BUNDLE_ID.vr"
    fi
    [ "${INSTALL_DEVICE:-0}" = "1" ] && INSTALL=1
fi

# --- Loader OpenXR Khronos (libopenxr_loader.so arm64) ---------------------------
# Le Quest n'embarque pas de loader global : chaque APK apporte le sien, qui
# trouve ensuite le runtime Meta. Pris dans l'AAR officiel Khronos (Maven Central),
# téléchargé une seule fois puis mis en cache (non versionné, cf. .gitignore).
LOADER_VERSION="1.1.63"
RUNTIME_LIBS="packaging/quest/runtime_libs"
LOADER="$RUNTIME_LIBS/arm64-v8a/libopenxr_loader.so"
if [ ! -f "$LOADER" ] || [ "$(cat "$RUNTIME_LIBS/VERSION" 2>/dev/null)" != "$LOADER_VERSION" ]; then
    echo "▶ Loader OpenXR Khronos $LOADER_VERSION…"
    TMP="$(mktemp -d)"
    curl -fsSL -o "$TMP/loader.aar" \
        "https://repo1.maven.org/maven2/org/khronos/openxr/openxr_loader_for_android/$LOADER_VERSION/openxr_loader_for_android-$LOADER_VERSION.aar"
    mkdir -p "$RUNTIME_LIBS/arm64-v8a"
    unzip -p "$TMP/loader.aar" "jni/arm64-v8a/libopenxr_loader.so" > "$LOADER"
    echo "$LOADER_VERSION" > "$RUNTIME_LIBS/VERSION"
    rm -rf "$TMP"
fi

# Clé debug Android (APK de test) : créée comme le ferait Android Studio si absente.
if [ ! -f "$HOME/.android/debug.keystore" ]; then
    mkdir -p "$HOME/.android"
    "$JAVA_HOME/bin/keytool" -genkeypair -keystore "$HOME/.android/debug.keystore" \
        -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 \
        -storepass android -keypass android -dname "CN=Android Debug,O=Android,C=US"
fi

# --- Manifeste Quest, injecté dans Cargo.toml le temps du build ------------------
# Même mécanique que build_apk.sh (cargo-apk ne lit que Cargo.toml) : copie de
# sauvegarde restaurée par le trap, quoi qu'il arrive.
CARGO_BAK="$(mktemp)"
cp Cargo.toml "$CARGO_BAK"
restore_cargo() { cp "$CARGO_BAK" Cargo.toml; rm -f "$CARGO_BAK"; }
trap restore_cargo EXIT

BUNDLE_ID="${BUNDLE_ID:-com.berthod.rusteegear.vr}" \
APP_NAME="${APP_NAME:-RusteeGear VR}" \
RUNTIME_LIBS="$RUNTIME_LIBS" \
DEBUG_KEYSTORE="$HOME/.android/debug.keystore" \
KS_PASS="${RUSTEEGEAR_KEYSTORE_PASS:-}" \
APP_VERSION="${APP_VERSION:-}" \
VR_TARGET="${VR_TARGET:-quest}" \
python3 - <<'EOF'
import os, re
p = "Cargo.toml"
s = open(p).read()
def sub(pattern, repl):
    global s
    s, n = re.subn(pattern, repl, s, count=1, flags=re.M)
    assert n == 1, pattern
sub(r'^package = "[^"]*"', f'package = "{os.environ["BUNDLE_ID"]}"')
sub(r'^label = "[^"]*"', f'label = "{os.environ["APP_NAME"]}"')
# minSdk 29 (Android 10, plancher de Quest OS) ; targetSdk 32 : valeurs attendues
# par Meta pour le Horizon Store — à re-vérifier au moment d'une soumission.
sub(r'^min_sdk_version = \d+', 'min_sdk_version = 29')
sub(r'^target_sdk_version = \d+', 'target_sdk_version = 32')
sub(r'^(build_targets = .*)$', r'\1' + f'\nruntime_libs = "{os.environ["RUNTIME_LIBS"]}"')
# Version (panneau Export) : cargo-apk en dérive aussi le versionCode, qui doit
# croître à chaque envoi sur le Horizon Store.
if os.environ["APP_VERSION"]:
    sub(r'^version = "[^"]*"', f'version = "{os.environ["APP_VERSION"]}"')
if os.environ["KS_PASS"]:
    sub(r'^keystore_password = "[^"]*"', f'keystore_password = "{os.environ["KS_PASS"]}"')
# APK de test (profil `dev-fast`) : cargo-apk exige une clé pour tout profil
# personnalisé ; on lui donne la clé debug standard d'Android (créée par
# cargo-apk au premier build debug, mot de passe public « android »).
s += f'''
[package.metadata.android.signing.dev-fast]
path = "{os.environ["DEBUG_KEYSTORE"]}"
keystore_password = "android"
'''
s += '''
# --- Injecté par packaging/build_quest.sh (APK VR Meta Quest) ---
[[package.metadata.android.uses_feature]]
name = "android.hardware.vr.headtracking"
required = true
version = 1

# Hand tracking (phase 8, Mouvéo/PhysioTech) : optionnel, l'app marche aussi aux manettes.
[[package.metadata.android.uses_feature]]
name = "oculus.software.handtracking"
required = false

[[package.metadata.android.uses_permission]]
name = "com.oculus.permission.HAND_TRACKING"

[package.metadata.android.application.activity]
orientation = "landscape"
launch_mode = "singleTask"
config_changes = "density|keyboard|keyboardHidden|navigation|orientation|screenLayout|screenSize|uiMode"
resizeable_activity = false

# Catégories VR : lancement en immersif (sans elles, l'app s'ouvre en fenêtre
# 2D) — celle de Meta et la catégorie standard OpenXR de Khronos (Pico,
# Android XR et autres casques OpenXR Android la reconnaissent).
[[package.metadata.android.application.activity.intent_filter]]
actions = ["android.intent.action.MAIN"]
categories = ["com.oculus.intent.category.VR", "org.khronos.openxr.intent.category.IMMERSIVE_HMD", "android.intent.category.LAUNCHER"]
'''
# Métadonnées propres à chaque famille de casques (VR_TARGET, défaut quest).
# Pico / Android XR : non vérifiées sur casque — à confirmer avec la doc du
# constructeur avant toute publication (cf. docs/VR_PUBLICATION.md).
target = os.environ["VR_TARGET"]
if target == "quest":
    s += '''
[[package.metadata.android.application.meta_data]]
name = "com.oculus.supportedDevices"
value = "quest3|quest3s|quest2|questpro"

[[package.metadata.android.application.meta_data]]
name = "com.oculus.handtracking.version"
value = "V2.0"
'''
elif target == "pico":
    s += '''
[[package.metadata.android.application.meta_data]]
name = "pvr.app.type"
value = "vr"
'''
elif target == "androidxr":
    s += '''
[[package.metadata.android.uses_feature]]
name = "android.software.xr.api.openxr"
required = true
'''
else:
    raise SystemExit(f"VR_TARGET inconnu : {target} (quest, pico, androidxr)")
open(p, "w").write(s)
EOF

PROFILE_ARGS=()
if [ "$RELEASE" = 1 ]; then
    : "${RUSTEEGEAR_KEYSTORE_PASS:?--release exige RUSTEEGEAR_KEYSTORE_PASS (cf. build_apk.sh)}"
    [ -f packaging/release.keystore ] || { echo "❌ packaging/release.keystore absent — lancer build_apk.sh une fois pour le générer"; exit 1; }
    PROFILE_ARGS=(--release)
    APK="target/release/apk/motor3derust.apk"
else
    # Profil `dev-fast` (opt-level 1, cf. Cargo.toml) plutôt que `dev` : le code
    # du moteur non optimisé (scène Rivière : 3 104 objets parcourus à chaque
    # image) ne tiendrait pas 72-90 Hz dans le casque. Sans infos de débogage :
    # ~500 Mo de `.so` sinon (constaté), interminable à pousser en USB. Les
    # journaux `log::` restent.
    PROFILE_ARGS=(--profile dev-fast)
    APK="target/dev-fast/apk/motor3derust.apk"
    export CARGO_PROFILE_DEV_FAST_DEBUG=0 CARGO_PROFILE_DEV_FAST_STRIP=symbols
fi

# Réglages lus à la compilation par `xr::hello` (option_env!) : scène,
# fréquence d'affichage (défaut 72 Hz), résolution de rendu (× recommandée).
export RUSTEEGEAR_VR_SCENE="${VR_SCENE:-riviere}"
export RUSTEEGEAR_VR_HZ="${VR_HZ:-72}"
export RUSTEEGEAR_VR_RENDER_SCALE="${VR_RENDER_SCALE:-1.0}"
echo "▶ Scène VR : $RUSTEEGEAR_VR_SCENE · $RUSTEEGEAR_VR_HZ Hz · résolution × $RUSTEEGEAR_VR_RENDER_SCALE"
echo "▶ cargo apk build ${PROFILE_ARGS[*]+"${PROFILE_ARGS[*]}"} --lib --features vr"
cargo apk build ${PROFILE_ARGS[@]+"${PROFILE_ARGS[@]}"} --lib --features vr
# Renomme selon OUTPUT_NAME (panneau Export), comme build_apk.sh.
if [ -n "${OUTPUT_NAME:-}" ]; then
    mkdir -p target/export
    cp "$APK" "target/export/${OUTPUT_NAME}-quest.apk"
    APK="target/export/${OUTPUT_NAME}-quest.apk"
fi
echo "✅ APK Quest : $APK ($(du -h "$APK" | cut -f1))"

if [ "${INSTALL:-0}" = 1 ]; then
    BUNDLE="${BUNDLE_ID:-com.berthod.rusteegear.vr}"
    "$ADB" install -r "$APK"
    "$ADB" shell am start -n "$BUNDLE/android.app.NativeActivity"
    # android_logger étiquette chaque ligne avec le chemin du module Rust
    # (`motor3derust::xr::hello`…) : filtrer par motif, pas par tag exact.
    echo "▶ Journaux : $ADB logcat | grep -E 'motor3derust|OpenXR|VrApi|panicked'"
fi
