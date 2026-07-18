//! Petit visualiseur de fichiers `.glb`/`.gltf` (assets/models/) — indépendant de
//! l'éditeur principal. Deux contextes wgpu séparés : le nôtre (fenêtre + UI egui)
//! et un `Renderer` headless (moteur) qui rasterise la scène 3D dans une texture
//! affichée comme une image egui — évite de tirer toute la chrome de l'éditeur
//! (menus, hiérarchie, HUD...), cf. `motor3derust::gfx::renderer::Renderer::render`
//! qui exige un `Editor` complet pour peindre sur une vraie surface de fenêtre.
//!
//! `cargo run --bin glbviewer`

use std::path::PathBuf;
use std::sync::Arc;

use egui::ViewportId;
use glam::Vec3;
use motor3derust::app::AppState;
use motor3derust::gfx::renderer::Renderer;
use motor3derust::scene::{AnimationState, ImportedMesh, MeshKind, Scene, SceneObject, Transform};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const MODELS_DIR: &str = "assets/models";
/// Côté (pixels) des vignettes affichées dans la liste — décodées puis
/// redimensionnées une seule fois au premier affichage (cf. `load_thumbnail`),
/// pas la pleine résolution (640×480) des aperçus générés par le pipeline
/// Blender : inutile de garder ça en mémoire GPU pour un aperçu de 28 px.
const THUMB_SIZE: u32 = 28;

fn discover_models() -> Vec<PathBuf> {
    let mut models: Vec<PathBuf> = std::fs::read_dir(MODELS_DIR)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("glb"))
        .collect();
    models.sort();
    models
}

/// Décode et sous-échantillonne le PNG d'aperçu de `model_path` (convention du
/// pipeline Blender : `<nom>_preview.png` à côté du `.glb`, cf.
/// `scripts/blender/`) — `None` si absent (~45 % des modèles n'en ont pas
/// encore) ou illisible, pas une erreur bloquante.
fn load_thumbnail(model_path: &std::path::Path) -> Option<egui::ColorImage> {
    let preview_path =
        model_path.with_file_name(format!("{}_preview.png", model_path.file_stem()?.to_str()?));
    let img = image::open(&preview_path).ok()?.to_rgba8();
    let img = image::imageops::resize(
        &img,
        THUMB_SIZE,
        THUMB_SIZE,
        image::imageops::FilterType::Triangle,
    );
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [THUMB_SIZE as usize, THUMB_SIZE as usize],
        img.as_raw(),
    ))
}

/// Repli pour les modèles sans `_preview.png` (~45 % du catalogue) : on a
/// déjà tout ce qu'il faut sous la main pour en rendre une nous-mêmes — un
/// `Renderer` headless et le même chargeur glTF que la scène principale.
/// Réutilise `thumb_state` (au lieu de reconstruire un `AppState` à chaque
/// appel) : `AppState::new()` recharge les réglages depuis le disque et
/// initialise une VM Lua, bien trop coûteux pour en refaire un par vignette.
fn render_fallback_thumbnail(
    renderer: &mut Renderer,
    thumb_state: &mut Option<AppState>,
    model_path: &std::path::Path,
) -> Option<egui::ColorImage> {
    let (data, aabb_min, aabb_max) =
        motor3derust::scene::import::load_gltf(&model_path.to_string_lossy()).ok()?;
    let imported = ImportedMesh {
        data,
        aabb_min,
        aabb_max,
        ..Default::default()
    };
    let center = (aabb_min + aabb_max) * 0.5;
    let radius = (aabb_max - aabb_min).max(Vec3::splat(0.01)).length() * 0.5;

    let state = thumb_state.get_or_insert_with(AppState::new);
    // Ajoute plutôt que remplace `scene.imported` (même piège que
    // `Viewer::load_model`, cf. sa doc) : `Renderer::sync_imported` n'uploade
    // que ce qui dépasse son cache GPU déjà construit — repartir d'un vecteur
    // à un élément à chaque appel ne redéclencherait jamais l'upload au-delà
    // du tout premier, et chaque vignette suivante afficherait la précédente.
    let mesh_index = state.scene.imported.len() as u32;
    state.scene.imported.push(imported);
    state.scene.objects = vec![SceneObject {
        mesh: MeshKind::Imported(mesh_index),
        ..Default::default()
    }];
    state.camera.target = center;
    state.camera.yaw = 0.7;
    state.camera.pitch = 0.45;
    state.camera.distance = (radius * 2.4).clamp(1.5, 50.0);

    let pixels = renderer.render_scene_headless(state, THUMB_SIZE, THUMB_SIZE);
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [THUMB_SIZE as usize, THUMB_SIZE as usize],
        &pixels,
    ))
}

/// Catégorie déduite du préfixe de nom de fichier (convention déjà en place
/// dans `assets/models/`, cf. `nature_*`/`monster_*`/`fauna_*`/`hamlet_*`/
/// `item_*`/`creature*`) — pas de métadonnées séparées à maintenir.
fn category_of(stem: &str) -> &'static str {
    if stem.starts_with("creature") {
        "🐉 Créatures"
    } else if stem.starts_with("monster_") {
        "👹 Monstres"
    } else if stem.starts_with("fauna_") {
        "🦋 Faune"
    } else if stem.starts_with("nature_") {
        "🌿 Nature"
    } else if stem.starts_with("hamlet_") {
        "🏘️ Hameau"
    } else if stem.starts_with("item_") {
        "🎒 Objets"
    } else {
        "📦 Autres"
    }
}

/// Ordre d'affichage des catégories (plutôt qu'un tri alphabétique) : les
/// groupes les plus nombreux/consultés en premier.
const CATEGORY_ORDER: [&str; 7] = [
    "🐉 Créatures",
    "👹 Monstres",
    "🦋 Faune",
    "🌿 Nature",
    "🏘️ Hameau",
    "🎒 Objets",
    "📦 Autres",
];

/// Filtre de recherche partagé par la liste principale et les sous-groupes —
/// vide = tout passe.
fn matches_filter(models: &[PathBuf], needle: &str, i: usize) -> bool {
    needle.is_empty() || {
        let name = models[i].file_stem().and_then(|s| s.to_str()).unwrap_or("");
        name.to_lowercase().contains(needle)
    }
}

/// Préfixe de fichier associé à `category_of`, à retirer avant de chercher un
/// sous-groupe (cf. `group_by_category`) — `""` pour « Autres », qui n'a pas de
/// préfixe fixe (fourre-tout de tout ce que `category_of` ne reconnaît pas).
fn category_prefix(stem: &str) -> &'static str {
    const PREFIXES: [&str; 5] = ["monster_", "fauna_", "nature_", "hamlet_", "item_"];
    if stem.starts_with("creature") {
        return "creature";
    }
    PREFIXES
        .into_iter()
        .find(|p| stem.starts_with(p))
        .unwrap_or("")
}

/// Une catégorie de la liste, avec ses sous-catégories générées
/// automatiquement (cf. `group_by_category`).
struct Category {
    label: &'static str,
    /// Sous-groupes détectés automatiquement : au moins 6 fichiers de cette
    /// catégorie partagent le même premier mot après le préfixe (ex. les 40
    /// `siege_*` dans « Autres », ajoutés après coup et ne correspondant à
    /// aucun préfixe connu de `category_of`) — évite de maintenir `category_of`
    /// à la main à chaque nouveau pack d'assets. En dessous de ce seuil, un mot
    /// partagé par 2-3 fichiers (ex. `nature_willow`/`nature_willow_sway`) ne
    /// vaut pas un niveau de repli supplémentaire, cf. `flat`.
    subgroups: Vec<(String, Vec<usize>)>,
    /// Reste de la catégorie, sans sous-groupe assez grand pour se distinguer.
    flat: Vec<usize>,
}

/// Regroupe les indices de `models` par catégorie puis sous-catégorie,
/// calculé une fois au démarrage (la liste de fichiers ne change pas en cours
/// de session).
fn group_by_category(models: &[PathBuf]) -> Vec<Category> {
    let mut buckets: Vec<(&'static str, Vec<usize>)> =
        CATEGORY_ORDER.iter().map(|&c| (c, Vec::new())).collect();
    for (i, path) in models.iter().enumerate() {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let category = category_of(stem);
        if let Some((_, indices)) = buckets.iter_mut().find(|(c, _)| *c == category) {
            indices.push(i);
        }
    }
    buckets.retain(|(_, indices)| !indices.is_empty());

    buckets
        .into_iter()
        .map(|(label, indices)| {
            let mut by_word: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
            for &i in &indices {
                let stem = models[i].file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let rest = stem
                    .strip_prefix(category_prefix(stem))
                    .unwrap_or(stem)
                    .trim_start_matches('_');
                if let Some(word) = rest.split('_').next().filter(|w| !w.is_empty()) {
                    by_word.entry(word.to_string()).or_default().push(i);
                }
            }
            let mut subgroups = Vec::new();
            let mut flat = Vec::new();
            for (word, group) in by_word {
                if group.len() > 5 {
                    subgroups.push((word, group));
                } else {
                    flat.extend(group);
                }
            }
            flat.sort();
            Category {
                label,
                subgroups,
                flat,
            }
        })
        .collect()
}

#[derive(Clone, Copy)]
struct ModelStats {
    vertices: usize,
    triangles: usize,
    size: Vec3,
}

/// Cadrage caméra visé, atteint en douceur par `Viewer::update_camera` plutôt
/// qu'un saut instantané — rend les changements de modèle/recentrages
/// nettement plus fluides à l'œil qu'un simple `self.state.camera = ...`.
struct CameraGoal {
    target: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

/// Différence angulaire la plus courte de `from` vers `to` (dans `[-π, π]`) —
/// évite qu'une orbite lerp fasse le tour long quand l'angle traverse ±π.
fn shortest_angle_delta(from: f32, to: f32) -> f32 {
    let mut d = (to - from) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

/// Contexte wgpu propre à notre fenêtre (présentation + UI egui) — distinct du
/// device interne du `Renderer` headless utilisé pour rasteriser la scène.
struct WindowGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl WindowGpu {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .expect("création de la surface impossible");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("aucun adaptateur GPU trouvé");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("glbviewer_device"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("échec de la création du device");
        // Format non-sRGB de préférence : egui peint déjà en couleurs gamma-
        // corrigées lui-même, une surface sRGB lui ferait appliquer une
        // double correction (`egui_wgpu` avertit sinon à chaque lancement).
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        Self {
            device,
            queue,
            surface,
            config,
        }
    }

    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }
}

struct Egui {
    ctx: egui::Context,
    winit_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl Egui {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Window) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let winit_state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(
            device,
            format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: true,
                predictable_texture_filtering: false,
            },
        );
        Self {
            ctx,
            winit_state,
            renderer,
        }
    }
}

#[derive(Default)]
struct Viewer {
    window: Option<Arc<Window>>,
    gpu: Option<WindowGpu>,
    egui: Option<Egui>,
    /// Renderer headless du moteur — sert uniquement à rasteriser la scène 3D
    /// dans une texture RGBA8 lue par egui, jamais à peindre directement sur
    /// notre fenêtre (deux contextes wgpu distincts).
    scene_renderer: Option<Renderer>,
    /// Renderer headless séparé pour `render_fallback_thumbnail`. Ne PEUT PAS
    /// partager `scene_renderer` : `Renderer::sync_imported` (src/gfx/renderer.rs)
    /// n'ajoute jamais qu'à la fin de son cache GPU, indexé globalement au
    /// `Renderer` — deux scènes indépendantes qui grandissent chacune de leur
    /// côté sur le même `Renderer` verraient leurs indices se marcher dessus
    /// (une vignette afficherait le mesh d'un tout autre modèle).
    thumb_renderer: Option<Renderer>,
    state: AppState,
    models: Vec<PathBuf>,
    /// Indices de `models` groupés par catégorie/sous-catégorie (cf.
    /// `group_by_category`), calculé une fois au démarrage.
    categories: Vec<Category>,
    filter: String,
    current: Option<usize>,
    stats: Option<ModelStats>,
    scene_tex: Option<egui::TextureHandle>,
    /// Taille (pixels physiques) utilisée pour rasteriser la scène ce tour-ci —
    /// recalculée sur la taille réellement disponible du frame précédent (un
    /// tour de retard sur un redimensionnement, imperceptible en pratique).
    viewport_size: (u32, u32),
    error: Option<String>,
    /// Noms des clips d'animation du modèle courant (vide = mesh statique,
    /// pas de mode Play possible).
    available_clips: Vec<String>,
    /// Mode Play actif : fait avancer `SceneObject.animation.time` chaque
    /// frame (cf. `render_frame`). Coupé au chargement d'un nouveau modèle.
    playing: bool,
    /// Horodatage de la frame précédente, pour calculer `dt` — `None` juste
    /// après le lancement ou juste après une reprise (`resumed`), pour ne
    /// jamais intégrer un `dt` géant sur la toute première frame.
    last_frame: Option<std::time::Instant>,
    /// Cadrage caméra en cours de transition douce (`None` = caméra déjà à
    /// destination, plus rien à interpoler).
    camera_goal: Option<CameraGoal>,
    /// `true` s'il faut re-rasteriser la scène 3D cette frame (changement de
    /// modèle, caméra en mouvement, redimensionnement…). En dehors du mode
    /// Play, sans ça, on repasserait par `render_scene_headless` — un aller-
    /// retour GPU complet + lecture des pixels — à *chaque* frame même figée,
    /// juste pour repeindre une image identique : le geste (glisser dans la
    /// liste, taper une recherche) devenait perceptiblement saccadé.
    dirty: bool,
    /// Vignettes de liste, chargées à la demande au premier affichage de
    /// chaque entrée (indexées comme `models`) — absent = pas encore tenté ou
    /// en attente de budget (cf. `render_frame`), `None` = tenté et
    /// définitivement sans aperçu possible (glTF illisible).
    thumbnails: std::collections::HashMap<usize, Option<egui::TextureHandle>>,
    /// `AppState` dédié à `render_fallback_thumbnail`, réutilisé d'un appel à
    /// l'autre — jamais celui de la vue principale (`state`), qu'on ne veut
    /// pas déranger juste pour générer une vignette.
    thumb_state: Option<AppState>,
}

impl Viewer {
    fn load_model(&mut self, index: usize) {
        let Some(path) = self.models.get(index).cloned() else {
            return;
        };
        let path_str = path.to_string_lossy().to_string();
        match motor3derust::scene::import::load_gltf(&path_str) {
            Ok((data, aabb_min, aabb_max)) => {
                let vertices = data.vertices.len();
                let triangles = data.indices.len() / 3;
                let mut imported = ImportedMesh {
                    name: path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("modèle")
                        .to_string(),
                    path: path_str,
                    data,
                    aabb_min,
                    aabb_max,
                    ..Default::default()
                };
                // Reparse le glTF pour le squelette/les clips (mesh statique
                // sans effet, cf. la doc de `load_skinning`) — nécessaire pour
                // que le mode Play ait quelque chose à jouer.
                imported.load_skinning();
                self.available_clips = imported.clips.iter().map(|c| c.name.clone()).collect();
                // « Walk » de préférence à « Idle » comme clip par défaut : la
                // convention des packs Blender du projet (cf. `scripts/blender/
                // creature_kit.py`) donne aux créatures un clip Idle très
                // subtil (respiration/balancement à peine visible) — au premier
                // coup d'œil dans un viewer, ça se voit à peine et donne
                // l'impression que « rien ne joue ». Walk, quand il existe,
                // montre un vrai mouvement dès le chargement.
                let clip_name = self
                    .available_clips
                    .iter()
                    .find(|c| c.as_str() == "Walk")
                    .cloned()
                    .or_else(|| imported.default_clip().map(str::to_string));
                let animation = clip_name.map(|clip| AnimationState {
                    clip,
                    ..Default::default()
                });
                self.playing = animation.is_some();

                // `Renderer::sync_imported` n'uploade que les entrées AU-DELÀ de
                // `imported_gpu.len()` (cf. src/gfx/renderer.rs) : remplacer
                // `scene.imported` en gardant la même longueur (1) ne redéclenche
                // jamais l'upload GPU, et le viewport continue d'afficher le tout
                // premier modèle chargé. On ajoute donc plutôt une nouvelle entrée
                // à chaque changement, et on pointe l'unique objet dessus.
                let mesh_index = self.state.scene.imported.len() as u32;
                self.state.scene.imported.push(imported);
                self.state.scene.objects = vec![SceneObject {
                    name: "modèle".into(),
                    transform: Transform::from_pos(Vec3::ZERO),
                    mesh: MeshKind::Imported(mesh_index),
                    animation,
                    ..Default::default()
                }];
                self.current = Some(index);
                self.stats = Some(ModelStats {
                    vertices,
                    triangles,
                    size: aabb_max - aabb_min,
                });
                self.error = None;
                self.frame_camera_on_aabb(aabb_min, aabb_max);
                if let Some(window) = &self.window {
                    let title = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("GLB Viewer");
                    window.set_title(&format!("GLB Viewer — {title}"));
                }
            }
            Err(e) => {
                self.error = Some(format!("Échec du chargement : {e}"));
            }
        }
    }

    /// Vise un cadrage 3/4 légèrement plongeant sur l'AABB du modèle,
    /// distance proportionnelle à la plus grande dimension — atteint en
    /// douceur par `update_camera`, pas de saut instantané (cf. la doc de
    /// `CameraGoal`).
    fn frame_camera_on_aabb(&mut self, aabb_min: Vec3, aabb_max: Vec3) {
        let center = (aabb_min + aabb_max) * 0.5;
        let extent = (aabb_max - aabb_min).max(Vec3::splat(0.01));
        let radius = extent.length() * 0.5;
        self.camera_goal = Some(CameraGoal {
            target: center,
            yaw: 0.7,
            pitch: 0.45,
            distance: (radius * 2.4).clamp(1.5, 50.0),
        });
        self.dirty = true;
    }

    fn recenter(&mut self) {
        if let Some(imported) = self.state.scene.imported.first() {
            let (min, max) = (imported.aabb_min, imported.aabb_max);
            self.frame_camera_on_aabb(min, max);
        }
    }

    /// Fait avancer la caméra vers `camera_goal` d'un pas proportionnel à
    /// `dt` (lissage exponentiel, indépendant du frame-rate) — appelé chaque
    /// frame tant qu'une transition est en cours. S'arrête (et référence la
    /// destination exactement) une fois suffisamment proche, plutôt que de
    /// tourner indéfiniment à une distance infinitésimale du but.
    fn update_camera(&mut self, dt: f32) {
        let Some(goal) = &self.camera_goal else {
            return;
        };
        const SHARPNESS: f32 = 12.0;
        let t = 1.0 - (-SHARPNESS * dt).exp();
        let cam = &mut self.state.camera;
        let d_target = goal.target - cam.target;
        let d_distance = goal.distance - cam.distance;
        let d_yaw = shortest_angle_delta(cam.yaw, goal.yaw);
        let d_pitch = goal.pitch - cam.pitch;
        cam.target += d_target * t;
        cam.distance += d_distance * t;
        cam.yaw += d_yaw * t;
        cam.pitch += d_pitch * t;
        let settled = d_target.length() < 0.005
            && d_distance.abs() < 0.005
            && d_yaw.abs() < 0.001
            && d_pitch.abs() < 0.001;
        if settled {
            let goal = self.camera_goal.take().unwrap();
            let cam = &mut self.state.camera;
            cam.target = goal.target;
            cam.distance = goal.distance;
            cam.yaw = goal.yaw;
            cam.pitch = goal.pitch;
        }
        self.dirty = true;
    }

    /// Charge le modèle `delta` positions plus loin dans la liste complète
    /// (pas filtrée), en bouclant — raccourcis clavier ←/→.
    fn load_relative(&mut self, delta: i32) {
        let n = self.models.len();
        if n == 0 {
            return;
        }
        let current = self.current.unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(n as i32) as usize;
        self.load_model(next);
    }

    fn render_frame(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if self.gpu.is_none()
            || self.egui.is_none()
            || self.scene_renderer.is_none()
            || self.thumb_renderer.is_none()
        {
            return;
        }

        // Acquérir la surface de présentation AVANT de construire quoi que ce
        // soit avec egui (même pattern que `Renderer::render`, cf.
        // `src/gfx/renderer.rs`) : `ctx.run_ui` consomme irrémédiablement le
        // delta de texture de l'atlas de polices (une fois pris, il n'est
        // jamais régénéré tant qu'aucun nouveau glyphe n'apparaît) — sortir
        // en cours de frame *après* avoir construit l'UI perdrait ce delta
        // pour de bon et l'atlas de polices ne s'afficherait plus jamais.
        let gpu = self.gpu.as_mut().unwrap();
        use wgpu::CurrentSurfaceTexture as C;
        let frame = match gpu.surface.get_current_texture() {
            C::Success(t) | C::Suboptimal(t) => t,
            C::Outdated | C::Lost => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
            C::Timeout | C::Occluded => return,
            C::Validation => {
                log::error!("erreur de validation de la surface");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("glbviewer_encoder"),
            });

        // `self.update_camera`/l'avance d'animation demandent un `&mut self`
        // entier — fait avant d'emprunter `self.egui`/`self.scene_renderer`
        // ci-dessous (phase 1), sinon ces emprunts partiels l'en empêcheraient.
        let now = std::time::Instant::now();
        let dt = self
            .last_frame
            .map(|prev| (now - prev).as_secs_f32())
            .unwrap_or(0.0);
        self.last_frame = Some(now);
        self.update_camera(dt);
        if self.playing {
            for obj in &mut self.state.scene.objects {
                if let Some(anim) = obj.animation.as_mut() {
                    anim.time += dt * anim.speed;
                }
            }
        }
        // Ne re-rasterise la scène 3D (aller-retour GPU complet + lecture des
        // pixels, cf. la doc de `dirty`) que si quelque chose a réellement
        // changé cette frame — sinon on réaffiche juste la texture déjà
        // uploadée. `scene_tex.is_none()` couvre le tout premier appel.
        let need_render = self.playing || self.dirty || self.scene_tex.is_none();
        self.dirty = false;

        // Phase 1 : rasteriser la scène + construire l'UI. `gpu` (surface de
        // présentation) n'est ensuite plus emprunté — les actions collectées
        // (changement de modèle, recentrage caméra) demandent un `&mut self`
        // complet, incompatible avec un emprunt de `self.gpu` tenu jusqu'à la
        // phase de peinture plus bas. `frame`/`view`/`encoder` sont des
        // valeurs possédées, indépendantes de cet emprunt.
        let egui = self.egui.as_mut().unwrap();
        let scene_renderer = self.scene_renderer.as_mut().unwrap();
        let thumb_renderer = self.thumb_renderer.as_mut().unwrap();

        let (vw, vh) = self.viewport_size;
        let pixels =
            need_render.then(|| scene_renderer.render_scene_headless(&mut self.state, vw, vh));
        let tex_options = egui::TextureOptions::LINEAR;

        let raw_input = egui.winit_state.take_egui_input(&window);

        let mut filter = std::mem::take(&mut self.filter);
        let models = &self.models;
        let categories = &self.categories;
        let current = self.current;
        let stats = self.stats;
        let error = self.error.clone();
        let available_clips = &self.available_clips;
        let mut playing = self.playing;
        let current_anim = self
            .state
            .scene
            .objects
            .first()
            .and_then(|o| o.animation.as_ref())
            .map(|a| (a.clip.clone(), a.time));
        let current_clip = current_anim
            .as_ref()
            .map(|(clip, _)| clip.clone())
            .unwrap_or_default();
        // Progression 0..1 dans le clip courant, pour la barre de lecture —
        // confirme visuellement que ça tourne (source de confusion avant :
        // le clip « Idle » par défaut était trop subtil pour qu'on le
        // remarque, cf. l'historique de ce fichier).
        let clip_progress = current_anim.as_ref().and_then(|(clip, time)| {
            let duration = self
                .state
                .scene
                .imported
                .last()?
                .clips
                .iter()
                .find(|c| &c.name == clip)?
                .duration;
            (duration > 0.0).then(|| (time.rem_euclid(duration)) / duration)
        });
        let mut clip_to_set: Option<String> = None;
        let mut model_to_load: Option<usize> = None;
        let mut recenter = false;
        let mut orbit_delta = egui::Vec2::ZERO;
        let mut pan_delta = egui::Vec2::ZERO;
        let mut zoom_delta = 0.0f32;
        let mut new_viewport = self.viewport_size;
        // Le texte handle doit être (re)créé/mis à jour **à l'intérieur** du
        // callback de `ctx.run` (comme `editor::hud::HudImageCache`) — appeler
        // `load_texture` avant le premier `run()` grappille l'identifiant
        // `Managed(0)` normalement réservé à l'atlas de polices, encore
        // inexistant à ce stade (« Missing texture » à chaque frame sinon).
        let mut scene_tex = self.scene_tex.take();
        let mut thumbnails = std::mem::take(&mut self.thumbnails);
        let mut thumb_state = self.thumb_state.take();
        // Au plus N rendus 3D de repli par frame (cf. `render_fallback_thumbnail`) :
        // dérouler toute une catégorie sans aperçu (ex. « Faune ») générerait sinon
        // des dizaines de rendus GPU synchrones d'un coup, perceptibles comme un
        // à-coup. Au-delà du budget, l'entrée reste simplement absente du cache
        // cette frame — retentée (et donc affichée) une frame plus tard.
        let mut thumb_render_budget = 2;

        let full_output = egui.ctx.clone().run_ui(raw_input, |root_ui| {
            let ctx = root_ui.ctx().clone();
            if let Some(pixels) = &pixels {
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([vw as usize, vh as usize], pixels);
                match &mut scene_tex {
                    Some(tex) => tex.set(image, tex_options),
                    None => scene_tex = Some(ctx.load_texture("scene", image, tex_options)),
                }
            }
            let tex_id = scene_tex.as_ref().map(|t| t.id());
            egui::Panel::left("models")
                .resizable(true)
                .default_size(260.0)
                .show_inside(root_ui, |ui| {
                    ui.heading("GLB Viewer");
                    ui.label(format!("{} modèles ({})", models.len(), MODELS_DIR));
                    ui.add(
                        egui::TextEdit::singleline(&mut filter)
                            .hint_text("Rechercher…")
                            .desired_width(f32::INFINITY),
                    );
                    // Infos/contrôles du modèle courant AVANT la liste (pas
                    // après) : la liste peut compter des dizaines d'entrées
                    // une fois une catégorie dépliée et grandit avec elle —
                    // placés après, sommets/triangles/Recentrer/Lecture
                    // finissaient poussés hors de la zone visible du panneau,
                    // inaccessibles sans réduire la liste. Épinglés ici, ils
                    // restent toujours visibles quelle que soit la longueur
                    // de la liste, qui prend le reste de la hauteur et défile
                    // elle-même.
                    if let Some(s) = stats {
                        ui.separator();
                        ui.label(format!("Sommets : {}", s.vertices));
                        ui.label(format!("Triangles : {}", s.triangles));
                        ui.label(format!(
                            "Taille : {:.2} × {:.2} × {:.2} m",
                            s.size.x, s.size.y, s.size.z
                        ));
                        if ui.button("🎯 Recentrer").clicked() {
                            recenter = true;
                        }
                    }
                    if !available_clips.is_empty() {
                        ui.separator();
                        ui.label("Animation");
                        // Libellés en texte simple plutôt qu'en symboles ▶/⏸ :
                        // ces glyphes n'existent pas dans le sous-ensemble
                        // d'icônes intégré à la police par défaut d'egui, ce
                        // qui rendait le bouton vide/mal dimensionné (glyphe
                        // manquant ⇒ largeur nulle) et donc peu fiable à
                        // cliquer.
                        let label = if playing { "Pause" } else { "Lecture" };
                        if ui
                            .add_sized([ui.available_width(), 24.0], egui::Button::new(label))
                            .clicked()
                        {
                            playing = !playing;
                        }
                        egui::ComboBox::from_id_salt("clip")
                            .width(ui.available_width())
                            .selected_text(if current_clip.is_empty() {
                                "—".to_string()
                            } else {
                                current_clip.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in available_clips {
                                    if ui.selectable_label(*name == current_clip, name).clicked() {
                                        clip_to_set = Some(name.clone());
                                    }
                                }
                            });
                        // Barre de progression dans le clip : retour visuel
                        // continu que la lecture tourne réellement, même sur
                        // un clip à mouvement subtil (ex. Idle).
                        if let Some(progress) = clip_progress {
                            ui.add(egui::ProgressBar::new(progress).desired_height(4.0));
                        }
                        ui.label(
                            egui::RichText::new("Espace = lecture/pause · ←/→ = modèle suivant")
                                .weak()
                                .small(),
                        );
                    }
                    if let Some(err) = &error {
                        ui.separator();
                        ui.colored_label(egui::Color32::LIGHT_RED, err);
                    }
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let needle = filter.to_lowercase();
                        // Capturé une fois, réutilisé pour une ligne de la
                        // liste comme pour une ligne de sous-catégorie — même
                        // rendu (vignette + libellé cliquable) aux deux niveaux
                        // d'imbrication.
                        let mut draw_item = |ui: &mut egui::Ui, i: usize| {
                            let name = models[i]
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("?");
                            // Chargée à la demande, une seule fois par entrée
                            // (cf. la doc de `thumbnails`) : le premier
                            // affichage d'une catégorie décode ses aperçus, les
                            // suivants réutilisent le handle en cache. Repli sur
                            // un rendu 3D (cf. `render_fallback_thumbnail`) pour
                            // les ~45 % de modèles sans `_preview.png`, limité
                            // par `thumb_render_budget` — au-delà, l'entrée
                            // reste absente du cache et sera retentée plus tard,
                            // pas de placeholder figé en cache.
                            let thumb = match thumbnails.get(&i) {
                                Some(cached) => cached.as_ref().map(egui::TextureHandle::id),
                                None => {
                                    // `Some(_)` = tenté cette frame (à mettre en
                                    // cache, y compris un échec définitif) ;
                                    // `None` = pas de budget, on retente plus
                                    // tard sans rien mettre en cache.
                                    let attempted = match load_thumbnail(&models[i]) {
                                        Some(img) => Some(Some(img)),
                                        None if thumb_render_budget > 0 => {
                                            thumb_render_budget -= 1;
                                            Some(render_fallback_thumbnail(
                                                thumb_renderer,
                                                &mut thumb_state,
                                                &models[i],
                                            ))
                                        }
                                        None => None,
                                    };
                                    attempted.and_then(|img| {
                                        let tex = img.map(|img| {
                                            ctx.load_texture(
                                                format!("thumb{i}"),
                                                img,
                                                egui::TextureOptions::LINEAR,
                                            )
                                        });
                                        let id = tex.as_ref().map(egui::TextureHandle::id);
                                        thumbnails.insert(i, tex);
                                        id
                                    })
                                }
                            };
                            ui.horizontal(|ui| {
                                ui.set_min_height(THUMB_SIZE as f32);
                                match thumb {
                                    Some(id) => {
                                        ui.add(egui::Image::new((
                                            id,
                                            egui::vec2(THUMB_SIZE as f32, THUMB_SIZE as f32),
                                        )));
                                    }
                                    None => {
                                        ui.add_space(THUMB_SIZE as f32);
                                    }
                                }
                                if ui.selectable_label(current == Some(i), name).clicked() {
                                    model_to_load = Some(i);
                                }
                            });
                        };

                        for category in categories {
                            let matching_subgroups: Vec<(&String, Vec<usize>)> = category
                                .subgroups
                                .iter()
                                .map(|(name, idxs)| {
                                    let filtered: Vec<usize> = idxs
                                        .iter()
                                        .copied()
                                        .filter(|&i| matches_filter(models, &needle, i))
                                        .collect();
                                    (name, filtered)
                                })
                                .filter(|(_, v)| !v.is_empty())
                                .collect();
                            let matching_flat: Vec<usize> = category
                                .flat
                                .iter()
                                .copied()
                                .filter(|&i| matches_filter(models, &needle, i))
                                .collect();
                            let total: usize = matching_subgroups
                                .iter()
                                .map(|(_, v)| v.len())
                                .sum::<usize>()
                                + matching_flat.len();
                            if total == 0 {
                                continue;
                            }
                            egui::CollapsingHeader::new(format!("{} ({total})", category.label))
                                .default_open(!needle.is_empty() || categories.len() == 1)
                                .show(ui, |ui| {
                                    for (sub_name, idxs) in &matching_subgroups {
                                        egui::CollapsingHeader::new(format!(
                                            "{sub_name} ({})",
                                            idxs.len()
                                        ))
                                        .default_open(!needle.is_empty())
                                        .show(ui, |ui| {
                                            for &i in idxs {
                                                draw_item(ui, i);
                                            }
                                        });
                                    }
                                    for &i in &matching_flat {
                                        draw_item(ui, i);
                                    }
                                });
                        }
                    });
                });

            egui::CentralPanel::default().show_inside(root_ui, |ui| {
                let avail = ui.available_size();
                let ppp = ctx.pixels_per_point();
                new_viewport = (
                    (avail.x * ppp).max(1.0) as u32,
                    (avail.y * ppp).max(1.0) as u32,
                );
                if let Some(id) = tex_id {
                    let resp =
                        ui.add(egui::Image::new((id, avail)).sense(egui::Sense::click_and_drag()));
                    if resp.dragged() {
                        let d = resp.drag_delta();
                        let pan_mod = ctx.input(|i| {
                            i.modifiers.shift
                                || i.pointer.button_down(egui::PointerButton::Middle)
                                || i.pointer.button_down(egui::PointerButton::Secondary)
                        });
                        if pan_mod {
                            pan_delta = d;
                        } else {
                            orbit_delta = d;
                        }
                    }
                    if resp.hovered() {
                        zoom_delta = ctx.input(|i| i.smooth_scroll_delta.y);
                    }
                    ui.put(
                        egui::Rect::from_min_size(
                            ui.min_rect().left_top() + egui::vec2(10.0, 10.0),
                            egui::vec2(360.0, 18.0),
                        ),
                        egui::Label::new(
                            egui::RichText::new(
                                "glisser = orbite · molette = zoom · Maj+glisser = pan",
                            )
                            .weak()
                            .small(),
                        ),
                    );
                } else {
                    ui.centered_and_justified(|ui| ui.label("Sélectionnez un modèle à gauche"));
                }
            });
        });

        self.filter = filter;
        self.scene_tex = scene_tex;
        self.thumbnails = thumbnails;
        self.thumb_state = thumb_state;
        self.playing = playing;
        if let Some(clip) = clip_to_set {
            if let Some(obj) = self.state.scene.objects.first_mut() {
                // Coupe franche plutôt que `AnimationState::set_clip` : celle-ci
                // démarre un fondu enchaîné (`blend` part à 0.0, `prev_time` figé)
                // que seule la boucle de jeu complète (`AppState::sim_step`/
                // `advance_play`, jamais appelée ici — ce viewer n'avance que
                // `anim.time` à la main) fait progresser jusqu'à 1.0. Sans ça,
                // `blend` restait bloqué à 0 pour de bon : le rendu n'affichait
                // plus que l'ancien clip, figé sur la pose qu'il avait pile au
                // moment du changement — d'où l'impression de plantage/gel au
                // lieu d'un vrai changement d'animation.
                let anim = obj.animation.get_or_insert_with(AnimationState::default);
                anim.clip = clip;
                anim.time = 0.0;
                anim.prev_clip.clear();
                anim.blend = 1.0;
            }
            self.playing = true;
        }
        // Phase 2 : appliquer les actions collectées, ce qui demande un `&mut
        // self` complet — d'où l'emprunt frais de `self.egui` juste après,
        // plutôt que de prolonger le binding de la phase 1.
        if let Some(i) = model_to_load {
            self.load_model(i);
        }
        if recenter {
            self.recenter();
        }
        if orbit_delta != egui::Vec2::ZERO {
            // Une manipulation manuelle reprend la main immédiatement sur une
            // transition douce en cours (changement de modèle juste avant) —
            // sinon la caméra continuerait de dériver vers l'ancien but sous
            // le geste de l'utilisateur, contre-intuitif.
            self.camera_goal = None;
            self.state.camera.orbit(orbit_delta.x, orbit_delta.y);
            self.dirty = true;
        }
        if pan_delta != egui::Vec2::ZERO {
            self.camera_goal = None;
            self.state.camera.pan(pan_delta.x, pan_delta.y);
            self.dirty = true;
        }
        if zoom_delta != 0.0 {
            self.camera_goal = None;
            self.state.camera.zoom_drag(-zoom_delta * 0.5);
            self.dirty = true;
        }
        let new_viewport = (new_viewport.0.max(1), new_viewport.1.max(1));
        if new_viewport != self.viewport_size {
            self.viewport_size = new_viewport;
            self.dirty = true;
        }

        // Phase 3 : peindre `full_output` sur la surface acquise plus haut.
        let gpu = self.gpu.as_mut().unwrap();
        let egui = self.egui.as_mut().unwrap();

        egui.winit_state
            .handle_platform_output(&window, full_output.platform_output.clone());

        let ppp = full_output.pixels_per_point;
        for (id, delta) in &full_output.textures_delta.set {
            egui.renderer
                .update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
        let primitives = egui.ctx.tessellate(full_output.shapes, ppp);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gpu.config.width, gpu.config.height],
            pixels_per_point: ppp,
        };
        let cmds = egui.renderer.update_buffers(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &primitives,
            &screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("glbviewer_egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.05,
                                g: 0.05,
                                b: 0.06,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            egui.renderer.render(&mut pass, &primitives, &screen);
        }
        for id in &full_output.textures_delta.free {
            egui.renderer.free_texture(id);
        }
        gpu.queue
            .submit(cmds.into_iter().chain(std::iter::once(encoder.finish())));
        frame.present();
    }
}

impl ApplicationHandler for Viewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("GLB Viewer")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Création de la fenêtre impossible : {e}");
                event_loop.exit();
                return;
            }
        };

        let gpu = pollster::block_on(WindowGpu::new(window.clone()));
        let egui = Egui::new(&gpu.device, gpu.config.format, &window);
        let scene_renderer = pollster::block_on(Renderer::new_headless(1280, 800))
            .expect("initialisation du renderer headless impossible");
        // Renderer distinct pour les vignettes de repli (cf. sa doc sur le
        // champ `thumb_renderer`) — taille fixe, pas besoin de suivre le
        // redimensionnement de la fenêtre comme `scene_renderer`.
        let thumb_renderer = pollster::block_on(Renderer::new_headless(THUMB_SIZE, THUMB_SIZE))
            .expect("initialisation du renderer de vignettes impossible");

        self.viewport_size = (1024, 720);
        self.models = discover_models();
        self.categories = group_by_category(&self.models);
        self.state = AppState::new();
        self.state.scene = Scene::default();
        self.gpu = Some(gpu);
        self.egui = Some(egui);
        self.scene_renderer = Some(scene_renderer);
        self.thumb_renderer = Some(thumb_renderer);
        self.window = Some(window);
        self.dirty = true;

        if !self.models.is_empty() {
            self.load_model(0);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let consumed = self
            .egui
            .as_mut()
            .map(|e| e.winit_state.on_window_event(&window, &event).consumed)
            .unwrap_or(false);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size);
                }
                self.dirty = true;
            }
            WindowEvent::RedrawRequested => self.render_frame(),
            // Raccourcis clavier — ignorés si egui a consommé l'événement
            // (curseur dans le champ de recherche : Espace/←/→ doivent taper
            // du texte, pas piloter la lecture/la sélection).
            WindowEvent::KeyboardInput { event: key, .. }
                if !consumed && key.state == winit::event::ElementState::Pressed =>
            {
                use winit::keyboard::{KeyCode, PhysicalKey};
                match key.physical_key {
                    PhysicalKey::Code(KeyCode::Space) => {
                        self.playing = !self.playing;
                        self.dirty = true;
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => self.load_relative(1),
                    PhysicalKey::Code(KeyCode::ArrowLeft) => self.load_relative(-1),
                    PhysicalKey::Code(KeyCode::KeyF) => self.recenter(),
                    _ => {}
                }
            }
            _ if consumed => {}
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            log::error!("Création de la boucle d'événements impossible : {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut viewer = Viewer::default();
    if let Err(e) = event_loop.run_app(&mut viewer) {
        log::error!("Boucle d'événements terminée sur erreur : {e}");
    }
}
