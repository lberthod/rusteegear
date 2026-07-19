use super::*;

impl Renderer {
    /// Chaîne de bloom : seuil (`hdr_source` → `mip_views[0]`), descente
    /// (`mip_views[i]` → `mip_views[i+1]`, remplace), puis remontée (`mip_views[i+1]` →
    /// `mip_views[i]`, additionne) — `mip_views[0]` porte le résultat final en sortie,
    /// à moitié résolution HDR, remonté à pleine taille par le filtrage bilinéaire du
    /// sampler quand `tonemap()` l'échantillonne.
    pub(super) fn render_bloom(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        hdr_source: &wgpu::TextureView,
        mip_views: &[wgpu::TextureView],
    ) {
        let bind = |src: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom_bg"),
                layout: &self.bloom_sample_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.tonemap_sampler),
                    },
                ],
            })
        };
        let draw_into = |encoder: &mut wgpu::CommandEncoder,
                         pipeline: &wgpu::RenderPipeline,
                         bind_group: &wgpu::BindGroup,
                         target: &wgpu::TextureView| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        };

        let threshold_bg = bind(hdr_source);
        draw_into(
            encoder,
            &self.bloom_threshold_pipeline,
            &threshold_bg,
            &mip_views[0],
        );
        for i in 0..mip_views.len() - 1 {
            let bg = bind(&mip_views[i]);
            draw_into(
                encoder,
                &self.bloom_downsample_pipeline,
                &bg,
                &mip_views[i + 1],
            );
        }
        for i in (0..mip_views.len() - 1).rev() {
            let bg = bind(&mip_views[i + 1]);
            draw_into(encoder, &self.bloom_upsample_pipeline, &bg, &mip_views[i]);
        }
    }

    /// Passe de tone mapping + composition du bloom : lit
    /// `hdr_source` (`HDR_FORMAT`, rempli par la passe principale) et `bloom_source`
    /// (résultat de `render_bloom`, `mip_views[0]`), écrit le résultat dans `output`
    /// (format d'affichage final, `config.format`). Partagée par les trois chemins de
    /// rendu (`render`, `render_scene_headless`, `render_skinned_test`) : un seul
    /// endroit qui connaît la courbe ACES. `bloom_intensity` à 0 (opt-out mobile, cf.
    /// `RenderQuality::bloom_enabled`) neutralise `bloom_source` quel que soit son
    /// contenu — pas besoin d'une texture noire dédiée.
    pub(super) fn tonemap(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        hdr_source: &wgpu::TextureView,
        bloom_source: &wgpu::TextureView,
        bloom_intensity: f32,
        output: &wgpu::TextureView,
    ) {
        // Le shader écrit une couleur linéaire (0..1 après tone mapping) en
        // supposant qu'une vue sRGB de `output` applique l'encodage gamma
        // automatiquement au moment de l'écriture (comportement standard des
        // formats *Srgb — c'est ce qui se passe côté natif, `config.format` y
        // est toujours srgb, cf. `new_impl`). Sur wasm32/WebGPU (Chrome, testé
        // Sprint 114), le canvas n'expose **aucun** format de surface srgb
        // (uniquement `Bgra8Unorm`) : sans ce correctif, l'image sortait
        // beaucoup trop sombre (quasi noire à l'écran) faute d'encodage —
        // `needs_srgb_encode` fait appliquer l'encodage **dans le shader** à la
        // place quand la surface réelle n'est pas srgb, quelle que soit la
        // plateforme (pas un `#[cfg(wasm32)]` : suit le format effectivement
        // choisi, robuste à un futur backend natif sans format srgb non plus).
        let needs_srgb_encode = if self.config.format.is_srgb() {
            0.0
        } else {
            1.0
        };
        self.queue.write_buffer(
            &self.bloom_intensity_buf,
            0,
            bytemuck::bytes_of(&BloomUniform {
                intensity: [bloom_intensity, needs_srgb_encode, 0.0, 0.0],
            }),
        );
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap_bg"),
            layout: &self.tonemap_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr_source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.tonemap_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bloom_source),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.bloom_intensity_buf.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tonemap_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.tonemap_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Lit les timestamp queries de la frame qu'on vient de soumettre (Sprint 112) et
    /// remplit `gpu_pass_timings_ms`. Appelée seulement quand le panneau Profiler est
    /// ouvert (`render`) : `map_async` + `device.poll(Wait)` bloque jusqu'à ce que le
    /// GPU ait fini — un vrai coût, acceptable pour un outil de dev opt-in, exclu du
    /// chemin de rendu par défaut. `resolve_query_set` renvoie des ticks GPU bruts
    /// (`u64`), convertis en ms via `period_ns` (`Queue::get_timestamp_period`).
    pub(super) fn read_gpu_pass_timings(&mut self) {
        let Some(prof) = self.gpu_profiler.as_ref() else {
            return;
        };
        // Borné à 1s (au lieu de `wait_indefinitely`, Sprint 112 d'origine) : un
        // pilote/adaptateur qui ne relance jamais le callback de `map_async` gelait
        // l'éditeur sans retour possible dès l'ouverture du Profiler (rapporté
        // Phase 0 de `sprintoptimation3daudit10h.md`, 2026-07-18) — mieux vaut
        // renoncer à la mesure GPU de cette frame que geler l'app.
        let slice = prof.readback_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        let timeout = std::time::Duration::from_secs(1);
        let polled = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(timeout),
        });
        if polled.is_err() {
            log::warn!("read_gpu_pass_timings : device.poll a expiré, mesure GPU ignorée");
            return;
        }
        let Ok(Ok(())) = rx.recv_timeout(timeout) else {
            log::warn!("read_gpu_pass_timings : map_async n'a jamais répondu, mesure GPU ignorée");
            return;
        };
        let ticks: Vec<u64> = {
            let data = slice.get_mapped_range();
            // `chunks_exact(8)` garantit des tranches de 8 octets : la conversion en
            // `[u8; 8]` ne peut jamais échouer (Sprint 113b, audit unwrap/expect).
            data.chunks_exact(8)
                .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                .collect()
        };
        prof.readback_buf.unmap();
        self.gpu_pass_timings_ms = GpuProfiler::PASS_NAMES
            .iter()
            .enumerate()
            .filter_map(|(i, &name)| {
                let (t0, t1) = (*ticks.get(i)?, *ticks.get(i + 1)?);
                let ms = t1.saturating_sub(t0) as f32 * prof.period_ns / 1_000_000.0;
                Some((name, ms))
            })
            .collect();
    }

    /// Durée (ms) de chaque passe GPU mesurée à la dernière frame profilée, et
    /// estimation du nombre de draw calls (Sprint 112) — lu par le panneau
    /// « 📊 Profiler FPS ». Vide si le panneau n'a jamais été ouvert, ou si
    /// l'adaptateur ne supporte pas les timestamp queries.
    pub fn gpu_profiler_info(&self) -> (&[(&'static str, f32)], u32) {
        (&self.gpu_pass_timings_ms, self.last_frame_draw_calls)
    }
}
