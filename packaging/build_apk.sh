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
cargo apk build --release

APK="target/release/apk/motor3derust.apk"
echo "✅ APK : $APK ($(du -h "$APK" | cut -f1))"
echo "   Installer : adb install -r $APK"
