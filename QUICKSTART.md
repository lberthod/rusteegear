# Démarrage rapide — ~30 minutes la première fois

Objectif : voir une scène jouable dans l'éditeur, sans aucune décision à
prendre. Chaque commande est à copier telle quelle.

Budget réaliste : **5 minutes de manipulation**, plus ce que les outils font
tout seuls — installation de rustup et Git LFS, clone (~240 Mo),
téléchargement de la toolchain épinglée 1.98.0 (~200 Mo) et **première
compilation : 5-10 minutes** (~5 sur Apple M4, plus sur une machine plus
ancienne). Les lancements suivants prennent **moins de 30 secondes**. Sans
toolchain Rust, le `.dmg` des [releases GitHub](https://github.com/lberthod/rusteegear/releases)
saute les étapes 1 à 3 (cf. [docs/TEST_SCENARIO.md](docs/TEST_SCENARIO.md),
Test A).

## 1. Prérequis

Rust via [rustup](https://rustup.rs). Si tu ne l'as pas :

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

[Git LFS](https://git-lfs.com) — `assets/models/` (482 modèles `.glb`, plus
344 aperçus PNG) est suivi par Git LFS depuis le 2026-09-03 :

```bash
brew install git-lfs   # ou apt/dnf/pacman selon la distribution
git lfs install
```

Sans ça, `git clone` récupère des pointeurs texte à la place des vrais
fichiers `.glb` — `doctor.sh` (étape suivante) le détecte.

## 2. Cloner et vérifier l'environnement

```bash
git clone https://github.com/lberthod/rusteegear
cd rusteegear
rustup update
./scripts/doctor.sh
```

`doctor.sh` doit afficher « Environnement prêt ». Sinon, il indique la
commande de réparation pour chaque ✗.

## 3. Lancer l'éditeur

```bash
cargo run --profile dev-fast
```

⏱️ **La première compilation prend 5-10 minutes** (~5 mesurées sur Apple M4 —
compter plus sur une machine plus ancienne). Les lancements suivants
recompilent en **moins de 30 secondes**. C'est le comportement normal de
Rust, pas un problème d'installation.

Au démarrage, la console affiche :

```text
RusteeGear 0.1.0
GPU : <ta carte> (<Metal|Vulkan>)
```

et l'éditeur s'ouvre sur la scène de démonstration (le hameau du jeu) au
premier lancement ; ensuite, sur le dernier projet ouvert (réglable dans
Paramètres ▸ 📁 Démarrage).

## 4. Ouvrir le projet exemple

1. Menu **Fichier ▸ 🎬 Démos ▸ ⭐ Commencer ▸ ⭐ Premier jeu** (le projet
   est embarqué dans l'application — **📂 Ouvrir un projet…** → dossier
   `examples/first_game` du clone revient au même)
2. Cliquer **Play**

## 5. Jouer

- **Flèches / WASD** : déplacer le joueur (la capsule orange)
- **Espace** : sauter
- Marcher sur la **zone jaune** : elle devient verte
- Ramasser les **3 pièces dorées** : l'objectif de la scène

**Stop** ramène la scène exactement à l'état d'avant Play.

## Et ensuite ?

- **Créer quelque chose toi-même (10 min)** : [docs/FIRST_GAME.md](docs/FIRST_GAME.md)
- **Comprendre le moteur (1 page)** : [docs/MENTAL_MODEL.md](docs/MENTAL_MODEL.md)
- Le contenu du projet exemple : [examples/first_game/README.md](examples/first_game/README.md)

## Aller plus loin

- **Tous les contrôles** (clavier, souris, jeu, tactile, manette) :
  [docs/CONTROLS.md](docs/CONTROLS.md) — aussi dans **Aide › ⌨ Raccourcis clavier**.
- **Piloter l'application depuis un script ou un agent** (`--pilot`) :
  [docs/PILOT.md](docs/PILOT.md).
- **Contribuer** (clippy, tests, ce que vérifie la CI avant de pousser) :
  [CONTRIBUTING.md](CONTRIBUTING.md).

## Dépannage express

| Symptôme | Cause probable |
| --- | --- |
| `error: package … requires rustc 1.x` | `rustup update` |
| compilation très longue | normal la 1re fois (cf. §3) |
| logs verbeux souhaités | `RUST_LOG=debug cargo run --profile dev-fast` |
| mode joueur (écran d'accueil : pseudo, en ligne / seul) | `cargo run --profile dev-fast -- --player` |
| jouer sans réseau, sans écran d'accueil | `RUSTEEGEAR_OFFLINE=1 cargo run --profile dev-fast -- --player` |
