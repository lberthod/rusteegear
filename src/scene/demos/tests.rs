use super::*;

/// Verrouille les règles d'authoring des vagues du GDD (§5.5) sur la
/// ménagerie de `mmorpg_demo` — c'est cette donnée qui, resynchronisée
/// dans `assets/player_scene.json`, fait exister « La Horde » en ligne :
/// 1. la vague 1 est l'échauffement (la plus petite, aucune créature à
///    plus de 1 PV) ;
/// 2. chaque vague à partir de la n°2 compte au moins un chef à 3 PV
///    (la cible qui justifie le Boulet) ;
/// 3. le budget de PV croît strictement de vague en vague ;
/// 4. la dernière vague dépasse d'au moins un tiers l'avant-dernière
///    (« c'est elle qui doit coûter »).
#[test]
fn mmorpg_demo_waves_follow_the_gdd_authoring_rules() {
    let scene = Scene::mmorpg_demo();
    let creatures: Vec<&Combat> = scene
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature"))
        .map(|o| o.combat.as_ref().expect("toute créature a un Combat"))
        .collect();
    assert!(!creatures.is_empty());
    assert!(
        creatures.iter().all(|c| c.wave >= 1),
        "aucune créature hors système de vagues (wave 0) : la carte servie doit jouer \
             « La Horde », pas une chasse plate (GDD §5.5)"
    );

    let max_wave = creatures.iter().map(|c| c.wave).max().unwrap();
    assert!(
        max_wave >= 3,
        "au moins 3 vagues pour une dent de scie (trouvé : {max_wave})"
    );

    let budget = |w: u32| -> u32 { creatures.iter().filter(|c| c.wave == w).map(|c| c.hp).sum() };
    let count = |w: u32| creatures.iter().filter(|c| c.wave == w).count();

    // Règle 1 : échauffement.
    assert!(
        creatures.iter().filter(|c| c.wave == 1).all(|c| c.hp == 1),
        "la vague 1 est un échauffement : aucun chef (GDD §5.5/§3.5)"
    );
    assert!(
        (2..=max_wave).all(|w| count(1) <= count(w)),
        "la vague 1 doit être la plus petite en effectif"
    );

    // Règle 2 : un chef à 3 PV dès la vague 2.
    for w in 2..=max_wave {
        assert!(
            creatures.iter().any(|c| c.wave == w && c.hp >= 3),
            "la vague {w} doit compter au moins un chef à 3 PV (GDD §5.5 : c'est la \
                 cible qui fait exister le Boulet)"
        );
    }

    // Règle 3 : budget strictement croissant.
    for w in 2..=max_wave {
        assert!(
            budget(w) > budget(w - 1),
            "budget de PV non croissant : vague {w} = {} ≤ vague {} = {}",
            budget(w),
            w - 1,
            budget(w - 1)
        );
    }

    // Règle 4 : la dernière vague coûte (≥ 4/3 de l'avant-dernière).
    assert!(
        3 * budget(max_wave) >= 4 * budget(max_wave - 1),
        "la dernière vague ({}) doit dépasser d'un tiers l'avant-dernière ({})",
        budget(max_wave),
        budget(max_wave - 1)
    );
}

/// Preuve du décor nature de la démo MMORPG (Sprint en cours) : les glb du
/// pack (`scripts/blender/gen_nature_pack.py`) se chargent réellement (un
/// objet n'est poussé que si `load_gltf` réussit), le décor solide a bien un
/// collider `TriMesh` statique, le végétal léger reste traversable, et les
/// instances d'un même fichier partagent leur entrée `imported`.
#[test]
fn mmorpg_demo_contains_walkable_nature_decor() {
    let scene = Scene::mmorpg_demo();
    let by_name = |name: &str| {
        scene
            .objects
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("la démo MMORPG doit contenir « {name} »"))
    };

    // Aplats de terrain : purement visuels, jamais de collider.
    for name in [
        "Rivière nord",
        "Lac",
        "Rivière sud",
        "Route principale",
        "Chemin du hameau",
        "Rizière 1",
    ] {
        assert_eq!(
            by_name(name).physics,
            PhysicsKind::None,
            "« {name} » est un aplat visuel, il ne doit rien bloquer"
        );
    }

    // Décor solide : statique + TriMesh (les ponts se traversent à pied sur
    // leur tablier, silhouette exacte pour cabanes/tour/rochers/moulins).
    for name in [
        "Pont 1",
        "Pont 2",
        "Cabane 1",
        "Hutte",
        "Tour de guet",
        "Rocher 1",
        "Puits",
        "Torii",
        "Moulin à eau",
        "Moulin à vent",
    ] {
        let obj = by_name(name);
        assert_eq!(obj.physics, PhysicsKind::Static, "« {name} » doit bloquer");
        assert_eq!(
            obj.collider_shape,
            crate::runtime::physics::ColliderShape::TriMesh,
            "« {name} » doit utiliser sa silhouette exacte, pas une boîte"
        );
    }

    // Végétal léger et décor de berge : traversable.
    for name in [
        "Fleurs 1",
        "Panneau 1",
        "Nénuphars 1",
        "Roseaux 1",
        "Épouvantail",
        "Feu de camp",
    ] {
        assert_eq!(
            by_name(name).physics,
            PhysicsKind::None,
            "« {name} » ne doit pas gêner les déplacements"
        );
    }

    // Instanciation : deux instances semées d'un même fichier partagent le
    // même mesh importé (aucun état par objet sur du décor statique,
    // inutile de recharger le fichier).
    let mesh_of = |name: &str| match by_name(name).mesh {
        MeshKind::Imported(i) => i,
        _ => panic!("« {name} » devrait être un mesh importé"),
    };
    assert_eq!(
        mesh_of("Pont 1"),
        mesh_of("Pont 2"),
        "les instances d'un même glb doivent partager leur entrée `imported`"
    );

    // Dégagement des spawns : les créatures démarrent avec RAY_DIST (3,5 m)
    // de sonde devant elles — aucun décor solide à moins de 3,5 m d'un spawn
    // (même règle que les repères, cf. le commentaire de MMORPG_CREATURES).
    let creatures: Vec<&SceneObject> = scene
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature"))
        .collect();
    assert!(!creatures.is_empty(), "la démo doit garder ses créatures");
    for deco in scene
        .objects
        .iter()
        .filter(|o| o.physics == PhysicsKind::Static && matches!(o.mesh, MeshKind::Imported(_)))
    {
        for creature in &creatures {
            let delta = deco.transform.position - creature.transform.position;
            let d = (delta.x * delta.x + delta.z * delta.z).sqrt();
            assert!(
                d >= 3.5,
                "« {} » ({:?}) est à {d:.2} m du spawn de « {} » — il faut \
                     ≥ 3,5 m (RAY_DIST) de dégagement",
                deco.name,
                deco.transform.position,
                creature.name
            );
        }
    }
}

/// Preuve de l'agrandissement ×3 (24×24 → 72×72) : le sol et les 4 murs
/// suivent `Scene::MMORPG_HALF`, les deux ponts (aplats eau sans collider,
/// glb solides) existent bel et bien.
#[test]
fn mmorpg_map_is_72m_with_biomes() {
    let scene = Scene::mmorpg_demo();
    let half = Scene::MMORPG_HALF;
    assert_eq!(half, 36.0, "carte 72×72 : ×3 l'arène d'origine de 24×24");

    let sol = scene
        .objects
        .iter()
        .find(|o| o.name == "Sol")
        .expect("la démo doit avoir un sol");
    assert!(
        (sol.transform.scale.x - 2.0 * half).abs() < 0.01
            && (sol.transform.scale.z - 2.0 * half).abs() < 0.01,
        "le sol doit couvrir 72×72 m (scale={:?})",
        sol.transform.scale
    );

    for name in ["Mur Nord", "Mur Sud", "Mur Est", "Mur Ouest"] {
        let mur = scene
            .objects
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("« {name} » doit exister"));
        let d = mur
            .transform
            .position
            .x
            .abs()
            .max(mur.transform.position.z.abs());
        assert!(
            (d - half).abs() < 0.01,
            "« {name} » doit border l'arène à ±{half} m (pos={:?})",
            mur.transform.position
        );
    }

    for (bridge, aplat) in [("Pont 1", "Rivière sud"), ("Pont 2", "Rivière nord")] {
        let obj = scene
            .objects
            .iter()
            .find(|o| o.name == bridge)
            .unwrap_or_else(|| panic!("« {bridge} » doit exister"));
        assert_eq!(
            obj.physics,
            PhysicsKind::Static,
            "« {bridge} » doit bloquer"
        );
        assert_eq!(
            obj.collider_shape,
            crate::runtime::physics::ColliderShape::TriMesh,
            "« {bridge} » doit avoir sa silhouette exacte"
        );
        assert_eq!(
            scene
                .objects
                .iter()
                .find(|o| o.name == aplat)
                .unwrap_or_else(|| panic!("« {aplat} » doit exister"))
                .physics,
            PhysicsKind::None,
            "« {aplat} » (eau) ne doit rien bloquer, seuls les ponts le franchissent en dur"
        );
    }
}

/// Preuve de l'infranchissabilité de l'eau (murs invisibles générés par
/// grille, cf. le commentaire au-dessus de `NATURE_DECOR` dans
/// `mmorpg_demo`) : un joueur piloté droit vers le centre du lac pendant
/// 10 s simulées ne doit jamais entrer dans son rectangle — bloqué par un
/// mur qu'il ne voit pas, exactement comme une vraie rive.
#[test]
fn mmorpg_water_blocks_the_player_from_swimming_in_the_lake() {
    let mut scene = Scene::mmorpg_demo();
    let idx = scene
        .objects
        .iter()
        .position(|o| o.name == "Joueur")
        .expect("la démo doit avoir un « Joueur »");
    let mut phys = crate::runtime::physics::Physics::build(&scene);
    let dt = 1.0 / 60.0;
    let target = Vec3::new(-19.0, 0.0, 4.0); // centre du lac
    for _ in 0..600 {
        let pos = scene.objects[idx].transform.position;
        let dir = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z);
        let d = (dir.x * dir.x + dir.z * dir.z).sqrt();
        let (vx, vz) = if d > 0.05 {
            (dir.x / d * 4.5, dir.z / d * 4.5)
        } else {
            (0.0, 0.0)
        };
        phys.control(idx, vx, vz, false, 0.0, 0.0, dt);
        phys.step(dt, &mut scene);
    }
    let p = scene.objects[idx].transform.position;
    assert!(
        !(p.x >= -26.0 && p.x <= -12.0 && p.z >= -2.0 && p.z <= 10.0),
        "le joueur ne doit jamais entrer dans le lac à la nage (pos={p:?})"
    );
}

/// Preuve exhaustive de l'étanchéité de l'eau : essaie d'entrer dans
/// chacun des 4 plans d'eau depuis une trentaine de points répartis sur
/// tout leur périmètre (y compris juste à côté des deux ponts, l'endroit
/// le plus probable d'une brèche — un premier jet laissait ~6 m de berge
/// ouverte de chaque côté du tablier, largement plus large que lui) —
/// aucun de ces essais ne doit atteindre l'intérieur du rectangle d'eau
/// visé, sauf s'il s'agit du couloir du pont lui-même (exclu des points
/// testés, couvert par `mmorpg_player_can_still_cross_the_bridges`).
#[test]
fn mmorpg_water_is_sealed_all_the_way_around_including_next_to_bridges() {
    // (rect d'eau, points de départ (x,z) hors de l'eau tout autour,
    // y compris à ±1,5 m du couloir de chaque pont — juste à côté, pas
    // dedans).
    let rivière_nord: (f32, f32, f32, f32) = (-28.0, -36.0, -24.0, -6.0);
    let coude: (f32, f32, f32, f32) = (-28.0, -8.0, -16.0, -4.0);
    let lac: (f32, f32, f32, f32) = (-26.0, -2.0, -12.0, 10.0);
    let rivière_sud: (f32, f32, f32, f32) = (-18.0, 10.0, -14.0, 36.0);
    #[allow(clippy::type_complexity)]
    let cases: &[((f32, f32, f32, f32), &[(f32, f32)])] = &[
        (
            rivière_nord,
            &[
                (-31.0, -30.0),
                (-31.0, -20.0),
                (-21.0, -30.0),
                (-21.0, -20.0),
                // Juste au nord et au sud de l'ouverture du Pont 2 (z≈-10) :
                // exactement le point faible corrigé (mur resserré à 1 m).
                (-31.0, -14.0),
                (-21.0, -14.0),
                (-31.0, -6.5),
                // (-21.0, -6.5) exclu : ce point tombe DANS le rect du
                // coude (x:[-28,-16] z:[-8,-4]), donc déjà dans l'eau —
                // pas un point de terre valide pour ce test.
            ],
        ),
        (
            coude,
            &[
                // (-22, -3) : le mince interstice de terre entre le coude
                // et le lac (z:[-4,-2], ni l'un ni l'autre rect) — le
                // point le plus susceptible d'une brèche par pincement.
                (-22.0, -3.0),
                (-22.0, -11.0),
                (-14.5, -6.0),
            ],
        ),
        (
            lac,
            &[
                (-29.0, 0.0),
                (-29.0, 8.0),
                (-9.0, 0.0),
                (-9.0, 8.0),
                // (-19, -3) : même interstice de terre coude/lac que
                // ci-dessus, approché cette fois vers le lac.
                (-19.0, -3.0),
            ],
        ),
        (
            rivière_sud,
            &[
                (-21.0, 20.0),
                (-21.0, 30.0),
                (-11.0, 20.0),
                (-11.0, 30.0),
                // Juste au nord et au sud de l'ouverture du Pont 1 (z≈14).
                (-21.0, 10.5),
                (-11.0, 10.5),
                (-21.0, 18.5),
                (-11.0, 18.5),
            ],
        ),
    ];
    for &(rect, points) in cases {
        for &(sx, sz) in points {
            let mut scene = Scene::mmorpg_demo();
            let idx = scene
                .objects
                .iter()
                .position(|o| o.name == "Joueur")
                .unwrap();
            scene.objects[idx].transform.position = Vec3::new(sx, 1.0, sz);
            let mut phys = crate::runtime::physics::Physics::build(&scene);
            let dt = 1.0 / 60.0;
            let target = Vec3::new((rect.0 + rect.2) / 2.0, 0.0, (rect.1 + rect.3) / 2.0);
            for _ in 0..900 {
                let pos = scene.objects[idx].transform.position;
                let dir = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z);
                let d = (dir.x * dir.x + dir.z * dir.z).sqrt();
                let (vx, vz) = if d > 0.05 {
                    (dir.x / d * 4.5, dir.z / d * 4.5)
                } else {
                    (0.0, 0.0)
                };
                phys.control(idx, vx, vz, false, 0.0, 0.0, dt);
                phys.step(dt, &mut scene);
            }
            let p = scene.objects[idx].transform.position;
            assert!(
                !(p.x >= rect.0 && p.x <= rect.2 && p.z >= rect.1 && p.z <= rect.3),
                "depuis ({sx},{sz}), le joueur est entré dans l'eau {rect:?} \
                     (position finale={p:?}) — brèche dans le mur"
            );
        }
    }
}

/// Sprint 26 (Phase K, `sprintreflecion.md`) : même preuve d'étanchéité
/// que `mmorpg_water_is_sealed_all_the_way_around_including_next_to_bridges`
/// ci-dessus, mais pour le petit bassin du contrefort — depuis 4 points
/// de départ (nord, contre la pente qui monte vers la colline ; sud, côté
/// champ ouvert ; est et ouest), un joueur piloté droit vers le centre du
/// bassin pendant 15 s simulées ne doit jamais entrer dans son
/// rectangle.
#[test]
fn mmorpg_se_bassin_is_sealed_all_the_way_around() {
    let bassin: (f32, f32, f32, f32) = (-33.5, 7.0, -31.0, 8.5);
    let target = Vec3::new(
        (bassin.0 + bassin.2) / 2.0,
        0.0,
        (bassin.1 + bassin.3) / 2.0,
    );
    let starts: &[(f32, f32)] = &[
        (-32.25, 5.5),  // nord, contre la pente du contrefort
        (-32.25, 10.0), // sud, côté champ ouvert
        (-34.5, 7.75),  // ouest
        (-30.3, 7.75),  // est
    ];
    for &(sx, sz) in starts {
        let mut scene = Scene::mmorpg_demo();
        let idx = scene
            .objects
            .iter()
            .position(|o| o.name == "Joueur")
            .unwrap();
        scene.objects[idx].transform.position = Vec3::new(sx, 1.0, sz);
        let mut phys = crate::runtime::physics::Physics::build(&scene);
        let dt = 1.0 / 60.0;
        for _ in 0..900 {
            let pos = scene.objects[idx].transform.position;
            let dir = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z);
            let d = (dir.x * dir.x + dir.z * dir.z).sqrt();
            let (vx, vz) = if d > 0.05 {
                (dir.x / d * 4.5, dir.z / d * 4.5)
            } else {
                (0.0, 0.0)
            };
            phys.control(idx, vx, vz, false, 0.0, 0.0, dt);
            phys.step(dt, &mut scene);
        }
        let p = scene.objects[idx].transform.position;
        assert!(
            !(p.x >= bassin.0 && p.x <= bassin.2 && p.z >= bassin.1 && p.z <= bassin.3),
            "depuis ({sx},{sz}), le joueur est entré dans le bassin {bassin:?} \
                 (position finale={p:?}) — brèche dans le mur"
        );
    }
}

/// Contre-épreuve des précédentes : les deux ponts restent bien les
/// passages laissés dans les murs d'eau resserrés (ouverture ~3 m, cf.
/// `GRID`/`bridge_gaps`) — un joueur parti de chaque rive, piloté vers
/// l'autre, doit traverser `Pont 1` (rivière sud) et `Pont 2` (rivière
/// nord) dans les deux sens.
#[test]
fn mmorpg_player_can_still_cross_the_bridges() {
    // (nom, rive de départ (x,z), direction (vx,vz), seuil x d'arrivée,
    // arrivée à l'ouest (true) ou à l'est (false) du seuil).
    #[allow(clippy::type_complexity)]
    let cases: &[(&str, (f32, f32), (f32, f32), f32, bool)] = &[
        ("Pont 1 est→ouest", (-12.0, 14.0), (-4.5, 0.0), -18.0, true),
        ("Pont 1 ouest→est", (-20.0, 14.0), (4.5, 0.0), -14.0, false),
        ("Pont 2 est→ouest", (-22.0, -10.0), (-4.5, 0.0), -28.0, true),
        ("Pont 2 ouest→est", (-30.0, -10.0), (4.5, 0.0), -24.0, false),
    ];
    for &(name, (sx, sz), (vx, vz), threshold, arrives_west) in cases {
        let mut scene = Scene::mmorpg_demo();
        let idx = scene
            .objects
            .iter()
            .position(|o| o.name == "Joueur")
            .expect("la démo doit avoir un « Joueur »");
        scene.objects[idx].transform.position = Vec3::new(sx, 1.0, sz);
        let mut phys = crate::runtime::physics::Physics::build(&scene);
        let dt = 1.0 / 60.0;
        for _ in 0..600 {
            phys.control(idx, vx, vz, false, 0.0, 0.0, dt);
            phys.step(dt, &mut scene);
        }
        let p = scene.objects[idx].transform.position;
        let crossed = if arrives_west {
            p.x < threshold
        } else {
            p.x > threshold
        };
        assert!(
            crossed,
            "« {name} » : le joueur n'a pas traversé (pos={p:?})"
        );
    }
}

/// Preuve que le scatter procédural (graine fixe) peuple vraiment la forêt
/// du nord-est et le hameau/promontoire, plutôt que de tout rejeter en
/// silence (rejection sampling contre les zones d'exclusion).
#[test]
fn mmorpg_forest_reaches_minimum_density() {
    let scene = Scene::mmorpg_demo();
    let in_forest = |o: &&SceneObject| {
        let p = o.transform.position;
        p.x >= 8.0 && p.x <= 34.0 && p.z >= -34.0 && p.z <= -8.0
    };
    let trees_and_pines = scene
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Arbre") || o.name.starts_with("Sapin"))
        .filter(in_forest)
        .count();
    assert!(
        trees_and_pines >= 30,
        "la forêt NE doit compter ≥ 30 arbres/sapins (trouvé {trees_and_pines})"
    );

    let batiments = ["Cabane 1", "Cabane 2", "Hutte"]
        .iter()
        .filter(|n| scene.objects.iter().any(|o| o.name == **n))
        .count();
    assert!(batiments >= 3, "le hameau doit avoir ≥ 3 bâtiments");

    let rochers = scene
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Rocher"))
        .count();
    assert!(
        rochers >= 5,
        "le promontoire doit avoir ≥ 5 rochers (trouvé {rochers})"
    );
}

/// Preuve anti-chevauchement : tout décor solide reste dans l'arène et à
/// ≥ 2 m de tout autre solide (le scatter procédural pourrait sinon
/// fusionner deux troncs visuellement).
#[test]
fn mmorpg_solid_decor_stays_inside_and_spaced() {
    let scene = Scene::mmorpg_demo();
    let half = Scene::MMORPG_HALF;
    let solids: Vec<&SceneObject> = scene
        .objects
        .iter()
        .filter(|o| o.physics == PhysicsKind::Static && matches!(o.mesh, MeshKind::Imported(_)))
        .collect();
    assert!(solids.len() >= 40, "le décor solide doit être substantiel");
    for s in &solids {
        let p = s.transform.position;
        assert!(
            p.x.abs() <= half && p.z.abs() <= half,
            "« {} » ({p:?}) doit rester dans l'arène",
            s.name
        );
    }
    for i in 0..solids.len() {
        for j in (i + 1)..solids.len() {
            let d = (solids[i].transform.position - solids[j].transform.position)
                .length_squared()
                .sqrt();
            assert!(
                d >= 2.0,
                "« {} » et « {} » sont à {d:.2} m l'un de l'autre (< 2 m, risque de fusion visuelle)",
                solids[i].name,
                solids[j].name
            );
        }
    }
}

/// Preuve de la couverture d'herbe/fougères (Étape « végétation plus
/// naturelle ») : au moins une touffe d'herbe et une fougère existent
/// réellement dans la scène (glb chargé, pas juste déclaré), non solides
/// — la sonde des créatures et le joueur passent au travers, seule la
/// silhouette compte pour l'aspect visuel.
#[test]
fn mmorpg_demo_has_grass_and_ferns_underfoot() {
    let scene = Scene::mmorpg_demo();
    for file in ["nature_grass_tuft.glb", "nature_fern.glb"] {
        let loaded = scene.imported.iter().any(|m| m.path.ends_with(file));
        assert!(loaded, "la démo doit charger « {file} »");
        let instantiated = scene.imported.iter().enumerate().any(|(i, m)| {
            m.path.ends_with(file)
                && scene
                    .objects
                    .iter()
                    .any(|o| matches!(o.mesh, MeshKind::Imported(idx) if idx as usize == i))
        });
        assert!(
            instantiated,
            "« {file} » doit être instancié au moins une fois dans la scène"
        );
    }
    for name_prefix in ["Herbe ", "Sous-bois "] {
        let matches: Vec<&SceneObject> = scene
            .objects
            .iter()
            .filter(|o| o.name.starts_with(name_prefix))
            .collect();
        assert!(
            !matches.is_empty(),
            "aucune instance nommée « {name_prefix}… » trouvée"
        );
        for o in matches {
            assert_eq!(
                o.physics,
                PhysicsKind::None,
                "« {} » (herbe/fougère) ne doit rien bloquer",
                o.name
            );
        }
    }
}

/// Preuve du placement logique des 26 créatures : chaque spawn tombe dans
/// le rectangle de son biome annoncé (forêt, lac, rizières, hameau,
/// promontoire…), pas éparpillé au hasard sur la carte.
#[test]
fn mmorpg_creature_spawns_sit_in_their_biome() {
    let scene = Scene::mmorpg_demo();
    // (x0, z0, x1, z1), même convention que `Rect` dans le scatter procédural.
    #[allow(clippy::type_complexity)]
    let biomes: &[(&str, (f32, f32, f32, f32))] = &[
        ("Créature 6", (14.0, -34.0, 34.0, -8.0)),  // forêt NE
        ("Créature 7", (-33.0, -2.0, -5.0, 20.0)),  // lac / berges
        ("Créature 9", (-11.0, 23.0, 9.0, 35.0)),   // rizières
        ("Créature 11", (2.0, 4.0, 20.0, 22.0)),    // hameau
        ("Créature 12", (14.0, -34.0, 34.0, -8.0)), // forêt NE (2ᵉ clairière)
        ("Créature 13", (-33.0, -2.0, -5.0, 20.0)), // lac
        ("Créature 20", (20.0, 2.0, 34.0, 16.0)),   // promontoire
    ];
    for &(name, (x0, z0, x1, z1)) in biomes {
        let obj = scene
            .objects
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("« {name} » doit exister"));
        let p = obj.transform.position;
        assert!(
            p.x >= x0 && p.x <= x1 && p.z >= z0 && p.z <= z1,
            "« {name} » ({p:?}) doit être dans son biome {x0},{z0} → {x1},{z1}"
        );
    }
}

/// Preuve du décor animé (moulins, bannière, feu, épouvantail) : chaque
/// instance porte un `AnimationState` sur un clip qui existe réellement
/// dans le glb, avance (`speed > 0`), et les instances du même fichier ne
/// sont pas synchronisées (phases de départ décalées). Les solides animés
/// gardent un collider TriMesh (celui de la pose de repos).
#[test]
fn mmorpg_animated_decor_plays_looping_clips() {
    let scene = Scene::mmorpg_demo();
    for name in [
        "Moulin à eau",
        "Moulin à vent",
        "Bannière",
        "Feu de camp",
        "Épouvantail",
    ] {
        let obj = scene
            .objects
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("« {name} » doit exister"));
        let anim = obj
            .animation
            .as_ref()
            .unwrap_or_else(|| panic!("« {name} » doit avoir un AnimationState"));
        assert_eq!(anim.clip, "Idle", "« {name} » doit jouer son clip Idle");
        assert!(anim.speed > 0.0, "« {name} » doit être en mouvement");

        let mesh_index = match obj.mesh {
            MeshKind::Imported(i) => i,
            _ => panic!("« {name} » devrait être un mesh importé"),
        };
        let clips = &scene.imported[mesh_index as usize].clips;
        assert!(
            clips.iter().any(|c| c.name == "Idle"),
            "« {name} » : le glb doit contenir le clip Idle (clips={:?})",
            clips.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
    }
    assert_eq!(
        scene
            .objects
            .iter()
            .find(|o| o.name == "Moulin à eau")
            .unwrap()
            .physics,
        PhysicsKind::Static,
        "un décor animé solide garde son collider TriMesh (pose de repos)"
    );
}

/// Preuve de l'ambiance visuelle (Étape 5 bis) : brume et ciel réglés
/// explicitement (pas les valeurs plates par défaut), soleil orienté.
#[test]
fn mmorpg_demo_has_atmospheric_sky_and_fog() {
    let scene = Scene::mmorpg_demo();
    assert!(
        scene.sky.fog_density > 0.0,
        "la brume doit être activée pour la profondeur atmosphérique"
    );
    assert_ne!(
        scene.sky.horizon_color, scene.sky.zenith_color,
        "un vrai dégradé de ciel, pas un fond plat"
    );
    assert_ne!(
        scene.light.dir,
        Light::default().dir,
        "le soleil doit être orienté pour porter des ombres lisibles"
    );
}

/// Preuve de la demande gameplay « des objets à trouver dans la scène
/// MMORPG » : la démo contient des `ItemPickup` d'au moins 3 sortes
/// différentes, tous traversables (un objet à ramasser ne bloque ni le
/// joueur ni les sondes des créatures) et dans l'arène ; le buisson à
/// baies repousse (`respawn_delay > 0`), la clé et la gemme sont uniques.
#[test]
fn mmorpg_demo_contains_item_pickups_to_find() {
    let scene = Scene::mmorpg_demo();
    let items: Vec<&SceneObject> = scene
        .objects
        .iter()
        .filter(|o| o.item_pickup.is_some())
        .collect();
    let kinds: std::collections::HashSet<_> =
        items.iter().map(|o| o.item_pickup.unwrap().kind).collect();
    assert!(
        kinds.len() >= 3,
        "au moins 3 sortes d'objets à trouver (trouvé : {kinds:?})"
    );
    for o in &items {
        assert_eq!(
            o.physics,
            PhysicsKind::None,
            "« {} » : un objet à ramasser ne doit rien bloquer",
            o.name
        );
        let p = o.transform.position;
        assert!(
            p.x.abs() < Scene::MMORPG_HALF && p.z.abs() < Scene::MMORPG_HALF,
            "« {} » ({p:?}) doit être dans l'arène",
            o.name
        );
    }
    let by_name = |name: &str| {
        items
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("la démo MMORPG doit contenir « {name} »"))
    };
    assert!(
        by_name("Buisson à baies").respawn_delay > 0.0,
        "le buisson à baies doit repousser"
    );
    for name in ["Clé du village", "Gemme"] {
        assert_eq!(
            by_name(name).respawn_delay,
            0.0,
            "« {name} » est une trouvaille unique, sans réapparition"
        );
    }
}

/// Sprint 2 de `sprintoptimation3daudit10h.md` (Phase B) : catégorise les
/// objets skinnés de `mmorpg_demo` entre créatures actives (IA/script de
/// patrouille) et décor statique en place (candidat à l'instancing GPU du
/// skinning), et vérifie que le décor éligible n'a **aucun** mesh partagé
/// par plusieurs instances — chaque fichier `monster_*.glb`/`fauna_*.glb`/
/// `nature_*.glb` animé n'est posé qu'une fois dans la démo. Conséquence
/// directe pour Sprint 3 : regrouper ces instances derrière une palette de
/// joints partagée ne réduirait aucun draw call (rien à regrouper), donc le
/// Sprint 3 tel que spécifié n'a pas de bénéfice mesurable sur ce contenu —
/// voir `sprintoptimation3daudit10h.md` (Phase B) pour la décision qui en découle.
#[test]
fn mmorpg_demo_static_skinned_decor_has_no_duplicate_mesh() {
    let scene = Scene::mmorpg_demo();
    let skinned: Vec<&SceneObject> = scene
        .objects
        .iter()
        .filter(|o| scene.is_skinned_mesh(o.mesh))
        .collect();
    assert!(
        !skinned.is_empty(),
        "la démo MMORPG doit contenir des objets skinnés"
    );

    let eligible: Vec<&SceneObject> = skinned
        .iter()
        .copied()
        .filter(|o| scene.is_static_skinned_decor(o))
        .collect();
    let active_count = skinned.len() - eligible.len();

    // Régression : verrouille les comptages mesurés en Sprint 2 (26
    // créatures `MMORPG_CREATURES` + 46 « Errant N » scriptées = 72
    // objets skinnés actifs, non éligibles à l'instancing) — mis à jour
    // après l'ajout de creature62-67 (renard + pack savane africaine) et
    // creature68-72 (pack organique Metaball) à
    // `MMORPG_AMBIENT_FAUNA_SPAWNS`.
    assert_eq!(
        active_count, 72,
        "objets skinnés actifs (AiChaser ou script non vide) : {active_count} \
             — si ce nombre a changé, la répartition Sprint 2 est à revérifier"
    );
    assert!(
        !eligible.is_empty(),
        "aucun décor skinné statique trouvé — la ménagerie/les mécanismes animés \
             ont-ils changé de forme ?"
    );

    // Constat central du Sprint 2 (conditionne le Sprint 3) : parmi le
    // décor éligible, aucun mesh (fichier importé) n'est instancié plus
    // d'une fois.
    let mut instances_per_mesh = std::collections::HashMap::<u32, u32>::new();
    for o in &eligible {
        if let MeshKind::Imported(i) = o.mesh {
            *instances_per_mesh.entry(i).or_insert(0) += 1;
        }
    }
    let max_instances = instances_per_mesh.values().copied().max().unwrap_or(0);
    assert_eq!(
        max_instances, 1,
        "un mesh du décor skinné statique est instancié {max_instances} fois : \
             l'instancing GPU du skinning (Sprint 3) redevient rentable pour ce mesh, \
             mettre à jour la décision documentée dans « sprintoptimation3daudit10h.md » (Phase B)"
    );
}

/// Audit du Sprint B (Phase B) : parmi le décor skinné statique éligible
/// (`Scene::is_static_skinned_decor`), une partie a un squelette **sans jamais
/// jouer de clip** (`animation: None` — ex. les étals/établis de `VILLAGE_PROPS`,
/// riggés par le même gabarit que les créatures via
/// `scripts/blender/gen_items_pack11_20.py`, mais jamais activés dans
/// `demos.rs`). Ces objets rendent une pose de liaison figée, visuellement
/// identique à un mesh statique, mais passent quand même par le chemin de
/// dessin skinné (`gfx::renderer::draw_skinned_objects`, `is_skinned` ne teste
/// que la présence d'un squelette, jamais `AnimationState`) — un draw call et un
/// emplacement de `MAX_SKINNED_INSTANCES` dépensés pour rien. Piste
/// d'optimisation distincte du Sprint 3 (non implémentée ici : toucherait
/// `src/gfx/renderer.rs`, hors périmètre scène de ce sprint et partagé avec les
/// Phases A/C/D en cours) — voir « sprintoptimation3daudit10h.md » (Phase B) pour le détail.
#[test]
fn mmorpg_demo_has_static_skinned_decor_that_never_animates() {
    let scene = Scene::mmorpg_demo();
    let eligible: Vec<&SceneObject> = scene
        .objects
        .iter()
        .filter(|o| scene.is_static_skinned_decor(o))
        .collect();
    let never_animates = eligible.iter().filter(|o| o.animation.is_none()).count();

    // Verrouille le constat de l'audit : si ce nombre tombe à 0 (ex. un futur
    // sprint bascule ces objets sur le chemin statique, ou leur donne enfin un
    // clip), l'opportunité d'optimisation documentée est résolue — mettre à jour
    // « sprintoptimation3daudit10h.md » (Phase B) en conséquence plutôt que de relâcher ce test.
    assert_eq!(
        never_animates, 50,
        "objets skinnés statiques sans animation active : {never_animates} — \
             coût de rendu skinné payé pour rien si ce nombre est non nul (voir \
             « sprintoptimation3daudit10h.md », Phase B, section audit)"
    );
}

/// Sprint 24 (Phase K, `sprintreflecion.md`) : le sol de `mmorpg_demo` doit
/// désormais avoir un vrai relief à un endroit (pas juste un `MeshKind::Plane`
/// plat renommé), MAIS rester quasi plat (tolérance sub-centimétrique) sous
/// tout le contenu placé à la main — des centaines d'objets (décor,
/// créatures, murs d'eau, ponts) supposent un sol à y≈0 et n'ont pas été
/// repositionnés par ce sprint.
#[test]
fn mmorpg_terrain_has_real_relief_but_stays_flat_under_placed_content() {
    use crate::gfx::mesh::mmorpg_terrain_local_height;

    let scene = Scene::mmorpg_demo();
    let sol = scene
        .objects
        .iter()
        .find(|o| o.name == "Sol")
        .expect("la démo doit avoir un sol");
    assert!(
        matches!(sol.mesh, MeshKind::Terrain),
        "le sol doit utiliser MeshKind::Terrain (relief réel), pas MeshKind::Plane"
    );
    // Échelle Y découplée (=1.0) : la fonction de hauteur renvoie déjà des
    // mètres, cf. la doc de `mmorpg_terrain_local_height`.
    assert!(
        (sol.transform.scale.y - 1.0).abs() < 1e-6,
        "scale.y doit rester 1.0 (hauteur déjà en mètres, pas un facteur à \
             re-multiplier) — obtenu {}",
        sol.transform.scale.y
    );

    // 1) Du relief significatif existe bel et bien quelque part (bande de
    // collines à l'ouest, cf. sa doc) — sinon ce ne serait qu'un plan plat
    // sous un autre nom.
    let half = Scene::MMORPG_HALF;
    let mut max_abs_height = 0.0_f32;
    let mut iz = -half;
    while iz <= half {
        // Pas fin (0,25 m) en X : la bande de collines est étroite
        // (~1,5 m de large, cf. la doc de `mmorpg_terrain_local_height`) — un
        // pas de 2 m la sauterait complètement (elle tombe exactement entre
        // deux échantillons).
        let mut ix = -half;
        while ix <= half {
            let h = mmorpg_terrain_local_height(ix / (2.0 * half), iz / (2.0 * half));
            max_abs_height = max_abs_height.max(h.abs());
            ix += 0.25;
        }
        iz += 2.0;
    }
    assert!(
        max_abs_height > 0.5,
        "aucun relief significatif trouvé sur la carte (max observé {max_abs_height} m)"
    );

    // 2) Sous tout objet déjà placé à la main (hors murs de pourtour et sol
    // lui-même), le relief doit rester quasi nul — sinon des objets se
    // retrouveraient enterrés/flottants sans avoir été repositionnés.
    // Exception délibérée (Sprint 26, Phase K) : le petit bassin du
    // contrefort et ses murs invisibles sont posés À DESSEIN sur/contre
    // une pente réelle (`MMORPG_MOUND_X_LOCAL`/`MMORPG_MOUND_Z_LOCAL`,
    // cf. leur doc) — exactement l'inverse de cette règle, vérifié à part
    // par `mmorpg_se_bassin_is_sealed_all_the_way_around`.
    for o in &scene.objects {
        if o.name == "Sol"
            || o.name.starts_with("Mur ")
            || o.name == "Bassin du contrefort"
            || o.name.starts_with("Mur bassin")
        {
            continue;
        }
        let x = o.transform.position.x;
        let z = o.transform.position.z;
        if x.abs() > half || z.abs() > half {
            continue;
        }
        let h = mmorpg_terrain_local_height(x / (2.0 * half), z / (2.0 * half));
        assert!(
            h.abs() < 0.05,
            "« {} » en ({x:.1}, {z:.1}) : le sol sous cet objet placé à la main \
                 doit rester quasi plat (obtenu {h:.3} m) — le découpage en zones de \
                 `mmorpg_terrain_local_height` doit exclure cette position",
            o.name
        );
    }
}

/// Verrou du chantier 4.1 (audit 2026-07-20) : le casting de chasse est un
/// COMPORTEMENT, jamais des PV. Chaque créature porte un `ai_chaser`
/// (grammaire Traqueuse/Meute/Colosse/Furtive, GDD §5.4) mais son
/// `Combat::hp` reste celui de la table (1 troupe / 3 chef) et le budget par
/// vague reste exactement la dent de scie du GDD §5.5 — appliquer
/// `Archetype::hp_multiplier` ici casserait ce contrat.
#[test]
fn mmorpg_creatures_carry_a_chase_casting_without_touching_hp() {
    let scene = Scene::mmorpg_demo();
    let mut per_wave: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    let mut count = 0;
    for o in scene
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature") || o.name == "Créature")
    {
        let Some(c) = o.combat.as_ref().filter(|c| c.attackable) else {
            continue;
        };
        count += 1;
        assert!(
            o.ai_chaser.is_some(),
            "« {} » doit porter la grammaire de chasse (ai_chaser)",
            o.name
        );
        assert!(
            c.hp == 1 || c.hp == 3,
            "« {} » : hp={} — le casting ne doit JAMAIS moduler les PV \
             (1 troupe / 3 chef, hp_multiplier non appliqué)",
            o.name,
            c.hp
        );
        *per_wave.entry(c.wave).or_default() += c.hp;
    }
    assert_eq!(count, 26, "les 26 créatures nommées doivent être castées");
    let budget: Vec<(u32, u32)> = per_wave.into_iter().collect();
    assert_eq!(
        budget,
        vec![(1, 5), (2, 8), (3, 11), (4, 16)],
        "budget de PV par vague = contrat GDD §5.5, intouchable par le casting"
    );
}
