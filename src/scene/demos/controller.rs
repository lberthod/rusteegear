use super::*;

impl Scene {
    /// Démo « contrôleur » **sans script** (niveau 1) : joueur pilotable au joystick,
    /// saut, collisions, pièces à ramasser, lave à éviter.
    pub fn controller_demo() -> Self {
        Self::controller_level(1)
    }

    /// Niveau `level` (1-based) de la démo contrôleur. Les niveaux supérieurs sont plus
    /// grands/chargés (plus de pièces, lave plus large, bonus plus fréquents).
    pub fn controller_level(level: u32) -> Self {
        let lvl = level.max(1);
        let hard = (lvl - 1) as f32; // 0 au niveau 1, 1 au niveau 2, …

        // Sol statique (teinte qui varie par niveau pour les distinguer).
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(16.0, 1.0, 16.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.30 + 0.12 * hard, 0.5 - 0.08 * hard, 0.42];

        // Joueur pilotable : Input Receiver + saut sur le bouton « Saut ».
        // Démarre au bord (pas sur la lave centrale).
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, -6.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Attaque au corps-à-corps : vainc les ennemis `attackable` à portée (cf.
        // `Scene::attack_at`), sur pression du bouton tactile « Attaque » ou de la
        // touche J (desktop, cf. `PlayerInput::attack`). Portée courte (0,7 m) : au-delà
        // de `attack_range`, ce qui compte c'est l'écart avec la portée de morsure de la
        // cible (son propre rayon) — un écart de 1,5 m rendait le combat sans risque
        // (audit gameplay : un bot qui approche puis attaque ne prenait jamais de dégâts).
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.6,
            attack_button: "Attaque".into(),
            attack_range: 0.7,
            ..Default::default()
        });

        // Effet visuel du coup : sphère blanche invisible par défaut, téléportée sur la
        // cible et affichée brièvement par `App` quand une attaque porte (cf.
        // `AppState::attack_flash`) — rend le coup lisible, pas juste sonore.
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;
        let mut objects = vec![sol, joueur, fx];

        // --- Murs de pourtour : enferment l'aire de jeu (le joueur ne tombe plus) ---
        // Le sol (plan unité × 16) couvre [-8, 8] ; on pose 4 murs statiques aux bords.
        let half = 7.5_f32;
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.45, 0.5, 0.62];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.6, -half),
            Vec3::new(16.0, 1.2, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.6, half),
            Vec3::new(16.0, 1.2, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.6, 0.0),
            Vec3::new(0.5, 1.2, 16.0),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.6, 0.0),
            Vec3::new(0.5, 1.2, 16.0),
        );

        // Mare de lave **au centre** (plus large aux niveaux supérieurs) : à contourner.
        // Note : le mesh Plane a une épaisseur visuelle nulle (y=0 pour tous les sommets),
        // donc l'échelle Y ne change rien au rendu — on s'en sert pour épaissir l'AABB de
        // collision verticalement (≈0.6 m autour du sol) afin que la zone mortelle détecte
        // fiablement un joueur qui marche dessus (capsule au repos ~y=0.5), tout en restant
        // franchissable en sautant par-dessus (le pic du saut dépasse cette plage).
        let lave_s = 3.0 + hard;
        let mut lave = demo_obj("Lave", MeshKind::Plane, Vec3::new(0.0, 0.02, 0.0));
        lave.transform = lave.transform.with_scale(Vec3::new(lave_s, 30.0, lave_s));
        lave.color = [0.95, 0.3, 0.1];
        lave.emissive = 0.7;
        lave.deadly = true;
        // Bouillonnement : la teinte pulse (deux fréquences superposées) sans toucher à
        // l'échelle Y (réservée à l'épaisseur de collision, cf. note ci-dessus).
        lave.script = "\
local b = 0.5 + 0.5 * math.sin(time * 2.2) + 0.25 * math.sin(time * 5.3)\n\
obj.r = 0.85 + 0.15 * b; obj.g = 0.22 + 0.18 * b; obj.b = 0.05 + 0.1 * b"
            .into();
        objects.push(lave);

        // Bulles de lave décoratives : jaillissent puis retombent en boucle, déphasées,
        // pour animer la surface (aucune collision/danger propre : la mare mère suffit).
        let bub_r = (lave_s * 0.5 - 0.4).max(0.3);
        for (n, (bx, bz, ph)) in [
            (0.5_f32, -0.3_f32, 0.0_f32),
            (-0.4, 0.4, 1.1),
            (0.1, 0.6, 2.3),
            (-0.5, -0.5, 3.6),
            (0.6, 0.1, 4.8),
        ]
        .into_iter()
        .enumerate()
        {
            let pos = Vec3::new(bx * bub_r, 0.05, bz * bub_r);
            let mut bubble = demo_obj(&format!("Bulle Lave {}", n + 1), MeshKind::Sphere, pos);
            bubble.color = [1.0, 0.5, 0.15];
            bubble.emissive = 1.0;
            bubble.script = format!(
                "local cyc = (time * 0.6 + {ph}) % 2.0\n\
                 local h = math.max(0.0, math.sin(cyc * math.pi))\n\
                 obj.y = 0.02 + h * 0.4\n\
                 obj.sx = 0.12 + h * 0.28; obj.sy = 0.12 + h * 0.28; obj.sz = 0.12 + h * 0.28"
            );
            objects.push(bubble);
        }

        // --- Pont surélevé traversant la lave (axe Z) : raccourci risqué mais direct.
        // Reste hors de portée verticale de la lave (marge ≈0.23 m) — sûr tant qu'on ne
        // tombe pas sur les côtés, ce qui ramène au niveau du sol au-dessus de la lave
        // (mort instantanée). Récompensé par une gemme suprême flottant en son centre.
        let bridge_half = lave_s * 0.5 + 0.8;
        let mut bridge = demo_obj("Pont", MeshKind::Cube, Vec3::new(0.0, 1.0, 0.0));
        bridge.transform = bridge
            .transform
            .with_scale(Vec3::new(0.9, 0.3, bridge_half * 2.0));
        bridge.physics = PhysicsKind::Static;
        bridge.color = [0.4, 0.36, 0.42];
        bridge.metallic = 0.25;
        bridge.roughness = 0.5;
        objects.push(bridge);

        let mut supreme = demo_obj("Gemme Suprême", MeshKind::Sphere, Vec3::new(0.0, 1.75, 0.0));
        supreme.transform = supreme.transform.with_scale(Vec3::splat(0.5));
        supreme.color = [0.85, 0.3, 0.95];
        supreme.emissive = 1.1;
        supreme.metallic = 0.5;
        supreme.tappable = true;
        supreme.tap_action = TapAction::Hide;
        supreme.respawn_delay = 7.0 - hard;
        objects.push(supreme);

        // Piliers-obstacles aux diagonales, surmontés d'une **étoile bonus** (en hauteur,
        // atteignable au saut ; réapparaît → score continu).
        for (n, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let base = Vec3::new(sx * 4.3, 0.0, sz * 4.3);
            let mut pil = demo_obj(
                &format!("Pilier {}", n + 1),
                MeshKind::Cube,
                base + Vec3::Y * 0.7,
            );
            pil.transform = pil.transform.with_scale(Vec3::new(0.8, 1.4, 0.8));
            pil.physics = PhysicsKind::Static;
            pil.color = [0.5, 0.52, 0.6];
            objects.push(pil);

            let mut star = demo_obj(
                &format!("Étoile {}", n + 1),
                MeshKind::Sphere,
                base + Vec3::Y * 1.9,
            );
            star.transform = star.transform.with_scale(Vec3::splat(0.4));
            star.color = [0.55, 0.85, 1.0];
            star.emissive = 0.8;
            star.tappable = true;
            star.tap_action = TapAction::Hide;
            star.respawn_delay = 4.0 - hard; // réapparition plus rapide au niveau 2
            objects.push(star);
        }

        // --- Pièces-objectif : anneaux générés automatiquement autour de la lave ---
        let rings: &[(u32, f32)] = if hard > 0.5 {
            &[(6, 3.8), (8, 6.4)]
        } else {
            &[(6, 3.4), (6, 6.2)]
        };
        let mut p = 0;
        for &(ring, radius) in rings {
            for k in 0..ring {
                // anneau extérieur décalé d'un demi-pas (disposition en quinconce).
                let off = if radius > 5.0 { 0.5 } else { 0.0 };
                let angle = (k as f32 + off) / ring as f32 * std::f32::consts::TAU;
                let pos = Vec3::new(angle.cos() * radius, 0.5, angle.sin() * radius);
                p += 1;
                let mut gem = demo_obj(&format!("Pièce {p}"), MeshKind::Sphere, pos);
                gem.transform = gem.transform.with_scale(Vec3::splat(0.45));
                gem.color = [1.0, 0.85, 0.2];
                gem.emissive = 0.5;
                gem.metallic = 0.6;
                gem.roughness = 0.25;
                gem.tappable = true;
                gem.tap_action = TapAction::Hide;
                objects.push(gem);
            }
        }

        // --- Escalier + plateforme surélevée côté ouest : défi de plateforme optionnel,
        // récompensé par des pièces bonus et un trophée (ne bloque pas la victoire).
        for i in 0..3u32 {
            let sy = 0.3 + i as f32 * 0.3;
            let sx = -7.0 + i as f32 * 0.65;
            let mut step = demo_obj(
                &format!("Marche {}", i + 1),
                MeshKind::Cube,
                Vec3::new(sx, sy * 0.5, 0.0),
            );
            step.transform = step.transform.with_scale(Vec3::new(0.75, sy, 2.2));
            step.physics = PhysicsKind::Static;
            step.color = [0.55, 0.5, 0.4];
            objects.push(step);
        }
        let mut podium = demo_obj("Plateforme", MeshKind::Cube, Vec3::new(-5.0, 0.95, 0.0));
        podium.transform = podium.transform.with_scale(Vec3::new(1.7, 0.3, 2.6));
        podium.physics = PhysicsKind::Static;
        podium.color = [0.52, 0.48, 0.58];
        podium.metallic = 0.35;
        podium.roughness = 0.35;
        objects.push(podium);

        // Deux pièces bonus flanquant le trophée, en hauteur sur la plateforme.
        for (n, dz) in [(1, -0.8), (2, 0.8)] {
            let mut bonus = demo_obj(
                &format!("Pièce Bonus {n}"),
                MeshKind::Sphere,
                Vec3::new(-5.0, 1.5, dz),
            );
            bonus.transform = bonus.transform.with_scale(Vec3::splat(0.4));
            bonus.color = [0.4, 0.9, 0.6];
            bonus.emissive = 0.7;
            bonus.tappable = true;
            bonus.tap_action = TapAction::Hide;
            bonus.respawn_delay = 6.0 - hard;
            objects.push(bonus);
        }
        // Trophée : bonus le plus précieux (score continu), au sommet de la plateforme.
        let mut trophy = demo_obj(
            "Étoile Trophée",
            MeshKind::Sphere,
            Vec3::new(-5.0, 2.1, 0.0),
        );
        trophy.transform = trophy.transform.with_scale(Vec3::splat(0.55));
        trophy.color = [1.0, 0.75, 0.25];
        trophy.emissive = 1.0;
        trophy.metallic = 0.4;
        trophy.tappable = true;
        trophy.tap_action = TapAction::Hide;
        trophy.respawn_delay = 5.0 - hard;
        objects.push(trophy);

        // --- Portique décoratif encadrant l'entrée côté sud (lisibilité + ambiance) ---
        for sx in [-1.6_f32, 1.6] {
            let mut post = demo_obj("Pilier Portique", MeshKind::Cube, Vec3::new(sx, 1.1, -5.6));
            post.transform = post.transform.with_scale(Vec3::new(0.5, 2.2, 0.5));
            post.physics = PhysicsKind::Static;
            post.color = [0.45, 0.4, 0.5];
            post.metallic = 0.5;
            post.roughness = 0.3;
            objects.push(post);
        }
        let mut lintel = demo_obj(
            "Linteau Portique",
            MeshKind::Cube,
            Vec3::new(0.0, 2.35, -5.6),
        );
        lintel.transform = lintel.transform.with_scale(Vec3::new(3.6, 0.4, 0.5));
        lintel.physics = PhysicsKind::Static;
        lintel.color = [0.45, 0.4, 0.5];
        lintel.metallic = 0.5;
        lintel.roughness = 0.3;
        objects.push(lintel);

        // --- Ennemis patrouilleurs : hazards mobiles (scriptés), infligent des **dégâts
        // progressifs** au contact (via `damage()`) plutôt qu'une mort instantanée comme
        // la lave — plus indulgent, encourage à esquiver/se replier plutôt qu'à figer la
        // partie au premier effleurement. Plus rapides et plus punitifs au niveau 2 (`hard`).
        // Pulsent en rouge (menace visuelle). Vaincus par l'attaque du joueur (à portée) :
        // disparaissent puis réapparaissent après un répit, plutôt que d'être éliminés
        // définitivement (le niveau reste tendu même après un bon coup).
        let enemy_speed = 1.0 + 0.4 * hard;
        let dmg_rate = 0.9 + 0.3 * hard;
        let mut enemy = |name: &str, pos: Vec3, script: String| {
            let mut e = demo_obj(name, MeshKind::Sphere, pos);
            e.transform = e.transform.with_scale(Vec3::new(0.7, 0.6, 0.7));
            e.color = [0.85, 0.08, 0.08];
            e.emissive = 0.5;
            e.trigger = true;
            e.combat = Some(Combat {
                attackable: true,
                ..Default::default()
            });
            e.respawn_delay = 8.0 - hard;
            e.script = script;
            objects.push(e);
        };
        // Sentinelle sud : va-et-vient devant l'entrée, le long du mur sud.
        enemy(
            "Ennemi Sentinelle",
            Vec3::new(0.0, 0.5, -7.0),
            format!(
                "local s = {enemy_speed}\n\
                 obj.x = math.sin(time * s) * 3.0\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );
        // Rôdeur est : va-et-vient le long du couloir est, entre le mur et les piliers.
        enemy(
            "Ennemi Rôdeur",
            Vec3::new(5.6, 0.5, 0.0),
            format!(
                "local s = {enemy_speed}\n\
                 obj.z = math.sin(time * s * 0.8) * 3.0\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );
        // Gardien du trésor : tourne en orbite près de la gemme suprême / du pont.
        enemy(
            "Ennemi Gardien",
            Vec3::new(2.2, 0.5, -2.2),
            format!(
                "local s = {enemy_speed}\n\
                 obj.x = 2.2 + math.cos(time * s * 0.9) * 1.1\n\
                 obj.z = -2.2 + math.sin(time * s * 0.9) * 1.1\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );

        // --- Torches aux 4 coins de l'arène (flamme émissive + halo de lumière chaude) ---
        let mut lights = vec![PointLight {
            // Lumière ponctuelle chaude au-dessus de l'arène (ambiance + lisibilité).
            position: [0.0, 6.0, 0.0],
            color: [1.0, 0.92, 0.78],
            intensity: 1.4,
            range: 16.0,
            ..PointLight::default()
        }];
        for (n, (cx, cz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let base = Vec3::new(cx * 6.9, 0.0, cz * 6.9);
            let mut torch = demo_obj(
                &format!("Torche {}", n + 1),
                MeshKind::Cube,
                base + Vec3::Y * 0.8,
            );
            torch.transform = torch.transform.with_scale(Vec3::new(0.3, 1.6, 0.3));
            torch.physics = PhysicsKind::Static;
            torch.color = [0.3, 0.28, 0.3];
            objects.push(torch);

            let mut flame = demo_obj(
                &format!("Flamme {}", n + 1),
                MeshKind::Sphere,
                base + Vec3::Y * 1.7,
            );
            flame.transform = flame.transform.with_scale(Vec3::splat(0.3));
            flame.color = [1.0, 0.55, 0.15];
            flame.emissive = 1.2;
            // Vacillement (déphasé par torche) : taille + teinte fluctuent, deux fréquences
            // superposées pour un scintillement moins mécanique qu'une simple sinusoïde.
            let phase = n as f32 * 1.7;
            flame.script = format!(
                "local f = 0.75 + 0.15 * math.sin(time * 9.0 + {phase}) \
                 + 0.10 * math.sin(time * 23.0 + {phase} * 2.0)\n\
                 obj.sx = 0.3 * f; obj.sy = 0.3 * f; obj.sz = 0.3 * f\n\
                 obj.r = 1.0; obj.g = 0.45 + 0.2 * f; obj.b = 0.1 + 0.15 * f"
            );
            objects.push(flame);

            lights.push(PointLight {
                position: (base + Vec3::Y * 1.7).into(),
                color: [1.0, 0.6, 0.25],
                intensity: 0.9,
                range: 6.0,
                ..PointLight::default()
            });
        }
        // Lueur rouge au ras de la lave : renforce le danger visuel de la zone mortelle.
        lights.push(PointLight {
            position: [0.0, 0.6, 0.0],
            color: [1.0, 0.35, 0.1],
            intensity: 1.1,
            range: 7.0,
            ..PointLight::default()
        });
        // Lueur violette autour de la gemme suprême, sur le pont : signale la récompense
        // la plus prestigieuse du niveau, visible de loin par contraste avec la lave.
        lights.push(PointLight {
            position: [0.0, 2.0, 0.0],
            color: [0.85, 0.4, 1.0],
            intensity: 0.8,
            range: 5.0,
            ..PointLight::default()
        });

        Scene {
            objects,
            camera_follow: true,
            point_lights: lights,
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            // Widgets HUD déclaratifs (Sprint 109) : score en bas-gauche et jauge de
            // vie en bas-droite, en plus des overlays historiques (barre de vie
            // haut-gauche, manche haut-centre…) — démontrent le système texte/jauge
            // dans un niveau réellement joué, sans remplacer les overlays déjà
            // éprouvés (vie, viseur…) ni leurs tests.
            hud_widgets: vec![
                HudWidget {
                    id: "score_label".into(),
                    anchor: HudAnchor::BottomLeft,
                    offset: [16.0, -16.0],
                    size: [0.0, 0.0],
                    kind: HudWidgetKind::Text {
                        content: "Score".into(),
                        binding: HudBinding::Score,
                    },
                },
                HudWidget {
                    id: "health_gauge".into(),
                    anchor: HudAnchor::BottomRight,
                    offset: [-16.0, -16.0],
                    size: [140.0, 14.0],
                    kind: HudWidgetKind::Gauge {
                        binding: HudBinding::Health,
                        max: 1.0,
                        color: [0.8, 0.15, 0.15],
                    },
                },
            ],
            ..Default::default()
        }
    }
}
