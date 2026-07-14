#!/usr/bin/env bash
# Construit la cible wasm32 et génère les bindings JS (Sprint 114, défrichage).
# Pré-requis : rustup target add wasm32-unknown-unknown ; cargo install wasm-bindgen-cli
# (version EXACTE de la crate `wasm-bindgen` du lockfile — cf. Cargo.lock).
#
# État connu à ce sprint : compile et s'exécute dans Chrome sans panic (moteur,
# éditeur exclus du binaire web restant compilés mais inertes), mais le premier
# rendu affiche un canvas noir plutôt que la scène — bug non résolu (cf.
# ROADMAP_SPRINTS.md Sprint 114). Utile pour continuer le diagnostic, pas encore
# pour une démo publique.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "▶ cargo build --lib --target wasm32-unknown-unknown (release)…"
cargo build --lib --release --target wasm32-unknown-unknown

echo "▶ wasm-bindgen…"
mkdir -p packaging/web/pkg
wasm-bindgen --target web \
    --out-dir packaging/web/pkg \
    --out-name motor3derust \
    target/wasm32-unknown-unknown/release/motor3derust.wasm

echo "✓ packaging/web/pkg/ prêt — servir packaging/web/ avec un serveur HTTP statique."
