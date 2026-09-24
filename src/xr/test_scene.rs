//! Scène de test VR commune à l'APK Quest (`xr::hello`) et au simulateur
//! desktop (`src/bin/quest_sim.rs`) : 5 cubes de 30 cm devant le joueur et une
//! dalle de sol, dans l'espace `STAGE` (origine au sol). Même code de rendu des
//! deux côtés — ce que montre le simulateur est ce que montrera le casque.

use glam::{Mat4, Vec3};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const SHADER: &str = r#"
struct Eye { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> eye: Eye;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>, @builtin(instance_index) i: u32) -> Out {
    // (centre.xyz, demi-taille) — 5 cubes de 30 cm devant le joueur, 1 dalle de sol.
    var placements = array<vec4<f32>, 6>(
        vec4<f32>(0.0, 1.3, -1.2, 0.15),
        vec4<f32>(-0.6, 1.0, -1.5, 0.15),
        vec4<f32>(0.6, 1.0, -1.5, 0.15),
        vec4<f32>(-1.2, 1.5, -2.5, 0.15),
        vec4<f32>(1.2, 1.5, -2.5, 0.15),
        vec4<f32>(0.0, -0.01, 0.0, 3.0),
    );
    var colors = array<vec3<f32>, 6>(
        vec3<f32>(0.95, 0.55, 0.15),
        vec3<f32>(0.25, 0.6, 0.95),
        vec3<f32>(0.3, 0.85, 0.45),
        vec3<f32>(0.9, 0.3, 0.4),
        vec3<f32>(0.85, 0.8, 0.3),
        vec3<f32>(0.35, 0.38, 0.42),
    );
    let pl = placements[i];
    var scale = vec3<f32>(pl.w);
    if (i == 5u) { scale.y = 0.01; }
    let world = pl.xyz + p * scale;
    let light = normalize(vec3<f32>(0.4, 1.0, 0.3));
    var out: Out;
    out.pos = eye.view_proj * vec4<f32>(world, 1.0);
    out.color = colors[i] * (0.35 + 0.65 * max(dot(n, light), 0.0));
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;

pub struct CubeScene {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    eye_buffers: [wgpu::Buffer; 2],
    eye_groups: [wgpu::BindGroup; 2],
    depth: wgpu::TextureView,
}

impl CubeScene {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        use wgpu::util::DeviceExt as _;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("xr-hello"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xr-eye"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let eye_buffers = [0, 1].map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("xr-eye"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let eye_groups = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("xr-eye"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: eye_buffers[i].as_entire_binding(),
                }],
            })
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xr-hello"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("xr-hello"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                }],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("xr-cube"),
            contents: bytemuck::cast_slice(&cube_vertices()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("xr-depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Self {
            pipeline,
            vertices,
            eye_buffers,
            eye_groups,
            depth,
        }
    }

    /// Une passe par œil (le multiview viendra en phase 2).
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        targets: [&wgpu::TextureView; 2],
        view_projs: [Mat4; 2],
    ) {
        for (buffer, vp) in self.eye_buffers.iter().zip(view_projs) {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&vp.to_cols_array()));
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        for (target, group) in targets.into_iter().zip(&self.eye_groups) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("xr-eye"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.08,
                            b: 0.14,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..36, 0..6);
        }
        queue.submit([encoder.finish()]);
    }
}

/// Cube unité (−1..1) : 36 sommets (position, normale), faces sortantes en CCW.
pub fn cube_vertices() -> Vec<[f32; 6]> {
    // (normale, « haut » de la face) ; la « droite » s'en déduit : `right = up × n`
    // donne `right × up = n`, donc l'ordre (−,−) → (+,−) → (+,+) est CCW vu de dehors.
    let faces: [(Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::Y),
        (Vec3::NEG_X, Vec3::Y),
        (Vec3::Y, Vec3::Z),
        (Vec3::NEG_Y, Vec3::Z),
        (Vec3::Z, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y),
    ];
    let mut out = Vec::with_capacity(36);
    for (n, up) in faces {
        let right = up.cross(n);
        let c = |a: f32, b: f32| {
            let p = n + right * a + up * b;
            [p.x, p.y, p.z, n.x, n.y, n.z]
        };
        out.extend([
            c(-1., -1.),
            c(1., -1.),
            c(1., 1.),
            c(-1., -1.),
            c(1., 1.),
            c(-1., 1.),
        ]);
    }
    out
}
