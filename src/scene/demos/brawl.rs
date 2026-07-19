use super::*;

impl Scene {
    /// Démo « Duel » façon Tekken/Smash Bros : arène compacte flottant au-dessus du
    /// vide, joueur contre un unique rival qui encaisse plusieurs coups (cf.
    /// `Combat::hp`) avant de tomber — un vrai combat, pas une mise à mort au premier
    /// coup. Deux façons de gagner, comme dans un vrai jeu de combat : l'achever à coups
    /// de poing (hp à 0, cf. `Scene::damage_attackable`), ou le faire sortir de l'arène
    /// d'un coup de recul (« ring out », cf. `AppState::stagger` — le vide sous la scène
    /// est une zone mortelle, cf. `deadly`, réutilisée pour l'IA comme pour le joueur).
    /// Réutilise le système de manches (`Combat::wave = 1`, un seul adversaire) plutôt
    /// qu'un mécanisme de victoire dédié : dès que le rival est invisible (achevé ou
    /// sorti de l'arène), `AppState::update_waves` déclenche la victoire tout seul.
    pub fn brawl_demo() -> Self {
        let half = 7.0_f32;

        let mut sol = demo_obj("Arène", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.18, 0.16, 0.22];
        sol.metallic = 0.5;
        sol.roughness = 0.3;

        // Le vide sous l'arène : aucun mur, aucun sol au-delà du bord — tomber suffit à
        // perdre (joueur) ou à être vaincu (rival, cf. la vérification de ring out dans
        // `AppState::advance_play`). Invisible : la chute elle-même (rien sous les
        // pieds) suffit à faire comprendre le danger, pas besoin d'un aplat coloré.
        let mut vide = demo_obj("Vide", MeshKind::Cube, Vec3::new(0.0, -8.0, 0.0));
        vide.transform = vide.transform.with_scale(Vec3::new(60.0, 10.0, 60.0));
        vide.deadly = true;
        vide.visible = false;

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(-4.0, 1.0, 0.0));
        joueur.color = [0.9, 0.75, 0.3];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.2,
            attack_button: "Attaque".into(),
            // Portée courte et préparation vive : des coups qui se rapprochent d'un jab
            // de jeu de combat, pas d'un missile à distance.
            attack_range: 1.3,
            attack_cooldown: 0.45,
            attack_windup: 0.15,
            ..Default::default()
        });

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.9, 0.6];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        let mut rival = demo_obj("Rival", MeshKind::Capsule, Vec3::new(4.0, 1.0, 0.0));
        rival.transform = rival.transform.with_scale(Vec3::splat(1.05));
        rival.color = [0.55, 0.08, 0.12];
        rival.emissive = 0.35;
        rival.trigger = true;
        rival.ai_chaser = Some(AiChaser {
            speed: 2.8,
            ..Default::default()
        });
        rival.combat = Some(Combat {
            attackable: true,
            // Une seule « manche » (cf. `Combat::wave`) : un adversaire unique, pas des
            // vagues — juste pour déclencher la victoire via `AppState::update_waves`
            // une fois qu'il est invisible (achevé ou sorti de l'arène), sans avoir à
            // écrire une condition de victoire dédiée à cette démo.
            wave: 1,
            // 3 coups pour l'achever : un vrai duel, pas une mise à mort au premier
            // coup (`Combat::hp` par défaut ailleurs). Reste vainquable par ring out
            // avant d'y arriver (cf. la vérification dans `AppState::advance_play`).
            hp: 3,
            ..Default::default()
        });
        rival.respawn_delay = 0.0;
        rival.script = "if obj.triggered then damage(0.9 * dt) end\n\
             local p = 0.5 + 0.5 * math.sin(time * 6.0)\n\
             obj.r = 0.55 + 0.35 * p; obj.g = 0.08; obj.b = 0.12"
            .into();

        Scene {
            objects: vec![sol, vide, joueur, fx, rival],
            camera_follow: true,
            // Angle plus bas et plus horizontal que les autres démos (pitch ~0,35 contre
            // ~0,62) : cadrage de profil façon jeu de combat plutôt qu'une vue plongeante
            // de action-aventure — le point de vue précis se règle facilement dans
            // l'éditeur (`Vue → Définir la caméra de jeu`) si besoin d'un angle différent.
            game_camera: Some(GameCamera {
                target: [0.0, 1.0, 0.0],
                yaw: 0.0,
                pitch: 0.35,
                distance: 9.0,
            }),
            point_lights: vec![
                // Lumière chaude du côté du joueur, froide du côté du rival — cadrage
                // « vs » à deux couleurs typique des jeux de combat.
                PointLight {
                    position: [-4.0, 4.0, 2.0],
                    color: [1.0, 0.65, 0.3],
                    intensity: 1.1,
                    range: 14.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [4.0, 4.0, -2.0],
                    color: [0.3, 0.55, 1.0],
                    intensity: 1.1,
                    range: 14.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
