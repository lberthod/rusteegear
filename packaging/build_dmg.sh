#!/usr/bin/env bash
# Construit l'application .app et le .dmg distribuable (macOS).
# Pré-requis : cargo install cargo-bundle
set -euo pipefail

cd "$(dirname "$0")/.."

# OUTPUT_NAME : nom du fichier de sortie. PLAYER_BUILD=1 : bâtit un player jouable
# (mode Player + scène embarquée) au lieu de l'éditeur. Tous deux fournis par le panneau Export.
OUTPUT_NAME="${OUTPUT_NAME:-RusteeGear}"
FEATURES=""
if [ "${PLAYER_BUILD:-0}" = "1" ]; then
    FEATURES="--features player_build"
    echo "▶ Build PLAYER « $OUTPUT_NAME » (mode joueur, scène embarquée)…"
else
    echo "▶ Compilation + bundle éditeur (release)…"
fi
cargo bundle --release --bin motor3derust $FEATURES

if [ "$OUTPUT_NAME" != "RusteeGear" ]; then
    # Export : applique l'identité (id/nom/version) au .app puis (re)crée le .dmg.
    APP="target/release/bundle/osx/RusteeGear.app"
    PLIST="$APP/Contents/Info.plist"
    PB=/usr/libexec/PlistBuddy
    "$PB" -c "Set :CFBundleIdentifier ${BUNDLE_ID:-com.berthod.motor3derust}" "$PLIST" 2>/dev/null || true
    "$PB" -c "Set :CFBundleName $OUTPUT_NAME" "$PLIST" 2>/dev/null || true
    "$PB" -c "Set :CFBundleDisplayName $OUTPUT_NAME" "$PLIST" 2>/dev/null || true
    [ -n "${APP_VERSION:-}" ] && "$PB" -c "Set :CFBundleShortVersionString $APP_VERSION" "$PLIST" 2>/dev/null || true
    [ -n "${BUILD_NUMBER:-}" ] && "$PB" -c "Set :CFBundleVersion $BUILD_NUMBER" "$PLIST" 2>/dev/null || true

    mkdir -p target/export
    DMG="target/export/${OUTPUT_NAME}.dmg"
    rm -f "$DMG"
    hdiutil create -volname "$OUTPUT_NAME" -srcfolder "$APP" -ov -format UDZO "$DMG" >/dev/null
else
    DMG="target/release/bundle/dmg/RusteeGear.dmg"
fi
echo "✅ DMG créé : $DMG"
echo "   Taille : $(du -h "$DMG" | cut -f1)"
echo
echo "Note : le .dmg n'est pas signé. Au premier lancement, faire"
echo "clic droit ▸ Ouvrir pour contourner Gatekeeper."
