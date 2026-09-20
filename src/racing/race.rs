//! Règles de course : compte à rebours, points de passage pris dans l'ordre, tours,
//! temps intermédiaires comparés au meilleur tour, points de réapparition, médailles et
//! fantôme du meilleur run. Pur : aucune dépendance à la scène ni au rendu.

use glam::Vec3;

use super::car::Car;
use super::track::Track;

/// Nombre de tours d'une course.
pub const LAPS: u32 = 3;
/// Durée du compte à rebours 3-2-1 avant le « GO » (s).
pub const COUNTDOWN_SECS: f32 = 3.0;
/// Durée d'affichage du « GO ! » (s).
pub const GO_SECS: f32 = 0.9;

/// Temps total (s) des médailles sur 3 tours.
// Repère : le pilote automatique de test boucle les 3 tours en ~100 s.
pub const MEDAL_AUTHOR: f32 = 92.0;
pub const MEDAL_GOLD: f32 = 100.0;
pub const MEDAL_SILVER: f32 = 112.0;
pub const MEDAL_BRONZE: f32 = 130.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Countdown,
    Running,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Medal {
    None,
    Bronze,
    Silver,
    Gold,
    Author,
}

impl Medal {
    pub fn for_time(t: f32) -> Medal {
        if t <= MEDAL_AUTHOR {
            Medal::Author
        } else if t <= MEDAL_GOLD {
            Medal::Gold
        } else if t <= MEDAL_SILVER {
            Medal::Silver
        } else if t <= MEDAL_BRONZE {
            Medal::Bronze
        } else {
            Medal::None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Medal::None => "—",
            Medal::Bronze => "BRONZE",
            Medal::Silver => "ARGENT",
            Medal::Gold => "OR",
            Medal::Author => "AUTEUR",
        }
    }
}

/// Événements de course, consommés par l'interface (sons, bandeaux, couleurs de portique).
#[derive(Clone, Debug, PartialEq)]
pub enum RaceEvent {
    /// Chiffre du compte à rebours affiché (3, 2, 1).
    Beep(u32),
    Go,
    /// Point de passage `index` franchi ; `delta` = écart au meilleur tour (s, < 0 = en avance).
    Checkpoint { index: usize, delta: Option<f32> },
    LapDone { lap: u32, time: f32, best: bool },
    Finished { total: f32, record: bool },
    Respawn,
    Restart,
}

#[derive(Clone, Copy, Debug)]
pub struct GhostSample {
    pub pos: Vec3,
    pub fwd: Vec3,
    pub up: Vec3,
}

/// Point de réapparition : position et repère de la voiture au dernier point de passage.
#[derive(Clone, Copy, Debug)]
pub struct Spawn {
    pub pos: Vec3,
    pub fwd: Vec3,
    pub up: Vec3,
    pub frame: usize,
}

pub struct Race {
    pub phase: Phase,
    /// Temps écoulé depuis le début du compte à rebours (s).
    pub clock: f32,
    /// Temps de course depuis le « GO » (s).
    pub time: f32,
    pub lap: u32,
    /// Prochain portique attendu : 0..N-1 = points de passage, N = ligne d'arrivée.
    pub next_gate: usize,
    pub lap_start: f32,
    pub splits: Vec<f32>,
    pub best_splits: Vec<f32>,
    pub best_lap: Option<f32>,
    pub lap_times: Vec<f32>,
    pub last_delta: Option<f32>,
    pub spawn: Spawn,
    pub respawns: u32,
    pub total: Option<f32>,
    pub medal: Medal,
    /// Record de temps total (session courante ou fichier).
    pub best_total: Option<f32>,
    pub recording: Vec<GhostSample>,
    pub ghost: Vec<GhostSample>,
    pub events: Vec<RaceEvent>,
    start_spawn: Spawn,
    beeps: u32,
    gates: usize,
}

impl Race {
    pub fn new(track: &Track, best_total: Option<f32>) -> Race {
        let f = track.frames[track.start_frame()];
        let spawn = Spawn {
            pos: f.pos,
            fwd: f.fwd,
            up: f.up,
            frame: track.start_frame(),
        };
        let gates = track.checkpoints.len() + 1;
        Race {
            phase: Phase::Countdown,
            clock: 0.0,
            time: 0.0,
            lap: 0,
            next_gate: 0,
            lap_start: 0.0,
            splits: Vec::new(),
            best_splits: Vec::new(),
            best_lap: None,
            lap_times: Vec::new(),
            last_delta: None,
            spawn,
            respawns: 0,
            total: None,
            medal: Medal::None,
            best_total,
            recording: Vec::new(),
            ghost: Vec::new(),
            events: Vec::new(),
            start_spawn: spawn,
            beeps: 0,
            gates,
        }
    }

    /// Nombre de portiques par tour (points de passage + arrivée).
    pub fn gate_count(&self) -> usize {
        self.gates
    }

    /// Recommence une course : garde les records, le fantôme et les meilleurs intermédiaires.
    pub fn restart(&mut self) {
        self.phase = Phase::Countdown;
        self.clock = 0.0;
        self.time = 0.0;
        self.lap = 0;
        self.next_gate = 0;
        self.lap_start = 0.0;
        self.splits.clear();
        self.lap_times.clear();
        self.last_delta = None;
        self.spawn = self.start_spawn;
        self.respawns = 0;
        self.total = None;
        self.medal = Medal::None;
        self.recording.clear();
        self.beeps = 0;
        self.events.push(RaceEvent::Restart);
    }

    /// Chiffre du compte à rebours à afficher (3, 2, 1), `0` pour « GO ! », `None` sinon.
    pub fn countdown_digit(&self) -> Option<u32> {
        match self.phase {
            Phase::Countdown => Some((COUNTDOWN_SECS - self.clock).ceil().max(1.0) as u32),
            Phase::Running if self.time < GO_SECS => Some(0),
            _ => None,
        }
    }

    /// Fantôme à la date `time` de la course (échantillonné à 60 Hz).
    pub fn ghost_at(&self, time: f32) -> Option<GhostSample> {
        if self.ghost.is_empty() {
            return None;
        }
        let i = (time * 60.0) as usize;
        self.ghost.get(i).copied()
    }

    /// Fait avancer les règles d'un pas. La voiture a déjà été intégrée ; `prev` est sa
    /// position avant ce pas.
    pub fn update(&mut self, dt: f32, car: &Car, prev: Vec3, track: &Track) {
        match self.phase {
            Phase::Countdown => {
                let before = self.clock;
                if self.beeps == 0 {
                    self.beeps = 1;
                    self.events.push(RaceEvent::Beep(COUNTDOWN_SECS.ceil() as u32));
                }
                self.clock += dt;
                let digit = (COUNTDOWN_SECS - self.clock).ceil() as i32;
                let prev_digit = (COUNTDOWN_SECS - before).ceil() as i32;
                if digit != prev_digit && digit >= 1 && self.beeps < 3 {
                    self.beeps += 1;
                    self.events.push(RaceEvent::Beep(digit as u32));
                }
                if self.clock >= COUNTDOWN_SECS {
                    self.phase = Phase::Running;
                    self.time = 0.0;
                    self.events.push(RaceEvent::Go);
                }
            }
            Phase::Running => {
                self.time += dt;
                self.recording.push(GhostSample {
                    pos: car.pos,
                    fwd: car.fwd,
                    up: car.up,
                });
                self.check_gates(dt, car, prev, track);
            }
            Phase::Finished => {}
        }
    }

    fn gate_frame(&self, track: &Track, gate: usize) -> usize {
        if gate < track.checkpoints.len() {
            track.checkpoints[gate]
        } else {
            0
        }
    }

    fn check_gates(&mut self, dt: f32, car: &Car, prev: Vec3, track: &Track) {
        let gate = self.next_gate;
        let fi = self.gate_frame(track, gate);
        let f = &track.frames[fi];
        let d0 = (prev - f.pos).dot(f.fwd);
        let d1 = (car.pos - f.pos).dot(f.fwd);
        if !(d0 < 0.0 && d1 >= 0.0) {
            return;
        }
        let lateral = (car.pos - f.pos).dot(f.right).abs();
        if lateral > f.half_width + 3.0 || (car.pos.y - f.pos.y).abs() > 6.0 {
            return;
        }
        // Instant exact du franchissement, interpolé à l'intérieur du pas.
        let frac = if d1 > d0 { d1 / (d1 - d0) } else { 0.0 };
        let t_cross = self.time - dt * frac;
        let lap_time = t_cross - self.lap_start;

        if gate < track.checkpoints.len() {
            let delta = self.best_splits.get(gate).map(|b| lap_time - b);
            self.splits.push(lap_time);
            self.last_delta = delta;
            self.events.push(RaceEvent::Checkpoint { index: gate, delta });
            self.spawn = Spawn {
                pos: f.pos + f.up * 0.05,
                fwd: f.fwd,
                up: f.up,
                frame: fi,
            };
            self.next_gate += 1;
        } else {
            // Ligne d'arrivée : tour bouclé.
            self.lap += 1;
            let best = self.best_lap.is_none_or(|b| lap_time < b);
            if best {
                self.best_lap = Some(lap_time);
                self.best_splits = self.splits.clone();
            }
            self.lap_times.push(lap_time);
            self.events.push(RaceEvent::LapDone {
                lap: self.lap,
                time: lap_time,
                best,
            });
            self.splits.clear();
            self.lap_start = t_cross;
            self.next_gate = 0;
            self.spawn = Spawn {
                pos: f.pos + f.up * 0.05,
                fwd: f.fwd,
                up: f.up,
                frame: fi,
            };
            if self.lap >= LAPS {
                self.finish(t_cross);
            }
        }
    }

    fn finish(&mut self, total: f32) {
        self.phase = Phase::Finished;
        self.total = Some(total);
        self.medal = Medal::for_time(total);
        let record = self.best_total.is_none_or(|b| total < b);
        if record {
            self.best_total = Some(total);
            self.ghost = std::mem::take(&mut self.recording);
        }
        self.events.push(RaceEvent::Finished { total, record });
    }

    /// Réapparition au dernier point de passage : le chrono continue, on compte le nombre
    /// de retours.
    pub fn note_respawn(&mut self) {
        self.respawns += 1;
        self.events.push(RaceEvent::Respawn);
    }
}

/// Format `m:ss.mmm`, comme le chrono d'un jeu de course.
pub fn fmt_time(t: f32) -> String {
    let t = t.max(0.0);
    let ms = (t * 1000.0).round() as u32;
    format!("{}:{:02}.{:03}", ms / 60000, (ms / 1000) % 60, ms % 1000)
}

/// Écart signé `+0.42` / `-0.13`.
pub fn fmt_delta(d: f32) -> String {
    format!("{}{:.2}", if d < 0.0 { "-" } else { "+" }, d.abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::racing::track::TrackSpec;

    #[test]
    fn time_formatting() {
        assert_eq!(fmt_time(0.0), "0:00.000");
        assert_eq!(fmt_time(83.4567), "1:23.457");
        assert_eq!(fmt_delta(-0.129), "-0.13");
        assert_eq!(fmt_delta(1.5), "+1.50");
    }

    #[test]
    fn medals_follow_the_thresholds() {
        assert_eq!(Medal::for_time(MEDAL_AUTHOR - 0.1), Medal::Author);
        assert_eq!(Medal::for_time(MEDAL_GOLD - 0.1), Medal::Gold);
        assert_eq!(Medal::for_time(MEDAL_SILVER - 0.1), Medal::Silver);
        assert_eq!(Medal::for_time(MEDAL_BRONZE - 0.1), Medal::Bronze);
        assert_eq!(Medal::for_time(MEDAL_BRONZE + 5.0), Medal::None);
    }

    #[test]
    fn countdown_then_go() {
        let t = Track::build(&TrackSpec::herroad());
        let mut r = Race::new(&t, None);
        let f = t.frames[t.start_frame()];
        let car = Car::placed(f.pos, f.fwd, f.up, t.start_frame());
        assert_eq!(r.countdown_digit(), Some(3));
        for _ in 0..(COUNTDOWN_SECS * 60.0) as usize + 2 {
            r.update(1.0 / 60.0, &car, car.pos, &t);
        }
        assert_eq!(r.phase, Phase::Running);
        assert!(r.events.contains(&RaceEvent::Go));
        let beeps = r
            .events
            .iter()
            .filter(|e| matches!(e, RaceEvent::Beep(_)))
            .count();
        assert_eq!(beeps, 3, "trois bips 3-2-1");
    }

    #[test]
    fn gates_must_be_crossed_in_order() {
        let t = Track::build(&TrackSpec::herroad());
        let mut r = Race::new(&t, None);
        r.phase = Phase::Running;
        // Franchir directement le 2e point de passage ne valide rien.
        let f2 = t.frames[t.checkpoints[1]];
        let mut car = Car::placed(f2.pos - f2.fwd, f2.fwd, f2.up, t.checkpoints[1]);
        let prev = car.pos;
        car.pos = f2.pos + f2.fwd;
        r.update(1.0 / 60.0, &car, prev, &t);
        assert_eq!(r.next_gate, 0);
        // Franchir le premier, si.
        let f1 = t.frames[t.checkpoints[0]];
        let prev = f1.pos - f1.fwd;
        car.pos = f1.pos + f1.fwd;
        r.update(1.0 / 60.0, &car, prev, &t);
        assert_eq!(r.next_gate, 1);
        assert_eq!(r.spawn.frame, t.checkpoints[0]);
    }
}
