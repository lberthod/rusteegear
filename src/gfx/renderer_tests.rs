    use super::*;

    #[test]
    fn mip_count_for_matches_the_standard_formula() {
        // 1 + log2(plus grande dimension) — vérifié contre des puissances
        // de deux connues plutôt qu'en réimplémentant la formule dans le test.
        assert_eq!(mip_count_for(1, 1), 1); // rien à générer sous une texture 1×1
        assert_eq!(mip_count_for(2, 2), 2);
        assert_eq!(mip_count_for(256, 256), 9); // 256,128,64,32,16,8,4,2,1
        assert_eq!(mip_count_for(1024, 1024), 11);
        // Non carrée : la plus grande dimension domine (l'autre s'arrête avant 1×1,
        // ce qui reste correct — wgpu accepte des mips plus petits que 1 sur un axe
        // tant que l'autre n'est pas encore à 1).
        assert_eq!(mip_count_for(256, 64), 9);
        assert_eq!(mip_count_for(64, 256), 9);
    }

    /// Sprint 111 : preuve que `invalidate_asset_textures` force un rechargement
    /// depuis le disque au prochain `sync_textures`, plutôt que de continuer à
    /// servir la version déjà en cache — c'est tout le mécanisme du hot-reload
    /// (`lib.rs::poll_asset_hot_reload` appelle cette méthode dès qu'un événement du
    /// dossier d'assets arrive). Utilise un chemin disque brut (pas `asset://`) :
    /// `assets::read_bytes` le lit tel quel via `std::fs::read`, donc le test n'a
    /// besoin de toucher ni `$HOME` ni le dossier d'assets réel (cf. la garde-fou
    /// d'isolation des tests système, Sprint 105a-3).
    #[test]
    fn invalidate_asset_textures_forces_a_reload_from_disk_on_the_next_sync() {
        let mut renderer = match pollster::block_on(Renderer::new_headless(64, 64)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "invalidate_asset_textures_forces_a_reload_from_disk_on_the_next_sync : \
                     pas de GPU headless ({e}) — test sauté."
                );
                return;
            }
        };

        let dir = std::env::temp_dir().join(format!(
            "motor3derust_hot_reload_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("texture.png");
        image::save_buffer(&path, &[255, 0, 0, 255], 1, 1, image::ColorType::Rgba8).unwrap();

        let scene = crate::scene::Scene {
            objects: vec![crate::scene::SceneObject {
                texture: path.to_str().unwrap().to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        renderer.sync_textures(&scene);
        assert!(
            renderer.textures.contains_key(path.to_str().unwrap()),
            "la texture doit être en cache après le premier sync"
        );

        renderer.invalidate_asset_textures();
        assert!(
            !renderer.textures.contains_key(path.to_str().unwrap()),
            "invalidate_asset_textures doit vider l'entrée (sauf la blanche par défaut)"
        );
        assert!(
            renderer.textures.contains_key(""),
            "la texture blanche par défaut ne doit pas être jetée"
        );

        // Re-synchroniser recharge bien depuis le disque (le fichier n'a pas
        // changé ici, mais c'est exactement ce que ferait une retouche réelle : le
        // point important est qu'aucun état ne bloque le rechargement après coup).
        renderer.sync_textures(&scene);
        assert!(
            renderer.textures.contains_key(path.to_str().unwrap()),
            "sync_textures doit recharger l'entrée invalidée"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Audit du 16 juillet 2026 : le héros féérique (`assets/models/fairy_hero.glb`),
    /// intégré comme joueur dans une scène MMORPG à 20 créatures skinnées, s'affichait
    /// éclaté en morceaux disjoints — chaque partie du mesh transformée par le squelette
    /// d'une *autre* créature. Cause : au-delà de `MAX_SKINNED_INSTANCES`,
    /// `write_joint_matrices` renvoyait l'offset `0` sans rien écrire, et
    /// `draw_skinned_objects` dessinait quand même l'objet avec cet offset — qui est
    /// *aussi* l'offset légitime de l'objet réellement au slot 0. Ce test construit une
    /// scène avec plus d'objets skinnés que `MAX_SKINNED_INSTANCES` et vérifie que
    /// `prepare_skinned_draws` renvoie `None` (pas un offset aliasé) pour ceux en trop.
    #[test]
    fn skinned_instances_beyond_capacity_get_no_offset_instead_of_aliasing_slot_zero() {
        use crate::scene::import::{Joint, Skeleton};
        use crate::scene::{ImportedMesh, SceneObject};

        let mut renderer = match pollster::block_on(Renderer::new_headless(64, 64)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "skinned_instances_beyond_capacity_get_no_offset_instead_of_aliasing_slot_zero : \
                     pas de GPU headless ({e}) — test sauté."
                );
                return;
            }
        };

        // Un squelette minimal (une racine) suffit : seul le nombre d'instances
        // skinnées visibles compte pour ce bug, pas la richesse du rig.
        let skeleton = Skeleton {
            joints: vec![Joint {
                name: "Root".into(),
                parent: None,
                bind_local: glam::Mat4::IDENTITY,
                inverse_bind: glam::Mat4::IDENTITY,
            }],
        };
        let imported = ImportedMesh {
            skeleton: Some(skeleton),
            ..Default::default()
        };

        // Un objet skinné visible de plus que la capacité — avant le correctif, les
        // instances au-delà de `MAX_SKINNED_INSTANCES` dessinaient avec l'offset 0.
        let n = MAX_SKINNED_INSTANCES + 3;
        let scene = crate::scene::Scene {
            imported: vec![imported],
            objects: (0..n)
                .map(|_| SceneObject {
                    mesh: MeshKind::Imported(0),
                    visible: true,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };

        renderer.draw_plan_skinned = (0..n).map(|i| (i, i as u32)).collect();
        renderer.prepare_skinned_draws(&scene);
        let offsets = &renderer.skinned_offsets_scratch;

        assert_eq!(offsets.len(), n);
        let valid = MAX_SKINNED_INSTANCES;
        for (slot, offset) in offsets.iter().enumerate() {
            if slot < valid {
                assert!(
                    offset.is_some(),
                    "le slot {slot} (dans la capacité de {valid}) doit avoir un offset"
                );
            } else {
                assert!(
                    offset.is_none(),
                    "le slot {slot} dépasse la capacité de {valid} : doit être `None` \
                     (sauté), pas un offset qui aliaserait la palette de joints d'un \
                     autre objet skinné"
                );
            }
        }
        // Les offsets valides doivent tous être distincts (un créneau par objet) —
        // sinon deux objets partageraient la même palette de joints sans même
        // dépasser la capacité, même bug sous une autre forme.
        let mut valid_offsets: Vec<u32> = offsets[..valid].iter().filter_map(|o| *o).collect();
        valid_offsets.sort_unstable();
        valid_offsets.dedup();
        assert_eq!(
            valid_offsets.len(),
            valid,
            "les offsets des objets dans la capacité doivent être tous distincts"
        );

        // Garde-fou visible (audit du 17 juillet 2026) : les 3 objets au-delà de la
        // capacité ne doivent pas rester un simple `log::warn` — le compteur exposé
        // (`skinned_dropped_count`) doit les recenser, prêt à être affiché dans un
        // panneau de stats.
        assert_eq!(
            renderer.skinned_dropped_count(),
            3,
            "les objets skinnés ignorés faute de créneau doivent être comptés"
        );

        // Et il se réinitialise à chaque frame préparée : une frame qui tient dans la
        // capacité doit revenir à 0, pas cumuler les frames passées.
        renderer.draw_plan_skinned = vec![(0, 0)];
        renderer.prepare_skinned_draws(&scene);
        assert_eq!(
            renderer.skinned_dropped_count(),
            0,
            "le compteur d'objets ignorés doit repartir de zéro à chaque frame"
        );
    }

    /// P1 (audit du 17 juillet 2026) : les objets skinnés ne projetaient **aucune**
    /// ombre — la passe d'ombre n'itérait que `draw_plan` (statiques), jamais
    /// `draw_plan_skinned`. Preuve par le rendu : un triangle skinné horizontal
    /// au-dessus d'un sol nu doit créer une zone nettement sombre quelque part sur ce
    /// sol (détectée par balayage, sans dépendre de la position exacte de l'ombre —
    /// la géométrie de la fixture après import/skinning rend le calcul analytique
    /// fragile). Sans le correctif, le sol est un dégradé lisse sans aucune zone
    /// sombre : le test échoue. Passe par `render_scene_headless`, le même chemin que
    /// les golden tests — le correctif y est donc couvert aussi.
    #[test]
    fn skinned_objects_cast_a_shadow_on_the_ground() {
        use crate::scene::{ImportedMesh, SceneObject, import};

        let (width, height) = (160u32, 120u32);
        let mut renderer = match pollster::block_on(Renderer::new_headless(width, height)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "skinned_objects_cast_a_shadow_on_the_ground : pas de GPU ({e}) — sauté."
                );
                return;
            }
        };

        // Mesh skinné minimal : la fixture triangle + squelette de `import::tests`,
        // rechargée par `load_skinning` (même chemin que le vrai import).
        let bytes = import::tests::skinned_triangle_glb();
        let path = import::tests::write_temp_glb(&bytes, "renderer_skinned_shadow");
        let (data, aabb_min, aabb_max) =
            import::load_gltf(path.to_str().unwrap()).expect("glTF de test valide");
        let mut imported = ImportedMesh {
            path: path.to_str().unwrap().to_string(),
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        imported.load_skinning();
        let _ = std::fs::remove_file(&path);
        assert!(imported.skeleton.is_some(), "fixture : squelette attendu");

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            imported: vec![imported],
            objects: vec![
                // Sol : cube aplati et élargi, blanc par défaut.
                SceneObject {
                    mesh: MeshKind::Cube,
                    transform: crate::scene::Transform {
                        scale: glam::Vec3::new(10.0, 0.1, 10.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                // Triangle skinné : couché à l'horizontale 3 m au-dessus du sol,
                // normale vers le bas — sa face **arrière** regarde la lumière, ce que
                // le cull front de la passe d'ombre laisse passer (vérifié : l'autre
                // orientation est cullée, une fixture à un seul triangle n'est pas un
                // volume fermé). Émissif : vu de dessus, sa face avant tournée vers le
                // sol ne reçoit aucune lumière directe et rendrait sa silhouette aussi
                // sombre que l'ombre cherchée — l'émissif la garde brillante sans
                // changer quoi que ce soit à la profondeur écrite dans la carte
                // d'ombre, donc aucun faux positif du balayage ci-dessous.
                SceneObject {
                    mesh: MeshKind::Imported(0),
                    transform: crate::scene::Transform {
                        position: glam::Vec3::new(0.0, 3.0, 0.0),
                        rotation: glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                        scale: glam::Vec3::splat(2.0),
                    },
                    emissive: 2.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        // Lumière inclinée depuis +X (ombre décalée hors de la silhouette des
        // triangles vus de haut), ambiante réduite : l'ombre n'atténue que la part
        // directe, un plancher ambiant fort noierait le contraste recherché.
        app.scene.light.dir = [0.6, 1.0, 0.0];
        app.scene.light.ambient = 0.05;
        // Vue quasi zénithale : sol, triangle et zone d'ombre tous à l'écran.
        app.camera.target = glam::Vec3::ZERO;
        app.camera.distance = 10.0;
        app.camera.pitch = 1.4;
        app.camera.yaw = 0.0;

        let pixels = renderer.render_scene_headless(&mut app, width, height);

        // Projette un point monde en pixel (l'aspect de la caméra vient d'être fixé
        // par `render_scene_headless`).
        let vp = app.camera.view_proj();
        let to_px = |p: glam::Vec3| -> (u32, u32) {
            let clip = vp * p.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            (
                (((ndc.x * 0.5 + 0.5) * width as f32) as u32).min(width - 1),
                (((0.5 - ndc.y * 0.5) * height as f32) as u32).min(height - 1),
            )
        };
        // Luminance moyenne d'un carré 3×3 autour d'un pixel.
        let lum_at = |(cx, cy): (u32, u32)| -> f32 {
            let mut sum = 0.0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let x = (cx as i32 + dx).clamp(0, width as i32 - 1) as u32;
                    let y = (cy as i32 + dy).clamp(0, height as i32 - 1) as u32;
                    let i = ((y * width + x) * 4) as usize;
                    sum += pixels[i] as f32 + pixels[i + 1] as f32 + pixels[i + 2] as f32;
                }
            }
            sum / (9.0 * 3.0)
        };

        // Balayage du sol autour de l'origine : sans ombre skinnée, cette zone est un
        // dégradé lisse et bien éclairé (~150-215 mesurés, seuls le ciel/hors-sol
        // descendent plus bas mais sont hors de la zone balayée) ; l'ombre portée y
        // creuse une plage quasi noire (~2 mesuré, plancher ambiant). Les triangles
        // eux-mêmes, blancs et éclairés par-dessus, restent clairs même s'ils occluent
        // un point balayé — aucun faux positif possible à ce seuil.
        let mut dark_samples = 0;
        for ix in -22..=22 {
            for iz in -22..=22 {
                let p = glam::Vec3::new(ix as f32 * 0.2, 0.05, iz as f32 * 0.2);
                if lum_at(to_px(p)) < 90.0 {
                    dark_samples += 1;
                }
            }
        }
        assert!(
            dark_samples >= 10,
            "aucune zone d'ombre détectée sur le sol ({dark_samples} échantillons sombres) \
             — la passe d'ombre ne dessine probablement pas les objets skinnés"
        );
    }

    /// Même correctif que le test précédent, mais sur la scène embarquée **réelle**
    /// (`assets/player_scene.json`) : 20 créatures skinnées + le joueur
    /// (`fairy_hero.glb`) visibles ensemble, exactement le scénario qui produisait le
    /// héros éclaté en jeu avant le correctif de `MAX_SKINNED_INSTANCES`/
    /// `write_joint_matrices`.
    #[test]
    fn the_embedded_mmorpg_scene_gives_the_player_its_own_joint_offset() {
        let mut renderer = match pollster::block_on(Renderer::new_headless(64, 64)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "the_embedded_mmorpg_scene_gives_the_player_its_own_joint_offset : \
                     pas de GPU headless ({e}) — test sauté."
                );
                return;
            }
        };

        let mut app = AppState::new();
        app.scene = crate::scene::Scene::embedded_player();
        // Vue quasi zénithale englobant toute l'arène (BOUND ±11 dans les scripts de
        // créature) : sans ça, le culling frustum par défaut ne laisserait qu'une
        // poignée de créatures visibles et ce test ne dépasserait jamais l'ancienne
        // capacité de 8, passant même sans le correctif.
        app.camera.target = glam::Vec3::ZERO;
        app.camera.distance = 40.0;
        app.camera.pitch = 1.5;
        app.camera.yaw = 0.0;

        renderer.sync_objects(&app.scene);
        renderer.write_uniforms(&app);
        renderer.prepare_skinned_draws(&app.scene);
        let offsets = &renderer.skinned_offsets_scratch;

        let player_obj_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.tag == "joueur")
            .expect("objet joueur introuvable dans la scène embarquée");

        // Le joueur n'est plus forcément skinné (ex. primitive Sphere) : dans ce cas,
        // il est dessiné par le plan statique batché, pas par `draw_plan_skinned`, et
        // n'a donc pas de palette de joints à protéger — seule sa présence compte.
        if !is_skinned(&app.scene, app.scene.objects[player_obj_idx].mesh) {
            assert!(
                renderer.draw_plan.iter().any(|d| d.obj == player_obj_idx),
                "le joueur non skinné doit apparaître dans le plan de dessin statique"
            );
            return;
        }

        let slot = renderer
            .draw_plan_skinned
            .iter()
            .position(|&(obj_idx, _)| obj_idx == player_obj_idx)
            .expect("le joueur doit être un objet skinné visible du plan de dessin");

        assert!(
            offsets[slot].is_some(),
            "le joueur (slot {slot} sur {} objets skinnés visibles) n'a pas de créneau \
             de joints propre — il se ferait dessiner avec la palette d'une créature",
            renderer.draw_plan_skinned.len()
        );
    }
