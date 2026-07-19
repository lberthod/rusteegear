# RusteeGear — Sprint 9 : découper les fichiers monolithes (sous-phases détaillées)

> Développe le brief Sprint 9 (« ramener chaque module à une seule raison de changer, par
> extraction mécanique, sans réécriture ni changement de comportement ») en sous-phases par
> fichier et par sous-fichier, avec lignes réelles relevées le 2026-07-19.
> Convention identique à [sprint10audit.md](sprint10audit.md) / [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md) :
> un sprint ≈ 1 à 3 jours, **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**,
> **un commit par extraction**, on ne démarre une sous-phase que si la précédente est verte.

---

## État des lieux mesuré (2026-07-19)

| Fichier | Lignes (brief) | Lignes (mesurées) | Delta |
|---|---|---|---|
| `src/scene/demos.rs` | 10 820 | 10 820 | = |
| `src/app/mod.rs` | 4 393 | 4 415 | +22 |
| `src/scene/mod.rs` | 4 016 | 4 016 | = |
| `src/gfx/renderer.rs` | 3 432 | 3 526 | +94 |
| `src/app/network_client.rs` | 3 228 | 3 228 | = |
| `src/app/simulation.rs` | 3 050 | 3 050 | = |

Léger delta sur `app/mod.rs` et `renderer.rs` (travail récent), rien qui change le diagnostic.

## Diagnostic clé : la moitié du problème, c'est `mod tests`

> Section historique (état **avant** 9.0) — conservée pour expliquer le raisonnement qui a mené
> à ajouter la phase 9.0. Voir le tableau « Sous-phase … Fichier source après » en 9.0 pour
> l'état réel après exécution.

Avant de toucher à la logique, un relevé structurel (`grep` des `impl`/`fn`/`mod tests` de
premier niveau) montre que **4 des 6 fichiers sont majoritairement des tests inline** :

| Fichier | Ligne où commence `mod tests` | Lignes de tests | % du fichier |
|---|---|---|---|
| `app/mod.rs` | 1653 | ~2 762 | **63 %** |
| `scene/mod.rs` | 1432 | ~2 584 | **64 %** |
| `app/network_client.rs` | 1560 | ~1 668 | **52 %** |
| `app/simulation.rs` | 1499 | ~1 551 | **51 %** |
| `gfx/renderer.rs` | 3129 | ~397 | 11 % |
| `scene/demos.rs` | 9867 | ~953 | 9 % |

**Conséquence pour l'ordonnancement du sprint** : sortir `mod tests` dans un fichier frère
(`#[path = "..."] mod tests;`) est l'extraction la **plus mécanique et la moins risquée
possible** (aucune ligne de logique déplacée, juste le bloc de tests tel quel), et elle suffit
à elle seule à ramener **4 fichiers sur 6 sous la barre des ~1 500 lignes visée** :

| Fichier | Lignes hors tests | Sous ~1 500 après ce seul déplacement ? |
|---|---|---|
| `scene/mod.rs` | ~1 431 | ✅ oui, directement |
| `app/simulation.rs` | ~1 498 | ✅ oui, directement |
| `app/network_client.rs` | ~1 559 | ≈ oui (léger dépassement) |
| `app/mod.rs` | ~1 652 | ≈ oui (léger dépassement) |
| `gfx/renderer.rs` | ~3 129 | ❌ non, nécessite le découpage 9.2 |
| `scene/demos.rs` | ~9 867 | ❌ non, nécessite le découpage 9.1 |

→ On ajoute donc une **phase 9.0** (nouvelle, en tête) qui traite ce quick win sur les 6
fichiers avant d'attaquer le travail plus fin sur `demos.rs` et `renderer.rs`, seuls fichiers
qui exigent un vrai découpage de logique.

---

## Ordre de traitement recommandé

```
9.0  Extraction de mod tests (6 fichiers, 6 commits indépendants) ─ ✅ FAIT (2026-07-19)
  │
  ├─► 9.1  scene/demos.rs   (9 868 → dossier scene/demos/, 26 fichiers) ─ ✅ FAIT (2026-07-19)
  ├─► 9.2  gfx/renderer.rs  (3 159 → dossier gfx/renderer/, 9 fichiers) ─ ✅ FAIT (2026-07-19)
  ├─► 9.3  app/mod.rs       (1 654 → trim optionnel vers ~1 500)     ─ ⬜ optionnel
  ├─► 9.4  scene/mod.rs     (1 433, déjà sous la cible)              ─ ✅ terminé dès 9.0
  ├─► 9.5  app/network_client.rs (1 561 → trim optionnel vers ~1 500) ─ ⬜ optionnel
  └─► 9.6  app/simulation.rs     (1 500, pile la cible)              ─ ✅ terminé dès 9.0

9.7  Garde-fou final (cargo test --features net_tests, goldens, relecture git log)
```

9.1 et 9.2 n'ont aucune dépendance l'un envers l'autre et peuvent être menés en parallèle
(sessions différentes) une fois 9.0 vert. 9.3 à 9.6 sont indépendants entre eux et de 9.1/9.2.

---

## 9.0 — Extraction de `mod tests` (les 6 fichiers) — ✅ FAIT (2026-07-19)

**Objectif** : sortir chaque bloc `mod tests { ... }` inline dans un fichier frère, sans toucher
une seule ligne de test, via l'attribut `#[path]` (pas besoin de convertir le fichier en dossier
`mod.rs` pour ça — Rust autorise un chemin de module arbitraire).

**Technique** (identique pour les 6, un commit par fichier) :
1. Couper le bloc `mod tests { ... }` (avec son `#[cfg(test)]`) du fichier source, coller tel
   quel dans `src/<chemin>_tests.rs` (ex. `src/app/network_client_tests.rs`), en retirant
   l'enveloppe `mod tests { }` externe (le fichier devient le corps du module).
2. Remplacer dans le fichier source par :
   `#[cfg(test)]\n#[path = "network_client_tests.rs"]\nmod tests;`
3. `cargo test --lib <module>::tests` pour vérifier que les tests sont toujours découverts et
   passent à l'identique.

| Sous-phase | Fichier source | Nouveau fichier | Lignes déplacées | Fichier source après | Tests vérifiés |
|---|---|---|---|---|---|
| 9.0.1 ✅ | `src/scene/mod.rs` | `src/scene/mod_tests.rs` (2 583) | 4016 → 1433 | 1433 | 68 passed (`scene::tests::*`) |
| 9.0.2 ✅ | `src/app/mod.rs` | `src/app/mod_tests.rs` (2 761) | 4415 → 1654 | 1654 | 55 passed (`app::tests::*`) |
| 9.0.3 ✅ | `src/app/network_client.rs` | `src/app/network_client_tests.rs` (1 667) | 3228 → 1561 | 1561 | 22 passed sans `net_tests`, 36 passed avec `--features net_tests` |
| 9.0.4 ✅ | `src/app/simulation.rs` | `src/app/simulation_tests.rs` (1 550) | 3050 → 1500 | 1500 | 47 passed (`app::simulation::tests::*`) |
| 9.0.5 ✅ | `src/gfx/renderer.rs` | `src/gfx/renderer_tests.rs` (396) | 3555 → 3159 | 3159 | 5 passed (`gfx::renderer::tests::*`) |
| 9.0.6 ✅ | `src/scene/demos.rs` | `src/scene/demos_tests.rs` (952) | 10820 → 9868 | 9868 | 17 passed (`scene::demos::tests::*`) |

Note d'exécution : `src/app/network_client.rs` avait en réalité un attribut composé
`#[cfg(all(test, not(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))))]`
au lieu d'un simple `#[cfg(test)]` — préservé à l'identique sur la déclaration `#[path]`.
Garde-fou final de la phase : `cargo test --lib` complet → **649 passed, 0 failed, 9 ignored**
(même profil qu'avant les extractions). Six commits atomiques, un par sous-phase.

> 9.0.5 et 9.0.6 sont refaits par la suite en 9.1/9.2 (le fichier devient un dossier, donc le
> fichier de tests migre encore une fois vers `.../tests.rs`) — mais les faire dès 9.0 donne un
> point de repère vert immédiat et découple le risque « tests » du risque « re-découpage du
> dossier ».

**Livrable vérifiable** : `cargo test` inchangé (même nombre de tests, même résultat),
`wc -l` sur chaque fichier source montre la baisse attendue.
**Risques** : quasi nul — copier-coller mécanique, aucune ligne de logique de production
touchée.

---

## 9.1 — `scene/demos.rs` (10 820 lignes avant 9.0 → 9 868 après 9.0.6 → `scene/demos/`)

État courant (2026-07-19, après 9.0.6) : `src/scene/demos.rs` fait **9 868 lignes** (tests déjà
sortis dans `src/scene/demos_tests.rs`, 952 lignes, via `#[path]` — reste à migrer vers
`demos/tests.rs` une fois le dossier créé, cf. 9.1.7). Relevé structurel de `impl Scene { ... }`
(ligne 934 à 9864, ~8 931 lignes = **90 % du fichier restant**) :

| Méthode | Lignes | Taille |
|---|---|---|
| `controller_demo` / `controller_level` | 937 – 1394 | ~458 |
| `tower_demo` | 1395 – 1541 | ~147 |
| `temple_run_demo` | 1542 – 1725 | ~184 |
| `components_demo` | 1726 – 1807 | ~82 |
| `zombies_demo` | 1808 – 2042 | ~235 |
| **`mmorpg_demo`** | 2043 – 6454 | **~4 412** ⚠️ le vrai monstre |
| **`hameau_gdd_demo`** | 6455 – 8969 | **~2 515** ⚠️ deuxième monstre |
| `roguelike_demo` | 8970 – 9289 | ~320 |
| `brawl_demo` | 9290 – 9412 | ~123 |
| `boss_demo` | 9413 – 9528 | ~116 |
| `escorte_demo` | 9529 – 9637 | ~109 |
| `gameplay_demo` / `embedded_player` / `demo` / `mobile_demo` | 9638 – 9866 | ~229 |

Plus, avant `impl Scene` : les scripts de créatures (`creature_bite_script` … `creature_turret_script`,
lignes 258–906, ~650 lignes) et `import_single_model` (907–933).

### 9.1.1 — Scaffolding du dossier — ✅ FAIT (2026-07-19)

**Tâches** : créer `src/scene/demos/mod.rs` qui déclare les sous-modules et **réexporte tout à
l'identique** (`pub use` des mêmes symboles publics qu'aujourd'hui), pour ne casser aucun
`use crate::scene::demos::...` existant. Aucune ligne de logique déplacée à cette étape — juste
la coquille + un premier sous-module trivial pour valider le pattern.
**Fichiers** : `src/scene/demos/mod.rs` (nouveau), `src/scene/demos.rs` supprimé (contenu migré
progressivement dans les sous-phases suivantes).
**Livrable vérifiable** : `cargo build` toujours vert, chemins d'import inchangés.
**Risques** : faible — erreurs de visibilité (`pub(crate)` vs `pub`) à surveiller si des items
étaient `pub(super)` implicitement.

Réalisé plus simplement que prévu : renommage pur via `git mv` (`demos.rs` → `demos/mod.rs`,
`demos_tests.rs` → `demos/tests.rs`, chemin `#[path]` mis à jour), sans réexport nécessaire —
`pub(crate) mod demos;` dans `scene/mod.rs` résout indifféremment `demos.rs` ou `demos/mod.rs`.
`cargo build` + `cargo test --lib scene::demos::` verts (17 tests). Un commit.

### 9.1.2 — Scripts de créatures et import — ✅ FAIT (2026-07-19)

**Tâches** : déplacer `creature_bite_script` … `creature_turret_script` + `import_single_model`
(258–933, ~675 lignes) vers `demos/creature_scripts.rs`.
**Livrable vérifiable** : `cargo test` (tests qui exercent ces scripts, ex. patrouille/attaque
de créature) inchangé.

Plage réelle 229–932 (704 lignes, la doc de `creature_bite_script` commençait 29 lignes avant
le `fn`, ratée par l'estimation initiale). Fonctions rendues `pub(super)`, importées dans
`mod.rs` via `use creature_scripts::*;`. `creature_wander_script` (pub(crate), référencé depuis
`app::simulation_tests` par le chemin `crate::scene::demos::creature_wander_script`) reste
délibérément dans `mod.rs`, hors de ce sous-module. `cargo test --lib scene::demos:: app::simulation::`
verts (17 + 47 tests). Un commit.

### 9.1.3 — Démos courtes et indépendantes — ✅ FAIT (2026-07-19)

**Tâches** : une extraction par démo, chacune dans son propre fichier, chacune un commit :

| Sous-phase | Fichier cible | Contenu | Résultat |
|---|---|---|---|
| 9.1.3.a ✅ | `demos/controller.rs` (460 l.) | `controller_demo`, `controller_level` | 10 tests OK |
| 9.1.3.b ✅ | `demos/tower.rs` (149 l.) | `tower_demo` | 2 tests OK |
| 9.1.3.c ✅ | `demos/temple_run.rs` (189 l.) | `temple_run_demo` | 1 test OK |
| 9.1.3.d ✅ | `demos/components.rs` (84 l.) | `components_demo` | 1 test OK |
| 9.1.3.e ✅ | `demos/zombies.rs` (229 l.) | `zombies_demo` | 2 tests OK |
| 9.1.3.f ✅ | `demos/roguelike.rs` (323 l.) | `roguelike_demo` | 3 OK + 1 ignoré (flaky préexistant) |
| 9.1.3.g ✅ | `demos/brawl.rs` (128 l.) | `brawl_demo` | 3 tests OK |
| 9.1.3.h ✅ | `demos/boss.rs` (122 l.) | `boss_demo` | 2 tests OK |
| 9.1.3.i ✅ | `demos/escorte.rs` (114 l.) | `escorte_demo` | 5 tests OK |
| 9.1.3.j ✅ | `demos/misc.rs` (233 l.) | `gameplay_demo`, `embedded_player`, `demo`, `mobile_demo` | suite complète OK |

Chacune reste un `impl Scene { pub fn xxx_demo() -> Self { ... } }` complet et autonome dans son
fichier — Rust autorise plusieurs blocs `impl Scene` répartis sur plusieurs fichiers du même
crate, donc c'est un copier-coller pur, sans changement de signature ni de corps. Les fichiers
extraits en milieu de bloc (roguelike/brawl/boss/escorte/misc, situés après `mmorpg_demo`/
`hameau_gdd_demo` dans le fichier d'origine) laissent la déclaration `mod xxx;` groupée avec les
autres en tête de `demos/mod.rs`, et seul le contenu de la méthode est retiré du bloc `impl Scene`
— celui-ci reste continu autour du trou. Garde-fou final de la sous-phase : `cargo test --lib`
complet → **649 passed, 0 failed, 9 ignored** (identique à la baseline post-9.0). 10 commits.
`demos/mod.rs` passe de 9 868 à **7 188 lignes**, ne contenant plus que `MMORPG_HALF`,
`mmorpg_demo` (~4 412 l.) et `hameau_gdd_demo` (~2 515 l.) — restent 9.1.4 à 9.1.7.
**Risques** : faible, sauf si une démo référence une fonction locale (`fn` imbriquée) définie
dans une autre démo — vérifier au `cargo build` qui signalera l'import manquant immédiatement.

### 9.1.4 — `mmorpg_demo` (~4 412 lignes) : lecture préalable — ✅ FAIT (2026-07-19)

**Tâches** : avant de couper, lire la fonction en entier pour repérer les coupures sûres — il
n'y a **aucun commentaire de section** dans le code actuel (vérifié), donc les frontières
thématiques (terrain / village / créatures / vagues) doivent être identifiées à la lecture,
pas devinées. Repérer aussi l'état partagé entre « sections » : `rng`, compteurs d'index, trois
fonctions locales `scatter` (l. 5209), `scatter_clustered` (l. 5271), `scatter_each` (l. 5858),
qui sont probablement utilisées par plusieurs sections (décor, forêt, village) et doivent devenir
des fonctions partagées plutôt que dupliquées.
**Livrable** : note de découpage (peut être un commentaire de PR, pas un fichier permanent)
listant les plages de lignes retenues et les dépendances croisées trouvées.
**Risques** : c'est l'étape qui protège tout le reste — sauter la lecture et couper à l'aveugle
est le principal risque de régression comportementale de tout le sprint.

Corrections issues de la lecture réelle (contrairement au premier relevé, il existe bien des
marqueurs `// --- ... ---`, ratés par le premier grep car indentés à 8 espaces, pas 4) :
- Un seul accumulateur `objects: Vec<SceneObject>` traverse toute la fonction via `.push()`
  (jamais `.extend()`), plus `imported: Vec<ImportedMesh>` — le pattern `build_x() -> Vec<...>`
  envisagé au brief n'existe pas dans le code réel ; chaque section pousse directement dans le
  même vecteur partagé, donc chaque fonction extraite prend `&mut Vec<SceneObject>` en
  paramètre plutôt que de retourner un `Vec`.
- Structure déjà en partie data-driven : `struct DemoCreature` + `const MMORPG_CREATURES`
  (créatures nommées), et surtout `struct DemoDecor` + 3 `const` (`NATURE_DECOR`/
  `VILLAGE_PROPS`/`MONSTER_DECOR`) — cf. la mémoire de session sur le piège des 3 tableaux
  `DemoDecor` et `solid_spots`. Ces tables sont consommées par une closure `poser` qui **capture
  mutable** `objects`/`imported`/`anim_count`, closure elle-même réutilisée par la section de
  scatter procédural (`rng`, les 3 `scatter*`) juste après — **`poser` et le scatter procédural
  ne sont pas séparables** sans transformer la closure en fonction libre (changement de
  structure plus profond que prévu, laissé de côté pour ce sprint).
- Découpage réel retenu (murs/repères/vent, créatures nommées, faune ambiante, aplats
  terrain/eau/voirie séparables ; tables de décor séparables comme pure donnée ; le reste —
  `poser` + boucle + scatter + expansion prairie + flore + items + convoi + `Scene{}` final —
  reste groupé dans l'orchestrateur).

### 9.1.5 — `mmorpg_demo` : découpage mécanique — ✅ FAIT (2026-07-19)

**Tâches**, une fois les coupures validées en 9.1.4, créer `demos/mmorpg/` :
- `mmorpg/mod.rs` — `pub fn mmorpg_demo() -> Scene` orchestrateur : appelle les fonctions de
  construction dans le même ordre qu'aujourd'hui et assemble le `Vec<SceneObject>` final à
  l'identique (le pattern attendu est `objects.extend(build_terrain()); objects.extend(build_village()); ...` —
  **l'ordre d'ajout dans le vecteur doit rester strictement identique**, certains tests/golden
  peuvent dépendre d'un ordre d'itération stable).
- `mmorpg/scatter.rs` — les 3 fonctions `scatter`/`scatter_clustered`/`scatter_each`, rendues
  `pub(super)`.
- `mmorpg/terrain.rs`, `mmorpg/village.rs`, `mmorpg/creatures.rs`, `mmorpg/waves.rs`,
  `mmorpg/decor.rs` (ou tout autre découpage validé en 9.1.4) — chacune une fonction
  `pub(super) fn build_xxx(...) -> Vec<SceneObject>` (ou `fn build_xxx(objects: &mut Vec<SceneObject>, ...)`
  si l'accumulation en place est plus fidèle au code actuel).
- Un commit par fichier extrait, dans l'ordre où les sections apparaissent dans la fonction
  d'origine (le premier commit extrait la première section, etc.) — ça garde chaque commit
  petit et relisible individuellement.
**Livrable vérifiable** : après chaque commit, `cargo test` sur les tests `mmorpg_demo_*`
existants (ils sont nombreux, voir 9.1.7) doit rester vert à l'identique.
**Risques** : le risque principal identifié en 9.1.4 (état partagé entre sections) ; sinon,
risque de sur-découpage — ne pas descendre sous des fichiers de ~200-300 lignes juste pour la
forme, viser des fichiers thématiquement cohérents.

Réalisé en 7 commits (le pattern `build_x() -> Vec<...>` du brief remplacé par
`fn add_x(objects: &mut Vec<SceneObject>, ...)`, cf. 9.1.4) :

| Étape | Fichier | Lignes | Contenu |
|---|---|---|---|
| 1/6 | `mmorpg/mod.rs` (déplacement initial) | 4378 → | `mmorpg_demo` entier déplacé tel quel avant découpage interne |
| 2/6 | `mmorpg/terrain.rs` | 76 | murs de pourtour, repères, zone de vent |
| 3/6 | `mmorpg/creatures.rs` | 513 | `DemoCreature` + `MMORPG_CREATURES` (promus `pub(super)`, y compris le champ `spawn` — nécessaire au scatter procédural resté dans `mod.rs`) + boucle de spawn |
| 4/6 | `mmorpg/fauna.rs` | 146 | faune ambiante errante (créatures 27-61) |
| 5/6 | `mmorpg/decor_data.rs` | 1911 | `DemoDecor` (7 champs `pub(super)`) + `NATURE_DECOR`/`VILLAGE_PROPS`/`MONSTER_DECOR`, pure donnée, `use decor_data::*;` dans `mod.rs` (~200 sites d'usage, trop pour qualifier un par un) |
| 6/6 | `mmorpg/decor_terrain.rs` | 368 | aplats eau/terrain/routes + murs d'eau (partie de « Décor nature » sans dépendance à `poser`/`DemoDecor`) |
| — | `mmorpg/mod.rs` (final) | **1396** | orchestrateur : init, 4 appels en séquence, `poser` + boucle + scatter procédural + expansion prairie + flore + items + convoi + `Scene{}` final |

**Deux incidents pendant l'extraction, tous deux corrigés avant commit** : un `head -N` mal
compté a fait disparaître silencieusement l'appel `creatures::add_named_creatures` (étape 4),
puis un second a fait disparaître `fauna::add_ambient_fauna` (étape 6) — dans les deux cas
`cargo build` a réussi quand même (un appel de fonction manquant n'est pas une erreur de
compilation ; seul `dead_code` l'a signalé la seconde fois). Repérés par grep manuel des appels
d'orchestrateur avant `cargo test`, jamais par la compilation seule — **leçon retenue : après
toute suppression de plage de lignes par `sed`/`head`, `grep` la liste des appels attendus avant
de faire confiance à `cargo build`.**

Un doc-comment orphelin (celui de `mmorpg_demo`, resté collé à `MMORPG_HALF` dans
`demos/mod.rs` au lieu de suivre la fonction déplacée) a aussi été trouvé et corrigé, signalé
par `clippy::empty_line_after_doc_comments` — `cargo clippy` fait donc partie du garde-fou de
fin de sous-phase, pas seulement `cargo build`/`cargo test`.

Garde-fou final : `cargo build`, `cargo clippy --lib` (0 warning), `cargo test --lib mmorpg`
(28 passed) puis suite complète `cargo test --lib` → **649 passed, 0 failed, 9 ignored**
(identique à la baseline post-9.0). `mmorpg/mod.rs` recule de ~4374 à **1396 lignes**, sous la
cible de ~1500.

### 9.1.6 — `hameau_gdd_demo` (~2 515 lignes) : même traitement — ✅ FAIT (2026-07-19)

**Tâches** : même méthode qu'en 9.1.4/9.1.5 — lecture préalable pour repérer les coupures,
puis dossier `demos/hameau_gdd/` (`mod.rs` orchestrateur + fichiers thématiques, ex. fort,
village, créatures, décor). Le commentaire de tête de la fonction (l. 4376-4412) indique déjà
que les créatures sont « reprises telles quelles de `mmorpg_demo()` » — vérifier si un partage
de code avec `demos/mmorpg/creatures.rs` est possible sans changer le comportement, sinon
dupliquer sciemment plutôt que factoriser à ce stade (une factorisation est un changement de
structure, pas une extraction mécanique — hors scope du sprint 9).
**Risques** : identiques à 9.1.5, en plus petit.

Lecture préalable : contrairement à `mmorpg_demo`, `hameau_gdd_demo` a 24 marqueurs de section
`// --- ... ---` bien identifiés, et surtout ses « fn locales de pose » (`at`, `in_corridor`,
`poser`, `marker`, `box_seg`, `aplat`, `foret_scatter`, `faune_scatter`, `poser_scaled`,
`wall_run`) sont de vraies fonctions libres, pas des closures — aucune capture implicite, donc
partageables entre plusieurs fichiers extraits (contrairement au `poser`/scatter de
`mmorpg_demo`, cf. 9.1.4). `HALF`/`GATE_HALF`/`TRIM` promus `pub(super) const` au niveau module
(utilisés à travers plusieurs sections) ; `MODULE_LEN` gardé local à `wall_run` (son seul usage).
Le partage de code créatures évoqué au brief n'a pas été tenté : `hameau_gdd_demo` extrait ses
créatures depuis `Scene::mmorpg_demo()` (`let base = Scene::mmorpg_demo(); for c in
base.objects...`), un couplage déjà présent dans le code d'origine, laissé tel quel.

Réalisé en 7 commits :

| Étape | Fichier | Lignes | Contenu |
|---|---|---|---|
| 1/7 | `hameau_gdd/mod.rs` (déplacement initial) | 2547 → | `hameau_gdd_demo` entier déplacé, doc-comment inclus dès cette étape (contrairement au premier essai sur `mmorpg_demo`, cf. 9.1.5) |
| 2/7 | `hameau_gdd/helpers.rs` | 319 | les 10 fn libres de pose, promues `pub(super)` |
| 3/7 | `hameau_gdd/fort.rs` | 489 | créatures (reprises de `mmorpg_demo`), remparts, chemin de ronde, dressing remparts |
| 4/7 | `hameau_gdd/village.rs` | 692 | place centrale, anneau de spawns, îlots bâtis, artisanat, marché, lanternes/bannières |
| 5/7 | `hameau_gdd/water.rs` | 352 | rivière/lac hors les murs + habillage organique `shore_*`/`grotto_*` |
| 6/7 | `hameau_gdd/wilds.rs` | 444 | poste de guet, cabane, camps, second point d'eau, prairies, verger, forêt en anneau |
| 7/7 | `hameau_gdd/tail.rs` | 158 | faune aquatique, lucioles, lande, Aînée de la lande (boss), tweak émissif du feu communal |
| — | `hameau_gdd/mod.rs` (final) | **131** | orchestrateur pur : init, 5 appels en séquence, `Scene{}` final |

**Nouveau piège trouvé (distinct de mmorpg_demo), rencontré à chaque extraction** : une fonction
qui reçoit `objects: &mut Vec<SceneObject>` en paramètre et fait ensuite `poser(&mut objects,
...)` en son sein échoue à la compilation (E0596 « cannot borrow as mutable ») sauf si le
paramètre lui-même est déclaré `mut objects: &mut Vec<SceneObject>` — un `&mut T` reçu en
paramètre est un binding immuable par défaut, le reborrow `&mut objects` a besoin que le binding
soit `mut`. Corrigé à chaque fichier, puis `cargo clippy --fix --lib --allow-dirty` a supprimé
les réemprunts `&mut` devenus inutiles (`objects`/`imported` se reborrowent implicitement aux
sites d'appel une fois qu'ils sont eux-mêmes des références) et signalé le `mut` du binding
redevenu inutile à son tour — retiré manuellement à chaque fois. `git status --short` vérifié
après chaque `clippy --fix` pour confirmer qu'il n'avait touché que le fichier visé.

Deux erreurs de comptage de lignes supplémentaires trouvées et corrigées avant commit (mêmes
symptômes qu'en 9.1.5, cf. son tableau) : un appel de fonction manquant côté `village.rs`
(repéré cette fois côté documentation : un `tail -n +764` avait fait sauter la première ligne
du commentaire « --- Hors les murs » restant dans `mod.rs`, perte de doc pas de comportement).

Garde-fou final : `cargo build`, `cargo clippy --lib` (0 warning), suite complète
`cargo test --lib` → **649 passed, 0 failed, 9 ignored** (identique à la baseline).

### 9.1.7 — Tests — ✅ FAIT (dès 9.1.1, confirmé 2026-07-19)

**Tâches** : `src/scene/demos_tests.rs` (952 lignes, 17 tests `mmorpg_demo_*`/`mmorpg_map_*`/
`mmorpg_water_*`/etc., déjà extrait par 9.0.6 — ✅ fait) doit être déplacé tel quel vers
`demos/tests.rs` une fois le dossier créé en 9.1.1 (renommage de fichier, la déclaration
`#[path = "demos_tests.rs"]` devient `#[path = "tests.rs"]` dans `demos/mod.rs`, aucun contenu
touché). Vu qu'ils testent surtout `mmorpg_demo()` et `hameau_gdd_demo()` par leur sortie
publique (`Scene`), ils peuvent rester groupés dans un seul fichier de tests plutôt que d'être
éclatés par sous-module — ce sont des tests de bout en bout de chaque démo, pas des tests
unitaires de `build_terrain`.
**Livrable vérifiable** : `cargo test` — même liste de tests (17), même résultat.

Fait mécaniquement lors du renommage `demos.rs` → `demos/mod.rs` en 9.1.1 (`git mv
demos_tests.rs demos/tests.rs`, chemin `#[path]` mis à jour) — rien à refaire ici. 952 lignes,
groupées comme prévu, aucun éclatement par sous-module.

---

## 9.1 — Bilan final

`src/scene/demos.rs` (10 820 lignes avant sprint) est maintenant un dossier de 26 fichiers,
**10 958 lignes au total** (léger delta dû au découpage : signatures de fonctions, `use`
répétés). Aucun fichier ne dépasse 1 911 lignes (`mmorpg/decor_data.rs`, pure donnée, 3 tables
`const`), la plupart sont sous 700 lignes :

| Fichier | Lignes | | Fichier | Lignes |
|---|---|---|---|---|
| `mmorpg/decor_data.rs` | 1911 | | `boss.rs` | 122 |
| `mmorpg/mod.rs` | 1402 | | `escorte.rs` | 114 |
| `tests.rs` | 952 | | `mmorpg/terrain.rs` | 76 |
| `creature_scripts.rs` | 706 | | `hameau_gdd/mod.rs` | 131 |
| `hameau_gdd/village.rs` | 692 | | `components.rs` | 84 |
| `hameau_gdd/fort.rs` | 489 | | — | — |
| `controller.rs` | 460 | | — | — |
| `hameau_gdd/wilds.rs` | 444 | | — | — |
| `mmorpg/creatures.rs` | 513 | | — | — |
| `hameau_gdd/water.rs` | 352 | | — | — |
| `mmorpg/decor_terrain.rs` | 368 | | — | — |
| `roguelike.rs` | 323 | | — | — |
| `hameau_gdd/helpers.rs` | 319 | | — | — |
| `zombies.rs` | 229 | | — | — |
| `misc.rs` | 233 | | — | — |
| `demos/mod.rs` | 268 | | — | — |
| `temple_run.rs` | 189 | | — | — |
| `hameau_gdd/tail.rs` | 158 | | — | — |
| `tower.rs` | 149 | | — | — |
| `mmorpg/fauna.rs` | 146 | | — | — |
| `brawl.rs` | 128 | | — | — |

24 commits au total pour la phase 9.1 (9.1.1 à 9.1.6), tous atomiques et relisibles
individuellement, suite complète verte à chaque étape.

---

## 9.2 — `gfx/renderer.rs` (3 526/3 555 lignes avant 9.0 → 3 159 après 9.0.5 → `gfx/renderer/`) — ✅ FAIT (2026-07-19)

État final : `gfx/renderer/mod.rs` fait **45 lignes** (imports + 7 déclarations `mod` + `mod
tests;`) — plus aucun `impl Renderer` dedans, tout a migré vers 7 fichiers frères. 8 commits
(9.2 scaffolding + 9.2.1 à 9.2.8), suite complète verte à chaque étape, **zéro octet modifié
sous `tests/golden/`** sur toute la phase (piège n°3 du guide vérifié après chaque extraction,
pas seulement à la fin).

État avant découpage interne (après 9.0.5) : `src/gfx/renderer.rs` faisait **3 159 lignes**
(tests déjà sortis dans `src/gfx/renderer_tests.rs`, 396 lignes, via `#[path]`). Relevé
structurel de `impl Renderer { ... }` (ligne 421 à 3155, ~2 735 lignes = **87 % du fichier
restant**) :

| Groupe thématique | Méthodes | Lignes | Taille |
|---|---|---|---|
| **Ressources** | `new(...)` (constructeur), `resize`, `ensure_debug_capacity` | 421 – 746 | ~326 |
| **Ombres / skinning** | `write_joint_matrices`, `prepare_skinned_draws`, `skinned_dropped_count`, `draw_skinned_objects`, `draw_skinned_shadows`, `render_skinned_test` | 747 – 1086 | ~340 |
| **UI** | `on_ui_event`, `settings`, `toggle_multiplayer_window`, `toggle_play_hud`, `toggle_player_settings`, `toggle_player_map` | 1087 – 1131 | ~45 |
| **Synchro scène** | `sync_objects`, `resolve_mesh`, `sync_imported`, `sync_textures`, `write_uniforms` | 1132 – 1439 | ~308 |
| **Post-process** | `render_bloom`, `tonemap`, `read_gpu_pass_timings`, `gpu_profiler_info` | 1440 – 1652 | ~213 |
| **Frame** ⚠️ | `render` (fonction unique) | 1653 – 2776 | **~1 124** |
| **Headless / capture** | `render_scene_headless`, `screenshot_png`, `finish_and_read_rgba` | 2777 – 3155 | ~379 |

Plus les définitions de types en tête de fichier (`GizmoVertex`, `PointLightU`, `BloomUniform`,
`InstanceDraw`, `GpuProfiler`, struct `Renderer`, lignes 34–420, ~390 lignes).

> **Piège n°3 du guide** (goldens sensibles aux changements de shader) : ce sprint ne touche
> **aucun** shader ni ordre de passes, seulement l'emplacement du code Rust. Si un test golden
> ou visuel change après une extraction 9.2, c'est un signal d'erreur (comportement modifié par
> inadvertance), pas une mise à jour de golden à accepter.

### Scaffolding — ✅ FAIT

Renommage pur (`git mv renderer.rs renderer/mod.rs`, `git mv renderer_tests.rs renderer/tests.rs`,
`#[path]` mis à jour), identique au patron 9.1.1/9.1.6. Baseline posée avant tout découpage
interne : `cargo test --lib gfx::renderer::` (5 passed) + `cargo test --test golden_render --test
golden_skinning` (8 passed).

### Piège de visibilité central de la phase 9.2 (nouveau, absent de 9.1)

L'ancien `renderer.rs` était un seul module `gfx::renderer` : certains types internes
(`GizmoVertex`, `CameraUniform`, `ModelUniform`, `SceneUniform` + 6 `const` GPU) étaient déjà
`pub(super)`, ce qui à cette profondeur-là signifiait « visible depuis `gfx` » — nécessaire
puisque `gfx::pipelines`/`gfx::passes` (modules frères de `renderer`) les importent directement
(`use super::renderer::{GizmoVertex, CameraUniform, ...};`). En déplaçant ces types dans
`gfx::renderer::types` (un niveau plus profond), **le même mot-clé `pub(super)` change de sens**
— il devient « visible seulement depuis `gfx::renderer` » — et casse ces imports cross-module.
Trouvé par le compilateur (E0603/E0422/E0425/E0451/E0624 en cascade, 52 erreurs au premier essai),
pas par relecture. Corrigé en promouvant ces types précis (+ leurs champs pour `GizmoVertex`, la
seule struct dont les champs sont construits hors de `renderer/`) en `pub(crate)`. `Renderer`
lui-même (déjà pleinement `pub`, utilisé par `tests/golden_*.rs` et `src/bin/glbviewer.rs` en
dehors du crate) a en plus exigé un `pub use types::Renderer;` séparé, explicite : le
`pub(crate) use types::*;` global plafonne tout le reste à `pub(crate)`, y compris un item par
ailleurs pleinement public.

Pour les 9.2.2 à 9.2.8 suivants, aucun de ces pièges : les méthodes qui migrent d'un fichier vers
un autre étaient soit déjà `pub`/`pub(crate)` (inchangées), soit **privées** dans l'ancien
fichier plat — et une méthode privée dans l'ancien `renderer.rs` signifiait déjà « visible
seulement dans `renderer` », exactement ce que donne `pub(super)` une fois déplacée dans
n'importe quel enfant de `renderer` (tous les fichiers frères créés dans cette phase sont des
enfants directs de `renderer`, donc `pub(super)` y préserve la portée d'origine sans la
rétrécir ni l'élargir). Chaque section privée appelée depuis un fichier qui migre plus tard
(typiquement `render()`/`render_scene_headless`, restés dans `mod.rs` jusqu'à 9.2.7/9.2.8) a été
promue `pub(super)` au moment de son extraction — vérifié section par section via `grep` des
sites d'appel avant chaque extraction, jamais deviné.

### 9.2.1 — Types — ✅ FAIT

**Tâches** : déplacer `GizmoVertex`, `PointLightU`, `BloomUniform`, `InstanceDraw`,
`GpuProfiler` + struct `Renderer` (34–420) vers `renderer/types.rs`.
**Fichiers** : `src/gfx/renderer/types.rs` (395 lignes).

Piège de visibilité décrit ci-dessus, corrigé avant commit. Garde-fou : `cargo build
--all-targets`, `cargo clippy --lib` (0 warning), `cargo test --lib` (649 passed), 8 golden
passed, zéro PNG modifié.

### 9.2.2 — Ressources (init/resize) — ✅ FAIT

**Tâches** : extraire `new`, `resize`, `ensure_debug_capacity` vers `renderer/resources.rs`
(307 lignes). Inclut aussi `new_headless` et `new_impl` (non listées au brief initial — `new`/
`new_headless` sont deux façades publiques minces qui délèguent à `new_impl`, la vraie
construction). `new_impl`/`ensure_debug_capacity` (privées) promues `pub(super)`.

Deuxième `sed -i` mal borné cette étape (`ensure_debug_capacity` coupée avant sa propre accolade
fermante) — repéré par déséquilibre d'accolades avant même `cargo build`, corrigé. Garde-fou
complet vert.

### 9.2.3 — Ombres / skinning — ✅ FAIT

**Tâches** : extraire le groupe skinning/ombres (747–1086) vers `renderer/shadows.rs`
(363 lignes). `prepare_skinned_draws`/`draw_skinned_objects`/`draw_skinned_shadows` (privées,
appelées depuis `render()`/`render_scene_headless` restés dans `mod.rs`) promues `pub(super)`.
`write_joint_matrices` reste privée (appelée uniquement depuis l'intérieur de ce même fichier).

Incident : une hypothèse non revérifiée sur le numéro de ligne de `impl Renderer {` (mémorisée
d'un tour de parole précédent plutôt que re-grepée) a fait supprimer la ligne elle-même au
premier essai — repéré immédiatement par une erreur de syntaxe `cargo build`, corrigé en
re-grepant avant de refaire l'opération. Garde-fou complet vert.

### 9.2.4 / 9.2.6 — Synchro scène + UI — ✅ FAIT (bundlées, comme suggéré au brief)

**Tâches** : extraire `sync_objects`, `resolve_mesh`, `sync_imported`, `sync_textures`,
`write_uniforms` (synchro scène) + `on_ui_event`, `settings`, `toggle_multiplayer_window`,
`toggle_play_hud`, `toggle_player_settings`, `toggle_player_map` (UI, petit groupe) dans un seul
fichier/commit : `renderer/sync.rs` (352 lignes). Les 5 méthodes de synchro (privées) promues
`pub(super)`, le groupe UI (déjà `pub`) inchangé. Garde-fou complet vert.

### 9.2.5 — Post-process — ✅ FAIT

**Tâches** : extraire `render_bloom`, `tonemap`, `read_gpu_pass_timings`, `gpu_profiler_info`
vers `renderer/post_process.rs` (221 lignes). Les 3 premières (privées) promues `pub(super)` —
`tonemap` en particulier déjà appelée depuis `shadows::render_skinned_test` (extrait en 9.2.3),
confirmant qu'un frère peut consommer un autre frère du moment que la visibilité est au moins
`pub(super)`. `gpu_profiler_info` déjà `pub`, inchangée. Garde-fou complet vert.

### 9.2.7 — Frame (`render`, ~1 124 lignes → 1 122 lignes réelles) — ✅ FAIT

**Tâches** : déplacer `render()` telle quelle vers `renderer/frame.rs`, **sans la sous-découper**
en sous-fonctions. C'est délibéré : `render()` orchestre les passes dans un ordre précis
(shadow → main → bloom → tonemap → UI) et la re-router en plusieurs méthodes est un changement
de structure de contrôle, pas une extraction mécanique de fichier — donc hors scope du sprint 9,
et exactement le genre de changement qui peut faire bouger un golden sans bug réel. À noter
comme candidat pour un sprint dédié si `frame.rs` reste jugée trop grosse plus tard.
**Livrable vérifiable** : diff du golden de rendu = zéro pixel changé.

Déjà `pub fn`, aucune promotion de visibilité nécessaire — extraction purement mécanique, zéro
incident. `cargo test --test golden_render --test golden_skinning` → 8 passed, `git status
--short tests/golden/` vide : piège n°3 du guide directement vérifié sur l'étape la plus à
risque de toute la phase. Garde-fou complet vert.

### 9.2.8 — Headless / capture — ✅ FAIT

**Tâches** : extraire `render_scene_headless`, `screenshot_png`, `finish_and_read_rgba` vers
`renderer/headless.rs` (387 lignes). `finish_and_read_rgba` (privée, appelée depuis
`shadows::render_skinned_test`) promue `pub(super)`. Dernière extraction : `impl Renderer {}`
dans `mod.rs` devenait vide, retiré entièrement — `mod.rs` final ne contient plus que des
imports et 7 déclarations `mod` (+ `mod tests;`). Garde-fou complet vert.

### 9.2.9 — Tests — ✅ FAIT (dès le scaffolding)

**Tâches** : `src/gfx/renderer_tests.rs` (396 lignes, 5 tests, déjà extrait par 9.0.5 — ✅ fait)
doit être déplacé tel quel vers `renderer/tests.rs` une fois le dossier créé en 9.2.1
(renommage de fichier, la déclaration `#[path = "renderer_tests.rs"]` devient
`#[path = "tests.rs"]` dans `renderer/mod.rs`, aucun contenu touché).

Fait mécaniquement lors du scaffolding (avant même 9.2.1), même patron que 9.1.7.

**Livrable vérifiable global 9.2** : `cargo test`, et une vérification visuelle des goldens
(`UPDATE_GOLDEN` **interdit** pour ce sprint — tout diff de golden est un bug à corriger, pas à
accepter). Confirmé à chaque sous-phase, jamais un seul octet de `tests/golden/*.png` modifié.

Fichiers finaux `gfx/renderer/` : `frame.rs` 1122, `headless.rs` 387, `tests.rs` 396,
`types.rs` 395, `shadows.rs` 363, `sync.rs` 352, `resources.rs` 307, `post_process.rs` 221,
`mod.rs` 45. 8 commits pour la phase 9.2 (scaffolding + 9.2.1 à 9.2.8).

---

## 9.3 — `app/mod.rs` (trim optionnel)

Après 9.0.2 (tests sortis, ✅ fait) : **1 654 lignes** confirmées (`wc -l`). Struct `AppState`
(184–795, ~611 lignes de champs), `GizmoMode`/`DebugView` (796–873), `impl AppState` (874–1511,
~638 lignes de méthodes), puis `MinimapPoint`/`MinimapDecorKind`/`MinimapDecor`/
`MinimapCreature`/`MinimapData` + `classify_decor`/`thin_decor` (1512–1639, ~128 lignes).

**Tâches (optionnel, mais peu coûteux)** : extraire le bloc minimap (1512–1639) vers
`src/app/minimap.rs` — thématiquement distinct (données d'affichage de la mini-carte) et
suffisant pour repasser sous ~1 500 lignes.
**Livrable vérifiable** : `cargo test`.
**Risques** : nul — bloc déjà autonome (pas de dépendance vers le reste du fichier au-delà des
imports).

## 9.4 — `scene/mod.rs` — ✅ terminé dès 9.0

Après 9.0.1 (tests sortis, ✅ fait) : **1 433 lignes** confirmées (`wc -l`), modèle de données
pur (structs/enums avec leurs `derive`/`Default`/valeurs par défaut serde) — **déjà sous la
cible de ~1 500 lignes**.
**Tâches** : aucune obligatoire. Optionnel si on veut regrouper thématiquement (types de
transform/mesh vs types de gameplay/combat vs conteneur `Scene`/`Sky`/`Light`) — à ne faire que
si un besoin de lisibilité se fait sentir plus tard, pas requis pour clore le sprint.

## 9.5 — `app/network_client.rs` (trim optionnel)

Après 9.0.3 (tests sortis, ✅ fait) : **1 561 lignes** confirmées (`wc -l`). Structs
`RemotePlayer`/`NetConnState`/`ChatLine`/`LeaderboardLine` (42–206, ~165 lignes) + plusieurs
blocs `impl AppState` (207–1561) pour la connexion, le protocole, le chat.
**Tâches (optionnel)** : extraire les structs de types (42–206) vers `network_client/types.rs`
pour repasser franchement sous ~1 500 lignes. Pas de découpage supplémentaire nécessaire.

## 9.6 — `app/simulation.rs` — ✅ terminé dès 9.0

Après 9.0.4 (tests sortis, ✅ fait) : **1 500 lignes** confirmées (`wc -l`) — **pile la cible**.
Aucune tâche obligatoire.

---

## 9.7 — Garde-fou final

**Tâches** :
1. Après chaque extraction individuelle (toutes sous-phases ci-dessus) : vérification standard
   du guide (`cargo build`, `cargo test` du module touché, `cargo clippy`).
2. Une fois 9.0 à 9.6 terminées : `cargo test --features net_tests` complet, une seule fois.
3. Vérification golden/visuelle sur `gfx/renderer.rs` (9.2) — zéro diff attendu.
4. Relecture de `git log` : chaque extraction doit apparaître comme un commit atomique,
   relisible indépendamment (diff = déplacement de lignes, pas de réécriture).

**Terminé quand** :
- Aucun fichier de `src/` ne dépasse ~4 000 lignes, cible ~1 500 pour les nouveaux modules issus
  du découpage (exception assumée : `renderer/frame.rs` ~1 095 lignes en une seule fonction,
  volontairement non re-découpée — cf. 9.2.7).
- La suite complète passe (`cargo test --features net_tests`).
- Zéro diff de comportement : aucun golden, aucun test visuel, aucun test réseau n'a changé de
  résultat.
- `git log` montre des extractions atomiques relisibles une par une, dans l'ordre 9.0 → 9.6.
