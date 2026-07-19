//! Génère `examples/first_game/preview.png` : rendu headless de la scène
//! exemple (Phase B, sprint.19matin.md) — la preuve visuelle « scène lisible
//! d'un coup d'œil », versionnée à côté du JSON comme les previews de
//! créatures dans `assets/models/`.
//!
//! Usage : `cargo run --example gen_first_game_preview --profile dev-fast`
//! (nécessite un GPU — même contrainte que les goldens de rendu.)

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::scene::Scene;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/first_game");
    let scene =
        Scene::load(dir.join("scene.json").to_str().unwrap()).expect("scene.json doit charger");

    let mut renderer = pollster::block_on(Renderer::new_headless(WIDTH, HEIGHT))
        .expect("GPU requis pour générer la preview (même contrainte que les goldens)");
    let mut app = AppState::default();
    app.scene = scene;
    // Cadrage d'ensemble : vue plongeante centrée, assez loin pour voir sol,
    // joueur, caisses, cube tournant, zone et pièces d'un coup d'œil.
    app.camera.target = glam::Vec3::new(0.0, 0.0, 0.5);
    app.camera.distance = 22.0;
    app.camera.yaw = 0.6;
    app.camera.pitch = 0.9;
    let pixels = renderer.render_scene_headless(&mut app, WIDTH, HEIGHT);

    let out = dir.join("preview.png");
    image::save_buffer(&out, &pixels, WIDTH, HEIGHT, image::ColorType::Rgba8)
        .expect("écriture de preview.png");
    println!("Preview écrite : {}", out.display());
}
