//! Pilote automatique par poursuite de point : sert de banc d'essai (une course complète en
//! test, calibrage des médailles) et de témoin que le circuit est réellement jouable.

use glam::Vec3;

use super::car::{Car, CarInput};
use super::track::{FRAME_STEP, Track};

/// Accélération latérale que le bot s'autorise en virage (m/s²).
const LATERAL_BUDGET: f32 = 34.0;
/// Décélération sur laquelle il compte pour anticiper un virage (m/s²).
const BRAKE_BUDGET: f32 = 30.0;

pub struct Bot {
    hint: usize,
}

impl Bot {
    pub fn new(frame: usize) -> Bot {
        Bot { hint: frame }
    }

    fn turn_at(track: &Track, i: usize) -> f32 {
        let n = track.frames.len();
        let a = track.frames[i % n].fwd;
        let b = track.frames[(i + 1) % n].fwd;
        let cross = a.z * b.x - a.x * b.z;
        (cross.abs().clamp(0.0, 1.0)).asin() / FRAME_STEP
    }

    pub fn drive(&mut self, car: &Car, track: &Track) -> CarInput {
        let n = track.frames.len();
        self.hint = track.frame_near(car.pos, self.hint, 20);
        let speed = car.speed().max(1.0);

        // Point visé : plus loin quand on va vite.
        let look = (5.0 + speed * 0.28) as usize;
        let target = track.frames[(self.hint + look) % n].pos;
        let to = target - car.pos;
        let fwd_h = Vec3::new(car.fwd.x, 0.0, car.fwd.z).normalize_or_zero();
        let right_h = Vec3::new(car.right().x, 0.0, car.right().z).normalize_or_zero();
        let angle = to.dot(right_h).atan2(to.dot(fwd_h));
        let steer = (angle * 2.4).clamp(-1.0, 1.0);

        // Vitesse admissible : virage le plus contraignant à venir, ramené à ici par le
        // freinage disponible.
        let mut allowed = f32::MAX;
        for k in 0..70 {
            let kappa = Self::turn_at(track, self.hint + k).max(1e-4);
            let vt = (LATERAL_BUDGET / kappa).sqrt();
            let dist = k as f32 * FRAME_STEP;
            allowed = allowed.min((vt * vt + 2.0 * BRAKE_BUDGET * dist).sqrt());
        }
        // Dans un saut, on ne touche à rien : plein gaz, cap tenu.
        let (throttle, brake) = if !car.grounded {
            (1.0, 0.0)
        } else if car.speed() > allowed * 1.02 {
            (0.0, ((car.speed() - allowed) / 6.0).clamp(0.2, 1.0))
        } else {
            (1.0, 0.0)
        };
        CarInput {
            throttle,
            brake,
            steer,
            handbrake: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::racing::race::{LAPS, Phase, Race};
    use crate::racing::track::TrackSpec;

    /// Une course complète pilotée par le bot : le circuit est bouclable, sans tomber ni se
    /// bloquer, et le temps obtenu sert de repère pour les médailles.
    #[test]
    fn the_bot_can_finish_a_full_race() {
        let track = Track::build(&TrackSpec::herroad());
        let mut race = Race::new(&track, None);
        let f = track.frames[track.start_frame()];
        let mut car = Car::placed(f.pos, f.fwd, f.up, track.start_frame());
        let mut bot = Bot::new(track.start_frame());
        let dt = 1.0 / 60.0;
        let mut steps = 0;
        let mut falls = 0;
        while race.phase != Phase::Finished && steps < 60 * 300 {
            steps += 1;
            let prev = car.pos;
            if race.phase == Phase::Running {
                let inp = bot.drive(&car, &track);
                car.step(inp, &track, dt);
                if car.pos.y < track.min_y - 8.0 {
                    falls += 1;
                    let s = race.spawn;
                    car = Car::placed(s.pos, s.fwd, s.up, s.frame);
                    bot = Bot::new(s.frame);
                }
            }
            race.update(dt, &car, prev, &track);
            if std::env::var_os("BOT_TRACE").is_some() && (steps % 120 == 0 || (race.time > 22.0 && race.time < 30.0 && steps % 6 == 0)) {
                eprintln!(
                    "t={:.1} frame={} kmh={:.0} grounded={} lap={} gate={} pos=({:.0},{:.0},{:.0})",
                    race.time,
                    car.frame,
                    car.kmh(),
                    car.grounded,
                    race.lap,
                    race.next_gate,
                    car.pos.x,
                    car.pos.y,
                    car.pos.z
                );
            }
        }
        assert_eq!(race.phase, Phase::Finished, "course non terminée après {steps} pas");
        let total = race.total.unwrap();
        eprintln!(
            "bot : {total:.2} s pour {LAPS} tours ({falls} chutes), tours {:?}",
            race.lap_times
        );
        assert_eq!(falls, 0, "le bot ne doit jamais tomber");
        assert_eq!(race.lap_times.len(), LAPS as usize);
    }
}
