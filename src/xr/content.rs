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
use crate::app::race::RaceInput;
use crate::gfx::renderer::Renderer;

/// Scène VR choisie au lancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneChoice {
    Cubes,
    Riviere,
    /// La scène du projet embarquée par le panneau Export
    /// (`assets/player_scene.json`, cf. `Scene::embedded_player`).
    Embedded,
    /// La démo de rééducation Mouvéo (phase 8 : mains suivies en VR).
    Reeducation,
    /// Le hameau MMORPG (scène embarquée par défaut du moteur).
    Hameau,
    /// HerRoad, la course façon Trackmania : conduite depuis le cockpit.
    HerRoad,
    /// RageQuit, le plateformer 2D : joué comme une maquette posée devant soi.
    RageQuit,
    /// Sélecteur de niveaux : Rivière en fond, menu « Choisir un niveau » ouvert.
    Launcher,
    /// « Balles & cubes » : bac à sable léger aux mains (`xr::balls`), sans le
    /// `Renderer` du moteur — tient la fréquence du casque.
    Balls,
}

/// Niveaux proposés par le menu « Choisir un niveau », dans l'ordre affiché.
pub const LEVELS: [SceneChoice; 5] = [
    SceneChoice::Riviere,
    SceneChoice::Hameau,
    SceneChoice::HerRoad,
    SceneChoice::RageQuit,
    SceneChoice::Reeducation,
];

/// Comment une scène se joue en VR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    /// Un personnage à incarner (première personne ou spectateur derrière lui).
    Character,
    /// Pas de personnage : le joueur se tient à la place de la caméra (Mouvéo).
    Rehab,
    /// Assis dans la voiture (HerRoad) : le monde suit la voiture.
    Vehicle,
    /// Plateformer 2D vu de côté, comme une maquette à quelques mètres.
    Diorama,
}

impl SceneChoice {
    /// `"cubes"`, `"embedded"` (export depuis l'éditeur), `"reeduc"`,
    /// `"riviere"`, `"hameau"`, `"herroad"`, `"ragequit"` ; tout le reste →
    /// `Launcher` (le sélecteur de niveaux, défaut de l'APK).
    pub fn parse(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "cubes" => Self::Cubes,
            "embedded" => Self::Embedded,
            "reeduc" => Self::Reeducation,
            "riviere" | "water" => Self::Riviere,
            "hameau" => Self::Hameau,
            "herroad" => Self::HerRoad,
            "ragequit" => Self::RageQuit,
            "balles" | "balls" => Self::Balls,
            _ => Self::Launcher,
        }
    }

    /// Nom affiché dans le menu des niveaux.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cubes => "Cubes de test",
            Self::Riviere | Self::Launcher => "Rivière",
            Self::Embedded => "Mon jeu",
            Self::Reeducation => "Rééducation Mouvéo",
            Self::Hameau => "Hameau",
            Self::HerRoad => "HerRoad · course",
            Self::RageQuit => "RageQuit · plateformer",
            Self::Balls => "Balles & cubes",
        }
    }

    /// Une ligne d'explication sous le nom, dans le menu des niveaux.
    fn blurb(self) -> &'static str {
        match self {
            Self::Riviere | Self::Launcher => "Vallée, cascade et créatures",
            Self::Hameau => "Le village fortifié du MMORPG",
            Self::HerRoad => "Gâchette droite = gaz, gauche = frein",
            Self::RageQuit => "Stick gauche + A pour sauter",
            Self::Reeducation => "Mouvements des bras et des mains",
            Self::Cubes | Self::Embedded | Self::Balls => "",
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Reeducation => Style::Rehab,
            Self::HerRoad => Style::Vehicle,
            Self::RageQuit => Style::Diorama,
            _ => Style::Character,
        }
    }

    /// Une partie de ce niveau, prête à jouer.
    fn load(self) -> AppState {
        let mut app = AppState::default();
        match self {
            Self::Embedded | Self::Hameau => app.load_embedded_player_scene(),
            Self::Reeducation => app.load_reeducation_demo(),
            Self::HerRoad => app.load_herroad_demo(),
            Self::RageQuit => app.load_scene(ragequit_scene()),
            Self::Riviere | Self::Launcher | Self::Cubes | Self::Balls => app.load_riviere_demo(),
        }
        app.playing = true;
        app
    }
}

/// La scène de RageQuit (`../ragequit/scenes/main.scene.json`, générée par
/// son `build_scene.py`), embarquée compressée : `assets/vr/ragequit.scene.json.zst`.
fn ragequit_scene() -> crate::scene::Scene {
    use std::io::Read;
    const ZST: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/vr/ragequit.scene.json.zst"
    ));
    let mut json = String::new();
    let parsed = ruzstd::decoding::StreamingDecoder::new(ZST)
        .map_err(|e| e.to_string())
        .and_then(|mut d| d.read_to_string(&mut json).map_err(|e| e.to_string()))
        .and_then(|_| {
            serde_json::from_str::<crate::scene::Scene>(&json).map_err(|e| e.to_string())
        });
    match parsed {
        Ok(mut scene) => {
            scene.reload_imported();
            scene
        }
        Err(e) => {
            log::error!("RageQuit : scène embarquée illisible ({e}) — Rivière à la place.");
            crate::scene::Scene::riviere_demo()
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
    /// Page « Choisir un niveau » plutôt que la pause.
    levels: bool,
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
    /// Rééducation : lance la séance (bouton « Démarrer » de la page web).
    StartSession,
    /// Ouvre la page « Choisir un niveau ».
    Levels,
    /// Revient de la page des niveaux à la pause.
    Back,
    /// Charge ce niveau.
    Load(SceneChoice),
    Quit,
}

pub enum XrContent {
    Cubes(CubeScene),
    Balls(Box<super::balls::BallScene>),
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
    /// Niveau en cours et façon de le jouer.
    choice: SceneChoice,
    style: Style,
    /// Voiture (HerRoad) : position de la tête dans la pièce au dernier
    /// recentrage — le siège y est ancré, assis comme debout.
    seat_head: Option<Vec3>,
    /// Sélecteur de niveaux : ouvre la page des niveaux dès que le rig est posé.
    open_levels: bool,
    /// Début du pincement gauche en cours (mode mains : un pincement tenu
    /// `MENU_PINCH_SECONDS` ouvre/ferme le menu, faute de bouton menu).
    left_pinch_since: Option<crate::time_compat::Instant>,
    focus_pause: FocusPause,
    /// Instant de l'image précédente (lissages de la voiture et de la maquette).
    last_frame: Option<crate::time_compat::Instant>,
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
            SceneChoice::Balls => Self::Balls(Box::new(super::balls::BallScene::new(
                device, format, width, height,
            ))),
            _ => {
                let renderer = Renderer::new_external(
                    adapter,
                    device.clone(),
                    queue.clone(),
                    format,
                    width,
                    height,
                );
                let mut game = GameView {
                    app: AppState::default(),
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
                    choice,
                    style: choice.style(),
                    seat_head: None,
                    open_levels: choice == SceneChoice::Launcher,
                    left_pinch_since: None,
                    focus_pause: FocusPause::default(),
                    last_frame: None,
                };
                game.load_level(choice);
                Self::Game(Box::new(game))
            }
        }
    }

    /// Remplace la partie en cours par ce niveau (menu « Choisir un niveau »,
    /// ou `QUEST_SIM_SWITCH` du simulateur). Sans effet sur la scène de cubes.
    pub fn load_level(&mut self, choice: SceneChoice) {
        if let Self::Game(game) = self {
            game.load_level(choice);
        }
    }

    /// Le casque a (`true`) ou n'a plus le focus : menu système Meta ouvert,
    /// casque retiré, notification… Sans focus, le jeu est mis en pause (et
    /// les manettes ne répondent plus, le runtime les rend inactives) ; il
    /// reprend au retour du focus — sauf si le joueur l'avait mis en pause
    /// lui-même. Exigence du Horizon Store.
    pub fn set_focused(&mut self, focused: bool) {
        match self {
            Self::Game(game) => game.set_focused(focused),
            Self::Balls(balls) => balls.set_focused(focused),
            Self::Cubes(_) => {}
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
            Self::Balls(balls) => FrameOut {
                haptics: balls.render(device, queue, eyes, input, targets),
                quit: false,
            },
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
        if self.style == Style::Vehicle {
            self.app.input_state.race = if menu_open {
                RaceInput::default()
            } else {
                race_input(input)
            };
        }
        let now = crate::time_compat::Instant::now();
        let dt = self
            .last_frame
            .map_or(1.0 / 72.0, |t| now.duration_since(t).as_secs_f32())
            .clamp(1e-3, 0.1);
        self.last_frame = Some(now);

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
        if let Some(rig) = self.rig.as_mut()
            && matches!(self.style, Style::Character | Style::Rehab)
        {
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
        } else if let (Some(rig), Style::Diorama) = (self.rig, self.style) {
            // Maquette : le stick va à gauche / à droite du niveau, où que
            // l'on regarde.
            self.app.vr_camera_yaw = Some(rig.yaw);
        }

        // 3 bis. Rééducation (phase 8) : corps et mains vus par une webcam
        //    virtuelle face au joueur → tables Lua `pose` et `hand`, comme la
        //    page web de Mouvéo. Poignets : mains suivies, sinon manettes.
        {
            let look_stage = head_rot * Vec3::NEG_Z;
            let cam = super::hands::VirtualWebcam::facing(head_stage, look_stage);
            let wrists = [LEFT, RIGHT].map(|i| {
                input.hand_joints[i]
                    .map(|j| j[super::hands::WRIST])
                    .or(input.hands[i].grip.map(|(p, _)| p))
            });
            self.app
                .set_pose(&super::hands::body_flat(&cam, head_stage, wrists));
            self.app.set_hands(&super::hands::hands_flat(
                &cam,
                [
                    input.hand_joints[LEFT].as_ref(),
                    input.hand_joints[RIGHT].as_ref(),
                ],
            ));
        }

        // 4. Simulation — les scripts voient la tête et les manettes (table
        //    Lua `vr`, phase 6), l'audio s'écoute depuis la tête.
        if let Some(rig) = self.rig {
            let head_world = rig.origin + Quat::from_rotation_y(rig.yaw) * head_stage;
            let look = rig.look_world(head_rot);
            self.app.vr_listener = Some((head_world, look));
            self.app.vr_script = Some(script_state(&rig, head_world, look, input));
        }
        let t0 = crate::time_compat::Instant::now();
        self.app.advance_play();
        self.last_sim_ms = t0.elapsed().as_secs_f32() * 1000.0;

        // 5. Rig : suit le personnage en première personne, fixe en spectateur ;
        //    assis dans la voiture ; face à la maquette du plateformer.
        let feet = player_feet(&self.app);
        let comfort = self.comfort;
        let style = self.style;
        if self.rig.is_none() {
            self.seat_head = Some(head_stage);
        }
        let rig = self.rig.get_or_insert_with(|| {
            let rig = match (style, comfort.view) {
                (Style::Rehab, _) => {
                    Rig::from_camera(&self.app, crate::xr::sim::STANDING_EYE_HEIGHT)
                }
                (Style::Character, ViewMode::Spectator) => Rig::spectator(&self.app),
                (Style::Diorama, view) => {
                    let yaw = Rig::yaw_facing(self.app.camera.target - self.app.camera.eye());
                    Rig {
                        origin: diorama_origin(&self.app, yaw, view)
                            .unwrap_or(self.app.camera.eye()),
                        yaw,
                    }
                }
                (Style::Vehicle, _) => Rig::default(),
                (Style::Character, ViewMode::FirstPerson) => Rig {
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
        let first_person = style == Style::Character && comfort.view == ViewMode::FirstPerson;
        match style {
            Style::Character if first_person => {
                if let Some(feet) = feet {
                    rig.follow_first_person(feet, head_stage);
                }
            }
            Style::Vehicle => {
                if let Some((pos, fwd, up, _)) = self.app.race_car_view() {
                    let target = Rig::yaw_facing(fwd);
                    // Lacet lissé : la direction de la voiture n'avance qu'au pas
                    // fixe (60 Hz), le casque affiche à 72–90 Hz.
                    let d = (target - rig.yaw + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    rig.yaw += d * (1.0 - (-dt * 18.0).exp());
                    let flat = Quat::from_rotation_y(rig.yaw) * Vec3::NEG_Z;
                    let seat = match comfort.view {
                        ViewMode::FirstPerson => pos + up * COCKPIT_EYE + fwd * COCKPIT_FORWARD,
                        ViewMode::Spectator => pos + Vec3::Y * 2.6 - flat * 7.5,
                    };
                    let head = self.seat_head.unwrap_or(head_stage);
                    rig.origin = seat - Quat::from_rotation_y(rig.yaw) * head;
                }
            }
            Style::Diorama => {
                if let Some(want) = diorama_origin(&self.app, rig.yaw, comfort.view) {
                    let gap = want - rig.origin;
                    if gap.length() > 15.0 {
                        rig.origin = want; // porte vers le niveau suivant : on suit d'un coup
                    } else {
                        let k = |rate: f32| 1.0 - (-dt * rate).exp();
                        rig.origin += Vec3::new(gap.x * k(3.0), gap.y * k(1.2), gap.z * k(3.0));
                    }
                }
            }
            _ => {}
        }
        let rig = *rig;
        if self.open_levels {
            self.open_levels = false;
            let look = rig.look_world(head_rot);
            let head_world = rig.origin + Quat::from_rotation_y(rig.yaw) * head_stage;
            self.toggle_menu(head_world, look);
            if let Some(menu) = self.menu.as_mut() {
                menu.levels = true;
            }
        }

        // 6. Vignette selon la vitesse du personnage (première personne).
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

        // 8. Repères des manettes (P3) : boîte de la poignée et direction —
        //    ou squelette de la main quand elle est suivie (P8).
        for (hand, joints) in input.hands.iter().zip(&input.hand_joints) {
            if let Some(joints) = joints {
                push_hand_lines(&mut self.app.debug_lines, &rig, joints);
            } else if let Some((pos, rot)) = hand.grip {
                let (p, r) = to_world(&rig, pos, rot);
                push_controller_lines(&mut self.app.debug_lines, p, r);
            }
        }

        // 9. Interface (P5) : menu (bouton menu de la manette gauche) et poignet.
        let eyes_world = eyes.map(|e| rig.to_world(e));
        let head_world = (eyes_world[0].position + eyes_world[1].position) * 0.5;
        let look = rig.look_world(head_rot);
        let menu_btn = input.hands[LEFT].menu || self.long_left_pinch(input);
        if menu_btn && !self.menu_was {
            self.toggle_menu(head_world, look);
        }
        self.menu_was = menu_btn;
        let mut actions = Vec::new();
        if let Some(menu) = self.menu {
            let pointer = self.menu_pointer(&rig, input, menu.pose);
            let page = MenuPage {
                comfort: self.comfort,
                style: self.style,
                levels: menu.levels,
                can_go_back: self.choice != SceneChoice::Launcher,
            };
            self.ui.paint(device, queue, MENU, pointer, |ui| {
                menu_ui(ui, page, &mut actions);
            });
        }
        let wrist = if style == Style::Vehicle {
            // Voiture : tableau de bord fixe devant le volant, les mains sont
            // sur les gâchettes.
            let dash = rig.origin
                + Quat::from_rotation_y(rig.yaw)
                    * (self.seat_head.unwrap_or(head_stage) + Vec3::new(0.0, -0.32, -0.75));
            Some(PanelPose::facing(dash, head_world, Vec2::new(0.42, 0.21)))
        } else {
            input.hands[LEFT].grip.map(|(pos, rot)| {
                let (p, _) = to_world(&rig, pos, rot);
                PanelPose::facing(p + Vec3::Y * 0.09, head_world, Vec2::new(0.18, 0.09))
            })
        };
        if wrist.is_some() {
            let health = match style {
                Style::Character | Style::Rehab => self.app.hud_health,
                // HerRoad range sa jauge de vitesse dans `hud_health`.
                Style::Vehicle | Style::Diorama => None,
            };
            let kills = self.app.displayed_kill_count();
            // Rééducation : la consigne de la séance (écrite par les scripts
            // Mouvéo pour la page web) plutôt que le score.
            let hint = wrist_hint(&self.app, style);
            self.ui
                .paint(device, queue, WRIST, Pointer::default(), |ui| {
                    wrist_ui(ui, health, kills, hint.as_deref());
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
                MenuAction::Restart if self.style == Style::Vehicle => {
                    // Nouvelle course (le fantôme et le record sont sur disque).
                    self.load_level(SceneChoice::HerRoad);
                }
                MenuAction::Restart => {
                    self.close_menu();
                    self.app.restart_game();
                    self.rig = None;
                }
                MenuAction::Levels | MenuAction::Back => {
                    if let Some(menu) = self.menu.as_mut() {
                        menu.levels = action == MenuAction::Levels;
                    }
                }
                MenuAction::Load(choice)
                    if self.choice == SceneChoice::Launcher && choice == SceneChoice::Riviere =>
                {
                    // Rivière tourne déjà en fond du sélecteur.
                    self.choice = choice;
                    self.close_menu();
                }
                MenuAction::Load(choice) => self.load_level(choice),
                MenuAction::StartSession => {
                    self.close_menu();
                    self.app.push_hud_event("demarrer");
                }
                MenuAction::Quit => quit = true,
            }
        }

        // 11. Vibrations sur les fronts montants des effets du jeu.
        let fx = (self.app.fx.damage_flash, self.app.fx.attack_flash);
        let mut haptics = haptics_from_fx(self.prev_fx, fx);
        self.prev_fx = fx;
        // … et celles demandées par les scripts (`vr.haptic`), la plus forte gagne.
        for (hand, amplitude, seconds) in crate::app::script_ctx::take_vr_haptics() {
            let h = &mut haptics[hand.min(1)];
            if h.is_none_or(|h| h.amplitude < amplitude) {
                *h = Some(Haptic { amplitude, seconds });
            }
        }
        FrameOut { haptics, quit }
    }

    /// Focus du casque (cf. `XrContent::set_focused`).
    fn set_focused(&mut self, focused: bool) {
        self.focus_pause.update(focused, &mut self.app.paused);
    }

    /// Mode mains (pas de bouton menu) : pincement gauche tenu
    /// `MENU_PINCH_SECONDS` → vrai une fois (front), jusqu'au relâchement.
    fn long_left_pinch(&mut self, input: &XrInput) -> bool {
        let pinching = input.hand_joints[LEFT]
            .is_some_and(|j| super::hands::pinch_distance(&j) < super::hands::PINCH_CLICK);
        if !pinching {
            self.left_pinch_since = None;
            return false;
        }
        let since = *self
            .left_pinch_since
            .get_or_insert_with(crate::time_compat::Instant::now);
        since.elapsed().as_secs_f32() >= MENU_PINCH_SECONDS
    }

    /// Ouvre le menu devant le joueur (jeu en pause) ou le referme.
    fn toggle_menu(&mut self, head_world: Vec3, look: Vec3) {
        if self.menu.is_some() {
            self.close_menu();
        } else {
            self.menu = Some(MenuState {
                pose: PanelPose::in_front_of(head_world, look, 1.3, Vec2::new(1.0, 0.75)),
                paused_before: self.app.paused,
                levels: false,
            });
            self.app.paused = true;
        }
    }

    /// Remplace la partie par ce niveau ; réglages de confort conservés.
    fn load_level(&mut self, choice: SceneChoice) {
        self.menu = None;
        self.app = choice.load();
        self.choice = choice;
        self.style = choice.style();
        self.comfort.view = match self.style {
            Style::Character | Style::Vehicle => ViewMode::FirstPerson,
            Style::Rehab | Style::Diorama => ViewMode::Spectator,
        };
        self.rig = None;
        self.seat_head = None;
        self.prev_feet = None;
        self.speed = 0.0;
        self.prev_fx = (0.0, 0.0);
        self.renderer.vr_vignette = 0.0;
        log::info!("VR : niveau « {} »", choice.label());
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
        // Manette : pose de visée, gâchette. Main suivie (P8) : rayon depuis la
        // base de l'index, pincement = clic.
        let (origin, dir, pressed) = if let Some((pos, rot)) = right.aim {
            let (origin, rot) = to_world(rig, pos, rot);
            (origin, rot * Vec3::NEG_Z, right.trigger >= PRESS_THRESHOLD)
        } else if let Some(joints) = &input.hand_joints[RIGHT] {
            let (o, d) = super::hands::aim_ray(joints);
            let r = Quat::from_rotation_y(rig.yaw);
            (
                rig.origin + r * o,
                r * d,
                super::hands::pinch_distance(joints) < super::hands::PINCH_CLICK,
            )
        } else {
            return Pointer::default();
        };
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
            pressed,
        }
    }
}

/// Pause automatique à la perte du focus du casque (cf. `XrContent::set_focused`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct FocusPause {
    /// Vrai si c'est la perte de focus qui a mis le jeu en pause (à lever au
    /// retour du focus) ; faux si le joueur était déjà en pause.
    paused_by_focus: bool,
}

impl FocusPause {
    fn update(&mut self, focused: bool, paused: &mut bool) {
        if !focused && !*paused {
            *paused = true;
            self.paused_by_focus = true;
        } else if focused && self.paused_by_focus {
            *paused = false;
            self.paused_by_focus = false;
        }
    }
}

/// Durée d'un pincement gauche qui ouvre le menu en mode mains.
const MENU_PINCH_SECONDS: f32 = 0.8;

/// Couleurs de l'écran d'accueil (page web et lobby natif, #12141a / #e8763b).
const BG: egui::Color32 = egui::Color32::from_rgb(18, 20, 26);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(232, 118, 59);
const TEXT: egui::Color32 = egui::Color32::from_rgb(230, 232, 227);

/// Ce que le menu VR affiche à cette image.
#[derive(Debug, Clone, Copy)]
struct MenuPage {
    comfort: Comfort,
    style: Style,
    /// Page « Choisir un niveau » plutôt que la pause.
    levels: bool,
    /// Faux au lancement (sélecteur) : il n'y a pas encore de partie où revenir.
    can_go_back: bool,
}

/// Menu VR : gros boutons (visés au rayon, cliqués à la gâchette) — la pause,
/// ou la liste des niveaux.
fn menu_ui(root: &mut egui::Ui, page: MenuPage, actions: &mut Vec<MenuAction>) {
    let frame = egui::Frame::NONE
        .fill(BG)
        .corner_radius(24)
        .stroke(egui::Stroke::new(
            2.0_f32,
            egui::Color32::from_rgb(40, 45, 54),
        ))
        .inner_margin(24);
    egui::CentralPanel::default()
        .frame(frame)
        .show_inside(root, |ui| {
            ui.visuals_mut().override_text_color = Some(TEXT);
            ui.vertical_centered(|ui| {
                if page.levels {
                    levels_ui(ui, page.can_go_back, actions);
                } else {
                    pause_ui(ui, page, actions);
                }
            });
            // Curseur : là où vise la manette.
            if let Some(pos) = ui.ctx().pointer_latest_pos() {
                ui.painter().circle_filled(pos, 7.0, ACCENT);
            }
        });
}

fn pause_ui(ui: &mut egui::Ui, page: MenuPage, actions: &mut Vec<MenuAction>) {
    ui.label(egui::RichText::new("Pause").size(26.0).color(ACCENT));
    ui.add_space(6.0);
    let size = egui::vec2(ui.available_width() * 0.9, 36.0);
    let mut button = |label: String, action: MenuAction| {
        let b = egui::Button::new(egui::RichText::new(label).size(19.0)).min_size(size);
        if ui.add(b).clicked() {
            actions.push(action);
        }
    };
    if page.style == Style::Rehab {
        button("Démarrer la séance".into(), MenuAction::StartSession);
    }
    button("Reprendre".into(), MenuAction::Resume);
    button("Choisir un niveau".into(), MenuAction::Levels);
    button("Recentrer la vue".into(), MenuAction::Recenter);
    let view = match (page.style, page.comfort.view) {
        (Style::Vehicle, ViewMode::FirstPerson) => "cockpit",
        (Style::Vehicle, ViewMode::Spectator) => "poursuite",
        (Style::Diorama, ViewMode::FirstPerson) => "proche",
        (Style::Diorama, ViewMode::Spectator) => "éloignée",
        (_, ViewMode::FirstPerson) => "première personne",
        (_, ViewMode::Spectator) => "spectateur",
    };
    button(format!("Vue : {view}"), MenuAction::ToggleView);
    if matches!(page.style, Style::Character | Style::Rehab) {
        button(
            format!("Rotation par crans : {:.0}°", page.comfort.snap_degrees),
            MenuAction::CycleSnap,
        );
    }
    if page.style == Style::Character {
        let vignette = if page.comfort.vignette { "oui" } else { "non" };
        button(
            format!("Vignette de confort : {vignette}"),
            MenuAction::ToggleVignette,
        );
    }
    let restart = match page.style {
        Style::Vehicle => "Recommencer la course",
        _ => "Rejouer la manche",
    };
    button(restart.into(), MenuAction::Restart);
    button("Quitter".into(), MenuAction::Quit);
}

fn levels_ui(ui: &mut egui::Ui, can_go_back: bool, actions: &mut Vec<MenuAction>) {
    ui.label(
        egui::RichText::new("Choisir un niveau")
            .size(26.0)
            .color(ACCENT),
    );
    ui.add_space(6.0);
    let size = egui::vec2(ui.available_width() * 0.9, 52.0);
    for level in LEVELS {
        let text = format!("{}\n{}", level.label(), level.blurb());
        let mut job = egui::text::LayoutJob::default();
        let (title, blurb) = text.split_once('\n').unwrap_or((&text, ""));
        job.append(
            title,
            0.0,
            egui::TextFormat::simple(egui::FontId::proportional(20.0), TEXT),
        );
        job.append(
            &format!("\n{blurb}"),
            0.0,
            egui::TextFormat::simple(
                egui::FontId::proportional(14.0),
                egui::Color32::from_rgb(160, 166, 160),
            ),
        );
        if ui.add(egui::Button::new(job).min_size(size)).clicked() {
            actions.push(MenuAction::Load(level));
        }
    }
    if can_go_back {
        ui.add_space(4.0);
        let b = egui::Button::new(egui::RichText::new("Retour").size(18.0))
            .min_size(egui::vec2(size.x * 0.5, 34.0));
        if ui.add(b).clicked() {
            actions.push(MenuAction::Back);
        }
    }
}

/// Affichage au poignet gauche : vie et score, lisible d'un coup d'œil.
fn wrist_ui(root: &mut egui::Ui, health: Option<f32>, kills: u32, hint: Option<&str>) {
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
            match hint {
                Some(hint) if !hint.is_empty() => {
                    ui.label(egui::RichText::new(hint).size(18.0));
                }
                _ => {
                    ui.label(egui::RichText::new(format!("Ennemis vaincus : {kills}")).size(18.0));
                }
            }
        });
}

/// Hauteur de l'œil au-dessus du centre de la voiture, et avancée depuis ce
/// centre, en vue cockpit : au-dessus du capot, comme la caméra « capot » de
/// HerRoad (plus en arrière, on est dans la carrosserie).
const COCKPIT_EYE: f32 = 1.45;
const COCKPIT_FORWARD: f32 = 1.4;

/// Manettes Touch → commandes de HerRoad : gâchette droite = gaz, gauche =
/// frein et marche arrière, stick gauche = direction, poignées = frein à main,
/// B = dernier point de passage, Y = recommencer (appuis prolongés, comme au
/// clavier).
fn race_input(input: &XrInput) -> RaceInput {
    let (l, r) = (&input.hands[LEFT], &input.hands[RIGHT]);
    RaceInput {
        throttle: r.trigger,
        brake: l.trigger,
        steer: super::input::deadzone(l.stick).0,
        handbrake: r.squeeze >= PRESS_THRESHOLD || l.squeeze >= PRESS_THRESHOLD,
        respawn: r.secondary,
        restart: l.secondary,
        camera: false,
    }
}

/// Plateformer 2D en maquette : le sol de la pièce 1 m sous le personnage (les
/// yeux ~0,6 m au-dessus de lui, debout), à 7 m (vue proche) ou 11 m
/// (éloignée) du plan du niveau.
fn diorama_origin(app: &AppState, yaw: f32, view: ViewMode) -> Option<Vec3> {
    let p = app.player_position()?;
    let plane_z = app.scene.platformer.map_or(p.z, |pl| pl.plane_z);
    let distance = match view {
        ViewMode::FirstPerson => 7.0,
        ViewMode::Spectator => 11.0,
    };
    let forward = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
    Some(Vec3::new(p.x, p.y - 1.0, plane_z) - forward * distance)
}

/// Texte du poignet (ou du tableau de bord) selon le jeu.
fn wrist_hint(app: &AppState, style: Style) -> Option<String> {
    let text = |k: &str| app.hud_texts.get(k).cloned().unwrap_or_default();
    match style {
        Style::Rehab => app.hud_texts.get("consigne").cloned(),
        Style::Vehicle => {
            let center = text("hr_center");
            let head = match center.as_str() {
                "PRÊT ?" => "PRÊT ? Gâchette droite pour partir".to_string(),
                "" => text("hr_turbo"),
                _ => center,
            };
            Some(format!(
                "{}   {} km/h\n{}\n{}",
                text("hr_time"),
                text("hr_speed"),
                text("hr_lap"),
                head
            ))
        }
        Style::Diorama => Some(format!(
            "Niveau {}   ·   Morts {}",
            app.platformer_level().map_or(1, |l| l + 1),
            app.deaths()
        )),
        Style::Character => None,
    }
}

/// État VR publié aux scripts (table Lua `vr`) : tête et manettes en monde.
fn script_state(
    rig: &Rig,
    head_world: Vec3,
    look: Vec3,
    input: &XrInput,
) -> crate::app::script_ctx::VrScriptState {
    let hand = |h: &super::input::HandInput| {
        h.grip
            .map(|(pos, rot)| crate::app::script_ctx::VrHandScript {
                pos: to_world(rig, pos, rot).0,
                trigger: h.trigger,
                grip: h.squeeze,
                primary: h.primary,
                secondary: h.secondary,
            })
    };
    crate::app::script_ctx::VrScriptState {
        head: head_world,
        // Même convention que `obj.ry` : regard −Z à 0, positif vers la gauche.
        yaw: (-look.x).atan2(-look.z),
        hands: [hand(&input.hands[LEFT]), hand(&input.hands[RIGHT])],
    }
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

/// Squelette d'une main suivie (phase 8) : poignet → chaque doigt, en lignes.
fn push_hand_lines(
    lines: &mut Vec<(Vec3, Vec3, [f32; 3])>,
    rig: &Rig,
    joints: &super::hands::Joints,
) {
    const HAND: [f32; 3] = [0.95, 0.85, 0.7];
    let r = Quat::from_rotation_y(rig.yaw);
    let w = |i: usize| rig.origin + r * joints[i];
    // Pouce : 1 → 2..5 ; doigts : 1 → métacarpe → … → bout.
    for chain in [
        [1usize, 2, 3, 4, 5],
        [1, 6, 7, 8, 9],
        [1, 11, 12, 13, 14],
        [1, 16, 17, 18, 19],
        [1, 21, 22, 23, 24],
    ] {
        for pair in chain.windows(2) {
            lines.push((w(pair[0]), w(pair[1]), HAND));
        }
    }
    for (a, b) in [(9usize, 10usize), (14, 15), (19, 20), (24, 25)] {
        lines.push((w(a), w(b), HAND));
    }
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

#[cfg(test)]
mod tests {
    use super::{FocusPause, LEVELS, SceneChoice, Style};

    #[test]
    fn every_level_parses_back_from_its_build_name() {
        for (name, choice) in [
            ("riviere", SceneChoice::Riviere),
            ("hameau", SceneChoice::Hameau),
            ("herroad", SceneChoice::HerRoad),
            ("ragequit", SceneChoice::RageQuit),
            ("reeduc", SceneChoice::Reeducation),
            ("cubes", SceneChoice::Cubes),
            ("embedded", SceneChoice::Embedded),
            ("menu", SceneChoice::Launcher),
            ("", SceneChoice::Launcher),
        ] {
            assert_eq!(SceneChoice::parse(name), choice, "{name}");
        }
        assert!(LEVELS.iter().all(|l| !l.blurb().is_empty()));
    }

    #[test]
    fn herroad_is_driven_and_ragequit_is_a_diorama() {
        assert_eq!(SceneChoice::HerRoad.style(), Style::Vehicle);
        assert_eq!(SceneChoice::RageQuit.style(), Style::Diorama);
        let app = SceneChoice::HerRoad.load();
        assert!(app.race.is_some() && app.playing);
    }

    /// RageQuit en VR : le stick gauche fait avancer le personnage (le second
    /// joueur, masqué hors coop, ne bloque plus le départ).
    #[test]
    fn ragequit_player_walks_right_with_the_stick() {
        let mut app = SceneChoice::RageQuit.load();
        app.vr_camera_yaw = Some(0.0);
        app.input_state.gamepad_move = (1.0, 0.0);
        app.advance_steps(60);
        let i = app.player_index().expect("joueur");
        let x = app.scene.objects[i].transform.position.x;
        assert!(x > 3.0, "le joueur est resté à x = {x}");
    }

    /// La scène RageQuit embarquée se lit (sinon repli silencieux sur Rivière).
    #[test]
    fn embedded_ragequit_scene_is_the_platformer() {
        let scene = super::ragequit_scene();
        assert!(scene.platformer.is_some(), "mode plateformer 2D");
        assert!(scene.objects.len() > 1000);
    }

    #[test]
    fn losing_focus_pauses_and_regaining_it_resumes_only_what_it_paused() {
        let mut f = FocusPause::default();
        let mut paused = false;
        f.update(false, &mut paused);
        assert!(paused, "menu système ouvert : pause");
        f.update(false, &mut paused);
        f.update(true, &mut paused);
        assert!(!paused, "focus revenu : reprise");
        // Déjà en pause (menu du jeu) : la perte de focus ne la lèvera pas au retour.
        paused = true;
        f.update(false, &mut paused);
        f.update(true, &mut paused);
        assert!(paused, "la pause du joueur est respectée");
    }
}
