use super::*;

mod helpers;
use helpers::*;

mod fort;

mod village;

mod water;

mod wilds;

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

        wilds::add_wilds(&mut objects, &mut imported);

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
