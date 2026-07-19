use super::*;

pub(super) fn add_water(objects: &mut Vec<SceneObject>, imported: &mut Vec<ImportedMesh>) {
    // --- Hors les murs : rivière (deux bras, ouest et sud) rejoignant un
    // lac au coin sud-ouest, pont, moulin à eau, berges, petite rizière.
    const EAU: [f32; 3] = [0.18, 0.42, 0.65];
    const EAU_LAC: [f32; 3] = [0.14, 0.34, 0.55];
    const SABLE: [f32; 3] = [0.72, 0.64, 0.44];
    aplat(objects, "Rivière ouest", -31.5, 0.0, 5.0, 58.0, 0.02, EAU);
    aplat(objects, "Rivière sud", 0.0, 31.5, 58.0, 5.0, 0.02, EAU);
    aplat(
        objects,
        "Berge du lac",
        -42.5,
        42.5,
        29.0,
        29.0,
        0.012,
        SABLE,
    );
    aplat(objects, "Lac", -42.0, 42.0, 24.0, 24.0, 0.015, EAU_LAC);
    aplat(
        objects,
        "Rizière du sud",
        -42.0,
        60.0,
        8.0,
        6.0,
        0.03,
        [0.55, 0.6, 0.25],
    );
    poser(
        objects,
        imported,
        "Pont de la rivière ouest",
        "nature_bridge.glb",
        -31.5,
        0.0,
        1.15,
        0.0,
        true,
    );
    poser(
        objects,
        imported,
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
            objects,
            imported,
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
            objects,
            imported,
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
            objects,
            imported,
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
            objects,
            imported,
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
            objects,
            imported,
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
            objects,
            imported,
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
}
