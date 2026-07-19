use super::*;

impl Renderer {
    /// Envoie la palette de matrices de joints d'**une** instance skinnée au GPU, dans son
    /// créneau `slot` du buffer partagé (offset dynamique, cf. commentaire
    /// sur `JOINT_SLOT_BYTES`). Tronque silencieusement (`log::warn!`) au-delà de
    /// `joint_capacity` plutôt que de paniquer ou d'écrire hors créneau — un rig
    /// anormalement gros dégraderait l'anim plutôt que de planter le rendu. `slot` au-delà
    /// de `MAX_SKINNED_INSTANCES` renvoie `None` : **aucune écriture** n'a lieu, l'offset
    /// `0` resterait sinon un offset valide (celui du véritable occupant du slot 0) —
    /// le renvoyer ici aurait fait dessiner cet objet avec la palette de joints d'un
    /// *autre* objet skinné (squelette différent) au lieu de rester simplement invisible.
    /// Bug observé en pratique : le joueur (mesh importé le plus loin dans l'ordre de tri
    /// par mesh/texture) dépassait la capacité dès que ≥ `MAX_SKINNED_INSTANCES` créatures
    /// skinnées étaient aussi visibles, et se faisait dessiner éclaté/scindé avec le
    /// squelette d'une créature quelconque.
    ///
    /// Renvoie l'offset dynamique (octets) à passer à `set_bind_group(1, &skinned_models_bind_group, &[offset])`,
    /// ou `None` si l'instance doit être sautée (pas de créneau disponible — les
    /// appelants comptabilisent ce cas dans `skinned_dropped_last_frame`, garde-fou
    /// visible du dépassement de `MAX_SKINNED_INSTANCES`).
    ///
    /// `raw` : tampon de conversion fourni par l'appelant (vidé puis rempli) — évite
    /// une allocation par objet skinné et par frame (audit perf, juillet 2026).
    fn write_joint_matrices(
        &self,
        slot: usize,
        matrices: &[glam::Mat4],
        raw: &mut Vec<[[f32; 4]; 4]>,
    ) -> Option<u32> {
        if slot >= MAX_SKINNED_INSTANCES {
            log::warn!(
                "skinning : créneau {slot} au-delà de la capacité ({MAX_SKINNED_INSTANCES}) — objet ignoré"
            );
            return None;
        }
        let n = matrices.len().min(self.joint_capacity);
        if matrices.len() > self.joint_capacity {
            log::warn!(
                "skinning : {} joints, capacité {} — le reste est ignoré",
                matrices.len(),
                self.joint_capacity
            );
        }
        raw.clear();
        raw.extend(matrices[..n].iter().map(|m| m.to_cols_array_2d()));
        let offset = slot as wgpu::BufferAddress * JOINT_SLOT_BYTES;
        self.queue
            .write_buffer(&self.joint_buf, offset, bytemuck::cast_slice(raw));
        Some(offset as u32)
    }

    /// Calcule et envoie au GPU la palette de joints de chaque objet skinné visible de la
    /// frame (`self.draw_plan_skinned`, déjà construit par `write_uniforms`),
    /// **avant** toute passe de rendu (cf. commentaire aux sites d'appel : `write_buffer`
    /// n'est pas ordonné avec les draw calls d'un encoder pas encore soumis). Remplit
    /// `self.skinned_offsets_scratch` avec les offsets dynamiques, dans l'ordre de
    /// `draw_plan_skinned`, à passer à
    /// `set_bind_group(1, &skinned_models_bind_group, &[offset])` lors du dessin réel dans la
    /// passe — `None` (mesh non importé/introuvable, pas de squelette, ou capacité de
    /// créneaux dépassée dans `write_joint_matrices`) signifie que `draw_skinned_objects`
    /// doit sauter l'objet plutôt que de le dessiner avec l'offset ambigu `0`, qui est
    /// *aussi* un offset valide pour l'objet réellement au slot 0.
    ///
    /// Audit perf (juillet 2026) : plus aucune allocation par frame ici — les tampons
    /// (`skinned_offsets_scratch`, `joint_matrices_scratch`, `joint_raw_scratch`,
    /// `skinning_scratch`) sont des champs réutilisés, et `draw_plan_skinned` est
    /// `mem::take`-é le temps de la boucle (au lieu de l'ancien `.clone()` par frame,
    /// simple contournement d'emprunt).
    pub(super) fn prepare_skinned_draws(&mut self, scene: &Scene) {
        self.skinned_dropped_last_frame = 0;
        let plan = std::mem::take(&mut self.draw_plan_skinned);
        let mut offsets = std::mem::take(&mut self.skinned_offsets_scratch);
        let mut matrices = std::mem::take(&mut self.joint_matrices_scratch);
        let mut raw = std::mem::take(&mut self.joint_raw_scratch);
        let mut scratch = std::mem::take(&mut self.skinning_scratch);
        offsets.clear();
        for (slot, &(obj_idx, _instance)) in plan.iter().enumerate() {
            let obj = &scene.objects[obj_idx];
            let MeshKind::Imported(mesh_idx) = obj.mesh else {
                offsets.push(None);
                continue;
            };
            let Some(imported) = scene.imported.get(mesh_idx as usize) else {
                offsets.push(None);
                continue;
            };
            let Some(skeleton) = &imported.skeleton else {
                offsets.push(None);
                continue;
            };
            // Garde-fou visible (audit du 17 juillet 2026) : au-delà de la capacité de
            // créneaux, l'objet est **ignoré** (pas dessiné) — compté ici pour que le
            // dépassement ne reste pas qu'un `log::warn` (cf. `skinned_dropped_count`).
            if slot >= MAX_SKINNED_INSTANCES {
                self.skinned_dropped_last_frame += 1;
            }
            // Sans `AnimationState` (ou clip introuvable/vide) : pose de liaison figée,
            // pas une erreur — un mesh skinné a le droit de rester immobile (décor posé).
            let find_clip = |name: &str| imported.clips.iter().find(|c| c.name == name);
            let anim = obj.animation.as_ref();
            let clip = anim
                .filter(|a| !a.clip.is_empty())
                .and_then(|a| find_clip(&a.clip));
            let time = anim.map(|a| a.time).unwrap_or(0.0);
            // Fondu enchaîné : `blend < 1.0` tant qu'une transition est en
            // cours (cf. `AppState::sim_step`) — mélange avec le clip quitté au niveau
            // des poses locales, pas des matrices monde (`compute_joint_matrices_blended`).
            match anim.filter(|a| a.blend < 1.0 && !a.prev_clip.is_empty()) {
                Some(a) => crate::scene::import::compute_joint_matrices_blended_into(
                    skeleton,
                    find_clip(&a.prev_clip),
                    a.prev_time,
                    clip,
                    time,
                    a.blend,
                    &mut scratch,
                    &mut matrices,
                ),
                None => crate::scene::import::compute_joint_matrices_into(
                    skeleton,
                    clip,
                    time,
                    &mut scratch,
                    &mut matrices,
                ),
            };
            offsets.push(self.write_joint_matrices(slot, &matrices, &mut raw));
        }
        self.draw_plan_skinned = plan;
        self.skinned_offsets_scratch = offsets;
        self.joint_matrices_scratch = matrices;
        self.joint_raw_scratch = raw;
        self.skinning_scratch = scratch;
    }

    /// Nombre d'objets skinnés visibles **non dessinés** à la dernière frame préparée,
    /// faute de créneau de palette de joints (`slot >= MAX_SKINNED_INSTANCES`) —
    /// garde-fou visible du dépassement de capacité (audit du 17 juillet 2026) : `0`
    /// en régime normal ; une valeur non nulle signifie qu'il faut remonter
    /// `MAX_SKINNED_INSTANCES` (cf. sa doc). Note : le panneau de stats vit dans
    /// `src/editor` (hors de portée ici) — la stat est exposée côté renderer, prête à
    /// y être affichée.
    pub fn skinned_dropped_count(&self) -> u32 {
        self.skinned_dropped_last_frame
    }

    /// Dessine les objets skinnés de `self.draw_plan_skinned`, un draw individuel par
    /// objet (chacun avec sa propre palette de joints — pas de batching possible ici,
    /// contrairement aux objets statiques). `offsets` doit venir de
    /// `prepare_skinned_draws` sur la même frame, dans le même ordre — un `None` (mesh
    /// non importé/introuvable, pas de squelette, ou capacité de créneaux dépassée)
    /// **saute** l'objet plutôt que de le dessiner avec l'offset `0`, qui appartient à un
    /// autre objet skinné (cf. la doc de `write_joint_matrices` : le bug qui scindait le
    /// mesh du joueur venait exactement de dessiner malgré un `None`/absence de créneau).
    ///
    /// Renvoie le nombre de draw calls réellement émis (compteur de stats, cf.
    /// `last_frame_draw_calls`).
    pub(super) fn draw_skinned_objects<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        scene: &Scene,
        offsets: &[Option<u32>],
    ) -> u32 {
        let mut draws = 0;
        for (&(obj_idx, instance_index), &offset) in self.draw_plan_skinned.iter().zip(offsets) {
            let Some(offset) = offset else {
                continue;
            };
            let obj = &scene.objects[obj_idx];
            let MeshKind::Imported(mesh_idx) = obj.mesh else {
                continue;
            };
            let Some(Some(gpu_mesh)) = self.imported_gpu_skinned.get(mesh_idx as usize) else {
                continue;
            };
            let tex = self
                .textures
                .get(&obj.texture)
                .unwrap_or(&self.textures[""]);
            pass.set_pipeline(&self.skinned_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.skinned_models_bind_group, &[offset]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(3, tex, &[]);
            pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(
                0..gpu_mesh.num_indices,
                0,
                instance_index..instance_index + 1,
            );
            draws += 1;
        }
        draws
    }

    /// Ombres des objets skinnés (audit du 17 juillet 2026) : dessine chaque objet de
    /// `draw_plan_skinned` dans la carte d'ombre avec `skinned_shadow_pipeline`
    /// (profondeur seule + vertex de skinning — sans ce chemin, un objet skinné ne
    /// projetait **aucune** ombre, la passe d'ombre n'itérant que le plan statique).
    /// Mêmes règles de saut que `draw_skinned_objects` (`offsets` de la même frame,
    /// `None` ⇒ objet sauté). Choix assumé : `draw_plan_skinned` est déjà filtré par le
    /// frustum caméra (contrairement au plan statique, rendu hors champ dans l'ombre) —
    /// une créature hors champ ne projette donc pas d'ombre dans le champ, imprécision
    /// mineure acceptée plutôt que de maintenir un second plan skinné non cullé.
    ///
    /// Renvoie le nombre de draw calls réellement émis (compteur de stats).
    pub(super) fn draw_skinned_shadows<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        scene: &Scene,
        offsets: &[Option<u32>],
    ) -> u32 {
        let mut draws = 0;
        for (&(obj_idx, instance_index), &offset) in self.draw_plan_skinned.iter().zip(offsets) {
            let Some(offset) = offset else {
                continue;
            };
            let MeshKind::Imported(mesh_idx) = scene.objects[obj_idx].mesh else {
                continue;
            };
            let Some(Some(gpu_mesh)) = self.imported_gpu_skinned.get(mesh_idx as usize) else {
                continue;
            };
            pass.set_pipeline(&self.skinned_shadow_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.skinned_models_bind_group, &[offset]);
            pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(
                0..gpu_mesh.num_indices,
                0,
                instance_index..instance_index + 1,
            );
            draws += 1;
        }
        draws
    }

    /// Rendu headless d'**un** mesh skinné, en une seule instance (chemin de
    /// test/vérification dédié — pas piloté par `draw_plan_skinned`). `app` ne sert qu'à
    /// fournir caméra + lumière (`write_uniforms`) ; sa scène n'est pas dessinée ici.
    pub fn render_skinned_test(
        &mut self,
        app: &mut AppState,
        mesh: &crate::gfx::mesh::SkinnedMeshData,
        joint_matrices: &[glam::Mat4],
        model_transform: glam::Mat4,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        app.camera.aspect = width as f32 / (height as f32).max(1.0);
        self.write_uniforms(app);
        let mut raw = std::mem::take(&mut self.joint_raw_scratch);
        let joint_offset = self
            .write_joint_matrices(0, joint_matrices, &mut raw)
            .expect("slot 0 < MAX_SKINNED_INSTANCES, toujours valide");
        self.joint_raw_scratch = raw;

        let gpu_mesh = crate::gfx::mesh::GpuMesh::new_skinned(&self.device, mesh);

        let model_uniform = ModelUniform {
            model: model_transform.to_cols_array_2d(),
            normal: glam::Mat4::from_mat3(
                glam::Mat3::from_mat4(model_transform).inverse().transpose(),
            )
            .to_cols_array_2d(),
            params: [0.0, 0.0, 0.6, 0.0], // pas de surbrillance ; roughness 0.6, reste par défaut
            color: [1.0, 1.0, 1.0, 1.0],
        };
        self.queue
            .write_buffer(&self.models_buf, 0, bytemuck::bytes_of(&model_uniform));

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skinned_test_target"),
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
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skinned_test_depth"),
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
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        // Cible HDR, locale à cet appel — cf. `hdr_view` de `render()`.
        // Chemin headless : toujours mono-échantillon (`self.msaa_samples == 1`,
        // cf. `new_impl`), donc pas de cible multisample à gérer ici.
        let (hdr_view, _) = create_hdr_view(&self.device, width, height, self.msaa_samples);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("skinned_test_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("skinned_test_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &hdr_view,
                    resolve_target: None,
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
            pass.set_pipeline(&self.skinned_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.skinned_models_bind_group, &[joint_offset]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(3, &self.textures[""], &[]);
            pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..gpu_mesh.num_indices, 0, 0..1);
        }

        // Tone mapping : HDR → `view` (le format lu par `finish_and_read_rgba`).
        // Pas de bloom ici : ce chemin sert uniquement au golden test de
        // skinning, qui n'a pas besoin du post-effet — `hdr_view` réutilisée comme
        // source de bloom factice, neutralisée par une intensité à 0.
        self.tonemap(&mut encoder, &hdr_view, &hdr_view, 0.0, &view);

        self.finish_and_read_rgba(encoder, &target, width, height)
    }
}
