use super::*;

pub(super) fn add_village(objects: &mut Vec<SceneObject>, imported: &mut Vec<ImportedMesh>) {
        // --- Place centrale : brasero communal (pack siège, remplace le feu
        // de camp générique — pièce signature de la place, cf.
        // creation3DBlendersuite.md), chaudron, gazebo, beffroi + girouette,
        // lucioles en cercle, dressing de mode (bannière/fanion/trophées).
        poser(
            objects,
            imported,
            "Feu communal",
            "siege_communal_brazier.glb",
            0.0,
            0.0,
            1.2,
            0.0,
            false,
        );
        poser(
            objects,
            imported,
            "Bannière de mode de la place",
            "siege_mode_banner.glb",
            -2.0,
            4.5,
            1.0,
            20.0,
            false,
        );
        poser(
            objects,
            imported,
            "Fanion de la place",
            "siege_team_pennant.glb",
            2.0,
            4.5,
            1.0,
            -20.0,
            false,
        );
        poser(
            objects,
            imported,
            "Trophée de fin de manche",
            "siege_round_trophy.glb",
            0.0,
            3.5,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Portail de fin",
            "siege_end_portal.glb",
            0.0,
            6.0,
            1.0,
            180.0,
            true,
        );
        poser(
            objects,
            imported,
            "Chaudron de la place",
            "hamlet_cauldron.glb",
            1.3,
            0.8,
            1.0,
            20.0,
            true,
        );
        poser(
            objects,
            imported,
            "Gazebo de la place",
            "hamlet_gazebo.glb",
            4.5,
            -3.0,
            1.1,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Beffroi",
            "hamlet_bell_tower.glb",
            -4.5,
            -3.0,
            1.1,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Girouette du beffroi",
            "nature_weathervane.glb",
            -4.5,
            -3.0,
            1.0,
            0.0,
            false,
        );
        for i in 0..6 {
            let az = i as f32 * 60.0;
            let (x, z) = at(2.2, az);
            poser(
                objects,
                imported,
                &format!("Luciole de la place {}", i + 1),
                "fauna_firefly.glb",
                x,
                z,
                1.0,
                az,
                false,
            );
        }

        // --- Anneau de 16 spawns joueur (repères, pas des meshes — cf. la
        // doc de fonction) + 6 lisières de spawn de vagues, une par porte/
        // brèche, à 27 m du centre.
        for i in 0..16 {
            let az = i as f32 * 22.5;
            let (x, z) = at(6.5, az);
            marker(
                objects,
                &format!("Point de spawn joueur {}", i + 1),
                x,
                z,
                [0.3, 0.8, 0.4],
            );
        }
        const WAVE_EDGES: [(&str, f32); 6] = [
            ("Nord", 0.0),
            ("Nord-Est", 45.0),
            ("Est", 90.0),
            ("Sud", 180.0),
            ("Sud-Ouest", 225.0),
            ("Ouest", 270.0),
        ];
        for (label, az) in WAVE_EDGES {
            let (x, z) = at(27.0, az);
            marker(
                objects,
                &format!("Lisière de vague {label}"),
                x,
                z,
                [0.85, 0.25, 0.2],
            );
            poser(
                objects,
                imported,
                &format!("Marqueur de zone {label}"),
                "siege_ground_marker.glb",
                x,
                z,
                1.0,
                az,
                false,
            );
        }

        // --- 4 îlots bâtis aux diagonales (maison + cour clôturée) : la
        // faune paisible (moutons/poules) vit dans deux des quatre cours, la
        // 4ᵉ héberge l'épouvantail + les parterres de fleurs.
        struct Islet {
            label: &'static str,
            house_file: &'static str,
            az: f32,
            fauna: Option<(&'static str, &'static str)>,
            extra: Option<&'static str>,
        }
        const ISLETS: &[Islet] = &[
            Islet {
                label: "Nord-Est",
                house_file: "hamlet_house_a.glb",
                az: 45.0,
                fauna: Some(("Mouton", "fauna_sheep.glb")),
                extra: None,
            },
            Islet {
                label: "Sud-Est",
                house_file: "hamlet_inn.glb",
                az: 135.0,
                fauna: Some(("Poule", "fauna_chicken.glb")),
                extra: None,
            },
            Islet {
                label: "Sud-Ouest",
                house_file: "hamlet_house_b.glb",
                az: 225.0,
                fauna: None,
                extra: None,
            },
            Islet {
                label: "Nord-Ouest",
                house_file: "hamlet_house_c.glb",
                az: 315.0,
                fauna: None,
                extra: Some("nature_scarecrow.glb"),
            },
        ];
        for isl in ISLETS {
            let (hx, hz) = at(13.0, isl.az);
            poser(
                objects,
                imported,
                &format!("Maison {}", isl.label),
                isl.house_file,
                hx,
                hz,
                1.0,
                isl.az + 180.0,
                true,
            );
            let (yx, yz) = at(17.5, isl.az);
            let yh = 3.0;
            let sides = [
                ("Nord", 0.0_f32, -1.0_f32),
                ("Sud", 0.0, 1.0),
                ("Est", 1.0, 0.0),
                ("Ouest", -1.0, 0.0),
            ];
            let mut best = 0usize;
            let mut best_d = f32::MAX;
            for (i, &(_, nx, nz)) in sides.iter().enumerate() {
                let mx = yx + nx * yh;
                let mz = yz + nz * yh;
                let d = mx * mx + mz * mz;
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            for (i, &(slabel, nx, nz)) in sides.iter().enumerate() {
                if i == best {
                    continue; // ouverture côté place
                }
                let mx = yx + nx * yh;
                let mz = yz + nz * yh;
                let yaw = if nz != 0.0 { 0.0 } else { 90.0 };
                poser(
                    objects,
                    imported,
                    &format!("Clôture {} {slabel}", isl.label),
                    "hamlet_fence.glb",
                    mx,
                    mz,
                    3.0,
                    yaw,
                    true,
                );
            }
            if let Some((name, file)) = isl.fauna {
                poser(
                    objects,
                    imported,
                    &format!("{name} {}", isl.label),
                    file,
                    yx - 1.0,
                    yz - 1.0,
                    0.9,
                    0.0,
                    false,
                );
                poser(
                    objects,
                    imported,
                    &format!("{name} {} 2", isl.label),
                    file,
                    yx + 1.0,
                    yz + 1.0,
                    0.9,
                    90.0,
                    false,
                );
            }
            if let Some(file) = isl.extra {
                poser(
                    objects,
                    imported,
                    "Épouvantail",
                    file,
                    yx,
                    yz,
                    1.0,
                    0.0,
                    true,
                );
                poser(
                    objects,
                    imported,
                    "Parterre de fleurs",
                    "nature_flowers.glb",
                    yx + 1.5,
                    yz,
                    1.0,
                    0.0,
                    false,
                );
            }
        }

        // --- Bâtiments d'artisanat, entre les îlots et les murs (flancs des
        // 4 portes cardinales).
        poser(
            objects,
            imported,
            "Forge",
            "hamlet_blacksmith.glb",
            8.0,
            -19.0,
            1.1,
            180.0,
            true,
        );
        poser(
            objects,
            imported,
            "Soufflet de la forge",
            "nature_bellows.glb",
            9.4,
            -18.0,
            1.0,
            180.0,
            true,
        );
        poser(
            objects,
            imported,
            "Marteau-pilon",
            "nature_forge_hammer.glb",
            6.6,
            -18.0,
            1.0,
            180.0,
            true,
        );
        poser(
            objects,
            imported,
            "Métier à tisser",
            "nature_weaving_loom.glb",
            -8.0,
            -19.0,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Écurie",
            "hamlet_stable.glb",
            19.0,
            -8.0,
            1.1,
            270.0,
            true,
        );
        poser(
            objects,
            imported,
            "Foin de l'écurie",
            "hamlet_hay.glb",
            19.0,
            -5.8,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Rouet",
            "nature_spinning_wheel.glb",
            19.0,
            8.0,
            1.0,
            90.0,
            true,
        );
        poser(
            objects,
            imported,
            "Scierie",
            "hamlet_sawmill.glb",
            8.0,
            19.0,
            1.1,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Lame de la scierie",
            "hamlet_sawmill_saw.glb",
            9.4,
            19.6,
            1.0,
            0.0,
            false,
        );
        poser(
            objects,
            imported,
            "Pompe à eau",
            "nature_water_pump.glb",
            -8.0,
            19.0,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Puits",
            "hamlet_well.glb",
            -19.0,
            -8.0,
            1.0,
            90.0,
            true,
        );
        poser(
            objects,
            imported,
            "Treuil du puits",
            "nature_well_windlass.glb",
            -19.0,
            -6.0,
            1.0,
            90.0,
            true,
        );
        poser(
            objects,
            imported,
            "Moulin",
            "hamlet_mill.glb",
            -19.0,
            8.0,
            1.1,
            90.0,
            true,
        );

        // --- Mobilier de place/marché.
        poser(
            objects,
            imported,
            "Étal du marché 1",
            "hamlet_market_stand_a.glb",
            6.0,
            3.5,
            1.0,
            200.0,
            true,
        );
        poser(
            objects,
            imported,
            "Étal du marché 2",
            "hamlet_market_stand_b.glb",
            -6.0,
            3.5,
            1.0,
            340.0,
            true,
        );
        poser(
            objects,
            imported,
            "Banc du marché 1",
            "hamlet_bench_a.glb",
            3.0,
            6.5,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Banc du marché 2",
            "hamlet_bench_b.glb",
            -3.0,
            6.5,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Tonneau du marché 1",
            "hamlet_barrel.glb",
            6.5,
            -0.5,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Tonneau du marché 2",
            "hamlet_barrel.glb",
            6.9,
            1.0,
            1.0,
            30.0,
            true,
        );
        poser(
            objects,
            imported,
            "Tonneau du marché 3",
            "hamlet_barrel.glb",
            -6.5,
            -0.5,
            1.0,
            60.0,
            true,
        );
        poser(
            objects,
            imported,
            "Caisse du marché 1",
            "hamlet_crate.glb",
            7.5,
            0.5,
            1.0,
            0.0,
            true,
        );
        poser(
            objects,
            imported,
            "Caisse du marché 2",
            "hamlet_crate.glb",
            -7.5,
            0.5,
            1.0,
            45.0,
            true,
        );
        poser(
            objects,
            imported,
            "Caisse du marché 3",
            "hamlet_crate.glb",
            -7.2,
            -1.5,
            1.0,
            90.0,
            true,
        );
        poser(
            objects,
            imported,
            "Sac du marché",
            "hamlet_bag.glb",
            5.0,
            5.0,
            1.0,
            0.0,
            false,
        );
        poser(
            objects,
            imported,
            "Sac ouvert du marché",
            "hamlet_bag_open.glb",
            5.5,
            5.4,
            1.0,
            20.0,
            false,
        );
        poser(
            objects,
            imported,
            "Sacs du marché",
            "hamlet_bags.glb",
            -5.0,
            5.0,
            1.0,
            0.0,
            false,
        );
        poser(
            objects,
            imported,
            "Paquet du marché 1",
            "hamlet_package_a.glb",
            -5.5,
            -5.5,
            1.0,
            0.0,
            false,
        );
        poser(
            objects,
            imported,
            "Paquet du marché 2",
            "hamlet_package_b.glb",
            5.5,
            -5.5,
            1.0,
            30.0,
            false,
        );
        poser(
            objects,
            imported,
            "Chaise du marché 1",
            "hamlet_chair.glb",
            2.0,
            6.0,
            1.0,
            0.0,
            false,
        );
        poser(
            objects,
            imported,
            "Chaise du marché 2",
            "hamlet_chair.glb",
            -2.0,
            6.0,
            1.0,
            180.0,
            false,
        );

        // --- Lanternes (2/porte) + bannière (1/porte) aux 4 portes
        // principales : télégraphe visuel de l'arrivée des vagues (GDD §10).
        const GATES: [(&str, f32, f32, f32); 4] = [
            ("Nord", 0.0, -HALF, 0.0),
            ("Sud", 0.0, HALF, 180.0),
            ("Est", HALF, 0.0, 90.0),
            ("Ouest", -HALF, 0.0, 270.0),
        ];
        for (label, gx, gz, yaw) in GATES {
            let (dx, dz) = if gz.abs() > gx.abs() {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            };
            poser(
                objects,
                imported,
                &format!("Lanterne {label} 1"),
                "nature_lantern.glb",
                gx - dx * 3.0,
                gz - dz * 3.0,
                1.0,
                0.0,
                false,
            );
            poser(
                objects,
                imported,
                &format!("Lanterne {label} 2"),
                "nature_lantern.glb",
                gx + dx * 3.0,
                gz + dz * 3.0,
                1.0,
                0.0,
                false,
            );
            poser(
                objects,
                imported,
                &format!("Bannière {label}"),
                "nature_banner.glb",
                gx,
                gz,
                1.0,
                yaw,
                false,
            );
        }

}
