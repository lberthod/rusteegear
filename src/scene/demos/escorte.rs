use super::*;

impl Scene {
    /// Démo « Escorte » (Phase C, Sprint 7 de `sprint10audit.md`, `RoundObjective::Escorte`) :
    /// un convoi lent traverse un couloir d'une porte à l'autre (GDD §4) pendant que des
    /// créatures le prennent pour cible en priorité (cf. `AppState::update_escorte` et le
    /// ciblage prioritaire dans `AppState::advance_play`). Victoire à l'arrivée, défaite
    /// si le convoi est détruit avant (`AppState::is_room_lost`).
    pub fn escorte_demo() -> Self {
        let longueur = 40.0_f32;
        let mut imported: Vec<ImportedMesh> = Vec::new();

        let mut sol = demo_obj("Couloir", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(8.0, 1.0, longueur + 8.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.2, 0.18, 0.14];

        let mut joueur = demo_obj(
            "Joueur",
            MeshKind::Capsule,
            Vec3::new(-2.0, 1.0, -longueur / 2.0 + 2.0),
        );
        joueur.color = [0.9, 0.75, 0.3];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 5.0,
            jump_button: "Saut".into(),
            jump_height: 1.2,
            attack_button: "Attaque".into(),
            attack_range: 2.0,
            attack_cooldown: 0.4,
            attack_windup: 0.12,
            ..Default::default()
        });

        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.9, 0.6];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        // Modèle réel plutôt qu'un cube (« chariot lent », GDD §4) : même
        // asset que la démo « Charrette » du hameau (`nature_cart.glb`, décor
        // déjà utilisé ailleurs à l'échelle 1.0 — cf. `NATURE_DECOR`), repli
        // sur un cube si l'asset est introuvable.
        let convoi_mesh = import_single_model(&mut imported, "nature_cart.glb", MeshKind::Cube);
        let mut convoi = demo_obj(
            "Convoi — chariot de braises",
            convoi_mesh,
            Vec3::new(0.0, 0.0, -longueur / 2.0),
        );
        convoi.transform.rotation = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        convoi.emissive = 0.3;
        convoi.combat = Some(Combat {
            attackable: true,
            hp: 8,
            ..Default::default()
        });
        convoi.convoy = Some(Convoy {
            destination: Vec3::new(0.0, 0.0, longueur / 2.0),
            speed: 1.2,
        });

        // Créature réelle plutôt qu'une capsule teintée : silhouette figée
        // (comme `monster_dragon_evolved.glb` du boss, cf. sa doc) mais
        // suffisante pour un chasseur qui fonce en ligne droite (`AiChaser`),
        // sans animation de marche à proprement parler.
        let chasseresse_mesh =
            import_single_model(&mut imported, "monster_alien.glb", MeshKind::Capsule);
        let mut chasseresse = demo_obj("Chasseresse", chasseresse_mesh, Vec3::new(3.0, 0.0, 0.0));
        chasseresse.trigger = true;
        chasseresse.ai_chaser = Some(AiChaser {
            speed: 3.0,
            archetype: Archetype::Traqueuse,
        });
        chasseresse.combat = Some(Combat {
            attackable: true,
            hp: 2,
            ..Default::default()
        });
        chasseresse.respawn_delay = 0.0;
        chasseresse.script = "if obj.triggered then damage(0.8 * dt) end".into();

        Scene {
            objects: vec![sol, joueur, fx, convoi, chasseresse],
            imported,
            camera_follow: true,
            game_camera: Some(GameCamera {
                target: [0.0, 1.5, 0.0],
                yaw: 0.0,
                pitch: 0.5,
                distance: 10.0,
            }),
            point_lights: vec![PointLight {
                position: [0.0, 6.0, -longueur / 2.0],
                color: [1.0, 0.6, 0.3],
                intensity: 1.2,
                range: 24.0,
                ..PointLight::default()
            }],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
