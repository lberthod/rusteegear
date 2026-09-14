//! Planche d'aperçu des modèles générés par `scripts/gen_riviere_vegetation.py`
//! (`assets/models/riviere/*.glb`), rendus en ligne, headless — pour juger de
//! chaque asset sans lancer l'éditeur.
//!
//! Usage : `cargo run --example gen_riviere_assets_preview --profile dev-fast [-- <sortie.png>]`

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::scene::{ImportedMesh, Light, MeshKind, Scene, SceneObject, Sky, Transform};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 700;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        format!(
            "{}/docs/img/riviere_assets_preview.png",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    let names = [
        ("epicea_a", 1.0),
        ("epicea_lod", 1.0),
        ("pin_a", 1.0),
        ("hetre_a", 1.0),
        ("hetre_lod", 1.0),
        ("bouleau_a", 1.0),
        ("rocher_a", 3.0),
        ("rocher_b", 3.0),
        ("galet_a", 3.0),
        ("tronc_mort", 2.0),
        ("fougere", 4.0),
        ("herbe_touffe", 4.0),
        ("herbe_haute", 4.0),
    ];
    let mut scene = Scene::default();
    let mut sol = SceneObject {
        name: "Sol".into(),
        mesh: MeshKind::Plane,
        color: [0.12, 0.16, 0.08],
        ..Default::default()
    };
    sol.transform = Transform::from_pos(glam::Vec3::new(24.0, 0.0, 0.0))
        .with_scale(glam::Vec3::new(90.0, 1.0, 40.0));
    scene.objects.push(sol);
    let mut x = 0.0;
    for (name, scale) in names {
        let path = format!(
            "{}/assets/models/riviere/{name}.glb",
            env!("CARGO_MANIFEST_DIR")
        );
        let (data, aabb_min, aabb_max) =
            motor3derust::scene::import::load_gltf(&path).expect("asset généré");
        let width = (aabb_max.x - aabb_min.x) * scale;
        x += width * 0.5 + 0.8;
        let mut mesh = ImportedMesh {
            path,
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        mesh.load_skinning();
        scene.imported.push(mesh);
        let idx = (scene.imported.len() - 1) as u32;
        let mut o = SceneObject {
            name: name.into(),
            mesh: MeshKind::Imported(idx),
            ..Default::default()
        };
        o.transform =
            Transform::from_pos(glam::Vec3::new(x, 0.0, 0.0)).with_scale(glam::Vec3::splat(scale));
        scene.objects.push(o);
        x += width * 0.5;
    }
    scene.light = Light {
        dir: [0.5, 0.7, 0.4],
        color: [1.0, 0.95, 0.85],
        ambient: 0.38,
    };
    scene.sky = Sky {
        horizon_color: [0.7, 0.75, 0.82],
        zenith_color: [0.3, 0.5, 0.8],
        fog_color: [0.7, 0.75, 0.82],
        fog_density: 0.0,
        ..Sky::default()
    };
    let mut renderer = pollster::block_on(Renderer::new_headless(WIDTH, HEIGHT)).expect("GPU");
    let mut app = AppState::default();
    app.scene = scene;
    app.camera.target = glam::Vec3::new(x * 0.5, 4.0, 0.0);
    app.camera.distance = x * 0.62;
    app.camera.yaw = 0.0;
    app.camera.pitch = 0.12;
    let pixels = renderer.render_scene_headless(&mut app, WIDTH, HEIGHT);
    image::save_buffer(&out, &pixels, WIDTH, HEIGHT, image::ColorType::Rgba8).expect("png");
    println!("Planche écrite : {out}");
}
