//! Génère `docs/img/reeducation_preview.png` : rendu headless de la démo
//! « Rééducation — mobilité guidée » (`Scene::reeducation_demo`, portage de
//! Mouvéo) en cours de séance (mode démo, joystick), pour `docs/REEDUCATION.md`
//! — même mécanique que `gen_first_game_preview`.
//!
//! Usage : `cargo run --example gen_reeduc_preview` (nécessite un GPU — même
//! contrainte que les goldens de rendu). Le HUD egui n'est pas rendu en headless :
//! l'image montre l'avatar, la cible et le point suivi, pas les textes.

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;

const WIDTH: u32 = 960;
const HEIGHT: u32 = 640;

/// Un tour de boucle « temps réel » : `advance_play` intègre le temps écoulé
/// depuis le tour précédent (pas fixes à 60 Hz), d'où la petite attente.
fn tick(app: &mut AppState) {
    std::thread::sleep(std::time::Duration::from_millis(16));
    app.advance_play();
}

fn pos(app: &AppState, name: &str) -> glam::Vec3 {
    app.scene
        .objects
        .iter()
        .find(|o| o.name == name)
        .map(|o| o.transform.position)
        .unwrap_or_default()
}

fn main() {
    let mut renderer = pollster::block_on(Renderer::new_headless(WIDTH, HEIGHT))
        .expect("GPU requis pour générer la preview (même contrainte que les goldens)");
    let mut app = AppState::new();
    app.load_reeducation_demo();
    app.playing = true;
    tick(&mut app);
    // `REEDUC_AVATAR=0|1|2` (bâtons / héros / ninja) : le bouton « Avatar »
    // cycle héros → ninja → bâtons, on le presse autant de fois que nécessaire.
    let presses = match std::env::var("REEDUC_AVATAR").as_deref() {
        Ok("2") => 1,
        Ok("0") => 2,
        _ => 0,
    };
    for _ in 0..presses {
        app.push_hud_event("avatar");
        tick(&mut app);
    }
    app.push_hud_event("demarrer");
    tick(&mut app);
    // Deux répétitions jouées, puis le point suivi s'arrête à mi-chemin de la
    // 3ᵉ cible : cible, halo, avatar et point suivi tous visibles.
    let mut hits = 0.0;
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(12) {
        let target = pos(&app, "Cible");
        let hand = pos(&app, "Point suivi");
        app.input_state.joy.0 =
            (app.input_state.joy.0 + 0.2 * (target.x - hand.x)).clamp(-1.0, 1.0);
        app.input_state.joy.1 =
            (app.input_state.joy.1 + 0.2 * (target.y - hand.y)).clamp(-1.0, 1.0);
        tick(&mut app);
        hits = app.script_var("rd_hits").unwrap_or(0.0);
        if hits >= 2.0 && app.script_var("rd_phase").unwrap_or(0.0) == 0.0 {
            break;
        }
    }
    app.input_state.joy = (0.6, 0.55);
    for _ in 0..20 {
        tick(&mut app);
    }
    println!("répétitions jouées avant capture : {hits}");
    let pixels = renderer.render_scene_headless(&mut app, WIDTH, HEIGHT);
    let out = std::env::var("REEDUC_PREVIEW_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("docs/img/reeducation_preview.png")
        });
    image::save_buffer(&out, &pixels, WIDTH, HEIGHT, image::ColorType::Rgba8)
        .expect("écriture de reeducation_preview.png");
    println!("Preview écrite : {}", out.display());
}
