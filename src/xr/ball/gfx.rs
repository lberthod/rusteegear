//! Rendu de Ball, pensé pour le GPU mobile du Quest : deux maillages (cube,
//! sphère) dessinés par instances, un soleil avec **ombres portées** (une
//! carte d'ombre 2048² en 16 bits, calculée une fois par image pour les deux
//! yeux, filtrée sur 4 échantillons), reflets de Blinn-Phong, brume, MSAA 4×
//! résolu directement dans la swapchain. Pas d'image HDR ni de
//! post-traitement : ~6 appels de dessin par image.

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::xr::math::EyeView;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;
const SHADOW_SIZE: u32 = 2048;
/// Demi-largeur (m) de la zone couverte par la carte d'ombre, autour du trou.
const SHADOW_EXTENT: f32 = 8.0;
const SAMPLES: u32 = 4;
pub const MAX_INSTANCES: usize = 1536;
/// Direction **vers** le soleil.
pub const SUN: Vec3 = Vec3::new(0.35, 1.0, 0.45);
/// Couleur du ciel à l'horizon (fond et brume).
const HORIZON: [f64; 3] = [0.66, 0.76, 0.88];

const SHADER: &str = r#"
struct Eye {
    view_proj: mat4x4<f32>,
    // xyz : position de l'œil ; w : fondu (1 = image normale, 0 = noir).
    pos: vec4<f32>,
    light_vp: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> eye: Eye;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs(
    @location(0) p: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) color: vec4<f32>,
) -> Out {
    let model = mat4x4<f32>(m0, m1, m2, m3);
    let world = model * vec4<f32>(p, 1.0);
    var out: Out;
    out.clip = eye.view_proj * world;
    out.world = world.xyz;
    // Échelles non uniformes : la normale suit l'inverse transposée, ici
    // approchée en divisant par l'échelle au carré de chaque axe.
    let s = vec3<f32>(dot(m0.xyz, m0.xyz), dot(m1.xyz, m1.xyz), dot(m2.xyz, m2.xyz));
    out.normal = normalize((model * vec4<f32>(n / s, 0.0)).xyz);
    out.color = color;
    return out;
}

fn lit(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let lp = eye.light_vp * vec4<f32>(world + n * 0.015, 1.0);
    let uv = lp.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || lp.z > 1.0) {
        return 1.0;
    }
    let t = 1.0 / 2048.0;
    let z = lp.z - 0.0015;
    var s = textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2<f32>(-0.7, -0.7) * t, z);
    s += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2<f32>(0.7, -0.7) * t, z);
    s += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2<f32>(-0.7, 0.7) * t, z);
    s += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2<f32>(0.7, 0.7) * t, z);
    return s * 0.25;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    var base = in.color.rgb;
    let mode = in.color.a;
    // mode 2 : herbe en damier de 1 m.
    if (mode > 1.5) {
        let c = floor(in.world.x) + floor(in.world.z);
        let odd = abs(c - 2.0 * floor(c * 0.5));
        base = mix(base, base * 0.86, odd);
    }
    let n = normalize(in.normal);
    let sun = normalize(vec3<f32>(0.35, 1.0, 0.45));
    let v = normalize(eye.pos.xyz - in.world);
    let shadow = lit(in.world, n);
    let diffuse = max(dot(n, sun), 0.0) * shadow;
    let h = normalize(sun + v);
    let spec = pow(max(dot(n, h), 0.0), 40.0) * 0.35 * shadow;
    // Ciel au-dessus, rebond chaud du sol en dessous.
    let sky = mix(vec3<f32>(0.36, 0.33, 0.28), vec3<f32>(0.52, 0.6, 0.75), n.y * 0.5 + 0.5);
    var rgb = base * (sky * 0.55 + vec3<f32>(1.0, 0.95, 0.85) * diffuse * 0.85);
    if (mode < 1.5) {
        rgb += vec3<f32>(spec);
    }
    // mode 0.5 : émissif (boutons, repères, rayon de visée).
    if (mode > 0.25 && mode < 0.75) {
        rgb = base;
    }
    let d = distance(in.world, eye.pos.xyz);
    let fog = clamp((d - 10.0) / 45.0, 0.0, 1.0);
    rgb = mix(rgb, vec3<f32>(0.66, 0.76, 0.88), fog);
    return vec4<f32>(rgb * eye.pos.w, 1.0);
}

struct Light { vp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> light: Light;

@vertex
fn vs_shadow(
    @location(0) p: vec3<f32>,
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(m0, m1, m2, m3);
    return light.vp * model * vec4<f32>(p, 1.0);
}
"#;

/// Une instance : matrice modèle et couleur (`a` = mode : 1 matière, 2 herbe,
/// 0.5 émissif).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
}

impl Instance {
    pub fn new(model: Mat4, color: Vec4) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            color: color.to_array(),
        }
    }

    pub fn boxed(center: Vec3, half: Vec3, rot: Quat, color: Vec4) -> Self {
        Self::new(Mat4::from_scale_rotation_translation(half, rot, center), color)
    }
}

/// Matrice vue-projection du soleil, centrée sur `center`.
pub fn light_view_proj(center: Vec3) -> Mat4 {
    let dir = SUN.normalize();
    let view = glam::camera::rh::view::look_at_mat4(center + dir * 20.0, center, Vec3::Y);
    let proj = glam::camera::rh::proj::directx::orthographic(
        -SHADOW_EXTENT,
        SHADOW_EXTENT,
        -SHADOW_EXTENT,
        SHADOW_EXTENT,
        0.5,
        45.0,
    );
    proj * view
}

pub struct Gfx {
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    cube_range: std::ops::Range<u32>,
    sphere_range: std::ops::Range<u32>,
    instances: wgpu::Buffer,
    eye_buffers: [wgpu::Buffer; 2],
    eye_groups: [wgpu::BindGroup; 2],
    light_buffer: wgpu::Buffer,
    light_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl Gfx {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        use wgpu::util::DeviceExt as _;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ball"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ball-eye"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ball-light"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let shadow_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("ball-shadow"),
                size: wgpu::Extent3d {
                    width: SHADOW_SIZE,
                    height: SHADOW_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SHADOW_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ball-shadow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let eye_buffers = [0, 1].map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ball-eye"),
                size: 144,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let eye_groups = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ball-eye"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: eye_buffers[i].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                    },
                ],
            })
        });
        let light_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ball-light"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let light_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ball-light"),
            layout: &light_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }],
        });
        let mesh_layout = wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
        };
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4
            ],
        };
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ball"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ball"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[mesh_layout.clone(), instance_layout.clone()],
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
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ball-shadow"),
            bind_group_layouts: &[Some(&light_layout)],
            immediate_size: 0,
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ball-shadow"),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &[mesh_layout, instance_layout],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let mut verts = crate::xr::test_scene::cube_vertices();
        let cube_range = 0..verts.len() as u32;
        verts.extend(sphere_vertices(12, 16));
        let sphere_range = cube_range.end..verts.len() as u32;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ball-mesh"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ball-instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let target = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: SAMPLES,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        Self {
            pipeline,
            shadow_pipeline,
            vertices,
            cube_range,
            sphere_range,
            instances,
            eye_buffers,
            eye_groups,
            light_buffer,
            light_group,
            shadow_view,
            msaa: target("ball-msaa", format),
            depth: target("ball-depth", DEPTH_FORMAT),
        }
    }

    /// Dessine l'ombre puis les deux yeux. `eyes` en coordonnées monde,
    /// `shadow_center` : centre de la zone ombrée (le trou en cours), `fade` :
    /// 1 = image normale, 0 = noir (transition entre trous).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: [EyeView; 2],
        targets: [&wgpu::TextureView; 2],
        cubes: &[Instance],
        spheres: &[Instance],
        shadow_center: Vec3,
        fade: f32,
    ) {
        let light_vp = light_view_proj(shadow_center);
        queue.write_buffer(
            &self.light_buffer,
            0,
            bytemuck::cast_slice(&light_vp.to_cols_array()),
        );
        for (buffer, e) in self.eye_buffers.iter().zip(eyes) {
            let mut data = [0.0f32; 36];
            data[..16].copy_from_slice(&e.view_proj().to_cols_array());
            data[16..19].copy_from_slice(&e.position.to_array());
            data[19] = fade.clamp(0.0, 1.0);
            data[20..36].copy_from_slice(&light_vp.to_cols_array());
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&data));
        }
        let n_cubes = cubes.len().min(MAX_INSTANCES) as u32;
        let all: Vec<Instance> = cubes
            .iter()
            .chain(spheres)
            .take(MAX_INSTANCES)
            .copied()
            .collect();
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&all));
        let n_all = all.len() as u32;
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ball-shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.light_group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.draw(self.cube_range.clone(), 0..n_cubes);
            pass.draw(self.sphere_range.clone(), n_cubes..n_all);
        }
        let f = f64::from(fade.clamp(0.0, 1.0));
        for (target, group) in targets.into_iter().zip(&self.eye_groups) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ball-eye"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: HORIZON[0] * f,
                            g: HORIZON[1] * f,
                            b: HORIZON[2] * f,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
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
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.draw(self.cube_range.clone(), 0..n_cubes);
            pass.draw(self.sphere_range.clone(), n_cubes..n_all);
        }
        queue.submit([encoder.finish()]);
    }
}

/// Sphère unité basse définition (`rings` × `segments`), faces sortantes CCW.
fn sphere_vertices(rings: u32, segments: u32) -> Vec<[f32; 6]> {
    let point = |r: u32, s: u32| {
        let theta = std::f32::consts::PI * r as f32 / rings as f32;
        let phi = std::f32::consts::TAU * s as f32 / segments as f32;
        let p = Vec3::new(theta.sin() * phi.cos(), theta.cos(), -theta.sin() * phi.sin());
        [p.x, p.y, p.z, p.x, p.y, p.z]
    };
    let mut out = Vec::with_capacity((rings * segments * 6) as usize);
    for r in 0..rings {
        for s in 0..segments {
            let (a, b, c, d) = (point(r, s), point(r + 1, s), point(r + 1, s + 1), point(r, s + 1));
            out.extend([a, b, c, a, c, d]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(super::SHADER).expect("WGSL valide");
        naga::valid::Validator::new(Default::default(), naga::valid::Capabilities::all())
            .validate(&module)
            .expect("module validé");
    }

    #[test]
    fn the_sun_sees_the_hole_it_is_centred_on() {
        let c = glam::Vec3::new(0.0, 0.0, -14.0);
        let p = super::light_view_proj(c) * (c + glam::Vec3::new(1.0, 1.0, -2.0)).extend(1.0);
        let p = p / p.w;
        assert!(p.x.abs() < 1.0 && p.y.abs() < 1.0 && (0.0..1.0).contains(&p.z));
    }
}
