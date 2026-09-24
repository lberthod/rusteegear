//! Les trois parcours de Ball, trois trous chacun, façon mini-golf : le
//! joueur se tient sur le repère du trou (origine, face à −Z), fait tomber
//! toutes les **cibles** (blocs rouge-orangé) en le moins de lancers possible,
//! puis passe au trou suivant, 14 m plus loin.
//!
//! Chaque trou est décrit une seule fois par `layout` ; le même code pose le
//! décor fixe de tout le parcours (`Part::Statics`, au début) ou les blocs et
//! éléments mobiles d'un trou (`Part::Dynamics`, à l'arrivée sur le trou et à
//! chaque « recommencer »).

use glam::{Vec3, Vec4};

use super::world::{Material, Motion, World};

pub struct HoleSpec {
    pub name: &'static str,
    pub par: u32,
}

pub struct CourseSpec {
    pub name: &'static str,
    pub blurb: &'static str,
    pub holes: [HoleSpec; 3],
}

pub const COURSES: [CourseSpec; 3] = [
    CourseSpec {
        name: "Prairie",
        blurb: "Pyramides et tours, pour prendre la main",
        holes: [
            HoleSpec {
                name: "Premier lancer",
                par: 3,
            },
            HoleSpec {
                name: "Les jumelles",
                par: 4,
            },
            HoleSpec {
                name: "La grande pyramide",
                par: 4,
            },
        ],
    },
    CourseSpec {
        name: "Mécanique",
        blurb: "Des cibles qui bougent",
        holes: [
            HoleSpec {
                name: "Le chariot",
                par: 3,
            },
            HoleSpec {
                name: "Le manège",
                par: 4,
            },
            HoleSpec {
                name: "Le mur mobile",
                par: 5,
            },
        ],
    },
    CourseSpec {
        name: "Défi",
        blurb: "Loin, haut, protégé",
        holes: [
            HoleSpec {
                name: "Sommets",
                par: 4,
            },
            HoleSpec {
                name: "Le moulin",
                par: 5,
            },
            HoleSpec {
                name: "Le grand final",
                par: 7,
            },
        ],
    },
];

pub const HOLE_SPACING: f32 = 14.0;

/// Où se tient le joueur au trou `hole`.
pub fn hole_origin(hole: usize) -> Vec3 {
    Vec3::new(0.0, 0.0, -HOLE_SPACING * hole as f32)
}

/// Console à gauche du joueur : bouton rouge (recommencer le trou) et bleu
/// (menu), à toucher de la main.
pub const RESET_BUTTON: Vec3 = Vec3::new(-0.75, 0.97, -0.13);
pub const MENU_BUTTON: Vec3 = Vec3::new(-0.75, 0.97, 0.13);
pub const BUTTON_RADIUS: f32 = 0.07;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Statics,
    Dynamics,
}

const GRASS: Vec4 = Vec4::new(0.42, 0.62, 0.36, 2.0);
const TABLE: Vec4 = Vec4::new(0.36, 0.3, 0.26, 1.0);
const STEEL: Vec4 = Vec4::new(0.45, 0.55, 0.68, 1.0);
const PAD: Vec4 = Vec4::new(0.95, 0.85, 0.55, 0.5);

/// Le sol et le décor fixe de tout un parcours.
pub fn build_statics(course: usize, world: &mut World) {
    world.add_static(
        Vec3::new(0.0, -0.05, -20.0),
        Vec3::new(40.0, 0.05, 60.0),
        GRASS,
    );
    for hole in 0..3 {
        let o = hole_origin(hole);
        // Repère au sol et console des boutons.
        world.add_decor(o + Vec3::Y * 0.003, Vec3::new(0.35, 0.003, 0.35), PAD);
        world.add_static(
            o + Vec3::new(-0.75, 0.45, 0.0),
            Vec3::new(0.08, 0.45, 0.2),
            Vec4::new(0.3, 0.32, 0.36, 1.0),
        );
        world.add_decor(
            o + RESET_BUTTON,
            Vec3::new(0.055, 0.025, 0.055),
            Vec4::new(0.95, 0.15, 0.1, 0.5),
        );
        world.add_decor(
            o + MENU_BUTTON,
            Vec3::new(0.055, 0.025, 0.055),
            Vec4::new(0.15, 0.45, 0.95, 0.5),
        );
        layout(course, hole, Part::Statics, world);
    }
}

/// Pose les blocs et éléments mobiles d'un trou ; renvoie l'indice du premier
/// bloc posé (les suivants sont ceux du trou).
pub fn populate(course: usize, hole: usize, world: &mut World) -> usize {
    let first = world.blocks.len();
    layout(course, hole, Part::Dynamics, world);
    first
}

/// Constructeur d'un trou : chaque méthode ne fait quelque chose que dans la
/// bonne `Part`.
struct B<'a> {
    w: &'a mut World,
    o: Vec3,
    part: Part,
}

impl B<'_> {
    fn table(&mut self, x: f32, z: f32, half: Vec3) {
        if self.part == Part::Statics {
            let c = self.o + Vec3::new(x, half.y, z);
            self.w.add_static(c, half, TABLE);
        }
    }

    fn block(&mut self, p: Vec3, half: f32, m: Material) {
        if self.part == Part::Dynamics {
            self.w.add_block(self.o + p, Vec3::splat(half), m);
        }
    }

    /// Pyramide de `n` blocs à la base, posée à la hauteur `top`.
    fn pyramid(&mut self, x: f32, z: f32, top: f32, n: usize, half: f32, m: impl Fn(usize) -> Material) {
        let size = half * 2.0 + 0.004;
        for row in 0..n {
            let k = n - row;
            for i in 0..k {
                let px = x + (i as f32 - (k - 1) as f32 * 0.5) * size;
                let py = top + half + 0.002 + row as f32 * (half * 2.0 + 0.001);
                self.block(Vec3::new(px, py, z), half, m(row));
            }
        }
    }

    fn tower(&mut self, x: f32, z: f32, top: f32, n: usize, half: f32, m: Material) {
        for k in 0..n {
            let py = top + half + 0.002 + k as f32 * (half * 2.0 + 0.001);
            self.block(Vec3::new(x, py, z), half, m);
        }
    }

    /// Élément mobile, posé avec le trou (il s'anime à partir de là).
    fn mover(&mut self, half: Vec3, motion: Motion) {
        if self.part == Part::Dynamics {
            let motion = match motion {
                Motion::Slide { a, b, period } => Motion::Slide {
                    a: a + self.o,
                    b: b + self.o,
                    period,
                },
                Motion::Spin { center, speed } => Motion::Spin {
                    center: center + self.o,
                    speed,
                },
            };
            self.w.add_mover(half, motion, STEEL);
        }
    }
}

use Material::{Stone, Target, Wood};

fn layout(course: usize, hole: usize, part: Part, world: &mut World) {
    let mut b = B {
        w: world,
        o: hole_origin(hole),
        part,
    };
    match (course, hole) {
        // --- Prairie ---
        (0, 0) => {
            b.table(0.0, -1.8, Vec3::new(0.4, 0.4, 0.35));
            b.pyramid(0.0, -1.8, 0.8, 3, 0.06, |_| Target);
        }
        (0, 1) => {
            b.table(-0.7, -2.3, Vec3::new(0.25, 0.45, 0.25));
            b.table(0.7, -2.3, Vec3::new(0.25, 0.45, 0.25));
            b.tower(-0.7, -2.3, 0.9, 4, 0.06, Target);
            b.tower(0.7, -2.3, 0.9, 4, 0.06, Target);
        }
        (0, 2) => {
            b.table(0.0, -3.0, Vec3::new(0.5, 0.4, 0.35));
            b.pyramid(0.0, -3.0, 0.8, 5, 0.06, |row| if row == 0 { Wood } else { Target });
        }
        // --- Mécanique ---
        (1, 0) => {
            // Rail sous le chariot, qui glisse de gauche à droite.
            b.table(0.0, -2.6, Vec3::new(1.4, 0.33, 0.12));
            let (a, c, period) = (
                Vec3::new(-1.0, 0.72, -2.6),
                Vec3::new(1.0, 0.72, -2.6),
                5.0,
            );
            b.mover(Vec3::new(0.3, 0.04, 0.3), Motion::Slide { a, b: c, period });
            // Le plateau part de `a`, à l'arrêt (cf. `world::Mover::t0`).
            let p = a;
            b.tower(p.x, p.z, p.y + 0.04, 3, 0.06, Target);
        }
        (1, 1) => {
            // Manège : plateau tournant sur un pied, pyramide et gardes de pierre.
            b.table(0.0, -2.8, Vec3::new(0.2, 0.39, 0.2));
            b.mover(
                Vec3::new(0.55, 0.04, 0.55),
                Motion::Spin {
                    center: Vec3::new(0.0, 0.82, -2.8),
                    speed: 0.6,
                },
            );
            b.pyramid(0.0, -2.8, 0.86, 3, 0.06, |_| Target);
            b.block(Vec3::new(0.0, 0.86 + 0.09, -2.4), 0.09, Stone);
            b.block(Vec3::new(0.0, 0.86 + 0.09, -3.2), 0.09, Stone);
        }
        (1, 2) => {
            b.table(0.0, -3.4, Vec3::new(0.55, 0.45, 0.3));
            b.pyramid(0.0, -3.4, 0.9, 3, 0.06, |_| Target);
            b.block(Vec3::new(-0.42, 0.97, -3.4), 0.07, Target);
            b.block(Vec3::new(0.42, 0.97, -3.4), 0.07, Target);
            // Mur d'acier qui passe devant, laissant des trouées.
            b.mover(
                Vec3::new(0.45, 0.4, 0.04),
                Motion::Slide {
                    a: Vec3::new(-0.8, 1.15, -2.4),
                    b: Vec3::new(0.8, 1.15, -2.4),
                    period: 3.5,
                },
            );
        }
        // --- Défi ---
        (2, 0) => {
            for (x, z, top) in [(-1.2, -4.5, 1.2), (0.0, -5.2, 1.5), (1.2, -4.5, 1.2)] {
                b.table(x, z, Vec3::new(0.12, top * 0.5, 0.12));
                b.block(Vec3::new(x, top + 0.072, z), 0.07, Target);
            }
        }
        (2, 1) => {
            b.table(0.0, -3.6, Vec3::new(0.14, 0.8, 0.14));
            b.tower(0.0, -3.6, 1.6, 3, 0.06, Target);
            // Bras du moulin : balaie l'espace devant la tour.
            b.table(0.0, -2.6, Vec3::new(0.06, 0.6, 0.06));
            b.mover(
                Vec3::new(0.9, 0.05, 0.05),
                Motion::Spin {
                    center: Vec3::new(0.0, 1.3, -2.6),
                    speed: 1.6,
                },
            );
        }
        (2, 2) => {
            // À gauche : cibles derrière un muret de pierre.
            b.table(-1.0, -2.2, Vec3::new(0.35, 0.4, 0.3));
            b.block(Vec3::new(-1.1, 0.89, -2.0), 0.09, Stone);
            b.block(Vec3::new(-0.9, 0.89, -2.0), 0.09, Stone);
            b.tower(-1.0, -2.35, 0.8, 2, 0.06, Target);
            // Au centre : chariot, moins de course mais une tour plus haute à viser.
            b.table(0.0, -3.6, Vec3::new(1.2, 0.33, 0.12));
            let (a, c, period) = (
                Vec3::new(-0.9, 0.72, -3.6),
                Vec3::new(0.9, 0.72, -3.6),
                // Pas plus vite : au-delà de ~3 m/s² d'accélération, la tour de
                // trois cubes bascule toute seule (test ci-dessous).
                4.2,
            );
            b.mover(Vec3::new(0.25, 0.04, 0.25), Motion::Slide { a, b: c, period });
            // Le plateau part de `a`, à l'arrêt (cf. `world::Mover::t0`).
            let p = a;
            b.tower(p.x, p.z, p.y + 0.04, 3, 0.06, Target);
            // À droite, au loin : une cible sur un pilier.
            b.table(1.3, -5.0, Vec3::new(0.12, 0.65, 0.12));
            b.tower(1.3, -5.0, 1.3, 2, 0.065, Target);
        }
        _ => {}
    }
}

/// Étoiles d'un parcours : 3 au par ou mieux, 2 jusqu'à par + 3, sinon 1.
pub fn stars(strokes: u32, par: u32) -> u32 {
    if strokes <= par {
        3
    } else if strokes <= par + 3 {
        2
    } else {
        1
    }
}

pub fn course_par(course: usize) -> u32 {
    COURSES[course].holes.iter().map(|h| h.par).sum()
}
