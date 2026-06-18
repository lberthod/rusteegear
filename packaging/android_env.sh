#!/usr/bin/env bash
# Environnement de cross-compilation Android (NDK installé via Android Studio).
# Usage : source packaging/android_env.sh   puis   cargo build --target aarch64-linux-android
#
# NDK installé avec les command-line tools :
#   sdkmanager "ndk;28.2.13676358"   (depuis ~/Library/Android/sdk/cmdline-tools/latest/bin)

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"

# API 26 minimum : requis pour AAudio (lib audio Android utilisée par kira/cpal).
TC="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64"
export CC_aarch64_linux_android="$TC/bin/aarch64-linux-android26-clang"
export AR_aarch64_linux_android="$TC/bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/bin/aarch64-linux-android26-clang"

echo "NDK : $ANDROID_NDK_HOME"
echo "→ cargo build --target aarch64-linux-android"
