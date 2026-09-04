# Roadmap — analyse comparative du 4 septembre 2026, lot « court terme »

Suivi d'exécution des neuf emprunts « court terme (un à deux mois, sans
changer l'architecture) » recommandés par
[ANALYSE_COMPARATIVE_MOTEURS_2026-09-04.md](ANALYSE_COMPARATIVE_MOTEURS_2026-09-04.md)
(§11). Exécutés le jour même, dans l'ordre ci-dessous. Chaque ligne dit ce
qui est livré, où, comment c'est vérifié, et ce qui reste volontairement
hors périmètre.

| # | Emprunt | Modèle | État |
|---|---|---|---|
| 1 | Passe transparente triée | Tous | ✅ |
| 2 | Ombres en cascade (3) | Godot, Bevy | ✅ |
| 3 | Caméra éditeur en vol WASD + clic droit | Unity, Godot | ✅ |
| 4 | Undo de l'inspecteur | Unity | ✅ (déjà fait le matin, roadmap UX 5.1 ; import glTF ajouté) |
| 5 | Blend 1D vitesse → idle/marche/course | Godot `AnimationTree` | ✅ |
| 6 | Joints rapier dans l'inspecteur | Godot | ✅ |
| 7 | Sensors rapier pour les triggers | Tous | ✅ |
| 8 | Validation CI Windows et Linux | Tous | ✅ jobs ajoutés, `continue-on-error` jusqu'à stabilité observée |
| 9 | README et QUICKSTART en anglais | Tous | ✅ |

## 1. Passe transparente triée

- **Donnée** : `SceneObject::opacity` (défaut 1, `#[serde(default)]` — aucune
  scène existante ne change). Curseur « Opacité » dans Inspecteur › Matériau.
- **Rendu** (`gfx/renderer/sync.rs`, `frame.rs`, `pipelines.rs`) : les objets
  `opacity < 1` quittent le lot opaque instancié et la passe d'ombre ; leur
  `ModelUniform` est ajouté en fin de buffer, trié du plus loin au plus près
  du centre de l'objet ; `transparent_pipeline` = mêmes shaders, blend
  `ALPHA_BLENDING`, profondeur lue mais pas écrite ; dessinés en dernier de la
  passe principale (après skinnés). `main.wgsl` sort l'alpha de l'objet ;
  `skinned.wgsl` sort 1 (VsOut identiques, exigence wgpu).
- **Vérifié** : golden `tests/golden/transparency.png` (vitre bleue devant la
  sphère) + test `a_translucent_object_lets_the_scene_behind_show_through`
  (translucide plus proche du fond que l'opaque). Les quatre goldens existants
  passent inchangés.
- **Hors périmètre** : tri par triangle, transparence des skinnés, ombres des
  translucides (documenté dans KNOWN_LIMITATIONS).

## 2. Ombres en cascade

- **Calcul** (`gfx/passes.rs::compute_cascades`) : découpage « practical
  split » (λ = 0,75) du frustum [0,1 m ; 100 m] en trois tranches (≈ 9, 24,
  100 m) ; sphère englobante par tranche (stable en rotation), rayon arrondi
  au décimètre, ortho de lumière calée au texel ; marge de 40 m côté lumière
  pour les projecteurs hors tranche.
- **GPU** : texture d'ombre `D2Array` à 3 couches, une vue par couche
  (cibles), une vue tableau (échantillonnage) ; 3 bind groups « cascade »
  (copie de `SceneUniform` avec `light_vp` de la cascade) pour que
  `shadow.wgsl`/`skinned.wgsl` n'aient rien à savoir des cascades ;
  `SceneUniform` étendu **en fin** (`cascade_vp`, `cascade_splits`) sans
  décaler les préfixes ; `main.wgsl` choisit la couche par distance caméra
  (`select`, sans index dynamique) et lit le texel depuis l'uniform.
- **Qualité** : `RenderQuality::shadow_size` (Basse 1024², sinon 2048²) ;
  headless toujours 2048² (goldens déterministes).
- **Vérifié** : tests `cascade_tests` (tranches contenues, calage au texel),
  goldens inchangés, capture de la démo MMORPG au pont `--pilot`.
- **Hors périmètre** : fondu entre cascades, ombres des lumières ponctuelles.

## 3. Caméra en vol au clic droit

- `OrbitCamera::look_around` (rotation autour de l'œil, `target` recalculé) ;
  `AppState::fly_look`/`fly_boost`, `fly_look_delta` ; `lib.rs` : clic droit
  pressé = vol, relâché sans glisser (< 4 px) = menu contextuel comme avant ;
  WASD via `key_move` existant, E/Q via `fly_vertical`, Maj ×3 ; W/E/R/Q ne
  changent plus d'outil pendant le vol. Relâchement traité avant la garde
  `consumed` (sinon `fly_look` restait coincé au-dessus d'un panneau egui).
- **Vérifié** : `fly_look_turns_the_head_around_a_fixed_eye`,
  `fly_look_is_ignored_in_play_and_in_player_mode`,
  `fly_boost_triples_the_flight_speed`, tests `camera::tests`.
- Doc : CONTROLS.md, table `SHORTCUTS` (fenêtre Aide).

## 4. Undo de l'inspecteur

- Déjà livré le matin même (roadmap UX 5.1, `push_ui_edit_undo`) : la ligne
  de KNOWN_LIMITATIONS était périmée. Ajouté : l'**import glTF** est annulable
  (`finish_import` capture l'état avant), test
  `gltf_import_is_undoable`.

## 5. Locomotion automatique

- `scene::Locomotion` (idle/walk/run, `walk_speed`, `run_speed`, vitesse
  lissée τ = 0,12 s, non sérialisée) dans `AnimationState::locomotion` ;
  `apply_locomotion` pose `prev_clip`/`clip`/`blend` chaque pas — le rendu
  (`prepare_skinned_draws`) mélange comme pour un crossfade sans rien savoir.
  Les deux temps de lecture avancent en continu (pas de redémarrage à 0 à
  chaque aller-retour). Section « 🚶 Locomotion auto » dans l'Inspecteur.
- **Vérifié** : 7 tests `locomotion_*` (repos, marche pure, mi-chemin, course,
  retour à idle sans saut, sérialisation, no-op).
- **Hors périmètre** : blend tree général, couches, IK, synchronisation des
  pas au sol.

## 6. Joints rapier

- `scene::Joint` (`kind` Fixe/Charnière/Rotule, `target` par nom ou monde,
  `anchor`/`target_anchor` en unités locales avant échelle, `axis`, `limits`
  en degrés) dans `SceneObject::joint` ; `Physics::build` relie après la
  création de tous les corps (`body_of`), corps fixe créé à la volée pour une
  cible sans physique ou pour le monde ; soudure qui **préserve la pose
  relative** d'entrée en Play ; axe de charnière exprimé dans chaque repère.
  Section « 🔗 Articulation » dans Inspecteur › Physique.
- **Vérifié** : pendule rotule (longueur constante), pendule charnière (plan
  XY), butées ±20°, soudure au monde, soudure poteau→lanterne à 30°, joint
  sans physique ignoré.

## 7. Capteurs rapier pour les triggers

- Un collider **capteur** supplémentaire par objet `trigger` visible (jamais à
  la place du solide) ; objet sans physique = corps fixe portant seulement le
  capteur ; `ActiveCollisionTypes::all()` (fixe↔cinématique sinon ignoré).
  `Physics::sensor_overlaps` → `obj.overlapped` / `overlap_count` /
  `overlap_names` sur les deux backends Lua. `obj.triggered` inchangé (AABB
  joueur), pour ne rien changer aux créatures qui mordent au contact.
- **Bug trouvé au passage** : le contrôleur cinématique (joueur et créatures
  scriptées) traitait un capteur comme un mur — `exclude_sensors()` ajouté aux
  `QueryFilter` des contrôleurs et des requêtes `raycast`/`overlap_sphere`.
- **Vérifié** : plaque de pression + caisse (sans joueur), marcheur
  cinématique, pas d'auto-détection, capteurs invisibles aux requêtes, test
  différentiel natif/web, test bout en bout `a_pressure_plate_script_fires_…`.

## 8. CI Linux et Windows

- `editor-linux` : build des 4 binaires, apt `mesa-vulkan-drivers xvfb`,
  lancement réel de l'éditeur sous Xvfb avec `--pilot`, `pilot state`,
  capture d'écran publiée en artefact, `console play`/`stop`.
- `editor-windows` : build des 4 binaires + `cargo test --lib`.
- Les deux en `continue-on-error: true` (convention du dépôt : `golden` et
  `audit` ont eu la même période d'observation) — **à retirer après 15 runs
  verts**, comme pour `golden`.

## 9. Documentation anglaise

- `README.en.md` (condensé fidèle, liens vers les docs françaises signalées
  comme telles) et `QUICKSTART.en.md` ; lien en tête du README français.

## Reste à faire (moyen terme, cf. §11 de l'analyse)

Hiérarchie parent/enfant, extraction des composants du Hameau hors de
`SceneObject`, particules GPU, PBR réel (GGX + IBL), assets par projet, UDP
pour le gameplay natif, liste de salons, machine à états d'animation
déclarative.
