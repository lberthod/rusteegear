//! Rendu VR stéréo (phase 1 de `docs/roadmapExportVRQuest24septembre.md`) :
//! la scène du jeu, avec tout le pipeline du moteur (ombres en cascade, ciel,
//! eau et sa réflexion, skinning, translucides, particules, bloom, tone
//! mapping), rendue une fois par œil dans des cibles fournies par l'appelant —
//! swapchain OpenXR sur le casque (`xr::hello`), textures du simulateur sur
//! desktop (`quest_sim`).

use glam::{Mat4, Vec3};

use super::*;
use crate::xr::math::{EyeView, NEAR, projection_from_fov, view_from_pose};

impl Renderer {
    /// Renderer sur un device **déjà créé** par l'appelant (instance Vulkan du
    /// runtime OpenXR, ou device de la fenêtre du simulateur) : ni fenêtre, ni
    /// surface, ni UI egui. `format` = format des cibles passées à
    /// `render_views` (celui de la swapchain XR, sRGB).
    ///
    /// Qualité : mono-échantillon et ombres à la taille de référence pour
    /// l'instant — le profil `RenderQuality::Vr` (MSAA 4×, ombres réduites, bloom
    /// coupé) est l'objet de la phase 2.
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
        log::info!("GPU : {} ({:?}) — rendu VR", info.name, info.backend);
        Self::assemble(
            None,
            None,
            device,
            queue,
            config,
            size,
            1,
            SHADOW_SIZE,
            format!("{:?}", info.backend),
            None,
        )
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
                0,
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
            self.queue.submit([encoder.finish()]);
        }
        self.last_frame_draw_calls = draw_calls;
        app.camera = game_camera;
    }

    /// Uniform caméra d'un œil (et de sa passe de réflexion d'eau) — pendant de
    /// la première partie de `write_uniforms`, sans tremblement de caméra (le
    /// *camera shake* rend malade en VR).
    fn write_eye_camera(&self, app: &AppState, eye: &EyeView, far: f32) {
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

    /// (Re)crée les cibles intermédiaires d'un œil si la taille a changé.
    fn ensure_xr_targets(&mut self, w: u32, h: u32) {
        if self.xr_targets.as_ref().is_some_and(|t| t.size == (w, h)) {
            return;
        }
        let depth = self
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("xr_depth"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.msaa_samples,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let (hdr, msaa) = create_hdr_view(&self.device, w, h, self.msaa_samples);
        let bloom_mips = create_bloom_mip_views(&self.device, w, h);
        self.xr_targets = Some(XrTargets {
            size: (w, h),
            depth,
            hdr,
            msaa,
            bloom_mips,
        });
    }
}
