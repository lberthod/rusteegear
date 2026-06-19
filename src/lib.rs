//! RusteeGear — moteur 3D minimaliste (winit + wgpu).
//! Exposé en bibliothèque pour partager le point d'entrée entre desktop (bin)
//! et Android (cdylib `android_main`).

use std::collections::HashMap;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub mod app;
pub mod assets;
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
                let Some(&(px, py)) = self.touches.values().next() else {
                    return;
                };
                if self.orbiting {
                    self.state
                        .handle_input(InputEvent::PointerMove { x: px, y: py });
                } else {
                    self.state
                        .handle_input(InputEvent::PointerMove { x: px, y: py });
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
                let (Some(&a), Some(&b)) = (it.next(), it.next()) else {
                    return;
                };
                let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() as f32;
                if let Some(prev) = self.pinch {
                    self.state.handle_input(InputEvent::Scroll {
                        delta: (d - prev) * 0.02,
                    });
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
            .with_title("RusteeGear")
            .with_window_icon(load_window_icon())
            .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 720.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Création de la fenêtre impossible : {e}");
                event_loop.exit();
                return;
            }
        };
        match pollster::block_on(Renderer::new(window)) {
            Ok(renderer) => {
                self.state
                    .set_viewport(renderer.size.width, renderer.size.height);
                self.renderer = Some(renderer);
            }
            Err(e) => {
                log::error!("Initialisation du renderer impossible : {e}");
                event_loop.exit();
            }
        }
    }

    /// Mobile : la surface GPU devient invalide quand l'app passe en arrière-plan.
    /// On lâche le renderer ; `resumed` le reconstruira (l'état applicatif est préservé).
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.renderer = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        // En mode player, egui ne doit pas intercepter le tactile.
        let consumed = if self.state.player {
            false
        } else {
            renderer.on_ui_event(&event)
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                renderer.resize(size);
                self.state.set_viewport(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                renderer.render(&mut self.state);
                // Le ré-armement du redraw est centralisé dans `about_to_wait`
                // (indispensable sur iOS) : pas de double demande ici.
            }
            _ if consumed => {}
            WindowEvent::MouseInput {
                state: btn_state,
                button: MouseButton::Left,
                ..
            } => {
                let ev = if btn_state == ElementState::Pressed {
                    // Cmd/Maj enfoncé au clic = sélection additive (multi-sélection 3D).
                    let st = self.modifiers.state();
                    self.state
                        .set_additive(st.control_key() || st.super_key() || st.shift_key());
                    InputEvent::PointerDown
                } else {
                    InputEvent::PointerUp
                };
                self.state.handle_input(ev);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.state.handle_input(InputEvent::PointerMove {
                    x: position.x,
                    y: position.y,
                });
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
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                use winit::keyboard::{KeyCode, PhysicalKey};
                if key_event.state == ElementState::Pressed
                    && let PhysicalKey::Code(code) = key_event.physical_key
                {
                    let st = self.modifiers.state();
                    let cmd = st.control_key() || st.super_key();
                    match code {
                        KeyCode::KeyW => self.state.set_gizmo_mode(GizmoMode::Translate),
                        KeyCode::KeyE => self.state.set_gizmo_mode(GizmoMode::Rotate),
                        KeyCode::KeyR if !cmd => self.state.set_gizmo_mode(GizmoMode::Scale),
                        KeyCode::KeyZ if cmd && st.shift_key() => self.state.redo(),
                        KeyCode::KeyZ if cmd => self.state.undo(),
                        KeyCode::KeyD if cmd => self.state.duplicate_selected(),
                        KeyCode::KeyC if cmd => self.state.copy_selected(),
                        KeyCode::KeyV if cmd => self.state.paste(),
                        KeyCode::Backspace | KeyCode::Delete => self.state.delete_selected(),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Ré-arme le rendu (indispensable sur iOS) et ajuste la cadence : plein régime
    /// (`Poll`) en Play ou pendant une interaction, throttle léger au repos pour
    /// économiser CPU/batterie sur desktop tout en restant réactif aux entrées
    /// et aux chargements asynchrones.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(renderer) = &self.renderer {
            renderer.window.request_redraw();
        }
        if self.state.is_active() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::wait_duration(
                std::time::Duration::from_millis(60),
            ));
        }
    }
}

/// Icône de fenêtre/dock, embarquée dans le binaire (PNG 64×64 décodé au lancement).
fn load_window_icon() -> Option<winit::window::Icon> {
    const PNG: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon/icon_64.png"));
    let img = image::load_from_memory(PNG).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

fn make_app(player: bool) -> App {
    let mut app = App::default();
    if player {
        app.state.player = true;
        app.state.playing = true;
        // En mode Player on joue le jeu exporté (scène embarquée), pas la démo de l'éditeur.
        app.state.use_embedded_scene();
    }
    app
}

/// Point d'entrée desktop (et iOS via le bin).
pub fn run() {
    env_logger::init();
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            log::error!("Création de la boucle d'événements impossible : {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    // Mobile = mode Player (plein écran, sans éditeur) ; desktop via --player ou
    // via la feature `player_build` (utilisée pour exporter un .app jouable).
    let player = std::env::args().any(|a| a == "--player")
        || cfg!(target_os = "ios")
        || cfg!(feature = "player_build");
    let mut app = make_app(player);
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("Boucle d'événements terminée sur erreur : {e}");
    }
}

/// Point d'entrée Android (appelé par android-activity via la NativeActivity).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    let event_loop = match EventLoop::builder().with_android_app(android_app).build() {
        Ok(el) => el,
        Err(e) => {
            log::error!("Création de la boucle d'événements Android impossible : {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = make_app(true); // mobile = mode player
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("Boucle d'événements Android terminée sur erreur : {e}");
    }
}
