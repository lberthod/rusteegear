//! Vue de dessus du circuit HerRoad (`docs/img/herroad_map.png`) + tableau des fractions du
//! tour (x, z, altitude, rayon de courbure) — sert à placer sauts, turbos et points de passage
//! sur des lignes droites.
//!
//! Usage : `cargo run --example herroad_map --profile dev-fast [-- <sortie.png>]`

use motor3derust::racing::{Track, TrackSpec};

fn main() {
    let spec = TrackSpec::herroad();
    let track = Track::build(&spec);
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        format!("{}/docs/img/herroad_map.png", env!("CARGO_MANIFEST_DIR"))
    });
    let n = track.frames.len();
    println!("longueur {:.0} m, {} repères", track.length, n);

    // Tableau tous les 2 % du tour.
    for k in 0..50 {
        let i = k * n / 50;
        let f = &track.frames[i];
        let g = &track.frames[(i + 6) % n];
        let ang = f.fwd.x * g.fwd.z - f.fwd.z * g.fwd.x;
        let ds = 6.0 * 2.0;
        let radius = if ang.abs() < 1e-3 { f32::INFINITY } else { ds / ang.asin().abs() };
        println!(
            "{:>4.0}% i={:<4} x={:>7.1} z={:>7.1} y={:>5.1} rayon={:>7.0} {}",
            k as f32 * 2.0,
            i,
            f.pos.x,
            f.pos.z,
            f.pos.y,
            radius,
            if f.void { "VIDE" } else { "" }
        );
    }

    // Rendu 2D.
    let (mut min_x, mut max_x, mut min_z, mut max_z) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for f in &track.frames {
        min_x = min_x.min(f.pos.x - 12.0);
        max_x = max_x.max(f.pos.x + 12.0);
        min_z = min_z.min(f.pos.z - 12.0);
        max_z = max_z.max(f.pos.z + 12.0);
    }
    let scale = 1.6_f32;
    let w = ((max_x - min_x) * scale) as u32 + 1;
    let h = ((max_z - min_z) * scale) as u32 + 1;
    let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([24, 26, 34]));
    let mut put = |x: f32, z: f32, c: [u8; 3], r: i32| {
        let px = ((x - min_x) * scale) as i32;
        let py = ((max_z - z) * scale) as i32;
        for dx in -r..=r {
            for dy in -r..=r {
                let (a, b) = (px + dx, py + dy);
                if a >= 0 && b >= 0 && (a as u32) < w && (b as u32) < h {
                    img.put_pixel(a as u32, b as u32, image::Rgb(c));
                }
            }
        }
    };
    for (i, f) in track.frames.iter().enumerate() {
        let elev = ((f.pos.y + 8.0) / 24.0).clamp(0.0, 1.0);
        let col = [
            (70.0 + 170.0 * elev) as u8,
            (90.0 + 120.0 * elev) as u8,
            (120.0 + 60.0 * (1.0 - elev)) as u8,
        ];
        if f.void {
            continue;
        }
        let steps = (f.half_width * scale) as i32;
        for s in -steps..=steps {
            let o = s as f32 / scale;
            put(f.pos.x + f.right.x * o, f.pos.z + f.right.z * o, col, 0);
        }
        let _ = i;
    }
    let mark = |put: &mut dyn FnMut(f32, f32, [u8; 3], i32), i: usize, c: [u8; 3], r: i32| {
        let f = &track.frames[i % n];
        put(f.pos.x, f.pos.z, c, r);
    };
    mark(&mut put, 0, [255, 255, 255], 5); // arrivée
    for &c in &track.checkpoints {
        mark(&mut put, c, [80, 255, 120], 4);
    }
    for b in &track.boosts {
        mark(&mut put, b.frame, [255, 150, 30], 4);
    }
    // Tick de départ de sens : petit point rouge au 1er repère.
    mark(&mut put, 12, [255, 60, 60], 3);
    if let Some(parent) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    img.save(&out).expect("écriture");
    println!("carte : {out}");
}
