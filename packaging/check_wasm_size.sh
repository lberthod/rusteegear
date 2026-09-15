#!/usr/bin/env bash
# Garde-fou de taille pour le .wasm produit par build_web.sh (roadmap.md,
# § Priorisation perf, quick win 1) : un build concurrent (ex. un autre
# `cargo build` en cours ailleurs) peut écraser le .wasm en cours d'écriture
# par wasm-bindgen/wasm-opt et produire un fichier tronqué qui charge quand
# même dans le navigateur, avec la scène dégradée en cubes faute d'assets —
# déjà vécu deux fois (3-4 septembre 2026).
#
# Usage : check_wasm_size.sh <fichier.wasm> [seuil_min_en_Mo]
# Sortie : 0 si le fichier existe et fait au moins le seuil (30 Mo par
# défaut) ; 1 sinon, avec un message expliquant la cause probable.
#
# Extrait de build_web.sh pour être testable isolément — cf.
# packaging/test_check_wasm_size.sh.
set -euo pipefail

WASM_FILE="${1:?usage: check_wasm_size.sh <fichier.wasm> [seuil_min_en_Mo]}"
# 30 Mo est calibré sur la taille actuelle du bundle (assets + moteur). Si une
# optimisation future fait légitimement baisser cette taille sous le seuil
# (compression de texture GPU, lazy-loading — pistes mentionnées dans
# roadmap.md), ce script se mettra à rejeter des builds sains : réévaluer/
# abaisser MIN_MB en conséquence plutôt que de désactiver le garde-fou.
MIN_MB="${2:-30}"
MIN_WASM_BYTES=$((MIN_MB * 1024 * 1024))

if [ ! -f "$WASM_FILE" ]; then
    echo "✗ .wasm introuvable : $WASM_FILE" >&2
    exit 1
fi

wasm_size=$(wc -c < "$WASM_FILE" | tr -d ' ')
if [ "$wasm_size" -lt "$MIN_WASM_BYTES" ]; then
    echo "✗ .wasm anormalement petit : ${wasm_size} octets (< ${MIN_MB} Mo attendus)." >&2
    echo "  Build probablement tronqué (ex. cargo build concurrent qui a écrasé le .wasm en cours d'écriture — cf. roadmap.md)." >&2
    echo "  Abandon avant packaging/déploiement." >&2
    exit 1
fi
echo "✓ Taille .wasm : $((wasm_size / 1024 / 1024)) Mo (seuil : ${MIN_MB} Mo)."
