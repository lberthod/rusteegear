# Architecture et dette technique (2026-07-20)

*Photographie au commit `429a764` — les chiffres périment au prochain gros refactor
(« Sprint 9 bis »). Le piège de mesure du worktree ci-dessous est décrit ici une fois
pour toutes ; les autres fichiers y renvoient.*

## Volumétrie

111 fichiers `.rs` sous `src/`, **73 761 lignes**.

⚠️ Piège de mesure : `.claude/worktrees/compassionate-einstein-575c8b/` contient une copie
complète et **divergente** de `src/` (versions anciennes) et de `assets/`. Tout
`grep`/`wc` récursif lancé depuis la racine est faussé (+418 `#[test]` fantômes). À
exclure de tout script de métriques.

## Poids par module

| Module | Fichiers | Lignes | Responsabilité |
|---|---|---|---|
| `src/app` | 29 | 26 012 | Logique de jeu, `AppState` et sous-systèmes |
| `src/scene` | 34 | 17 618 | Modèle de scène (`Vec<SceneObject>`, pas d'ECS), import GLB, démos |
| `src/editor` | 7 | 9 600 | UI egui (fenêtres, HUD, hiérarchie, export) |
| `src/gfx` | 16 | 6 658 | Rendu wgpu (renderer découpé en 8 sous-fichiers) |
| `src/runtime` | 6 | 3 700 | Physique, audio, savegame, rng |
| `src/net` | 8 | 3 618 | Protocole bincode, serveur tokio, firebase, interpolation |
| `src/bin` | 3 | 3 245 | `server.rs`, `glbviewer.rs`, `pilot.rs` |
| racine | 8 | 3 310 | `lib.rs` (App+run), `assets.rs`, `project.rs`… |

**Couplage** : la règle « `AppState` sans dépendance GPU » est tenue. Les seules
dépendances app→gfx sont des types de données (`OrbitCamera`, `MeshData`), pas de
handles wgpu. Le sens Renderer→AppState est respecté.

## Bilan du découpage Sprint 9

✅ **Réel et vérifiable** : `gfx/renderer.rs` (ex-monolithe ~2 571 l.) n'existe plus,
remplacé par `src/gfx/renderer/` (frame 1 125, types 396, headless 387, shadows 363,
sync 352, resources 307, post_process 221). `network_client_types.rs` et `minimap.rs`
extraits.

❌ **Monolithes restants** (non ciblés par le Sprint 9) :

| Lignes | Fichier | Problème |
|---|---|---|
| 2 888 | `src/editor/mod.rs` | God-module UI, 25 fonctions — plus gros fichier non-test |
| 2 359 | `src/runtime/physics.rs` | Monolithe physique |
| 1 562 | `src/app/mod.rs` | `AppState` à **119 champs** — les `impl` sont extraits, l'*état* reste monolithique |
| 1 911 | `src/scene/demos/mmorpg/decor_data.rs` | Données `const` en dur — dette faible mais candidat à l'externalisation en asset |

## Dette mesurée

- **TODO/FIXME/HACK dans le code non-test : 1 seule occurrence.** Base remarquablement propre.
- **Garde-fou unwrap/expect/panic : vert.** `scripts/check_unwrap_budget.py` (appelé par
  la CI) : « OK : 14 unwrap/expect/panic en code de production, tous whitelistés », chaque
  site justifié par un commentaire. Les ~630 unwrap/expect bruts sont à ~97 % en test.
  - *Fragilité* : c'est un parseur Python maison (comptage d'accolades), pas clippy. Une
    migration vers `clippy::unwrap_used` scopé serait plus robuste.
- **`allow` clippy** : 32 × `too_many_arguments` (dette d'API masquée — signatures à
  regrouper en structs de contexte), 3 × `type_complexity`, 1 × `dead_code` (audio.rs).
- **Duplication** : rien de flagrant côté logique.

## Docs vs code

- `docs/MENTAL_MODEL.md` : **à jour** (modèle données/rôles/Play-Stop conforme).
- `docs/architecture.md` : majoritairement fidèle, mais **2 chemins morts post-Sprint 9** :
  - ligne 40 : « `src/gfx/renderer.rs::Renderer` » → c'est désormais `src/gfx/renderer/`
  - ligne 75 : « `src/net/client.rs::NetClient` » → c'est désormais `src/net/client/`
  Correction triviale, à faire (ironie : ce sont les deux modules refactorés par Sprint 9).

## Risques priorisés

1. **`AppState` 119 champs** — cœur de couplage du gameplay, frein principal à la
   testabilité. Piste : regrouper par sous-système (état réseau, état combat, état
   caméra…) en structs dédiées, comme l'ont été les `impl`. *Priorité haute.*
2. **`editor/mod.rs` et `physics.rs` non découpés** — candidats naturels d'un « Sprint 9
   bis ». *Priorité haute.*
3. **Garde-fou unwrap = parseur maison** — migrer vers clippy lints scopés. *Moyenne.*
4. **2 chemins morts dans `architecture.md`** — correction en 5 minutes. *Moyenne.*
5. **32 `too_many_arguments`** — structs de contexte. *Moyenne.*
6. **Worktree `.claude/worktrees/` divergent dans l'arbre** — fausse les métriques,
   risque de build/commit sur la mauvaise copie (contient aussi un `player_scene.json`
   divergent). À nettoyer ou documenter. *Moyenne.*
7. **`decor_data.rs` en `const`** — externaliser en asset pour éditer sans recompiler. *Basse.*
