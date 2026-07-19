use super::*;

impl Scene {
    /// Démo « Boss » (Phase C, Sprint 8 de `sprint10audit.md`, `RoundObjective::Boss`) :
    /// arène fermée, un unique adversaire à PV massifs, lent, contact doublé (GDD §4 :
    /// « dernière vague : une créature unique, PV massifs, lente, contact doublé » —
    /// archétype `Colosse`, cf. `GDD_MMORPG.md:368` « c'est aussi le boss »). Une seule
    /// manche (`Combat::wave: 1`) contenant le boss : `AppState::update_round` gagne la
    /// partie dès qu'elle est vidée (comportement `Vagues`, cf. sa doc), donc « mort du
    /// boss » et « dernière manche vidée » sont ici la même condition — pas de logique
    /// de victoire dédiée à écrire pour ce sprint, juste ce contenu.
    pub fn boss_demo() -> Self {
        let half = 10.0_f32;
        let mut imported: Vec<ImportedMesh> = Vec::new();

        let mut sol = demo_obj("Arène", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.14, 0.12, 0.16];
        sol.roughness = 0.7;

        let mut mur = demo_obj("Mur d'arène", MeshKind::Cube, Vec3::new(0.0, 2.0, -half));
        mur.transform = mur.transform.with_scale(Vec3::new(2.0 * half, 4.0, 0.6));
        mur.physics = PhysicsKind::Static;
        mur.color = [0.2, 0.18, 0.24];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 6.0));
        joueur.color = [0.9, 0.75, 0.3];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.2,
            attack_button: "Attaque".into(),
            attack_range: 1.6,
            attack_cooldown: 0.45,
            attack_windup: 0.15,
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

        // Modèle réel plutôt qu'un primitif (le GDD §4 nomme explicitement
        // l'archétype Colosse « yéti, dragon, roi-champignon, alpaking » —
        // GDD_MMORPG.md:368) : `monster_dragon_evolved.glb`, silhouette figée
        // sans squelette (comme tout `MONSTER_DECOR`, cf. sa doc — armature
        // retirée à l'export), suffisant pour un adversaire massif qui charge
        // plus qu'il n'anime. Repli sur une capsule si l'asset est introuvable.
        let boss_mesh = import_single_model(
            &mut imported,
            "monster_dragon_evolved.glb",
            MeshKind::Capsule,
        );
        let mut boss = demo_obj(
            "Boss — L'Aînée de la lande",
            boss_mesh,
            Vec3::new(0.0, 1.4, -4.0),
        );
        boss.transform = boss.transform.with_scale(Vec3::splat(2.2));
        boss.emissive = 0.3;
        boss.trigger = true;
        boss.ai_chaser = Some(AiChaser {
            // Lente (GDD §4) : l'archétype Colosse ralentit déjà la poursuite une fois
            // engagée (`Archetype::speed_multiplier`), une vitesse de base modeste la
            // garde lente même avant application du multiplicateur.
            speed: 1.8,
            archetype: Archetype::Colosse,
        });
        boss.combat = Some(Combat {
            attackable: true,
            wave: 1,
            // PV massifs (GDD §4) : très au-dessus du rival du Duel (`hp: 3`).
            hp: 15,
            ..Default::default()
        });
        boss.respawn_delay = 0.0;
        // Contact doublé (GDD §4) : deux fois le dégât de contact du rival du Duel
        // (`Scene::brawl_demo`, 0.9) — pattern d'attaque distinct par son intensité,
        // pas par un nouveau système. Pulse de teinte rouge (télégraphe la menace)
        // en plus de la couleur propre du modèle, pas à sa place (`color` reste
        // blanc = inchangée au repos, cf. `demo_obj`) — un tint fixe agressif
        // écraserait la texture du modèle importé en continu, pas seulement au pic.
        boss.script = "if obj.triggered then damage(1.8 * dt) end\n\
             local p = 0.5 + 0.5 * math.sin(time * 3.0)\n\
             obj.r = 1.0; obj.g = 1.0 - 0.5 * p; obj.b = 1.0 - 0.5 * p"
            .into();

        Scene {
            objects: vec![sol, mur, joueur, fx, boss],
            imported,
            camera_follow: true,
            game_camera: Some(GameCamera {
                target: [0.0, 1.5, 0.0],
                yaw: 0.0,
                pitch: 0.45,
                distance: 11.0,
            }),
            point_lights: vec![PointLight {
                position: [0.0, 6.0, -2.0],
                color: [0.7, 0.3, 0.9],
                intensity: 1.3,
                range: 20.0,
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
