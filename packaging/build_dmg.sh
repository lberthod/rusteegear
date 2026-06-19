#!/usr/bin/env bash
# Construit l'application .app et le .dmg distribuable (macOS).
# Pré-requis : cargo install cargo-bundle
set -euo pipefail

cd "$(dirname "$0")/.."

# OUTPUT_NAME : nom du fichier de sortie. PLAYER_BUILD=1 : bâtit un player jouable
# (mode Player + scène embarquée) au lieu de l'éditeur. Tous deux fournis par le panneau Export.
OUTPUT_NAME="${OUTPUT_NAME:-Motor3DeRust}"
FEATURES=""
if [ "${PLAYER_BUILD:-0}" = "1" ]; then
    FEATURES="--features player_build"
    echo "▶ Build PLAYER « $OUTPUT_NAME » (mode joueur, scène embarquée)…"
else
    echo "▶ Compilation + bundle éditeur (release)…"
fi
cargo bundle --release $FEATURES

SRC="target/release/bundle/dmg/Motor3DeRust.dmg"
if [ "$OUTPUT_NAME" != "Motor3DeRust" ]; then
    mkdir -p target/export
    DMG="target/export/${OUTPUT_NAME}.dmg"
    cp "$SRC" "$DMG"
else
    DMG="$SRC"
fi
echo "✅ DMG créé : $DMG"
echo "   Taille : $(du -h "$DMG" | cut -f1)"
echo
echo "Note : le .dmg n'est pas signé. Au premier lancement, faire"
echo "clic droit ▸ Ouvrir pour contourner Gatekeeper."
