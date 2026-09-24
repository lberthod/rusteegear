//! Ce qu'une session VR affiche — **même code** pour l'APK Quest (`xr::hello`)
//! et le simulateur desktop (`quest_sim`) : seules la source des poses d'yeux
//! et les cibles de rendu diffèrent.
//!
//! - `Cubes` : scène de test de la phase 0 (`xr::test_scene`) ;
//! - `Game` : une vraie partie du moteur (phase 1 : Rivière), rendue par le
//!   `Renderer` complet via `render_views`, la pièce du joueur placée dans le
//!   monde par un `Rig`.

use glam::{Quat, Vec3};

use super::input::{Haptic, RIGHT, XrInput, haptics_from_fx};
use super::locomotion::{Comfort, SnapTurn, ViewMode, vignette_strength};
use super::math::EyeView;
use super::rig::Rig;
use super::test_scene::CubeScene;
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
                }))
            }
        }
    }

    /// Avance le jeu d'une image et rend les deux yeux. `eyes` : poses dans la
    /// **pièce** (espace `STAGE` / tête simulée), `input` : manettes dans la
    /// pièce ; le rig place tout dans le monde. Renvoie les vibrations à jouer
    /// (manette gauche, droite).
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
    ) -> [Option<Haptic>; 2] {
        match self {
            Self::Cubes(scene) => {
                scene.render(device, queue, targets, eyes.map(|e| e.view_proj()));
                [None, None]
            }
            Self::Game(game) => game.frame(eyes, input, targets, width, height),
        }
    }
}

impl GameView {
    fn frame(
        &mut self,
        eyes: [EyeView; 2],
        input: &XrInput,
        targets: [&wgpu::TextureView; 2],
        width: u32,
        height: u32,
    ) -> [Option<Haptic>; 2] {
        let head_stage = (eyes[0].position + eyes[1].position) * 0.5;
        let head_rot = eyes[0].orientation;

        // 1. Manettes → commandes du jeu (P3).
        input.apply_to(&mut self.app.input_state);

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
            let turn = self
                .snap
                .update(input.hands[RIGHT].stick.0, self.comfort.snap_degrees);
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

        let t1 = crate::time_compat::Instant::now();
        self.renderer.render_views(
            &mut self.app,
            eyes.map(|e| rig.to_world(e)),
            targets,
            width,
            height,
        );
        self.last_render_ms = t1.elapsed().as_secs_f32() * 1000.0;
        if let Some(i) = hidden {
            self.app.scene.objects[i].visible = true;
        }

        // 9. Vibrations sur les fronts montants des effets du jeu.
        let fx = (self.app.fx.damage_flash, self.app.fx.attack_flash);
        let haptics = haptics_from_fx(self.prev_fx, fx);
        self.prev_fx = fx;
        haptics
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
