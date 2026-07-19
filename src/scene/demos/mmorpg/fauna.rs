use super::*;

pub(super) fn add_ambient_fauna(
    objects: &mut Vec<SceneObject>,
    imported: &mut Vec<ImportedMesh>,
    half: f32,
) {
    // --- Faune ambiante (créatures 27-61, packs générés Blender headless) ----
    // Purement décorative : même pipeline d'import + `creature_wander_script`
    // que `MMORPG_CREATURES` ci-dessus (elles errent, évitent les murs, jouent
    // Idle/Walk), mais SANS `Combat` ni morsure — ni tuable, ni dangereuse.
    // Préfixe de nom « Errant » (ni « Créature », réservé à
    // `MMORPG_CREATURES`/ses outils de synchro, ni « Faune », déjà pris par
    // `hameau_gdd_demo()` — cf. son bloc « Faune ambiante »
    // `gen_menagerie_pack*.py` et ses noms `Faune {n} {cluster}-{poses}` de
    // `faune_scatter`, présents dans la scène déjà embarquée : un préfixe
    // partagé aurait fait retirer ce décor-là par erreur, constaté à
    // l'exécution de l'outil de synchro avant ce correctif).
    //
    // Table data-driven (fichier, spawn) plutôt que 35 blocs de champs répétés
    // (même choix que `MMORPG_CREATURES`) : cap initial et déphasage du
    // méandre dérivés de l'index pour que deux instances ne partent jamais
    // dans le même sens (cf. la doc de `creature_wander_script` sur ce bug).
    // Couche de collision : 5 bits partagés en rotation (27..31, pas un bit
    // par créature comme `MMORPG_CREATURES` — u32 ne peut décaler que jusqu'à
    // 31) ; deux errantes qui partagent un bit s'ignorent mutuellement au
    // raycast (se traversent), acceptable pour du décor sans aucun garde-fou
    // dessus, cf. le commentaire de doc sur `ray_mask`.
    const MMORPG_AMBIENT_FAUNA_SPAWNS: &[(&str, f32, f32)] = &[
        // Forêt nord-est (x resserré sur 9..20 pour rester à l'écart du
        // second hameau fortifié de `VILLAGE_PROPS`, x 23..35).
        ("creature27.glb", 10.0, -10.0),
        ("creature28.glb", 14.0, -14.0),
        ("creature29.glb", 18.0, -18.0),
        ("creature30.glb", 10.0, -18.0),
        ("creature31.glb", 14.0, -24.0),
        ("creature32.glb", 18.0, -28.0),
        ("creature33.glb", 10.0, -28.0),
        ("creature34.glb", 14.0, -32.0),
        ("creature35.glb", 18.0, -12.0),
        ("creature36.glb", 10.0, -32.0),
        // Prairie centrale.
        ("creature37.glb", -6.0, -8.0),
        ("creature38.glb", 0.0, -6.0),
        ("creature39.glb", 4.0, -2.0),
        ("creature40.glb", -4.0, 2.0),
        ("creature41.glb", 2.0, 6.0),
        ("creature42.glb", -8.0, 4.0),
        // Rives du lac et des rivières (x = -30/-31, à l'ouest des plans
        // d'eau et de leurs berges).
        ("creature43.glb", -31.0, -30.0),
        ("creature44.glb", -31.0, -20.0),
        ("creature45.glb", -30.0, 0.0),
        ("creature46.glb", -31.0, 20.0),
        ("creature47.glb", -30.0, 30.0),
        ("creature48.glb", -22.0, 16.0),
        // Rizières en damier (sud-ouest).
        ("creature49.glb", -8.0, 25.0),
        ("creature50.glb", -2.0, 26.0),
        ("creature51.glb", 4.0, 28.0),
        ("creature52.glb", -6.0, 32.0),
        ("creature53.glb", 2.0, 33.0),
        // Promontoire rocheux (est).
        ("creature54.glb", 22.0, 4.0),
        ("creature55.glb", 26.0, 6.0),
        ("creature56.glb", 30.0, 8.0),
        ("creature57.glb", 24.0, 12.0),
        ("creature58.glb", 32.0, 14.0),
        // Lisières diverses (complètent la répartition par biome).
        ("creature59.glb", 25.0, -5.0),
        ("creature60.glb", 15.0, -5.0),
        ("creature61.glb", 20.0, -30.0),
        // Renard (creature62, jusqu'ici généré mais jamais spawné — cf.
        // `docs/rapport_qualite_creatures_vs_hyper3d.md`), lisière ouest
        // de la prairie centrale, à l'écart du lac (`EXCL_EAU_ROUTES`) et
        // de la halte « Ouest souche/fleurs ».
        ("creature62.glb", -9.5, -0.5),
        // Pack savane africaine (creature63-67, `gen_creature_pack63_67.py`),
        // couloir libre entre rizières (x -11..9, z 23.5..34.5) et
        // promontoire rocheux (x 20..34, z 2..16) — chaque position
        // vérifiée à ≥4 m de tout décor/créature/halte déjà posé et de
        // ses 4 voisines du pack (`EXCL_EAU_ROUTES`/`EXCL_ZONES_AMENAGEES`
        // exclus par construction).
        ("creature63.glb", 9.0, 26.0),
        ("creature64.glb", 16.5, 33.0),
        ("creature65.glb", 22.0, 25.0),
        ("creature66.glb", 22.0, 29.0),
        ("creature67.glb", 22.0, 33.0),
        // Pack mammifères ronds au corps organique (creature68-72,
        // `gen_creature_pack68_72_organic.py`, Metaball + Automatic
        // Weights — cf. `proto_creature62_fox_organic.py`) : hippopotame,
        // capybara, loutre de mer, koala, marmotte. Dispersés en lisière
        // ouest/sud-ouest, à ≥4,5 m de tout décor/créature/halte déjà
        // posé (`EXCL_EAU_ROUTES`/`EXCL_ZONES_AMENAGEES` exclus par
        // construction).
        ("creature68.glb", -18.0, -12.0),
        ("creature69.glb", -23.5, -33.0),
        ("creature70.glb", -13.5, -13.0),
        ("creature71.glb", -12.5, 34.0),
        ("creature72.glb", -21.5, -3.5),
    ];
    for (i, &(file, x, z)) in MMORPG_AMBIENT_FAUNA_SPAWNS.iter().enumerate() {
        let n = i + 27;
        let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file);
        match crate::scene::import::load_gltf(&path) {
            Ok((data, aabb_min, aabb_max)) => {
                let mut mesh = ImportedMesh {
                    path: path.clone(),
                    data,
                    aabb_min,
                    aabb_max,
                    ..Default::default()
                };
                mesh.load_skinning();
                let mesh_index = imported.len() as u32;
                let name = format!("Errant {n}");
                let prefix = format!("faune{n}_");
                let mut fauna =
                    demo_obj(&name, MeshKind::Imported(mesh_index), Vec3::new(x, 0.0, z));
                // Gabarit 0.28..0.45 plutôt qu'une échelle 0.35 fixe pour les 35
                // instances (constaté sur une capture en jeu : à taille identique
                // et à hauteur d'œil, elles se fondent en points flous indistincts).
                // Suite du ratio doré (même famille de trucs que `heading0`/`phase`
                // ci-dessous) : distribution à faible discrépance, deux voisines
                // n'ont jamais un gabarit presque identique.
                let gabarit = 0.28 + 0.17 * (i as f32 * 0.618_034).fract();
                fauna.transform = fauna.transform.with_scale(Vec3::splat(gabarit));
                fauna.animation = Some(AnimationState {
                    clip: "Idle".into(),
                    ..Default::default()
                });
                fauna.physics = PhysicsKind::Kinematic;
                let layer_bit = 27 + (i as u32 % 5);
                fauna.collision_layer = 1 << layer_bit;
                let heading0 = (i as f32 * 47.0).rem_euclid(360.0);
                let phase = i as f32 * 0.833;
                fauna.script =
                    creature_wander_script(half, &prefix, !(1_u32 << layer_bit), heading0, phase);
                objects.push(fauna);
                imported.push(mesh);
            }
            Err(e) => log::error!("Errant {n} MMORPG ({path}) : {e}"),
        }
    }
}
