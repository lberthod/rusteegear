//! Particules CPU (Sprint 132) : pool de quads billboardés, alpha-blend triés
//! par distance au rendu (`gfx::renderer`), pilotés soit par un émetteur posé
//! sur un `SceneObject` (`SceneObject::particle_emitter`, éditable dans
//! l'inspecteur et scriptable via `particles(...)`, cf. `app::scripting`), soit
//! par une rafale ponctuelle (`ParticlePool::spawn_burst`) déclenchée par le
//! moteur — c'est cette seconde voie qui donne au vent (`wind(force)`, mode
//! plateformer 2D) sa traînée visuelle automatique, cf.
//! `AppState::apply_script_outcomes`.
//!
//! Pool à plat (`Vec<Particle>`), capacité bornée (`MAX_LIVE_PARTICLES`),
//! suppression par `swap_remove` : aucune allocation par particule à l'usage
//! régulier, et un budget dépassé laisse simplement tomber les nouveaux
//! spawns plutôt que de faire grossir le pool sans borne — la description du
//! sprint est explicite sur ce point (« jamais de falaise de temps de frame »).

use glam::Vec3;
use serde::{Deserialize, Serialize};

use super::rng::Rng;

/// Nombre maximum de particules vivantes simultanément, tous émetteurs et
/// rafales confondus. Au-delà, un nouveau spawn est silencieusement ignoré.
pub const MAX_LIVE_PARTICLES: usize = 4000;

fn default_true() -> bool {
    true
}
fn default_rate() -> f32 {
    20.0
}
fn default_lifetime_min() -> f32 {
    0.5
}
fn default_lifetime_max() -> f32 {
    1.2
}
fn default_speed_min() -> f32 {
    1.0
}
fn default_speed_max() -> f32 {
    2.5
}
fn default_direction() -> [f32; 3] {
    [0.0, 1.0, 0.0]
}
fn default_spread() -> f32 {
    0.35
}
fn default_size_min() -> f32 {
    0.05
}
fn default_size_max() -> f32 {
    0.15
}
fn white() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
fn default_alpha() -> f32 {
    1.0
}

/// Émetteur de particules : posé sur `SceneObject::particle_emitter`
/// (`None` = aucune particule, zéro coût, comme `water`/`audio`), édité dans
/// l'inspecteur (section « Particules »), et réutilisé tel quel comme
/// paramètres d'une rafale ponctuelle (`ParticlePool::spawn_burst`).
#[derive(Clone, Serialize, Deserialize)]
pub struct ParticleEmitter {
    /// Émission active ce tick. Un émetteur désactivé garde ses réglages
    /// (édition dans l'inspecteur sans perdre la config) mais ne spawn rien —
    /// même idiome que `trigger` pour les zones de vent.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Particules émises par seconde (fraction accumulée d'un pas à l'autre,
    /// cf. `ParticlePool::update` — un taux de 0,5/s spawn bien une particule
    /// toutes les deux secondes en moyenne, pas 0 ou 1 selon l'arrondi du pas).
    #[serde(default = "default_rate")]
    pub rate: f32,
    #[serde(default = "default_lifetime_min")]
    pub lifetime_min: f32,
    #[serde(default = "default_lifetime_max")]
    pub lifetime_max: f32,
    #[serde(default = "default_speed_min")]
    pub speed_min: f32,
    #[serde(default = "default_speed_max")]
    pub speed_max: f32,
    /// Direction de base (monde), pas nécessairement normalisée dans le JSON
    /// — normalisée à l'usage par `direction_vec3()`. `[0,1,0]` par défaut
    /// (fumée montante).
    #[serde(default = "default_direction")]
    pub direction: [f32; 3],
    /// Demi-angle (radians) du cône d'émission autour de `direction` : `0` =
    /// toutes les particules partent exactement dans `direction`.
    #[serde(default = "default_spread")]
    pub spread: f32,
    #[serde(default = "default_size_min")]
    pub size_min: f32,
    #[serde(default = "default_size_max")]
    pub size_max: f32,
    #[serde(default = "white")]
    pub color: [f32; 3],
    /// Opacité de départ (0..1) — s'estompe linéairement jusqu'à 0 en fin de
    /// vie au rendu (cf. `gfx::renderer::sync`), jamais mutée dans le pool.
    #[serde(default = "default_alpha")]
    pub start_alpha: f32,
    /// Accélération le long de -Y (m/s²) : positif = tombe (étincelles,
    /// débris), négatif = monte (fumée). `0` = trajectoire rectiligne.
    #[serde(default)]
    pub gravity: f32,
    /// Freinage exponentiel de la vitesse (1/s), `0` = aucun.
    #[serde(default)]
    pub drag: f32,
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            rate: default_rate(),
            lifetime_min: default_lifetime_min(),
            lifetime_max: default_lifetime_max(),
            speed_min: default_speed_min(),
            speed_max: default_speed_max(),
            direction: default_direction(),
            spread: default_spread(),
            size_min: default_size_min(),
            size_max: default_size_max(),
            color: white(),
            start_alpha: default_alpha(),
            gravity: 0.0,
            drag: 0.0,
        }
    }
}

impl ParticleEmitter {
    /// `direction`, normalisée — retombe sur `+Y` si nulle (évite une
    /// direction dégénérée sur une scène mal éditée à la main).
    pub fn direction_vec3(&self) -> Vec3 {
        let d = Vec3::from(self.direction);
        if d.length_squared() > 1e-8 {
            d.normalize()
        } else {
            Vec3::Y
        }
    }
}

/// Une particule vivante. Jamais sérialisée (état runtime pur, comme
/// `SceneObject::bone_dirs`) — reconstruite entièrement par les émetteurs à
/// chaque entrée en Play.
#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub size: f32,
    pub color: [f32; 3],
    /// Opacité de départ (avant l'estompage par âge appliqué au rendu).
    pub start_alpha: f32,
    pub age: f32,
    pub lifetime: f32,
    gravity: f32,
    drag: f32,
}

impl Particle {
    /// Opacité courante (0..1) : estompage linéaire de `start_alpha` à `0`
    /// entre le milieu et la fin de la vie de la particule — les particules
    /// fraîches restent pleinement visibles au lieu de s'estomper dès la
    /// naissance, plus lisible pour une traînée courte (étincelle, poussière).
    pub fn current_alpha(&self) -> f32 {
        let t = (self.age / self.lifetime.max(1e-4)).clamp(0.0, 1.0);
        let fade = if t < 0.5 { 1.0 } else { 1.0 - (t - 0.5) * 2.0 };
        self.start_alpha * fade
    }
}

/// Direction aléatoire dans un cône de demi-angle `spread` (radians) autour
/// de `base` (déjà normalisée). `spread <= 0` renvoie `base` telle quelle.
fn jitter_direction(base: Vec3, spread: f32, rng: &mut Rng) -> Vec3 {
    if spread <= 1e-4 {
        return base;
    }
    // Base orthonormée autour de `base` : n'importe quel axe non colinéaire
    // convient comme aide, `X` si `base` est proche de `Y` (cas fréquent :
    // direction verticale par défaut), `Y` sinon.
    let helper = if base.dot(Vec3::Y).abs() > 0.99 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let t1 = base.cross(helper).normalize();
    let t2 = base.cross(t1);
    let az = rng.next_range(0.0, std::f32::consts::TAU);
    let polar = rng.next_range(0.0, spread);
    (base * polar.cos() + (t1 * az.cos() + t2 * az.sin()) * polar.sin()).normalize_or_zero()
}

/// Pool CPU de particules vivantes, plus l'accumulateur de spawn fractionnaire
/// par objet-émetteur (indexé comme `Scene::objects`).
pub struct ParticlePool {
    particles: Vec<Particle>,
    spawn_accum: Vec<f32>,
    rng: Rng,
}

impl Default for ParticlePool {
    fn default() -> Self {
        // Graine fixe plutôt que `Rng::from_system_time()` (Sprint 131) : la
        // dispersion des particules n'a pas besoin d'imprévisibilité entre deux
        // lancements, et une graine fixe garde les tests déterministes sans
        // exiger que chaque appelant passe la sienne.
        Self::with_seed(0xC0FF_EE01)
    }
}

impl ParticlePool {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            particles: Vec::new(),
            spawn_accum: Vec::new(),
            rng: Rng::new(seed),
        }
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Particle> {
        self.particles.iter()
    }

    /// Vide le pool sans le désallouer (entrée/sortie de Play, cf.
    /// `AppState` — mêmes garanties que `sim_poses` remis à zéro).
    pub fn clear(&mut self) {
        self.particles.clear();
        self.spawn_accum.clear();
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_one(
        &mut self,
        pos: Vec3,
        vel: Vec3,
        size: f32,
        color: [f32; 3],
        start_alpha: f32,
        lifetime: f32,
        gravity: f32,
        drag: f32,
    ) {
        if self.particles.len() >= MAX_LIVE_PARTICLES {
            return;
        }
        self.particles.push(Particle {
            pos,
            vel,
            size: size.max(0.001),
            color,
            start_alpha,
            age: 0.0,
            lifetime: lifetime.max(0.01),
            gravity,
            drag,
        });
    }

    /// Rafale ponctuelle de `count` particules à `pos`, paramétrée par `cfg`
    /// (réutilise `ParticleEmitter` comme jeu de réglages — `enabled`/`rate`
    /// ignorés) : utilisée pour une traînée déclenchée par le moteur plutôt
    /// que par un émetteur posé sur un objet (cf. couplage `wind()`).
    pub fn spawn_burst(&mut self, pos: Vec3, count: u32, cfg: &ParticleEmitter) {
        let base_dir = cfg.direction_vec3();
        for _ in 0..count {
            let speed = self.rng.next_range(cfg.speed_min, cfg.speed_max);
            let dir = jitter_direction(base_dir, cfg.spread, &mut self.rng);
            let size = self.rng.next_range(cfg.size_min, cfg.size_max);
            let lifetime = self.rng.next_range(cfg.lifetime_min, cfg.lifetime_max);
            self.spawn_one(
                pos,
                dir * speed,
                size,
                cfg.color,
                cfg.start_alpha,
                lifetime,
                cfg.gravity,
                cfg.drag,
            );
        }
    }

    /// Avance le pool d'un pas de simulation fixe : fait émettre chaque
    /// `SceneObject` dont l'émetteur est actif et visible (taux `rate`,
    /// fraction accumulée par objet), intègre position/vitesse de chaque
    /// particule vivante (gravité + freinage propres à sa particule, posés à
    /// sa naissance), puis élague les particules mortes.
    pub fn update(&mut self, dt: f32, scene: &crate::scene::Scene) {
        if self.spawn_accum.len() < scene.objects.len() {
            self.spawn_accum.resize(scene.objects.len(), 0.0);
        }
        for (idx, obj) in scene.objects.iter().enumerate() {
            let Some(em) = obj.particle_emitter.as_ref() else {
                continue;
            };
            if !em.enabled || !obj.visible || em.rate <= 0.0 {
                continue;
            }
            self.spawn_accum[idx] += em.rate * dt;
            let base_dir = em.direction_vec3();
            while self.spawn_accum[idx] >= 1.0 {
                self.spawn_accum[idx] -= 1.0;
                let speed = self.rng.next_range(em.speed_min, em.speed_max);
                let dir = jitter_direction(base_dir, em.spread, &mut self.rng);
                let size = self.rng.next_range(em.size_min, em.size_max);
                let lifetime = self.rng.next_range(em.lifetime_min, em.lifetime_max);
                self.spawn_one(
                    obj.transform.position,
                    dir * speed,
                    size,
                    em.color,
                    em.start_alpha,
                    lifetime,
                    em.gravity,
                    em.drag,
                );
            }
        }

        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            p.age += dt;
            if p.age >= p.lifetime {
                self.particles.swap_remove(i);
                continue;
            }
            p.vel.y -= p.gravity * dt;
            if p.drag > 0.0 {
                p.vel *= (1.0 - p.drag * dt).max(0.0);
            }
            p.pos += p.vel * dt;
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Scene, SceneObject};

    fn scene_with_emitter(em: ParticleEmitter) -> Scene {
        let mut scene = Scene::default();
        let mut obj = SceneObject {
            particle_emitter: Some(em),
            ..SceneObject::default()
        };
        obj.transform.position = Vec3::new(1.0, 2.0, 3.0);
        scene.objects.push(obj);
        scene
    }

    #[test]
    fn spawn_rate_produces_the_expected_average_count_over_one_second() {
        let em = ParticleEmitter {
            rate: 8.0,
            spread: 0.0,
            // Durées de vie hors de portée du test (1 s) : aucune ne meurt
            // avant la mesure, seul le compte de spawns est vérifié ici.
            lifetime_min: 100.0,
            lifetime_max: 100.0,
            ..Default::default()
        };
        let scene = scene_with_emitter(em);
        let mut pool = ParticlePool::with_seed(1);
        // 8 pas de 0,125 s = 1 s pile, à un taux de 8/s ⇒ 8 particules
        // exactement (0,125 = 1/8 est exact en binaire, aucune dérive
        // d'arrondi possible sur l'accumulateur fractionnaire contrairement à
        // 1/60, qui ne l'est pas).
        for _ in 0..8 {
            pool.update(0.125, &scene);
        }
        assert_eq!(pool.len(), 8);
    }

    #[test]
    fn disabled_emitter_never_spawns() {
        let em = ParticleEmitter {
            enabled: false,
            rate: 100.0,
            ..Default::default()
        };
        let scene = scene_with_emitter(em);
        let mut pool = ParticlePool::with_seed(1);
        for _ in 0..30 {
            pool.update(1.0 / 60.0, &scene);
        }
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn invisible_object_does_not_spawn_even_with_an_enabled_emitter() {
        let mut scene = scene_with_emitter(ParticleEmitter {
            rate: 100.0,
            ..Default::default()
        });
        scene.objects[0].visible = false;
        let mut pool = ParticlePool::with_seed(1);
        for _ in 0..30 {
            pool.update(1.0 / 60.0, &scene);
        }
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn particles_die_exactly_at_their_lifetime() {
        // Rafale directe plutôt qu'un émetteur : isole l'intégration
        // âge/mort de l'accumulateur de spawn (une particule qui vieillirait
        // du `dt` complet de son propre pas de naissance, cf. `spawn_burst`
        // suivi d'`update`, est le cas normal en jeu — ce test vérifie
        // seulement le seuil de mort une fois la particule vivante).
        let cfg = ParticleEmitter {
            lifetime_min: 0.5,
            lifetime_max: 0.5,
            speed_min: 0.0,
            speed_max: 0.0,
            gravity: 0.0,
            ..Default::default()
        };
        let scene = Scene::default();
        let mut pool = ParticlePool::with_seed(2);
        pool.spawn_burst(Vec3::ZERO, 1, &cfg);
        assert_eq!(pool.len(), 1);
        // Juste avant la fin de vie (0,5 s) : encore vivante.
        pool.update(0.49, &scene);
        assert_eq!(pool.len(), 1);
        // Passe le seuil de 0,5 s cumulé : morte.
        pool.update(0.02, &scene);
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn spawning_never_exceeds_the_global_cap() {
        let em = ParticleEmitter {
            rate: 1_000_000.0,
            lifetime_min: 1000.0,
            lifetime_max: 1000.0,
            ..Default::default()
        };
        let scene = scene_with_emitter(em);
        let mut pool = ParticlePool::with_seed(3);
        pool.update(10.0, &scene);
        assert_eq!(pool.len(), MAX_LIVE_PARTICLES);
    }

    #[test]
    fn same_seed_reproduces_the_same_burst() {
        let cfg = ParticleEmitter {
            speed_min: 1.0,
            speed_max: 5.0,
            size_min: 0.1,
            size_max: 0.3,
            lifetime_min: 0.2,
            lifetime_max: 0.8,
            spread: 0.6,
            ..Default::default()
        };
        let mut a = ParticlePool::with_seed(42);
        let mut b = ParticlePool::with_seed(42);
        a.spawn_burst(Vec3::ZERO, 20, &cfg);
        b.spawn_burst(Vec3::ZERO, 20, &cfg);
        let av: Vec<(f32, f32, f32)> = a.iter().map(|p| (p.pos.x, p.vel.x, p.lifetime)).collect();
        let bv: Vec<(f32, f32, f32)> = b.iter().map(|p| (p.pos.x, p.vel.x, p.lifetime)).collect();
        assert_eq!(av, bv);
    }

    #[test]
    fn burst_respects_the_global_cap_too() {
        let cfg = ParticleEmitter::default();
        let mut pool = ParticlePool::with_seed(4);
        pool.spawn_burst(Vec3::ZERO, MAX_LIVE_PARTICLES as u32 + 500, &cfg);
        assert_eq!(pool.len(), MAX_LIVE_PARTICLES);
    }

    #[test]
    fn jitter_direction_with_zero_spread_returns_the_base_direction_unchanged() {
        let mut rng = Rng::new(7);
        let base = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(jitter_direction(base, 0.0, &mut rng), base);
    }

    #[test]
    fn current_alpha_fades_to_zero_by_the_end_of_life() {
        let p = Particle {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            size: 0.1,
            color: [1.0, 1.0, 1.0],
            start_alpha: 1.0,
            age: 1.0,
            lifetime: 1.0,
            gravity: 0.0,
            drag: 0.0,
        };
        assert_eq!(p.current_alpha(), 0.0);
    }
}
