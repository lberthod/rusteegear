//! HerRoad — jeu de course arcade façon Trackmania : `cargo run --release --bin herroad`.
//! Lance la démo HerRoad en plein écran joueur (manette Switch Pro/Joy-Con, Xbox, PlayStation
//! ou clavier), sans l'éditeur.

fn main() {
    motor3derust::run_demo("herroad");
}
