# `src/runtime/physics.rs`

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans le fichier.

## Attribution par sprint

- **Sprint 100** — `ColliderShape::TriMesh`/`ConvexHull` : colliders fidèles à
  la géométrie importée (au lieu d'une boîte englobante approximative).
- **Sprint 101** — `SceneObject::ccd`, `collision_layer`/`collision_mask`,
  `Physics::set_velocity`.
- **Sprint 102** — `Physics::raycast`/`overlap_sphere` (`collider_owner`,
  `RaycastHit`, `query_broad_phase`).
- **Sprint 66bis** — `Physics::set_position` (réconciliation réseau du joueur
  local, cf. `SPRINTNETWORK.md`).

## Bugs réels trouvés en testant

- **Rebond des personnages (2026-07-12)** : la restitution valait `0.5`
  jusque-là. Constaté en jeu réel : « comme une boule qui bug, pas fluide » —
  chaque atterrissage/contact renvoyait la moitié de la vitesse d'impact,
  mouvement visuellement instable. Passée à `0.0` (rien dans le projet ne
  dépend d'un rebond, aucun mécanisme façon trampoline).

- **Correction de position réseau qui ne persistait pas** : capture d'écran
  utilisateur montrant un personnage qui semblait dupliqué/trembler entre deux
  points. Cause : `step()` recopie la pose du corps rigide dans
  `transform.position` à *chaque* appel (sync à sens unique physique →
  transform) — écrire directement dans `transform.position` sans passer par
  `Physics::set_position` n'avait donc d'effet que pour la frame courante,
  écrasé au tick suivant. `set_position` corrige en écrivant sur le corps
  rigide lui-même.

- **Tunneling en écrivant les tests du Sprint 100** : une boule lâchée de
  trop haut traversait un `TriMesh` (pas d'épaisseur, un seul pas de
  simulation suffit à sauter par-dessus sans jamais toucher). Corrigé en
  lâchant les boules de test d'assez bas plutôt que d'anticiper la CCD par
  objet (devenue le Sprint 101).

- **Premier essai de `raycast`/`overlap_sphere` (Sprint 102) avec une
  broad-phase partagée** : peupler directement `self.broad` (la BVH
  incrémentale de `step`) avant le tout premier pas de simulation cassait la
  physique réelle — chasseurs et joueur se retrouvaient téléportés dans les
  tests d'IA. Cause probable : ça perturbait le suivi interne des colliders
  modifiés entre deux pas (compteurs de changement, détection de première
  passe). Corrigé en construisant une broad-phase **jetable**, reconstruite à
  chaque requête spatiale à partir de `collider_owner`, sans jamais toucher à
  `self.broad`.

- **Filtrage par masque, faux positif apparent (Sprint 101)** : un premier
  test de `collision_mask` semblait indiquer un bug de filtrage (le missile
  traversait le mur même *sans* masque réglé). En réalité la gravité faisait
  tomber le missile sous un mur de hauteur normale avant qu'il n'ait eu le
  temps de parcourir la distance à vitesse modeste — sans lien avec les
  couches de collision. Corrigé en agrandissant le mur des tests concernés.

- **Audit qualité du déplacement (2026-07-12)** : accélération/freinage
  instantanés (comportement d'origine) donnaient un personnage qui part et
  s'arrête comme un « on/off » robotique. Ajout d'une accélération/freinage
  progressifs (`BRAKE_FACTOR`, `AIR_CONTROL`, `FALL_GRAVITY_FACTOR`), avec
  `accel = 0.0` conservé pour l'IA/le recul qui n'ont pas besoin d'inertie.
