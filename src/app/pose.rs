//! Entrée « pose corporelle » (démo Rééducation, portage de Mouvéo) : les 33
//! repères du modèle *Pose Landmarker* de MediaPipe, poussés par la page web
//! (`packaging/web/reeduc.html` → export wasm `set_pose_landmarks`, cf. `lib.rs`)
//! ou par un test (`AppState::set_pose`), et exposés aux scripts Lua sous la
//! table globale `pose` (cf. `scripting::run_script` / `scripting_web::
//! run_script_web`, via `script_ctx::set_pose`).
//!
//! Le moteur ne fait **aucune** détection lui-même : pas de crate caméra ni
//! d'inférence en Rust — la page web fait tourner MediaPipe (WebGPU/wasm) et ne
//! transmet que des nombres. Sans page hôte (éditeur desktop, APK), `pose.ok`
//! reste faux et la démo Rééducation bascule sur son mode démo au joystick.
//!
//! Convention des coordonnées (celle de MediaPipe, non modifiée ici) : `x`, `y`
//! normalisés dans `[0, 1]` sur l'image caméra, `y` vers le **bas**, `x`
//! vers la droite de l'image **non** miroir (la main droite du patient est
//! donc à `x < 0.5`) ; `z` profondeur relative aux hanches ; `visibility`
//! dans `[0, 1]`. C'est au script de projeter dans le monde (cf.
//! `scene::demos::reeducation`).

/// Nombre de repères du modèle *Pose Landmarker* (BlazePose 33 points).
pub const LANDMARK_COUNT: usize = 33;

/// Nombre de flottants par repère dans la représentation « à plat » (`x, y,
/// z, visibility`) — cf. `PoseFrame::from_flat`.
pub const FLOATS_PER_LANDMARK: usize = 4;

/// Au-delà de ce nombre de pas de simulation sans nouvelle image, la pose
/// est considérée perdue (`pose.ok = false`) : 45 pas à 60 Hz = 0,75 s —
/// bien plus que l'intervalle d'inférence de la page web (~66 ms), assez court
/// pour qu'un patient sorti du cadre mette la séance en pause vite.
pub const STALE_AFTER_TICKS: u32 = 45;

/// Repères nommés exposés à Lua (`pose.<nom>`), avec leur index MediaPipe.
/// Les 13 qu'utilise la rééducation ; les autres (visage, mains, pieds) ne
/// sont pas exposés pour garder la table légère (elle est reconstruite à
/// chaque script et à chaque pas).
pub const NAMED: [(&str, usize); 13] = [
    ("nose", 0),
    ("shoulder_l", 11),
    ("shoulder_r", 12),
    ("elbow_l", 13),
    ("elbow_r", 14),
    ("wrist_l", 15),
    ("wrist_r", 16),
    ("hip_l", 23),
    ("hip_r", 24),
    ("knee_l", 25),
    ("knee_r", 26),
    ("ankle_l", 27),
    ("ankle_r", 28),
];

/// Un repère : position normalisée image + visibilité (cf. la doc du module).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Landmark {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub visibility: f32,
}

/// Dernière pose reçue + son « âge » en pas de simulation.
///
/// **Interpolation** : la caméra livre ~15-30 images/s, la simulation tourne à
/// 60 Hz. Plutôt que de sauter d'une mesure à la suivante, `sampled()` glisse
/// linéairement de l'avant-dernière (`prev`) vers la dernière (`landmarks`) sur
/// la durée observée entre les deux arrivées (`interval`) — mouvement continu à
/// 60 Hz au prix d'une latence d'une image caméra.
#[derive(Clone, Debug, PartialEq)]
pub struct PoseFrame {
    pub landmarks: [Landmark; LANDMARK_COUNT],
    /// Échantillon précédent (point de départ de l'interpolation).
    pub prev: [Landmark; LANDMARK_COUNT],
    /// Au moins une image complète a été reçue depuis le lancement.
    pub present: bool,
    /// Pas de simulation écoulés depuis la dernière image (cf. `STALE_AFTER_TICKS`).
    pub age: u32,
    /// Pas écoulés entre les deux dernières images (durée de l'interpolation).
    pub interval: u32,
}

impl Default for PoseFrame {
    fn default() -> Self {
        Self {
            landmarks: [Landmark::default(); LANDMARK_COUNT],
            prev: [Landmark::default(); LANDMARK_COUNT],
            present: false,
            age: u32::MAX,
            interval: 1,
        }
    }
}

/// Pas maximal d'interpolation : au-delà (caméra très lente), on saute à la
/// dernière mesure plutôt que de traîner un demi-seconde derrière.
const MAX_INTERVAL_TICKS: u32 = 20;

/// Interpole deux repères.
fn lerp_landmark(a: Landmark, b: Landmark, t: f32) -> Landmark {
    Landmark {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        z: a.z + (b.z - a.z) * t,
        visibility: a.visibility + (b.visibility - a.visibility) * t,
    }
}

impl PoseFrame {
    /// Décode `33 × 4` flottants (`x, y, z, visibility` par repère, ordre
    /// MediaPipe). Une tranche plus courte (personne non détectée : la page
    /// envoie un tableau vide) signifie « pas de corps » : `present` passe à
    /// faux sans toucher aux derniers repères connus (un script peut vouloir
    /// figer le dernier squelette plutôt que le faire disparaître d'un coup).
    pub fn apply_flat(&mut self, flat: &[f32]) {
        if flat.len() < LANDMARK_COUNT * FLOATS_PER_LANDMARK {
            self.present = false;
            self.age = 0;
            return;
        }
        // Point de départ de l'interpolation : là où l'affichage en était
        // (mesure précédente si elle était atteinte, sinon la valeur interpolée).
        self.prev = if self.present {
            self.sampled()
        } else {
            let mut fresh = [Landmark::default(); LANDMARK_COUNT];
            let (chunks, _rest) = flat.as_chunks::<FLOATS_PER_LANDMARK>();
            for (lm, chunk) in fresh.iter_mut().zip(chunks) {
                *lm = Landmark {
                    x: chunk[0],
                    y: chunk[1],
                    z: chunk[2],
                    visibility: chunk[3],
                };
            }
            fresh
        };
        self.interval = if self.present {
            self.age.clamp(1, MAX_INTERVAL_TICKS)
        } else {
            1
        };
        let (chunks, _rest) = flat.as_chunks::<FLOATS_PER_LANDMARK>();
        for (lm, chunk) in self.landmarks.iter_mut().zip(chunks) {
            *lm = Landmark {
                x: chunk[0],
                y: chunk[1],
                z: chunk[2],
                visibility: chunk[3],
            };
        }
        self.present = true;
        self.age = 0;
    }

    /// Un pas de simulation de plus sans image (saturant).
    pub fn tick(&mut self) {
        self.age = self.age.saturating_add(1);
    }

    /// Pose utilisable par un script : reçue **et** fraîche.
    pub fn is_ok(&self) -> bool {
        self.present && self.age < STALE_AFTER_TICKS
    }

    /// Repères **interpolés** pour le pas courant (cf. la doc du type) : la
    /// dernière mesure une fois `interval` pas écoulés, entre les deux avant.
    pub fn sampled(&self) -> [Landmark; LANDMARK_COUNT] {
        let t = (self.age as f32 / self.interval.max(1) as f32).clamp(0.0, 1.0);
        if t >= 1.0 {
            return self.landmarks;
        }
        let mut out = self.landmarks;
        for (o, (a, b)) in out.iter_mut().zip(self.prev.iter().zip(&self.landmarks)) {
            *o = lerp_landmark(*a, *b, t);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_with(index: usize, lm: Landmark) -> Vec<f32> {
        let mut v = vec![0.0; LANDMARK_COUNT * FLOATS_PER_LANDMARK];
        v[index * 4] = lm.x;
        v[index * 4 + 1] = lm.y;
        v[index * 4 + 2] = lm.z;
        v[index * 4 + 3] = lm.visibility;
        v
    }

    #[test]
    fn default_pose_is_not_ok() {
        let p = PoseFrame::default();
        assert!(!p.is_ok());
        assert!(!p.present);
    }

    #[test]
    fn apply_flat_decodes_landmarks_in_mediapipe_order() {
        let mut p = PoseFrame::default();
        let wrist = Landmark {
            x: 0.25,
            y: 0.6,
            z: -0.1,
            visibility: 0.9,
        };
        p.apply_flat(&flat_with(16, wrist));
        assert!(p.is_ok());
        assert_eq!(p.landmarks[16], wrist);
        assert_eq!(p.landmarks[15], Landmark::default());
    }

    #[test]
    fn pose_goes_stale_after_the_timeout_and_recovers_on_next_frame() {
        let mut p = PoseFrame::default();
        p.apply_flat(&vec![0.5; LANDMARK_COUNT * FLOATS_PER_LANDMARK]);
        for _ in 0..STALE_AFTER_TICKS - 1 {
            p.tick();
        }
        assert!(p.is_ok(), "un pas avant le délai : encore fraîche");
        p.tick();
        assert!(!p.is_ok(), "délai atteint : perdue");
        p.apply_flat(&vec![0.5; LANDMARK_COUNT * FLOATS_PER_LANDMARK]);
        assert!(p.is_ok());
    }

    #[test]
    fn an_empty_frame_means_no_body_but_keeps_the_last_landmarks() {
        let mut p = PoseFrame::default();
        p.apply_flat(&flat_with(
            0,
            Landmark {
                x: 0.5,
                y: 0.2,
                z: 0.0,
                visibility: 1.0,
            },
        ));
        p.apply_flat(&[]);
        assert!(!p.is_ok());
        assert_eq!(p.landmarks[0].x, 0.5, "derniers repères conservés");
    }

    /// Entre deux images caméra, les repères glissent linéairement de l'ancienne
    /// mesure vers la nouvelle sur la durée observée entre les deux arrivées.
    #[test]
    fn samples_interpolate_between_the_last_two_camera_frames() {
        let mut p = PoseFrame::default();
        let at = |x: f32| {
            flat_with(
                16,
                Landmark {
                    x,
                    y: 0.5,
                    z: 0.0,
                    visibility: 1.0,
                },
            )
        };
        p.apply_flat(&at(0.0));
        assert_eq!(
            p.sampled()[16].x,
            0.0,
            "première image : aucune interpolation"
        );
        for _ in 0..4 {
            p.tick();
        }
        p.apply_flat(&at(1.0)); // 4 pas après la première : intervalle = 4
        assert_eq!(p.interval, 4);
        assert!(
            (p.sampled()[16].x - 0.0).abs() < 1e-6,
            "à l'arrivée, l'affichage part de l'ancienne mesure"
        );
        p.tick();
        assert!((p.sampled()[16].x - 0.25).abs() < 1e-6);
        p.tick();
        assert!((p.sampled()[16].x - 0.5).abs() < 1e-6);
        p.tick();
        p.tick();
        assert!(
            (p.sampled()[16].x - 1.0).abs() < 1e-6,
            "intervalle écoulé : mesure atteinte"
        );
        p.tick();
        assert!((p.sampled()[16].x - 1.0).abs() < 1e-6, "et on y reste");
        // Nouvelle image avant la fin de l'interpolation : on repart d'où on était.
        p.apply_flat(&at(2.0));
        p.tick();
        p.tick();
        p.apply_flat(&at(4.0));
        let start = p.sampled()[16].x;
        assert!(
            (start - 1.4).abs() < 1e-6,
            "départ = valeur interpolée au moment de l'arrivée ({start})"
        );
    }

    #[test]
    fn named_landmarks_are_within_the_model_range_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (name, idx) in NAMED {
            assert!(idx < LANDMARK_COUNT, "{name}: index {idx} hors modèle");
            assert!(seen.insert(idx), "{name}: index {idx} dupliqué");
        }
    }
}

// ---------------------------------------------------------------------------
// Mains (doigts) — MediaPipe *Hand Landmarker*, 21 repères par main
// ---------------------------------------------------------------------------

/// Repères d'une main (MediaPipe Hand Landmarker) : 0 poignet ; 1-4 pouce
/// (CMC, MCP, IP, bout) ; 5-8 index (MCP, PIP, DIP, bout) ; 9-12 majeur ;
/// 13-16 annulaire ; 17-20 auriculaire. Exposés à Lua en tableaux `x`/`y`/`z`
/// 1-based (`hand.right.x[9]` = bout de l'index).
pub const HAND_LANDMARK_COUNT: usize = 21;

/// Flottants par repère de main (`x, y, z` — pas de visibilité : le modèle
/// n'en fournit pas) — cf. `HandFrame::apply_flat`.
pub const FLOATS_PER_HAND_LANDMARK: usize = 3;

/// Flottants par main dans la représentation à plat : un marqueur de côté
/// (`0` gauche, `1` droite) puis les 21 repères.
pub const FLOATS_PER_HAND: usize = 1 + HAND_LANDMARK_COUNT * FLOATS_PER_HAND_LANDMARK;

/// Une main détectée : 21 repères `[x, y, z]` (mêmes conventions image que
/// `Landmark`, `z` relatif au poignet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hand {
    pub landmarks: [[f32; 3]; HAND_LANDMARK_COUNT],
}

/// Dernières mains reçues (au plus une par côté) + âge en pas de simulation.
/// Le côté est décidé par la page (repère de poignet du corps le plus proche,
/// repli sur la latéralité du modèle) — cf. `packaging/web/reeduc.html`.
#[derive(Clone, Debug, PartialEq)]
pub struct HandFrame {
    pub left: Option<Hand>,
    pub right: Option<Hand>,
    /// Mains de l'image précédente (départ de l'interpolation, cf. `PoseFrame`).
    pub prev_left: Option<Hand>,
    pub prev_right: Option<Hand>,
    /// Pas de simulation écoulés depuis la dernière image (cf. `STALE_AFTER_TICKS`).
    pub age: u32,
    /// Pas écoulés entre les deux dernières images.
    pub interval: u32,
}

impl Default for HandFrame {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            prev_left: None,
            prev_right: None,
            age: u32::MAX,
            interval: 1,
        }
    }
}

fn lerp_hand(a: &Hand, b: &Hand, t: f32) -> Hand {
    let mut out = *b;
    for (o, (pa, pb)) in out
        .landmarks
        .iter_mut()
        .zip(a.landmarks.iter().zip(&b.landmarks))
    {
        for k in 0..3 {
            o[k] = pa[k] + (pb[k] - pa[k]) * t;
        }
    }
    out
}

impl HandFrame {
    /// Décode `n × 64` flottants (`[côté, 21 × (x, y, z)]` par main, `n` ∈ 0..2).
    /// Une tranche vide signifie « aucune main » : les deux côtés passent à
    /// `None` (contrairement à `PoseFrame`, on ne fige pas la dernière main :
    /// une main qui sort du cadre doit disparaître, pas rester plantée).
    pub fn apply_flat(&mut self, flat: &[f32]) {
        // Départ de l'interpolation : la main telle qu'affichée à l'instant de
        // l'arrivée (ou rien si elle vient d'apparaître).
        let fresh = self.age >= STALE_AFTER_TICKS;
        self.prev_left = if fresh {
            None
        } else {
            self.sampled_side(false)
        };
        self.prev_right = if fresh { None } else { self.sampled_side(true) };
        self.interval = if fresh {
            1
        } else {
            self.age.clamp(1, MAX_INTERVAL_TICKS)
        };
        self.left = None;
        self.right = None;
        self.age = 0;
        let (hands, _rest) = flat.as_chunks::<FLOATS_PER_HAND>();
        for chunk in hands {
            let mut hand = Hand {
                landmarks: [[0.0; 3]; HAND_LANDMARK_COUNT],
            };
            let (pts, _) = chunk[1..].as_chunks::<FLOATS_PER_HAND_LANDMARK>();
            for (dst, src) in hand.landmarks.iter_mut().zip(pts) {
                *dst = *src;
            }
            if chunk[0] >= 0.5 {
                self.right = Some(hand);
            } else {
                self.left = Some(hand);
            }
        }
    }

    pub fn tick(&mut self) {
        self.age = self.age.saturating_add(1);
    }

    /// Au moins une main fraîche.
    pub fn is_ok(&self) -> bool {
        self.age < STALE_AFTER_TICKS && (self.left.is_some() || self.right.is_some())
    }

    /// Main d'un côté, seulement si l'image est fraîche.
    pub fn side(&self, right: bool) -> Option<&Hand> {
        if self.age >= STALE_AFTER_TICKS {
            return None;
        }
        if right {
            self.right.as_ref()
        } else {
            self.left.as_ref()
        }
    }

    /// Main d'un côté **interpolée** pour le pas courant (cf. `PoseFrame::sampled`) ;
    /// une main qui vient d'apparaître n'a pas de point de départ : mesure brute.
    pub fn sampled_side(&self, right: bool) -> Option<Hand> {
        let cur = self.side(right)?;
        let prev = if right {
            &self.prev_right
        } else {
            &self.prev_left
        };
        let t = (self.age as f32 / self.interval.max(1) as f32).clamp(0.0, 1.0);
        match prev {
            Some(p) if t < 1.0 => Some(lerp_hand(p, cur, t)),
            _ => Some(*cur),
        }
    }
}

#[cfg(test)]
mod hand_tests {
    use super::*;

    fn hand_flat(side: f32, tip_x: f32) -> Vec<f32> {
        let mut v = vec![0.0; FLOATS_PER_HAND];
        v[0] = side;
        v[1 + 8 * 3] = tip_x; // bout de l'index (repère 8)
        v
    }

    #[test]
    fn hands_decode_by_side_and_expose_their_landmarks() {
        let mut h = HandFrame::default();
        assert!(!h.is_ok());
        let mut flat = hand_flat(1.0, 0.3);
        flat.extend(hand_flat(0.0, 0.7));
        h.apply_flat(&flat);
        assert!(h.is_ok());
        assert_eq!(h.side(true).unwrap().landmarks[8][0], 0.3);
        assert_eq!(h.side(false).unwrap().landmarks[8][0], 0.7);
        h.apply_flat(&hand_flat(1.0, 0.4));
        assert!(h.side(false).is_none(), "main gauche disparue = None");
        assert_eq!(h.side(true).unwrap().landmarks[8][0], 0.4);
        h.apply_flat(&[]);
        assert!(!h.is_ok());
        assert!(h.left.is_none() && h.right.is_none());
    }

    #[test]
    fn hands_interpolate_between_two_frames_and_start_raw_when_they_appear() {
        let mut h = HandFrame::default();
        h.apply_flat(&hand_flat(1.0, 0.0));
        assert_eq!(
            h.sampled_side(true).unwrap().landmarks[8][0],
            0.0,
            "apparition : brute"
        );
        for _ in 0..2 {
            h.tick();
        }
        h.apply_flat(&hand_flat(1.0, 1.0));
        assert_eq!(h.interval, 2);
        h.tick();
        assert!((h.sampled_side(true).unwrap().landmarks[8][0] - 0.5).abs() < 1e-6);
        h.tick();
        assert!((h.sampled_side(true).unwrap().landmarks[8][0] - 1.0).abs() < 1e-6);
        assert!(h.sampled_side(false).is_none());
    }

    #[test]
    fn hands_go_stale_like_the_pose() {
        let mut h = HandFrame::default();
        h.apply_flat(&hand_flat(1.0, 0.1));
        for _ in 0..STALE_AFTER_TICKS {
            h.tick();
        }
        assert!(!h.is_ok());
        assert!(h.side(true).is_none());
    }
}
