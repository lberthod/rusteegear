//! Preuves de chargement des packs flore (gen_flora_pack{,2,_animated}.py) :
//! chaque GLB passe par le vrai importeur du moteur — géométrie non vide,
//! base posée au sol (y≈0, repère jeu Y-up), et pour les variantes animées
//! `*_sway` : squelette présent + clip « Idle » non vide.

use motor3derust::scene::import::{load_gltf, load_gltf_clips, load_gltf_skeleton};

const STATIC_FLORA: &[&str] = &[
    // gen_flora_pack.py
    "nature_birch",
    "nature_willow",
    "nature_cherry_blossom",
    "nature_dead_tree",
    "nature_apple_tree",
    "nature_bamboo",
    "nature_mushrooms",
    "nature_sunflowers",
    "nature_lavender",
    "nature_berry_bush",
    // gen_flora_pack2.py
    "nature_oak",
    "nature_maple_autumn",
    "nature_cypress",
    "nature_olive",
    "nature_pine_parasol",
    "nature_pumpkins",
    "nature_wheat",
    "nature_bramble",
    "nature_daisies",
    "nature_mossy_log",
    // gen_flora_pack3.py
    "nature_poplar",
    "nature_ginkgo",
    "nature_magnolia",
    "nature_hazel",
    "nature_plum",
    "nature_topiary",
    "nature_cattails",
    "nature_thistle",
    "nature_tomatoes",
    "nature_cabbages",
    // gen_flora_pack4.py
    "nature_sequoia",
    "nature_palm",
    "nature_holly",
    "nature_wisteria_arch",
    "nature_vine_trellis",
    "nature_corn",
    "nature_carrots",
    "nature_irises",
    "nature_moss_boulder",
    "nature_giant_mushroom",
];

const ANIMATED_FLORA: &[&str] = &[
    "nature_willow_sway",
    "nature_bamboo_sway",
    "nature_wheat_sway",
    "nature_sunflowers_sway",
];

fn path(name: &str) -> String {
    format!("{}/assets/models/{name}.glb", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn flore_statique_chargeable_et_posee_au_sol() {
    for name in STATIC_FLORA {
        let (mesh, min, max) = load_gltf(&path(name)).unwrap_or_else(|e| {
            panic!("{name} : échec de chargement — {e}");
        });
        assert!(!mesh.vertices.is_empty(), "{name} : mesh vide");
        assert!(
            min.y > -0.15 && min.y < 0.25,
            "{name} : base à y={} au lieu de ~0 (l'asset doit poser au sol)",
            min.y
        );
        assert!(max.y > min.y, "{name} : hauteur nulle");
    }
}

#[test]
fn flore_animee_a_squelette_et_clip_idle() {
    for name in ANIMATED_FLORA {
        let p = path(name);
        let (mesh, _min, _max) =
            load_gltf(&p).unwrap_or_else(|e| panic!("{name} : échec de chargement — {e}"));
        assert!(!mesh.vertices.is_empty(), "{name} : mesh vide");
        let (skeleton, skins) = load_gltf_skeleton(&p)
            .unwrap_or_else(|e| panic!("{name} : échec squelette — {e}"))
            .unwrap_or_else(|| panic!("{name} : aucun squelette exporté"));
        assert!(!skeleton.joints.is_empty(), "{name} : squelette sans os");
        assert_eq!(
            skins.len(),
            mesh.vertices.len(),
            "{name} : poids de skinning désalignés du mesh"
        );
        let clips = load_gltf_clips(&p).unwrap_or_else(|e| panic!("{name} : échec clips — {e}"));
        let idle = clips
            .iter()
            .find(|c| c.name == "Idle")
            .unwrap_or_else(|| panic!("{name} : clip « Idle » absent"));
        assert!(idle.duration > 0.5, "{name} : clip Idle trop court");
    }
}
