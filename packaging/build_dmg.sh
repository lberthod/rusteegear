#!/usr/bin/env bash
# Construit l'application .app et le .dmg distribuable (macOS).
# Pré-requis : cargo install cargo-bundle
set -euo pipefail

cd "$(dirname "$0")/.."

echo "▶ Compilation + bundle (release)…"
cargo bundle --release

DMG="target/release/bundle/dmg/Motor3DeRust.dmg"
echo "✅ DMG créé : $DMG"
echo "   Taille : $(du -h "$DMG" | cut -f1)"
echo
echo "Note : le .dmg n'est pas signé. Au premier lancement, faire"
echo "clic droit ▸ Ouvrir pour contourner Gatekeeper."
