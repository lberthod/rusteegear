#!/usr/bin/env bash
# Construit l'app iOS et un .ipa, signé avec votre identité de développeur Apple.
#
# Pré-requis :
#   - Xcode complet : export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
#   - rustup target add aarch64-apple-ios
#   - Identité « Apple Development » dans le trousseau (security find-identity -v -p codesigning)
#
# Variables :
#   IDENTITY  : nom de l'identité de signature (sinon .ipa NON signé)
#   PROFILE   : chemin d'un .mobileprovision (sinon pas de profil → n'installe pas sur device)
#   TEAM_ID   : Team identifier (défaut N668CK695Q)
set -euo pipefail
cd "$(dirname "$0")/.."

export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
APP_NAME="Motor3DeRust"; BIN="motor3derust"; BID="com.berthod.motor3derust"
TEAM_ID="${TEAM_ID:-N668CK695Q}"
IDENTITY="${IDENTITY:-Apple Development: lberthod@gmail.com (32AA99MN7M)}"

echo "▶ Compilation Rust pour iOS (release)…"
cargo build --release --target aarch64-apple-ios

OUT="target/ios"; APP="$OUT/Payload/$APP_NAME.app"
rm -rf "$OUT"; mkdir -p "$APP"
cp "target/aarch64-apple-ios/release/$BIN" "$APP/$BIN"

cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
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
</dict></plist>
PLIST

if [ -n "${IDENTITY:-}" ]; then
    cat > "$OUT/entitlements.plist" <<ENT
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>application-identifier</key><string>$TEAM_ID.$BID</string>
<key>com.apple.developer.team-identifier</key><string>$TEAM_ID</string>
<key>get-task-allow</key><true/>
</dict></plist>
ENT
    if [ -n "${PROFILE:-}" ]; then
        echo "▶ Intégration du profil de provisioning…"
        cp "$PROFILE" "$APP/embedded.mobileprovision"
    fi
    echo "▶ Signature avec : $IDENTITY"
    codesign --force --sign "$IDENTITY" --entitlements "$OUT/entitlements.plist" "$APP"
    codesign -dv --verbose=2 "$APP" 2>&1 | grep -E "Authority|TeamIdentifier" | head -2
fi

( cd "$OUT" && rm -f "$APP_NAME.ipa" && zip -qr "$APP_NAME.ipa" Payload )
IPA="$OUT/$APP_NAME.ipa"
# Renomme selon OUTPUT_NAME (fourni par le panneau Export).
if [ -n "${OUTPUT_NAME:-}" ] && [ "${OUTPUT_NAME}" != "$APP_NAME" ]; then
    mkdir -p target/export
    cp "$IPA" "target/export/${OUTPUT_NAME}.ipa"
    IPA="target/export/${OUTPUT_NAME}.ipa"
fi
echo "✅ IPA : $IPA ($(du -h "$IPA" | cut -f1))"
[ -z "${PROFILE:-}" ] && echo "⚠️ Sans PROFILE (.mobileprovision), l'app ne s'installe pas sur un appareil."
