//! Données de mesh côté CPU + layout de vertex pour wgpu.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
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
            ],
        }
    }
}

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
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
}

/// Cube unitaire centré sur l'origine, normales par face.
pub fn cube(color: [f32; 3]) -> MeshData {
    // (normale, 4 coins dans le sens trigonométrique)
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // +X
        ([1.0, 0.0, 0.0], [[0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5]]),
        // -X
        ([-1.0, 0.0, 0.0], [[-0.5, -0.5, 0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [-0.5, 0.5, 0.5]]),
        // +Y
        ([0.0, 1.0, 0.0], [[-0.5, 0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]]),
        // -Y
        ([0.0, -1.0, 0.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [-0.5, -0.5, -0.5]]),
        // +Z
        ([0.0, 0.0, 1.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]]),
        // -Z
        ([0.0, 0.0, -1.0], [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]]),
    ];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let base = vertices.len() as u16;
        for pos in corners {
            vertices.push(Vertex { position: pos, normal, color });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    MeshData { vertices, indices }
}

/// Sphère UV de rayon 0.5 centrée sur l'origine.
pub fn sphere(color: [f32; 3]) -> MeshData {
    use std::f32::consts::PI;
    let sectors = 24u16;
    let stacks = 16u16;
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
        Vertex { position: [-0.5, 0.0, -0.5], normal: n, color },
        Vertex { position: [0.5, 0.0, -0.5], normal: n, color },
        Vertex { position: [0.5, 0.0, 0.5], normal: n, color },
        Vertex { position: [-0.5, 0.0, 0.5], normal: n, color },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    MeshData { vertices, indices }
}
