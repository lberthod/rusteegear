use super::*;

        pub(super) fn at(radius: f32, az_deg: f32) -> (f32, f32) {
            // Convention du fichier : -Z = Nord, +X = Est (cf. « Mur Nord » de
            // `mmorpg_demo`, posé à z = -half). az_deg = 0 ⇒ Nord, sens horaire.
            let r = az_deg.to_radians();
            (radius * r.sin(), -radius * r.cos())
        }

        pub(super) fn in_corridor(az_deg: f32) -> bool {
            // Couloirs dégagés (±13°) dans l'axe des 6 lisières de spawn de
            // vagues (4 portes cardinales + 2 brèches diagonales) : l'arrivée
            // d'une vague doit rester visible depuis le fort, pas masquée par
            // un mur d'arbres semé juste devant.
            const AZIMUTHS: [f32; 6] = [0.0, 45.0, 90.0, 180.0, 225.0, 270.0];
            AZIMUTHS.iter().any(|&a| {
                let d = (az_deg - a + 180.0).rem_euclid(360.0) - 180.0;
                d.abs() < 13.0
            })
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn poser(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            name: &str,
            file: &'static str,
            x: f32,
            z: f32,
            scale: f32,
            yaw_deg: f32,
            solide: bool,
        ) {
            let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file);
            let mesh_index = match imported.iter().position(|m| m.path == path) {
                Some(i) => i as u32,
                None => match crate::scene::import::load_gltf(&path) {
                    Ok((data, aabb_min, aabb_max)) => {
                        let mut mesh = ImportedMesh {
                            path,
                            data,
                            aabb_min,
                            aabb_max,
                            ..Default::default()
                        };
                        mesh.load_skinning();
                        imported.push(mesh);
                        (imported.len() - 1) as u32
                    }
                    Err(e) => {
                        log::error!("{name} ({file}) : {e}");
                        return;
                    }
                },
            };
            let mut deco = demo_obj(name, MeshKind::Imported(mesh_index), Vec3::new(x, 0.0, z));
            deco.transform = deco.transform.with_scale(Vec3::splat(scale));
            if yaw_deg != 0.0 {
                deco.transform.rotation = glam::Quat::from_rotation_y(yaw_deg.to_radians());
            }
            if solide {
                deco.physics = PhysicsKind::Static;
                deco.collider_shape = crate::runtime::physics::ColliderShape::TriMesh;
            }
            objects.push(deco);
        }

        pub(super) fn marker(objects: &mut Vec<SceneObject>, name: &str, x: f32, z: f32, color: [f32; 3]) {
            // Cf. la doc de fonction : substitut d'« Empty » (le moteur n'a
            // que des meshes) — petit disque non solide, ne bloque ni ne
            // gêne rien, juste un repère visuel/de conception.
            let mut m = demo_obj(name, MeshKind::Cylinder, Vec3::new(x, 0.05, z));
            m.transform = m.transform.with_scale(Vec3::new(0.4, 0.1, 0.4));
            m.physics = PhysicsKind::None;
            m.color = color;
            objects.push(m);
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn box_seg(
            objects: &mut Vec<SceneObject>,
            name: &str,
            x0: f32,
            x1: f32,
            z0: f32,
            z1: f32,
            y: f32,
            height: f32,
            min_thick: f32,
            color: [f32; 3],
        ) {
            let cx = (x0 + x1) * 0.5;
            let cz = (z0 + z1) * 0.5;
            let sx = (x1 - x0).abs().max(min_thick);
            let sz = (z1 - z0).abs().max(min_thick);
            let mut w = demo_obj(name, MeshKind::Cube, Vec3::new(cx, y, cz));
            w.transform = w.transform.with_scale(Vec3::new(sx, height, sz));
            w.physics = PhysicsKind::Static;
            w.color = color;
            objects.push(w);
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn aplat(
            objects: &mut Vec<SceneObject>,
            name: &str,
            cx: f32,
            cz: f32,
            sx: f32,
            sz: f32,
            y: f32,
            color: [f32; 3],
        ) {
            let mut o = demo_obj(name, MeshKind::Plane, Vec3::new(cx, y, cz));
            o.transform = o.transform.with_scale(Vec3::new(sx, 1.0, sz));
            o.physics = PhysicsKind::None;
            o.color = color;
            objects.push(o);
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn foret_scatter(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            rng: &mut crate::runtime::rng::Rng,
            r_in: f32,
            r_out: f32,
            excl: &[(f32, f32, f32, f32)],
            n: usize,
        ) {
            const FILES: [&str; 4] = [
                "nature_tree.glb",
                "nature_tree2.glb",
                "nature_pine.glb",
                "nature_pine2.glb",
            ];
            let mut poses = 0usize;
            let mut essais = 0usize;
            while poses < n && essais < n * 30 {
                essais += 1;
                let x = rng.next_range(-r_out, r_out);
                let z = rng.next_range(-r_out, r_out);
                let r = (x * x + z * z).sqrt();
                if r < r_in || r > r_out {
                    continue;
                }
                let az = x.atan2(-z).to_degrees().rem_euclid(360.0);
                if in_corridor(az) {
                    continue;
                }
                if excl
                    .iter()
                    .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                {
                    continue;
                }
                poses += 1;
                let bush = rng.next_range(0.0, 1.0) < 0.15;
                let file = if bush {
                    "nature_bush.glb"
                } else {
                    FILES[rng.next_below(FILES.len())]
                };
                let scale = rng.next_range(0.9, 1.4);
                let yaw = rng.next_range(0.0, 360.0);
                poser(
                    objects,
                    imported,
                    &format!(
                        "{} {poses}",
                        if bush {
                            "Buisson de forêt"
                        } else {
                            "Arbre de forêt"
                        }
                    ),
                    file,
                    x,
                    z,
                    scale,
                    yaw,
                    true,
                );
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn faune_scatter(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            rng: &mut crate::runtime::rng::Rng,
            r_in: f32,
            r_out: f32,
            excl: &[(f32, f32, f32, f32)],
            file: &'static str,
            prefix: &str,
            n: usize,
        ) {
            let mut poses = 0usize;
            let mut essais = 0usize;
            while poses < n && essais < n * 40 {
                essais += 1;
                let x = rng.next_range(-r_out, r_out);
                let z = rng.next_range(-r_out, r_out);
                let r = (x * x + z * z).sqrt();
                if r < r_in || r > r_out {
                    continue;
                }
                if excl
                    .iter()
                    .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                {
                    continue;
                }
                poses += 1;
                let scale = rng.next_range(0.85, 1.2);
                let yaw = rng.next_range(0.0, 360.0);
                poser(
                    objects,
                    imported,
                    &format!("{prefix} {poses}"),
                    file,
                    x,
                    z,
                    scale,
                    yaw,
                    false,
                );
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn poser_scaled(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            name: &str,
            file: &'static str,
            x: f32,
            z: f32,
            scale: Vec3,
            yaw_deg: f32,
            solide: bool,
        ) {
            let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file);
            let mesh_index = match imported.iter().position(|m| m.path == path) {
                Some(i) => i as u32,
                None => match crate::scene::import::load_gltf(&path) {
                    Ok((data, aabb_min, aabb_max)) => {
                        let mut mesh = ImportedMesh {
                            path,
                            data,
                            aabb_min,
                            aabb_max,
                            ..Default::default()
                        };
                        mesh.load_skinning();
                        imported.push(mesh);
                        (imported.len() - 1) as u32
                    }
                    Err(e) => {
                        log::error!("{name} ({file}) : {e}");
                        return;
                    }
                },
            };
            let mut deco = demo_obj(name, MeshKind::Imported(mesh_index), Vec3::new(x, 0.0, z));
            deco.transform = deco.transform.with_scale(scale);
            if yaw_deg != 0.0 {
                deco.transform.rotation = glam::Quat::from_rotation_y(yaw_deg.to_radians());
            }
            if solide {
                deco.physics = PhysicsKind::Static;
                deco.collider_shape = crate::runtime::physics::ColliderShape::TriMesh;
            }
            objects.push(deco);
        }

        // Répète siege_wall_segment.glb sur la longueur exacte d'un pan (mise
        // à l'échelle X non uniforme si la longueur ne divise pas rond par
        // MODULE_LEN — appliquée en espace local avant la rotation de yaw,
        // cf. convention `poser`/hamlet_common : échelle avant rotation).
        #[allow(clippy::too_many_arguments)]
        pub(super) fn wall_run(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            label: &str,
            x0: f32,
            x1: f32,
            z0: f32,
            z1: f32,
            yaw_deg: f32,
        ) {
            const MODULE_LEN: f32 = 4.0; // largeur de siege_wall_segment.glb
            let dx = x1 - x0;
            let dz = z1 - z0;
            let total = (dx * dx + dz * dz).sqrt();
            if total < 0.5 {
                return;
            }
            let n = (total / MODULE_LEN).round().max(1.0) as usize;
            let seg_len = total / n as f32;
            let x_scale = seg_len / MODULE_LEN;
            for i in 0..n {
                let t = (i as f32 + 0.5) / n as f32;
                let x = x0 + dx * t;
                let z = z0 + dz * t;
                poser_scaled(
                    objects,
                    imported,
                    &format!("{label} {}", i + 1),
                    "siege_wall_segment.glb",
                    x,
                    z,
                    Vec3::new(x_scale, 1.0, 1.0),
                    yaw_deg,
                    true,
                );
            }
        }
