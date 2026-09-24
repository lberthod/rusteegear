//! Branchement de HerRoad (`crate::racing`) au moteur : entrées de conduite, pas de simulation
//! de la voiture et des règles, recopie des poses dans la scène, caméra de poursuite, HUD,
//! sons et record persistant. Tout l'état vit dans `RaceSession` (`AppState::race`), présent
//! seulement quand la scène courante est la démo HerRoad.

use glam::{Quat, Vec3};

use super::AppState;
use crate::racing::car::{Car, CarInput};
use crate::racing::layout::{CarPart, RaceLayout};
use crate::racing::race::{LAPS, Phase, Race, RaceEvent, SavedBest, fmt_delta, fmt_time};
use crate::racing::track::Track;
use crate::runtime::sfx::Sfx;
use crate::scene::Scene;
use crate::scene::demos::herroad::{GATE_DONE, GATE_IDLE, GATE_NEXT};

/// Commandes de conduite résolues par `lib.rs` (manette + clavier).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RaceInput {
    pub throttle: f32,
    pub brake: f32,
    /// -1 (gauche) .. +1 (droite).
    pub steer: f32,
    pub handbrake: bool,
    /// Retour au dernier point de passage (front montant).
    pub respawn: bool,
    /// Nouvelle course (front montant).
    pub restart: bool,
    /// Changement de caméra (front montant).
    pub camera: bool,
}

#[cfg_attr(test, allow(dead_code))]
const BEST_FILE: &str = "herroad_best.txt";
/// Distance de vue de la caméra de course (m).
const CAMERA_FAR: f32 = 600.0;

#[derive(Default)]
struct StepOut {
    sfx: Vec<Sfx>,
    shake: f32,
}

pub struct RaceSession {
    pub track: Track,
    pub layout: RaceLayout,
    pub car: Car,
    pub race: Race,
    /// Nombre d'objets de la scène à la construction : sert à détecter un changement de scène.
    object_count: usize,
    pub needs_reset: bool,
    prev: RaceInput,
    cam_yaw: f32,
    cam_mode: u8,
    /// Message temporaire (titre, sous-titre, secondes restantes).
    banner: Option<(String, String, f32)>,
    /// Écart au dernier point de passage : (texte, détail, secondes restantes).
    split: Option<(String, String, f32)>,
    gate_cache: Option<(usize, usize)>,
    wall_cooldown: f32,
    /// Durée (s) pendant laquelle « recommencer » / « point de passage » sont tenus : ces deux
    /// actions destructrices exigent un appui prolongé, pour qu'un bouton parasite (manette qui
    /// bruite, effleurement) ne relance jamais la course.
    hold_restart: f32,
    hold_respawn: f32,
    /// Le compte à rebours ne démarre qu'au premier geste du pilote (écran « PRÊT ? »).
    armed: bool,
    /// Durée (s) de conduite à contresens, pour l'alerte « sens interdit ».
    wrong_way: f32,
    /// Sons synthétisés (moteur, pneus), calculés une fois.
    engine_wav: Vec<u8>,
    skid_wav: Vec<u8>,
    /// Position de la pédale d'accélérateur au dernier pas (pilote le régime du moteur).
    last_throttle: f32,
    /// Pilote automatique (`HERROAD_AUTOPILOT=1`) : démonstration et captures de vérification.
    autopilot: Option<crate::racing::bot::Bot>,
}

/// Noms des voix en boucle du module audio.
const VOICE_ENGINE: &str = "herroad-engine";
const VOICE_SKID: &str = "herroad-skid";

/// Appui prolongé requis (s) pour recommencer / revenir au dernier point de passage.
const HOLD_RESTART: f32 = 0.4;
const HOLD_RESPAWN: f32 = 0.25;

#[cfg_attr(test, allow(dead_code))]
const GHOST_FILE: &str = "herroad_ghost.bin";

/// Record, meilleur tour et fantôme du disque (natif). Retombe sur l'ancien fichier texte de
/// record si le fichier binaire n'existe pas encore.
fn read_saved() -> SavedBest {
    // Les tests ne lisent ni n'écrasent jamais le vrai record de l'utilisateur.
    #[cfg(test)]
    return SavedBest::default();
    #[cfg(not(any(test, target_arch = "wasm32")))]
    if let Some(dir) = crate::assets::user_dir()
        && let Ok(bytes) = std::fs::read(dir.join(GHOST_FILE))
        && let Some(saved) = SavedBest::decode(&bytes)
    {
        return saved;
    }
    #[cfg(not(test))]
    {
        let best_total = crate::assets::persisted_read(crate::assets::user_dir(), BEST_FILE)
            .and_then(|b| std::str::from_utf8(&b).ok()?.trim().parse::<f32>().ok());
        SavedBest {
            best_total,
            ..Default::default()
        }
    }
}

fn write_saved(saved: &SavedBest) {
    #[cfg(test)]
    {
        let _ = saved;
        return;
    }
    #[cfg(not(test))]
    write_saved_to_disk(saved);
}

#[cfg(not(test))]
fn write_saved_to_disk(saved: &SavedBest) {
    // Le record texte reste écrit (lisible, et seul persisté sur le web).
    if let Some(t) = saved.best_total
        && let Err(e) = crate::assets::persisted_write(
            crate::assets::user_dir(),
            BEST_FILE,
            format!("{t:.3}").as_bytes(),
        )
    {
        log::warn!("HerRoad : record non enregistré ({e})");
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(dir) = crate::assets::user_dir() {
        let _ = std::fs::create_dir_all(&dir);
        // Écriture atomique : fichier temporaire puis renommage, jamais un fantôme à moitié écrit.
        let tmp = dir.join(format!("{GHOST_FILE}.tmp"));
        if std::fs::write(&tmp, saved.encode()).is_ok()
            && let Err(e) = std::fs::rename(&tmp, dir.join(GHOST_FILE))
        {
            log::warn!("HerRoad : fantôme non enregistré ({e})");
        }
    }
}

impl RaceSession {
    pub fn new(scene: &Scene, track: Track, layout: RaceLayout) -> RaceSession {
        let track_start = track.start_frame();
        let f = track.frames[track_start];
        let car = Car::placed(f.pos, f.fwd, f.up, track_start);
        let mut race = Race::new(&track, None);
        race.restore(read_saved());
        RaceSession {
            object_count: scene.objects.len(),
            track,
            layout,
            car,
            race,
            needs_reset: true,
            prev: RaceInput::default(),
            cam_yaw: 0.0,
            cam_mode: 0,
            banner: None,
            split: None,
            gate_cache: None,
            wall_cooldown: 0.0,
            hold_restart: 0.0,
            hold_respawn: 0.0,
            armed: false,
            wrong_way: 0.0,
            engine_wav: crate::racing::sound::engine_wav(),
            skid_wav: crate::racing::sound::skid_wav(),
            last_throttle: 0.0,
            autopilot: std::env::var_os("HERROAD_AUTOPILOT")
                .map(|_| crate::racing::bot::Bot::new(track_start)),
        }
    }

    fn valid_for(&self, scene: &Scene) -> bool {
        // `>=` : le moteur ajoute des objets **à la fin** à l'entrée en Play (gabarit joueur,
        // ancres d'effets…) sans décaler les indices de la démo ; seule une scène plus courte
        // ou dont la voiture a changé de place signale un vrai changement de scène.
        scene.objects.len() >= self.object_count
            && self
                .layout
                .car
                .first()
                .and_then(|p| scene.objects.get(p.index))
                .is_some_and(|o| o.name == "Caisse")
    }

    fn place_car(&mut self, pos: Vec3, fwd: Vec3, up: Vec3, frame: usize) {
        self.car = Car::placed(pos, fwd, up, frame);
        self.cam_yaw = (-fwd.x).atan2(-fwd.z);
    }

    fn reset(&mut self, scene: &mut Scene, cause: &str) {
        log::info!("HerRoad : (re)démarrage de la course ({cause})");
        self.needs_reset = false;
        self.race.restart();
        self.race.events.clear();
        let s = self.race.spawn;
        self.place_car(s.pos, s.fwd, s.up, s.frame);
        self.banner = None;
        self.split = None;
        self.gate_cache = None;
        self.refresh_gates(scene);
    }

    fn set_gate_color(scene: &mut Scene, ids: &[usize], (color, emissive): ([f32; 3], f32)) {
        for &i in ids {
            if let Some(o) = scene.objects.get_mut(i)
                && o.name != "Damier"
            {
                o.color = color;
                o.emissive = emissive;
            }
        }
    }

    fn refresh_gates(&mut self, scene: &mut Scene) {
        let key = (self.race.next_gate, self.race.lap as usize);
        if self.gate_cache == Some(key) {
            return;
        }
        self.gate_cache = Some(key);
        let n = self.layout.gates.len();
        for g in 0..n {
            let finish = g == n - 1;
            let style = if g < self.race.next_gate {
                GATE_DONE
            } else if g == self.race.next_gate {
                GATE_NEXT
            } else if finish {
                ([1.0, 1.0, 1.0], 1.0)
            } else {
                GATE_IDLE
            };
            let ids = self.layout.gates[g].clone();
            Self::set_gate_color(scene, &ids, style);
        }
    }

    fn respawn(&mut self) {
        let s = self.race.spawn;
        self.place_car(s.pos, s.fwd, s.up, s.frame);
        self.race.note_respawn();
    }

    fn step(&mut self, dt: f32, mut input: RaceInput, scene: &mut Scene) -> StepOut {
        let mut out = StepOut::default();
        if let Some(bot) = self.autopilot.as_mut() {
            let c = bot.drive(&self.car, &self.track);
            input.throttle = c.throttle.max(0.4 * (1.0 - c.brake));
            input.brake = c.brake;
            input.steer = c.steer;
            input.handbrake = false;
        }
        if self.needs_reset {
            self.reset(scene, "état initial / retour de Play");
        }
        let edge = |now: bool, before: bool| now && !before;
        // Franchissement du seuil d'appui prolongé (un seul déclenchement par appui).
        let held = |on: bool, timer: &mut f32, threshold: f32| {
            let before = *timer;
            *timer = if on { before + dt } else { 0.0 };
            before < threshold && *timer >= threshold
        };
        let restart = held(input.restart, &mut self.hold_restart, HOLD_RESTART);
        let respawn = held(input.respawn, &mut self.hold_respawn, HOLD_RESPAWN);
        if edge(input.camera, self.prev.camera) {
            self.cam_mode = (self.cam_mode + 1) % 3;
        }
        self.prev = input;
        self.wall_cooldown = (self.wall_cooldown - dt).max(0.0);

        if restart {
            self.reset(scene, "touche recommencer");
            // Le pilote est déjà dans la partie : pas d'écran d'attente, 3-2-1 immédiat.
            self.armed = true;
        } else if respawn && self.race.phase == Phase::Running {
            self.respawn();
            out.sfx.push(Sfx::Jump);
        }

        if !self.armed
            && (input.throttle > 0.2
                || input.brake > 0.2
                || input.steer.abs() > 0.3
                || input.handbrake)
        {
            self.armed = true;
        }
        self.last_throttle = if self.race.phase == Phase::Running { input.throttle } else { 0.0 };
        let prev = self.car.pos;
        match self.race.phase {
            Phase::Countdown => {
                self.car.prev_pos = self.car.pos;
            }
            Phase::Running | Phase::Finished => {
                let running = self.race.phase == Phase::Running;
                let inp = if running {
                    CarInput {
                        throttle: input.throttle,
                        brake: input.brake,
                        steer: input.steer,
                        handbrake: input.handbrake,
                    }
                } else {
                    // Ligne franchie : la voiture roule sur son élan et s'arrête doucement.
                    CarInput {
                        brake: 0.25,
                        ..Default::default()
                    }
                };
                let info = self.car.step(inp, &self.track, dt);
                if running {
                    self.apply_boosts(&mut out);
                }
                if let Some(v) = info.landed
                    && v > 9.0
                {
                    out.shake = out.shake.max((v / 40.0).min(0.6));
                }
                if let Some(v) = info.wall
                    && v > 6.0
                    && self.wall_cooldown <= 0.0
                {
                    self.wall_cooldown = 0.35;
                    out.shake = out.shake.max((v / 35.0).min(0.7));
                    out.sfx.push(Sfx::Hit);
                }
                let under_terrain = !self.car.grounded
                    && self.car.pos.y
                        < self.layout.terrain.height(self.car.pos.x, self.car.pos.z) + 0.1;
                if running
                    && (self.car.pos.y < self.track.min_y - 5.0
                        || self.car.air_time > 5.0
                        || under_terrain)
                {
                    self.respawn();
                    out.sfx.push(Sfx::Lose);
                }
            }
        }

        if self.race.phase != Phase::Countdown || self.armed {
            self.race.update(dt, &self.car, prev, &self.track);
        }
        self.consume_events(&mut out);
        // Conduite à contresens : cap opposé à la route pendant plus d'une seconde.
        let f = &self.track.frames[self.car.frame % self.track.frames.len()];
        let backwards = self.race.phase == Phase::Running
            && self.car.grounded
            && self.car.speed() > 6.0
            && self.car.vel.normalize_or_zero().dot(f.fwd) < -0.4;
        self.wrong_way = if backwards { self.wrong_way + dt } else { 0.0 };
        if let Some(b) = self.banner.as_mut() {
            b.2 -= dt;
        }
        if self.banner.as_ref().is_some_and(|b| b.2 <= 0.0) {
            self.banner = None;
        }
        if let Some(b) = self.split.as_mut() {
            b.2 -= dt;
        }
        if self.split.as_ref().is_some_and(|b| b.2 <= 0.0) {
            self.split = None;
        }
        self.refresh_gates(scene);
        self.write_scene(scene);
        out
    }

    fn apply_boosts(&mut self, out: &mut StepOut) {
        let car = &mut self.car;
        for b in &self.track.boosts {
            let d = car.pos - b.pos;
            let right = b.fwd.cross(b.up).normalize_or_zero();
            if d.dot(b.fwd).abs() < 3.8 && d.dot(right).abs() < 3.2 && d.dot(b.up).abs() < 2.5 {
                if car.boost < 0.3 {
                    out.sfx.push(Sfx::WaveStart);
                    out.shake = out.shake.max(0.25);
                }
                car.boost = car.boost.max(1.5);
            }
        }
    }

    fn consume_events(&mut self, out: &mut StepOut) {
        let events = std::mem::take(&mut self.race.events);
        for e in events {
            match e {
                RaceEvent::Beep(_) => out.sfx.push(Sfx::Pickup),
                RaceEvent::Go => {
                    log::info!("HerRoad : GO !");
                    out.sfx.push(Sfx::WaveStart);
                }
                RaceEvent::Checkpoint { index, delta } => {
                    out.sfx.push(Sfx::Pickup);
                    let total = self.track.checkpoints.len();
                    let lap_time = self.race.splits.last().copied().unwrap_or(0.0);
                    let head = delta.map_or_else(|| format!("PASSAGE {}/{total}", index + 1), fmt_delta);
                    self.split = Some((head, fmt_time(lap_time), 2.6));
                }
                RaceEvent::LapDone { lap, time, best } => {
                    out.sfx.push(Sfx::Pickup);
                    if lap < LAPS {
                        let tag = if best { "  MEILLEUR TOUR" } else { "" };
                        self.banner = Some((
                            format!("TOUR {lap}/{LAPS}"),
                            format!("{}{tag}", fmt_time(time)),
                            2.4,
                        ));
                    }
                }
                RaceEvent::Finished { total: _, record } => {
                    out.sfx.push(Sfx::Win);
                    // Record battu ou meilleur tour amélioré : on garde tout (fantôme compris).
                    if record || self.race.best_lap.is_some() {
                        write_saved(&self.race.saved());
                    }
                }
                RaceEvent::Respawn | RaceEvent::Restart => {}
            }
        }
    }

    fn part_pose(&self, part: &CarPart, pos: Vec3, rot: Quat, steer: f32) -> (Vec3, Quat) {
        let steer_rot = if part.front_wheel {
            Quat::from_rotation_y(-steer * 0.45)
        } else {
            Quat::IDENTITY
        };
        (pos + rot * part.offset, rot * steer_rot * part.local_rot)
    }

    fn write_scene(&self, scene: &mut Scene) {
        let car = &self.car;
        let rot = car.rotation();
        for part in &self.layout.car {
            let (p, r) = self.part_pose(part, car.pos, rot, car.steer);
            if let Some(o) = scene.objects.get_mut(part.index) {
                o.transform.position = p;
                o.transform.rotation = r;
            }
        }
        // Fantôme du meilleur run, rejoué à la même date de course.
        let ghost = match self.race.phase {
            Phase::Countdown => self.race.ghost_at(0.0),
            _ => self.race.ghost_at(self.race.time),
        };
        for part in &self.layout.ghost {
            let Some(o) = scene.objects.get_mut(part.index) else {
                continue;
            };
            match ghost {
                Some(g) if self.race.phase != Phase::Finished => {
                    let r = crate::racing::car::basis_rotation(g.fwd, g.up);
                    o.visible = true;
                    o.transform.position = g.pos + r * part.offset;
                    o.transform.rotation = r * part.local_rot;
                }
                _ => o.visible = false,
            }
        }
        // Fumée de dérapage et flamme de turbo, à l'arrière de la voiture.
        let rear = car.pos - car.fwd * 1.9 + car.up * 0.3;
        if let Some(o) = self.layout.smoke.and_then(|i| scene.objects.get_mut(i)) {
            o.transform.position = rear;
            if let Some(em) = o.particle_emitter.as_mut() {
                let on = car.grounded && (car.drift > 0.3 || car.surface != crate::racing::track::Surface::Asphalt && car.speed() > 15.0);
                em.enabled = on;
                em.rate = if on { 40.0 + 90.0 * car.drift } else { 0.0 };
                if car.surface == crate::racing::track::Surface::Dirt {
                    em.color = [0.55, 0.42, 0.28];
                } else {
                    em.color = [0.86, 0.86, 0.9];
                }
            }
        }
        if let Some(o) = self.layout.flame.and_then(|i| scene.objects.get_mut(i)) {
            o.transform.position = rear;
            if let Some(em) = o.particle_emitter.as_mut() {
                let on = car.boost > 0.0;
                em.enabled = on;
                em.rate = if on { 160.0 } else { 0.0 };
                em.direction = (-car.fwd).to_array();
            }
        }
        if let Some(l) = self.layout.headlight.and_then(|i| scene.point_lights.get_mut(i)) {
            l.position = (car.pos + car.up * 2.5 + car.fwd * 9.0).to_array();
        }
    }

    /// Hauteur et volume du moteur, volume du crissement de pneus.
    fn sound_state(&self) -> (f32, f32, f32) {
        let car = &self.car;
        let (mut rate, mut gain) =
            crate::racing::sound::engine_voice(car.speed(), self.last_throttle, car.boost > 0.0);
        if !car.grounded {
            // En l'air le moteur s'emballe et se fait plus discret.
            rate *= 1.15;
            gain *= 0.6;
        }
        match self.race.phase {
            Phase::Countdown => gain *= 0.7,
            Phase::Finished => gain *= 0.35,
            Phase::Running => {}
        }
        let skid = if car.grounded {
            crate::racing::sound::skid_gain(car.drift, car.speed())
        } else {
            0.0
        };
        (rate, gain, skid)
    }

    fn hud(&self, texts: &mut std::collections::HashMap<String, String>) {
        let r = &self.race;
        let mut set = |k: &str, v: String| {
            texts.insert(k.to_string(), v);
        };
        let shown = match r.phase {
            Phase::Finished => r.total.unwrap_or(r.time),
            _ => r.time,
        };
        set("hr_time", fmt_time(shown));
        let cps = self.track.checkpoints.len();
        set(
            "hr_lap",
            format!(
                "TOUR {}/{LAPS}     POINT {}/{cps}",
                (r.lap + 1).min(LAPS),
                r.next_gate.min(cps)
            ),
        );
        let best = r.best_lap.map_or_else(|| "--".to_string(), fmt_time);
        let rec = r
            .best_total
            .map_or_else(String::new, |t| format!("   RECORD {}", fmt_time(t)));
        set("hr_best", format!("MEILLEUR TOUR {best}{rec}"));
        // Dernier point de passage : écart au meilleur tour, coloré par le signe dans le texte.
        let split = self.split.as_ref().map_or_else(String::new, |s| format!("{}    {}", s.0, s.1));
        set("hr_cp", split);
        set("hr_speed", format!("{:.0}", self.car.kmh()));
        set(
            "hr_turbo",
            if self.car.boost > 0.0 { "TURBO !".into() } else { String::new() },
        );
        let (mut center, mut sub, mut sub2) = (String::new(), String::new(), String::new());
        if !self.armed && r.phase == Phase::Countdown {
            center = "PRÊT ?".into();
            sub = "Appuie sur ZR, A ou flèche haut pour lancer le départ".into();
            sub2 = "Tu peux d'abord regarder le circuit : C ou − change de caméra".into();
        } else {
            match r.countdown_digit() {
                Some(0) => center = "GO !".into(),
                Some(d) => center = d.to_string(),
                None => {}
            }
            if r.phase == Phase::Finished {
                center = "ARRIVÉE".into();
                sub = format!("MÉDAILLE {}  ·  {}", r.medal.label(), fmt_time(r.total.unwrap_or(0.0)));
                let laps: Vec<String> = r.lap_times.iter().map(|t| fmt_time(*t)).collect();
                sub2 = format!("{}   —   maintiens X ou + pour rejouer", laps.join("  /  "));
            } else if self.wrong_way > 1.0 {
                sub = "SENS INTERDIT — FAIS DEMI-TOUR".into();
                sub2 = "Maintiens Y (ou Retour arrière) pour revenir au dernier point de passage".into();
            } else if let Some((t, s, _)) = &self.banner {
                sub = format!("{t}   {s}");
            }
        }
        set("hr_center", center);
        set("hr_sub", sub);
        set("hr_sub2", sub2);
        // Aide affichée seulement au début (avant et pendant les 12 premières secondes),
        // sur deux lignes pour rester à gauche du panneau de vitesse.
        let show = r.phase == Phase::Countdown || (r.phase == Phase::Running && r.time < 12.0);
        let (h1, h2) = if show {
            (
                "ZR/A accélérer · ZL/B freiner · stick diriger · R frein à main",
                "Y (maintenir) dernier point de passage · X ou + (maintenir) recommencer · − caméra",
            )
        } else {
            ("", "")
        };
        set("hr_help", h1.into());
        set("hr_help2", h2.into());
    }
}

impl AppState {
    /// Charge la démo HerRoad : circuit, voiture et état de course.
    pub fn load_herroad_demo(&mut self) {
        self.push_undo();
        let (scene, layout, track) = crate::scene::demos::herroad::herroad_build();
        self.race = Some(RaceSession::new(&scene, track, layout));
        self.scene = scene;
        self.scene_file = None;
        self.async_load.imported_dirty = true;
        self.hud_health = None;
        self.fx.damage_flash = 0.0;
        self.fx.camera_shake = 0.0;
        self.fx.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.camera.far = CAMERA_FAR;
        self.clear_selection();
    }

    /// Un pas de simulation de la course (appelé au début de chaque pas fixe).
    pub fn race_step(&mut self, dt: f32) {
        let Some(mut session) = self.race.take() else {
            return;
        };
        if !session.valid_for(&self.scene) {
            // La scène a changé (autre démo, fichier ouvert) : la course s'arrête là.
            self.camera.far = crate::gfx::camera::OrbitCamera::FAR;
            self.audio.loop_voice_stop(VOICE_ENGINE);
            self.audio.loop_voice_stop(VOICE_SKID);
            return;
        }
        let out = session.step(dt, self.input_state.race, &mut self.scene);
        session.hud(&mut self.hud_texts);
        // Jauge de vitesse : le HUD lie sa barre à la « vie » (0..1) — jamais 0, sinon le
        // moteur déclarerait la manche perdue.
        self.hud_health = Some((session.car.kmh() / 400.0).clamp(0.03, 1.0));
        for s in out.sfx {
            crate::runtime::sfx::play(&mut self.audio, s);
        }
        let (rate, gain, skid) = session.sound_state();
        self.audio.loop_voice(VOICE_ENGINE, &session.engine_wav, rate, gain);
        self.audio
            .loop_voice(VOICE_SKID, &session.skid_wav, 1.0 + session.car.drift * 0.25, skid);
        if out.shake > 0.0 {
            self.fx.camera_shake = self.fx.camera_shake.max(out.shake);
        }
        self.race = Some(session);
    }

    /// Jeu en pause : le moteur et les pneus se taisent (les voix repartent à la reprise).
    pub(super) fn race_pause_audio(&mut self) {
        if self.race.is_some() {
            self.audio.loop_voice_stop(VOICE_ENGINE);
            self.audio.loop_voice_stop(VOICE_SKID);
        }
    }

    /// La course doit repartir de zéro au prochain pas (retour d'un Stop, nouvelle partie).
    pub(super) fn race_request_reset(&mut self) {
        if let Some(r) = self.race.as_mut() {
            r.needs_reset = true;
        }
    }

    /// Voiture vue depuis la VR (cockpit, `xr::content`) : position interpolée entre deux
    /// pas fixes comme `race_camera` (sinon le monde saccade à 72–90 Hz), direction avant,
    /// haut, vitesse en km/h.
    pub fn race_car_view(&self) -> Option<(Vec3, Vec3, Vec3, f32)> {
        let s = self.race.as_ref()?;
        let alpha = (self.sim_poses.sim_accumulator * 60.0).clamp(0.0, 1.0); // pas fixe 1/60 s
        let car = &s.car;
        Some((car.prev_pos.lerp(car.pos, alpha), car.fwd, car.up, car.kmh()))
    }

    /// Caméra de poursuite, au rythme des frames (interpolée entre deux pas fixes).
    pub fn race_camera(&mut self, dt: f32, alpha: f32) {
        let Some(s) = self.race.as_mut() else {
            return;
        };
        let car = &s.car;
        let pos = car.prev_pos.lerp(car.pos, alpha.clamp(0.0, 1.0));
        let speed = car.speed();
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or_zero();
        let mut heading = flat(car.fwd);
        if speed > 8.0 {
            heading = (heading * 0.55 + flat(car.vel) * 0.45).normalize_or_zero();
        }
        let want = (-heading.x).atan2(-heading.z);
        let follow = match s.cam_mode {
            2 => 16.0,
            _ => 5.5,
        };
        if s.race.phase == Phase::Finished {
            s.cam_yaw += dt * 0.55;
        } else {
            let mut d = want - s.cam_yaw;
            d = (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            s.cam_yaw += d * (1.0 - (-dt * follow).exp());
        }
        let cam = &mut self.camera;
        cam.far = CAMERA_FAR;
        cam.yaw = s.cam_yaw;
        cam.collision_distance = None;
        let boost = if car.boost > 0.0 { 9.0 } else { 0.0 };
        cam.fovy = (58.0 + speed.min(85.0) * 0.28 + boost).to_radians();
        let slope = -car.fwd.y * 0.5;
        match s.cam_mode {
            1 => {
                cam.target = pos + car.up * 1.6 + heading * (speed * 0.02);
                cam.distance = 15.0 + speed * 0.04;
                cam.pitch = (0.32 + slope).clamp(0.08, 0.7);
            }
            2 => {
                // Vue capot : l'œil est posé au-dessus du capot, devant l'habitacle (sinon la
                // caméra se retrouve dans la carrosserie).
                cam.target = pos + car.up * 1.45 + car.fwd * 11.0;
                cam.distance = 9.6;
                cam.pitch = (0.0 + slope * 0.5).clamp(-0.2, 0.3);
            }
            _ => {
                cam.target = pos + car.up * 1.35 + heading * (speed * 0.025);
                cam.distance = 8.6 + speed * 0.03;
                cam.pitch = (0.2 + slope).clamp(0.05, 0.6);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> AppState {
        let mut app = AppState::default();
        app.load_herroad_demo();
        app
    }

    #[test]
    fn loading_the_demo_creates_a_race_session() {
        let app = loaded();
        assert!(app.race.is_some());
        assert!(app.camera.far > 300.0);
    }

    #[test]
    fn stepping_runs_the_countdown_then_lets_the_car_drive() {
        let mut app = loaded();
        app.input_state.race.throttle = 1.0;
        for _ in 0..(3.2 * 60.0) as usize {
            app.race_step(1.0 / 60.0);
        }
        assert_eq!(app.race.as_ref().unwrap().race.phase, Phase::Running);
        for _ in 0..120 {
            app.race_step(1.0 / 60.0);
        }
        let s = app.race.as_ref().unwrap();
        assert!(s.car.kmh() > 60.0, "la voiture doit rouler ({} km/h)", s.car.kmh());
        // Le HUD reflète la vitesse et le chrono.
        assert!(app.hud_texts["hr_speed"].parse::<f32>().unwrap() > 60.0);
        assert!(app.hud_texts["hr_time"].starts_with("0:0"));
        // La scène suit la voiture.
        let body = s.layout.car[0].index;
        assert!(app.scene.objects[body].transform.position.distance(s.car.pos) < 3.0);
    }

    #[test]
    fn objects_added_at_play_start_do_not_stop_the_race() {
        // Régression : le moteur ajoute des objets à l'entrée en Play ; la course ne doit pas
        // être abandonnée (compte à rebours figé sur 3).
        let mut app = loaded();
        app.input_state.race.throttle = 1.0;
        app.scene.objects.push(crate::scene::SceneObject::default());
        app.scene.objects.push(crate::scene::SceneObject::default());
        for _ in 0..(3.5 * 60.0) as usize {
            app.race_step(1.0 / 60.0);
        }
        let s = app.race.as_ref().expect("la course doit continuer");
        assert_eq!(s.race.phase, Phase::Running);
    }

    #[test]
    fn real_play_start_keeps_the_race_alive_and_counts_down() {
        // Chemin réel (Player) : `advance_play` traite l'entrée en Play (snapshot, physique,
        // objets ajoutés par le moteur) avant les pas fixes.
        let mut app = loaded();
        app.player = true;
        app.playing = true;
        app.input_state.race.throttle = 1.0;
        assert!(app.advance_steps(240));
        let s = app.race.as_ref().expect("la course ne doit pas être abandonnée");
        assert_eq!(s.race.phase, Phase::Running, "le 3-2-1 doit se terminer");
        assert!(app.hud_texts.contains_key("hr_time"));
    }

    #[test]
    fn a_flickering_restart_button_never_restarts_the_race() {
        // Régression : manette qui bruite (bouton enfoncé/relâché à chaque pas) → le
        // compte à rebours repartait à 3 en boucle et la course ne démarrait jamais.
        let mut app = loaded();
        app.input_state.race.throttle = 1.0;
        for i in 0..(4.0 * 60.0) as usize {
            app.input_state.race.restart = i % 2 == 0;
            app.input_state.race.respawn = i % 3 == 0;
            app.race_step(1.0 / 60.0);
        }
        let s = app.race.as_ref().unwrap();
        assert_eq!(s.race.phase, Phase::Running, "le 3-2-1 doit aller à son terme");
        assert_eq!(s.race.respawns, 0);
    }

    #[test]
    fn the_countdown_waits_for_the_first_input() {
        let mut app = loaded();
        for _ in 0..(5.0 * 60.0) as usize {
            app.race_step(1.0 / 60.0);
        }
        assert_eq!(app.hud_texts["hr_center"], "PRÊT ?", "écran d'attente sans geste");
        assert_eq!(app.race.as_ref().unwrap().race.phase, Phase::Countdown);
        app.input_state.race.throttle = 1.0;
        for _ in 0..(3.2 * 60.0) as usize {
            app.race_step(1.0 / 60.0);
        }
        assert_eq!(app.race.as_ref().unwrap().race.phase, Phase::Running);
    }

    #[test]
    fn driving_backwards_raises_the_wrong_way_warning() {
        let mut app = loaded();
        app.input_state.race.throttle = 1.0;
        for _ in 0..(3.3 * 60.0) as usize {
            app.race_step(1.0 / 60.0);
        }
        // Voiture retournée : cap et vitesse à l'opposé de la route.
        {
            let s = app.race.as_mut().unwrap();
            let f = s.track.frames[s.car.frame];
            s.car.fwd = -f.fwd;
            s.car.vel = -f.fwd * 30.0;
        }
        app.input_state.race.throttle = 0.0;
        for _ in 0..90 {
            let s = app.race.as_mut().unwrap();
            let f = s.track.frames[s.car.frame];
            s.car.vel = -f.fwd * 30.0;
            app.race_step(1.0 / 60.0);
        }
        assert!(app.hud_texts["hr_sub"].contains("SENS INTERDIT"), "{:?}", app.hud_texts["hr_sub"]);
    }

    #[test]
    fn changing_scene_drops_the_race() {
        let mut app = loaded();
        app.scene = Scene::demo();
        app.race_step(1.0 / 60.0);
        assert!(app.race.is_none());
    }

    #[test]
    fn restart_button_resets_the_race() {
        let mut app = loaded();
        app.input_state.race.throttle = 1.0;
        for _ in 0..400 {
            app.race_step(1.0 / 60.0);
        }
        app.input_state.race.restart = true;
        for _ in 0..30 {
            app.race_step(1.0 / 60.0);
        }
        let s = app.race.as_ref().unwrap();
        assert_eq!(s.race.phase, Phase::Countdown);
        assert!(s.car.speed() < 1.0);
    }
}
