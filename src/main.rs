//! Motor3DeRust — moteur 3D minimaliste (winit + wgpu).
//! Sprint 0+1 : fenêtre + cube en perspective avec depth buffer.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

mod editor;
mod gfx;
mod scene;
use gfx::renderer::State;

#[derive(Default)]
struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Motor3DeRust")
            .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        self.state = Some(pollster::block_on(State::new(window)));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        // egui voit l'événement en premier ; s'il le consomme, on ne touche pas à la caméra.
        let consumed = state.on_ui_event(&event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::RedrawRequested => {
                state.render();
                state.window.request_redraw();
            }
            _ if consumed => {}
            WindowEvent::MouseInput { state: btn_state, button: MouseButton::Left, .. } => {
                state.on_mouse_button(btn_state == ElementState::Pressed);
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.on_cursor_moved(position.x, position.y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
                };
                state.on_scroll(d);
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
