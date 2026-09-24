//! Simulateur de casque Meta Quest (desktop) — remplace le casque réel pour
//! développer les phases 1 à 6 de la roadmap VR sur le poste de dev (macOS n'a
//! aucun runtime OpenXR). Logique pure ici (profil matériel, tête et manettes
//! simulées) ; la fenêtre stéréo est `src/bin/quest_sim.rs`.
//!
//! Ce que le simulateur valide : la stéréo (même `EyeView`/projection que
//! l'APK), la scène, les entrées et l'UI VR, le confort de locomotion, la
//! résolution réelle par œil. Ce qu'il ne valide **pas** : l'interop
//! OpenXR ↔ Vulkan ↔ wgpu (`xr::hello`) et les performances du GPU mobile —
//! seul le casque le peut (phase 0, go / no-go).

use glam::{Quat, Vec3};

use super::math::{EyeView, Fov};

/// Caractéristiques d'un modèle de casque, telles que le runtime OpenXR les
/// annonce (`xrEnumerateViewConfigurationViews`, `xrLocateViews`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuestProfile {
    pub name: &'static str,
    /// Résolution recommandée **par œil** (pixels).
    pub eye_width: u32,
    pub eye_height: u32,
    /// Fréquence d'affichage visée (Hz) : fixe le budget par image.
    pub refresh_hz: f32,
    /// Champ de vision de l'œil **gauche** (radians) ; l'œil droit en est le
    /// miroir horizontal (côté nasal étroit, côté temporal large).
    pub left_eye_fov: Fov,
    /// Écart interpupillaire par défaut (m).
    pub ipd: f32,
}

impl QuestProfile {
    /// Meta Quest 3 : 2064×2208 par œil, 90 Hz visé (72/90/120 possibles).
    /// FOV approximatifs relevés sur `xrLocateViews` (≈ 110° horizontal
    /// binoculaire, ≈ 96° vertical) — à remplacer par les valeurs exactes
    /// journalisées par l'APK (`VR : …`) au premier test casque.
    pub const QUEST3: Self = Self {
        name: "Meta Quest 3",
        eye_width: 2064,
        eye_height: 2208,
        refresh_hz: 90.0,
        left_eye_fov: Fov {
            left: -0.942,
            right: 0.698,
            up: 0.768,
            down: -0.960,
        },
        ipd: 0.063,
    };

    /// Meta Quest 2 : plancher de performance (Adreno 650).
    pub const QUEST2: Self = Self {
        name: "Meta Quest 2",
        eye_width: 1832,
        eye_height: 1920,
        refresh_hz: 72.0,
        left_eye_fov: Fov {
            left: -0.908,
            right: 0.768,
            up: 0.855,
            down: -0.960,
        },
        ipd: 0.063,
    };

    /// FOV de l'œil `eye` (0 = gauche, 1 = droit).
    pub fn fov(&self, eye: usize) -> Fov {
        let f = self.left_eye_fov;
        if eye == 0 {
            f
        } else {
            Fov {
                left: -f.right,
                right: -f.left,
                up: f.up,
                down: f.down,
            }
        }
    }

    /// Même casque, résolution de rendu par œil multipliée par `scale`
    /// (bornée à 0,25..1,5) — ce que fait une app Quest qui rend sous la
    /// résolution recommandée pour tenir le budget (le compositeur agrandit).
    pub fn scaled(self, scale: f32) -> Self {
        let s = scale.clamp(0.25, 1.5);
        Self {
            eye_width: ((self.eye_width as f32 * s).round() as u32).max(1),
            eye_height: ((self.eye_height as f32 * s).round() as u32).max(1),
            ..self
        }
    }

    /// Budget de temps par image (ms) à la fréquence visée.
    pub fn frame_budget_ms(&self) -> f32 {
        1000.0 / self.refresh_hz
    }
}

/// Tête simulée, dans l'espace `STAGE` (origine au sol, Y en haut, −Z devant
/// au départ) : position des yeux (milieu), lacet et tangage. Pilotée par la
/// souris et le clavier dans `quest_sim`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimHead {
    pub position: Vec3,
    /// Lacet (rad), positif = tourner à gauche (sens trigonométrique autour de +Y).
    pub yaw: f32,
    /// Tangage (rad), positif = regarder vers le haut, borné à ±85°.
    pub pitch: f32,
}

/// Hauteur des yeux d'un adulte debout (m).
pub const STANDING_EYE_HEIGHT: f32 = 1.65;
/// Vitesse de marche réelle dans la pièce (m/s) — la locomotion artificielle
/// (stick) viendra en phase 4, avec ses propres réglages de confort.
pub const WALK_SPEED: f32 = 1.4;
const PITCH_LIMIT: f32 = 85.0 * std::f32::consts::PI / 180.0;

impl Default for SimHead {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, STANDING_EYE_HEIGHT, 0.0),
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

impl SimHead {
    pub fn orientation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(self.pitch)
    }

    /// Tourne la tête (radians).
    pub fn look(&mut self, d_yaw: f32, d_pitch: f32) {
        self.yaw = (self.yaw + d_yaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + d_pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Marche dans le plan horizontal : `forward`/`strafe` dans \[−1, 1\]
    /// (relatifs au lacet, le tangage ne fait pas décoller), `rise` = monter
    /// (+1) ou s'accroupir (−1), hauteur bornée entre 0,3 m et 2,2 m.
    pub fn walk(&mut self, forward: f32, strafe: f32, rise: f32, dt: f32) {
        let yaw = Quat::from_rotation_y(self.yaw);
        let fwd = yaw * Vec3::NEG_Z;
        let right = yaw * Vec3::X;
        let mut d = fwd * forward + right * strafe;
        if d.length_squared() > 1.0 {
            d = d.normalize();
        }
        self.position += d * WALK_SPEED * dt;
        self.position.y = (self.position.y + rise * WALK_SPEED * dt).clamp(0.3, 2.2);
    }

    /// Les deux yeux : décalés de ±IPD/2 le long de l'axe droit de la tête,
    /// même orientation, FOV asymétriques du profil.
    pub fn eye_views(&self, profile: &QuestProfile) -> [EyeView; 2] {
        let orientation = self.orientation();
        let half = orientation * Vec3::X * (profile.ipd * 0.5);
        [
            EyeView {
                orientation,
                position: self.position - half,
                fov: profile.fov(0),
            },
            EyeView {
                orientation,
                position: self.position + half,
                fov: profile.fov(1),
            },
        ]
    }

    /// Pose d'une manette simulée (0 = gauche, 1 = droite) : tenue à hauteur de
    /// taille, devant le corps, suivant le lacet (pas le tangage — on baisse
    /// les yeux sans baisser les mains).
    pub fn controller_pose(&self, hand: usize) -> (Vec3, Quat) {
        let yaw = Quat::from_rotation_y(self.yaw);
        let side = if hand == 0 { -0.2 } else { 0.2 };
        let offset = yaw * Vec3::new(side, -0.45, -0.35);
        (self.position + offset, yaw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_eye_fov_mirrors_the_left_one() {
        let p = QuestProfile::QUEST3;
        let (l, r) = (p.fov(0), p.fov(1));
        assert_eq!(r.left, -l.right);
        assert_eq!(r.right, -l.left);
        assert_eq!((r.up, r.down), (l.up, l.down));
    }

    #[test]
    fn eyes_are_one_ipd_apart_and_centered_on_the_head() {
        let mut head = SimHead::default();
        head.look(0.7, 0.3);
        let [l, r] = head.eye_views(&QuestProfile::QUEST3);
        assert!((l.position.distance(r.position) - 0.063).abs() < 1e-5);
        assert!(((l.position + r.position) * 0.5).distance(head.position) < 1e-5);
        // L'œil gauche est bien à gauche : côté −X local de la tête.
        let local = head.orientation().inverse() * (l.position - head.position);
        assert!(local.x < 0.0);
    }

    #[test]
    fn a_point_straight_ahead_has_crossed_disparity() {
        // Un cube à 1,2 m droit devant : dans l'œil gauche il apparaît à droite
        // du point de fuite de celui-ci, et inversement — la parallaxe qui
        // donne la profondeur (et que le casque doit reproduire à l'identique).
        let head = SimHead::default();
        let target = head.position + Vec3::new(0.0, 0.0, -1.2);
        let [l, r] = head.eye_views(&QuestProfile::QUEST3);
        let ndc = |e: &EyeView| {
            let c = e.view_proj() * target.extend(1.0);
            c.x / c.w
        };
        let center = |e: &EyeView| {
            // Abscisse NDC du « tout droit » (asymétrie du FOV).
            let (lt, rt) = (e.fov.left.tan(), e.fov.right.tan());
            -(rt + lt) / (rt - lt)
        };
        assert!(ndc(&l) > center(&l));
        assert!(ndc(&r) < center(&r));
    }

    #[test]
    fn walking_ignores_pitch_and_respects_heading() {
        let mut head = SimHead::default();
        head.look(std::f32::consts::FRAC_PI_2, 0.8); // tourné vers −X, regard levé
        head.walk(1.0, 0.0, 0.0, 1.0);
        assert!((head.position.y - STANDING_EYE_HEIGHT).abs() < 1e-5);
        assert!(
            (head.position.x + WALK_SPEED).abs() < 1e-4,
            "{:?}",
            head.position
        );
        assert!(head.position.z.abs() < 1e-4);
    }

    #[test]
    fn pitch_never_flips_over() {
        let mut head = SimHead::default();
        head.look(0.0, 10.0);
        assert!(head.pitch <= PITCH_LIMIT);
        head.look(0.0, -20.0);
        assert!(head.pitch >= -PITCH_LIMIT);
    }

    #[test]
    fn scaled_keeps_the_eye_aspect_and_clamps() {
        let p = QuestProfile::QUEST3.scaled(0.5);
        assert_eq!((p.eye_width, p.eye_height), (1032, 1104));
        assert_eq!(QuestProfile::QUEST3.scaled(0.01).eye_width, 516);
    }

    #[test]
    fn frame_budget_matches_refresh_rate() {
        assert!((QuestProfile::QUEST3.frame_budget_ms() - 11.111).abs() < 1e-2);
        assert!((QuestProfile::QUEST2.frame_budget_ms() - 13.889).abs() < 1e-2);
    }
}
