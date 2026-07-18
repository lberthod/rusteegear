# Sprint B — Instancing GPU du skinning (Phase B) — rapport

> Exécution de la **Phase B** (Sprints 2 et 3) de
> [sprintoptimation3daudit10h.md](sprintoptimation3daudit10h.md#phase-b), elle-même issue de
> [optimisation3D.Analys.md](optimisation3D.Analys.md). Phase déclarée indépendante de A/C/D/E —
> seuls `src/scene/queries.rs` et `src/scene/demos.rs` ont bougé (aucun conflit avec les sessions
> concurrentes qui, au moment de ce sprint, travaillaient sur la Phase C
> ([sprintCoptimisation10h.md](sprintCoptimisation10h.md), terminée) et la Phase D
> ([sprintD_optimisation10h.md](sprintD_optimisation10h.md), en cours — toutes deux dans
> `src/gfx/passes.rs`/`renderer.rs`, zone non touchée ici).

## Statut : Sprint 2 ✅ fait — Sprint 3 ⚠️ descopé (décision documentée, pas d'implémentation)

---

## Sprint 2 — Isoler le décor animé « statique en place »

**Objectif du plan** : distinguer les vraies créatures IA (mouvement, combat) du décor animé qui
ne fait que jouer un clip `Idle` sur place, candidats naturels à l'instancing GPU du skinning.

### Ce qui a été fait

**Catégorisation (`src/scene/queries.rs`)** — deux méthodes sur `Scene`, calculées plutôt que
stockées (pas de nouveau champ sur `SceneObject`, pas de churn de format de scène pour un dérivé
stable) :
- `is_skinned_mesh(mesh)` : `true` si le mesh importé a un squelette
  (`ImportedMesh::skeleton.is_some()`) — même règle que `gfx::passes::is_skinned`, dupliquée ici
  car cette dernière est `pub(super)` à `gfx` (le renderer n'a pas à être une dépendance de la
  catégorisation gameplay/scène).
- `is_static_skinned_decor(obj)` : `true` si le mesh est skinné **et** que l'objet n'a ni
  `AiChaser` ni script (`obj.script.is_empty()`) — le signal de « bouge/patrouille » déjà utilisé
  ailleurs dans le moteur, pas un nouveau concept.

**Preuve (`src/scene/demos.rs`, test
`mmorpg_demo_static_skinned_decor_has_no_duplicate_mesh`)** — construit `Scene::mmorpg_demo()` et
mesure :

| Catégorie | Nombre | Détail |
|---|---|---|
| Objets skinnés au total | **201** | confirme exactement le chiffre de `optimisation3D.Analys.md` |
| Actifs (IA/script, non éligibles) | **61** | 26 `MMORPG_CREATURES` (script de patrouille par raycast) + 35 `MMORPG_AMBIENT_FAUNA_SPAWNS` (« Errant N », labellisées « décoratives » dans le code mais scriptées `creature_wander_script` — donc bien actives, exclues) |
| Décor statique éligible | **140** | `NATURE_DECOR` (mécanismes animés : moulins, forge, balançoires…) + `MONSTER_DECOR` (menagerie, 45 monstres posés en Idle) + quelques entrées `anim: None` dont le glb porte quand même un squelette inutilisé |

Les comptages sont verrouillés par assertions (`active_count == 61`), pas juste affichés — un
changement de contenu qui reclasserait une créature par erreur ferait échouer le test
immédiatement.

### Écart avec le plan

Le plan attendait ~90 objets « Idle only » (mon estimation initiale par lecture statique du code,
limitée aux entrées avec `anim: Some("Idle")`). Le chiffre réel mesuré est 140 : `is_skinned_mesh`
suit la présence du squelette, pas celle d'un clip actif — certains décors sans animation jouée
(`anim: None`) ont quand même un glb riggé (chargé sans conséquence visuelle, juste comptabilisé
« skinné » par le renderer). Le test capture le comportement réel du moteur, pas l'intention du
plan.

---

## Sprint 3 — Instancing GPU-skinning : décision de ne pas implémenter

**Objectif du plan** : rendre le décor identifié en Sprint 2 via un chemin de rendu instancié
(palette de joints échantillonnée par instance) plutôt qu'un `draw_indexed` par objet, pour
réduire les draw calls skinnés.

### Le constat qui change la donne

L'instancing GPU du skinning ne réduit un draw call que s'il existe **au moins deux instances du
même mesh** partageant une palette de joints (ou un lot de palettes) — exactement comme le
batching statique existant (`renderer.rs:2375-2386`) ne fusionne que des plages contiguës du
**même** `(mesh, texture)`.

Or le test du Sprint 2 mesure, parmi les 140 objets éligibles :

```
max_instances = 1   (aucun mesh instancié plus d'une fois)
```

Chaque `monster_*.glb` (menagerie), `fauna_*.glb` et mécanisme `nature_*.glb` animé n'est posé
**qu'à un seul endroit** dans `mmorpg_demo()` — pas de doublon. Une palette de joints partagée
entre instances n'a donc rien à partager : instancier ce sous-ensemble produirait exactement le
même nombre de draw calls qu'aujourd'hui (un par objet), pour un coût d'implémentation non nul
(nouveau chemin de rendu, shader, bind groups).

**Vérification de l'hypothèse implicite du plan** — `optimisation3D.Analys.md` cite
`nature_grass_tuft.glb` ×112 et `nature_fern.glb` ×69 comme « meshes les plus instanciés »,
suggérant qu'ils seraient de bons candidats. Vérifié dans
[scripts/blender/gen_nature_pack.py](scripts/blender/gen_nature_pack.py) (`gen_grass_tuft`,
`gen_fern`) : ce sont des cônes/blobs générés sans armature — **des primitives statiques**, pas
skinnées. Leurs ×112/×69 instances sont déjà couvertes par le batching statique existant (et par
la Phase D, LOD géométrique, en cours en parallèle). Le plan avait involontairement mélangé deux
chantiers distincts (instancing statique déjà résolu vs. instancing skinné, sujet réel de ce
sprint) sous une même intuition « meshes très instanciés ».

### Décision

**Ne pas implémenter le chemin de rendu instancié tel que spécifié** — construire un shader et un
bind-group layout supplémentaires pour un gain de 0 draw call mesuré serait le contraire de ce que
demande le reste du projet (pas de complexité pour un besoin hypothétique). Le sprint est descopé,
pas abandonné silencieusement : la décision et sa preuve sont documentées et testées.

**Garde-fou pour la reprise future** : si une prochaine passe de contenu duplique un mesh skinné
décoratif (ex. plusieurs `fauna_sheep.glb` dans un troupeau), le test
`mmorpg_demo_static_skinned_decor_has_no_duplicate_mesh` échoue explicitement
(`max_instances > 1`, avec le compte exact) — c'est le signal qui doit rouvrir ce sprint, pas une
inspection manuelle périodique.

### Design retenu pour une reprise future (non implémenté)

Recherche menée sur le chemin de rendu skinné actuel (`src/gfx/renderer.rs`,
`src/gfx/shaders/skinned.wgsl`) pour ne pas repartir de zéro le jour où ce sprint redevient
pertinent :

- **Buffer/bind group actuels** : un stockage `joint_buf` (`JOINT_SLOT_BYTES * MAX_SKINNED_INSTANCES`)
  avec offset dynamique par instance (group 1, binding 1, `pipelines.rs:993-1017`) ; `models[]`
  (binding 0) donne la transform par instance via `@builtin(instance_index)`. `draw_skinned_objects`
  fait aujourd'hui un `draw_indexed` par instance avec son propre offset de palette
  (`renderer.rs:853-890`).
- **Piège produit identifié** : `src/scene/demos.rs:4928-4934` décale volontairement la phase de
  départ de chaque instance d'un même clip (`time: anim_count as f32 * 0.37`, commentaire *« deux
  instances du même clip ne pulsent jamais à l'unisson »*) — une palette de joints strictement
  partagée entre toutes les instances d'un mesh les remettrait en lockstep, régression visuelle
  directe.
- **Design recommandé si repris** : ne pas viser une palette 100 % unifiée par mesh, mais des
  **lots `(mesh, texture, phase quantifiée)`** — regrouper les instances par tranche de phase (4 à
  8 buckets suffisent à casser l'effet lockstep à l'œil) ; chaque lot obtient une palette calculée
  une fois et un `draw_indexed` en plage contiguë, comme le batching statique. Le buffer/bind group
  existants n'ont pas besoin de changer de structure (toujours un offset dynamique par « groupe »
  plutôt que par instance) — le travail réel serait le tri des instances en lots contigus dans
  `models[]` avant écriture, symétrique à ce que fait déjà le chemin statique.
- **Option écartée** : palette de joints par texture indexée par instance (aucun compromis de
  phase, mais complexité shader/bind-group nettement plus élevée) — pas justifiée pour une
  animation d'idle discrète, à réserver si un jour un besoin de variation par-instance plus riche
  apparaît.

---

## Audit du sprint (relecture après clôture)

Relecture demandée du travail ci-dessus, sans toucher aux fichiers des autres phases
(`src/gfx/renderer.rs`, `src/gfx/passes.rs`, réservés aux Phases A/C/D en cours). Verdict :
**Sprints 2 et 3 sont corrects et complets tels que scopés**, mais l'audit a trouvé une piste
d'optimisation réelle, adjacente au sujet du sprint, non couverte par le plan d'origine.

### Ce qui est confirmé solide
- Les comptes (201/61/140) et l'absence de doublon de mesh sont verrouillés par test, pas
  seulement affirmés dans ce rapport — reproductible par `cargo test`.
- La catégorisation (`is_static_skinned_decor`) n'a pas de faux négatif évident : tous les champs
  de `SceneObject` pouvant indiquer un mouvement actif (`ai_chaser`, `script`) sont couverts ; les
  autres champs de mouvement potentiels (`controller`, `wind`) concernent le joueur ou une
  translation cosmétique, pas la logique qui justifierait d'exclure un objet du partage de palette
  de joints. Le chemin `poser` (`src/scene/demos.rs`) qui construit `NATURE_DECOR`/`MONSTER_DECOR`
  ne fixe jamais `ai_chaser`/`script`, confirmé par lecture directe.
- La décision de ne pas implémenter Sprint 3 est correcte : 0 duplication mesurée = 0 draw call à
  gagner par l'instancing tel que spécifié. L'hypothèse du plan (`nature_grass_tuft`/`nature_fern`
  comme candidats) est bien fausse, vérifiée dans le générateur Blender (primitives sans armature).

### Ce qui manquait — nouvelle piste trouvée pendant l'audit
En creusant l'écart 140 vs. l'estimation initiale de ~90 (déjà noté dans le rapport), l'audit
sépare les 140 éligibles en deux groupes :

| Sous-groupe | Nombre | Détail |
|---|---|---|
| Jouent réellement un clip (`animation: Some`) | **90** | `NATURE_DECOR` (mécanismes) + `MONSTER_DECOR` (menagerie) — l'ensemble visé par l'intention originale du plan |
| Squelette présent mais **`animation: None`** | **50** | `VILLAGE_PROPS` : étals (« Étal des vivres », « Table d'apothicaire », « Coin trésor »), établi d'armes, mobilier du hameau (fontaine, cadran solaire, nichoir…), quelques éléments « exotiques »/« Rive du lac » nommés `*_sway`/`*_bob` |

Verrouillé par un nouveau test, `mmorpg_demo_has_static_skinned_decor_that_never_animates`
(`src/scene/demos.rs`).

**Pourquoi ces 50 ont un squelette** : `scripts/blender/gen_items_pack11_20.py` (le générateur de
`VILLAGE_PROPS` : armes, nourriture, trésors, mobilier) rigge et bake systématiquement un clip
`Idle` pour **tout** ce qu'il exporte — même les objets qui n'en ont pas l'usage — mais
`src/scene/demos.rs` n'active jamais ce clip pour ces entrées (`anim: None` dans leur
`DemoDecor`). Le squelette existe donc dans le fichier `.glb`, `is_skinned_mesh` le détecte, mais
rien ne l'anime jamais.

**Pourquoi c'est un problème de rendu, indépendamment de l'instancing** : `gfx::passes::is_skinned`
(et sa réplique `Scene::is_skinned_mesh` de ce sprint) ne teste que la présence du squelette, pas
si `AnimationState` est actif. Le renderer route donc ces 50 objets vers `draw_skinned_objects` —
un `draw_indexed` par objet, une palette de joints calculée chaque frame (`compute_joint_matrices`)
et un emplacement des 160 `MAX_SKINNED_INSTANCES` consommé — pour un résultat visuel **strictement
identique** à un rendu statique en pose de liaison (aucune anim jamais jouée). Le code source des
deux chemins de rendu partage la même géométrie de base (`ImportedMesh::data.vertices`,
`load_skinning()` ne fait qu'ajouter les poids de peau par-dessus sans la modifier) — rien
n'indique de blocage technique à rendre ces 50 objets par le chemin statique batché à la place.

**Impact potentiel** (non mesuré en jeu, à confirmer en Phase 0/F) : basculer ces 50 objets sur le
chemin statique ferait passer les skinnés réels de 201 à **151**, sous la capacité actuelle de 160
— cela réduirait, voire éliminerait entièrement, le besoin du pansement de la Phase A
(`MAX_SKINNED_INSTANCES` à relever), et libérerait 50 draw calls (candidats au batching statique
`(mesh, texture)` existant, plusieurs de ces items étant potentiellement réutilisés ailleurs).

**Non implémenté ici, volontairement** : la correction toucherait `src/gfx/renderer.rs` (routage
`is_skinned`/`draw_skinned_objects` vs. plan de dessin statique) — un fichier partagé avec les
Phases A et C actuellement en cours dans d'autres sessions. L'implémenter maintenant risquerait un
conflit de merge sur une zone que ce sprint n'a pas mandat de toucher. La piste est documentée et
verrouillée par test pour qu'un futur sprint (probablement une extension de la Phase A, puisqu'il
s'agit du même fichier et du même sujet « capacité skinnée ») puisse la reprendre directement.

---

## Vérification

- `cargo test --lib scene::` : 104 passés, 0 échec, 4 ignorés avant l'audit ; 105 passés après
  ajout du test d'audit (`mmorpg_demo_has_static_skinned_decor_that_never_animates`).
- `cargo test --lib mmorpg_demo_static_skinned_decor_has_no_duplicate_mesh -- --nocapture` : vert,
  confirme `skinned=201 eligible=140`.
- `cargo test --lib mmorpg_demo_has_static_skinned_decor_that_never_animates` : vert, confirme
  `never_animates=50`.
- `cargo clippy --lib --tests` : aucun avertissement sur les fichiers touchés
  (`src/scene/queries.rs`, `src/scene/demos.rs`).
- `cargo fmt --check` : aucune différence sur les fichiers touchés (les diffs restants dans
  `src/gfx/passes.rs`/`pipelines.rs`/`texcompress.rs` sont préexistants, hors périmètre de ce
  sprint, propriété des sessions concurrentes sur les Phases C/D).
- Aucun changement de rendu : ce sprint n'a touché aucun fichier de `src/gfx/`, donc rien à
  vérifier visuellement (pas de prévisualisation lancée pour ce rapport).

## Suite

- **Phase B est verte** au sens du plan : Sprint 2 livré, Sprint 3 décidé (descopé avec preuve),
  aucune tâche bloquée en suspens.
- Le seul travail restant *dans le périmètre strict de la Phase B* est **conditionnel** : relancer
  le test de Sprint 2 si du contenu skinné dupliqué est ajouté à `mmorpg_demo`, et si c'est le cas,
  reprendre Sprint 3 avec le design de lots par phase ci-dessus.
- **Nouvelle piste hors périmètre de ce sprint** (trouvée par l'audit, décrite ci-dessus) : 50
  objets `VILLAGE_PROPS` payent le coût du rendu skinné sans jamais animer — correction possible
  dans `src/gfx/renderer.rs`, à traiter par un sprint dédié (probablement rattaché à la Phase A,
  même fichier et même sujet). Verrouillée par
  `mmorpg_demo_has_static_skinned_decor_that_never_animates`.
- **Phase F** (validation finale) reste bloquée sur A/C/D/E comme documenté dans le plan — ce
  sprint ne change pas cette dépendance.
