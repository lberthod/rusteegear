//! Benchmark headless de la démo Rivière & cascade : temps de rendu moyen
//! (passes ombre + réflexion + principale + bloom + tonemap + relecture) selon
//! ce qu'on retire de la scène — pour savoir où va le budget GPU.
//!
//! Usage : `cargo run --example bench_riviere --profile dev-fast`

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::scene::Scene;

type Variant = (&'static str, Box<dyn Fn(&mut Scene)>);

fn main() {
    let (w, h) = std::env::var("BENCH_RES")
        .ok()
        .and_then(|v| {
            let mut it = v.split("x");
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .unwrap_or((1280u32, 720u32));
    let mut renderer = pollster::block_on(Renderer::new_headless(w, h)).expect("GPU");
    let base = Scene::riviere_demo();
    let variants: [Variant; 6] = [
        ("complète", Box::new(|_| {})),
        (
            "sans arbres",
            Box::new(|s| s.objects.retain(|o| o.group != "Arbre")),
        ),
        (
            "sans sous-bois/berges",
            Box::new(|s| {
                s.objects
                    .retain(|o| o.group != "Sous-bois" && o.group != "Berge")
            }),
        ),
        (
            "sans eau (ni réflexion)",
            Box::new(|s| s.objects.retain(|o| o.water.is_none())),
        ),
        (
            "sans terrain",
            Box::new(|s| s.objects.retain(|o| o.name != "Vallée")),
        ),
        (
            "arbres seulement",
            Box::new(|s| s.objects.retain(|o| o.group == "Arbre")),
        ),
    ];
    for (name, f) in variants.iter() {
        let mut scene = base.clone();
        f(&mut scene);
        let mut app = AppState::default();
        app.scene = scene;
        app.camera.target = glam::Vec3::new(0.0, 0.6, -14.0);
        app.camera.distance = 30.0;
        app.camera.yaw = 0.14;
        app.camera.pitch = 0.09;
        renderer.render_scene_headless(&mut app, w, h);
        let t = std::time::Instant::now();
        let n = 4;
        for _ in 0..n {
            renderer.render_scene_headless(&mut app, w, h);
        }
        let ms = t.elapsed().as_secs_f32() * 1000.0 / n as f32;
        println!(
            "{name:28} {:6} objets  {ms:7.1} ms/frame",
            app.scene.objects.len()
        );
    }
}
