use super::*;

pub(super) fn add_walls_and_landmarks(objects: &mut Vec<SceneObject>, half: f32) {
    // Murs de pourtour (enferment l'aire de jeu, ne servent qu'à ne pas tomber).
    let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
        let mut w = demo_obj(name, MeshKind::Cube, pos);
        w.transform = w.transform.with_scale(scale);
        w.physics = PhysicsKind::Static;
        w.color = [0.3, 0.32, 0.38];
        objects.push(w);
    };
    wall(
        "Mur Nord",
        Vec3::new(0.0, 0.9, -half),
        Vec3::new(2.0 * half, 1.8, 0.5),
    );
    wall(
        "Mur Sud",
        Vec3::new(0.0, 0.9, half),
        Vec3::new(2.0 * half, 1.8, 0.5),
    );
    wall(
        "Mur Est",
        Vec3::new(half, 0.9, 0.0),
        Vec3::new(0.5, 1.8, 2.0 * half),
    );
    wall(
        "Mur Ouest",
        Vec3::new(-half, 0.9, 0.0),
        Vec3::new(0.5, 1.8, 2.0 * half),
    );

    // Repères visuels statiques (juste pour situer les déplacements, sans danger)
    // aux quatre coins de la prairie centrale.
    for (n, (x, z)) in [
        (-15.0_f32, -15.0),
        (15.0, -15.0),
        (-15.0, 15.0),
        (15.0, 15.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut repere = demo_obj(
            &format!("Repère {}", n + 1),
            MeshKind::Cylinder,
            Vec3::new(x, 0.9, z),
        );
        repere.transform = repere.transform.with_scale(Vec3::new(1.0, 1.8, 1.0));
        repere.physics = PhysicsKind::Static;
        // Gris-vert mousseux (cohérent avec `nature_rock`/`nature_moss_boulder`,
        // cf. leurs `baseColorFactor` ~0.44/0.44/0.42 et ~0.30/0.48/0.20) — l'ancien
        // mauve/lavande ([0.5, 0.45, 0.62]) jurait avec la palette naturelle,
        // d'autant plus visible que le Repère du coin (-15, 15) tombe juste sur
        // la rive ouest du lac (capture en jeu : « rocher rose » signalé là).
        repere.color = [0.42, 0.43, 0.36];
        objects.push(repere);
    }

    // Zone de vent (Sprint 125, preuve d'implémentation visible dans une démo
    // jouée réellement plutôt que seulement en test unitaire) : plate-forme basse
    // et distinctement teintée (cyan) dans un coin de l'arène — traverser son AABB
    // pousse tout corps dynamique le long de `wind`, tant qu'il y reste. Pas de
    // collider (`PhysicsKind::None`) : une zone de vent ne doit rien bloquer,
    // seulement pousser.
    // Placée sur le « col venté » entre la prairie centrale et le promontoire
    // rocheux de l'est.
    let mut vent = demo_obj("Zone de vent", MeshKind::Cube, Vec3::new(14.0, 0.3, -4.0));
    vent.transform = vent.transform.with_scale(Vec3::new(8.0, 0.6, 8.0));
    vent.physics = PhysicsKind::None;
    vent.trigger = true;
    vent.wind = Some(Vec3::new(5.0, 0.0, 0.0));
    vent.color = [0.25, 0.75, 0.85];
    objects.push(vent);
}
