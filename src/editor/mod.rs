//! UI de l'éditeur basée sur egui : toolbar, hiérarchie, inspecteur.
//! Encapsule toute la plomberie egui-winit / egui-wgpu.

pub mod export;

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
    pub import: Option<String>,
    pub add: Option<MeshKind>,
    pub delete: Option<usize>,
    pub duplicate: bool,
    pub undo: bool,
    pub redo: bool,
    pub play_audio: Option<String>,
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
        }
    }

    /// Transmet l'événement à egui. Retourne `true` si egui l'a consommé.
    pub fn on_window_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.winit_state.on_window_event(window, event).consumed
    }

    /// Construit l'UI (mutant la scène et la sélection) et renvoie la sortie egui à peindre.
    pub fn run(
        &mut self,
        window: &Window,
        scene: &mut Scene,
        selection: &mut Option<usize>,
        playing: &mut bool,
        gizmo_mode: &mut GizmoMode,
        status: StatusInfo,
    ) -> (egui::FullOutput, UiActions) {
        let raw_input = self.winit_state.take_egui_input(window);
        let mut actions = UiActions::default();

        let export = &mut self.export;
        let hier_filter = &mut self.hier_filter;
        let output = self.ctx.run_ui(raw_input, |ui| {
            build_ui(
                ui,
                scene,
                selection,
                playing,
                gizmo_mode,
                &status,
                export,
                hier_filter,
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

/// Hiérarchie ergonomique : recherche, regroupement par type (en-têtes repliables
/// avec compteur), icônes et badges (physique/script/audio).
fn hierarchy_panel(
    ui: &mut egui::Ui,
    scene: &Scene,
    selection: &mut Option<usize>,
    filter: &mut String,
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
    ui.separator();

    let needle = filter.trim().to_lowercase();
    let matches = |o: &crate::scene::SceneObject| {
        needle.is_empty() || o.name.to_lowercase().contains(&needle)
    };

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut shown = 0;
            // Une section repliable par catégorie de mesh (ordre stable).
            for (cat, icon) in [
                ("Cubes", "🧊"),
                ("Sphères", "⚪"),
                ("Plans", "▦"),
                ("Modèles", "📦"),
            ] {
                let items: Vec<(usize, &crate::scene::SceneObject)> = scene
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| mesh_category(o.mesh).0 == cat && matches(o))
                    .collect();
                if items.is_empty() {
                    continue;
                }
                shown += items.len();
                egui::CollapsingHeader::new(format!("{icon} {cat}  ({})", items.len()))
                    .default_open(true)
                    .id_salt(cat)
                    .show(ui, |ui| {
                        for (i, obj) in items {
                            let selected = *selection == Some(i);
                            let label = format!("{}{}", obj.name, object_badges(obj));
                            if ui.selectable_label(selected, label).clicked() {
                                *selection = Some(i);
                            }
                        }
                    });
            }
            if shown == 0 {
                ui.weak(if scene.objects.is_empty() {
                    "(scène vide)"
                } else {
                    "(aucun objet ne correspond)"
                });
            }
        });
}

#[allow(clippy::too_many_arguments)] // panneau d'UI : chaque paramètre est un état distinct à muter
fn build_ui(
    root: &mut egui::Ui,
    scene: &mut Scene,
    selection: &mut Option<usize>,
    playing: &mut bool,
    gizmo_mode: &mut GizmoMode,
    status: &StatusInfo,
    export: &mut export::ExportPanel,
    hier_filter: &mut String,
    actions: &mut UiActions,
) {
    // Fenêtre flottante « Build & Export » (Sprint 19).
    export.ui(root.ctx(), scene);

    // Bandeau d'état (bas) : FPS, nombre d'objets, mode, backend GPU.
    egui::Panel::bottom("status_bar").show_inside(root, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("⏱ {:.0} FPS", status.fps));
            ui.separator();
            ui.label(format!("🧊 {} objets", scene.objects.len()));
            ui.separator();
            ui.label(if *playing { "▶ Play" } else { "⏸ Edit" });
            ui.separator();
            ui.label(format!("GPU : {}", status.backend));
        });
    });

    egui::Panel::top("toolbar").show_inside(root, |ui| {
        ui.horizontal(|ui| {
            let play_label = if *playing { "⏹ Stop" } else { "▶ Play" };
            if ui.button(play_label).clicked() {
                *playing = !*playing;
            }
            ui.separator();
            ui.label("Gizmo :");
            ui.selectable_value(gizmo_mode, GizmoMode::Translate, "Déplacer (W)");
            ui.selectable_value(gizmo_mode, GizmoMode::Rotate, "Tourner (E)");
            ui.selectable_value(gizmo_mode, GizmoMode::Scale, "Redim. (R)");
            ui.separator();
            if ui.button("↩ Undo").clicked() {
                actions.undo = true;
            }
            if ui.button("↪ Redo").clicked() {
                actions.redo = true;
            }
            if ui
                .add_enabled(selection.is_some(), egui::Button::new("⧉ Dupliquer"))
                .clicked()
            {
                actions.duplicate = true;
            }
            ui.separator();
            ui.label("Ajouter :");
            if ui.button("Cube").clicked() {
                actions.add = Some(MeshKind::Cube);
            }
            if ui.button("Sphère").clicked() {
                actions.add = Some(MeshKind::Sphere);
            }
            if ui.button("Plan").clicked() {
                actions.add = Some(MeshKind::Plane);
            }
            ui.separator();
            if ui.button("💾 Save").clicked() {
                actions.save = true;
            }
            if ui.button("📂 Load").clicked() {
                actions.load = true;
            }
            if ui.button("📥 Importer glTF").clicked() {
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("glTF", &["glb", "gltf"])
                    .pick_file()
                {
                    actions.import = Some(p.to_string_lossy().into_owned());
                }
            }
            ui.separator();
            if ui
                .selectable_label(export.open, "📦 Export")
                .on_hover_text("Exporter .dmg / .apk / .ipa")
                .clicked()
            {
                export.open = !export.open;
            }
        });
    });

    egui::Panel::left("hierarchy")
        .default_size(200.0)
        .show_inside(root, |ui| {
            hierarchy_panel(ui, scene, selection, hier_filter);
        });

    egui::Panel::right("inspector")
        .default_size(240.0)
        .show_inside(root, |ui| {
            ui.heading("Inspecteur");
            ui.separator();
            match *selection {
                Some(i) if i < scene.objects.len() => {
                    let obj = &mut scene.objects[i];
                    ui.horizontal(|ui| {
                        ui.label("Nom");
                        ui.text_edit_singleline(&mut obj.name);
                    });
                    ui.separator();
                    transform_editor(ui, &mut obj.transform);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Physique");
                        ui.selectable_value(&mut obj.physics, PhysicsKind::None, "Aucune");
                        ui.selectable_value(&mut obj.physics, PhysicsKind::Static, "Statique");
                        ui.selectable_value(&mut obj.physics, PhysicsKind::Dynamic, "Dynamique");
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
                        ui.label("Variables : obj.x/y/z, obj.rx/ry/rz (°), obj.sx/sy/sz, dt, time");
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
