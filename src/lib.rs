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
pub mod log_buffer;
pub mod net;
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

    /// Touches de déplacement actuellement enfoncées (WASD + flèches). Sert à
    /// recalculer `key_move`/`tilt` à partir de **toutes** les touches tenues
    /// à chaque pression/relâchement, plutôt que d'écraser l'axe avec la seule
    /// touche qui vient de changer — cf. `recompute_move_axes`.
    keys_held: std::collections::HashSet<winit::keyboard::KeyCode>,
}

/// Résout un axe (-1/0/1) à partir de l'état « tenu » des deux touches
/// opposées. Fonction pure, testable sans dépendre de winit ou d'une fenêtre
/// réelle.
///
/// **Correctif (2026-07-12, déplacements WASD)** : l'ancien code assignait
/// directement `v` (0.0 ou 1.0 selon pressé/relâché) à l'axe pour la touche
/// qui venait de changer, sans tenir compte de l'autre touche du même axe.
/// Conséquence concrète : tenir A (gauche, axe=-1), puis appuyer D (droite,
/// axe=+1) pendant que A est encore enfoncée, puis relâcher D — l'axe
/// retombait à 0 (D relâchée écrit `v=0.0` sans condition) au lieu de revenir
/// à -1 (A est pourtant toujours enfoncée). Ce bug rendait les changements de
/// direction rapides (fréquents en jeu) imprécis/saccadés. En recalculant
/// l'axe à partir de l'état actuel des **deux** touches à chaque changement,
/// le résultat est toujours cohérent avec ce qui est réellement enfoncé.
fn axis_from_held(negative: bool, positive: bool) -> f32 {
    match (negative, positive) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        // Ni l'une ni l'autre, ou les deux à la fois (s'annulent) : neutre.
        _ => 0.0,
    }
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

        // En mode player sans contrôles tactiles, egui n'intercepte rien. Mais si la
        // scène a un joystick/boutons, on laisse egui traiter l'évènement (et il
        // n'est « consommé » pour le jeu que si un contrôle l'a effectivement utilisé).
        let consumed = if self.state.player && !self.state.scene.mobile.any() {
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
                // Fermeture demandée par le menu Fichier → Quitter.
                if self.state.should_quit {
                    event_loop.exit();
                }
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
            // En Player avec des contrôles tactiles actifs (joystick, croix
            // directionnelle, boutons), un doigt ne doit **jamais** faire orbiter
            // la caméra à la place d'agir sur un contrôle — même si `consumed`
            // (egui) ne l'a pas repéré cette frame précise. Un appui immobile sur
            // un bouton (la croix directionnelle, contrairement au joystick, ne
            // génère quasiment aucun `TouchPhase::Moved` une fois le doigt posé)
            // peut laisser l'état « survolé/enfoncé » d'egui en retard d'une frame
            // : sans cette garde, ce trou laissait passer le toucher jusqu'à
            // l'orbite caméra, qui bougeait la vue au lieu de déplacer le
            // personnage (constaté en test réel sur l'APK, 2026-07-12). L'orbite
            // tactile reste réservée à l'éditeur/l'aperçu (sans contrôles mobiles).
            WindowEvent::Touch(touch) if !(self.state.player && self.state.scene.mobile.any()) => {
                self.handle_touch(touch);
            }
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
                        KeyCode::KeyF if !cmd => self.state.frame_selected(),
                        KeyCode::KeyZ if cmd && st.shift_key() => self.state.redo(),
                        KeyCode::KeyZ if cmd => self.state.undo(),
                        KeyCode::KeyD if cmd => self.state.duplicate_selected(),
                        KeyCode::KeyC if cmd => self.state.copy_selected(),
                        KeyCode::KeyV if cmd => self.state.paste(),
                        KeyCode::KeyX if cmd => self.state.cut_selected(),
                        KeyCode::KeyA if cmd => self.state.select_all(),
                        KeyCode::Backspace | KeyCode::Delete => self.state.delete_selected(),
                        // Sélection directe de l'arme à distance (cf.
                        // `app::fireball::RANGED_WEAPONS`) — le pendant tactile
                        // est le bouton « Arme », qui cycle.
                        KeyCode::Digit1 if !cmd => self.state.select_weapon(0),
                        KeyCode::Digit2 if !cmd => self.state.select_weapon(1),
                        KeyCode::Digit3 if !cmd => self.state.select_weapon(2),
                        _ => {}
                    }
                }
                // Contrôles « ordinateur » : flèches / WASD = déplacement, Espace = saut.
                if let PhysicalKey::Code(code) = key_event.physical_key {
                    let pressed = key_event.state == ElementState::Pressed;
                    let is_move_key = matches!(
                        code,
                        KeyCode::ArrowLeft
                            | KeyCode::ArrowRight
                            | KeyCode::ArrowUp
                            | KeyCode::ArrowDown
                            | KeyCode::KeyA
                            | KeyCode::KeyD
                            | KeyCode::KeyW
                            | KeyCode::KeyS
                    );
                    if is_move_key {
                        if pressed {
                            self.keys_held.insert(code);
                        } else {
                            self.keys_held.remove(&code);
                        }
                    }
                    let inp = &mut self.state.input_state;
                    match code {
                        KeyCode::Space => inp.jump = pressed,
                        KeyCode::KeyJ => inp.attack = pressed,
                        // Boule de feu (attaque à distance, cf. `app::fireball`) :
                        // voisine de J (Attaque) pour garder les deux sous la main.
                        KeyCode::KeyK => inp.fire = pressed,
                        // Soin coopératif (cf. `app::health`) : voisine de J/K.
                        KeyCode::KeyH => inp.heal = pressed,
                        _ => {}
                    }
                    if is_move_key {
                        // Recalcule les axes à partir de **toutes** les touches
                        // actuellement tenues (cf. `axis_from_held`) : sans ça,
                        // relâcher une touche opposée à une autre encore enfoncée
                        // remettait l'axe à 0 au lieu de revenir à la direction
                        // encore tenue.
                        let held = &self.keys_held;
                        let arrow_left = held.contains(&KeyCode::ArrowLeft);
                        let arrow_right = held.contains(&KeyCode::ArrowRight);
                        let arrow_up = held.contains(&KeyCode::ArrowUp);
                        let arrow_down = held.contains(&KeyCode::ArrowDown);
                        let a = held.contains(&KeyCode::KeyA);
                        let d = held.contains(&KeyCode::KeyD);
                        let w = held.contains(&KeyCode::KeyW);
                        let s = held.contains(&KeyCode::KeyS);

                        let inp = &mut self.state.input_state;
                        // Flèches : déplacement relatif à la caméra (comportement
                        // inchangé, cf. `camera_relative_move`).
                        inp.key_move.0 = axis_from_held(arrow_left, arrow_right);
                        inp.key_move.1 = axis_from_held(arrow_down, arrow_up);
                        // WASD : contrôles « tank », indépendants de la caméra. A/D
                        // tournent le personnage sur lui-même (A = droite, D = gauche —
                        // demandé le 2026-07-12) plutôt que de le faire strafer ; W/S
                        // avancent/reculent le long de son orientation *actuelle* (cf.
                        // `AppState::advance_play`). Tourner à droite fait décroître le
                        // yaw (cf. `Physics::face_direction` : yaw=0 pointe vers -Z, et
                        // tourner vers +X, à droite, correspond à un yaw négatif).
                        inp.key_turn = axis_from_held(a, d);
                        inp.key_thrust = axis_from_held(s, w);
                        // Les flèches alimentent aussi le gyroscope simulé (objets
                        // gyro_control) — WASD n'y touche pas (comportement inchangé).
                        inp.tilt.0 = axis_from_held(arrow_left, arrow_right);
                        inp.tilt.1 = axis_from_held(arrow_down, arrow_up);
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
        if let Some(renderer) = &self.renderer
            && let Some(window) = &renderer.window
        {
            window.request_redraw();
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
    const PNG: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icon/icon_64.png"
    ));
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
        // Connexion automatique au serveur RusteeGear par défaut (VPS) : sans
        // ça, chaque test APK ↔ desktop demande de ressaisir l'adresse et un
        // pseudo à la main des deux côtés avant de pouvoir se voir bouger.
        // Reste un simple point de départ : la fenêtre/overlay Multijoueur
        // permet toujours de se déconnecter et pointer ailleurs.
        app.state.connect_to_server(
            crate::app::network_client::DEFAULT_SERVER_URL,
            &guest_name(),
        );
    }
    app
}

/// Pseudo généré au hasard (« InvitéNNNN ») pour la connexion automatique en
/// mode Player — évite d'exiger une saisie manuelle juste pour rejoindre le
/// serveur par défaut. Basé sur l'horloge plutôt qu'une dépendance `rand`
/// (aucune autre n'existe déjà dans le projet pour ce besoin ponctuel).
fn guest_name() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("Invité{}", nanos % 10000)
}

/// Point d'entrée desktop (et iOS via le bin).
pub fn run() {
    crate::log_buffer::install();
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

    // Sauvegarde de partie (Sprint 98) : seule façon d'obtenir un dossier écrivable
    // garanti sur Android (`$HOME` n'existe pas) — posé une fois, avant tout accès à
    // `assets::user_dir()` (`AppState::save_game`/`load_game`, en cours de Play).
    match android_app.internal_data_path() {
        Some(path) => crate::assets::set_android_data_dir(path),
        None => log::error!(
            "Chemin de stockage interne Android indisponible : les sauvegardes de \
             partie ne fonctionneront pas cette session."
        ),
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_from_held_is_neutral_when_neither_key_is_held() {
        assert_eq!(axis_from_held(false, false), 0.0);
    }

    #[test]
    fn axis_from_held_follows_the_single_held_key() {
        assert_eq!(axis_from_held(true, false), -1.0);
        assert_eq!(axis_from_held(false, true), 1.0);
    }

    #[test]
    fn axis_from_held_cancels_out_when_both_keys_are_held() {
        // Ex. A et D tenues ensemble : ni gauche ni droite, comme dans la
        // plupart des jeux (pas de préférence arbitraire pour l'une ou l'autre).
        assert_eq!(axis_from_held(true, true), 0.0);
    }

    #[test]
    fn axis_from_held_returns_to_the_remaining_key_after_releasing_the_other() {
        // Le bug corrigé : A tenue, D pressée puis relâchée — l'axe doit
        // revenir à -1 (A toujours tenue), pas retomber à 0.
        assert_eq!(axis_from_held(true, true), 0.0, "les deux tenues : neutre");
        assert_eq!(
            axis_from_held(true, false),
            -1.0,
            "D relâchée, A toujours tenue : doit revenir à gauche"
        );
    }
}
