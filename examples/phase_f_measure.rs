use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::scene::Scene;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const WARMUP: u32 = 5;
const SAMPLES: u32 = 60;

fn main() {
    let mut renderer = pollster::block_on(Renderer::new_headless(WIDTH, HEIGHT))
        .expect("pas de GPU headless disponible");

    let mut app = AppState::default();
    app.scene = Scene::mmorpg_demo();
    // Vue large/plongée, cf. Sprint 0 : distance haute + pitch marqué pour
    // englober le plus d'objets possible sans dépasser le plan éloigné (100 m,
    // OrbitCamera::view_proj_shaken) sur une arène de rayon MMORPG_HALF=36 m.
    app.camera.target = glam::Vec3::ZERO;
    app.camera.distance = 90.0;
    app.camera.yaw = 0.7;
    app.camera.pitch = 1.1;

    for _ in 0..WARMUP {
        renderer.render_scene_headless(&mut app, WIDTH, HEIGHT);
    }

    let start = std::time::Instant::now();
    for _ in 0..SAMPLES {
        renderer.render_scene_headless(&mut app, WIDTH, HEIGHT);
    }
    let elapsed = start.elapsed();
    let ms_per_frame = elapsed.as_secs_f64() * 1000.0 / SAMPLES as f64;
    let fps = 1000.0 / ms_per_frame;

    let (pass_timings, draw_calls) = renderer.gpu_profiler_info();
    let skinned_dropped = renderer.skinned_dropped_count();

    println!("resolution: {}x{}", WIDTH, HEIGHT);
    println!("objects total in scene: {}", app.scene.objects.len());
    println!("imported meshes: {}", app.scene.imported.len());
    println!("samples: {SAMPLES} (warmup {WARMUP})");
    println!(
        "headless render loop: {:.2} ms/frame -> {:.1} FPS (equiv.)",
        ms_per_frame, fps
    );
    println!("gpu_draw_calls: {}", draw_calls);
    println!("skinned_dropped: {}", skinned_dropped);
    println!("gpu_pass_timings_ms: {:?}", pass_timings);
}
