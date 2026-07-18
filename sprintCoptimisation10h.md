# Sprint C — Culling par distance (Phase C, `sprintoptimation3daudit10h.md`)

> Compte-rendu du Sprint 4 (« Rayon de culling par type d'objet »), Phase C du plan
> [sprintoptimation3daudit10h.md](sprintoptimation3daudit10h.md). Statut : **fait**.

## Contexte

Le plan de sprints d'optimisation 3D définit la Phase C comme indépendante des Phases A/B/D/E
(après la Phase 0), avec pour but de réduire la charge en vue large/plongée sans occlusion
culling complet, via un rayon de culling par catégorie d'objet, en complément du frustum
culling déjà en place.

## Ce qui a été fait

- Ajout de `culling_radius_for(scene, mesh) -> Option<f32>` dans
  [src/gfx/passes.rs](src/gfx/passes.rs) : catégorise un mesh importé par sous-chaîne de son
  chemin de fichier (`ImportedMesh::path`) en 3 groupes, conformément au plan :
  - **Feuillage bas** (herbe/fougères/fleurs/cultures — `nature_grass_tuft`, `nature_fern`,
    `nature_reeds`, `nature_flowers`, etc.) → rayon court, **45 m**.
  - **Arbres/rochers** (`nature_oak`, `nature_pine`, `nature_rock`, `nature_bush`, etc.) →
    rayon moyen, **110 m**.
  - **Bâtiments/créatures et tout le reste** (non reconnu par les listes ci-dessus, y compris
    les créatures skinnées `creature*.glb`) → **pas de limite** (`None`), conformément au plan
    (« bâtiments/créatures : rayon large ou pas de limite »).
  - Une primitive codée (`MeshKind::Cube`, etc., pas de mesh importé) n'a jamais de limite.
- Ajout de `distance_visible(eye, world_pos, radius)` dans le même fichier : `true` si la
  distance à la caméra est sous le rayon, toujours `true` si `radius = None`.
- Câblage dans [src/gfx/renderer.rs](src/gfx/renderer.rs), dans la boucle qui construit le
  plan de dessin des objets **statiques** (celle qui alimentait déjà `aabb_visible` pour le
  frustum) : `visible = obj.visible && distance_visible(...) && aabb_visible(...)`. Appliqué
  **avant** le tri en plages contiguës, comme demandé par le plan.
  - La position caméra utilisée est `app.camera.eye()` (position « pure »), **pas** le
    décalage cosmétique de recul caméra (`camera_shake_offset`, Sprint 1 d'un autre chantier)
    qui n'affecte que le rendu (`write_uniforms`), jamais la visibilité — sinon un léger recul
    caméra ferait clignoter des objets proches du seuil de rayon.
  - Le cache de reconstruction du plan de dessin (`render_input_hash`) hash déjà
    `app.camera.view_proj()`, qui change avec la position caméra → le culling par distance se
    remet à jour à chaque déplacement caméra sans changement supplémentaire au cache.
- **Non appliqué** à la boucle des objets **skinnés** (`draw_plan_skinned`) : ce sont les
  créatures/PNJ, catégorie « pas de limite » du plan — `culling_radius_for` y renverrait de
  toute façon `None` puisque les fichiers `creature*.glb`/packs menagerie ne correspondent à
  aucune des deux listes de mots-clés, donc le culling par distance ne change rien pour eux ;
  laissé hors de la boucle skinnée pour ne pas toucher au code de la Phase A/B (capacité
  skinnée), en cours dans une autre session sur ce même dépôt.

## Preuve (tests)

Trois tests dans [src/gfx/passes.rs](src/gfx/passes.rs) (`gfx::passes::culling_distance_tests`),
le troisième ajouté lors de l'audit a posteriori (voir plus bas) :

- `categorizes_foliage_trees_and_unbounded_correctly` : vérifie que l'herbe a un rayon plus
  court que les arbres, qu'un bâtiment et une créature n'ont aucune limite, et qu'une
  primitive codée n'a jamais de limite.
- `distance_visible_respects_radius_and_none_is_unbounded` : vérifie qu'un objet proche reste
  visible, qu'un objet lointain est coupé avec un rayon fini, et qu'un rayon `None` ne coupe
  jamais rien même à grande distance.
- `rocking_chair_is_not_matched_by_rock_keyword` : non-régression du bug de catégorisation
  trouvé et corrigé pendant l'audit (voir « Audit a posteriori du sprint »).

```
$ cargo test --lib culling_distance_tests
running 3 tests
test gfx::passes::culling_distance_tests::distance_visible_respects_radius_and_none_is_unbounded ... ok
test gfx::passes::culling_distance_tests::categorizes_foliage_trees_and_unbounded_correctly ... ok
test gfx::passes::culling_distance_tests::rocking_chair_is_not_matched_by_rock_keyword ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 540 filtered out; finished in 0.00s
```

## Audit a posteriori du sprint

Relecture ciblée après la première livraison, en ne touchant que les fichiers propres à ce
sprint (`src/gfx/passes.rs` + zone culling de `src/gfx/renderer.rs`) pour ne rien croiser avec
les autres phases en cours ailleurs dans le dépôt.

- **Bug trouvé et corrigé** : la catégorisation par sous-chaîne (`path.contains(k)`) faisait
  matcher le mot-clé `rock` (« arbres/rochers ») dans `nature_rocking_chair.glb` (un meuble de
  jardin, pas un rocher) — vérifié en confrontant systématiquement les deux listes de
  mots-clés à la liste complète des ~110 assets `nature_*.glb` du dépôt (script one-off, pas
  conservé). C'était le **seul** faux positif parmi tous les assets existants ; toutes les
  autres correspondances (arbres, feuillage bas) étaient correctes.
  - **Correctif** : remplacement du test par sous-chaîne par `contains_word()`, qui exige une
    frontière de mot (début/fin de chaîne ou caractère non alphanumérique) de part et d'autre
    du mot-clé — `nature_rock.glb` matche toujours `rock`, `nature_rocking_chair.glb` non.
  - **Test de non-régression ajouté** : `rocking_chair_is_not_matched_by_rock_keyword`.
  - Vérifié que ce changement ne modifie **aucune** autre catégorisation existante (script de
    confrontation ré-exécuté après le correctif sur les ~110 assets, résultat identique sauf
    pour la chaise).
- **Reste correct/inchangé après relecture** :
  - `distance_visible` : distance au carré (pas de `sqrt` inutile), `None` toujours visible —
    logiquement correct, confirmé par les tests.
  - Position caméra utilisée (`app.camera.eye()`, pas le décalage de shake) — toujours le bon
    choix, aucune régression détectée dans `write_uniforms` (zone d'un autre chantier, non
    touchée).
  - Le cache `render_input_hash` reste valide : il hash `view_proj`, donc tout déplacement
    caméra invalide bien le plan de dessin — pas de culling par distance figé sur une position
    caméra périmée.
  - Exclusion volontaire de la boucle skinnée toujours justifiée : aucun asset créature ne
    matche les mots-clés, `culling_radius_for` y renverrait `None` de toute façon.
- **Reste non calibré/non mesuré** (déjà signalé dans « Ce qui n'a PAS été fait » ci-dessous,
  confirmé toujours vrai après cette relecture) : aucune mesure Profiler en jeu n'a été faite,
  les rayons 45 m/110 m restent un point de départ non validé en conditions réelles, et
  plusieurs assets de décor peu instanciés (crops, mobilier de jardin, petits objets) ne sont
  reconnus par aucun mot-clé et restent donc en catégorie « pas de limite » — cohérent avec le
  plan qui ciblait d'abord les meshes les plus instanciés (`nature_grass_tuft` ×112,
  `nature_fern` ×69), déjà couverts, mais une extension future des listes de mots-clés reste
  possible si de nouveaux assets denses apparaissent.

**Conclusion de l'audit** : le sprint n'était pas « tout parfait » — un vrai bug de
catégorisation existait, il a été trouvé, corrigé et couvert par un test dans le cadre de ce
même sprint (aucun fichier d'une autre phase touché). Le reste de la logique livrée est
correct. Le point non fermé (mesure Profiler réelle + calibrage des rayons) reste identique à
ce qui était déjà documenté avant cet audit — pas un oubli nouveau, mais toujours la tâche
restante la plus importante avant de considérer la Phase C totalement « verte » au sens du
plan.

## Ce qui n'a PAS été fait (hors scope de ce sprint)

- **Pas de mesure en jeu** (`gpu_draw_calls`/temps de passe « Scène » avant/après) : le plan
  demande d'itérer avec le Profiler ouvert pour trouver le compromis distance/qualité — à
  faire en lançant le binaire desktop sur `mmorpg_demo`, vue large/plongée, comme la Phase 0.
  Les rayons ci-dessus (45 m / 110 m) sont un point de départ raisonnable (herbe rarement
  utile à voir au-delà de quelques dizaines de mètres) mais **non calibrés en conditions
  réelles** — à ajuster si popping visible gênant constaté.
- **Pas de fondu/transition** à l'entrée/sortie du rayon : un objet disparaît net au
  franchissement du seuil (comme le frustum culling existant). Si le popping est gênant en
  test manuel, un fondu alpha serait un sprint de suivi, pas ajouté ici pour rester dans le
  scope du plan (« rayon de culling », pas « transition »).
- **Pas de re-catégorisation générique** au-delà des mots-clés `nature_*` déjà présents dans
  `src/scene/demos.rs` : si de nouveaux packs d'assets avec d'autres préfixes sont ajoutés
  plus tard, il faudra étendre `FOLIAGE_LOW_RADIUS_KEYWORDS`/`MEDIUM_RADIUS_KEYWORDS`.

## Fichiers touchés

- [src/gfx/passes.rs](src/gfx/passes.rs) — `culling_radius_for`, `distance_visible`, listes de
  mots-clés, constantes de rayon, tests.
- [src/gfx/renderer.rs](src/gfx/renderer.rs) — import des deux nouvelles fonctions, câblage
  dans la boucle de construction du plan de dessin statique.

## Remarque sur le dépôt (session concurrente)

Au moment de ce sprint, plusieurs autres sessions Claude Code étaient actives sur ce même
dépôt (fichiers `src/editor/windows.rs`, `src/app/*`, `src/net/*`, `src/gfx/renderer.rs`
zone `write_uniforms`/minimap déjà modifiés hors de ce sprint, et un nouveau fichier
`src/gfx/lod.rs` apparu en cours de route — vraisemblablement la Phase D en parallèle,
cohérent avec le plan qui autorise A/B/C/D/E en parallèle). `cargo build` a temporairement
échoué à cause d'un changement en cours ailleurs (`PlayerClass` non propagé partout) puis a
recompilé sans erreur une fois cet autre chantier stabilisé — aucune interférence constatée
avec les fichiers propres à ce sprint (`passes.rs`, zone culling de `renderer.rs`). Voir la
mémoire *Sessions concurrentes sur ce dépôt* : ne pas committer en bloc, isoler les hunks de
ce sprint si un commit est demandé.
