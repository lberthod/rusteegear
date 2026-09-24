//! Rendu VR stéréo (phase 1 de `docs/roadmapExportVRQuest24septembre.md`) :
//! la scène du jeu, avec tout le pipeline du moteur (ombres en cascade, ciel,
//! eau et sa réflexion, skinning, translucides, particules, bloom, tone
//! mapping), rendue une fois par œil dans des cibles fournies par l'appelant —
//! swapchain OpenXR sur le casque (`xr::hello`), textures du simulateur sur
//! desktop (`quest_sim`).

use glam::{Mat4, Vec3};

use super::*;
use crate::xr::math::{EyeView, NEAR, projection_from_fov, view_from_pose};

/// Profil de rendu VR (phase 2, mesuré sur le simulateur le 24 septembre 2026 :
/// le GPU, pas le remplissage, était chargé par la géométrie — 2 549 draw
/// calls par image stéréo dans Rivière). Distances de culling et de LOD du
/// feuillage × 0,6 (feuillage coupé à 27 m, objets moyens à 66 m, arbres en
/// impostor dès 24 m), carte d'ombre 1024, pas de réflexion planaire.
pub const VR_DRAW_DISTANCE_SCALE: f32 = 0.6;
const VR_SHADOW_SIZE: u32 = 1024;

/// Vignette de confort : noir transparent au centre, opaque aux bords, selon
/// l'intensité (phase 4 — réduit la cinétose du déplacement au stick).
const VIGNETTE_WGSL: &str = r#"
// 16 octets tout rond (tampon de 16) : pas de `vec3` de bourrage, aligné sur 16.
struct Vignette { strength: f32, _p0: f32, _p1: f32, _p2: f32 };
@group(0) @binding(0) var<uniform> vignette: Vignette;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: Out;
    out.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = p * 2.0 - 1.0;
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    // Zone dégagée plus large quand l'intensité est faible.
    let inner = mix(0.95, 0.35, vignette.strength);
    let a = smoothstep(inner, 1.05, length(in.uv)) * vignette.strength;
    return vec4<f32>(0.0, 0.0, 0.0, a);
}
"#;

impl Renderer {
    /// Renderer sur un device **déjà créé** par l'appelant (instance Vulkan du
    /// runtime OpenXR, ou device de la fenêtre du simulateur) : ni fenêtre, ni
    /// surface, ni UI egui. `format` = format des cibles passées à
    /// `render_views` (celui de la swapchain XR, sRGB).
    ///
    /// Qualité : profil VR (cf. `VR_DRAW_DISTANCE_SCALE`) — distances
    /// d'affichage réduites, ombres 1024, pas de réflexion planaire ;
    /// mono-échantillon (MSAA à venir avec le multiview).
    pub fn new_external(
        adapter: &wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Renderer {
        let size = winit::dpi::PhysicalSize::new(width.max(1), height.max(1));
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        let info = adapter.get_info();
        // Multiview si le device l'a activé (l'appelant le demande à la création
        // quand l'adaptateur l'expose : simulateur, runtime OpenXR).
        let device_multiview = device.features().contains(wgpu::Features::MULTIVIEW);
        log::info!(
            "GPU : {} ({:?}) — rendu VR, multiview {}",
            info.name,
            info.backend,
            if device_multiview { "oui" } else { "non" }
        );
        let mut renderer = Self::assemble(
            None,
            None,
            device,
            queue,
            config,
            size,
            1,
            VR_SHADOW_SIZE,
            format!("{:?}", info.backend),
            None,
            device_multiview,
        );
        renderer.draw_distance_scale = VR_DRAW_DISTANCE_SCALE;
        renderer.planar_reflections = false;
        renderer
    }

    /// Règle l'échelle des distances d'affichage (culling + LOD du feuillage),
    /// `1.0` = celles du jeu desktop — réglage du profil VR, exposé au
    /// simulateur (`QUEST_SIM_DRAW_DISTANCE`) pour mesurer son effet.
    pub fn set_draw_distance_scale(&mut self, scale: f32) {
        self.draw_distance_scale = scale.clamp(0.1, 4.0);
        // Le plan de dessin est mis en cache tant que la scène et la caméra
        // ne changent pas : le forcer à se reconstruire.
        self.last_render_hash = 0;
    }

    /// Rend la scène pour les deux yeux, en coordonnées **monde** (poses déjà
    /// passées par le rig, cf. `xr::rig::Rig::to_world`), dans `targets[0]`
    /// (œil gauche) et `targets[1]` (œil droit), toutes deux de
    /// `width`×`height` au format donné à `new_external`.
    ///
    /// Tout ce qui ne dépend pas de l'œil (culling, cascades d'ombre, lumières,
    /// tri des translucides, carte d'ombre elle-même) est calculé **une fois**
    /// depuis une caméra centrale englobant les deux frustums
    /// (`xr::rig::cull_camera`) : `app.camera` est remplacée le temps de l'image
    /// puis restaurée, la caméra de jeu n'est jamais modifiée. Seul l'uniform
    /// caméra change d'un œil à l'autre — d'où une soumission GPU par œil (un
    /// `write_buffer` s'applique à tout le lot soumis).
    pub fn render_views(
        &mut self,
        app: &mut AppState,
        eyes: [EyeView; 2],
        targets: [&wgpu::TextureView; 2],
        width: u32,
        height: u32,
    ) {
        let (w, h) = (width.max(1), height.max(1));
        self.anim_time = self.anim_clock.elapsed().as_secs_f32();
        self.sync_objects(&app.scene);
        self.sync_imported(&app.scene);
        self.sync_textures(&app.scene);

        let game_camera = app.camera.clone();
        let (cull_pos, cull_fwd, cull_fovy) = crate::xr::rig::cull_camera(&eyes);
        let cam = &mut app.camera;
        cam.distance = 1.0;
        cam.collision_distance = None;
        cam.ortho_height = 0.0;
        cam.snap = 0.0;
        cam.aspect = 1.0;
        cam.fovy = cull_fovy;
        cam.target = cull_pos + cull_fwd;
        // `eye() = target + distance·(cos p sin y, sin p, cos p cos y)` regarde
        // vers −offset : on inverse pour obtenir le regard `cull_fwd`.
        cam.pitch = (-cull_fwd.y).clamp(-1.0, 1.0).asin();
        cam.yaw = (-cull_fwd.x).atan2(-cull_fwd.z);

        self.main_viewport = (0.0, 0.0, w as f32, h as f32);
        self.ensure_reflection_target(&app.scene, (w / 2).max(1), (h / 2).max(1));
        self.write_uniforms(app);
        self.prepare_skinned_draws(&app.scene);
        self.ensure_xr_targets(w, h);
        // Lignes de debug de la frame (repères des manettes, rayon de pointage).
        let debug_count = self.upload_debug_lines(app);
        let vignette = self.vr_vignette.clamp(0.0, 1.0);
        if vignette > 0.01 {
            self.ensure_vignette();
            if let Some((_, buf, _)) = &self.vignette {
                self.queue
                    .write_buffer(buf, 0, bytemuck::cast_slice(&[vignette, 0.0, 0.0, 0.0]));
            }
        }

        // Ombres : une seule carte pour les deux yeux.
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut draw_calls = self.render_shadow_pass(&mut encoder, app);
        self.queue.submit([encoder.finish()]);

        let bloom_intensity = if app.bloom_enabled && app.render_quality.bloom_enabled() {
            app.scene.sky.bloom_intensity
        } else {
            0.0
        };
        let far = game_camera.far;
        // Multiview : une seule passe de scène pour les deux yeux. Impossible
        // avec la réflexion planaire (sa texture est rendue depuis la caméra
        // d'un seul œil) — coupée par le profil VR de toute façon.
        let multiview = self.mv.is_some() && !self.reflection_active;
        if multiview {
            let cameras = eyes.map(|e| self.eye_camera_uniform(&e, far).0);
            if let Some(m) = self.mv.as_ref() {
                self.queue
                    .write_buffer(&m.camera_buf, 0, bytemuck::cast_slice(&cameras));
            }
            let mut encoder = self.device.create_command_encoder(&Default::default());
            self.mv_active = true;
            if let Some(t) = self.xr_targets.as_ref() {
                draw_calls += self.encode_scene_pass(
                    &mut encoder,
                    app,
                    &t.hdr,
                    None,
                    &t.depth,
                    (w, h),
                    debug_count,
                );
                for (hdr_eye, target) in t.hdr_eyes.iter().zip(targets) {
                    if bloom_intensity > 0.0 {
                        self.render_bloom(&mut encoder, hdr_eye, &t.bloom_mips);
                    }
                    self.tonemap(
                        &mut encoder,
                        hdr_eye,
                        &t.bloom_mips[0],
                        bloom_intensity,
                        target,
                    );
                    if vignette > 0.01 {
                        self.draw_vignette(&mut encoder, target);
                    }
                }
            }
            self.mv_active = false;
            self.queue.submit([encoder.finish()]);
        } else {
            for (eye, target) in eyes.iter().zip(targets) {
                self.write_eye_camera(app, eye, far);
                let mut encoder = self.device.create_command_encoder(&Default::default());
                draw_calls += self.render_reflection_pass(&mut encoder, app);
                let Some(t) = self.xr_targets.as_ref() else {
                    break; // impossible (`ensure_xr_targets` juste au-dessus)
                };
                draw_calls += self.encode_scene_pass(
                    &mut encoder,
                    app,
                    &t.hdr,
                    t.msaa.as_ref(),
                    &t.depth,
                    (w, h),
                    debug_count,
                );
                if bloom_intensity > 0.0 {
                    self.render_bloom(&mut encoder, &t.hdr, &t.bloom_mips);
                }
                self.tonemap(
                    &mut encoder,
                    &t.hdr,
                    &t.bloom_mips[0],
                    bloom_intensity,
                    target,
                );
                if vignette > 0.01 {
                    self.draw_vignette(&mut encoder, target);
                }
                self.queue.submit([encoder.finish()]);
            }
        }
        self.last_frame_draw_calls = draw_calls;
        app.camera = game_camera;
    }

    /// Uniform caméra d'un œil (et de sa passe de réflexion d'eau) — pendant de
    /// la première partie de `write_uniforms`, sans tremblement de caméra (le
    /// *camera shake* rend malade en VR).
    fn write_eye_camera(&self, app: &AppState, eye: &EyeView, far: f32) {
        let (camera, view_proj) = self.eye_camera_uniform(eye, far);
        let p = eye.position;
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&camera));
        // Réflexion planaire : même caméra miroir que `write_uniforms`, par œil.
        if let (true, Some(r), Some(plane)) = (
            self.reflection_active,
            self.reflection.as_ref(),
            self.reflection_plane_y(app),
        ) {
            let reflect = Mat4::from_cols_array_2d(&[
                [1.0, 0.0, 0.0, 0.0],
                [0.0, -1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 2.0 * plane, 0.0, 1.0],
            ]);
            let vp_r = view_proj * reflect;
            let camera_r = CameraUniform {
                view_proj: vp_r.to_cols_array_2d(),
                eye: [p.x, 2.0 * plane - p.y, p.z, self.anim_time],
                inv_view_proj: vp_r.inverse().to_cols_array_2d(),
                cam_right: [0.0; 4],
                cam_up: [0.0; 4],
            };
            self.queue
                .write_buffer(&r.camera_buf, 0, bytemuck::bytes_of(&camera_r));
        }
    }

    /// Uniform caméra d'un œil (et sa matrice vue-projection).
    fn eye_camera_uniform(&self, eye: &EyeView, far: f32) -> (CameraUniform, Mat4) {
        let view_proj = projection_from_fov(eye.fov, NEAR, far.max(NEAR * 2.0))
            * view_from_pose(eye.orientation, eye.position);
        let right = eye.orientation * Vec3::X;
        let up = eye.orientation * Vec3::Y;
        let p = eye.position;
        let camera = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            eye: [p.x, p.y, p.z, self.anim_time],
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            cam_right: [right.x, right.y, right.z, 0.0],
            cam_up: [up.x, up.y, up.z, 0.0],
        };
        (camera, view_proj)
    }

    /// (Re)crée les cibles intermédiaires si la taille a changé : pour un œil
    /// (réutilisées par les deux, rendus l'un après l'autre), ou à **deux
    /// couches** en multiview (profondeur et HDR en texture-tableau, une vue
    /// par couche pour le bloom et le tone mapping de chaque œil).
    fn ensure_xr_targets(&mut self, w: u32, h: u32) {
        let layers = if self.mv.is_some() { 2 } else { 1 };
        if self
            .xr_targets
            .as_ref()
            .is_some_and(|t| t.size == (w, h) && t.layers == layers)
        {
            return;
        }
        let array_view = |tex: &wgpu::Texture| {
            tex.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(if layers == 2 {
                    wgpu::TextureViewDimension::D2Array
                } else {
                    wgpu::TextureViewDimension::D2
                }),
                ..Default::default()
            })
        };
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: layers,
        };
        let samples = if layers == 2 { 1 } else { self.msaa_samples };
        let depth_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("xr_depth"),
            size,
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let (hdr, msaa, hdr_eyes) = if layers == 2 {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("xr_hdr_mv"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let eyes = [0, 1].map(|layer| {
                tex.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            });
            (array_view(&tex), None, eyes)
        } else {
            let (hdr, msaa) = create_hdr_view(&self.device, w, h, self.msaa_samples);
            let eyes = [hdr.clone(), hdr.clone()];
            (hdr, msaa, eyes)
        };
        let bloom_mips = create_bloom_mip_views(&self.device, w, h);
        self.xr_targets = Some(XrTargets {
            size: (w, h),
            layers,
            depth: array_view(&depth_tex),
            hdr,
            hdr_eyes,
            msaa,
            bloom_mips,
        });
    }

    /// Crée le pipeline de la vignette de confort (au format des cibles VR).
    fn ensure_vignette(&mut self) {
        if self.vignette.is_some() {
            return;
        }
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("vr_vignette"),
                source: wgpu::ShaderSource::Wgsl(VIGNETTE_WGSL.into()),
            });
        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vr_vignette"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vr_vignette"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vr_vignette"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        let pl = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vr_vignette"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("vr_vignette"),
                layout: Some(&pl),
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
                        format: self.config.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
        self.vignette = Some((pipeline, buf, bg));
    }

    /// Assombrit les bords d'un œil (`vr_vignette`), par-dessus l'image finale.
    fn draw_vignette(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let Some((pipeline, _, bg)) = &self.vignette else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vr_vignette"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bg, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Pipeline à utiliser : variante multiview pendant une passe VR
    /// multiview, sinon le pipeline normal.
    pub(super) fn mv_or<'a>(
        &'a self,
        pick: impl Fn(&'a pipelines::MultiviewSet) -> &'a wgpu::RenderPipeline,
        normal: &'a wgpu::RenderPipeline,
    ) -> &'a wgpu::RenderPipeline {
        match (&self.mv, self.mv_active) {
            (Some(m), true) => pick(m),
            _ => normal,
        }
    }

    /// Groupe caméra de la passe de scène : à deux vues en multiview.
    pub(super) fn scene_camera_bg(&self) -> &wgpu::BindGroup {
        match (&self.mv, self.mv_active) {
            (Some(m), true) => &m.camera_bind_group,
            _ => &self.camera_bind_group,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn vignette_shader_is_valid_and_its_uniform_is_16_bytes() {
        let module = naga::front::wgsl::parse_str(super::VIGNETTE_WGSL).expect("WGSL");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validation");
        drop(info);
        let size = module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref() == Some("Vignette"))
            .map(|(_, t)| t.inner.size(module.to_ctx()))
            .expect("struct Vignette");
        assert_eq!(size, 16, "doit tenir dans le tampon de 16 octets");
    }
}
