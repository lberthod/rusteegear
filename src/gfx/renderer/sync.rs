use super::*;

/// Encodage GPU du composant eau d'un objet (`[0; 4]` = pas une surface d'eau).
fn water_uniform(obj: &crate::scene::SceneObject) -> [f32; 4] {
    obj.water.map(|w| w.as_uniform()).unwrap_or([0.0; 4])
}

impl Renderer {
    /// Altitude du plan de réflexion : la surface d'eau « rivière » visible la
    /// plus proche de la cible caméra, échantillonnée au sommet le plus proche
    /// (une rivière en pente n'a pas une altitude unique — on prend celle qui
    /// est sous les yeux du joueur). `None` sans surface d'eau.
    fn reflection_plane_y(&self, app: &AppState) -> Option<f32> {
        let target = app.camera.target;
        let mut best: Option<(f32, f32)> = None; // (distance² en xz, altitude)
        for obj in &app.scene.objects {
            if !obj.visible
                || !matches!(obj.water, Some(w) if w.kind == crate::scene::WaterKind::Riviere)
            {
                continue;
            }
            let m = obj.transform.matrix();
            let mut consider = |p: glam::Vec3| {
                let d = (p.x - target.x).powi(2) + (p.z - target.z).powi(2);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, p.y));
                }
            };
            match obj.mesh {
                MeshKind::Imported(i) => {
                    if let Some(mesh) = app.scene.imported.get(i as usize) {
                        // Un sommet sur quatre suffit (les nappes sont denses).
                        for v in mesh.data.vertices.iter().step_by(4) {
                            consider(m.transform_point3(glam::Vec3::from_array(v.position)));
                        }
                    }
                }
                _ => consider(obj.transform.position),
            }
        }
        best.map(|(_, y)| y)
    }

    /// Crée/redimensionne la cible de réflexion planaire si la scène contient
    /// une surface d'eau « rivière », la libère sinon.
    pub(super) fn ensure_reflection_target(&mut self, scene: &Scene, width: u32, height: u32) {
        let needed = scene.objects.iter().any(|o| {
            o.visible && matches!(o.water, Some(w) if w.kind == crate::scene::WaterKind::Riviere)
        });
        if !needed {
            self.reflection = None;
            return;
        }
        if self
            .reflection
            .as_ref()
            .is_some_and(|r| r.width == width && r.height == height)
        {
            return;
        }
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("reflection_color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("reflection_depth"),
            size,
            mip_level_count: 1,
            sample_count: self.msaa_samples,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = (self.msaa_samples > 1).then(|| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("reflection_color_msaa"),
                    size,
                    mip_level_count: 1,
                    sample_count: self.msaa_samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: HDR_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        });
        let view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let tex_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reflection_tex_bg"),
            layout: &self.tex_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.tex_sampler),
                },
            ],
        });
        let uniform = |label: &str, size: usize| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let camera_buf = uniform("reflection_camera", std::mem::size_of::<CameraUniform>());
        let light_buf = uniform("reflection_light", std::mem::size_of::<SceneUniform>());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reflection_camera_bg"),
            layout: &self.camera_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_buf.as_entire_binding(),
                },
            ],
        });
        self.reflection = Some(ReflectionTarget {
            width,
            height,
            view,
            msaa_view,
            depth_view,
            tex_bind_group,
            camera_buf,
            light_buf,
            bind_group,
        });
    }

    /// Fixe le temps d'animation des shaders (eau) pour un rendu headless —
    /// `render_scene_headless` ne fait pas avancer l'horloge (goldens
    /// déterministes) ; un aperçu (`examples/gen_riviere_preview.rs`) choisit
    /// ainsi l'instant à capturer.
    pub fn set_anim_time(&mut self, seconds: f32) {
        self.anim_time = seconds;
    }
}

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
        // Base droite/haut de la caméra (billboards de particules,
        // `particles.wgsl`) : même idiome que `OrbitCamera::pan` — le décalage
        // de tremblement s'annule dans la direction (`target`/`eye` décalés
        // à l'identique par `view_proj_shaken`), pas besoin de le reprendre ici.
        let forward = (app.camera.target - app.camera.eye()).normalize_or_zero();
        let cam_right = forward.cross(glam::Vec3::Y).normalize_or_zero();
        let cam_up = cam_right.cross(forward);
        let camera_uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, self.anim_time],
            // `view_proj` est toujours inversible (projection perspective + vue
            // rigide, jamais dégénérée) : pas de garde-fou nécessaire ici.
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            cam_right: [cam_right.x, cam_right.y, cam_right.z, 0.0],
            cam_up: [cam_up.x, cam_up.y, cam_up.z, 0.0],
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
        // Réflexion planaire (eau) + extensions d'atmosphère (cf. `SceneUniform::extra`).
        let sky = &app.scene.sky;
        let plane_y = self.reflection_plane_y(app);
        let reflection_on = plane_y.is_some() && self.reflection.is_some();
        let (vx, vy, vw, vh) = self.main_viewport;
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
            extra: [
                sky.fog_height_base,
                sky.fog_height_falloff.max(0.0),
                sky.sun_glow.max(0.0),
                plane_y.unwrap_or(0.0),
            ],
            extra2: [vx, vy, vw, vh],
            extra3: [if reflection_on { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.light_buf, 0, bytemuck::bytes_of(&scene_uniform));
        // Passe de réflexion : caméra miroir (symétrique par rapport au plan
        // d'eau y = h — `view_proj · R`, R = réflexion sur ce plan), même uniform
        // scène mais viewport de la cible et drapeau « rejeter sous le plan ».
        self.reflection_active = reflection_on;
        if let (Some(r), Some(h)) = (self.reflection.as_ref(), plane_y)
            && reflection_on
        {
            let reflect = glam::Mat4::from_cols_array_2d(&[
                [1.0, 0.0, 0.0, 0.0],
                [0.0, -1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 2.0 * h, 0.0, 1.0],
            ]);
            let vp_r = view_proj * reflect;
            let eye_r = glam::Vec3::new(eye.x, 2.0 * h - eye.y, eye.z);
            let camera_r = CameraUniform {
                view_proj: vp_r.to_cols_array_2d(),
                eye: [eye_r.x, eye_r.y, eye_r.z, self.anim_time],
                inv_view_proj: vp_r.inverse().to_cols_array_2d(),
                // Passe de réflexion planaire : ne dessine jamais de particules
                // (texture lue par le shader d'eau, cf. plus haut), inutilisé.
                cam_right: [0.0; 4],
                cam_up: [0.0; 4],
            };
            self.queue
                .write_buffer(&r.camera_buf, 0, bytemuck::bytes_of(&camera_r));
            let light_r = SceneUniform {
                extra2: [0.0, 0.0, r.width as f32, r.height as f32],
                extra3: [0.0, 1.0, 0.0, 0.0],
                ..scene_uniform
            };
            self.queue
                .write_buffer(&r.light_buf, 0, bytemuck::bytes_of(&light_r));
        }
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
                water: water_uniform(obj),
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
                water: water_uniform(obj),
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
                water: water_uniform(obj),
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

        // Particules (Sprint 132) : buffer/pipeline/shader dédiés (`particles.wgsl`,
        // `particle_buf`), pas mêlées à `models`/`model_layout` — triées du plus
        // loin au plus près comme les objets transparents ci-dessus (même `eye`
        // « pure », sans tremblement de caméra), dessinées après eux
        // (`draw_particles`, appelée en tout dernier de la passe principale).
        let live: Vec<&crate::runtime::particles::Particle> = app.particles.iter().collect();
        let mut particle_order: Vec<usize> = (0..live.len()).collect();
        particle_order.sort_by(|&a, &b| {
            let da = eye.distance_squared(live[a].pos);
            let db = eye.distance_squared(live[b].pos);
            db.total_cmp(&da)
        });
        self.particle_scratch.clear();
        for &i in &particle_order {
            let p = live[i];
            let alpha = p.current_alpha();
            if alpha <= 0.001 {
                continue;
            }
            self.particle_scratch.push(ParticleInstance {
                center: [p.pos.x, p.pos.y, p.pos.z],
                size: p.size,
                color: [p.color[0], p.color[1], p.color[2], alpha],
            });
        }
        if !self.particle_scratch.is_empty() {
            self.ensure_particle_capacity(self.particle_scratch.len());
            self.queue.write_buffer(
                &self.particle_buf,
                0,
                bytemuck::cast_slice(&self.particle_scratch),
            );
        }
    }
}
