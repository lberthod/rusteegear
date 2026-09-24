//! Suivi des mains en VR → rééducation Mouvéo / PhysioTech (phase 8 de la
//! roadmap). Le casque donne 26 articulations par main (`XR_EXT_hand_tracking`)
//! et la tête ; les scripts de rééducation attendent, eux, les repères d'une
//! **webcam posée face au patient** (MediaPipe : `pose`, 33 repères du corps ;
//! `hand`, 21 repères par main, coordonnées image). Ce module fait le pont :
//!
//! - une **webcam virtuelle** à 2 m devant le joueur, tournée vers lui (champ
//!   horizontal de 70°) projette les articulations comme une vraie webcam
//!   (la main droite du patient apparaît à gauche de l'image, cf.
//!   `scene::demos::reeducation::world_to_pose`) ;
//! - les 26 articulations OpenXR sont ramenées aux 21 repères MediaPipe ;
//! - un **corps minimal** (nez, épaules, coudes, poignets, hanches, genoux,
//!   chevilles) est synthétisé depuis la tête et les poignets : la calibration
//!   de Mouvéo voit un patient debout face à la caméra.
//!
//! Les scripts Mouvéo marchent donc tels quels en VR. Logique pure, testée ;
//! une main synthétique (`synthetic_hand`) sert au simulateur et aux tests.

use glam::{Quat, Vec3};

use crate::app::pose::{FLOATS_PER_HAND, FLOATS_PER_LANDMARK, HAND_LANDMARK_COUNT, LANDMARK_COUNT};

/// Articulations d'une main dans l'ordre OpenXR (`XrHandJointEXT`) :
/// 0 paume, 1 poignet, 2-5 pouce (métacarpe, proximale, distale, bout),
/// 6-10 index (métacarpe, proximale, intermédiaire, distale, bout), 11-15
/// majeur, 16-20 annulaire, 21-25 auriculaire.
pub const OPENXR_JOINTS: usize = 26;
pub type Joints = [Vec3; OPENXR_JOINTS];

pub const WRIST: usize = 1;
pub const THUMB_TIP: usize = 5;
pub const INDEX_PROXIMAL: usize = 7;
pub const INDEX_TIP: usize = 10;
pub const MIDDLE_PROXIMAL: usize = 12;

/// Repère MediaPipe `i` ← articulation OpenXR `MEDIAPIPE_FROM_OPENXR[i]` :
/// poignet ; pouce CMC, MCP, IP, bout ; puis pour chaque doigt MCP, PIP, DIP,
/// bout (le métacarpe OpenXR n'a pas d'équivalent MediaPipe).
pub const MEDIAPIPE_FROM_OPENXR: [usize; HAND_LANDMARK_COUNT] = [
    1, 2, 3, 4, 5, 7, 8, 9, 10, 12, 13, 14, 15, 17, 18, 19, 20, 22, 23, 24, 25,
];

/// Webcam virtuelle posée face au joueur (cf. la doc du module).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualWebcam {
    position: Vec3,
    /// Direction horizontale dans laquelle **le joueur** regarde (la caméra
    /// regarde dans l'autre sens, vers lui).
    forward: Vec3,
    right: Vec3,
    /// 1 / (2·tan(demi-champ)) : largeur d'image par unité de pente.
    k: f32,
}

impl VirtualWebcam {
    /// Webcam à 2 m devant la tête, à 1,2 m du sol de la pièce, tournée vers le
    /// joueur. `head`/`look` dans l'espace de la pièce (`STAGE`, sol en y = 0).
    pub fn facing(head: Vec3, look: Vec3) -> Self {
        let forward = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
        let position = Vec3::new(head.x, 1.2, head.z) + forward * 2.0;
        Self {
            position,
            forward,
            right: forward.cross(Vec3::Y),
            k: 1.0 / (2.0 * 35f32.to_radians().tan()),
        }
    }

    /// Point → coordonnées image (x vers la droite **de l'image**, y vers le
    /// bas, dans ~\[0, 1\]) et profondeur (m).
    pub fn project(&self, p: Vec3) -> (f32, f32, f32) {
        let v = p - self.position;
        let depth = (-v.dot(self.forward)).max(0.05);
        // La caméra regarde le joueur : sa droite est la gauche du joueur.
        let x_cam = -v.dot(self.right);
        let y_cam = v.y;
        (
            0.5 + x_cam / depth * self.k,
            0.5 - y_cam / depth * self.k,
            depth,
        )
    }

    /// Une main vue par la webcam : 21 repères MediaPipe `[x, y, z]`, `z`
    /// relatif au poignet (négatif = plus près de la caméra), à l'échelle de x.
    pub fn hand_landmarks(&self, joints: &Joints) -> [[f32; 3]; HAND_LANDMARK_COUNT] {
        let (_, _, wrist_depth) = self.project(joints[WRIST]);
        let mut out = [[0.0; 3]; HAND_LANDMARK_COUNT];
        for (dst, &j) in out.iter_mut().zip(&MEDIAPIPE_FROM_OPENXR) {
            let (x, y, d) = self.project(joints[j]);
            *dst = [x, y, (d - wrist_depth) * self.k / wrist_depth];
        }
        out
    }
}

/// Mains au format de `HandFrame::apply_flat` (`[côté, 21 × (x, y, z)]` par
/// main suivie ; vide = aucune main).
pub fn hands_flat(cam: &VirtualWebcam, hands: [Option<&Joints>; 2]) -> Vec<f32> {
    let mut flat = Vec::with_capacity(2 * FLOATS_PER_HAND);
    for (side, joints) in hands.iter().enumerate() {
        let Some(joints) = joints else { continue };
        flat.push(side as f32); // 0 gauche, 1 droite
        for lm in cam.hand_landmarks(joints) {
            flat.extend(lm);
        }
    }
    flat
}

/// Corps synthétique au format de `PoseFrame::apply_flat` (33 × `[x, y, z, v]`)
/// depuis la tête et les poignets (pièce) : un patient debout face à la
/// webcam. Repères non synthétisés : visibilité 0.
pub fn body_flat(cam: &VirtualWebcam, head: Vec3, wrists: [Option<Vec3>; 2]) -> Vec<f32> {
    let f = cam.forward;
    let r = cam.right;
    let up = Vec3::Y;
    let shoulder = |side: f32| head - up * 0.25 + r * (0.19 * side);
    let hip = |side: f32| head - up * 0.75 + r * (0.12 * side);
    let (sl, sr) = (shoulder(-1.0), shoulder(1.0));
    let wrist_l = wrists[0].unwrap_or(sl - up * 0.55);
    let wrist_r = wrists[1].unwrap_or(sr - up * 0.55);
    let (hl, hr) = (hip(-1.0), hip(1.0));
    let mut points: [Option<Vec3>; LANDMARK_COUNT] = [None; LANDMARK_COUNT];
    points[0] = Some(head + f * 0.08 - up * 0.03);
    points[11] = Some(sl);
    points[12] = Some(sr);
    points[13] = Some((sl + wrist_l) * 0.5 - up * 0.05);
    points[14] = Some((sr + wrist_r) * 0.5 - up * 0.05);
    points[15] = Some(wrist_l);
    points[16] = Some(wrist_r);
    points[23] = Some(hl);
    points[24] = Some(hr);
    points[25] = Some(hl - up * 0.45);
    points[26] = Some(hr - up * 0.45);
    points[27] = Some(hl - up * 0.85);
    points[28] = Some(hr - up * 0.85);
    let mut flat = Vec::with_capacity(LANDMARK_COUNT * FLOATS_PER_LANDMARK);
    for p in points {
        match p {
            Some(p) => {
                let (x, y, _) = cam.project(p);
                flat.extend([x, y, 0.0, 1.0]);
            }
            None => flat.extend([0.0, 0.0, 0.0, 0.0]),
        }
    }
    flat
}

/// Écart pouce-index (m) : pincement quand il passe sous ~2 cm.
pub fn pinch_distance(joints: &Joints) -> f32 {
    joints[THUMB_TIP].distance(joints[INDEX_TIP])
}

/// Pincement franc (clic de l'interface en mode mains).
pub const PINCH_CLICK: f32 = 0.02;

/// Rayon de pointage d'une main (sans manette) : depuis la base de l'index,
/// dans l'axe poignet → base du majeur.
pub fn aim_ray(joints: &Joints) -> (Vec3, Vec3) {
    let dir = (joints[MIDDLE_PROXIMAL] - joints[WRIST]).normalize_or(Vec3::NEG_Z);
    (joints[INDEX_PROXIMAL], dir)
}

/// Main synthétique (simulateur, tests) : poignet en `wrist`, orientation
/// `rot` (doigts vers −Z local, dos de la main vers +Y local), `right` =
/// main droite (pouce côté −X local), `pinch` et `fist` dans \[0, 1\].
pub fn synthetic_hand(wrist: Vec3, rot: Quat, right: bool, pinch: f32, fist: f32) -> Joints {
    let thumb_side = if right { -1.0 } else { 1.0 };
    let at = |x: f32, y: f32, ahead: f32| wrist + rot * Vec3::new(x, y, -ahead);
    let mut j = [Vec3::ZERO; OPENXR_JOINTS];
    j[0] = at(0.0, 0.0, 0.045);
    j[1] = wrist;
    // Pouce.
    j[2] = at(0.02 * thumb_side, -0.01, 0.02);
    j[3] = at(0.04 * thumb_side, -0.015, 0.045);
    j[4] = at(0.05 * thumb_side, -0.02, 0.07);
    j[5] = at(0.055 * thumb_side, -0.02, 0.09);
    // Doigts : écartés côté opposé au pouce, repliés vers la paume par `fist`.
    let palm = j[0] - rot * Vec3::Y * 0.02;
    for (finger, spread) in [(0usize, 0.02f32), (1, 0.0), (2, -0.02), (3, -0.038)] {
        let base = 6 + finger * 5;
        let x = spread * thumb_side;
        j[base] = at(x, 0.0, 0.03);
        j[base + 1] = at(x, 0.0, 0.09);
        for (k, ahead) in [(2usize, 0.125f32), (3, 0.15), (4, 0.17)] {
            let open = at(x, 0.0, ahead);
            j[base + k] = open.lerp(palm, fist.clamp(0.0, 1.0) * (0.35 + 0.2 * k as f32));
        }
    }
    // Pincement : bouts du pouce et de l'index se rejoignent.
    let meet = (j[THUMB_TIP] + j[INDEX_TIP]) * 0.5;
    let p = pinch.clamp(0.0, 1.0);
    j[THUMB_TIP] = j[THUMB_TIP].lerp(meet, p);
    j[INDEX_TIP] = j[INDEX_TIP].lerp(meet, p);
    j
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::pose::{HandFrame, PoseFrame};

    fn head() -> Vec3 {
        Vec3::new(0.0, 1.65, 0.0)
    }

    /// Main droite tendue devant soi, à hauteur d'épaule, doigts vers l'avant.
    fn right_hand(pinch: f32, fist: f32) -> Joints {
        synthetic_hand(
            Vec3::new(0.25, 1.35, -0.35),
            Quat::IDENTITY,
            true,
            pinch,
            fist,
        )
    }

    #[test]
    fn the_player_s_right_side_appears_on_the_left_of_the_webcam_image() {
        let cam = VirtualWebcam::facing(head(), Vec3::NEG_Z);
        let (xr, _, _) = cam.project(head() + Vec3::X * 0.5);
        let (xl, _, _) = cam.project(head() - Vec3::X * 0.5);
        let (_, y_head, _) = cam.project(head());
        let (_, y_low, _) = cam.project(head() - Vec3::Y);
        assert!(xr < 0.5 && xl > 0.5, "webcam face au joueur : image miroir");
        assert!(y_low > y_head, "y vers le bas de l'image");
        // Même convention que la démo de rééducation (`world_to_pose`).
        let (wx, _) = crate::scene::demos::reeducation::world_to_pose(0.5, 1.0);
        assert!(wx < 0.5);
    }

    #[test]
    fn openxr_joints_map_onto_the_21_mediapipe_landmarks() {
        let j = right_hand(0.0, 0.0);
        let cam = VirtualWebcam::facing(head(), Vec3::NEG_Z);
        let lm = cam.hand_landmarks(&j);
        assert_eq!(lm[0][2], 0.0, "z relatif au poignet");
        assert_eq!(
            cam.project(j[INDEX_TIP]).0,
            lm[8][0],
            "repère 8 = bout de l'index"
        );
        assert_eq!(
            cam.project(j[THUMB_TIP]).0,
            lm[4][0],
            "repère 4 = bout du pouce"
        );
    }

    fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
        (a[0] - b[0]).hypot(a[1] - b[1])
    }

    #[test]
    fn open_hand_fist_and_pinch_stay_recognisable_after_projection() {
        let cam = VirtualWebcam::facing(head(), Vec3::NEG_Z);
        let size = |lm: &[[f32; 3]; 21]| dist2(lm[0], lm[9]);
        let open = cam.hand_landmarks(&right_hand(0.0, 0.0));
        let fist = cam.hand_landmarks(&right_hand(0.0, 1.0));
        let pinched = cam.hand_landmarks(&right_hand(1.0, 0.0));
        // Bout du majeur loin du poignet main ouverte, ramené poing fermé.
        let reach = |lm: &[[f32; 3]; 21]| dist2(lm[0], lm[12]) / size(lm);
        assert!(
            reach(&open) > reach(&fist) * 1.3,
            "{} vs {}",
            reach(&open),
            reach(&fist)
        );
        // Pouce-index : écartés main ouverte, joints au pincement.
        let gap = |lm: &[[f32; 3]; 21]| dist2(lm[4], lm[8]) / size(lm);
        assert!(
            gap(&pinched) < 0.15 && gap(&open) > 0.3,
            "{} / {}",
            gap(&pinched),
            gap(&open)
        );
        assert!(pinch_distance(&right_hand(1.0, 0.0)) < PINCH_CLICK);
        assert!(pinch_distance(&right_hand(0.0, 0.0)) > PINCH_CLICK);
    }

    #[test]
    fn flat_formats_decode_into_the_rehabilitation_frames() {
        let cam = VirtualWebcam::facing(head(), Vec3::NEG_Z);
        let j = right_hand(0.0, 0.0);
        let mut hands = HandFrame::default();
        hands.apply_flat(&hands_flat(&cam, [None, Some(&j)]));
        assert!(hands.is_ok() && hands.right.is_some() && hands.left.is_none());
        let mut pose = PoseFrame::default();
        pose.apply_flat(&body_flat(&cam, head(), [None, Some(j[WRIST])]));
        assert!(pose.is_ok());
        let lm = pose.sampled();
        // Tête au-dessus des épaules, épaules au-dessus des hanches (y image vers le bas).
        assert!(lm[0].y < lm[11].y && lm[11].y < lm[23].y);
        // Épaule droite du patient à gauche de l'image, comme sur une webcam.
        assert!(lm[12].x < lm[11].x);
        // Poignet droit du corps = poignet de la main suivie.
        let (wx, wy, _) = cam.project(j[WRIST]);
        assert!((lm[16].x - wx).abs() < 1e-5 && (lm[16].y - wy).abs() < 1e-5);
        assert!(lm[16].visibility > 0.5 && lm[1].visibility < 0.5);
    }
}
