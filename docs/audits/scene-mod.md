# `src/scene/mod.rs`

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans le fichier.

Note : ce fichier ne couvre que `src/scene/mod.rs` lui-même (`Transform`,
`SceneObject`, `Controller`, `Combat`, `Scene`, etc.). Les modules frères issus
du même découpage (Sprint 103a-2) — `demos.rs`, `queries.rs`,
`persistence.rs`, `prefab.rs`, `import.rs` — ont leur propre historique.

## Attribution par sprint

- **Sprint 41** — Composants mobiles Android (`Controller` : boutons tactiles,
  joystick).
- **Sprint 84** — Import glTF skinné : `ImportedMesh::skeleton`/`vertex_skins`,
  `ImportedMesh::load_skinning`.
- **Sprint 85** — Clips d'animation glTF (`ImportedMesh::clips`).
- **Sprint 86** — Mesh GPU skinné (`gfx::mesh::SkinnedVertex`,
  `ImportedMesh::skinned_mesh_data`).
- **Sprint 87** — Lecture d'animation en jeu : `AnimationState`, fondu enchaîné
  entre clips (`prev_clip`/`blend`, `AnimationState::set_clip`), champ
  `SceneObject::animation`.
- **Sprint 80** — Tests `golden_render`/rendu headless sautés (pas en échec)
  sans GPU disponible en CI Linux — même convention reprise par les tests
  d'intégration animation/crossfade de ce fichier.
- **Sprint 89** — Fond de scène : `Sky` (dégradé horizon/zénith + brouillard
  exponentiel).
- **Sprint 91** — `Sky::bloom_intensity`.
- **Sprint 92** — Tangentes par sommet (`ImportedMesh::tangents`), calculées
  pour tout mesh importé (skinné ou non), pas encore consommées par le rendu
  (pas de normal mapping à ce stade).
- **Sprint 93** — Événements de script `anim:<nom>` (`AppState::game_events`),
  déclenchés par les marqueurs de `ImportedMesh::notifies`.
- **Sprint 95** — Versionnage de schéma JSON (`Scene::version`,
  `Scene::migrate`, `Scene::CURRENT_VERSION`) et dédoublonnage des groupes
  d'une scène legacy (`version == 0`).
- **Sprint 96** — Système de prefabs : `PrefabInstance`, `SceneObject::prefab`,
  `Scene::save_prefab`/`instantiate_prefab`/`sync_prefab_instances`.
- **Sprint 97** — `SceneObject::tag`, interrogeable en Lua via `find_tag`.
- **Sprint 98** — `Scene::hud_layout` (décalages des overlays HUD
  repositionnables depuis l'éditeur).
- **Sprint 99** — Marqueurs temporels d'animation (`ImportedMesh::notifies`,
  `notifies_crossed`).
- **Sprint 100/101** — cf. `docs/audits/physics.md` (`ColliderShape::TriMesh`,
  `SceneObject::ccd`, `collision_layer`/`collision_mask`) : les champs vivent
  dans `SceneObject`, le comportement physique dans `runtime::physics`.
- **2026-07-12** — `Controller::acceleration` : accélération/freinage
  progressifs du déplacement joueur (au lieu d'une vitesse imposée
  instantanément), pour un déplacement moins abrupt.
- **2026-07-13** — `MobileControls::dpad` : pavé tactile « tank » W/A/S/D,
  pour retrouver sur APK les mêmes contrôles que le clavier desktop.
- **Audit gameplay antérieur** — Retrait de l'attaque en zone (`AttackMode`)
  du comportement par défaut : un swing par défaut qui vainc tout un groupe
  convergent avant qu'aucun monstre n'ait pu mordre rendait le combat trivial
  (cf. commit « attack_at cible désormais une seule cible, pas la zone »).
  `AttackMode::Zone` est réintroduit ensuite en opt-in par arme (le Marteau),
  avec un coût (préparation/recharge les plus longues) qui compense l'effet de
  groupe.

## Bugs réels trouvés en testant

- **Lave/vide qui ne tuait jamais un joueur en surface** (`controller_demo`,
  `tower_demo`) : le mesh `Plane` a une AABB locale quasi nulle en hauteur (Y).
  Sans épaissir l'échelle Y de la zone mortelle à la génération, son AABB
  réelle ne recoupait jamais la hauteur d'un joueur debout (~0,5 m) — la zone
  semblait fonctionnelle visuellement (le sol rouge/le vide est bien affiché)
  mais ne déclenchait jamais `deadly_at`. Corrigé en épaississant `scale.y`
  de la zone à la génération de la démo ; les tests
  (`controller_demo_lava_kills_standing_player`,
  `tower_demo_lava_style_void_kills_a_falling_player`) verrouillent
  spécifiquement `scale.y > 1.0` pour ne pas régresser silencieusement.

- **Défauts Rust vs défauts serde qui divergent silencieusement** (`Controller`,
  `Combat`, `AiChaser`) : `#[derive(Default)]` donne 0.0/vide/faux à chaque
  champ, alors que plusieurs champs ont un défaut serde non trivial
  (`move_speed` = 3.0, `attack_cooldown` = 0.5, `Combat::hp` = 1, etc.). Un
  premier passage utilisant `#[derive(Default)]` aurait fait diverger
  silencieusement les scènes construites en Rust (toutes les démos, via
  `..Default::default()`) des scènes chargées depuis un JSON ancien sans ces
  champs (désérialisées avec les défauts serde) — un ennemi construit en Rust
  serait par exemple né avec 0 PV (déjà vaincu) au lieu de 1. Corrigé par des
  `impl Default` manuels qui appellent les mêmes fonctions `default_*` que
  serde ; verrouillé par
  `controller_and_ai_chaser_rust_default_matches_serde_default`, qui compare
  explicitement les deux chemins.
