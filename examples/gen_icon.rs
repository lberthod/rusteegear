//! Génère l'icône de l'app (engrenage « rouille » sur fond sombre arrondi).
//! Lancer : `cargo run --example gen_icon` → écrit `assets/icon/icon_<taille>.png`.

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Couleur d'un pixel (RGBA) de l'icône, coordonnées normalisées centrées [-1, 1].
fn pixel(nx: f32, ny: f32) -> [f32; 4] {
    // --- fond : carré arrondi sombre ---
    let bg = [0.106, 0.118, 0.141, 1.0]; // #1b1e24
    let corner = 0.82; // demi-côté du carré arrondi
    let radius = 0.18;
    // distance au carré arrondi (champ de distance signé)
    let qx = (nx.abs() - (corner - radius)).max(0.0);
    let qy = (ny.abs() - (corner - radius)).max(0.0);
    let sq = (qx * qx + qy * qy).sqrt() - radius;
    let bg_a = 1.0 - smoothstep(0.0, 0.02, sq);
    if bg_a <= 0.0 {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let dist = (nx * nx + ny * ny).sqrt();
    let ang = ny.atan2(nx);

    // --- engrenage ---
    let teeth = 9.0;
    // rayon extérieur modulé par les dents (créneau adouci)
    let tooth = (0.5 + 0.5 * (ang * teeth).cos()).powf(0.6);
    let outer = 0.46 + 0.10 * smoothstep(0.35, 0.65, tooth);
    let gear_edge = smoothstep(outer + 0.015, outer - 0.015, dist); // 1 dedans
    // trou central (moyeu)
    let hole = 0.17;
    let gear_inner = smoothstep(hole - 0.015, hole + 0.015, dist); // 1 hors du trou
    let gear_a = gear_edge * gear_inner;

    // couleur rouille avec un léger dégradé (ombrage diagonal)
    let shade = 0.85 + 0.25 * (-ny * 0.5 + nx * 0.2);
    let rust = [
        (0.82 * shade).min(1.0),
        (0.40 * shade).min(1.0),
        (0.16 * shade).min(1.0),
        1.0,
    ];

    // composition gear sur fond
    let a = gear_a;
    [
        rust[0] * a + bg[0] * (1.0 - a),
        rust[1] * a + bg[1] * (1.0 - a),
        rust[2] * a + bg[2] * (1.0 - a),
        bg_a,
    ]
}

fn render(size: u32) -> image::RgbaImage {
    // supersampling 2x pour des bords nets
    let ss = 2;
    image::RgbaImage::from_fn(size, size, |x, y| {
        let mut acc = [0.0f32; 4];
        for sy in 0..ss {
            for sx in 0..ss {
                let fx = (x as f32 + (sx as f32 + 0.5) / ss as f32) / size as f32;
                let fy = (y as f32 + (sy as f32 + 0.5) / ss as f32) / size as f32;
                let nx = fx * 2.0 - 1.0;
                let ny = fy * 2.0 - 1.0;
                let p = pixel(nx, ny);
                for i in 0..4 {
                    acc[i] += p[i];
                }
            }
        }
        let n = (ss * ss) as f32;
        image::Rgba([
            (acc[0] / n * 255.0) as u8,
            (acc[1] / n * 255.0) as u8,
            (acc[2] / n * 255.0) as u8,
            (acc[3] / n * 255.0) as u8,
        ])
    })
}

fn main() {
    std::fs::create_dir_all("assets/icon").unwrap();
    for size in [1024u32, 512, 256, 128, 64, 32] {
        let img = render(size);
        let path = format!("assets/icon/icon_{size}.png");
        img.save(&path).unwrap();
        println!("écrit {path}");
    }
}
