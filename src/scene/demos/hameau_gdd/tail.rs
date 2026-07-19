use super::*;

pub(super) fn add_tail(objects: &mut Vec<SceneObject>, imported: &mut Vec<ImportedMesh>) {
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
        poser(objects, imported, name, file, x, z, 0.9, 0.0, false);
    }
    // --- Lucioles supplémentaires près du camp de chasseurs (ambiance
    // nocturne, en plus du cercle de 6 sur la place).
    for (i, (x, z)) in [(20.0_f32, 49.0_f32), (14.0, 45.0), (18.0, 51.0)]
        .into_iter()
        .enumerate()
    {
        poser(
            objects,
            imported,
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
            objects,
            imported,
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
        objects,
        imported,
        "Autel de l'Aînée",
        "siege_elder_altar.glb",
        42.4,
        42.4,
        1.0,
        0.0,
        true,
    );
    poser(
        objects,
        imported,
        "Cage du chef",
        "siege_chief_cage.glb",
        45.4,
        40.4,
        1.0,
        -30.0,
        true,
    );
    poser(
        objects,
        imported,
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
}
