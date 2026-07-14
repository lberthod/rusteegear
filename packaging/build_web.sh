#!/usr/bin/env bash
# Construit la cible wasm32 et génère les bindings JS (Phase Q, Sprints 114-117).
# Pré-requis : rustup target add wasm32-unknown-unknown ; cargo install wasm-bindgen-cli
# (version EXACTE de la crate `wasm-bindgen` du lockfile — cf. Cargo.lock).
#
# État connu : rendu (114), audio SFX (115) et multijoueur (116, WebSocket natif
# du navigateur) fonctionnels dans Chrome via WebGPU — jouable au clavier,
# connexion automatique au serveur par défaut comme sur desktop/APK. Limitations
# connues : scripting Lua inerte, musique en flux absente, meshes à animation
# squelettale non affichés (limite de bind groups WebGPU) — détail dans
# ROADMAP_SPRINTS.md, Sprints 114-116.
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
