use super::*;

/// Pendant côté PV de `creature_archetypes_produce_visibly_different_chase_speeds`
/// (`src/app/mod.rs`) : `hp_multiplier` doit, comme `speed_multiplier`, produire
/// un `hp` final différent pour deux créatures de même `hp` de base mais
/// d'archétype différent (GDD_MMORPG.md §5.4 — Meute PV réduits, Colosse PV
/// élevés).
#[test]
fn creature_archetypes_produce_visibly_different_hp() {
    let base_hp = 10u32;
    let final_hp = |archetype: Archetype| -> u32 {
        ((base_hp as f32) * archetype.hp_multiplier()).round() as u32
    };
    let meute = final_hp(Archetype::Meute);
    let traqueuse = final_hp(Archetype::Traqueuse);
    let colosse = final_hp(Archetype::Colosse);
    assert!(
        meute < traqueuse,
        "la Meute doit avoir moins de PV que la Traqueuse : {meute} >= {traqueuse}"
    );
    assert!(
        traqueuse < colosse,
        "le Colosse doit avoir plus de PV que la Traqueuse : {traqueuse} >= {colosse}"
    );
}

/// Intégration bout en bout : un `SceneObject.animation` fait bouger un
/// mesh skinné à travers `Renderer::render_scene_headless`, pas seulement les briques
/// isolées (déjà testées ailleurs : `Clip::sample_joint`, `ImportedMesh::load_skinning`,
/// `skinned_mesh_data`, le pipeline GPU via `tests/golden_skinning.rs`). Sauté (pas en
/// échec) sans GPU headless — même raison que `tests/golden_render.rs` (CI Linux sans
/// GPU).
#[test]
fn scene_object_animation_moves_a_skinned_mesh_through_the_full_render_path() {
    let bytes = import::tests::animated_skinned_glb();
    let path = import::tests::write_temp_glb(&bytes, "scene_object_animation_integration");

    let render_at = |time: f32| -> Option<Vec<u8>> {
        let mut renderer =
            match pollster::block_on(crate::gfx::renderer::Renderer::new_headless(64, 64)) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "scene_object_animation_integration : pas de GPU headless ({e}) \
                             — test sauté."
                    );
                    return None;
                }
            };
        let mut app = crate::app::AppState::default();
        app.scene.light.ambient = 0.4;
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
        let clip_name = imported.clips[0].name.clone();
        app.scene.imported.push(imported);
        app.scene.objects.push(SceneObject {
            mesh: MeshKind::Imported(0),
            transform: Transform::default(),
            color: [0.9, 0.5, 0.2],
            animation: Some(AnimationState {
                clip: clip_name,
                time,
                speed: 1.0,
                ..Default::default()
            }),
            ..Default::default()
        });
        Some(renderer.render_scene_headless(&mut app, 64, 64))
    };

    let (Some(at_0), Some(at_1)) = (render_at(0.0), render_at(1.0)) else {
        return; // pas de GPU : rien à comparer (message déjà expliqué ci-dessus)
    };
    let _ = std::fs::remove_file(&path);

    assert_eq!(at_0.len(), at_1.len());
    let differing = at_0
        .iter()
        .zip(&at_1)
        .filter(|(a, b)| a.abs_diff(**b) > 8)
        .count();
    assert!(
        differing > 0,
        "l'image à t=0 et t=1 est identique : l'animation du joint (translation \
             linéaire testée séparément dans import::tests) ne semble pas atteindre le \
             rendu — la chaîne SceneObject → prepare_skinned_draws → shader est cassée \
             quelque part"
    );
}

/// Intégration bout en bout du fondu enchaîné : un `SceneObject` en
/// pleine transition (`blend` intermédiaire, `prev_clip` renseigné) doit produire un
/// rendu **différent** de la pose de liaison pure et du clip cible pur — preuve que
/// `prepare_skinned_draws` prend bien la branche mélangée (`compute_joint_matrices_blended`)
/// à travers le rendu réel, pas seulement testée isolément côté CPU
/// (`blended_joint_matrices_*` dans `import::tests`). `prev_clip` pointe vers un nom
/// de clip inexistant : `find_clip` retombe sur la pose de liaison pour ce côté du
/// mélange, un cas valide (transition depuis un état non animé) et pratique à
/// construire sans fixture à deux clips.
#[test]
fn scene_object_crossfade_renders_differently_from_either_pure_endpoint() {
    let bytes = import::tests::animated_skinned_glb();
    let path = import::tests::write_temp_glb(&bytes, "scene_object_crossfade_integration");

    let render_with = |anim: AnimationState| -> Option<Vec<u8>> {
        let mut renderer =
            match pollster::block_on(crate::gfx::renderer::Renderer::new_headless(64, 64)) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "scene_object_crossfade_integration : pas de GPU headless ({e}) \
                             — test sauté."
                    );
                    return None;
                }
            };
        let mut app = crate::app::AppState::default();
        app.scene.light.ambient = 0.4;
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
        app.scene.imported.push(imported);
        app.scene.objects.push(SceneObject {
            mesh: MeshKind::Imported(0),
            transform: Transform::default(),
            color: [0.9, 0.5, 0.2],
            animation: Some(anim),
            ..Default::default()
        });
        Some(renderer.render_scene_headless(&mut app, 64, 64))
    };

    let clip_name = {
        let mut m = ImportedMesh {
            path: path.to_str().unwrap().to_string(),
            ..Default::default()
        };
        m.load_skinning();
        m.clips[0].name.clone()
    };

    let base = AnimationState {
        clip: clip_name,
        time: 1.0, // clip cible pur : à t=1.0, translation (10,0,0) de la fixture
        speed: 1.0,
        prev_clip: "PoseDeLiaisonInexistante".into(), // cf. doc du test
        prev_time: 0.0,
        blend: 1.0,
    };
    let Some(pure_target) = render_with(base.clone()) else {
        return; // pas de GPU
    };
    let mut mid = base.clone();
    mid.blend = 0.5;
    let Some(mid_transition) = render_with(mid) else {
        return;
    };
    let mut pure_bind = base;
    pure_bind.blend = 0.0;
    let Some(pure_bind_pose) = render_with(pure_bind) else {
        return;
    };
    let _ = std::fs::remove_file(&path);

    // Comparaison volontairement limitée à « mi-transition vs pose de liaison pure » :
    // à blend=1.0 la translation du clip (10 unités sur X de la fixture) pousse
    // l'objet hors du petit cadre 64×64 de ce test, rendant blend=0.5 et blend=1.0
    // visuellement indiscernables (les deux hors champ) bien que les matrices de
    // joints diffèrent réellement entre les deux (déjà prouvé au niveau CPU par
    // `blended_joint_matrices_at_midpoint_interpolate_translation`). La pose de
    // liaison, elle, reste à l'origine — toujours dans le cadre, comparaison fiable.
    let differs = |a: &[u8], b: &[u8]| a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 8);
    assert!(
        differs(&mid_transition, &pure_bind_pose),
        "à mi-transition, le rendu ne doit pas être identique à la pose de liaison pure \
             — le fondu ne semble pas atteindre le rendu réel"
    );
    assert!(
        differs(&pure_target, &pure_bind_pose),
        "précondition : le clip cible pur doit lui-même différer de la pose de liaison \
             (sinon toute cette comparaison serait vide de sens)"
    );
}

#[test]
fn imported_mesh_load_skinning_populates_skeleton_clips_and_vertex_skins() {
    // Réutilise la fixture .glb existante (`import::tests`) plutôt que d'en
    // reconstruire une : elle est déjà vérifiée correcte, seule la *plomberie*
    // `ImportedMesh::load_skinning` est testée ici.
    let path = import::tests::write_temp_glb(
        &import::tests::skinned_triangle_glb(),
        "scene_load_skinning",
    );
    let mut m = ImportedMesh {
        path: path.to_str().unwrap().to_string(),
        ..Default::default()
    };
    m.load_skinning();
    let _ = std::fs::remove_file(&path);

    let skeleton = m.skeleton.expect("la fixture a un skin");
    assert_eq!(skeleton.joints.len(), 2);
    assert_eq!(
        m.vertex_skins.len(),
        3,
        "un VertexSkin par sommet du triangle"
    );
    // Cette fixture n'a pas de bloc "animations" : pas de clip, mais pas d'erreur
    // non plus (skin sans animation = squelette utilisable en pose de liaison seule).
    assert!(m.clips.is_empty());
}

#[test]
fn imported_mesh_load_skinning_leaves_a_static_mesh_untouched() {
    let path = import::tests::write_temp_glb(
        &import::tests::unskinned_triangle_glb(),
        "scene_load_skinning_static",
    );
    let mut m = ImportedMesh {
        path: path.to_str().unwrap().to_string(),
        ..Default::default()
    };
    m.load_skinning();
    let _ = std::fs::remove_file(&path);

    assert!(m.skeleton.is_none());
    assert!(m.clips.is_empty());
    assert!(m.vertex_skins.is_empty());
}

#[test]
fn skinned_mesh_data_combines_geometry_and_skin_weights() {
    let bytes = import::tests::skinned_triangle_glb();
    let path = import::tests::write_temp_glb(&bytes, "scene_skinned_mesh_data");
    let (data, aabb_min, aabb_max) = import::load_gltf(path.to_str().unwrap()).unwrap();
    let mut m = ImportedMesh {
        path: path.to_str().unwrap().to_string(),
        data,
        aabb_min,
        aabb_max,
        ..Default::default()
    };
    m.load_skinning();
    let _ = std::fs::remove_file(&path);

    let skinned = m.skinned_mesh_data().expect("mesh skinné : Some attendu");
    assert_eq!(skinned.vertices.len(), 3);
    assert_eq!(skinned.indices, m.data.indices);
    // Sommet 2 de la fixture : joints [0,1,0,0], poids [0.5,0.5,0,0].
    assert_eq!(skinned.vertices[2].joints, [0, 1, 0, 0]);
    assert_eq!(skinned.vertices[2].weights, [0.5, 0.5, 0.0, 0.0]);
    // Géométrie transportée telle quelle depuis `data.vertices`.
    assert_eq!(skinned.vertices[0].position, m.data.vertices[0].position);
}

#[test]
fn load_skinning_also_populates_tangents_for_any_imported_mesh() {
    // Contrairement au squelette (skin glTF requis), les tangentes
    // sont calculées pour n'importe quel mesh importé — vérifié ici sur la même
    // fixture skinnée que `skinned_mesh_data_combines_geometry_and_skin_weights`,
    // mais rien dans `compute_tangents` ne dépend du skin.
    let bytes = import::tests::skinned_triangle_glb();
    let path = import::tests::write_temp_glb(&bytes, "scene_load_skinning_tangents");
    let (data, aabb_min, aabb_max) = import::load_gltf(path.to_str().unwrap()).unwrap();
    let mut m = ImportedMesh {
        path: path.to_str().unwrap().to_string(),
        data,
        aabb_min,
        aabb_max,
        ..Default::default()
    };
    m.load_skinning();
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        m.tangents.len(),
        m.data.vertices.len(),
        "une tangente par sommet"
    );
    for t in &m.tangents {
        assert!(t[0].is_finite() && t[1].is_finite() && t[2].is_finite());
        assert!(
            t[3] == 1.0 || t[3] == -1.0,
            "signe de bitangente : {}",
            t[3]
        );
    }
}

#[test]
fn skinned_mesh_data_is_none_for_a_static_mesh() {
    let bytes = import::tests::unskinned_triangle_glb();
    let path = import::tests::write_temp_glb(&bytes, "scene_skinned_mesh_data_static");
    let (data, aabb_min, aabb_max) = import::load_gltf(path.to_str().unwrap()).unwrap();
    let mut m = ImportedMesh {
        path: path.to_str().unwrap().to_string(),
        data,
        aabb_min,
        aabb_max,
        ..Default::default()
    };
    m.load_skinning();
    let _ = std::fs::remove_file(&path);

    assert!(m.skinned_mesh_data().is_none());
}

#[test]
fn hue_to_rgb_primary_colors() {
    let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3);
    assert!(close(hue_to_rgb(0.0), [1.0, 0.0, 0.0]), "rouge");
    assert!(close(hue_to_rgb(1.0 / 3.0), [0.0, 1.0, 0.0]), "vert");
    assert!(close(hue_to_rgb(2.0 / 3.0), [0.0, 0.0, 1.0]), "bleu");
    // Périodicité : h et h+1 donnent la même couleur.
    assert!(close(hue_to_rgb(0.2), hue_to_rgb(1.2)), "période");
}

#[test]
fn nearest_point_lights_picks_closest_to_camera() {
    let mut s = Scene::default();
    // 3 lumières à x = 0, 5, 10 ; caméra à l'origine.
    for x in [0.0, 5.0, 10.0] {
        s.point_lights.push(PointLight {
            position: [x, 0.0, 0.0],
            ..PointLight::default()
        });
    }
    // Limite 2 → garde les deux plus proches (x=0 puis x=5), dans l'ordre.
    let chosen = s.nearest_point_lights(Vec3::ZERO, 2);
    assert_eq!(chosen, vec![0, 1]);
    // Caméra près de la 3ᵉ → garde x=10 puis x=5.
    let chosen = s.nearest_point_lights(Vec3::new(10.0, 0.0, 0.0), 2);
    assert_eq!(chosen, vec![2, 1]);
    // Sous la limite → toutes, ordre d'origine (pas de tri).
    assert_eq!(s.nearest_point_lights(Vec3::ZERO, 8), vec![0, 1, 2]);
}

#[test]
fn transform_matrix_translates_point() {
    let t = Transform::from_pos(Vec3::new(1.0, 2.0, 3.0));
    let p = t.matrix() * Vec3::ZERO.extend(1.0);
    assert!((p.truncate() - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-6);
}

#[test]
fn transform_matrix_applies_scale() {
    let t = Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(2.0));
    let p = t.matrix() * Vec3::new(1.0, 0.0, 0.0).extend(1.0);
    assert!((p.truncate() - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-6);
}

#[test]
fn mobile_demo_is_playable() {
    let s = Scene::mobile_demo();
    // contrôles tactiles présents
    assert!(s.mobile.joystick);
    assert!(s.mobile.buttons.iter().any(|b| b == "Saut"));
    // un personnage scripté qui lit le joystick
    let player = s.objects.iter().find(|o| o.name == "Joueur").unwrap();
    assert!(player.script.contains("input.jx"));
    assert!(player.script.contains("input.btn.Saut"));
    // et un sol
    assert!(s.objects.iter().any(|o| matches!(o.mesh, MeshKind::Plane)));
}

#[test]
fn tower_demo_is_a_distinct_no_combat_climbing_level() {
    let s = Scene::tower_demo();
    // Contrôles : joystick + saut, comme la démo contrôleur, mais pas d'attaque
    // (aucun combat dans ce style de niveau).
    assert!(s.mobile.joystick);
    assert!(s.mobile.buttons.iter().any(|b| b == "Saut"));
    assert!(!s.mobile.buttons.iter().any(|b| b == "Attaque"));
    let player = s
        .objects
        .iter()
        .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        .expect("un joueur pilotable");
    assert!(
        player.controller.as_ref().unwrap().attack_button.is_empty(),
        "pas de bouton d'attaque dans ce niveau"
    );
    // Aucun ennemi, aucune lave (contrairement à la démo contrôleur) : le seul danger
    // est la chute (zone `deadly` unique).
    assert!(!s.objects.iter().any(|o| o.name.starts_with("Ennemi")));
    assert!(!s.objects.iter().any(|o| o.name == "Lave"));
    let deadly: Vec<_> = s.objects.iter().filter(|o| o.deadly).collect();
    assert_eq!(deadly.len(), 1, "un seul danger : le vide en contrebas");
    assert_eq!(deadly[0].name, "Vide");
    // Au moins une plateforme non triviale au-dessus du socle de départ, et une
    // gemme-objectif obligatoire par plateforme (collectibles => victoire en gravissant).
    let platforms = s
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Plateforme"))
        .count();
    assert!(
        platforms >= 10,
        "une vraie tour à gravir, pas un décor minimal"
    );
    let (collected, total) = s.collectibles().expect("des gemmes-objectif");
    assert_eq!(collected, 0);
    assert_eq!(total, platforms, "une gemme obligatoire par plateforme");
}

#[test]
fn tower_demo_lava_style_void_kills_a_falling_player() {
    // Même piège que pour la lave (cf. `controller_demo_lava_kills_standing_player`) :
    // le mesh Plane a une AABB locale quasi nulle en Y, donc sans épaississement de
    // l'échelle Y à la génération, le vide ne détecterait jamais un joueur en chute.
    let s = Scene::tower_demo();
    let vide = s.objects.iter().find(|o| o.name == "Vide").unwrap();
    assert!(
        vide.transform.scale.y > 1.0,
        "l'échelle Y du vide doit être épaissie pour détecter la chute"
    );
    assert!(
        s.deadly_at(vide.transform.position),
        "un joueur en chute au niveau du vide doit mourir"
    );
    // Loin au-dessus (sur une plateforme), on est en sécurité.
    assert!(!s.deadly_at(Vec3::new(0.0, 5.0, 0.0)));
}

#[test]
fn temple_run_demo_is_a_distinct_endless_runner_style() {
    let s = Scene::temple_run_demo();
    // Joueur : course automatique, pas de bouton d'attaque (3ᵉ style, encore différent
    // des deux précédents : ni combat, ni pur platforming vertical).
    let player = s
        .objects
        .iter()
        .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        .expect("un joueur pilotable");
    let ctrl = player.controller.as_ref().unwrap();
    assert!(ctrl.auto_run_speed > 0.0, "la course doit être automatique");
    assert!(
        ctrl.attack_button.is_empty(),
        "pas de combat dans ce style de niveau"
    );
    assert!(!s.objects.iter().any(|o| o.name.starts_with("Ennemi")));

    // Des obstacles mortels (haies/barrages) et des pièces existent.
    assert!(s.objects.iter().any(|o| o.name == "Haie" && o.deadly));
    assert!(s.objects.iter().any(|o| o.name == "Barrage" && o.deadly));
    let coins = s.objects.iter().filter(|o| o.name == "Pièce").count();
    assert!(coins >= 10, "un vrai parcours, pas un décor minimal");

    // Un seul objectif obligatoire : la ligne d'arrivée (les pièces sont des bonus,
    // respawn_delay élevé ⇒ exclues du calcul de victoire).
    let (collected, total) = s.collectibles().expect("un objectif de victoire");
    assert_eq!(collected, 0);
    assert_eq!(total, 1, "seule l'étoile d'arrivée doit être obligatoire");
    assert!(
        s.objects
            .iter()
            .any(|o| o.name == "Étoile Arrivée" && o.respawn_delay == 0.0)
    );
}

#[test]
fn default_clip_prefers_idle_then_first() {
    let mut m = ImportedMesh {
        clips: vec![
            import::Clip::without_tracks("Walk", 1.0),
            import::Clip::without_tracks("Idle", 2.0),
        ],
        ..Default::default()
    };
    assert_eq!(m.default_clip(), Some("Idle"));
    m.clips.remove(1);
    assert_eq!(m.default_clip(), Some("Walk"));
    m.clips.clear();
    assert_eq!(m.default_clip(), None, "mesh statique : rien à jouer");
}

#[test]
fn ensure_default_animations_fills_only_missing_states() {
    let mut scene = Scene::demo(); // Sol/Cube/Sphère : meshes builtin, jamais touchés
    scene.imported.push(ImportedMesh {
        clips: vec![import::Clip::without_tracks("Idle", 2.0)],
        ..Default::default()
    });
    // Un GLB riggé sans état (le bug : T-pose pour toujours) et un autre dont le
    // clip a déjà été choisi (par une démo ou un script) qui doit rester intact.
    scene.objects.push(SceneObject {
        mesh: MeshKind::Imported(0),
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        mesh: MeshKind::Imported(0),
        animation: Some(AnimationState {
            clip: "Walk".into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    scene.ensure_default_animations();
    assert!(
        scene.objects[..3].iter().all(|o| o.animation.is_none()),
        "les meshes builtin ne reçoivent pas d'état d'animation"
    );
    assert_eq!(scene.objects[3].animation.as_ref().unwrap().clip, "Idle");
    assert_eq!(
        scene.objects[4].animation.as_ref().unwrap().clip,
        "Walk",
        "un état existant n'est jamais écrasé"
    );
}

#[test]
fn scene_json_round_trip_preserves_objects() {
    let scene = Scene::demo();
    let json = serde_json::to_string(&scene).unwrap();
    let back: Scene = serde_json::from_str(&json).unwrap();
    assert_eq!(scene.objects.len(), back.objects.len());
    assert_eq!(back.objects[1].name, "Cube");
    assert_eq!(back.objects[1].physics, PhysicsKind::None);
    let p0 = scene.objects[0].transform.position;
    let p1 = back.objects[0].transform.position;
    assert!((p0 - p1).length() < 1e-6);
}

#[test]
fn scene_round_trip_preserves_groups_color_light() {
    let mut scene = Scene::demo();
    scene.groups = vec!["Décor".into(), "Acteurs".into()];
    scene.objects[0].group = "Décor".into();
    scene.objects[1].color = [0.2, 0.4, 0.8];
    scene.light.ambient = 0.5;
    scene.light.color = [1.0, 0.5, 0.25];

    let json = serde_json::to_string(&scene).unwrap();
    let back: Scene = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back.groups,
        vec!["Décor".to_string(), "Acteurs".to_string()]
    );
    assert_eq!(back.objects[0].group, "Décor");
    assert_eq!(back.objects[1].color, [0.2, 0.4, 0.8]);
    assert!((back.light.ambient - 0.5).abs() < 1e-6);
    assert_eq!(back.light.color, [1.0, 0.5, 0.25]);
}

#[test]
fn old_scene_without_new_fields_loads_with_defaults() {
    // Scène d'une version antérieure : ni group, ni color, ni light, ni groups.
    let json = r#"{"objects":[{"name":"X","transform":{"position":[0,0,0],
            "rotation":[0,0,0,1],"scale":[1,1,1]},"mesh":"Cube"}]}"#;
    let s: Scene = serde_json::from_str(json).unwrap();
    assert_eq!(s.objects.len(), 1);
    assert_eq!(s.objects[0].color, [1.0, 1.0, 1.0]);
    assert_eq!(s.objects[0].group, "");
    assert!(s.groups.is_empty());
    assert!((s.light.ambient - 0.25).abs() < 1e-6);
    // Composants récents : valeurs par défaut sûres sur une vieille scène.
    assert!(
        s.objects[0].controller.is_none(),
        "pas pilotable par défaut"
    );
    assert!(
        s.objects[0].visible,
        "visible doit défauter à true (sinon invisible)"
    );
    assert_eq!(s.objects[0].tap_action, TapAction::None);
}

#[test]
fn a_legacy_json_file_loads_at_the_current_version() {
    // Une scène sans champ `version` du tout (fichier antérieur à l'introduction de
    // ce champ) doit ressortir de `Scene::load` au numéro courant, migrations
    // appliquées.
    let json = r#"{"objects":[],"groups":["A","A","B"]}"#;
    let path = std::env::temp_dir().join(format!(
        "rusteegear_legacy_scene_test_{}.json",
        std::process::id()
    ));
    std::fs::write(&path, json).unwrap();
    let scene = Scene::load(path.to_str().unwrap()).unwrap();
    assert_eq!(scene.version, Scene::CURRENT_VERSION);
    assert_eq!(
        scene.groups,
        vec!["A".to_string(), "B".to_string()],
        "la migration doit dédoublonner les groupes d'une scène legacy (version 0)"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_scene_already_at_the_current_version_is_left_untouched_by_migrate() {
    // `migrate` ne doit rien changer à une scène déjà à jour, même avec des
    // doublons de groupe (probablement volontaires, recréés à la main par
    // l'utilisateur) : le nettoyage n'est appliqué qu'à `version == 0`.
    let mut scene = Scene {
        groups: vec!["A".into(), "A".into()],
        version: Scene::CURRENT_VERSION,
        ..Default::default()
    };
    scene.migrate();
    assert_eq!(scene.groups, vec!["A".to_string(), "A".to_string()]);
    assert_eq!(scene.version, Scene::CURRENT_VERSION);
}

/// Sprint 131 : migration v1 → v2, la première migration réelle de ce projet (pas
/// juste un champ absent comblé par `#[serde(default)]`) — une scène `version < 2`
/// avec `roughness: 0.0` (valeur explicitement présente dans le JSON, possible
/// avant que l'inspecteur n'impose un plancher de 0,04) doit être relevée au
/// plancher par `migrate()`.
#[test]
fn migrate_v1_to_v2_raises_zero_roughness_to_the_inspector_floor() {
    let mut scene = Scene {
        objects: vec![SceneObject {
            roughness: 0.0,
            ..Default::default()
        }],
        version: 1,
        ..Default::default()
    };
    scene.migrate();
    assert_eq!(scene.objects[0].roughness, 0.04);
    assert_eq!(scene.version, Scene::CURRENT_VERSION);
}

/// La migration ne doit pas toucher une valeur de roughness déjà au-dessus du
/// plancher (pas une correction générale, juste un relevage du plancher minimal),
/// ni une scène déjà à `CURRENT_VERSION` (même logique que le test de dédoublonnage
/// des groupes ci-dessus — les migrations sont gardées par version, pas rejouées).
#[test]
fn migrate_v1_to_v2_leaves_valid_roughness_and_up_to_date_scenes_untouched() {
    let mut scene = Scene {
        objects: vec![SceneObject {
            roughness: 0.6,
            ..Default::default()
        }],
        version: 1,
        ..Default::default()
    };
    scene.migrate();
    assert_eq!(scene.objects[0].roughness, 0.6);

    let mut already_current = Scene {
        objects: vec![SceneObject {
            roughness: 0.0,
            ..Default::default()
        }],
        version: Scene::CURRENT_VERSION,
        ..Default::default()
    };
    already_current.migrate();
    assert_eq!(
        already_current.objects[0].roughness, 0.0,
        "une scène déjà à jour n'est pas re-corrigée, même avec une valeur \
             qu'une scène plus ancienne aurait fait migrer"
    );
}

/// Dossier temporaire unique par test (même schéma que `assets::tests::
/// temp_assets_dir`) : `Scene::save_prefab_at`/`instantiate_prefab_at`/
/// `sync_prefab_instances_at` ne touchent plus `~/.motor3derust/assets/` réel
/// depuis ce complément — auparavant ces tests y écrivaient réellement (comme le
/// ferait l'éditeur), faute d'une variante testable par répertoire séparé.
fn temp_prefabs_dir(tag: &str) -> std::path::PathBuf {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    tag.hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let dir = std::env::temp_dir().join(format!("rusteegear_prefab_test_{:x}", hasher.finish()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn modifying_a_prefab_updates_its_instances_except_overrides() {
    // Un prefab « gemme » modifié met à jour ses N instances, sauf les propriétés
    // surchargées.
    let dir = temp_prefabs_dir("gemme");
    let gemme_v1 = SceneObject {
        name: "Gemme".into(),
        mesh: MeshKind::Sphere,
        color: [1.0, 1.0, 0.0], // jaune
        tap_action: TapAction::Hide,
        ..Default::default()
    };
    let asset_id = Scene::save_prefab_at(
        &dir,
        &gemme_v1,
        "Gemme",
        &crate::assets::PrefabScope::General,
    )
    .expect("sauvegarde du prefab impossible");

    // 20 instances, chacune à sa propre position (transform/name surchargés par
    // défaut par `instantiate_prefab_at`).
    let mut scene = Scene::default();
    for i in 0..20 {
        let obj = Scene::instantiate_prefab_at(
            &dir,
            &asset_id,
            format!("Gemme {i}"),
            Vec3::new(i as f32, 0.0, 0.0),
        )
        .expect("instanciation impossible");
        scene.objects.push(obj);
    }

    // L'utilisateur retouche la couleur d'une seule instance (#5) à la main : ce
    // champ devient une surcharge, protégée des futures resynchronisations.
    scene.objects[5].color = [1.0, 0.0, 0.0]; // rouge
    scene.objects[5]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .push("color".to_string());

    // Le prefab change de couleur (verte) — sauvegardé sous le même nom/asset_id.
    let gemme_v2 = SceneObject {
        color: [0.0, 1.0, 0.0],
        ..gemme_v1
    };
    Scene::save_prefab_at(
        &dir,
        &gemme_v2,
        "Gemme",
        &crate::assets::PrefabScope::General,
    )
    .unwrap();
    scene.sync_prefab_instances_at(&dir);

    for (i, obj) in scene.objects.iter().enumerate() {
        if i == 5 {
            assert_eq!(
                obj.color,
                [1.0, 0.0, 0.0],
                "l'instance surchargée garde sa couleur"
            );
        } else {
            assert_eq!(
                obj.color,
                [0.0, 1.0, 0.0],
                "l'instance {i} doit suivre la nouvelle couleur du prefab"
            );
        }
        // `transform`/`name` restent propres à chaque instance (surchargés par
        // défaut), jamais écrasés par la resynchronisation.
        assert_eq!(obj.transform.position, Vec3::new(i as f32, 0.0, 0.0));
        assert_eq!(obj.name, format!("Gemme {i}"));
        assert!(
            obj.mesh == MeshKind::Sphere,
            "le mesh doit suivre le template"
        );
        assert_eq!(obj.tap_action, TapAction::Hide);
    }
}

#[test]
fn sync_prefab_instances_leaves_non_prefab_objects_untouched() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Solo".into(),
        color: [0.3, 0.3, 0.3],
        ..Default::default()
    });
    scene.sync_prefab_instances();
    assert_eq!(scene.objects[0].name, "Solo");
    assert_eq!(scene.objects[0].color, [0.3, 0.3, 0.3]);
}

#[test]
fn sync_prefab_instances_is_a_no_op_when_the_prefab_file_is_missing() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Orpheline".into(),
        prefab: Some(PrefabInstance {
            asset_id: "asset-id://inconnu".into(),
            overrides: vec![],
        }),
        ..Default::default()
    });
    // Ne doit pas paniquer, et laisser l'objet inchangé (prefab introuvable).
    scene.sync_prefab_instances();
    assert_eq!(scene.objects[0].name, "Orpheline");
}

#[test]
fn notifies_crossed_detects_a_marker_within_a_simple_forward_step() {
    let markers = vec![(0.5, "hit".to_string())];
    let hit = notifies_crossed(&markers, 0.4, 0.6, 1.0);
    assert_eq!(hit, vec!["hit".to_string()]);
}

#[test]
fn notifies_crossed_ignores_a_marker_outside_the_step() {
    let markers = vec![(0.5, "hit".to_string())];
    assert!(notifies_crossed(&markers, 0.6, 0.8, 1.0).is_empty());
}

#[test]
fn notifies_crossed_handles_the_wraparound_at_the_end_of_the_clip() {
    // Le pas traverse la fin du clip (0.95 -> 0.05 après rebouclage) : un marqueur
    // proche de la fin (0.97) doit être détecté malgré `cur < prev`.
    let markers = vec![(0.97, "fin".to_string())];
    let hit = notifies_crossed(&markers, 0.95, 1.05, 1.0);
    assert_eq!(hit, vec!["fin".to_string()]);
}

#[test]
fn notifies_crossed_is_empty_when_time_is_frozen() {
    // Vitesse nulle (pause, `AnimationState::speed == 0`) : rien ne doit se
    // déclencher en boucle à chaque tick sous prétexte que `prev == cur`.
    let markers = vec![(0.5, "hit".to_string())];
    assert!(notifies_crossed(&markers, 0.5, 0.5, 1.0).is_empty());
}

#[test]
fn notifies_crossed_is_empty_for_a_zero_duration_clip() {
    let markers = vec![(0.0, "hit".to_string())];
    assert!(notifies_crossed(&markers, 0.0, 0.1, 0.0).is_empty());
}

#[test]
fn deadly_zone_detects_player() {
    let mut zone = SceneObject {
        mesh: MeshKind::Cube,
        transform: Transform::from_pos(Vec3::new(0.0, 0.0, 0.0)).with_scale(Vec3::splat(2.0)),
        deadly: true,
        ..Default::default()
    };
    zone.name = "Piège".into();
    let s = Scene {
        objects: vec![zone],
        ..Default::default()
    };
    assert!(s.deadly_at(Vec3::ZERO), "le centre touche la zone");
    assert!(
        !s.deadly_at(Vec3::new(10.0, 0.0, 0.0)),
        "loin = pas de contact"
    );
    // La démo contrôleur a bien une zone mortelle.
    assert!(Scene::controller_demo().objects.iter().any(|o| o.deadly));
}

#[test]
fn collectible_spins_only_while_visible() {
    // Collectible visible : il tourne (rotation ≠ identité après animation).
    let angle = |o: &SceneObject| o.transform.rotation.to_axis_angle().1.abs();
    let mut o = SceneObject {
        tap_action: TapAction::Hide,
        ..Default::default()
    };
    animate_collectible(&mut o, 1.0);
    assert!(angle(&o) > 0.1, "doit tourner si visible");
    // Une fois ramassé (invisible), on ne touche plus à sa rotation.
    let mut o2 = SceneObject {
        tap_action: TapAction::Hide,
        visible: false,
        ..Default::default()
    };
    animate_collectible(&mut o2, 1.0);
    assert!(angle(&o2) < 1e-6, "figé une fois ramassé");
    // Un objet normal (pas un collectible) n'est pas animé.
    let mut n = SceneObject::default();
    animate_collectible(&mut n, 1.0);
    assert!(angle(&n) < 1e-6);
}

/// Sprint 126 : `asset_references` indexe les 4 champs qui peuvent porter une
/// référence `asset-id://` stable (texture, audio, mesh importé, image HUD) et
/// ignore les chemins qui n'ont pas ce schéma (`asset://`/`bundle://` bruts,
/// aucune identité stable à indexer) — cf. sa doc.
#[test]
fn asset_references_indexes_all_four_reference_fields_by_uuid() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Caisse".into(),
        texture: "asset-id://tex-uuid".into(),
        audio: Some(AudioSource {
            clip: "asset-id://audio-uuid".into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    // Chemin `asset://` brut : pas de schéma `asset-id://`, ne doit apparaître
    // dans aucune entrée (aucune identité stable à indexer).
    scene.objects.push(SceneObject {
        name: "Sans référence stable".into(),
        texture: "asset://old_style.png".into(),
        ..Default::default()
    });
    scene.imported.push(ImportedMesh {
        name: "Robot".into(),
        path: "asset-id://mesh-uuid".into(),
        ..Default::default()
    });
    scene.hud_widgets.push(HudWidget {
        id: "icone_vie".into(),
        anchor: HudAnchor::TopLeft,
        offset: [0.0, 0.0],
        size: [32.0, 32.0],
        kind: HudWidgetKind::Image {
            path: "asset-id://hud-uuid".into(),
        },
    });

    let refs = scene.asset_references();
    assert_eq!(refs.len(), 4, "un uuid par référence stable, pas plus");
    assert!(refs["tex-uuid"][0].contains("Caisse"));
    assert!(refs["audio-uuid"][0].contains("Caisse"));
    assert!(refs["mesh-uuid"][0].contains("Robot"));
    assert!(refs["hud-uuid"][0].contains("icone_vie"));
}

#[test]
fn collect_at_picks_up_touched_pieces() {
    let mut s = Scene::controller_demo();
    assert_eq!(s.collectibles().unwrap().0, 0, "rien au départ");
    // On se place exactement sur une pièce (position trouvée dynamiquement).
    let piece_pos = s
        .objects
        .iter()
        .find(|o| o.tap_action == TapAction::Hide && o.visible)
        .map(|o| o.transform.position)
        .unwrap();
    let n = s.collect_at(piece_pos, 0.7).len();
    assert!(n >= 1, "doit ramasser la pièce touchée");
    // Très loin de l'arène : rien ramassé.
    assert!(s.collect_at(Vec3::new(100.0, 0.5, 100.0), 0.7).is_empty());
}

#[test]
fn attack_at_defeats_only_attackable_enemies_in_range() {
    let mut s = Scene::controller_demo();
    let enemies: Vec<_> = s
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| o.name.starts_with("Ennemi"))
        .map(|(i, o)| (i, o.transform.position))
        .collect();
    assert!(enemies.len() >= 3, "au moins 3 ennemis dans la démo");
    for (i, o) in s.objects.iter().enumerate() {
        if o.name.starts_with("Ennemi") {
            assert!(
                o.combat.as_ref().is_some_and(|c| c.attackable),
                "un ennemi doit être une cible d'attaque valide : {i}"
            );
        }
    }
    // Loin de tout ennemi : aucune attaque ne touche.
    assert!(s.attack_at(Vec3::new(100.0, 0.5, 100.0), 1.5).is_empty());
    // Sur le premier ennemi : il est vaincu (masqué), et une deuxième attaque au même
    // endroit ne le retouche pas (déjà invisible).
    let (idx, pos) = enemies[0];
    let hit = s.attack_at(pos, 1.5);
    assert_eq!(hit, vec![idx]);
    assert!(!s.objects[idx].visible, "l'ennemi vaincu devient invisible");
    assert!(
        s.attack_at(pos, 1.5).is_empty(),
        "un ennemi déjà vaincu n'est pas retouché"
    );
}

#[test]
fn attack_zone_at_defeats_every_attackable_target_in_range_at_once() {
    // Contrairement à `attack_at` (une seule cible, cf. sa doc), `attack_zone_at`
    // (mode `AttackMode::Zone`, réservé aux armes qui l'assument via un coût élevé,
    // cf. `Weapon::mode` — le Marteau) doit vaincre TOUT un groupe d'un coup.
    let mk_enemy = |name: &str, pos: Vec3| SceneObject {
        name: name.into(),
        transform: Transform::from_pos(pos),
        combat: Some(Combat {
            attackable: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut s = Scene {
        objects: vec![
            mk_enemy("E1", Vec3::new(0.0, 0.5, 0.0)),
            mk_enemy("E2", Vec3::new(0.5, 0.5, 0.0)),
            mk_enemy("E3", Vec3::new(-0.4, 0.5, 0.3)),
            mk_enemy("Loin", Vec3::new(50.0, 0.5, 0.0)),
        ],
        ..Default::default()
    };

    let hit = s.attack_zone_at(Vec3::ZERO, 2.0);
    assert_eq!(
        hit.len(),
        3,
        "les 3 cibles groupées doivent toutes être vaincues d'un coup"
    );
    for &i in &hit {
        assert!(
            !s.objects[i].visible,
            "chaque cible touchée devient invisible"
        );
    }
    assert!(
        s.objects.iter().find(|o| o.name == "Loin").unwrap().visible,
        "une cible hors de portée ne doit pas être concernée"
    );
    assert!(
        s.attack_zone_at(Vec3::ZERO, 2.0).is_empty(),
        "un groupe déjà vaincu n'est pas retouché"
    );
}

#[test]
fn damage_attackable_survives_until_hp_reaches_zero() {
    // Fondation du duel façon Tekken/Smash (`Scene::brawl_demo`) : une cible à
    // plusieurs PV doit encaisser plusieurs coups, pas tomber au premier — la
    // différence entre `damage_attackable` (décompte `Combat.hp`) et l'ancien
    // masquage immédiat de `attack_at`/`attack_zone_at`.
    let mut s = Scene {
        objects: vec![SceneObject {
            name: "Rival".into(),
            combat: Some(Combat {
                attackable: true,
                hp: 3,
                ..Default::default()
            }),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(
        !s.damage_attackable(0),
        "1er coup : encaisse, ne meurt pas (hp 3 -> 2)"
    );
    assert!(s.objects[0].visible, "encore visible après le 1er coup");
    assert!(
        !s.damage_attackable(0),
        "2e coup : encaisse encore (hp 2 -> 1)"
    );
    assert!(s.objects[0].visible, "encore visible après le 2e coup");
    assert!(
        s.damage_attackable(0),
        "3e coup : achève la cible (hp 1 -> 0)"
    );
    assert!(!s.objects[0].visible, "invisible une fois achevée");
    // Un index invalide ou sans `Combat` ne doit pas paniquer.
    assert!(!s.damage_attackable(99));
}

#[test]
fn brawl_demo_has_a_multi_hit_rival_a_ring_out_void_and_a_single_wave() {
    let s = Scene::brawl_demo();
    // Un seul adversaire (pas des vagues de monstres comme zombies/donjon).
    let rivals: Vec<_> = s.objects.iter().filter(|o| o.ai_chaser.is_some()).collect();
    assert_eq!(rivals.len(), 1, "un seul rival, pas des vagues de monstres");
    let rival = rivals[0];
    let combat = rival
        .combat
        .as_ref()
        .expect("le rival doit être attaquable");
    assert!(combat.attackable);
    assert!(
        combat.hp > 1,
        "le rival doit encaisser plusieurs coups, pas tomber au premier : hp={}",
        combat.hp
    );
    // `wave = 1` : réutilise le système de manches existant pour déclencher la
    // victoire dès que le rival est invisible (achevé ou ring out), sans condition
    // de victoire dédiée à cette démo (cf. doc de `Scene::brawl_demo`).
    assert_eq!(combat.wave, 1);
    assert!(
        rival.trigger,
        "le rival doit pouvoir mordre/frapper au contact"
    );

    // Une zone mortelle (le vide) existe : le ring out doit être possible.
    assert!(
        s.objects.iter().any(|o| o.deadly),
        "l'arène doit avoir une zone mortelle (le vide) pour le ring out"
    );
    // Pas de mur autour de l'arène (contrairement au donjon/zombies) : rien n'empêche
    // physiquement de sortir de l'arène.
    assert!(!s.objects.iter().any(|o| o.name.starts_with("Mur")));

    // Le joueur a une attaque courte et vive (façon jab), pas un tir longue portée.
    let player = s
        .objects
        .iter()
        .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        .expect("un joueur pilotable")
        .controller
        .as_ref()
        .unwrap();
    assert!(
        player.attack_range < 2.0,
        "portée courte, façon corps-à-corps"
    );

    // Une caméra de jeu est définie (cadrage de duel), pas la vue par défaut.
    assert!(s.game_camera.is_some());
    assert!(s.camera_follow);
}

/// Phase C (Sprint 8, `sprint10audit.md`) : le prefab boss respecte le GDD §4
/// (« dernière vague : une créature unique, PV massifs, lente, contact doublé »).
#[test]
fn boss_demo_has_a_single_high_hp_slow_colosse_on_wave_one() {
    let s = Scene::boss_demo();
    let bosses: Vec<_> = s.objects.iter().filter(|o| o.ai_chaser.is_some()).collect();
    assert_eq!(
        bosses.len(),
        1,
        "un unique boss, pas des vagues de monstres"
    );
    let boss = bosses[0];
    let chaser = boss.ai_chaser.as_ref().unwrap();
    assert_eq!(
        chaser.archetype,
        Archetype::Colosse,
        "GDD_MMORPG.md:368 — Colosse est explicitement « aussi le boss »"
    );
    let combat = boss.combat.as_ref().expect("le boss doit être attaquable");
    assert!(combat.attackable);
    assert!(
        combat.hp >= 10,
        "PV massifs (GDD §4) : hp={} doit dépasser largement le rival du Duel (3)",
        combat.hp
    );
    assert_eq!(
        combat.wave, 1,
        "manche unique : sa mort est déjà la dernière manche vidée (cf. AppState::update_round)"
    );
    assert!(boss.trigger, "attaque de contact au trigger");
    assert!(s.objects.iter().any(|o| o.controller.is_some()));
    assert!(
        matches!(boss.mesh, MeshKind::Imported(_)),
        "le boss doit utiliser un vrai modèle (monster_dragon_evolved.glb), pas un primitif — \
             `MeshKind::Capsule` signalerait que l'asset n'a pas pu être chargé"
    );
    assert!(
        !s.imported.is_empty(),
        "le mesh importé du boss doit être présent dans `Scene::imported`"
    );
}

/// Phase C (Sprint 7, `sprint10audit.md`) : le prefab escorte a un convoi
/// attaquable avec une destination distincte de son point de départ, et au moins
/// une créature poursuivante qui pourra le cibler en priorité
/// (`AppState::advance_play`).
#[test]
fn escorte_demo_has_an_attackable_convoy_with_a_reachable_destination() {
    let s = Scene::escorte_demo();
    let convois: Vec<_> = s.objects.iter().filter(|o| o.convoy.is_some()).collect();
    assert_eq!(convois.len(), 1, "un seul convoi par scène Escorte");
    let convoi = convois[0];
    let combat = convoi
        .combat
        .as_ref()
        .expect("le convoi doit être attaquable pour pouvoir être détruit");
    assert!(combat.attackable);
    let route = convoi.convoy.as_ref().unwrap();
    assert!(
        (route.destination - convoi.transform.position).length() > 1.0,
        "la destination doit être distincte du point de départ"
    );
    assert!(
        route.speed > 0.0,
        "un convoi immobile ne peut jamais gagner"
    );
    assert!(
        s.objects.iter().any(|o| o.ai_chaser.is_some()),
        "au moins une créature pour menacer le convoi"
    );
    assert!(
        matches!(convoi.mesh, MeshKind::Imported(_)),
        "le convoi doit utiliser un vrai modèle (nature_cart.glb), pas un primitif — \
             `MeshKind::Cube` signalerait que l'asset n'a pas pu être chargé"
    );
    let chasseresse = s
        .objects
        .iter()
        .find(|o| o.ai_chaser.is_some())
        .expect("au moins une créature");
    assert!(
        matches!(chasseresse.mesh, MeshKind::Imported(_)),
        "la créature poursuivante doit utiliser un vrai modèle, pas une capsule"
    );
    assert_eq!(
        s.imported.len(),
        2,
        "un mesh importé pour le convoi et un pour la chasseresse"
    );
}

#[test]
fn controller_demo_lava_kills_standing_player() {
    // Le mesh Plane a une AABB locale très fine (±0.02 en Y) ; sans épaississement de
    // l'échelle Y à la génération, la lave ne recouperait jamais la hauteur réelle d'un
    // joueur debout (~0.5) et ne tuerait donc jamais personne. Verrouille la correction.
    let s = Scene::controller_demo();
    let lava_top = s
        .objects
        .iter()
        .find(|o| o.name == "Lave")
        .expect("la lave existe");
    assert!(
        lava_top.transform.scale.y > 1.0,
        "l'échelle Y de la lave doit être épaissie pour détecter un joueur debout"
    );
    // Un joueur debout au centre de la lave (hauteur de repos typique d'une capsule).
    assert!(
        s.deadly_at(Vec3::new(0.0, 0.5, 0.0)),
        "un joueur debout sur la lave doit mourir"
    );
    // Mais un joueur en plein saut au-dessus (loin dans les airs) doit pouvoir franchir.
    assert!(
        !s.deadly_at(Vec3::new(0.0, 2.5, 0.0)),
        "un joueur qui saute par-dessus la lave ne doit pas mourir"
    );
}

#[test]
fn collectibles_count_and_win() {
    let mut s = Scene::controller_demo();
    let (collected, total) = s.collectibles().expect("la démo a des collectibles");
    assert!(total >= 3, "au moins 3 gemmes");
    assert_eq!(collected, 0, "rien ramassé au départ");
    // Ramasse tout : chaque collectible devient invisible.
    for o in s
        .objects
        .iter_mut()
        .filter(|o| o.tap_action == TapAction::Hide)
    {
        o.visible = false;
    }
    let (collected, total2) = s.collectibles().unwrap();
    assert_eq!(collected, total2, "tout ramassé = gagné");
    // Une scène sans collectible renvoie None.
    let empty = Scene::default();
    assert!(empty.collectibles().is_none());
}

#[test]
fn tap_actions_apply_correctly() {
    let start = Vec3::new(0.0, 1.0, 0.0);
    // Hide : devient invisible.
    let mut o = SceneObject {
        tap_action: TapAction::Hide,
        ..Default::default()
    };
    apply_tap_action(&mut o, start, 0.0);
    assert!(!o.visible);
    // Grow : grossit mais reste plafonné à 4.
    let mut o = SceneObject {
        tap_action: TapAction::Grow,
        ..Default::default()
    };
    apply_tap_action(&mut o, start, 0.0);
    assert!(o.transform.scale.x > 1.0);
    for _ in 0..50 {
        apply_tap_action(&mut o, start, 0.0);
    }
    assert!(o.transform.scale.x <= 4.0 + 1e-3, "plafonné à 4");
    // Respawn : revient à la position de départ.
    let mut o = SceneObject {
        tap_action: TapAction::Respawn,
        transform: Transform::from_pos(Vec3::new(5.0, 5.0, 5.0)),
        ..Default::default()
    };
    apply_tap_action(&mut o, start, 0.0);
    assert!((o.transform.position - start).length() < 1e-6);
}

#[test]
fn controller_and_ai_chaser_rust_default_matches_serde_default() {
    // Piège classique : `#[derive(Default)]` donne 0.0/vide à chaque champ, alors
    // que plusieurs ont un défaut serde non trivial (`default = "fn"`). Un
    // `Controller { ..Default::default() }` en Rust doit produire les MÊMES valeurs
    // qu'un objet JSON sans ces champs (désérialisé avec les défauts serde) — sinon
    // les scènes construites en Rust (toutes les démos) divergent silencieusement
    // des scènes chargées depuis un fichier ancien.
    let rust_default = Controller::default();
    let from_json: Controller = serde_json::from_str("{}").unwrap();
    assert_eq!(rust_default.move_speed, from_json.move_speed);
    assert_eq!(rust_default.jump_height, from_json.jump_height);
    assert_eq!(rust_default.attack_range, from_json.attack_range);
    assert_eq!(rust_default.attack_cooldown, from_json.attack_cooldown);
    assert!(
        rust_default.attack_cooldown > 0.0,
        "sans quoi l'attaque n'a aucune limite"
    );

    let ai_rust_default = AiChaser::default();
    let ai_from_json: AiChaser = serde_json::from_str("{}").unwrap();
    assert_eq!(ai_rust_default.speed, ai_from_json.speed);
    assert!(
        ai_rust_default.speed > 0.0,
        "sans quoi le chasseur reste immobile"
    );
}

#[test]
fn controller_fields_survive_round_trip() {
    let mut o = SceneObject {
        name: "Joueur".into(),
        ..Default::default()
    };
    o.controller = Some(Controller {
        input: true,
        jump_button: "Saut".into(),
        jump_height: 2.2,
        ..Default::default()
    });
    o.tap_action = TapAction::Hide;
    o.visible = false;
    let scene = Scene {
        objects: vec![o],
        ..Default::default()
    };
    let json = serde_json::to_string(&scene).unwrap();
    let back: Scene = serde_json::from_str(&json).unwrap();
    let b = &back.objects[0];
    let ctrl = b.controller.as_ref().expect("controller round-trip");
    assert!(ctrl.input);
    assert_eq!(ctrl.jump_button, "Saut");
    assert!((ctrl.jump_height - 2.2).abs() < 1e-6);
    assert_eq!(b.tap_action, TapAction::Hide);
    assert!(!b.visible);
}

#[test]
fn audio_source_component_is_optional_and_survives_round_trip() {
    // Un objet sans son garde `audio: None` (pas de bloat JSON pour la majorité des
    // objets). Un objet avec son voit ses 3 champs regroupés survivre à la sérialisation.
    let silent = SceneObject::default();
    assert!(silent.audio.is_none());

    let mut o = SceneObject {
        name: "Ambiance".into(),
        ..Default::default()
    };
    o.audio = Some(AudioSource {
        clip: "assets/wind.wav".into(),
        autoplay: true,
        spatial: true,
        ..Default::default()
    });
    let scene = Scene {
        objects: vec![o],
        ..Default::default()
    };
    let json = serde_json::to_string(&scene).unwrap();
    let back: Scene = serde_json::from_str(&json).unwrap();
    let a = back.objects[0].audio.as_ref().expect("audio round-trip");
    assert_eq!(a.clip, "assets/wind.wav");
    assert!(a.autoplay);
    assert!(a.spatial);
}

#[test]
fn combat_component_is_optional_and_survives_round_trip() {
    // Un objet hors combat garde `combat: None` (décor, collectibles...). Un ennemi
    // voit ses 2 champs regroupés (attackable, is_attack_fx) survivre à la sérialisation.
    let peaceful = SceneObject::default();
    assert!(peaceful.combat.is_none());

    let mut o = SceneObject {
        name: "Ennemi".into(),
        ..Default::default()
    };
    o.combat = Some(Combat {
        attackable: true,
        is_attack_fx: false,
        wave: 2,
        ..Default::default()
    });
    let scene = Scene {
        objects: vec![o],
        ..Default::default()
    };
    let json = serde_json::to_string(&scene).unwrap();
    let back: Scene = serde_json::from_str(&json).unwrap();
    let c = back.objects[0].combat.as_ref().expect("combat round-trip");
    assert!(c.attackable);
    assert!(!c.is_attack_fx);
    assert_eq!(c.wave, 2);
}

#[test]
fn components_demo_exercises_exactly_one_object_per_component() {
    // Scène exemple : chaque composant optionnel (Controller/AudioSource/Combat)
    // n'apparaît que là où il est pertinent, jamais sur les autres objets — c'est
    // tout l'intérêt pédagogique (et la preuve que le bloat plat est bien évité).
    let s = Scene::components_demo();
    assert_eq!(
        s.objects.len(),
        5,
        "5 objets : sol, joueur, boîte, cible, FX"
    );

    let with_controller = s.objects.iter().filter(|o| o.controller.is_some()).count();
    assert_eq!(with_controller, 1, "un seul objet pilotable (le joueur)");

    let with_audio = s.objects.iter().filter(|o| o.audio.is_some()).count();
    assert_eq!(with_audio, 1, "un seul objet sonore (la boîte à musique)");

    let attackable = s
        .objects
        .iter()
        .filter(|o| o.combat.as_ref().is_some_and(|c| c.attackable))
        .count();
    assert_eq!(attackable, 1, "une seule cible d'attaque");

    let fx_anchors = s
        .objects
        .iter()
        .filter(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
        .count();
    assert_eq!(fx_anchors, 1, "une seule ancre d'effet visuel");

    // Le sol n'a aucun des trois : c'est du pur décor.
    let sol = s.objects.iter().find(|o| o.name == "Sol").unwrap();
    assert!(sol.controller.is_none() && sol.audio.is_none() && sol.combat.is_none());
}

#[test]
fn zombies_demo_has_four_waves_of_varied_active_chasers() {
    let s = Scene::zombies_demo();
    let monsters: Vec<_> = s.objects.iter().filter(|o| o.ai_chaser.is_some()).collect();
    // 3 archétypes distincts (Rôdeur/Coureur/Brute), pas un seul type répété.
    let distinct_names: std::collections::HashSet<&str> = monsters
        .iter()
        .map(|o| o.name.split(' ').next().unwrap())
        .collect();
    assert!(
        distinct_names.len() >= 3,
        "au moins 3 archétypes de monstres différents : {distinct_names:?}"
    );
    for m in &monsters {
        assert!(
            m.ai_chaser.is_some(),
            "un monstre doit poursuivre activement, pas suivre un script de patrouille"
        );
        assert!(
            m.combat.as_ref().is_some_and(|c| c.attackable),
            "un monstre doit être une cible d'attaque valide (défendable)"
        );
        assert!(
            m.trigger,
            "un monstre doit détecter le contact pour infliger des dégâts"
        );
        assert!(
            m.combat.as_ref().is_some_and(|c| c.wave > 0),
            "un monstre doit appartenir à une manche"
        );
        assert_eq!(
            m.respawn_delay, 0.0,
            "un monstre vaincu reste mort pour la manche"
        );
    }
    // 4 manches, difficulté croissante (de plus en plus de monstres).
    let max_wave = monsters
        .iter()
        .filter_map(|o| o.combat.as_ref())
        .map(|c| c.wave)
        .max()
        .unwrap();
    assert_eq!(max_wave, 4, "4 manches");
    let per_wave = |w: u32| {
        monsters
            .iter()
            .filter(|o| o.combat.as_ref().is_some_and(|c| c.wave == w))
            .count()
    };
    assert!(
        per_wave(1) < per_wave(4),
        "la dernière manche doit être plus dense"
    );

    // Pas d'objectif « collectible » séparé : la victoire vient de vider les manches
    // (cf. `App::update_waves`), pas de ramasser une gemme.
    assert!(s.collectibles().is_none());
    assert!(!s.objects.iter().any(|o| o.name == "Lave"));

    let player = s
        .objects
        .iter()
        .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        .expect("un joueur pilotable");
    assert!(!player.controller.as_ref().unwrap().attack_button.is_empty());
}

#[test]
fn mmorpg_demo_is_a_bare_arena_with_no_monsters_and_mobile_controls_on() {
    let s = Scene::mmorpg_demo();
    assert!(
        !s.objects.iter().any(|o| o.ai_chaser.is_some()),
        "la démo MMORPG ne doit avoir aucun monstre (test de connectivité, pas de combat)"
    );
    assert!(
        s.mobile.joystick,
        "le joystick doit être actif par défaut, sans passer par l'éditeur (APK direct)"
    );
    let player = s
        .objects
        .iter()
        .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        .expect("un joueur pilotable");
    assert!(!player.controller.as_ref().unwrap().jump_button.is_empty());
}

#[test]
fn mmorpg_demo_has_a_visible_wind_zone_with_no_collider() {
    // Sprint 125, preuve d'implémentation : une zone de vent jouable, pas
    // seulement testée en isolation dans `runtime::physics`.
    let s = Scene::mmorpg_demo();
    let vent = s
        .objects
        .iter()
        .find(|o| o.name == "Zone de vent")
        .expect("une zone de vent dans la démo MMORPG");
    assert!(
        vent.trigger,
        "sans trigger, la zone de vent n'a aucun volume de détection"
    );
    assert!(vent.wind.is_some(), "wind doit être renseigné");
    assert_eq!(
        vent.physics,
        crate::runtime::physics::PhysicsKind::None,
        "une zone de vent ne doit rien bloquer physiquement"
    );
    assert!(
        vent.visible,
        "doit être visible à l'écran, pas un objet caché"
    );
}

#[test]
fn exactly_one_weapon_profile_uses_the_zone_attack_mode() {
    // Le mode `Zone` (frappe tout un groupe d'un coup) reste une exception délibérée,
    // pas la norme : un seul profil l'assume (le Marteau, via son coût le plus élevé
    // de la table — préparation et recharge les plus longues), tous les autres
    // restent en mode `Single` (comportement historique de toutes les démos).
    let zone: Vec<_> = WEAPONS
        .iter()
        .filter(|w| w.mode == AttackMode::Zone)
        .collect();
    assert_eq!(zone.len(), 1, "un seul profil en mode Zone : {zone:?}");
    assert_eq!(zone[0].label, "Marteau");
    assert_eq!(
        zone[0].windup,
        WEAPONS.iter().map(|w| w.windup).fold(0.0, f32::max),
        "le mode Zone doit rester la préparation la plus longue de la table"
    );
}

#[test]
fn roguelike_demo_has_three_rooms_one_monster_each_and_a_random_weapon() {
    let s = Scene::roguelike_demo();
    let monsters: Vec<_> = s.objects.iter().filter(|o| o.ai_chaser.is_some()).collect();
    assert_eq!(monsters.len(), 3, "une salle = un monstre, 3 salles");
    // 3 archétypes distincts (Gobelin/Squelette/Ogre), un par salle.
    let distinct_names: std::collections::HashSet<&str> =
        monsters.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(
        distinct_names.len(),
        3,
        "3 monstres distincts, pas 3 copies du même"
    );
    for m in &monsters {
        assert!(
            m.combat
                .as_ref()
                .is_some_and(|c| c.attackable && c.wave > 0),
            "chaque monstre doit être une cible d'attaque valide, une manche = une salle"
        );
        assert!(m.trigger, "un monstre doit détecter le contact pour mordre");
    }
    // Une salle à la fois : 3 manches distinctes, une par monstre (pas plusieurs
    // monstres entassés dans la même manche comme dans `zombies_demo`).
    let waves: std::collections::HashSet<u32> = monsters
        .iter()
        .filter_map(|o| o.combat.as_ref())
        .map(|c| c.wave)
        .collect();
    assert_eq!(waves, std::collections::HashSet::from([1, 2, 3]));

    // Arme de départ : un des 5 profils connus (`WEAPONS`), jamais les défauts
    // génériques de `Controller` (qui ne correspondent à aucun des 5).
    let player = s
        .objects
        .iter()
        .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        .expect("un joueur pilotable")
        .controller
        .as_ref()
        .unwrap();
    let stat = |w: &Weapon| (w.range, w.cooldown, w.windup);
    let starting = stat(
        WEAPONS
            .iter()
            .find(|w| {
                stat(w)
                    == (
                        player.attack_range,
                        player.attack_cooldown,
                        player.attack_windup,
                    )
            })
            .expect(
                "l'arme de départ doit être l'un des 5 profils connus, pas les défauts génériques",
            ),
    );

    // 2 butins d'arme dans le donjon (cf. `WeaponPickup`), un par salle 1/2 — la
    // salle 3 (l'Ogre) n'en a pas : le joueur doit avoir déjà trouvé sa meilleure
    // arme avant d'y entrer.
    let loot: Vec<_> = s
        .objects
        .iter()
        .filter_map(|o| o.weapon_pickup.map(|wp| WEAPONS[wp.weapon]))
        .collect();
    assert_eq!(
        loot.len(),
        2,
        "2 butins d'arme, un dans chaque première salle"
    );
    // Les 3 armes en jeu (départ + 2 butins) doivent être 3 profils DISTINCTS :
    // sinon trouver un butin n'apporterait rien (même arme que celle déjà en main).
    let mut all_three: std::collections::HashSet<(u32, u32, u32)> = loot
        .iter()
        .map(|w| (w.range.to_bits(), w.cooldown.to_bits(), w.windup.to_bits()))
        .collect();
    all_three.insert((
        starting.0.to_bits(),
        starting.1.to_bits(),
        starting.2.to_bits(),
    ));
    assert_eq!(
        all_three.len(),
        3,
        "l'arme de départ et les 2 butins doivent être 3 profils distincts"
    );

    // Portes fermées (pas de couloir séparé) entre les salles : au moins 4 segments
    // de mur transversal supplémentaires (2 portes à 2 segments chacune), en plus de
    // l'enveloppe extérieure à 4 murs.
    let walls = s
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Mur") || o.name.starts_with("Porte"))
        .count();
    assert!(
        walls >= 8,
        "enveloppe (4 murs) + 2 portes à 2 segments : {walls}"
    );
}

/// Sur un grand nombre de tirages, les profils d'arme tirés ne doivent pas toujours
/// être les mêmes — sinon le tirage serait biaisé (ou codé en dur sur un seul profil).
#[test]
fn roguelike_demo_weapon_draw_is_not_always_the_same_profile() {
    let mut seen: std::collections::HashSet<(u32, u32, u32)> = std::collections::HashSet::new();
    for _ in 0..40 {
        let s = Scene::roguelike_demo();
        let c = s
            .objects
            .iter()
            .find_map(|o| o.controller.as_ref().filter(|c| c.input))
            .unwrap();
        // Bits flottants exacts (valeurs codées en dur, pas de calcul) : comparaison
        // par bits sûre pour un ensemble de discrimination.
        seen.insert((
            c.attack_range.to_bits(),
            c.attack_cooldown.to_bits(),
            c.attack_windup.to_bits(),
        ));
        if seen.len() >= 2 {
            break;
        }
    }
    assert!(
        seen.len() >= 2,
        "40 tirages n'ont produit qu'un seul profil d'arme : le tirage semble figé"
    );
}

#[test]
fn weapon_pickup_at_equips_the_right_profile_and_is_one_shot() {
    let mut s = Scene::roguelike_demo();
    let (pos, expected) = s
        .objects
        .iter()
        .find_map(|o| {
            o.weapon_pickup
                .map(|wp| (o.transform.position, WEAPONS[wp.weapon]))
        })
        .expect("le donjon a au moins un butin d'arme");

    let got = s
        .weapon_pickup_at(pos, 0.9)
        .expect("doit ramasser le butin exactement sur sa position");
    assert_eq!(
        (got.range, got.cooldown, got.windup),
        (expected.range, expected.cooldown, expected.windup),
        "doit renvoyer le profil du butin ramassé, pas un autre"
    );

    // Ramassage à usage unique : retoucher le même endroit ne renvoie plus rien
    // (l'objet a été masqué), contrairement à une pièce qui pourrait réapparaître.
    assert!(
        s.weapon_pickup_at(pos, 0.9).is_none(),
        "un butin déjà ramassé ne doit pas se reramasser"
    );

    // Très loin de tout butin : rien ramassé.
    assert!(
        s.weapon_pickup_at(Vec3::new(500.0, 0.5, 500.0), 0.9)
            .is_none()
    );
}

#[test]
fn hud_anchor_fraction_matches_the_named_corner() {
    assert_eq!(HudAnchor::TopLeft.fraction(), (0.0, 0.0));
    assert_eq!(HudAnchor::TopRight.fraction(), (1.0, 0.0));
    assert_eq!(HudAnchor::BottomLeft.fraction(), (0.0, 1.0));
    assert_eq!(HudAnchor::BottomRight.fraction(), (1.0, 1.0));
    assert_eq!(HudAnchor::Center.fraction(), (0.5, 0.5));
}

#[test]
fn hud_widget_round_trips_through_json_with_its_kind_and_binding_intact() {
    let widgets = vec![
        HudWidget {
            id: "score_label".into(),
            anchor: HudAnchor::TopRight,
            offset: [-10.0, 10.0],
            size: [0.0, 0.0],
            kind: HudWidgetKind::Text {
                content: "Score".into(),
                binding: HudBinding::Score,
            },
        },
        HudWidget {
            id: "health_gauge".into(),
            anchor: HudAnchor::BottomLeft,
            offset: [10.0, -10.0],
            size: [160.0, 18.0],
            kind: HudWidgetKind::Gauge {
                binding: HudBinding::Health,
                max: 1.0,
                color: [0.8, 0.15, 0.15],
            },
        },
        HudWidget {
            id: "restart_btn".into(),
            anchor: HudAnchor::Center,
            offset: [0.0, 0.0],
            size: [140.0, 36.0],
            kind: HudWidgetKind::Button {
                label: "Recommencer".into(),
                action: "restart".into(),
            },
        },
    ];
    let scene = Scene {
        hud_widgets: widgets.clone(),
        ..Default::default()
    };

    let json = serde_json::to_string(&scene).unwrap();
    let back: Scene = serde_json::from_str(&json).unwrap();

    assert_eq!(back.hud_widgets, widgets);
}

#[test]
fn scene_without_hud_widgets_field_deserializes_to_an_empty_vec() {
    // Scène pré-Sprint 109 (JSON antérieur, sans le champ) : ne doit pas échouer
    // à charger, cf. `#[serde(default)]` sur `Scene::hud_widgets`.
    let legacy = r#"{"objects": []}"#;
    let scene: Scene = serde_json::from_str(legacy).unwrap();
    assert!(scene.hud_widgets.is_empty());
}

/// Garde-fou compagnon de `the_embedded_scene_ships_monsters_and_the_fire_button`
/// (`app::fireball`) : chaque mesh `bundle://` référencé par la scène embarquée
/// doit se résoudre **réellement** (clé présente dans `assets/bundle/`, inclus à
/// la compilation) — les deux créatures Blender comprises, squelette inclus. Une
/// clé manquante ne serait sinon qu'un `log::error!` silencieux au chargement :
/// la créature apparaîtrait comme un mesh vide invisible dans le jeu exporté.
/// OUTIL, pas une preuve (lancé explicitement :
/// `cargo test sync_embedded_scene_creatures_from_the_demo -- --ignored --nocapture`) :
/// resynchronise les créatures de `assets/player_scene.json` (la scène
/// embarquée) depuis `Scene::mmorpg_demo()`, la source de vérité — objets
/// « Créature* » remplacés, imports réécrits en `bundle://m{i}_<fichier>`
/// (même ordre d'indices), tag « joueur » posé. Remplace les fusions JSON
/// à la main qui ont déjà perdu 3 fois le contenu multijoueur de cette
/// scène (cf. le garde-fou `the_embedded_scene_creatures_match_the_demo`).
/// Tout le reste du fichier (monstres, tour, boutons) est préservé tel quel.
#[test]
#[ignore = "outil : réécrit assets/player_scene.json, à lancer explicitement"]
fn sync_embedded_scene_creatures_from_the_demo() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
    let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
    let mut embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
    let demo = Scene::mmorpg_demo();

    embedded.objects.retain(|o| !o.name.starts_with("Créature"));
    for obj in demo
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature"))
    {
        embedded.objects.push(obj.clone());
    }
    // Seuls les imports des **créatures** relèvent de cet outil : elles sont
    // ajoutées en premier dans `Scene::mmorpg_demo` (avant le décor nature,
    // Sprint parallèle), donc contiguës en tête de `demo.imported` — ne
    // reconstruit que ces N premières entrées (bundle://m{i}_<fichier>,
    // même convention que `editor::export::bundle_scene_json`), et
    // préserve tel quel le reste d'`embedded.imported` (décor déjà
    // embarqué séparément, hors du périmètre de cet outil).
    let n_creatures = demo
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature"))
        .count();
    let mut imported: Vec<ImportedMesh> = demo
        .imported
        .iter()
        .take(n_creatures)
        .enumerate()
        .map(|(i, m)| {
            let file = std::path::Path::new(&m.path)
                .file_name()
                .and_then(|f| f.to_str())
                .expect("nom de fichier d'import");
            ImportedMesh {
                path: format!("{}m{i}_{file}", crate::assets::SCHEME),
                ..Default::default()
            }
        })
        .collect();
    imported.extend(embedded.imported.into_iter().skip(n_creatures));
    embedded.imported = imported;
    if let Some(joueur) = embedded.objects.iter_mut().find(|o| o.name == "Joueur") {
        joueur.tag = "joueur".into();
    }

    std::fs::write(
        path,
        serde_json::to_string_pretty(&embedded).expect("sérialisation"),
    )
    .expect("écriture de player_scene.json");
    println!(
        "player_scene.json resynchronisé : {} créatures, {} imports",
        embedded
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Créature"))
            .count(),
        embedded.imported.len()
    );
}

/// Outil (portée plus large que `sync_embedded_scene_creatures_from_the_demo`
/// ci-dessus) : remplace tout l'environnement de la scène embarquée
/// (`assets/player_scene.json`) par `Scene::hameau_gdd_demo()` (le nouveau
/// hameau fortifié, cf. la doc de cette fonction) sans jamais toucher les
/// champs listés dans la consigne d'intégration : `mobile`, `hud_layout`,
/// `hud_widgets`, `point_lights`, `camera_follow`, `game_camera`, `sky`,
/// `version`, et l'objet « Joueur » (mesh riggé `fairy_hero` +
/// `fire_button`/`weapon_button`/`heal_button`). Les imports sont réécrits
/// en `bundle://m{i}_<fichier>` (même convention que
/// `editor::export::bundle_scene_json`) — ne compresse rien lui-même, ne
/// fait que réécrire des chemins ; chaque fichier référencé doit déjà
/// exister dans `assets/bundle/` (vrai pour tous les modèles cités par la
/// spec du hameau au moment de l'intégration).
#[test]
#[ignore = "outil : réécrit assets/player_scene.json, à lancer explicitement"]
fn sync_embedded_scene_hameau_from_the_demo() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
    let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
    let embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
    let demo = Scene::hameau_gdd_demo();

    let mut joueur = embedded
        .objects
        .iter()
        .find(|o| o.name == "Joueur")
        .cloned()
        .expect("« Joueur » doit exister dans la scène embarquée actuelle");
    let joueur_mesh_key = match joueur.mesh {
        MeshKind::Imported(i) => embedded.imported.get(i as usize).map(|m| m.path.clone()),
        _ => None,
    };

    let mut objects: Vec<SceneObject> = demo
        .objects
        .into_iter()
        .filter(|o| o.name != "Joueur")
        .collect();
    let mut imported = demo.imported;
    if let Some(key) = joueur_mesh_key {
        let idx = match imported.iter().position(|m: &ImportedMesh| m.path == key) {
            Some(i) => i,
            None => {
                imported.push(ImportedMesh {
                    path: key,
                    ..Default::default()
                });
                imported.len() - 1
            }
        };
        joueur.mesh = MeshKind::Imported(idx as u32);
    }
    objects.push(joueur);

    // Un chemin déjà `bundle://mNN_<fichier>` (cas du mesh du joueur,
    // repris tel quel de la scène embarquée actuelle) doit perdre son
    // ancien préfixe numérique avant d'en recevoir un nouveau — sinon la
    // clé réécrite (`mNN_m126_fairy_hero.glb`) ne correspondrait plus au
    // fichier réellement présent dans `assets/bundle/`.
    fn clean_file_name(path: &str) -> String {
        let file = std::path::Path::new(path)
            .file_name()
            .and_then(|f| f.to_str())
            .expect("nom de fichier d'import")
            .to_string();
        if let Some(rest) = file.strip_prefix('m')
            && let Some(us) = rest.find('_')
            && rest[..us].chars().all(|c| c.is_ascii_digit())
        {
            return rest[us + 1..].to_string();
        }
        file
    }
    let imported: Vec<ImportedMesh> = imported
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let file = clean_file_name(&m.path);
            ImportedMesh {
                path: format!("{}m{i}_{file}", crate::assets::SCHEME),
                ..Default::default()
            }
        })
        .collect();

    let merged = Scene {
        objects,
        imported,
        groups: embedded.groups,
        light: demo.light,
        point_lights: embedded.point_lights,
        mobile: embedded.mobile,
        camera_follow: embedded.camera_follow,
        game_camera: embedded.game_camera,
        // Ciel du hameau fortifié : nuit bleutée avec brouillard léger,
        // conforme à GDD_MMORPG.md §2.3/§10 ("féerique crépusculaire").
        // L'ancien ciel embarqué (hérité de `mmorpg_demo`) était une
        // palette de plein jour — incohérente avec la fiction et avec
        // `Sky::default()` qui, lui, est déjà nocturne.
        sky: Sky {
            horizon_color: [0.10, 0.11, 0.20],
            zenith_color: [0.04, 0.05, 0.12],
            fog_color: [0.09, 0.10, 0.16],
            fog_density: 0.02,
            bloom_intensity: 0.9,
        },
        version: embedded.version,
        hud_layout: embedded.hud_layout,
        hud_widgets: embedded.hud_widgets,
    };

    std::fs::write(
        path,
        serde_json::to_string_pretty(&merged).expect("sérialisation"),
    )
    .expect("écriture de player_scene.json");
    println!(
        "player_scene.json remplacé par le hameau fortifié : {} objets, {} imports",
        merged.objects.len(),
        merged.imported.len()
    );
}

/// Garde-fou compagnon de `sync_embedded_scene_creatures_from_the_demo` :
/// les créatures de la scène embarquée doivent rester **identiques** à
/// celles de `Scene::mmorpg_demo`
/// (script, collisions, trigger, mesh, physique) — c'est la démo qui est la
/// source de vérité, la scène embarquée n'en est qu'une copie avec des
/// chemins `bundle://`. Une divergence = quelqu'un a modifié une créature
/// d'un seul côté : relancer l'outil de synchronisation.
#[test]
fn the_embedded_scene_creatures_match_the_demo() {
    let embedded = Scene::embedded_player();
    let demo = Scene::mmorpg_demo();
    let demo_creatures: Vec<&SceneObject> = demo
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature"))
        .collect();
    assert!(!demo_creatures.is_empty());
    for d in demo_creatures {
        let e = embedded
            .objects
            .iter()
            .find(|o| o.name == d.name)
            .unwrap_or_else(|| {
                panic!(
                    "« {} » absente de la scène embarquée — lancer `cargo test \
                         sync_embedded_scene_creatures_from_the_demo -- --ignored`",
                    d.name
                )
            });
        let sync_hint = "désynchronisé de la démo — lancer `cargo test \
                 sync_embedded_scene_creatures_from_the_demo -- --ignored`";
        assert_eq!(e.script, d.script, "script de « {} » {sync_hint}", d.name);
        assert_eq!(
            e.trigger, d.trigger,
            "trigger de « {} » {sync_hint}",
            d.name
        );
        assert_eq!(
            e.collision_layer, d.collision_layer,
            "couche de « {} » {sync_hint}",
            d.name
        );
        assert!(e.mesh == d.mesh, "mesh de « {} » {sync_hint}", d.name);
        assert!(
            e.physics == d.physics,
            "physique de « {} » {sync_hint}",
            d.name
        );
    }
    // Imports : seuls ceux référencés par les créatures doivent correspondre
    // (même indice, même fichier — juste `bundle://` au lieu du chemin
    // disque). La démo peut porter d'autres imports (décor nature ajouté
    // par le chantier MMORPG) sans que la scène embarquée n'y soit tenue.
    for d in demo
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature"))
    {
        let MeshKind::Imported(i) = d.mesh else {
            panic!("« {} » devrait être un mesh importé", d.name);
        };
        let demo_file = std::path::Path::new(&demo.imported[i as usize].path)
            .file_name()
            .and_then(|f| f.to_str())
            .expect("nom de fichier d'import démo");
        let embedded_path = &embedded
            .imported
            .get(i as usize)
            .unwrap_or_else(|| {
                panic!("import {i} absent de la scène embarquée — lancer l'outil de sync")
            })
            .path;
        assert!(
            embedded_path.ends_with(demo_file),
            "import {i} : « {embedded_path} » devrait pointer le même fichier que la \
                 démo (« {demo_file} ») — lancer l'outil de synchronisation"
        );
    }
    assert_eq!(
        embedded
            .objects
            .iter()
            .find(|o| o.name == "Joueur")
            .map(|o| o.tag.as_str()),
        Some("joueur"),
        "le joueur embarqué doit porter le tag « joueur » (scripts des créatures 12/13)"
    );
}

/// Garde-fou du trou de synchro démo ↔ scène servie : l'authoring des vagues
/// (`mmorpg_demo_waves_follow_the_gdd_authoring_rules`, cf. `demos.rs`) ne
/// valide que `Scene::mmorpg_demo()`, alors que la scène réellement servie en
/// ligne est `assets/player_scene.json` (embarquée via `embedded_player`,
/// réécrite par `editor::export::bundle_scene_json` et les outils
/// `sync_embedded_scene_*`). Sans ce test, on peut retoucher les vagues de la
/// démo, garder tous les tests verts, et laisser la scène en ligne diverger.
/// Comparaison **structurelle** (aucune constante en dur) : chaque créature
/// attaquable doit porter la même manche (`Combat::wave`) et les mêmes PV
/// (`Combat::hp`) des deux côtés, appariée par nom — plus précis qu'un simple
/// multiset (wave, hp), qui laisserait passer un échange de stats entre deux
/// créatures. Lit le JSON du disque (pas `embedded_player()`, dont le repli
/// silencieux vers `Scene::demo()` masquerait un fichier corrompu).
#[test]
fn the_embedded_scene_waves_and_hp_match_the_demo() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
    let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
    let embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
    let demo = Scene::mmorpg_demo();

    // name → (wave, hp) des cibles attaquables d'une scène.
    fn combat_stats(scene: &Scene) -> std::collections::BTreeMap<&str, (u32, u32)> {
        scene
            .objects
            .iter()
            .filter_map(|o| {
                let c = o.combat.as_ref()?;
                c.attackable.then_some((o.name.as_str(), (c.wave, c.hp)))
            })
            .collect()
    }
    let demo_stats = combat_stats(&demo);
    let embedded_stats = combat_stats(&embedded);
    assert!(
        !demo_stats.is_empty(),
        "la démo MMORPG doit avoir des cibles attaquables"
    );
    let sync_hint = "démo et scène servie divergent — relancer `cargo test \
             sync_embedded_scene_creatures_from_the_demo -- --ignored` puis \
             recompiler (cf. embedded-scene-export-overwrite-trap)";
    assert_eq!(
        demo_stats.len(),
        embedded_stats.len(),
        "nombre de cibles attaquables : {sync_hint}"
    );
    for (name, &(wave, hp)) in &demo_stats {
        let &(e_wave, e_hp) = embedded_stats
            .get(name)
            .unwrap_or_else(|| panic!("« {name} » absente de la scène embarquée : {sync_hint}"));
        assert_eq!(
            (e_wave, e_hp),
            (wave, hp),
            "(wave, hp) de « {name} » : {sync_hint}"
        );
    }
}

/// Garde-fou décor (Phase B, Sprint 1) : le même trou de synchro démo ↔
/// scène servie que `the_embedded_scene_creatures_match_the_demo`, mais
/// pour le décor du siège (remparts/tours) et la faune, jamais couvert.
/// Source de vérité du décor : `Scene::hameau_gdd_demo()` (cf.
/// `sync_embedded_scene_hameau_from_the_demo` ci-dessus), pas
/// `mmorpg_demo()` qui ne porte que les créatures/vagues. Comparaison
/// **pure** (aucune réécriture du fichier) : multiset des noms d'objets
/// « Rempart… »/« Tour … »/« Faune… », des deux côtés.
#[test]
fn the_embedded_scene_decor_and_wildlife_match_the_demo() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
    let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
    let embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
    let demo = Scene::hameau_gdd_demo();

    fn names<'a>(scene: &'a Scene, prefix: &str) -> std::collections::BTreeSet<&'a str> {
        scene
            .objects
            .iter()
            .filter(|o| o.name.starts_with(prefix))
            .map(|o| o.name.as_str())
            .collect()
    }
    let sync_hint = "décor/faune désynchronisés entre la démo et la scène servie — \
             ré-exporter assets/player_scene.json";
    for prefix in ["Rempart", "Tour ", "Faune"] {
        let demo_names = names(&demo, prefix);
        let embedded_names = names(&embedded, prefix);
        assert!(
            !demo_names.is_empty(),
            "la démo MMORPG doit contenir des objets « {prefix}… »"
        );
        assert_eq!(
            demo_names, embedded_names,
            "objets « {prefix}… » : {sync_hint}"
        );
    }
}

#[test]
fn the_embedded_scene_resolves_its_bundle_creatures() {
    let scene = Scene::embedded_player();
    for name in ["Créature", "Créature 2"] {
        assert!(
            scene.objects.iter().any(|o| o.name == name),
            "la scène embarquée doit contenir « {name} » (cf. assets/player_scene.json)"
        );
    }
    assert!(
        scene.imported.len() >= 2,
        "la scène embarquée doit référencer les deux glb de créatures \
             (imports trouvés : {})",
        scene.imported.len()
    );
    for m in &scene.imported {
        assert!(
            !m.data.vertices.is_empty(),
            "mesh embarqué « {} » non résolu (clé absente du bundle ?)",
            m.path
        );
    }
    // Seuls les imports référencés par les **créatures** (et le joueur riggé)
    // doivent être skinnés : depuis le décor village/nature embarqué
    // (cf. commits « Décor MMORPG »), la scène référence aussi des meshes
    // statiques (pont, cabane…) légitimement sans squelette.
    for o in scene
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature") || o.tag == "joueur")
    {
        let MeshKind::Imported(i) = o.mesh else {
            continue;
        };
        let m = &scene.imported[i as usize];
        assert!(
            m.skeleton.is_some(),
            "mesh embarqué « {} » (objet « {} ») doit être skinné (rig requis)",
            m.path,
            o.name
        );
    }
}

/// Préfixes de noms du décor ambiant intégré (faune 27-61 + flore/objets des
/// packs Blender headless générés en 2026-07) : partagés entre l'outil de
/// synchro et son garde-fou compagnon ci-dessous. Aucun ne recoupe
/// « Créature » (combat, `MMORPG_CREATURES`) ni les préfixes déjà utilisés
/// par le décor historique (`NATURE_DECOR`/`VILLAGE_PROPS`/`MONSTER_DECOR`) —
/// ni, surtout, « Faune » : déjà utilisé par `hameau_gdd_demo()`
/// (`Faune {n} {cluster}-{poses}`, cf. `faune_scatter`), présent dans la
/// scène déjà embarquée. Un run avec « Faune » comme préfixe ici a
/// effectivement retiré 119 de ces objets sans les réinjecter (la source de
/// vérité de cet outil est `Scene::mmorpg_demo`, qui ne les contient pas) —
/// détecté par `git diff` avant tout commit, corrigé en renommant en
/// « Errant » ci-dessous.
const AMBIENT_DECOR_PREFIXES: &[&str] = &[
    "Errant ",
    "Arbre exotique",
    "Mobilier du hameau",
    "Rocher moussu",
    "Sous-bois exotique",
    "Fleur des prés",
    "Culture ",
    "Rive du lac",
    "Décor du hameau",
    "Établi d'armes",
    "Étal des vivres",
    "Coin trésor",
    "Table d'apothicaire",
    // Prairie centrale élargie + haltes à mi-distance (audit de composition
    // du paysage, capture en jeu : grand aplat vert vide entre le spawn et
    // les biomes). Vérifié : ni "Prairie", ni "Halte" ne préfixe aucun nom
    // déjà embarqué (contrairement à l'incident "Faune"/`hameau_gdd_demo`).
    "Prairie centrale",
    "Halte ",
];

/// OUTIL (portée : décor ambiant ajouté à `Scene::mmorpg_demo` — faune 27-61
/// non combattante + flore/objets complémentaires), pas une preuve (lancé
/// explicitement : `cargo test sync_embedded_scene_ambient_decor_from_the_demo
/// -- --ignored --nocapture`). Même patron que
/// `sync_embedded_scene_creatures_from_the_demo` (retire puis réinjecte les
/// objets du préfixe visé, réécrit leurs imports en `bundle://m{i}_<fichier>`,
/// préserve tout le reste), en PUREMENT ADDITIF sur `assets/bundle/` :
/// contrairement aux deux outils existants (qui supposent leurs fichiers déjà
/// bundlés), celui-ci copie et compresse zstd chaque fichier réellement
/// nouveau — jamais de suppression, jamais de renumérotation des entrées déjà
/// présentes dans `assets/player_scene.json` (la numérotation continue après
/// la dernière entrée `imported` existante).
#[test]
#[ignore = "outil : réécrit assets/player_scene.json et copie/compresse dans assets/bundle/, à lancer explicitement"]
#[cfg(not(target_arch = "wasm32"))]
fn sync_embedded_scene_ambient_decor_from_the_demo() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
    let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
    let mut embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
    let demo = Scene::mmorpg_demo();

    // Idempotent : un second run retire d'abord toute instance issue d'un
    // run précédent avant de réinjecter celles de la démo (pas de doublons).
    embedded
        .objects
        .retain(|o| !AMBIENT_DECOR_PREFIXES.iter().any(|p| o.name.starts_with(p)));

    let to_add: Vec<&SceneObject> = demo
        .objects
        .iter()
        .filter(|o| AMBIENT_DECOR_PREFIXES.iter().any(|p| o.name.starts_with(p)))
        .collect();
    assert!(
        !to_add.is_empty(),
        "la démo doit contenir le nouveau décor ambiant (faune 27-61, flore, objets)"
    );

    let bundle_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bundle"));
    std::fs::create_dir_all(bundle_dir).expect("assets/bundle/ doit exister");

    // Indice d'import démo → indice d'import embarqué : réutilise une entrée
    // déjà présente si le même fichier y est déjà référencé (dédoublonnage,
    // p. ex. `nature_tree.glb` déjà embarqué par un autre outil), sinon crée
    // une entrée neuve en continuant la numérotation.
    let mut index_map: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut next_index = embedded.imported.len() as u32;
    for obj in &to_add {
        let MeshKind::Imported(demo_idx) = obj.mesh else {
            continue;
        };
        if index_map.contains_key(&demo_idx) {
            continue;
        }
        let demo_path = demo.imported[demo_idx as usize].path.clone();
        let file = std::path::Path::new(&demo_path)
            .file_name()
            .and_then(|f| f.to_str())
            .expect("nom de fichier d'import")
            .to_string();
        let embedded_idx = match embedded
            .imported
            .iter()
            .position(|m| m.path.ends_with(&file))
        {
            Some(i) => i as u32,
            None => {
                let key = format!("m{next_index}_{file}");
                let bundle_path = bundle_dir.join(&key);
                if !bundle_path.exists() {
                    let data = std::fs::read(&demo_path)
                        .unwrap_or_else(|e| panic!("lecture de {demo_path} : {e}"));
                    // Même appel que `editor::export::copy_to_bundle` — un seul
                    // frame zstd niveau par défaut, décompressé par
                    // `assets::bundle_bytes` à la lecture.
                    let compressed =
                        zstd::stream::encode_all(data.as_slice(), 0).expect("compression zstd");
                    std::fs::write(&bundle_path, compressed)
                        .unwrap_or_else(|e| panic!("écriture de {key} : {e}"));
                }
                embedded.imported.push(ImportedMesh {
                    path: format!("{}{key}", crate::assets::SCHEME),
                    ..Default::default()
                });
                let idx = next_index;
                next_index += 1;
                idx
            }
        };
        index_map.insert(demo_idx, embedded_idx);
    }

    let n_added = to_add.len();
    for obj in to_add {
        let mut clone = obj.clone();
        if let MeshKind::Imported(demo_idx) = clone.mesh {
            clone.mesh = MeshKind::Imported(index_map[&demo_idx]);
        }
        embedded.objects.push(clone);
    }

    std::fs::write(
        path,
        serde_json::to_string_pretty(&embedded).expect("sérialisation"),
    )
    .expect("écriture de player_scene.json");
    println!(
        "player_scene.json : décor ambiant synchronisé ({n_added} objets ajoutés, {} imports au total)",
        embedded.imported.len()
    );
}

/// Garde-fou compagnon de `sync_embedded_scene_ambient_decor_from_the_demo` :
/// le décor ambiant (faune 27-61 + flore/objets) doit être présent et
/// résolu dans la scène embarquée après synchronisation — même logique que
/// `the_embedded_scene_creatures_match_the_demo`/
/// `the_embedded_scene_resolves_its_bundle_creatures`, étendue aux nouveaux
/// préfixes.
#[test]
fn the_embedded_scene_ambient_decor_matches_the_demo() {
    let embedded = Scene::embedded_player();
    let demo = Scene::mmorpg_demo();

    let demo_names: std::collections::BTreeSet<&str> = demo
        .objects
        .iter()
        .filter(|o| AMBIENT_DECOR_PREFIXES.iter().any(|p| o.name.starts_with(p)))
        .map(|o| o.name.as_str())
        .collect();
    assert!(
        !demo_names.is_empty(),
        "la démo doit contenir le nouveau décor ambiant"
    );
    let sync_hint = "lancer `cargo test sync_embedded_scene_ambient_decor_from_the_demo \
             -- --ignored --nocapture`";
    for name in &demo_names {
        assert!(
            embedded.objects.iter().any(|o| o.name == *name),
            "« {name} » absent de la scène embarquée — {sync_hint}"
        );
    }
    for o in embedded
        .objects
        .iter()
        .filter(|o| AMBIENT_DECOR_PREFIXES.iter().any(|p| o.name.starts_with(p)))
    {
        let MeshKind::Imported(i) = o.mesh else {
            continue;
        };
        let m = embedded.imported.get(i as usize).unwrap_or_else(|| {
            panic!(
                "« {} » référence un import {i} absent — {sync_hint}",
                o.name
            )
        });
        assert!(
            !m.data.vertices.is_empty(),
            "mesh embarqué « {} » (objet « {} ») non résolu (clé absente d'assets/bundle/ ?) \
                 — {sync_hint}",
            m.path,
            o.name
        );
    }
}

/// OUTIL, pas une preuve (lancé explicitement : `cargo test
/// sync_embedded_scene_pickups_from_the_demo -- --ignored --nocapture`) :
/// `Scene::mmorpg_demo` définit des `ItemPickup` (potions, baies, clé,
/// gemme — `MMORPG_ITEMS`) qu'aucun des trois outils `sync_embedded_scene_*`
/// existants ne reporte sur la scène servie (le remplacement d'environnement
/// vient de `hameau_gdd_demo`, qui n'en définit aucun ; le décor ambiant ne
/// filtre que les préfixes de `AMBIENT_DECOR_PREFIXES`) — audité dans
/// SPRINT3D_AUDIT_GAMEDESIGN.md §4 : la carte servie n'avait donc **aucun**
/// objet à ramasser, contredisant GDD_MMORPG.md §5.1/§15.4/§17.1. Ces objets
/// utilisent des meshes primitifs (`Sphere`/`Capsule`, cf. `DemoItem` dans
/// `demos.rs`) : aucun import/bundle à gérer, contrairement aux deux autres
/// outils. Idempotent : retire d'abord toute instance déjà synchronisée
/// (marquée par `item_pickup.is_some()`) avant de réinjecter celles de la
/// démo.
#[test]
#[ignore = "outil : réécrit assets/player_scene.json, à lancer explicitement"]
fn sync_embedded_scene_pickups_from_the_demo() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
    let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
    let mut embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
    let demo = Scene::mmorpg_demo();

    embedded.objects.retain(|o| o.item_pickup.is_none());
    let to_add: Vec<&SceneObject> = demo
        .objects
        .iter()
        .filter(|o| o.item_pickup.is_some())
        .collect();
    assert!(
        !to_add.is_empty(),
        "la démo doit contenir des `ItemPickup` (GDD §5.1/§15.4)"
    );
    for obj in &to_add {
        assert!(
            matches!(obj.mesh, MeshKind::Sphere | MeshKind::Capsule),
            "« {} » : cet outil ne gère que des meshes primitifs, pas d'import à \
                 bundler (ajouter la gestion d'import si un futur pickup en a besoin)",
            obj.name
        );
    }
    let n_added = to_add.len();
    for obj in to_add {
        embedded.objects.push(obj.clone());
    }

    std::fs::write(
        path,
        serde_json::to_string_pretty(&embedded).expect("sérialisation"),
    )
    .expect("écriture de player_scene.json");
    println!("player_scene.json : {n_added} objets ramassables synchronisés");
}

/// Garde-fou compagnon de `sync_embedded_scene_pickups_from_the_demo`.
#[test]
fn the_embedded_scene_has_item_pickups_from_the_demo() {
    let embedded = Scene::embedded_player();
    let demo = Scene::mmorpg_demo();
    let demo_names: std::collections::BTreeSet<&str> = demo
        .objects
        .iter()
        .filter(|o| o.item_pickup.is_some())
        .map(|o| o.name.as_str())
        .collect();
    assert!(
        !demo_names.is_empty(),
        "la démo doit contenir des `ItemPickup`"
    );
    let sync_hint = "lancer `cargo test sync_embedded_scene_pickups_from_the_demo \
             -- --ignored --nocapture`";
    for name in &demo_names {
        assert!(
            embedded
                .objects
                .iter()
                .any(|o| o.name == *name && o.item_pickup.is_some()),
            "« {name} » absent (ou sans `item_pickup`) de la scène embarquée — {sync_hint}"
        );
    }
}

/// Synchronise le convoi (GDD §4, mode Escorte) depuis `Scene::mmorpg_demo`
/// vers la scène embarquée — absent jusqu'ici (Phase L, `sprintreflecion.md` :
/// un salon réseau en `RoundObjective::Escorte` ne se terminait jamais faute
/// de `convoy` dans la scène réellement servie). Réutilise l'import existant
/// de `nature_cart.glb` si `sync_embedded_scene_ambient_decor_from_the_demo`
/// (ou un autre outil) l'a déjà bundlé, sinon en ajoute un nouveau en fin de
/// `imported` (clé `bundle://m{i}_nature_cart.glb`, même convention que les
/// autres outils de cette famille) — le fichier source doit déjà exister
/// dans `assets/bundle/` (`bundle_missing_assets_referenced_by_the_embedded_scene`
/// le complète sinon).
#[test]
#[ignore = "outil : réécrit assets/player_scene.json, à lancer explicitement"]
fn sync_embedded_scene_convoy_from_the_demo() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
    let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
    let mut embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
    let demo = Scene::mmorpg_demo();

    let demo_convoi = demo
        .objects
        .iter()
        .find(|o| o.convoy.is_some())
        .expect("Scene::mmorpg_demo doit contenir un objet `convoy`");
    let demo_mesh_key = match demo_convoi.mesh {
        MeshKind::Imported(i) => demo.imported[i as usize]
            .path
            .rsplit('/')
            .next()
            .expect("nom de fichier")
            .to_string(),
        _ => panic!("le convoi de la démo doit référencer un mesh importé"),
    };

    embedded.objects.retain(|o| o.convoy.is_none());
    let existing_key = embedded
        .imported
        .iter()
        .position(|m| m.path.ends_with(&demo_mesh_key));
    let mesh_index = match existing_key {
        Some(i) => i as u32,
        None => {
            let i = embedded.imported.len() as u32;
            embedded.imported.push(ImportedMesh {
                path: format!("{}m{i}_{demo_mesh_key}", crate::assets::SCHEME),
                ..Default::default()
            });
            i
        }
    };

    let mut convoi = demo_convoi.clone();
    convoi.mesh = MeshKind::Imported(mesh_index);
    embedded.objects.push(convoi);

    std::fs::write(
        path,
        serde_json::to_string_pretty(&embedded).expect("sérialisation"),
    )
    .expect("écriture de player_scene.json");
    println!(
        "player_scene.json : convoi synchronisé (mesh index {mesh_index}, {} imports au total)",
        embedded.imported.len()
    );
}

/// Garde-fou compagnon de `sync_embedded_scene_convoy_from_the_demo`.
#[test]
fn the_embedded_scene_has_a_convoy_for_the_escorte_mode() {
    let embedded = Scene::embedded_player();
    assert!(
        embedded.objects.iter().any(|o| o.convoy.is_some()),
        "la scène embarquée doit contenir un objet `convoy` (mode Escorte, GDD §4) — \
             lancer `cargo test sync_embedded_scene_convoy_from_the_demo -- --ignored --nocapture`"
    );
}
