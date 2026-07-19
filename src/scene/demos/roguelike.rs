use super::*;

impl Scene {
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
            archetype: Archetype,
            /// PV de base, avant `Archetype::hp_multiplier` (GDD_MMORPG.md §5.4).
            hp: u32,
        }
        const GOBELIN: Kind = Kind {
            label: "Gobelin",
            speed: 3.2,
            dmg: 0.6,
            scale: 0.6,
            color: [0.35, 0.6, 0.3],
            archetype: Archetype::Meute,
            hp: 2,
        };
        const SQUELETTE: Kind = Kind {
            label: "Squelette",
            speed: 2.4,
            dmg: 1.0,
            scale: 0.85,
            color: [0.75, 0.72, 0.65],
            archetype: Archetype::Furtive,
            hp: 2,
        };
        const OGRE: Kind = Kind {
            label: "Ogre",
            speed: 1.6,
            dmg: 2.4,
            scale: 1.4,
            color: [0.4, 0.15, 0.15],
            archetype: Archetype::Colosse,
            hp: 2,
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

        // Objets d'inventaire (cf. `ItemPickup`) : de quoi remplir le sac en
        // explorant — une potion par salle 1/2 (soin de secours avant l'Ogre,
        // coin opposé au butin d'arme pour inciter à fouiller toute la salle),
        // un buisson à baies qui se régénère dans le couloir, et le trésor de
        // l'Ogre (clé + gemme) au fond de la salle 3.
        for (n, &z) in room_z.iter().take(2).enumerate() {
            let side = if n == 0 { -1.0 } else { 1.0 };
            let mut potion = demo_obj(
                "Potion de soin",
                MeshKind::Capsule,
                Vec3::new(side * 3.0, 0.35, z - side * 3.0),
            );
            potion.transform = potion.transform.with_scale(Vec3::splat(0.35));
            potion.color = ItemKind::Potion.color();
            potion.emissive = 0.8;
            potion.item_pickup = Some(ItemPickup {
                kind: ItemKind::Potion,
                count: 1,
            });
            objects.push(potion);
        }
        let mut baies = demo_obj(
            "Buisson à baies",
            MeshKind::Sphere,
            Vec3::new(1.5, 0.3, (room_z[0] + room_z[1]) * 0.5),
        );
        baies.transform = baies.transform.with_scale(Vec3::splat(0.5));
        baies.color = ItemKind::Baie.color();
        baies.emissive = 0.4;
        baies.respawn_delay = 20.0;
        baies.item_pickup = Some(ItemPickup {
            kind: ItemKind::Baie,
            count: 2,
        });
        objects.push(baies);
        for (name, kind, dx) in [
            ("Clé du donjon", ItemKind::Cle, -1.0),
            ("Gemme de l'Ogre", ItemKind::Gemme, 1.0),
        ] {
            let mut tresor = demo_obj(name, MeshKind::Cube, Vec3::new(dx, 0.3, room_z[2] - 3.5));
            tresor.transform = tresor.transform.with_scale(Vec3::splat(0.3));
            tresor.color = kind.color();
            tresor.emissive = 1.0;
            tresor.item_pickup = Some(ItemPickup { kind, count: 1 });
            objects.push(tresor);
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
}
