use super::*;

impl Scene {
    /// Démo « Vagues de zombies » : jeu **local contre l'ordinateur**, sans réseau, en
    /// **manches** (style Call of Duty Zombies) — 3 profils de monstres (`AiChaser`,
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

        // --- 3 profils de monstres, de plus en plus présents/variés à chaque manche
        // (comme les vagues d'un mode zombies) : Rôdeur (basique), Coureur (rapide et
        // fragile), Brute (lente mais très punitive et plus difficile à esquiver).
        // Chacun porte aussi un `archetype` (grammaire GDD §5.4, cf. `Archetype`) —
        // à ne pas confondre : `Kind` est un profil d'auteur local à cette démo
        // (stats/couleur/dégâts), `Archetype` est la famille de chasse partagée par
        // tout `AiChaser` du moteur.
        struct Kind {
            label: &'static str,
            speed: f32,
            dmg: f32,
            scale: f32,
            color: [f32; 3],
            archetype: Archetype,
            /// PV de base, avant `Archetype::hp_multiplier` (GDD_MMORPG.md §5.4).
            hp: u32,
        }
        const RODEUR: Kind = Kind {
            label: "Rôdeur",
            speed: 2.6,
            dmg: 0.8,
            scale: 0.7,
            color: [0.35, 0.55, 0.25],
            archetype: Archetype::Traqueuse,
            hp: 2,
        };
        const COUREUR: Kind = Kind {
            label: "Coureur",
            speed: 4.6,
            dmg: 0.5,
            scale: 0.55,
            color: [0.75, 0.8, 0.2],
            archetype: Archetype::Meute,
            hp: 2,
        };
        const BRUTE: Kind = Kind {
            label: "Brute",
            speed: 1.8,
            dmg: 2.2,
            scale: 1.3,
            color: [0.45, 0.08, 0.25],
            archetype: Archetype::Colosse,
            hp: 2,
        };
        // (manche, profils de cette manche) — la difficulté monte : plus de monstres,
        // puis des profils plus dangereux introduits progressivement.
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
                m.ai_chaser = Some(AiChaser {
                    speed: k.speed,
                    archetype: k.archetype,
                });
                m.combat = Some(Combat {
                    attackable: true,
                    wave,
                    // PV différenciés par archétype (GDD_MMORPG.md §5.4), cf.
                    // `Archetype::hp_multiplier`.
                    hp: ((k.hp as f32) * k.archetype.hp_multiplier())
                        .round()
                        .max(1.0) as u32,
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
}
