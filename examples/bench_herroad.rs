//! Mesures de HerRoad : temps de construction de la scène, objets, sommets, puis coût moyen
//! d'un rendu headless et d'un pas de course. Usage :
//! `cargo run --release --example bench_herroad`
use std::time::Instant;

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::racing::{Track, TrackSpec};
use motor3derust::racing::terrain::Terrain;

fn main() {
    let t = Instant::now();
    let track = Track::build(&TrackSpec::herroad());
    println!("circuit           : {:>7.1} ms", t.elapsed().as_secs_f32() * 1000.0);
    let t = Instant::now();
    let terrain = Terrain::build(&track);
    println!("relief (grille)   : {:>7.1} ms", t.elapsed().as_secs_f32() * 1000.0);
    let t = Instant::now();
    let mesh = terrain.mesh();
    println!("relief (maillage) : {:>7.1} ms ({} sommets)", t.elapsed().as_secs_f32() * 1000.0, mesh.vertices.len());
    let t = Instant::now();
    let scene = motor3derust::scene::Scene::herroad_demo();
    println!("scène complète    : {:>7.1} ms", t.elapsed().as_secs_f32() * 1000.0);
    let by_group = |g: &str| scene.objects.iter().filter(|o| o.group == g).count();
    println!(
        "objets : {} (forêt {}, tribunes {}, décor {}, voiture {})",
        scene.objects.len(),
        by_group("Forêt"),
        by_group("Tribunes"),
        by_group("Décor"),
        by_group("Voiture")
    );
    let tris: usize = scene.imported.iter().map(|m| m.data.indices.len() / 3).sum();
    println!("maillages importés : {} ({} triangles)", scene.imported.len(), tris);

    let (w, h) = (1280, 720);
    let mut renderer = pollster::block_on(Renderer::new_headless(w, h)).expect("GPU");
    let mut app = AppState::default();
    app.load_herroad_demo();
    app.camera.aspect = w as f32 / h as f32;
    app.playing = true;
    app.input_state.race.throttle = 1.0;
    for _ in 0..240 {
        app.race_step(1.0 / 60.0);
    }
    app.race_camera(1.0, 1.0);
    // Échauffement (chargement GPU des maillages).
    for _ in 0..3 {
        renderer.render_scene_headless(&mut app, w, h);
    }
    let n = 30;
    let t = Instant::now();
    for _ in 0..n {
        renderer.render_scene_headless(&mut app, w, h);
    }
    println!("rendu 1280x720    : {:>7.1} ms/image (headless, lecture pixels comprise)", t.elapsed().as_secs_f32() * 1000.0 / n as f32);
    let t = Instant::now();
    for _ in 0..600 {
        app.race_step(1.0 / 60.0);
    }
    println!("pas de course     : {:>7.3} ms/pas", t.elapsed().as_secs_f32() * 1000.0 / 600.0);
    let t = Instant::now();
    for _ in 0..600 {
        app.race_camera(1.0 / 60.0, 0.5);
    }
    println!("caméra            : {:>7.4} ms", t.elapsed().as_secs_f32() * 1000.0 / 600.0);
}
