#!/usr/bin/env bash
# Construit l'APK Android (release, signé). Nécessite : NDK installé + cargo-apk.
#   cargo install cargo-apk
set -euo pipefail
cd "$(dirname "$0")/.."

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$ANDROID_HOME/ndk/28.2.13676358}"
export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"

KS="packaging/release.keystore"
if [ ! -f "$KS" ]; then
    echo "▶ Génération d'une clé de signature ($KS)…"
    "$JAVA_HOME/bin/keytool" -genkeypair -v -keystore "$KS" \
        -alias motor3derust -keyalg RSA -keysize 2048 -validity 10000 \
        -storepass android -keypass android -dname "CN=Motor3DeRust, O=Berthod, C=CH"
fi

echo "▶ Build APK (release)…"
# --lib : l'app Android est le cdylib (android_main). Sans ça, cargo-apk voit aussi
# le bin desktop (main.rs) et panique (« Bin is not compatible with Cdylib »).
cargo apk build --release --lib

APK="target/release/apk/motor3derust.apk"
# Renomme selon OUTPUT_NAME (fourni par le panneau Export) ; sinon garde le nom par défaut.
OUTPUT_NAME="${OUTPUT_NAME:-}"
if [ -n "$OUTPUT_NAME" ] && [ "$OUTPUT_NAME" != "motor3derust" ]; then
    mkdir -p target/export
    cp "$APK" "target/export/${OUTPUT_NAME}.apk"
    APK="target/export/${OUTPUT_NAME}.apk"
fi
echo "✅ APK : $APK ($(du -h "$APK" | cut -f1))"

# Installation directe sur l'appareil branché (option du panneau Export).
if [ "${INSTALL_DEVICE:-0}" = "1" ]; then
    if command -v adb >/dev/null 2>&1; then
        echo "▶ Installation sur l'appareil (adb install -r)…"
        adb install -r "$APK"
    else
        echo "⚠️ adb introuvable : installation ignorée."
    fi
else
    echo "   Installer : adb install -r $APK"
fi
