//! Circuit de HerRoad : une ligne centrale fermée (spline de Catmull-Rom) échantillonnée
//! tous les 2 m en « repères » (position, base orthonormée, dévers), d'où l'on tire à la
//! fois le maillage affiché (route, bordures, barrières) et les requêtes de collision
//! (hauteur/normale du sol, murs). Une seule source de vérité : ce que l'on voit est
//! exactement ce sur quoi la voiture roule.
//!
//! Aucune dépendance au moteur au-delà de `glam` et du type de sommet `gfx::mesh` :
//! tout ce module se teste sans GPU ni fenêtre.

use std::collections::HashMap;

use glam::{Vec2, Vec3};

use crate::gfx::mesh::{MeshData, Vertex};

/// Pas entre deux repères le long de la ligne centrale (m).
pub const FRAME_STEP: f32 = 2.0;
/// Demi-largeur de la chaussée (m) — 15 m de large, comme un circuit Stadium.
pub const HALF_WIDTH: f32 = 7.5;
/// Hauteur des barrières latérales (m).
pub const WALL_HEIGHT: f32 = 1.2;
/// Largeur des bandes rouge/blanc du bord de route (m).
const CURB_W: f32 = 0.9;
/// Côté d'une case de la grille spatiale des requêtes (m).
const GRID_CELL: f32 = 8.0;

/// Nature du revêtement : adhérence, poussée moteur et vitesse de pointe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Asphalt,
    Dirt,
    Ice,
}

impl Surface {
    /// Multiplicateur d'adhérence latérale (1 = asphalte).
    pub fn grip(self) -> f32 {
        match self {
            Surface::Asphalt => 1.0,
            Surface::Dirt => 0.62,
            Surface::Ice => 0.2,
        }
    }

    /// Multiplicateur de poussée moteur (patinage sur terre/glace).
    pub fn engine(self) -> f32 {
        match self {
            Surface::Asphalt => 1.0,
            Surface::Dirt => 0.82,
            Surface::Ice => 0.55,
        }
    }

    /// Multiplicateur de vitesse de pointe.
    pub fn top_speed(self) -> f32 {
        match self {
            Surface::Asphalt => 1.0,
            Surface::Dirt => 0.88,
            Surface::Ice => 0.95,
        }
    }

    fn color(self) -> [f32; 3] {
        match self {
            Surface::Asphalt => [0.27, 0.28, 0.32],
            Surface::Dirt => [0.46, 0.31, 0.17],
            Surface::Ice => [0.62, 0.82, 0.93],
        }
    }
}

/// Coupe transversale de la piste à un point de la ligne centrale.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub pos: Vec3,
    pub fwd: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub half_width: f32,
    /// Abscisse curviligne (m) depuis la ligne de départ.
    pub s: f32,
    /// Vide (saut) : aucune chaussée entre ce repère et ses voisins.
    pub void: bool,
    pub surface: Surface,
    pub wall_l: bool,
    pub wall_r: bool,
}

/// Plaque de turbo posée sur la chaussée.
#[derive(Clone, Copy, Debug)]
pub struct Boost {
    pub pos: Vec3,
    pub fwd: Vec3,
    pub up: Vec3,
    pub frame: usize,
}

/// Saut : rampe, vide, réception. Distances en mètres, `at` en fraction du tour.
#[derive(Clone, Copy, Debug)]
pub struct JumpSpec {
    pub at: f32,
    pub ramp: f32,
    pub gap: f32,
    pub rise: f32,
    pub land: f32,
}

/// Description déclarative d'un circuit.
#[derive(Clone, Debug)]
pub struct TrackSpec {
    /// Points de contrôle (x, z) de la spline fermée, dans l'ordre de parcours.
    pub points: Vec<Vec2>,
    /// Fraction de la spline où se trouve la ligne d'arrivée (repère 0) : toutes les autres
    /// fractions de la description s'entendent **à partir de cette ligne**.
    pub start: f32,
    pub jumps: Vec<JumpSpec>,
    /// Amplitude des collines (m).
    pub hills: f32,
    /// Hauteur du pont quand la piste se croise elle-même (m).
    pub bridge_rise: f32,
    /// Intervalles (fractions du tour) en terre / glace.
    pub dirt: Vec<(f32, f32)>,
    pub ice: Vec<(f32, f32)>,
    /// Intervalles (fractions du tour) **sans** barrières (bords ouverts sur le vide).
    pub open: Vec<(f32, f32)>,
    /// Positions (fractions du tour) des plaques de turbo.
    pub boosts: Vec<f32>,
    /// Positions (fractions du tour) des points de passage, ligne d'arrivée exclue.
    pub checkpoints: Vec<f32>,
}

#[derive(Clone, Copy)]
struct Tri {
    a: Vec3,
    b: Vec3,
    c: Vec3,
    na: Vec3,
    nb: Vec3,
    nc: Vec3,
    frame: u32,
}

impl Tri {
    /// Hauteur et normale interpolée du triangle à la verticale de (x, z).
    fn sample(&self, x: f32, z: f32) -> Option<(f32, Vec3)> {
        let (v0x, v0z) = (self.b.x - self.a.x, self.b.z - self.a.z);
        let (v1x, v1z) = (self.c.x - self.a.x, self.c.z - self.a.z);
        let (px, pz) = (x - self.a.x, z - self.a.z);
        let d = v0x * v1z - v1x * v0z;
        if d.abs() < 1e-9 {
            return None;
        }
        let u = (px * v1z - v1x * pz) / d;
        let v = (v0x * pz - px * v0z) / d;
        const EPS: f32 = 1e-4;
        if u < -EPS || v < -EPS || u + v > 1.0 + EPS {
            return None;
        }
        let y = self.a.y + u * (self.b.y - self.a.y) + v * (self.c.y - self.a.y);
        let n = self.na * (1.0 - u - v) + self.nb * u + self.nc * v;
        Some((y, n.normalize_or_zero()))
    }
}

/// Segment de barrière (en plan) : normale `n` tournée vers l'intérieur de la piste.
#[derive(Clone, Copy)]
struct Wall {
    a: Vec2,
    b: Vec2,
    n: Vec2,
    y0: f32,
    y1: f32,
}

/// Résultat d'une requête de sol.
#[derive(Clone, Copy, Debug)]
pub struct Ground {
    pub y: f32,
    pub normal: Vec3,
    pub frame: usize,
    pub surface: Surface,
}

/// Contact avec une barrière : normale (plan) à suivre et profondeur d'enfoncement.
#[derive(Clone, Copy, Debug)]
pub struct WallHit {
    pub normal: Vec2,
    pub depth: f32,
}

pub struct Track {
    pub frames: Vec<Frame>,
    pub length: f32,
    /// Indices de repère des points de passage (ligne d'arrivée exclue, repère 0).
    pub checkpoints: Vec<usize>,
    pub boosts: Vec<Boost>,
    /// Intervalles de repères occupés par un vide de saut (début inclus, fin exclue).
    pub voids: Vec<(usize, usize)>,
    /// Repères des deux passages du croisement (le second est sur le pont).
    pub crossing: Option<(usize, usize)>,
    pub min_y: f32,
    tris: Vec<Tri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    walls: Vec<Wall>,
    wall_grid: HashMap<(i32, i32), Vec<u32>>,
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Courbe 0 → 1 de pente nulle en 0 et de pente ≥ 1 en 1 (raccord doux d'une rampe).
fn ease(x: f32) -> f32 {
    const E: f32 = 0.3;
    let x = x.clamp(0.0, 1.0);
    let v = if x < E { x * x / (2.0 * E) } else { x - E * 0.5 };
    v / (1.0 - E * 0.5)
}

fn wrap_dist(a: f32, b: f32, len: f32) -> f32 {
    let d = (a - b).abs();
    d.min(len - d)
}

/// `f` (fraction du tour) dans l'intervalle `(a, b)`, avec rebouclage si a > b.
fn in_interval(f: f32, (a, b): (f32, f32)) -> bool {
    if a <= b {
        f >= a && f < b
    } else {
        f >= a || f < b
    }
}

fn catmull(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, u: f32) -> Vec2 {
    let u2 = u * u;
    let u3 = u2 * u;
    0.5 * (2.0 * p1
        + (p2 - p0) * u
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * u3)
}

fn cell_of(x: f32, z: f32) -> (i32, i32) {
    ((x / GRID_CELL).floor() as i32, (z / GRID_CELL).floor() as i32)
}

impl Track {
    pub fn build(spec: &TrackSpec) -> Track {
        // 1. Spline fermée, échantillonnée finement puis rééchantillonnée à pas constant.
        let pts = &spec.points;
        let n_pts = pts.len();
        let mut dense: Vec<Vec2> = Vec::new();
        const SUB: usize = 48;
        for i in 0..n_pts {
            let p0 = pts[(i + n_pts - 1) % n_pts];
            let p1 = pts[i];
            let p2 = pts[(i + 1) % n_pts];
            let p3 = pts[(i + 2) % n_pts];
            for k in 0..SUB {
                dense.push(catmull(p0, p1, p2, p3, k as f32 / SUB as f32));
            }
        }
        let m = dense.len();
        let mut cum = vec![0.0_f32; m + 1];
        for i in 0..m {
            cum[i + 1] = cum[i] + dense[i].distance(dense[(i + 1) % m]);
        }
        let total = cum[m];
        let n = (total / FRAME_STEP).round().max(16.0) as usize;
        let length = n as f32 * FRAME_STEP;
        let mut xz: Vec<Vec2> = Vec::with_capacity(n);
        let mut seg = 0usize;
        for k in 0..n {
            let target = total * k as f32 / n as f32;
            while seg + 1 < m && cum[seg + 1] < target {
                seg += 1;
            }
            let span = (cum[seg + 1] - cum[seg]).max(1e-6);
            let t = ((target - cum[seg]) / span).clamp(0.0, 1.0);
            xz.push(dense[seg].lerp(dense[(seg + 1) % m], t));
        }
        xz.rotate_left(((spec.start.rem_euclid(1.0)) * n as f32).round() as usize % n);
        let s_of = |i: usize| i as f32 * FRAME_STEP;

        // 2. Croisement de la piste avec elle-même : le second passage devient un pont.
        let mut crossing: Option<(usize, usize)> = None;
        let mut best = 4.0_f32;
        for i in 0..n {
            for j in (i + 40)..n {
                if n - (j - i) < 40 {
                    continue;
                }
                let d = xz[i].distance(xz[j]);
                if d < best {
                    best = d;
                    crossing = Some((i, j));
                }
            }
        }

        // 3. Profil d'altitude : collines douces + pont + sauts.
        let mut zones: Vec<(f32, f32)> = vec![(0.0, 170.0)]; // (centre s, rayon) où le relief s'efface
        if let Some((i, j)) = crossing {
            zones.push((s_of(i), 90.0));
            zones.push((s_of(j), 130.0));
        }
        for j in &spec.jumps {
            let s0 = j.at * length;
            zones.push((s0 + (j.ramp + j.gap + j.land) * 0.5, (j.ramp + j.gap + j.land) * 0.5 + 40.0));
        }
        let height = |s: f32| -> f32 {
            let f = s / length;
            let mut mask = 1.0_f32;
            for &(c, r) in &zones {
                mask *= smoothstep(r * 0.55, r, wrap_dist(s, c, length));
            }
            let tau = std::f32::consts::TAU;
            let hills = spec.hills
                * (0.62 * (tau * (f * 2.0 + 0.13)).sin()
                    + 0.38 * (tau * (f * 5.0 + 0.41)).sin());
            let mut h = hills * mask;
            if let Some((_, j)) = crossing {
                let d = wrap_dist(s, s_of(j), length);
                h += spec.bridge_rise * (1.0 - smoothstep(26.0, 92.0, d));
            }
            for j in &spec.jumps {
                let s0 = j.at * length;
                let u = s - s0;
                if u < 0.0 || u > j.ramp + j.gap + j.land {
                    continue;
                }
                if u < j.ramp {
                    h += j.rise * ease(u / j.ramp);
                } else if u < j.ramp + j.gap {
                    h += j.rise;
                } else {
                    h += j.rise * ease(1.0 - (u - j.ramp - j.gap) / j.land);
                }
            }
            h
        };
        let ys: Vec<f32> = (0..n).map(|i| height(s_of(i))).collect();

        // 4. Courbure signée (gauche > 0) lissée → dévers.
        let mut turn = vec![0.0_f32; n];
        for i in 0..n {
            let t0 = (xz[i] - xz[(i + n - 1) % n]).normalize_or_zero();
            let t1 = (xz[(i + 1) % n] - xz[i]).normalize_or_zero();
            // Repère (x, z) : x à gauche quand on regarde vers +z ⇒ produit croisé
            // y = z0·x1 − x0·z1 > 0 pour un virage à gauche.
            let cross_y = t0.y * t1.x - t0.x * t1.y;
            let ang = cross_y.clamp(-1.0, 1.0).asin();
            turn[i] = ang / FRAME_STEP;
        }
        const SMOOTH: i32 = 6;
        let curv: Vec<f32> = (0..n)
            .map(|i| {
                let mut acc = 0.0;
                for k in -SMOOTH..=SMOOTH {
                    acc += turn[(i as i32 + k).rem_euclid(n as i32) as usize];
                }
                acc / (2 * SMOOTH + 1) as f32
            })
            .collect();

        // 5. Repères.
        let voids_of = |s: f32| -> bool {
            spec.jumps.iter().any(|j| {
                let s0 = j.at * length + j.ramp;
                s > s0 && s < s0 + j.gap
            })
        };
        let mut frames: Vec<Frame> = Vec::with_capacity(n);
        for i in 0..n {
            let s = s_of(i);
            let f = s / length;
            let pos = Vec3::new(xz[i].x, ys[i], xz[i].y);
            let prev = Vec3::new(xz[(i + n - 1) % n].x, ys[(i + n - 1) % n], xz[(i + n - 1) % n].y);
            let next = Vec3::new(xz[(i + 1) % n].x, ys[(i + 1) % n], xz[(i + 1) % n].y);
            let fwd = (next - prev).normalize_or_zero();
            let right0 = fwd.cross(Vec3::Y).normalize_or_zero();
            let up0 = right0.cross(fwd);
            let bank = (curv[i] * 11.0).clamp(-0.4, 0.4);
            let (sb, cb) = bank.sin_cos();
            // bank > 0 relève le bord droit (virage à gauche : l'intérieur reste bas).
            let right = right0 * cb + up0 * sb;
            let up = up0 * cb - right0 * sb;
            let surface = if spec.ice.iter().any(|&iv| in_interval(f, iv)) {
                Surface::Ice
            } else if spec.dirt.iter().any(|&iv| in_interval(f, iv)) {
                Surface::Dirt
            } else {
                Surface::Asphalt
            };
            let walled = !spec.open.iter().any(|&iv| in_interval(f, iv))
                && !spec.jumps.iter().any(|j| {
                    let s0 = j.at * length;
                    s > s0 - 20.0 && s < s0 + j.ramp + j.gap + j.land * 0.6
                });
            frames.push(Frame {
                pos,
                fwd,
                right,
                up,
                half_width: HALF_WIDTH,
                s,
                void: voids_of(s),
                surface,
                wall_l: walled,
                wall_r: walled,
            });
        }

        let mut voids = Vec::new();
        let mut i = 0;
        while i < n {
            if frames[i].void {
                let start = i;
                while i < n && frames[i].void {
                    i += 1;
                }
                voids.push((start, i));
            } else {
                i += 1;
            }
        }

        // Un point de passage ne doit jamais tomber sur un saut (rampe, vide ou réception) :
        // on le repousse après la réception, sinon une réapparition se ferait dans le vide.
        let in_jump = |idx: usize| {
            let s = idx as f32 * FRAME_STEP;
            spec.jumps.iter().any(|j| {
                let s0 = j.at * length;
                s > s0 - 12.0 && s < s0 + j.ramp + j.gap + j.land + 6.0
            })
        };
        let checkpoints: Vec<usize> = spec
            .checkpoints
            .iter()
            .map(|f| {
                let mut idx = ((f * n as f32).round() as usize).clamp(1, n - 2);
                while in_jump(idx) && idx < n - 2 {
                    idx += 1;
                }
                idx
            })
            .collect();
        let boosts: Vec<Boost> = spec
            .boosts
            .iter()
            .map(|f| {
                let idx = ((f * n as f32).round() as usize) % n;
                let fr = &frames[idx];
                Boost {
                    pos: fr.pos + fr.up * 0.06,
                    fwd: fr.fwd,
                    up: fr.up,
                    frame: idx,
                }
            })
            .collect();
        let min_y = ys.iter().copied().fold(f32::MAX, f32::min);

        let mut track = Track {
            frames,
            length,
            checkpoints,
            boosts,
            voids,
            crossing,
            min_y,
            tris: Vec::new(),
            grid: HashMap::new(),
            walls: Vec::new(),
            wall_grid: HashMap::new(),
        };
        track.build_collision();
        track
    }

    fn n(&self) -> usize {
        self.frames.len()
    }

    /// Bord gauche/droit d'un repère (position monde).
    pub fn edge(&self, i: usize, side: f32) -> Vec3 {
        let f = &self.frames[i % self.n()];
        f.pos + f.right * (side * f.half_width)
    }

    fn quad_exists(&self, i: usize) -> bool {
        let n = self.n();
        !self.frames[i].void && !self.frames[(i + 1) % n].void
    }

    fn build_collision(&mut self) {
        let n = self.n();
        let mut tris = Vec::new();
        for i in 0..n {
            if !self.quad_exists(i) {
                continue;
            }
            let (f0, f1) = (&self.frames[i], &self.frames[(i + 1) % n]);
            let l0 = self.edge(i, -1.0);
            let r0 = self.edge(i, 1.0);
            let l1 = self.edge(i + 1, -1.0);
            let r1 = self.edge(i + 1, 1.0);
            tris.push(Tri {
                a: l0,
                b: r0,
                c: l1,
                na: f0.up,
                nb: f0.up,
                nc: f1.up,
                frame: i as u32,
            });
            tris.push(Tri {
                a: r0,
                b: r1,
                c: l1,
                na: f0.up,
                nb: f1.up,
                nc: f1.up,
                frame: i as u32,
            });
        }
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            let min_x = t.a.x.min(t.b.x).min(t.c.x);
            let max_x = t.a.x.max(t.b.x).max(t.c.x);
            let min_z = t.a.z.min(t.b.z).min(t.c.z);
            let max_z = t.a.z.max(t.b.z).max(t.c.z);
            let (cx0, cz0) = cell_of(min_x, min_z);
            let (cx1, cz1) = cell_of(max_x, max_z);
            for cx in cx0..=cx1 {
                for cz in cz0..=cz1 {
                    grid.entry((cx, cz)).or_default().push(ti as u32);
                }
            }
        }
        self.tris = tris;
        self.grid = grid;

        let mut walls = Vec::new();
        for i in 0..n {
            if !self.quad_exists(i) {
                continue;
            }
            for (side, on) in [(-1.0_f32, self.frames[i].wall_l), (1.0, self.frames[i].wall_r)] {
                if !on {
                    continue;
                }
                let a3 = self.edge(i, side);
                let b3 = self.edge(i + 1, side);
                let a = Vec2::new(a3.x, a3.z);
                let b = Vec2::new(b3.x, b3.z);
                let d = (b - a).normalize_or_zero();
                // Normale vers l'intérieur : du bord vers la ligne centrale.
                let mut nrm = Vec2::new(-d.y, d.x);
                let c = self.frames[i].pos;
                if nrm.dot(Vec2::new(c.x - a.x, c.z - a.y)) < 0.0 {
                    nrm = -nrm;
                }
                walls.push(Wall {
                    a,
                    b,
                    n: nrm,
                    y0: a3.y.min(b3.y),
                    y1: a3.y.max(b3.y) + WALL_HEIGHT,
                });
            }
        }
        let mut wall_grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (wi, w) in walls.iter().enumerate() {
            let (cx0, cz0) = cell_of(w.a.x.min(w.b.x) - 2.0, w.a.y.min(w.b.y) - 2.0);
            let (cx1, cz1) = cell_of(w.a.x.max(w.b.x) + 2.0, w.a.y.max(w.b.y) + 2.0);
            for cx in cx0..=cx1 {
                for cz in cz0..=cz1 {
                    wall_grid.entry((cx, cz)).or_default().push(wi as u32);
                }
            }
        }
        self.walls = walls;
        self.wall_grid = wall_grid;
    }

    /// Chaussée la plus haute sous (x, z) sans dépasser `max_y` : le pont l'emporte sur la
    /// route qu'il surplombe seulement si la voiture est à son niveau.
    pub fn ground_at(&self, x: f32, z: f32, max_y: f32) -> Option<Ground> {
        let cell = self.grid.get(&cell_of(x, z))?;
        let mut best: Option<Ground> = None;
        for &ti in cell {
            let t = &self.tris[ti as usize];
            if let Some((y, normal)) = t.sample(x, z)
                && y <= max_y
                && best.is_none_or(|b| y > b.y)
            {
                let frame = t.frame as usize;
                best = Some(Ground {
                    y,
                    normal,
                    frame,
                    surface: self.frames[frame].surface,
                });
            }
        }
        best
    }

    /// Pénétration la plus profonde d'un disque de rayon `radius` dans une barrière.
    pub fn wall_hit(&self, pos: Vec3, radius: f32) -> Option<WallHit> {
        let cell = self.wall_grid.get(&cell_of(pos.x, pos.z))?;
        let p = Vec2::new(pos.x, pos.z);
        let mut best: Option<WallHit> = None;
        for &wi in cell {
            let w = &self.walls[wi as usize];
            if pos.y < w.y0 - 0.6 || pos.y > w.y1 + 0.4 {
                continue;
            }
            let ab = w.b - w.a;
            let len2 = ab.length_squared().max(1e-6);
            let t = ((p - w.a).dot(ab) / len2).clamp(0.0, 1.0);
            let q = w.a + ab * t;
            let inner = (p - w.a).dot(w.n);
            let dist = p.distance(q);
            // Le long du segment : profondeur mesurée sur la normale (une voiture passée de
            // l'autre côté est repoussée vers la piste) ; aux extrémités : distance au coin.
            let depth = if t > 0.0 && t < 1.0 {
                radius - inner
            } else if dist < radius {
                radius - dist
            } else {
                continue;
            };
            if depth > 0.0 && best.is_none_or(|b| depth > b.depth) {
                let normal = if t > 0.0 && t < 1.0 || dist < 1e-4 {
                    w.n
                } else {
                    (p - q).normalize_or_zero()
                };
                best = Some(WallHit { normal, depth });
            }
        }
        best
    }

    /// Repère le plus proche de `pos`, cherché autour de `hint` (±`window` repères).
    pub fn frame_near(&self, pos: Vec3, hint: usize, window: usize) -> usize {
        let n = self.n() as i32;
        let mut best = hint % self.n();
        let mut best_d = f32::MAX;
        for k in -(window as i32)..=(window as i32) {
            let i = (hint as i32 + k).rem_euclid(n) as usize;
            let d = self.frames[i].pos.distance_squared(pos);
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    }

    /// Repère le plus proche sur tout le circuit (recherche linéaire, hors boucle chaude).
    pub fn frame_global(&self, pos: Vec3) -> usize {
        self.frame_near(pos, 0, self.n() / 2)
    }

    /// Position d'apparition sur la grille de départ : juste après la ligne (repère 0).
    pub fn start_frame(&self) -> usize {
        3
    }

    // ------------------------------------------------------------------ maillages

    fn push_quad(
        out: &mut MeshData,
        p: [Vec3; 4],
        n: [Vec3; 4],
        color: [f32; 3],
    ) {
        let base = out.vertices.len() as u32;
        for k in 0..4 {
            out.vertices.push(Vertex {
                position: p[k].to_array(),
                normal: n[k].to_array(),
                color,
                uv: [0.0, 0.0],
            });
        }
        out.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Chaussée : bandes de bordure, asphalte, ligne médiane pointillée, damier d'arrivée.
    pub fn road_mesh(&self) -> MeshData {
        let mut mesh = MeshData::default();
        let n = self.n();
        for i in 0..n {
            if !self.quad_exists(i) {
                continue;
            }
            let (f0, f1) = (&self.frames[i], &self.frames[(i + 1) % n]);
            let hw = f0.half_width;
            let stripe = (i / 2) % 2 == 0;
            let dash = (i / 3) % 2 == 0;
            let base = f0.surface.color();
            // Ligne d'arrivée : vrai damier de 14 cases sur les deux premiers repères.
            if i < 2 {
                let cols = 14;
                for c in 0..cols {
                    let o0 = -hw + 2.0 * hw * c as f32 / cols as f32;
                    let o1 = -hw + 2.0 * hw * (c + 1) as f32 / cols as f32;
                    let black = (c + i) % 2 == 0;
                    let color = if black { [0.05, 0.05, 0.05] } else { [0.95, 0.95, 0.95] };
                    let p = |f: &Frame, o: f32| f.pos + f.right * o + f.up * 0.01;
                    Self::push_quad(
                        &mut mesh,
                        [p(f0, o0), p(f0, o1), p(f1, o1), p(f1, o0)],
                        [f0.up, f0.up, f1.up, f1.up],
                        color,
                    );
                }
                continue;
            }
            // Chaussée détaillée : bordure, filet blanc, ornières plus sombres, ligne médiane
            // pointillée, léger grain d'un tronçon à l'autre.
            let grain = 0.94 + 0.09 * ((i as f32 * 12.9898).sin() * 43758.547).fract().abs();
            let asphalt = [base[0] * grain, base[1] * grain, base[2] * grain];
            let groove = [asphalt[0] * 0.86, asphalt[1] * 0.86, asphalt[2] * 0.88];
            let line = 0.35;
            let inner = hw - CURB_W;
            let curb = if stripe { [0.86, 0.10, 0.08] } else { [0.92, 0.92, 0.92] };
            let white = [0.9, 0.9, 0.86];
            let paved = f0.surface == Surface::Asphalt;
            let strips: Vec<(f32, f32, [f32; 3])> = if paved {
                vec![
                    (-hw, -inner, curb),
                    (-inner, -inner + 0.3, asphalt),
                    (-inner + 0.3, -inner + 0.3 + line, white),
                    (-inner + 0.3 + line, -4.4, asphalt),
                    (-4.4, -2.6, groove),
                    (-2.6, -0.15, asphalt),
                    (-0.15, 0.15, if dash { white } else { asphalt }),
                    (0.15, 2.6, asphalt),
                    (2.6, 4.4, groove),
                    (4.4, inner - 0.3 - line, asphalt),
                    (inner - 0.3 - line, inner - 0.3, white),
                    (inner - 0.3, inner, asphalt),
                    (inner, hw, curb),
                ]
            } else {
                let edge = [base[0] * 0.7, base[1] * 0.7, base[2] * 0.7];
                vec![
                    (-hw, -inner, edge),
                    (-inner, -2.6, asphalt),
                    (-2.6, 2.6, groove),
                    (2.6, inner, asphalt),
                    (inner, hw, edge),
                ]
            };
            for (o0, o1, color) in strips {
                let p = |f: &Frame, o: f32| f.pos + f.right * o;
                Self::push_quad(
                    &mut mesh,
                    [p(f0, o0), p(f0, o1), p(f1, o1), p(f1, o0)],
                    [f0.up, f0.up, f1.up, f1.up],
                    color,
                );
            }
        }
        mesh
    }

    /// Barrières rouge/blanc le long des bords protégés, double face + arête haute.
    pub fn wall_mesh(&self) -> MeshData {
        let mut mesh = MeshData::default();
        let n = self.n();
        for i in 0..n {
            if !self.quad_exists(i) {
                continue;
            }
            let (f0, f1) = (&self.frames[i], &self.frames[(i + 1) % n]);
            let color = if (i / 3) % 2 == 0 {
                [0.88, 0.1, 0.08]
            } else {
                [0.93, 0.93, 0.93]
            };
            for (side, on) in [(-1.0_f32, f0.wall_l), (1.0, f0.wall_r)] {
                if !on {
                    continue;
                }
                let a = self.edge(i, side);
                let b = self.edge(i + 1, side);
                let inward = -f0.right * side;
                let (ta, tb) = (a + f0.up * WALL_HEIGHT, b + f1.up * WALL_HEIGHT);
                // Face intérieure (vue depuis la piste).
                Self::push_quad(&mut mesh, [a, b, tb, ta], [inward; 4], color);
                // Face extérieure.
                Self::push_quad(&mut mesh, [b, a, ta, tb], [-inward; 4], color);
                // Arête haute épaisse (0,4 m) pour donner du volume à la barrière.
                let out = -inward * 0.4;
                Self::push_quad(
                    &mut mesh,
                    [ta, tb, tb + out, ta + out],
                    [f0.up; 4],
                    [color[0] * 0.8, color[1] * 0.8, color[2] * 0.8],
                );
            }
        }
        mesh
    }
}

impl TrackSpec {
    /// Le circuit livré : un huit avec pont, collines, deux sauts, une portion de terre,
    /// une portion de glace, trois plaques de turbo et six points de passage.
    pub fn herroad() -> TrackSpec {
        // Huit de Gerono déformé (croisement à l'origine, virages larges aux deux lobes) :
        // x = A·sin t, z = B·sin t·cos t, puis quelques points déplacés à la main pour
        // casser la symétrie (épingle à gauche, chicane à droite).
        let a = 250.0_f32;
        let b = 150.0_f32;
        let mut points: Vec<Vec2> = Vec::new();
        let count = 20;
        for k in 0..count {
            let t = std::f32::consts::TAU * k as f32 / count as f32;
            points.push(Vec2::new(a * t.sin(), b * t.sin() * t.cos() * 1.6));
        }
        // Pincement de l'épingle du lobe gauche et écart de la chicane du lobe droit.
        points[5].x -= 30.0;
        points[6].y += 25.0;
        points[15].x += 25.0;
        points[14].y -= 20.0;
        // Les fractions ci-dessous sont écrites sur la courbe brute (repérées avec
        // `examples/herroad_map`) puis ramenées à la ligne d'arrivée par `r`.
        let start = 0.03_f32;
        let r = |f: f32| (f - start).rem_euclid(1.0);
        TrackSpec {
            points,
            start,
            jumps: vec![
                // Sur la diagonale qui remonte vers le pont.
                JumpSpec {
                    at: r(0.365),
                    ramp: 10.0,
                    gap: 14.0,
                    rise: 2.4,
                    land: 44.0,
                },
                // Sur la diagonale du retour, avant de passer sous le pont.
                JumpSpec {
                    at: r(0.89),
                    ramp: 12.0,
                    gap: 18.0,
                    rise: 3.0,
                    land: 52.0,
                },
            ],
            hills: 5.0,
            bridge_rise: 8.5,
            dirt: vec![(r(0.24), r(0.30))],
            ice: vec![(r(0.815), r(0.865))],
            open: vec![(r(0.10), r(0.15)), (r(0.56), r(0.62))],
            boosts: vec![r(0.05), r(0.225), r(0.43), r(0.60), r(0.955)],
            checkpoints: vec![r(0.16), r(0.30), r(0.45), r(0.60), r(0.78), r(0.95)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track::build(&TrackSpec::herroad())
    }

    #[test]
    fn frames_form_a_closed_loop_with_constant_spacing() {
        let t = track();
        let n = t.frames.len();
        assert!(t.length > 900.0, "circuit trop court : {} m", t.length);
        for i in 0..n {
            let a = t.frames[i].pos;
            let b = t.frames[(i + 1) % n].pos;
            let horiz = Vec2::new(a.x - b.x, a.z - b.z).length();
            assert!(
                (horiz - FRAME_STEP).abs() < 0.6,
                "repère {i} : pas horizontal {horiz} m (attendu ~{FRAME_STEP})"
            );
        }
    }

    #[test]
    fn the_track_crosses_itself_once_and_the_second_pass_is_a_bridge() {
        let t = track();
        let (i, j) = t.crossing.expect("un huit doit se croiser");
        let dy = t.frames[j].pos.y - t.frames[i].pos.y;
        assert!(dy > 6.0, "le pont doit surplomber la route ({dy} m)");
    }

    #[test]
    fn ground_query_finds_the_road_under_every_frame() {
        let t = track();
        for (i, f) in t.frames.iter().enumerate() {
            if f.void {
                continue;
            }
            let g = t
                .ground_at(f.pos.x, f.pos.z, f.pos.y + 1.0)
                .unwrap_or_else(|| panic!("pas de sol sous le repère {i}"));
            assert!((g.y - f.pos.y).abs() < 0.3, "repère {i} : {} vs {}", g.y, f.pos.y);
        }
    }

    #[test]
    fn under_the_bridge_the_low_road_is_found_not_the_deck() {
        let t = track();
        let (i, _) = t.crossing.unwrap();
        let f = t.frames[i];
        let g = t.ground_at(f.pos.x, f.pos.z, f.pos.y + 1.0).unwrap();
        assert!((g.y - f.pos.y).abs() < 0.5);
    }

    #[test]
    fn jumps_leave_a_gap_in_the_road() {
        let t = track();
        assert_eq!(t.voids.len(), 2);
        for &(a, b) in &t.voids {
            let mid = (a + b) / 2;
            let f = t.frames[mid];
            assert!(
                t.ground_at(f.pos.x, f.pos.z, f.pos.y + 1.0).is_none(),
                "le vide doit n'avoir aucun sol"
            );
        }
    }

    #[test]
    fn walls_push_a_car_back_towards_the_road() {
        let t = track();
        // Repère 40 : droit, protégé.
        let i = 40;
        let f = t.frames[i];
        assert!(f.wall_l && f.wall_r);
        let outside = f.pos + f.right * (f.half_width - 0.2);
        let hit = t.wall_hit(outside, 1.2).expect("collision attendue au bord");
        assert!(hit.depth > 0.5);
        let inward = -f.right;
        assert!(hit.normal.dot(Vec2::new(inward.x, inward.z)) > 0.8);
        assert!(t.wall_hit(f.pos, 1.2).is_none(), "pas de mur au centre");
    }

    #[test]
    fn meshes_are_not_empty_and_indices_are_valid() {
        let t = track();
        for mesh in [t.road_mesh(), t.wall_mesh()] {
            assert!(!mesh.vertices.is_empty());
            let max = mesh.vertices.len() as u32;
            assert!(mesh.indices.iter().all(|&i| i < max));
        }
    }
}
