//! Rendus headless de HerRoad (`docs/img/herroad_*.png`) : depart, route a pleine vitesse,
//! pont, virage, saut. Necessite un GPU. Usage :
//! `cargo run --example gen_herroad_preview --profile dev-fast`

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::racing::bot::Bot;

const W: u32 = 1280;
const H: u32 = 720;

fn shot(renderer: &mut Renderer, app: &mut AppState, name: &str) {
    app.race_camera(1.0, 1.0);
    let pixels = renderer.render_scene_headless(app, W, H);
    let out = format!("{}/docs/img/herroad_{name}.png", env!("CARGO_MANIFEST_DIR"));
    image::save_buffer(&out, &pixels, W, H, image::ColorType::Rgba8).unwrap();
    println!("ecrit : {out}");
}

fn main() {
    let mut renderer = pollster::block_on(Renderer::new_headless(W, H)).expect("GPU requis");
    let mut app = AppState::default();
    app.load_herroad_demo();
    app.camera.aspect = W as f32 / H as f32;
    app.playing = true;
    for _ in 0..30 {
        app.race_step(1.0 / 60.0);
    }
    shot(&mut renderer, &mut app, "depart");

    let mut bot = Bot::new(3);
    let marks = [(9.0_f32, "vitesse"), (17.5, "pont"), (26.0, "virage"), (38.0, "saut")];
    let mut next = 0;
    for _ in 0..(60 * 90) {
        let inp = {
            let s = app.race.as_ref().unwrap();
            bot.drive(&s.car, &s.track)
        };
        app.input_state.race.throttle = inp.throttle;
        app.input_state.race.brake = inp.brake;
        app.input_state.race.steer = inp.steer;
        app.race_step(1.0 / 60.0);
        let t = app.race.as_ref().unwrap().race.time;
        if next < marks.len() && t >= marks[next].0 {
            shot(&mut renderer, &mut app, marks[next].1);
            next += 1;
        }
        if next == marks.len() {
            break;
        }
    }
}
