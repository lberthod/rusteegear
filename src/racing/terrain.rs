//! Relief autour du circuit : une grille de hauteurs qui épouse la route (talus qui la portent
//! quand elle est en hauteur, vallée sous le pont), une prairie vallonnée, puis des montagnes
//! à l'horizon. Sert à la fois au maillage affiché, au placement des arbres et à la détection
//! de chute (une voiture qui passe sous le relief est ramenée au dernier point de passage).

use glam::{Vec2, Vec3};

use super::track::{FRAME_STEP, Track};
use crate::gfx::mesh::{MeshData, Vertex};

/// Côté d'une maille (m).
const CELL: f32 = 7.0;
/// Marge de relief autour de l'emprise du circuit (m).
const MARGIN: f32 = 300.0;
/// Pente des talus (m de dénivelé par m horizontal).
const EMBANKMENT_SLOPE: f32 = 0.6;
/// Rayon d'influence d'une portion de route sur le relief (m).
const INFLUENCE: f32 = 70.0;
/// La chaussée est posée 1 m au-dessus du relief qui la porte.
const ROAD_DROP: f32 = 1.0;

#[derive(Clone, Debug, Default)]
pub struct Terrain {
    min: Vec2,
    nx: usize,
    nz: usize,
    heights: Vec<f32>,
    /// Altitude de la prairie plane (m).
    pub base: f32,
}

fn hash(x: f32, z: f32) -> f32 {
    let v = (x * 127.1 + z * 311.7).sin() * 43758.547;
    v - v.floor()
}

/// Bruit de valeur lissé, 0..1.
fn noise(x: f32, z: f32) -> f32 {
    let (xi, zi) = (x.floor(), z.floor());
    let (fx, fz) = (x - xi, z - zi);
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let a = hash(xi, zi);
    let b = hash(xi + 1.0, zi);
    let c = hash(xi, zi + 1.0);
    let d = hash(xi + 1.0, zi + 1.0);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(x: f32, z: f32) -> f32 {
    noise(x, z) * 0.55 + noise(x * 2.1 + 7.0, z * 2.1 - 3.0) * 0.3 + noise(x * 4.3 - 5.0, z * 4.3 + 9.0) * 0.15
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Terrain {
    pub fn build(track: &Track) -> Terrain {
        let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for f in &track.frames {
            min = min.min(Vec2::new(f.pos.x, f.pos.z));
            max = max.max(Vec2::new(f.pos.x, f.pos.z));
        }
        min -= Vec2::splat(MARGIN);
        max += Vec2::splat(MARGIN);
        let nx = ((max.x - min.x) / CELL).ceil() as usize + 1;
        let nz = ((max.y - min.y) / CELL).ceil() as usize + 1;
        let base = track.min_y - 3.0;

        // Portions de route qui portent le relief : ni le pont (les piliers le tiennent, la
        // vallée passe dessous), ni les vides de saut.
        let bridge = track.crossing.map(|(_, j)| j);
        let n = track.frames.len();
        let carries = |i: usize| -> bool {
            let f = &track.frames[i];
            if f.void {
                return false;
            }
            if let Some(j) = bridge {
                let d = (i as i32 - j as i32).unsigned_abs() as usize;
                let d = d.min(n - d);
                if (d as f32) * FRAME_STEP < 100.0 {
                    return false;
                }
            }
            true
        };
        // Grille de hachage des portions porteuses (cases de 40 m) pour borner la recherche.
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = Default::default();
        for i in 0..n {
            if carries(i) {
                let p = track.frames[i].pos;
                grid.entry(((p.x / 40.0).floor() as i32, (p.z / 40.0).floor() as i32))
                    .or_default()
                    .push(i);
            }
        }
        // Sous-échantillon pour la distance à la route (relief lointain).
        let far: Vec<Vec2> = track
            .frames
            .iter()
            .step_by(4)
            .map(|f| Vec2::new(f.pos.x, f.pos.z))
            .collect();

        let mut heights = vec![base; nx * nz];
        for iz in 0..nz {
            for ix in 0..nx {
                let x = min.x + ix as f32 * CELL;
                let z = min.y + iz as f32 * CELL;
                let p = Vec2::new(x, z);
                // Enveloppe des cônes de talus sous chaque portion de route proche.
                let mut env = f32::MIN;
                let mut near = f32::MAX;
                let (cx, cz) = ((x / 40.0).floor() as i32, (z / 40.0).floor() as i32);
                let reach = (INFLUENCE / 40.0).ceil() as i32;
                for gx in cx - reach..=cx + reach {
                    for gz in cz - reach..=cz + reach {
                        let Some(list) = grid.get(&(gx, gz)) else {
                            continue;
                        };
                        for &i in list {
                            let a = &track.frames[i];
                            let b = &track.frames[(i + 1) % n];
                            if !carries((i + 1) % n) {
                                continue;
                            }
                            let (a2, b2) = (Vec2::new(a.pos.x, a.pos.z), Vec2::new(b.pos.x, b.pos.z));
                            let ab = b2 - a2;
                            let len = ab.length().max(1e-4);
                            let dir = ab / len;
                            let t_raw = (p - a2).dot(dir) / len;
                            let t = t_raw.clamp(0.0, 1.0);
                            // Écart latéral à la droite du tronçon, et dépassement au-delà de ses
                            // extrémités : le plateau de la chaussée ne s'étend qu'en travers,
                            // jamais le long de la route (sinon une pente ferait remonter le
                            // relief au-dessus de la chaussée).
                            let lateral = (p - a2).perp_dot(dir).abs();
                            let beyond = (t_raw - t).abs() * len;
                            if lateral > INFLUENCE || beyond > INFLUENCE {
                                continue;
                            }
                            let y = a.pos.y + (b.pos.y - a.pos.y) * t;
                            // Dévers : la chaussée penche, son bord bas est plus bas que l'axe.
                            let r2 = Vec2::new(a.right.x, a.right.z);
                            let r_len = r2.length().max(1e-4);
                            let signed = (p - a2).dot(r2 / r_len);
                            let inside = signed.clamp(-a.half_width, a.half_width);
                            let y_bank = y + (a.right.y / r_len) * inside;
                            let v = y_bank
                                - ROAD_DROP
                                - EMBANKMENT_SLOPE * (lateral - a.half_width - 2.5).max(0.0)
                                - 1.2 * beyond;
                            env = env.max(v);
                            near = near.min((lateral * lateral + beyond * beyond).sqrt());
                        }
                    }
                }
                let dmin = far.iter().map(|q| q.distance(p)).fold(f32::MAX, f32::min);
                // Prairie vallonnée + montagnes qui montent avec l'éloignement.
                let meadow = base + 1.5 * fbm(x * 0.012, z * 0.012) + 1.2 * fbm(x * 0.04, z * 0.04);
                let ridge = 85.0 * smoothstep(150.0, 330.0, dmin) * (0.45 + 0.9 * fbm(x * 0.006 + 3.0, z * 0.006 - 1.0));
                // Près de la route le relief suit le talus (la prairie est arasée sous la
                // chaussée) ; il rejoint la prairie à quelques dizaines de mètres.
                let blend = smoothstep(11.0, 48.0, near);
                let ground = if env > f32::MIN {
                    env + (env.max(meadow) - env) * blend
                } else {
                    meadow
                };
                heights[iz * nx + ix] = ground + ridge;
            }
        }
        Terrain {
            min,
            nx,
            nz,
            heights,
            base,
        }
    }

    fn at(&self, ix: usize, iz: usize) -> f32 {
        self.heights[iz.min(self.nz - 1) * self.nx + ix.min(self.nx - 1)]
    }

    /// Altitude du relief en (x, z) — interpolation bilinéaire, bord le plus proche hors grille.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        if self.heights.is_empty() {
            return f32::MIN;
        }
        let fx = ((x - self.min.x) / CELL).clamp(0.0, (self.nx - 1) as f32);
        let fz = ((z - self.min.y) / CELL).clamp(0.0, (self.nz - 1) as f32);
        let (ix, iz) = (fx.floor() as usize, fz.floor() as usize);
        let (tx, tz) = (fx - ix as f32, fz - iz as f32);
        let h0 = self.at(ix, iz) * (1.0 - tx) + self.at(ix + 1, iz) * tx;
        let h1 = self.at(ix, iz + 1) * (1.0 - tx) + self.at(ix + 1, iz + 1) * tx;
        h0 * (1.0 - tz) + h1 * tz
    }

    /// Normale du relief en (x, z), par différences finies.
    pub fn normal(&self, x: f32, z: f32) -> Vec3 {
        let e = CELL * 0.6;
        let dx = self.height(x + e, z) - self.height(x - e, z);
        let dz = self.height(x, z + e) - self.height(x, z - e);
        Vec3::new(-dx, 2.0 * e, -dz).normalize_or_zero()
    }

    /// Maillage du relief, coloré par altitude et pente (prairie, terre, roche, neige).
    pub fn mesh(&self) -> MeshData {
        let mut mesh = MeshData::default();
        if self.heights.is_empty() {
            return mesh;
        }
        for iz in 0..self.nz {
            for ix in 0..self.nx {
                let x = self.min.x + ix as f32 * CELL;
                let z = self.min.y + iz as f32 * CELL;
                let h = self.at(ix, iz);
                let n = self.normal(x, z);
                let slope = 1.0 - n.y;
                let grass = 0.85 + 0.3 * fbm(x * 0.09, z * 0.09);
                let mut c = [0.20 * grass, 0.40 * grass, 0.14 * grass];
                // Prairie sèche par plaques.
                let dry = smoothstep(0.55, 0.8, fbm(x * 0.02 + 11.0, z * 0.02));
                for k in 0..3 {
                    c[k] += ([0.20, 0.10, -0.03][k]) * dry;
                }
                // Roche sur les pentes fortes, neige en altitude.
                let rock = smoothstep(0.12, 0.3, slope) + smoothstep(38.0, 60.0, h - self.base) * 0.6;
                let rock = rock.clamp(0.0, 1.0);
                let rock_c = [0.42, 0.39, 0.36];
                for k in 0..3 {
                    c[k] = c[k] * (1.0 - rock) + rock_c[k] * rock;
                }
                let snow = smoothstep(62.0, 80.0, h - self.base);
                for k in 0..3 {
                    c[k] = c[k] * (1.0 - snow) + 0.93 * snow;
                }
                mesh.vertices.push(Vertex {
                    position: [x, h, z],
                    normal: n.to_array(),
                    color: c,
                    uv: [0.0, 0.0],
                });
            }
        }
        for iz in 0..self.nz - 1 {
            for ix in 0..self.nx - 1 {
                let a = (iz * self.nx + ix) as u32;
                let b = a + 1;
                let c = a + self.nx as u32;
                let d = c + 1;
                mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::racing::track::TrackSpec;

    #[test]
    fn terrain_carries_the_road_without_poking_through_it() {
        let track = Track::build(&TrackSpec::herroad());
        let terrain = Terrain::build(&track);
        let mut checked = 0;
        for f in track.frames.iter().step_by(3) {
            if f.void {
                continue;
            }
            let h = terrain.height(f.pos.x, f.pos.z);
            // Jamais au-dessus de la chaussée (sinon elle serait enterrée) ; le pont surplombe
            // la vallée, les portions porteuses restent à ~1 m dessous.
            assert!(h < f.pos.y - 0.2, "relief au-dessus de la route à s={} : {h} vs {}", f.s, f.pos.y);
            // Et sous les deux bords (dévers).
            for side in [-1.0_f32, 1.0] {
                let e = f.pos + f.right * side * (f.half_width - 0.3);
                let he = terrain.height(e.x, e.z);
                assert!(he < e.y - 0.1, "bord enterré à s={} côté {side} : {he} vs {}", f.s, e.y);
            }
            checked += 1;
        }
        assert!(checked > 100);
    }

    #[test]
    fn mountains_rise_far_from_the_road_and_the_mesh_is_valid() {
        let track = Track::build(&TrackSpec::herroad());
        let terrain = Terrain::build(&track);
        let mesh = terrain.mesh();
        assert!(mesh.vertices.len() > 5000);
        let max = mesh.vertices.len() as u32;
        assert!(mesh.indices.iter().all(|&i| i < max));
        let peak = mesh.vertices.iter().map(|v| v.position[1]).fold(f32::MIN, f32::max);
        assert!(peak - terrain.base > 40.0, "des montagnes à l'horizon ({peak})");
    }
}
