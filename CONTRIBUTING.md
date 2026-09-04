# Contribuer à RusteeGear

Ce fichier regroupe ce qui concerne quelqu'un qui modifie le code — pas
quelqu'un qui découvre le moteur (pour ça : [QUICKSTART.md](QUICKSTART.md)).

## Avant de pousser

La CI compile aussi le code gaté par la feature `net_tests` (`cargo clippy
--all-targets --all-features`) sans l'exécuter — un oubli de ce côté est
invisible localement avec un simple `cargo check`. Pour l'attraper avant de
pousser :

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

En hook automatique :

```bash
printf '#!/bin/sh\nexec cargo clippy --all-targets --all-features -- -D warnings\n' > .git/hooks/pre-push
chmod +x .git/hooks/pre-push
```

La CI GitHub (`.github/workflows/ci.yml`) et la publication de la démo web
(`pages.yml`) utilisent la toolchain épinglée par `rust-toolchain.toml` ;
monter de version = un commit dédié qui change ce fichier et les deux
workflows.

## Démo web

```bash
./packaging/build_web.sh        # wasm32 + wasm-bindgen + wasm-opt (features explicites)
```

`pages.yml` refuse de publier un `.wasm` que V8 n'accepte pas sans flag
expérimental (`WebAssembly.validate`) — c'est ce qui a rendu la démo noire du
3 au 4 septembre 2026. Ne pas remettre `wasm-opt --all-features`.

## Raccourcis et documentation

Tout raccourci vit dans la table `src/app/shortcuts.rs`, qui alimente la
fenêtre Aide et que `docs/CONTROLS.md` doit citer (test
`docs_controls_lists_every_shortcut`). Les limitations connues se déclarent
dans `docs/KNOWN_LIMITATIONS.md` ; les roadmaps d'audit dans `docs/roadmapaudit*.md`.

## Piloter l'application dans un test

Le pont TCP `--pilot` (cf. [docs/PILOT.md](docs/PILOT.md)) permet de charger
une scène, lancer Play, injecter des entrées et capturer le rendu depuis un
script — c'est le moyen le plus fiable de vérifier une régression visuelle
sans cliquer.
