//! UI de l'éditeur basée sur egui : toolbar, hiérarchie, inspecteur.
//! Encapsule toute la plomberie egui-winit / egui-wgpu.

pub mod export;
pub mod readiness;

use egui::ViewportId;
use glam::{EulerRot, Quat};
use winit::window::Window;

use crate::app::GizmoMode;
use crate::runtime::physics::PhysicsKind;
use crate::scene::{MeshKind, Scene, Transform};

pub struct Editor {
    ctx: egui::Context,
    winit_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    export: export::ExportPanel,
    /// Texte de filtre de la hiérarchie (recherche par nom).
    hier_filter: String,
    /// Nom saisi pour créer un nouveau groupe.
    hier_new_group: String,
    /// Renommage inline en cours : (index objet, texte en édition).
    hier_rename: Option<(usize, String)>,
    /// État des fenêtres flottantes (Aide + Outils).
    panels: Panels,
}

/// Visibilité et état des fenêtres flottantes des menus « Aide » et « Outils ».
#[derive(Default)]
struct Panels {
    // Aide
    shortcuts: bool,
    diagnostic: bool,
    about: bool,
    // Outils
    console: bool,
    profiler: bool,
    /// Historique récent des FPS pour le graphe du profiler.
    fps_history: std::collections::VecDeque<f32>,
    readiness: bool,
    /// Résultats du dernier « APK Readiness Check » (vide tant qu'on n'a pas analysé).
    readiness_results: Vec<readiness::Check>,
}

/// Informations de diagnostic affichées dans le bandeau d'état (lecture seule).
pub struct StatusInfo<'a> {
    pub fps: f32,
    pub backend: &'a str,
}

/// Actions demandées par l'UI durant une frame, à traiter par l'appelant.
#[derive(Default)]
pub struct UiActions {
    pub save: bool,
    pub load: bool,
    /// « Enregistrer sous » : chemin JSON choisi.
    pub save_path: Option<String>,
    /// « Ouvrir » : chemin JSON choisi.
    pub load_path: Option<String>,
    pub import: Option<String>,
    pub add: Option<MeshKind>,
    pub delete: Option<usize>,
    pub duplicate: bool,
    pub undo: bool,
    pub redo: bool,
    /// « Nouveau projet » : vide la scène.
    pub new_scene: bool,
    /// « Démo mobile » : charge une scène jouable (joystick + saut).
    pub load_demo: bool,
    /// « Aligner au sol » : pose la base de la sélection sur y = 0.
    pub align_ground: bool,
    /// « Réinitialiser transform » : remet rotation/échelle par défaut.
    pub reset_transform: bool,
    /// « Quitter » : ferme l'application.
    pub quit: bool,
    pub play_audio: Option<String>,
    /// Réordonnancement de l'objet sélectionné : `Some(true)` = descendre, `Some(false)` = monter.
    pub move_in_list: Option<bool>,
}

impl Editor {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat, window: &Window) -> Self {
        let ctx = egui::Context::default();
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
            color_format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: true,
                predictable_texture_filtering: false,
            },
        );
        Editor {
            ctx,
            winit_state,
            renderer,
            export: export::ExportPanel::new(),
            hier_filter: String::new(),
            hier_new_group: String::new(),
            hier_rename: None,
            panels: Panels::default(),
        }
    }

    /// Mode Player : dessine **uniquement** les contrôles tactiles en surimpression
    /// (pas de panneaux d'éditeur) et met à jour l'état d'entrée lu par les scripts.
    pub fn run_player_overlay(
        &mut self,
        window: &Window,
        scene: &Scene,
        input_state: &mut crate::app::PlayerInput,
        device_preview: bool,
        device_portrait: bool,
    ) -> egui::FullOutput {
        let raw_input = self.winit_state.take_egui_input(window);
        let mobile = &scene.mobile;
        let output = self.ctx.run_ui(raw_input, |ui| {
            let ctx = ui.ctx();
            let area = play_area_rect(ctx.content_rect(), device_preview, device_portrait);
            if device_preview {
                device_bezel(ctx, area);
            }
            if mobile.any() {
                mobile_overlay(ctx, area, mobile, input_state);
            } else {
                input_state.joy = (0.0, 0.0);
                input_state.buttons.clear();
            }
        });
        self.winit_state
            .handle_platform_output(window, output.platform_output.clone());
        output
    }

    /// Transmet l'événement à egui. Retourne `true` si egui l'a consommé.
    pub fn on_window_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.winit_state.on_window_event(window, event).consumed
    }

    /// Construit l'UI (mutant la scène et la sélection) et renvoie la sortie egui à peindre.
    #[allow(clippy::too_many_arguments)] // états distincts à muter
    pub fn run(
        &mut self,
        window: &Window,
        scene: &mut Scene,
        selection: &mut Option<usize>,
        selected: &mut Vec<usize>,
        playing: &mut bool,
        paused: &mut bool,
        gizmo_mode: &mut GizmoMode,
        input_state: &mut crate::app::PlayerInput,
        device_preview: &mut bool,
        device_portrait: &mut bool,
        view_rect: &mut (f32, f32, f32, f32),
        status: StatusInfo,
    ) -> (egui::FullOutput, UiActions) {
        let raw_input = self.winit_state.take_egui_input(window);
        let mut actions = UiActions::default();

        let export = &mut self.export;
        let hier_filter = &mut self.hier_filter;
        let hier_new_group = &mut self.hier_new_group;
        let hier_rename = &mut self.hier_rename;
        let panels = &mut self.panels;
        let output = self.ctx.run_ui(raw_input, |ui| {
            build_ui(
                ui,
                scene,
                selection,
                selected,
                playing,
                paused,
                gizmo_mode,
                input_state,
                device_preview,
                device_portrait,
                view_rect,
                &status,
                export,
                hier_filter,
                hier_new_group,
                hier_rename,
                panels,
                &mut actions,
            );
        });

        self.winit_state
            .handle_platform_output(window, output.platform_output.clone());
        (output, actions)
    }

    /// Peint l'UI egui dans `view`. Renvoie d'éventuels command buffers à soumettre avant l'encodeur.
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size_in_pixels: [u32; 2],
        output: egui::FullOutput,
    ) -> Vec<wgpu::CommandBuffer> {
        let ppp = output.pixels_per_point;
        for (id, delta) in &output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }
        let primitives = self.ctx.tessellate(output.shapes, ppp);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels,
            pixels_per_point: ppp,
        };
        let cmds = self
            .renderer
            .update_buffers(device, queue, encoder, &primitives, &screen);

        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &primitives, &screen);
        }

        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        cmds
    }
}

/// Catégorie (libellé + icône) d'un type de mesh, pour le regroupement de la hiérarchie.
fn mesh_category(mesh: MeshKind) -> (&'static str, &'static str) {
    match mesh {
        MeshKind::Cube => ("Cubes", "🧊"),
        MeshKind::Sphere => ("Sphères", "⚪"),
        MeshKind::Plane => ("Plans", "▦"),
        MeshKind::Cylinder => ("Cylindres", "🛢"),
        MeshKind::Capsule => ("Capsules", "💊"),
        MeshKind::Imported(_) => ("Modèles", "📦"),
    }
}

/// Badges compacts d'un objet : physique / script / audio.
fn object_badges(obj: &crate::scene::SceneObject) -> String {
    let mut b = String::new();
    match obj.physics {
        PhysicsKind::Static => b.push_str(" 🧱"),
        PhysicsKind::Dynamic => b.push_str(" ⚙"),
        PhysicsKind::None => {}
    }
    if !obj.script.trim().is_empty() {
        b.push_str(" 📜");
    }
    if !obj.audio_clip.is_empty() {
        b.push_str(" 🔊");
    }
    b
}

/// Hiérarchie ergonomique : recherche, **groupes définis par l'utilisateur** avec
/// glisser-déposer des objets, icônes et badges (physique/script/audio).
#[allow(clippy::too_many_arguments)] // panneau d'UI : états distincts à muter
fn hierarchy_panel(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    selection: &mut Option<usize>,
    selected: &mut Vec<usize>,
    filter: &mut String,
    new_group: &mut String,
    rename: &mut Option<(usize, String)>,
) {
    ui.horizontal(|ui| {
        ui.heading("Hiérarchie");
        ui.weak(format!("({})", scene.objects.len()));
    });
    ui.add(
        egui::TextEdit::singleline(filter)
            .hint_text("🔎 filtrer…")
            .desired_width(f32::INFINITY),
    );
    // Création d'un groupe.
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(new_group)
                .hint_text("nouveau groupe")
                .desired_width(130.0),
        );
        if ui.button("➕ Groupe").clicked() {
            let n = new_group.trim().to_string();
            if !n.is_empty() && !scene.groups.contains(&n) {
                scene.groups.push(n);
            }
            new_group.clear();
        }
    });
    ui.separator();

    let needle = filter.trim().to_lowercase();
    // Sections : groupes utilisateur (ordre conservé) puis « Sans groupe ».
    let mut sections: Vec<Option<String>> = scene.groups.iter().cloned().map(Some).collect();
    sections.push(None);

    // Mutations différées (appliquées après l'UI pour éviter les conflits d'emprunt).
    let mut moves: Vec<(usize, String)> = Vec::new();
    let mut delete_group: Option<String> = None;
    let mut commit_rename: Vec<(usize, String)> = Vec::new();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for sec in &sections {
                let gname = sec.clone().unwrap_or_default();
                let title = match sec {
                    Some(g) => format!("📁 {g}"),
                    None => "🗂 Sans groupe".to_string(),
                };
                let items: Vec<(usize, &crate::scene::SceneObject)> = scene
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| {
                        o.group == gname
                            && (needle.is_empty() || o.name.to_lowercase().contains(&needle))
                    })
                    .collect();

                // La zone de dépôt couvre tout le groupe : déposer un objet l'y assigne.
                let (_, payload) = ui.dnd_drop_zone::<usize, ()>(egui::Frame::default(), |ui| {
                    ui.horizontal(|ui| {
                        egui::CollapsingHeader::new(format!("{title}  ({})", items.len()))
                            .default_open(true)
                            .id_salt(("grp", &gname))
                            .show(ui, |ui| {
                                for (i, obj) in &items {
                                    let i = *i;
                                    // Mode renommage inline pour cette ligne.
                                    if let Some((ri, buf)) =
                                        rename.as_mut().filter(|(ri, _)| *ri == i)
                                    {
                                        let r = ui.add(
                                            egui::TextEdit::singleline(buf)
                                                .desired_width(f32::INFINITY),
                                        );
                                        r.request_focus();
                                        // Valide à la perte de focus (Entrée ou clic ailleurs).
                                        if r.lost_focus() {
                                            commit_rename.push((*ri, buf.clone()));
                                        }
                                        continue;
                                    }
                                    let is_sel = selected.contains(&i);
                                    let label = format!(
                                        "{} {}{}",
                                        mesh_category(obj.mesh).1,
                                        obj.name,
                                        object_badges(obj)
                                    );
                                    let resp = ui
                                        .dnd_drag_source(egui::Id::new(("obj", i)), i, |ui| {
                                            let _ = ui.selectable_label(is_sel, label);
                                        })
                                        .response;
                                    if resp.clicked() {
                                        let m = ui.input(|inp| inp.modifiers);
                                        if m.command || m.shift {
                                            // toggle dans l'ensemble
                                            if let Some(p) = selected.iter().position(|&x| x == i) {
                                                selected.remove(p);
                                                *selection = selected.last().copied();
                                            } else {
                                                selected.push(i);
                                                *selection = Some(i);
                                            }
                                        } else {
                                            *selection = Some(i);
                                            *selected = vec![i];
                                        }
                                    }
                                    if resp.double_clicked() {
                                        *rename = Some((i, obj.name.clone()));
                                    }
                                }
                                if items.is_empty() {
                                    ui.weak("  (déposer ici)");
                                }
                            });
                        // Bouton de suppression pour les groupes utilisateur.
                        if sec.is_some() && ui.small_button("🗑").clicked() {
                            delete_group = Some(gname.clone());
                        }
                    });
                });
                if let Some(idx) = payload {
                    moves.push((*idx, gname.clone()));
                }
            }
            if scene.objects.is_empty() {
                ui.weak("(scène vide)");
            }
        });

    // Application des mutations.
    for (idx, g) in moves {
        if let Some(o) = scene.objects.get_mut(idx) {
            o.group = g;
        }
    }
    if let Some(g) = delete_group {
        scene.groups.retain(|x| x != &g);
        for o in &mut scene.objects {
            if o.group == g {
                o.group.clear();
            }
        }
    }
    // Renommage validé.
    if let Some((idx, name)) = commit_rename.into_iter().next() {
        if let Some(o) = scene.objects.get_mut(idx) {
            o.name = name;
        }
        *rename = None;
    }
}

/// Menu « Fichier » : sauvegarde, ouverture, import, export.
fn menu_fichier(ui: &mut egui::Ui, export: &mut export::ExportPanel, actions: &mut UiActions) {
    ui.menu_button("Fichier", |ui| {
        if ui.button("✨  Nouveau projet").clicked() {
            actions.new_scene = true;
            ui.close();
        }
        if ui
            .button("🎮  Démo mobile (jouable)")
            .on_hover_text("Charge une scène : joystick + bouton Saut + personnage scripté")
            .clicked()
        {
            actions.load_demo = true;
            ui.close();
        }
        ui.separator();
        if ui.button("💾  Enregistrer").clicked() {
            actions.save = true;
            ui.close();
        }
        if ui.button("💾  Enregistrer sous…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Scène JSON", &["json"])
                .set_file_name("scene.json")
                .save_file()
            {
                actions.save_path = Some(p.to_string_lossy().into_owned());
            }
            ui.close();
        }
        if ui.button("📂  Ouvrir…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Scène JSON", &["json"])
                .pick_file()
            {
                actions.load_path = Some(p.to_string_lossy().into_owned());
            }
            #[cfg(any(target_os = "ios", target_os = "android"))]
            {
                actions.load = true;
            }
            ui.close();
        }
        ui.separator();
        if ui.button("📥  Importer glTF…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("glTF", &["glb", "gltf"])
                .pick_file()
            {
                actions.import = Some(p.to_string_lossy().into_owned());
            }
            ui.close();
        }
        ui.separator();
        if ui.button("📦  Build & Export…").clicked() {
            export.open = true;
            ui.close();
        }
        ui.separator();
        if ui.button("🚪  Quitter").clicked() {
            actions.quit = true;
            ui.close();
        }
    });
}

/// Menu « Édition » : historique et opérations sur la sélection.
fn menu_edition(ui: &mut egui::Ui, selection: &Option<usize>, actions: &mut UiActions) {
    ui.menu_button("Édition", |ui| {
        if ui.button("↩  Annuler").clicked() {
            actions.undo = true;
            ui.close();
        }
        if ui.button("↪  Rétablir").clicked() {
            actions.redo = true;
            ui.close();
        }
        ui.separator();
        let has = selection.is_some();
        if ui
            .add_enabled(has, egui::Button::new("⧉  Dupliquer"))
            .clicked()
        {
            actions.duplicate = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("🗑  Supprimer"))
            .clicked()
        {
            actions.delete = *selection;
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has, egui::Button::new("⬇  Aligner au sol"))
            .on_hover_text("Pose la base de l'objet sur le plan (y = 0)")
            .clicked()
        {
            actions.align_ground = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("↺  Réinitialiser transform"))
            .on_hover_text("Rotation et échelle par défaut (position conservée)")
            .clicked()
        {
            actions.reset_transform = true;
            ui.close();
        }
    });
}

/// Menu « Ajouter » : primitives.
fn menu_ajouter(ui: &mut egui::Ui, scene: &mut Scene, actions: &mut UiActions) {
    ui.menu_button("Ajouter", |ui| {
        ui.menu_button("🧱  Objet 3D", |ui| {
            if ui.button("🧊  Cube").clicked() {
                actions.add = Some(MeshKind::Cube);
                ui.close();
            }
            if ui.button("⚪  Sphère").clicked() {
                actions.add = Some(MeshKind::Sphere);
                ui.close();
            }
            if ui.button("▦  Plan").clicked() {
                actions.add = Some(MeshKind::Plane);
                ui.close();
            }
            if ui.button("🛢  Cylindre").clicked() {
                actions.add = Some(MeshKind::Cylinder);
                ui.close();
            }
            if ui.button("💊  Capsule").clicked() {
                actions.add = Some(MeshKind::Capsule);
                ui.close();
            }
        });
        ui.separator();
        ui.menu_button("🕹  Contrôles mobiles", |ui| {
            let joy_on = scene.mobile.joystick;
            if ui.selectable_label(joy_on, "🕹  Joystick virtuel").clicked() {
                scene.mobile.joystick = !joy_on;
                ui.close();
            }
            if ui.button("🔘  Ajouter un bouton tactile").clicked() {
                let n = scene.mobile.buttons.len() + 1;
                scene.mobile.buttons.push(format!("B{n}"));
                ui.close();
            }
            if !scene.mobile.buttons.is_empty()
                && ui.button("✕  Retirer le dernier bouton").clicked()
            {
                scene.mobile.buttons.pop();
                ui.close();
            }
        });
        ui.separator();
        // Catégories prévues (à venir) : grisées pour montrer la feuille de route.
        ui.add_enabled(false, egui::Button::new("💡  Lumière  (à venir)"));
        ui.add_enabled(false, egui::Button::new("🎥  Caméra  (à venir)"));
    });
}

/// Menu « Outils » : mode de manipulation du gizmo + diagnostics.
fn menu_outils(
    ui: &mut egui::Ui,
    gizmo_mode: &mut GizmoMode,
    export: &mut export::ExportPanel,
    panels: &mut Panels,
) {
    ui.menu_button("Outils", |ui| {
        ui.selectable_value(gizmo_mode, GizmoMode::Translate, "↔  Déplacer (W)");
        ui.selectable_value(gizmo_mode, GizmoMode::Rotate, "↻  Tourner (E)");
        ui.selectable_value(gizmo_mode, GizmoMode::Scale, "⤢  Redimensionner (R)");
        ui.separator();
        if ui.button("🖥  Console").clicked() {
            panels.console = true;
            ui.close();
        }
        if ui.button("📊  Profiler FPS").clicked() {
            panels.profiler = true;
            ui.close();
        }
        ui.separator();
        if ui.button("🤖  Build Android…").clicked() {
            export.open = true;
            ui.close();
        }
        if ui.button("✔  Contrôle qualité APK").clicked() {
            panels.readiness = true;
            panels.readiness_results.clear(); // forcer une nouvelle analyse à l'ouverture
            ui.close();
        }
        if ui.button("🩺  Diagnostic système").clicked() {
            panels.diagnostic = true;
            ui.close();
        }
    });
}

/// Menu « Aide » : raccourcis, guide export, diagnostic, à propos.
fn menu_aide(ui: &mut egui::Ui, panels: &mut Panels) {
    ui.menu_button("Aide", |ui| {
        if ui.button("⌨  Raccourcis clavier").clicked() {
            panels.shortcuts = true;
            ui.close();
        }
        if ui.button("🩺  Diagnostic système").clicked() {
            panels.diagnostic = true;
            ui.close();
        }
        ui.separator();
        ui.hyperlink_to(
            "📖  Guide export APK",
            "https://github.com/lberthod/rusteegear",
        );
        ui.hyperlink_to("🐙  Dépôt GitHub", "https://github.com/lberthod/rusteegear");
        ui.separator();
        if ui.button("ℹ  À propos de RusteeGear").clicked() {
            panels.about = true;
            ui.close();
        }
    });
}

/// Fenêtres flottantes des menus « Aide » et « Outils ».
fn tool_windows(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &Scene,
    export: &export::ExportPanel,
    status: &StatusInfo,
) {
    // --- Console (logs en mémoire) ---
    egui::Window::new("🖥  Console")
        .open(&mut panels.console)
        .default_size([460.0, 280.0])
        .show(ctx, |ui| {
            if ui.button("🧹  Effacer").clicked() {
                crate::log_buffer::clear();
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in crate::log_buffer::snapshot() {
                        ui.monospace(line);
                    }
                });
        });

    // --- Profiler FPS (n'accumule l'historique que lorsque la fenêtre est ouverte) ---
    if !panels.profiler {
        panels.fps_history.clear();
    } else {
        panels.fps_history.push_back(status.fps);
        while panels.fps_history.len() > 120 {
            panels.fps_history.pop_front();
        }
    }
    let fps_hist = panels.fps_history.clone();
    egui::Window::new("📊  Profiler FPS")
        .open(&mut panels.profiler)
        .resizable(false)
        .show(ctx, |ui| {
            let avg = if fps_hist.is_empty() {
                0.0
            } else {
                fps_hist.iter().sum::<f32>() / fps_hist.len() as f32
            };
            let min = fps_hist.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = fps_hist.iter().cloned().fold(0.0_f32, f32::max);
            ui.label(format!("FPS actuel : {:.0}", status.fps));
            ui.label(format!(
                "min {:.0} · moy {:.0} · max {:.0}",
                if min.is_finite() { min } else { 0.0 },
                avg,
                max
            ));
            ui.label(format!("🧊 {} objets", scene.objects.len()));
            ui.separator();
            // Sparkline simple : barres verticales normalisées sur 60 FPS.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(240.0, 60.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
            let n = fps_hist.len().max(1);
            let bar_w = rect.width() / n as f32;
            for (i, &f) in fps_hist.iter().enumerate() {
                let h = (f / 60.0).clamp(0.0, 1.0) * rect.height();
                let x = rect.left() + i as f32 * bar_w;
                let color = if f >= 55.0 {
                    egui::Color32::from_rgb(80, 200, 120)
                } else if f >= 30.0 {
                    egui::Color32::from_rgb(220, 180, 60)
                } else {
                    egui::Color32::from_rgb(220, 90, 80)
                };
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x, rect.bottom() - h),
                        egui::pos2(x + bar_w.max(1.0), rect.bottom()),
                    ),
                    0.0,
                    color,
                );
            }
        });

    // --- Contrôle qualité APK (APK Readiness Check) ---
    let mut do_analyze = false;
    egui::Window::new("✔  Contrôle qualité APK")
        .open(&mut panels.readiness)
        .default_size([420.0, 380.0])
        .show(ctx, |ui| {
            if panels.readiness_results.is_empty() {
                panels.readiness_results = readiness::analyze(scene, export.config());
            }
            let (ok, warn, fail) = readiness::summary(&panels.readiness_results);
            ui.horizontal(|ui| {
                ui.label(format!("✅ {ok}"));
                ui.label(format!("⚠ {warn}"));
                ui.label(format!("❌ {fail}"));
                if ui.button("🔄  Ré-analyser").clicked() {
                    do_analyze = true;
                }
            });
            if fail == 0 {
                ui.colored_label(
                    egui::Color32::from_rgb(80, 200, 120),
                    "Prêt pour l'export Android 🎉",
                );
            } else {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 90, 80),
                    format!("{fail} blocage(s) à corriger avant l'export"),
                );
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for c in &panels.readiness_results {
                        ui.horizontal(|ui| {
                            ui.label(c.status.icon());
                            ui.label(&c.label);
                        });
                    }
                });
        });
    if do_analyze {
        panels.readiness_results = readiness::analyze(scene, export.config());
    }

    egui::Window::new("⌨  Raccourcis clavier")
        .open(&mut panels.shortcuts)
        .resizable(false)
        .show(ctx, |ui| {
            egui::Grid::new("shortcuts_grid")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    for (k, v) in [
                        ("W", "Déplacer (translation)"),
                        ("E", "Tourner (rotation)"),
                        ("R", "Redimensionner (échelle)"),
                        ("Cmd/Ctrl + Z", "Annuler"),
                        ("Cmd/Ctrl + Maj + Z", "Rétablir"),
                        ("Cmd/Ctrl + D", "Dupliquer la sélection"),
                        ("Suppr", "Supprimer la sélection"),
                    ] {
                        ui.strong(k);
                        ui.label(v);
                        ui.end_row();
                    }
                });
        });

    egui::Window::new("🩺  Diagnostic système")
        .open(&mut panels.diagnostic)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Environnement de build Android :");
            ui.separator();
            let check = |ui: &mut egui::Ui, label: &str, ok: bool| {
                ui.horizontal(|ui| {
                    ui.label(if ok { "✅" } else { "❌" });
                    ui.label(label);
                });
            };
            // Le binaire tourne forcément via la toolchain Rust.
            check(ui, "Rust / Cargo", true);
            let android_sdk = std::env::var("ANDROID_HOME")
                .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
                .is_ok();
            let android_ndk = std::env::var("ANDROID_NDK_HOME")
                .or_else(|_| std::env::var("NDK_HOME"))
                .is_ok();
            check(ui, "Android SDK (ANDROID_HOME)", android_sdk);
            check(ui, "Android NDK (ANDROID_NDK_HOME)", android_ndk);
            ui.separator();
            ui.label(format!(
                "🖥  Backend graphique : {}",
                if cfg!(target_os = "macos") {
                    "Metal"
                } else if cfg!(target_os = "windows") {
                    "DX12 / Vulkan"
                } else {
                    "Vulkan"
                }
            ));
        });

    egui::Window::new("ℹ  À propos de RusteeGear")
        .open(&mut panels.about)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("RusteeGear");
            ui.label("Éditeur 3D en Rust orienté export Android natif.");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.hyperlink_to(
                "github.com/lberthod/rusteegear",
                "https://github.com/lberthod/rusteegear",
            );
        });
}

/// Rectangle (points egui) de la zone de jeu : écran de téléphone centré dans la
/// région `central` si l'aperçu mobile est actif, sinon `central` en entier.
fn play_area_rect(central: egui::Rect, preview: bool, portrait: bool) -> egui::Rect {
    if !preview {
        return central;
    }
    let (x, y, w, h) = crate::app::device_rect(central.width(), central.height(), portrait);
    egui::Rect::from_min_size(central.min + egui::vec2(x, y), egui::vec2(w, h))
}

/// Dessine le cadre « téléphone » (biseau arrondi + encoche) autour de la zone de jeu.
fn device_bezel(ctx: &egui::Context, rect: egui::Rect) {
    use egui::{Color32, Stroke};
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("device_bezel"),
    ));
    // Contour épais arrondi, façon châssis de smartphone.
    painter.rect_stroke(
        rect.expand(6.0),
        22.0,
        Stroke::new(10.0, Color32::from_rgb(20, 20, 24)),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect,
        16.0,
        Stroke::new(1.5, Color32::from_white_alpha(40)),
        egui::StrokeKind::Inside,
    );
    // Encoche centrale en haut.
    let notch = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 9.0),
        egui::vec2(rect.width().min(160.0) * 0.45, 14.0),
    );
    painter.rect_filled(notch, 7.0, Color32::from_rgb(20, 20, 24));
}

/// Dessine les contrôles tactiles (joystick virtuel + boutons) à l'intérieur de
/// `area` et met à jour l'état d'entrée lu par les scripts Lua.
fn mobile_overlay(
    ctx: &egui::Context,
    area: egui::Rect,
    cfg: &crate::scene::MobileControls,
    input: &mut crate::app::PlayerInput,
) {
    use egui::{Color32, Sense, Stroke, Vec2};

    input.joy = (0.0, 0.0);
    input.buttons.clear();

    let margin = 32.0;

    // --- Joystick (bas-gauche de la zone de jeu) ---
    if cfg.joystick {
        let radius = 55.0;
        let pos = egui::pos2(area.left() + margin, area.bottom() - margin - radius * 2.0);
        egui::Area::new("mobile_joystick".into())
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.0), Sense::drag());
                let center = rect.center();
                let painter = ui.painter();
                painter.circle_filled(center, radius, Color32::from_black_alpha(110));
                painter.circle_stroke(
                    center,
                    radius,
                    Stroke::new(2.0, Color32::from_white_alpha(120)),
                );
                let mut knob = center;
                if let Some(p) = resp.interact_pointer_pos() {
                    let mut off = p - center;
                    if off.length() > radius {
                        off = off.normalized() * radius;
                    }
                    knob = center + off;
                    input.joy = (off.x / radius, -off.y / radius); // y inversé : haut = +1
                }
                painter.circle_filled(knob, 22.0, Color32::from_white_alpha(200));
            });
    }

    // --- Boutons (bas-droite de la zone de jeu) ---
    if !cfg.buttons.is_empty() {
        let btn = 64.0;
        let spacing = 8.0;
        let width = cfg.buttons.len() as f32 * (btn + spacing);
        let pos = egui::pos2(area.right() - margin - width, area.bottom() - margin - btn);
        egui::Area::new("mobile_buttons".into())
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for name in &cfg.buttons {
                        let resp =
                            ui.add_sized([btn, btn], egui::Button::new(name).corner_radius(32.0));
                        // Bouton « maintenu » : actif tant que le pointeur est enfoncé dessus.
                        if resp.is_pointer_button_down_on() {
                            input.buttons.insert(name.clone());
                        }
                    }
                });
            });
    }
}

#[allow(clippy::too_many_arguments)] // panneau d'UI : chaque paramètre est un état distinct à muter
fn build_ui(
    root: &mut egui::Ui,
    scene: &mut Scene,
    selection: &mut Option<usize>,
    selected: &mut Vec<usize>,
    playing: &mut bool,
    paused: &mut bool,
    gizmo_mode: &mut GizmoMode,
    input_state: &mut crate::app::PlayerInput,
    device_preview: &mut bool,
    device_portrait: &mut bool,
    view_rect: &mut (f32, f32, f32, f32),
    status: &StatusInfo,
    export: &mut export::ExportPanel,
    hier_filter: &mut String,
    hier_new_group: &mut String,
    hier_rename: &mut Option<(usize, String)>,
    panels: &mut Panels,
    actions: &mut UiActions,
) {
    // Fenêtre flottante « Build & Export » (Sprint 19).
    export.ui(root.ctx(), scene);
    // Fenêtres des menus « Aide » et « Outils » (raccourcis, diagnostic, console, profiler, qualité APK).
    tool_windows(root.ctx(), panels, scene, export, status);

    // Bandeau d'état (bas) : FPS, nombre d'objets, mode, backend GPU.
    egui::Panel::bottom("status_bar").show_inside(root, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("⏱ {:.0} FPS", status.fps));
            ui.separator();
            ui.label(format!("🧊 {} objets", scene.objects.len()));
            ui.separator();
            ui.label(match (*playing, *paused) {
                (true, true) => "⏸ Pause",
                (true, false) => "▶ Play",
                _ => "✎ Edit",
            });
            ui.separator();
            ui.label(format!("GPU : {}", status.backend));
        });
    });

    // --- Barre de menus (style application de bureau) ---
    egui::Panel::top("menubar").show_inside(root, |ui| {
        ui.horizontal(|ui| {
            menu_fichier(ui, export, actions);
            menu_edition(ui, selection, actions);
            menu_ajouter(ui, scene, actions);
            menu_outils(ui, gizmo_mode, export, panels);
            menu_aide(ui, panels);
        });
    });

    // --- Barre d'outils rapide ---
    egui::Panel::top("toolbar").show_inside(root, |ui| {
        ui.horizontal(|ui| {
            // Play / Pause / Stop distincts (style lecteur).
            if !*playing {
                if ui.button("▶ Play").clicked() {
                    *playing = true;
                    *paused = false;
                }
            } else {
                let pause_label = if *paused {
                    "▶ Reprendre"
                } else {
                    "⏸ Pause"
                };
                if ui.button(pause_label).clicked() {
                    *paused = !*paused;
                }
                if ui.button("⏹ Stop").clicked() {
                    *playing = false;
                    *paused = false;
                }
            }
            ui.separator();
            ui.selectable_value(gizmo_mode, GizmoMode::Translate, "↔ Déplacer");
            ui.selectable_value(gizmo_mode, GizmoMode::Rotate, "↻ Tourner");
            ui.selectable_value(gizmo_mode, GizmoMode::Scale, "⤢ Redim.");
            ui.separator();
            if ui.button("↩").on_hover_text("Annuler (Cmd+Z)").clicked() {
                actions.undo = true;
            }
            if ui
                .button("↪")
                .on_hover_text("Rétablir (Cmd+Maj+Z)")
                .clicked()
            {
                actions.redo = true;
            }
            ui.separator();
            if ui.button("💾").on_hover_text("Enregistrer").clicked() {
                actions.save = true;
            }
            ui.separator();
            let has_sel = selection.is_some();
            if ui
                .add_enabled(has_sel, egui::Button::new("⧉"))
                .on_hover_text("Dupliquer (Cmd+D)")
                .clicked()
            {
                actions.duplicate = true;
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("🗑"))
                .on_hover_text("Supprimer")
                .clicked()
            {
                actions.delete = *selection;
            }
            ui.separator();
            // Aperçu mobile : cadre téléphone + orientation.
            if ui
                .selectable_label(*device_preview, "📱 Aperçu mobile")
                .on_hover_text("Affiche la scène dans un écran de téléphone")
                .clicked()
            {
                *device_preview = !*device_preview;
            }
            if *device_preview {
                let label = if *device_portrait {
                    "⟳ Portrait"
                } else {
                    "⟳ Paysage"
                };
                if ui
                    .button(label)
                    .on_hover_text("Basculer l'orientation")
                    .clicked()
                {
                    *device_portrait = !*device_portrait;
                }
            }
            // Build APK : différenciateur du moteur, mis en avant à droite.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(export.open, "🤖 Build APK")
                    .on_hover_text("Build & Export (.dmg / .apk / .ipa)")
                    .clicked()
                {
                    export.open = !export.open;
                }
            });
        });
    });

    // Mode « focus jeu » : en aperçu mobile, on masque les panneaux latéraux pour
    // laisser toute la place au téléphone (la toolbar reste pour quitter l'aperçu).
    let show_panels = !*device_preview;

    if show_panels {
        egui::Panel::left("hierarchy")
            .default_size(200.0)
            .show_inside(root, |ui| {
                hierarchy_panel(
                    ui,
                    scene,
                    selection,
                    selected,
                    hier_filter,
                    hier_new_group,
                    hier_rename,
                );
            });
    }

    if show_panels {
        egui::Panel::right("inspector")
            .default_size(240.0)
            .show_inside(root, |ui| {
                ui.heading("Inspecteur");
                ui.separator();
                ui.collapsing("🔆 Éclairage (scène)", |ui| {
                    let l = &mut scene.light;
                    ui.label("Direction");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut l.dir[0]).speed(0.02).prefix("x "));
                        ui.add(egui::DragValue::new(&mut l.dir[1]).speed(0.02).prefix("y "));
                        ui.add(egui::DragValue::new(&mut l.dir[2]).speed(0.02).prefix("z "));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Couleur");
                        ui.color_edit_button_rgb(&mut l.color);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Ambiante");
                        ui.add(egui::Slider::new(&mut l.ambient, 0.0..=1.0));
                    });
                });
                ui.separator();
                match *selection {
                    Some(i) if i < scene.objects.len() => {
                        let obj = &mut scene.objects[i];
                        ui.horizontal(|ui| {
                            ui.label("Nom");
                            ui.text_edit_singleline(&mut obj.name);
                            if ui.small_button("▲").on_hover_text("Monter").clicked() {
                                actions.move_in_list = Some(false);
                            }
                            if ui.small_button("▼").on_hover_text("Descendre").clicked() {
                                actions.move_in_list = Some(true);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Couleur (teinte)");
                            ui.color_edit_button_rgb(&mut obj.color);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Texture");
                            if ui.button("Choisir…").clicked() {
                                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter("Image", &["png", "jpg", "jpeg"])
                                    .pick_file()
                                {
                                    obj.texture = p.to_string_lossy().into_owned();
                                }
                            }
                            if !obj.texture.is_empty() && ui.button("✕").clicked() {
                                obj.texture.clear();
                            }
                            let t = if obj.texture.is_empty() {
                                "(aucune)".to_string()
                            } else {
                                std::path::Path::new(&obj.texture)
                                    .file_name()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            };
                            ui.label(t);
                        });
                        ui.separator();
                        transform_editor(ui, &mut obj.transform);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Physique");
                            ui.selectable_value(&mut obj.physics, PhysicsKind::None, "Aucune");
                            ui.selectable_value(&mut obj.physics, PhysicsKind::Static, "Statique");
                            ui.selectable_value(
                                &mut obj.physics,
                                PhysicsKind::Dynamic,
                                "Dynamique",
                            );
                        });
                        ui.separator();
                        ui.collapsing("Audio", |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("Choisir un son…").clicked() {
                                    #[cfg(not(any(target_os = "ios", target_os = "android")))]
                                    if let Some(p) = rfd::FileDialog::new()
                                        .add_filter("Audio", &["wav", "ogg", "flac", "mp3"])
                                        .pick_file()
                                    {
                                        obj.audio_clip = p.to_string_lossy().into_owned();
                                    }
                                }
                                if !obj.audio_clip.is_empty() && ui.button("▶ Tester").clicked() {
                                    actions.play_audio = Some(obj.audio_clip.clone());
                                }
                            });
                            let label = if obj.audio_clip.is_empty() {
                                "(aucun)".to_string()
                            } else {
                                std::path::Path::new(&obj.audio_clip)
                                    .file_name()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            };
                            ui.label(label);
                            ui.checkbox(&mut obj.audio_autoplay, "Jouer au lancement (Play)");
                        });
                        ui.separator();
                        ui.collapsing("Script (Lua)", |ui| {
                            ui.label(
                                "Variables : obj.x/y/z, obj.rx/ry/rz (°), obj.sx/sy/sz, dt, time",
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut obj.script)
                                    .code_editor()
                                    .desired_rows(4)
                                    .hint_text("ex : obj.ry = obj.ry + dt * 90"),
                            );
                        });
                        ui.separator();
                        if ui.button("🗑 Supprimer").clicked() {
                            actions.delete = Some(i);
                        }
                    }
                    _ => {
                        ui.label("Aucun objet sélectionné.");
                    }
                }
            });
    }

    // Région centrale 3D (ce qui reste après les panneaux) : base de l'aperçu mobile.
    let central = root.available_rect_before_wrap();
    let ppp = root.ctx().pixels_per_point();
    *view_rect = (
        central.left() * ppp,
        central.top() * ppp,
        central.width() * ppp,
        central.height() * ppp,
    );

    // Cadre « téléphone » + contrôles tactiles, confinés à la zone de jeu.
    let play_rect = play_area_rect(central, *device_preview, *device_portrait);
    if *device_preview {
        device_bezel(root.ctx(), play_rect);
    }
    if *playing && scene.mobile.any() {
        mobile_overlay(root.ctx(), play_rect, &scene.mobile, input_state);
    } else {
        input_state.joy = (0.0, 0.0);
        input_state.buttons.clear();
    }

    // Les actions (add/delete/duplicate/undo/redo) sont appliquées par AppState
    // après cette frame, afin de passer par l'historique.
}

fn transform_editor(ui: &mut egui::Ui, t: &mut Transform) {
    ui.label("Position");
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut t.position.x)
                .speed(0.05)
                .prefix("x "),
        );
        ui.add(
            egui::DragValue::new(&mut t.position.y)
                .speed(0.05)
                .prefix("y "),
        );
        ui.add(
            egui::DragValue::new(&mut t.position.z)
                .speed(0.05)
                .prefix("z "),
        );
    });

    // rotation éditée en degrés via les angles d'Euler
    let (mut rx, mut ry, mut rz) = t.rotation.to_euler(EulerRot::XYZ);
    rx = rx.to_degrees();
    ry = ry.to_degrees();
    rz = rz.to_degrees();
    ui.label("Rotation (°)");
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui
            .add(egui::DragValue::new(&mut rx).speed(1.0).prefix("x "))
            .changed();
        changed |= ui
            .add(egui::DragValue::new(&mut ry).speed(1.0).prefix("y "))
            .changed();
        changed |= ui
            .add(egui::DragValue::new(&mut rz).speed(1.0).prefix("z "))
            .changed();
    });
    if changed {
        t.rotation = Quat::from_euler(
            EulerRot::XYZ,
            rx.to_radians(),
            ry.to_radians(),
            rz.to_radians(),
        );
    }

    ui.label("Échelle");
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut t.scale.x)
                .speed(0.05)
                .prefix("x "),
        );
        ui.add(
            egui::DragValue::new(&mut t.scale.y)
                .speed(0.05)
                .prefix("y "),
        );
        ui.add(
            egui::DragValue::new(&mut t.scale.z)
                .speed(0.05)
                .prefix("z "),
        );
    });
}
