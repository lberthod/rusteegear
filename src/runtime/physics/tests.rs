use super::*;
use crate::scene::{ImportedMesh, Scene, SceneObject};

/// Décor triangulaire : un seul triangle plat couvrant la moitié « arrière-gauche »
/// du carré `[-1, 1] × [-1, 1]` (z=0 fixe) — sa boîte englobante est le carré
/// entier, mais sa silhouette réelle laisse le coin « avant-droit » (x>0, z>0
/// environ) complètement vide. Un collider `Box`/`Auto` (bounding box) bloquerait
/// donc n'importe où sur tout le carré ; un `TriMesh`/`ConvexHull` fidèle ne
/// bloque que sur la moitié réellement couverte.
fn wedge_scene(shape: ColliderShape) -> Scene {
    use crate::gfx::mesh::{MeshData, Vertex};
    let v = |x: f32, z: f32| Vertex {
        position: [x, 0.0, z],
        normal: [0.0, 1.0, 0.0],
        color: [1.0, 1.0, 1.0],
        uv: [0.0, 0.0],
    };
    let data = MeshData {
        vertices: vec![v(-1.0, -1.0), v(1.0, -1.0), v(-1.0, 1.0)],
        // Ordre choisi pour une normale +Y (règle de la main droite) : une boule
        // qui tombe dessus doit heurter la face « du dessus », pas le dos du
        // triangle — l'ordre [0,1,2] donnerait une normale vers -Y, et la boule
        // tomberait au travers malgré un TriMesh construit avec succès.
        indices: vec![0, 2, 1],
    };
    let mut imported = ImportedMesh {
        name: "Coin".into(),
        ..Default::default()
    };
    imported.data = data;
    // `local_aabb` (utilisé par le repli `Auto`/`Box`) lit ces champs directement,
    // pas les vertices de `data` — sans eux, la boîte englobante serait nulle et
    // les deux tests seraient des faux positifs (tout tomberait à travers, y
    // compris le cas `Auto` censé bloquer).
    imported.aabb_min = Vec3::new(-1.0, -0.05, -1.0);
    imported.aabb_max = Vec3::new(1.0, 0.05, 1.0);
    let mut scene = Scene::default();
    scene.imported.push(imported);
    scene.objects.push(SceneObject {
        name: "Décor".into(),
        mesh: crate::scene::MeshKind::Imported(0),
        physics: PhysicsKind::Static,
        collider_shape: shape,
        ..Default::default()
    });
    scene
}

/// Départ bas (0.5 m, pas 3 m) : un `TriMesh` n'a pas d'épaisseur, et une boule
/// qui tombe assez vite peut le traverser en un seul pas de simulation sans jamais
/// être détectée en collision (tunneling) — la CCD qui corrigerait ça sur un corps
/// dynamique rapide (`ccd` par objet) est hors sujet ici. Une chute courte reste
/// assez lente pour ne pas tunneliser, sans avoir besoin d'anticiper ce mécanisme.
fn drop_ball(scene: &mut Scene, name: &str, x: f32, z: f32) -> usize {
    scene.objects.push(SceneObject {
        name: name.into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(x, 0.5, z))
            .with_scale(Vec3::splat(0.2)),
        physics: PhysicsKind::Dynamic,
        ..Default::default()
    });
    scene.objects.len() - 1
}

/// Un décor importé (`TriMesh`) doit bloquer une boule qui tombe sur sa silhouette
/// réelle, et **laisser tomber** une boule au-dessus d'un coin vide de sa boîte
/// englobante — la preuve que le collider suit la géométrie, pas juste l'AABB
/// (`Auto`/`Box` ne suivent que l'AABB).
#[test]
fn a_trimesh_collider_follows_the_actual_silhouette_not_the_bounding_box() {
    let mut scene = wedge_scene(ColliderShape::TriMesh);
    let covered = drop_ball(&mut scene, "Boule couverte", -0.5, -0.5);
    let empty_corner = drop_ball(&mut scene, "Boule coin vide", 0.6, 0.6);
    let mut phys = Physics::build(&scene);
    for _ in 0..120 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let y_covered = scene.objects[covered].transform.position.y;
    let y_empty = scene.objects[empty_corner].transform.position.y;
    assert!(
        y_covered > -0.5,
        "au-dessus du triangle, la boule doit être arrêtée près du sol (y={y_covered})"
    );
    assert!(
        y_empty < -1.0,
        "au-dessus du coin vide, la boule doit être passée à travers (y={y_empty})"
    );
}

/// Contre-épreuve : **sans** le repli `TriMesh` (`Auto`, la boîte englobante du
/// triangle), la même boule « coin vide » resterait bloquée — la preuve que le
/// test précédent mesure bien la fidélité du collider, pas autre chose (ex. une
/// gravité qui ne s'applique jamais).
#[test]
fn without_trimesh_the_bounding_box_wrongly_blocks_the_empty_corner() {
    let mut scene = wedge_scene(ColliderShape::Auto);
    let empty_corner = drop_ball(&mut scene, "Boule coin vide", 0.6, 0.6);
    let mut phys = Physics::build(&scene);
    for _ in 0..120 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let y_empty = scene.objects[empty_corner].transform.position.y;
    assert!(
        y_empty > -1.0,
        "avec un collider en boîte englobante, la boule doit être (à tort) \
             bloquée au-dessus du coin vide (y={y_empty})"
    );
}

/// Petit tétraèdre (4 points non coplanaires) : un `ConvexHull` en a besoin pour
/// un volume 3D bien défini — contrairement au triangle plat de `wedge_scene`,
/// suffisant pour `TriMesh` (une surface) mais dégénéré comme volume.
fn tetrahedron_mesh() -> ImportedMesh {
    use crate::gfx::mesh::{MeshData, Vertex};
    let v = |x: f32, y: f32, z: f32| Vertex {
        position: [x, y, z],
        normal: [0.0, 1.0, 0.0],
        color: [1.0, 1.0, 1.0],
        uv: [0.0, 0.0],
    };
    let data = MeshData {
        vertices: vec![
            v(-0.2, -0.2, -0.2),
            v(0.2, -0.2, -0.2),
            v(0.0, -0.2, 0.2),
            v(0.0, 0.2, 0.0),
        ],
        indices: vec![0, 1, 2, 0, 2, 3, 0, 3, 1, 1, 3, 2],
    };
    let mut imported = ImportedMesh {
        name: "Rocher".into(),
        ..Default::default()
    };
    imported.data = data;
    imported.aabb_min = Vec3::splat(-0.2);
    imported.aabb_max = Vec3::splat(0.2);
    imported
}

fn floor_and_falling_rock(shape: ColliderShape) -> Scene {
    let mut scene = Scene::default();
    scene.imported.push(tetrahedron_mesh());
    scene.objects.push(SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
            .with_scale(Vec3::new(10.0, 1.0, 10.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Rocher".into(),
        mesh: crate::scene::MeshKind::Imported(0),
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.3, 0.0)),
        physics: PhysicsKind::Dynamic,
        collider_shape: shape,
        ..Default::default()
    });
    scene
}

/// Second cas : contrairement à `TriMesh` (pas de propriétés de masse), un
/// `ConvexHull` doit fonctionner sur un corps **dynamique** — c'est tout l'intérêt
/// de proposer les deux formes plutôt qu'une seule. Un rocher importé tombe sur un
/// sol et doit s'y arrêter, pas le traverser (ce qui arriverait si
/// `SharedShape::convex_hull` échouait silencieusement et que le repli `cuboid()`
/// était lui-même mal dimensionné).
#[test]
fn a_convex_hull_collider_works_on_a_dynamic_body() {
    let mut scene = floor_and_falling_rock(ColliderShape::ConvexHull);
    let mut phys = Physics::build(&scene);
    for _ in 0..120 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let y = scene.objects[1].transform.position.y;
    assert!(
        y > -1.5,
        "le rocher (ConvexHull, dynamique) doit se poser sur le sol, pas le \
             traverser (y={y})"
    );
}

/// Sprint 24 (Phase K, `sprintreflecion.md`) : un objet dynamique lâché
/// au-dessus de la bande de collines du sol `MeshKind::Terrain` doit tomber
/// et se stabiliser à une hauteur cohérente avec
/// `gfx::mesh::mmorpg_terrain_local_height` — ni la traverser (creux
/// physique absent), ni léviter dessus (relief visuel sans collider
/// correspondant). Même structure que `floor_and_falling_rock`, avec un sol
/// `MeshKind::Terrain` (échelle 72×1×72, comme `Scene::mmorpg_demo`) au lieu
/// d'un `MeshKind::Cube` plat.
fn terrain_hill_and_falling_ball() -> Scene {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Terrain,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(72.0, 1.0, 72.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    // Mur ouest, comme dans `Scene::mmorpg_demo` : la bande de collines
    // penche vers x=-36 (cf. sa doc) — sans ce mur, une balle qui roule dessus
    // continue indéfiniment dans le vide au-delà de la carte, ce qui ne
    // prouverait rien sur le collider du relief lui-même (constaté : sans ce
    // mur, la balle sortait de la carte et tombait à l'infini).
    scene.objects.push(SceneObject {
        name: "Mur Ouest".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(-36.0, 0.9, 0.0))
            .with_scale(Vec3::new(0.5, 1.8, 72.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Balle".into(),
        mesh: crate::scene::MeshKind::Sphere,
        // x=-35.25, z=0 : au cœur (plus haut point) de la bande de collines
        // (cf. la doc de `mmorpg_terrain_local_height`), lâchée bien au-dessus
        // du relief maximal (~1,3 m).
        transform: crate::scene::Transform::from_pos(Vec3::new(-35.25, 6.0, 0.0))
            .with_scale(Vec3::splat(0.4)),
        physics: PhysicsKind::Dynamic,
        ..Default::default()
    });
    scene
}

#[test]
fn a_dynamic_body_settles_on_the_terrain_hill_at_the_right_height() {
    let mut scene = terrain_hill_and_falling_ball();
    let mut phys = Physics::build(&scene);
    for _ in 0..300 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let ball = &scene.objects[2];
    let radius = 0.2; // scale.splat(0.4) × demi-AABB locale de Sphere (0.5)
    // Hauteur attendue au point où la balle s'est RÉELLEMENT arrêtée (pas au
    // point de lâcher) : la bande de collines a une pente, la balle a pu
    // rouler avant de se stabiliser — c'est la relation hauteur-sol/position
    // qui doit tenir, pas une position figée.
    let expected_ground = crate::gfx::mesh::mmorpg_terrain_local_height(
        ball.transform.position.x / 72.0,
        ball.transform.position.z / 72.0,
    );
    let y = ball.transform.position.y;
    assert!(
        y > expected_ground - 0.3,
        "la balle a traversé le relief du terrain (y={y}, sol attendu \
             ≈{expected_ground})"
    );
    assert!(
        y < expected_ground + radius + 1.0,
        "la balle lévite au-dessus du relief (y={y}, sol attendu ≈{expected_ground})"
    );
    // La balle a pu rouler hors de la bande de collines (étroite, ~1,5 m de
    // large) jusqu'au sol plat environnant (cuboid fin, y≈0) — dans les deux
    // cas, elle doit s'être arrêtée près de sa hauteur de sol locale, pas être
    // tombée à travers vers le vide (`y` très négatif, preuve d'absence de
    // collider sous elle).
    assert!(
        y > -1.0,
        "la balle est tombée bien en dessous de tout sol connu (y={y})"
    );
}

/// Garde-fou : demander `TriMesh` sur un corps dynamique ne doit ni planter ni
/// laisser l'objet traverser indéfiniment le décor — `Physics::build` doit se
/// replier sur `ConvexHull` (cf. le `log::warn!` correspondant), avec le même
/// comportement observable que le test précédent.
#[test]
fn requesting_trimesh_on_a_dynamic_body_falls_back_to_convex_hull() {
    let mut scene = floor_and_falling_rock(ColliderShape::TriMesh);
    let mut phys = Physics::build(&scene);
    for _ in 0..120 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let y = scene.objects[1].transform.position.y;
    assert!(
        y > -1.5,
        "TriMesh sur un corps dynamique doit se replier sur ConvexHull, pas \
             laisser tomber l'objet indéfiniment (y={y})"
    );
}

/// Mur fin (5 cm d'épaisseur) + missile positionné juste devant, à `x=5` — cf.
/// `ccd`. Index 0 = mur, 1 = missile.
fn missile_and_thin_wall(ccd: bool) -> Scene {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Mur".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
            .with_scale(Vec3::new(0.05, 2.0, 2.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Missile".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(0.1)),
        physics: PhysicsKind::Dynamic,
        ccd,
        ..Default::default()
    });
    scene
}

/// Un missile assez rapide pour traverser un mur fin en un seul pas de simulation
/// (le même « tunneling » que `drop_ball` évite en partant bas) ne doit plus le
/// faire une fois `ccd` activé.
#[test]
fn ccd_prevents_a_fast_missile_from_tunneling_through_a_thin_wall() {
    let mut scene = missile_and_thin_wall(true);
    let mut phys = Physics::build(&scene);
    phys.set_velocity(1, Vec3::new(200.0, 0.0, 0.0));
    for _ in 0..30 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let x = scene.objects[1].transform.position.x;
    assert!(
        x < 5.0,
        "avec ccd, le missile doit être arrêté par le mur fin (x={x})"
    );
}

/// Contre-épreuve : sans `ccd`, le même missile à la même vitesse traverse le mur
/// — la preuve que le test précédent mesure bien l'effet de `ccd`, pas autre
/// chose (ex. un mur mal placé).
#[test]
fn without_ccd_the_same_fast_missile_tunnels_through_the_wall() {
    let mut scene = missile_and_thin_wall(false);
    let mut phys = Physics::build(&scene);
    phys.set_velocity(1, Vec3::new(200.0, 0.0, 0.0));
    for _ in 0..30 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let x = scene.objects[1].transform.position.x;
    assert!(
        x > 5.0,
        "sans ccd, le missile doit traverser le mur fin par tunneling (x={x})"
    );
}

/// `collision_mask` doit pouvoir faire ignorer une couche précise — un missile
/// qui ne collisionne pas la couche du mur (`collision_mask` sans le bit du mur)
/// doit le traverser à vitesse normale (pas besoin de `ccd` ici, la vitesse reste
/// modeste).
#[test]
fn a_collision_mask_lets_a_projectile_ignore_a_specific_layer() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Mur".into(),
        mesh: crate::scene::MeshKind::Cube,
        // Très haut (pas juste 2 m) : à 3 m/s le missile met ~1,7 s à atteindre le
        // mur, largement assez pour que la gravité le fasse tomber sous un mur de
        // hauteur normale avant d'y arriver — un mur haut isole le test de cet
        // effet, pour ne mesurer que le filtrage par couche.
        transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
            .with_scale(Vec3::new(0.5, 100.0, 2.0)),
        physics: PhysicsKind::Static,
        collision_layer: 0b010, // couche 2
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Missile".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(0.1)),
        physics: PhysicsKind::Dynamic,
        collision_mask: 0b101, // couches 1 et 3 — pas la couche 2 du mur
        ..Default::default()
    });
    let mut phys = Physics::build(&scene);
    phys.set_velocity(1, Vec3::new(3.0, 0.0, 0.0));
    for _ in 0..120 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let x = scene.objects[1].transform.position.x;
    assert!(
        x > 5.0,
        "un missile dont le masque exclut la couche du mur doit le traverser (x={x})"
    );
}

/// Contre-épreuve : sans réglage de masque (défaut = toutes les couches), le même
/// missile à la même vitesse est bloqué normalement par le mur.
#[test]
fn without_a_mask_the_same_projectile_collides_normally() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Mur".into(),
        mesh: crate::scene::MeshKind::Cube,
        // Très haut : cf. le commentaire équivalent dans le test précédent — sans
        // ça, la gravité ferait passer le missile sous un mur de hauteur normale
        // avant qu'il n'ait le temps de parcourir les 5 m à cette vitesse modeste.
        transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
            .with_scale(Vec3::new(0.5, 100.0, 2.0)),
        physics: PhysicsKind::Static,
        collision_layer: 0b010,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Missile".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(0.1)),
        physics: PhysicsKind::Dynamic,
        ..Default::default()
    });
    let mut phys = Physics::build(&scene);
    phys.set_velocity(1, Vec3::new(3.0, 0.0, 0.0));
    for _ in 0..120 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let x = scene.objects[1].transform.position.x;
    assert!(
        x < 5.0,
        "sans masque, le missile doit être bloqué normalement par le mur (x={x})"
    );
}

/// Index de l'objet pilotable (`controller.input`) dans la scène.
fn player_index(scene: &Scene) -> usize {
    scene
        .objects
        .iter()
        .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
        .expect("la démo contrôleur a un joueur pilotable")
}

#[test]
fn controller_demo_player_moves_with_joystick() {
    let mut scene = Scene::controller_demo();
    let p = player_index(&scene);
    let x0 = scene.objects[p].transform.position.x;

    let mut phys = Physics::build(&scene);
    // Joystick poussé vers +X (vx = move_speed) pendant ~0,5 s.
    for _ in 0..30 {
        phys.control(p, 4.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
        phys.step(1.0 / 60.0, &mut scene);
    }
    let x1 = scene.objects[p].transform.position.x;
    assert!(
        x1 > x0 + 0.3,
        "le joueur doit avancer en +X (x0={x0}, x1={x1})"
    );
}

#[test]
fn controller_demo_player_can_jump() {
    let mut scene = Scene::controller_demo();
    let p = player_index(&scene);
    let mut phys = Physics::build(&scene);
    // Laisse le joueur se poser au sol (gravité) avant de sauter.
    for _ in 0..40 {
        phys.control(p, 0.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
        phys.step(1.0 / 60.0, &mut scene);
    }
    let grounded_y = scene.objects[p].transform.position.y;
    // Impulsion de saut (vitesse pour ~1,6 m), puis on relâche.
    let jump_speed = (2.0 * 9.81 * 1.6_f32).sqrt();
    phys.control(p, 0.0, 0.0, true, jump_speed, 0.0, 1.0 / 60.0);
    let mut max_y = grounded_y;
    for _ in 0..25 {
        phys.control(p, 0.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
        phys.step(1.0 / 60.0, &mut scene);
        max_y = max_y.max(scene.objects[p].transform.position.y);
    }
    assert!(
        max_y > grounded_y + 0.3,
        "le joueur doit s'élever en sautant (sol={grounded_y}, max={max_y})"
    );
}

#[test]
fn controller_demo_player_collides_with_wall() {
    let mut scene = Scene::controller_demo();
    let p = player_index(&scene);
    // Le mur de pourtour Est est à x = 7.5 (demi-épaisseur 0.25 → face interne ~7.25).
    let mut phys = Physics::build(&scene);
    // Pousse fort vers +X pendant 3 s : sans mur il sortirait largement de l'aire.
    for _ in 0..180 {
        phys.control(p, 8.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
        phys.step(1.0 / 60.0, &mut scene);
    }
    let x = scene.objects[p].transform.position.x;
    assert!(
        x < 7.2,
        "le joueur doit être bloqué par le mur de pourtour (x≈7), mais x={x}"
    );
}

/// Scène « couloir » pour les corps scriptés (`PhysicsKind::Kinematic`) :
/// sol, mur fixe à x=+4 (face intérieure à 3.75), joueur pilotable (capsule,
/// donc vrai corps kinématique joueur) à x=−4, et un « marcheur » cubique au
/// centre dont les tests jouent le rôle du script Lua (écriture directe de
/// `transform.position`, exactement ce que fait `obj.x = …` côté Lua).
fn scripted_walker_scene(kind: PhysicsKind) -> Scene {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
            .with_scale(Vec3::new(20.0, 1.0, 20.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Mur".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(4.0, 1.0, 0.0))
            .with_scale(Vec3::new(0.5, 2.0, 4.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(-4.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Marcheur".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.5, 0.0)),
        physics: kind,
        ..Default::default()
    });
    scene
}

/// Preuve de la demande gameplay « les créatures ne doivent pas marcher sur
/// le joueur ni traverser murs et objets fixes » : un corps scripté
/// (`PhysicsKind::Kinematic`) dont le script force tout droit est bloqué par
/// le mur, puis par le joueur — sans grimper ni pousser personne.
#[test]
fn a_scripted_kinematic_body_cannot_walk_through_walls_or_the_player() {
    let mut scene = scripted_walker_scene(PhysicsKind::Kinematic);
    let mut phys = Physics::build(&scene);
    let dt = 1.0 / 60.0;
    let walker = 3;
    // 10 s plein est, vers le mur (le script demande 2 m/s quoi qu'il arrive).
    for _ in 0..600 {
        scene.objects[walker].transform.position.x += 2.0 * dt;
        phys.resolve_scripted_moves(dt, &mut scene);
        phys.step(dt, &mut scene);
    }
    let p = scene.objects[walker].transform.position;
    assert!(
        p.x > 2.0,
        "le marcheur doit avoir avancé librement (x={})",
        p.x
    );
    assert!(
        p.x < 3.3,
        "…mais être bloqué par le mur (face intérieure à 3.75, demi-cube 0.5, \
             arrêt attendu ≈ 3.25) : x={}",
        p.x
    );
    assert!(p.y < 1.0, "…sans grimper sur le mur (y={})", p.y);

    // 15 s plein ouest, vers le joueur (capsule en x=−4, rayon 0.5).
    for _ in 0..900 {
        scene.objects[walker].transform.position.x -= 2.0 * dt;
        phys.resolve_scripted_moves(dt, &mut scene);
        phys.step(dt, &mut scene);
    }
    let p = scene.objects[walker].transform.position;
    let joueur = scene.objects[2].transform.position;
    assert!(
        p.x - joueur.x > 0.6,
        "le marcheur doit buter sur le joueur, pas le pénétrer (demi-cube 0.5 \
             + rayon de la capsule : l'écart doit dépasser largement le demi-cube \
             seul) — marcheur x={}, joueur x={}",
        p.x,
        joueur.x
    );
    assert!(p.y < 1.0, "…sans marcher sur le joueur (y={})", p.y);
    assert!(
        (joueur.x + 4.0).abs() < 0.3,
        "un corps kinématique ne doit pas pousser le joueur (x={})",
        joueur.x
    );
}

/// Contre-épreuve : le même marcheur **sans** corps physique (`None`, l'état
/// des créatures avant `PhysicsKind::Kinematic`) traverse le mur comme si de
/// rien n'était — c'est bien le nouveau variant qui apporte le blocage.
#[test]
fn without_kinematic_physics_the_same_scripted_walker_passes_through_the_wall() {
    let mut scene = scripted_walker_scene(PhysicsKind::None);
    let mut phys = Physics::build(&scene);
    let dt = 1.0 / 60.0;
    let walker = 3;
    for _ in 0..600 {
        scene.objects[walker].transform.position.x += 2.0 * dt;
        phys.resolve_scripted_moves(dt, &mut scene);
        phys.step(dt, &mut scene);
    }
    let x = scene.objects[walker].transform.position.x;
    assert!(
        x > 5.0,
        "sans corps physique, rien ne devrait bloquer le marcheur (x={x})"
    );
}

/// Laisse le joueur (corps kinématique, Sprint 103b) se poser réellement au
/// sol (gravité + `move_shape`/snap au sol) avant de mesurer la maths de
/// `control` — à l'apparition, la capsule n'est pas encore en contact avec
/// le sol (`Scene::controller_demo` la fait tomber depuis y=1.0), et un
/// corps kinématique détecte l'air *réellement* (shapecast) là où l'ancien
/// corps dynamique se croyait « au sol » dès la première frame (heuristique
/// de vitesse, toujours vraie à vitesse nulle) — sans se poser d'abord, les
/// tests ci-dessous mesureraient l'autorité réduite de l'air (`AIR_CONTROL`)
/// au lieu de celle du sol.
fn settle_on_ground(phys: &mut Physics, scene: &mut Scene, p: usize) {
    let dt = 1.0 / 60.0;
    for _ in 0..40 {
        phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
        phys.step(dt, scene);
    }
}

#[test]
fn control_with_acceleration_ramps_up_instead_of_snapping_to_target() {
    let mut scene = Scene::controller_demo();
    let p = player_index(&scene);
    let mut phys = Physics::build(&scene);
    settle_on_ground(&mut phys, &mut scene, p);
    // Accélération de 4 m/s² : après un seul pas de 1/60 s, la vitesse ne doit
    // pas déjà valoir la cible (8 m/s) — contrairement à `accel = 0.0` (instantané).
    phys.control(p, 8.0, 0.0, false, 0.0, 4.0, 1.0 / 60.0);
    let vx = phys.velocity(p).unwrap().x;
    assert!(
        vx > 0.0 && vx < 8.0,
        "la vitesse doit monter progressivement, pas instantanément (vx={vx})"
    );
}

#[test]
fn control_brakes_harder_than_it_accelerates() {
    // Le freinage doit décélérer nettement plus vite (`BRAKE_FACTOR`) qu'une
    // accélération de même magnitude ne fait progresser la vitesse depuis
    // l'arrêt — arrêt net quand le joueur relâche, pas une glissade symétrique
    // du départ. Comparaison entre deux scénarios (plutôt qu'une formule
    // figée sur la vitesse absolue) : un corps kinématique subit un léger
    // frottement de contact avec le sol (`KinematicCharacterController`, sans
    // équivalent sur l'ancien corps dynamique) qui décale une vitesse absolue
    // exacte, mais affecte les deux scénarios de façon comparable — le
    // *ratio* freinage/accélération reste la grandeur fiable à tester.
    let mut scene = Scene::controller_demo();
    let p = player_index(&scene);
    let dt = 1.0 / 60.0;

    let mut phys_brake = Physics::build(&scene);
    settle_on_ground(&mut phys_brake, &mut scene, p);
    phys_brake.control(p, 8.0, 0.0, false, 0.0, 0.0, dt);
    let v1 = phys_brake.velocity(p).unwrap().x;
    phys_brake.control(p, 0.0, 0.0, false, 0.0, 20.0, dt);
    let brake_delta = v1 - phys_brake.velocity(p).unwrap().x;

    let mut scene2 = Scene::controller_demo();
    let mut phys_accel = Physics::build(&scene2);
    settle_on_ground(&mut phys_accel, &mut scene2, p);
    phys_accel.control(p, 8.0, 0.0, false, 0.0, 20.0, dt);
    let accel_delta = phys_accel.velocity(p).unwrap().x;

    assert!(
        brake_delta > accel_delta * (BRAKE_FACTOR * 0.75),
        "le freinage (Δ={brake_delta}) doit décélérer nettement plus vite \
             que l'accélération (Δ={accel_delta}) ne progresse (facteur \
             attendu ≈ {BRAKE_FACTOR})"
    );
}

#[test]
fn control_has_reduced_authority_in_the_air() {
    // En l'air (saut en cours), l'accélération horizontale doit être réduite à
    // `AIR_CONTROL` : la trajectoire d'un saut s'engage à l'impulsion, elle ne se
    // repilote pas librement comme au sol (effet « téléguidé » sinon).
    let scene = Scene::controller_demo();
    let p = player_index(&scene);
    let mut phys = Physics::build(&scene);
    let dt = 1.0 / 60.0;
    // Saut : vitesse verticale nette (5 m/s) → plus « au sol » pour l'appel suivant.
    phys.control(p, 0.0, 0.0, true, 5.0, 0.0, dt);
    phys.control(p, 8.0, 0.0, false, 0.0, 20.0, dt);
    let vx = phys.velocity(p).unwrap().x;
    let expected = 20.0 * AIR_CONTROL * dt;
    assert!(
        (vx - expected).abs() < 1e-4,
        "en l'air, l'accélération doit être ×{AIR_CONTROL} (vx={vx}, attendu={expected})"
    );
}

#[test]
fn control_makes_falling_faster_than_rising() {
    // Gravité renforcée en descente (`FALL_GRAVITY_FACTOR`) : un saut retombe
    // plus vite qu'il ne monte — saut vif et lisible, pas une parabole flottante.
    let mut scene = Scene::controller_demo();
    let p = player_index(&scene);
    let mut phys = Physics::build(&scene);
    let dt = 1.0 / 60.0;
    // Se pose au sol, saute, puis laisse la simulation courir jusqu'à la chute.
    for _ in 0..40 {
        phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
        phys.step(dt, &mut scene);
    }
    phys.control(p, 0.0, 0.0, true, 6.0, 0.0, dt);
    for _ in 0..200 {
        if phys.velocity(p).unwrap().y < -1.5 {
            break;
        }
        phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
        phys.step(dt, &mut scene);
    }
    let vy_before = phys.velocity(p).unwrap().y;
    assert!(
        vy_before < -1.5,
        "le joueur doit être en chute (vy={vy_before})"
    );
    // Un appel `control` seul (sans pas de simulation) doit appliquer la
    // gravité de chute renforcée en un seul coup (pas de solveur `step`
    // séparé pour un corps kinématique, cf. `control_kinematic`).
    phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
    let vy_after = phys.velocity(p).unwrap().y;
    let boost = 9.81 * FALL_GRAVITY_FACTOR * dt;
    assert!(
        (vy_before - vy_after - boost).abs() < 1e-3,
        "la chute doit être accélérée de {boost} m/s par pas (avant={vy_before}, après={vy_after})"
    );
}

#[test]
fn control_with_zero_acceleration_snaps_instantly_as_before() {
    let mut scene = Scene::controller_demo();
    let p = player_index(&scene);
    let mut phys = Physics::build(&scene);
    settle_on_ground(&mut phys, &mut scene, p);
    phys.control(p, 8.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
    let vx = phys.velocity(p).unwrap().x;
    assert!(
        (vx - 8.0).abs() < 0.05,
        "vx doit valoir la cible à peu près instantanément, vx={vx}"
    );
}

fn kinematic_player(pos: Vec3) -> SceneObject {
    SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(pos),
        controller: Some(crate::scene::Controller {
            input: true,
            move_speed: 2.0,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Livrable du Sprint 103b (« escalier montable ») : un escalier de 4 marches
/// de 20 cm de haut (sous `PLAYER_AUTOSTEP_HEIGHT` = 30 cm) doit être franchi
/// sans ralentir en butant contre chaque contremarche — la preuve que
/// `KinematicCharacterController::autostep` fait le travail que l'ancienne
/// heuristique de vitesse (`cur.y.abs() < 1.0`, sans aucune notion de forme du
/// sol) ne pouvait pas faire.
#[test]
fn kinematic_player_climbs_a_low_staircase() {
    const STEP_RISE: f32 = 0.2;
    const STEP_DEPTH: f32 = 0.6;
    const STEPS: i32 = 4;

    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -0.1, -1.5))
            .with_scale(Vec3::new(4.0, 0.2, 3.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    for k in 0..STEPS {
        let top = STEP_RISE * (k + 1) as f32;
        scene.objects.push(SceneObject {
            name: format!("Marche {k}"),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(
                0.0,
                top * 0.5,
                (k as f32 + 0.5) * STEP_DEPTH,
            ))
            .with_scale(Vec3::new(4.0, top, STEP_DEPTH)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
    }
    // Palier au sommet : sans lui, le joueur (avancée constante en +Z) finit
    // par dépasser le bord de la dernière marche et tombe dans le vide — ce
    // test vérifie l'ascension, pas la chute derrière l'escalier.
    let top_step = STEP_RISE * STEPS as f32;
    scene.objects.push(SceneObject {
        name: "Palier".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(
            0.0,
            top_step * 0.5,
            STEPS as f32 * STEP_DEPTH + 1.0,
        ))
        .with_scale(Vec3::new(4.0, top_step, 2.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene
        .objects
        .push(kinematic_player(Vec3::new(0.0, 1.0, -1.5)));
    let p = scene.objects.len() - 1;

    let mut phys = Physics::build(&scene);
    let dt = 1.0 / 60.0;
    for _ in 0..200 {
        phys.control(p, 0.0, 2.0, false, 0.0, 0.0, dt);
        phys.step(dt, &mut scene);
    }
    let pos = scene.objects[p].transform.position;
    assert!(
        pos.z > STEP_DEPTH * STEPS as f32 - 1.0,
        "le joueur doit avoir avancé jusqu'au sommet de l'escalier (z={})",
        pos.z
    );
    assert!(
        pos.y > top_step - STEP_RISE * 1.5,
        "le joueur doit être monté sur les marches (y={}, sommet={})",
        pos.y,
        top_step
    );
}

/// Décor en pente : un plan incliné statique (rotation autour de X), du bas
/// (z négatif, y≈0) vers le haut (z positif). `angle_deg` positif fait monter
/// la pente en +Z, direction dans laquelle le joueur avance dans les tests.
fn ramp_scene(angle_deg: f32) -> (Scene, usize) {
    let theta = -angle_deg.to_radians();
    let mut scene = Scene::default();
    // Sol plat avant le bas de la rampe (bord bas ≈ z=-2.7, cf. commentaire
    // ci-dessous) — sans lui, le joueur tombe dans le vide avant même
    // d'atteindre la rampe.
    scene.objects.push(SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -0.1, -4.5))
            .with_scale(Vec3::new(4.0, 0.2, 3.5)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Rampe".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform {
            position: Vec3::new(0.0, angle_deg.to_radians().sin() * 3.0, 0.0),
            rotation: Quat::from_rotation_x(theta),
            scale: Vec3::new(4.0, 0.2, 6.0),
        },
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    // Départ à plat, juste avant le bas de la rampe (bord bas ≈ z=-2.7 pour
    // une demi-longueur de 3 m, cf. calcul géométrique en commentaire du test).
    scene
        .objects
        .push(kinematic_player(Vec3::new(0.0, 1.0, -4.0)));
    let p = scene.objects.len() - 1;
    (scene, p)
}

/// Pente franchissable (25°, sous `PLAYER_MAX_SLOPE_CLIMB_DEG` = 50°) : le
/// joueur doit rester au contact et monter avec elle, pas rebondir/tunneler.
#[test]
fn kinematic_player_climbs_a_gentle_slope() {
    // 220 pas (~3,7 s) : le joueur atteint le haut de la rampe (~z=2,7) sans
    // la dépasser — au-delà, il marcherait dans le vide derrière la rampe,
    // ce que ce test ne vérifie pas.
    let (mut scene, p) = ramp_scene(25.0);
    let mut phys = Physics::build(&scene);
    let dt = 1.0 / 60.0;
    for _ in 0..220 {
        phys.control(p, 0.0, 2.0, false, 0.0, 0.0, dt);
        phys.step(dt, &mut scene);
    }
    let pos = scene.objects[p].transform.position;
    assert!(
        pos.y > 2.0,
        "le joueur doit avoir grimpé une pente franchissable (y={})",
        pos.y
    );
    assert!(
        pos.z > -1.0,
        "le joueur doit avoir avancé sur la pente (z={})",
        pos.z
    );
}

/// Contre-épreuve : une pente trop raide (65°, au-delà de
/// `PLAYER_MAX_SLOPE_CLIMB_DEG`/`PLAYER_MIN_SLOPE_SLIDE_DEG`) ne doit pas se
/// gravir comme la précédente — le joueur reste bloqué en bas / glisse,
/// loin de la hauteur atteinte sur la pente franchissable.
#[test]
fn kinematic_player_cannot_climb_a_steep_slope() {
    let (mut scene, p) = ramp_scene(65.0);
    let mut phys = Physics::build(&scene);
    let dt = 1.0 / 60.0;
    for _ in 0..360 {
        phys.control(p, 0.0, 2.0, false, 0.0, 0.0, dt);
        phys.step(dt, &mut scene);
    }
    let pos = scene.objects[p].transform.position;
    assert!(
        pos.y < 0.8,
        "une pente trop raide ne doit pas être gravie comme une pente \
             franchissable (y={})",
        pos.y
    );
}

/// Sol plat (index 0) + mur vertical (index 1), tous deux statiques — sert aux
/// tests de `raycast`/`overlap_sphere` (`QueryPipeline`).
fn ground_and_wall_scene() -> Scene {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
            .with_scale(Vec3::new(10.0, 1.0, 10.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Mur".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
            .with_scale(Vec3::new(0.5, 2.0, 2.0)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene
}

/// `raycast` doit trouver le collider le plus proche sur la trajectoire et
/// identifier l'objet touché — brique du « capteur de sol » (rayon vers le bas)
/// et du « cône de vision » (ligne de vue vers une cible).
#[test]
fn raycast_hits_the_nearest_collider_and_reports_its_object_index() {
    let scene = ground_and_wall_scene();
    let phys = Physics::build(&scene);
    // Vers le bas depuis 5 m au-dessus du sol (demi-épaisseur 0.5, face haute à
    // y=-0.5) : capteur de sol typique.
    let hit = phys
        .raycast(
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            100.0,
            u32::MAX,
        )
        .expect("le rayon vers le bas doit toucher le sol");
    assert_eq!(hit.index, Some(0), "doit identifier l'objet « Sol »");
    assert!(
        (hit.distance - 5.5).abs() < 0.05,
        "distance attendue ~5.5 m (dist={})",
        hit.distance
    );
    assert!(
        (hit.point.y - -0.5).abs() < 0.05,
        "le point d'impact doit être sur la face haute du sol (y={})",
        hit.point.y
    );

    // Vers +X depuis l'origine : ligne de vue vers le mur.
    let hit_wall = phys
        .raycast(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 100.0, u32::MAX)
        .expect("le rayon vers le mur doit toucher quelque chose");
    assert_eq!(hit_wall.index, Some(1), "doit identifier l'objet « Mur »");

    // Vers le haut : rien au-dessus des deux objets, aucun impact.
    assert!(
        phys.raycast(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), 100.0, u32::MAX)
            .is_none(),
        "un rayon vers le ciel ne doit rien toucher"
    );
}

/// La broad-phase de requête est mémoïsée entre deux mutations
/// (`with_query_broad_phase`/`invalidate_query_cache`) : un corps qui
/// **entre** dans la trajectoire du rayon pendant `step()` doit être vu par
/// le rayon suivant — un cache jamais invalidé garderait son AABB à
/// l'ancienne position et le rayon le raterait (faux « rien devant » pour
/// les sondes des créatures).
#[test]
fn raycast_sees_a_body_that_fell_into_the_ray_path_despite_the_query_cache() {
    let mut scene = ground_and_wall_scene();
    scene.objects.push(SceneObject {
        name: "Caisse".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(2.0, 5.0, 0.0)),
        physics: PhysicsKind::Dynamic,
        ..Default::default()
    });
    let mut phys = Physics::build(&scene);
    // Premier rayon (remplit le cache) : la caisse est en l'air, le rayon
    // horizontal à y=0 atteint le mur derrière elle.
    let first = phys
        .raycast(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 100.0, u32::MAX)
        .expect("le rayon doit toucher le mur");
    assert_eq!(first.index, Some(1), "caisse en l'air : le mur est touché");
    // 2 s de chute : la caisse se pose sur le sol (centre ≈ y=0), en travers
    // du rayon. Le rayon suivant doit la toucher, pas re-servir le monde
    // d'avant la chute.
    for _ in 0..120 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let second = phys
        .raycast(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 100.0, u32::MAX)
        .expect("le rayon doit toucher la caisse posée");
    assert_eq!(
        second.index,
        Some(2),
        "après step, la requête doit décrire le monde à jour (dist={})",
        second.distance
    );
}

/// Contre-épreuve : direction nulle → `None` sans diviser par zéro (`try_normalize`).
#[test]
fn raycast_with_a_zero_direction_returns_none_instead_of_panicking() {
    let scene = ground_and_wall_scene();
    let phys = Physics::build(&scene);
    assert!(
        phys.raycast(Vec3::ZERO, Vec3::ZERO, 100.0, u32::MAX)
            .is_none()
    );
}

/// `mask` doit filtrer les colliders par couche, mêmes bits que
/// `collision_layer`/`collision_mask` — un rayon ne doit toucher que les colliders
/// dont la couche recoupe le masque demandé.
#[test]
fn raycast_mask_filters_by_collision_layer() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Mur".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
            .with_scale(Vec3::new(0.5, 2.0, 2.0)),
        physics: PhysicsKind::Static,
        collision_layer: 0b010, // couche 2
        ..Default::default()
    });
    let phys = Physics::build(&scene);
    let origin = Vec3::ZERO;
    let dir = Vec3::new(1.0, 0.0, 0.0);
    assert!(
        phys.raycast(origin, dir, 100.0, 0b010).is_some(),
        "un masque incluant la couche du mur doit le toucher"
    );
    assert!(
        phys.raycast(origin, dir, 100.0, 0b101).is_none(),
        "un masque excluant la couche du mur ne doit rien toucher"
    );
}

/// `overlap_sphere` doit détecter les colliders à portée et ignorer ceux hors de
/// la sphère — brique du « cône de vision » (détection de proximité avant même de
/// tester l'angle/la ligne de vue).
#[test]
fn overlap_sphere_finds_colliders_within_radius_and_ignores_far_ones() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Proche".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.0, 0.0))
            .with_scale(Vec3::splat(0.2)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    scene.objects.push(SceneObject {
        name: "Loin".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(20.0, 0.0, 0.0))
            .with_scale(Vec3::splat(0.2)),
        physics: PhysicsKind::Static,
        ..Default::default()
    });
    let phys = Physics::build(&scene);

    let near_only = phys.overlap_sphere(Vec3::ZERO, 2.0, u32::MAX);
    assert_eq!(
        near_only,
        vec![0],
        "seul l'objet proche doit être détecté (trouvé={near_only:?})"
    );

    let mut both = phys.overlap_sphere(Vec3::ZERO, 25.0, u32::MAX);
    both.sort_unstable();
    assert_eq!(
        both,
        vec![0, 1],
        "un rayon suffisant doit détecter les deux objets (trouvé={both:?})"
    );
}

/// Même filtrage par couche que `raycast` (mêmes bits `collision_layer`/`mask`).
#[test]
fn overlap_sphere_mask_filters_by_collision_layer() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Capteur".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.0, 0.0))
            .with_scale(Vec3::splat(0.2)),
        physics: PhysicsKind::Static,
        collision_layer: 0b010, // couche 2
        ..Default::default()
    });
    let phys = Physics::build(&scene);
    assert_eq!(
        phys.overlap_sphere(Vec3::ZERO, 2.0, 0b010),
        vec![0],
        "un masque incluant la couche du capteur doit le détecter"
    );
    assert!(
        phys.overlap_sphere(Vec3::ZERO, 2.0, 0b101).is_empty(),
        "un masque excluant la couche du capteur ne doit rien détecter"
    );
}

/// Sprint 125 : zone de vent — un corps dynamique dont l'AABB touche celle d'une
/// zone `trigger` + `wind` doit dériver dans la direction du vent ; un corps hors
/// de la zone garde son comportement normal (chute verticale, pas de dérive
/// horizontale) — la preuve que la force est bien **locale** à la zone, pas globale.
#[test]
fn a_wind_zone_pushes_a_dynamic_body_only_while_inside_its_aabb() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Vent".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(10.0)),
        physics: PhysicsKind::None,
        trigger: true,
        wind: Some(Vec3::new(4.0, 0.0, 0.0)),
        ..Default::default()
    });
    let inside = drop_ball(&mut scene, "Dedans", 0.0, 0.0);
    let outside = drop_ball(&mut scene, "Dehors", 20.0, 0.0);
    let mut phys = Physics::build(&scene);
    for _ in 0..30 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let x_inside = scene.objects[inside].transform.position.x;
    let x_outside = scene.objects[outside].transform.position.x;
    assert!(
        x_inside > 0.5,
        "poussée par le vent attendue en x, x={x_inside}"
    );
    assert!(
        (x_outside - 20.0).abs() < 0.05,
        "hors de la zone, aucune dérive horizontale attendue, x={x_outside}"
    );
}

/// Contre-épreuve : sans `trigger`, un `wind` renseigné ne pousse personne — la
/// zone n'a alors aucun volume de détection (cohérent avec les autres zones,
/// `obj.triggered`).
#[test]
fn a_wind_zone_without_trigger_pushes_nobody() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Vent sans trigger".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(10.0)),
        physics: PhysicsKind::None,
        trigger: false,
        wind: Some(Vec3::new(4.0, 0.0, 0.0)),
        ..Default::default()
    });
    let ball = drop_ball(&mut scene, "Dedans", 0.0, 0.0);
    let mut phys = Physics::build(&scene);
    for _ in 0..30 {
        phys.step(1.0 / 60.0, &mut scene);
    }
    let x = scene.objects[ball].transform.position.x;
    assert!(x.abs() < 0.05, "aucune dérive horizontale attendue, x={x}");
}

/// Sprint 103c : une correction de position réseau (`set_position`) ne
/// doit pas laisser le joueur figé « au sol » un tick de plus après avoir
/// été téléporté en l'air — la gravité doit reprendre dès le prochain
/// `control`, pas seulement après un tick de retard (cf. le correctif de
/// `set_position`, qui remet `grounded` à `false`).
#[test]
fn set_position_does_not_trust_a_stale_grounded_state() {
    let mut scene = Scene::controller_demo();
    let p = player_index(&scene);
    let mut phys = Physics::build(&scene);
    let dt = 1.0 / 60.0;
    settle_on_ground(&mut phys, &mut scene, p);
    assert!(
        phys.velocity(p).unwrap().y.abs() < 1e-6,
        "posé au sol, la vitesse verticale doit être nulle avant le test"
    );

    // Téléporte loin en l'air, bien au-dessus de tout support — simule une
    // correction réseau qui déplace le joueur hors de portée du sol connu.
    phys.set_position(p, Vec3::new(0.0, 20.0, -6.0));
    phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
    let vy = phys.velocity(p).unwrap().y;
    assert!(
        vy < 0.0,
        "la gravité doit s'appliquer dès le premier `control` après une \
             téléportation, pas seulement au tick suivant (vy={vy})"
    );
}

/// Chantier 4.1 (audit 2026-07-20) : une créature kinématique **scriptée**
/// qui reçoit un `ai_chaser` doit rester un corps scripté (patrouille Lua
/// résolue par `resolve_scripted_moves`), PAS devenir un corps dynamique
/// piloté par vitesse — la chasse de ces créatures passe par la
/// réécriture de position, même canal que la patrouille.
#[test]
fn a_kinematic_scripted_object_with_ai_chaser_stays_a_scripted_body() {
    let mut scene = Scene::default();
    scene.objects.push(SceneObject {
        name: "Créature chasseuse scriptée".into(),
        mesh: crate::scene::MeshKind::Cube,
        physics: PhysicsKind::Kinematic,
        script: "obj.x = obj.x + dt".into(),
        ai_chaser: Some(crate::scene::AiChaser::default()),
        ..Default::default()
    });
    let phys = Physics::build(&scene);
    assert!(
        phys.is_scripted_body(0),
        "kinématique + script + ai_chaser = corps scripté (le scripté prime)"
    );

    // Contre-épreuve : le même chasseur SANS `PhysicsKind::Kinematic`
    // garde le comportement historique (corps dynamique contrôlé).
    let mut scene2 = Scene::default();
    scene2.objects.push(SceneObject {
        name: "Chasseur historique".into(),
        mesh: crate::scene::MeshKind::Cube,
        ai_chaser: Some(crate::scene::AiChaser::default()),
        ..Default::default()
    });
    let phys2 = Physics::build(&scene2);
    assert!(
        !phys2.is_scripted_body(0),
        "un chasseur non kinématique reste un corps contrôlé par vitesse"
    );
}
