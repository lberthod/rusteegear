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
xcodebuild -project RusteeGear.xcodeproj -scheme RusteeGear -configuration Release \
  -destination "id=$DEV" -derivedDataPath build/dd -allowProvisioningUpdates build
APP="build/dd/Build/Products/Release-iphoneos/RusteeGear.app"

# Le binaire Rust est injecté APRÈS la signature Xcode → re-signer nous-mêmes
# (sinon "No code signature found"). On réutilise les entitlements générés par Xcode.
XCENT=$(find build/dd -name "RusteeGear.app.xcent" | head -1)
IDENTITY=$(security find-identity -v -p codesigning | grep "Apple Development" | head -1 | sed -E 's/.*"(.*)"/\1/')
codesign --force --sign "$IDENTITY" --entitlements "$XCENT" --generate-entitlement-der "$APP"
codesign --verify --deep --strict "$APP"
xcrun devicectl device install app --device "$DEV" "$APP"
xcrun devicectl device process launch --terminate-existing --device "$DEV" com.berthod.motor3derust
echo "✅ Installée et lancée sur l'appareil."
