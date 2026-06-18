//! UI de l'éditeur basée sur egui : toolbar, hiérarchie, inspecteur.
//! Encapsule toute la plomberie egui-winit / egui-wgpu.

use egui::ViewportId;
use glam::{EulerRot, Quat, Vec3};
use winit::window::Window;

use crate::app::GizmoMode;
use crate::scene::{MeshKind, Scene, SceneObject, Transform};

pub struct Editor {
    ctx: egui::Context,
    winit_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

/// Actions demandées par l'UI durant une frame, à traiter par l'appelant.
#[derive(Default)]
pub struct UiActions {
    pub save: bool,
    pub load: bool,
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
        Editor { ctx, winit_state, renderer }
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
    ) -> (egui::FullOutput, UiActions) {
        let raw_input = self.winit_state.take_egui_input(window);
        let mut actions = UiActions::default();

        let output = self.ctx.run_ui(raw_input, |ui| {
            build_ui(ui, scene, selection, playing, gizmo_mode, &mut actions);
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

fn build_ui(
    root: &mut egui::Ui,
    scene: &mut Scene,
    selection: &mut Option<usize>,
    playing: &mut bool,
    gizmo_mode: &mut GizmoMode,
    actions: &mut UiActions,
) {
    let mut to_add: Option<MeshKind> = None;
    let mut to_delete: Option<usize> = None;

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
            ui.label("Ajouter :");
            if ui.button("Cube").clicked() {
                to_add = Some(MeshKind::Cube);
            }
            if ui.button("Sphère").clicked() {
                to_add = Some(MeshKind::Sphere);
            }
            if ui.button("Plan").clicked() {
                to_add = Some(MeshKind::Plane);
            }
            ui.separator();
            if ui.button("💾 Save").clicked() {
                actions.save = true;
            }
            if ui.button("📂 Load").clicked() {
                actions.load = true;
            }
        });
    });

    egui::Panel::left("hierarchy")
        .default_size(180.0)
        .show_inside(root, |ui| {
            ui.heading("Hiérarchie");
            ui.separator();
            for (i, obj) in scene.objects.iter().enumerate() {
                let selected = *selection == Some(i);
                if ui.selectable_label(selected, &obj.name).clicked() {
                    *selection = Some(i);
                }
            }
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
                    if ui.button("🗑 Supprimer").clicked() {
                        to_delete = Some(i);
                    }
                }
                _ => {
                    ui.label("Aucun objet sélectionné.");
                }
            }
        });

    // --- application des actions différées ---
    if let Some(kind) = to_add {
        let name = format!("{} {}", kind_label(kind), scene.objects.len());
        scene.objects.push(SceneObject {
            name,
            transform: Transform::from_pos(Vec3::new(0.0, 0.0, 0.0)),
            mesh: kind,
        });
        *selection = Some(scene.objects.len() - 1);
    }
    if let Some(i) = to_delete {
        if i < scene.objects.len() {
            scene.objects.remove(i);
            *selection = None;
        }
    }
}

fn transform_editor(ui: &mut egui::Ui, t: &mut Transform) {
    ui.label("Position");
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut t.position.x).speed(0.05).prefix("x "));
        ui.add(egui::DragValue::new(&mut t.position.y).speed(0.05).prefix("y "));
        ui.add(egui::DragValue::new(&mut t.position.z).speed(0.05).prefix("z "));
    });

    // rotation éditée en degrés via les angles d'Euler
    let (mut rx, mut ry, mut rz) = t.rotation.to_euler(EulerRot::XYZ);
    rx = rx.to_degrees();
    ry = ry.to_degrees();
    rz = rz.to_degrees();
    ui.label("Rotation (°)");
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.add(egui::DragValue::new(&mut rx).speed(1.0).prefix("x ")).changed();
        changed |= ui.add(egui::DragValue::new(&mut ry).speed(1.0).prefix("y ")).changed();
        changed |= ui.add(egui::DragValue::new(&mut rz).speed(1.0).prefix("z ")).changed();
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
        ui.add(egui::DragValue::new(&mut t.scale.x).speed(0.05).prefix("x "));
        ui.add(egui::DragValue::new(&mut t.scale.y).speed(0.05).prefix("y "));
        ui.add(egui::DragValue::new(&mut t.scale.z).speed(0.05).prefix("z "));
    });
}

fn kind_label(kind: MeshKind) -> &'static str {
    match kind {
        MeshKind::Cube => "Cube",
        MeshKind::Sphere => "Sphère",
        MeshKind::Plane => "Plan",
    }
}
