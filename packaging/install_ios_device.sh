#!/usr/bin/env bash
# Build + signature automatique (compte Apple dans Xcode) + install sur iPhone branché.
# Pré-requis : Xcode, xcodegen (brew install xcodegen), rustup target add aarch64-apple-ios.
set -euo pipefail
cd "$(dirname "$0")/.."
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer

echo "▶ Binaire Rust iOS (release)…"
cargo build --release --target aarch64-apple-ios

DEV=$(xcrun xctrace list devices 2>&1 | grep -iE "iphone|ipad" | grep -vi simulator | head -1 | sed -E 's/.*\(([0-9A-Fa-f-]{8,})\)$/\1/')
[ -z "$DEV" ] && { echo "Aucun iPhone/iPad branché."; exit 1; }
echo "▶ Appareil : $DEV"

cd packaging/ios-xcode
xcodegen generate
xcodebuild -project Motor3DeRust.xcodeproj -scheme Motor3DeRust -configuration Release \
  -destination "id=$DEV" -derivedDataPath build/dd -allowProvisioningUpdates build
APP="build/dd/Build/Products/Release-iphoneos/Motor3DeRust.app"
codesign --verify --deep --strict "$APP"
xcrun devicectl device install app --device "$DEV" "$APP"
xcrun devicectl device process launch --device "$DEV" com.berthod.motor3derust
echo "✅ Installée et lancée sur l'appareil."
