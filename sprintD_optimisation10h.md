# Sprint D — LOD géométrique herbe/fougères (Phase D)

> Détail d'exécution de la **Phase D** décrite dans
> [sprintoptimation3daudit10h.md](sprintoptimation3daudit10h.md#phase-d) (Sprint 5).
> Constat source : [optimisation3D.Analys.md](optimisation3D.Analys.md).

Statut : ✅ **câblé et testé** — le blocant « primitive billboard manquante » est levé
(section « Finalisation » ci-dessous), le LOD géométrique est actif dans `Renderer::render`.
Reste ouvert : mesure Profiler avant/après en jeu (Phase F) et validation visuelle directe
dans le binaire (non faite dans cette session, cf. « Ce qui reste »).

---

## Audit du 18 juillet 2026 (relecture après premier passage)

Relecture critique de `src/gfx/lod.rs` demandée avant de considérer le sprint terminé, sans
toucher aux fichiers des autres phases. Deux défauts réels trouvés, un corrigé, un documenté
comme bloquant pour le câblage.

### 🐞 Corrigé : collision avec les variantes animées `_sway`

`FOLIAGE_LOD_KEYWORDS` contient `"reeds"`, qui matchait aussi **`nature_reeds_sway.glb`**
(rive du lac, `src/scene/demos.rs` ligne ~5879) — une variante animée distincte de
`nature_reeds.glb`, très probablement skinnée (suffixe `_sway` = animation de balancement,
même famille que `nature_wheat_sway.glb`, `nature_willow_sway.glb`, `nature_bamboo_sway.glb`,
`nature_sunflowers_sway.glb`). La substituer par l'impostor `Plane` statique lui aurait fait
perdre son animation silencieusement si le câblage avait réutilisé ce mot-clé tel quel.
**Fix** : `foliage_lod_mesh` exclut désormais tout chemin contenant `_sway`, indépendamment du
mot-clé matché — défensif aussi pour de futures variantes sway de `grass_tuft`/`fern` qui
n'existent pas encore mais pourraient être ajoutées par le pipeline d'assets. Nouveau test
`animated_sway_variant_is_never_substituted_even_far_away`. 7 tests verts après correctif.

### ⚠️ Non corrigé, bloquant pour le câblage : `MeshKind::Plane` n'est pas un impostor valide

`mesh::plane()` (`src/gfx/mesh.rs:288-318`) génère un quad **horizontal**, normale `+Y`,
posé à plat au sol (`y=0`, étendue en X/Z) — c'est un plan de sol, pas un billboard vertical.
Les touffes d'herbe/fougères sont posées debout (rotation générée par `poser` dans
`scatter_clustered`/`demos.rs`, essentiellement un yaw aléatoire, pas de tangage) : leur
substitut LOD serait donc un **plan couché**, vu quasiment par la tranche depuis une caméra à
hauteur d'œil (surface projetée à l'écran proche de zéro → touffe qui semble disparaître plutôt
que se simplifier), ou comme un aplat de texture vue du dessus en plongée — dans les deux cas,
pas le résultat visuel attendu d'un impostor (« sans changement perceptible en vue rapprochée »
devient un changement très perceptible dès le seuil de LOD).

**Cause racine** : `MeshKind` (`src/scene/mod.rs`) n'a aucune primitive de billboard vertical
face-caméra — seulement `Cube/Sphere/Plane/Cylinder/Capsule/Terrain`. En ajouter une (ou
réorienter dynamiquement l'instance LOD à 90° et la faire pivoter vers la caméra à chaque
frame) demande de toucher `src/scene/mod.rs` et le chemin de rendu — tous deux hors périmètre
de cette session (fichiers chauds d'autres sessions au moment de l'audit, cf. contexte
ci-dessus) et de toute façon nécessaires seulement **au moment du câblage**, pas maintenant
puisque `foliage_lod_mesh` n'est pas encore appelée en jeu.

**Décision** : ne pas corriger dans ce sprint (portée limitée à `lod.rs`, aucune primitive
valide disponible sans toucher un fichier hors scope). Le code reste tel quel — la fonction de
décision (« quels objets, à quelle distance ») est correcte et testée indépendamment du choix
d'impostor. **Ce point bloque explicitly le câblage** décrit plus bas : ne pas appeler
`foliage_lod_mesh` telle quelle dans `Renderer::render` sans d'abord régler le choix de
primitive, sous peine de faire disparaître visuellement l'herbe/les fougères au lieu de les
simplifier. Ajouté à la checklist « Prochaines étapes » ci-dessous.

### Points mineurs relevés, non bloquants

- **Seuil `FOLIAGE_LOD_DISTANCE = 40.0`** : valeur au jugé, non validée contre une mesure réelle
  (Phase 0 baseline pas encore faite au moment de l'écriture). À ajuster une fois la Phase 0
  disponible, comme déjà noté dans le plan général.
- **Duplication de mots-clés avec Phase C** : `FOLIAGE_LOD_KEYWORDS` (`lod.rs`, 3 entrées) et
  `FOLIAGE_LOW_RADIUS_KEYWORDS` (`passes.rs`, 16 entrées, écrit par la session Phase C en
  parallèle) catégorisent le même concept de « feuillage dense » avec deux listes distinctes.
  Pas de bug actuel (les deux fonctionnent indépendamment), mais un risque de dérive si l'une
  est mise à jour sans l'autre — déjà noté comme recommandation de réutilisation dans la
  section « Prochaines étapes ».
- **Tests** : couvrent bien la fonction de décision (proximité/distance/hors-liste/primitives/
  seuil exact/collision sway) — pas de test manquant identifié sur le périmètre actuel de
  `lod.rs`. Aucun test d'intégration possible pour l'instant puisque rien n'est câblé.

### Conclusion de l'audit

Le sprint n'est **pas** « tout parfait » : la fonction de décision est solide et bien testée,
mais le choix d'impostor (`MeshKind::Plane`) est visuellement incorrect pour du feuillage
vertical et doit être résolu avant tout câblage — sans quoi le résultat serait une régression
visuelle (herbe/fougères qui semblent disparaître) plutôt qu'une optimisation transparente.
Le bug de collision `_sway` est corrigé. Le sprint reste 🟡 en cours, verrouillé sur la même
dépendance qu'avant (câblage reporté, coordination avec Phase C), avec un blocant
supplémentaire identifié (choix de primitive d'impostor) à lever avant de passer au câblage.

---

## Finalisation du 18 juillet 2026 — blocant levé, câblage fait

Suite à l'audit ci-dessus, le blocant (`MeshKind::Plane` inadapté) a été résolu et le LOD a
été câblé dans `Renderer::render`. Phase C ayant entre-temps été committée (`culling_radius_for`/
`distance_visible` actifs dans `src/gfx/passes.rs`/`renderer.rs`), la coordination attendue
n'était plus un obstacle.

### Nouvelle primitive `MeshKind::Billboard`

Ajout d'un variant `Billboard` à `MeshKind` (`src/scene/mod.rs`) : impostor « croix » — deux
plans verticaux perpendiculaires (normales +Z et +X), base à `y=0`, sommet à `y=1` — généré
par `mesh::billboard_cross()` (`src/gfx/mesh.rs`, nouveau, testé). Technique classique
d'impostor d'herbe/fougères : contrairement à `Plane` (horizontal), une croix verticale
présente toujours une face visible sous un angle de vue à hauteur d'œil, sans calcul de
rotation face-caméra par frame — le pipeline principal a déjà `cull_mode: None`
(`src/gfx/pipelines.rs`), donc les deux plans sont visibles des deux côtés.

Empreinte de l'ajout d'un 7ᵉ variant à `MeshKind` (enum non exhaustif ailleurs dans le code) :
4 sites à mettre à jour, tous des `match` exhaustifs sans bras `_` — trouvés par le compilateur
(`cargo build` a listé les 3 erreurs restantes après l'ajout de l'enum, `hierarchy.rs` inclus) :

- `src/scene/mod.rs` : `ALL` (6→7), `mesh_data()`, `label()`.
- `src/gfx/passes.rs` : `mesh_key()` (clé de tri `6`, avant la plage `100+` des imports).
- `src/scene/queries.rs` : `local_aabb()` (AABB `[-0.5,0,-0.5]`–`[0.5,1,0.5]`, cohérente avec
  la géométrie du billboard).
- `src/editor/hierarchy.rs` : catégorie de hiérarchie (« Impostors », 🌿) — fichier non listé
  parmi les fichiers chauds au moment de l'édition, modification sûre.

Aucun de ces 4 fichiers n'a nécessité de logique nouvelle au-delà d'un bras de `match`
supplémentaire — la primitive est enregistrée automatiquement dans le cache de meshes GPU
via la boucle existante `for kind in MeshKind::ALL` (`src/gfx/pipelines.rs:1312-1314`), aucune
modification nécessaire là.

`foliage_lod_mesh` (`src/gfx/lod.rs`) substitue désormais `MeshKind::Billboard` au lieu de
`MeshKind::Plane` — 7 tests mis à jour et verts.

### Câblage dans `Renderer::render`

`InstanceDraw` (`src/gfx/renderer.rs`) gagne un champ `mesh: MeshKind`, précalculé une fois
par reconstruction du plan de dessin (`foliage_lod_mesh(&app.scene, obj.mesh,
eye.distance(obj.transform.position))`, même position caméra « pure » que le culling par
distance de Phase C, jamais le décalage cosmétique de `write_uniforms`). Les 4 sites de
dessin identifiés dans l'analyse initiale (passe d'ombre, passe principale, et leurs deux
équivalents du rendu headless) lisent maintenant `plan[i].mesh` (effectif, avec LOD) au lieu
de relire `objs[plan[i].obj].mesh` (original) pour grouper les plages contiguës et résoudre
le `GpuMesh` à dessiner — la texture continue de venir de l'objet original
(`objs[...].texture`), donc le billboard échantillonne la même texture que le mesh glTF
complet qu'il remplace.

**Build propre** (`cargo build --lib`, plus aucun warning dead-code sur `lod.rs` ni sur le
champ `mesh` d'`InstanceDraw`), **23 tests `gfx::` verts**, **120 tests `scene::` verts**,
**7 tests `golden_render` verts** (aucune régression visuelle sur les scènes de référence
existantes — attendu, aucune d'elles ne place de feuillage dense au-delà de 40 m de la
caméra), `cargo clippy --lib` et `rustfmt --check` propres sur les 7 fichiers touchés.

### Limite connue, non bloquante : fragmentation du batching

Le tri des instances (`order.sort_by` dans `Renderer::render`) groupe toujours par mesh
**d'origine**, pas par mesh **effectif** (post-LOD) — optimisation « re-tri paresseux »
existante, qui ne recalcule l'ordre que si le nombre d'objets change, pas à chaque frame. Les
instances proches (mesh glTF complet) et lointaines (billboard) d'un même import peuvent donc
alterner dans l'ordre de tri au lieu d'être regroupées en deux plages contiguës — le
regroupement par mesh effectif (fait au dessin, cf. ci-dessus) produit alors plusieurs petits
lots au lieu d'un ou deux gros, avec plus de `draw_indexed` que l'optimum. **Correction
visuelle** : aucune (chaque instance est dessinée avec le bon mesh). **Coût** : draw calls
plus nombreux que l'optimal théorique — acceptable pour ce sprint, dont l'objectif mesuré est
le temps de passe « Scène » (fill-rate, réduit par les billboards) et pas le nombre de draw
calls (objectif de la Phase B, déjà traité séparément). Amélioration possible plus tard : trier
par distance en buckets, en acceptant un re-tri plus fréquent que juste au changement de
nombre d'objets.

### Ce qui reste (non fait dans cette session)

- **Validation visuelle en jeu** : aucun test manuel dans le binaire (`target/debug/motor3derust
  --player` sur `mmorpg_demo`, vue large avec du feuillage à plus de 40 m) — seule la
  géométrie du mesh a été testée unitairement (`billboard_cross_has_two_perpendicular_quads_...`).
  Une autre session faisait déjà tourner une instance du jeu au moment de la finalisation ;
  pas de rebuild/relance faite pour ne pas interférer.
- **Mesure Phase F** : `gpu_draw_calls`/temps de passe « Scène » avant/après sur `mmorpg_demo`
  avec le Profiler — la Phase 0 a déjà mesuré un « avant » (125 FPS, ~382 draw calls, voir
  tableau en tête de `sprintoptimation3daudit10h.md`) ; un « après » ciblé sur cette Phase D
  reste à prendre.
- **Seuil `FOLIAGE_LOD_DISTANCE = 40.0`** : toujours une valeur au jugé, à ajuster si le
  popping (bascule visible mesh complet ↔ billboard) est gênant en jeu.

---

## Contexte au démarrage

Au moment de démarrer cette phase (18 juillet 2026, ~10h00), plusieurs sessions Claude Code
tournaient en parallèle sur ce dépôt (confirmé par `ps aux` : `cargo build`/`rustfmt` actifs,
plusieurs transcripts `.claude/projects/.../*.jsonl` modifiés à la même minute) :

- `src/gfx/passes.rs` était en cours de modification live par une autre session, qui y
  implémentait déjà la **Phase C** (culling par distance, fonctions `culling_radius_for` /
  `distance_visible`, listes de mots-clés `FOLIAGE_LOW_RADIUS_KEYWORDS` /
  `MEDIUM_RADIUS_KEYWORDS`).
- `src/gfx/renderer.rs` et `src/scene/demos.rs` avaient été modifiés dans les minutes
  précédentes (mtime) par d'autres sessions.
- Une tentative de `cargo build --lib` a échoué sur deux erreurs de type dans
  `renderer.rs` (`actions.connect_to_server`, tuple à 2 éléments vs 3 attendus,
  lignes 1815/2001) — **sans rapport avec ce sprint**, signe d'un autre chantier en cours
  d'édition (probablement lié à `PlayerClass`) au moment de la mesure.

Conformément à la mémoire projet sur les sessions concurrentes (cf. `[[concurrent-sessions-hazard]]`),
le choix a été fait de **ne pas éditer `src/gfx/passes.rs`, `src/gfx/renderer.rs` ni
`src/scene/demos.rs`** pendant cette session pour éviter d'écraser ou de percuter le travail
en cours d'une autre session sur ces mêmes fichiers. Le travail de ce sprint a donc été isolé
dans un nouveau fichier (`src/gfx/lod.rs`), sans dépendance d'édition sur les fichiers chauds.

---

## Ce qui a été fait

### Nouveau module `src/gfx/lod.rs`

Fonction pure de décision de LOD, indépendante du pipeline de rendu :

```rust
pub(super) fn foliage_lod_mesh(scene: &Scene, mesh: MeshKind, camera_distance: f32) -> MeshKind
```

- Reconnaît le feuillage dense le plus instancié (mesuré dans `optimisation3D.Analys.md`) :
  `nature_grass_tuft.glb` (×112), `nature_fern.glb` (×69), `nature_reeds.glb` (×19), via
  sous-chaîne du `path` de l'import (`FOLIAGE_LOD_KEYWORDS`).
- Au-delà de `FOLIAGE_LOD_DISTANCE = 40.0` mètres de la caméra, substitue `MeshKind::Plane`
  (primitive déjà présente dans le cache de meshes GPU, aucun nouvel asset à charger) au mesh
  glTF complet — un impostor plan bon marché plutôt qu'un billboard dédié, suffisant pour du
  feuillage bas et proche du sol.
- Inchangé pour : primitives codées, imports hors liste, distance ≤ seuil.
- **6 tests unitaires** couvrant : proximité (mesh conservé), distance (substitution herbe/
  fougère/roseaux), non-feuillage à distance (inchangé), primitives (jamais substituées),
  cas limite exactement au seuil (mesh conservé — `<=`, pas de flottement à la frontière).

Le module est déclaré `pub(crate) mod lod;` dans `src/gfx/mod.rs` (seule modification hors
`lod.rs` — fichier non touché par les autres sessions au moment de l'édition, donc sûr).

Choix délibéré de réutiliser une primitive existante (`Plane`) plutôt que de générer un nouvel
asset `.glb` simplifié : livrable plus vite, zéro dépendance sur le pipeline Blender headless
pour ce premier passage, cohérent avec le risque de conflit de fichiers déjà élevé sur cette
session.

### Ce qui n'a **pas** été fait : le câblage dans `Renderer::render`

Le point d'intégration identifié (non modifié) : dans `src/gfx/renderer.rs`, la boucle de
construction du plan de dessin (~ligne 1317-1340, `sync_objects`/`render`) résout le mesh d'un
objet directement via `objs[i].mesh`, et **4 sites de dessin distincts** relisent ce même champ
pour grouper les plages contiguës et choisir le `GpuMesh` :

- passe principale (~ligne 2368-2405)
- passe d'ombre (~ligne 2298 et environs)
- rendu headless (~ligne 2605)
- un quatrième site (~ligne 2682)

Câbler la substitution proprement demande :

1. Ajouter un champ `effective_mesh: MeshKind` à `InstanceDraw` (ou équivalent), calculé une
   fois par frame à partir de `foliage_lod_mesh(scene, obj.mesh, eye.distance(obj.transform.position))`.
2. Remplacer les 4 lectures de `objs[plan[i].obj].mesh` par ce champ précalculé — **et** revoir
   la clé de tri/groupement (`order.sort_by` sur `mesh_key`, ligne ~1304-1310), qui groupe
   actuellement par mesh *avant* LOD : un lot d'herbe proche/lointaine mélangé casserait le
   batching en plages contiguës si le tri ne suit pas.
3. Vérifier l'invariant de re-tri paresseux (« l'ordre ne dépend pas des transforms », commentaire
   ligne ~1297-1300) : il devient faux dès que le LOD dépend de la position caméra — soit
   accepter un tri par frame pour les objets concernés uniquement, soit quantifier en
   « buckets » de distance et ne re-trier que quand un objet change de bucket (préférable pour
   les perfs, non implémenté ici).

Ce périmètre touche exactement les fonctions que la session Phase C modifie en ce moment dans
`passes.rs`/`renderer.rs` (culling par distance, même famille de problème : distance caméra
→ décision par instance). **Recommandation** : câbler Phase C et Phase D ensemble, dans une
session dédiée une fois Phase C committée, plutôt qu'en parallèle sur les mêmes fonctions —
la Phase C a déjà l'infrastructure de calcul de distance caméra↔objet dont Phase D a besoin
(`distance_visible`, `culling_radius_for` dans `passes.rs`), autant la réutiliser au lieu de la
dupliquer dans `lod.rs`.

---

## Vérification

- `src/gfx/lod.rs` : nouveau fichier, 6 tests unitaires (`cargo test --lib gfx::lod::`).
- Build complet (`cargo build --lib`) **bloqué** au moment de la rédaction par une erreur
  préexistante et sans rapport dans `renderer.rs` (`connect_to_server`, tuple 2 vs 3 éléments,
  probablement liée à un chantier `PlayerClass` en cours dans une autre session) — à re-tester
  une fois cette erreur résolue par la session qui l'a introduite. La lecture du message
  d'erreur de `rustc` confirme qu'aucune erreur n'a été signalée dans `lod.rs` lui-même.

## Prochaines étapes (pour la session qui reprend cette phase)

1. Attendre/confirmer que `cargo build --lib` compile de nouveau (dépend d'une autre session).
2. Lancer `cargo test --lib gfx::lod::` pour confirmer les 7 tests verts en isolation.
3. **Bloquant avant tout câblage** (trouvé à l'audit du 18 juillet) : résoudre le choix de
   primitive d'impostor — `MeshKind::Plane` est un plan de sol horizontal, pas un billboard
   vertical, inadapté au feuillage debout. Deux options : (a) ajouter une primitive de
   billboard vertical face-caméra à `MeshKind` (`src/scene/mod.rs`) + logique d'orientation
   dynamique dans `Renderer::render`, ou (b) réutiliser un mesh existant plus proche d'un
   quad vertical (`Cube` aplati en profondeur, par ex.) le temps d'une vraie primitive dédiée.
   Voir la section « Audit » ci-dessus pour le détail.
4. Une fois Phase C committée (culling par distance dans `passes.rs`), câbler
   `foliage_lod_mesh` dans `Renderer::render` selon le plan ci-dessus, idéalement en réutilisant
   le calcul de distance déjà fait par Phase C plutôt qu'un second calcul redondant.
5. Livrable final de la Phase D (inchangé du plan) : temps de passe « Scène » réduit en vue
   large avec beaucoup de feuillage visible, sans changement perceptible en vue rapprochée —
   à mesurer avec le Profiler (`gpu_draw_calls`, temps de passe) une fois câblé.
