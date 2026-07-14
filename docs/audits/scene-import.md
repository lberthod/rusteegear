# `src/scene/import.rs`

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans le fichier.

## Attribution par sprint

- **Sprint 84** — `Joint`/`Skeleton`/`VertexSkin`, `load_gltf_skeleton`,
  `build_skeleton`/`read_vertex_skins` : lecture du squelette (hiérarchie de
  joints, poses de liaison) et des poids de peau par sommet — données pures,
  sans encore de skinning ni d'animation.
- **Sprint 85** — `Interp`, `TrackVec3`/`TrackQuat`, `nlerp`, `sample_keyed`,
  `Clip`/`JointTracks`/`JointPose`, `load_gltf_clips`/`build_clip` :
  échantillonnage des clips d'animation (interpolation Step/Linear, bouclage).
- **Sprint 86** — `compute_joint_matrices`, `resolve_world_matrices`,
  `local_pose`/`decompose` : skinning GPU proprement dit (matrices de joint
  envoyées au shader), fusion pose de liaison / pose animée.
- **Sprint 87** — `compute_joint_matrices_blended` : crossfade entre deux
  clips (ex. idle→run), mélange au niveau de la pose locale avant composition
  de la hiérarchie.
- **Sprint 92** — `compute_tangents` : calcul de tangentes par sommet quand
  le glTF n'en fournit pas (méthode de Lengyel).
- **Sprint 99** — `Clip::without_tracks` : constructeur de clip minimal pour
  les tests d'échange notifies/événements (`app::mod`, tests de
  `notifies_crossed`), sans passer par un vrai fichier glTF.

## Bugs réels trouvés en testant

- **Fichiers temporaires GLB partagés entre tests (`write_temp_glb`)** : les
  premières fixtures nommaient le fichier temporaire à partir du seul
  `std::process::id()`. `cargo test` exécute les tests d'un même binaire sur
  plusieurs threads du même processus (pas des processus séparés) — deux
  tests utilisant le même `name` écrivaient donc le même chemin en parallèle,
  l'un tronquant le fichier pendant que l'autre le lisait. Symptôme observé :
  échec intermittent avec `« failed to fill whole buffer »`, reproductible
  seulement selon l'ordonnancement des threads (donc pas à chaque run).
  Corrigé en ajoutant un compteur atomique (`TEMP_GLB_COUNTER`) au nom de
  fichier, en plus du PID.
