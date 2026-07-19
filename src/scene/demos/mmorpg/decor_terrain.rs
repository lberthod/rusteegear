use super::*;

pub(super) fn add_terrain_layers(objects: &mut Vec<SceneObject>, half: f32) {
    // --- Décor « nature » : la carte 72×72 devient un petit monde ------------
    // Biomes (nord = -Z) : prairie centrale autour du spawn joueur, forêt
    // dense au nord-est (deux clairières habitées), lac et rivières à
    // l'ouest (deux ponts), rizières en damier au sud-ouest, hameau sur la
    // route est-ouest et promontoire rocheux à l'est (tour de guet). La
    // carte reste PLATE : le sol est un `Plane` et les scripts des
    // créatures ne suivent aucun relief — le « promontoire » est un anneau
    // de rochers au sol, pas une élévation. Trois couches :
    //
    // 1) Aplats de terrain : primitives `Plane`, décalées de quelques
    //    centimètres en Y (eau < sable < chemins < route) pour éviter le
    //    z-fighting avec le sol et entre elles. L'eau reste un `Plane`
    //    (pas de heightmap dans ce moteur), mais désormais bordée de murs
    //    invisibles (étape 2 ci-dessous) : rivières et lac ne se
    //    traversent plus à gué, seuls les deux ponts passent.
    let eau = [0.18, 0.42, 0.65];
    let eau_sombre = [0.14, 0.34, 0.55];
    let terre = [0.42, 0.36, 0.26];
    let vert_riziere = [0.24, 0.44, 0.38];
    // Surface plus lisse/brillante que le sol : sans reflet d'environnement
    // réel (le moteur n'a ni alpha ni uniforme de temps, cf. la doc de
    // `mmorpg_demo`), un `roughness` bas donne un reflet spéculaire net qui
    // suffit à distinguer l'eau d'un simple aplat de peinture bleue.
    let mut aplat_eau = |name: &str, pos: Vec3, scale: Vec3, color: [f32; 3]| {
        let mut p = demo_obj(name, MeshKind::Plane, pos);
        p.transform = p.transform.with_scale(scale);
        p.color = color;
        p.roughness = 0.08;
        p.metallic = 0.15;
        objects.push(p);
    };
    aplat_eau(
        "Rivière nord",
        Vec3::new(-26.0, 0.02, -21.0),
        Vec3::new(4.0, 1.0, 30.0),
        eau,
    );
    aplat_eau(
        "Coude de rivière",
        Vec3::new(-22.0, 0.02, -6.0),
        Vec3::new(12.0, 1.0, 4.0),
        eau,
    );
    aplat_eau(
        "Lac",
        Vec3::new(-19.0, 0.015, 4.0),
        Vec3::new(14.0, 1.0, 12.0),
        eau_sombre,
    );
    aplat_eau(
        "Rivière sud",
        Vec3::new(-16.0, 0.02, 23.0),
        Vec3::new(4.0, 1.0, 26.0),
        eau,
    );
    let mut aplat = |name: &str, pos: Vec3, scale: Vec3, color: [f32; 3]| {
        let mut p = demo_obj(name, MeshKind::Plane, pos);
        p.transform = p.transform.with_scale(scale);
        p.color = color;
        objects.push(p);
    };
    aplat(
        "Berge du lac",
        Vec3::new(-19.0, 0.012, 12.0),
        Vec3::new(14.0, 1.0, 3.0),
        [0.72, 0.64, 0.44],
    );
    aplat(
        "Route principale",
        Vec3::new(0.0, 0.03, 14.0),
        Vec3::new(2.0 * half, 1.0, 2.2),
        terre,
    );
    aplat(
        "Chemin du hameau",
        Vec3::new(10.0, 0.028, 2.0),
        Vec3::new(2.0, 1.0, 24.0),
        terre,
    );
    aplat(
        "Chemin du pont nord",
        Vec3::new(-10.0, 0.028, -10.0),
        Vec3::new(32.0, 1.0, 1.6),
        terre,
    );
    for (i, (rx, rz)) in [
        (-8.0_f32, 26.0),
        (-1.0, 26.0),
        (-8.0, 32.0),
        (-1.0, 32.0),
        (6.0, 29.0),
    ]
    .into_iter()
    .enumerate()
    {
        aplat(
            &format!("Rizière {}", i + 1),
            Vec3::new(rx, 0.015, rz),
            Vec3::new(6.0, 1.0, 5.0),
            vert_riziere,
        );
    }
    // Trois liaisons de voirie qui recousent les biomes entre eux (les
    // biomes existaient mais on n'y « allait » que hors piste) : un chemin
    // route → rizières dans l'interstice entre les damiers 1 et 2, le
    // prolongement du chemin du pont nord vers le promontoire (le chemin
    // s'arrêtait à x=6, la tour de guet restait hors réseau), et une place
    // de terre battue autour du puits du hameau. Y : la place (0.031)
    // au-dessus du chemin du hameau (0.028) qu'elle chevauche ; les deux
    // chemins sous la route (0.03), même logique anti z-fighting que les
    // aplats existants.
    aplat(
        "Chemin des rizières",
        Vec3::new(-4.5, 0.026, 20.0),
        Vec3::new(1.8, 1.0, 12.0),
        terre,
    );
    aplat(
        "Sentier du promontoire",
        Vec3::new(20.0, 0.027, -10.0),
        Vec3::new(28.0, 1.0, 1.6),
        terre,
    );
    aplat(
        "Place du hameau",
        Vec3::new(10.0, 0.031, 7.0),
        Vec3::new(7.0, 1.0, 6.0),
        terre,
    );

    // 2) Murs d'eau invisibles : bordent les 4 plans d'eau ci-dessus pour
    //    les rendre réellement infranchissables (collider `Static` seul,
    //    `visible = false` — l'eau garde son aspect, elle bloque juste le
    //    passage comme une vraie rivière). Seules ouvertures : les deux
    //    ponts existants (`Pont 1` sur la rivière sud, `Pont 2` sur la
    //    rivière nord), qui redeviennent ainsi les seuls passages — le
    //    commentaire historique « les ponts sont narratifs » ne l'est
    //    plus.
    //
    //    Les 4 rects d'eau (rivière nord, coude, lac, rivière sud) se
    //    chevauchent/se touchent de façon irrégulière (le coude et le lac
    //    ont même un interstice de 2 m entre eux) : les murer à la main
    //    côté par côté s'est révélé source d'erreurs de continuité (essayé
    //    puis abandonné — un bord mal raccordé laisse une brèche). Plus
    //    fiable : rasteriser l'UNION des 4 rects sur une grille et poser
    //    un segment de mur à chaque frontière eau→terre — la topologie du
    //    contour est alors dérivée automatiquement, pas raisonnée à la
    //    main. `GRID` = 3 m (assez fin pour suivre les angles à ~1,5 m
    //    près, largement sous la marge de 3,5 m des sondes créatures).
    //    `GRID` = 1 m (pas 3 m comme au premier jet) : au grain plus
    //    large, l'ouverture minimale au droit d'un pont devait couvrir 2
    //    cellules pour ne pas rogner le tablier (~1,84 m de large), ce qui
    //    ouvrait ~6 m de berge — largement plus que le pont, le joueur
    //    pouvait entrer dans l'eau en longeant la rive à côté du tablier
    //    sans jamais poser le pied dessus. À 1 m, l'ouverture se resserre
    //    à ~3 m (tablier + ~0,6 m de marge de chaque côté), et le mur
    //    repousse immédiatement quiconque s'écarte du pont.
    {
        const GRID: f32 = 1.0;
        let water_rects: [(f32, f32, f32, f32); 4] = [
            (-28.0, -36.0, -24.0, -6.0), // rivière nord
            (-28.0, -8.0, -16.0, -4.0),  // coude
            (-26.0, -2.0, -12.0, 10.0),  // lac
            (-18.0, 10.0, -14.0, 36.0),  // rivière sud
        ];
        // Rectangles où aucun mur ne doit être posé : juste assez larges
        // pour couvrir le tablier du pont (largeur réelle ~1,84 m,
        // `gen_bridge()` × échelle 1.15) sans plus — un gap trop large
        // laisserait le joueur entrer dans l'eau à côté du pont sans
        // jamais l'emprunter.
        let bridge_gaps: [(f32, f32, f32, f32); 2] = [
            (-29.0, -11.5, -23.0, -8.5), // Pont 2 (rivière nord, z≈-10)
            (-19.0, 12.5, -13.0, 15.5),  // Pont 1 (rivière sud, z≈14)
        ];
        let is_water = |x: f32, z: f32| {
            water_rects
                .iter()
                .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
        };
        let in_gap = |x: f32, z: f32| {
            bridge_gaps
                .iter()
                .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
        };
        // Bornes de balayage : englobent les 4 rects avec une marge d'une
        // cellule pour détecter la frontière extérieure.
        let (mut gx0, mut gz0, mut gx1, mut gz1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x0, z0, x1, z1) in &water_rects {
            gx0 = gx0.min(x0);
            gz0 = gz0.min(z0);
            gx1 = gx1.max(x1);
            gz1 = gz1.max(z1);
        }
        let mut mur_n = 0u32;
        let mut cx = gx0 - GRID;
        while cx <= gx1 + GRID {
            let mut cz = gz0 - GRID;
            while cz <= gz1 + GRID {
                if is_water(cx, cz) {
                    // Frontière est/ouest (mur vertical le long de X constant).
                    for &(nx, wall_x) in
                        &[(cx + GRID, cx + GRID / 2.0), (cx - GRID, cx - GRID / 2.0)]
                    {
                        if !is_water(nx, cz) && !in_gap(wall_x, cz) {
                            mur_n += 1;
                            let mut w = demo_obj(
                                &format!("Mur d'eau {mur_n}"),
                                MeshKind::Cube,
                                Vec3::new(wall_x, 0.9, cz),
                            );
                            w.transform = w.transform.with_scale(Vec3::new(0.4, 1.8, GRID));
                            w.physics = PhysicsKind::Static;
                            w.visible = false;
                            objects.push(w);
                        }
                    }
                    // Frontière nord/sud (mur horizontal le long de Z constant).
                    for &(nz, wall_z) in
                        &[(cz + GRID, cz + GRID / 2.0), (cz - GRID, cz - GRID / 2.0)]
                    {
                        if !is_water(cx, nz) && !in_gap(cx, wall_z) {
                            mur_n += 1;
                            let mut w = demo_obj(
                                &format!("Mur d'eau {mur_n}"),
                                MeshKind::Cube,
                                Vec3::new(cx, 0.9, wall_z),
                            );
                            w.transform = w.transform.with_scale(Vec3::new(GRID, 1.8, 0.4));
                            w.physics = PhysicsKind::Static;
                            w.visible = false;
                            objects.push(w);
                        }
                    }
                }
                cz += GRID;
            }
            cx += GRID;
        }
    }

    // 1 bis) Sprint 26 (Phase K, `sprintreflecion.md`) : petit bassin
    //    intégré à un contrefort du relief existant (`gfx::mesh::
    //    mmorpg_terrain_local_height`, zone `MMORPG_MOUND_X_LOCAL`/
    //    `MMORPG_MOUND_Z_LOCAL`) — PAS un retrofit du lac historique
    //    ci-dessus (`water_rects`), qui reste posé sur un sol resté plat
    //    à dessein (des centaines de placements en dépendent, cf. la doc
    //    de `mmorpg_terrain_local_height`). Zone (x∈[-34,-30],
    //    z∈[3,5;8,5]) vérifiée numériquement libre de tout décor/spawn
    //    placé à la main à ≥3 m près (`NATURE_DECOR`/`VILLAGE_PROPS`/
    //    `MONSTER_DECOR`/`MMORPG_HALTES`/`MMORPG_CREATURES`/
    //    `MMORPG_AMBIENT_FAUNA_SPAWNS`/`EXCL_*`), y compris du semis
    //    procédural le plus proche (« Arbre du sud », x∈[22,34] z∈[30,35],
    //    seule autre poche libre trouvée côté sud-est de la carte — trop
    //    loin pour interférer ici) : juste à l'est de la bande de
    //    collines existante, dont le relief retombe déjà à 0 dès
    //    x=-34,5. Rive nord du bassin (z≈7, contre le contrefort) suit
    //    donc une pente réelle du terrain ; rive sud (z≈8,5, côté champ
    //    ouvert) reste sur du plat, comme n'importe quelle berge. Murs
    //    invisibles sur les 4 côtés — même principe que les « Mur d'eau »
    //    ci-dessus, mais un seul rectangle isolé (pas de pont à
    //    ménager) : pas besoin de l'algorithme de rastérisation par
    //    union, 4 plaques statiques suffisent.
    {
        const SE_LAKE: (f32, f32, f32, f32) = (-33.5, 7.0, -31.0, 8.5); // (x0,z0,x1,z1)
        let (x0, z0, x1, z1) = SE_LAKE;
        let (cx, cz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
        let mut p = demo_obj(
            "Bassin du contrefort",
            MeshKind::Plane,
            Vec3::new(cx, 0.02, cz),
        );
        p.transform = p.transform.with_scale(Vec3::new(x1 - x0, 1.0, z1 - z0));
        p.color = eau_sombre;
        p.roughness = 0.08;
        p.metallic = 0.15;
        objects.push(p);

        let wall_h = 1.8;
        let thick = 0.4;
        let mut bassin_wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.visible = false;
            objects.push(w);
        };
        bassin_wall(
            "Mur bassin Nord",
            Vec3::new(cx, wall_h * 0.5, z0),
            Vec3::new(x1 - x0 + thick, wall_h, thick),
        );
        bassin_wall(
            "Mur bassin Sud",
            Vec3::new(cx, wall_h * 0.5, z1),
            Vec3::new(x1 - x0 + thick, wall_h, thick),
        );
        bassin_wall(
            "Mur bassin Ouest",
            Vec3::new(x0, wall_h * 0.5, cz),
            Vec3::new(thick, wall_h, z1 - z0 + thick),
        );
        bassin_wall(
            "Mur bassin Est",
            Vec3::new(x1, wall_h * 0.5, cz),
            Vec3::new(thick, wall_h, z1 - z0 + thick),
        );
    }

    // Tunnel/surplomb (Sprint 26) : passage praticable sous un arceau
    // statique — géométrie non-heightmap à dessein, un heightmap XZ→Y ne
    // représente pas un surplomb (cf. l'objectif du sprint dans
    // `sprintreflecion.md`) — posé sur du sol resté PLAT (hors de toute
    // zone de relief : x∈[-34.2,-31.0] est à l'ouest de
    // `MMORPG_MOUND_X_LOCAL`, qui commence à x=-30,2, donc `mound_h` y
    // est nul ; la bande de collines historique est, elle, nulle dès
    // x=-34,5). Même poche libre que le bassin ci-dessus mais décalée en
    // Z (couloir x∈[-34.4,-30.8] z∈[-8.8,-2.8], vérifié libre
    // séparément) : deux piliers encadrant un passage de 2 m de large,
    // surmontés d'un toit — tous statiques, le sol sous le passage garde
    // son collider dalle plate habituel (aucun trou de collision).
    {
        let pillar_y = 1.1;
        let pillar_h = 2.2;
        let z_center = -5.75;
        let z_len = 5.1;
        let mut arche = |name: &str, pos: Vec3, scale: Vec3| {
            let mut c = demo_obj(name, MeshKind::Cube, pos);
            c.transform = c.transform.with_scale(scale);
            c.physics = PhysicsKind::Static;
            c.color = [0.5, 0.48, 0.44];
            objects.push(c);
        };
        arche(
            "Pilier tunnel Ouest",
            Vec3::new(-33.9, pillar_y, z_center),
            Vec3::new(0.6, pillar_h, z_len),
        );
        arche(
            "Pilier tunnel Est",
            Vec3::new(-31.3, pillar_y, z_center),
            Vec3::new(0.6, pillar_h, z_len),
        );
        arche(
            "Toit tunnel",
            Vec3::new(-32.6, pillar_y + pillar_h * 0.5 + 0.2, z_center),
            Vec3::new(3.2, 0.4, z_len),
        );
    }

    // 2) Meshes glb générés par Blender headless (gen_nature_pack.py pour
    //    les statiques, gen_nature_animated.py pour les riggés). `solide` →
    //    corps statique avec collider `TriMesh` (silhouette exacte : les
    //    ponts se traversent à pied, on se faufile entre les troncs) ;
    //    sinon pur décor traversable (fleurs, riz, roseaux, panneaux…).
    //    `anim` → l'instance reçoit un `AnimationState` sur le clip nommé :
    //    le mesh reste partagé entre instances (chargé une seule fois),
    //    seul l'état d'animation est par objet — même mécanique que les
    //    créatures. Le TriMesh d'un solide animé est celui de la POSE DE
    //    REPOS (l'animation est purement visuelle) : les parties mobiles
    //    des moulins sont hors de portée du joueur (en hauteur/côté eau).
    //    Tout décor solide respecte ≥ 3,5 m (RAY_DIST des sondes) de
    //    dégagement autour des spawns de créatures — vérifié par le test
    //    `mmorpg_demo_contains_walkable_nature_decor`, y compris pour le
    //    décor semé procéduralement ci-dessous.
}
