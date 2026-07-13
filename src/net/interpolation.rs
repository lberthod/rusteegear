//! Prédiction client & interpolation des entités distantes (SPRINT_MMORPG.md,
//! Sprint 54). Logique **pure** (aucune dépendance à `winit`/`AppState`) : ce
//! qui rend le jeu jouable malgré la latence tient dans ces deux règles —
//! interpoler les autres entités entre deux snapshots plutôt que les téléporter
//! à chaque tick réseau, et ne corriger la position prédite du joueur local que
//! si l'écart devient significatif (sinon la moindre latence réseau produirait
//! une micro-saccade visible à chaque snapshot).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use glam::Vec3;

use super::protocol::EntityDelta;

/// Une valeur horodatée à l'heure de réception **locale** (pas le tick serveur) :
/// on interpole en temps client réel, pas en ticks réseau, pour rester correct
/// quel que soit le jitter du réseau.
#[derive(Clone, Debug)]
struct Timed<T> {
    at: Instant,
    value: T,
}

/// Nombre de snapshots conservés par entité distante (Sprint 67,
/// `SPRINTNETWORK.md`) — plus que les 2 minimums nécessaires pour interpoler
/// « avant/après » l'instant courant : sert à retrouver les deux snapshots qui
/// encadrent un instant **passé** (`now - RENDER_DELAY`, cf. `sample_delayed`)
/// même quand le dernier paquet en date est arrivé en retard. Toujours borné
/// (pas un historique complet) : on ne rejoue jamais plus loin que
/// `RENDER_DELAY` dans le passé.
const HISTORY_CAPACITY: usize = 6;

/// Délai de rendu (Sprint 67, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.4) : les
/// fantômes distants sont affichés à `now - RENDER_DELAY` plutôt qu'à `now`.
/// Sans ce délai, `sample` interpolait entre les deux *derniers* snapshots
/// reçus — correct tant qu'ils arrivent à intervalle régulier, mais dès qu'un
/// paquet est retardé au-delà de cet intervalle, l'échantillon se figeait sur
/// le dernier état connu jusqu'à l'arrivée du suivant (saccade visible sous
/// gigue réseau réelle). En restant systématiquement un peu dans le passé, il
/// y a presque toujours un snapshot de part et d'autre de l'instant demandé
/// dans l'historique (`HISTORY_CAPACITY`), même si le tout dernier paquet est
/// en retard. Valeur choisie assez large devant un tick serveur de 16 ms
/// (`SERVER_TICK`, `src/bin/server.rs`) pour absorber une gigue réaliste, assez
/// courte pour ne pas afficher un état visiblement périmé.
pub const RENDER_DELAY: Duration = Duration::from_millis(100);

/// Historique borné (`HISTORY_CAPACITY` derniers snapshots) pour **une**
/// entité distante — assez pour interpoler entre les deux points qui encadrent
/// un instant passé (`RENDER_DELAY` derrière `now`), sans accumuler un
/// historique non borné (inutile : on ne rejoue jamais plus loin que ça).
#[derive(Default)]
pub struct RemoteEntity {
    history: VecDeque<Timed<EntityDelta>>,
}

impl RemoteEntity {
    /// Enregistre un nouveau snapshot reçu pour cette entité, à l'heure locale
    /// `at` (généralement `Instant::now()` au moment de la réception réseau).
    /// Évince le plus ancien si `HISTORY_CAPACITY` est dépassé.
    pub fn push(&mut self, delta: EntityDelta, at: Instant) {
        self.history.push_back(Timed { at, value: delta });
        if self.history.len() > HISTORY_CAPACITY {
            self.history.pop_front();
        }
    }

    /// Position, orientation (yaw) et visibilité interpolées à l'instant `at`.
    /// `None` tant qu'aucun snapshot n'a été reçu. Avec un seul snapshot reçu,
    /// retourne cet état brut (rien à interpoler avec un seul point). En
    /// dehors de la plage couverte par l'historique, clampe au premier/dernier
    /// point connu plutôt que d'extrapoler.
    pub fn sample(&self, at: Instant) -> Option<(Vec3, f32, bool)> {
        let first = self.history.front()?;
        if self.history.len() == 1 || at <= first.at {
            return Some(raw(first));
        }
        let last = self.history.back().expect("len() >= 2 vérifié ci-dessus");
        if at >= last.at {
            return Some(raw(last));
        }
        // `history` est trié par `at` croissant (append-only via `push_back`) :
        // trouve la paire consécutive qui encadre `at`.
        for pair in Vec::from_iter(self.history.iter().cloned()).windows(2) {
            let (prev, next) = (&pair[0], &pair[1]);
            if prev.at <= at && at <= next.at {
                let span = next.at.saturating_duration_since(prev.at).as_secs_f32();
                let t = if span <= 1e-6 {
                    1.0
                } else {
                    (at.saturating_duration_since(prev.at).as_secs_f32() / span).clamp(0.0, 1.0)
                };
                let a = Vec3::from_array(prev.value.position);
                let b = Vec3::from_array(next.value.position);
                return Some((
                    a.lerp(b, t),
                    lerp_angle(prev.value.yaw, next.value.yaw, t),
                    next.value.visible,
                ));
            }
        }
        unreachable!("les deux clamps ci-dessus couvrent tout instant hors [first.at, last.at]")
    }

    /// Comme `sample`, mais à `now - RENDER_DELAY` plutôt qu'à `now` — à
    /// utiliser pour l'affichage des fantômes distants (cf.
    /// `app::network_client::poll_network`), pas pour la réconciliation du
    /// joueur local (`net_local_interp`, qui reste échantillonné à `now` :
    /// retarder la référence autoritative y ferait dériver systématiquement
    /// la position prédite, plus avancée de `RENDER_DELAY`, et déclencherait
    /// des corrections inutiles).
    pub fn sample_delayed(&self, now: Instant) -> Option<(Vec3, f32, bool)> {
        self.sample(now.checked_sub(RENDER_DELAY).unwrap_or(now))
    }

    /// Dernier clip d'animation connu pour cette entité (Sprint 88, réplication de
    /// l'animation). Contrairement à la position, on ne l'interpole pas dans le
    /// temps : chaque client avance déjà localement le temps de lecture de son
    /// propre `AnimationState` à chaque pas fixe, qu'il s'agisse d'un objet local
    /// ou d'un fantôme réseau (cf. `AppState::sim_step`) — seul le *choix* du
    /// clip a besoin d'être répliqué, via `AnimationState::set_clip()` (fondu
    /// enchaîné inclus, cf. Sprint 87). `None` tant qu'aucun snapshot n'est
    /// encore arrivé.
    pub fn latest_anim_clip(&self) -> Option<&str> {
        self.history.back().map(|t| t.value.anim_clip.as_str())
    }
}

fn raw(t: &Timed<EntityDelta>) -> (Vec3, f32, bool) {
    (
        Vec3::from_array(t.value.position),
        t.value.yaw,
        t.value.visible,
    )
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
            player_id: None,
            position: pos,
            yaw,
            visible: true,
            health: None,
            anim_clip: String::new(),
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
    fn latest_anim_clip_is_none_before_any_snapshot() {
        let e = RemoteEntity::default();
        assert_eq!(e.latest_anim_clip(), None);
    }

    #[test]
    fn latest_anim_clip_tracks_the_most_recent_snapshot() {
        // Sprint 88 : le clip répliqué doit refléter le dernier snapshot reçu, pas
        // le premier — contrairement à la position, jamais interpolé/mélangé.
        let mut e = RemoteEntity::default();
        let t0 = Instant::now();
        let mut idle = delta([0.0, 0.0, 0.0], 0.0);
        idle.anim_clip = "idle".into();
        e.push(idle, t0);
        assert_eq!(e.latest_anim_clip(), Some("idle"));

        let mut run = delta([1.0, 0.0, 0.0], 0.0);
        run.anim_clip = "run".into();
        e.push(run, t0 + Duration::from_millis(50));
        assert_eq!(e.latest_anim_clip(), Some("run"));
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

    /// Sprint 67 (`SPRINTNETWORK.md`, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.4) :
    /// avant ce sprint, `RemoteEntity` ne gardait que les 2 derniers snapshots
    /// — dès que le dernier paquet en date était en retard, `sample(now)`
    /// dépassait `latest.at` et se figeait sur le dernier état connu.
    /// `sample_delayed` interroge `now - RENDER_DELAY`, un instant qui reste
    /// généralement encadré par deux snapshots déjà reçus dans l'historique
    /// élargi (`HISTORY_CAPACITY`) même quand le tout dernier paquet accuse un
    /// retard inférieur à `RENDER_DELAY` — donc toujours interpolé en douceur,
    /// pas figé.
    #[test]
    fn sample_delayed_keeps_interpolating_smoothly_when_the_latest_packet_is_late() {
        let mut e = RemoteEntity::default();
        let t0 = Instant::now();
        // Cadence régulière toutes les 20 ms, jusqu'à t0+100ms (6 points,
        // exactement HISTORY_CAPACITY).
        for i in 0..6 {
            e.push(
                delta([i as f32 * 10.0, 0.0, 0.0], 0.0),
                t0 + Duration::from_millis(i * 20),
            );
        }
        // Horloge actuelle 130 ms après t0 : le paquet attendu vers t0+120ms
        // n'est pas encore arrivé (retard réseau de 30 ms), `latest.at` reste
        // donc à t0+100ms.
        let now = t0 + Duration::from_millis(130);

        // `sample(now)` directement : dépasse `latest.at` (100 ms), se fige
        // sur le dernier état connu (position x=50.0).
        let (frozen, ..) = e.sample(now).unwrap();
        assert_eq!(
            frozen.x, 50.0,
            "sample(now) directement doit rester figé sur le dernier état connu"
        );

        // `sample_delayed(now)` vise now - RENDER_DELAY (100 ms) = t0+30ms,
        // encadré par les snapshots à t0+20ms (x=10.0) et t0+40ms (x=20.0) :
        // toujours interpolé, jamais figé sur une extrémité.
        let (delayed, ..) = e.sample_delayed(now).unwrap();
        assert!(
            delayed.x > 10.0 && delayed.x < 20.0,
            "sample_delayed doit continuer d'interpoler entre deux points connus \
             plutôt que de se figer : x={}",
            delayed.x
        );
    }

    /// L'historique ne doit jamais grandir sans borne (`HISTORY_CAPACITY`) :
    /// après plus de pushs que la capacité, les échantillons les plus anciens
    /// sont évincés — vérifié en interrogeant un instant qui ne correspond
    /// plus qu'au point le plus ancien restant (clampé, pas une erreur).
    #[test]
    fn history_never_grows_past_its_capacity() {
        let mut e = RemoteEntity::default();
        let t0 = Instant::now();
        for i in 0..50 {
            e.push(
                delta([i as f32, 0.0, 0.0], 0.0),
                t0 + Duration::from_millis(i * 16),
            );
        }
        // Le tout premier point poussé (x=0.0 à t0) doit avoir été évincé
        // depuis longtemps : interroger avant même t0 doit clamper sur le
        // plus ancien point *restant*, pas sur x=0.0.
        let (before, ..) = e.sample(t0 - Duration::from_secs(1)).unwrap();
        assert_ne!(
            before.x, 0.0,
            "le tout premier snapshot doit avoir été évincé de l'historique borné"
        );
    }
}
