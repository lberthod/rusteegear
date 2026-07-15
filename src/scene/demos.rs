//! Scènes de démo prêtes à jouer (`Scene::mobile_demo`, `Scene::zombies_demo`,
//! niveaux du contrôleur, etc.) et la scène embarquée du player exporté.
//! Extrait de `scene/mod.rs`.

use glam::Vec3;

use super::{
    AiChaser, AudioSource, Combat, Controller, GameCamera, HudAnchor, HudBinding, HudLayout,
    HudWidget, HudWidgetKind, Light, MeshKind, MobileControls, PointLight, Scene, SceneObject, Sky,
    TapAction, Transform, WEAPONS, WeaponPickup, demo_obj,
};
use crate::runtime::physics::PhysicsKind;

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

    /// Démo « Tour d'ascension » : style de jeu très différent de la démo contrôleur
    /// (arène de combat) — pur platforming vertical, sans ennemi ni combat. Plateformes
    /// en spirale à gravir jusqu'au sommet ; une chute hors des plateformes est une mort
    /// instantanée (vide en contrebas), ce qui remplace la lave comme unique danger.
    pub fn tower_demo() -> Self {
        let mut objects = Vec::new();

        // Sol de départ (petit, juste pour l'atterrissage initial — pas d'arène close ici,
        // le style est vertical, pas horizontal).
        let mut sol = demo_obj("Socle", MeshKind::Cylinder, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(4.0, 0.6, 4.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.35, 0.4, 0.5];
        objects.push(sol);

        // Joueur pilotable : mêmes contrôles que la démo contrôleur (joystick + saut),
        // mais ici la précision de saut est ce qui compte, pas le combat.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.7,
            ..Default::default()
        });
        objects.push(joueur);

        // Vide mortel loin en contrebas : toute chute hors des plateformes est une mort
        // instantanée (remplace la lave comme unique danger de ce style de niveau). Même
        // technique que la lave : l'échelle Y épaissit l'AABB de collision (le mesh Plane
        // a une épaisseur visuelle nulle, cf. note dans `controller_level`) pour détecter
        // fiablement le joueur en chute malgré le pas de simulation fixe.
        let mut vide = demo_obj("Vide", MeshKind::Plane, Vec3::new(0.0, -4.0, 0.0));
        vide.transform = vide.transform.with_scale(Vec3::new(80.0, 60.0, 80.0));
        vide.color = [0.05, 0.05, 0.12];
        vide.deadly = true;
        objects.push(vide);

        // --- Plateformes en spirale ascendante : 4 positions en rotation (avant/droite/
        // arrière/gauche), qui montent d'un cran à chaque tour. Chaque plateforme porte une
        // gemme-objectif (obligatoire pour gagner) légèrement au-dessus, au centre.
        const N: usize = 16;
        for i in 0..N {
            let angle_step = (i % 4) as f32;
            let (dx, dz) = match angle_step as u32 {
                0 => (0.0, -2.6),
                1 => (2.6, 0.0),
                2 => (0.0, 2.6),
                _ => (-2.6, 0.0),
            };
            let y = 1.4 + i as f32 * 1.75;
            let pos = Vec3::new(dx, y, dz);

            let mut plat = demo_obj(&format!("Plateforme {}", i + 1), MeshKind::Cylinder, pos);
            plat.transform = plat.transform.with_scale(Vec3::new(1.6, 0.35, 1.6));
            plat.physics = PhysicsKind::Static;
            // Dégradé froid (bleu nuit → cyan clair) à mesure qu'on grimpe : lisibilité de
            // la progression même sans HUD de score consulté.
            let t = i as f32 / (N - 1) as f32;
            plat.color = [0.25 + 0.15 * t, 0.4 + 0.35 * t, 0.55 + 0.35 * t];
            plat.metallic = 0.3;
            plat.roughness = 0.35;
            objects.push(plat);

            let mut gem = demo_obj(
                &format!("Gemme {}", i + 1),
                MeshKind::Sphere,
                pos + Vec3::Y * 0.85,
            );
            gem.transform = gem.transform.with_scale(Vec3::splat(0.4));
            gem.color = [0.6, 0.9, 1.0];
            gem.emissive = 0.7;
            gem.tappable = true;
            gem.tap_action = TapAction::Hide;
            objects.push(gem);
        }

        // Trophée décoratif au sommet, au-dessus de la dernière plateforme : bonus (score
        // continu, ne bloque pas la victoire — gagner = avoir gravi toute la tour).
        let top = Vec3::new(0.0, 1.4 + (N - 1) as f32 * 1.75, 0.0)
            + match ((N - 1) % 4) as u32 {
                0 => Vec3::new(0.0, 0.0, -2.6),
                1 => Vec3::new(2.6, 0.0, 0.0),
                2 => Vec3::new(0.0, 0.0, 2.6),
                _ => Vec3::new(-2.6, 0.0, 0.0),
            };
        let mut trophy = demo_obj("Étoile Sommet", MeshKind::Sphere, top + Vec3::Y * 1.6);
        trophy.transform = trophy.transform.with_scale(Vec3::splat(0.55));
        trophy.color = [1.0, 0.85, 0.3];
        trophy.emissive = 1.1;
        trophy.tappable = true;
        trophy.tap_action = TapAction::Hide;
        trophy.respawn_delay = 6.0;
        objects.push(trophy);

        // Étoiles décoratives (ciel nocturne) : petits points statiques loin en hauteur,
        // pure ambiance — contraste avec les torches chaudes de la démo contrôleur.
        for i in 0..24 {
            let a = i as f32 * 2.399963; // angle doré : répartition sans motif visible
            let r = 6.0 + (i % 5) as f32 * 3.0;
            let h = 4.0 + (i * 7 % 40) as f32;
            let mut star = demo_obj(
                &format!("Étoile Ciel {}", i + 1),
                MeshKind::Sphere,
                Vec3::new(a.cos() * r, h, a.sin() * r),
            );
            star.transform = star.transform.with_scale(Vec3::splat(0.12));
            star.color = [0.85, 0.9, 1.0];
            star.emissive = 1.0;
            objects.push(star);
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 6.0, 0.0],
                    color: [0.75, 0.85, 1.0],
                    intensity: 1.2,
                    range: 14.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: top.into(),
                    color: [1.0, 0.9, 0.7],
                    intensity: 1.3,
                    range: 10.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Course infinie » (style Temple Run) : 3ᵉ style de jeu, distinct de l'arène
    /// de combat et de la tour de platforming — course automatique en avant, le joueur ne
    /// contrôle que le changement de voie (gauche/centre/droite) et le saut. Obstacles à
    /// esquiver (voie) ou à sauter, pièces à ramasser, ligne d'arrivée obligatoire.
    /// (Piste longue et procédurale plutôt que réellement infinie : le moteur n'a pas de
    /// génération/dé-spawn à la volée — cf. `Scene::temple_run_demo` pour le détail.)
    pub fn temple_run_demo() -> Self {
        const LANES: [f32; 3] = [-2.2, 0.0, 2.2];
        const TRACK_LEN: f32 = 190.0;

        let mut objects = Vec::new();

        // Sol unique sur toute la longueur de la piste.
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, TRACK_LEN * 0.5));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(8.0, 1.0, TRACK_LEN + 10.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.42, 0.38, 0.5];
        objects.push(sol);

        // Murs latéraux : bloquent (sans tuer) toute sortie des 3 voies.
        for sx in [-3.6_f32, 3.6] {
            let mut wall = demo_obj(
                "Mur Voie",
                MeshKind::Cube,
                Vec3::new(sx, 0.9, TRACK_LEN * 0.5),
            );
            wall.transform = wall
                .transform
                .with_scale(Vec3::new(0.4, 1.8, TRACK_LEN + 10.0));
            wall.physics = PhysicsKind::Static;
            wall.color = [0.3, 0.28, 0.4];
            objects.push(wall);
        }

        // Joueur : course automatique en +Z, le joystick/clavier X ne pilote que la voie.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, -3.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 7.0,
            auto_run_speed: 5.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            ..Default::default()
        });
        objects.push(joueur);

        // --- Génération procédurale de la piste : motifs répétés tous les 6 m, densité
        // croissante avec la distance (difficulté progressive, comme un vrai endless runner).
        let seg_len = 6.0_f32;
        let n_segments = (TRACK_LEN / seg_len) as u32;
        for seg in 0..n_segments {
            let z = 8.0 + seg as f32 * seg_len;
            // Les 2 premiers segments sont un couloir d'échauffement (aucun obstacle).
            if seg < 2 {
                continue;
            }
            match seg % 5 {
                0 => {
                    // Haie à sauter (barre pleine largeur, franchissable au saut : sa
                    // hauteur réelle de 1,1 m couvre naturellement l'AABB nécessaire pour
                    // détecter un joueur debout sans la traverser en l'air — contrairement
                    // à un mesh Plane plat, un Cube épais n'a pas besoin de l'astuce
                    // d'échelle Y utilisée pour la lave (cf. `controller_level`).
                    let mut haie = demo_obj("Haie", MeshKind::Cube, Vec3::new(0.0, 0.55, z));
                    haie.transform = haie.transform.with_scale(Vec3::new(7.0, 1.1, 0.6));
                    haie.color = [0.75, 0.35, 0.2];
                    haie.deadly = true;
                    objects.push(haie);
                }
                1 => {
                    // Barrage : 2 des 3 voies bloquées (hauteur non franchissable au saut),
                    // la voie ouverte tourne à chaque occurrence pour ne pas être mémorisable.
                    let open = (seg / 5) % 3;
                    for (lane, &lx) in LANES.iter().enumerate() {
                        if lane as u32 == open {
                            continue;
                        }
                        let mut bar = demo_obj("Barrage", MeshKind::Cube, Vec3::new(lx, 1.0, z));
                        bar.transform = bar.transform.with_scale(Vec3::new(1.8, 2.0, 0.6));
                        bar.color = [0.6, 0.2, 0.2];
                        bar.deadly = true;
                        objects.push(bar);
                    }
                }
                2 => {
                    // Arc de pièces sur les 3 voies : encourage à zigzaguer.
                    for &lx in &LANES {
                        let mut coin = demo_obj("Pièce", MeshKind::Sphere, Vec3::new(lx, 1.0, z));
                        coin.transform = coin.transform.with_scale(Vec3::splat(0.4));
                        coin.color = [1.0, 0.85, 0.2];
                        coin.emissive = 0.6;
                        coin.tappable = true;
                        coin.tap_action = TapAction::Hide;
                        // Bonus (score continu) : ne bloque pas la victoire, seule la
                        // ligne d'arrivée (plus bas) compte comme objectif obligatoire.
                        coin.respawn_delay = 999.0;
                        objects.push(coin);
                    }
                }
                3 => {
                    // Ligne de pièces dans une seule voie (récompense un choix de trajectoire).
                    let lane = (seg / 3) % 3;
                    let mut coin = demo_obj(
                        "Pièce",
                        MeshKind::Sphere,
                        Vec3::new(LANES[lane as usize], 1.0, z),
                    );
                    coin.transform = coin.transform.with_scale(Vec3::splat(0.4));
                    coin.color = [1.0, 0.85, 0.2];
                    coin.emissive = 0.6;
                    coin.tappable = true;
                    coin.tap_action = TapAction::Hide;
                    coin.respawn_delay = 999.0;
                    objects.push(coin);
                }
                _ => {} // couloir de respiration : pas d'obstacle
            }
        }

        // Ligne d'arrivée : seul objectif obligatoire (victoire = l'atteindre), un portique
        // lumineux bien visible + une étoile à ramasser.
        let finish_z = 8.0 + n_segments as f32 * seg_len + 4.0;
        for sx in [-3.2_f32, 3.2] {
            let mut post = demo_obj(
                "Pilier Arrivée",
                MeshKind::Cube,
                Vec3::new(sx, 1.4, finish_z),
            );
            post.transform = post.transform.with_scale(Vec3::new(0.5, 2.8, 0.5));
            post.physics = PhysicsKind::Static;
            post.color = [0.9, 0.75, 0.2];
            post.metallic = 0.5;
            objects.push(post);
        }
        let mut lintel = demo_obj(
            "Linteau Arrivée",
            MeshKind::Cube,
            Vec3::new(0.0, 3.0, finish_z),
        );
        lintel.transform = lintel.transform.with_scale(Vec3::new(6.9, 0.4, 0.5));
        lintel.physics = PhysicsKind::Static;
        lintel.color = [0.9, 0.75, 0.2];
        lintel.metallic = 0.5;
        objects.push(lintel);

        let mut finish = demo_obj(
            "Étoile Arrivée",
            MeshKind::Sphere,
            Vec3::new(0.0, 1.5, finish_z),
        );
        finish.transform = finish.transform.with_scale(Vec3::splat(0.6));
        finish.color = [1.0, 0.9, 0.3];
        finish.emissive = 1.2;
        finish.tappable = true;
        finish.tap_action = TapAction::Hide;
        // respawn_delay = 0 (défaut) ⇒ objectif obligatoire : seule pièce dont la victoire dépend.
        objects.push(finish);

        Scene {
            objects,
            camera_follow: true,
            // Éclairage réparti le long de la piste (190 m) : la lumière directionnelle
            // par défaut (`light`) couvre l'ambiance générale, ces points ponctuels
            // renforcent la lisibilité aux endroits clés (départ, milieu, arrivée).
            point_lights: [10.0, 70.0, 130.0, finish_z]
                .into_iter()
                .map(|z| PointLight {
                    position: [0.0, 8.0, z],
                    color: [1.0, 0.95, 0.85],
                    intensity: 1.2,
                    range: 26.0,
                    ..PointLight::default()
                })
                .collect(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Scène **exemple**, minimale et commentée : montre les 3 composants optionnels
    /// (`Controller`, `AudioSource`, `Combat`) chacun sur un seul objet, sans le décor
    /// dense d'un vrai niveau. Sert de référence rapide pour qui étend le moteur — pas
    /// une démo de gameplay comme les autres (arène/tour/course).
    pub fn components_demo() -> Self {
        // Sol minimal (juste assez pour marcher/sauter).
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(10.0, 1.0, 10.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.4, 0.45, 0.5];

        // --- Controller : rend un objet pilotable (joystick + saut + attaque). `None`
        // pour tous les autres objets de cette scène — un seul joueur en a besoin.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(-2.5, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            attack_button: "Attaque".into(),
            attack_range: 1.5,
            ..Default::default()
        });

        // --- AudioSource : son associé à un objet (clip/autoplay/spatialisation). Le
        // clip est vide ici (aucun fichier son fourni avec l'exemple) — assigne-en un
        // via l'inspecteur (panneau Audio › Choisir un son…) pour l'entendre en Play.
        let mut boite = demo_obj("Boîte à musique", MeshKind::Cube, Vec3::new(0.0, 0.5, 2.0));
        boite.color = [0.6, 0.4, 0.8];
        boite.audio = Some(AudioSource {
            clip: String::new(),
            autoplay: true,
            spatial: true,
            ..Default::default()
        });

        // --- Combat : cible d'attaque (`attackable`) et ancre visuelle de l'effet
        // d'impact (`is_attack_fx`), rarement sur le même objet (ici, deux objets
        // séparés). Approche le joueur et appuie sur Attaque (ou touche J) pour tester.
        let mut cible = demo_obj(
            "Cible d'entraînement",
            MeshKind::Sphere,
            Vec3::new(2.5, 1.0, 0.0),
        );
        cible.color = [0.85, 0.15, 0.15];
        cible.emissive = 0.4;
        cible.combat = Some(Combat {
            attackable: true,
            ..Default::default()
        });
        cible.respawn_delay = 3.0;

        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        Scene {
            objects: vec![sol, joueur, boite, cible, fx],
            camera_follow: true,
            point_lights: vec![PointLight {
                position: [0.0, 5.0, 0.0],
                color: [1.0, 0.95, 0.85],
                intensity: 1.2,
                range: 14.0,
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

    /// Démo « Vagues de zombies » : jeu **local contre l'ordinateur**, sans réseau, en
    /// **manches** (style Call of Duty Zombies) — 3 archétypes de monstres (`AiChaser`,
    /// poursuite active, pas de patrouille scriptée), de plus en plus nombreux et variés
    /// à chaque manche. Vaincre tous les monstres d'une manche révèle la suivante ; la
    /// dernière vaincue ⇒ victoire (`App` pilote la progression, cf. `AppState::wave`).
    pub fn zombies_demo() -> Self {
        let half = 10.0_f32;
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.22, 0.24, 0.28];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Portée courte (0,7 m, pas 1,5) : audit gameplay — un bot qui approche puis
        // attaque au cooldown ne prenait jamais un seul point de dégâts sur les 4 manches,
        // la portée dépassant bien trop largement le rayon de morsure des monstres.
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            attack_button: "Attaque".into(),
            attack_range: 0.7,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour.
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.3, 0.32, 0.38];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );

        // Piliers de couverture : obstacles pour casser une poursuite (les monstres ne
        // les contournent pas intelligemment, ils foncent tout droit vers le joueur).
        for (sx, sz) in [
            (-3.0_f32, 2.0),
            (3.0, -2.0),
            (0.0, 5.5),
            (-4.0, -5.0),
            (4.5, 4.5),
        ] {
            let mut pilier = demo_obj("Pilier", MeshKind::Cylinder, Vec3::new(sx, 0.9, sz));
            pilier.transform = pilier.transform.with_scale(Vec3::new(1.4, 1.8, 1.4));
            pilier.physics = PhysicsKind::Static;
            pilier.color = [0.4, 0.4, 0.45];
            objects.push(pilier);
        }

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;
        objects.push(fx);

        // --- 3 archétypes de monstres, de plus en plus présents/variés à chaque manche
        // (comme les vagues d'un mode zombies) : Rôdeur (basique), Coureur (rapide et
        // fragile), Brute (lente mais très punitive et plus difficile à esquiver).
        struct Kind {
            label: &'static str,
            speed: f32,
            dmg: f32,
            scale: f32,
            color: [f32; 3],
        }
        const RODEUR: Kind = Kind {
            label: "Rôdeur",
            speed: 2.6,
            dmg: 0.8,
            scale: 0.7,
            color: [0.35, 0.55, 0.25],
        };
        const COUREUR: Kind = Kind {
            label: "Coureur",
            speed: 4.6,
            dmg: 0.5,
            scale: 0.55,
            color: [0.75, 0.8, 0.2],
        };
        const BRUTE: Kind = Kind {
            label: "Brute",
            speed: 1.8,
            dmg: 2.2,
            scale: 1.3,
            color: [0.45, 0.08, 0.25],
        };
        // (manche, archétypes de cette manche) — la difficulté monte : plus de monstres,
        // puis des archétypes plus dangereux introduits progressivement.
        let waves: &[(u32, &[&Kind])] = &[
            (1, &[&RODEUR, &RODEUR, &RODEUR]),
            (2, &[&RODEUR, &RODEUR, &RODEUR, &COUREUR, &COUREUR]),
            (
                3,
                &[
                    &RODEUR, &RODEUR, &COUREUR, &COUREUR, &COUREUR, &BRUTE, &BRUTE,
                ],
            ),
            (
                4,
                &[&RODEUR, &RODEUR, &COUREUR, &COUREUR, &BRUTE, &BRUTE, &BRUTE],
            ),
        ];
        let total: usize = waves.iter().map(|(_, ks)| ks.len()).sum();
        let mut spawned = 0usize;
        for &(wave, kinds) in waves {
            for (n, k) in kinds.iter().enumerate() {
                // Répartis en cercle sur tout le pourtour (indice global, pas par manche) :
                // les manches suivantes n'occupent pas les mêmes points que la précédente.
                let angle = spawned as f32 / total as f32 * std::f32::consts::TAU;
                let radius = half - 1.4;
                let pos = Vec3::new(
                    angle.cos() * radius,
                    k.scale.max(0.5) * 0.5,
                    angle.sin() * radius,
                );
                spawned += 1;

                let mut m = demo_obj(&format!("{} {}", k.label, n + 1), MeshKind::Sphere, pos);
                m.transform = m.transform.with_scale(Vec3::splat(k.scale));
                m.color = k.color;
                m.emissive = 0.5;
                m.trigger = true;
                m.ai_chaser = Some(AiChaser { speed: k.speed });
                m.combat = Some(Combat {
                    attackable: true,
                    wave,
                    ..Default::default()
                });
                // Pas de réapparition : un monstre vaincu reste mort pour la manche
                // (contrairement aux ennemis de l'arène de combat, qui reviennent).
                m.respawn_delay = 0.0;
                m.script = format!(
                    "if obj.triggered then damage({} * dt) end\n\
                     local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                     obj.r = {} + {} * p; obj.g = {}; obj.b = {}",
                    k.dmg,
                    k.color[0] * 0.7,
                    k.color[0] * 0.3,
                    k.color[1] * 0.6,
                    k.color[2] * 0.6,
                );
                objects.push(m);
            }
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 9.0, 0.0],
                    color: [0.75, 0.85, 1.0],
                    intensity: 1.3,
                    range: 24.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 3.0, 0.0],
                    color: [1.0, 0.5, 0.3],
                    intensity: 0.7,
                    range: 10.0,
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

    /// Démo « MMORPG » : arène minimale dédiée au test multijoueur PC ↔ mobile —
    /// pas de monstres ni de manches (contrairement à `zombies_demo`), juste un
    /// joueur pilotable (joystick + saut) sur une
    /// carte simple avec quelques repères visuels statiques, pour voir
    /// clairement un joueur desktop et un joueur APK se déplacer l'un par
    /// rapport à l'autre (fantômes réseau, cf. `app::network_client`).
    pub fn mmorpg_demo() -> Self {
        let half = 12.0_f32;
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.2, 0.28, 0.24];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour (enferment l'aire de jeu, ne servent qu'à ne pas tomber).
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.3, 0.32, 0.38];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );

        // Repères visuels statiques (juste pour situer les déplacements, sans danger).
        for (n, (x, z)) in [(-6.0_f32, -6.0), (6.0, -6.0), (-6.0, 6.0), (6.0, 6.0)]
            .into_iter()
            .enumerate()
        {
            let mut repere = demo_obj(
                &format!("Repère {}", n + 1),
                MeshKind::Cylinder,
                Vec3::new(x, 0.9, z),
            );
            repere.transform = repere.transform.with_scale(Vec3::new(1.0, 1.8, 1.0));
            repere.physics = PhysicsKind::Static;
            repere.color = [0.5, 0.45, 0.62];
            objects.push(repere);
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![PointLight {
                position: [0.0, 10.0, 0.0],
                color: [0.9, 0.95, 1.0],
                intensity: 1.2,
                range: 30.0,
                ..PointLight::default()
            }],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Donjon » façon roguelike : 3 salles reliées par des portes (une salle à la
    /// fois, comme un couloir de progression), chacune gardée par un monstre — réutilise
    /// le système de manches (`Combat::wave`) de `zombies_demo` : un monstre par manche,
    /// la salle suivante ne se révèle (et n'obtient de corps physique, cf.
    /// `Physics::build`) qu'une fois la précédente vidée. Particularité roguelike : à
    /// chaque chargement, 3 armes **distinctes** sont tirées au sort parmi les 5 profils
    /// connus (cf. `WEAPONS`) — une équipée au départ, les 2 autres cachées en butin
    /// dans les salles 1 et 2 (cf. `WeaponPickup`) à trouver en explorant avant d'arriver
    /// à la salle 3 (l'Ogre). Score +1 par monstre vaincu *et* par arme trouvée (cf.
    /// `AppState::advance_play`) : un vrai objectif d'exploration, pas juste un combat.
    pub fn roguelike_demo() -> Self {
        // Salles carrées de 9 m de côté, alignées le long de +Z, séparées par une porte
        // (mur avec une ouverture centrale de 3 m) plutôt que par un couloir séparé —
        // plus compact qu'un vrai couloir, mais tout aussi lisible comme 3 pièces
        // distinctes (ligne de vue coupée hors de l'ouverture).
        let half_x = 4.5_f32;
        let room_depth = 9.0_f32;
        let room_z = [-room_depth, 0.0, room_depth]; // centres des 3 salles
        let total_half_z = 1.5 * room_depth;

        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half_x, 1.0, 2.0 * total_half_z));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.2, 0.18, 0.24];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, room_z[0]));
        joueur.color = [0.95, 0.6, 0.25];

        // --- Tirage de 3 armes DISTINCTES parmi les 5 profils connus (`WEAPONS`) : une
        // pour l'équipement de départ, les 2 autres cachées en butin plus bas (cf.
        // `WeaponPickup`). Mélange de Fisher-Yates via `runtime::rng::Rng` (Sprint 131,
        // unifie ce qui était une copie locale du même xorshift64 maison que
        // `runtime::sfx`) : l'horloge système sert de graine.
        let mut rng = crate::runtime::rng::Rng::from_system_time();
        let mut order: [usize; WEAPONS.len()] = std::array::from_fn(|i| i);
        rng.shuffle(&mut order);
        let (starting_idx, found_idx) = (order[0], [order[1], order[2]]);
        let weapon = WEAPONS[starting_idx];
        log::info!(
            "Donjon : arme de départ « {} » (portée {:.1} m, recharge {:.2} s, préparation {:.2} s) — à trouver : {}, {}",
            weapon.label,
            weapon.range,
            weapon.cooldown,
            weapon.windup,
            WEAPONS[found_idx[0]].label,
            WEAPONS[found_idx[1]].label,
        );
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            attack_button: "Attaque".into(),
            attack_range: weapon.range,
            attack_cooldown: weapon.cooldown,
            attack_windup: weapon.windup,
            attack_mode: weapon.mode,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour de tout le donjon (une seule enveloppe extérieure).
        let wall = |name: &str, pos: Vec3, scale: Vec3, objects: &mut Vec<SceneObject>| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.32, 0.28, 0.35];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -total_half_z - 0.25),
            Vec3::new(2.0 * half_x + 0.5, 1.8, 0.5),
            &mut objects,
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, total_half_z + 0.25),
            Vec3::new(2.0 * half_x + 0.5, 1.8, 0.5),
            &mut objects,
        );
        wall(
            "Mur Est",
            Vec3::new(half_x + 0.25, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * total_half_z + 0.5),
            &mut objects,
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half_x - 0.25, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * total_half_z + 0.5),
            &mut objects,
        );

        // Portes entre les salles : mur transversal avec une ouverture centrale de 3 m
        // (deux segments latéraux), à mi-chemin entre chaque paire de salles.
        let door_gap_half = 1.5_f32;
        for (n, z) in [room_z[0], room_z[1]]
            .iter()
            .map(|z| z + room_depth * 0.5)
            .enumerate()
        {
            wall(
                &format!("Porte {} (gauche)", n + 1),
                Vec3::new(-(half_x + door_gap_half) * 0.5, 0.9, z),
                Vec3::new(half_x - door_gap_half, 1.8, 0.4),
                &mut objects,
            );
            wall(
                &format!("Porte {} (droite)", n + 1),
                Vec3::new((half_x + door_gap_half) * 0.5, 0.9, z),
                Vec3::new(half_x - door_gap_half, 1.8, 0.4),
                &mut objects,
            );
        }

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;
        objects.push(fx);

        // --- Un monstre par salle (une manche chacun, cf. `Combat::wave`) : la salle 2
        // ne se révèle qu'une fois le monstre de la salle 1 vaincu, etc. — progression
        // « une salle à la fois » typique d'un roguelike, sans script de porte à part.
        struct Kind {
            label: &'static str,
            speed: f32,
            dmg: f32,
            scale: f32,
            color: [f32; 3],
        }
        const GOBELIN: Kind = Kind {
            label: "Gobelin",
            speed: 3.2,
            dmg: 0.6,
            scale: 0.6,
            color: [0.35, 0.6, 0.3],
        };
        const SQUELETTE: Kind = Kind {
            label: "Squelette",
            speed: 2.4,
            dmg: 1.0,
            scale: 0.85,
            color: [0.75, 0.72, 0.65],
        };
        const OGRE: Kind = Kind {
            label: "Ogre",
            speed: 1.6,
            dmg: 2.4,
            scale: 1.4,
            color: [0.4, 0.15, 0.15],
        };
        // Décalage du monstre par rapport au centre de sa salle : loin du point d'entrée
        // du joueur — son spawn pour la salle 1 (sinon le Gobelin apparaissait pile sur
        // le joueur et mordait avant même qu'il ait pu bouger), la porte d'entrée pour
        // les salles 2 et 3 (sinon le monstre suivant mord dès le franchissement de la
        // porte, sans le moindre temps de réaction).
        const MONSTER_Z_OFFSET: [f32; 3] = [-3.0, 3.0, 3.0];
        for (wave, ((k, z), z_offset)) in [GOBELIN, SQUELETTE, OGRE]
            .into_iter()
            .zip(room_z)
            .zip(MONSTER_Z_OFFSET)
            .enumerate()
        {
            let wave = wave as u32 + 1;
            let mut m = demo_obj(
                k.label,
                MeshKind::Sphere,
                Vec3::new(0.0, k.scale.max(0.5) * 0.5, z + z_offset),
            );
            m.transform = m.transform.with_scale(Vec3::splat(k.scale));
            m.color = k.color;
            m.emissive = 0.5;
            m.trigger = true;
            m.ai_chaser = Some(AiChaser { speed: k.speed });
            m.combat = Some(Combat {
                attackable: true,
                wave,
                ..Default::default()
            });
            m.respawn_delay = 0.0;
            m.script = format!(
                "if obj.triggered then damage({} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = {} + {} * p; obj.g = {}; obj.b = {}",
                k.dmg,
                k.color[0] * 0.7,
                k.color[0] * 0.3,
                k.color[1] * 0.6,
                k.color[2] * 0.6,
            );
            objects.push(m);
        }

        // Butins d'arme (cf. `WeaponPickup`) : un dans la salle 1, un dans la salle 2 —
        // la salle 3 (l'Ogre, le combat le plus dur) doit pouvoir être abordée avec la
        // meilleure arme déjà trouvée en explorant, pas en découvrir une nouvelle en
        // pleine bagarre. Coin de salle opposé au monstre (au centre), pour ne pas
        // forcer le joueur à passer devant le monstre juste pour le voir.
        for (n, &weapon_idx) in found_idx.iter().enumerate() {
            let w = WEAPONS[weapon_idx];
            let side = if n == 0 { 1.0 } else { -1.0 };
            let mut loot = demo_obj(
                &format!("Butin: {}", w.label),
                MeshKind::Cube,
                Vec3::new(side * 3.0, 0.4, room_z[n] + side * 3.0),
            );
            loot.transform = loot.transform.with_scale(Vec3::splat(0.4));
            loot.color = [1.0, 0.85, 0.2];
            loot.emissive = 1.2;
            loot.weapon_pickup = Some(WeaponPickup { weapon: weapon_idx });
            objects.push(loot);
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 6.0, room_z[0]],
                    color: [0.4, 0.8, 0.5],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 6.0, room_z[1]],
                    color: [0.9, 0.85, 0.7],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 6.0, room_z[2]],
                    color: [0.9, 0.3, 0.3],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

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
        rival.ai_chaser = Some(AiChaser { speed: 2.8 });
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

    /// Démo « gameplay complet » : joueur (joystick + gyroscope + saut + vibration),
    /// zone de danger qui retire de la vie (HUD), et cube tactile qui change de couleur.
    /// Montre toute l'API de script en une scène jouable.
    pub fn gameplay_demo() -> Self {
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(16.0, 1.0, 16.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.35, 0.5, 0.4];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 0.5, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.script = "\
local s = 4.0
obj.x = obj.x + (input.jx + tilt.x) * s * dt
obj.z = obj.z - (input.jy + tilt.y) * s * dt
if input.btn.Saut then obj.y = 1.4; vibrate(40) else obj.y = 0.5 end"
            .into();

        let mut danger = demo_obj("Zone danger", MeshKind::Cube, Vec3::new(3.0, 0.5, 0.0));
        danger.color = [0.8, 0.2, 0.2];
        danger.emissive = 0.3;
        danger.trigger = true;
        danger.script = "\
if obj.triggered then set_health(0.25); vibrate(120) else set_health(1.0) end"
            .into();

        let mut bouton = demo_obj("Cube couleur", MeshKind::Cube, Vec3::new(-3.0, 0.5, 0.0));
        bouton.color = [0.3, 0.6, 0.9];
        bouton.tappable = true;
        bouton.script = "\
if obj.tapped then
  obj.r = (time * 0.7) % 1.0; obj.g = (time * 1.3) % 1.0; obj.b = (time * 1.9) % 1.0
end"
        .into();

        Scene {
            objects: vec![sol, joueur, danger, bouton],
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
        }
    }
    /// Scène **embarquée dans le binaire** (figée à la compilation depuis
    /// `assets/player_scene.json`, réécrite à chaque export). C'est le jeu que joue
    /// le mode Player d'un `.dmg`/`.apk`/`.ipa` exporté.
    pub fn embedded_player() -> Self {
        const JSON: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/player_scene.json"
        ));
        match serde_json::from_str::<Scene>(JSON) {
            Ok(mut s) => {
                s.reload_imported();
                s
            }
            Err(e) => {
                log::error!("Scène embarquée invalide ({e}) — retour à la démo.");
                Scene::demo()
            }
        }
    }

    /// Scène de démonstration : un sol, un cube, une sphère.
    pub fn demo() -> Self {
        Scene {
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls::default(),
            camera_follow: false,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                        .with_scale(Vec3::new(10.0, 1.0, 10.0)),
                    mesh: MeshKind::Plane,
                    script: String::new(),
                    physics: PhysicsKind::Static,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Cube".into(),
                    transform: Transform::from_pos(Vec3::new(-1.2, -0.5, 0.0)),
                    mesh: MeshKind::Cube,
                    // exemple : tourne autour de Y à 60°/s en mode Play
                    script: "obj.ry = obj.ry + dt * 60.0".into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Sphère".into(),
                    transform: Transform::from_pos(Vec3::new(1.2, 2.5, 0.0)),
                    mesh: MeshKind::Sphere,
                    script: String::new(),
                    // tombe et rebondit sur le sol en mode Play
                    physics: PhysicsKind::Dynamic,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
            ],
        }
    }

    /// Démo mobile « prête à jouer » : un sol, un personnage piloté au joystick
    /// (avec saut au bouton) et contrôles tactiles activés. Démontre toute la
    /// boucle joystick → script → rendu en mode Play.
    pub fn mobile_demo() -> Self {
        let player_script = "\
local speed = 4.0
obj.x = obj.x + input.jx * speed * dt
obj.z = obj.z - input.jy * speed * dt
if input.btn.Saut then obj.y = 1.4 else obj.y = 0.5 end";
        Scene {
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.0, 0.0))
                        .with_scale(Vec3::new(14.0, 1.0, 14.0)),
                    mesh: MeshKind::Plane,
                    script: String::new(),
                    physics: PhysicsKind::Static,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.4, 0.5, 0.45],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Joueur".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.5, 0.0)),
                    mesh: MeshKind::Capsule,
                    script: player_script.into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.95, 0.6, 0.25],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Bouton couleur".into(),
                    transform: Transform::from_pos(Vec3::new(2.5, 0.5, -1.0)),
                    mesh: MeshKind::Cube,
                    // Tap → couleur aléatoire (changeante) via le temps.
                    script: "if obj.tapped then\n  obj.r = (time * 0.7) % 1.0\n  obj.g = (time * 1.3) % 1.0\n  obj.b = (time * 1.9) % 1.0\nend".into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.3, 0.6, 0.9],
                    texture: String::new(),
                    tappable: true,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
            ],
        }
    }
}
