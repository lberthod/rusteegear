// Force la recompilation quand les assets embarqués changent : `include_dir!`
// (src/assets.rs, bundle zstd) et `include_str!` (player_scene.json) figent les
// octets à la compilation mais n'émettent aucun signal de fraîcheur à Cargo —
// sans ces directives, modifier un asset produit un binaire silencieusement
// périmé tant qu'on n'a pas touché src/assets.rs à la main.
fn main() {
    println!("cargo:rerun-if-changed=assets/bundle");
    println!("cargo:rerun-if-changed=assets/player_scene.json");
}
