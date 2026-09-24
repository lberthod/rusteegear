//! Ce qu'une session VR affiche — **même code** pour l'APK Quest (`xr::hello`)
//! et le simulateur desktop (`quest_sim`) : seules la source des poses d'yeux
//! et les cibles de rendu diffèrent.
//!
//! - `Cubes` : scène de test de la phase 0 (`xr::test_scene`) ;
//! - `Game` : une vraie partie du moteur (phase 1 : Rivière), rendue par le
//!   `Renderer` complet via `render_views`, la pièce du joueur placée dans le
//!   monde par un `Rig`.

use glam::{Quat, Vec2, Vec3};

use super::input::{Haptic, LEFT, PRESS_THRESHOLD, RIGHT, XrInput, haptics_from_fx};
use super::locomotion::{Comfort, SnapTurn, ViewMode, vignette_strength};
use super::math::EyeView;
use super::rig::Rig;
use super::test_scene::CubeScene;
use super::ui::{MENU, PanelPose, Pointer, VrUi, WRIST};
use crate::app::AppState;
use crate::gfx::renderer::Renderer;

/// Scène VR choisie au lancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneChoice {
    Cubes,
    Riviere,
}

impl SceneChoice {
    /// `"cubes"` → `Cubes`, tout le reste → `Riviere` (défaut depuis la phase 1).
    pub fn parse(name: &str) -> Self {
        if name.eq_ignore_ascii_case("cubes") {
            Self::Cubes
        } else {
            Self::Riviere
        }
    }
}

/// Ce qu'une image VR renvoie à la boucle (casque ou simulateur).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameOut {
    /// Vibrations à jouer (manette gauche, droite).
    pub haptics: [Option<Haptic>; 2],
    /// « Quitter » choisi dans le menu VR : fermer la session.
    pub quit: bool,
}

/// Menu VR ouvert : où il flotte, et l'état de pause à rétablir à sa fermeture.
#[derive(Debug, Clone, Copy)]
struct MenuState {
    pose: PanelPose,
    paused_before: bool,
}

/// Actions du menu VR (phase 5).
#[derive(Debug, Clone, Copy, PartialEq)]
enum MenuAction {
    Resume,
    Recenter,
    ToggleView,
    CycleSnap,
    ToggleVignette,
    Restart,
    Quit,
}

pub enum XrContent {
    Cubes(CubeScene),
    Game(Box<GameView>),
}

/// Partie en cours vue en VR.
pub struct GameView {
    pub app: AppState,
    pub renderer: Renderer,
    /// Posé à la première image, une fois le monde physique construit
    /// (`Rig::spectator` a besoin du terrain).
    pub rig: Option<Rig>,
    /// Durées (ms) de la dernière image : simulation du jeu (`advance_play`)
    /// et rendu des deux yeux (préparation CPU + encodage, hors attente GPU) —
    /// mesure de la phase 2.
    pub last_sim_ms: f32,
    pub last_render_ms: f32,
    /// Réglages de confort (vue, crans de rotation, vignette) — menu VR (P5).
    pub comfort: Comfort,
    snap: SnapTurn,
    /// Clic du stick droit à l'image précédente (bascule de vue sur front).
    view_click_was: bool,
    /// (`fx.damage_flash`, `fx.attack_flash`) de l'image précédente : fronts
    /// montants → vibrations.
    prev_fx: (f32, f32),
    /// Pieds du personnage à l'image précédente et instant : vitesse → vignette.
    prev_feet: Option<(Vec3, crate::time_compat::Instant)>,
    /// Vitesse lissée (m/s) : l'écart image à image est bruité par
    /// l'interpolation des poses de simulation.
    speed: f32,
    /// Interface VR (phase 5) : menu flottant et affichage au poignet.
    ui: VrUi,
    menu: Option<MenuState>,
    /// Bouton menu tenu à l'image précédente (ouverture sur front).
    menu_was: bool,
}

impl XrContent {
    /// `format`/`width`/`height` : format et taille **d'un œil** des cibles
    /// qui seront passées à `render`.
    pub fn new(
        choice: SceneChoice,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        match choice {
            SceneChoice::Cubes => Self::Cubes(CubeScene::new(device, format, width, height)),
            SceneChoice::Riviere => {
                let mut app = AppState::default();
                app.load_riviere_demo();
                app.playing = true;
                let renderer = Renderer::new_external(
                    adapter,
                    device.clone(),
                    queue.clone(),
                    format,
                    width,
                    height,
                );
                Self::Game(Box::new(GameView {
                    app,
                    renderer,
                    rig: None,
                    last_sim_ms: 0.0,
                    last_render_ms: 0.0,
                    comfort: Comfort::default(),
                    snap: SnapTurn::default(),
                    view_click_was: false,
                    prev_fx: (0.0, 0.0),
                    prev_feet: None,
                    speed: 0.0,
                    ui: VrUi::new(device, format),
                    menu: None,
                    menu_was: false,
                }))
            }
        }
    }

    /// Avance le jeu d'une image et rend les deux yeux. `eyes` : poses dans la
    /// **pièce** (espace `STAGE` / tête simulée), `input` : manettes dans la
    /// pièce ; le rig place tout dans le monde. Renvoie les vibrations à jouer
    /// et une éventuelle demande de sortie (menu « Quitter »).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: [EyeView; 2],
        input: &XrInput,
        targets: [&wgpu::TextureView; 2],
        width: u32,
        height: u32,
    ) -> FrameOut {
        match self {
            Self::Cubes(scene) => {
                scene.render(device, queue, targets, eyes.map(|e| e.view_proj()));
                FrameOut::default()
            }
            Self::Game(game) => game.frame(device, queue, eyes, input, targets, width, height),
        }
    }
}

impl GameView {
    #[allow(clippy::too_many_arguments)]
    fn frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: [EyeView; 2],
        input: &XrInput,
        targets: [&wgpu::TextureView; 2],
        width: u32,
        height: u32,
    ) -> FrameOut {
        let head_stage = (eyes[0].position + eyes[1].position) * 0.5;
        let head_rot = eyes[0].orientation;
        let menu_open = self.menu.is_some();

        // 1. Manettes → commandes du jeu (P3) — relâchées tant que le menu est
        //    ouvert (la gâchette y clique, elle ne doit pas attaquer).
        if menu_open {
            XrInput::default().apply_to(&mut self.app.input_state);
        } else {
            input.apply_to(&mut self.app.input_state);
        }

        // 2. Clic du stick droit : première personne ⇄ spectateur (P4).
        let click = input.hands[RIGHT].stick_click;
        if click && !self.view_click_was {
            self.comfort.view = self.comfort.view.toggled();
            self.rig = None; // replacé à l'étape 5 selon la nouvelle vue
            log::info!("VR : vue {:?}", self.comfort.view);
        }
        self.view_click_was = click;

        // 3. Rotation par crans, puis caméra de jeu alignée sur le regard : le
        //    déplacement au stick (relatif à la caméra) va là où l'on regarde.
        if let Some(rig) = self.rig.as_mut() {
            let stick_x = if menu_open {
                0.0
            } else {
                input.hands[RIGHT].stick.0
            };
            let turn = self.snap.update(stick_x, self.comfort.snap_degrees);
            if turn != 0.0 {
                rig.rotate_about_head(turn, head_stage);
            }
            let look = rig.look_world(head_rot);
            let (moves, yaw) =
                super::locomotion::engine_move(self.app.input_state.gamepad_move, look);
            self.app.input_state.gamepad_move = moves;
            self.app.vr_camera_yaw = Some(yaw);
        }

        // 4. Simulation.
        let t0 = crate::time_compat::Instant::now();
        self.app.advance_play();
        self.last_sim_ms = t0.elapsed().as_secs_f32() * 1000.0;

        // 5. Rig : suit le personnage en première personne, fixe en spectateur.
        let feet = player_feet(&self.app);
        let comfort = self.comfort;
        let rig = self.rig.get_or_insert_with(|| {
            let rig = match comfort.view {
                ViewMode::Spectator => Rig::spectator(&self.app),
                ViewMode::FirstPerson => Rig {
                    origin: Vec3::ZERO,
                    yaw: Rig::yaw_facing(self.app.camera.target - self.app.camera.eye()),
                },
            };
            log::info!(
                "VR : rig posé en ({:.2}, {:.2}, {:.2}), lacet {:.0}°",
                rig.origin.x,
                rig.origin.y,
                rig.origin.z,
                rig.yaw.to_degrees()
            );
            rig
        });
        let first_person = comfort.view == ViewMode::FirstPerson;
        if let (true, Some(feet)) = (first_person, feet) {
            rig.follow_first_person(feet, head_stage);
        }
        let rig = *rig;

        // 6. Vignette selon la vitesse du personnage (première personne).
        let now = crate::time_compat::Instant::now();
        let speed = match (self.prev_feet, feet) {
            (Some((p, t)), Some(f)) => {
                let dt = now.duration_since(t).as_secs_f32().max(1e-3);
                Vec3::new(f.x - p.x, 0.0, f.z - p.z).length() / dt
            }
            _ => 0.0,
        };
        self.prev_feet = feet.map(|f| (f, now));
        self.speed += (speed - self.speed) * 0.2;
        self.renderer.vr_vignette = if first_person {
            vignette_strength(self.speed, comfort.vignette)
        } else {
            0.0
        };

        // 7. Le personnage est masqué en première personne (les yeux sont dedans).
        let hidden = first_person
            .then(|| self.app.player_index())
            .flatten()
            .filter(|&i| self.app.scene.objects[i].visible);
        if let Some(i) = hidden {
            self.app.scene.objects[i].visible = false;
        }

        // 8. Repères des manettes (P3) : boîte de la poignée et direction.
        for hand in &input.hands {
            if let Some((pos, rot)) = hand.grip {
                let (p, r) = to_world(&rig, pos, rot);
                push_controller_lines(&mut self.app.debug_lines, p, r);
            }
        }

        // 9. Interface (P5) : menu (bouton menu de la manette gauche) et poignet.
        let eyes_world = eyes.map(|e| rig.to_world(e));
        let head_world = (eyes_world[0].position + eyes_world[1].position) * 0.5;
        let look = rig.look_world(head_rot);
        let menu_btn = input.hands[LEFT].menu;
        if menu_btn && !self.menu_was {
            self.toggle_menu(head_world, look);
        }
        self.menu_was = menu_btn;
        let mut actions = Vec::new();
        if let Some(menu) = self.menu {
            let pointer = self.menu_pointer(&rig, input, menu.pose);
            let comfort = self.comfort;
            self.ui.paint(device, queue, MENU, pointer, |ui| {
                menu_ui(ui, comfort, &mut actions);
            });
        }
        let wrist = input.hands[LEFT].grip.map(|(pos, rot)| {
            let (p, _) = to_world(&rig, pos, rot);
            PanelPose::facing(p + Vec3::Y * 0.09, head_world, Vec2::new(0.18, 0.09))
        });
        if wrist.is_some() {
            let health = self.app.hud_health;
            let kills = self.app.displayed_kill_count();
            self.ui
                .paint(device, queue, WRIST, Pointer::default(), |ctx| {
                    wrist_ui(ctx, health, kills);
                });
        }

        let t1 = crate::time_compat::Instant::now();
        self.renderer
            .render_views(&mut self.app, eyes_world, targets, width, height);
        self.ui.draw(
            device,
            queue,
            eyes_world,
            targets,
            [self.menu.map(|m| m.pose), wrist],
        );
        self.last_render_ms = t1.elapsed().as_secs_f32() * 1000.0;
        if let Some(i) = hidden {
            self.app.scene.objects[i].visible = true;
        }

        // 10. Actions du menu, appliquées après l'image.
        let mut quit = false;
        for action in actions {
            match action {
                MenuAction::Resume => self.close_menu(),
                MenuAction::Recenter => {
                    self.rig = None;
                    self.close_menu();
                }
                MenuAction::ToggleView => {
                    self.comfort.view = self.comfort.view.toggled();
                    self.rig = None;
                }
                MenuAction::CycleSnap => {
                    self.comfort.snap_degrees = match self.comfort.snap_degrees as u32 {
                        15 => 30.0,
                        30 => 45.0,
                        _ => 15.0,
                    };
                }
                MenuAction::ToggleVignette => self.comfort.vignette = !self.comfort.vignette,
                MenuAction::Restart => {
                    self.close_menu();
                    self.app.restart_game();
                    self.rig = None;
                }
                MenuAction::Quit => quit = true,
            }
        }

        // 11. Vibrations sur les fronts montants des effets du jeu.
        let fx = (self.app.fx.damage_flash, self.app.fx.attack_flash);
        let haptics = haptics_from_fx(self.prev_fx, fx);
        self.prev_fx = fx;
        FrameOut { haptics, quit }
    }

    /// Ouvre le menu devant le joueur (jeu en pause) ou le referme.
    fn toggle_menu(&mut self, head_world: Vec3, look: Vec3) {
        if self.menu.is_some() {
            self.close_menu();
        } else {
            self.menu = Some(MenuState {
                pose: PanelPose::in_front_of(head_world, look, 1.3, Vec2::new(1.0, 0.75)),
                paused_before: self.app.paused,
            });
            self.app.paused = true;
        }
    }

    fn close_menu(&mut self) {
        if let Some(menu) = self.menu.take() {
            self.app.paused = menu.paused_before;
        }
    }

    /// Rayon de la manette droite vers le menu : position du pointeur en pixels
    /// de texture, gâchette ; dessine le rayon (lignes de debug).
    fn menu_pointer(&mut self, rig: &Rig, input: &XrInput, pose: PanelPose) -> Pointer {
        let right = &input.hands[RIGHT];
        let Some((pos, rot)) = right.aim else {
            return Pointer::default();
        };
        let (origin, rot) = to_world(rig, pos, rot);
        let dir = rot * Vec3::NEG_Z;
        let hit = pose.hit(origin, dir);
        let end = hit.map_or(origin + dir * 3.0, |(_, t)| origin + dir * t);
        let color = if hit.is_some() {
            [0.95, 0.55, 0.15]
        } else {
            [0.6, 0.6, 0.65]
        };
        self.app.debug_lines.push((origin, end, color));
        Pointer {
            pos_px: hit.map(|(uv, _)| uv * self.ui.panel_px(MENU)),
            pressed: right.trigger >= PRESS_THRESHOLD,
        }
    }
}

/// Couleurs de l'écran d'accueil (page web et lobby natif, #12141a / #e8763b).
const BG: egui::Color32 = egui::Color32::from_rgb(18, 20, 26);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(232, 118, 59);
const TEXT: egui::Color32 = egui::Color32::from_rgb(230, 232, 227);

/// Menu de pause VR : gros boutons (visés au rayon, cliqués à la gâchette).
fn menu_ui(root: &mut egui::Ui, comfort: Comfort, actions: &mut Vec<MenuAction>) {
    let frame = egui::Frame::NONE
        .fill(BG)
        .corner_radius(24)
        .stroke(egui::Stroke::new(
            2.0_f32,
            egui::Color32::from_rgb(40, 45, 54),
        ))
        .inner_margin(28);
    egui::CentralPanel::default()
        .frame(frame)
        .show_inside(root, |ui| {
            ui.visuals_mut().override_text_color = Some(TEXT);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Pause").size(30.0).color(ACCENT));
                ui.add_space(12.0);
                let size = egui::vec2(ui.available_width() * 0.9, 44.0);
                let mut button = |label: String, action: MenuAction| {
                    let b = egui::Button::new(egui::RichText::new(label).size(21.0)).min_size(size);
                    if ui.add(b).clicked() {
                        actions.push(action);
                    }
                };
                button("Reprendre".into(), MenuAction::Resume);
                button("Recentrer la vue".into(), MenuAction::Recenter);
                let view = match comfort.view {
                    ViewMode::FirstPerson => "première personne",
                    ViewMode::Spectator => "spectateur",
                };
                button(format!("Vue : {view}"), MenuAction::ToggleView);
                button(
                    format!("Rotation par crans : {:.0}°", comfort.snap_degrees),
                    MenuAction::CycleSnap,
                );
                let vignette = if comfort.vignette { "oui" } else { "non" };
                button(
                    format!("Vignette de confort : {vignette}"),
                    MenuAction::ToggleVignette,
                );
                button("Rejouer la manche".into(), MenuAction::Restart);
                button("Quitter".into(), MenuAction::Quit);
            });
            // Curseur : là où vise la manette.
            if let Some(pos) = ui.ctx().pointer_latest_pos() {
                ui.painter().circle_filled(pos, 7.0, ACCENT);
            }
        });
}

/// Affichage au poignet gauche : vie et score, lisible d'un coup d'œil.
fn wrist_ui(root: &mut egui::Ui, health: Option<f32>, kills: u32) {
    let frame = egui::Frame::NONE
        .fill(BG)
        .corner_radius(16)
        .inner_margin(14);
    egui::CentralPanel::default()
        .frame(frame)
        .show_inside(root, |ui| {
            ui.visuals_mut().override_text_color = Some(TEXT);
            if let Some(h) = health {
                ui.label(
                    egui::RichText::new(format!("Vie {:.0} %", h.clamp(0.0, 1.0) * 100.0))
                        .size(24.0),
                );
                ui.add(egui::ProgressBar::new(h.clamp(0.0, 1.0)).fill(ACCENT));
            }
            ui.label(egui::RichText::new(format!("Ennemis vaincus : {kills}")).size(18.0));
        });
}

/// Pieds du personnage local dans le monde (bas de sa boîte englobante).
fn player_feet(app: &AppState) -> Option<Vec3> {
    let i = app.player_index()?;
    let o = &app.scene.objects[i];
    let (min, _) = app.scene.local_aabb(o.mesh);
    let p = o.transform.position;
    Some(Vec3::new(p.x, p.y + min.y * o.transform.scale.y, p.z))
}

/// Pose de la pièce vers le monde.
fn to_world(rig: &Rig, pos: Vec3, rot: Quat) -> (Vec3, Quat) {
    let r = Quat::from_rotation_y(rig.yaw);
    (rig.origin + r * pos, r * rot)
}

/// Boîte de 8 cm (la manette) et trait de 12 cm vers l'avant (−Z) — dessinés en
/// lignes (`AppState::debug_lines`, rendues par `render_views`).
fn push_controller_lines(lines: &mut Vec<(Vec3, Vec3, [f32; 3])>, pos: Vec3, rot: Quat) {
    const BODY: [f32; 3] = [0.85, 0.88, 0.92];
    const POINTER: [f32; 3] = [0.95, 0.55, 0.15];
    let h = 0.04;
    let c = |x: f32, y: f32, z: f32| pos + rot * Vec3::new(x * h, y * h, z * h);
    for (a, b) in [
        ((-1., -1., -1.), (1., -1., -1.)),
        ((1., -1., -1.), (1., 1., -1.)),
        ((1., 1., -1.), (-1., 1., -1.)),
        ((-1., 1., -1.), (-1., -1., -1.)),
        ((-1., -1., 1.), (1., -1., 1.)),
        ((1., -1., 1.), (1., 1., 1.)),
        ((1., 1., 1.), (-1., 1., 1.)),
        ((-1., 1., 1.), (-1., -1., 1.)),
        ((-1., -1., -1.), (-1., -1., 1.)),
        ((1., -1., -1.), (1., -1., 1.)),
        ((1., 1., -1.), (1., 1., 1.)),
        ((-1., 1., -1.), (-1., 1., 1.)),
    ] {
        lines.push((c(a.0, a.1, a.2), c(b.0, b.1, b.2), BODY));
    }
    lines.push((pos, pos + rot * Vec3::new(0.0, 0.0, -0.12), POINTER));
}
