#!/usr/bin/env bash
# Construit un .ipa iOS (NON signé) à partir du binaire Rust arm64.
#
# Pré-requis :
#   - Xcode complet installé (pas seulement les Command Line Tools).
#     Activer : sudo xcode-select -s /Applications/Xcode.app
#     ou exporter : export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
#   - Cible Rust : rustup target add aarch64-apple-ios
#
# ⚠️ Le .ipa produit n'est PAS signé : il ne s'installe pas tel quel sur un appareil.
#    Pour un device, il faut un compte développeur Apple + signature/provisioning
#    (codesign + embedded.mobileprovision), généralement via Xcode.
set -euo pipefail
cd "$(dirname "$0")/.."

export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
APP_NAME="Motor3DeRust"
BIN="motor3derust"
BID="com.berthod.motor3derust"

echo "▶ Compilation Rust pour iOS (release)…"
cargo build --release --target aarch64-apple-ios

OUT="target/ios"
APP="$OUT/Payload/$APP_NAME.app"
rm -rf "$OUT"
mkdir -p "$APP"

cp "target/aarch64-apple-ios/release/$BIN" "$APP/$BIN"

cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleExecutable</key><string>$BIN</string>
    <key>CFBundleIdentifier</key><string>$BID</string>
    <key>CFBundleName</key><string>$APP_NAME</string>
    <key>CFBundleDisplayName</key><string>$APP_NAME</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>MinimumOSVersion</key><string>13.0</string>
    <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
    <key>CFBundleSupportedPlatforms</key><array><string>iPhoneOS</string></array>
    <key>UILaunchScreen</key><dict/>
    <key>UIRequiredDeviceCapabilities</key><array><string>arm64</string><string>metal</string></array>
</dict>
</plist>
PLIST

echo "▶ Création du .ipa…"
( cd "$OUT" && zip -qr "$APP_NAME.ipa" Payload )

echo "✅ IPA (non signé) : $OUT/$APP_NAME.ipa"
echo "   $(du -h "$OUT/$APP_NAME.ipa" | cut -f1)"
echo
echo "Pour installer sur un appareil : signature requise (compte développeur Apple)."
echo "Le chemin recommandé reste un projet Xcode (cargo-mobile2) pour signer/provisionner."
