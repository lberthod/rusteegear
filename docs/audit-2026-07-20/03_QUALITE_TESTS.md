# Qualité, tests et CI (2026-07-20)

*Photographie au commit `429a764` — les décomptes périment à chaque ajout de tests ;
exclure le worktree divergent des mesures (voir [01](01_ARCHITECTURE_DETTE.md)).*

## État instantané

- `cargo fmt --all --check` : **PASS**
- `cargo clippy --all-targets -- -D warnings` : **PASS** (zéro warning)
- Garde-fou `scripts/check_unwrap_budget.py` : **PASS** (14 sites whitelistés, justifiés)

## Inventaire des tests

**764 `#[test]`** (hors copie du worktree `.claude/worktrees/…` qui en duplique 418 —
à exclure de toute métrique).

Réconciliation des décomptes (le total 764 couvre `src/` + `tests/` + `examples/`) :
- 657 tests unitaires dans les 6 modules du tableau de couverture ci-dessous
- ~60 tests dans le reste : racine `src/` (`lib.rs`, `assets.rs`…), `src/bin/`, `examples/`
- 47 tests d'intégration sur 11 fichiers `tests/` (goldens rendu/skinning, serveur local,
  pilot bridge, exemples de scènes, manifest projet…)
- 0 `#[tokio::test]` : le réseau est testé via sockets réels synchrones sous feature `net_tests`
- **9 `#[ignore]`** : 2 dépendant du VPS public, 1 flaky avéré (roguelike), 6 « outils »
  qui réécrivent `assets/player_scene.json` / `assets/bundle/` (dont le test de resynchro
  de scène — voir le risque d'écrasement dans [05](05_ASSETS_PIPELINE.md))
- **Goldens** : infrastructure sérieuse (`tests/golden_render.rs`, `golden_skinning.rs`,
  `assert_matches_golden` avec tolérance, références binaires dans `tests/golden/`)

## CI (`.github/workflows/`)

| Job | Contenu |
|---|---|
| `check` (Ubuntu) | fmt + clippy `-D warnings` + `cargo test --all-targets` + garde-fou unwrap |
| `net-tests` (Ubuntu) | `cargo test --features net_tests` (sockets TCP loopback réels) |
| `golden` (macOS/Metal) | goldens rendu + skinning ; `continue-on-error` retiré après 15 runs verts |
| `cross-build` | build-only `aarch64-linux-android`, `aarch64-apple-ios`, `wasm32` |

> Note de réconciliation : l'audit réseau signalait « `net_tests` jamais activée en CI »
> en citant `docs/AUDIT_JEU_2026-07-17.md` — c'était vrai au 17/07, le job `net-tests`
> existe désormais dans `ci.yml`. Restent hors CI : les 2 tests `#[ignore]` dépendant du
> VPS réel, et `smoke_vps` (manuel, post-déploiement).

## Couverture par module (densité)

| Module | `#[test]` | Lignes | 1 test / N lignes |
|---|---|---|---|
| net | 68 | 3 618 | 53 (meilleur) |
| runtime | 56 | 3 700 | 66 |
| app | 363 | 26 012 | 72 |
| scene | 121 | 17 618 | 146 |
| gfx | 25 | 6 658 | **266** (compensé partiellement par les goldens GPU) |
| editor | 24 | 9 600 | **400** (le plus gros trou) |

## Flaky

- **`roguelike_demo_clears_rooms_one_at_a_time_to_victory`** (`src/app/demos.rs:342`) :
  ~60-80 % d'échec sur HEAD au 18/07, marqué `#[ignore]`, suivi dans
  `docs/KNOWN_LIMITATIONS.md`. Cause : budget de frames insuffisant par intermittence
  (sensible au tirage d'arme et aux trajectoires du missile homing). Correctif recommandé :
  **rendre déterministe** (vitesse missile / distances figées), pas élargir le budget.
- Tests « wave » : actifs et stables (l'équivalent déterministe du scénario roguelike).
- Goldens : instabilité potentielle **matérielle** (divergence GPU entre machines),
  gérée par tolérance — pas un flake logiciel.

## Recommandations priorisées

1. **Rendre déterministe puis réactiver le test roguelike** — seul vrai flaky, il masque
   un scénario bout-en-bout important. Un flaky ignoré = régression non détectée. *Haute.*
2. **Combler le trou `editor`** (1/400) — cibler la logique testable sans UI :
   export/sérialisation, commandes undo-redo, manipulation de scène. *Haute.*
3. **Donner un signal réseau au `cargo test` local** — un dev local n'a aucune couverture
   réseau sans `--features net_tests` ; soit un sous-ensemble loopback sans feature, soit
   le documenter dans README/CONTRIBUTING. *Moyenne.*
4. **Verrouiller wasm** — le job `cross-build` est build-only ; ajouter au moins un test
   headless (wasm-bindgen-test) sur une fonction pure critique. *Moyenne.*
5. **Étendre les goldens gfx** (ombres, post-process, texcompress) plutôt que des tests
   unitaires fragiles sur du code GPU. *Basse.*
