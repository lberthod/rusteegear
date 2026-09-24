//! Prendre et lancer avec les mains (suivi OpenXR) ou les manettes.
//!
//! Leçons du 1er test au casque : le lancer « ne partait pas ». Trois causes,
//! trois corrections :
//! - la prise se souvient de son **geste** (pincement ou poing) et se relâche
//!   selon ce même geste — avant, il fallait à la fois écarter pouce et index
//!   ET déplier tous les doigts, ce qu'on ne fait pas en lançant ;
//! - la vitesse du lancer se mesure sur la **paume** (articulation stable),
//!   pas sur le point de prise qui sautait du pincement à la paume au lâcher ;
//! - les doigts ne heurtent plus la boule qu'on lâche (groupes de collision,
//!   cf. `world`).

use std::collections::VecDeque;

use glam::Vec3;
use rapier3d::prelude::RigidBodyHandle;

use crate::xr::hands::{INDEX_TIP, THUMB_TIP, pinch_distance};
use crate::xr::input::XrInput;

/// Pouce-index sous ce seuil : pincement (prise).
const PINCH_ON: f32 = 0.022;
/// Pouce-index au-dessus : pincement relâché.
const PINCH_OFF: f32 = 0.038;
/// Distance moyenne bouts des doigts → paume sous ce seuil : poing fermé.
const FIST_ON: f32 = 0.065;
const FIST_OFF: f32 = 0.08;
/// Manette : poignée ou gâchette.
const PRESS_ON: f32 = 0.6;
const PRESS_OFF: f32 = 0.35;
/// Fenêtre (s) de mesure de la vitesse au lâcher.
const THROW_WINDOW: f32 = 0.07;

/// Mesures brutes d'une main à une image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandFrame {
    /// Référence stable pour la vitesse (paume, ou poignée de la manette).
    pub anchor: Vec3,
    /// Point de prise en pincement (entre pouce et index).
    pub pinch_point: Vec3,
    /// Point de prise au poing ou à la manette.
    pub grip_point: Vec3,
    pub pinch: f32,
    pub fist: f32,
    /// Manette : max(poignée, gâchette) ; `None` pour une main suivie.
    pub press: Option<f32>,
    /// Points de contact avec les blocs (paume, bouts des doigts) et rayon.
    pub contacts: [(Vec3, f32); 6],
}

const AWAY: (Vec3, f32) = (Vec3::new(0.0, -100.0, 0.0), 0.01);

impl HandFrame {
    /// Main suivie (26 articulations OpenXR) ou manette ; `offset` : position
    /// du point de départ du joueur dans le monde (tout le reste est dans la
    /// pièce).
    pub fn from_input(input: &XrInput, hand: usize, offset: Vec3) -> Option<Self> {
        if let Some(j) = &input.hand_joints[hand] {
            let palm = j[0] + offset;
            let tips = [THUMB_TIP, INDEX_TIP, 15, 20, 25];
            let fist = tips[1..].iter().map(|&t| j[t].distance(j[0])).sum::<f32>() / 4.0;
            let mut contacts = [AWAY; 6];
            contacts[0] = (palm, 0.035);
            for (c, &t) in contacts[1..].iter_mut().zip(&tips) {
                *c = (j[t] + offset, 0.012);
            }
            // Au poing, la boule est tenue dans le creux de la main : entre la
            // paume et le bout du majeur replié.
            let grip_point = (palm + j[15] + offset) * 0.5;
            return Some(Self {
                anchor: palm,
                pinch_point: (j[THUMB_TIP] + j[INDEX_TIP]) * 0.5 + offset,
                grip_point,
                pinch: pinch_distance(j),
                fist,
                press: None,
                contacts,
            });
        }
        let h = &input.hands[hand];
        let (pos, rot) = h.grip?;
        let pos = pos + offset;
        let mut contacts = [AWAY; 6];
        contacts[0] = (pos, 0.05);
        Some(Self {
            anchor: pos,
            pinch_point: pos,
            grip_point: pos + rot * Vec3::new(0.0, -0.02, -0.06),
            pinch: 1.0,
            fist: 1.0,
            press: Some(h.squeeze.max(h.trigger)),
            contacts,
        })
    }
}

/// Geste qui tient la boule : il décide du point de prise et du relâcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    Pinch,
    Fist,
    Controller,
}

/// Ce que fait la main à cette image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Grab {
    Idle,
    /// Vient de se fermer ici.
    Start(Vec3),
    /// Tient, la boule doit être ici.
    Hold(Vec3),
    /// Vient de s'ouvrir : vitesse de la main (m/s).
    Release(Vec3),
}

#[derive(Debug, Default)]
pub struct Grabber {
    hold: Option<Hold>,
    pub held: Option<RigidBodyHandle>,
    history: VecDeque<(f32, Vec3)>,
}

impl Grabber {
    pub fn reset(&mut self) {
        self.hold = None;
        self.history.clear();
    }

    pub fn closed(&self) -> bool {
        self.hold.is_some()
    }

    pub fn step(&mut self, t: f32, f: &HandFrame, can_grab: bool) -> Grab {
        self.history.push_back((t, f.anchor));
        while self.history.len() > 2 && t - self.history[0].0 > 0.3 {
            self.history.pop_front();
        }
        match self.hold {
            None => {
                if !can_grab {
                    return Grab::Idle;
                }
                let hold = match f.press {
                    Some(p) if p > PRESS_ON => Some(Hold::Controller),
                    Some(_) => None,
                    None if f.pinch < PINCH_ON => Some(Hold::Pinch),
                    None if f.fist < FIST_ON => Some(Hold::Fist),
                    None => None,
                };
                match hold {
                    Some(h) => {
                        self.hold = Some(h);
                        Grab::Start(Self::point(h, f))
                    }
                    None => Grab::Idle,
                }
            }
            Some(h) => {
                let released = match h {
                    Hold::Pinch => f.pinch > PINCH_OFF,
                    Hold::Fist => f.fist > FIST_OFF,
                    Hold::Controller => f.press.unwrap_or(0.0) < PRESS_OFF,
                };
                if released {
                    self.hold = None;
                    Grab::Release(self.velocity())
                } else {
                    Grab::Hold(Self::point(h, f))
                }
            }
        }
    }

    fn point(h: Hold, f: &HandFrame) -> Vec3 {
        match h {
            Hold::Pinch => f.pinch_point,
            Hold::Fist | Hold::Controller => f.grip_point,
        }
    }

    /// Vitesse de la paume sur les `THROW_WINDOW` dernières secondes, la
    /// dernière image exclue : au moment où les doigts s'ouvrent, la main
    /// freine déjà.
    fn velocity(&self) -> Vec3 {
        let n = self.history.len();
        if n < 3 {
            return Vec3::ZERO;
        }
        let (t1, p1) = self.history[n - 2];
        let (t0, p0) = self
            .history
            .iter()
            .take(n - 2)
            .rev()
            .find(|(t, _)| t1 - t >= THROW_WINDOW)
            .copied()
            .unwrap_or(self.history[0]);
        if t1 - t0 < 1e-3 {
            return Vec3::ZERO;
        }
        (p1 - p0) / (t1 - t0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xr::hands::synthetic_hand;
    use crate::xr::input::RIGHT;
    use glam::Quat;

    fn frame(wrist: Vec3, pinch: f32, fist: f32) -> HandFrame {
        let mut input = XrInput::default();
        input.hand_joints[RIGHT] = Some(synthetic_hand(wrist, Quat::IDENTITY, true, pinch, fist));
        HandFrame::from_input(&input, RIGHT, Vec3::ZERO).expect("main suivie")
    }

    /// Le geste du 1er test : pincer, lancer vers l'avant en ouvrant
    /// seulement pouce et index (les autres doigts restent à moitié pliés).
    #[test]
    fn a_pinch_throw_releases_forward_even_with_curled_fingers() {
        let mut g = Grabber::default();
        let dt = 1.0 / 72.0;
        let mut w = Vec3::new(0.2, 1.2, 0.0);
        assert!(matches!(g.step(0.0, &frame(w, 1.0, 0.5), true), Grab::Start(_)));
        let mut t = 0.0;
        for _ in 0..10 {
            t += dt;
            w.z -= 5.0 * dt;
            assert!(matches!(g.step(t, &frame(w, 1.0, 0.5), true), Grab::Hold(_)));
        }
        t += dt;
        w.z -= 5.0 * dt;
        match g.step(t, &frame(w, 0.0, 0.5), true) {
            Grab::Release(v) => assert!(v.z < -4.0, "lancé vers l'avant : {v:?}"),
            other => panic!("devait lâcher : {other:?}"),
        }
    }

    #[test]
    fn a_fist_grab_releases_when_the_fingers_open() {
        let mut g = Grabber::default();
        let w = Vec3::new(0.2, 1.2, 0.0);
        assert!(matches!(g.step(0.0, &frame(w, 0.0, 1.0), true), Grab::Start(_)));
        assert!(matches!(g.step(0.02, &frame(w, 0.0, 1.0), true), Grab::Hold(_)));
        assert!(matches!(g.step(0.04, &frame(w, 0.0, 0.0), true), Grab::Release(_)));
    }

    #[test]
    fn no_grab_while_a_menu_is_open() {
        let mut g = Grabber::default();
        let w = Vec3::new(0.2, 1.2, 0.0);
        assert_eq!(g.step(0.0, &frame(w, 1.0, 0.0), false), Grab::Idle);
    }
}
