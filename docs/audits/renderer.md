# `src/gfx/renderer.rs`

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans le fichier.
C'est le fichier le plus dense en tags `(Sprint N)` du projet (~68 occurrences
avant nettoyage) : la quasi-totalité était de l'attribution collée à une doc
technique par ailleurs valable, pas des récits de bugs — le tri a surtout
consisté à couper le tag en gardant le POURQUOI juste derrière.

## Attribution par sprint

- **Sprint 80** — Rendu **headless** (`Renderer::new_headless`,
  `render_scene_headless`) : mêmes shaders/pipelines que `render()`, sans
  fenêtre/surface/UI — fondation des golden tests de non-régression visuelle
  (`tests/golden_render.rs`).
- **Sprint 83** — Debug drawing (`debug_line`/`debug_box`/`debug_sphere`,
  buffer dédié redimensionnable) + sélecteur de vue (Éclairé/Normales/
  Profondeur, encodé dans `ambient.y` de `SceneUniform`).
- **Sprint 86** — Skinning GPU : `SkinnedVertex`, `shaders/skinned.wgsl`
  (fragment partagée avec `main.wgsl`), palette de joints en storage buffer
  (groupe 4, capacité 128). `render_skinned_test` : chemin headless dédié à un
  seul mesh, pas encore branché sur la scène générale à ce stade.
- **Sprint 87** — Intégration Play : `draw_plan_skinned`/
  `prepare_skinned_draws`/`draw_skinned_objects` branchent enfin le skinning
  sur `render()`/`render_scene_headless()` ; `MAX_SKINNED_INSTANCES = 8`
  (offset dynamique dans `joint_buf`, un créneau par instance) ; fondu
  enchaîné (`compute_joint_matrices_blended`, mélange des poses locales).
- **Sprint 89** — Ciel (`shaders/sky.wgsl`, triangle plein écran, direction de
  vue reconstruite via `Camera::inv_view_proj`) + brouillard exponentiel
  (`SceneUniform::fog`).
- **Sprint 90** — Cible de rendu HDR (`HDR_FORMAT = Rgba16Float`) + tone
  mapping ACES (`shaders/tonemap.wgsl`, approximation de Narkowicz 2015) :
  les 5 pipelines de la passe principale dessinent dans `hdr_view` avant la
  conversion vers `config.format`.
- **Sprint 91** — Bloom : seuil + chaîne de mips down/upsample
  (`shaders/bloom.wgsl`, `BLOOM_MIP_LEVELS = 4`), composé dans `tonemap()`.
  Réglages doublés (`Sky::bloom_intensity` + `BuildConfig::bloom`) et
  opt-out mobile (`RenderQuality::bloom_enabled`, passes GPU sautées).
- **Sprint 92** — Mipmaps générées à l'import (`shaders/mipgen.wgsl`, blits
  chaînés, `mip_count_for`) + tangentes pour le normal mapping.

## Bugs réels trouvés en testant

- **Golden test de skinning validé par régression injectée (Sprint 86)** :
  la planche à charnière pondérée (moitié joint fixe, moitié joint pivotant)
  a d'abord été vérifiée à l'œil à 0°/45°/-90°, avant d'être figée en golden
  test. Pour prouver que le test détectait vraiment une régression et pas
  seulement une différence de rendu triviale, un bug a été injecté puis
  reverté : 5,51 % de pixels divergents avant correctif, confirmant que le
  harnais golden réagit à un vrai changement de skinning, pas à du bruit.
- **Golden tests de rendu, même exercice (Sprint 80)** : après avoir régénéré
  le golden de référence sur GPU Metal, une régression a été injectée dans
  `main.wgsl` (le shader d'éclairage) pour vérifier que le test échouait
  effectivement, avant d'être revertée. Sans cette étape, un golden test qui
  passe toujours ne prouve rien — il aurait pu être cassé dès le départ
  (comparaison qui ne compare rien, seuil de tolérance trop large, etc.).
- **Vue de debug « Profondeur » illisible au premier essai (Sprint 83)** : la
  profondeur brute (NDC, near/far réel de la caméra 0.1..100) écrasait toute
  scène compacte dans le même blanc — aucune information visuelle utile.
  Corrigé en linéarisant sur une échelle visuelle fixe de 20 m plutôt que sur
  les plans near/far réels, ajusté après un premier rendu peu lisible.
- **Champ `skinned_pipeline` avec un commentaire devenu faux** : le
  commentaire du groupe de champs (« palette de joints ») affirmait encore
  « pas encore branché sur la boucle de rendu de scène générale » alors que
  l'intégration Play du Sprint 87 (`draw_plan_skinned`) l'a branché depuis
  longtemps sur `render()`/`render_scene_headless()`. Un commentaire qui
  décrit un état de sprint plutôt que l'invariant courant devient trompeur
  dès que le code évolue sans que quelqu'un pense à le relire — corrigé au
  passage de ce nettoyage (Sprint 103a-3) en décrivant l'état actuel plutôt
  que l'historique d'intégration.
