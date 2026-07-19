#!/bin/sh
# doctor.sh — diagnostic d'environnement avant de lancer RusteeGear.
#
# Vérifie l'ENVIRONNEMENT uniquement (toolchain, droits, port) : le contenu
# d'une scène/d'un build est déjà couvert par le « Readiness Check » intégré à
# l'éditeur (src/editor/readiness.rs) — pas de doublon ici.
#
# Usage : ./scripts/doctor.sh
# Sortie : une ligne ✓/✗ par vérification ; code de retour 0 si tout est vert.

set -u

OK=0
FAIL=0

pass() { printf '  \033[32m✓\033[0m %s\n' "$1"; OK=$((OK + 1)); }
fail() {
    printf '  \033[31m✗\033[0m %s\n' "$1"
    printf '      → %s\n' "$2"
    FAIL=$((FAIL + 1))
}

echo "RusteeGear doctor"
echo

# --- Toolchain Rust -----------------------------------------------------------
if command -v rustup >/dev/null 2>&1; then
    pass "rustup présent"
else
    fail "rustup introuvable" \
        "installer via https://rustup.rs : curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi

if command -v cargo >/dev/null 2>&1; then
    pass "cargo présent"
else
    fail "cargo introuvable" \
        "rustup installe cargo ; sinon vérifier que ~/.cargo/bin est dans le PATH"
fi

# Édition Rust 2024 (Cargo.toml) → rustc >= 1.85 requis ; le projet est
# développé avec une toolchain récente, « rustup update » est la réparation.
MIN_MAJOR=1
MIN_MINOR=85
if command -v rustc >/dev/null 2>&1; then
    RUSTC_VERSION=$(rustc --version | awk '{print $2}')
    RUSTC_MAJOR=$(echo "$RUSTC_VERSION" | cut -d. -f1)
    RUSTC_MINOR=$(echo "$RUSTC_VERSION" | cut -d. -f2)
    if [ "$RUSTC_MAJOR" -gt "$MIN_MAJOR" ] || {
        [ "$RUSTC_MAJOR" -eq "$MIN_MAJOR" ] && [ "$RUSTC_MINOR" -ge "$MIN_MINOR" ]
    }; then
        pass "rustc $RUSTC_VERSION (>= $MIN_MAJOR.$MIN_MINOR requis, édition 2024)"
    else
        fail "rustc $RUSTC_VERSION trop ancien (>= $MIN_MAJOR.$MIN_MINOR requis, édition 2024)" \
            "rustup update"
    fi
else
    fail "rustc introuvable" "rustup update (ou installer rustup, cf. ci-dessus)"
fi

# Cible native installée (rustup géré) — sur une installation rustup standard
# la cible hôte est toujours là ; on vérifie quand même, un toolchain minimal
# « rustup toolchain install --profile minimal » sans composant peut surprendre.
if command -v rustup >/dev/null 2>&1; then
    HOST_TARGET=$(rustup show 2>/dev/null | awk '/Default host/ {print $3}')
    if [ -n "${HOST_TARGET:-}" ]; then
        pass "cible native ($HOST_TARGET)"
    else
        fail "cible native indéterminée" "rustup show ; puis rustup update"
    fi
fi

# --- Dépôt --------------------------------------------------------------------
REPO_DIR=$(cd "$(dirname "$0")/.." && pwd)
if [ -f "$REPO_DIR/Cargo.toml" ]; then
    pass "dépôt RusteeGear ($REPO_DIR)"
else
    fail "Cargo.toml introuvable à côté de scripts/" \
        "lancer ce script depuis un clone du dépôt : ./scripts/doctor.sh"
fi

if [ -d "$REPO_DIR/assets/bundle" ]; then
    pass "assets embarqués présents (assets/bundle/)"
else
    fail "assets/bundle/ manquant (le build échouera : include_dir! à la compilation)" \
        "clone incomplet ? re-cloner le dépôt, sans filtre sur assets/"
fi

# --- Dossier utilisateur ------------------------------------------------------
# ~/.motor3derust/ est créé au premier lancement ; ce qui compte ici est que
# $HOME soit défini et accessible en écriture.
if [ -n "${HOME:-}" ] && [ -w "$HOME" ]; then
    pass "dossier utilisateur écrivable ($HOME → ~/.motor3derust/ au premier lancement)"
else
    fail "\$HOME absent ou non écrivable" \
        "vérifier l'utilisateur courant ; l'éditeur fonctionne sans, mais sans assets de projet ni sauvegardes"
fi

# --- Port du serveur local ----------------------------------------------------
# Le serveur multijoueur local écoute sur 127.0.0.1:7777 (src/bin/server.rs).
# Jouer/éditer n'en a pas besoin ; on prévient seulement si le port est pris.
if command -v nc >/dev/null 2>&1; then
    if nc -z 127.0.0.1 7777 >/dev/null 2>&1; then
        fail "port 7777 déjà occupé (serveur local multijoueur)" \
            "optionnel — utile seulement pour « cargo run --bin server » ; libérer le port ou changer d'adresse"
    else
        pass "port 7777 libre (serveur local multijoueur, optionnel)"
    fi
else
    pass "port 7777 non vérifié (nc absent) — optionnel, sans impact sur l'éditeur"
fi

echo
if [ "$FAIL" -eq 0 ]; then
    echo "Environnement prêt ($OK vérifications)."
    echo "Lancer : cargo run --profile dev-fast"
    exit 0
else
    echo "$FAIL problème(s) à corriger ci-dessus ($OK vérifications passées)."
    exit 1
fi
