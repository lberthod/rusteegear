use super::*;

impl Physics {
    /// Pilote un objet (corps `controlled`) : fait tendre la vitesse horizontale vers
    /// `(vx, vz)` (joystick/gyro) et déclenche un saut si demandé **et** que l'objet est
    /// au sol. La vitesse verticale est sinon conservée (gravité), avec une gravité
    /// renforcée en descente (cf. `FALL_GRAVITY_FACTOR` : saut vif plutôt que
    /// « lunaire »). `jump_speed` = vitesse initiale du saut (m/s). `accel` (m/s²)
    /// borne la variation de vitesse horizontale par seconde — `0.0` fixe la vitesse
    /// instantanément (utilisé par l'IA/le recul, qui n'ont pas besoin d'inertie). Une
    /// valeur positive (mouvement du joueur, cf. `Controller::acceleration`) lisse
    /// départs et arrêts au lieu d'un « on/off » robotique, avec un freinage plus fort
    /// que l'accélération (`BRAKE_FACTOR` : arrêts nets) et une autorité réduite en
    /// l'air (`AIR_CONTROL` : arc de saut crédible). Renvoie `true` si un **saut** a
    /// effectivement été déclenché (objet au sol).
    /// Vrai si l'objet `index` a un corps **scripté** (kinématique piloté par
    /// `resolve_scripted_moves`) dans ce monde physique — la boucle chasseurs
    /// (`app::simulation`) s'en sert pour router la poursuite : corps dynamique
    /// → `control()` (vitesse), corps scripté → réécriture de position (même
    /// canal que la patrouille Lua, chantier 4.1 audit 2026-07-20).
    pub fn is_scripted_body(&self, index: usize) -> bool {
        self.scripted.iter().any(|&(i, _)| i == index)
    }

    #[allow(clippy::too_many_arguments)] // paramètres physiques distincts d'un même appel
    pub fn control(
        &mut self,
        index: usize,
        vx: f32,
        vz: f32,
        jump: bool,
        jump_speed: f32,
        accel: f32,
        dt: f32,
    ) -> bool {
        self.invalidate_query_cache();
        if let Some(slot) = self.kinematic.iter().position(|&(i, _, _)| i == index) {
            return self.control_kinematic(slot, vx, vz, jump, jump_speed, accel, dt);
        }
        let mut jumped = false;
        for &(i, handle) in &self.controlled {
            if i != index {
                continue;
            }
            if let Some(body) = self.bodies.get_mut(handle) {
                let cur = body.linvel();
                // Au sol : vitesse verticale quasi nulle (heuristique simple, sans raycast).
                // Effet secondaire bienvenu : le seuil large (< 1 m/s, soit ~0,1 s de chute
                // libre) offre un « coyote time » naturel — sauter juste après avoir quitté
                // un rebord fonctionne encore, comme dans les plateformers soignés.
                let grounded = cur.y.abs() < 1.0;
                let do_jump = jump && grounded;
                let vy = if do_jump {
                    jump_speed
                } else if !grounded && cur.y < 0.0 {
                    // Descente : gravité renforcée (cf. `FALL_GRAVITY_FACTOR`) — la part
                    // de base (×1) est déjà intégrée par `step`, on n'ajoute que l'excès.
                    cur.y - 9.81 * (FALL_GRAVITY_FACTOR - 1.0) * dt
                } else {
                    cur.y
                };
                let (nx, nz) = if accel > 0.0 {
                    // Accélération effective : renforcée au freinage (la cible ne
                    // prolonge pas la vitesse courante — relâchement, demi-tour,
                    // virage : cf. `BRAKE_FACTOR`), réduite en l'air (`AIR_CONTROL`).
                    let cur_sq = cur.x * cur.x + cur.z * cur.z;
                    let braking = vx * cur.x + vz * cur.z < cur_sq - 1e-6;
                    let mut a = accel;
                    if braking {
                        a *= BRAKE_FACTOR;
                    }
                    if !grounded {
                        a *= AIR_CONTROL;
                    }
                    let dx = vx - cur.x;
                    let dz = vz - cur.z;
                    let dist = (dx * dx + dz * dz).sqrt();
                    let max_step = a * dt;
                    if dist <= max_step || dist < 1e-6 {
                        (vx, vz)
                    } else {
                        (cur.x + dx / dist * max_step, cur.z + dz / dist * max_step)
                    }
                } else {
                    (vx, vz)
                };
                body.set_linvel(Vector::new(nx, vy, nz), true);
                jumped |= do_jump;
            }
        }
        jumped
    }

    /// Chemin `control` pour un corps **kinématique** (joueur, Sprint 103b) : même
    /// contrat que la boucle `dynamic` ci-dessus (freinage/autorité en l'air/chute
    /// accélérée identiques, cf. `BRAKE_FACTOR`/`AIR_CONTROL`/`FALL_GRAVITY_FACTOR`),
    /// mais la vitesse n'existe plus dans rapier (corps `kinematic_position_based`) :
    /// elle est gardée dans `KinematicState` et le déplacement réel passe par
    /// `KinematicCharacterController::move_shape`, qui gère nativement pentes/
    /// marches/snap au sol (contrairement à l'ancienne heuristique `cur.y.abs() <
    /// 1.0`, remplacée ici par `state.grounded`, le résultat du `move_shape`
    /// précédent).
    #[allow(clippy::too_many_arguments)]
    fn control_kinematic(
        &mut self,
        slot: usize,
        vx: f32,
        vz: f32,
        jump: bool,
        jump_speed: f32,
        accel: f32,
        dt: f32,
    ) -> bool {
        let (_, handle, state) = self.kinematic[slot];

        let grounded = state.grounded;
        let do_jump = jump && grounded;
        let vspeed = if do_jump {
            jump_speed
        } else if grounded {
            // Pas de solveur de contact pour maintenir un corps kinématique au
            // repos sur le sol : on remet explicitement à zéro plutôt que de
            // laisser une vitesse verticale résiduelle s'accumuler.
            0.0
        } else {
            // Gravité manuelle (rapier n'intègre pas de gravité sur un corps
            // kinématique) : base + excès de chute combinés en un seul terme,
            // même physique que l'ancien couple step()+control() sur corps
            // dynamique (cf. `FALL_GRAVITY_FACTOR`).
            let factor = if state.vspeed < 0.0 {
                FALL_GRAVITY_FACTOR
            } else {
                1.0
            };
            state.vspeed - 9.81 * factor * dt
        };

        let (nx, nz) = if accel > 0.0 {
            let cur = state.hvel;
            let cur_sq = cur.x * cur.x + cur.z * cur.z;
            let braking = vx * cur.x + vz * cur.z < cur_sq - 1e-6;
            let mut a = accel;
            if braking {
                a *= BRAKE_FACTOR;
            }
            if !grounded {
                a *= AIR_CONTROL;
            }
            let dx = vx - cur.x;
            let dz = vz - cur.z;
            let dist = (dx * dx + dz * dz).sqrt();
            let max_step = a * dt;
            if dist <= max_step || dist < 1e-6 {
                (vx, vz)
            } else {
                (cur.x + dx / dist * max_step, cur.z + dz / dist * max_step)
            }
        } else {
            (vx, vz)
        };

        let Some(body) = self.bodies.get(handle) else {
            return false;
        };
        let Some(&collider_handle) = body.colliders().first() else {
            return false;
        };
        let Some(collider) = self.colliders.get(collider_handle) else {
            return false;
        };
        let shape = collider.shape();
        let shape_pos = *body.position();
        let translation = body.translation();

        let desired = Vector::new(nx, vspeed, nz) * dt;
        // `exclude_sensors` : une zone de déclenchement (collider capteur, cf.
        // `Physics::sensor_overlaps`) est immatérielle — sans ça, le contrôleur
        // cinématique la traitait comme un mur (constaté : marcheur scripté bloqué
        // au bord d'une zone, x = 1,74 au lieu de 3).
        let filter = QueryFilter::new()
            .exclude_rigid_body(handle)
            .exclude_sensors();
        let queries = self.broad.as_query_pipeline(
            self.narrow.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        let controller = KinematicCharacterController {
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(PLAYER_AUTOSTEP_HEIGHT),
                min_width: CharacterLength::Relative(PLAYER_AUTOSTEP_MIN_WIDTH),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: PLAYER_MAX_SLOPE_CLIMB_DEG.to_radians(),
            min_slope_slide_angle: PLAYER_MIN_SLOPE_SLIDE_DEG.to_radians(),
            snap_to_ground: Some(CharacterLength::Relative(PLAYER_SNAP_TO_GROUND)),
            ..Default::default()
        };
        let movement = controller.move_shape(dt, &queries, shape, &shape_pos, desired, |_| {});
        let new_translation = translation + movement.translation;

        // Vitesse horizontale dérivée du mouvement **réel** (post-collision), pas
        // de la cible commandée : un mur doit freiner le joueur visiblement au
        // tick suivant, pas être ignoré par la continuité d'accélération (même
        // sensation que le solveur de contact sur l'ancien corps dynamique). La
        // composante verticale reste analytique (`vspeed` calculé ci-dessus) :
        // un petit ajustement de `snap_to_ground` ne doit pas se lire comme un
        // freinage de chute.
        let new_hvel = if dt > 1e-6 {
            Vec3::new(movement.translation.x, 0.0, movement.translation.z) / dt
        } else {
            Vec3::ZERO
        };
        self.kinematic[slot].2 = KinematicState {
            hvel: new_hvel,
            vspeed,
            grounded: movement.grounded,
        };

        if let Some(body) = self.bodies.get_mut(handle) {
            body.set_next_kinematic_translation(new_translation);
        }

        do_jump
    }

    /// Résout les déplacements écrits par les scripts Lua pour les objets
    /// `PhysicsKind::Kinematic` (cf. `scripted`) — à appeler chaque pas fixe
    /// **après** la boucle des scripts et **avant** `step`. Le script écrit
    /// librement `obj.x/y/z` ; ici, le déplacement demandé (position écrite −
    /// position réelle du corps) passe par un `KinematicCharacterController` :
    /// l'objet glisse le long des murs, objets fixes et autres corps (joueur
    /// compris) au lieu de les traverser, et la position **réellement atteinte**
    /// est réécrite dans la scène (le script du tick suivant repart de là — même
    /// principe que la vitesse post-collision de `control_kinematic`).
    ///
    /// Pas d'`autostep` (contrairement au joueur) : une créature ne doit pas
    /// « escalader » automatiquement le joueur ou un petit obstacle — elle bute
    /// et glisse, c'est tout. Une descente constante (`SCRIPTED_FALL_SPEED`)
    /// plaque l'objet au sol : les scripts de patrouille ne pilotent que x/z, et
    /// sans elle un objet apparu légèrement au-dessus du sol flotterait pour
    /// toujours (un corps kinématique ne subit pas la gravité de rapier).
    ///
    /// **Dépénétration bornée** (audit gameplay « gros sauts ») : quand deux
    /// corps scriptés finissent superposés — chacun résolu contre la position
    /// *d'avant-pas* de l'autre (`set_next_kinematic_translation` ne s'applique
    /// qu'au `step`), deux créatures qui se croisent peuvent avancer l'une dans
    /// l'autre au même tick — le contrôleur les expulsait d'un seul coup au tick
    /// suivant : un **pop latéral** de plusieurs fois la vitesse de marche,
    /// visible en jeu comme une téléportation. Le déplacement horizontal résolu
    /// est donc plafonné au déplacement demandé, plus un petit budget de
    /// séparation (`DEPEN_SPEED`) : la même expulsion s'étale sur quelques
    /// ticks — une poussée, pas un bond. Preuve :
    /// `app::simulation::tests::mmorpg_creatures_never_teleport_nor_snap_turn`.
    pub fn resolve_scripted_moves(&mut self, dt: f32, scene: &mut Scene) {
        self.invalidate_query_cache();
        /// Vitesse maximale (m/s) que la dépénétration peut ajouter au
        /// déplacement demandé par le script.
        const DEPEN_SPEED: f32 = 0.8;
        for slot in 0..self.scripted.len() {
            let (index, handle) = self.scripted[slot];
            let Some(obj) = scene.objects.get_mut(index) else {
                continue;
            };
            let Some(body) = self.bodies.get(handle) else {
                continue;
            };
            let Some(&collider_handle) = body.colliders().first() else {
                continue;
            };
            let Some(collider) = self.colliders.get(collider_handle) else {
                continue;
            };
            let shape = collider.shape();
            let cur = body.translation();
            // Pose du shape : translation réelle du corps, mais rotation écrite
            // par le script ce tick (`obj.ry`, pas encore commise sur le corps),
            // composée avec l'offset local du collider (mesh importé non centré).
            let local = collider.position_wrt_parent().copied().unwrap_or_default();
            let body_pose = Pose::from_parts(cur, obj.transform.rotation);
            let shape_pos = body_pose * local;

            let target = obj.transform.position;
            let mut desired = target - cur;
            desired.y -= SCRIPTED_FALL_SPEED * dt;

            // Même exclusion des capteurs que pour le joueur (ci-dessus).
            let filter = QueryFilter::new()
                .exclude_rigid_body(handle)
                .exclude_sensors();
            let queries = self.broad.as_query_pipeline(
                self.narrow.query_dispatcher(),
                &self.bodies,
                &self.colliders,
                filter,
            );
            let controller = KinematicCharacterController {
                slide: true,
                snap_to_ground: Some(CharacterLength::Relative(PLAYER_SNAP_TO_GROUND)),
                ..Default::default()
            };
            let movement = controller.move_shape(dt, &queries, shape, &shape_pos, desired, |_| {});
            let mut translation = movement.translation;
            // Plafond horizontal : jamais plus loin que demandé + le budget de
            // dépénétration (cf. la doc de cette fonction).
            let wanted_xz = Vec3::new(desired.x, 0.0, desired.z).length();
            let got_xz = Vec3::new(translation.x, 0.0, translation.z).length();
            let cap = wanted_xz + DEPEN_SPEED * dt;
            if got_xz > cap && got_xz > 1e-9 {
                let k = cap / got_xz;
                translation.x *= k;
                translation.z *= k;
            }
            let resolved = cur + translation;

            obj.transform.position = resolved;
            let next_rotation = obj.transform.rotation;
            if let Some(body) = self.bodies.get_mut(handle) {
                body.set_next_kinematic_translation(resolved);
                body.set_next_kinematic_rotation(next_rotation);
            }
        }
    }
}
