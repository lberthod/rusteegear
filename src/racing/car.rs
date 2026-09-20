//! Voiture arcade à la Trackmania : pas de rapier ici, un modèle maison de quelques lignes
//! qu'on maîtrise entièrement — accélération à courbe de moteur, freinage/marche arrière,
//! adhérence latérale qui glisse au frein à main, gravité sur pente, vol libre après une
//! rampe, barrières qui renvoient la voiture. Les sauts, le dévers et les ponts tombent du
//! fait que le sol est un maillage interrogé sous la voiture (cf. `Track::ground_at`).

use glam::{Quat, Vec2, Vec3};

use super::track::{Surface, Track};

/// Gravité (m/s²) : plus forte que la réalité pour des sauts vifs et lisibles.
pub const GRAVITY: f32 = 24.0;
/// Rayon de collision (plan) contre les barrières.
pub const CAR_RADIUS: f32 = 1.15;
/// Vitesse de pointe nominale sur asphalte (m/s), ≈ 280 km/h.
pub const TOP_SPEED: f32 = 78.0;

/// Commandes du pilote, déjà résolues (manette ou clavier) : `throttle` et `brake` dans
/// [0, 1], `steer` dans [-1, 1] (+1 = braquage à droite).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CarInput {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub handbrake: bool,
}

/// Ce qui s'est passé pendant un pas — sons, secousses, particules.
#[derive(Clone, Copy, Debug, Default)]
pub struct StepInfo {
    /// Vitesse normale à l'impact d'un atterrissage (m/s).
    pub landed: Option<f32>,
    /// Vitesse d'impact contre une barrière (m/s).
    pub wall: Option<f32>,
}

/// Rotation qui envoie +Z sur `fwd` et +Y sur `up` (repère droit : +X part vers la gauche
/// du véhicule vu de derrière).
pub fn basis_rotation(fwd: Vec3, up: Vec3) -> Quat {
    let z = fwd.normalize_or_zero();
    let x = up.cross(z).normalize_or_zero();
    let y = z.cross(x);
    Quat::from_mat3(&glam::Mat3::from_cols(x, y, z))
}

#[derive(Clone, Debug)]
pub struct Car {
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub vel: Vec3,
    pub fwd: Vec3,
    pub up: Vec3,
    pub grounded: bool,
    /// Temps passé en l'air sans discontinuer (s).
    pub air_time: f32,
    /// Temps de turbo restant (s).
    pub boost: f32,
    /// Braquage lissé (-1..1) — sert aussi à orienter les roues avant.
    pub steer: f32,
    /// Intensité de dérapage 0..1 (fumée, traces).
    pub drift: f32,
    pub surface: Surface,
    /// Dernier repère de piste sous la voiture.
    pub frame: usize,
}

impl Car {
    pub fn placed(pos: Vec3, fwd: Vec3, up: Vec3, frame: usize) -> Car {
        let up = up.normalize_or_zero();
        let fwd = (fwd - up * fwd.dot(up)).normalize_or_zero();
        Car {
            pos,
            prev_pos: pos,
            vel: Vec3::ZERO,
            fwd,
            up,
            grounded: true,
            air_time: 0.0,
            boost: 0.0,
            steer: 0.0,
            drift: 0.0,
            surface: Surface::Asphalt,
            frame,
        }
    }

    pub fn right(&self) -> Vec3 {
        self.fwd.cross(self.up).normalize_or_zero()
    }

    /// Vitesse le long du cap (m/s, négative en marche arrière).
    pub fn forward_speed(&self) -> f32 {
        self.vel.dot(self.fwd)
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    pub fn kmh(&self) -> f32 {
        self.speed() * 3.6
    }

    /// Orientation monde de la caisse (axe Z = cap, axe Y = haut).
    pub fn rotation(&self) -> Quat {
        basis_rotation(self.fwd, self.up)
    }

    pub fn step(&mut self, inp: CarInput, track: &Track, dt: f32) -> StepInfo {
        let mut info = StepInfo::default();
        self.prev_pos = self.pos;
        self.boost = (self.boost - dt).max(0.0);

        // Braquage lissé : un stick analogique arrive déjà progressif, un clavier saute de
        // 0 à ±1 — dans les deux cas la direction ne claque jamais.
        let target = inp.steer.clamp(-1.0, 1.0);
        let rate = if target.abs() >= self.steer.abs() { 7.5 } else { 11.0 };
        self.steer += (target - self.steer).clamp(-rate * dt, rate * dt);

        if self.grounded {
            self.drive_on_ground(inp, dt);
        } else {
            self.fly(dt);
        }

        // Intégration.
        self.pos += self.vel * dt;

        // Barrières : deux passes pour les coins.
        for _ in 0..2 {
            let Some(hit) = track.wall_hit(self.pos, CAR_RADIUS) else {
                break;
            };
            self.pos.x += hit.normal.x * hit.depth;
            self.pos.z += hit.normal.y * hit.depth;
            let v2 = Vec2::new(self.vel.x, self.vel.z);
            let vn = v2.dot(hit.normal);
            if vn < 0.0 {
                info.wall = Some(info.wall.unwrap_or(0.0).max(-vn));
                let tangent = v2 - hit.normal * vn;
                // Rebond amorti + frottement : on perd de la vitesse, pas la trajectoire.
                let out = tangent * 0.93 + hit.normal * (-vn * 0.18);
                self.vel.x = out.x;
                self.vel.z = out.y;
            }
        }

        // Sol.
        let ground = track.ground_at(self.pos.x, self.pos.z, self.pos.y + 0.7);
        let mut touching = false;
        if let Some(g) = ground {
            let dy = self.pos.y - g.y;
            let vn = self.vel.dot(g.normal);
            let glued = self.grounded && dy < 0.35 && vn < 6.0;
            if dy <= 0.05 || glued {
                if !self.grounded && vn < 0.0 {
                    info.landed = Some(-vn);
                }
                self.pos.y = g.y;
                if vn < 0.0 || self.grounded {
                    self.vel -= g.normal * vn;
                }
                let k = 1.0 - (-16.0 * dt).exp();
                self.up = self.up.lerp(g.normal, k).normalize_or_zero();
                self.fwd = (self.fwd - self.up * self.fwd.dot(self.up)).normalize_or_zero();
                self.surface = g.surface;
                self.frame = g.frame;
                touching = true;
            }
        }
        if touching {
            self.grounded = true;
            self.air_time = 0.0;
        } else {
            self.grounded = false;
            self.air_time += dt;
        }
        info
    }

    fn drive_on_ground(&mut self, inp: CarInput, dt: f32) {
        let surf = self.surface;
        let right = self.right();
        let mut vf = self.vel.dot(self.fwd);
        let mut vr = self.vel.dot(right);
        let boosting = self.boost > 0.0;
        let vmax = TOP_SPEED * surf.top_speed() * if boosting { 1.35 } else { 1.0 };

        // Poussée moteur, freinage, marche arrière.
        let mut acc = 0.0;
        if boosting {
            acc += 46.0;
        }
        if inp.throttle > 0.0 && inp.brake < 0.05 {
            let k = (1.0 - vf / vmax).clamp(0.0, 1.6);
            acc += 25.0 * inp.throttle * surf.engine() * k;
        }
        if inp.brake > 0.0 {
            if vf > 1.0 {
                acc -= 46.0 * inp.brake * surf.grip().sqrt();
            } else {
                let k = (1.0 + vf / 16.0).clamp(0.0, 1.0);
                acc -= 16.0 * inp.brake * k * surf.engine();
            }
        }
        vf += acc * dt;
        // Résistances (roulement + air), sans jamais inverser le sens de la marche.
        let drag = (1.2 + 0.0009 * vf * vf) * dt;
        vf = if drag >= vf.abs() { 0.0 } else { vf - drag * vf.signum() };

        // Direction : rayon de braquage qui s'ouvre avec la vitesse.
        let sp = vf.abs();
        let base = 2.35 - 1.55 * (sp / 65.0).clamp(0.0, 1.0);
        let dir = (vf / 5.0).clamp(-1.0, 1.0);
        let mut yaw_rate = -self.steer * base * dir;
        let mut grip = 15.0 * surf.grip();
        grip *= 1.0 - 0.45 * self.steer.abs() * (sp / 70.0).clamp(0.0, 1.0);
        let mut drifting = 0.0;
        if inp.handbrake && sp > 8.0 {
            grip *= 0.16;
            yaw_rate *= 1.55;
            vf -= vf.signum() * 3.0 * dt;
            drifting = 1.0;
        }

        // Adhérence latérale : la vitesse de dérapage fond, l'énergie retirée retourne en
        // partie dans l'avance (un virage propre garde sa vitesse, un dérapage en perd un peu).
        let vr_new = vr * (-grip * dt).exp();
        let removed = vr * vr - vr_new * vr_new;
        let signed = if vf < 0.0 { -1.0 } else { 1.0 };
        vf = signed * (vf * vf + 0.88 * removed).sqrt();
        vr = vr_new;

        let slide = (vr.abs() / sp.max(6.0)).clamp(0.0, 1.0);
        let want_drift = (slide * 3.0).clamp(0.0, 1.0).max(drifting * 0.6);
        self.drift += (want_drift - self.drift) * (1.0 - (-8.0 * dt).exp());

        self.vel = self.fwd * vf + right * vr;
        // Gravité tangentielle : pentes qui freinent ou relancent.
        let g = Vec3::new(0.0, -GRAVITY, 0.0);
        self.vel += (g - self.up * g.dot(self.up)) * dt;

        self.fwd = (Quat::from_axis_angle(self.up, yaw_rate * dt) * self.fwd).normalize_or_zero();
    }

    fn fly(&mut self, dt: f32) {
        self.vel.y -= GRAVITY * dt;
        self.vel *= 1.0 - 0.04 * dt;
        self.drift *= 1.0 - (6.0 * dt).min(1.0);
        // Un peu de direction en l'air (cap seulement), pas de poussée.
        let yaw = -self.steer * 0.55;
        self.fwd = (Quat::from_axis_angle(self.up, yaw * dt) * self.fwd).normalize_or_zero();
        // Le nez suit doucement la trajectoire, la caisse revient vers l'horizontale.
        let speed = self.vel.length();
        if speed > 6.0 {
            let want = self.vel / speed;
            self.fwd = self.fwd.lerp(want, (1.6 * dt).min(1.0)).normalize_or_zero();
        }
        let flat_up = (Vec3::Y - self.fwd * self.fwd.dot(Vec3::Y)).normalize_or_zero();
        self.up = self.up.lerp(flat_up, (0.7 * dt).min(1.0)).normalize_or_zero();
        self.up = (self.up - self.fwd * self.fwd.dot(self.up)).normalize_or_zero();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::racing::track::TrackSpec;

    fn setup() -> (Track, Car) {
        let t = Track::build(&TrackSpec::herroad());
        let f = t.frames[t.start_frame()];
        let car = Car::placed(f.pos, f.fwd, f.up, t.start_frame());
        (t, car)
    }

    struct Lcg(u32);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.0 >> 8) as f32 / (1u32 << 24) as f32
        }
    }

    /// Des commandes aléatoires (changées toutes les ~0,3 s, turbo compris) pendant 10 minutes
    /// de jeu ne doivent jamais produire de valeur non finie ni de vitesse absurde, et la
    /// voiture ne doit pas quitter le monde autrement qu'en tombant (détecté par la course).
    #[test]
    fn random_driving_never_produces_nan_or_runaway_speed() {
        let (t, _) = setup();
        for seed in 1..=4u32 {
            let mut rng = Lcg(seed * 7919);
            let f = t.frames[t.start_frame()];
            let mut car = Car::placed(f.pos, f.fwd, f.up, t.start_frame());
            let mut inp = CarInput::default();
            for step in 0..60 * 150 {
                if step % 18 == 0 {
                    inp = CarInput {
                        throttle: if rng.next() < 0.7 { rng.next() } else { 0.0 },
                        brake: if rng.next() < 0.15 { rng.next() } else { 0.0 },
                        steer: rng.next() * 2.0 - 1.0,
                        handbrake: rng.next() < 0.2,
                    };
                }
                if step % 900 == 0 {
                    car.boost = 1.5;
                }
                car.step(inp, &t, 1.0 / 60.0);
                assert!(
                    car.pos.is_finite() && car.vel.is_finite() && car.fwd.is_finite() && car.up.is_finite(),
                    "valeur non finie (graine {seed}, pas {step}) : {car:?}"
                );
                assert!(car.speed() < 160.0, "vitesse absurde {} (graine {seed}, pas {step})", car.speed());
                assert!((car.fwd.length() - 1.0).abs() < 0.01, "cap dénormalisé");
                if car.pos.y < t.min_y - 12.0 {
                    // Tombée : la course l'aurait replacée, on repart de la ligne.
                    car = Car::placed(f.pos, f.fwd, f.up, t.start_frame());
                }
            }
        }
    }

    /// Choc à très haute vitesse (turbo, ~105 m/s) sous tous les angles : la barrière ne doit
    /// jamais être traversée (pas de « tunneling »).
    #[test]
    fn fast_impacts_never_go_through_the_walls() {
        let (t, _) = setup();
        for deg in [10.0_f32, 25.0, 45.0, 70.0, 89.0] {
            for side in [-1.0_f32, 1.0] {
                // Un tronçon droit et protégé sur les 45 repères (90 m) qui suivent.
                let i = (0..t.frames.len() - 50)
                    .find(|&i| {
                        (i..i + 45).all(|k| t.frames[k].wall_l && t.frames[k].wall_r && !t.frames[k].void)
                            && t.frames[i].fwd.dot(t.frames[i + 45].fwd) > 0.995
                    })
                    .expect("un tronçon protégé assez long");
                let f = t.frames[i];
                let mut car = Car::placed(f.pos, f.fwd, f.up, i);
                let a = deg.to_radians();
                car.vel = (f.fwd * a.cos() + f.right * (side * a.sin())) * 105.0;
                car.fwd = car.vel.normalize();
                for _ in 0..40 {
                    car.step(CarInput { throttle: 1.0, ..Default::default() }, &t, 1.0 / 60.0);
                    let fr = &t.frames[t.frame_near(car.pos, i, 20)];
                    let lateral = (car.pos - fr.pos).dot(fr.right).abs();
                    assert!(
                        lateral < fr.half_width + 0.3,
                        "traversée de barrière à {deg}° côté {side} : {lateral} m"
                    );
                }
            }
        }
    }

    #[test]
    fn full_throttle_accelerates_and_stays_on_the_road() {
        let (t, mut car) = setup();
        let inp = CarInput {
            throttle: 1.0,
            ..Default::default()
        };
        for _ in 0..120 {
            car.step(inp, &t, 1.0 / 60.0);
        }
        assert!(car.grounded, "la voiture doit coller au sol");
        assert!(car.kmh() > 80.0, "2 s plein gaz : {} km/h", car.kmh());
        assert!(car.forward_speed() > 0.0);
    }

    #[test]
    fn zero_to_100_kmh_takes_a_couple_of_seconds() {
        let (t, mut car) = setup();
        let inp = CarInput {
            throttle: 1.0,
            ..Default::default()
        };
        let mut secs = 0.0;
        while car.kmh() < 100.0 && secs < 6.0 {
            car.step(inp, &t, 1.0 / 60.0);
            secs += 1.0 / 60.0;
        }
        assert!((1.0..3.5).contains(&secs), "0→100 km/h en {secs} s");
    }

    #[test]
    fn braking_stops_the_car_then_reverses() {
        let (t, mut car) = setup();
        car.vel = car.fwd * 30.0;
        let inp = CarInput {
            brake: 1.0,
            ..Default::default()
        };
        for _ in 0..90 {
            car.step(inp, &t, 1.0 / 60.0);
        }
        assert!(car.forward_speed() < 0.0, "marche arrière attendue");
    }

    #[test]
    fn steering_right_turns_the_car_to_its_right() {
        let (t, mut car) = setup();
        car.vel = car.fwd * 25.0;
        let right0 = car.right();
        let inp = CarInput {
            throttle: 0.5,
            steer: 1.0,
            ..Default::default()
        };
        for _ in 0..40 {
            car.step(inp, &t, 1.0 / 60.0);
        }
        assert!(car.fwd.dot(right0) > 0.2, "le cap doit tourner à droite");
    }

    #[test]
    fn handbrake_makes_the_car_slide_sideways() {
        let (t, mut grip_car) = setup();
        let mut slide_car = grip_car.clone();
        grip_car.vel = grip_car.fwd * 35.0;
        slide_car.vel = slide_car.fwd * 35.0;
        let steer = CarInput {
            throttle: 1.0,
            steer: 0.6,
            ..Default::default()
        };
        let drift = CarInput {
            handbrake: true,
            ..steer
        };
        for _ in 0..24 {
            grip_car.step(steer, &t, 1.0 / 60.0);
            slide_car.step(drift, &t, 1.0 / 60.0);
        }
        // Angle de dérapage : écart entre le cap et la vitesse.
        let slip = |c: &Car| c.vel.dot(c.right()).abs().atan2(c.vel.dot(c.fwd).abs());
        assert!(
            slip(&slide_car) > slip(&grip_car) * 1.5,
            "le frein à main doit faire déraper ({} vs {})",
            slip(&slide_car),
            slip(&grip_car)
        );
        assert!(slide_car.drift > 0.3);
    }

    #[test]
    fn leaving_the_track_makes_the_car_fall() {
        let (t, mut car) = setup();
        car.pos += car.right() * 60.0;
        car.grounded = false;
        for _ in 0..90 {
            car.step(CarInput::default(), &t, 1.0 / 60.0);
        }
        assert!(!car.grounded);
        assert!(car.pos.y < t.frames[t.start_frame()].pos.y - 5.0);
    }

    #[test]
    fn walls_keep_the_car_on_the_road() {
        let (t, mut car) = setup();
        // Plein gaz en braquant à droite jusqu'à la barrière : la voiture ne doit pas sortir.
        let inp = CarInput {
            throttle: 1.0,
            steer: 0.3,
            ..Default::default()
        };
        for _ in 0..240 {
            car.step(inp, &t, 1.0 / 60.0);
            let f = &t.frames[t.frame_near(car.pos, car.frame, 12)];
            let lateral = (car.pos - f.pos).dot(f.right).abs();
            assert!(lateral < f.half_width + 0.5, "sortie de piste ({lateral} m)");
        }
    }
}

