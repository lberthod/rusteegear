//! Génère l'icône de l'app (dégradé + pièce dorée à l'étoile, thème « collectibles »).
//! Lancer : `cargo run --example gen_icon` → écrit `assets/icon/icon_<taille>.png`
//! + un dossier `icon.iconset` ; sur macOS, `iconutil -c icns` produit le `.icns`.

use image::{Rgba, RgbaImage};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] + (b[0] - a[0]) * t) as u8,
        (a[1] + (b[1] - a[1]) * t) as u8,
        (a[2] + (b[2] - a[2]) * t) as u8,
    ]
}

/// Carré arrondi : alpha lissé sur le bord (0 dehors, 1 dedans).
fn rounded_alpha(fx: f32, fy: f32, s: f32, r: f32) -> f32 {
    let cx = fx.clamp(r, s - r);
    let cy = fy.clamp(r, s - r);
    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
    (1.0 - (d - r)).clamp(0.0, 1.0)
}

/// Point dans une étoile à 5 branches centrée en 0 (pointe en haut), rayon extérieur `r`.
fn star_contains(x: f32, y: f32, r: f32) -> bool {
    let inner = r * 0.42;
    let mut poly = [(0.0f32, 0.0f32); 10];
    for (k, p) in poly.iter_mut().enumerate() {
        let rad = if k % 2 == 0 { r } else { inner };
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
        *p = (a.cos() * rad, a.sin() * rad);
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn render(size: u32) -> RgbaImage {
    let s = size as f32;
    let (cx, cy) = (s / 2.0, s / 2.0);
    let corner = s * 0.22;
    let coin_r = s * 0.345;
    let star_r = s * 0.205;
    let mut img = RgbaImage::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let a = rounded_alpha(fx, fy, s, corner);
            if a <= 0.0 {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                continue;
            }
            // Fond : dégradé bleu nuit → violet (diagonale).
            let t = ((fx + fy) / (2.0 * s)).clamp(0.0, 1.0);
            let mut col = lerp([28.0, 30.0, 64.0], [62.0, 40.0, 96.0], t);
            // Pièce dorée (dégradé radial) + liseré + étoile + reflet.
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            if d < coin_r {
                let rr = d / coin_r;
                col = lerp([255.0, 226.0, 130.0], [206.0, 144.0, 36.0], rr);
                if rr > 0.92 {
                    col = [150, 102, 22];
                }
                if star_contains(fx - cx, fy - cy, star_r) {
                    col = [120, 78, 12];
                }
                let hx = fx - (cx - coin_r * 0.35);
                let hy = fy - (cy - coin_r * 0.35);
                if (hx * hx + hy * hy).sqrt() < coin_r * 0.12 {
                    col = [255, 248, 210];
                }
            }
            img.put_pixel(x, y, Rgba([col[0], col[1], col[2], (a * 255.0) as u8]));
        }
    }
    img
}

fn main() {
    use image::imageops::FilterType::Lanczos3;
    let master = render(1024);
    let dir = format!("{ROOT}/assets/icon");
    master.save(format!("{dir}/icon_1024.png")).unwrap();
    for sz in [512u32, 256, 128, 64, 32] {
        image::imageops::resize(&master, sz, sz, Lanczos3)
            .save(format!("{dir}/icon_{sz}.png"))
            .unwrap();
    }
    let iset = format!("{dir}/icon.iconset");
    let _ = std::fs::create_dir_all(&iset);
    for (sz, name) in [
        (16u32, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ] {
        image::imageops::resize(&master, sz, sz, Lanczos3)
            .save(format!("{iset}/{name}"))
            .unwrap();
    }
    println!("OK. macOS : iconutil -c icns {iset} -o {dir}/icon.icns");
}
