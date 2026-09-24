//! Interface en VR (phase 5 de la roadmap) : l'écran plat n'existe pas dans un
//! casque, l'interface egui est donc rendue **dans des textures** puis posée
//! dans le monde sur des panneaux — un menu flottant devant le joueur, piloté
//! au **rayon de la manette droite** (gâchette = clic), et un petit affichage au
//! **poignet gauche** (vie, score).
//!
//! Géométrie pure (placement, intersection rayon → pixel) testée ici ; le rendu
//! (`VrUi`) est commun à l'APK et au simulateur, comme le reste de `xr::content`.

use glam::{Mat4, Quat, Vec2, Vec3};

use super::math::EyeView;

/// Un panneau plan dans le monde : centre, orientation (le panneau est dans le
/// plan XY local, face visible vers +Z local), taille en mètres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPose {
    pub center: Vec3,
    pub rotation: Quat,
    pub size: Vec2,
}

impl PanelPose {
    /// Panneau à `distance` devant la tête, face à elle, légèrement sous
    /// l'horizon du regard (lacet seul : il reste vertical quand on baisse la
    /// tête pour l'ouvrir).
    pub fn in_front_of(head: Vec3, look: Vec3, distance: f32, size: Vec2) -> Self {
        let flat = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
        let yaw = (-flat.x).atan2(-flat.z);
        Self {
            center: head + flat * distance - Vec3::Y * 0.15,
            rotation: Quat::from_rotation_y(yaw),
            size,
        }
    }

    /// Panneau tourné vers la tête depuis `center` (affichage au poignet),
    /// **sans roulis** : ses bords horizontaux restent horizontaux, quel que
    /// soit l'endroit d'où on le regarde.
    pub fn facing(center: Vec3, head: Vec3, size: Vec2) -> Self {
        let to_head = (head - center).normalize_or(Vec3::Z);
        let yaw = to_head.x.atan2(to_head.z);
        let pitch = -to_head.y.clamp(-1.0, 1.0).asin();
        Self {
            center,
            rotation: Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch),
            size,
        }
    }

    /// Matrice modèle du quad unité (−0,5..0,5) vers le monde.
    pub fn model(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::new(self.size.x, self.size.y, 1.0),
            self.rotation,
            self.center,
        )
    }

    /// Rayon `origin` + t·`dir` → point touché en coordonnées de texture
    /// (u vers la droite, v vers le bas, dans \[0, 1\]) et distance ; `None` si le
    /// rayon rate le panneau ou part dans l'autre sens.
    pub fn hit(&self, origin: Vec3, dir: Vec3) -> Option<(Vec2, f32)> {
        let inv = self.rotation.inverse();
        let o = inv * (origin - self.center);
        let d = inv * dir.normalize_or_zero();
        if d.z.abs() < 1e-6 {
            return None;
        }
        let t = -o.z / d.z;
        if t <= 0.0 {
            return None;
        }
        let p = o + d * t;
        let uv = Vec2::new(p.x / self.size.x + 0.5, 0.5 - p.y / self.size.y);
        ((0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y)).then_some((uv, t))
    }
}

/// Format des textures de panneau : egui peint en couleurs gamma, sur une cible
/// non sRGB (comme l'éditeur) ; le shader du quad les linéarise avant de les
/// écrire dans la swapchain sRGB.
const PANEL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const QUAD_WGSL: &str = r#"
struct Quad { mvp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> quad: Quad;
@group(1) @binding(0) var panel: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, 0.5),
    );
    let c = corners[i];
    var out: Out;
    out.pos = quad.mvp * vec4<f32>(c, 0.0, 1.0);
    out.uv = vec2<f32>(c.x + 0.5, 0.5 - c.y);
    return out;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    // egui : alpha prémultiplié, couleurs gamma → linéaire prémultiplié.
    let c = textureSample(panel, samp, in.uv);
    if (c.a <= 0.0) { discard; }
    return vec4<f32>(to_linear(c.rgb / c.a) * c.a, c.a);
}
"#;

/// Un panneau : son contexte egui (mémoire, polices) et sa texture.
struct PanelTarget {
    ctx: egui::Context,
    egui: egui_wgpu::Renderer,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    px: (u32, u32),
    pixels_per_point: f32,
}

/// Pointeur d'un panneau pour une image : position en pixels de texture
/// (`None` = rayon hors du panneau) et gâchette tenue.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pointer {
    pub pos_px: Option<Vec2>,
    pub pressed: bool,
}

/// Rendu des panneaux d'interface VR.
pub struct VrUi {
    panels: [PanelTarget; 2],
    pipeline: wgpu::RenderPipeline,
    /// Une matrice par (œil, panneau) : chaque `write_buffer` s'applique à
    /// tout le lot soumis, il en faut donc une par dessin.
    uniforms: [(wgpu::Buffer, wgpu::BindGroup); 4],
    /// Bouton tenu à l'image précédente, par panneau (fronts → clics egui).
    pressed_was: [bool; 2],
}

/// Panneau du menu.
pub const MENU: usize = 0;
/// Panneau du poignet.
pub const WRIST: usize = 1;

impl VrUi {
    /// `target_format` : format des cibles d'œil (swapchain XR).
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vr_ui"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vr_ui_tex"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vr_ui_quad"),
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
        let panel = |px: (u32, u32), ppp: f32| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vr_ui_panel"),
                size: wgpu::Extent3d {
                    width: px.0,
                    height: px.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: PANEL_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vr_ui_panel"),
                layout: &tex_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            let egui = egui_wgpu::Renderer::new(
                device,
                PANEL_FORMAT,
                egui_wgpu::RendererOptions {
                    msaa_samples: 1,
                    depth_stencil_format: None,
                    dithering: true,
                    predictable_texture_filtering: false,
                },
            );
            PanelTarget {
                ctx: egui::Context::default(),
                egui,
                view,
                bind_group,
                px,
                pixels_per_point: ppp,
            }
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vr_ui_quad"),
            source: wgpu::ShaderSource::Wgsl(QUAD_WGSL.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vr_ui_quad"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vr_ui_quad"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = [0, 1, 2, 3].map(|_| {
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vr_ui_quad"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vr_ui_quad"),
                layout: &uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });
            (buf, bg)
        });
        Self {
            // Menu : 1024×768 px sur 1,0 × 0,75 m ; poignet : 512×256 px.
            panels: [panel((1024, 768), 1.6), panel((512, 256), 1.6)],
            pipeline,
            uniforms,
            pressed_was: [false; 2],
        }
    }

    /// Taille en pixels de texture d'un panneau.
    pub fn panel_px(&self, which: usize) -> Vec2 {
        let (w, h) = self.panels[which].px;
        Vec2::new(w as f32, h as f32)
    }

    /// Peint une image d'interface dans la texture du panneau `which` :
    /// `build` reçoit l'`Ui` racine (panneaux, boutons…), `pointer` le rayon
    /// de la manette converti en pixels. Transparent là où rien n'est peint.
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        which: usize,
        pointer: Pointer,
        build: impl FnMut(&mut egui::Ui),
    ) {
        let was = std::mem::replace(&mut self.pressed_was[which], pointer.pressed);
        let p = &mut self.panels[which];
        let ppp = p.pixels_per_point;
        let size_pt = egui::vec2(p.px.0 as f32 / ppp, p.px.1 as f32 / ppp);
        let mut events = Vec::new();
        match pointer.pos_px {
            Some(px) => {
                let pos = egui::pos2(px.x / ppp, px.y / ppp);
                events.push(egui::Event::PointerMoved(pos));
                if pointer.pressed != was {
                    events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: pointer.pressed,
                        modifiers: Default::default(),
                    });
                }
            }
            None => events.push(egui::Event::PointerGone),
        }
        p.ctx.set_pixels_per_point(ppp);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size_pt)),
            events,
            ..Default::default()
        };
        let out = p.ctx.run_ui(raw, build);
        for (id, delta) in &out.textures_delta.set {
            p.egui.update_texture(device, queue, *id, delta);
        }
        let primitives = p.ctx.tessellate(out.shapes, out.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [p.px.0, p.px.1],
            pixels_per_point: out.pixels_per_point,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let cmds = p
            .egui
            .update_buffers(device, queue, &mut encoder, &primitives, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vr_ui_paint"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &p.view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            p.egui.render(&mut pass, &primitives, &screen);
        }
        for id in &out.textures_delta.free {
            p.egui.free_texture(id);
        }
        queue.submit(cmds.into_iter().chain(std::iter::once(encoder.finish())));
    }

    /// Dessine les panneaux visibles (`poses[i]` pour le panneau `i`) dans les
    /// deux yeux, par-dessus l'image finale (pas de test de profondeur :
    /// l'interface reste lisible même derrière un arbre). Poses et yeux en
    /// coordonnées **monde**.
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: [EyeView; 2],
        targets: [&wgpu::TextureView; 2],
        poses: [Option<PanelPose>; 2],
    ) {
        if poses.iter().all(Option::is_none) {
            return;
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        for (e, (eye, target)) in eyes.iter().zip(targets).enumerate() {
            let vp = eye.view_proj();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vr_ui_draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            for (i, pose) in poses.iter().enumerate() {
                let Some(pose) = pose else { continue };
                let (buf, bg) = &self.uniforms[e * 2 + i];
                let mvp = vp * pose.model();
                queue.write_buffer(buf, 0, bytemuck::cast_slice(&mvp.to_cols_array()));
                pass.set_bind_group(0, bg, &[]);
                pass.set_bind_group(1, &self.panels[i].bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
        }
        queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_panel_faces_the_player_in_front_of_the_head() {
        let head = Vec3::new(2.0, 1.6, 5.0);
        let look = Vec3::new(1.0, -0.4, 0.0);
        let p = PanelPose::in_front_of(head, look, 1.3, Vec2::new(1.0, 0.75));
        // Devant, dans la direction du regard (à plat), un peu plus bas.
        assert!((p.center.x - 3.3).abs() < 1e-4 && (p.center.z - 5.0).abs() < 1e-4);
        assert!(p.center.y < head.y);
        // Face visible (+Z local) tournée vers la tête.
        let normal = p.rotation * Vec3::Z;
        assert!(normal.dot((head - p.center).normalize()) > 0.95);
    }

    #[test]
    fn a_ray_to_the_panel_center_hits_its_middle() {
        let p = PanelPose::in_front_of(
            Vec3::new(0.0, 1.6, 0.0),
            Vec3::NEG_Z,
            1.0,
            Vec2::new(1.0, 0.5),
        );
        let origin = Vec3::new(0.0, 1.6, 0.0);
        let (uv, t) = p.hit(origin, p.center - origin).expect("touché");
        assert!((uv - Vec2::splat(0.5)).length() < 1e-4, "{uv:?}");
        assert!(t > 0.9);
        // Coin haut gauche du panneau → (0, 0) ; hors panneau → rien ; derrière → rien.
        let top_left = p.center + p.rotation * Vec3::new(-0.49, 0.24, 0.0);
        let (uv, _) = p.hit(origin, top_left - origin).expect("coin");
        assert!(uv.x < 0.02 && uv.y < 0.03, "{uv:?}");
        assert!(p.hit(origin, Vec3::new(1.0, 0.0, -0.2)).is_none());
        assert!(p.hit(origin, Vec3::Z).is_none());
    }

    #[test]
    fn wrist_panel_faces_the_head() {
        let head = Vec3::new(0.0, 1.6, 0.0);
        let wrist = Vec3::new(-0.25, 1.1, -0.35);
        let p = PanelPose::facing(wrist, head, Vec2::new(0.16, 0.08));
        assert!((p.rotation * Vec3::Z).dot((head - wrist).normalize()) > 0.999);
    }

    #[test]
    fn panel_quad_shader_is_valid() {
        let module = naga::front::wgsl::parse_str(QUAD_WGSL).expect("WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validation");
    }
}
