//! Génère `docs/img/riviere_cascade_preview.png` : rendu headless de la démo
//! « Rivière & cascade » (`Scene::riviere_demo`), cadrée sur la cascade depuis
//! l'aval — la preuve visuelle du shader d'eau et de la forêt, versionnée à
//! côté de la doc (`docs/RIVIERE_CASCADE.md`).
//!
//! Usage : `cargo run --example gen_riviere_preview --profile dev-fast [-- <t>]`
//! (`t` = instant d'animation de l'eau en secondes, 3,7 par défaut ; nécessite
//! un GPU, même contrainte que les goldens de rendu).

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::scene::Scene;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

fn main() {
    let anim_time: f32 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(3.7);
    let out = std::env::args().nth(2).unwrap_or_else(|| {
        format!(
            "{}/docs/img/riviere_cascade_preview.png",
            env!("CARGO_MANIFEST_DIR")
        )
    });

    let mut renderer = pollster::block_on(Renderer::new_headless(WIDTH, HEIGHT))
        .expect("GPU requis pour générer la preview");
    renderer.set_anim_time(anim_time);
    let mut app = AppState::default();
    app.scene = Scene::riviere_demo();
    // Depuis la rive aval, à hauteur d'homme, regard vers la cascade (nord = -Z).
    app.camera.target = glam::Vec3::new(0.0, 0.6, -14.0);
    app.camera.distance = 30.0;
    app.camera.yaw = 0.14;
    app.camera.pitch = 0.09;
    let pixels = renderer.render_scene_headless(&mut app, WIDTH, HEIGHT);

    if let Some(parent) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    image::save_buffer(&out, &pixels, WIDTH, HEIGHT, image::ColorType::Rgba8)
        .expect("écriture de la preview");
    println!("Preview écrite : {out}");
}
