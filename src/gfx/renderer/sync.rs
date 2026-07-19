use super::*;

impl Renderer {
    /// Transmet l'événement à l'UI. Retourne `true` s'il a été consommé par egui.
    pub fn on_ui_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        let (Some(window), Some(editor)) = (self.window.as_ref(), self.editor.as_mut()) else {
            return false; // rendu headless : pas d'UI
        };
        editor.on_window_event(window, event)
    }

    /// Réglages persistés courants (clé API, remapping manette…), `None` en rendu
    /// headless (pas d'`Editor`). Sprint 110 : lu par `App::gamepad_bindings`, qui
    /// n'a sinon aucun accès direct à `Editor` (privé à ce module).
    pub fn settings(&self) -> Option<&crate::app::settings::Settings> {
        self.editor.as_ref().map(|e| e.settings())
    }

    /// Bascule la fenêtre Multijoueur (bouton Start de la manette) — simple
    /// relais vers `Editor`, privé à ce module ; sans effet en headless.
    pub fn toggle_multiplayer_window(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_multiplayer_window();
        }
    }

    /// Bascule le HUD de Play (bouton Select de la manette) — même relais.
    pub fn toggle_play_hud(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_play_hud();
        }
    }

    /// Bascule l'overlay Paramètres minimal du mode Player (bouton Start de la
    /// manette ou touche Tab, en mode `--player`/mobile — Sprint 2) — même relais.
    pub fn toggle_player_settings(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_player_settings();
        }
    }

    /// Bascule la carte plein écran du mode Player (touche `M`) — même relais.
    pub fn toggle_player_map(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_player_map();
        }
    }

    /// Garantit que le buffer d'instances peut contenir `n` objets (le recrée s'il faut).
    pub(super) fn sync_objects(&mut self, scene: &Scene) {
        let n = scene.objects.len();
        if n > self.models_capacity {
            let cap = n.next_power_of_two().max(64);
            let (buf, bg) = create_models_buffer(&self.device, &self.model_layout, cap);
            self.models_buf = buf;
            self.models_bind_group = bg;
            self.models_capacity = cap;
            // `skinned_models_bind_group` référence `models_buf` par valeur : doit être
            // recréé avec le nouveau buffer, sinon le pipeline skinné continue de
            // dessiner avec l'ancien (erreur de validation ou instances obsolètes dès
            // que la scène dépasse la capacité initiale avec un mesh skinné présent).
            self.skinned_models_bind_group = create_skinned_models_bind_group(
                &self.device,
                &self.skinned_model_layout,
                &self.models_buf,
                &self.joint_buf,
            );
        }
    }

    /// Résout le `GpuMesh` d'un type de mesh (None si un modèle importé n'est pas encore chargé).
    pub(super) fn resolve_mesh(&self, mesh: MeshKind) -> Option<&GpuMesh> {
        match mesh {
            MeshKind::Imported(i) => self.imported_gpu.get(i as usize),
            k => self.meshes.get(&k),
        }
    }

    /// Construit les `GpuMesh` des modèles importés pas encore chargés sur GPU.
    pub(super) fn sync_imported(&mut self, scene: &Scene) {
        while self.imported_gpu.len() < scene.imported.len() {
            let m = &scene.imported[self.imported_gpu.len()];
            self.imported_gpu.push(GpuMesh::new(&self.device, &m.data));
            // Skinning GPU : mesh skinné en plus du statique si le glTF a un
            // skin (`ImportedMesh::skeleton`) — `None` sinon, la grande majorité des imports.
            let skinned = m
                .skinned_mesh_data()
                .map(|d| GpuMesh::new_skinned(&self.device, &d));
            self.imported_gpu_skinned.push(skinned);
        }
    }

    /// Hot-reload (Sprint 111) : vide le cache de textures (sauf la blanche par
    /// défaut, `""`, qui n'est pas chargée depuis un fichier) suite à un changement
    /// détecté dans le dossier d'assets de projet. `sync_textures` recharge alors
    /// depuis le disque au prochain appel — la nouvelle version d'un fichier
    /// retouché s'affiche donc sans redémarrer, quel que soit le schéma utilisé
    /// pour le référencer (`asset://`, `asset-id://`) : plus simple et robuste
    /// qu'une invalidation ciblée par chemin, qui devrait résoudre chaque forme
    /// vers le même fichier disque avant de savoir laquelle jeter.
    pub(crate) fn invalidate_asset_textures(&mut self) {
        self.textures.retain(|k, _| k.is_empty());
        // Les échecs mémorisés redeviennent tentables : un fichier réparé/ajouté
        // sur le disque doit pouvoir se charger au prochain `sync_textures`.
        self.failed_textures.clear();
    }

    /// Charge les textures référencées par la scène pas encore en cache.
    pub(super) fn sync_textures(&mut self, scene: &Scene) {
        for obj in &scene.objects {
            if obj.texture.is_empty()
                || self.textures.contains_key(&obj.texture)
                || self.failed_textures.contains(&obj.texture)
            {
                continue;
            }
            let Some((rgba, w, h)) = load_rgba(&obj.texture) else {
                log::error!("Texture illisible : {}", obj.texture);
                // Repli : mémorise l'échec pour ne pas réessayer (ni re-logger) à
                // chaque frame — les sites de dessin retombent déjà sur la texture
                // blanche `""` quand le chemin est absent du cache, inutile d'en
                // recréer une 1×1 par chemin cassé comme avant (audit juillet 2026).
                self.failed_textures.insert(obj.texture.clone());
                continue;
            };
            let bg = make_texture(
                &self.device,
                &self.queue,
                &self.tex_layout,
                &self.tex_sampler,
                &self.mipgen_pipeline,
                &self.mipgen_layout,
                &self.mipgen_sampler,
                &rgba,
                w,
                h,
            );
            self.textures.insert(obj.texture.clone(), bg);
        }
    }

    /// Pousse les uniforms (caméra + matrices modèle + surbrillance) depuis l'état.
    /// N'écrit le buffer d'un objet que si sa pose ou sa surbrillance a changé.
    pub(super) fn write_uniforms(&mut self, app: &AppState) {
        // Recul caméra (Sprint 1, `sprint10audit.md`) : décalage cosmétique du
        // rendu seulement (cf. doc `OrbitCamera::view_proj_shaken`), jamais de
        // `app.camera` lui-même.
        let shake = app.camera_shake_offset();
        let eye = app.camera.eye() + shake;
        let view_proj = app.camera.view_proj_shaken(shake);
        let camera_uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            // `view_proj` est toujours inversible (projection perspective + vue
            // rigide, jamais dégénérée) : pas de garde-fou nécessaire ici.
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&camera_uniform));

        // Éclairage de la scène + matrice de la carte d'ombre.
        let l = &app.scene.light;
        let mut dir = glam::Vec3::from_array(l.dir);
        if dir.length_squared() < 1e-6 {
            dir = glam::Vec3::Y;
        }
        dir = dir.normalize();
        // caméra orthographique placée « au niveau de la lumière », regardant l'origine.
        let up = if dir.x.abs() < 1e-3 && dir.z.abs() < 1e-3 {
            glam::Vec3::Z
        } else {
            glam::Vec3::Y
        };
        let view = glam::Mat4::look_at_rh(dir * 20.0, glam::Vec3::ZERO, up);
        let proj = glam::Mat4::orthographic_rh(-12.0, 12.0, -12.0, 12.0, 0.1, 60.0);
        let light_vp = proj * view;
        let mut points = [PointLightU {
            pos_range: [0.0; 4],
            color_int: [0.0; 4],
            spot: [0.0, -1.0, 0.0, -1.0],
        }; crate::scene::MAX_POINT_LIGHTS];
        // Culling/LOD : au-delà de la limite, on garde les lumières les plus proches
        // de la caméra (les plus visibles) plutôt que les premières de la liste. Le
        // plafond dépend de la qualité de rendu visée (perf en mode interactif « Basse »).
        let chosen = app
            .scene
            .nearest_point_lights(eye, app.render_quality.light_budget());
        let count = chosen.len();
        for (slot, &li) in points.iter_mut().zip(&chosen) {
            let pl = &app.scene.point_lights[li];
            slot.pos_range = [
                pl.position[0],
                pl.position[1],
                pl.position[2],
                pl.range.max(0.01),
            ];
            slot.color_int = [pl.color[0], pl.color[1], pl.color[2], pl.intensity];
            // Spot : direction normalisée + cos(demi-angle) ; w = -1 → lumière ponctuelle.
            let d = glam::Vec3::from_array(pl.spot_dir);
            let dir = if d.length_squared() > 1e-6 {
                d.normalize()
            } else {
                glam::Vec3::NEG_Y
            };
            let cos_cut = if pl.spot_angle > 0.0 {
                pl.spot_angle.to_radians().cos()
            } else {
                -1.0
            };
            slot.spot = [dir.x, dir.y, dir.z, cos_cut];
        }
        let scene_uniform = SceneUniform {
            light_dir: [l.dir[0], l.dir[1], l.dir[2], 0.0],
            light_color: [l.color[0], l.color[1], l.color[2], 0.0],
            // .y : vue de debug — canal inutilisé jusqu'ici, réutilisé plutôt
            // que d'agrandir l'uniform. Décodé dans `main.wgsl`.
            ambient: [l.ambient, app.debug_view.as_uniform(), 0.0, 0.0],
            light_vp: light_vp.to_cols_array_2d(),
            num_points: [count as f32, 0.0, 0.0, 0.0],
            points,
            sky_horizon: [
                app.scene.sky.horizon_color[0],
                app.scene.sky.horizon_color[1],
                app.scene.sky.horizon_color[2],
                0.0,
            ],
            sky_zenith: [
                app.scene.sky.zenith_color[0],
                app.scene.sky.zenith_color[1],
                app.scene.sky.zenith_color[2],
                0.0,
            ],
            fog: [
                app.scene.sky.fog_color[0],
                app.scene.sky.fog_color[1],
                app.scene.sky.fog_color[2],
                app.scene.sky.fog_density.max(0.0),
            ],
        };
        self.queue
            .write_buffer(&self.light_buf, 0, bytemuck::bytes_of(&scene_uniform));

        // Skip-rebuild : si les entrées de rendu (transforms/couleurs/sélection + caméra)
        // sont identiques à la frame précédente, le plan de dessin et le buffer d'instances
        // sont déjà à jour. Le hash capte TOUT changement pertinent → pas d'affichage figé.
        // (Les uniforms caméra/lumière ci-dessus sont toujours réécrits, ils sont bon marché.)
        let hash = render_input_hash(app);
        if hash == self.last_render_hash && !self.draw_plan.is_empty() {
            return;
        }
        self.last_render_hash = hash;

        // Instances ordonnées par (mesh, texture) pour permettre des draws groupés.
        // On bâtit en parallèle le buffer storage et le plan de rendu (même ordre).
        let planes = frustum_planes(app.camera.view_proj());
        // Culling par distance (Phase C, `sprintoptimation3daudit10h.md`) : complète le
        // frustum ci-dessus, sur la position caméra « pure » (pas le décalage cosmétique
        // de `write_uniforms`, qui ne doit affecter que le rendu, jamais la visibilité).
        let eye = app.camera.eye();
        let n = app.scene.objects.len();
        let order = &mut self.order_scratch;
        // Re-tri paresseux : l'ordre (groupé par mesh/texture pour le batching) ne dépend
        // pas des transforms ; on ne le recalcule que quand le nombre d'objets change.
        // Un ordre « périmé » reste une permutation valide de 0..n → rendu correct, au pire
        // batching sous-optimal jusqu'au prochain ajout/retrait.
        if self.last_sort_len != n {
            order.clear();
            order.extend(0..n);
            order.sort_by(|&a, &b| {
                let oa = &app.scene.objects[a];
                let ob = &app.scene.objects[b];
                mesh_key(oa.mesh)
                    .cmp(&mesh_key(ob.mesh))
                    .then_with(|| oa.texture.cmp(&ob.texture))
            });
            self.last_sort_len = n;
        }

        let models = &mut self.models_scratch;
        models.clear();
        self.draw_plan.clear();
        for &i in order.iter() {
            let obj = &app.scene.objects[i];
            // Skinning GPU : un objet skinné a sa propre palette de joints,
            // incompatible avec le batching par instances de ce plan — dessiné à part par
            // `draw_skinned_objects`, jamais ici (sinon il apparaîtrait deux fois).
            if is_skinned(&app.scene, obj.mesh) {
                continue;
            }
            let model = obj.transform.matrix();
            let highlight = app.highlight_of(i);
            // Matrice normale = inverse-transposée du bloc 3×3 (correct en scale non uniforme).
            let normal3 = glam::Mat3::from_mat4(model).inverse().transpose();
            models.push(ModelUniform {
                model: model.to_cols_array_2d(),
                normal: glam::Mat4::from_mat3(normal3).to_cols_array_2d(),
                params: [highlight, obj.metallic, obj.roughness, obj.emissive],
                color: [obj.color[0], obj.color[1], obj.color[2], 1.0],
            });
            let (lmin, lmax) = app.scene.local_aabb(obj.mesh);
            let radius = culling_radius_for(&app.scene, obj.mesh);
            let visible = obj.visible
                && distance_visible(eye, obj.transform.position, radius)
                && aabb_visible(&planes, model, lmin, lmax);
            // LOD géométrique (Phase D) : distance à la caméra « pure », comme le culling
            // par distance ci-dessus — jamais le décalage cosmétique de `write_uniforms`.
            let lod_mesh =
                foliage_lod_mesh(&app.scene, obj.mesh, eye.distance(obj.transform.position));
            self.draw_plan.push(InstanceDraw {
                obj: i,
                visible,
                mesh: lod_mesh,
            });
        }

        // Objets skinnés : leur ModelUniform occupe la queue de `models`,
        // après tous les objets statiques ci-dessus — `draw_skinned_objects` s'en sert
        // comme `base_instance` pour un draw individuel par objet (chacun avec sa propre
        // palette de joints, incompatible avec le batching des statiques).
        self.draw_plan_skinned.clear();
        for &i in order.iter() {
            let obj = &app.scene.objects[i];
            if !is_skinned(&app.scene, obj.mesh) || !obj.visible {
                continue;
            }
            let model = obj.transform.matrix();
            // Culling AABB approximatif : basé sur la pose de liaison (`aabb_min/max` de
            // l'import), pas sur l'enveloppe réelle de la pose animée — simplification
            // assumée (déplacement des os hors de cette boîte possible sur une anim
            // ample), commune même dans des moteurs de production comme premier jet.
            let (lmin, lmax) = app.scene.local_aabb(obj.mesh);
            if !aabb_visible(&planes, model, lmin, lmax) {
                continue;
            }
            let highlight = app.highlight_of(i);
            let normal3 = glam::Mat3::from_mat4(model).inverse().transpose();
            let instance_index = models.len() as u32;
            models.push(ModelUniform {
                model: model.to_cols_array_2d(),
                normal: glam::Mat4::from_mat3(normal3).to_cols_array_2d(),
                params: [highlight, obj.metallic, obj.roughness, obj.emissive],
                color: [obj.color[0], obj.color[1], obj.color[2], 1.0],
            });
            self.draw_plan_skinned.push((i, instance_index));
        }

        if !models.is_empty() {
            self.queue
                .write_buffer(&self.models_buf, 0, bytemuck::cast_slice(models));
        }
    }
}
