//! Prédiction client & interpolation des entités distantes (SPRINT_MMORPG.md,
//! Sprint 54). Logique **pure** (aucune dépendance à `winit`/`AppState`) : ce
//! qui rend le jeu jouable malgré la latence tient dans ces deux règles —
//! interpoler les autres entités entre deux snapshots plutôt que les téléporter
//! à chaque tick réseau, et ne corriger la position prédite du joueur local que
//! si l'écart devient significatif (sinon la moindre latence réseau produirait
//! une micro-saccade visible à chaque snapshot).

use std::time::Instant;

use glam::Vec3;

use super::protocol::EntityDelta;

/// Une valeur horodatée à l'heure de réception **locale** (pas le tick serveur) :
/// on interpole en temps client réel, pas en ticks réseau, pour rester correct
/// quel que soit le jitter du réseau.
#[derive(Clone, Copy, Debug)]
struct Timed<T> {
    at: Instant,
    value: T,
}

/// Historique borné (2 derniers snapshots) pour **une** entité distante — assez
/// pour interpoler entre « avant » et « après » l'instant courant, sans
/// accumuler un historique non borné (inutile : on ne rejoue jamais le passé).
#[derive(Default)]
pub struct RemoteEntity {
    prev: Option<Timed<EntityDelta>>,
    latest: Option<Timed<EntityDelta>>,
}

impl RemoteEntity {
    /// Enregistre un nouveau snapshot reçu pour cette entité, à l'heure locale
    /// `at` (généralement `Instant::now()` au moment de la réception réseau).
    pub fn push(&mut self, delta: EntityDelta, at: Instant) {
        self.prev = self.latest.take();
        self.latest = Some(Timed { at, value: delta });
    }

    /// Position, orientation (yaw) et visibilité interpolées à l'instant `now`.
    /// `None` tant qu'aucun snapshot n'a été reçu. Avec un seul snapshot reçu,
    /// retourne cet état brut (rien à interpoler avec un seul point).
    pub fn sample(&self, now: Instant) -> Option<(Vec3, f32, bool)> {
        match (&self.prev, &self.latest) {
            (Some(prev), Some(latest)) => {
                let span = latest.at.saturating_duration_since(prev.at).as_secs_f32();
                let t = if span <= 1e-6 {
                    1.0
                } else {
                    (now.saturating_duration_since(prev.at).as_secs_f32() / span).clamp(0.0, 1.0)
                };
                let a = Vec3::from_array(prev.value.position);
                let b = Vec3::from_array(latest.value.position);
                Some((
                    a.lerp(b, t),
                    lerp_angle(prev.value.yaw, latest.value.yaw, t),
                    latest.value.visible,
                ))
            }
            (None, Some(latest)) => Some((
                Vec3::from_array(latest.value.position),
                latest.value.yaw,
                latest.value.visible,
            )),
            (Some(_), None) | (None, None) => None,
        }
    }
}

/// Interpolation angulaire par le chemin le plus court (évite un détour d'un
/// tour complet quand l'angle passe de ~π à ~-π, ex. un demi-tour du joueur).
fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    const TAU: f32 = std::f32::consts::TAU;
    const PI: f32 = std::f32::consts::PI;
    let mut diff = (b - a) % TAU;
    if diff > PI {
        diff -= TAU;
    } else if diff < -PI {
        diff += TAU;
    }
    a + diff * t
}

/// Écart de position (m) au-delà duquel on corrige la prédiction du joueur
/// local plutôt que de la laisser dériver — cf. `reconcile`.
pub const SNAP_THRESHOLD: f32 = 0.5;

/// Réconciliation du joueur local : le client applique son input immédiatement
/// (prédiction, cf. `AppState::sim_step`, inchangé par ce sprint — c'est déjà la
/// prédiction). À réception d'un snapshot serveur pour ce joueur, ne corriger
/// que si l'écart dépasse `SNAP_THRESHOLD` (sinon un aller-retour réseau normal
/// produirait une micro-saccade à chaque snapshot, pour rien). Retourne la
/// position à appliquer si une correction est nécessaire, `None` sinon.
pub fn reconcile(predicted: Vec3, authoritative: Vec3) -> Option<Vec3> {
    if predicted.distance(authoritative) > SNAP_THRESHOLD {
        Some(authoritative)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn delta(pos: [f32; 3], yaw: f32) -> EntityDelta {
        EntityDelta {
            index: 0,
            position: pos,
            yaw,
            visible: true,
            health: None,
        }
    }

    #[test]
    fn sample_is_none_before_any_snapshot() {
        let e = RemoteEntity::default();
        assert!(e.sample(Instant::now()).is_none());
    }

    #[test]
    fn sample_returns_raw_state_with_a_single_snapshot() {
        let mut e = RemoteEntity::default();
        let t0 = Instant::now();
        e.push(delta([1.0, 0.0, 2.0], 0.5), t0);
        let (pos, yaw, visible) = e.sample(t0 + Duration::from_millis(30)).unwrap();
        assert_eq!(pos, Vec3::new(1.0, 0.0, 2.0));
        assert_eq!(yaw, 0.5);
        assert!(visible);
    }

    #[test]
    fn sample_interpolates_at_the_midpoint_between_two_snapshots() {
        let mut e = RemoteEntity::default();
        let t0 = Instant::now();
        e.push(delta([0.0, 0.0, 0.0], 0.0), t0);
        e.push(
            delta([10.0, 0.0, 0.0], 0.0),
            t0 + Duration::from_millis(100),
        );

        let (pos, ..) = e.sample(t0 + Duration::from_millis(50)).unwrap();
        assert!(
            (pos.x - 5.0).abs() < 1e-3,
            "à mi-chemin temporel, la position doit être à mi-chemin spatial : {pos:?}"
        );
    }

    #[test]
    fn sample_clamps_before_the_first_and_after_the_last_snapshot() {
        let mut e = RemoteEntity::default();
        let t0 = Instant::now();
        e.push(delta([0.0, 0.0, 0.0], 0.0), t0);
        e.push(
            delta([10.0, 0.0, 0.0], 0.0),
            t0 + Duration::from_millis(100),
        );

        let (before, ..) = e.sample(t0 - Duration::from_millis(50)).unwrap();
        assert_eq!(
            before.x, 0.0,
            "avant le premier snapshot : pas d'extrapolation en arrière"
        );
        let (after, ..) = e.sample(t0 + Duration::from_millis(500)).unwrap();
        assert_eq!(
            after.x, 10.0,
            "après le dernier snapshot : reste sur le dernier état connu"
        );
    }

    #[test]
    fn sample_takes_the_shortest_path_across_the_angle_wraparound() {
        let mut e = RemoteEntity::default();
        let t0 = Instant::now();
        // Passe de ~+π à ~-π : le chemin court traverse π/-π, pas un demi-tour
        // par 0.
        e.push(delta([0.0, 0.0, 0.0], 3.0), t0);
        e.push(
            delta([0.0, 0.0, 0.0], -3.0),
            t0 + Duration::from_millis(100),
        );

        let (_, yaw, _) = e.sample(t0 + Duration::from_millis(50)).unwrap();
        // Le chemin court entre 3.0 et -3.0 passe par π (~3.14), pas par 0.
        assert!(
            yaw.abs() > 3.0,
            "l'interpolation doit passer par le chemin court (autour de π), pas par 0 : yaw={yaw}"
        );
    }

    #[test]
    fn reconcile_ignores_small_prediction_drift() {
        let predicted = Vec3::new(1.0, 0.0, 1.0);
        let authoritative = Vec3::new(1.1, 0.0, 1.0); // 0.1 m d'écart
        assert!(reconcile(predicted, authoritative).is_none());
    }

    #[test]
    fn reconcile_snaps_on_significant_drift() {
        let predicted = Vec3::new(0.0, 0.0, 0.0);
        let authoritative = Vec3::new(2.0, 0.0, 0.0); // 2 m d'écart
        assert_eq!(reconcile(predicted, authoritative), Some(authoritative));
    }
}
