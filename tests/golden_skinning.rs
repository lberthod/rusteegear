//! Golden test du skinning GPU (Sprint 86, Phase L) : une « planche à charnière » (16
//! segments le long de X, poitié gauche pondérée au joint 0 fixe, moitié droite au
//! joint 1 pivotant) rendue à -90°. Une régression du mélange de poids ou de la palette
//! de matrices romprait la courbe lisse attendue en un résultat plat ou en angle vif —
//! visuellement évident, et donc un bon candidat de golden test.
//!
//! Vérifié manuellement en construisant ce test (0°, 45°, -90°, cf. historique de
//! commit) : la courbe est un vrai arc lisse, pas un coude — la preuve que le poids par
//! sommet est réellement mélangé, pas juste qu'un des deux joints « gagne ».
//!
//! Même politique que `tests/golden_render.rs` : sauté (pas en échec) sans GPU headless
//! (CI `ubuntu-latest`). Régénérer après un changement de rendu intentionnel :
//! ```text
//! UPDATE_GOLDEN=1 cargo test --test golden_skinning
//! ```

use motor3derust::app::AppState;
use motor3derust::gfx::mesh::{SkinnedMeshData, SkinnedVertex};
use motor3derust::gfx::renderer::Renderer;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
const CHANNEL_TOLERANCE: i32 = 12;
const MAX_DIFFERING_RATIO: f64 = 0.01;

/// Planche de 16 segments le long de X (-2..2), poids linéaires joint0→joint1.
fn hinge_strip() -> SkinnedMeshData {
    let segments = 16;
    let half_len = 2.0f32;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let x = -half_len + t * 2.0 * half_len;
        let w1 = t.clamp(0.0, 1.0);
        let w0 = 1.0 - w1;
        for y in [-0.3f32, 0.3] {
            vertices.push(SkinnedVertex {
                position: [x, y, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [0.8, 0.6, 0.3],
                uv: [t, if y < 0.0 { 0.0 } else { 1.0 }],
                joints: [0, 1, 0, 0],
                weights: [w0, w1, 0.0, 0.0],
            });
        }
    }
    for i in 0..segments {
        let a = (i * 2) as u32;
        let (b, c, d) = (a + 1, a + 2, a + 3);
        indices.extend_from_slice(&[a, c, b, b, c, d]);
    }
    SkinnedMeshData { vertices, indices }
}

fn render_headless() -> Option<Vec<u8>> {
    let mut renderer = match pollster::block_on(Renderer::new_headless(WIDTH, HEIGHT)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "golden_skinning: pas de GPU headless disponible dans cet environnement \
                 ({e}) — test sauté, pas en échec."
            );
            return None;
        }
    };
    let mut app = AppState::default();
    app.scene.light.ambient = 0.3;
    // Ciel clair (valeurs de la démo) : le Sky::default() nuit rend l'ambiante
    // hémisphérique quasi noire et la charnière perdrait tout contraste.
    app.scene.sky.horizon_color = [0.85, 0.78, 0.62];
    app.scene.sky.zenith_color = [0.30, 0.52, 0.78];

    let joint0 = glam::Mat4::IDENTITY;
    let joint1 = glam::Mat4::from_rotation_z((-90.0f32).to_radians());
    let mesh = hinge_strip();
    Some(renderer.render_skinned_test(
        &mut app,
        &mesh,
        &[joint0, joint1],
        glam::Mat4::IDENTITY,
        WIDTH,
        HEIGHT,
    ))
}

fn diff_ratio(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "tailles d'image différentes");
    let pixels = a.len() / 4;
    let differing = (0..pixels)
        .filter(|&i| {
            let base = i * 4;
            (0..4).any(|c| (a[base + c] as i32 - b[base + c] as i32).abs() > CHANNEL_TOLERANCE)
        })
        .count();
    differing as f64 / pixels as f64
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join("skinned_hinge_bent90.png")
}

#[test]
fn golden_skinned_hinge_bent_90_degrees() {
    let Some(actual) = render_headless() else {
        return;
    };
    let path = golden_path();

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        image::save_buffer(&path, &actual, WIDTH, HEIGHT, image::ColorType::Rgba8)
            .unwrap_or_else(|e| panic!("écriture golden {path:?} impossible : {e}"));
        eprintln!("golden régénéré : {path:?}");
        return;
    }

    let expected = image::open(&path)
        .unwrap_or_else(|e| {
            panic!(
                "golden absent ou illisible : {path:?} ({e}). \
                 Régénérer avec : UPDATE_GOLDEN=1 cargo test --test golden_skinning"
            )
        })
        .to_rgba8();
    assert_eq!(
        (expected.width(), expected.height()),
        (WIDTH, HEIGHT),
        "golden {path:?} : dimensions différentes de celles attendues"
    );

    let ratio = diff_ratio(expected.as_raw(), &actual);
    if ratio > MAX_DIFFERING_RATIO {
        let actual_path = golden_path().with_extension("actual.png");
        let _ = image::save_buffer(
            &actual_path,
            &actual,
            WIDTH,
            HEIGHT,
            image::ColorType::Rgba8,
        );
        panic!(
            "skinning divergent de {path:?} : {:.2}% des pixels dépassent la tolérance \
             (max {:.2}%). Image obtenue écrite dans {actual_path:?} pour inspection.",
            ratio * 100.0,
            MAX_DIFFERING_RATIO * 100.0
        );
    }
}
