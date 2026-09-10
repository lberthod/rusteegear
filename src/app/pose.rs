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
#[derive(Clone, Debug, PartialEq)]
pub struct PoseFrame {
    pub landmarks: [Landmark; LANDMARK_COUNT],
    /// Au moins une image complète a été reçue depuis le lancement.
    pub present: bool,
    /// Pas de simulation écoulés depuis la dernière image (cf. `STALE_AFTER_TICKS`).
    pub age: u32,
}

impl Default for PoseFrame {
    fn default() -> Self {
        Self {
            landmarks: [Landmark::default(); LANDMARK_COUNT],
            present: false,
            age: u32::MAX,
        }
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

    #[test]
    fn named_landmarks_are_within_the_model_range_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (name, idx) in NAMED {
            assert!(idx < LANDMARK_COUNT, "{name}: index {idx} hors modèle");
            assert!(seen.insert(idx), "{name}: index {idx} dupliqué");
        }
    }
}
