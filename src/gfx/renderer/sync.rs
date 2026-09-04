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

    /// Bascule les Paramètres du mode Player (bouton Start de la manette, en
    /// mode `--player`/mobile — Sprint 2 ; au clavier, via le menu pause depuis
    /// la roadmap post-audit UX v2 2026-09-04, 1.6) — même relais.
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

    /// Aide en jeu (F1, roadmap post-audit UX 2026-09-04, 5.5) — même relais.
    pub fn toggle_help(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_help();
        }
    }

    /// Fenêtre « Raccourcis clavier » (F1 hors Play, roadmap post-audit UX v2
    /// 2026-09-04, 4.3) — même relais.
    pub fn toggle_shortcuts(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_shortcuts();
        }
    }

    /// Une fenêtre du mode Player est-elle ouverte (`Editor::player_overlay_open`,
    /// roadmap v2 5.6) ? `false` en headless.
    pub fn player_overlay_open(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|e| e.player_overlay_open())
    }

    /// Coupe/rétablit le son et persiste (`Editor::toggle_mute`, roadmap v2
    /// 5.5) ; `None` en headless (rien à persister, rien à couper).
    pub fn toggle_mute(&mut self) -> Option<bool> {
        self.editor.as_mut().map(|e| e.toggle_mute())
    }

    /// Points egui par pixel physique (`Editor::pixels_per_point`) — 1 en headless.
    pub fn ui_pixels_per_point(&self) -> f32 {
        self.editor.as_ref().map_or(1.0, |e| e.pixels_per_point())
    }

    /// Une fenêtre egui occupe-t-elle ce point (`Editor::ui_owns_point`) ?
    /// `false` en headless.
    pub fn ui_owns_point(&self, p: egui::Pos2) -> bool {
        self.editor.as_ref().is_some_and(|e| e.ui_owns_point(p))
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
        let found = match mesh {
            MeshKind::Imported(i) => self.imported_gpu.get(i as usize),
            k => self.meshes.get(&k),
        };
        // Un mesh sans géométrie (asset introuvable au rechargement, cf.
        // `Scene::reload_imported` et `examples/broken_scene`) a des tampons GPU
        // vides : `Buffer::slice(..)` sur un tampon de taille 0 fait paniquer
        // wgpu (« buffer slices can not be empty ») — l'éditeur plantait en
        // ouvrant la scène-exemple des pannes (roadmap post-audit UX
        // 2026-09-04, lot 1.B). Un tel objet est simplement ignoré au dessin.
        found.filter(|m| m.num_indices > 0)
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
        // Ombres en cascade : une caméra orthographique de lumière par tranche du
        // frustum (cf. `compute_cascades`) — remplace l'ancienne boîte fixe de
        // 24 m autour de l'origine, qui laissait floues (ou sans ombre) les grandes
        // cartes. Caméra « pure » (sans recul cosmétique) : les ombres ne doivent
        // pas tressauter avec le shake d'encaissement.
        let (cascade_vps, splits) = compute_cascades(&app.camera, dir, self.shadow_size);
        let light_vp = cascade_vps[0];
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
            cascade_vp: cascade_vps.map(|m| m.to_cols_array_2d()),
            cascade_splits: [
                splits[0],
                splits[1],
                splits[2],
                1.0 / self.shadow_size.max(1) as f32,
            ],
        };
        self.queue
            .write_buffer(&self.light_buf, 0, bytemuck::bytes_of(&scene_uniform));
        // Une copie par cascade dont `light_vp` est la matrice de la cascade : bind
        // group 0 des passes d'ombre (cf. `Renderer::cascade_bind_groups`).
        for (buf, vp) in self.cascade_bufs.iter().zip(cascade_vps) {
            let cascade_uniform = SceneUniform {
                light_vp: vp.to_cols_array_2d(),
                ..scene_uniform
            };
            self.queue
                .write_buffer(buf, 0, bytemuck::bytes_of(&cascade_uniform));
        }

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
            // Objet translucide : hors du lot opaque (et de la passe d'ombre) — dessiné
            // à part, trié, par `draw_transparent_objects` (plan construit plus bas).
            if obj.opacity < 1.0 {
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

        // Objets translucides (`opacity < 1`, non skinnés) : après tout le reste dans
        // `models`, triés du plus loin au plus près de la caméra — l'ordre de dessin
        // est l'ordre du mélange alpha, un objet proche doit se composer par-dessus
        // un objet lointain. Le tri est sur `eye` « pure », comme le culling.
        self.draw_plan_transparent.clear();
        let mut translucent: Vec<(usize, f32)> = order
            .iter()
            .copied()
            .filter(|&i| {
                let obj = &app.scene.objects[i];
                obj.opacity < 1.0 && obj.visible && !is_skinned(&app.scene, obj.mesh)
            })
            .map(|i| {
                (
                    i,
                    eye.distance_squared(app.scene.objects[i].transform.position),
                )
            })
            .collect();
        translucent.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (i, _) in translucent {
            let obj = &app.scene.objects[i];
            let model = obj.transform.matrix();
            let (lmin, lmax) = app.scene.local_aabb(obj.mesh);
            if !aabb_visible(&planes, model, lmin, lmax) {
                continue;
            }
            let normal3 = glam::Mat3::from_mat4(model).inverse().transpose();
            let instance = models.len() as u32;
            models.push(ModelUniform {
                model: model.to_cols_array_2d(),
                normal: glam::Mat4::from_mat3(normal3).to_cols_array_2d(),
                params: [
                    app.highlight_of(i),
                    obj.metallic,
                    obj.roughness,
                    obj.emissive,
                ],
                color: [
                    obj.color[0],
                    obj.color[1],
                    obj.color[2],
                    obj.opacity.clamp(0.0, 1.0),
                ],
            });
            self.draw_plan_transparent.push(TransparentDraw {
                obj: i,
                instance,
                mesh: obj.mesh,
            });
        }

        if !models.is_empty() {
            self.queue
                .write_buffer(&self.models_buf, 0, bytemuck::cast_slice(models));
        }
    }
}
