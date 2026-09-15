//! Mesure headless du coût de `sim_step` (scripts Lua vs physique rapier3d)
//! sur la démo Rivière & cascade, en conditions qui se rapprochent d'une
//! session longue : joueur en mouvement, tirs de boss répétés (fait grossir
//! les pools d'affichage `boss.shot_pool`/`boss2`/créatures comme en jeu
//! réel, cf. `sync_boss_shot_pool`).
//!
//! Usage : `cargo run --example bench_sim_riviere --profile dev-fast`

use motor3derust::app::AppState;
use motor3derust::scene::Scene;

const FIXED_DT: f32 = 1.0 / 60.0;

fn main() {
    let mut app = AppState::default();
    app.scene = Scene::riviere_demo();
    app.playing = true;
    app.advance_play(); // construit la physique, entre en Play

    println!(
        "Scène de départ : {} objets ({} scriptés)",
        app.scene.objects.len(),
        app.scene.objects.iter().filter(|o| !o.script.trim().is_empty()).count()
    );

    // Simule ~90 s de jeu (5400 pas fixes à 60 Hz) : le joueur avance vers le
    // boss (déclenche sa mécanique de tir réelle) pour laisser les pools
    // d'affichage grossir comme lors d'une vraie session, sans tricher sur le
    // chemin de code emprunté.
    app.input_state.key_thrust = 1.0;

    let mut worst_scripts = 0.0f32;
    let mut worst_physics = 0.0f32;
    let mut worst_total = 0.0f32;
    let mut sum_scripts = 0.0f64;
    let mut sum_physics = 0.0f64;
    let n = 5400u32;
    let checkpoints = [900u32, 1800, 2700, 3600, 4500, 5399];

    for step in 0..n {
        app.advance_steps(1);
        let (scripts_ms, physics_ms) = app.sim_perf_ms();
        worst_scripts = worst_scripts.max(scripts_ms);
        worst_physics = worst_physics.max(physics_ms);
        worst_total = worst_total.max(scripts_ms + physics_ms);
        sum_scripts += scripts_ms as f64;
        sum_physics += physics_ms as f64;
        if checkpoints.contains(&step) {
            println!(
                "t={:5.1}s  objets={:5}  scripts={:.3}ms  physique={:.3}ms",
                step as f32 * FIXED_DT,
                app.scene.objects.len(),
                scripts_ms,
                physics_ms,
            );
        }
    }

    println!("\n--- Bilan sur {n} pas ({:.1} s simulées) ---", n as f32 * FIXED_DT);
    println!(
        "objets finaux : {} (scriptés : {})",
        app.scene.objects.len(),
        app.scene.objects.iter().filter(|o| !o.script.trim().is_empty()).count()
    );
    println!(
        "scripts  : moy {:.3} ms, pire {:.3} ms",
        sum_scripts / n as f64,
        worst_scripts
    );
    println!(
        "physique : moy {:.3} ms, pire {:.3} ms",
        sum_physics / n as f64,
        worst_physics
    );
    println!("scripts+physique, pire pas : {worst_total:.3} ms");
}
