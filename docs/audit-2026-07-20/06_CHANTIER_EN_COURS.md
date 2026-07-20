# Chantier non commité en cours — refonte des contrôles (2026-07-20)

> ⚠️ **Document à péremption immédiate** : il analyse un diff non commité et **meurt au
> commit du chantier**. Avant de l'archiver, reporter les deux points encore ouverts dans
> le suivi : le test manquant sur `update_camera_collision` et la décision sur le masque
> de couches du raycast (repris en action 2.1 du [plan d'action](07_PLAN_ACTION.md)).

*8 fichiers modifiés, +271/−82 lignes : `src/app/{input,mod,network_client,simulation,simulation_tests}.rs`, `src/editor/hud.rs`, `src/gfx/camera.rs`, `src/lib.rs`.*

## Thème : contrôles « action moderne » à la place du schéma « tank »

1. **Déplacement caméra-relatif unifié** : WASD, flèches et stick gauche alimentent un même
   canal d'intention (`key_move`/`gamepad_move`) relatif à la caméra ; le personnage pivote
   seul vers la direction (`rotate_towards_smooth`). Avant : WASD = rotation+poussée tank.
2. **Caméra libre découplée** : stick droit → `gamepad_yaw` (2,4 rad/s), orbite la caméra
   sans tourner le personnage.
3. **Collision de caméra 3ᵉ personne** : nouveau `update_camera_collision` (raycast
   cible→œil) + `OrbitCamera::collision_distance` pour ne pas traverser les murs.

Renommages structurants : `GamepadInput.turn/thrust` → `move_x/move_y` ;
`PlayerInput.gamepad_turn/gamepad_thrust` → `gamepad_move` + `gamepad_yaw`.
Zone morte manette passée de par-axe à **circulaire**.

## Verdict : cohérent et quasi complet

- Aucune référence pendante (grep vert sur les anciens noms) ; remplacements propres partout.
- **Chemin réseau synchronisé** avec la prédiction locale : `network_move_axes` ajoute
  `gamepad_move` aux mêmes axes qu'`advance_play` (pas de tirage-arrière de réconciliation).
- **3 nouveaux tests** dans `simulation_tests.rs` (+92 l.) : déplacement caméra-relatif +
  auto-rotation, orbite libre du stick droit, garde-fou auto-run (stick droit ignoré en
  mode « Temple Run »). Tests d'`input.rs` migrés (zone morte circulaire, D-pad).
- Convention tank conservée pour le pavé tactile mobile (`hud.rs`), documentée.

## Points de vigilance avant commit

1. **`update_camera_collision` sans aucun test** — la seule pièce non couverte, et la plus
   fragile (constantes magiques SKIP=0.35, MARGIN=0.25, plancher 0.3). Surtout : le
   raycast utilise `mask = u32::MAX` → il accroche **toutes** les couches, y compris les
   colliders dynamiques (ennemis, autres joueurs passant derrière) qui rapprocheront
   brutalement la caméra. Un masque « décor solide » serait plus sûr.
2. **Comparaisons flottantes exactes** (`gamepad_yaw != 0.0`, `camera.yaw == 0.0`) —
   fonctionnent parce que la zone morte force un zéro exact, mais tout futur lissage de
   `gamepad_yaw` casserait le garde-fou silencieusement. Documenter ou passer par un epsilon.
3. **Cumul avant clamp** : flèches + WASD peuvent atteindre ±2.0 (borné en aval) ; entrées
   opposées s'annulent — acceptable mais implicite, non testé.
4. **Rotation vers `camera.yaw` à l'arrêt** (arme à distance + stick droit) : dépend du
   fait que `fireball.rs` tire depuis `transform.rotation` — couplage inter-modules
   correct aujourd'hui, sans test dédié.
5. **Dette de nommage** : `PlayerInput::turn()/thrust()` et `key_turn/key_thrust`
   subsistent pour le tactile mobile et le pont `pilot.rs` — canaux partiellement morts
   côté desktop, source de confusion future.

## Recommandation

Avant commit : ajouter un test sur `update_camera_collision` (mur simple entre œil et
cible → distance réduite ; rien entre les deux → `None`) et décider du masque de couches.
Le reste peut partir tel quel.
