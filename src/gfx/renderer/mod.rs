//! Couche **rendu pur** (wgpu + egui). Ne contient aucun état métier : la scène,
//! la caméra et la sélection vivent dans `AppState` et sont passées à `render`.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use super::lod::foliage_lod_mesh;
use super::mesh::GpuMesh;
use super::passes::{
    aabb_visible, culling_radius_for, distance_visible, frustum_planes, is_skinned, mesh_key,
    render_input_hash,
};
#[cfg(test)]
use super::pipelines::mip_count_for;
use super::pipelines::{
    self, PipelineBundle, create_bloom_mip_views, create_depth_view, create_hdr_view,
    create_models_buffer, create_skinned_models_bind_group, load_rgba, make_texture,
};
use crate::app::{AppState, GIZMO_LEN, GizmoMode, RING_SEGMENTS, axis_basis, axis_dir};
use crate::editor::Editor;
use crate::scene::{MeshKind, Scene};
use crate::time_compat::Instant;

mod types;
pub use types::Renderer;
pub(crate) use types::*;

mod resources;

mod shadows;

mod sync;

mod post_process;

mod frame;

impl Renderer {
    /// Rendu headless d'une scène dans une texture hors-écran : passe d'ombre + passe
    /// principale, **sans** grille, gizmos ni UI egui (golden tests de
    /// non-régression visuelle). Le pipeline utilisé — mêmes shaders, mêmes bind groups —
    /// est celui de [`Renderer::render`] : un shader qui dérive fait dériver les deux.
    /// Retourne les pixels RGBA8 (`width`×`height`, 4 octets/pixel, sans padding de ligne).
    pub fn render_scene_headless(
        &mut self,
        app: &mut AppState,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        self.sync_objects(&app.scene);
        self.sync_imported(&app.scene);
        self.sync_textures(&app.scene);
        app.camera.aspect = width as f32 / (height as f32).max(1.0);
        self.write_uniforms(app);
        // Skinning GPU : cf. commentaire équivalent dans `render()`.
        self.prepare_skinned_draws(&app.scene);

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        // Depth dédiée à la taille demandée (peut différer de `self.depth_view`, qui suit
        // la taille de la fenêtre en mode interactif). Même sample count que les
        // pipelines : mono-échantillon via `new_headless` (goldens), mais celui de la
        // fenêtre quand ce chemin sert de capture depuis un renderer fenêtré
        // (`screenshot_png`, pont de pilotage) — les pipelines y sont compilés en
        // MSAA, une cible mono-échantillon ferait échouer la validation wgpu.
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless_depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: self.msaa_samples,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        // Cibles HDR, locales à cet appel — cf. `hdr_view`/`msaa_color_view` de
        // `render()` : `msaa_color_view` n'est `Some` qu'en MSAA (renderer fenêtré).
        let (hdr_view, msaa_color_view) =
            create_hdr_view(&self.device, width, height, self.msaa_samples);
        // Chaîne de bloom, locale à cet appel — cf. `bloom_mip_views` de
        // `render()`.
        let bloom_mip_views = create_bloom_mip_views(&self.device, width, height);

        // Debug drawing : même logique que `render()` (préparer + vider avant
        // les passes, dessiner après les meshes texturés dans la passe principale).
        let debug_count = {
            let verts: Vec<GizmoVertex> = app
                .debug_lines
                .iter()
                .flat_map(|&(a, b, color)| {
                    [
                        GizmoVertex {
                            position: a.to_array(),
                            color,
                        },
                        GizmoVertex {
                            position: b.to_array(),
                            color,
                        },
                    ]
                })
                .collect();
            app.debug_lines.clear();
            if !verts.is_empty() {
                self.ensure_debug_capacity(verts.len());
                self.queue
                    .write_buffer(&self.debug_vbuf, 0, bytemuck::cast_slice(&verts));
            }
            verts.len() as u32
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("headless_encoder"),
            });

        // Compte des draw calls (Phase F, `sprintoptimation3daudit10h.md`) — même
        // logique que `render()`/`last_frame_draw_calls` : jusqu'ici ce chemin ne
        // renseignait jamais `last_frame_draw_calls`, donc `gpu_profiler_info()` après
        // un rendu headless retournait toujours 0, aucune régression de draw calls
        // n'aurait été détectable via un benchmark headless.
        let mut scene_draw_calls: u32 = 0;

        // Passe d'ombre — identique à celle de `render()`, sans les gizmos ni l'UI.
        {
            let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless_shadow_pass"),
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
            spass.set_pipeline(&self.shadow_pipeline);
            spass.set_bind_group(0, &self.camera_bind_group, &[]);
            spass.set_bind_group(1, &self.models_bind_group, &[]);
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = plan[i].mesh;
                let mut j = i + 1;
                while j < plan.len() && plan[j].mesh == mi {
                    j += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    spass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    spass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < j {
                        if !objs[plan[k].obj].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < j && objs[plan[k].obj].visible {
                            k += 1;
                        }
                        spass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                        scene_draw_calls += 1;
                    }
                }
                i = j;
            }
            // Objets skinnés dans la carte d'ombre — même correctif que `render()`
            // (audit du 17 juillet 2026), appliqué ici aussi pour que les golden
            // tests capturent les ombres skinnées.
            scene_draw_calls +=
                self.draw_skinned_shadows(&mut spass, &app.scene, &self.skinned_offsets_scratch);
        }

        // Passe principale — identique à celle de `render()`, sans grille ni gizmos.
        // Dessine dans `hdr_view` ; `self.tonemap()` fait le dernier pas
        // vers `view`, juste avant la lecture des pixels.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless_main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    // MSAA : dessine dans la cible multi-échantillonnée et se résout
                    // vers `hdr_view` — même branchement que la passe principale de
                    // `render()`, sinon comportement inchangé (goldens mono-échantillon).
                    view: msaa_color_view.as_ref().unwrap_or(&hdr_view),
                    resolve_target: msaa_color_view.as_ref().map(|_| &hdr_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
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
            pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);

            // Ciel : même geste que dans `render()`.
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.draw(0..3, 0..1);

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(1, &self.models_bind_group, &[]);

            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = plan[i].mesh;
                let tex_key = &objs[plan[i].obj].texture;
                let mut group_end = i + 1;
                while group_end < plan.len()
                    && plan[group_end].mesh == mi
                    && &objs[plan[group_end].obj].texture == tex_key
                {
                    group_end += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    let tex = self
                        .textures
                        .get(tex_key)
                        .unwrap_or_else(|| &self.textures[""]);
                    pass.set_bind_group(3, tex, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < group_end {
                        if !plan[k].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < group_end && plan[k].visible {
                            k += 1;
                        }
                        pass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                        scene_draw_calls += 1;
                    }
                }
                i = group_end;
            }

            // Debug drawing.
            if debug_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.debug_vbuf.slice(..));
                pass.draw(0..debug_count, 0..1);
            }

            // Objets skinnés : cf. commentaire équivalent dans `render()`.
            scene_draw_calls +=
                self.draw_skinned_objects(&mut pass, &app.scene, &self.skinned_offsets_scratch);
        }

        // Cf. `render()` : `last_frame_draw_calls` sert de source unique à
        // `gpu_profiler_info()`, lu aussi bien après `render()` (panneau Profiler en
        // jeu) qu'après `render_scene_headless()` (mesures/benchmarks headless).
        self.last_frame_draw_calls = scene_draw_calls;

        // Bloom : cf. commentaire équivalent dans `render()`.
        let bloom_intensity = if app.bloom_enabled && app.render_quality.bloom_enabled() {
            app.scene.sky.bloom_intensity
        } else {
            0.0
        };
        if bloom_intensity > 0.0 {
            self.render_bloom(&mut encoder, &hdr_view, &bloom_mip_views);
        }
        // Tone mapping : HDR → `view` (le format lu par `finish_and_read_rgba`).
        self.tonemap(
            &mut encoder,
            &hdr_view,
            &bloom_mip_views[0],
            bloom_intensity,
            &view,
        );

        self.finish_and_read_rgba(encoder, &target, width, height)
    }

    /// Capture PNG de l'état courant de la scène (pont de pilotage externe, cf.
    /// `crate::pilot`) : rendu hors-écran via [`Renderer::render_scene_headless`],
    /// donc disponible aussi depuis un renderer **fenêtré** — la cible offscreen
    /// hérite alors du format de la surface (BGRA sur macOS/Metal, contrairement
    /// au RGBA imposé par `new_headless`), d'où le swizzle avant l'écriture PNG.
    pub fn screenshot_png(
        &mut self,
        app: &mut AppState,
        width: u32,
        height: u32,
        path: &std::path::Path,
    ) -> Result<(), String> {
        // `render_scene_headless` consomme `app.debug_lines` (vidées après dessin,
        // comme `render()`) : on les repose ensuite pour que la frame fenêtrée en
        // cours ne perde pas ses lignes de debug à cause d'une capture. L'aspect
        // caméra, lui aussi écrasé, est recalculé par `render()` à chaque frame —
        // rien à restaurer.
        let debug_lines = app.debug_lines.clone();
        let mut pixels = self.render_scene_headless(app, width, height);
        app.debug_lines = debug_lines;
        if matches!(
            self.config.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)
            .map_err(|e| format!("écriture de {} impossible : {e}", path.display()))
    }

    /// Copie `target` vers un buffer lisible CPU, soumet `encoder` et attend le résultat —
    /// partagé par tous les rendus headless (`render_scene_headless`, `render_skinned_test`).
    /// `encoder` doit déjà contenir toutes les passes de dessin dans `target` ;
    /// cette méthode ne fait que la copie finale + lecture.
    fn finish_and_read_rgba(
        &self,
        mut encoder: wgpu::CommandEncoder,
        target: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        // wgpu impose que `bytes_per_row` soit un multiple de
        // `COPY_BYTES_PER_ROW_ALIGNMENT` (256) → on copie avec ce padding puis on le retire.
        let bytes_per_pixel = 4u32;
        let unpadded = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless_readback"),
            size: (padded * height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        // Le callback de `map_async` est invoqué par `poll` ci-dessus : à ce stade le
        // résultat est déjà dans le canal.
        let _ = rx.recv();
        let mapped = slice.get_mapped_range();
        let mut out = vec![0u8; (unpadded * height) as usize];
        for y in 0..height {
            let src_start = (y * padded) as usize;
            let dst_start = (y * unpadded) as usize;
            out[dst_start..dst_start + unpadded as usize]
                .copy_from_slice(&mapped[src_start..src_start + unpadded as usize]);
        }
        drop(mapped);
        readback.unmap();
        out
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
