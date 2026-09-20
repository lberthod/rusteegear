//! Démo « HerRoad » : la scène 3D du jeu de course (`crate::racing`). Route et barrières sont
//! deux maillages générés depuis le circuit (mêmes triangles que la collision), la voiture et
//! le fantôme sont des assemblages de primitives repositionnés à chaque pas par `app::race`.

use std::f32::consts::FRAC_PI_2;

use glam::{Quat, Vec3};

use super::*;
use crate::gfx::mesh::MeshData;
use crate::racing::car::basis_rotation;
use crate::racing::layout::{CarPart, RaceLayout};
use crate::racing::terrain::Terrain;
use crate::racing::track::{FRAME_STEP, Track, TrackSpec};

/// Couleurs de portique : à venir / prochain / franchi.
pub const GATE_IDLE: ([f32; 3], f32) = ([0.25, 0.55, 1.0], 1.2);
pub const GATE_NEXT: ([f32; 3], f32) = ([1.0, 0.85, 0.2], 3.0);
pub const GATE_DONE: ([f32; 3], f32) = ([0.2, 0.9, 0.4], 0.5);

fn generated_mesh(name: &str, data: MeshData) -> ImportedMesh {
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &data.vertices {
        let p = Vec3::from(v.position);
        min = min.min(p);
        max = max.max(p);
    }
    ImportedMesh {
        name: name.into(),
        path: format!("generated://herroad/{name}"),
        data,
        aabb_min: min,
        aabb_max: max,
        ..Default::default()
    }
}

/// Générateur pseudo-aléatoire déterministe (le décor est identique à chaque lancement).
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Chemin d'un asset partagé avec la démo Rivière : dossier `assets/models/` en natif,
/// `embedded://` (compilé dans le `.wasm`) sur le web.
fn asset_path(file: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        format!("{}{file}", crate::assets::EMBEDDED_SCHEME)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file)
    }
}

/// Charge un glTF une seule fois (les instances partagent l'entrée de `Scene::imported`) ;
/// renvoie (indice, hauteur du modèle, plancher local).
fn load_model(imported: &mut Vec<ImportedMesh>, file: &str) -> Option<(u32, f32, f32)> {
    let path = asset_path(file);
    let idx = match imported.iter().position(|m| m.path == path) {
        Some(i) => i,
        None => match crate::scene::import::load_gltf(&path) {
            Ok((data, aabb_min, aabb_max)) => {
                let mut mesh = ImportedMesh {
                    path,
                    data,
                    aabb_min,
                    aabb_max,
                    ..Default::default()
                };
                mesh.load_skinning();
                imported.push(mesh);
                imported.len() - 1
            }
            Err(e) => {
                log::warn!("HerRoad : décor « {file} » indisponible ({e})");
                return None;
            }
        },
    };
    let m = &imported[idx];
    Some((idx as u32, (m.aabb_max.y - m.aabb_min.y).max(0.01), m.aabb_min.y))
}

fn hash2(x: f32, z: f32) -> f32 {
    let v = (x * 12.9898 + z * 78.233).sin() * 43758.547;
    v - v.floor()
}

struct Part {
    name: &'static str,
    mesh: MeshKind,
    offset: Vec3,
    scale: Vec3,
    rot: Quat,
    color: [f32; 3],
    emissive: f32,
    metallic: f32,
    roughness: f32,
    front_wheel: bool,
}

fn car_model() -> Vec<Part> {
    let wheel_rot = Quat::from_rotation_z(FRAC_PI_2);
    let p = |name, mesh, offset: [f32; 3], scale: [f32; 3], color: [f32; 3]| Part {
        name,
        mesh,
        offset: Vec3::from(offset),
        scale: Vec3::from(scale),
        rot: Quat::IDENTITY,
        color,
        emissive: 0.0,
        metallic: 0.5,
        roughness: 0.3,
        front_wheel: false,
    };
    let red = [0.92, 0.10, 0.14];
    let dark = [0.06, 0.07, 0.09];
    let glass = [0.05, 0.09, 0.16];
    let white = [0.95, 0.95, 0.97];
    let tilted = |mut part: Part, rx: f32| {
        part.rot = Quat::from_rotation_x(rx);
        part
    };
    let mut parts = vec![
        p("Caisse", MeshKind::Cube, [0.0, 0.5, 0.0], [1.9, 0.42, 4.4], red),
        tilted(p("Capot", MeshKind::Cube, [0.0, 0.5, 1.65], [1.74, 0.28, 1.5], red), 0.09),
        p("Habitacle", MeshKind::Cube, [0.0, 0.93, -0.35], [1.46, 0.42, 1.7], glass),
        p("Toit", MeshKind::Cube, [0.0, 1.16, -0.4], [1.36, 0.08, 1.35], red),
        tilted(p("Pare-brise", MeshKind::Cube, [0.0, 0.98, 0.72], [1.4, 0.05, 0.95], glass), -0.62),
        tilted(p("Lunette", MeshKind::Cube, [0.0, 0.98, -1.4], [1.3, 0.05, 0.7], glass), 0.55),
        p("Bande capot", MeshKind::Cube, [0.0, 0.66, 1.62], [0.34, 0.02, 1.5], white),
        p("Bande toit", MeshKind::Cube, [0.0, 1.21, -0.4], [0.34, 0.02, 1.36], white),
        p("Jupe G", MeshKind::Cube, [0.99, 0.3, 0.0], [0.08, 0.16, 3.7], dark),
        p("Jupe D", MeshKind::Cube, [-0.99, 0.3, 0.0], [0.08, 0.16, 3.7], dark),
        p("Lame avant", MeshKind::Cube, [0.0, 0.2, 2.3], [1.95, 0.06, 0.6], dark),
        p("Diffuseur", MeshKind::Cube, [0.0, 0.24, -2.25], [1.6, 0.16, 0.45], dark),
        p("Aileron", MeshKind::Cube, [0.0, 1.28, -2.05], [2.1, 0.07, 0.6], dark),
        p("Support G", MeshKind::Cube, [0.7, 0.98, -2.0], [0.08, 0.52, 0.12], dark),
        p("Support D", MeshKind::Cube, [-0.7, 0.98, -2.0], [0.08, 0.52, 0.12], dark),
        p("Dérive G", MeshKind::Cube, [1.06, 1.22, -2.05], [0.05, 0.3, 0.6], red),
        p("Dérive D", MeshKind::Cube, [-1.06, 1.22, -2.05], [0.05, 0.3, 0.6], red),
        p("Rétroviseur G", MeshKind::Cube, [1.06, 0.86, 0.55], [0.2, 0.1, 0.14], red),
        p("Rétroviseur D", MeshKind::Cube, [-1.06, 0.86, 0.55], [0.2, 0.1, 0.14], red),
    ];
    for (x, z, front) in [(1.03, 1.45, true), (-1.03, 1.45, true), (1.03, -1.45, false), (-1.03, -1.45, false)] {
        let mut tyre = p("Roue", MeshKind::Cylinder, [x, 0.42, z], [0.86, 0.46, 0.86], dark);
        tyre.rot = wheel_rot;
        tyre.front_wheel = front;
        tyre.roughness = 0.95;
        tyre.metallic = 0.05;
        parts.push(tyre);
        let mut rim = p("Jante", MeshKind::Cylinder, [x * 1.045, 0.42, z], [0.5, 0.47, 0.5], [0.78, 0.8, 0.86]);
        rim.rot = wheel_rot;
        rim.front_wheel = front;
        rim.metallic = 0.9;
        rim.roughness = 0.2;
        parts.push(rim);
    }
    for x in [0.62, -0.62] {
        let mut head = p("Phare", MeshKind::Cube, [x, 0.62, 2.36], [0.42, 0.16, 0.06], [1.0, 0.97, 0.8]);
        head.emissive = 3.2;
        parts.push(head);
        let mut tail = p("Feu arrière", MeshKind::Cube, [x, 0.68, -2.22], [0.44, 0.14, 0.06], [1.0, 0.08, 0.05]);
        tail.emissive = 2.8;
        parts.push(tail);
    }
    parts
}

impl Scene {
    /// Le jeu HerRoad seul (sans son état de course) — cf. `herroad_build`.
    pub fn herroad_demo() -> Self {
        herroad_build().0
    }
}

/// Construit la scène, la correspondance objets/course et le circuit lui-même.
pub fn herroad_build() -> (Scene, RaceLayout, Track) {
    let spec = TrackSpec::herroad();
    let track = Track::build(&spec);
    let mut objects: Vec<SceneObject> = Vec::new();
    let mut imported: Vec<ImportedMesh> = Vec::new();
    let mut layout = RaceLayout::default();

    // Relief : talus qui portent la route, prairie vallonnée, montagnes à l'horizon.
    let terrain = Terrain::build(&track);
    let plane_y = terrain.base - 6.0;
    let mut ground = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, plane_y, 0.0));
    ground.transform = ground.transform.with_scale(Vec3::new(4000.0, 1.0, 4000.0));
    ground.color = [0.16, 0.3, 0.17];
    ground.roughness = 1.0;
    objects.push(ground);
    imported.push(generated_mesh("relief", terrain.mesh()));
    let mut relief = demo_obj("Relief", MeshKind::Imported(imported.len() as u32 - 1), Vec3::ZERO);
    relief.roughness = 0.95;
    relief.group = "Décor".into();
    objects.push(relief);

    // Route et barrières.
    imported.push(generated_mesh("route", track.road_mesh()));
    let mut road = demo_obj("Route", MeshKind::Imported(imported.len() as u32 - 1), Vec3::ZERO);
    road.roughness = 0.8;
    road.group = "Circuit".into();
    objects.push(road);
    imported.push(generated_mesh("barrieres", track.wall_mesh()));
    let mut walls = demo_obj("Barrières", MeshKind::Imported(imported.len() as u32 - 1), Vec3::ZERO);
    walls.roughness = 0.6;
    walls.group = "Circuit".into();
    objects.push(walls);

    // Piliers du pont, un de chaque côté de la chaussée, du relief jusqu'au tablier.
    if let Some((i0, j)) = track.crossing {
        let c = track.frames[i0].pos;
        for k in (j.saturating_sub(44)..(j + 44).min(track.frames.len())).step_by(14) {
            let f = &track.frames[k];
            if Vec3::new(c.x - f.pos.x, 0.0, c.z - f.pos.z).length() < 26.0 {
                continue;
            }
            for side in [-1.0_f32, 1.0] {
                let p = f.pos + f.right * side * (f.half_width - 0.6);
                let ground_y = terrain.height(p.x, p.z) - 0.5;
                let height = (f.pos.y - ground_y - 0.3).max(0.5);
                let mut pillar = demo_obj(
                    "Pilier de pont",
                    MeshKind::Cube,
                    Vec3::new(p.x, ground_y + height * 0.5, p.z),
                );
                pillar.transform = pillar.transform.with_scale(Vec3::new(1.6, height, 1.6));
                pillar.color = [0.62, 0.62, 0.66];
                pillar.roughness = 0.85;
                pillar.group = "Décor".into();
                objects.push(pillar);
            }
        }
    }

    // Forêt, rochers : vrais modèles glTF de la démo Rivière, semés sur le relief hors de la
    // route et du pont, par bosquets (bruit) plus denses en s'éloignant du circuit.
    {
        // (fichier, hauteur mini, hauteur maxi en m)
        const SPECIES: [(&str, f32, f32); 4] = [
            ("riviere/epicea_a.glb", 11.0, 19.0),
            ("riviere/pin_a.glb", 10.0, 16.0),
            ("riviere/hetre_a.glb", 9.0, 14.0),
            ("riviere/bouleau_a.glb", 8.0, 12.0),
        ];
        const ROCKS: [(&str, f32, f32); 3] = [
            ("riviere/rocher_a.glb", 2.0, 5.0),
            ("riviere/rocher_b.glb", 2.0, 5.0),
            ("riviere/rocher_c.glb", 2.0, 5.5),
        ];
        let trees: Vec<Option<(u32, f32, f32)>> =
            SPECIES.iter().map(|(f, _, _)| load_model(&mut imported, f)).collect();
        let rocks: Vec<Option<(u32, f32, f32)>> =
            ROCKS.iter().map(|(f, _, _)| load_model(&mut imported, f)).collect();
        let frames: Vec<Vec3> = track.frames.iter().step_by(2).map(|f| f.pos).collect();
        let bridge_frames: Vec<Vec3> = track
            .crossing
            .map(|(_, j)| {
                let n = track.frames.len();
                (0..n)
                    .filter(|&k| {
                        let d = (k as i32 - j as i32).unsigned_abs() as usize;
                        d.min(n - d) < 60
                    })
                    .map(|k| track.frames[k].pos)
                    .collect()
            })
            .unwrap_or_default();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for f in &track.frames {
            lo = lo.min(f.pos);
            hi = hi.max(f.pos);
        }
        let mut placed_trees = 0;
        let mut placed_rocks = 0;
        let mut z = lo.z - 260.0;
        while z < hi.z + 260.0 && (placed_trees < 460 || placed_rocks < 90) {
            let mut x = lo.x - 260.0;
            while x < hi.x + 260.0 {
                let jx = x + (hash2(x, z) - 0.5) * 16.0;
                let jz = z + (hash2(z, x) - 0.5) * 16.0;
                x += 15.0;
                let d_road = frames
                    .iter()
                    .map(|f| Vec3::new(f.x - jx, 0.0, f.z - jz).length())
                    .fold(f32::MAX, f32::min);
                let near_bridge = bridge_frames
                    .iter()
                    .any(|f| Vec3::new(f.x - jx, 0.0, f.z - jz).length() < 32.0);
                if d_road < 17.0 || near_bridge {
                    continue;
                }
                let h = terrain.height(jx, jz);
                let n = terrain.normal(jx, jz);
                let alt = h - terrain.base;
                if n.y < 0.72 || alt > 52.0 {
                    continue;
                }
                // Bosquets : bruit basse fréquence, moins dense près de la route.
                let clump = (jx * 0.018).sin() * (jz * 0.021 + 1.3).cos() * 0.5 + 0.5;
                let near_pen = ((d_road - 17.0) / 60.0).clamp(0.0, 1.0);
                let want = clump * (0.25 + 0.75 * near_pen);
                let r = hash2(jx * 1.7, jz * 0.9);
                let yaw = hash2(jz, jx) * std::f32::consts::TAU;
                if r < want * 0.55 && placed_trees < 460 {
                    let k = ((hash2(jx + 5.0, jz - 3.0) * 4.0) as usize).min(3);
                    // En altitude : surtout des résineux.
                    let k = if alt > 25.0 { k % 2 } else { k };
                    let Some((mesh, h0, floor)) = trees[k] else {
                        continue;
                    };
                    let (hmin, hmax) = (SPECIES[k].1, SPECIES[k].2);
                    let target = hmin + (hmax - hmin) * hash2(jx * 3.1, jz * 2.3);
                    let sc = target / h0;
                    let mut t = demo_obj("Arbre", MeshKind::Imported(mesh), Vec3::new(jx, h - floor * sc - 0.25, jz));
                    t.transform = t.transform.with_scale(Vec3::splat(sc));
                    t.transform.rotation = Quat::from_rotation_y(yaw);
                    t.group = "Forêt".into();
                    t.roughness = 0.9;
                    objects.push(t);
                    placed_trees += 1;
                } else if r > 0.93 && placed_rocks < 90 && d_road < 70.0 {
                    let k = ((hash2(jx - 2.0, jz + 8.0) * 3.0) as usize).min(2);
                    let Some((mesh, h0, floor)) = rocks[k] else {
                        continue;
                    };
                    let target = ROCKS[k].1 + (ROCKS[k].2 - ROCKS[k].1) * hash2(jx * 0.7, jz * 1.9);
                    let sc = target / h0;
                    let mut t = demo_obj("Rocher", MeshKind::Imported(mesh), Vec3::new(jx, h - floor * sc - 0.3, jz));
                    t.transform = t.transform.with_scale(Vec3::splat(sc));
                    t.transform.rotation = Quat::from_rotation_y(yaw);
                    t.group = "Décor".into();
                    t.roughness = 0.95;
                    objects.push(t);
                    placed_rocks += 1;
                }
            }
            z += 15.0;
        }
        log::info!("HerRoad : {placed_trees} arbres, {placed_rocks} rochers");
    }

    // Lampadaires : montant + tête émettrice, de chaque côté, tous les ~60 m.
    let mut rng = Lcg(0xC0FFEE);
    for i in (10..track.frames.len()).step_by(30) {
        let f = &track.frames[i];
        if f.void || !f.wall_l {
            continue;
        }
        let side = if rng.next() < 0.5 { -1.0 } else { 1.0 };
        let base = f.pos + f.right * side * (f.half_width + 1.8);
        let mut pole = demo_obj("Mât", MeshKind::Cylinder, base + Vec3::Y * 3.0);
        pole.transform = pole.transform.with_scale(Vec3::new(0.25, 6.0, 0.25));
        pole.color = [0.3, 0.3, 0.34];
        pole.group = "Décor".into();
        objects.push(pole);
        let mut lamp = demo_obj("Lampe", MeshKind::Sphere, base + Vec3::Y * 6.2);
        lamp.transform = lamp.transform.with_scale(Vec3::splat(0.8));
        lamp.color = [1.0, 0.92, 0.7];
        lamp.emissive = 3.5;
        lamp.group = "Décor".into();
        objects.push(lamp);
    }

    // Plaques de turbo : dalle orange + deux chevrons.
    for b in &track.boosts {
        let rot = basis_rotation(b.fwd, b.up);
        let mut pad = demo_obj("Turbo", MeshKind::Cube, b.pos + b.up * 0.03);
        pad.transform = pad.transform.with_scale(Vec3::new(5.6, 0.06, 7.0));
        pad.transform.rotation = rot;
        pad.color = [1.0, 0.45, 0.08];
        pad.emissive = 2.2;
        pad.group = "Turbo".into();
        objects.push(pad);
        for (k, z) in [1.2_f32, -1.2].into_iter().enumerate() {
            for side in [-1.0_f32, 1.0] {
                let local = Vec3::new(side * 0.85, 0.05, z);
                let mut bar = demo_obj("Chevron", MeshKind::Cube, b.pos + rot * local + b.up * 0.03);
                bar.transform = bar.transform.with_scale(Vec3::new(0.5, 0.06, 3.0));
                bar.transform.rotation = rot * Quat::from_rotation_y(-side * 0.6);
                bar.color = [1.0, 0.95, 0.6];
                bar.emissive = 3.5 - k as f32;
                bar.group = "Turbo".into();
                objects.push(bar);
            }
        }
    }

    // Portiques : deux poteaux + traverse, un par point de passage, puis l'arrivée.
    let gate_frames: Vec<usize> = track
        .checkpoints
        .iter()
        .copied()
        .chain(std::iter::once(0))
        .collect();
    for (g, &fi) in gate_frames.iter().enumerate() {
        let f = &track.frames[fi];
        let finish = g == gate_frames.len() - 1;
        let rot = basis_rotation(f.fwd, f.up);
        let hw = f.half_width + 1.0;
        let top = if finish { 7.5 } else { 6.5 };
        let (color, emissive) = if finish { ([1.0, 1.0, 1.0], 1.0) } else { GATE_IDLE };
        let mut ids = Vec::new();
        for side in [-1.0_f32, 1.0] {
            let p = f.pos + f.right * side * hw + f.up * top * 0.5;
            let mut post = demo_obj("Poteau", MeshKind::Cylinder, p);
            post.transform = post.transform.with_scale(Vec3::new(0.7, top, 0.7));
            post.transform.rotation = rot;
            post.color = color;
            post.emissive = emissive;
            post.group = "Portiques".into();
            ids.push(objects.len());
            objects.push(post);
        }
        let mut bar = demo_obj("Traverse", MeshKind::Cube, f.pos + f.up * top);
        bar.transform = bar.transform.with_scale(Vec3::new(2.0 * hw, 0.7, 0.7));
        bar.transform.rotation = rot;
        bar.color = color;
        bar.emissive = emissive;
        bar.group = "Portiques".into();
        ids.push(objects.len());
        objects.push(bar);
        if finish {
            // Damier sous la traverse.
            let cells = 12;
            let w = 2.0 * hw / cells as f32;
            for c in 0..cells {
                let x = -hw + (c as f32 + 0.5) * w;
                let mut tile = demo_obj(
                    "Damier",
                    MeshKind::Cube,
                    f.pos + f.right * x + f.up * (top - 0.9),
                );
                tile.transform = tile.transform.with_scale(Vec3::new(w, 1.1, 0.2));
                tile.transform.rotation = rot;
                tile.color = if c % 2 == 0 { [0.05, 0.05, 0.05] } else { [0.95, 0.95, 0.95] };
                tile.group = "Portiques".into();
                ids.push(objects.len());
                objects.push(tile);
            }
        }
        layout.gates.push(ids);
    }

    // Tribunes de chaque côté de la ligne de départ : gradins de béton, public coloré, auvent.
    {
        let mid = 16usize;
        let f = &track.frames[mid];
        let rot = basis_rotation(f.fwd, f.up);
        let palette = [
            [0.95, 0.25, 0.2],
            [0.2, 0.55, 0.95],
            [0.98, 0.82, 0.2],
            [0.95, 0.95, 0.95],
            [0.25, 0.8, 0.45],
            [0.9, 0.4, 0.75],
        ];
        let mut crowd = Lcg(0xBEEF);
        let len = 44.0;
        for side in [-1.0_f32, 1.0] {
            for tier in 0..4 {
                let lateral = f.half_width + 6.5 + tier as f32 * 1.8;
                let top = f.pos.y + 1.3 + tier as f32 * 1.15;
                let centre = f.pos + f.right * side * lateral;
                let bottom = terrain.height(centre.x, centre.z) - 1.0;
                let height = top - bottom;
                let mut slab = demo_obj(
                    "Gradin",
                    MeshKind::Cube,
                    Vec3::new(centre.x, bottom + height * 0.5, centre.z),
                );
                slab.transform = slab.transform.with_scale(Vec3::new(1.8, height, len));
                slab.transform.rotation = rot;
                slab.color = [0.66, 0.66, 0.7];
                slab.roughness = 0.9;
                slab.group = "Tribunes".into();
                objects.push(slab);
                let mut d = -len * 0.5 + 1.2;
                while d < len * 0.5 - 0.6 {
                    let c = palette[(crowd.next() * palette.len() as f32) as usize % palette.len()];
                    let pos = centre + f.fwd * d;
                    let mut fan = demo_obj("Spectateur", MeshKind::Cube, Vec3::new(pos.x, top + 0.45, pos.z));
                    fan.transform = fan.transform.with_scale(Vec3::new(0.7, 0.9, 0.6));
                    fan.transform.rotation = rot;
                    fan.color = c;
                    fan.roughness = 0.9;
                    fan.group = "Tribunes".into();
                    objects.push(fan);
                    d += 1.9 + crowd.next() * 0.5;
                }
            }
            // Auvent sur pilotis.
            let lateral = f.half_width + 6.5 + 1.5 * 1.8;
            let centre = f.pos + f.right * side * (lateral + 0.8);
            let roof_y = f.pos.y + 7.4;
            let mut roof = demo_obj("Auvent", MeshKind::Cube, Vec3::new(centre.x, roof_y, centre.z));
            roof.transform = roof.transform.with_scale(Vec3::new(9.0, 0.35, len + 2.0));
            roof.transform.rotation = rot;
            roof.color = [0.92, 0.92, 0.95];
            roof.group = "Tribunes".into();
            objects.push(roof);
            for d in [-len * 0.5, 0.0, len * 0.5] {
                let base = f.pos + f.right * side * (lateral + 4.0) + f.fwd * d;
                let bottom = terrain.height(base.x, base.z) - 1.0;
                let h = roof_y - bottom;
                let mut post = demo_obj("Pilotis", MeshKind::Cube, Vec3::new(base.x, bottom + h * 0.5, base.z));
                post.transform = post.transform.with_scale(Vec3::new(0.5, h, 0.5));
                post.color = [0.55, 0.55, 0.6];
                post.group = "Tribunes".into();
                objects.push(post);
            }
        }
    }

    // Banderoles derrière les barrières, sur les lignes droites.
    {
        let colors = [[0.95, 0.2, 0.2], [0.2, 0.55, 0.95], [1.0, 0.8, 0.15], [0.2, 0.8, 0.4]];
        let n = track.frames.len();
        let mut k = 40usize;
        let mut idx = 0usize;
        while k < n {
            let f = &track.frames[k];
            k += 34;
            if f.void || !f.wall_l {
                continue;
            }
            let side = if idx % 2 == 0 { -1.0 } else { 1.0 };
            let c = colors[idx % colors.len()];
            idx += 1;
            // Pas de panneau sous le pont, ni sur un virage serré.
            let ahead = &track.frames[(k) % n];
            if f.fwd.dot(ahead.fwd) < 0.985 {
                continue;
            }
            let pos = f.pos + f.right * side * (f.half_width + 1.6) + f.up * 2.1;
            let mut board = demo_obj("Banderole", MeshKind::Cube, pos);
            board.transform = board.transform.with_scale(Vec3::new(0.15, 1.5, 7.0));
            board.transform.rotation = basis_rotation(f.fwd, f.up);
            board.color = c;
            board.emissive = 0.35;
            board.group = "Décor".into();
            objects.push(board);
        }
    }

    // Voiture et fantôme.
    let start = track.frames[track.start_frame()];
    let start_rot = basis_rotation(start.fwd, start.up);
    for ghost in [false, true] {
        for part in car_model() {
            if ghost && !matches!(part.name, "Caisse" | "Habitacle" | "Capot") {
                continue;
            }
            let name = if ghost {
                format!("Fantôme {}", part.name)
            } else {
                part.name.to_string()
            };
            let mut o = demo_obj(&name, part.mesh, start.pos + start_rot * part.offset);
            o.transform = o.transform.with_scale(part.scale);
            o.transform.rotation = start_rot * part.rot;
            o.color = part.color;
            o.emissive = part.emissive;
            o.metallic = part.metallic;
            o.roughness = part.roughness;
            o.group = if ghost { "Fantôme".into() } else { "Voiture".into() };
            if ghost {
                o.color = [0.35, 0.85, 1.0];
                o.emissive = 0.8;
                o.opacity = 0.35;
                o.visible = false;
            }
            let cp = CarPart {
                index: objects.len(),
                offset: part.offset,
                local_rot: part.rot,
                front_wheel: part.front_wheel,
            };
            objects.push(o);
            if ghost {
                layout.ghost.push(cp);
            } else {
                layout.car.push(cp);
            }
        }
    }

    // Émetteurs de particules (posés sur de minuscules objets suivant la voiture).
    let mut smoke = demo_obj("Fumée", MeshKind::Sphere, start.pos);
    smoke.transform = smoke.transform.with_scale(Vec3::splat(0.05));
    smoke.particle_emitter = Some(crate::runtime::particles::ParticleEmitter {
        enabled: false,
        rate: 0.0,
        lifetime_min: 0.5,
        lifetime_max: 1.0,
        speed_min: 0.5,
        speed_max: 2.0,
        direction: [0.0, 1.0, 0.0],
        spread: 0.7,
        size_min: 0.5,
        size_max: 1.2,
        color: [0.85, 0.85, 0.88],
        start_alpha: 0.4,
        gravity: -0.6,
        drag: 1.2,
    });
    layout.smoke = Some(objects.len());
    objects.push(smoke);
    let mut flame = demo_obj("Flamme", MeshKind::Sphere, start.pos);
    flame.transform = flame.transform.with_scale(Vec3::splat(0.05));
    flame.particle_emitter = Some(crate::runtime::particles::ParticleEmitter {
        enabled: false,
        rate: 0.0,
        lifetime_min: 0.15,
        lifetime_max: 0.35,
        speed_min: 6.0,
        speed_max: 12.0,
        direction: [0.0, 0.0, -1.0],
        spread: 0.18,
        size_min: 0.12,
        size_max: 0.26,
        color: [1.0, 0.55, 0.1],
        start_alpha: 0.6,
        gravity: 0.0,
        drag: 2.0,
    });
    layout.flame = Some(objects.len());
    objects.push(flame);

    layout.headlight = Some(0);
    let point_lights = vec![PointLight {
        position: (start.pos + Vec3::Y * 3.0).to_array(),
        color: [1.0, 0.95, 0.8],
        intensity: 1.6,
        range: 34.0,
        ..PointLight::default()
    }];

    let hud = |id: &str, anchor: HudAnchor, offset: [f32; 2], size: f32, content: &str| HudWidget {
        id: id.into(),
        anchor,
        offset,
        size: [0.0, size],
        kind: HudWidgetKind::Text {
            content: content.into(),
            binding: HudBinding::None,
        },
    };
    let mut hud_widgets: Vec<HudWidget> = Vec::new();
    // Panneaux sombres translucides derrière les blocs de texte (lisibles sur ciel clair).
    // Fichiers du dépôt : natif seulement (le web n'embarque pas ce dossier).
    #[cfg(not(target_arch = "wasm32"))]
    {
        let panel = |id: &str, anchor: HudAnchor, offset: [f32; 2], size: [f32; 2], file: &str| HudWidget {
            id: id.into(),
            anchor,
            offset,
            size,
            kind: HudWidgetKind::Image {
                path: format!("{}/assets/herroad/{file}", env!("CARGO_MANIFEST_DIR")),
            },
        };
        hud_widgets.push(panel("hr_panel_time", HudAnchor::TopLeft, [14.0, 12.0], [460.0, 176.0], "panel_time.png"));
        hud_widgets.push(panel("hr_panel_speed", HudAnchor::BottomRight, [-14.0, -14.0], [320.0, 138.0], "panel_speed.png"));
    }
    hud_widgets.extend([
        hud("hr_time", HudAnchor::TopLeft, [40.0, 26.0], 42.0, ""),
        hud("hr_lap", HudAnchor::TopLeft, [40.0, 80.0], 22.0, ""),
        hud("hr_best", HudAnchor::TopLeft, [40.0, 110.0], 17.0, ""),
        hud("hr_cp", HudAnchor::TopLeft, [40.0, 140.0], 21.0, ""),
        hud("hr_speed", HudAnchor::BottomRight, [-112.0, -52.0], 66.0, ""),
        hud("hr_unit", HudAnchor::BottomRight, [-36.0, -58.0], 22.0, "km/h"),
        hud("hr_turbo", HudAnchor::BottomRight, [-36.0, -112.0], 20.0, ""),
        HudWidget {
            id: "hr_gauge".into(),
            anchor: HudAnchor::BottomRight,
            offset: [-34.0, -30.0],
            size: [252.0, 12.0],
            kind: HudWidgetKind::Gauge {
                binding: HudBinding::Health,
                max: 1.0,
                color: [1.0, 0.55, 0.1],
            },
        },
        hud("hr_center", HudAnchor::Center, [0.0, -120.0], 130.0, ""),
        hud("hr_sub", HudAnchor::Center, [0.0, 10.0], 30.0, ""),
        hud("hr_sub2", HudAnchor::Center, [0.0, 54.0], 21.0, ""),
        hud("hr_help", HudAnchor::BottomLeft, [26.0, -56.0], 16.0, ""),
        hud("hr_help2", HudAnchor::BottomLeft, [26.0, -30.0], 16.0, ""),
    ]);

    let scene = Scene {
        objects,
        imported,
        point_lights,
        // Heure dorée : soleil bas et chaud (longues ombres), ciel qui rosit à l'horizon,
        // brume tiède qui fond les montagnes lointaines.
        light: Light {
            dir: [0.72, 0.46, 0.38],
            color: [1.0, 0.84, 0.62],
            ambient: 0.34,
        },
        sky: Sky {
            horizon_color: [0.96, 0.78, 0.62],
            zenith_color: [0.16, 0.34, 0.74],
            fog_color: [0.83, 0.76, 0.72],
            fog_density: 0.0014,
            bloom_intensity: 0.5,
            sun_glow: 1.5,
            fog_height_base: 0.0,
            fog_height_falloff: 0.0,
        },
        hud_widgets,
        arcade_hud: true,
        version: Scene::CURRENT_VERSION,
        ..Default::default()
    };
    let _ = FRAME_STEP;
    (scene, layout, track)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_has_car_ghost_gates_and_generated_meshes() {
        let (scene, layout, track) = herroad_build();
        assert!(scene.imported.len() >= 3, "relief + route + barrières au moins");
        assert!(scene.imported.iter().all(|m| !m.data.vertices.is_empty()));
        assert_eq!(layout.gates.len(), track.checkpoints.len() + 1);
        assert!(layout.car.len() >= 10);
        assert!(!layout.ghost.is_empty());
        for p in layout.car.iter().chain(&layout.ghost) {
            assert!(p.index < scene.objects.len());
        }
        assert!(layout.smoke.is_some() && layout.flame.is_some());
    }
}
