//! Génère le JSON de la démo « contrôleur » (joueur pilotable au joystick + saut).
//! Usage : `cargo run --example gen_controller_demo > assets/examples/demo_controleur.json`

fn main() {
    let scene = motor3derust::scene::Scene::controller_demo();
    let json = serde_json::to_string_pretty(&scene).expect("sérialisation de la scène");
    println!("{json}");
}
