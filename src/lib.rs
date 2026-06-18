//! Motor3DeRust — moteur 3D minimaliste (winit + wgpu).
//! Exposé en bibliothèque pour partager le point d'entrée entre desktop (bin)
//! et Android (cdylib `android_main`).

use std::collections::HashMap;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub mod app;
pub mod editor;
pub mod gfx;
pub mod runtime;
pub mod scene;

use app::input::InputEvent;
use app::{AppState, GizmoMode};
use gfx::renderer::Renderer;

#[derive(Default)]
struct App {
    renderer: Option<Renderer>,
    state: AppState,
    modifiers: winit::event::Modifiers,

    // --- état tactile ---
    touches: HashMap<u64, (f64, f64)>,
    orbiting: bool,
    pinch: Option<f32>,
}

impl App {
    /// Traduit les événements tactiles : 1 doigt = orbit, 2 doigts = pinch-zoom.
    fn handle_touch(&mut self, touch: Touch) {
        let (x, y) = (touch.location.x, touch.location.y);
        match touch.phase {
            TouchPhase::Started | TouchPhase::Moved => {
                self.touches.insert(touch.id, (x, y));
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.touches.remove(&touch.id);
            }
        }

        match self.touches.len() {
            1 => {
                let (px, py) = *self.touches.values().next().unwrap();
                if self.orbiting {
                    self.state.handle_input(InputEvent::PointerMove { x: px, y: py });
                } else {
                    self.state.handle_input(InputEvent::PointerMove { x: px, y: py });
                    self.state.handle_input(InputEvent::PointerDown);
                    self.orbiting = true;
                }
                self.pinch = None;
            }
            2 => {
                if self.orbiting {
                    self.state.handle_input(InputEvent::PointerUp);
                    self.orbiting = false;
                }
                let mut it = self.touches.values();
                let a = *it.next().unwrap();
                let b = *it.next().unwrap();
                let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() as f32;
                if let Some(prev) = self.pinch {
                    self.state.handle_input(InputEvent::Scroll { delta: (d - prev) * 0.02 });
                }
                self.pinch = Some(d);
            }
            _ => {
                if self.orbiting {
                    self.state.handle_input(InputEvent::PointerUp);
                    self.orbiting = false;
                }
                self.pinch = None;
            }
        }
    }
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
            WindowEvent::Touch(touch) => self.handle_touch(touch),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m,
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                use winit::keyboard::{KeyCode, PhysicalKey};
                if key_event.state == ElementState::Pressed {
                    if let PhysicalKey::Code(code) = key_event.physical_key {
                        let st = self.modifiers.state();
                        let cmd = st.control_key() || st.super_key();
                        match code {
                            KeyCode::KeyW => self.state.set_gizmo_mode(GizmoMode::Translate),
                            KeyCode::KeyE => self.state.set_gizmo_mode(GizmoMode::Rotate),
                            KeyCode::KeyR if !cmd => self.state.set_gizmo_mode(GizmoMode::Scale),
                            KeyCode::KeyZ if cmd && st.shift_key() => self.state.redo(),
                            KeyCode::KeyZ if cmd => self.state.undo(),
                            KeyCode::KeyD if cmd => self.state.duplicate_selected(),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn make_app(player: bool) -> App {
    let mut app = App::default();
    if player {
        app.state.player = true;
        app.state.playing = true;
    }
    app
}

/// Point d'entrée desktop (et iOS via le bin).
pub fn run() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    // Mobile = mode Player (plein écran, sans éditeur) ; desktop via --player.
    let player = std::env::args().any(|a| a == "--player") || cfg!(target_os = "ios");
    let mut app = make_app(player);
    event_loop.run_app(&mut app).unwrap();
}

/// Point d'entrée Android (appelé par android-activity via la NativeActivity).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    let event_loop = EventLoop::builder()
        .with_android_app(android_app)
        .build()
        .unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = make_app(true); // mobile = mode player
    event_loop.run_app(&mut app).unwrap();
}
