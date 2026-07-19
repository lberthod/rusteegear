use super::*;

pub(super) fn add_wilds(objects: &mut Vec<SceneObject>, imported: &mut Vec<ImportedMesh>) {
        // --- Poste de guet, sur l'axe d'approche Nord (léger décalage pour
        // ne pas boucher la lisière de vague), avec 2 lanternes.
        poser(
            objects,
            imported,
            "Poste de guet",
            "nature_tower.glb",
            7.0,
            -49.5,
            1.3,
            8.0,
            true,
        );
        poser(
            objects,
            imported,
            "Lanterne du poste de guet 1",
            "nature_lantern.glb",
            4.0,
            -49.5,
            1.0,
            0.0,
            false,
        );
        poser(
            objects,
            imported,
            "Lanterne du poste de guet 2",
            "nature_lantern.glb",
            10.0,
            -49.5,
            1.0,
            0.0,
            false,
        );

        // --- Cabane de garde-forestier, en clairière, entourée de bois de
        // chauffage et de réserves.
        poser(
            objects,
            imported,
            "Cabane du garde-forestier",
            "nature_cabin.glb",
            39.0,
            -22.5,
            1.2,
            250.0,
            true,
        );
        poser(
            objects,
            imported,
            "Bois du garde-forestier 1",
            "nature_woodpile.glb",
            36.0,
            -20.0,
            1.0,
            20.0,
            true,
        );
        poser(
            objects,
            imported,
            "Bois du garde-forestier 2",
            "nature_woodpile.glb",
            42.0,
            -25.0,
            1.0,
            80.0,
            true,
        );
        poser(
            objects,
            imported,
            "Tonneau du garde-forestier",
            "hamlet_barrel.glb",
            37.0,
            -25.5,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Caisse du garde-forestier 1",
            "hamlet_crate.glb",
            41.5,
            -19.5,
            1.0,
            30.0,
            true,
        );
        poser(
            objects,
            imported,
            "Caisse du garde-forestier 2",
            "hamlet_crate.glb",
            35.5,
            -24.0,
            1.0,
            60.0,
            true,
        );

        // --- Camp de chasseurs : second foyer, foin et provisions.
        poser(
            objects,
            imported,
            "Foyer du camp de chasseurs",
            "nature_campfire.glb",
            17.0,
            47.0,
            1.1,
            0.0,
            false,
        );
        poser(
            objects,
            imported,
            "Foin du camp de chasseurs 1",
            "hamlet_hay.glb",
            14.0,
            49.5,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Foin du camp de chasseurs 2",
            "hamlet_hay.glb",
            20.0,
            44.5,
            1.0,
            90.0,
            true,
        );
        poser(
            objects,
            imported,
            "Caisse du camp de chasseurs 1",
            "hamlet_crate.glb",
            19.5,
            50.0,
            1.0,
            15.0,
            true,
        );
        poser(
            objects,
            imported,
            "Caisse du camp de chasseurs 2",
            "hamlet_crate.glb",
            15.5,
            44.0,
            1.0,
            75.0,
            true,
        );
        poser(
            objects,
            imported,
            "Sac du camp de chasseurs",
            "hamlet_bag.glb",
            18.0,
            43.0,
            1.0,
            0.0,
            false,
        );

        // --- Second point d'eau (mare), au Nord-Est, à l'opposé du lac —
        // berge, roseaux, nénuphars, rochers, + ponton de pêche.
        const MARE: [f32; 3] = [0.20, 0.44, 0.66];
        const MARE_SABLE: [f32; 3] = [0.70, 0.62, 0.42];
        aplat(
            objects,
            "Berge de la mare",
            44.0,
            -46.0,
            18.0,
            18.0,
            0.012,
            MARE_SABLE,
        );
        aplat(objects, "Mare", 44.0, -46.0, 14.0, 14.0, 0.015, MARE);
        poser(
            objects,
            imported,
            "Ponton de pêche",
            "nature_boat.glb",
            44.0,
            -37.5,
            1.1,
            180.0,
            false,
        );
        poser(
            objects,
            imported,
            "Panneau du ponton",
            "nature_signpost.glb",
            41.0,
            -38.0,
            1.0,
            160.0,
            false,
        );
        for (i, (name, x, z)) in [
            ("Rocher de la mare 1", 44.0, -55.0),
            ("Rocher de la mare 2", 53.0, -46.0),
            ("Rocher de la mare 3", 35.0, -46.0),
            ("Rocher de la mare 4", 44.0, -37.0),
            ("Rocher de la mare 5", 49.0, -52.0),
            ("Rocher de la mare 6", 38.0, -40.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                objects,
                imported,
                name,
                "nature_rock.glb",
                x,
                z,
                1.0,
                i as f32 * 55.0,
                false,
            );
        }
        for (i, (name, x, z)) in [
            ("Roseaux de la mare 1", 39.0, -51.0),
            ("Roseaux de la mare 2", 49.0, -51.0),
            ("Roseaux de la mare 3", 39.0, -41.5),
            ("Roseaux de la mare 4", 49.0, -41.5),
            ("Roseaux de la mare 5", 44.0, -53.5),
            ("Roseaux de la mare 6", 44.0, -38.5),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                objects,
                imported,
                name,
                "nature_reeds.glb",
                x,
                z,
                1.0,
                i as f32 * 45.0,
                false,
            );
        }
        for (i, (name, x, z)) in [
            ("Nénuphar de la mare 1", 41.0, -46.0),
            ("Nénuphar de la mare 2", 47.0, -46.0),
            ("Nénuphar de la mare 3", 44.0, -49.0),
            ("Nénuphar de la mare 4", 44.0, -43.0),
            ("Nénuphar de la mare 5", 42.0, -49.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                objects,
                imported,
                name,
                "nature_lily.glb",
                x,
                z,
                1.0,
                i as f32 * 60.0,
                false,
            );
        }

        // --- Prairies fleuries : 3 clairières plus denses que le fleurissage
        // existant des îlots, réparties hors des couloirs de vague.
        const PRAIRIES: &[(&str, f32, f32)] = &[
            ("Est", 51.7, 18.8),
            ("Sud-Ouest", -17.8, 48.9),
            ("Ouest", -45.1, -16.4),
            ("Nord", 8.0, -60.0),
        ];
        for (label, cx, cz) in PRAIRIES {
            for i in 0..9 {
                let ang = i as f32 * 40.0;
                let ring = if i % 2 == 0 { 1.8 } else { 3.2 };
                let (dx, dz) = at(ring, ang);
                poser(
                    objects,
                    imported,
                    &format!("Fleurs de la prairie {label} {}", i + 1),
                    "nature_flowers.glb",
                    cx + dx,
                    cz + dz,
                    1.1,
                    ang,
                    false,
                );
            }
        }

        // --- Bosquet/verger : petite clairière plus dense, façon lieu de
        // repos, avec quelques rochers.
        const VERGER_CX: f32 = 28.3;
        const VERGER_CZ: f32 = 28.3;
        const VERGER_FILES: [&str; 4] = [
            "nature_tree.glb",
            "nature_tree2.glb",
            "nature_pine.glb",
            "nature_pine2.glb",
        ];
        {
            let mut rng_verger = crate::runtime::rng::Rng::new(0x5645_5247_4552_3238); // « VERGER28 »
            for i in 0..26 {
                let ang = rng_verger.next_range(0.0, 360.0);
                let r = rng_verger.next_range(1.5, 6.5);
                let (dx, dz) = at(r, ang);
                let file = VERGER_FILES[rng_verger.next_below(VERGER_FILES.len())];
                let scale = rng_verger.next_range(0.85, 1.2);
                poser(
                    objects,
                    imported,
                    &format!("Arbre du verger {}", i + 1),
                    file,
                    VERGER_CX + dx,
                    VERGER_CZ + dz,
                    scale,
                    ang,
                    true,
                );
            }
            for (i, (dx, dz)) in [
                (3.0_f32, 0.0_f32),
                (-3.0, 1.5),
                (0.0, -3.0),
                (4.5, -2.5),
                (-4.5, -1.0),
            ]
            .into_iter()
            .enumerate()
            {
                poser(
                    objects,
                    imported,
                    &format!("Rocher du verger {}", i + 1),
                    "nature_rock.glb",
                    VERGER_CX + dx,
                    VERGER_CZ + dz,
                    1.0,
                    i as f32 * 90.0,
                    false,
                );
            }
        }

        // --- Forêt en anneau (27 → 70 m), couloirs dégagés dans l'axe des 6
        // lisières de spawn, eau/rizière exclues + faune variée.
        let excl_eau: [(f32, f32, f32, f32); 15] = [
            (-34.0, -29.0, -29.0, 29.0),  // rivière ouest
            (-29.0, 29.0, 29.0, 34.0),    // rivière sud
            (-58.0, 27.0, -27.0, 58.0),   // lac + berge
            (-46.0, 57.0, -38.0, 63.0),   // rizière du sud
            (33.0, -58.0, 55.0, -35.0),   // mare du Nord-Est + ponton
            (0.0, -56.0, 14.0, -43.0),    // poste de guet
            (33.0, -28.0, 45.0, -17.0),   // cabane du garde-forestier
            (11.0, 41.0, 23.0, 53.0),     // camp de chasseurs
            (48.0, 15.0, 55.0, 22.0),     // prairie fleurie Est
            (-21.0, 45.0, -14.0, 52.0),   // prairie fleurie Sud-Ouest
            (-49.0, -20.0, -42.0, -13.0), // prairie fleurie Ouest
            (3.0, -64.0, 13.0, -56.0),    // prairie fleurie Nord
            (21.0, 21.0, 36.0, 36.0),     // bosquet/verger
            (39.0, 36.0, 49.0, 46.0),     // clairière de l'Aînée (autel + cage du chef) — Phase D
            (-47.0, -9.0, -36.0, 9.0),    // entrée de grotte, marge ouest — Phase F
        ];
        let mut rng = crate::runtime::rng::Rng::new(0x4841_4D45_4155_3438); // « HAMEAU48 »
        foret_scatter(
            objects,
            imported,
            &mut rng,
            27.0,
            70.0,
            &excl_eau,
            195,
        );

        const FOREST_FAUNA: &[&str] = &[
            "fauna_deer.glb",
            "fauna_rabbit.glb",
            "fauna_squirrel.glb",
            "fauna_fox.glb",
            "fauna_boar.glb",
            "fauna_hedgehog.glb",
            "fauna_goat.glb",
            "fauna_raccoon.glb",
            "fauna_mole.glb",
        ];
        const AIR_FAUNA: &[&str] = &[
            "fauna_bird.glb",
            "fauna_crow.glb",
            "fauna_jay.glb",
            "fauna_bat.glb",
            "fauna_butterfly.glb",
            "fauna_dragonfly.glb",
            "fauna_bee.glb",
            "fauna_ladybug.glb",
        ];
        for (i, &file) in FOREST_FAUNA.iter().chain(AIR_FAUNA.iter()).enumerate() {
            faune_scatter(
                objects,
                imported,
                &mut rng,
                28.0,
                65.0,
                &excl_eau,
                file,
                &format!("Faune {}", i + 1),
                7,
            );
        }
        for (i, (x, z)) in [(0.0_f32, -23.0_f32), (7.0, -49.5), (39.0, -22.5)]
            .into_iter()
            .enumerate()
        {
            poser(
                objects,
                imported,
                &format!("Chouette {}", i + 1),
                "fauna_owl.glb",
                x,
                z,
                0.8,
                0.0,
                false,
            );
        }
}
