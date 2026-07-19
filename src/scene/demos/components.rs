use super::*;

impl Scene {
    /// Scène **exemple**, minimale et commentée : montre les 3 composants optionnels
    /// (`Controller`, `AudioSource`, `Combat`) chacun sur un seul objet, sans le décor
    /// dense d'un vrai niveau. Sert de référence rapide pour qui étend le moteur — pas
    /// une démo de gameplay comme les autres (arène/tour/course).
    pub fn components_demo() -> Self {
        // Sol minimal (juste assez pour marcher/sauter).
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(10.0, 1.0, 10.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.4, 0.45, 0.5];

        // --- Controller : rend un objet pilotable (joystick + saut + attaque). `None`
        // pour tous les autres objets de cette scène — un seul joueur en a besoin.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(-2.5, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            attack_button: "Attaque".into(),
            attack_range: 1.5,
            ..Default::default()
        });

        // --- AudioSource : son associé à un objet (clip/autoplay/spatialisation). Le
        // clip est vide ici (aucun fichier son fourni avec l'exemple) — assigne-en un
        // via l'inspecteur (panneau Audio › Choisir un son…) pour l'entendre en Play.
        let mut boite = demo_obj("Boîte à musique", MeshKind::Cube, Vec3::new(0.0, 0.5, 2.0));
        boite.color = [0.6, 0.4, 0.8];
        boite.audio = Some(AudioSource {
            clip: String::new(),
            autoplay: true,
            spatial: true,
            ..Default::default()
        });

        // --- Combat : cible d'attaque (`attackable`) et ancre visuelle de l'effet
        // d'impact (`is_attack_fx`), rarement sur le même objet (ici, deux objets
        // séparés). Approche le joueur et appuie sur Attaque (ou touche J) pour tester.
        let mut cible = demo_obj(
            "Cible d'entraînement",
            MeshKind::Sphere,
            Vec3::new(2.5, 1.0, 0.0),
        );
        cible.color = [0.85, 0.15, 0.15];
        cible.emissive = 0.4;
        cible.combat = Some(Combat {
            attackable: true,
            ..Default::default()
        });
        cible.respawn_delay = 3.0;

        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        Scene {
            objects: vec![sol, joueur, boite, cible, fx],
            camera_follow: true,
            point_lights: vec![PointLight {
                position: [0.0, 5.0, 0.0],
                color: [1.0, 0.95, 0.85],
                intensity: 1.2,
                range: 14.0,
                ..PointLight::default()
            }],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
