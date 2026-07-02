//! Génère le JSON de la scène **exemple** des composants optionnels (Controller,
//! AudioSource, Combat) — une scène minimale et commentée, pas un niveau de jeu.
//! Usage : `cargo run --example gen_components_demo > assets/examples/demo_composants.json`

fn main() {
    let scene = motor3derust::scene::Scene::components_demo();
    let json = serde_json::to_string_pretty(&scene).expect("sérialisation de la scène");
    println!("{json}");
}
