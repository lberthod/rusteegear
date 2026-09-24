//! **Ball** — jeu VR de lancer, façon mini-golf, pensé pour tenir 90 Hz sur
//! Meta Quest 3 sans le `Renderer` du moteur (`gfx`, rendu dédié).
//!
//! Trois parcours de trois trous (`course`). À chaque trou, faire tomber
//! toutes les cibles (blocs rouge-orangé) en le moins de lancers possible :
//! chaque lancer est un coup, comparé au par comme au golf ; un parcours fini
//! donne 1 à 3 étoiles et un record gardé sur le casque.
//!
//! Mains (suivi OpenXR) ou manettes : fermer la main fait naître une boule
//! (ou attrape la plus proche), l'ouvrir en lançant la projette (`hands`). À
//! gauche du repère, une console : bouton rouge = recommencer le trou, bleu =
//! menu (ou bouton menu de la manette gauche). Les menus se visent au rayon de
//! la main droite, pincement ou gâchette pour cliquer.
//!
//! On ne se déplace jamais au stick : entre deux trous, un fondu au noir
//! transporte le joueur (aucun inconfort).

mod course;
mod gfx;
mod hands;
mod world;

use glam::{Quat, Vec2, Vec3, Vec4};

use course::{BUTTON_RADIUS, COURSES, MENU_BUTTON, RESET_BUTTON, hole_origin};
use gfx::{Gfx, Instance};
use hands::HandFrame;
use world::{Material, World};

use crate::time_compat::Instant;
use crate::xr::input::{Haptic, LEFT, RIGHT, XrInput};
use crate::xr::math::EyeView;
use crate::xr::ui::{MENU, PanelPose, Pointer, VrUi, WRIST};

/// Temps d'affichage du « trou réussi » avant de passer au suivant.
const HOLE_DONE_SECONDS: f32 = 2.5;
/// Durée d'un demi-fondu (noir → image).
const FADE_SECONDS: f32 = 0.3;
/// Centre du panneau des menus, par rapport au repère du joueur.
const MENU_AT: Vec3 = Vec3::new(-0.6, 1.45, -1.5);

const BG: egui::Color32 = egui::Color32::from_rgb(18, 22, 30);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(255, 170, 60);
const TEXT: egui::Color32 = egui::Color32::from_rgb(236, 238, 232);
const MUTED: egui::Color32 = egui::Color32::from_rgb(160, 168, 176);

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// Menu des parcours, avec une pyramide d'entraînement.
    Lobby,
    Playing,
    /// Toutes les cibles sont tombées ; passage au trou suivant à `until`.
    HoleDone { until: f32 },
    /// Parcours fini : bilan.
    CourseDone,
}

/// Où aller une fois l'écran au noir.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Goto {
    Lobby,
    Hole(usize),
}

/// Actions des menus.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Start(usize),
    Resume,
    RestartHole,
    ToLobby,
}

pub struct BallGame {
    world: World,
    gfx: Gfx,
    ui: VrUi,
    state: State,
    course: usize,
    hole: usize,
    /// Coups par trou du parcours en cours.
    strokes: [u32; 3],
    /// Premier bloc du trou en cours (`world.blocks[first..]`).
    first_block: usize,
    /// Position du repère du joueur dans le monde (origine de la pièce).
    offset: Vec3,
    /// Fondu : 1 = image, 0 = noir ; `goto` = destination en cours de fondu.
    fade: f32,
    goto: Option<Goto>,
    pause: bool,
    best: [Option<u32>; 3],
    /// Record battu au dernier parcours fini.
    new_record: bool,
    clock: f32,
    last: Option<Instant>,
    focused: bool,
    menu_btn_was: bool,
    button_cooldown: f32,
    /// Contenu peint dans chaque panneau (on ne repeint que s'il change), et
    /// images de repeinte restantes : egui ne pose le texte qu'à partir de
    /// la 2e image d'un contenu (atlas de police), une seule peinture
    /// laisserait le panneau vide.
    painted: [String; 2],
    repaint: [u8; 2],
    lobby_cleared_at: Option<f32>,
}

impl BallGame {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let mut g = Self {
            world: World::new(),
            gfx: Gfx::new(device, format, width, height),
            ui: VrUi::new(device, format),
            state: State::Lobby,
            course: 0,
            hole: 0,
            strokes: [0; 3],
            first_block: 0,
            offset: Vec3::ZERO,
            fade: 0.0,
            goto: None,
            pause: false,
            best: load_best(),
            new_record: false,
            clock: 0.0,
            last: None,
            focused: true,
            menu_btn_was: false,
            button_cooldown: 0.0,
            painted: Default::default(),
            repaint: [0; 2],
            lobby_cleared_at: None,
        };
        g.enter_lobby();
        g
    }

    /// Sans focus (menu système Meta, casque retiré) : tout est figé.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.last = None;
    }

    /// Démarre directement un trou (simulateur, tests).
    pub fn start_at(&mut self, course: usize, hole: usize) {
        self.start_course(course.min(2));
        for _ in 0..hole.min(2) {
            self.go_to_hole(self.hole + 1);
        }
        self.fade = 1.0;
    }

    fn enter_lobby(&mut self) {
        self.world.clear();
        self.world.add_static(
            Vec3::new(0.0, -0.05, 0.0),
            Vec3::new(30.0, 0.05, 30.0),
            Vec4::new(0.42, 0.62, 0.36, 2.0),
        );
        self.world.add_decor(
            Vec3::Y * 0.003,
            Vec3::new(0.35, 0.003, 0.35),
            Vec4::new(0.95, 0.85, 0.55, 0.5),
        );
        self.world.add_static(
            Vec3::new(0.7, 0.4, -1.6),
            Vec3::new(0.35, 0.4, 0.3),
            Vec4::new(0.36, 0.3, 0.26, 1.0),
        );
        self.populate_lobby();
        self.offset = Vec3::ZERO;
        self.state = State::Lobby;
        self.pause = false;
    }

    /// Pyramide d'entraînement du menu (rien ne compte).
    fn populate_lobby(&mut self) {
        self.world.clear_dynamic();
        self.first_block = self.world.blocks.len();
        let half = 0.06;
        for row in 0..3 {
            for i in 0..(3 - row) {
                let x = 0.7 + (i as f32 - (2 - row) as f32 * 0.5) * (half * 2.0 + 0.004);
                let y = 0.8 + half + 0.002 + row as f32 * (half * 2.0 + 0.001);
                self.world
                    .add_block(Vec3::new(x, y, -1.6), Vec3::splat(half), Material::Target);
            }
        }
        self.lobby_cleared_at = None;
    }

    fn start_course(&mut self, course: usize) {
        self.world.clear();
        self.course = course;
        self.strokes = [0; 3];
        self.new_record = false;
        course::build_statics(course, &mut self.world);
        self.hole = 0;
        self.first_block = course::populate(course, 0, &mut self.world);
        self.offset = hole_origin(0);
        self.state = State::Playing;
        self.pause = false;
    }

    fn go_to_hole(&mut self, hole: usize) {
        self.world.clear_dynamic();
        self.hole = hole;
        self.first_block = course::populate(self.course, hole, &mut self.world);
        self.offset = hole_origin(hole);
        self.state = State::Playing;
    }

    fn restart_hole(&mut self) {
        self.world.clear_dynamic();
        self.first_block = course::populate(self.course, self.hole, &mut self.world);
        self.state = State::Playing;
    }

    /// Cibles du trou en cours : (tombées, total).
    fn targets(&self) -> (usize, usize) {
        let targets = self.world.blocks[self.first_block..]
            .iter()
            .filter(|b| b.material == Material::Target);
        let total = targets.clone().count();
        let down = targets.filter(|b| self.world.is_down(b)).count();
        (down, total)
    }

    fn total_strokes(&self) -> u32 {
        self.strokes.iter().sum()
    }

    /// Écart au par des trous déjà joués (et du trou en cours s'il est fini).
    fn to_par(&self) -> i32 {
        let played = match self.state {
            State::Playing => self.hole,
            _ => self.hole + 1,
        };
        let par: u32 = COURSES[self.course].holes[..played.min(3)]
            .iter()
            .map(|h| h.par)
            .sum();
        let strokes: u32 = self.strokes[..played.min(3)].iter().sum();
        strokes as i32 - par as i32
    }

    fn finish_course(&mut self) {
        let total = self.total_strokes();
        self.new_record = self.best[self.course].is_none_or(|b| total < b);
        if self.new_record {
            self.best[self.course] = Some(total);
            save_best(&self.best);
        }
        self.state = State::CourseDone;
    }

    /// Une image : entrées, jeu, physique, rendu. Renvoie les vibrations.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: [EyeView; 2],
        input: &XrInput,
        targets: [&wgpu::TextureView; 2],
    ) -> [Option<Haptic>; 2] {
        let now = Instant::now();
        let dt = self
            .last
            .map_or(1.0 / 90.0, |t| now.duration_since(t).as_secs_f32())
            .min(0.1);
        self.last = Some(now);
        let mut haptics = [None, None];
        let mut actions = Vec::new();
        let eyes_world = eyes.map(|e| EyeView {
            position: e.position + self.offset,
            ..e
        });
        let head = (eyes_world[0].position + eyes_world[1].position) * 0.5;
        let look = eyes[0].orientation * Vec3::NEG_Z;

        if self.focused {
            haptics = self.update(dt, input);
        }

        // Interface : panneau principal (menu, tableau des scores, bilan) et
        // bandeau (trou réussi).
        let menu = self.menu_pose(head, look);
        let interactive = menu.is_some_and(|(_, i)| i);
        if let Some((pose, _)) = menu {
            let pointer = if interactive {
                self.pointer(input, pose)
            } else {
                Pointer::default()
            };
            let key = self.menu_key();
            if key != self.painted[MENU] {
                self.painted[MENU] = key;
                self.repaint[MENU] = 3;
            }
            if interactive || self.repaint[MENU] > 0 {
                self.repaint[MENU] = self.repaint[MENU].saturating_sub(1);
                let view = MenuView::of(self);
                self.ui.paint(device, queue, MENU, pointer, |ui| {
                    menu_ui(ui, &view, &mut actions);
                });
            }
        }
        let banner = matches!(self.state, State::HoleDone { .. }).then(|| {
            let o = self.offset;
            PanelPose::facing(o + Vec3::new(0.0, 1.95, -1.6), head, Vec2::new(0.8, 0.4))
        });
        if banner.is_some() {
            let key = self.banner_key();
            if key != self.painted[WRIST] {
                self.painted[WRIST] = key;
                self.repaint[WRIST] = 3;
            }
            if self.repaint[WRIST] > 0 {
                self.repaint[WRIST] -= 1;
                let text = self.painted[WRIST].clone();
                self.ui.paint(device, queue, WRIST, Pointer::default(), |ui| {
                    banner_ui(ui, &text);
                });
            }
        }

        let (cubes, spheres) = self.instances(input);
        self.gfx.draw(
            device,
            queue,
            eyes_world,
            targets,
            &cubes,
            &spheres,
            self.offset + Vec3::new(0.0, 0.0, -2.5),
            self.fade,
        );
        if self.fade > 0.5 {
            self.ui
                .draw(device, queue, eyes_world, targets, [menu.map(|m| m.0), banner]);
        }

        for a in actions {
            self.apply(a);
        }
        haptics
    }

    fn update(&mut self, dt: f32, input: &XrInput) -> [Option<Haptic>; 2] {
        self.clock += dt;
        let mut haptics = [None, None];

        // Fondu entre deux lieux : au noir, on change de lieu.
        match self.goto {
            Some(goto) => {
                self.fade = (self.fade - dt / FADE_SECONDS).max(0.0);
                if self.fade == 0.0 {
                    self.goto = None;
                    match goto {
                        Goto::Lobby => self.enter_lobby(),
                        Goto::Hole(h) => self.go_to_hole(h),
                    }
                }
            }
            None => self.fade = (self.fade + dt / FADE_SECONDS).min(1.0),
        }

        // Bouton menu de la manette gauche : pause.
        let menu_btn = input.hands[LEFT].menu;
        if menu_btn && !self.menu_btn_was && self.state == State::Playing {
            self.pause = !self.pause;
        }
        self.menu_btn_was = menu_btn;

        let frames = [LEFT, RIGHT].map(|h| HandFrame::from_input(input, h, self.offset));
        // Un menu interactif ouvert : le pincement y clique, pas de boule.
        // Au menu d'accueil, on peut encore jouer avec la pyramide
        // d'entraînement : seule la main droite qui vise le panneau clique.
        let menu_open = self.pause || self.state == State::CourseDone;
        for hand in [LEFT, RIGHT] {
            let can_grab = !menu_open
                && self.goto.is_none()
                && !(hand == RIGHT && self.pointing_at_menu(input));
            let (h, stroke) = self.world.update_hand(hand, frames[hand], can_grab);
            haptics[hand] = h;
            if stroke && self.state == State::Playing && !self.pause {
                self.strokes[self.hole] += 1;
            }
        }

        // Console : rouge = recommencer le trou, bleu = menu.
        self.button_cooldown = (self.button_cooldown - dt).max(0.0);
        if self.state == State::Playing && self.button_cooldown == 0.0 {
            let touching = |b: Vec3| {
                frames.iter().flatten().any(|f| {
                    f.contacts
                        .iter()
                        .any(|(p, r)| p.distance(self.offset + b) < BUTTON_RADIUS + r)
                })
            };
            if touching(RESET_BUTTON) {
                self.restart_hole();
                self.button_cooldown = 1.0;
                haptics = [Some(Haptic {
                    amplitude: 0.5,
                    seconds: 0.06,
                }); 2];
            } else if touching(MENU_BUTTON) {
                self.pause = !self.pause;
                self.button_cooldown = 1.0;
            }
        }

        if !self.pause {
            self.world.step(dt);
        }

        // Règles.
        match self.state {
            State::Playing if !self.pause => {
                let (down, total) = self.targets();
                if total > 0 && down == total {
                    self.state = State::HoleDone {
                        until: self.clock + HOLE_DONE_SECONDS,
                    };
                    haptics = [Some(Haptic {
                        amplitude: 0.7,
                        seconds: 0.2,
                    }); 2];
                }
            }
            State::HoleDone { until } if self.clock >= until && self.goto.is_none() => {
                if self.hole + 1 < 3 {
                    self.goto = Some(Goto::Hole(self.hole + 1));
                } else {
                    self.finish_course();
                }
            }
            State::Lobby => {
                let (down, total) = self.targets();
                if total > 0 && down == total {
                    let since = *self.lobby_cleared_at.get_or_insert(self.clock);
                    if self.clock - since > 2.0 {
                        self.populate_lobby();
                    }
                }
            }
            _ => {}
        }
        haptics
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Start(course) => {
                self.start_course(course);
                self.fade = 0.0;
            }
            Action::Resume => self.pause = false,
            Action::RestartHole => {
                self.pause = false;
                self.restart_hole();
            }
            Action::ToLobby => {
                self.pause = false;
                self.goto = Some(Goto::Lobby);
            }
        }
    }

    /// Rayon de visée des menus : main droite (ou gauche à défaut), dans le
    /// monde, et bouton de clic.
    fn aim(&self, input: &XrInput) -> Option<(Vec3, Vec3, bool)> {
        for hand in [RIGHT, LEFT] {
            let h = &input.hands[hand];
            if let Some((pos, rot)) = h.aim {
                return Some((pos + self.offset, rot * Vec3::NEG_Z, h.trigger > 0.6));
            }
            if let Some(j) = &input.hand_joints[hand] {
                let (o, d) = crate::xr::hands::aim_ray(j);
                let pinch = crate::xr::hands::pinch_distance(j) < 0.022;
                return Some((o + self.offset, d, pinch));
            }
        }
        None
    }

    fn pointing_at_menu(&self, input: &XrInput) -> bool {
        let Some((pose, true)) = self.menu_pose_cached() else {
            return false;
        };
        self.aim(input)
            .is_some_and(|(o, d, _)| pose.hit(o, d).is_some())
    }

    fn pointer(&self, input: &XrInput, pose: PanelPose) -> Pointer {
        let Some((o, d, pressed)) = self.aim(input) else {
            return Pointer::default();
        };
        Pointer {
            pos_px: pose.hit(o, d).map(|(uv, _)| uv * self.ui.panel_px(MENU)),
            pressed,
        }
    }

    /// Panneau principal : où, et s'il est interactif.
    fn menu_pose(&self, head: Vec3, look: Vec3) -> Option<(PanelPose, bool)> {
        let o = self.offset;
        let size = Vec2::new(1.2, 0.9);
        match self.state {
            _ if self.pause => Some((
                PanelPose::in_front_of(head, Vec3::new(look.x, 0.0, look.z), 1.2, size),
                true,
            )),
            State::Lobby | State::CourseDone => {
                Some((PanelPose::facing(o + MENU_AT, head, size), true))
            }
            State::Playing | State::HoleDone { .. } => Some((
                PanelPose::facing(o + Vec3::new(0.95, 1.9, -1.9), head, Vec2::new(0.64, 0.48)),
                false,
            )),
        }
    }

    /// Même panneau, sans dépendre de la tête (tests de visée).
    fn menu_pose_cached(&self) -> Option<(PanelPose, bool)> {
        match self.state {
            State::Lobby | State::CourseDone if !self.pause => Some((
                PanelPose::facing(
                    self.offset + MENU_AT,
                    self.offset + Vec3::new(0.0, 1.6, 0.0),
                    Vec2::new(1.2, 0.9),
                ),
                true,
            )),
            _ => None,
        }
    }

    fn menu_key(&self) -> String {
        format!(
            "{:?}|{}|{}|{:?}|{}|{:?}|{}",
            self.state,
            self.course,
            self.hole,
            self.strokes,
            self.pause,
            self.best,
            self.targets().0
        )
    }

    fn banner_key(&self) -> String {
        let par = COURSES[self.course].holes[self.hole].par;
        let s = self.strokes[self.hole];
        let verdict = match s as i32 - par as i32 {
            i32::MIN..=-2 => "Magnifique !",
            -1 => "Birdie !",
            0 => "Par !",
            1 => "Bogey",
            _ => "Trou réussi",
        };
        format!("{verdict}\n{s} coup{} · par {par}", if s > 1 { "s" } else { "" })
    }

    fn instances(&self, input: &XrInput) -> (Vec<Instance>, Vec<Instance>) {
        let w = &self.world;
        let mut cubes = Vec::with_capacity(160);
        let mut spheres = Vec::with_capacity(96);
        for &(c, half, color) in &w.statics {
            cubes.push(Instance::boxed(c, half, Quat::IDENTITY, color));
        }
        for m in &w.movers {
            cubes.push(Instance::boxed(
                w.position(m.body),
                m.half,
                w.rotation(m.body),
                m.color,
            ));
        }
        for (i, b) in w.blocks.iter().enumerate() {
            let color = if b.material == Material::Target && w.is_down(b) {
                // Cible tombée : éteinte (grise), ce qui reste saute aux yeux.
                Vec4::new(0.42, 0.43, 0.42, 1.0)
            } else {
                b.color
            };
            let _ = i;
            cubes.push(Instance::boxed(
                w.position(b.body),
                b.half,
                w.rotation(b.body),
                color,
            ));
        }
        for &ball in &w.balls {
            spheres.push(Instance::boxed(
                w.position(ball),
                Vec3::splat(world::BALL_RADIUS),
                w.rotation(ball),
                Vec4::new(0.95, 0.95, 0.98, 1.0),
            ));
        }
        // Mains et manettes.
        for hand in [LEFT, RIGHT] {
            let color = if w.grabbers[hand].closed() {
                Vec4::new(1.0, 0.62, 0.2, 0.5)
            } else {
                Vec4::new(0.88, 0.9, 0.96, 1.0)
            };
            if let Some(j) = &input.hand_joints[hand] {
                for (k, p) in j.iter().enumerate() {
                    let r = if k == 0 { 0.013 } else { 0.008 };
                    spheres.push(Instance::boxed(
                        *p + self.offset,
                        Vec3::splat(r),
                        Quat::IDENTITY,
                        color,
                    ));
                }
            } else if let Some((pos, rot)) = input.hands[hand].grip {
                // Manette : petit boîtier sombre, orange quand il tient.
                let color = if w.grabbers[hand].closed() {
                    color
                } else {
                    Vec4::new(0.25, 0.27, 0.3, 1.0)
                };
                cubes.push(Instance::boxed(
                    pos + self.offset,
                    Vec3::new(0.018, 0.015, 0.035),
                    rot,
                    color,
                ));
            }
        }
        // Rayon de visée quand un menu interactif est ouvert.
        if (self.pause || matches!(self.state, State::Lobby | State::CourseDone))
            && let Some((o, d, _)) = self.aim(input)
        {
            let len = 1.6;
            cubes.push(Instance::boxed(
                o + d * len * 0.5,
                Vec3::new(0.002, 0.002, len * 0.5),
                Quat::from_rotation_arc(Vec3::NEG_Z, d),
                Vec4::new(1.0, 0.75, 0.35, 0.5),
            ));
        }
        (cubes, spheres)
    }
}

/// Ce que le panneau principal affiche (copié hors de `BallGame` pour le
/// peindre pendant que l'interface est empruntée).
struct MenuView {
    state: State,
    pause: bool,
    course: usize,
    hole: usize,
    strokes: [u32; 3],
    to_par: i32,
    targets: (usize, usize),
    best: [Option<u32>; 3],
    new_record: bool,
}

impl MenuView {
    fn of(g: &BallGame) -> Self {
        Self {
            state: g.state,
            pause: g.pause,
            course: g.course,
            hole: g.hole,
            strokes: g.strokes,
            to_par: g.to_par(),
            targets: g.targets(),
            best: g.best,
            new_record: g.new_record,
        }
    }
}

fn big_button(ui: &mut egui::Ui, title: &str, sub: &str, height: f32) -> bool {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        title,
        0.0,
        egui::TextFormat::simple(egui::FontId::proportional(22.0), TEXT),
    );
    if !sub.is_empty() {
        job.append(
            &format!("\n{sub}"),
            0.0,
            egui::TextFormat::simple(egui::FontId::proportional(15.0), MUTED),
        );
    }
    let size = egui::vec2(ui.available_width(), height);
    ui.add(egui::Button::new(job).min_size(size)).clicked()
}

fn stars_text(n: u32) -> String {
    "★".repeat(n as usize) + &"☆".repeat(3 - n as usize)
}

fn signed(n: i32) -> String {
    match n {
        0 => "par".into(),
        n if n > 0 => format!("+{n}"),
        n => format!("{n}"),
    }
}

fn menu_ui(root: &mut egui::Ui, v: &MenuView, actions: &mut Vec<Action>) {
    let frame = egui::Frame::NONE
        .fill(BG)
        .corner_radius(26)
        .stroke(egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(44, 52, 64)))
        .inner_margin(24);
    egui::CentralPanel::default()
        .frame(frame)
        .show_inside(root, |ui| {
            ui.visuals_mut().override_text_color = Some(TEXT);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.vertical_centered(|ui| {
                if v.pause {
                    ui.label(egui::RichText::new("Pause").size(30.0).color(ACCENT));
                    ui.add_space(8.0);
                    if big_button(ui, "Reprendre", "", 52.0) {
                        actions.push(Action::Resume);
                    }
                    if big_button(ui, "Recommencer le trou", "les coups déjà joués restent", 60.0) {
                        actions.push(Action::RestartHole);
                    }
                    if big_button(ui, "Menu des parcours", "abandonner ce parcours", 60.0) {
                        actions.push(Action::ToLobby);
                    }
                } else {
                    match v.state {
                        State::Lobby => lobby_ui(ui, v, actions),
                        State::CourseDone => summary_ui(ui, v, actions),
                        State::Playing | State::HoleDone { .. } => scoreboard_ui(ui, v),
                    }
                }
            });
            if (v.pause || matches!(v.state, State::Lobby | State::CourseDone))
                && let Some(pos) = ui.ctx().pointer_latest_pos()
            {
                ui.painter().circle_filled(pos, 7.0, ACCENT);
            }
        });
}

fn lobby_ui(ui: &mut egui::Ui, v: &MenuView, actions: &mut Vec<Action>) {
    ui.label(egui::RichText::new("BALL").size(38.0).strong().color(ACCENT));
    ui.label(
        egui::RichText::new("Ferme la main : une balle apparaît. Ouvre-la en lançant.")
            .size(15.0)
            .color(MUTED),
    );
    ui.add_space(6.0);
    for (i, c) in COURSES.iter().enumerate() {
        let par = course::course_par(i);
        let best = v.best[i].map_or_else(
            || format!("{} · par {par}", c.blurb),
            |b| format!("Record {b} coups (par {par}) {}", stars_text(course::stars(b, par))),
        );
        if big_button(ui, &format!("{}. {}", i + 1, c.name), &best, 64.0) {
            actions.push(Action::Start(i));
        }
    }
    ui.label(
        egui::RichText::new("Vise avec la main droite, pince pour choisir.")
            .size(14.0)
            .color(MUTED),
    );
}

fn scoreboard_ui(ui: &mut egui::Ui, v: &MenuView) {
    let c = &COURSES[v.course];
    let h = &c.holes[v.hole];
    // Panneau lu à ~2 m : gros caractères.
    ui.label(
        egui::RichText::new(format!("{} · trou {}/3", c.name, v.hole + 1))
            .size(38.0)
            .color(MUTED),
    );
    ui.label(egui::RichText::new(h.name).size(54.0).strong().color(ACCENT));
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(format!("Coups {}  ·  Par {}", v.strokes[v.hole], h.par))
            .size(62.0)
            .strong(),
    );
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(format!(
            "Cibles {}/{}  ·  Total {}",
            v.targets.0,
            v.targets.1,
            signed(v.to_par)
        ))
        .size(40.0)
        .color(MUTED),
    );
}

fn summary_ui(ui: &mut egui::Ui, v: &MenuView, actions: &mut Vec<Action>) {
    let c = &COURSES[v.course];
    let par = course::course_par(v.course);
    let total: u32 = v.strokes.iter().sum();
    ui.label(egui::RichText::new(format!("{} terminé", c.name)).size(28.0).color(ACCENT));
    ui.label(
        egui::RichText::new(stars_text(course::stars(total, par)))
            .size(44.0)
            .color(ACCENT),
    );
    ui.label(egui::RichText::new(format!("{total} coups · par {par} · {}", signed(total as i32 - par as i32))).size(24.0));
    let holes: Vec<String> = v.strokes.iter().map(u32::to_string).collect();
    ui.label(
        egui::RichText::new(format!("Trous : {}", holes.join(" / ")))
            .size(16.0)
            .color(MUTED),
    );
    if v.new_record {
        ui.label(egui::RichText::new("Nouveau record !").size(20.0).color(ACCENT));
    }
    ui.add_space(4.0);
    if big_button(ui, "Rejouer ce parcours", "", 46.0) {
        actions.push(Action::Start(v.course));
    }
    if v.course + 1 < COURSES.len()
        && big_button(ui, &format!("Parcours suivant : {}", COURSES[v.course + 1].name), "", 46.0)
    {
        actions.push(Action::Start(v.course + 1));
    }
    if big_button(ui, "Menu des parcours", "", 46.0) {
        actions.push(Action::ToLobby);
    }
}

fn banner_ui(root: &mut egui::Ui, text: &str) {
    let frame = egui::Frame::NONE
        .fill(BG)
        .corner_radius(20)
        .inner_margin(16);
    egui::CentralPanel::default()
        .frame(frame)
        .show_inside(root, |ui| {
            ui.vertical_centered(|ui| {
                let (title, sub) = text.split_once('\n').unwrap_or((text, ""));
                ui.label(egui::RichText::new(title).size(40.0).strong().color(ACCENT));
                ui.label(egui::RichText::new(sub).size(26.0).color(TEXT));
            });
        });
}

fn best_path() -> Option<std::path::PathBuf> {
    Some(crate::assets::user_dir()?.join("ball_best.json"))
}

fn load_best() -> [Option<u32>; 3] {
    best_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<[Option<u32>; 3]>(&s).ok())
        .unwrap_or_default()
}

fn save_best(best: &[Option<u32>; 3]) {
    let Some(path) = best_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(best) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::course::{COURSES, populate, stars};
    use super::world::{Material, World};

    /// Chaque trou tient debout seul (éléments mobiles compris) pendant 4 s.
    #[test]
    fn every_hole_stands_still_until_hit() {
        for course in 0..3 {
            for hole in 0..3 {
                let mut w = World::new();
                super::course::build_statics(course, &mut w);
                // En jeu, un trou démarre à un instant quelconque.
                w.time = 7.3;
                let first = populate(course, hole, &mut w);
                for _ in 0..(4 * 90) {
                    w.step(1.0 / 90.0);
                }
                let targets: Vec<_> = w.blocks[first..]
                    .iter()
                    .filter(|b| b.material == Material::Target)
                    .collect();
                assert!(!targets.is_empty(), "parcours {course} trou {hole} sans cible");
                let down = targets.iter().filter(|b| w.is_down(b)).count();
                assert_eq!(
                    down,
                    0,
                    "{} / {} : {down} cible(s) tombée(s) toute(s) seule(s)",
                    COURSES[course].name,
                    COURSES[course].holes[hole].name
                );
            }
        }
    }

    /// Un vrai lancer (manette : poignée serrée, bras vers l'avant, poignée
    /// relâchée) fait tomber une cible du premier trou, et compte un coup.
    #[test]
    fn a_throw_knocks_targets_off_the_first_hole() {
        use super::hands::HandFrame;
        use glam::Vec3;
        let mut w = World::new();
        super::course::build_statics(0, &mut w);
        let first = populate(0, 0, &mut w);
        let frame = |p: Vec3, press: f32| HandFrame {
            anchor: p,
            pinch_point: p,
            grip_point: p,
            pinch: 1.0,
            fist: 1.0,
            press: Some(press),
            contacts: [(Vec3::new(0.0, -100.0, 0.0), 0.01); 6],
        };
        let dt = 1.0 / 90.0;
        let mut p = Vec3::new(0.0, 1.05, -0.1);
        let mut strokes = 0;
        for i in 0..14 {
            // Bras vers la pyramide (1,8 m devant, dessus à 0,8 m), 5 m/s.
            p += Vec3::new(0.0, -0.4, -5.0) * dt;
            let press = if i < 13 { 1.0 } else { 0.0 };
            let (_, stroke) = w.update_hand(1, Some(frame(p, press)), true);
            strokes += u32::from(stroke);
            w.step(dt);
        }
        for _ in 0..(3 * 90) {
            w.update_hand(1, None, true);
            w.step(dt);
        }
        let down = w.blocks[first..]
            .iter()
            .filter(|b| b.material == Material::Target && w.is_down(b))
            .count();
        assert_eq!(strokes, 1, "un lâcher = un coup");
        // Une cible qui retombe sur la table ne compte pas : il faut la faire
        // tomber du socle (règle du jeu), un seul lancer n'y suffit pas toujours.
        assert!(down >= 1, "{down} cible(s) tombée(s)");
    }

    #[test]
    fn stars_follow_the_par() {
        assert_eq!(stars(10, 11), 3);
        assert_eq!(stars(13, 11), 2);
        assert_eq!(stars(20, 11), 1);
    }
}
