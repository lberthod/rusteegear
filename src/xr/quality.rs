//! Réglages de qualité VR côté casque (phase 2) : fréquence d'affichage et
//! résolution de rendu par œil. Logique pure, testée sur le poste de dev ;
//! appliquée par `xr::hello` (fixée à la construction de l'APK :
//! `VR_HZ=90 VR_RENDER_SCALE=0.8 ./packaging/build_quest.sh`).

/// Fréquence par défaut : 72 Hz, le mode de base de tous les Quest — 90 Hz
/// seulement quand les mesures au casque montrent qu'il tient (13,9 ms de
/// budget par image au lieu de 11,1).
pub const DEFAULT_REFRESH_HZ: f32 = 72.0;

/// Fréquence à demander au runtime parmi celles qu'il annonce
/// (`xrEnumerateDisplayRefreshRatesFB`) : la plus proche de `wanted` sans la
/// dépasser, à défaut la plus basse. `None` si la liste est vide.
pub fn pick_refresh_rate(available: &[f32], wanted: f32) -> Option<f32> {
    let at_most = available
        .iter()
        .copied()
        .filter(|&r| r <= wanted + 0.5)
        .fold(None, |best: Option<f32>, r| {
            Some(best.map_or(r, |b| b.max(r)))
        });
    at_most.or_else(|| available.iter().copied().reduce(f32::min))
}

/// Taille de rendu d'un œil : la résolution recommandée × `scale` (le
/// compositeur agrandit l'image), bornée par le maximum du runtime et par
/// 1 pixel ; `scale` borné à 0,5..1,5.
pub fn eye_size(recommended: (u32, u32), max: (u32, u32), scale: f32) -> (u32, u32) {
    let s = scale.clamp(0.5, 1.5);
    let dim = |rec: u32, max: u32| (((rec as f32) * s).round() as u32).clamp(1, max.max(1));
    (dim(recommended.0, max.0), dim(recommended.1, max.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_rate_is_the_closest_not_above_the_wanted_one() {
        let quest3 = [72.0, 80.0, 90.0, 120.0];
        assert_eq!(pick_refresh_rate(&quest3, 72.0), Some(72.0));
        assert_eq!(pick_refresh_rate(&quest3, 90.0), Some(90.0));
        assert_eq!(pick_refresh_rate(&quest3, 100.0), Some(90.0));
        assert_eq!(pick_refresh_rate(&quest3, 60.0), Some(72.0));
        assert_eq!(pick_refresh_rate(&[], 72.0), None);
    }

    #[test]
    fn eye_size_scales_and_respects_the_runtime_maximum() {
        assert_eq!(eye_size((2064, 2208), (4128, 4416), 0.8), (1651, 1766));
        assert_eq!(eye_size((2064, 2208), (2200, 2300), 1.5), (2200, 2300));
        assert_eq!(eye_size((2064, 2208), (4128, 4416), 0.1), (1032, 1104));
    }
}
