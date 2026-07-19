# Démarrage rapide — 5 minutes (hors compilation)

Objectif : voir une scène jouable dans l'éditeur, sans aucune décision à
prendre. Chaque commande est à copier telle quelle.

## 1. Prérequis

Rust via [rustup](https://rustup.rs). Si tu ne l'as pas :

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

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

⏱️ **La première compilation prend ~5 minutes** (mesuré sur Apple M4 —
compter plus sur une machine plus ancienne). Les lancements suivants
recompilent en **quelques secondes**. C'est le comportement normal de Rust,
pas un problème d'installation.

Au démarrage, la console affiche :

```text
RusteeGear 0.1.0
GPU : <ta carte> (<Metal|Vulkan>)
```

et l'éditeur s'ouvre sur la scène de démonstration (le hameau du jeu).

## 4. Ouvrir le projet exemple

1. Menu **📂 Ouvrir…**
2. Sélectionner `examples/first_game/scene.json` (dans le dossier cloné)
3. Cliquer **Play**

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

## Piloter l'application de l'extérieur (agent, script, audit)

L'application peut être télécommandée par TCP local (opt-in, jamais actif par
défaut — l'éval Lua est de l'exécution de code arbitraire) :

```bash
cargo run --profile dev-fast -- --pilot     # éditeur + pont sur 127.0.0.1:4517
```

Puis, dans un autre terminal, avec le client `pilot` :

```bash
cargo run --profile dev-fast --bin pilot -- state           # playing ? combien d'objets ?
cargo run --profile dev-fast --bin pilot -- console play    # démarre le mode Play
cargo run --profile dev-fast --bin pilot -- lua "return 1+1"
cargo run --profile dev-fast --bin pilot -- scene           # nom/position/visibilité des objets
cargo run --profile dev-fast --bin pilot -- input 0.0 1.0 jump   # avancer + sauter
cargo run --profile dev-fast --bin pilot -- screenshot /tmp/s.png 800 600
cargo run --profile dev-fast --bin pilot -- logs
cargo run --profile dev-fast --bin pilot -- console stop
```

Verbes console : `timescale`, `pause`, `play`, `stop`, `step`, `tp`, `select`,
`spawn`, `health`, `net_stats`. Protocole, architecture et limites :
[docs/PILOT.md](docs/PILOT.md). `--pilot=PORT` ou `RUSTEEGEAR_PILOT=PORT`
changent le port.

## Dépannage express

| Symptôme | Cause probable |
| --- | --- |
| `error: package … requires rustc 1.x` | `rustup update` |
| compilation très longue | normal la 1re fois (cf. §3) |
| logs verbeux souhaités | `RUST_LOG=debug cargo run --profile dev-fast` |
| jouer sans réseau (mode player) | `RUSTEEGEAR_OFFLINE=1 cargo run --profile dev-fast -- --player` |
