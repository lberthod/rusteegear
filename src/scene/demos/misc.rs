use super::*;

impl Scene {
    /// Démo « gameplay complet » : joueur (joystick + gyroscope + saut + vibration),
    /// zone de danger qui retire de la vie (HUD), et cube tactile qui change de couleur.
    /// Montre toute l'API de script en une scène jouable.
    pub fn gameplay_demo() -> Self {
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(16.0, 1.0, 16.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.35, 0.5, 0.4];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 0.5, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.script = "\
local s = 4.0
obj.x = obj.x + (input.jx + tilt.x) * s * dt
obj.z = obj.z - (input.jy + tilt.y) * s * dt
if input.btn.Saut then obj.y = 1.4; vibrate(40) else obj.y = 0.5 end"
            .into();

        let mut danger = demo_obj("Zone danger", MeshKind::Cube, Vec3::new(3.0, 0.5, 0.0));
        danger.color = [0.8, 0.2, 0.2];
        danger.emissive = 0.3;
        danger.trigger = true;
        danger.script = "\
if obj.triggered then set_health(0.25); vibrate(120) else set_health(1.0) end"
            .into();

        let mut bouton = demo_obj("Cube couleur", MeshKind::Cube, Vec3::new(-3.0, 0.5, 0.0));
        bouton.color = [0.3, 0.6, 0.9];
        bouton.tappable = true;
        bouton.script = "\
if obj.tapped then
  obj.r = (time * 0.7) % 1.0; obj.g = (time * 1.3) % 1.0; obj.b = (time * 1.9) % 1.0
end"
        .into();

        Scene {
            objects: vec![sol, joueur, danger, bouton],
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
        }
    }
    /// Scène **embarquée dans le binaire** (figée à la compilation depuis
    /// `assets/player_scene.json`, réécrite à chaque export). C'est le jeu que joue
    /// le mode Player d'un `.dmg`/`.apk`/`.ipa` exporté.
    pub fn embedded_player() -> Self {
        const JSON: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/player_scene.json"
        ));
        match serde_json::from_str::<Scene>(JSON) {
            Ok(mut s) => {
                s.reload_imported();
                s
            }
            Err(e) => {
                log::error!("Scène embarquée invalide ({e}) — retour à la démo.");
                Scene::demo()
            }
        }
    }

    /// Scène de démonstration : un sol, un cube, une sphère.
    pub fn demo() -> Self {
        Scene {
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls::default(),
            camera_follow: false,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                        .with_scale(Vec3::new(10.0, 1.0, 10.0)),
                    mesh: MeshKind::Plane,
                    script: String::new(),
                    physics: PhysicsKind::Static,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Cube".into(),
                    transform: Transform::from_pos(Vec3::new(-1.2, -0.5, 0.0)),
                    mesh: MeshKind::Cube,
                    // exemple : tourne autour de Y à 60°/s en mode Play
                    script: "obj.ry = obj.ry + dt * 60.0".into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Sphère".into(),
                    transform: Transform::from_pos(Vec3::new(1.2, 2.5, 0.0)),
                    mesh: MeshKind::Sphere,
                    script: String::new(),
                    // tombe et rebondit sur le sol en mode Play
                    physics: PhysicsKind::Dynamic,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
            ],
        }
    }

    /// Démo mobile « prête à jouer » : un sol, un personnage piloté au joystick
    /// (avec saut au bouton) et contrôles tactiles activés. Démontre toute la
    /// boucle joystick → script → rendu en mode Play.
    pub fn mobile_demo() -> Self {
        let player_script = "\
local speed = 4.0
obj.x = obj.x + input.jx * speed * dt
obj.z = obj.z - input.jy * speed * dt
if input.btn.Saut then obj.y = 1.4 else obj.y = 0.5 end";
        Scene {
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.0, 0.0))
                        .with_scale(Vec3::new(14.0, 1.0, 14.0)),
                    mesh: MeshKind::Plane,
                    script: String::new(),
                    physics: PhysicsKind::Static,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.4, 0.5, 0.45],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Joueur".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.5, 0.0)),
                    mesh: MeshKind::Capsule,
                    script: player_script.into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.95, 0.6, 0.25],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Bouton couleur".into(),
                    transform: Transform::from_pos(Vec3::new(2.5, 0.5, -1.0)),
                    mesh: MeshKind::Cube,
                    // Tap → couleur aléatoire (changeante) via le temps.
                    script: "if obj.tapped then\n  obj.r = (time * 0.7) % 1.0\n  obj.g = (time * 1.3) % 1.0\n  obj.b = (time * 1.9) % 1.0\nend".into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.3, 0.6, 0.9],
                    texture: String::new(),
                    tappable: true,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
            ],
        }
    }
}
