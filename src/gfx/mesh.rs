//! Données de mesh côté CPU + layout de vertex pour wgpu.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[derive(Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Sommet skinné (Sprint 86) : `Vertex` + jusqu'à 4 os influents et leurs poids
/// (convention glTF `JOINTS_0`/`WEIGHTS_0`, cf. `scene::import::VertexSkin`). Un type
/// **séparé** de `Vertex` plutôt qu'un ajout de champs à `Vertex` : ça laisse tous les
/// meshes statiques (primitives, imports glTF sans skin) et leur pipeline inchangés —
/// seul un mesh réellement skinné paie le coût des attributs supplémentaires.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
    /// Indices dans la palette de matrices de joints (cf. `Renderer` — Sprint 86).
    /// `u32` plutôt que `u16` (format du glTF source) : format de vertex GPU plus simple
    /// et plus largement pris en charge, au prix de 8 octets non significatifs par sommet
    /// — négligeable face au reste du vertex.
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

impl SkinnedVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SkinnedVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 44,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32x4,
                },
                wgpu::VertexAttribute {
                    offset: 60,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[derive(Default)]
pub struct SkinnedMeshData {
    pub vertices: Vec<SkinnedVertex>,
    pub indices: Vec<u32>,
}

/// Mesh chargé côté GPU (buffers prêts à dessiner).
pub struct GpuMesh {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub num_indices: u32,
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, data: &MeshData) -> Self {
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        GpuMesh {
            vertex_buf,
            index_buf,
            num_indices: data.indices.len() as u32,
        }
    }

    /// Identique à `new`, pour un mesh skinné (Sprint 86). `GpuMesh` lui-même ne connaît
    /// que des buffers bruts — indépendant du format de vertex, seul l'upload diffère.
    pub fn new_skinned(device: &wgpu::Device, data: &SkinnedMeshData) -> Self {
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skinned_vertices"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skinned_indices"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        GpuMesh {
            vertex_buf,
            index_buf,
            num_indices: data.indices.len() as u32,
        }
    }
}

/// Cube unitaire centré sur l'origine, normales par face.
pub fn cube(color: [f32; 3]) -> MeshData {
    // (normale, 4 coins dans le sens trigonométrique)
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // +X
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
        // -X
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        // +Y
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        // -Y
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ],
        ),
        // +Z
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        // -Z
        (
            [0.0, 0.0, -1.0],
            [
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
    ];

    // UV des 4 coins de chaque face (repère trigonométrique).
    let face_uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let base = vertices.len() as u32;
        for (k, pos) in corners.into_iter().enumerate() {
            vertices.push(Vertex {
                position: pos,
                normal,
                color,
                uv: face_uv[k],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    MeshData { vertices, indices }
}

/// Sphère UV de rayon 0.5 centrée sur l'origine.
pub fn sphere(color: [f32; 3]) -> MeshData {
    use std::f32::consts::PI;
    let sectors = 24u32;
    let stacks = 16u32;
    let radius = 0.5;

    let mut vertices = Vec::new();
    for i in 0..=stacks {
        let phi = PI * i as f32 / stacks as f32; // 0..π (du pôle nord au sud)
        let (sp, cp) = phi.sin_cos();
        for j in 0..=sectors {
            let theta = 2.0 * PI * j as f32 / sectors as f32;
            let (st, ct) = theta.sin_cos();
            let n = [sp * ct, cp, sp * st];
            vertices.push(Vertex {
                position: [radius * n[0], radius * n[1], radius * n[2]],
                normal: n,
                color,
                uv: [j as f32 / sectors as f32, i as f32 / stacks as f32],
            });
        }
    }

    let mut indices = Vec::new();
    let row = sectors + 1;
    for i in 0..stacks {
        for j in 0..sectors {
            let a = i * row + j;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    MeshData { vertices, indices }
}

/// Plan unitaire (1×1) dans le plan XZ, normale +Y. À mettre à l'échelle via le Transform.
pub fn plane(color: [f32; 3]) -> MeshData {
    let n = [0.0, 1.0, 0.0];
    let vertices = vec![
        Vertex {
            position: [-0.5, 0.0, -0.5],
            normal: n,
            color,
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.0, -0.5],
            normal: n,
            color,
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.0, 0.5],
            normal: n,
            color,
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.0, 0.5],
            normal: n,
            color,
            uv: [0.0, 1.0],
        },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    MeshData { vertices, indices }
}

/// Terrain : grille 1×1 (plan XZ) subdivisée avec un relief doux procédural.
/// Hauteur et normales analytiques ; à mettre à l'échelle via le Transform.
pub fn terrain(color: [f32; 3]) -> MeshData {
    const RES: u32 = 24; // quads par côté
    const AMP: f32 = 0.08; // amplitude du relief
    const FREQ: f32 = 8.0;
    let h = |x: f32, z: f32| AMP * (x * FREQ).sin() * (z * FREQ).cos();

    let mut vertices = Vec::new();
    for iz in 0..=RES {
        for ix in 0..=RES {
            let x = ix as f32 / RES as f32 - 0.5;
            let z = iz as f32 / RES as f32 - 0.5;
            let y = h(x, z);
            // Normale analytique : (-dh/dx, 1, -dh/dz) normalisé.
            let dhdx = AMP * FREQ * (x * FREQ).cos() * (z * FREQ).cos();
            let dhdz = -AMP * FREQ * (x * FREQ).sin() * (z * FREQ).sin();
            let n = [-dhdx, 1.0, -dhdz];
            let inv = 1.0 / (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            vertices.push(Vertex {
                position: [x, y, z],
                normal: [n[0] * inv, n[1] * inv, n[2] * inv],
                color,
                uv: [ix as f32 / RES as f32, iz as f32 / RES as f32],
            });
        }
    }
    let mut indices = Vec::new();
    let row = RES + 1;
    for iz in 0..RES {
        for ix in 0..RES {
            let a = iz * row + ix;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    MeshData { vertices, indices }
}

/// Cylindre unitaire : rayon 0.5, hauteur 1 (y de -0.5 à 0.5), axe +Y.
pub fn cylinder(color: [f32; 3]) -> MeshData {
    use std::f32::consts::PI;
    let sectors = 24u32;
    let radius = 0.5;
    let half = 0.5;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Paroi latérale : deux anneaux (bas/haut) avec normales radiales.
    for j in 0..=sectors {
        let theta = 2.0 * PI * j as f32 / sectors as f32;
        let (st, ct) = theta.sin_cos();
        let n = [ct, 0.0, st];
        let u = j as f32 / sectors as f32;
        vertices.push(Vertex {
            position: [radius * ct, -half, radius * st],
            normal: n,
            color,
            uv: [u, 1.0],
        });
        vertices.push(Vertex {
            position: [radius * ct, half, radius * st],
            normal: n,
            color,
            uv: [u, 0.0],
        });
    }
    for j in 0..sectors {
        let a = j * 2;
        let b = a + 1;
        let c = a + 2;
        let d = a + 3;
        indices.extend_from_slice(&[a, c, b, b, c, d]);
    }

    // Capuchons (centre + couronne) en haut (+Y) et en bas (-Y).
    for &(y, ny) in &[(half, 1.0f32), (-half, -1.0f32)] {
        let center = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, y, 0.0],
            normal: [0.0, ny, 0.0],
            color,
            uv: [0.5, 0.5],
        });
        let ring_start = vertices.len() as u32;
        for j in 0..=sectors {
            let theta = 2.0 * PI * j as f32 / sectors as f32;
            let (st, ct) = theta.sin_cos();
            vertices.push(Vertex {
                position: [radius * ct, y, radius * st],
                normal: [0.0, ny, 0.0],
                color,
                uv: [0.5 + 0.5 * ct, 0.5 + 0.5 * st],
            });
        }
        for j in 0..sectors {
            let a = ring_start + j;
            let b = ring_start + j + 1;
            // Orientation pour que la normale pointe vers l'extérieur.
            if ny > 0.0 {
                indices.extend_from_slice(&[center, b, a]);
            } else {
                indices.extend_from_slice(&[center, a, b]);
            }
        }
    }
    MeshData { vertices, indices }
}

/// Capsule unitaire : rayon 0.25, hauteur totale 1 (cylindre + deux demi-sphères), axe +Y.
pub fn capsule(color: [f32; 3]) -> MeshData {
    use std::f32::consts::PI;
    let sectors = 24u32;
    let cap_stacks = 8u32; // anneaux par demi-sphère
    let radius = 0.25;
    let cyl_half = 0.5 - radius; // partie cylindrique : 0.25 de chaque côté

    // Repère « visage » : patch sombre sur l'hémisphère haut (la tête), côté -Z local
    // — c'est la direction que `Physics::face_direction` fait correspondre au
    // déplacement (yaw=0 ⇒ le personnage avance vers -Z, cf. les tests
    // `camera_relative_move_*`). Sans ça, la capsule est parfaitement symétrique en
    // rotation : impossible de voir à l'écran vers où le personnage regarde une fois
    // que la rotation-vers-le-déplacement a été ajoutée (demandé le 2026-07-12).
    const FACE_COLOR: [f32; 3] = [0.08, 0.08, 0.08];
    const FACE_ROW_MIN: u32 = 1;
    const FACE_ROW_MAX: u32 = 5;
    const FACE_CENTER: f32 = 3.0 * PI / 2.0; // -Z
    const FACE_HALF_WIDTH: f32 = 0.55; // ± ~31°
    let angular_dist = |theta: f32| {
        let d = (theta - FACE_CENTER).rem_euclid(2.0 * PI);
        d.min(2.0 * PI - d)
    };

    let mut vertices = Vec::new();
    // Une grille (stack, sector) d'un pôle à l'autre, en décalant le centre des
    // hémisphères de ±cyl_half pour insérer le tube central.
    let rows = cap_stacks * 2 + 1;
    for i in 0..=rows {
        // phi : 0 (pôle haut) → π (pôle bas)
        let t = i as f32 / rows as f32;
        let phi = PI * t;
        let (sp, cp) = phi.sin_cos();
        // décalage vertical : hémisphère haut centré en +cyl_half, bas en -cyl_half
        let y_off = if cp >= 0.0 { cyl_half } else { -cyl_half };
        for j in 0..=sectors {
            let theta = 2.0 * PI * j as f32 / sectors as f32;
            let (st, ct) = theta.sin_cos();
            let n = [sp * ct, cp, sp * st];
            let is_face =
                (FACE_ROW_MIN..=FACE_ROW_MAX).contains(&i) && angular_dist(theta) < FACE_HALF_WIDTH;
            vertices.push(Vertex {
                position: [radius * n[0], radius * cp + y_off, radius * n[2]],
                normal: n,
                color: if is_face { FACE_COLOR } else { color },
                uv: [j as f32 / sectors as f32, t],
            });
        }
    }

    let mut indices = Vec::new();
    let row = sectors + 1;
    for i in 0..rows {
        for j in 0..sectors {
            let a = i * row + j;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    MeshData { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_marks_the_minus_z_face_with_a_distinct_color() {
        let base = [0.45, 0.7, 0.5];
        let data = capsule(base);
        let is_marked = |v: &Vertex| v.color != base;
        // Un sommet proche de -Z (avant, cf. `Physics::face_direction`), sur la tête
        // (z négatif dominant, y au-dessus du centre), doit porter la couleur du
        // visage — sinon on ne peut pas voir vers où le personnage regarde.
        assert!(
            data.vertices
                .iter()
                .any(|v| v.position[2] < -0.15 && v.position[1] > 0.0 && is_marked(v)),
            "aucun sommet marqué trouvé côté -Z (avant)"
        );
        // À l'opposé (+Z, dos), aucun sommet ne doit porter cette couleur.
        assert!(
            !data
                .vertices
                .iter()
                .any(|v| v.position[2] > 0.15 && v.position[1] > 0.0 && is_marked(v)),
            "un sommet marqué a été trouvé côté +Z (dos) — le repère visage doit rester devant"
        );
    }
}
