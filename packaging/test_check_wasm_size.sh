#!/usr/bin/env bash
# Test de non-régression pour check_wasm_size.sh (garde-fou de taille .wasm,
# roadmap.md § Priorisation perf, quick win 1). Pas de framework de test
# shell dans ce dépôt : petit harnais autonome, à lancer avec
# `packaging/test_check_wasm_size.sh`. Sortie 0 si tout passe, 1 sinon.
set -uo pipefail

cd "$(dirname "$0")/.."
CHECK="packaging/check_wasm_size.sh"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

fail=0

assert_fails() {
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        echo "✗ FAIL: $desc (attendu : échec, obtenu : succès)"
        fail=1
    else
        echo "✓ ok: $desc"
    fi
}

assert_succeeds() {
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        echo "✓ ok: $desc"
    else
        echo "✗ FAIL: $desc (attendu : succès, obtenu : échec)"
        fail=1
    fi
}

# Fichier factice sous le seuil (1 Mo, seuil par défaut 30 Mo) : doit être
# rejeté — reproduit l'incident du .wasm tronqué (~15 Mo observés en prod).
small="$TMP_DIR/truncated.wasm"
head -c $((1 * 1024 * 1024)) /dev/zero > "$small"
assert_fails "rejette un .wasm factice de 1 Mo (< 30 Mo)" "$CHECK" "$small"

# Fichier factice tout juste sous le seuil réduit (5 Mo, seuil 6 Mo) : doit
# être rejeté aussi, pour vérifier que le seuil personnalisable est bien pris
# en compte (pas seulement le défaut de 30 Mo).
just_under="$TMP_DIR/just_under.wasm"
head -c $((5 * 1024 * 1024)) /dev/zero > "$just_under"
assert_fails "rejette un fichier de 5 Mo avec seuil personnalisé 6 Mo" "$CHECK" "$just_under" 6

# Fichier factice au-dessus d'un seuil réduit (7 Mo, seuil 6 Mo) : doit
# passer — vérifie qu'un fichier de taille normale n'est pas rejeté à tort.
above="$TMP_DIR/above.wasm"
head -c $((7 * 1024 * 1024)) /dev/zero > "$above"
assert_succeeds "accepte un fichier de 7 Mo avec seuil personnalisé 6 Mo" "$CHECK" "$above" 6

# Fichier absent : doit échouer avec un message clair, pas planter le script
# appelant sans explication.
assert_fails "rejette un fichier .wasm absent" "$CHECK" "$TMP_DIR/does_not_exist.wasm"

if [ "$fail" = "0" ]; then
    echo "✓ tous les tests de check_wasm_size.sh passent"
else
    echo "✗ des tests de check_wasm_size.sh ont échoué"
fi
exit "$fail"
