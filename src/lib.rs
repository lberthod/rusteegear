//! RusteeGear — moteur 3D minimaliste (winit + wgpu).
//! Exposé en bibliothèque pour partager le point d'entrée entre desktop (bin)
//! et Android (cdylib `android_main`).

use std::collections::HashMap;
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub mod app;
pub mod assets;
pub mod crash_log;
pub mod editor;
pub mod gfx;
pub mod log_buffer;
pub mod net;
/// Pont de pilotage externe (TCP localhost, opt-in) — desktop uniquement, comme
/// le hot-reload d'assets : pas de socket serveur sur mobile/web.
#[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
pub mod pilot;
pub mod runtime;
pub mod scene;
pub mod time_compat;

use app::input::InputEvent;
use app::{AppState, GizmoMode};
use gfx::renderer::Renderer;

#[derive(Default)]
struct App {
    renderer: Option<Renderer>,
    /// wasm32 uniquement (Sprint 114) : `Renderer::new` est asynchrone (WebGPU/
    /// `request_adapter`/`request_device` n'ont pas d'équivalent bloquant dans un
    /// navigateur, contrairement à `pollster::block_on` côté natif) — posé par la
    /// tâche lancée dans `resumed` via `wasm_bindgen_futures::spawn_local`, récupéré
    /// dans `renderer` au prochain événement via `adopt_pending_renderer`.
    #[cfg(target_arch = "wasm32")]
    pending_renderer: std::rc::Rc<std::cell::RefCell<Option<Renderer>>>,
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
    /// Touches d'action tenues (Espace/J/K/H) — cf. `recompute_action_buttons`.
    action_keys_held: std::collections::HashSet<winit::keyboard::KeyCode>,

    // --- manette (Sprint 110) ---
    /// `None` si aucune manette n'a pu être énumérée au lancement (pas de backend
    /// disponible) — le jeu reste jouable au clavier/tactile, cf. `resumed`.
    gilrs: Option<gilrs::Gilrs>,
    /// Boutons manette actuellement tenus, tous contrôleurs connectés confondus
    /// (pas de multi-manette local pour l'instant — un seul joueur par poste).
    gamepad_held: std::collections::HashSet<gilrs::Button>,
    /// Stick gauche brut (avant zone morte, cf. `app::input::apply_deadzone`).
    gamepad_axes: (f32, f32),
    /// Stick droit brut (avant zone morte) : x = visée/rotation, y = tangage
    /// caméra (cf. `PlayerInput::gamepad_pitch`).
    gamepad_axes_right: (f32, f32),
    /// États tenus au frame précédent des bascules manette (Menu/HUD) : la
    /// bascule se déclenche sur le front montant seulement — un bouton tenu
    /// n'ouvre/ferme qu'une fois.
    gamepad_menu_was_held: bool,
    gamepad_hud_was_held: bool,

    // --- hot-reload des assets de projet (Sprint 111, desktop uniquement) ---
    /// Récepteur des événements du dossier d'assets (`assets::assets_dir()`) —
    /// gardé avec le `Watcher` qui l'alimente (cf. `resumed`) : abandonner le
    /// `Watcher` arrêterait la surveillance côté OS. `None` si le dossier était
    /// indisponible au lancement (`$HOME` absent) ou sur mobile.
    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
    asset_watch: Option<(
        notify::RecommendedWatcher,
        std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    )>,

    // --- pont de pilotage externe (opt-in, cf. `pilot`) ---
    /// `None` tant que `--pilot`/`RUSTEEGEAR_PILOT` n'a pas été demandé (cas
    /// normal : aucune surface de commande externe ouverte par défaut).
    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
    pilot: Option<pilot::PilotServer>,
    /// Instant du dernier `RedrawRequested` traité — détecte un rendu gelé
    /// (fenêtre occultée : macOS supprime les redraws) pour que le pont de
    /// pilotage continue de faire avancer le jeu, cf. `about_to_wait`.
    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
    last_render: Option<std::time::Instant>,
}

/// Résout un axe (-1/0/1) à partir de l'état « tenu » des deux touches
/// opposées. Fonction pure, testable sans dépendre de winit ou d'une fenêtre
/// réelle.
///
/// Recalcule l'axe à partir de l'état actuel des **deux** touches à chaque
/// changement, plutôt que d'assigner directement la valeur de la seule touche
/// qui vient de changer — sinon relâcher une touche opposée à une autre
/// encore enfoncée remettrait l'axe à 0 au lieu de revenir à la direction
/// encore tenue (cf. docs/audits/misc.md pour le bug concret que ça évite).
fn axis_from_held(negative: bool, positive: bool) -> f32 {
    match (negative, positive) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        // Ni l'une ni l'autre, ou les deux à la fois (s'annulent) : neutre.
        _ => 0.0,
    }
}

impl App {
    /// wasm32 uniquement : récupère le `Renderer` posé par la tâche asynchrone de
    /// `resumed` dès qu'il est prêt — cf. la doc de `pending_renderer`.
    #[cfg(target_arch = "wasm32")]
    fn adopt_pending_renderer(&mut self) {
        if self.renderer.is_none()
            && let Some(mut renderer) = self.pending_renderer.borrow_mut().take()
        {
            // `window.inner_size()` lu au tout début de `Renderer::new` (avant tout
            // `await`) peut ne pas encore refléter la taille réelle du canvas côté
            // navigateur — course avec la mise en page DOM constatée à l'écran (canvas
            // configuré à 1×1, étiré en un aplat de couleur par le CSS). On recale
            // avec la taille lue directement depuis `window` au moment de l'adoption,
            // une fois la tâche async forcément retombée après le premier tour de
            // boucle d'événements du navigateur.
            if let Some((w, h)) = web_sys::window().and_then(|win| {
                win.inner_width()
                    .ok()?
                    .as_f64()
                    .zip(win.inner_height().ok()?.as_f64())
            }) {
                let dpr = web_sys::window()
                    .map(|w| w.device_pixel_ratio())
                    .unwrap_or(1.0);
                let size = winit::dpi::PhysicalSize::new(
                    ((w * dpr) as u32).max(1),
                    ((h * dpr) as u32).max(1),
                );
                if size != renderer.size {
                    renderer.resize(size);
                }
            }
            self.state
                .set_viewport(renderer.size.width, renderer.size.height);
            self.renderer = Some(renderer);
        }
    }

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
                    self.state
                        .handle_input(InputEvent::PointerDown { pan: false });
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

    /// Table de remapping manette courante, lue dans les paramètres persistés (cf.
    /// `editor::Editor::settings`) — valeurs par défaut si l'éditeur n'est pas encore
    /// initialisé (ne devrait pas arriver : `resumed` crée `Renderer`/`Editor` avant
    /// toute entrée manette possible).
    fn gamepad_bindings(&self) -> app::settings::GamepadBindings {
        self.renderer
            .as_ref()
            .and_then(|r| r.settings())
            .map(|s| s.gamepad.clone())
            .unwrap_or_default()
    }

    /// Recalcule saut/attaque/tir/soin à partir de **toutes** les sources tenues
    /// (touches d'action clavier + boutons manette), même principe que
    /// `axis_from_held` pour les axes : combiner plutôt qu'écraser, sinon relâcher
    /// une des deux sources couperait l'action même si l'autre est encore tenue.
    fn recompute_action_buttons(&mut self) {
        use winit::keyboard::KeyCode;
        let bindings = self.gamepad_bindings();
        let gp = app::input::resolve_gamepad_input(
            &self.gamepad_held,
            self.gamepad_axes,
            self.gamepad_axes_right,
            &bindings,
        );
        let keys = &self.action_keys_held;
        let inp = &mut self.state.input_state;
        inp.jump = keys.contains(&KeyCode::Space) || gp.jump;
        inp.attack = keys.contains(&KeyCode::KeyJ) || gp.attack;
        inp.fire = keys.contains(&KeyCode::KeyK) || gp.fire;
        inp.heal = keys.contains(&KeyCode::KeyH) || gp.heal;
        // Élévation caméra libre (Espace = monte, C = descend) — cf. `AppState::fly_cam`.
        inp.fly_vertical = axis_from_held(
            keys.contains(&KeyCode::KeyC),
            keys.contains(&KeyCode::Space),
        );
        inp.weapon_cycle = gp.weapon;
        // Stick droit horizontal cumulé au stick gauche : en contrôles « tank »,
        // l'orientation du personnage EST la visée — le stick droit est donc un
        // second canal de rotation (habitude « stick de visée » des TPS), pas
        // une caméra libre découplée. Le cumul reste borné par `turn()`.
        inp.gamepad_turn = (gp.turn + gp.look_x).clamp(-1.0, 1.0);
        inp.gamepad_thrust = gp.thrust;
        inp.gamepad_pitch = gp.look_y;
        // Bascules sur front montant (Select = masquer le HUD) — routées vers
        // l'éditeur via le renderer, seul accès. Start : fenêtre Multijoueur en
        // éditeur desktop, mais overlay Paramètres minimal en mode Player (Sprint
        // 2, config hors éditeur) — `mobile_multiplayer_overlay` du mode Player
        // est un panneau toujours affiché, indépendant de `panels.multiplayer`,
        // donc ce bouton y est libre pour un autre usage.
        if gp.menu
            && !self.gamepad_menu_was_held
            && let Some(r) = self.renderer.as_mut()
        {
            if self.state.player {
                r.toggle_player_settings();
            } else {
                r.toggle_multiplayer_window();
            }
        }
        if gp.hud
            && !self.gamepad_hud_was_held
            && let Some(r) = self.renderer.as_mut()
        {
            r.toggle_play_hud();
        }
        self.gamepad_menu_was_held = gp.menu;
        self.gamepad_hud_was_held = gp.hud;
    }

    /// Vide les événements `gilrs` en attente (branchement/débranchement, boutons,
    /// axes) et recalcule l'état combiné — appelé à chaque tour de boucle
    /// (`about_to_wait`), `gilrs` n'ayant pas de mécanisme de callback/event-loop
    /// propre à intégrer à celle de winit.
    fn poll_gamepad(&mut self) {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return;
        };
        let mut changed = false;
        while let Some(gilrs::Event { id, event, .. }) = gilrs.next_event() {
            match event {
                // Diagnostic de branchement : nom + source du mapping. Une
                // Logitech F310/F710 a un commutateur X/D au dos — dans le
                // mauvais mode elle arrive sans mapping SDL (boutons/axes
                // fantaisistes) ou n'apparaît pas du tout selon l'OS ; ce log
                // est le seul indice pour le diagnostiquer sans deviner.
                gilrs::EventType::Connected => {
                    let pad = gilrs.gamepad(id);
                    log::info!(
                        "Manette connectée : « {} » (mapping : {:?}) — si les boutons ne \
                         répondent pas comme attendu (Logitech F310/F710), basculer le \
                         commutateur X/D au dos de la manette",
                        pad.name(),
                        pad.mapping_source()
                    );
                }
                gilrs::EventType::ButtonPressed(btn, _) => {
                    self.gamepad_held.insert(btn);
                    changed = true;
                }
                gilrs::EventType::ButtonReleased(btn, _) => {
                    self.gamepad_held.remove(&btn);
                    changed = true;
                }
                gilrs::EventType::AxisChanged(gilrs::Axis::LeftStickX, v, _) => {
                    self.gamepad_axes.0 = v;
                    changed = true;
                }
                gilrs::EventType::AxisChanged(gilrs::Axis::LeftStickY, v, _) => {
                    self.gamepad_axes.1 = v;
                    changed = true;
                }
                gilrs::EventType::AxisChanged(gilrs::Axis::RightStickX, v, _) => {
                    self.gamepad_axes_right.0 = v;
                    changed = true;
                }
                gilrs::EventType::AxisChanged(gilrs::Axis::RightStickY, v, _) => {
                    self.gamepad_axes_right.1 = v;
                    changed = true;
                }
                gilrs::EventType::Disconnected => {
                    self.gamepad_held.clear();
                    self.gamepad_axes = (0.0, 0.0);
                    self.gamepad_axes_right = (0.0, 0.0);
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            self.recompute_action_buttons();
        }
    }

    /// Vide les événements du watcher d'assets en attente (Sprint 111) et invalide
    /// le cache de textures du renderer dès qu'un fichier a bougé — appelé à chaque
    /// tour de boucle, même principe que `poll_gamepad` (pas de callback à
    /// intégrer à l'event-loop de winit, `notify` livre sur son propre thread via
    /// un canal). Groupe tous les événements en attente en une seule invalidation
    /// plutôt qu'une par événement : un éditeur d'image écrit souvent plusieurs
    /// fois de suite (fichier temporaire + renommage) pour une seule retouche.
    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
    fn poll_asset_hot_reload(&mut self) {
        let Some((_, rx)) = self.asset_watch.as_ref() else {
            return;
        };
        let mut changed = false;
        while rx.try_recv().is_ok() {
            changed = true;
        }
        if changed && let Some(renderer) = self.renderer.as_mut() {
            renderer.invalidate_asset_textures();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        if self.gilrs.is_none() {
            match gilrs::Gilrs::new() {
                Ok(g) => self.gilrs = Some(g),
                // Pas de backend manette sur cette plateforme/config (ex. CI sans
                // udev) : dégrade en silence, clavier/tactile restent utilisables.
                Err(e) => log::info!("Manette indisponible ({e}) — clavier/tactile seuls."),
            }
        }
        #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
        if self.asset_watch.is_none() {
            self.asset_watch = start_asset_watch();
        }
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut attrs = Window::default_attributes()
            .with_title("RusteeGear")
            .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 720.0));
        #[cfg(not(target_arch = "wasm32"))]
        {
            attrs = attrs.with_window_icon(load_window_icon());
        }
        // wasm32 : rattache la fenêtre au <canvas id="rustee-canvas"> de la page
        // hôte (cf. `packaging/web/index.html`) plutôt qu'un canvas créé/inséré tout
        // seul — la page garde le contrôle de sa mise en page.
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            let canvas = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("rustee-canvas"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
            // Remplace la taille par défaut (1024×720, pensée pour desktop) par celle
            // de la fenêtre du navigateur : winit pose `width`/`height` en style inline
            // sur le canvas, ce qui écrase le `width:100%; height:100%` de
            // `packaging/web/index.html` — sans ça, le canvas restait bloqué à
            // 1024×720 quelle que soit la taille réelle de la fenêtre Chrome (bande
            // noire sous le rendu, constaté à l'écran).
            if let Some((w, h)) = web_sys::window().and_then(|w| {
                w.inner_width()
                    .ok()?
                    .as_f64()
                    .zip(w.inner_height().ok()?.as_f64())
            }) {
                attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h));
            }
            attrs = attrs.with_canvas(canvas);
        }
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Création de la fenêtre impossible : {e}");
                event_loop.exit();
                return;
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
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
        // wasm32 : pas de `block_on` possible (le navigateur n'offre aucune attente
        // bloquante sur le thread principal) — la tâche écrit dans `pending_renderer`,
        // récupéré au prochain événement par `adopt_pending_renderer`.
        #[cfg(target_arch = "wasm32")]
        {
            let slot = self.pending_renderer.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Renderer::new(window).await {
                    Ok(renderer) => *slot.borrow_mut() = Some(renderer),
                    Err(e) => log::error!("Initialisation du renderer impossible : {e}"),
                }
            });
        }
    }

    /// Mobile : la surface GPU devient invalide quand l'app passe en arrière-plan.
    /// On lâche le renderer ; `resumed` le reconstruira (l'état applicatif est préservé).
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.renderer = None;
    }

    /// Seul émetteur d'événements utilisateur : le réveil du pont de pilotage
    /// (`EventLoopProxy`, cf. `run()`). Rien à faire ici — `about_to_wait` suit
    /// immédiatement et draine les requêtes en attente.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {}

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        #[cfg(target_arch = "wasm32")]
        self.adopt_pending_renderer();
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
            // Fermeture (croix de la fenêtre) : passe par `request_quit` — avec des
            // modifications non sauvegardées en éditeur, ça ouvre la confirmation
            // Enregistrer / Quitter sans enregistrer / Annuler au lieu de quitter
            // (Phase C, `sprint.19matin.md`) ; sinon on sort immédiatement.
            WindowEvent::CloseRequested => {
                self.state.request_quit();
                if self.state.should_quit {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                renderer.resize(size);
                self.state.set_viewport(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
                {
                    self.last_render = Some(std::time::Instant::now());
                }
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
                button: button @ (MouseButton::Left | MouseButton::Middle),
                ..
            } => {
                let ev = if btn_state == ElementState::Pressed {
                    // Cmd/Maj enfoncé au clic = sélection additive (multi-sélection 3D).
                    let st = self.modifiers.state();
                    self.state
                        .set_additive(st.control_key() || st.super_key() || st.shift_key());
                    // Clic milieu ou Maj+glisser = pan caméra, quel que soit l'outil
                    // (un simple Maj+clic sans glisser reste une sélection additive).
                    InputEvent::PointerDown {
                        pan: button == MouseButton::Middle || st.shift_key(),
                    }
                } else {
                    InputEvent::PointerUp
                };
                self.state.handle_input(ev);
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Touche modificatrice de snap (Ctrl, Sprint 112) : lue à chaque
                // mouvement plutôt que sur `ModifiersChanged` seul — sinon tenir Ctrl
                // *avant* de commencer un glissé de gizmo ne serait vu qu'au prochain
                // changement de modificateur, potentiellement jamais pendant ce glissé.
                self.state
                    .set_snap_modifier(self.modifiers.state().control_key());
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
            // peut laisser l'état « survolé/enfoncé » d'egui en retard d'une frame :
            // sans cette garde, ce trou laisserait passer le toucher jusqu'à
            // l'orbite caméra, qui bougerait la vue au lieu de déplacer le
            // personnage. L'orbite tactile reste réservée à l'éditeur/l'aperçu
            // (sans contrôles mobiles).
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
                        // Gizmos de manipulation d'objet (W/E/R) : réservés à l'éditeur — en
                        // Play, W est aussi la touche d'avance (cf. `is_move_key` plus bas) ;
                        // sans cette garde, avancer repasserait silencieusement `gizmo_mode`
                        // à Translate et désactiverait la garde anti-rattrapage de la caméra
                        // de suivi sur l'outil Main/Orbite/Loupe (cf. `advance_play`).
                        KeyCode::KeyW if !self.state.playing => {
                            self.state.set_gizmo_mode(GizmoMode::Translate)
                        }
                        KeyCode::KeyE if !self.state.playing => {
                            self.state.set_gizmo_mode(GizmoMode::Rotate)
                        }
                        KeyCode::KeyR if !cmd && !self.state.playing => {
                            self.state.set_gizmo_mode(GizmoMode::Scale)
                        }
                        // Outils de navigation caméra (Main/Orbite/Loupe) : utiles en Play
                        // comme en éditeur, donc jamais gardés par `!self.state.playing`.
                        KeyCode::KeyQ if !cmd => self.state.set_gizmo_mode(GizmoMode::Pan),
                        KeyCode::KeyT if !cmd => self.state.set_gizmo_mode(GizmoMode::Orbit),
                        KeyCode::KeyY if !cmd => self.state.set_gizmo_mode(GizmoMode::Zoom),
                        KeyCode::KeyF if !cmd => self.state.frame_selected(),
                        // Caméra libre (« vol libre »/noclip) de l'éditeur : voir
                        // partout sur la carte hors Play, cf. `AppState::toggle_fly_cam`.
                        KeyCode::KeyG if !cmd => self.state.toggle_fly_cam(),
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
                        // Menu pause (Phase J de `sprintreflecion.md`) : sans effet
                        // hors Play (garde dans `toggle_pause`).
                        KeyCode::Escape => self.state.toggle_pause(),
                        // Overlay Paramètres minimal du mode Player (Sprint 2, config
                        // hors éditeur) — équivalent clavier du bouton Start de la
                        // manette, pour tester `--player` sur une machine sans
                        // manette branchée. Gardé par `self.state.player` : `Tab`
                        // reste sans effet particulier en éditeur desktop.
                        KeyCode::Tab if self.state.player => {
                            if let Some(r) = self.renderer.as_mut() {
                                r.toggle_player_settings();
                            }
                        }
                        // Carte plein écran (joueur/alliés/monstres), cf.
                        // `Editor::toggle_player_map`/`player_map_overlay` — mode
                        // Player, mais aussi Play testé depuis l'éditeur
                        // (`self.state.playing`, cf. `build_ui`) : sans ce second
                        // cas, tester la carte nécessitait un vrai build joueur,
                        // impossible à vérifier depuis l'éditeur desktop.
                        KeyCode::KeyM if self.state.player || self.state.playing => {
                            if let Some(r) = self.renderer.as_mut() {
                                r.toggle_player_map();
                            }
                        }
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
                    // Espace/J/K/H/C : tenus dans un ensemble séparé (plutôt qu'assignés
                    // directement à `inp.jump`/etc.) pour pouvoir les combiner avec la
                    // manette (cf. `recompute_action_buttons`) sans que l'une des deux
                    // sources n'écrase l'état de l'autre au relâchement. C ne sert qu'à
                    // descendre en caméra libre (`fly_vertical`), sans binding manette.
                    let is_action_key = matches!(
                        code,
                        KeyCode::Space
                            | KeyCode::KeyJ
                            | KeyCode::KeyK
                            | KeyCode::KeyH
                            | KeyCode::KeyC
                    );
                    if is_action_key {
                        if pressed {
                            self.action_keys_held.insert(code);
                        } else {
                            self.action_keys_held.remove(&code);
                        }
                        self.recompute_action_buttons();
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
                        // tournent le personnage sur lui-même (A = droite, D = gauche)
                        // plutôt que de le faire strafer ; W/S avancent/reculent le
                        // long de son orientation *actuelle* (cf.
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
        #[cfg(target_arch = "wasm32")]
        self.adopt_pending_renderer();
        self.poll_gamepad();
        #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
        self.poll_asset_hot_reload();
        // Pont de pilotage : traite les requêtes externes sur ce thread, seul
        // détenteur légitime de `AppState`/`Renderer` — même modèle de drainage
        // que `poll_gamepad`/`poll_asset_hot_reload`.
        #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
        if let Some(p) = self.pilot.as_ref() {
            // Fenêtre occultée/masquée : macOS supprime les redraws, donc
            // `render()` — qui porte normalement `advance_play` — ne tourne
            // plus, et la simulation, les chargements asynchrones et le réseau
            // gèlent alors que le pont répond toujours (constaté à l'audit du
            // 19 juillet 2026 : `move` sans effet, `scene load` jamais
            // appliqué, appli derrière le terminal). Quand le rendu est resté
            // muet plus de 100 ms, on fait avancer le jeu d'ici — **avant** le
            // drainage, pour que les réponses reflètent l'état frais (imports
            // appliqués, temps intégré sous les entrées encore tenues).
            // `advance_play` est à accumulateur de pas fixes sur temps réel :
            // l'appeler depuis un second site ne double jamais la simulation,
            // ça partage le même budget de temps écoulé. Note : l'App Nap
            // étrangle aussi les timers du process — le temps réel reste donc
            // saccadé fenêtre masquée ; les gestes `move`/`step` du pont
            // avancent en temps *simulé* (`advance_steps`) précisément pour
            // rester déterministes dans cet état.
            let render_stalled = self
                .last_render
                .is_none_or(|t| t.elapsed() > std::time::Duration::from_millis(100));
            if render_stalled {
                self.state.advance_play();
            }
            p.poll(&mut self.state, self.renderer.as_mut());
        }
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

/// Démarre la surveillance du dossier d'assets de projet (Sprint 111), créé au
/// besoin (comme `assets::write_user_bytes_at` le fait déjà pour `user://`) — sans
/// ça, un poste qui n'a encore rien importé n'aurait pas de dossier à surveiller et
/// le hot-reload resterait inactif jusqu'au premier import suivi d'un redémarrage.
/// `None` si `$HOME` est indisponible (pas de dossier possible) ou si le backend de
/// surveillance de l'OS ne démarre pas (dégrade en silence, comme `poll_gamepad`
/// pour une manette absente — l'édition manuelle du fichier JSON de scène reste le
/// filet de secours dans les deux cas).
#[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
fn start_asset_watch() -> Option<(
    notify::RecommendedWatcher,
    std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
)> {
    use notify::Watcher;
    let dir = crate::assets::assets_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).ok()?;
    watcher
        .watch(&dir, notify::RecursiveMode::NonRecursive)
        .ok()?;
    Some((watcher, rx))
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
        // `RUSTEEGEAR_OFFLINE=1` (desktop) désactive cette connexion : un player
        // sans réseau ne doit jamais dépendre du serveur pour être jouable —
        // pas de variable d'environnement sur wasm32, le web garde l'auto-connexion.
        #[cfg(not(target_arch = "wasm32"))]
        let offline = offline_requested(std::env::var("RUSTEEGEAR_OFFLINE").ok().as_deref());
        #[cfg(target_arch = "wasm32")]
        let offline = false;
        if offline {
            log::info!("Mode hors-ligne (RUSTEEGEAR_OFFLINE) : pas de connexion au serveur.");
        } else {
            log::info!(
                "Connexion au serveur multijoueur par défaut : {} (RUSTEEGEAR_OFFLINE=1 pour jouer hors-ligne)",
                crate::app::network_client::DEFAULT_SERVER_URL
            );
            app.state.connect_to_server(
                crate::app::network_client::DEFAULT_SERVER_URL,
                &guest_name(),
            );
        }
    } else {
        // L'éditeur s'ouvre directement sur la scène de base du MMORPG
        // (`assets/player_scene.json`, embarquée — la même que jouent le site web
        // et les builds Player), pas sur la petite démo par défaut. `clear_history`
        // évite qu'un Ctrl+Z juste après l'ouverture ramène la scène vide interne.
        app.state.load_embedded_player_scene();
        app.state.clear_history();
    }
    app
}

/// Interprète `RUSTEEGEAR_OFFLINE` (Phase A pré-test externe) : toute valeur
/// autre que absente ou `"0"` demande le mode hors-ligne. Fonction pure (la
/// variable est lue par l'appelant) pour rester testable sans muter
/// l'environnement du process, cf. `assets::override_assets_dir_for_test`
/// qui suit le même principe.
#[cfg(not(target_arch = "wasm32"))]
fn offline_requested(var: Option<&str>) -> bool {
    matches!(var, Some(v) if v != "0")
}

/// Interprète la demande de pont de pilotage : `--pilot` (port par défaut),
/// `--pilot=NNNN`, ou `RUSTEEGEAR_PILOT` (`1`/vide = port par défaut, un nombre =
/// ce port, `0` = désactivé). Fonction pure sur les arguments/variable passés,
/// même principe que `offline_requested`. Un port invalide retombe sur le port
/// par défaut plutôt que d'ignorer la demande en silence.
#[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
fn pilot_port_requested<I: Iterator<Item = String>>(args: I, env: Option<&str>) -> Option<u16> {
    for arg in args {
        if arg == "--pilot" {
            return Some(pilot::DEFAULT_PORT);
        }
        if let Some(port) = arg.strip_prefix("--pilot=") {
            return Some(port.parse().unwrap_or(pilot::DEFAULT_PORT));
        }
    }
    match env {
        None | Some("0") => None,
        Some("") | Some("1") => Some(pilot::DEFAULT_PORT),
        Some(v) => Some(v.parse().unwrap_or(pilot::DEFAULT_PORT)),
    }
}

/// Pseudo généré au hasard (« InvitéNNNN ») pour la connexion automatique en
/// mode Player — évite d'exiger une saisie manuelle juste pour rejoindre le
/// serveur par défaut. Basé sur l'horloge plutôt qu'une dépendance `rand`
/// (aucune autre n'existe déjà dans le projet pour ce besoin ponctuel).
fn guest_name() -> String {
    let nanos = crate::time_compat::SystemTime::now()
        .duration_since(crate::time_compat::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("Invité{}", nanos % 10000)
}

/// Point d'entrée desktop (et iOS via le bin).
#[cfg(not(target_arch = "wasm32"))]
pub fn run() {
    crate::log_buffer::install();
    crate::crash_log::install();
    // Bannière de démarrage (Phase A, sprint.19matin.md) : la première ligne
    // qu'un nouvel utilisateur lit doit nommer le produit et sa version — le
    // détail (GPU, scène) suit sur les lignes de log de chaque sous-système.
    log::info!("RusteeGear {}", env!("CARGO_PKG_VERSION"));
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
    // Pont de pilotage externe (opt-in explicite : éval Lua = exécution de code
    // arbitraire — jamais actif sans demande, et annoncé en clair dans les logs).
    // `run()` compile pour toute cible non-wasm32 (donc aussi iOS/Android), mais
    // le module `pilot` lui-même exclut en plus iOS/Android (`pub mod pilot`,
    // plus haut) — sans ce gate, `cargo build` pour ces cibles échoue avec
    // « cannot find module `pilot` » (régression découverte lors du Sprint 2,
    // audit du 19 juillet 2026 : cassait les cross-builds iOS/Android de la CI
    // et le job Android de la Release, tous silencieusement rouges depuis
    // l'introduction du pont).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if let Some(port) = pilot_port_requested(
        std::env::args(),
        std::env::var("RUSTEEGEAR_PILOT").ok().as_deref(),
    ) {
        // Réveil immédiat de la boucle d'événements à chaque requête reçue : au
        // repos elle dort 60 ms entre deux tours (cf. `about_to_wait`) — sans ce
        // proxy, chaque commande pilot paierait cette latence.
        let proxy = event_loop.create_proxy();
        let waker: pilot::Waker = Box::new(move || {
            let _ = proxy.send_event(());
        });
        match pilot::PilotServer::start(port, Some(waker)) {
            Ok(server) => {
                log::info!(
                    "Pont de pilotage actif sur {} — toute commande locale (console, Lua, \
                     captures) est acceptée sur ce port.",
                    server.local_addr
                );
                app.pilot = Some(server);
            }
            Err(e) => log::error!("{e}"),
        }
    }
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("Boucle d'événements terminée sur erreur : {e}");
    }
}

/// Point d'entrée web (Sprint 114, défrichage) : appelé automatiquement au
/// chargement du module wasm (`#[wasm_bindgen(start)]`) depuis
/// `packaging/web/index.html`. Toujours en mode Player (pas de panneaux éditeur
/// egui dans le navigateur pour l'instant) ; `event_loop.run_app` bloquerait le
/// thread principal du navigateur (interdit), donc `spawn_app` plutôt que `run_app`
/// — l'App vit dans des callbacks web (`requestAnimationFrame` sous le capot),
/// jamais dans une boucle Rust bloquante comme sur desktop.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn run_web() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    crate::crash_log::install();
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            log::error!("Création de la boucle d'événements impossible : {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let app = make_app(true);
    use winit::platform::web::EventLoopExtWebSys;
    event_loop.spawn_app(app);
}

/// Point d'entrée Android (appelé par android-activity via la NativeActivity).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    // Sauvegarde de partie : seule façon d'obtenir un dossier écrivable
    // garanti sur Android (`$HOME` n'existe pas) — posé une fois, avant tout accès à
    // `assets::user_dir()` (`AppState::save_game`/`load_game`, en cours de Play).
    match android_app.internal_data_path() {
        Some(path) => crate::assets::set_android_data_dir(path),
        None => log::error!(
            "Chemin de stockage interne Android indisponible : les sauvegardes de \
             partie ne fonctionneront pas cette session."
        ),
    }
    // Après `set_android_data_dir` : le hook de crash écrit dans `user://`, qui en
    // dépend sur Android (cf. doc de `crash_log::install`).
    crate::crash_log::install();

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
    fn the_editor_opens_directly_on_the_embedded_player_scene_with_a_clean_history() {
        let mut app = make_app(false);
        // « Feu communal » n'existe que dans la scène du jeu exporté
        // (`assets/player_scene.json`, le hameau fortifié) — ni dans la démo par
        // défaut ni dans `Scene::mmorpg_demo`.
        assert!(
            app.state
                .scene
                .objects
                .iter()
                .any(|o| o.name == "Feu communal"),
            "l'éditeur doit s'ouvrir sur la scène de base du MMORPG \
             (assets/player_scene.json), pas sur une démo"
        );
        // Ctrl+Z juste après l'ouverture ne doit PAS ramener la scène vide
        // interne : l'historique est vidé au démarrage (`clear_history`).
        let n = app.state.scene.objects.len();
        app.state.undo();
        assert_eq!(
            app.state.scene.objects.len(),
            n,
            "annuler au démarrage ne doit rien changer (historique vide)"
        );
    }

    #[test]
    fn first_launch_without_any_user_folder_still_opens_the_embedded_scene() {
        // Phase A (pré-test externe) : sur une machine vierge, `~/.motor3derust/`
        // n'existe pas encore — l'éditeur doit s'ouvrir quand même, sur la scène
        // embarquée, sans paniquer ni exiger un asset de projet. On redirige
        // `assets_dir()` vers un dossier qui N'EXISTE PAS (pas juste vide) :
        // c'est exactement l'état d'un premier lancement.
        let missing = std::env::temp_dir().join(format!(
            "rusteegear_first_launch_{}_{}",
            std::process::id(),
            line!()
        ));
        assert!(!missing.exists());
        let _guard = crate::assets::override_assets_dir_for_test(missing.clone());

        let app = make_app(false);
        assert!(
            !app.state.scene.objects.is_empty(),
            "premier lancement : la scène embarquée doit se charger sans dossier utilisateur"
        );
        // Le navigateur d'assets doit lister les assets embarqués (bundle://)
        // sans erreur, même sans dossier projet sur disque.
        for a in crate::assets::list_assets() {
            assert!(
                a.starts_with(crate::assets::SCHEME),
                "sans dossier projet, seuls les assets embarqués existent (reçu : {a})"
            );
        }
        // Et rien ne doit avoir créé le dossier en douce juste pour démarrer :
        // le premier lancement est en lecture seule tant qu'on n'importe rien.
        assert!(
            !missing.exists(),
            "démarrer ne doit pas écrire dans le dossier d'assets projet"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn offline_is_requested_by_any_value_except_absent_or_zero() {
        assert!(!offline_requested(None), "variable absente : en ligne");
        assert!(
            !offline_requested(Some("0")),
            "0 : en ligne (opt-out du opt-out)"
        );
        assert!(offline_requested(Some("1")));
        assert!(
            offline_requested(Some("true")),
            "toute autre valeur : hors-ligne"
        );
    }

    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
    #[test]
    fn pilot_port_is_only_requested_explicitly_by_flag_or_env() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // Cas normal : jamais actif sans demande explicite.
        assert_eq!(pilot_port_requested(args(&[]).into_iter(), None), None);
        assert_eq!(pilot_port_requested(args(&[]).into_iter(), Some("0")), None);
        // Flag CLI, avec ou sans port.
        assert_eq!(
            pilot_port_requested(args(&["--pilot"]).into_iter(), None),
            Some(pilot::DEFAULT_PORT)
        );
        assert_eq!(
            pilot_port_requested(args(&["--pilot=5000"]).into_iter(), None),
            Some(5000)
        );
        // Port invalide : la demande est honorée quand même, sur le port par défaut.
        assert_eq!(
            pilot_port_requested(args(&["--pilot=abc"]).into_iter(), None),
            Some(pilot::DEFAULT_PORT)
        );
        // Variable d'environnement.
        assert_eq!(
            pilot_port_requested(args(&[]).into_iter(), Some("1")),
            Some(pilot::DEFAULT_PORT)
        );
        assert_eq!(
            pilot_port_requested(args(&[]).into_iter(), Some("5001")),
            Some(5001)
        );
    }

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
