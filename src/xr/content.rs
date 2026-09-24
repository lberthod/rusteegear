//! Ce qu'une session VR affiche — **même code** pour l'APK Quest (`xr::hello`)
//! et le simulateur desktop (`quest_sim`) : seules la source des poses d'yeux
//! et les cibles de rendu diffèrent.
//!
//! - `Cubes` : scène de test de la phase 0 (`xr::test_scene`) ;
//! - `Game` : une vraie partie du moteur (phase 1 : Rivière), rendue par le
//!   `Renderer` complet via `render_views`, la pièce du joueur placée dans le
//!   monde par un `Rig`.

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
                }))
            }
        }
    }

    /// Avance le jeu d'une image et rend les deux yeux. `eyes` : poses dans la
    /// **pièce** (espace `STAGE` / tête simulée), le rig les place dans le monde.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: [EyeView; 2],
        targets: [&wgpu::TextureView; 2],
        width: u32,
        height: u32,
    ) {
        match self {
            Self::Cubes(scene) => scene.render(device, queue, targets, eyes.map(|e| e.view_proj())),
            Self::Game(game) => {
                let t0 = crate::time_compat::Instant::now();
                game.app.advance_play();
                game.last_sim_ms = t0.elapsed().as_secs_f32() * 1000.0;
                let rig = *game.rig.get_or_insert_with(|| {
                    let rig = Rig::spectator(&game.app);
                    log::info!(
                        "VR : rig posé en ({:.2}, {:.2}, {:.2}), lacet {:.0}°",
                        rig.origin.x,
                        rig.origin.y,
                        rig.origin.z,
                        rig.yaw.to_degrees()
                    );
                    rig
                });
                let t1 = crate::time_compat::Instant::now();
                game.renderer.render_views(
                    &mut game.app,
                    eyes.map(|e| rig.to_world(e)),
                    targets,
                    width,
                    height,
                );
                game.last_render_ms = t1.elapsed().as_secs_f32() * 1000.0;
            }
        }
    }
}
