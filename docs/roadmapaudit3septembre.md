# Roadmap post-audit (2026-09-03)

*Suite au [Rapport d'audit RusteeGear](https://claude.ai/code/artifact/8008ffea-9d5f-4e76-a0d1-f9e867e7eae2)
du 2026-09-03 : relecture du dépôt à l'état du commit `dc42cb2` (main, copie de travail propre,
un commit local non poussé), toutes les affirmations vérifiées par exécution (`cargo`,
`gh run view` job par job, `git lfs`). Même format que [roadmapaudit29aout.md](roadmapaudit29aout.md) :
vagues priorisées par valeur débloquée / coût, coûts en taille de t-shirt (S ≈ ≤ 1 h, M ≈ ½ journée,
L ≈ 1-2 jours). Chaque item donne la commande ou le patch exact, puis le critère de « fait ».*

## Constat d'ensemble

Le code est sain : en local, `cargo fmt --check`, 743 tests (8 ignorés volontairement), le
garde-fou unwrap (14 whitelistés) et le garde-fou du bundle (322 clés, 0 orphelin) sont verts ;
un seul `unsafe`, un seul TODO, aucun secret versionné, migration Git LFS correcte.

Mais **la CI GitHub échoue sur chaque push depuis le 20 juillet** — 45 runs rouges consécutifs,
dernier run vert `0a62dde` (2026-07-20 09:16 UTC) — et personne ne l'a vu : pas de badge dans le
README, et la roadmap du 29 août classe « CI verte » parmi les acquis alors qu'au commit audité
(`163e081`) quatre jobs sur six échouaient. Conséquence directe : les 69 tests réseau à sockets
réels (anti-usurpation Firebase, plafonds de connexions, rate-limiting), cités comme preuve de la
sécurité serveur, **ne compilent plus** et n'ont pas tourné en CI depuis six semaines. Les 13 lots
du regroupement d'`AppState` (« 684/684 verts ») ont été validés sans eux.

Rien n'est cassé côté produit. Ce qui est cassé, c'est la boucle qui permettrait de le savoir.

## Règle de cette roadmap

**Vague 0 d'abord, tout le reste ensuite.** Aucun refactor de la vague 3 ne démarre tant qu'un
run CI complet (6 jobs) n'est pas vert sur `main`. Sinon on continue de « vérifier » des
changements avec une suite partielle.

---

## Vague 0 — Remettre la CI au vert aujourd'hui (S, ~1 h)

### 0.1 Clippy : lint `chunks_exact_to_as_chunks` (5 sites)

La toolchain stable actuelle (rustc 1.98.0) a introduit ce lint, refusé par `-D warnings`.
Remplacer `chunks_exact(N)` par `as_chunks::<N>()` : la méthode renvoie un tuple
`(&[[T; N]], &[T])` dont le premier élément est la suite de blocs complets — même sémantique que
`chunks_exact`, mais typée, ce qui supprime aussi les indexations `c[0]`/`c[1]`.

| Site | Avant | Après |
|---|---|---|
| `src/app/scripting.rs:407` | `for chunk in flat.chunks_exact(9) {` | `for chunk in flat.as_chunks::<9>().0 {` |
| `src/gfx/renderer/post_process.rs:198-199` | `data.chunks_exact(8).map(\|c\| u64::from_le_bytes(c.try_into().unwrap()))` | `data.as_chunks::<8>().0.iter().map(\|c\| u64::from_le_bytes(*c))` |
| `src/gfx/renderer/headless.rs:312` | `for px in pixels.chunks_exact_mut(4) {` | `for px in pixels.as_chunks_mut::<4>().0 {` |
| `src/runtime/physics/build.rs:152-154` | `.chunks_exact(3).map(\|c\| [c[0], c[1], c[2]]).collect()` | `.as_chunks::<3>().0.to_vec()` |
| `src/scene/import.rs:113` | `for tri in indices.chunks_exact(3) {` | `for tri in indices.as_chunks::<3>().0 {` |

Points d'attention :

- `post_process.rs` : le `try_into().unwrap()` disparaît — supprimer aussi le commentaire des
  lignes 196-197 qui le justifiait, **et** retirer l'entrée
  `("src/gfx/renderer/post_process.rs", "unwrap"): 1` de `WHITELIST` dans
  `scripts/check_unwrap_budget.py` (le script ne signale pas les entrées devenues inutiles, il
  faut le faire à la main). Le total attendu passe de 14 à 13.
- `physics/build.rs` : `tris` est déjà typé `Vec<[u32; 3]>` et `data.indices` est `Vec<u32>`,
  `to_vec()` suffit.
- `scripting.rs` et `import.rs` : `chunk[0]`/`tri[0]` continuent de fonctionner sur `[T; N]`,
  le corps des boucles ne change pas.

Vérification :

```bash
cargo clippy --all-targets -- -D warnings && python3 scripts/check_unwrap_budget.py
```

### 0.2 Tests réseau : 4 accès périmés dans `src/app/network_client_tests.rs`

Les champs ont été regroupés dans des sous-structs d'`AppState` aux lots 3 (`FirebaseAuthState`,
champ `firebase`) et 5 (`NetworkPlayersState`, champ `network`) de la roadmap 2.3, mais le code
gaté `#[cfg(feature = "net_tests")]` n'a pas suivi — rien ne le compile par défaut. Les champs
des sous-structs sont privés au module `app`, et `network_client_tests.rs` est un sous-module de
`app`, donc l'accès direct reste autorisé.

| Ligne | Avant | Après |
|---|---|---|
| 312 | `app.firebase_tx` | `app.firebase.firebase_tx` |
| 636 | `server_app.network_players` | `server_app.network.network_players` |
| 785 | `server_app.network_health.insert(id, 0.0);` | `server_app.network.network_health.insert(id, 0.0);` |
| 786 | `server_app.network_players.get(&id)` | `server_app.network.network_players.get(&id)` |

Vérification (compilation puis exécution réelle, la feature ouvre des sockets loopback) :

```bash
cargo test --features net_tests --no-run
```

```bash
cargo test --features net_tests
```

Critère : `cargo test --features net_tests` termine avec `0 failed` sur toutes les cibles
(lib + `tests/pilot_bridge.rs`). Si la première exécution révèle des tests réseau cassés
*fonctionnellement* (pas seulement en compilation) depuis juillet, les corriger fait partie de
cette vague : ce sont les tests de sécurité du serveur.

### 0.2 bis — trouvé en exécutant `--all-features` : `src/bin/server.rs` ne compilait pas non plus

Après correctif 0.2, `cargo clippy --all-targets --all-features` échouait encore avec
**14 erreurs E0061** dans le module `#[cfg(all(test, feature = "net_tests"))] mod tests` de
`src/bin/server.rs` : `handle_message` a grossi de 5 à 7 paramètres (`firebase:
&Option<(FirebaseConfig, AuthSession)>`, `verified_uids: &Sender<(PlayerId, String)>`) le jour où
la vérification serveur des `idToken` Firebase a été ajoutée, mais les 14 appels du module de
tests — gaté par la même feature que `network_client_tests.rs`, invisible pour les mêmes raisons
— n'ont pas suivi. C'est très probablement la cause racine du **tout premier** run CI rouge
(`9398fd7`, 2026-07-20, déjà « this function takes 7 arguments but 5 arguments were supplied »
selon les logs).

Corrigé avec une enveloppe de test plutôt que 14 corrections identiques (aucun test du module
n'envoie de `firebase_uid` vérifiable, tous passent `firebase_uid: None`) :

```rust
/// Enveloppe `handle_message` pour ce module : `firebase: None` et un canal
/// jetable suffisent puisqu'aucun test n'envoie de `firebase_uid` vérifiable.
fn test_handle_message(
    rooms: &mut HashMap<String, Room>,
    player_room: &mut HashMap<PlayerId, String>,
    net: &NetServer,
    id: PlayerId,
    msg: ClientMsg,
) {
    let (verified_tx, _verified_rx) = std::sync::mpsc::channel();
    handle_message(rooms, player_room, net, id, msg, &None, &verified_tx);
}
```

puis renommage des 14 appels `handle_message(` → `test_handle_message(` dans le module (l'appel
de production à `src/bin/server.rs:788`, hors du module de tests, avait déjà les 7 arguments et
n'a pas bougé).

**Conséquence pour 1.3** : cette découverte confirme qu'il faut bien `--all-features` (pas
seulement `--all-targets`) dans le job Clippy de la CI — `--all-targets` seul n'aurait pas
recompilé ce module.

### 0.3 Restreindre `lfs: true` aux deux jobs qui en ont besoin

Le commit local `dc42cb2` (non poussé) ajoute `lfs: true` aux **cinq** `actions/checkout@v4` de
`.github/workflows/ci.yml`. Chaque job télécharge alors les 159 Mo d'`assets/models/`, soit
~0,8 Go par push ; à 13 pushes le 3 septembre seul, un quota LFS mensuel s'épuise en quelques
jours et l'échec frappe tous les jobs en même temps (checkout refusé).

| Job | `lfs: true` | Pourquoi |
|---|---|---|
| `check` | **garder** | les tests `src/scene/import.rs:1500-1580` lisent `assets/models/creature*.glb` via `CARGO_MANIFEST_DIR` ; sur un pointeur texte, `load_gltf` échoue |
| `net-tests` | **garder** | `cargo test --features net_tests` exécute toute la suite, dont ces tests d'import |
| `golden` | **retirer** | `tests/golden_render.rs` / `golden_skinning.rs` ne référencent aucun fichier d'`assets/models/` |
| `cross-build` (×3) | **retirer** | `cargo build --lib` ; `build.rs` n'embarque qu'`assets/bundle` (13 Mo, Git normal) |

Amender `dc42cb2` avant de pousser (il n'est pas encore sur `origin`) :

```bash
git commit --amend --no-edit
```

Vérifier le quota : GitHub → *Settings* → *Billing and plans* → *Git LFS Data* (l'API REST
exige le scope `user`, `gh api` ne le montre pas avec le token actuel).

### 0.4 Pousser et attendre un run entièrement vert

```bash
git push origin main
```

```bash
gh run watch
```

Critère de fin de vague 0 : `gh run list --workflow CI --limit 1` affiche `success`, et
`gh run view <id>` liste les 6 jobs en `✓`. Noter le SHA dans le tableau de suivi en bas de ce
document.

---

## Vague 1 — Empêcher la récidive (S à M, cette semaine)

### 1.1 Badge CI dans le README

Après la ligne 9 de `README.md` (badge Rust), ajouter :

```markdown
![CI](https://github.com/lberthod/rusteegear/actions/workflows/ci.yml/badge.svg?branch=main)
```

C'est le mécanisme le moins cher pour rendre visible la prochaine dérive en une seconde.

### 1.2 Épingler la toolchain

Créer `rust-toolchain.toml` à la racine :

```toml
# Épinglé (roadmap post-audit 2026-09-03, 1.2) : la CI utilisait `stable` flottant et
# le passage à 1.98 a cassé Clippy sans changement de code. Monter de version = un
# commit dédié qui change cette ligne et corrige les nouveaux lints dans la foulée.
[toolchain]
channel = "1.98.0"
components = ["clippy", "rustfmt"]
```

Côté CI, l'action `dtolnay/rust-toolchain` **ne lit pas** `rust-toolchain.toml` (vérifié dans
son `action.yml` : la version vient du `@rev` ou de l'entrée `toolchain`). Son README demande
d'utiliser `@master` dès qu'on passe une version explicite. Remplacer donc les quatre
occurrences de `dtolnay/rust-toolchain@stable` dans `.github/workflows/ci.yml` par :

```yaml
      - name: Installer la toolchain Rust (même version que rust-toolchain.toml)
        uses: dtolnay/rust-toolchain@master
        with:
          toolchain: 1.98.0
```

en conservant les `components:` / `targets:` existants de chaque job. La version figure alors à
deux endroits (`rust-toolchain.toml` pour le poste, `ci.yml` pour les runners) ; c'est voulu, et
si les deux divergent, rustup applique de toute façon le fichier au moment d'invoquer `cargo`
(coût : un second téléchargement, pas une erreur). Mettre le badge du README à jour : `Rust-1.98`.

Rituel de montée de version (à noter dans `docs/architecture.md`, section outillage) :
`rustup update` → modifier `channel` → `cargo clippy --all-targets --all-features -- -D warnings`
→ corriger → un commit « Toolchain 1.xx ».

### 1.3 Compiler la feature `net_tests` dans le chemin critique

Le job `check` ne compile que les cibles par défaut ; un test gaté par feature peut donc pourrir
sans bruit. Dans `ci.yml`, job `check`, étape *Clippy* :

```yaml
      - name: Clippy (toutes features : compile aussi les tests `net_tests` sans les exécuter)
        run: cargo clippy --all-targets --all-features -- -D warnings
```

`--all-features` active aussi `player_build` — vérifier une fois en local que ça compile ; si
`player_build` change le comportement au démarrage seulement, aucun impact sur Clippy.

Même commande côté poste : ajouter à `scripts/doctor.sh` une section « avant de pousser » qui
affiche la commande (pas l'exécuter dans `doctor.sh`, qui doit rester rapide), et créer un hook
optionnel documenté dans `QUICKSTART.md` :

```bash
printf '#!/bin/sh\nexec cargo clippy --all-targets --all-features -- -D warnings\n' > .git/hooks/pre-push && chmod +x .git/hooks/pre-push
```

### 1.4 Cache des objets LFS en CI

Pour ne télécharger `assets/models/` que quand un modèle change, dans les jobs `check` et
`net-tests`, remplacer `with: lfs: true` par un checkout sans LFS suivi de :

```yaml
      - name: Liste des objets LFS (clé de cache)
        run: git lfs ls-files -l | cut -d' ' -f1 | sort > .lfs-assets-id

      - name: Cache LFS
        uses: actions/cache@v4
        with:
          path: .git/lfs
          key: lfs-${{ hashFiles('.lfs-assets-id') }}

      - name: Récupérer les modèles
        run: git lfs pull
```

Le pull ne télécharge que les objets absents de `.git/lfs` : coût nul tant que les modèles ne
changent pas. Critère : sur deux runs consécutifs sans changement d'asset, l'étape
« Récupérer les modèles » dure moins de 5 s au second.

### 1.5 Audit des dépendances en CI

Nouveau job dans `ci.yml`, non bloquant la première semaine (`continue-on-error: true`, à retirer
comme on l'a fait pour `golden` après 15 runs verts) :

```yaml
  audit:
    name: Avis de sécurité (cargo audit)
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable   # l'outil n'a pas besoin de la version épinglée
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-audit --locked
      - run: cargo audit
```

En local, une fois : `cargo install cargo-audit --locked && cargo audit`. Tout avis ouvert
(`RUSTSEC-…`) est traité dans 2.1.

---

## Vague 2 — Cohérence doc / code et hygiène (S, la semaine suivante)

### 2.1 `cargo update` en commit isolé

`Cargo.lock` date du 18 juillet, `Cargo.toml` du 29 août ; 157 paquets ont une mise à jour patch
ou mineure disponible (`glam`, `kira`, `cpal`, `cc`, `libc`, `wasm-bindgen`…). Une fois la CI
verte (vague 0) :

```bash
cargo update && cargo test --all-targets && cargo test --features net_tests
```

Un seul commit « Dépendances : cargo update (2026-09) », sans autre changement, pour que la CI
départage une régression éventuelle. `rilua` est épinglé `=0.1.21` volontairement (Cargo.toml),
il ne bougera pas. Si `cargo audit` (1.5) a signalé un avis, le noter dans le message de commit.

### 2.2 Corrections de documentation

| Fichier | Ligne | Erreur | Correction |
|---|---|---|---|
| `QUICKSTART.md` | 14 | « 826 modèles `.glb` » | « 482 modèles `.glb` (et 344 aperçus PNG) » — 826 est le total des fichiers d'`assets/models/` |
| `QUICKSTART.md` | 15 | « depuis le 2026-08-30 » | « depuis le 2026-09-03 » (commit `47a6ae2`) |
| `README.md` | 9 | badge `Rust-1.95` | `Rust-1.98` (cf. 1.2), plus le badge CI (1.1) |
| `docs/roadmapaudit29aout.md` | 26 | 1.1 encore `⏳` | `✅ 47a6ae2` + `b11a215` (LFS, doctor.sh, QUICKSTART) ; l'historique n'a pas été réécrit (choix explicite, pack Git toujours à 144 Mo + 98 Mo d'objets LFS) |
| `docs/roadmapaudit29aout.md` | section « déjà solide » | « CI verte » | remplacer par : « CI rouge du 20/07 au 03/09 (cf. roadmap du 3 septembre), tests réseau non compilés sur cette période » |
| `docs/roadmapaudit29aout.md` | 3.4 | « **Reste ouvert** : pourquoi le job CI cross-build (wasm32) ne l'a pas détecté » | fermer : le job **a** échoué à `163e081` puis `2e73226`, mais la CI était déjà rouge depuis `9398fd7` (tests réseau), donc un échec de plus n'a alerté personne |

### 2.3 Mot de passe du keystore hors de `Cargo.toml`

`Cargo.toml:90` contient `keystore_password = "android"` pour la clé **release** Android. Le
fichier `packaging/release.keystore` n'est pas versionné, mais toute copie de ce fichier suffit à
signer un APK au nom du projet. `cargo-apk` lit la table `[package.metadata.android.signing.release]`
sans substitution de variable : la solution est de ne plus déclarer cette table et de signer
après coup dans `packaging/build_apk.sh` avec `apksigner`, mot de passe lu depuis
`RUSTEEGEAR_KEYSTORE_PASS` (`${VAR:?message}`, même patron que `RUSTEEGEAR_VPS_SSH` dans
`deploy_vps.sh`). Générer un nouveau keystore avec un vrai mot de passe à cette occasion ; l'ancien
n'a signé que des builds de test.

### 2.4 Outils déguisés en tests — revu (2026-09-03), scope réduit

Plan initial (ci-dessous, biffé) revu en tentant de l'exécuter : deux de ses trois volets se
sont révélés avoir un coût ou un risque plus élevé que prévu à l'écriture du plan.

~~Huit tests `#[ignore = "outil : …"]` (`src/scene/mod_tests.rs` ×5, `src/editor/export.rs` ×1)
régénèrent `assets/player_scene.json` et le bundle ; deux tests de `src/net/client/native.rs`
dépendent du VPS. Fonctionnel, mais illisible pour un nouveau venu (« pourquoi 8 ignorés ? »).
Déplacer les régénérateurs vers `examples/regen_player_scene.rs` (lancé par
`cargo run --example regen_player_scene`), documenter la commande dans `docs/architecture.md`
(section assets), et gater les deux tests VPS par la feature `net_tests` plutôt que par `#[ignore]`.
Critère : `cargo test` affiche `0 ignored`.~~

**Tests VPS (`src/net/client/native.rs`, `tls_proof::wss_vps`/`ws_308_hint`) : ne pas gater sous
`net_tests`.** `net_tests` tourne automatiquement à chaque push dans le job `net-tests` — ces deux
tests-là frappent le vrai VPS de production (`ws.loicberthod.ch`) sur le vrai réseau, pas un socket
loopback comme le reste de la feature. Les y ajouter aurait couplé le statut vert/rouge de la CI à
la disponibilité du VPS et créé une dépendance réseau externe non isolée — exactement ce que
`net_tests` existe pour éviter (cf. `docs/architecture.md`, section réseau : « `cargo test` reste
rapide et indépendant d'un environnement CI qui restreint parfois le bind loopback »). Leur
`#[ignore]` actuel, avec le commentaire qui explique pourquoi et donne la commande manuelle
(`cargo test --lib tls_proof -- --ignored --nocapture`), est le bon choix : à garder tel quel.

**Régénérateurs (`src/scene/mod_tests.rs` ×5, `src/editor/export.rs` ×1) : migration vers
`examples/` plus grosse que prévu, à faire à part.** Mesuré : ces fonctions font 36 à 146 lignes
(736 lignes cumulées), et réécrivent réellement `assets/player_scene.json` et `assets/bundle/` —
les vrais fichiers de contenu du jeu, pas des fixtures de test. Les déplacer *et* vérifier qu'elles
fonctionnent encore correctement demanderait de les exécuter contre ces fichiers réels (mutation),
ce qui n'est pas une vérification à faire à la légère en accompagnement d'un ménage de lisibilité.
Reporté à une session dédiée, avec sauvegarde des fichiers cibles avant de lancer quoi que ce soit.
Leur étiquette actuelle (`#[ignore = "outil : …, à lancer explicitement"]`) reste correcte en
attendant — un nouveau venu qui lit le message comprend déjà que ce sont des outils, pas des tests
cassés.

---

## Vague 3 — Dette structurelle (L, par lots, après la vague 0 uniquement)

L'état mesuré le 3 septembre : `AppState` 94 champs (175 fin août), `build_ui` 338 lignes
(1 382), `Physics` en modules — le travail 2.x de la roadmap du 29 août est réel. Mais 47
fonctions dépassent 150 lignes :

| Fonction | Fichier | Lignes | Traitement |
|---|---|---|---|
| `mmorpg_demo` | `src/scene/demos/mmorpg/mod.rs:21` | 1 381 | contenu déclaratif de scène, **ne pas toucher** |
| `Renderer::render` | `src/gfx/renderer/frame.rs:4` | 1 125 | 3.2 |
| `Pipelines::build` | `src/gfx/pipelines.rs:426` | 941 | construction GPU linéaire, basse priorité |
| `sim_step` | `src/app/simulation.rs:824` | 878 | **3.1** |
| `inspector_panel` | `src/editor/mod.rs:1867` | 647 | déjà extrait de `build_ui`, suffisant |
| `advance_play` | `src/app/simulation.rs:344` | 439 | 3.1 |
| `run_script` | `src/app/scripting.rs:29` | 404 | 3.1 (dernier lot) |

Couverture par module (tests `#[test]` pour lignes de code) : `app` 375/27 212, `net` 71/3 835,
`runtime` 57/3 772, `scene` 129/17 924, `gfx` 25/6 662, `editor` 25/10 005.

### 3.1 Découper `sim_step` en phases nommées

Même méthode que les six lots de `build_ui` (un lot = une extraction + build + clippy + suite
complète **avec `--features net_tests`** + commit) : `sim_step` devient un orchestrateur qui
enchaîne `run_object_scripts`, `apply_tap_actions`, `apply_spawns_and_destroys`, `step_physics`,
`reconcile_network`, chacune avec au moins un test unitaire d'invariant dans
`simulation_tests.rs`. Critère : `sim_step` < 150 lignes, aucun changement de comportement
observable (golden `play_mode_audit.rs` vert, tests réseau verts).

### 3.2 `Renderer::render` par passes

Uniquement avec les goldens Metal comme garde-fou (job `golden`) : extraire une fonction par
passe (ombres, opaque, skinning, post-process, egui) dans les fichiers déjà créés au Sprint 9
(`shadows.rs`, `post_process.rs`…). Critère : goldens inchangés au pixel près sur le runner
macOS.

### 3.3 Terminer le regroupement d'`AppState`

Les 94 champs restants : lister ceux qui vont par trois ou plus (`grep -nE "^\s+[a-z_]+:" src/app/mod.rs`
entre `pub struct AppState` et son accolade fermante), regrouper par domaine comme les 13 lots
précédents. Ceux qui restent isolés le restent — l'objectif n'est pas zéro champ.

### 3.4 Tests sur `editor/` et `gfx/`

Pas de harnais egui : appliquer la méthode de 2.4 (roadmap d'août) — extraire les décisions
enfouies dans les closures en fonctions pures testées. Cibles : `readiness::analyze` (259 lignes,
logique pure), `export::ui` (306 lignes, dont la validation des chemins de bundle).

---

## Ce qu'il ne faut pas faire

- **Réécrire l'historique Git** pour purger les anciens blobs `.glb` du pack (144 Mo) : décision
  du 29 août, à conserver. Le coût est payé une fois au clone ; LFS empêche qu'il augmente.
- **Démarrer la vague 3 avec une CI rouge**, ou la mener sans `--features net_tests` dans la
  boucle de vérification de chaque lot.
- **Réagir aux sept crates en double version** (`base64`, `bitflags`, `getrandom`, `hashbrown`,
  `objc2` et satellites, `rustc-hash`, `webpki-roots`) : dépendances transitives, courant dans
  l'écosystème wgpu/winit ; `cargo update` (2.1) en absorbera une partie, le reste n'est pas un
  problème.
- **Ajouter `#[allow(clippy::chunks_exact_to_as_chunks)]`** globalement pour aller plus vite en
  0.1 : cinq sites, dix minutes, et `as_chunks` supprime un `unwrap` au passage.

## Ce qui est déjà solide (revérifié par exécution le 2026-09-03)

- `cargo fmt --all --check` propre ; `cargo test --all-targets` 743 verts.
- Garde-fous : 14 unwrap/expect/panic whitelistés, bundle sans orphelin (322 clés).
- Migration LFS : 482 pointeurs valides à HEAD, `doctor.sh` détecte les pointeurs bruts.
- Cross-build Android / iOS / wasm32 verts en CI depuis `5cec7a1`.
- Hygiène : 1 `unsafe`, 1 TODO, 44 `#[allow]` (37 `too_many_arguments`), aucun secret ni
  keystore versionné, `deploy_vps.sh` sans adresse en dur.

## Suivi

| # | Action | Coût | Statut |
|---|---|---|---|
| 0.1 | 5 sites Clippy `as_chunks` + whitelist unwrap | S | ✅ `e4914e7` |
| 0.2 | 4 accès `network_client_tests.rs` + run réel `net_tests` | S | ✅ `31c41fe` — inclut 0.2 bis (`src/bin/server.rs`, non prévu à l'origine, trouvé en vérifiant `--all-features`) ; `cargo test --features net_tests` : 718 verts, 0 échec |
| 0.3 | `lfs: true` limité à `check` et `net-tests` (amender `dc42cb2`) | S | ✅ `f734bbc` (amendement de `dc42cb2`, toujours non poussé) |
| 0.4 | Push + run CI 6/6 vert (noter le SHA) | S | ✅ Poussé (`f734bbc`→`d759158`) ; [run 33765270175](https://github.com/lberthod/rusteegear/actions/runs/33765270175) 6/6 vert — premier run entièrement vert depuis `0a62dde` (2026-07-20), soit 46 jours |
| 1.1 | Badge CI README | S | ✅ `8090a6b` |
| 1.2 | `rust-toolchain.toml` 1.98.0 + `@master` dans ci.yml + badge | S | ✅ `8090a6b` |
| 1.3 | `--all-features` dans Clippy CI + hook pre-push documenté | S | ✅ `787af6b` |
| 1.4 | Cache `.git/lfs` par `actions/cache` | M | ✅ `81d1032` — premier run à froid confirmé correct (« Cache not found », `git lfs pull` a récupéré les 482 objets, cache sauvegardé en fin de job) ; les runs suivants réutiliseront le cache |
| 1.5 | Job `cargo audit` (non bloquant d'abord) | S | ✅ `5b7b755` — vérifié en local avant l'ajout (0 vulnérabilité, 2 avis « unmaintained » informationnels). [Run 33786061629](https://github.com/lberthod/rusteegear/actions/runs/33786061629) 7/7 vert |
| 2.1 | `cargo update` en commit isolé | S | ✅ `e60adde` — a révélé une régression `glam` (déprécations `Mat4::*_rh`), corrigée dans le même commit ; goldens de rendu inchangés au pixel près. Deuxième régression trouvée seulement en CI (`cpal` 0.17→0.18 tire `libdbus-1-dev` sur Linux, absent des runners macOS où la vérification locale avait tourné) : corrigée dans `15c1fe9`, [run 33768777711](https://github.com/lberthod/rusteegear/actions/runs/33768777711) 6/6 vert |
| 2.2 | 6 corrections de doc (QUICKSTART, README, roadmap d'août) | S | ✅ `787af6b` (QUICKSTART) + `bb74d86` (`docs/roadmapaudit29aout.md` : intro, Outillage, 1.1, 3.4) |
| 2.3 | Mot de passe keystore hors `Cargo.toml`, nouveau keystore | M | ✅ `b5a932b` — `RUSTEEGEAR_KEYSTORE_PASS` obligatoire ; keystore local existant (généré en juin, mdp "android") à régénérer à la prochaine build APK |
| 2.4 | Outils hors de `cargo test` (0 ignoré) | M | 🔶 Revu en tentant de l'exécuter : gater les tests VPS sous `net_tests` les ferait tourner contre la prod à chaque push (annulé, `#[ignore]` gardé) ; migrer les 6 régénérateurs (736 lignes, mutent de vrais fichiers d'assets) est plus gros que prévu, reporté à une session dédiée. Détail : section 2.4 ci-dessus |
| 3.1 | `sim_step` en phases testées | L | ⏳ Plan détaillé (lots précis, ordre, risques) écrit après lecture intégrale des deux fonctions : [docs/plan_decoupage_sim_step_et_render.md](plan_decoupage_sim_step_et_render.md) |
| 3.2 | `Renderer::render` par passes (goldens) | L | ⏳ Même plan que 3.1, ci-dessus |
| 3.3 | Fin du regroupement `AppState` | M | 🔶 Lot 14/N (`NetConnectionState`, 15 champs, 163 sites) `e5b2443`, [run 33781176725](https://github.com/lberthod/rusteegear/actions/runs/33781176725) 6/6 vert. Lot 15/N (`PlayerAttackState`, 4 champs, 24 sites) `5131951`. iOS vérifié explicitement en local à chaque lot (`cargo check --target aarch64-apple-ios`), pas seulement en confiance sur le job CI. 94 → 76 champs. Reste surtout un noyau d'orchestration (`scene`, `playing`, `selection`…) sans cluster naturel évident — pas d'objectif « zéro champ » |
| 3.4 | Tests `readiness::analyze`, `export::ui` | M | ✅ `3ad1738` (12 tests, `readiness::analyze` 3 → 15) + `d4078a2` (3 décisions extraites d'`export::ui`, 7 tests, même méthode que `menus.rs` en août). [Run 33784484510](https://github.com/lberthod/rusteegear/actions/runs/33784484510) 6/6 vert |
