use super::*;

pub(super) fn add_fort(objects: &mut Vec<SceneObject>, imported: &mut Vec<ImportedMesh>) {
    // --- Créatures : reprises de `mmorpg_demo()`, cf. la doc de fonction.
    let base = Scene::mmorpg_demo();
    for c in base
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Créature"))
    {
        let mut c = c.clone();
        if let MeshKind::Imported(old_idx) = c.mesh {
            let path = base.imported[old_idx as usize].path.clone();
            let new_idx = match imported.iter().position(|m| m.path == path) {
                Some(i) => i,
                None => match crate::scene::import::load_gltf(&path) {
                    Ok((data, aabb_min, aabb_max)) => {
                        let mut mesh = ImportedMesh {
                            path,
                            data,
                            aabb_min,
                            aabb_max,
                            ..Default::default()
                        };
                        mesh.load_skinning();
                        imported.push(mesh);
                        imported.len() - 1
                    }
                    Err(e) => {
                        log::error!("Créature « {} » : {e}", c.name);
                        continue;
                    }
                },
            };
            c.mesh = MeshKind::Imported(new_idx as u32);
        }
        objects.push(c);
    }

    // --- Remparts : 4 pans, porte principale (brèche 5 m) au milieu de
    // chacun, 2 brèches diagonales secondaires (coins Nord-Est/Sud-Ouest,
    // en ne construisant pas les 5 derniers mètres des deux pans qui s'y
    // rejoignent). Habillés avec le pack siège (creation3DBlendersuite.md
    // / docs/integration_siege_scene.md) plutôt que des `box_seg` plats — même
    // périmètre exact (HALF/GATE_HALF/TRIM inchangés, donc `in_corridor`
    // et les couloirs de vague restent valides), juste le rendu change.
    wall_run(
        objects,
        imported,
        "Rempart Nord Ouest",
        -HALF,
        -GATE_HALF,
        -HALF,
        -HALF,
        0.0,
    );
    wall_run(
        objects,
        imported,
        "Rempart Nord Est",
        GATE_HALF,
        HALF - TRIM,
        -HALF,
        -HALF,
        0.0,
    );
    wall_run(
        objects,
        imported,
        "Rempart Est Nord",
        HALF,
        HALF,
        -HALF + TRIM,
        -GATE_HALF,
        90.0,
    );
    wall_run(
        objects,
        imported,
        "Rempart Est Sud",
        HALF,
        HALF,
        GATE_HALF,
        HALF,
        90.0,
    );
    wall_run(
        objects,
        imported,
        "Rempart Sud Ouest",
        -HALF + TRIM,
        -GATE_HALF,
        HALF,
        HALF,
        0.0,
    );
    wall_run(
        objects,
        imported,
        "Rempart Sud Est",
        GATE_HALF,
        HALF,
        HALF,
        HALF,
        0.0,
    );
    wall_run(
        objects,
        imported,
        "Rempart Ouest Nord",
        -HALF,
        -HALF,
        -HALF,
        -GATE_HALF,
        90.0,
    );
    wall_run(
        objects,
        imported,
        "Rempart Ouest Sud",
        -HALF,
        -HALF,
        GATE_HALF,
        HALF - TRIM,
        90.0,
    );

    // Tours d'angle aux 2 coins pleins (Nord-Ouest/Sud-Est) ; les coins
    // Nord-Est/Sud-Ouest restent des brèches ouvertes (cf. TRIM ci-dessus).
    poser(
        objects,
        imported,
        "Tour Nord-Ouest",
        "siege_tower.glb",
        -HALF,
        -HALF,
        1.0,
        0.0,
        true,
    );
    poser(
        objects,
        imported,
        "Tour Sud-Est",
        "siege_tower.glb",
        HALF,
        HALF,
        1.0,
        0.0,
        true,
    );

    // Portes : GATE_W (siege_gate_*) = 5.0 = 2×GATE_HALF, aucune
    // re-échelle nécessaire. Nord/Sud fermées, Est/Ouest embrasées — les
    // deux variantes doivent être présentes dans la carte (l'état réel
    // « embrasé » au signal de vague reste un chantier de gameplay
    // séparé, cf. docs/integration_siege_scene.md « hors scope »).
    poser(
        objects,
        imported,
        "Porte Nord",
        "siege_gate_closed.glb",
        0.0,
        -HALF,
        1.0,
        0.0,
        true,
    );
    poser(
        objects,
        imported,
        "Porte Sud",
        "siege_gate_closed.glb",
        0.0,
        HALF,
        1.0,
        0.0,
        true,
    );
    poser(
        objects,
        imported,
        "Porte Est",
        "siege_gate_burning.glb",
        HALF,
        0.0,
        1.0,
        90.0,
        true,
    );
    poser(
        objects,
        imported,
        "Porte Ouest",
        "siege_gate_burning.glb",
        -HALF,
        0.0,
        1.0,
        90.0,
        true,
    );

    // --- Chemin de ronde (hauteur ~2,2 m), longe l'intérieur des 4 murs,
    // mêmes brèches diagonales que les remparts (pas de coupure au droit
    // des portes : les défenseurs peuvent longer au-dessus de l'entrée).
    const RAMPART_R: f32 = HALF - 1.0;
    const RAMPART_COLOR: [f32; 3] = [0.4, 0.38, 0.4];
    box_seg(
        objects,
        "Chemin de ronde Nord",
        -RAMPART_R,
        RAMPART_R - TRIM,
        -RAMPART_R,
        -RAMPART_R,
        2.2,
        0.3,
        1.5,
        RAMPART_COLOR,
    );
    box_seg(
        objects,
        "Chemin de ronde Est",
        RAMPART_R,
        RAMPART_R,
        -RAMPART_R + TRIM,
        RAMPART_R,
        2.2,
        0.3,
        1.5,
        RAMPART_COLOR,
    );
    box_seg(
        objects,
        "Chemin de ronde Sud",
        -RAMPART_R + TRIM,
        RAMPART_R,
        RAMPART_R,
        RAMPART_R,
        2.2,
        0.3,
        1.5,
        RAMPART_COLOR,
    );
    box_seg(
        objects,
        "Chemin de ronde Ouest",
        -RAMPART_R,
        -RAMPART_R,
        -RAMPART_R,
        RAMPART_R - TRIM,
        2.2,
        0.3,
        1.5,
        RAMPART_COLOR,
    );
    poser(
        objects,
        imported,
        "Marches du rempart Nord-Ouest",
        "siege_rampart_stairs.glb",
        -HALF + 2.0,
        -HALF + 4.0,
        1.1,
        45.0,
        true,
    );
    poser(
        objects,
        imported,
        "Marches du rempart Sud-Est",
        "siege_rampart_stairs.glb",
        HALF - 2.0,
        HALF - 4.0,
        1.1,
        225.0,
        true,
    );

    // --- Dressing complémentaire des remparts/portes (pack siège) :
    // bastions à mi-pan, module de créneau en filler sur les tours,
    // chemin de ronde décoratif + torches sur le parapet, poterne à la
    // brèche Sud-Ouest, herse/caisse/pieux/bannière à la porte Nord,
    // boulets en tas près de la porte Est, chariot de braises sur le
    // chemin entre la porte Nord et la place.
    poser(
        objects,
        imported,
        "Bastion Nord",
        "siege_bastion.glb",
        0.0,
        -HALF,
        1.0,
        0.0,
        true,
    );
    poser(
        objects,
        imported,
        "Bastion Sud",
        "siege_bastion.glb",
        0.0,
        HALF,
        1.0,
        180.0,
        true,
    );
    poser(
        objects,
        imported,
        "Module de créneau Nord-Ouest",
        "siege_crenel_module.glb",
        -HALF + 1.0,
        -HALF - 0.3,
        1.0,
        0.0,
        false,
    );
    poser(
        objects,
        imported,
        "Module de créneau Sud-Est",
        "siege_crenel_module.glb",
        HALF - 1.0,
        HALF + 0.3,
        1.0,
        180.0,
        false,
    );
    for (i, (x, z, yaw)) in [
        (-HALF + 6.0_f32, -HALF + 1.0_f32, 0.0_f32),
        (HALF - 6.0, -HALF + 1.0, 0.0),
        (HALF - 1.0, -HALF + 6.0, 90.0),
        (HALF - 1.0, HALF - 6.0, 90.0),
        (HALF - 6.0, HALF - 1.0, 180.0),
        (-HALF + 6.0, HALF - 1.0, 180.0),
    ]
    .into_iter()
    .enumerate()
    {
        poser(
            objects,
            imported,
            &format!("Torche de rempart {}", i + 1),
            "siege_rampart_torch.glb",
            x,
            z,
            1.0,
            yaw,
            false,
        );
    }
    poser(
        objects,
        imported,
        "Chemin de ronde décoratif Nord",
        "siege_rampart_walk.glb",
        -6.0,
        -HALF + 1.1,
        1.0,
        0.0,
        false,
    );
    poser(
        objects,
        imported,
        "Poterne Sud-Ouest",
        "siege_postern.glb",
        -HALF + 2.0,
        HALF - 2.0,
        1.0,
        135.0,
        true,
    );
    poser(
        objects,
        imported,
        "Herse de la porte Nord",
        "siege_portcullis.glb",
        0.0,
        -HALF + 0.4,
        1.0,
        0.0,
        false,
    );
    poser(
        objects,
        imported,
        "Caisse de réserve de la porte Nord",
        "siege_reserve_crate.glb",
        1.6,
        -HALF + 1.5,
        1.0,
        10.0,
        true,
    );
    poser(
        objects,
        imported,
        "Rangée de pieux de la porte Nord",
        "siege_stake_row.glb",
        -1.5,
        -HALF + 2.5,
        1.0,
        0.0,
        true,
    );
    poser(
        objects,
        imported,
        "Bannière de vague Nord",
        "siege_wave_banner.glb",
        -3.0,
        -HALF + 0.6,
        1.0,
        0.0,
        false,
    );
    poser(
        objects,
        imported,
        "Bannière de vague Sud",
        "siege_wave_banner.glb",
        3.0,
        HALF - 0.6,
        1.0,
        180.0,
        false,
    );
    poser(
        objects,
        imported,
        "Corne d'alerte Nord",
        "siege_alert_horn.glb",
        -2.2,
        -HALF + 0.5,
        1.0,
        0.0,
        false,
    );
    poser(
        objects,
        imported,
        "Corne d'alerte Sud",
        "siege_alert_horn.glb",
        2.2,
        HALF - 0.5,
        1.0,
        180.0,
        false,
    );
    poser(
        objects,
        imported,
        "Panneau directionnel Est",
        "siege_rampart_signpost.glb",
        HALF - 3.0,
        -3.0,
        1.0,
        90.0,
        true,
    );
    for (i, (dx, dz)) in [(0.0_f32, 0.0_f32), (0.3, 0.25), (-0.25, 0.2)]
        .into_iter()
        .enumerate()
    {
        poser(
            objects,
            imported,
            &format!("Boulet {}", i + 1),
            "siege_cannonball.glb",
            HALF - 2.0 + dx,
            -2.0 + dz,
            1.0,
            0.0,
            true,
        );
    }
    poser(
        objects,
        imported,
        "Chariot de braises",
        "siege_ember_cart.glb",
        0.0,
        -12.0,
        1.0,
        0.0,
        true,
    );
}
