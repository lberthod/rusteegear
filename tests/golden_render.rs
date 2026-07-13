//! Golden tests de rendu (Sprint 80 — filet de sécurité avant les chantiers d'animation
//! et d'image, ROADMAP_SPRINTS.md Phase K) : rendu **headless** (sans fenêtre, sans UI)
//! d'une scène de référence via `Renderer::render_scene_headless`, comparé pixel à pixel
//! (avec tolérance) à une image de référence versionnée dans `tests/golden/`.
//!
//! But : que la CI passe au rouge si un shader ou le pipeline de rendu dérive — pas de
//! sprint de rendu (animation squelettale, HDR/bloom, ciel/fog…) sans ce filet.
//!
//! Régénérer une image de référence après un changement de rendu **intentionnel** :
//! ```text
//! UPDATE_GOLDEN=1 cargo test --test golden_render
//! ```
//! Puis vérifier visuellement le fichier modifié sous `tests/golden/` avant de le committer.
//!
//! Risque connu (documenté dans ROADMAP_SPRINTS.md, Sprint 80) : le rendu dépend du GPU/
//! driver réel (pas de rasteriseur logiciel forcé) — un écart entre deux machines peut
//! venir du matériel, pas d'une régression. La tolérance ci-dessous absorbe l'anti-aliasing
//! et les petits écarts de filtrage ; en cas de faux positif documenté, l'ajuster ici.
//!
//! La CI (`ubuntu-latest`) n'a pas de GPU : `Renderer::new_headless` y échoue à trouver un
//! adaptateur, et le test est alors **sauté** (pas mis en échec) — voir `render_headless`.
//! Il tourne réellement en local (macOS/Metal, ou toute machine avec un GPU) : c'est là
//! qu'il protège les chantiers de rendu à venir.

use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::scene::{Light, MeshKind, PointLight, Scene, SceneObject, Transform};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
/// Écart maximal toléré par canal (0..255) avant de compter un pixel comme différent.
const CHANNEL_TOLERANCE: i32 = 12;
/// Fraction maximale de pixels divergents avant échec (absorbe l'anti-aliasing aux bords).
const MAX_DIFFERING_RATIO: f64 = 0.01;

/// Scène de référence n°1 : primitives + lumières (directionnelle + ponctuelle), avec
/// ombre portée. Construite explicitement ici plutôt que via `Scene::demo()` : ce golden
/// ne doit dériver que si le **rendu** change, pas si la scène de démo éditeur évolue.
fn scene_primitives_lights() -> Scene {
    let ground = SceneObject {
        name: "Sol".into(),
        mesh: MeshKind::Plane,
        transform: Transform {
            scale: glam::Vec3::new(6.0, 1.0, 6.0),
            ..Transform::from_pos(glam::Vec3::new(0.0, -1.0, 0.0))
        },
        color: [0.5, 0.5, 0.55],
        ..Default::default()
    };
    let cube = SceneObject {
        name: "Cube".into(),
        mesh: MeshKind::Cube,
        transform: Transform::from_pos(glam::Vec3::new(-1.2, 0.0, 0.0)),
        color: [0.85, 0.35, 0.2],
        metallic: 0.1,
        roughness: 0.5,
        ..Default::default()
    };
    let sphere = SceneObject {
        name: "Sphère".into(),
        mesh: MeshKind::Sphere,
        transform: Transform::from_pos(glam::Vec3::new(1.2, 0.0, 0.3)),
        color: [0.25, 0.55, 0.85],
        metallic: 0.6,
        roughness: 0.25,
        ..Default::default()
    };

    Scene {
        objects: vec![ground, cube, sphere],
        light: Light {
            dir: [0.5, 1.0, 0.3],
            color: [1.0, 0.98, 0.92],
            ambient: 0.15,
        },
        point_lights: vec![PointLight {
            position: [1.5, 2.0, 1.5],
            color: [0.3, 0.55, 1.0],
            intensity: 2.5,
            range: 8.0,
            spot_dir: [0.0, -1.0, 0.0],
            spot_angle: 0.0,
        }],
        ..Default::default()
    }
}

/// `None` si l'environnement n'a pas de GPU/driver exploitable (typiquement la CI Linux
/// `ubuntu-latest`, sans Vulkan matériel ni rasteriseur logiciel installé) : dans ce cas
/// le golden test est **sauté**, pas mis en échec — l'absence de GPU n'est pas une
/// régression de rendu. Documenté comme risque connu dans ROADMAP_SPRINTS.md, Sprint 80.
fn render_headless(scene: Scene) -> Option<Vec<u8>> {
    let mut renderer = match pollster::block_on(Renderer::new_headless(WIDTH, HEIGHT)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "golden_render: pas de GPU headless disponible dans cet environnement \
                 ({e}) — test sauté, pas en échec."
            );
            return None;
        }
    };
    let mut app = AppState::default();
    app.scene = scene;
    // Caméra orbitale par défaut (OrbitCamera::new) : target/distance/yaw/pitch fixes,
    // donc déterministe d'un run à l'autre.
    Some(renderer.render_scene_headless(&mut app, WIDTH, HEIGHT))
}

/// Compare deux images RGBA8 de même taille. Retourne le ratio de pixels divergents
/// au-delà de `CHANNEL_TOLERANCE` sur au moins un canal.
fn diff_ratio(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "tailles d'image différentes");
    let mut differing = 0usize;
    let pixels = a.len() / 4;
    for i in 0..pixels {
        let base = i * 4;
        let mismatch =
            (0..4).any(|c| (a[base + c] as i32 - b[base + c] as i32).abs() > CHANNEL_TOLERANCE);
        if mismatch {
            differing += 1;
        }
    }
    differing as f64 / pixels as f64
}

fn golden_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// Charge ou régénère (`UPDATE_GOLDEN=1`) une image de référence, puis compare.
fn assert_matches_golden(name: &str, actual_rgba: &[u8], width: u32, height: u32) {
    let path = golden_path(name);

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        image::save_buffer(&path, actual_rgba, width, height, image::ColorType::Rgba8)
            .unwrap_or_else(|e| panic!("écriture golden {path:?} impossible : {e}"));
        eprintln!("golden régénéré : {path:?}");
        return;
    }

    let expected = image::open(&path).unwrap_or_else(|e| {
        panic!(
            "golden absent ou illisible : {path:?} ({e}). \
             Régénérer avec : UPDATE_GOLDEN=1 cargo test --test golden_render"
        )
    });
    let expected = expected.to_rgba8();
    assert_eq!(
        (expected.width(), expected.height()),
        (width, height),
        "golden {path:?} : dimensions différentes de celles attendues"
    );

    let ratio = diff_ratio(expected.as_raw(), actual_rgba);
    if ratio > MAX_DIFFERING_RATIO {
        let actual_path = golden_path(&format!("{name}.actual.png"));
        let _ = image::save_buffer(
            &actual_path,
            actual_rgba,
            width,
            height,
            image::ColorType::Rgba8,
        );
        panic!(
            "rendu divergent de {path:?} : {:.2}% des pixels dépassent la tolérance \
             (max {:.2}%). Image obtenue écrite dans {actual_path:?} pour inspection. \
             Si la différence est un changement de rendu voulu : \
             UPDATE_GOLDEN=1 cargo test --test golden_render",
            ratio * 100.0,
            MAX_DIFFERING_RATIO * 100.0
        );
    }
}

#[test]
fn golden_primitives_lights() {
    let Some(pixels) = render_headless(scene_primitives_lights()) else {
        return; // pas de GPU dans cet environnement (cf. `render_headless`) : rien à vérifier
    };
    assert_matches_golden("primitives_lights.png", &pixels, WIDTH, HEIGHT);
}
