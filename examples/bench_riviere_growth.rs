//! Mesure headless du coût de `render_scene_headless` (passes ombre + réflexion
//! plus principale, bloom, tonemap et relecture) pendant qu'on fait grossir
//! artificiellement `scene.objects` avec des clones invisibles, pour imiter ce
//! qu'un pool d'affichage qui ne recyclerait jamais ses emplacements
//! produirait en session longue (cf. investigation « les FPS se dégradent
//! progressivement en session longue »).
//!
//! Objectif : vérifier par la mesure, pas seulement par la lecture de code, si
//! la croissance de `scene.objects` cause des pics ponctuels au
//! redimensionnement du buffer d'instances GPU (`Renderer::sync_objects`,
//! `models_capacity`, dans `src/gfx/renderer/sync.rs`), une dégradation
//! progressive du coût par frame à mesure que `n` grandit, ou les deux.
//!
//! Usage : `cargo run --example bench_riviere_growth --profile dev-fast`

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::scene::Scene;

fn main() {
    let (w, h) = std::env::var("BENCH_RES")
        .ok()
        .and_then(|v| {
            let mut it = v.split("x");
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .unwrap_or((1280u32, 720u32));
    let mut renderer = pollster::block_on(Renderer::new_headless(w, h)).expect("GPU");

    let mut app = AppState::default();
    app.scene = Scene::riviere_demo();
    app.camera.target = glam::Vec3::new(0.0, 0.6, -14.0);
    app.camera.distance = 30.0;
    app.camera.yaw = 0.14;
    app.camera.pitch = 0.09;

    // Objet-modèle à cloner pour simuler la croissance d'un pool de projectiles :
    // un objet quelconque déjà présent (mesh/texture déjà valides), qu'on déplace
    // hors champ et qu'on rend invisible — comme un projectile « épuisé » que les
    // pools actuels laissent trainer indéfiniment dans `scene.objects` au lieu de
    // le recycler.
    let template = app
        .scene
        .objects
        .iter()
        .find(|o| o.mesh == motor3derust::scene::MeshKind::Cube)
        .cloned()
        .unwrap_or_else(|| app.scene.objects[0].clone());

    // Chauffe (compilation pipelines, premier upload) hors mesure.
    renderer.render_scene_headless(&mut app, w, h);

    println!("Départ : {} objets", app.scene.objects.len());
    println!("n_objets  ms/frame   note");

    let mut worst = 0.0f32;
    let mut worst_n = 0usize;
    let target_counts: Vec<usize> = (1..=40).map(|k| k * 100).collect(); // jusqu'à 4000 objets, par pas de 100

    let mut next_target_idx = 0;
    // Ajoute les objets un par un (comme un pool qui grossit tir après tir) et
    // mesure CHAQUE frame individuellement pour capter un pic ponctuel au
    // redimensionnement du buffer d'instances, pas seulement une moyenne qui le lisserait.
    let mut last_cap_note = String::new();
    let mut prev_ms = 0.0f32;
    for i in 0..6000u32 {
        let mut obj = template.clone();
        obj.name = format!("pool_clone_{i}");
        obj.visible = false; // « épuisé » — jamais affiché, comme un projectile mort non recyclé
        obj.transform.position = glam::Vec3::new(1.0e6, 1.0e6, 1.0e6); // hors champ, par sécurité
        app.scene.objects.push(obj);

        let t = std::time::Instant::now();
        renderer.render_scene_headless(&mut app, w, h);
        let ms = t.elapsed().as_secs_f32() * 1000.0;

        if ms > worst {
            worst = ms;
            worst_n = app.scene.objects.len();
        }

        let n = app.scene.objects.len();
        let mut note = String::new();
        // Capacité du buffer d'instances : puissance de deux >= n (mini 64), cf.
        // `Renderer::sync_objects` — le redimensionnement/recréation a lieu exactement
        // quand `n` dépasse la capacité courante.
        let cap = n.next_power_of_two().max(64);
        let cap_note = format!("cap={cap}");
        if cap_note != last_cap_note {
            note = format!("<- franchit une capacité de buffer ({cap_note})");
            last_cap_note = cap_note;
        }
        if !note.is_empty() {
            println!("{n:8}  {ms:8.3}  (frame précédente : {prev_ms:.3} ms)  {note}");
        }
        prev_ms = ms;

        if next_target_idx < target_counts.len() && n >= target_counts[next_target_idx] {
            println!("  -- checkpoint {n:5} objets : {ms:.3} ms cette frame");
            next_target_idx += 1;
        }
    }

    println!("\nPire frame sur toute la croissance : {worst:.3} ms à n={worst_n} objets");
}
