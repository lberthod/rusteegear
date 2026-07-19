use super::*;

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Renderer, String> {
        let size = window.inner_size();
        Self::new_impl(Some(window), size).await
    }

    /// Rendu headless : pas de fenêtre ni de surface d'écran (golden tests).
    /// `compatible_surface: None` à la création de l'adaptateur ; format fixe
    /// (`Rgba8UnormSrgb`) puisqu'il n'y a pas de surface pour en dicter un.
    pub async fn new_headless(width: u32, height: u32) -> Result<Renderer, String> {
        let size = winit::dpi::PhysicalSize::new(width.max(1), height.max(1));
        Self::new_impl(None, size).await
    }

    pub(super) async fn new_impl(
        window: Option<Arc<Window>>,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> Result<Renderer, String> {
        let instance = wgpu::Instance::default();
        let surface = match &window {
            Some(w) => Some(
                instance
                    .create_surface(w.clone())
                    .map_err(|e| format!("Création de la surface impossible : {e}"))?,
            ),
            None => None,
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("Aucun adaptateur GPU trouvé : {e}"))?;

        // Profiler GPU (Sprint 112) : timestamp queries, demandées seulement si
        // l'adaptateur les supporte — sinon `required_features` resterait vide et
        // `GpuProfiler::new` ne serait jamais appelé (`gpu_profiler` reste `None`,
        // dégradation silencieuse comme pour `gilrs`/`notify`).
        let profiler_features =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        // Phase E (`sprintoptimation3daudit10h.md`, Sprint 6) : compression BC3 des
        // textures d'albédo, demandée seulement si l'adaptateur la supporte (desktop en
        // pratique — mobile/web n'exposent typiquement pas `TEXTURE_COMPRESSION_BC`,
        // même dégradation silencieuse que le profiler ci-dessus). Cf. `gfx::texcompress`.
        let texture_compression_features = wgpu::Features::TEXTURE_COMPRESSION_BC;
        let requested_features =
            (profiler_features | texture_compression_features) & adapter.features();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: requested_features,
                // Limites du GPU réel (iOS/mobile en ont de plus basses que les défauts).
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| format!("Échec création du device : {e}"))?;

        let gpu_profiler = requested_features
            .contains(profiler_features)
            .then(|| GpuProfiler::new(&device, queue.get_timestamp_period()));

        let backend = format!("{:?}", adapter.get_info().backend);
        // Ligne de démarrage attendue par un nouvel utilisateur (Phase A,
        // sprint.19matin.md) : quel GPU et quel backend, une seule ligne.
        log::info!("GPU : {} ({backend})", adapter.get_info().name);

        let (format, alpha_mode) = match &surface {
            Some(s) => {
                let caps = s.get_capabilities(&adapter);
                // GPU dégénéré (surface incompatible) : `caps.formats`/`alpha_modes` peuvent
                // être vides → on remonte une erreur claire au lieu de paniquer en indexant `[0]`.
                let format = caps
                    .formats
                    .iter()
                    .copied()
                    .find(|f| f.is_srgb())
                    .or_else(|| caps.formats.first().copied())
                    .ok_or_else(|| "Aucun format de surface supporté par le GPU".to_string())?;
                let alpha_mode =
                    caps.alpha_modes.first().copied().ok_or_else(|| {
                        "Aucun mode alpha de surface supporté par le GPU".to_string()
                    })?;
                (format, alpha_mode)
            }
            // Rendu headless : pas de surface pour dicter un format → fixe, stable d'une
            // machine à l'autre (comparaison de pixels des golden tests).
            None => (
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::CompositeAlphaMode::Opaque,
            ),
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // Fifo (vsync) : cale le rendu sur l'écran, fluide et peu gourmand.
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        if let Some(s) = &surface {
            s.configure(&device, &config);
        }

        // MSAA : uniquement en mode fenêtré (le rendu headless doit rester déterministe
        // pixel pour pixel pour les golden tests, cf. `render_scene_headless`), au niveau
        // visé par `RenderQuality` (`msaa_samples`, opt-out mobile sur « Basse » — même
        // logique que `bloom_enabled`), et seulement si l'adaptateur supporte réellement
        // ce nombre d'échantillons pour les deux formats de la passe principale (repli
        // silencieux à 1 sinon, ex. certains backends GLES/WebGL).
        let msaa_samples = if window.is_some() {
            let wanted = crate::app::build_config::BuildConfig::load()
                .render_quality
                .msaa_samples();
            let supported = wanted <= 1
                || (adapter
                    .get_texture_format_features(HDR_FORMAT)
                    .flags
                    .sample_count_supported(wanted)
                    && adapter
                        .get_texture_format_features(DEPTH_FORMAT)
                        .flags
                        .sample_count_supported(wanted));
            if supported { wanted } else { 1 }
        } else {
            1
        };

        let bundle = pipelines::build(
            &device,
            &queue,
            &config,
            size,
            window.as_ref(),
            msaa_samples,
        );
        let PipelineBundle {
            pipeline,
            sky_pipeline,
            tonemap_pipeline,
            tonemap_layout,
            tonemap_sampler,
            hdr_view,
            msaa_color_view,
            bloom_threshold_pipeline,
            bloom_downsample_pipeline,
            bloom_upsample_pipeline,
            bloom_sample_layout,
            bloom_intensity_buf,
            bloom_mip_views,
            depth_view,
            model_layout,
            camera_buf,
            light_buf,
            camera_bind_group,
            meshes,
            models_buf,
            models_bind_group,
            models_capacity,
            gizmo_pipeline,
            gizmo_vbuf,
            debug_vbuf,
            debug_capacity,
            grid_pipeline,
            grid_vbuf,
            grid_count,
            shadow_view,
            shadow_bind_group,
            shadow_pipeline,
            tex_layout,
            tex_sampler,
            textures,
            mipgen_pipeline,
            mipgen_layout,
            mipgen_sampler,
            editor,
            skinned_pipeline,
            skinned_shadow_pipeline,
            skinned_model_layout,
            joint_buf,
            skinned_models_bind_group,
        } = bundle;

        Ok(Renderer {
            window,
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            sky_pipeline,
            tonemap_pipeline,
            tonemap_layout,
            tonemap_sampler,
            hdr_view,
            msaa_color_view,
            msaa_samples,
            bloom_threshold_pipeline,
            bloom_downsample_pipeline,
            bloom_upsample_pipeline,
            bloom_sample_layout,
            bloom_intensity_buf,
            bloom_mip_views,
            depth_view,
            model_layout,
            camera_buf,
            light_buf,
            camera_bind_group,
            meshes,
            imported_gpu: Vec::new(),
            imported_gpu_skinned: Vec::new(),
            models_buf,
            models_bind_group,
            models_capacity,
            draw_plan: Vec::new(),
            draw_plan_skinned: Vec::new(),
            order_scratch: Vec::new(),
            models_scratch: Vec::new(),
            last_sort_len: usize::MAX,
            last_render_hash: 0,
            gizmo_pipeline,
            gizmo_vbuf,
            debug_vbuf,
            debug_capacity,
            grid_pipeline,
            grid_vbuf,
            grid_count,
            shadow_view,
            shadow_bind_group,
            shadow_pipeline,
            tex_layout,
            tex_sampler,
            textures,
            mipgen_pipeline,
            mipgen_layout,
            mipgen_sampler,
            editor,
            backend,
            skinned_pipeline,
            skinned_shadow_pipeline,
            skinned_model_layout,
            joint_buf,
            skinned_models_bind_group,
            joint_capacity: JOINT_CAPACITY,
            skinned_offsets_scratch: Vec::new(),
            joint_matrices_scratch: Vec::new(),
            joint_raw_scratch: Vec::new(),
            skinning_scratch: crate::scene::import::SkinningScratch::default(),
            skinned_dropped_last_frame: 0,
            failed_textures: std::collections::HashSet::new(),
            gpu_profiler,
            gpu_pass_timings_ms: Vec::new(),
            last_frame_draw_calls: 0,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        let Some(surface) = self.surface.as_ref() else {
            return; // rendu headless : pas de surface à reconfigurer
        };
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, &self.config, self.msaa_samples);
        (self.hdr_view, self.msaa_color_view) = create_hdr_view(
            &self.device,
            new_size.width,
            new_size.height,
            self.msaa_samples,
        );
        self.bloom_mip_views =
            create_bloom_mip_views(&self.device, new_size.width, new_size.height);
    }

    /// Recrée `debug_vbuf` en le doublant tant qu'il ne peut pas contenir `n` sommets,
    /// même politique de croissance que `create_models_buffer`.
    pub(super) fn ensure_debug_capacity(&mut self, n: usize) {
        if n <= self.debug_capacity {
            return;
        }
        let cap = n.next_power_of_two().max(64);
        self.debug_vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("debug_vbuf"),
            size: (cap * std::mem::size_of::<GizmoVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.debug_capacity = cap;
    }
}
