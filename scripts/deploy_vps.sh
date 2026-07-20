#!/usr/bin/env bash
# Déploiement du serveur de jeu sur le VPS — version scriptée de la procédure
# jusqu'ici manuelle (docs/reflexion.md §11 ; audit 2026-07-20, risque R2 :
# incident réel d'un VPS resté 3 versions de PROTOCOL_VERSION en retard,
# aucun joueur ne pouvait se connecter).
#
# Chaîne : push GitHub → pull + build release SUR le VPS → restart systemd →
# double smoke test (façade TLS wss:// = chemin des vrais joueurs, puis route
# claire directe). Chaque étape échoue bruyamment (set -e) — pas de rollback
# automatique, mais un état intermédiaire est toujours visible et rejouable.
#
# Usage :
#   scripts/deploy_vps.sh            # déploie HEAD de main
#   RUSTEEGEAR_VPS_SSH="ubuntu@1.2.3.4" RUSTEEGEAR_VPS_KEY=~/.ssh/autre_cle \
#     scripts/deploy_vps.sh          # surcharge hôte/clé
#
# ⚠️ Bump de PROTOCOL_VERSION : ce script redéploie le SERVEUR ; les clients
# (DMG/APK/IPA/web) doivent être redistribués ensemble, sinon « version
# incompatible » pour tout le monde (cf. docs/reflexion.md).
set -euo pipefail

VPS_SSH="${RUSTEEGEAR_VPS_SSH:-ubuntu@179.237.71.235}"
VPS_KEY="${RUSTEEGEAR_VPS_KEY:-$HOME/.ssh/loicberthodvps}"
VPS_DIR="${RUSTEEGEAR_VPS_DIR:-rusteegear-server}"
WSS_URL="${RUSTEEGEAR_WSS_URL:-wss://ws.loicberthod.ch}"
WS_CLEAR_URL="${RUSTEEGEAR_WS_CLEAR_URL:-}" # ex. ws://179.237.71.235:80 ; vide = sauté

cd "$(dirname "$0")/.."

echo "── 1/5 Vérifications locales ──────────────────────────────────────────"
if [ -n "$(git status --porcelain)" ]; then
    echo "ERREUR : arbre de travail non propre — committez ou remisez d'abord." >&2
    exit 1
fi
if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then
    echo "ERREUR : déployez depuis main (branche courante : $(git rev-parse --abbrev-ref HEAD))." >&2
    exit 1
fi

echo "── 2/5 Push GitHub (le VPS tire depuis origin/main) ───────────────────"
git push origin main

echo "── 3/5 Pull + build release + restart sur le VPS ──────────────────────"
# --ff-only : le VPS ne doit jamais diverger de main ; un échec ici signale
# une divergence à résoudre à la main plutôt qu'à écraser en silence.
ssh -i "$VPS_KEY" "$VPS_SSH" "set -euo pipefail
    cd $VPS_DIR
    git pull --ff-only
    source ~/.cargo/env
    cargo build --release --bin server
    sudo systemctl restart rusteegear-server
    sleep 2
    sudo systemctl is-active rusteegear-server"

echo "── 4/5 Smoke test façade TLS ($WSS_URL) — le chemin des vrais joueurs ─"
cargo run --release --example smoke_vps "$WSS_URL"

if [ -n "$WS_CLEAR_URL" ]; then
    echo "── 5/5 Smoke test route claire ($WS_CLEAR_URL) ────────────────────────"
    cargo run --release --example smoke_vps "$WS_CLEAR_URL"
else
    echo "── 5/5 Route claire sautée (RUSTEEGEAR_WS_CLEAR_URL non renseignée) ───"
fi

echo "✅ Déploiement terminé : $(git rev-parse --short HEAD) servi par $VPS_SSH"
