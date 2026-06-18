//! Motor3DeRust — moteur 3D minimaliste (winit + wgpu).
//! `main` ne fait que : créer la fenêtre, traduire les événements winit en
//! `InputEvent` agnostiques, et piloter `AppState` (logique) + `Renderer` (GPU).

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

mod app;
mod editor;
mod gfx;
mod scene;

use app::input::InputEvent;
use app::{AppState, GizmoMode};
use gfx::renderer::Renderer;

#[derive(Default)]
struct App {
    renderer: Option<Renderer>,
    state: AppState,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Motor3DeRust")
            .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let renderer = pollster::block_on(Renderer::new(window));
        self.state.set_viewport(renderer.size.width, renderer.size.height);
        self.renderer = Some(renderer);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        // egui voit l'événement en premier ; s'il le consomme, on n'agit pas sur la scène.
        let consumed = renderer.on_ui_event(&event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                renderer.resize(size);
                self.state.set_viewport(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                renderer.render(&mut self.state);
                renderer.window.request_redraw();
            }
            _ if consumed => {}
            WindowEvent::MouseInput { state: btn_state, button: MouseButton::Left, .. } => {
                let ev = if btn_state == ElementState::Pressed {
                    InputEvent::PointerDown
                } else {
                    InputEvent::PointerUp
                };
                self.state.handle_input(ev);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.state
                    .handle_input(InputEvent::PointerMove { x: position.x, y: position.y });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
                };
                self.state.handle_input(InputEvent::Scroll { delta: d });
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                use winit::keyboard::{KeyCode, PhysicalKey};
                if key_event.state == ElementState::Pressed {
                    if let PhysicalKey::Code(code) = key_event.physical_key {
                        match code {
                            KeyCode::KeyW => self.state.set_gizmo_mode(GizmoMode::Translate),
                            KeyCode::KeyE => self.state.set_gizmo_mode(GizmoMode::Rotate),
                            KeyCode::KeyR => self.state.set_gizmo_mode(GizmoMode::Scale),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
