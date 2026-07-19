use super::*;

mod helpers;
use helpers::*;

mod fort;

mod village;

mod water;

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

        village::add_village(&mut objects, &mut imported);

        water::add_water(&mut objects, &mut imported);

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
