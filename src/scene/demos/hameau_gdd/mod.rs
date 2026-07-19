use super::*;

mod helpers;
use helpers::*;

mod fort;

pub(super) const HALF: f32 = 24.0; // fort 48×48, centré à l'origine
pub(super) const GATE_HALF: f32 = 2.5;
pub(super) const TRIM: f32 = 5.0;

impl Scene {
    /// Démo « Hameau fortifié » (GDD §7 « le hameau est du gameplay », §7.3
    /// « la vie du hameau », §5.4 archétypes de créatures, §10 direction
    /// artistique) — prototypée visuellement dans Blender avant intégration
    /// ici, patron identique à `mmorpg_demo` (tables de données + closures/fn
    /// locales de pose, pas de JSON écrit à la main) mais géométrie
    /// entièrement différente : fort carré 48×48 (remparts, 4 portes + 2
    /// brèches diagonales, chemin de ronde), place centrale, anneau de 16
    /// spawns joueur, 4 îlots bâtis, artisanat, marché, lanternes/bannières,
    /// rivière/lac hors les murs, forêt en anneau avec couloirs dégagés dans
    /// l'axe des 6 lisières de spawn de vagues, faune variée.
    ///
    /// Créatures : reprises telles quelles de `mmorpg_demo()` (mêmes
    /// composants — mesh/physics/script/trigger/collision_layer — c'est ce
    /// que compare le garde-fou `the_embedded_scene_creatures_match_the_demo`,
    /// **par nom**, pas par position), spawns conservés à l'identique : aucune
    /// vague/créature existante ne disparaît silencieusement (cf. la consigne
    /// d'intégration). Comme le nouveau fort occupe globalement le même ordre
    /// de grandeur que l'ancienne arène (le rayon de la forêt va jusqu'à 70 m,
    /// l'ancienne carte faisait 72 m de côté), les spawns d'origine restent
    /// dans une zone plausible (forêt/berge) plutôt que dans un mur neuf.
    ///
    /// Écarts assumés par rapport au prototype Blender (documentés dans le
    /// rapport d'intégration, pas de garde-fou automatisé ne les couvre) :
    /// - Les « marqueurs, pas des meshes » (lisières de vague, anneau de spawn
    ///   joueur) sont de minuscules cylindres non solides : le moteur n'a pas
    ///   de type « Empty » distinct d'un mesh (cf. `MeshKind`), c'est
    ///   l'équivalent le plus proche.
    /// - Chaque cour est fermée par exactement 3 panneaux `hamlet_fence.glb`
    ///   à l'échelle native (~3 m), un par côté bâti — pas une rangée de
    ///   panneaux jointifs : au sens strict ça laisse un jour entre panneau et
    ///   coin de cour plutôt qu'un mur continu, mais respecte la spec « 3
    ///   pans, une ouverture côté place ».
    /// - Pas de collision d'eau dédiée (pas de « Mur d'eau » invisible comme
    ///   dans `mmorpg_demo`) : la rivière/le lac ne sont que des aplats
    ///   visuels non solides. Aucun garde-fou de cette nouvelle démo n'exige
    ///   un blocage de baignade (contrairement à `mmorpg_demo`) ; à ajouter
    ///   si un jour cette carte a ses propres tests d'étanchéité.
    pub fn hameau_gdd_demo() -> Self {
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(90.0, 1.0, 90.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.30, 0.40, 0.22];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.tag = "joueur".into();
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            fire_button: "Feu".into(),
            weapon_button: "Arme".into(),
            heal_button: "Soin".into(),
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];
        let mut imported: Vec<ImportedMesh> = Vec::new();

        fort::add_fort(&mut objects, &mut imported);

        // --- Place centrale : brasero communal (pack siège, remplace le feu
        // de camp générique — pièce signature de la place, cf.
        // creation3DBlendersuite.md), chaudron, gazebo, beffroi + girouette,
        // lucioles en cercle, dressing de mode (bannière/fanion/trophées).
        poser(
            &mut objects,
            &mut imported,
            "Feu communal",
            "siege_communal_brazier.glb",
            0.0,
            0.0,
            1.2,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bannière de mode de la place",
            "siege_mode_banner.glb",
            -2.0,
            4.5,
            1.0,
            20.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Fanion de la place",
            "siege_team_pennant.glb",
            2.0,
            4.5,
            1.0,
            -20.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Trophée de fin de manche",
            "siege_round_trophy.glb",
            0.0,
            3.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Portail de fin",
            "siege_end_portal.glb",
            0.0,
            6.0,
            1.0,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Chaudron de la place",
            "hamlet_cauldron.glb",
            1.3,
            0.8,
            1.0,
            20.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Gazebo de la place",
            "hamlet_gazebo.glb",
            4.5,
            -3.0,
            1.1,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Beffroi",
            "hamlet_bell_tower.glb",
            -4.5,
            -3.0,
            1.1,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
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
                &mut objects,
                &mut imported,
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
                &mut objects,
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
                &mut objects,
                &format!("Lisière de vague {label}"),
                x,
                z,
                [0.85, 0.25, 0.2],
            );
            poser(
                &mut objects,
                &mut imported,
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
                &mut objects,
                &mut imported,
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
                    &mut objects,
                    &mut imported,
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
                    &mut objects,
                    &mut imported,
                    &format!("{name} {}", isl.label),
                    file,
                    yx - 1.0,
                    yz - 1.0,
                    0.9,
                    0.0,
                    false,
                );
                poser(
                    &mut objects,
                    &mut imported,
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
                    &mut objects,
                    &mut imported,
                    "Épouvantail",
                    file,
                    yx,
                    yz,
                    1.0,
                    0.0,
                    true,
                );
                poser(
                    &mut objects,
                    &mut imported,
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
            &mut objects,
            &mut imported,
            "Forge",
            "hamlet_blacksmith.glb",
            8.0,
            -19.0,
            1.1,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Soufflet de la forge",
            "nature_bellows.glb",
            9.4,
            -18.0,
            1.0,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Marteau-pilon",
            "nature_forge_hammer.glb",
            6.6,
            -18.0,
            1.0,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Métier à tisser",
            "nature_weaving_loom.glb",
            -8.0,
            -19.0,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Écurie",
            "hamlet_stable.glb",
            19.0,
            -8.0,
            1.1,
            270.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Foin de l'écurie",
            "hamlet_hay.glb",
            19.0,
            -5.8,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Rouet",
            "nature_spinning_wheel.glb",
            19.0,
            8.0,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Scierie",
            "hamlet_sawmill.glb",
            8.0,
            19.0,
            1.1,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Lame de la scierie",
            "hamlet_sawmill_saw.glb",
            9.4,
            19.6,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Pompe à eau",
            "nature_water_pump.glb",
            -8.0,
            19.0,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Puits",
            "hamlet_well.glb",
            -19.0,
            -8.0,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Treuil du puits",
            "nature_well_windlass.glb",
            -19.0,
            -6.0,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
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
            &mut objects,
            &mut imported,
            "Étal du marché 1",
            "hamlet_market_stand_a.glb",
            6.0,
            3.5,
            1.0,
            200.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Étal du marché 2",
            "hamlet_market_stand_b.glb",
            -6.0,
            3.5,
            1.0,
            340.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Banc du marché 1",
            "hamlet_bench_a.glb",
            3.0,
            6.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Banc du marché 2",
            "hamlet_bench_b.glb",
            -3.0,
            6.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tonneau du marché 1",
            "hamlet_barrel.glb",
            6.5,
            -0.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tonneau du marché 2",
            "hamlet_barrel.glb",
            6.9,
            1.0,
            1.0,
            30.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tonneau du marché 3",
            "hamlet_barrel.glb",
            -6.5,
            -0.5,
            1.0,
            60.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du marché 1",
            "hamlet_crate.glb",
            7.5,
            0.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du marché 2",
            "hamlet_crate.glb",
            -7.5,
            0.5,
            1.0,
            45.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du marché 3",
            "hamlet_crate.glb",
            -7.2,
            -1.5,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Sac du marché",
            "hamlet_bag.glb",
            5.0,
            5.0,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Sac ouvert du marché",
            "hamlet_bag_open.glb",
            5.5,
            5.4,
            1.0,
            20.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Sacs du marché",
            "hamlet_bags.glb",
            -5.0,
            5.0,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Paquet du marché 1",
            "hamlet_package_a.glb",
            -5.5,
            -5.5,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Paquet du marché 2",
            "hamlet_package_b.glb",
            5.5,
            -5.5,
            1.0,
            30.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Chaise du marché 1",
            "hamlet_chair.glb",
            2.0,
            6.0,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
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
                &mut objects,
                &mut imported,
                &format!("Lanterne {label} 1"),
                "nature_lantern.glb",
                gx - dx * 3.0,
                gz - dz * 3.0,
                1.0,
                0.0,
                false,
            );
            poser(
                &mut objects,
                &mut imported,
                &format!("Lanterne {label} 2"),
                "nature_lantern.glb",
                gx + dx * 3.0,
                gz + dz * 3.0,
                1.0,
                0.0,
                false,
            );
            poser(
                &mut objects,
                &mut imported,
                &format!("Bannière {label}"),
                "nature_banner.glb",
                gx,
                gz,
                1.0,
                yaw,
                false,
            );
        }

        // --- Hors les murs : rivière (deux bras, ouest et sud) rejoignant un
        // lac au coin sud-ouest, pont, moulin à eau, berges, petite rizière.
        const EAU: [f32; 3] = [0.18, 0.42, 0.65];
        const EAU_LAC: [f32; 3] = [0.14, 0.34, 0.55];
        const SABLE: [f32; 3] = [0.72, 0.64, 0.44];
        aplat(
            &mut objects,
            "Rivière ouest",
            -31.5,
            0.0,
            5.0,
            58.0,
            0.02,
            EAU,
        );
        aplat(&mut objects, "Rivière sud", 0.0, 31.5, 58.0, 5.0, 0.02, EAU);
        aplat(
            &mut objects,
            "Berge du lac",
            -42.5,
            42.5,
            29.0,
            29.0,
            0.012,
            SABLE,
        );
        aplat(&mut objects, "Lac", -42.0, 42.0, 24.0, 24.0, 0.015, EAU_LAC);
        aplat(
            &mut objects,
            "Rizière du sud",
            -42.0,
            60.0,
            8.0,
            6.0,
            0.03,
            [0.55, 0.6, 0.25],
        );
        poser(
            &mut objects,
            &mut imported,
            "Pont de la rivière ouest",
            "nature_bridge.glb",
            -31.5,
            0.0,
            1.15,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Moulin à eau",
            "nature_watermill.glb",
            -36.5,
            8.0,
            1.1,
            90.0,
            true,
        );
        for (i, (name, x, z)) in [
            ("Roseaux 1", -29.5, -15.0),
            ("Roseaux 2", -29.5, 15.0),
            ("Roseaux 3", 0.0, 30.5),
            ("Roseaux 4", -20.0, 30.5),
            ("Roseaux 5", -46.0, 30.0),
            ("Roseaux 6", -38.0, 46.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_reeds.glb",
                x,
                z,
                1.0,
                i as f32 * 40.0,
                false,
            );
        }
        for (i, (name, x, z)) in [
            ("Nénuphars 1", -38.0, 38.0),
            ("Nénuphars 2", -46.0, 46.0),
            ("Nénuphars 3", -44.0, 36.0),
            ("Nénuphars 4", -40.0, 44.0),
            ("Nénuphars 5", -44.0, 40.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_lily.glb",
                x,
                z,
                1.0,
                i as f32 * 50.0,
                false,
            );
        }
        for (i, (name, x, z)) in [
            ("Rocher de berge 1", -33.0, -20.0),
            ("Rocher de berge 2", -33.0, 20.0),
            ("Rocher de berge 3", -25.0, 33.0),
            ("Rocher de berge 4", -46.0, 42.0),
            ("Rocher de berge 5", 10.0, 32.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_rock.glb",
                x,
                z,
                1.0,
                i as f32 * 70.0,
                false,
            );
        }
        for i in 0..4 {
            poser(
                &mut objects,
                &mut imported,
                &format!("Riz du sud {}", i + 1),
                "nature_rice.glb",
                -45.0 + i as f32 * 3.0,
                60.0,
                1.0,
                0.0,
                false,
            );
        }

        // --- Habillage organique `shore_*` (Phase E, traduit de la
        // composition Blender `docs/blender/composition_eau.blend`, Phase B) :
        // berges/rochers lissés autour des 3 rects d'eau réels
        // (Rivière ouest/sud, Lac) — cf. `mapsJeuReflexionAnalyse.md` §2 bis.2
        // point 3. Décor purement cosmétique, non solide (comme les Roseaux/
        // Nénuphars ci-dessus), pour ne pas gêner les sondes IA (§4.2 règle 1
        // du doc de réflexion).
        const SHORE: &[(&str, f32, f32, f32, f32)] = &[
            ("shore_bank_moss", -22.15, 27.54, 0.757, 207.8),
            ("shore_bank_moss", 7.74, 27.71, 1.005, 170.3),
            ("shore_bank_moss", 11.59, 27.88, 0.812, 252.0),
            ("shore_bank_moss", -0.83, 28.15, 1.068, 64.4),
            ("shore_bank_moss", 3.08, 28.48, 0.793, 70.2),
            ("shore_bank_moss", -17.10, 28.53, 0.867, 117.5),
            ("shore_beached_algae", -33.48, 31.39, 0.695, 43.1),
            ("shore_beached_fish", -33.71, 32.97, 0.768, 269.1),
            ("shore_drift_line", 12.32, 30.29, 0.574, 270.9),
            ("shore_drift_line", 6.98, 30.31, 0.502, 228.4),
            ("shore_driftwood", -26.58, 30.81, 0.606, 335.6),
            ("shore_driftwood", 20.75, 31.89, 0.610, 304.9),
            ("shore_driftwood", 16.83, 32.01, 0.615, 270.1),
            ("shore_gentle_bank", -35.33, -19.33, 0.957, 117.5),
            ("shore_gentle_bank", -34.65, -10.40, 0.914, 327.7),
            ("shore_gentle_bank", -28.20, 2.89, 0.700, 356.4),
            ("shore_gentle_bank", -34.65, 12.65, 1.049, 141.9),
            ("shore_gentle_bank", -28.56, 13.02, 0.832, 52.7),
            ("shore_gentle_bank", -35.38, 20.69, 0.816, 89.8),
            ("shore_gentle_bank", -4.67, 27.40, 0.901, 128.1),
            ("shore_gentle_bank", 20.75, 27.53, 1.073, 40.5),
            ("shore_gentle_bank", -9.03, 28.23, 0.954, 134.6),
            ("shore_gentle_bank", -27.50, 28.31, 0.906, 220.7),
            ("shore_gentle_bank", 24.81, 28.34, 0.909, 292.2),
            ("shore_gentle_bank", -50.00, 28.80, 0.994, 334.1),
            ("shore_gentle_bank", -28.44, 33.00, 1.130, 160.3),
            ("shore_gentle_bank", 17.19, 34.64, 0.887, 302.8),
            ("shore_gentle_bank", 10.49, 34.75, 0.765, 42.5),
            ("shore_gentle_bank", -13.24, 34.82, 0.785, 90.6),
            ("shore_gentle_bank", -4.92, 34.91, 0.729, 144.2),
            ("shore_gentle_bank", -28.51, 42.00, 0.981, 149.0),
            ("shore_natural_basin", -34.00, 28.45, 0.912, 311.2),
            ("shore_natural_basin", -50.00, 55.84, 0.823, 356.6),
            ("shore_nest", -32.50, 32.00, 1.000, 39.6),
            ("shore_pebble_group", 21.87, 34.56, 0.894, 320.2),
            ("shore_pebble_group", 2.16, 34.86, 0.850, 304.8),
            ("shore_pebble_group", -9.55, 34.96, 0.927, 189.8),
            ("shore_pebble_group", -26.98, 35.05, 0.967, 124.4),
            ("shore_rooted_bank", -35.59, -15.27, 0.998, 302.5),
            ("shore_rooted_bank", -34.63, 2.51, 0.796, 263.5),
            ("shore_rooted_bank", -34.69, 16.57, 0.880, 216.5),
            ("shore_rooted_bank", -55.40, 33.00, 1.094, 84.0),
            ("shore_rooted_bank", -55.27, 42.00, 0.865, 327.2),
            ("shore_rooted_bank", -55.51, 51.00, 0.912, 339.7),
            ("shore_rooted_bank", -28.14, 51.00, 1.058, 218.7),
            ("shore_rooted_bank", -34.00, 55.11, 0.885, 115.8),
            ("shore_shell_cluster", -33.65, 32.02, 0.658, 73.0),
            ("shore_smooth_rock", -33.63, -23.27, 0.457, 21.4),
            ("shore_smooth_rock", -27.64, -22.96, 0.885, 235.4),
            ("shore_smooth_rock", -33.23, -15.61, 0.405, 24.1),
            ("shore_smooth_rock", -28.18, -9.15, 0.760, 203.8),
            ("shore_smooth_rock", -31.02, 13.18, 0.422, 139.0),
            ("shore_smooth_rock", -27.77, 16.44, 0.816, 48.9),
            ("shore_smooth_rock", -33.49, 20.06, 0.477, 184.4),
            ("shore_smooth_rock", -0.49, 34.44, 0.939, 192.8),
            ("shore_smooth_rock", -22.77, 34.59, 0.875, 305.9),
            ("shore_smooth_rock", -17.93, 34.71, 0.986, 281.2),
            ("shore_smooth_rock", 25.23, 34.94, 0.803, 50.3),
            ("shore_smooth_rock", 8.12, 35.35, 0.942, 39.9),
            ("shore_steep_bank", -28.41, -27.12, 0.889, 184.3),
            ("shore_steep_bank", -35.44, -27.00, 1.023, 308.5),
            ("shore_steep_bank", -34.57, -23.31, 1.062, 193.9),
            ("shore_steep_bank", -27.96, -20.12, 0.899, 179.9),
            ("shore_steep_bank", -28.09, -15.75, 0.869, 10.7),
            ("shore_steep_bank", -27.89, -7.22, 0.839, 3.9),
            ("shore_steep_bank", -34.80, -6.23, 1.015, 347.1),
            ("shore_steep_bank", -27.91, 21.31, 0.845, 139.8),
            ("shore_steep_bank", -27.67, 25.13, 0.738, 351.8),
            ("shore_steep_bank", -34.71, 25.67, 0.959, 296.5),
            ("shore_steep_bank", -13.32, 27.80, 0.767, 66.6),
            ("shore_steep_bank", 16.32, 28.19, 1.074, 264.6),
            ("shore_water_ripple", -33.00, 34.00, 0.803, 98.2),
            ("shore_water_ripple", -33.00, 50.00, 0.922, 188.9),
            ("shore_water_ripple", -50.00, 51.00, 0.874, 141.9),
        ];
        for (i, (base, x, z, scale, yaw)) in SHORE.iter().enumerate() {
            let file: &'static str = match *base {
                "shore_bank_moss" => "shore_bank_moss.glb",
                "shore_beached_algae" => "shore_beached_algae.glb",
                "shore_beached_fish" => "shore_beached_fish.glb",
                "shore_drift_line" => "shore_drift_line.glb",
                "shore_driftwood" => "shore_driftwood.glb",
                "shore_gentle_bank" => "shore_gentle_bank.glb",
                "shore_natural_basin" => "shore_natural_basin.glb",
                "shore_nest" => "shore_nest.glb",
                "shore_pebble_group" => "shore_pebble_group.glb",
                "shore_rooted_bank" => "shore_rooted_bank.glb",
                "shore_shell_cluster" => "shore_shell_cluster.glb",
                "shore_smooth_rock" => "shore_smooth_rock.glb",
                "shore_steep_bank" => "shore_steep_bank.glb",
                "shore_water_ripple" => "shore_water_ripple.glb",
                other => unreachable!("asset shore_* non mappé : {other}"),
            };
            poser(
                &mut objects,
                &mut imported,
                &format!("{base} {}", i + 1),
                file,
                *x,
                *z,
                *scale,
                *yaw,
                false,
            );
        }

        // --- Habillage organique `grotto_*` (Phase F, traduit de la
        // composition Blender `docs/blender/composition_grotto.blend`,
        // Phase C) : entrée de grotte sur la marge ouest de
        // `hameau_gdd_demo()`, juste au-delà de la Rivière ouest (bord est à
        // x=-34). **Composition dégradée assumée** : `hameau_gdd_demo()` n'a
        // aucun relief/heightmap (`Sol` = `MeshKind::Plane`), contrairement à
        // `mmorpg_demo()` — la Phase C a vérifié l'absence de terrain à
        // intégrer avant de composer (décision actée dans
        // `sprintjeurefelxion.md` §4, sculpter un nouveau relief reste hors
        // scope de ce sprint). Formations structurelles (mur du fond, arche,
        // passage bas, colonnes, poutres, stalagmites larges, bloc effondré)
        // solides en TriMesh — suit le maillage réel, ne bloque donc pas le
        // passage sous l'arche/le passage bas ; le reste est cosmétique.
        const GROTTO: &[(&str, f32, f32, f32, f32, f32)] = &[
            // (base, x, z, hauteur, scale, yaw_deg)
            ("grotto_back_wall", -45.0, 0.0, 0.0, 1.0, 90.0),
            ("grotto_bones", -37.0, -4.0, 0.0, 0.9, 45.0),
            ("grotto_bumpy_floor", -38.5, 0.0, -0.02, 1.0, 0.0),
            ("grotto_collapsed_block", -40.0, 6.5, 0.0, 1.0, 15.0),
            ("grotto_column", -38.0, -2.8, 0.0, 1.0, 0.0),
            ("grotto_column", -38.0, 2.8, 0.0, 1.0, 0.0),
            ("grotto_crystal", -44.0, -1.0, 0.0, 0.6, 120.0),
            ("grotto_crystal", -41.0, 4.5, 0.0, 0.8, 30.0),
            ("grotto_entrance_arch", -37.5, 0.0, 0.0, 1.1, 90.0),
            ("grotto_glow_mushroom_cluster", -39.5, -4.5, 0.0, 1.0, 0.0),
            ("grotto_glow_mushroom_small", -38.5, 4.0, 0.0, 0.8, 0.0),
            ("grotto_hanging_drop", -37.6, 0.6, 2.0, 1.0, 0.0),
            ("grotto_hanging_root", -44.7, -1.8, 1.7, 1.0, 25.0),
            ("grotto_low_passage", -39.5, 0.0, 0.0, 1.0, 90.0),
            ("grotto_mold_veil", -44.8, 1.8, 1.2, 1.0, 60.0),
            ("grotto_rubble", -36.5, 2.5, 0.0, 1.0, 0.0),
            ("grotto_stalactite_large", -44.6, 1.2, 1.9, 0.9, 40.0),
            ("grotto_stalactite_large", -37.8, -1.0, 2.1, 1.0, 0.0),
            ("grotto_stalactite_small", -44.8, -1.5, 1.8, 0.7, 70.0),
            ("grotto_stalactite_small", -38.2, 1.0, 1.9, 0.8, 10.0),
            ("grotto_stalagmite_large", -44.5, -3.0, 0.0, 0.95, 55.0),
            ("grotto_stalagmite_large", -42.0, 3.5, 0.0, 1.0, 0.0),
            ("grotto_stalagmite_small", -43.5, 2.5, 0.0, 0.6, 90.0),
            ("grotto_stalagmite_small", -39.0, -3.5, 0.0, 0.7, 20.0),
            ("grotto_support_beam", -41.5, -1.5, 0.0, 0.9, 15.0),
            ("grotto_support_beam", -41.5, 1.8, 0.0, 0.9, 340.0),
            ("grotto_underground_puddle", -42.0, 0.5, 0.01, 1.0, 0.0),
        ];
        for (i, (base, x, z, height, scale, yaw)) in GROTTO.iter().enumerate() {
            let file: &'static str = match *base {
                "grotto_back_wall" => "grotto_back_wall.glb",
                "grotto_bones" => "grotto_bones.glb",
                "grotto_bumpy_floor" => "grotto_bumpy_floor.glb",
                "grotto_collapsed_block" => "grotto_collapsed_block.glb",
                "grotto_column" => "grotto_column.glb",
                "grotto_crystal" => "grotto_crystal.glb",
                "grotto_entrance_arch" => "grotto_entrance_arch.glb",
                "grotto_glow_mushroom_cluster" => "grotto_glow_mushroom_cluster.glb",
                "grotto_glow_mushroom_small" => "grotto_glow_mushroom_small.glb",
                "grotto_hanging_drop" => "grotto_hanging_drop.glb",
                "grotto_hanging_root" => "grotto_hanging_root.glb",
                "grotto_low_passage" => "grotto_low_passage.glb",
                "grotto_mold_veil" => "grotto_mold_veil.glb",
                "grotto_rubble" => "grotto_rubble.glb",
                "grotto_stalactite_large" => "grotto_stalactite_large.glb",
                "grotto_stalactite_small" => "grotto_stalactite_small.glb",
                "grotto_stalagmite_large" => "grotto_stalagmite_large.glb",
                "grotto_stalagmite_small" => "grotto_stalagmite_small.glb",
                "grotto_support_beam" => "grotto_support_beam.glb",
                "grotto_underground_puddle" => "grotto_underground_puddle.glb",
                other => unreachable!("asset grotto_* non mappé : {other}"),
            };
            let solide = matches!(
                *base,
                "grotto_back_wall"
                    | "grotto_entrance_arch"
                    | "grotto_low_passage"
                    | "grotto_column"
                    | "grotto_support_beam"
                    | "grotto_stalagmite_large"
                    | "grotto_collapsed_block"
            );
            poser(
                &mut objects,
                &mut imported,
                &format!("{base} {}", i + 1),
                file,
                *x,
                *z,
                *scale,
                *yaw,
                solide,
            );
            if *height != 0.0
                && let Some(o) = objects.last_mut()
            {
                o.transform.position.y = *height;
            }
        }

        // --- Poste de guet, sur l'axe d'approche Nord (léger décalage pour
        // ne pas boucher la lisière de vague), avec 2 lanternes.
        poser(
            &mut objects,
            &mut imported,
            "Poste de guet",
            "nature_tower.glb",
            7.0,
            -49.5,
            1.3,
            8.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Lanterne du poste de guet 1",
            "nature_lantern.glb",
            4.0,
            -49.5,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
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
            &mut objects,
            &mut imported,
            "Cabane du garde-forestier",
            "nature_cabin.glb",
            39.0,
            -22.5,
            1.2,
            250.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bois du garde-forestier 1",
            "nature_woodpile.glb",
            36.0,
            -20.0,
            1.0,
            20.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bois du garde-forestier 2",
            "nature_woodpile.glb",
            42.0,
            -25.0,
            1.0,
            80.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tonneau du garde-forestier",
            "hamlet_barrel.glb",
            37.0,
            -25.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du garde-forestier 1",
            "hamlet_crate.glb",
            41.5,
            -19.5,
            1.0,
            30.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
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
            &mut objects,
            &mut imported,
            "Foyer du camp de chasseurs",
            "nature_campfire.glb",
            17.0,
            47.0,
            1.1,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Foin du camp de chasseurs 1",
            "hamlet_hay.glb",
            14.0,
            49.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Foin du camp de chasseurs 2",
            "hamlet_hay.glb",
            20.0,
            44.5,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du camp de chasseurs 1",
            "hamlet_crate.glb",
            19.5,
            50.0,
            1.0,
            15.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du camp de chasseurs 2",
            "hamlet_crate.glb",
            15.5,
            44.0,
            1.0,
            75.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
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
            &mut objects,
            "Berge de la mare",
            44.0,
            -46.0,
            18.0,
            18.0,
            0.012,
            MARE_SABLE,
        );
        aplat(&mut objects, "Mare", 44.0, -46.0, 14.0, 14.0, 0.015, MARE);
        poser(
            &mut objects,
            &mut imported,
            "Ponton de pêche",
            "nature_boat.glb",
            44.0,
            -37.5,
            1.1,
            180.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
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
                &mut objects,
                &mut imported,
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
                &mut objects,
                &mut imported,
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
                &mut objects,
                &mut imported,
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
                    &mut objects,
                    &mut imported,
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
                    &mut objects,
                    &mut imported,
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
                    &mut objects,
                    &mut imported,
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
            &mut objects,
            &mut imported,
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
                &mut objects,
                &mut imported,
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
                &mut objects,
                &mut imported,
                &format!("Chouette {}", i + 1),
                "fauna_owl.glb",
                x,
                z,
                0.8,
                0.0,
                false,
            );
        }
        // --- Faune aquatique : lac historique (Sud-Ouest) + nouvelle mare
        // (Nord-Est), comptes doublés/triplés par rapport à la version
        // d'origine (1 par espèce).
        for (name, file, x, z) in [
            ("Canard 1", "fauna_duck.glb", -31.5, 10.0),
            ("Canard 2", "fauna_duck.glb", 47.0, -44.0),
            ("Canard 3", "fauna_duck.glb", -44.0, 44.0),
            ("Oie 1", "fauna_goose.glb", -31.5, -10.0),
            ("Oie 2", "fauna_goose.glb", 41.0, -48.0),
            ("Oie 3", "fauna_goose.glb", -40.0, 38.0),
            ("Grenouille 1", "fauna_frog.glb", -30.0, 20.0),
            ("Grenouille 2", "fauna_frog.glb", 40.0, -42.0),
            ("Grenouille 3", "fauna_frog.glb", -36.0, 34.0),
            ("Grenouille 4", "fauna_frog.glb", 48.0, -49.0),
            ("Poisson 1", "fauna_fish.glb", -42.0, 42.0),
            ("Poisson 2", "fauna_fish.glb", 44.0, -46.0),
            ("Poisson 3", "fauna_fish.glb", -45.0, 39.0),
            ("Poisson 4", "fauna_fish.glb", 42.0, -49.0),
            ("Héron 1", "fauna_heron.glb", -34.0, 28.0),
            ("Héron 2", "fauna_heron.glb", 37.0, -37.0),
            ("Héron 3", "fauna_heron.glb", -48.0, 48.0),
            ("Tortue 1", "fauna_turtle.glb", -46.0, 46.0),
            ("Tortue 2", "fauna_turtle.glb", 50.0, -46.0),
            ("Crabe 1", "fauna_crab.glb", -30.0, 31.0),
            ("Crabe 2", "fauna_crab.glb", 43.0, -55.0),
            ("Crabe 3", "fauna_crab.glb", -25.0, 34.0),
            ("Escargot 1", "fauna_snail.glb", -29.5, 22.0),
            ("Escargot 2", "fauna_snail.glb", 36.0, -47.0),
            ("Escargot 3", "fauna_snail.glb", -47.0, 41.0),
        ] {
            poser(
                &mut objects,
                &mut imported,
                name,
                file,
                x,
                z,
                0.9,
                0.0,
                false,
            );
        }
        // --- Lucioles supplémentaires près du camp de chasseurs (ambiance
        // nocturne, en plus du cercle de 6 sur la place).
        for (i, (x, z)) in [(20.0_f32, 49.0_f32), (14.0, 45.0), (18.0, 51.0)]
            .into_iter()
            .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                &format!("Luciole du camp de chasseurs {}", i + 1),
                "fauna_firefly.glb",
                x,
                z,
                1.0,
                i as f32 * 40.0,
                false,
            );
        }

        // --- Lande environnante (pack siège, creation3DBlendersuite.md) :
        // décor dispersé dans l'anneau extérieur, en évitant les zones déjà
        // nommées (îlots r=13-20, camp de chasseurs ~(14-20,45-51), mare aux
        // nénuphars ~(41-49,-43 à -49), prairies, verger (28.3,28.3)) — au
        // meilleur effort, pas un compactage garanti sans chevauchement.
        const LANDE: &[(&str, f32, f32, f32)] = &[
            ("Rocher de lande", -38.0, 5.0, 0.0),
            ("Rocher de lande 2", 36.0, -20.0, 40.0),
            ("Arbre mort tourmenté", -34.0, -25.0, 20.0),
            ("Ossements épars", -30.0, 12.0, 60.0),
            ("Menhir de lande", 38.0, 8.0, 0.0),
            ("Broussaille épineuse", -40.0, -8.0, 0.0),
            ("Broussaille épineuse 2", 30.0, -32.0, 90.0),
            ("Mare stagnante", -36.0, 24.0, 0.0),
            ("Ravine de terrain", -25.0, -36.0, 30.0),
            ("Poteau de bannière en ruine", 34.0, 30.0, 0.0),
            ("Cairn de guerre", -20.0, 38.0, 0.0),
            ("Touffe de brume basse", -32.0, -18.0, 0.0),
            ("Touffe de brume basse 2", 20.0, -40.0, 0.0),
        ];
        for (name, x, z, yaw) in LANDE {
            let file = match *name {
                "Rocher de lande" | "Rocher de lande 2" => "siege_moor_rock.glb",
                "Arbre mort tourmenté" => "siege_dead_tree.glb",
                "Ossements épars" => "siege_scattered_bones.glb",
                "Menhir de lande" => "siege_menhir.glb",
                "Broussaille épineuse" | "Broussaille épineuse 2" => "siege_thorny_scrub.glb",
                "Mare stagnante" => "siege_stagnant_pond.glb",
                "Ravine de terrain" => "siege_ravine.glb",
                "Poteau de bannière en ruine" => "siege_ruined_banner_post.glb",
                "Cairn de guerre" => "siege_war_cairn.glb",
                _ => "siege_low_mist.glb",
            };
            poser(
                &mut objects,
                &mut imported,
                name,
                file,
                *x,
                *z,
                1.0,
                *yaw,
                file != "siege_low_mist.glb",
            );
        }

        // --- Aînée de la lande (boss, GDD §4) : autel de mise en scène + cage
        // du chef, dans une clairière dégagée de la lande (loin du camp de
        // chasseurs/mare/prairies/verger). Tas de trophées près du camp de
        // chasseurs (mode Survie).
        poser(
            &mut objects,
            &mut imported,
            "Autel de l'Aînée",
            "siege_elder_altar.glb",
            42.4,
            42.4,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Cage du chef",
            "siege_chief_cage.glb",
            45.4,
            40.4,
            1.0,
            -30.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tas de trophées du camp",
            "siege_trophy_pile.glb",
            22.0,
            44.0,
            1.0,
            0.0,
            false,
        );

        // Les Braises (GDD §2.1) sont la fiction du jeu : « c'est [le feu
        // communal] qui attire les hordes ». La charte (§10.1 « au centre,
        // les braises ; au loin, le danger », §10.2 orange = système
        // feu/joueur) exige que ce soit le point chaud/saturé le plus
        // lisible de la carte — jusqu'ici posé comme n'importe quel décor
        // inerte (pas d'émissif), cf. docs/SPRINT3D_AUDIT_GAMEDESIGN.md §1.1.
        if let Some(feu) = objects.iter_mut().find(|o| o.name == "Feu communal") {
            feu.emissive = 1.2;
        }

        Scene {
            objects,
            imported,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 20.0, 0.0],
                    color: [0.9, 0.95, 1.0],
                    intensity: 1.4,
                    range: 100.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 4.0, 0.0],
                    color: [1.0, 0.75, 0.4],
                    intensity: 1.2,
                    range: 14.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Feu".into(), "Arme".into(), "Soin".into()],
                ..Default::default()
            },
            light: Light {
                dir: [0.55, 1.0, -0.45],
                color: [1.0, 0.96, 0.88],
                ambient: 0.35,
            },
            sky: Sky {
                horizon_color: [0.85, 0.78, 0.62],
                zenith_color: [0.30, 0.52, 0.78],
                fog_color: [0.78, 0.74, 0.62],
                fog_density: 0.012,
                ..Sky::default()
            },
            ..Default::default()
        }
    }
}
