use super::*;

impl Physics {
    /// Construit le monde à partir des objets ayant un type de physique.
    pub fn build(scene: &Scene) -> Physics {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut dynamic = Vec::new();
        let mut controlled = Vec::new();
        let mut kinematic = Vec::new();
        let mut scripted = Vec::new();
        let mut collider_owner = std::collections::HashMap::new();

        for (i, obj) in scene.objects.iter().enumerate() {
            // Le joueur (joystick/gyro) devient un corps **kinématique** (Sprint
            // 103b, `KinematicCharacterController` : pentes/marches/snap au sol
            // natifs) ; une IA poursuivante **visible** reste un corps dynamique
            // ordinaire piloté par vitesse, comme avant — les deux sont « pilotés »
            // par `Physics::control` (le joueur par l'entrée, l'IA par la direction
            // vers le joueur, cf. `App::advance_play`), qui distingue en interne
            // selon la liste (`kinematic` vs `controlled`). Un chasseur masqué
            // (manche pas encore révélée, ou vaincu) n'a pas de corps : sinon son
            // collider bloquerait le joueur alors qu'il est invisible (cf.
            // `App::init_waves`/`update_waves`).
            let is_player = obj.controller.as_ref().is_some_and(|c| c.input || c.gyro);
            // Créature scriptée kinématique AVEC `ai_chaser` (chantier 4.1,
            // audit 2026-07-20 : créatures de la scène servie qui patrouillent
            // en Lua ET chassent par archétype) : le scripté PRIME. Sans ce
            // garde, `controllable` la basculerait en corps dynamique piloté
            // par vitesse — `resolve_scripted_moves` ne verrait plus jamais
            // ses écritures Lua (patrouille morte) et `step` écraserait la
            // position scriptée à chaque tick. La chasse de ces créatures
            // passe par le même canal que leur patrouille : une réécriture de
            // `transform.position` avant `resolve_scripted_moves`
            // (cf. `app::simulation`, boucle chasseurs).
            let is_ai =
                obj.ai_chaser.is_some() && obj.visible && obj.physics != PhysicsKind::Kinematic;
            let controllable = is_player || is_ai;
            if matches!(obj.physics, PhysicsKind::None) && !controllable {
                continue;
            }
            // Même garde-fou que `is_ai` ci-dessus, pour un fantôme réseau masqué
            // (`app::network_client::ensure_remote_player`, joueur/créature pas
            // encore diffusé ou déconnecté) : sans elle, il obtiendrait un corps
            // **fixe** (branche `else` plus bas, faute d'être `is_scripted`) qui
            // bloquerait le joueur local à sa dernière position connue, invisible
            // — un mur fantôme (cf. demande gameplay « les entités mobiles ne
            // doivent pas rester superposées »).
            if obj.physics == PhysicsKind::Kinematic && !controllable && !obj.visible {
                continue;
            }
            // Objet scripté à collisions (cf. `PhysicsKind::Kinematic`) : corps
            // kinématique piloté par `resolve_scripted_moves`, sauf s'il est déjà
            // joueur (le contrôleur joueur prime) ou IA poursuivante (corps
            // dynamique piloté par vitesse, comme avant).
            let is_scripted = obj.physics == PhysicsKind::Kinematic && !controllable;
            let is_dynamic =
                !is_player && !is_scripted && (obj.physics == PhysicsKind::Dynamic || controllable);

            let t = &obj.transform;
            let (axis, angle) = t.rotation.to_axis_angle();
            let rotvec = axis * angle;

            let mut builder = if is_player || is_scripted {
                RigidBodyBuilder::kinematic_position_based()
            } else if is_dynamic {
                RigidBodyBuilder::dynamic()
            } else {
                RigidBodyBuilder::fixed()
            };
            // Objet pilotable dynamique (IA) : on bloque les rotations pour qu'il
            // reste debout — moot pour un corps kinématique, jamais soumis au
            // solveur de toute façon (sa rotation reste entièrement pilotée par
            // l'appelant, jamais par rapier).
            if controllable && !is_player {
                builder = builder.lock_rotations();
            }
            // CCD : cf. la doc de `SceneObject::ccd` — seulement les objets qui en
            // ont explicitement besoin (missiles/projectiles rapides, toujours
            // dynamiques : un corps kinématique est déplacé par shapecast
            // successif, jamais par intégration rapide sujette au tunneling
            // que la CCD corrige).
            if obj.ccd && !is_player {
                builder = builder.ccd_enabled(true);
            }
            let body = builder
                .translation(Vector::new(t.position.x, t.position.y, t.position.z))
                .rotation(Vector::new(rotvec.x, rotvec.y, rotvec.z))
                .build();
            let handle = bodies.insert(body);

            // demi-dimensions du collider : AABB local mis à l'échelle. `center` :
            // les primitives du moteur sont modélisées centrées sur l'origine
            // (centre ≈ 0, offset sans effet), mais un mesh **importé** ne l'est
            // presque jamais (un personnage a les pieds à l'origine, son AABB
            // s'étend vers le haut) — sans cet offset, un collider déduit de
            // l'AABB (Box/Sphere/Capsule/Auto) serait centré sur les pieds,
            // à moitié enterré et débordant sous le sol.
            let (lmin, lmax) = scene.local_aabb(obj.mesh);
            let he = (lmax - lmin) * 0.5 * t.scale;
            let center = (lmin + lmax) * 0.5 * t.scale;
            let cuboid = || {
                ColliderBuilder::cuboid(
                    he.x.abs().max(0.01),
                    he.y.abs().max(0.01),
                    he.z.abs().max(0.01),
                )
                .translation(center)
            };
            let ball =
                || ColliderBuilder::ball(he.x.abs().max(he.z.abs()).max(0.01)).translation(center);
            let capsule = || {
                let r = he.x.abs().max(he.z.abs()).max(0.01);
                let half = (he.y.abs() - r).max(0.01);
                ColliderBuilder::capsule_y(half, r).translation(center)
            };
            // Vertices bruts du mesh importé, mis à l'échelle de l'objet — même
            // principe que `he` ci-dessus pour les primitives : le collider rapier
            // n'a pas de transform d'échelle séparée, l'échelle doit être bakée dans la
            // géométrie fournie. `None` pour tout ce qui n'est pas `MeshKind::Imported`
            // (primitives) ou dont l'import n'a pas encore chargé de données.
            let imported_points = || -> Option<Vec<Vec3>> {
                let MeshKind::Imported(idx) = obj.mesh else {
                    return None;
                };
                let data = &scene.imported.get(idx as usize)?.data;
                if data.vertices.is_empty() {
                    return None;
                }
                Some(
                    data.vertices
                        .iter()
                        .map(|v| Vec3::from(v.position) * t.scale)
                        .collect(),
                )
            };
            // Silhouette exacte : un triangle rapier par triangle du mesh importé.
            // Réservé au décor **statique** par l'appelant (cf. `ColliderShape::
            // TriMesh` ci-dessous) — `TriMesh` n'a pas de propriétés de masse définies,
            // rapier ne sait pas en faire un corps dynamique cohérent.
            let trimesh = || -> Option<ColliderBuilder> {
                let MeshKind::Imported(idx) = obj.mesh else {
                    return None;
                };
                let data = &scene.imported.get(idx as usize)?.data;
                if data.indices.len() < 3 {
                    return None;
                }
                let points = imported_points()?;
                let tris: Vec<[u32; 3]> = data
                    .indices
                    .chunks_exact(3)
                    .map(|c| [c[0], c[1], c[2]])
                    .collect();
                SharedShape::trimesh(points, tris)
                    .ok()
                    .map(ColliderBuilder::new)
            };
            // Enveloppe convexe : plus fidèle qu'une boîte, et — contrairement à
            // `TriMesh` — utilisable sur un corps dynamique (volume défini, propriétés
            // de masse calculables).
            let convex_hull = || -> Option<ColliderBuilder> {
                Some(ColliderBuilder::new(SharedShape::convex_hull(
                    &imported_points()?,
                )?))
            };
            // Forme explicite si demandée, sinon déduite du mesh. `MeshKind::Terrain`
            // reste un `cuboid()` plat ici (comme l'ancien `MeshKind::Plane` qu'il
            // remplace, Sprint 24 de `sprintreflecion.md`) : ne rien changer au
            // comportement du sol sur la quasi-totalité de la carte MMORPG,
            // strictement plate. Le relief obtient un collider heightfield
            // **additionnel**, restreint à sa seule bande non plate et inséré juste
            // après (cf. `terrain_hill_collider` plus bas) — deux colliders sur le
            // même corps plutôt qu'un seul heightfield global couvrant toute la carte :
            // un unique heightfield global cassait
            // `mmorpg_creature_never_gets_stuck_walking_into_a_wall` (créature figée
            // 43 % du temps alors qu'elle ne s'approche jamais de la bande de
            // collines) — le `KinematicCharacterController` des créatures/du joueur se
            // comporte différemment contre un heightfield composite que contre un
            // simple cuboid, même parfaitement plat. Restreindre le heightfield à la
            // bande ouest (jamais traversée par le contenu existant, cf. la doc de
            // `gfx::mesh::mmorpg_terrain_local_height`) élimine cette interaction
            // partout ailleurs.
            let collider = match obj.collider_shape {
                ColliderShape::Box => cuboid(),
                ColliderShape::Sphere => ball(),
                ColliderShape::Capsule => capsule(),
                ColliderShape::TriMesh => {
                    if is_dynamic {
                        log::warn!(
                            "{} : collider TriMesh demandé sur un corps dynamique (sans \
                             propriétés de masse définies) — repli sur ConvexHull.",
                            obj.name
                        );
                        convex_hull().unwrap_or_else(cuboid)
                    } else {
                        trimesh().unwrap_or_else(cuboid)
                    }
                }
                ColliderShape::ConvexHull => convex_hull().unwrap_or_else(cuboid),
                ColliderShape::Auto => match obj.mesh {
                    MeshKind::Sphere => ball(),
                    MeshKind::Capsule => capsule(),
                    MeshKind::Cylinder => {
                        ColliderBuilder::cylinder(he.y.abs().max(0.01), he.x.abs().max(0.01))
                            .translation(center)
                    }
                    // Dalle plate fine (demi-hauteur 0.02 × échelle Y), PAS `cuboid()` :
                    // `he`/`center` viennent de `scene.local_aabb(MeshKind::Terrain)`,
                    // volontairement élargi à ±2.2 en Y (`scene::queries::local_aabb`)
                    // pour englober le relief réel des collines — un `cuboid()` naïf sur
                    // cette AABB donnerait un pavé de 4,4 m de haut au lieu d'un sol fin,
                    // ce qui a fait échouer `mmorpg_creature_never_gets_stuck_walking_
                    // into_a_wall` (créature en collision avec un bloc culminant à
                    // y=+2,2 m au lieu du sol plat attendu à y≈0). Le VRAI relief passe
                    // par le collider heightfield additionnel inséré juste après
                    // l'insertion de ce collider principal (cf. plus bas).
                    MeshKind::Terrain => ColliderBuilder::cuboid(
                        he.x.abs().max(0.01),
                        (0.02 * t.scale.y).abs().max(0.005),
                        he.z.abs().max(0.01),
                    )
                    .translation(Vector::new(center.x, 0.0, center.z)),
                    _ => cuboid(),
                },
            }
            // Aucun rebond : un personnage n'est pas une balle (cf. docs/audits/
            // physics.md pour le mouvement instable observé avec un rebond non nul).
            // Rien dans le projet ne dépend d'un rebond (aucun mécanisme de type
            // trampoline).
            .restitution(0.0)
            .friction(0.6)
            // Couches de collision : `Group::from_bits_truncate` ignore silencieusement
            // les bits au-delà de 32 plutôt que de paniquer sur une valeur mal formée —
            // un JSON de scène corrompu/ancien ne doit pas faire planter l'entrée en
            // Play. `And` : les deux objets doivent s'accepter mutuellement (cf. la doc
            // de `InteractionGroups`), le mode le plus intuitif pour une paire
            // couche/masque.
            .collision_groups(InteractionGroups::new(
                Group::from_bits_truncate(obj.collision_layer),
                Group::from_bits_truncate(obj.collision_mask),
                InteractionTestMode::And,
            ))
            .build();
            let collider_handle = colliders.insert_with_parent(collider, handle, &mut bodies);
            collider_owner.insert(collider_handle, i);

            // Relief solide additionnel (Sprint 24/25 de `sprintreflecion.md`, Phase K) :
            // un second collider heightfield, restreint à la bande ouest non plate (cf.
            // le commentaire du match `ColliderShape::Auto` ci-dessus et la doc de
            // `gfx::mesh::MMORPG_HILL_STRIP_X_LOCAL`), attaché au MÊME corps que le
            // collider principal (`cuboid` plat). Échantillonne EXACTEMENT
            // `gfx::mesh::mmorpg_terrain_local_height` — la même fonction que le
            // maillage visuel — pour que sol visuel et sol solide coïncident dans
            // cette bande (sinon un joueur/une créature qui s'y aventure flotterait ou
            // s'enterrerait selon l'endroit). `PhysicsKind::Static` uniquement (comme
            // `TriMesh` : un heightfield n'a pas de propriétés de masse définies pour
            // un corps dynamique) — un `MeshKind::Terrain` dynamique n'a de toute façon
            // qu'un `cuboid()` plat (cf. ci-dessus), pas de relief du tout.
            if obj.mesh == MeshKind::Terrain && !is_dynamic {
                use crate::gfx::mesh::{
                    MMORPG_HILL_STRIP_RES, MMORPG_HILL_STRIP_X_LOCAL, mmorpg_terrain_local_height,
                };
                let (x_lo, x_hi) = MMORPG_HILL_STRIP_X_LOCAL;
                let (res_x, res_z) = MMORPG_HILL_STRIP_RES;
                // `heights[(i,j)]` : ligne `i` = axe Z (pleine étendue de l'objet),
                // colonne `j` = axe X (restreinte à `[x_lo,x_hi]`) — convention
                // `parry3d::shape::HeightField::new` (vérifiée dans les sources de la
                // dépendance, cf. le commentaire de `gfx::mesh::MMORPG_HILL_STRIP_X_LOCAL`).
                let heights =
                    Array2::from_fn((res_z + 1) as usize, (res_x + 1) as usize, |i, j| {
                        let x = x_lo + (x_hi - x_lo) * (j as f32 / res_x as f32);
                        let z = i as f32 / res_z as f32 - 0.5;
                        mmorpg_terrain_local_height(x, z)
                    });
                let strip_width = (x_hi - x_lo) * t.scale.x;
                let strip_center_x = (x_lo + x_hi) * 0.5 * t.scale.x;
                let hill_collider = ColliderBuilder::heightfield_with_flags(
                    heights,
                    Vector::new(strip_width, t.scale.y, t.scale.z),
                    HeightFieldFlags::FIX_INTERNAL_EDGES,
                )
                .translation(Vector::new(strip_center_x, 0.0, 0.0))
                .restitution(0.0)
                .friction(0.6)
                .collision_groups(InteractionGroups::new(
                    Group::from_bits_truncate(obj.collision_layer),
                    Group::from_bits_truncate(obj.collision_mask),
                    InteractionTestMode::And,
                ))
                .build();
                let hill_handle = colliders.insert_with_parent(hill_collider, handle, &mut bodies);
                collider_owner.insert(hill_handle, i);

                // Sprint 26 (Phase K, `sprintreflecion.md`) : troisième
                // collider heightfield sur le même corps, restreint cette
                // fois EN X **ET** EN Z (contrairement à celui juste
                // au-dessus, restreint uniquement en X) — le petit bassin du
                // Sprint 26 n'occupe qu'une poche bornée dans les deux axes
                // (`gfx::mesh::MMORPG_MOUND_X_LOCAL`/`MMORPG_MOUND_Z_LOCAL`),
                // pas une bande traversant toute la carte comme la bande de
                // collines ci-dessus. Même principe : échantillonne
                // EXACTEMENT `mmorpg_terrain_local_height`, `Static`
                // uniquement.
                use crate::gfx::mesh::{
                    MMORPG_MOUND_RES, MMORPG_MOUND_X_LOCAL, MMORPG_MOUND_Z_LOCAL,
                };
                let (mx_lo, mx_hi) = MMORPG_MOUND_X_LOCAL;
                let (mz_lo, mz_hi) = MMORPG_MOUND_Z_LOCAL;
                let (mres_x, mres_z) = MMORPG_MOUND_RES;
                let mound_heights =
                    Array2::from_fn((mres_z + 1) as usize, (mres_x + 1) as usize, |i, j| {
                        let x = mx_lo + (mx_hi - mx_lo) * (j as f32 / mres_x as f32);
                        let z = mz_lo + (mz_hi - mz_lo) * (i as f32 / mres_z as f32);
                        mmorpg_terrain_local_height(x, z)
                    });
                let mound_width = (mx_hi - mx_lo) * t.scale.x;
                let mound_depth = (mz_hi - mz_lo) * t.scale.z;
                let mound_center_x = (mx_lo + mx_hi) * 0.5 * t.scale.x;
                let mound_center_z = (mz_lo + mz_hi) * 0.5 * t.scale.z;
                let mound_collider = ColliderBuilder::heightfield_with_flags(
                    mound_heights,
                    Vector::new(mound_width, t.scale.y, mound_depth),
                    HeightFieldFlags::FIX_INTERNAL_EDGES,
                )
                .translation(Vector::new(mound_center_x, 0.0, mound_center_z))
                .restitution(0.0)
                .friction(0.6)
                .collision_groups(InteractionGroups::new(
                    Group::from_bits_truncate(obj.collision_layer),
                    Group::from_bits_truncate(obj.collision_mask),
                    InteractionTestMode::And,
                ))
                .build();
                let mound_handle =
                    colliders.insert_with_parent(mound_collider, handle, &mut bodies);
                collider_owner.insert(mound_handle, i);
            }

            if is_dynamic {
                dynamic.push((i, handle));
            }
            if is_player {
                kinematic.push((
                    i,
                    handle,
                    KinematicState {
                        hvel: Vec3::ZERO,
                        vspeed: 0.0,
                        // Vrai par défaut : au repos à l'apparition (vitesse nulle),
                        // même convention que l'ancienne heuristique dynamique
                        // (`cur.y.abs() < 1.0`, vraie tant qu'aucune chute n'a
                        // commencé).
                        grounded: true,
                    },
                ));
            } else if controllable {
                controlled.push((i, handle));
            } else if is_scripted {
                scripted.push((i, handle));
            }
        }

        // Plus d'itérations solveur que la valeur par défaut (4 → 8) : stabilise
        // les contacts (sol, murs, entre joueurs) — avec `restitution(0.0)` seul,
        // il restait un léger tremblement résiduel au repos/contact prolongé,
        // moins perceptible avec un solveur plus précis. Coût négligeable à cette
        // échelle (quelques corps dynamiques, pas des centaines).
        let integration = IntegrationParameters {
            num_solver_iterations: 8,
            ..Default::default()
        };

        Physics {
            bodies,
            colliders,
            gravity: Vector::new(0.0, -9.81, 0.0),
            integration,
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: DefaultBroadPhase::new(),
            narrow: NarrowPhase::new(),
            impulse: ImpulseJointSet::new(),
            multibody: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            dynamic,
            controlled,
            kinematic,
            scripted,
            collider_owner,
            query_cache: std::cell::RefCell::new(None),
        }
    }
}
