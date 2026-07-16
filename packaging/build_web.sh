#!/usr/bin/env bash
# Construit la cible wasm32 et génère les bindings JS (Phase Q, Sprints 114-117).
# Pré-requis : rustup target add wasm32-unknown-unknown ; cargo install wasm-bindgen-cli
# (version EXACTE de la crate `wasm-bindgen` du lockfile — cf. Cargo.lock) ;
# wasm-opt (paquet binaryen, `brew install binaryen`) pour la passe de réduction
# de taille post wasm-bindgen — sans lui le .wasm final est ~20-35 % plus gros.
#
# État connu : rendu (114), audio SFX (115) et multijoueur (116, WebSocket natif
# du navigateur) fonctionnels dans Chrome via WebGPU — jouable au clavier,
# connexion automatique au serveur par défaut comme sur desktop/APK. Scripts Lua
# (backend `rilua`, Sprint 137) et meshes à animation squelettale fonctionnels
# depuis le Sprint 137. Limitation connue restante : musique en flux absente —
# détail dans ROADMAP_SPRINTS.md, Sprints 114-116.
#
# PLAYER_BUILD=1 (panneau Export, même contrat que build_dmg.sh) : bâtit un player
# jouable (feature `player_build`, scène + assets embarqués — déjà écrits dans
# assets/player_scene.json + assets/bundle/ par le panneau avant l'appel) et
# produit une archive prête à servir : target/export/${OUTPUT_NAME}-web.zip
# (index.html + pkg/). Sans PLAYER_BUILD : comportement historique, moteur
# générique dans packaging/web/pkg/, pas d'archive.
set -euo pipefail

cd "$(dirname "$0")/.."

OUTPUT_NAME="${OUTPUT_NAME:-RusteeGear}"
FEATURES=""
if [ "${PLAYER_BUILD:-0}" = "1" ]; then
    FEATURES="--features player_build"
    echo "▶ Build PLAYER web « $OUTPUT_NAME » (mode joueur, scène embarquée)…"
fi

echo "▶ cargo build --lib --target wasm32-unknown-unknown (profil wasm-release)…"
cargo build --lib --profile wasm-release --target wasm32-unknown-unknown $FEATURES

echo "▶ wasm-bindgen…"
mkdir -p packaging/web/pkg
wasm-bindgen --target web \
    --out-dir packaging/web/pkg \
    --out-name motor3derust \
    target/wasm32-unknown-unknown/wasm-release/motor3derust.wasm

if command -v wasm-opt >/dev/null 2>&1; then
    echo "▶ wasm-opt -Oz…"
    # --all-features (plutôt qu'un --enable-* choisi à la main) : le binaire
    # émis par rustc utilise déjà bulk-memory/nontrapping-float-to-int/etc.,
    # et un jeu de features incomplet fait planter wasm-opt à la validation.
    # --all-features fait correspondre exactement les capacités activées par
    # rustc pour wasm32-unknown-unknown, sans devoir les lister à la main à
    # chaque montée de version de rustc/wasm-bindgen.
    wasm-opt -Oz --all-features \
        -o packaging/web/pkg/motor3derust_bg.wasm \
        packaging/web/pkg/motor3derust_bg.wasm
else
    echo "⚠ wasm-opt introuvable (brew install binaryen) — .wasm non compacté, ~20-35 % plus gros que nécessaire."
fi

if [ "${PLAYER_BUILD:-0}" = "1" ]; then
    STAGE="target/export/web_stage/${OUTPUT_NAME}"
    ZIP="target/export/${OUTPUT_NAME}-web.zip"
    rm -rf "$STAGE" "$ZIP"
    mkdir -p "$STAGE"
    cp packaging/web/index.html "$STAGE/"
    cp -R packaging/web/pkg "$STAGE/pkg"
    (cd target/export/web_stage && zip -qr "../${OUTPUT_NAME}-web.zip" "${OUTPUT_NAME}")
    rm -rf target/export/web_stage
    echo "✅ Archive web créée : $ZIP"
    echo "   Taille : $(du -h "$ZIP" | cut -f1)"
    echo "   Servir le dossier décompressé avec un serveur HTTP statique"
    echo "   (ex. python3 -m http.server) et ouvrir dans Chrome (WebGPU requis)."
else
    echo "✓ packaging/web/pkg/ prêt — servir packaging/web/ avec un serveur HTTP statique."
fi
