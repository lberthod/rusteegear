//! Boucle de simulation (Sprint 105a-1, extrait de `app/mod.rs` — code
//! inchangé, seulement déplacé) : `advance_play`/`sim_step`, accumulateur à
//! pas fixe, interpolation de poses de rendu.

use glam::{EulerRot, Quat, Vec3};
use std::collections::HashMap;

use crate::time_compat::Instant;

#[cfg(not(target_arch = "wasm32"))]
use super::scripting;
#[cfg(target_arch = "wasm32")]
use super::scripting_web;
use super::{AppState, multiplayer};
#[cfg(target_arch = "wasm32")]
use rilua::LuaApiMut;

/// Angle de plongée (radians) de la caméra de suivi par défaut : resserré derrière
/// l'épaule du personnage plutôt que le recul plus « isométrique » d'avant (~35°,
/// `0.62`) — plus proche d'une vue façon jeu d'action à la troisième personne.
pub(super) const DEFAULT_CHASE_PITCH: f32 = 0.75;

/// Recul (mètres) de la caméra de suivi par défaut : plus proche que l'ancien 11.0,
/// pour un cadrage plus serré façon caméra d'épaule.
pub(super) const DEFAULT_CHASE_DISTANCE: f32 = 7.0;

/// Décalage (mètres, axe Y) entre `player_position()` et la cible de la caméra de
/// suivi. `player_position()` renvoie `transform.position`, qui repère le CENTRE
/// pour la primitive `Capsule` (d'où l'ancien 0.8 ≈ mi-hauteur d'une capsule de 1 m
/// centrée à y=1.0, visant near le sommet) mais les PIEDS pour un mesh importé
/// (convention `physics.rs` : « un personnage a les pieds à l'origine »). Le héros
/// féérique (`assets/models/fairy_hero.glb`, ~1,95 m, pieds à l'origine) est monté
/// à `transform.position.y = 0.0` : viser la tête demande donc un décalage bien
/// plus grand qu'avec l'ancienne capsule centrée.
pub(super) const PLAYER_CAMERA_HEIGHT_OFFSET: f32 = 1.6;

/// Vitesse (rad/s) de la rotation « tank » manuelle (A/D tenus). Constante dédiée,
/// distincte de `Controller::turn_speed` : ce dernier (10 rad/s) est un taux de
/// *rattrapage* de l'orientation automatique (amorti exponentiel, la vitesse retombe
/// en approchant la cible) — tenu en continu comme vitesse brute, il ferait tourner
/// le personnage à ~570°/s, impossible à doser.
/// 3 rad/s ≈ 170°/s : demi-tour en ~1 s, vif mais contrôlable.
const MANUAL_TURN_SPEED: f32 = 3.0;

/// Nombre maximal de chasseurs (`AiChaser`) qui poursuivent activement la
/// **même** cible en même temps (cf. le bloc de pilotage IA plus haut) : au-delà,
/// les monstres en surnombre restent en place plutôt que de tous converger d'un
/// coup — sans ce plafond, un joueur seul face à plusieurs monstres se faisait
/// acculer contre un mur en quelques secondes, sans fenêtre de riposte. 2 = toujours
/// une vraie menace à plusieurs (pas trivialisé à un seul assaillant), sans jamais
/// submerger instantanément.
const MAX_ACTIVE_CHASERS_PER_TARGET: usize = 2;

/// Portée de détection (m) au-delà de laquelle un `AiChaser` reste totalement
/// immobile, quelle que soit la cible la plus proche parmi `candidate_targets`
/// (le plafond ci-dessus étale l'arrivée des chasseurs dans le temps, mais avec
/// un seul joueur solo, n'empêche pas la convergence *finale* : au bout d'assez
/// de temps, tous les monstres de la carte se relaient jusqu'à l'unique cible,
/// même partis de l'autre bout de l'arène). ~9 m : sur l'arène embarquée (24×24 m,
/// monstres à ±8 m du centre, joueurs qui apparaissent près du centre), seul 1-2
/// monstres réagissent tant qu'on reste près du point d'apparition — les
/// autres ne s'activent que si on s'aventure dans leur secteur.
const CHASER_DETECT_RANGE: f32 = 9.0;

/// Portée d'éveil (m) de l'archétype `Furtive` (GDD_MMORPG.md §5.4 : « éveil réduit
/// (< 9 m) mais vitesse accrue éveillée ») — plus courte que `CHASER_DETECT_RANGE` et,
/// contrairement à elle, appliquée **en toute circonstance** (pas seulement en réseau) :
/// c'est justement ce délai d'éveil court qui doit permettre au contre-jeu « l'Éclaireur
/// la déclenche de loin » d'exister aussi en solo.
const FURTIVE_DETECT_RANGE: f32 = 5.0;

/// Écart angulaire **signé le plus court** (radians, dans [-π, π]) de `cur` vers
/// `target` — jamais plus d'un demi-tour, quel que soit l'enroulement des angles.
fn shortest_angle(cur: f32, target: f32) -> f32 {
    let mut diff = (target - cur) % std::f32::consts::TAU;
    if diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    } else if diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }
    diff
}

/// Fait tourner `cur` (radians) vers `target` par le plus court chemin, en amorti
/// **exponentiel** : chaque seconde comble une fraction `1 - e^(-rate)` de l'écart
/// restant — rapide au départ, doux à l'approche, sans jamais « claquer » sur la
/// cible (contrairement à l'ancienne rotation à vitesse constante + arrêt sec).
/// La forme `1 - e^(-rate·dt)` rend le taux indépendant du framerate (deux pas de
/// dt/2 = un pas de dt). Utilisé pour l'orientation du joueur local (cf.
/// `advance_play`), purement cinématique — n'implique jamais le corps rigide :
/// forcer une rotation sur un corps en contact avec le décor déstabilisait le
/// solveur de contacts de rapier (vibrations).
pub(super) fn rotate_towards_smooth(cur: f32, target: f32, rate: f32, dt: f32) -> f32 {
    cur + shortest_angle(cur, target) * (1.0 - (-rate * dt).exp())
}

/// Convertit une entrée joystick/clavier `(mx, my)` (axes de l'écran : droite/haut)
/// en direction **monde** `(x, z)`, relative à l'orientation `yaw` de la caméra —
/// façon caméra de suivi à la Zelda : pousser le joystick « en haut » éloigne le
/// personnage de la caméra, quelle que soit sa rotation actuelle, plutôt que de
/// toujours avancer selon les mêmes axes du monde (ce qui rendait le déplacement
/// incohérent dès que la caméra pivotait). `yaw = 0`
/// laisse `(mx, my)` inchangé (compatible avec le comportement d'origine).
///
/// Appelée à la fois par `sim_step` (prédiction locale du joueur, caméra de *ce*
/// client) et par `network_client::poll_network` (valeur envoyée au serveur) :
/// le serveur, headless et sans caméra, reçoit ainsi directement une direction
/// monde déjà correcte — il n'a pas besoin de connaître l'orientation de qui que
/// ce soit.
pub(super) fn camera_relative_move(mx: f32, my: f32, yaw: f32) -> (f32, f32) {
    let (sin_y, cos_y) = yaw.sin_cos();
    let wx = mx * cos_y - my * sin_y;
    let wz = -mx * sin_y - my * cos_y;
    (wx, wz)
}

/// Décalage de recul caméra pour la frame courante (Sprint 1, `sprint10audit.md`) :
/// jitter dans le plan écran (axes droite/haut de la caméra), amplitude
/// proportionnelle à `camera_shake` et oscillante via `self.time` — pas de RNG à
/// état pour rester déterministe/rejouable comme le reste de la simulation
/// (cf. `deterministic_roll` dans `health.rs`). Ne mute jamais `self.camera` :
/// seul `Renderer::render` (via `OrbitCamera::view_proj_shaken`) l'applique,
/// la caméra de simulation (suivi joueur, IA, réseau) reste intacte.
impl AppState {
    pub(crate) fn camera_shake_offset(&self) -> Vec3 {
        // PHASE I Sprint 1 (accessibilité, §16.6) : `Settings::reduce_shake`,
        // copié dans `self.reduce_shake` (même patron que `music_volume`) —
        // coupe le recul caméra sans toucher `camera_shake`, dont d'autres
        // systèmes (flash de dégâts) restent indépendants.
        if self.reduce_shake || self.camera_shake <= 0.0 {
            return Vec3::ZERO;
        }
        let forward = (self.camera.target - self.camera.eye()).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward);
        // Amplitude (mètres) au pic (camera_shake = 1) : assez sensible pour se
        // sentir sans désorienter la visée.
        const AMPLITUDE: f32 = 0.12;
        let t = self.time;
        let jx = (t * 47.0).sin() + (t * 71.0).sin() * 0.5;
        let jy = (t * 59.0).sin() + (t * 83.0).sin() * 0.5;
        (right * jx + up * jy) * AMPLITUDE * self.camera_shake
    }
}

/// Rayon mort du joystick virtuel (0..1) : en-deçà, l'entrée est ramenée à zéro plutôt
/// que transmise brute. Un joystick tactile/analogique imparfait ne revient pas
/// toujours exactement au centre au repos — sans seuil, ce résidu ferait dériver
/// lentement le personnage même sans action du joueur.
pub(super) const JOYSTICK_DEADZONE: f32 = 0.15;

/// Écrase `v` à zéro si sa longueur est sous `threshold` (rayon mort), puis
/// **remappe** la plage utile `[threshold, 1]` vers `[0, 1]` (même direction).
/// Sans ce remappage, l'entrée sautait d'un coup de 0 à `threshold` en sortant du
/// rayon mort — un « cran » perceptible au joystick, l'inverse d'un départ
/// progressif. Avec lui, la vitesse démarre à zéro exactement au bord du rayon
/// mort et monte continûment jusqu'au plein débattement.
pub(super) fn apply_deadzone(v: (f32, f32), threshold: f32) -> (f32, f32) {
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    if len < threshold {
        return (0.0, 0.0);
    }
    let scaled = ((len - threshold) / (1.0 - threshold)).min(1.0);
    (v.0 / len * scaled, v.1 / len * scaled)
}

/// Déplacement (m) au-delà duquel un écart entre deux pas de simulation consécutifs
/// est traité comme une **téléportation** par l'interpolation de rendu (claqué sur la
/// pose finale au lieu d'être interpolé, cf. `blend_render_poses`). 0,5 m en 1/60 s
/// = 30 m/s : bien au-dessus de tout mouvement légitime du jeu (déplacement ≤ ~8 m/s,
/// recul compris), bien en dessous d'un vrai respawn/effet téléporté (plusieurs mètres).
const TELEPORT_SNAP_PER_STEP: f32 = 0.5;

/// `true` si le transform est resté (à un epsilon de f32 près) sur la pose donnée —
/// sert à `restore_sim_poses` pour détecter qu'une écriture externe a eu lieu depuis
/// le dernier mélange de rendu. Comparaison à epsilon plutôt qu'exacte : par valeur
/// écrite puis relue, l'égalité bit à bit tiendrait, mais un epsilon protège des
/// copies intermédiaires éventuelles sans risquer de faux « externe ».
fn pose_matches(t: &crate::scene::Transform, (p, r, s): (Vec3, Quat, Vec3)) -> bool {
    (t.position - p).length_squared() < 1e-10
        && (t.scale - s).length_squared() < 1e-10
        && t.rotation.dot(r).abs() > 1.0 - 1e-6
}

pub(super) fn clamp_move_vector(mx: f32, my: f32) -> (f32, f32) {
    let len_sq = mx * mx + my * my;
    if len_sq > 1.0 {
        let len = len_sq.sqrt();
        (mx / len, my / len)
    } else {
        (mx, my)
    }
}

/// Cadence à pas fixe : ajoute le temps de la frame à l'accumulateur (borné contre la
/// « spirale de la mort »), puis renvoie le nombre de sous-pas de `fixed_dt` à exécuter
/// et l'accumulateur restant. Au-delà de `max` sous-pas, le reliquat est jeté (pas de
/// retard accumulé sur une machine trop lente).
pub(super) fn fixed_substeps(
    accumulator: f32,
    frame_dt: f32,
    fixed_dt: f32,
    max: u32,
) -> (u32, f32) {
    let mut acc = accumulator + frame_dt.min(0.25);
    let mut steps = 0;
    while acc >= fixed_dt && steps < max {
        acc -= fixed_dt;
        steps += 1;
    }
    if steps == max {
        acc = 0.0;
    }
    (steps, acc)
}

/// Une créature `attackable` ne doit sauter son script local que si le serveur
/// diffuse *réellement* ses positions récemment : sans ça, une room jointe sans
/// succès (gabarit introuvable côté serveur) ou une désynchronisation d'index de
/// scène ne délivre jamais de `Snapshot` la concernant, et elle resterait figée
/// pour toujours (aucun autre filet ne rétablit la simulation locale).
pub(super) fn creature_is_server_synced(
    last_snapshot: Option<Instant>,
    now: Instant,
    timeout: std::time::Duration,
) -> bool {
    last_snapshot.is_some_and(|t| now.duration_since(t) < timeout)
}

impl AppState {
    /// Bilan de perf périodique (audit du 16 juillet 2026) : toutes les
    /// `PERF_WINDOW` en mode Play actif, logue en `info` le FPS lissé et la
    /// **pire** frame de la fenêtre — c'est elle qui fait sentir les à-coups,
    /// pas la moyenne. Diagnostic lisible dans les logs d'un build joueur
    /// déployé (VPS, testeurs) sans ouvrir le panneau Profiler de l'éditeur.
    /// Les `dt` aberrants (> 0,5 s : throttle au repos, mise en veille) sont
    /// ignorés comme pour le FPS lissé ci-dessous.
    fn log_perf_window(&mut self, now: Instant, dt: f32) {
        const PERF_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);
        if !self.playing || self.paused {
            // Hors Play la boucle throttle volontairement : une fenêtre en cours
            // mélangerait des frames throttlées — on repart de zéro.
            self.perf_window_start = now;
            self.perf_window_worst_dt = 0.0;
            return;
        }
        if dt > 1e-4 && dt < 0.5 {
            self.perf_window_worst_dt = self.perf_window_worst_dt.max(dt);
        }
        if now.duration_since(self.perf_window_start) >= PERF_WINDOW {
            if self.perf_window_worst_dt > 0.0 {
                log::info!(
                    "Perf : {:.0} FPS lissés, pire frame {:.1} ms (pire sim {:.1} ms) \
                     sur les 10 dernières s",
                    self.fps,
                    self.perf_window_worst_dt * 1000.0,
                    self.perf_window_worst_sim * 1000.0
                );
            }
            self.perf_window_start = now;
            self.perf_window_worst_dt = 0.0;
            self.perf_window_worst_sim = 0.0;
        }
    }

    /// Enregistre la durée d'`advance_play` de la frame courante pour le bilan de
    /// perf (cf. `log_perf_window`) — appelé par `Renderer::render`, seul endroit
    /// qui voit la frame entière.
    pub fn note_sim_duration(&mut self, d: std::time::Duration) {
        self.perf_window_worst_sim = self.perf_window_worst_sim.max(d.as_secs_f32());
    }

    /// Avance la simulation d'exactement `n` pas fixes de 1/60 s, immédiatement
    /// et sans dépendre de l'horloge réelle (contrairement à `advance_play`, qui
    /// mesure `dt` entre deux frames) — pas-à-pas déterministe du pont de
    /// pilotage (`crate::pilot`) et de la console (`step <n>`). Ne fait rien
    /// hors Play : la physique n'est construite qu'à l'entrée en Play (cf. les
    /// transitions dans `advance_play`), un pas hors Play muterait la scène
    /// d'édition sans snapshot de restauration. Renvoie `true` si les pas ont
    /// été exécutés.
    pub fn advance_steps(&mut self, n: u32) -> bool {
        if !self.playing {
            return false;
        }
        // Front d'entrée en Play pas encore traité (il vit dans `advance_play`,
        // porté par la boucle de rendu — qui peut être au ralenti, fenêtre
        // masquée/occultée) : le déclencher d'abord, sinon les pas simuleraient
        // sans monde physique (`self.physics` encore `None`) — aucun objet
        // pilotable ne bougerait (constaté à l'audit du 19 juillet 2026).
        if !self.was_playing {
            self.advance_play();
        }
        for _ in 0..n {
            self.sim_step(1.0 / 60.0);
        }
        true
    }

    /// En mode Play : scripts Lua + simulation physique (delta-time).
    /// Au démarrage de Play, capture l'état ; à l'arrêt, le restaure.
    pub fn advance_play(&mut self) {
        // chargements asynchrones (imports glTF, sons décodés, script IA) prêts cette frame
        self.poll_imports();
        self.poll_ai();
        self.poll_network();
        self.audio.update();

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        // FPS lissé (EMA) ; ignore les dt aberrants (première frame, throttle au repos).
        if dt > 1e-4 && dt < 0.5 {
            let inst = 1.0 / dt;
            self.fps = if self.fps == 0.0 {
                inst
            } else {
                self.fps * 0.9 + inst * 0.1
            };
        }
        self.log_perf_window(now, dt);

        // transitions Edit <-> Play
        if self.playing && !self.was_playing {
            self.play_snapshot = self.scene.objects.clone();
            // Manche 1 révélée, suivantes masquées, *avant* de construire la physique
            // (cf. `init_waves` : les monstres masqués n'ont pas de corps rigide).
            self.init_waves();
            self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
            // sons en autoplay (gain atténué par la distance à la caméra, panning
            // stéréo caméra→source si spatialisé — Sprint 104) : lus **en flux**
            // (`play_music_streaming_gain`, `StreamingSoundData`) plutôt que
            // décodés entièrement en mémoire — une musique/ambiance longue ne
            // provoque plus de pic mémoire au démarrage du mode Play.
            let listener = self.camera.target;
            let eye = self.camera.eye();
            let clips: Vec<(String, f32, f32)> = self
                .scene
                .objects
                .iter()
                .filter_map(|o| {
                    let a = o.audio.as_ref()?;
                    if !a.autoplay || a.clip.is_empty() {
                        return None;
                    }
                    let (gain, panning) = if a.spatial {
                        let dist = (o.transform.position - listener).length();
                        let gain = (1.0 - dist / 20.0).clamp(0.0, 1.0);
                        let panning = crate::runtime::audio::camera_panning(
                            eye,
                            listener,
                            o.transform.position,
                        );
                        (gain, panning)
                    } else {
                        (1.0, 0.0)
                    };
                    // `a.gain` (Sprint 126) : normalisation de loudness calculée à
                    // l'import, composée avec l'atténuation spatiale plutôt que
                    // l'écraser — les deux sont des facteurs multiplicatifs indépendants.
                    Some((a.clip.clone(), gain * a.gain, panning))
                })
                .collect();
            for (c, gain, panning) in clips {
                self.audio.play_music_streaming_gain(&c, gain, panning);
            }
            // Caméra de suivi : se cale d'emblée sur le joueur + adopte un bon angle de
            // jeu 3ᵉ personne (plongée douce + recul confortable) si aucune caméra de jeu
            // n'est définie.
            if self.scene.camera_follow
                && let Some(p) = self.player_position()
            {
                self.camera.target = p + Vec3::new(0.0, PLAYER_CAMERA_HEIGHT_OFFSET, 0.0);
                if self.scene.game_camera.is_none() {
                    self.camera.pitch = DEFAULT_CHASE_PITCH;
                    self.camera.distance = DEFAULT_CHASE_DISTANCE;
                }
            }
            // Caméra de jeu : applique le point de vue défini pour la scène.
            if let Some(gc) = self.scene.game_camera {
                self.camera.yaw = gc.yaw;
                self.camera.pitch = gc.pitch;
                self.camera.distance = gc.distance;
                if !self.scene.camera_follow {
                    self.camera.target = Vec3::from_array(gc.target);
                }
            }
        } else if !self.playing && self.was_playing {
            self.scene.objects = self.play_snapshot.clone();
            // cf. AUDIT_MMORPG.md §4.2 : même raison qu'à `restart_game`.
            self.clear_network_players();
            self.clear_fireballs();
            self.clear_creature_shots();
            self.physics = None;
            self.paused = false;
            self.hud_health = None;
            self.damage_flash = 0.0;
            self.attack_flash = 0.0;
            self.attack_cooldown_remaining = 0.0;
            self.attack_projectile = None;
            self.attack_charge = None;
            self.stagger.clear();
            // Poses d'interpolation de rendu périmées (la scène vient d'être restaurée
            // depuis le snapshot d'édition) : ne surtout pas les mélanger au retour en Play.
            self.sim_prev_poses.clear();
            self.sim_curr_poses.clear();
            self.sim_render_poses.clear();
            self.wave = 0;
            self.win_time = None;
            self.lost = false;
            self.clear_selection();
            self.audio.stop_all();
        }
        if self.playing && !self.was_playing {
            // Une sélection/gizmo laissé actif depuis l'éditeur resterait cliquable et
            // modifierait `transform` en concurrence directe avec la physique qui pilote
            // désormais le même objet — symétrique au `clear_selection()` de la sortie
            // de Play ci-dessus.
            self.clear_selection();
            // Démarrage de Play : repart d'un accumulateur vide (pas de rafale initiale)
            // et sans poses d'interpolation héritées d'une partie précédente.
            self.sim_accumulator = 0.0;
            self.sim_prev_poses.clear();
            self.sim_curr_poses.clear();
            self.sim_render_poses.clear();
            self.win_time = None;
            self.lost = false;
            self.score = 0;
            self.game_events.clear();
            self.trigger_prev.clear();
            self.furtive_awake.clear();
            self.lua_vars.clear();
            self.respawn_queue.clear();
            self.inventory.clear();
            self.time = 0.0;
            // Relit la qualité visée (modifiable dans le panneau Export sans redémarrer
            // l'app) : s'applique dès ce lancement de Play, pas seulement au build exporté.
            let cfg = crate::app::build_config::BuildConfig::load();
            self.render_quality = cfg.render_quality;
            self.bloom_enabled = cfg.bloom;
            // La caméra libre est un outil d'édition : la caméra de jeu prend le
            // relais en Play, cf. `toggle_fly_cam`.
            self.fly_cam = false;
        }
        self.was_playing = self.playing;

        // Caméra libre de l'éditeur (hors Play) : survole la carte aux flèches +
        // Espace/C, indépendamment de la simulation ci-dessous (gelée hors Play).
        if !self.playing && self.fly_cam {
            self.update_fly_cam(dt);
        }

        // Hors Play : les clips squelettaux continuent de tourner — prévisualisation
        // d'édition, un GLB riggé « vit » dans la vue sans lancer Play. Seule la
        // lecture avance (au dt de frame, pas besoin du pas fixe : aucun script ni
        // physique n'en dépend) ; `dt` est borné pour qu'un retour d'onglet/veille ne
        // fasse pas sauter les fondus en cours. En Play, c'est `sim_step` qui avance.
        if !self.playing {
            let adt = dt.min(0.1);
            for obj in self.scene.objects.iter_mut() {
                if let Some(anim) = obj.animation.as_mut() {
                    anim.time += adt * anim.speed;
                    if anim.blend < 1.0 {
                        anim.prev_time += adt * anim.speed;
                        anim.blend = (anim.blend
                            + adt / crate::scene::AnimationState::CROSSFADE_SECONDS)
                            .min(1.0);
                    }
                }
            }
        }

        // En pause : on reste en mode Play (snapshot conservé) mais on gèle la
        // simulation (ni scripts, ni physique, ni avance du temps) — sauf si un pas
        // unique a été demandé (cf. `request_step`) : dans ce cas on laisse
        // passer exactement cette frame pour avancer d'un pas fixe, puis on regèle.
        let step_once = self.paused && self.step_requested;
        self.step_requested = false;
        if !self.playing || (self.paused && !step_once) {
            self.sim_accumulator = 0.0;
            return;
        }

        // --- Simulation découplée du rendu : pas de temps FIXE ---
        // On accumule le temps réel écoulé et on simule par incréments fixes, quel que
        // soit le framerate → physique et scripts déterministes, indépendants du FPS.
        const FIXED_DT: f32 = 1.0 / 60.0;
        const MAX_SUBSTEPS: u32 = 5;
        // Time scale : n'affecte que le temps *consommé* par la simulation,
        // jamais `dt` lui-même (déjà utilisé ci-dessus pour le FPS affiché) ni `FIXED_DT`.
        // Pas unique en pause : force exactement un pas, indépendamment de `time_scale`
        // (`self.sim_accumulator` vaut 0 en entrant ici, cf. le early-return ci-dessus
        // qui le remet à 0 à chaque frame gelée → accumulateur + FIXED_DT = exactement
        // un pas dans `fixed_substeps`).
        let sim_dt = if step_once {
            FIXED_DT
        } else {
            dt * self.time_scale.max(0.0)
        };
        // Jeu figé une fois gagné ou perdu (l'écran de fin attend « Rejouer »).
        if !self.lost && self.win_time.is_none() {
            let (steps, acc) = fixed_substeps(self.sim_accumulator, sim_dt, FIXED_DT, MAX_SUBSTEPS);
            self.sim_accumulator = acc;
            // Avant de simuler, restaure l'état **exact** du dernier pas : les
            // transforms affichés contiennent la pose *mélangée* du rendu précédent
            // (cf. `blend_render_poses` ci-dessous), en retrait d'une fraction de pas
            // — simuler depuis cette pose lissée cumulerait une dérive (l'orientation
            // du joueur, notamment, est intégrée depuis `transform.rotation`).
            if steps > 0 {
                self.restore_sim_poses();
            }
            for _ in 0..steps {
                self.sim_step(FIXED_DT);
            }
            // --- Interpolation de rendu ---
            // La simulation avance par pas fixes de 1/60 s, mais les frames de rendu
            // ne s'alignent jamais exactement dessus (écran 120 Hz, gigue de frame…) :
            // afficher la dernière pose brute donne un mouvement saccadé (« judder »,
            // 0 pas simulé à une frame, 2 à la suivante). On affiche donc un mélange
            // prev→curr pondéré par le temps restant dans l'accumulateur — le rendu
            // retarde d'au plus un pas (≤ 16,7 ms), imperceptible, contre une
            // trajectoire parfaitement continue à l'écran.
            self.blend_render_poses(self.sim_accumulator / FIXED_DT);

            // Ramassage par contact : le joueur récupère les pièces qu'il traverse.
            // Score +1 par pièce ; les pièces bonus (respawn_delay>0) réapparaissent.
            if let Some(p) = self.player_position() {
                let now = self.time;
                let hit = self.scene.collect_at(p, 0.7);
                if !hit.is_empty() {
                    self.add_score(hit.len() as u32);
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Pickup);
                    for i in hit {
                        let d = self.scene.objects[i].respawn_delay;
                        if d > 0.0 {
                            self.respawn_queue.push((i, now + d));
                        }
                    }
                }
            }
            // Ramassage d'arme par contact (cf. `WeaponPickup`, donjon roguelike) :
            // équipe le nouveau profil sur le joueur et score +1, comme une pièce —
            // mais **natif** (pas un script Lua, qui ne peut pas modifier `Controller`).
            if let Some(pi) = self.player_index() {
                let p = self.scene.objects[pi].transform.position;
                if let Some(w) = self.scene.weapon_pickup_at(p, 0.9) {
                    if let Some(ctrl) = self.scene.objects[pi].controller.as_mut() {
                        ctrl.attack_range = w.range;
                        ctrl.attack_cooldown = w.cooldown;
                        ctrl.attack_windup = w.windup;
                        ctrl.attack_mode = w.mode;
                    }
                    self.add_score(1);
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Pickup);
                    log::info!(
                        "Arme trouvée : « {} » équipée (portée {:.1} m, recharge {:.2} s, préparation {:.2} s)",
                        w.label,
                        w.range,
                        w.cooldown,
                        w.windup
                    );
                }
            }
            // Ramassage d'objets d'inventaire par contact (cf. `app::inventory`) :
            // potions, clés… rejoignent le sac au lieu d'équiper ou de scorer.
            self.update_item_pickups();
            self.update_attack(dt);
            self.update_network_attacks(dt);
            self.update_fireballs(dt);
            // Vie individualisée des joueurs réseau (contact monstre, régénération
            // hors combat) puis soin coopératif — après les dégâts de ce tick, pour
            // qu'un soin ne soit pas aussitôt annulé par un contact déjà résolu
            // (cf. GAMEDESIGN_EN_LIGNE.md §3.1/§3.6).
            self.update_network_health(dt);
            self.update_creature_bite(dt);
            self.update_network_heal(dt);
            self.update_network_revive(dt);
            // Réapparition des pièces bonus et ennemis dont le délai est écoulé.
            let now = self.time;
            self.process_respawns(now);
            // Défaite : le joueur a touché une zone mortelle (mort instantanée, ex. lave).
            if !self.lost
                && let Some(p) = self.player_position()
                && self.scene.deadly_at(p)
            {
                self.lost = true;
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
            }
            self.check_ring_outs();
            // Défaite : la vie (dégâts cumulés des ennemis via `damage()`) est tombée à 0.
            // Contrairement aux zones mortelles, les ennemis punissent par usure (dégâts
            // progressifs + régénération hors contact), plus indulgent qu'une mort au tap.
            if !self.lost
                && let Some(h) = self.hud_health
                && h <= 0.0
            {
                self.lost = true;
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
            }
            // Victoire : fige le chrono quand toutes les pièces-objectif sont ramassées.
            if self.win_time.is_none()
                && let Some((c, t)) = self.scene.collectibles()
                && c == t
            {
                self.win_time = Some(self.time);
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Win);
            }
            // Système de manches (cf. `Combat::wave`) : révèle la manche suivante une
            // fois la courante vidée, ou déclenche la victoire (selon `self.objective`,
            // cf. `update_round`, Phase C `sprint10audit.md`). N'a aucun effet si la
            // scène n'a pas de monstres à manches (self.wave == 0).
            self.update_round(dt);
        }

        // Position réseau du joueur local : appliquée *après* la physique (cf. sa
        // doc) pour ne pas être aussitôt écrasée par `sim_step`, qui recalculerait
        // sinon une position légèrement différente à partir de l'ancienne.
        self.apply_local_network_position();

        // Caméra qui suit le joueur — au niveau frame (lissage visuel), avec le dt réel.
        // Cible légèrement au-dessus du joueur (regarde le buste, voit plus loin devant).
        // Suspendu pendant qu'un outil de navigation caméra (🖐 Main, 🔄 Orbite, 🔍 Loupe)
        // est actif : sinon ce rattrapage écraserait chaque frame le pan/orbite/zoom que
        // `camera.pan`/`orbit`/`zoom_drag` viennent d'appliquer, et la caméra resterait
        // rivée au joueur malgré le glisser de souris.
        if self.scene.camera_follow
            && !self.gizmo_mode.is_nav()
            && let Some(p) = self.player_position()
        {
            // Forme exponentielle `1 - e^(-k·dt)` plutôt que `k·dt` borné : le taux de
            // rattrapage devient indépendant du framerate (deux frames à 120 Hz lissent
            // exactement comme une à 60 Hz), là où la forme linéaire sur-amortissait à
            // bas FPS et créait de micro-à-coups de caméra sous gigue de frame.
            let t = 1.0 - (-dt * 6.0).exp();
            self.camera.target = self
                .camera
                .target
                .lerp(p + Vec3::new(0.0, PLAYER_CAMERA_HEIGHT_OFFSET, 0.0), t);
            // Caméra qui pivote derrière l'orientation du joueur, **seulement** pour
            // un personnage équipé d'une arme à distance (`fire_button`, cf. le
            // réticule central de `editor::crosshair`) : sans ce suivi, le réticule
            // (toujours au centre de l'écran) pointe dans la direction de VUE de la
            // caméra, pas celle du TIR (`aim_yaw`, l'orientation du personnage) — les
            // deux divergent dès qu'on tourne en tank (A/D) sans faire pivoter la
            // caméra à la souris, qui n'existe pas au tactile. Repos des
            // autres démos (joystick, plateformes) intentionnellement inchangé : la
            // caméra libre indépendante du personnage y est voulue, pas un défaut.
            if self
                .player_object()
                .and_then(|o| o.controller.as_ref())
                .is_some_and(|c| !c.fire_button.is_empty())
                && let Some(player_yaw) = self
                    .player_object()
                    .map(|o| o.transform.rotation.to_euler(EulerRot::YXZ).0)
            {
                self.camera.yaw = rotate_towards_smooth(self.camera.yaw, player_yaw, 8.0, dt);
            }
            // Tangage caméra au stick droit de la manette (axe vertical) : stick
            // vers le haut = regarder vers le haut (pitch réduit — la caméra
            // descend vers l'horizon). Bornes serrées par rapport à l'orbite
            // libre de l'éditeur : la caméra de jeu ne passe ni sous le sol ni
            // au zénith. Sans manette, `gamepad_pitch` reste à 0 — aucun effet.
            let look = self.input_state.gamepad_pitch;
            if look != 0.0 {
                const GAMEPAD_PITCH_RATE: f32 = 1.6; // rad/s à plein débattement
                self.camera.pitch =
                    (self.camera.pitch - look * GAMEPAD_PITCH_RATE * dt).clamp(0.08, 1.35);
            }
        }
        // Décroissance du flash de dégâts (~0,4 s), au niveau frame comme la caméra.
        if self.damage_flash > 0.0 {
            self.damage_flash = (self.damage_flash - dt * 2.5).max(0.0);
        }
        // Décroissance du recul caméra (~0,25 s, plus rapide que le flash : une
        // secousse qui traîne gênerait la visée du joueur en combat rapproché).
        if self.camera_shake > 0.0 {
            self.camera_shake = (self.camera_shake - dt * 4.0).max(0.0);
        }
        // Décroissance de la bannière « allié à terre » (~1,3 s : assez pour se
        // lire, conforme à GDD §16.3 « les bannières d'événement durent < 2 s »).
        if self.ally_down_flash > 0.0 {
            self.ally_down_flash = (self.ally_down_flash - dt * 0.75).max(0.0);
        }
        // Décroissance de la bannière de vague (Phase H, Sprint 2, ~2,5 s : un
        // peu plus longue que `ally_down_flash`, moins urgente à lire).
        if self.wave_banner_flash > 0.0 {
            self.wave_banner_flash = (self.wave_banner_flash - dt * 0.4).max(0.0);
        }
        // Décroissance de l'effet d'attaque (~0,33 s) : rétrécit l'ancre `is_attack_fx`
        // jusqu'à disparition, puis la remasque pour ne pas polluer le prochain coup.
        if self.attack_flash > 0.0 {
            self.attack_flash = (self.attack_flash - dt * 3.0).max(0.0);
            if let Some(fx) = self.attack_fx_index()
                && let Some(o) = self.scene.objects.get_mut(fx)
            {
                if self.attack_flash <= 0.0 {
                    o.visible = false;
                } else {
                    o.transform.scale = Vec3::splat(0.25 + 0.95 * self.attack_flash);
                }
            }
        }
    }

    /// Fait réapparaître les objets de `respawn_queue` dont le délai est écoulé
    /// (pièces bonus, ennemis à `respawn_delay > 0`). Un ennemi qui revient est
    /// remis à ses PV d'origine (`Combat::max_hp`, capturés au premier coup reçu,
    /// cf. `Scene::damage_attackable_by`) : sans cette restauration, un ennemi à
    /// plusieurs PV réapparaîtrait avec hp=0 — « déjà vaincu », re-masqué au
    /// premier coup suivant sans jamais encaisser sa barre de vie.
    fn process_respawns(&mut self, now: f32) {
        self.respawn_queue.retain(|&(i, at)| {
            if now >= at {
                if let Some(o) = self.scene.objects.get_mut(i) {
                    o.visible = true;
                    if let Some(c) = &mut o.combat
                        && c.max_hp > 0
                    {
                        c.hp = c.max_hp;
                    }
                }
                false
            } else {
                true
            }
        });
    }

    /// Déplace la caméra d'orbite comme une caméra libre (« vol libre »/noclip) :
    /// flèches = avance/recul/strafe relatifs à la vue (même mapping que
    /// `camera_relative_move`, réutilisé pour le joueur), Espace/C = monte/descend.
    /// Translate `target` (donc `eye()`, qui en dérive à distance/angle constants) :
    /// aucune collision, aucune limite — but assumé (« aller où on veut », survoler
    /// toute la carte pour repérer un décor sans passer par Play). Cf. `fly_cam`.
    fn update_fly_cam(&mut self, dt: f32) {
        const FLY_SPEED: f32 = 12.0;
        let (mx, my) = clamp_move_vector(self.input_state.key_move.0, self.input_state.key_move.1);
        let (wx, wz) = camera_relative_move(mx, my, self.camera.yaw);
        let wy = self.input_state.fly_vertical.clamp(-1.0, 1.0);
        self.camera.target += Vec3::new(wx, wy, wz) * FLY_SPEED * dt;
    }

    /// Un pas de simulation à **dt fixe** : scripts Lua, actions au tap, pilotage des
    /// objets pilotables et pas de physique. Appelé 0..N fois par frame (cf. `advance_play`).
    pub(super) fn sim_step(&mut self, dt: f32) {
        // 1. scripts
        self.time += dt;
        let time = self.time;
        // Avance la lecture des clips d'animation squelettale : indépendant
        // des scripts/tap actions ci-dessous — un objet skinné anime, script ou pas.
        // Le bouclage lui-même vit dans `Clip::sample_joint`, pas ici.
        // Marqueurs temporels : accumulés ici, délivrés aux scripts **ce
        // même tick** (fusionnés dans `events_in` plus bas) — contrairement aux
        // événements de gameplay (`game_events`) qui attendent le tick suivant pour
        // rester indépendants de l'ordre des scripts, cette boucle-ci s'exécute
        // entièrement avant qu'aucun script ne tourne : aucune ambiguïté d'ordre à éviter.
        let mut anim_notify_events: Vec<String> = Vec::new();
        let scene = &mut self.scene;
        for obj in scene.objects.iter_mut() {
            if let Some(anim) = obj.animation.as_mut() {
                let prev_time = anim.time;
                anim.time += dt * anim.speed;
                // Fondu enchaîné : le clip quitté continue de jouer pendant
                // la transition (ne se fige pas), et `blend` avance vers 1.0 sur
                // `CROSSFADE_SECONDS` — au-delà, plus rien à faire (transition terminée,
                // `prev_clip` ignoré par le rendu tant que `blend == 1.0`).
                if anim.blend < 1.0 {
                    anim.prev_time += dt * anim.speed;
                    anim.blend = (anim.blend
                        + dt / crate::scene::AnimationState::CROSSFADE_SECONDS)
                        .min(1.0);
                }
                if let crate::scene::MeshKind::Imported(mesh_idx) = obj.mesh
                    && let Some(imported) = scene.imported.get(mesh_idx as usize)
                    && let Some(markers) = imported.notifies.get(&anim.clip)
                    && let Some(clip) = imported.clips.iter().find(|c| c.name == anim.clip)
                {
                    for name in
                        crate::scene::notifies_crossed(markers, prev_time, anim.time, clip.duration)
                    {
                        anim_notify_events.push(format!("anim:{name}"));
                    }
                }
            }
        }
        // Zones de déclenchement : objets `trigger` visibles dont l'AABB monde touche
        // celui du joueur. Test d'*intersection* de volumes (et non « centre du joueur
        // dans la zone ») : quand la zone est un ennemi doté d'un corps physique, les
        // colliders empêchent le centre du joueur d'entrer dans son AABB — le contact
        // doit suffire pour qu'un monstre au corps-à-corps puisse mordre. `visible`
        // exclut les ennemis vaincus (masqués par l'attaque, cf. `Scene::attack_at`) :
        // un ennemi caché ne doit plus pouvoir infliger de dégâts.
        let triggered: std::collections::HashSet<usize> = match self.player_index() {
            Some(pi) => {
                let player = &self.scene.objects[pi];
                self.scene
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(i, o)| {
                        *i != pi
                            && o.trigger
                            && o.visible
                            && self.scene.world_aabb_intersects(o, player)
                    })
                    .map(|(i, _)| i)
                    .collect()
            }
            None => std::collections::HashSet::new(),
        };
        // Sortie de zone : objets `trigger` qui étaient en contact au tick
        // précédent (`trigger_prev`) et ne le sont plus ce tick-ci — exposé aux scripts
        // via `obj.exited`, symétrique de `obj.triggered`. Calculé avant de remplacer
        // `trigger_prev` par `triggered` (sinon la différence serait toujours vide).
        let exited: std::collections::HashSet<usize> =
            self.trigger_prev.difference(&triggered).copied().collect();
        self.trigger_prev = triggered.clone();
        let mut vibrations: Vec<f32> = Vec::new();
        // Sprint 121 : mélanges de réverbération demandés par les scripts ce tick
        // (`reverb(mix)`, typiquement depuis une zone `trigger`) — le dernier appel
        // l'emporte, appliqué après la boucle comme les vibrations.
        let mut reverb_requests: Vec<f32> = Vec::new();
        // Événements de gameplay : ceux émis au tick précédent (scripts ou
        // moteur) sont délivrés à tous les scripts de ce tick, puis jetés ; les `emit()`
        // de ce tick s'accumulent dans `events_out` et seront délivrés au suivant.
        let mut events_in = std::mem::take(&mut self.game_events);
        // Marqueurs d'animation franchis plus haut, livrés ce même tick.
        events_in.extend(anim_notify_events);
        let mut events_out: Vec<String> = Vec::new();
        // Régénération passive de la vie (hors contact) : appliquée avant les scripts pour
        // que les appels `damage()` de cette frame s'appliquent après, sans s'annuler.
        const HEALTH_REGEN_PER_S: f32 = 0.25;
        let mut health = self
            .hud_health
            .map(|h| (h + HEALTH_REGEN_PER_S * dt).min(1.0));
        // Positions de départ (snapshot d'entrée en Play) pour l'action « Respawn ».
        let start_pos: Vec<Vec3> = self
            .play_snapshot
            .iter()
            .map(|o| o.transform.position)
            .collect();
        // `find_tag` : instantané pris **avant** la boucle, pas de vue
        // vivante sur `scene.objects` (déjà emprunté mutable ci-dessous). Un objet
        // masqué ce tick (destroy) ou pas encore spawné n'y figure pas.
        let tagged: Vec<(String, Vec3)> = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible && !o.tag.is_empty())
            .map(|o| (o.tag.clone(), o.transform.position))
            .collect();
        // `spawn()`/`obj:destroy()` : accumulés pendant la boucle des
        // scripts, appliqués après — jamais pendant, `scene.objects` est emprunté
        // mutable par l'itération ci-dessous.
        let mut spawn_requests: Vec<(String, Vec3)> = Vec::new();
        // Calculé une fois : `self.scene.objects` est emprunté mutable par
        // l'itération ci-dessous, `is_online_client()` (méthode sur `&self` entier)
        // n'y serait pas appelable.
        let online_client = self.is_online_client();
        // Filet de secours : une créature `attackable` ne doit sauter son script
        // local que si le serveur diffuse *réellement* ses positions pour elle.
        // Si la room n'a jamais été rejointe avec succès (gabarit introuvable côté
        // serveur, cf. `spawn_network_player`) ou si la scène serveur diverge
        // (index qui ne correspond à rien côté client), aucun `Snapshot` ne la
        // couvre jamais et elle resterait figée pour toujours sans ce filet — de
        // même si le serveur cesse de diffuser en cours de partie (déconnexion
        // silencieuse, redémarrage). Nom distinct de la variable `now` réutilisée
        // (ombrée) plus haut dans cette fonction pour `self.time`.
        const CREATURE_SNAPSHOT_TIMEOUT: std::time::Duration =
            std::time::Duration::from_millis(2500);
        let net_check_now = Instant::now();
        for (idx, obj) in self.scene.objects.iter_mut().enumerate() {
            let just_tapped = self.tapped_obj == Some(idx);
            // Vibration Feedback : retour haptique quand l'objet est tapé.
            if obj.vibrate_on_tap > 0 && just_tapped {
                vibrations.push(obj.vibrate_on_tap as f32);
            }
            // Action au tap sans script (couleur / masquer / grandir / respawn).
            if just_tapped {
                let start = start_pos
                    .get(idx)
                    .copied()
                    .unwrap_or(obj.transform.position);
                crate::scene::apply_tap_action(obj, start, time);
            }
            // Game feel : les collectibles encore visibles tournent sur eux-mêmes.
            crate::scene::animate_collectible(obj, time);
            if obj.script.trim().is_empty() {
                continue;
            }
            // Créature synchronisée par le serveur (`Combat::attackable`, cf.
            // `AppState::network_snapshot`/`scene::demos::MMORPG_CREATURES`) : un
            // client connecté ne doit pas dupliquer sa simulation localement (sa
            // patrouille/morsure tourne réellement côté serveur, cf. `is_online_
            // client`), seulement se fier aux `EntityDelta` reçus. En solo et côté
            // serveur (jamais de `net_client`), rien ne change.
            if online_client
                && obj.controller.is_none()
                && obj.combat.as_ref().is_some_and(|c| c.attackable)
                && creature_is_server_synced(
                    self.net_creature_last_snapshot.get(&idx).copied(),
                    net_check_now,
                    CREATURE_SNAPSHOT_TIMEOUT,
                )
            {
                continue;
            }
            // `mlua`/`lua-src` ne ciblent pas `wasm32-unknown-unknown` (cf. Cargo.toml,
            // Sprint 114) : sur le web, les scripts tournent sur `scripting_web`
            // (backend `rilua`, Sprint 137) — même contrat que `scripting::run_script`
            // ci-dessous, cf. sa doc pour ce qui diffère en interne.
            #[cfg(target_arch = "wasm32")]
            {
                let key = scripting_web::script_key(&obj.script);
                let func = match self.script_cache_web.get(&key) {
                    Some(f) => *f,
                    None => match self.lua_web.load(&obj.script) {
                        Ok(f) => {
                            // Ancrage dans la table `registry` de `rilua` (racine GC) :
                            // sans ça, le cache Rust (`script_cache_web`) garde un
                            // handle invisible du GC, ramassé à la première collecte
                            // complète — cf. la doc d'`anchor_compiled_function`.
                            if let Err(e) =
                                scripting_web::anchor_compiled_function(&mut self.lua_web, key, f)
                            {
                                log::error!("Ancrage GC du script '{}' : {e}", obj.name);
                                continue;
                            }
                            self.script_cache_web.insert(key, f);
                            f
                        }
                        Err(e) => {
                            log::error!("Compilation du script '{}' : {e}", obj.name);
                            continue;
                        }
                    },
                };
                let tapped = self.tapped_obj == Some(idx);
                let mut destroy_requested = false;
                let mut spawns_this_obj: Vec<(String, Vec3)> = Vec::new();
                if let Err(e) = scripting_web::run_script_web(
                    &mut self.lua_web,
                    &func,
                    &mut obj.transform,
                    &mut obj.color,
                    &mut obj.animation,
                    dt,
                    time,
                    &self.input_state,
                    tapped,
                    triggered.contains(&idx),
                    &events_in,
                    &mut events_out,
                    &tagged,
                    &mut spawns_this_obj,
                    &mut destroy_requested,
                    &mut self.lua_vars,
                    &mut vibrations,
                    &mut health,
                    &mut self.debug_lines,
                    exited.contains(&idx),
                    self.physics.as_ref(),
                    &mut reverb_requests,
                ) {
                    log::error!("Script '{}' : {e}", obj.name);
                }
                if destroy_requested {
                    obj.visible = false;
                }
                spawn_requests.extend(spawns_this_obj);
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                // Récupère (ou compile une seule fois) le chunk associé à cette source.
                let key = scripting::script_key(&obj.script);
                let func = match self.script_cache.get(&key) {
                    Some(f) => f.clone(),
                    // Chunk nommé d'après l'objet : sans ça, mlua nomme le chunk
                    // d'après le call-site Rust (`src/app/simulation.rs:NNN`) et
                    // les erreurs Lua deviennent illisibles pour l'utilisateur
                    // (Phase C5, sprint.19matin.md) — avec, elles se lisent
                    // « script de « Nom » »:ligne: message ».
                    None => match self
                        .lua
                        .load(&obj.script)
                        .set_name(format!("script de « {} »", obj.name))
                        .into_function()
                    {
                        Ok(f) => {
                            self.script_cache.insert(key, f.clone());
                            f
                        }
                        Err(e) => {
                            log::error!("Compilation du script '{}' : {e}", obj.name);
                            continue;
                        }
                    },
                };
                let tapped = self.tapped_obj == Some(idx);
                let mut destroy_requested = false;
                let mut spawns_this_obj: Vec<(String, Vec3)> = Vec::new();
                if let Err(e) = scripting::run_script(
                    &self.lua,
                    &func,
                    &mut obj.transform,
                    &mut obj.color,
                    &mut obj.animation,
                    dt,
                    time,
                    &self.input_state,
                    tapped,
                    triggered.contains(&idx),
                    &events_in,
                    &mut events_out,
                    &tagged,
                    &mut spawns_this_obj,
                    &mut destroy_requested,
                    &mut self.lua_vars,
                    &mut vibrations,
                    &mut health,
                    &mut self.debug_lines,
                    exited.contains(&idx),
                    self.physics.as_ref(),
                    &mut reverb_requests,
                ) {
                    log::error!("Script '{}' : {e}", obj.name);
                }
                // `obj:destroy()` : suppression douce, cf. sa doc dans
                // `run_script` — jamais un retrait de `scene.objects`.
                if destroy_requested {
                    obj.visible = false;
                }
                spawn_requests.extend(spawns_this_obj);
            }
        }
        // Les événements émis pendant ce tick seront délivrés au suivant (cf. la doc de
        // `game_events` — le décalage rend l'ordre des scripts dans la boucle indifférent).
        self.game_events = events_out;
        // `spawn()` : appliqué maintenant que `scene.objects` n'est plus
        // emprunté — ajout en fin de tableau (jamais d'insertion/retrait ailleurs),
        // les indices existants (réseau, undo, IA) restent donc valides. Physique
        // reconstruite une seule fois si des objets ont réellement été ajoutés (coûte
        // cher, cf. le même garde-fou dans `spawn_network_player`).
        if !spawn_requests.is_empty() {
            for (prefab_ref, pos) in spawn_requests {
                let name = format!("Spawn {}", self.scene.objects.len());
                if let Some(obj) = crate::scene::Scene::instantiate_prefab(&prefab_ref, name, pos) {
                    self.scene.objects.push(obj);
                } else {
                    log::error!("spawn() : prefab introuvable ou invalide ({prefab_ref})");
                }
            }
            self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
        }
        // Détecte un coup encaissé (vie en baisse) pour le retour visuel/sonore (vignette
        // rouge + bip) : déclenché une fois par « coup », pas en continu tant que le
        // contact dure (sinon le son saturerait pendant qu'un ennemi colle au joueur).
        if let (Some(prev), Some(cur)) = (self.hud_health, health)
            && cur < prev - 1e-4
        {
            self.damage_flash = 1.0;
            self.camera_shake = 1.0;
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
        }
        self.hud_health = health;
        // Le tap n'est exposé qu'une frame.
        self.tapped_obj = None;
        // Retour haptique demandé par les scripts (natif sur mobile, log sur desktop).
        for ms in vibrations {
            crate::runtime::vibrate(ms);
        }
        // Réverbération demandée par les scripts ce tick (Sprint 121) — dernier
        // appel gagnant, transition douce (0,5 s) plutôt qu'un changement abrupt.
        if let Some(&mix) = reverb_requests.last() {
            self.audio.set_reverb_mix(mix, 0.5);
        }

        // Attaques à distance des créatures (cf. `creature_attack.rs`) : gèle
        // position/animation de celles en train de viser (annule le déplacement
        // que leur script de patrouille vient de calculer ci-dessus), et fait
        // voler/impacter leurs projectiles — doit tourner avant la physique
        // ci-dessous pour que les positions gelées soient celles réellement
        // vues par `Physics::resolve_scripted_moves`/le rendu ce tick.
        self.update_creature_ranged_attacks(dt, time);

        // 2. physique (écrase les poses des corps dynamiques)
        // Cibles de poursuite pour l'IA (`AiChaser`, cf. plus bas) : en solo, le
        // seul joueur local ; en réseau, **chaque** joueur réseau **vivant**
        // (GAMEDESIGN_EN_LIGNE.md §3.2 — avant ce correctif, un monstre ne
        // poursuivait jamais que le premier joueur à avoir rejoint, `self.
        // player_position()` désignant sur le serveur headless le premier objet
        // visible piloté trouvé, donc le joueur 1, jamais le 2e+ quelle que soit
        // sa proximité). `player_position()` reste utilisé tel quel en solo (pas
        // de joueurs réseau) : aucun changement de comportement pour ce cas.
        // Mode Escorte (Sprint 7, `sprint10audit.md`) : « les créatures ciblent le
        // convoi en priorité » (GDD §4) — implémenté comme cible **exclusive** tant
        // qu'il est vivant, plutôt qu'une simple entrée de plus dans la liste (où le
        // choix « au plus proche », ci-dessous, ne le préférerait aux joueurs que
        // par hasard de distance). Convoi détruit ou scène sans convoi ⇒ retombe sur
        // les joueurs, pour ne pas geler les chasseurs sans cible.
        let convoy_target = (self.objective == crate::app::multiplayer::RoundObjective::Escorte)
            .then(|| {
                self.scene
                    .objects
                    .iter()
                    .find(|o| o.convoy.is_some() && o.visible)
                    .map(|o| o.transform.position)
            })
            .flatten();
        let candidate_targets: Vec<Vec3> = if let Some(convoy) = convoy_target {
            vec![convoy]
        } else if self.network_players.is_empty() {
            self.player_position().into_iter().collect()
        } else {
            self.network_players
                .iter()
                .filter(|(id, _)| self.network_health.get(id).copied().unwrap_or(1.0) > 0.0)
                .filter_map(|(_, &idx)| self.scene.objects.get(idx))
                .filter(|o| o.visible)
                .map(|o| o.transform.position)
                .collect()
        };
        if let Some(phys) = &mut self.physics {
            // Pilotage des objets « pilotables » : vitesse horizontale (joystick + clavier
            // + gyro) et saut (bouton tactile ou Espace). Appliqué avant le pas de simulation.
            let inp = &self.input_state;
            // Mouvement combiné joystick/croix directionnelle + clavier (flèches/WASD),
            // puis tourné selon la caméra (cf. `camera_relative_move`) : « en haut »
            // sur le joystick éloigne le personnage de la caméra, comme dans un jeu
            // à la Zelda, quelle que soit sa rotation actuelle.
            let joy = apply_deadzone(inp.joy, JOYSTICK_DEADZONE);
            let (raw_mx, raw_my) =
                clamp_move_vector(joy.0 + inp.key_move.0, joy.1 + inp.key_move.1);
            let (mx, my) = camera_relative_move(raw_mx, raw_my, self.camera.yaw);
            let (tilt, space) = (inp.tilt, inp.jump);
            let (key_turn, key_thrust) = (inp.turn(), inp.thrust());
            let mut any_jump = false;
            // Objets pilotés par un joueur réseau (cf. `multiplayer.rs`) :
            // chacun a son propre `NetworkInput`, distinct de `self.input_state`
            // (qui ne pilote que l'objet « joueur local », clavier/tactile/gyro de
            // cette instance — ex. l'éditeur desktop, ou un client sans réseau).
            // Un joueur vaincu (0 PV, GAMEDESIGN_EN_LIGNE.md §3.1) est exclu de
            // cette table : `net_input` devient `None` pour son objet, qui
            // retombe alors sur la branche locale ci-dessous (`inp.state`) — sans
            // effet indésirable sur un serveur headless, dont l'entrée locale
            // reste toujours neutre (aucun joueur ne pilote le serveur lui-même).
            // Spectateur immobile jusqu'à la fin de la manche, comme voulu.
            let network_by_index: HashMap<usize, multiplayer::NetworkInput> = self
                .network_players
                .iter()
                .filter(|(id, _)| self.network_health.get(id).copied().unwrap_or(1.0) > 0.0)
                .filter_map(|(id, &idx)| self.network_inputs.get(id).map(|inp| (idx, *inp)))
                .collect();
            // Orientation du joueur local : calculée ici puis appliquée **après**
            // `phys.step()` ci-dessous, directement sur `transform.rotation` — jamais
            // sur le corps rigide (cf. `set_position`/réconciliation réseau, même
            // principe). Un corps *dynamique* en contact avec le décor (mur, pilier)
            // dont on impose la rotation à chaque frame via `RigidBody::set_rotation`
            // déstabilisait le solveur de contacts de rapier — vibrations visibles
            // dès qu'on combinait beaucoup de rotation et de déplacement en même
            // temps. Inutile physiquement de toute façon : le collider est une capsule,
            // parfaitement symétrique autour de l'axe Y, donc une rotation de lacet
            // ne change jamais sa géométrie de collision.
            let mut player_facing: Vec<(usize, f32)> = Vec::new();
            for (idx, obj) in self.scene.objects.iter().enumerate() {
                let Some(ctrl) = &obj.controller else {
                    continue;
                };
                if !ctrl.input && !ctrl.gyro {
                    continue;
                }
                let net_input = network_by_index.get(&idx);
                let (mx, my, space) = match net_input {
                    Some(n) => (n.move_x.clamp(-1.0, 1.0), n.move_y.clamp(-1.0, 1.0), n.jump),
                    None => (mx, my, space),
                };
                let mut vx = 0.0;
                let mut vz = 0.0;
                if ctrl.input {
                    vx += mx * ctrl.move_speed;
                    if ctrl.auto_run_speed > 0.0 {
                        // Course automatique (endless runner) : avance en continu en +Z ;
                        // l'entrée verticale du joystick ne fait rien (seul X = voie compte).
                        vz += ctrl.auto_run_speed;
                    } else {
                        vz += -my * ctrl.move_speed;
                    }
                }
                if ctrl.gyro && net_input.is_none() {
                    vx += tilt.0 * ctrl.move_speed;
                    vz += -tilt.1 * ctrl.move_speed;
                }
                // Avance/recul « tank » (W/S clavier) : le long de l'orientation
                // *actuelle* du personnage plutôt que de la caméra, contrairement au
                // reste du déplacement. `-sin(yaw)`/`-cos(yaw)`
                // = même formule que l'inverse de `camera_relative_move` (yaw=0 ⇒ avant
                // = -Z, cf. `Physics::face_direction`).
                if ctrl.input && net_input.is_none() && key_thrust != 0.0 {
                    let yaw = obj.transform.rotation.to_euler(EulerRot::YXZ).0;
                    vx += key_thrust * ctrl.move_speed * -yaw.sin();
                    vz += key_thrust * ctrl.move_speed * -yaw.cos();
                }
                // Saut : bouton tactile nommé (joueur local), ou Espace au clavier
                // (joueur local), ou demandé par l'`Input` réseau de ce joueur.
                let jump = (!ctrl.jump_button.is_empty()
                    && self.input_state.buttons.contains(&ctrl.jump_button))
                    || (space && ctrl.input);
                let jump_speed = (2.0 * 9.81 * ctrl.jump_height.max(0.0)).sqrt();
                any_jump |= phys.control(idx, vx, vz, jump, jump_speed, ctrl.acceleration, dt);
                // Oriente le personnage — seulement pour le joueur *local* : les autres
                // joueurs réseau reçoivent déjà leur orientation du serveur (cf.
                // `network_client::apply_local_network_position`), l'écraser ici avec
                // notre propre calcul créerait un conflit d'autorité.
                // Joueur réseau : son orientation vient de l'`aim_yaw` de son
                // `Input` — celle que **son** client prédit et affiche, pas un
                // recalcul local qui entrerait en conflit avec elle.
                if ctrl.input
                    && let Some(n) = net_input
                {
                    player_facing.push((idx, n.aim_yaw));
                }
                if ctrl.input && net_input.is_none() {
                    let cur_yaw = obj.transform.rotation.to_euler(EulerRot::YXZ).0;
                    let new_yaw = if key_turn != 0.0 {
                        // Rotation « tank » manuelle (A/D) : prioritaire sur la rotation
                        // automatique vers la direction de déplacement, qui se
                        // battrait sinon contre l'intention explicite du joueur.
                        // Vitesse dédiée (`MANUAL_TURN_SPEED`), pas `turn_speed` : ce
                        // dernier (10 rad/s ≈ 570°/s) est calibré pour *rattraper* une
                        // direction, pas pour être **tenu** — tenu, il rend le pilotage
                        // impossible à doser (un quart de tour par frame de retard).
                        cur_yaw + key_turn * MANUAL_TURN_SPEED * dt
                    } else if key_thrust != 0.0 {
                        // W/S « tank » : le personnage garde son orientation, ne tourne
                        // jamais pour « faire face » au déplacement — sinon reculer
                        // (vecteur de vitesse pointant vers l'arrière) le ferait pivoter
                        // à 180° en continu.
                        cur_yaw
                    } else if vx * vx + vz * vz > 1e-6 {
                        // Rotation vers la direction de déplacement en amorti
                        // **exponentiel** (rapide au départ, doux à l'approche) plutôt
                        // qu'à vitesse constante + arrêt sec (`rotate_towards`) : la
                        // vitesse angulaire constante donnait un pivot mécanique qui
                        // « claquait » en fin de course.
                        let target_yaw = (-vx).atan2(-vz);
                        rotate_towards_smooth(cur_yaw, target_yaw, ctrl.turn_speed, dt)
                    } else {
                        cur_yaw
                    };
                    player_facing.push((idx, new_yaw));
                }
            }
            // Pilotage des « chasseurs » IA (cf. `AiChaser`) : direction directe vers la
            // position courante du joueur, recalculée chaque frame — une vraie poursuite
            // réactive (jeu local vs IA), pas une trajectoire fixe scriptée à l'avance.
            if !candidate_targets.is_empty() {
                // Cible la plus proche parmi `candidate_targets` pour chaque chasseur
                // visible (GAMEDESIGN_EN_LIGNE.md §3.2), regroupée par cible choisie
                // (indice dans `candidate_targets`, pas la position elle-même : sert
                // au plafond ci-dessous).
                let mut by_target: HashMap<usize, Vec<(usize, f32)>> = HashMap::new();
                // Éveils `Furtive` à signaler ce tick (Phase O Sprint 1,
                // `sprint2audijeu0718.md`) : indices qui viennent de franchir la portée
                // d'éveil, collectés ici plutôt qu'appliqués en direct dans la boucle
                // (qui emprunte `self.scene.objects`) — `self.furtive_awake`/`self.audio`
                // sont mis à jour juste après, une fois la boucle terminée.
                let mut newly_awake_furtives: Vec<usize> = Vec::new();
                for (idx, obj) in self.scene.objects.iter().enumerate() {
                    // Un monstre vaincu (invisible) ou d'une manche pas encore révélée
                    // ne poursuit pas (et n'a de toute façon pas de corps physique tant
                    // qu'il est masqué, cf. le filtre `visible` dans `Physics::build`).
                    let Some(chaser) = obj.ai_chaser.as_ref() else {
                        continue;
                    };
                    if !obj.visible {
                        continue;
                    }
                    let (target_i, dist_sq) = candidate_targets
                        .iter()
                        .enumerate()
                        .map(|(i, &t)| (i, (t - obj.transform.position).length_squared()))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .expect("candidate_targets vérifié non vide ci-dessus");
                    // Portée de détection, **réseau uniquement** (GAMEDESIGN_EN_LIGNE.md) :
                    // le plafond ci-dessus étale l'ARRIVÉE des chasseurs dans le temps, mais avec
                    // un seul joueur solo connecté, il n'empêche pas la convergence
                    // *finale* — au bout d'assez de temps, tous les monstres de la
                    // carte se relaient jusqu'à l'unique cible, même partis de l'autre
                    // bout de l'arène. Volontairement limité au cas réseau
                    // (`!self.network_players.is_empty()`) plutôt qu'appliqué partout :
                    // en solo, plusieurs démos (`Scene::brawl_demo` notamment) comptent
                    // sur un chasseur qui **revient toujours** vers le joueur après un
                    // recul (knockback) pour ne pas tomber dans le vide de l'arène —
                    // une portée de détection universelle cassait ce ring-out en
                    // laissant le rival immobile une fois repoussé trop loin (régression
                    // détectée par `brawl_demo_rival_survives_two_hits_then_falls_on_
                    // the_third`, qui ne teste rien de spécifique au réseau).
                    if !self.network_players.is_empty()
                        && dist_sq > CHASER_DETECT_RANGE * CHASER_DETECT_RANGE
                    {
                        phys.control(idx, 0.0, 0.0, false, 0.0, 0.0, dt);
                        continue;
                    }
                    // Éveil de l'archétype `Furtive` (GDD §5.4) : portée réduite,
                    // appliquée en toute circonstance (pas seulement en réseau, cf.
                    // `FURTIVE_DETECT_RANGE`) — c'est ce délai court qui permet au
                    // contre-jeu « l'Éclaireur la déclenche de loin » d'exister aussi solo.
                    if chaser.archetype == crate::scene::Archetype::Furtive
                        && dist_sq > FURTIVE_DETECT_RANGE * FURTIVE_DETECT_RANGE
                    {
                        phys.control(idx, 0.0, 0.0, false, 0.0, 0.0, dt);
                        continue;
                    }
                    // Transition endormie → active (Phase O Sprint 1) : ce tick est le
                    // premier où cette `Furtive` passe les deux gardes ci-dessus — pas
                    // de ré-armement si le joueur ressort puis revient à portée (une
                    // fois éveillée, elle le reste pour le reste de la partie, comme
                    // `trigger_prev` pour les triggers de zone).
                    if chaser.archetype == crate::scene::Archetype::Furtive
                        && !self.furtive_awake.contains(&idx)
                    {
                        newly_awake_furtives.push(idx);
                    }
                    by_target.entry(target_i).or_default().push((idx, dist_sq));
                }
                // Un son perceptible par éveil (Phase O Sprint 1) : appliqué après la
                // boucle ci-dessus (qui emprunte `self.scene.objects`), pas dedans.
                for idx in newly_awake_furtives {
                    self.furtive_awake.insert(idx);
                    crate::runtime::sfx::play(
                        &mut self.audio,
                        crate::runtime::sfx::Sfx::CreatureWake,
                    );
                }
                // Plafond de chasseurs actifs par cible : sans lui, TOUS les monstres
                // visibles convergent au même instant sur l'unique joueur présent (le cas
                // le plus courant en solo), acculant le joueur contre un mur en quelques
                // secondes sans la moindre fenêtre pour riposter ou fuir.
                // Recalculé chaque frame par distance : seuls les `MAX_ACTIVE_CHASERS_
                // PER_TARGET` chasseurs les plus proches d'une cible donnée avancent
                // réellement ce tick ; les autres restent en place (toujours visibles/
                // menaçants, juste pas en train de foncer) — un chasseur relégué reprend
                // la poursuite dès qu'un des premiers meurt ou s'éloigne, sans script ni
                // état à mémoriser d'une frame à l'autre.
                for (target_i, mut group) in by_target {
                    group.sort_by(|a, b| a.1.total_cmp(&b.1));
                    let target = candidate_targets[target_i];
                    for (rank, &(idx, _)) in group.iter().enumerate() {
                        if rank >= MAX_ACTIVE_CHASERS_PER_TARGET {
                            phys.control(idx, 0.0, 0.0, false, 0.0, 0.0, dt);
                            continue;
                        }
                        let obj_pos = self.scene.objects[idx].transform.position;
                        let ai = self.scene.objects[idx]
                            .ai_chaser
                            .as_ref()
                            .expect("filtré ci-dessus : cet objet a un ai_chaser");
                        // Multiplicateur d'archétype (GDD §5.4) : Meute/Furtive accélèrent
                        // la poursuite, Colosse la ralentit — cf. `Archetype::speed_multiplier`.
                        let speed = ai.speed * ai.archetype.speed_multiplier();
                        let to_target = target - obj_pos;
                        let dir = Vec3::new(to_target.x, 0.0, to_target.z);
                        let (vx, vz) = if dir.length_squared() > 1e-6 {
                            let d = dir.normalize() * speed;
                            (d.x, d.z)
                        } else {
                            (0.0, 0.0)
                        };
                        phys.control(idx, vx, vz, false, 0.0, 0.0, dt);
                    }
                }
            }
            // Recul (knockback, cf. `AppState::stagger`) : appliqué en dernier, après le
            // pilotage joystick/IA ci-dessus, pour qu'un coup encaissé cette frame ne soit
            // pas immédiatement écrasé par la vitesse que le joystick ou la poursuite
            // viennent de recalculer.
            self.stagger.retain_mut(|(idx, vel, remaining)| {
                phys.control(*idx, vel.x, vel.z, false, 0.0, 0.0, dt);
                *remaining -= dt;
                *remaining > 0.0
            });
            // Objets scriptés à collisions (`PhysicsKind::Kinematic`) : le
            // déplacement que leurs scripts viennent d'écrire (boucle 1. plus
            // haut) est résolu contre le monde (murs, objets fixes, joueur) —
            // la position réellement atteinte est réécrite dans la scène.
            phys.resolve_scripted_moves(dt, &mut self.scene);
            phys.step(dt, &mut self.scene);
            // Les créatures en pleine visée suivent la position réellement
            // atteinte (bousculades comprises), cf. `refresh_frozen_anchors`.
            self.refresh_frozen_anchors();
            // Cf. la note plus haut : appliqué après `step` pour ne jamais passer par
            // le corps rigide, qui écraserait sinon (et déstabiliserait) cette valeur.
            for (idx, yaw) in player_facing {
                if let Some(obj) = self.scene.objects.get_mut(idx) {
                    obj.transform.rotation = Quat::from_rotation_y(yaw);
                }
            }
            if any_jump {
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Jump);
            }
        }

        // Instantané de fin de pas pour l'interpolation de rendu (cf. `advance_play`) :
        // l'ancien « courant » devient le « précédent », puis on capture les poses
        // fraîches de ce pas — physique **et** scripts (plateformes animées, pièces
        // qui tournent… tout ce qui bouge à pas fixe profite du lissage).
        std::mem::swap(&mut self.sim_prev_poses, &mut self.sim_curr_poses);
        self.sim_curr_poses.clear();
        self.sim_curr_poses
            .extend(self.scene.objects.iter().map(|o| {
                (
                    o.transform.position,
                    o.transform.rotation,
                    o.transform.scale,
                )
            }));
    }

    /// Réécrit dans la scène les poses **exactes** du dernier pas de simulation,
    /// annulant le mélange visuel de `blend_render_poses` — à appeler avant de
    /// simuler de nouveaux pas. Sans effet si les instantanés ne correspondent pas
    /// (objets ajoutés/retirés depuis : le prochain `sim_step` resynchronise).
    ///
    /// Un objet dont le transform a été **modifié de l'extérieur** depuis le dernier
    /// mélange (réconciliation réseau, effet d'attaque à la frame, test, futur gizmo
    /// d'édition en Play…) n'est pas restauré : sa nouvelle pose est l'intention de
    /// celui qui l'a écrite, pas un artefact de mélange à annuler — la restaurer la
    /// ramènerait en arrière et l'écriture externe ne « prendrait » jamais.
    pub(super) fn restore_sim_poses(&mut self) {
        let n = self.scene.objects.len();
        if self.sim_curr_poses.len() != n || self.sim_render_poses.len() != n {
            return;
        }
        let ghosts = self.remote_player_scene_indices();
        for (i, obj) in self.scene.objects.iter_mut().enumerate() {
            if ghosts.contains(&i) || !pose_matches(&obj.transform, self.sim_render_poses[i]) {
                continue;
            }
            let (p, r, s) = self.sim_curr_poses[i];
            obj.transform.position = p;
            obj.transform.rotation = r;
            obj.transform.scale = s;
        }
    }

    /// Interpolation de rendu (cf. `advance_play`) : écrit dans les transforms un
    /// mélange des poses de l'avant-dernier (`alpha` = 0) et du dernier (`alpha` = 1)
    /// pas de simulation. Purement visuel : l'état de simulation vit dans les corps
    /// rigides et `sim_curr_poses`, restauré avant le pas suivant. Sans effet si les
    /// instantanés ne couvrent pas la scène actuelle (début de Play, objet ajouté).
    pub(super) fn blend_render_poses(&mut self, alpha: f32) {
        let n = self.scene.objects.len();
        if self.sim_prev_poses.len() != n || self.sim_curr_poses.len() != n {
            // Instantanés inexploitables (début de Play, objet ajouté) : invalide
            // aussi les poses de rendu, sinon `restore_sim_poses` comparerait les
            // transforms à un mélange d'une scène qui n'existe plus.
            self.sim_render_poses.clear();
            return;
        }
        let alpha = alpha.clamp(0.0, 1.0);
        // Les « fantômes » réseau ont leur propre interpolation, pilotée par les
        // snapshots serveur à la frame (cf. `poll_network`) : le mélange local les
        // ferait revenir en arrière sur une pose de simulation qui ne les pilote pas.
        let ghosts = self.remote_player_scene_indices();
        self.sim_render_poses.clear();
        for (i, obj) in self.scene.objects.iter_mut().enumerate() {
            let (pp, pr, ps) = self.sim_prev_poses[i];
            let (cp, cr, cs) = self.sim_curr_poses[i];
            // Une **téléportation** (ancre FX déplacée sur la cible, respawn…) n'est
            // pas un mouvement : l'interpoler tracerait une traînée entre les deux
            // points. Au-delà d'un déplacement impossible en un seul pas de 1/60 s
            // (`TELEPORT_SNAP_PER_STEP`), on claque directement sur la pose finale.
            let teleported =
                (cp - pp).length_squared() > TELEPORT_SNAP_PER_STEP * TELEPORT_SNAP_PER_STEP;
            if !ghosts.contains(&i) {
                if teleported {
                    obj.transform.position = cp;
                    obj.transform.rotation = cr;
                    obj.transform.scale = cs;
                } else {
                    obj.transform.position = pp.lerp(cp, alpha);
                    obj.transform.rotation = pr.slerp(cr, alpha);
                    obj.transform.scale = ps.lerp(cs, alpha);
                }
            }
            // Mémorise ce que le mélange vient d'écrire (pose des fantômes incluse,
            // pour garder l'indexation alignée) : référence de `restore_sim_poses`
            // pour détecter une écriture externe survenue après cette frame.
            self.sim_render_poses.push((
                obj.transform.position,
                obj.transform.rotation,
                obj.transform.scale,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::PlayerInput;
    use super::*;
    use crate::scene::SceneObject;

    #[test]
    fn rotate_towards_smooth_eases_toward_the_target_the_short_way() {
        // Progresse vers la cible sans jamais la dépasser (amorti, pas d'oscillation).
        let r = rotate_towards_smooth(0.0, 1.0, 10.0, 1.0 / 60.0);
        assert!(r > 0.0 && r < 1.0, "r={r}");
        // De 3.0 vers -3.0 : le chemin direct (-6.0 rad) est plus long que par le
        // « dos » du cercle (~0.28 rad) — ne doit jamais tourner du mauvais côté.
        let r = rotate_towards_smooth(3.0, -3.0, 10.0, 1.0 / 60.0);
        assert!(r > 3.0, "doit passer par le dos du cercle (r={r})");
        // Ease-out : le pas suivant, plus proche de la cible, est plus petit — la
        // rotation ralentit à l'approche au lieu de « claquer » à vitesse constante.
        let first = rotate_towards_smooth(0.0, 1.0, 10.0, 1.0 / 60.0);
        let second = rotate_towards_smooth(first, 1.0, 10.0, 1.0 / 60.0) - first;
        assert!(
            second < first,
            "le pas doit décroître (1er={first}, 2e={second})"
        );
    }

    /// PHASE I Sprint 1 (accessibilité §16.6) : `reduce_shake` coupe le recul
    /// caméra à zéro même avec `camera_shake` au pic, sans le remettre à zéro
    /// lui-même (d'autres systèmes, ex. le flash de dégâts, en dépendent
    /// indépendamment — cf. la doc de `camera_shake_offset`).
    #[test]
    fn camera_shake_offset_is_zero_when_reduce_shake_is_set() {
        let mut app = AppState::new();
        app.camera_shake = 1.0;
        app.reduce_shake = false;
        // `t=0` annulerait le jitter sinusoïdal indépendamment de `reduce_shake`
        // (sin(0) = 0) — un instant non nul isole bien la cause testée ici.
        app.time = 1.0;
        assert_ne!(app.camera_shake_offset(), Vec3::ZERO);

        app.reduce_shake = true;
        assert_eq!(app.camera_shake_offset(), Vec3::ZERO);
        assert_eq!(
            app.camera_shake, 1.0,
            "reduce_shake ne doit pas muter camera_shake"
        );
    }

    #[test]
    fn rotate_towards_smooth_is_framerate_independent() {
        // Deux pas de dt/2 doivent donner (quasi) le même angle qu'un pas de dt :
        // le lissage ne doit pas dépendre de la cadence de rendu/simulation.
        let one_step = rotate_towards_smooth(0.0, 1.0, 10.0, 1.0 / 30.0);
        let half = rotate_towards_smooth(0.0, 1.0, 10.0, 1.0 / 60.0);
        let two_steps = rotate_towards_smooth(half, 1.0, 10.0, 1.0 / 60.0);
        assert!(
            (one_step - two_steps).abs() < 1e-4,
            "1 pas de dt ({one_step}) doit égaler 2 pas de dt/2 ({two_steps})"
        );
    }

    #[test]
    fn fly_cam_moves_the_orbit_target_forward_and_up_while_editing() {
        // Caméra libre : flèche haut + Espace doivent avancer ET monter, sans
        // toucher à rien hors Play (`update_fly_cam` n'est appelé que si
        // `!playing && fly_cam`, cf. `advance_play`).
        let mut app = AppState::new();
        app.fly_cam = true;
        app.camera.yaw = 0.0;
        let before = app.camera.target;
        app.input_state.key_move = (0.0, 1.0);
        app.input_state.fly_vertical = 1.0;
        app.update_fly_cam(1.0 / 60.0);
        let after = app.camera.target;
        assert!(
            after.z < before.z,
            "flèche haut doit avancer (yaw=0 pointe vers -Z)"
        );
        assert!(after.y > before.y, "Espace doit faire monter la caméra");
    }

    #[test]
    fn toggle_fly_cam_is_a_no_op_while_playing() {
        // La caméra libre est un outil d'édition : `G` ne doit rien faire en Play,
        // sinon la caméra de jeu et la caméra libre se battraient pour `camera.target`.
        let mut app = AppState::new();
        app.playing = true;
        app.toggle_fly_cam();
        assert!(!app.fly_cam, "toggle_fly_cam doit être un no-op en Play");
    }

    #[test]
    fn entering_play_turns_off_fly_cam() {
        // Repasser en Play doit désactiver la caméra libre laissée active en
        // éditeur, sinon `update_fly_cam` et la caméra de suivi du joueur se
        // disputeraient `camera.target` (cf. `advance_play`).
        let mut app = AppState::new();
        app.fly_cam = true;
        app.playing = true;
        app.advance_play();
        assert!(
            !app.fly_cam,
            "advance_play doit désactiver fly_cam à l'entrée en Play"
        );
    }

    #[test]
    fn hand_tool_pan_is_not_snapped_back_by_the_player_follow_cam() {
        // Sprint : sans la garde `!self.gizmo_mode.is_nav()`, `advance_play`
        // écrasait chaque frame le pan appliqué par l'outil 🖐 Main (Q), rendant
        // la caméra impossible à déplacer en mode Play (cf. le rattrapage
        // exponentiel de `camera.target` sur le joueur ci-dessus).
        let mut app = AppState::new();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        app.scene.camera_follow = true;
        app.playing = true;
        // Première frame de Play : consomme le cadrage initial sur le joueur
        // (cf. plus haut, hors de la garde testée ici) avant de simuler le pan.
        app.advance_play();
        app.gizmo_mode = crate::app::GizmoMode::Pan;
        // Simule le pan que `PickingController::handle_input` applique en glissant
        // avec l'outil Main actif.
        app.camera.pan(50.0, 0.0);
        let panned_target = app.camera.target;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert_eq!(
            app.camera.target, panned_target,
            "la caméra de suivi ne doit pas re-cibler le joueur pendant un pan à l'outil Main"
        );
    }

    #[test]
    fn orbit_tool_yaw_is_not_pulled_back_towards_a_ranged_players_facing() {
        // Même classe de bug que le pan (cf. le test ci-dessus), pour l'outil 🔄
        // Orbite : un personnage équipé d'une arme à distance fait pivoter la
        // caméra vers son orientation de tir chaque frame (`rotate_towards_smooth`
        // ci-dessus) — sans la garde `!self.gizmo_mode.is_nav()`, ce rattrapage
        // écraserait l'orbite manuelle de l'utilisateur.
        let mut app = AppState::new();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                fire_button: "Feu".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        app.scene.camera_follow = true;
        app.playing = true;
        app.advance_play();
        app.gizmo_mode = crate::app::GizmoMode::Orbit;
        // Simule l'orbite manuelle que `PickingController::handle_input` applique
        // en glissant avec l'outil Orbite actif.
        app.camera.orbit(300.0, 0.0);
        let orbited_yaw = app.camera.yaw;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert_eq!(
            app.camera.yaw, orbited_yaw,
            "la caméra de suivi ne doit pas re-pivoter vers le joueur pendant une orbite manuelle"
        );
    }

    #[test]
    fn entering_play_clears_a_selection_left_over_from_the_editor() {
        // Une sélection/gizmo laissé actif depuis l'éditeur resterait cliquable en
        // Play et modifierait `transform` en concurrence avec la physique qui
        // pilote désormais le même objet (cf. `clear_selection` à la sortie de
        // Play, symétrique).
        let mut app = AppState::new();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Caisse".into(),
            mesh: crate::scene::MeshKind::Cube,
            ..Default::default()
        });
        app.select_single(0);
        assert!(app.selection.is_some());
        app.playing = true;
        app.advance_play();
        assert!(
            app.selection.is_none(),
            "advance_play doit vider la sélection éditeur à l'entrée en Play"
        );
    }

    #[test]
    fn player_input_combines_keyboard_and_touch_tank_axes() {
        // Le pavé tactile W/A/S/D et le clavier alimentent les mêmes axes « tank »
        // sans s'écraser : cumulés, bornés à [-1, 1].
        let inp = PlayerInput {
            key_thrust: 1.0,
            touch_thrust: 1.0,
            key_turn: -1.0,
            touch_turn: 0.5,
            ..Default::default()
        };
        assert_eq!(inp.thrust(), 1.0, "le cumul doit rester borné à 1");
        assert!((inp.turn() - -0.5).abs() < 1e-6, "les sources se cumulent");
        let touch_only = PlayerInput {
            touch_thrust: -1.0,
            touch_turn: 1.0,
            ..Default::default()
        };
        assert_eq!(touch_only.thrust(), -1.0, "le pavé seul suffit (APK)");
        assert_eq!(touch_only.turn(), 1.0);
    }

    #[test]
    fn camera_relative_move_matches_world_axes_at_zero_yaw() {
        // yaw=0 : comportement d'origine inchangé (droite=+X, haut=-Z), sinon tout
        // déplacement solo/existant tournerait sans qu'aucune caméra n'ait bougé.
        let (wx, wz) = camera_relative_move(1.0, 0.0, 0.0);
        assert!((wx - 1.0).abs() < 1e-5 && wz.abs() < 1e-5);
        let (wx, wz) = camera_relative_move(0.0, 1.0, 0.0);
        assert!(wx.abs() < 1e-5 && (wz - -1.0).abs() < 1e-5);
    }

    #[test]
    fn apply_deadzone_zeroes_a_residual_stick_reading() {
        // Un joystick qui ne revient pas exactement au centre au repos ne doit pas
        // faire dériver le personnage.
        let (mx, my) = apply_deadzone((0.05, 0.02), JOYSTICK_DEADZONE);
        assert!(mx.abs() < 1e-6 && my.abs() < 1e-6);
    }

    #[test]
    fn apply_deadzone_preserves_direction_and_full_push() {
        // Poussée franche : direction conservée, plein débattement (longueur 1) intact.
        let (mx, my) = apply_deadzone((1.0, 0.0), JOYSTICK_DEADZONE);
        assert!((mx - 1.0).abs() < 1e-5 && my.abs() < 1e-6);
        let (mx, my) = apply_deadzone((0.5, 0.3), JOYSTICK_DEADZONE);
        // Remappée (donc un peu plus courte que l'entrée brute) mais même direction.
        assert!(mx > 0.0 && my > 0.0, "même quadrant que l'entrée");
        assert!((my / mx - 0.3 / 0.5).abs() < 1e-5, "direction conservée");
        let len = (mx * mx + my * my).sqrt();
        assert!(len > 0.0 && len < (0.5f32 * 0.5 + 0.3 * 0.3).sqrt());
    }

    #[test]
    fn apply_deadzone_starts_from_zero_at_the_edge_of_the_deadzone() {
        // Continuité au bord du rayon mort : juste au-dessus du seuil, l'entrée doit
        // être quasi nulle (départ progressif), pas sauter d'un coup à ~0.15 — le
        // « cran » perceptible que le remappage supprime.
        let (mx, my) = apply_deadzone((JOYSTICK_DEADZONE + 0.01, 0.0), JOYSTICK_DEADZONE);
        let len = (mx * mx + my * my).sqrt();
        assert!(
            len < 0.05,
            "l'entrée doit démarrer près de zéro au bord du rayon mort (len={len})"
        );
    }

    #[test]
    fn blend_render_poses_interpolates_between_the_last_two_sim_steps() {
        let mut app = AppState::new();
        let n = app.scene.objects.len();
        // Delta de 0,1 m par pas (6 m/s : un déplacement normal, sous le seuil de
        // téléportation) : à mi-accumulateur, le rendu doit être à mi-chemin.
        app.sim_prev_poses = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
        app.sim_curr_poses = vec![(Vec3::new(0.1, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE); n];
        app.blend_render_poses(0.5);
        let p = app.scene.objects[0].transform.position;
        assert!(
            (p.x - 0.05).abs() < 1e-6,
            "à mi-accumulateur, le rendu doit afficher la pose à mi-chemin (x={})",
            p.x
        );
    }

    /// Audit du 16 juillet 2026 : le bilan de perf périodique doit retenir la
    /// **pire** frame de la fenêtre (c'est elle qui fait les à-coups), ignorer
    /// les `dt` aberrants (throttle/veille), repartir de zéro à chaque fenêtre
    /// écoulée, et ne rien accumuler hors Play (frames volontairement throttlées).
    #[test]
    fn the_perf_log_window_tracks_the_worst_frame_and_resets_each_window() {
        let mut app = AppState::new();
        app.playing = true;
        let t0 = Instant::now();
        app.perf_window_start = t0;

        let d = std::time::Duration::from_secs;
        app.log_perf_window(t0 + d(1), 1.0 / 60.0);
        app.log_perf_window(t0 + d(2), 0.050); // à-coup réel : doit être retenu
        app.log_perf_window(t0 + d(3), 1.0 / 60.0);
        assert!(
            (app.perf_window_worst_dt - 0.050).abs() < 1e-6,
            "la pire frame de la fenêtre doit être retenue (worst={})",
            app.perf_window_worst_dt
        );
        // dt aberrant (> 0,5 s : throttle, mise en veille) : ignoré.
        app.log_perf_window(t0 + d(4), 2.0);
        assert!(
            (app.perf_window_worst_dt - 0.050).abs() < 1e-6,
            "un dt aberrant ne doit pas polluer la pire frame"
        );
        // Fenêtre écoulée : bilan flushé, la suivante repart de zéro.
        app.log_perf_window(t0 + d(11), 1.0 / 60.0);
        assert_eq!(
            app.perf_window_worst_dt, 0.0,
            "la fenêtre doit repartir de zéro après le bilan"
        );
        // Hors Play : rien n'est accumulé (les frames sont throttlées exprès).
        app.playing = false;
        app.log_perf_window(t0 + d(12), 0.2);
        assert_eq!(
            app.perf_window_worst_dt, 0.0,
            "hors Play, aucune frame ne doit être comptée"
        );
    }

    #[test]
    fn blend_render_poses_snaps_on_teleport_instead_of_streaking() {
        // Une téléportation (respawn, ancre FX déplacée sur sa cible) ne doit pas être
        // interpolée : le rendu claque directement sur la pose finale, sans traînée.
        let mut app = AppState::new();
        let n = app.scene.objects.len();
        let target = Vec3::new(5.0, 0.5, -3.0);
        app.sim_prev_poses = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
        app.sim_curr_poses = vec![(target, Quat::IDENTITY, Vec3::ONE); n];
        app.blend_render_poses(0.5);
        assert!(
            (app.scene.objects[0].transform.position - target).length() < 1e-6,
            "au-delà du seuil de téléportation, la pose finale doit être affichée telle quelle"
        );
    }

    #[test]
    fn restore_sim_poses_undoes_the_visual_blend_before_simulating() {
        // La pose affichée (mélangée) ne doit jamais servir d'état de départ à la
        // simulation : `restore_sim_poses` doit rétablir la pose exacte du dernier pas.
        let mut app = AppState::new();
        let n = app.scene.objects.len();
        let curr = Vec3::new(0.2, 0.0, -0.1);
        app.sim_prev_poses = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
        app.sim_curr_poses = vec![(curr, Quat::IDENTITY, Vec3::ONE); n];
        app.blend_render_poses(0.25);
        assert!((app.scene.objects[0].transform.position - curr * 0.25).length() < 1e-6);
        app.restore_sim_poses();
        assert!(
            (app.scene.objects[0].transform.position - curr).length() < 1e-6,
            "la pose de simulation exacte doit être rétablie avant le pas suivant"
        );
    }

    #[test]
    fn restore_sim_poses_respects_an_external_transform_write() {
        // Une écriture externe du transform (réconciliation réseau, test, futur gizmo
        // en Play) entre deux frames ne doit pas être annulée par la restauration :
        // c'est une intention, pas un artefact de mélange.
        let mut app = AppState::new();
        let n = app.scene.objects.len();
        app.sim_prev_poses = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
        app.sim_curr_poses = vec![(Vec3::new(0.1, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE); n];
        app.blend_render_poses(0.5);
        let moved = Vec3::new(50.0, 0.5, 50.0);
        app.scene.objects[0].transform.position = moved;
        app.restore_sim_poses();
        assert!(
            (app.scene.objects[0].transform.position - moved).length() < 1e-6,
            "une pose écrite de l'extérieur doit survivre à la restauration"
        );
        // Un objet non touché, lui, est bien restauré sur la pose de simulation.
        if n > 1 {
            assert!((app.scene.objects[1].transform.position.x - 0.1).abs() < 1e-6);
        }
    }

    #[test]
    fn blend_render_poses_is_a_no_op_without_matching_snapshots() {
        // Début de Play (instantanés vides) ou objet ajouté en cours de partie :
        // le mélange ne doit pas écrire des poses obsolètes dans la scène.
        let mut app = AppState::new();
        let before = app.scene.objects[0].transform.position;
        app.blend_render_poses(0.5);
        assert_eq!(app.scene.objects[0].transform.position, before);
    }

    #[test]
    fn clamp_move_vector_leaves_a_single_axis_unchanged() {
        let (mx, my) = clamp_move_vector(1.0, 0.0);
        assert!((mx - 1.0).abs() < 1e-6 && my.abs() < 1e-6);
    }

    #[test]
    fn clamp_move_vector_normalizes_a_diagonal_to_unit_length() {
        // Avant le correctif : (1.0, 1.0) restait tel quel (clamp par axe), donnant
        // une longueur √2 — un déplacement en diagonale ~41 % plus rapide qu'en
        // ligne droite. Le vecteur doit maintenant être ramené à une longueur de 1.
        let (mx, my) = clamp_move_vector(1.0, 1.0);
        let len = (mx * mx + my * my).sqrt();
        assert!((len - 1.0).abs() < 1e-5, "longueur={len}");
        // Toujours dans la même direction (diagonale), pas juste raccourci n'importe où.
        assert!((mx - my).abs() < 1e-6);
    }

    #[test]
    fn clamp_move_vector_never_amplifies_a_short_vector() {
        // Un joystick à mi-course (longueur < 1) ne doit pas être gonflé à 1 —
        // seuls les vecteurs qui dépassent 1 sont ramenés à cette longueur.
        let (mx, my) = clamp_move_vector(0.3, 0.0);
        assert!((mx - 0.3).abs() < 1e-6 && my.abs() < 1e-6);
    }

    #[test]
    fn camera_relative_move_rotates_forward_with_the_camera() {
        // À 90° (caméra tournée d'un quart de tour), « avancer » (my=1) ne doit
        // plus pointer vers -Z mais vers -X : le joystick doit suivre la caméra,
        // pas rester bloqué sur les axes du monde (façon caméra de suivi à la Zelda).
        let (wx, wz) = camera_relative_move(0.0, 1.0, std::f32::consts::FRAC_PI_2);
        assert!((wx - -1.0).abs() < 1e-4, "wx={wx}");
        assert!(wz.abs() < 1e-4, "wz={wz}");
    }

    #[test]
    fn creature_is_server_synced_stays_false_without_any_snapshot_ever_received() {
        // Room jointe sans succès côté serveur (gabarit introuvable) ou scène
        // désynchronisée : la créature n'a jamais reçu le moindre `Snapshot`. Sans
        // filet, elle resterait figée pour toujours — la synchro doit rester
        // fausse (et donc le script local continuer de tourner) tant qu'aucune
        // mise à jour n'est jamais arrivée, peu importe le timeout.
        let now = Instant::now();
        let timeout = std::time::Duration::from_millis(2500);
        assert!(!creature_is_server_synced(None, now, timeout));
    }

    #[test]
    fn creature_is_server_synced_true_right_after_a_fresh_snapshot() {
        let now = Instant::now();
        let timeout = std::time::Duration::from_millis(2500);
        assert!(creature_is_server_synced(Some(now), now, timeout));
    }

    #[test]
    fn creature_is_server_synced_resumes_local_simulation_once_snapshots_go_stale() {
        // Le serveur diffusait, puis s'arrête (déconnexion silencieuse,
        // redémarrage) : passé le délai de grâce, on ne doit plus considérer la
        // créature comme synchronisée — sinon elle resterait figée à sa dernière
        // position serveur pour toujours au lieu de reprendre son script local.
        let last_snapshot = Instant::now();
        let timeout = std::time::Duration::from_millis(2500);
        let still_fresh = last_snapshot + std::time::Duration::from_millis(2400);
        let now_stale = last_snapshot + std::time::Duration::from_millis(2600);
        assert!(creature_is_server_synced(
            Some(last_snapshot),
            still_fresh,
            timeout
        ));
        assert!(!creature_is_server_synced(
            Some(last_snapshot),
            now_stale,
            timeout
        ));
    }

    #[test]
    fn fixed_substeps_is_framerate_independent() {
        let fixed = 1.0 / 60.0;
        // 60 FPS : 1 frame = 1 pas, reliquat ~0.
        let (n, acc) = fixed_substeps(0.0, fixed, fixed, 5);
        assert_eq!(n, 1);
        assert!(acc.abs() < 1e-6);
        // 30 FPS : une frame longue = 2 pas fixes (rattrapage).
        let (n, _) = fixed_substeps(0.0, 1.0 / 30.0, fixed, 5);
        assert_eq!(n, 2);
        // 120 FPS : frame trop courte → 0 pas, le temps s'accumule.
        let (n, acc) = fixed_substeps(0.0, 1.0 / 120.0, fixed, 5);
        assert_eq!(n, 0);
        assert!(acc > 0.0);
        // Deux frames à 120 FPS finissent par produire un pas.
        let (n2, _) = fixed_substeps(acc, 1.0 / 120.0, fixed, 5);
        assert_eq!(n2, 1);
        // Gel long : borné par le cap (pas de spirale), accumulateur remis à 0.
        let (n, acc) = fixed_substeps(0.0, 5.0, fixed, 5);
        assert_eq!(n, 5);
        assert_eq!(acc, 0.0);
    }

    #[test]
    fn step_requested_advances_exactly_one_fixed_tick_while_paused() {
        // Le bouton « ⏭ » doit avancer d'exactement un pas fixe en pause,
        // ni plus (pas de rattrapage), ni moins (pas d'attente supplémentaire), puis
        // regeler la simulation tant qu'aucune nouvelle demande n'arrive.
        let mut app = AppState::new();
        app.playing = true;
        app.paused = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play(); // transition Edit→Play + première frame gelée
        assert_eq!(
            app.time, 0.0,
            "en pause sans demande, le temps ne doit pas avancer"
        );

        app.request_step();
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let fixed_dt = 1.0 / 60.0;
        assert!(
            (app.time - fixed_dt).abs() < 1e-5,
            "un seul pas fixe attendu : time={}, attendu≈{fixed_dt}",
            app.time
        );

        // Sans nouvelle demande, la pause suivante ne doit pas avancer davantage.
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert!(
            (app.time - fixed_dt).abs() < 1e-5,
            "sans nouvelle demande, le temps ne doit plus avancer : time={}",
            app.time
        );
    }

    #[test]
    fn sim_step_advances_animation_time_scaled_by_speed() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject {
            animation: Some(crate::scene::AnimationState {
                clip: "Run".into(),
                time: 0.0,
                speed: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        });
        app.sim_step(0.1);
        let anim = app.scene.objects[0].animation.as_ref().unwrap();
        assert!(
            (anim.time - 0.2).abs() < 1e-6,
            "0.1s à vitesse 2x doit avancer time de 0.2s, obtenu {}",
            anim.time
        );
    }

    /// Hors Play, les clips squelettaux tournent quand même (prévisualisation
    /// d'édition) : `advance_play` avance `anim.time` au dt de frame même quand
    /// `playing == false` — sans ça, tout GLB riggé reste figé en pose de liaison
    /// (T-pose) dans la vue d'édition tant qu'on ne lance pas Play.
    #[test]
    fn edit_mode_still_advances_skeletal_clips() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject {
            animation: Some(crate::scene::AnimationState {
                clip: "Idle".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(!app.playing, "AppState démarre en mode édition");
        // Simule ~50 ms écoulés depuis la frame précédente (l'horloge réelle
        // d'`advance_play` ne verrait que quelques µs entre deux appels de test).
        app.last_frame = Instant::now() - std::time::Duration::from_millis(50);
        app.advance_play();
        let anim = app.scene.objects[0].animation.as_ref().unwrap();
        assert!(
            anim.time >= 0.04,
            "~50 ms hors Play doivent avancer la lecture d'autant, obtenu {}",
            anim.time
        );
    }

    #[test]
    fn sim_step_leaves_objects_without_animation_untouched() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject::default());
        app.sim_step(0.1);
        assert!(app.scene.objects[0].animation.is_none());
    }

    /// Preuve jouable : la « créature » (assets/models/creature.glb, rig
    /// Root/Body/Head/LegL/LegR exporté depuis Blender via le connecteur MCP, clips
    /// `Idle`/`Walk`) se déplace réellement via un script Lua de wander, pas seulement
    /// en apparence (animation qui tourne sans que `transform.position` bouge). Le
    /// script alterne 3s de marche (`obj.anim = "Walk"`, position qui dérive en cercle)
    /// puis 1s d'arrêt (`obj.anim = "Idle"`, position figée) — mêmes mécanismes que
    /// `AiChaser`/`Combat` (Lua pilote `obj.x/z` et `obj.anim`, lus par `run_script` en
    /// fin d'appel), mais en patrouille scriptée plutôt qu'en poursuite du joueur (cf.
    /// la doc de `AiChaser` sur cette distinction).
    #[test]
    fn scripted_creature_wanders_then_idles_using_the_imported_walk_and_idle_clips() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/models/creature.glb");
        let (data, aabb_min, aabb_max) =
            crate::scene::import::load_gltf(path).expect("creature.glb doit être un glTF valide");
        let mut imported = crate::scene::ImportedMesh {
            path: path.to_string(),
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        imported.load_skinning();
        assert!(
            imported.skeleton.is_some(),
            "creature.glb doit être skinné (rig Blender exporté avec Export Skins)"
        );
        let clip_names: Vec<&str> = imported.clips.iter().map(|c| c.name.as_str()).collect();
        assert!(clip_names.contains(&"Idle") && clip_names.contains(&"Walk"));

        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.imported.clear();
        app.scene.imported.push(imported);
        app.scene.objects.push(SceneObject {
            mesh: crate::scene::MeshKind::Imported(0),
            animation: Some(crate::scene::AnimationState {
                clip: "Idle".into(),
                ..Default::default()
            }),
            script: r#"
                local t = time % 4.0
                if t < 3.0 then
                    obj.x = obj.x + math.sin(time * 1.5) * 0.6 * dt
                    obj.z = obj.z + math.cos(time * 1.5) * 0.6 * dt
                    obj.anim = "Walk"
                else
                    obj.anim = "Idle"
                end
            "#
            .into(),
            ..Default::default()
        });

        let dt = 1.0 / 60.0;
        for _ in 0..(3 * 60) {
            app.sim_step(dt);
        }
        let after_walk = app.scene.objects[0].transform.position;
        assert!(
            after_walk.distance(Vec3::ZERO) > 0.1,
            "après 3s de phase Walk, la créature doit s'être déplacée (position={after_walk:?})"
        );
        assert_eq!(
            app.scene.objects[0].animation.as_ref().unwrap().clip,
            "Walk"
        );

        for _ in 0..60 {
            app.sim_step(dt);
        }
        let after_idle = app.scene.objects[0].transform.position;
        assert!(
            (after_idle - after_walk).length() < 1e-5,
            "en phase Idle la position ne doit plus bouger : avant={after_walk:?}, après={after_idle:?}"
        );
        assert_eq!(
            app.scene.objects[0].animation.as_ref().unwrap().clip,
            "Idle"
        );
    }

    /// Preuve jouable, avec la **vraie** physique (raycasts réels contre les murs) :
    /// la créature de `Scene::mmorpg_demo` ne doit jamais rester collée contre un mur
    /// à jouer son animation « Walk » sans avancer. Bug observé en jeu (corrigé après
    /// cette preuve) : le déclenchement du virage anticipé (`near_edge`) et le clamp
    /// dur de fin de script comparaient tous deux `obj.x`/`obj.z` à la même borne
    /// (`BOUND`) — sans marge, un rayon manquant un mur en approche tangente laissait
    /// la créature dériver jusqu'au clamp, s'y faire plaquer chaque frame (jamais
    /// `> BOUND` une fois clampée, donc `near_edge` restait faux), et y rester bloquée
    /// en boucle d'animation. Ce test fait tourner ~30 s simulées du vrai script de
    /// production (`Scene::mmorpg_demo`, pas une version simplifiée) contre le vrai
    /// monde physique (`Physics::build`, murs/repères inclus) et échoue si la
    /// créature passe plus d'1 s d'affilée collée à moins de 20 cm d'une borne
    /// d'arène sans progresser.
    #[test]
    fn mmorpg_creature_never_gets_stuck_walking_into_a_wall() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Créature")
            .expect(
                "la démo MMORPG doit contenir une « Créature » (creature.glb chargé, \
                 cf. Scene::mmorpg_demo)",
            );
        for (name, glb) in [
            ("Créature 2", "creature2.glb"),
            ("Créature 3", "creature3.glb"),
            ("Créature 4", "creature4.glb"),
            ("Créature 5", "creature5.glb"),
            ("Créature 6", "creature6.glb"),
            ("Créature 7", "creature7.glb"),
            ("Créature 8", "creature8.glb"),
            ("Créature 9", "creature9.glb"),
            ("Créature 10", "creature10.glb"),
            ("Créature 11", "creature11.glb"),
            ("Créature 12", "creature12.glb"),
            ("Créature 13", "creature13.glb"),
            ("Créature 14", "creature14.glb"),
            ("Créature 15", "creature15.glb"),
            ("Créature 16", "creature16.glb"),
            ("Créature 17", "creature17.glb"),
            ("Créature 18", "creature18.glb"),
            ("Créature 19", "creature19.glb"),
            ("Créature 20", "creature20.glb"),
            ("Créature 21", "creature21.glb"),
            ("Créature 22", "creature22.glb"),
            ("Créature 23", "creature23.glb"),
            ("Créature 24", "creature24.glb"),
            ("Créature 25", "creature25.glb"),
            ("Créature 26", "creature26.glb"),
        ] {
            assert!(
                app.scene.objects.iter().any(|o| o.name == name),
                "la démo MMORPG doit aussi contenir la « {name} » ({glb}, généré \
                 sous Blender — cf. Scene::mmorpg_demo)"
            );
        }
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        // Bornes réelles de l'arène (cf. `Scene::MMORPG_HALF`) moins une petite
        // marge : au-delà, la créature est effectivement pressée contre un mur.
        let arena_limit = crate::scene::Scene::MMORPG_HALF - 0.6;
        let dt = 1.0 / 60.0;
        let mut pinned_frames = 0u32;
        let max_pinned_frames = 60; // 1 s d'affilée collée à un bord = bug
        let mut prev_yaw: Option<f32> = None;
        let mut prev_pos = app.scene.objects[idx].transform.position;
        let mut idle_frames = 0u32;

        for step in 0..(30 * 60) {
            app.sim_step(dt);
            let obj = &app.scene.objects[idx];
            let pos = obj.transform.position;
            assert!(
                pos.x.abs() <= arena_limit + 0.05 && pos.z.abs() <= arena_limit + 0.05,
                "step {step} : la créature est sortie de l'arène (position={pos:?})"
            );
            let pinned = pos.x.abs() > arena_limit - 0.2 || pos.z.abs() > arena_limit - 0.2;
            pinned_frames = if pinned { pinned_frames + 1 } else { 0 };
            assert!(
                pinned_frames <= max_pinned_frames,
                "step {step} : la créature semble bloquée contre un mur \
                 ({pinned_frames} frames d'affilée près d'un bord, position={pos:?})"
            );

            // Pas de pivot brusque d'une frame à l'autre : cf. la doc de
            // `creature_wander_script` (3ᵉ version) — un virage-cible instantané
            // donnait des demi-tours visibles d'une frame à l'autre.
            let (_, yaw, _) = obj.transform.rotation.to_euler(glam::EulerRot::XYZ);
            if let Some(prev) = prev_yaw {
                let mut delta = (yaw - prev).to_degrees();
                delta = ((delta + 180.0).rem_euclid(360.0)) - 180.0;
                assert!(
                    delta.abs() < 20.0,
                    "step {step} : virage brusque d'une frame à l'autre ({delta:.1}°) — \
                     devrait tourner progressivement, jamais faire un demi-tour instantané"
                );
            }
            prev_yaw = Some(yaw);

            if (pos - prev_pos).length() < 1e-4 {
                idle_frames += 1;
            }
            prev_pos = pos;
        }

        // Ne doit pas passer un temps disproportionné à l'arrêt (l'ancienne version
        // s'arrêtait 1 s sur 4 sur un minuteur fixe, plus l'arrêt en cours de virage) :
        // une patrouille naturelle marche la grande majorité du temps.
        let idle_ratio = idle_frames as f32 / (30.0 * 60.0);
        assert!(
            idle_ratio < 0.15,
            "la créature est restée immobile {:.0}% du temps (attendu < 15%) — \
             trop d'arrêts pour une patrouille censée avancer en continu",
            idle_ratio * 100.0
        );
    }

    /// Preuve dédiée à la Créature 13 (méduse, `creature_drift_script`) : une
    /// fois le Lac muré (murs d'eau invisibles, cf. `mmorpg_demo`), elle dérive
    /// dans un rayon local autour de son spawn plutôt que de viser le centre de
    /// l'arène — bug corrigé après une trace de 60 s qui la montrait plaquée
    /// contre le mur est du lac (`x≈-12`, jamais assez proche du centre de
    /// l'arène pour déclencher l'ancien rappel absolu). Même critère
    /// d'immobilité que le test générique ci-dessus, sur 60 s pour laisser le
    /// temps à plusieurs allers-retours dans le lac.
    #[test]
    fn mmorpg_creature_13_drifts_in_its_lake_without_getting_stuck() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Créature 13")
            .expect("la démo MMORPG doit contenir la « Créature 13 »");
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        let dt = 1.0 / 60.0;
        let mut idle_frames = 0u32;
        let mut prev_pos = app.scene.objects[idx].transform.position;
        const STEPS: u32 = 60 * 60;
        for _ in 0..STEPS {
            app.sim_step(dt);
            let pos = app.scene.objects[idx].transform.position;
            if (pos - prev_pos).length() < 1e-4 {
                idle_frames += 1;
            }
            prev_pos = pos;
        }
        let idle_ratio = idle_frames as f32 / STEPS as f32;
        assert!(
            idle_ratio < 0.15,
            "la Créature 13 est restée immobile {:.0}% du temps (attendu < 15%) — \
             probablement plaquée contre un mur d'eau",
            idle_ratio * 100.0
        );
    }

    /// Sprint 25 (Phase K, `sprintreflecion.md`) : la bande de collines à
    /// l'ouest (`gfx::mesh::MMORPG_HILL_STRIP_X_LOCAL`, x monde ∈[-36,-33])
    /// n'est pas qu'un décor visuel — un obstacle réel pour la sonde IA de
    /// patrouille. Ne réutilise pas juste une créature existante (elles
    /// évitent déjà la bande par construction, cf. `MMORPG_CREATURES`) : on
    /// reprend directement le vrai script de production
    /// (`scene::demos::creature_wander_script`, rendu accessible aux tests
    /// via `pub(crate)`) planté DANS le plateau (x=-35,25, pleine amplitude,
    /// cf. `mmorpg_terrain_local_height`), avec un cap plein nord (le long de
    /// la bande, pas perpendiculaire) : contrairement à l'axe X (rampe du
    /// plateau à 0 en seulement 0,5 m — bien trop raide pour un
    /// `KinematicCharacterController` de créature, qui n'a PAS d'`autostep`
    /// contrairement au joueur, cf. `resolve_scripted_moves`), le relief
    /// varie doucement le long de Z (fréquence bien plus basse dans
    /// `mmorpg_terrain_local_height`), ce qui laisse la patrouille suivre
    /// réellement la pente au lieu de buter dessus comme sur un mur.
    /// Vérifie : (1) jamais figée plus d'1 s d'affilée (même piège que
    /// documenté dans la mémoire projet : un obstacle non visible au
    /// raycast à 0,6 m fige la patrouille) pendant qu'elle chevauche la
    /// bande, et (2) sa hauteur `y` suit bien le relief sous elle — même
    /// gabarit de tolérance que
    /// `runtime::physics::tests::a_dynamic_body_settles_on_the_terrain_hill_at_the_right_height`
    /// (`y` jamais bien en-dessous du sol attendu, jamais en lévitation
    /// franche au-dessus), et une variation de hauteur bien réelle sur le
    /// trajet — preuve qu'elle « négocie » la pente plutôt que de rester
    /// plaquée à une hauteur constante.
    #[test]
    fn mmorpg_creature_wander_crosses_the_west_hill_band_without_getting_stuck() {
        use crate::gfx::mesh::{MMORPG_HILL_STRIP_X_LOCAL, mmorpg_terrain_local_height};
        use crate::scene::demos::creature_wander_script;

        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let half = crate::scene::Scene::MMORPG_HALF;
        let idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Créature")
            .expect("la démo MMORPG doit contenir une « Créature »");

        // Bornes monde de la bande de collines (cf. `MMORPG_HILL_STRIP_X_LOCAL`,
        // en coordonnées locales ∈[-0.5,0.5] à reconvertir en mètres via ×72).
        let (band_x_lo, band_x_hi) = MMORPG_HILL_STRIP_X_LOCAL;
        let world_size = 2.0 * half;
        let band_x_lo_m = band_x_lo * world_size;
        let band_x_hi_m = band_x_hi * world_size;
        assert!(
            band_x_lo_m < -33.0 && band_x_hi_m > -35.6,
            "bande attendue autour de x∈[-36,-33] (obtenu [{band_x_lo_m},{band_x_hi_m}])"
        );

        {
            let obj = &mut app.scene.objects[idx];
            // x=-35.25 : centre du plateau à pleine amplitude (x∈[-35.5,-35.0]).
            // z=-25 : loin de la route (coupée à plat entre z=9 et 19) et loin
            // du mur nord (retombée à 0 dès |z|>33) — relief bien réel des deux
            // côtés du trajet (cf. le calcul de `mmorpg_terrain_local_height`
            // le long de cet axe). Cap plein nord (heading=0°, cf.
            // `creature_wander_script` : fwd = (sin(h), cos(h)), donc fwd=(0,1)
            // à 0°).
            // Départ tout près du relief attendu à ce point (calculé par la
            // même fonction que le sol) plutôt qu'à y=0 : sinon la créature
            // démarre enfoncée sous le relief (ici ~1,1 m) et toute la
            // patrouille se limiterait à remonter cette chute de rattrapage
            // au lieu de longer la bande — l'objectif ici est de mesurer la
            // variation de hauteur PENDANT la patrouille, pas pendant un
            // rattrapage de spawn (contrairement à
            // `a_dynamic_body_settles_on_the_terrain_hill_at_the_right_height`,
            // qui mesure justement une chute).
            let spawn_x = -35.25_f32;
            let spawn_z = -25.0_f32;
            let spawn_h =
                mmorpg_terrain_local_height(spawn_x / (2.0 * half), spawn_z / (2.0 * half));
            obj.transform.position = Vec3::new(spawn_x, spawn_h + 0.05, spawn_z);
            let ray_mask = !obj.collision_layer;
            obj.script = creature_wander_script(half, "sprint25_hill_test_", ray_mask, 0.0, 0.0);
        }
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let dt = 1.0 / 60.0;

        const STEPS: u32 = 20 * 60;
        let mut prev_pos = app.scene.objects[idx].transform.position;
        let mut pinned_frames = 0u32;
        let mut entered_band = false;
        let mut min_y_in_band = f32::MAX;
        let mut max_y_in_band = f32::MIN;

        for step in 0..STEPS {
            app.sim_step(dt);
            let pos = app.scene.objects[idx].transform.position;

            if (pos - prev_pos).length() < 1e-4 {
                pinned_frames += 1;
            } else {
                pinned_frames = 0;
            }
            assert!(
                pinned_frames <= 60,
                "step {step} : la créature semble figée plus d'1 s d'affilée dans/près \
                 de la bande de collines (position={pos:?})"
            );

            if pos.x >= band_x_lo_m && pos.x <= band_x_hi_m {
                entered_band = true;
                let expected_h =
                    mmorpg_terrain_local_height(pos.x / world_size, pos.z / world_size);
                let y = pos.y;
                // Même gabarit de tolérance que
                // `a_dynamic_body_settles_on_the_terrain_hill_at_the_right_height` :
                // ni traversée du relief vers le bas, ni lévitation franche
                // au-dessus (un peu plus large ici pour la marche, pas une
                // chute libre qui se stabilise).
                assert!(
                    y > expected_h - 0.5,
                    "step {step} : la créature a traversé le relief vers le bas \
                     (y={y}, sol attendu ≈{expected_h}, position={pos:?})"
                );
                assert!(
                    y < expected_h + 1.5,
                    "step {step} : la créature lévite au-dessus du relief \
                     (y={y}, sol attendu ≈{expected_h}, position={pos:?})"
                );
                min_y_in_band = min_y_in_band.min(y);
                max_y_in_band = max_y_in_band.max(y);
            }

            prev_pos = pos;
        }

        assert!(
            entered_band,
            "la créature n'a jamais chevauché la bande de collines (x∈[{band_x_lo_m},\
             {band_x_hi_m}]) — le test ne prouve rien"
        );
        assert!(
            max_y_in_band - min_y_in_band > 0.3,
            "la créature a parcouru la bande sans que sa hauteur ne varie \
             (min={min_y_in_band}, max={max_y_in_band}) — suspect sur un relief \
             avec plusieurs dizaines de cm d'amplitude le long de ce trajet"
        );
    }

    /// Audit gameplay « gros sauts / déplacements illogiques » : preuve que les
    /// **20** créatures de la démo MMORPG bougent continûment, sans téléportation
    /// ni pivot brutal, avec la vraie physique. Bugs observés en jeu (corrigés
    /// après cette preuve) : le griffon (n°16) et le kraken (n°17) écrivaient
    /// leur position **en absolu** sur une courbe paramétrique — saut initial de
    /// tout le rayon au premier tick, et bond de rattrapage après chaque blocage
    /// (l'angle continuait d'avancer pendant que `resolve_scripted_moves`
    /// rabotait le déplacement) ; le félin (n°12) et la lanterne (n°19)
    /// claquaient vitesse et cap de 90-180° pile sur leurs seuils de distance ;
    /// l'escargot (n°14) se retournait de 180° en une frame en bout de navette.
    /// Chaque frame, pour chaque créature : déplacement horizontal ≤ vitesse max
    /// (3,2 m/s, la charge du ver) × dt × marge, et pivot ≤ 25° (sauf le ver n°18,
    /// dont le cap de charge « sous le sable » est un surgissement assumé).
    #[test]
    fn mmorpg_creatures_never_teleport_nor_snap_turn() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let creatures: Vec<usize> = app
            .scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.name.starts_with("Créature"))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(creatures.len(), 26, "la démo doit garder ses 26 créatures");

        let dt = 1.0 / 60.0;
        // 3,2 m/s (charge du ver, la plus rapide) + marge : au-delà en une frame,
        // c'est une téléportation, pas un déplacement.
        let max_step = 3.2 * dt * 1.7;
        let mut prev: Vec<(glam::Vec3, Option<f32>)> = creatures
            .iter()
            .map(|&i| (app.scene.objects[i].transform.position, None))
            .collect();

        for step in 0..(20 * 60) {
            app.sim_step(dt);
            for (k, &i) in creatures.iter().enumerate() {
                let obj = &app.scene.objects[i];
                let pos = obj.transform.position;
                let (prev_pos, prev_yaw) = prev[k];
                let d_xz = (glam::Vec2::new(pos.x, pos.z)
                    - glam::Vec2::new(prev_pos.x, prev_pos.z))
                .length();
                assert!(
                    d_xz <= max_step,
                    "step {step} : « {} » a sauté de {d_xz:.3} m en une frame \
                     (max {max_step:.3}) — téléportation ({prev_pos:?} → {pos:?})",
                    obj.name
                );
                let (_, yaw, _) = obj.transform.rotation.to_euler(glam::EulerRot::XYZ);
                // Les 5 premières frames absorbent l'orientation initiale (un
                // script qui démarre pose son premier cap d'un coup, sans
                // historique — pas un défaut visible en jeu).
                if step >= 5
                    && obj.name != "Créature 18"
                    && let Some(py) = prev_yaw
                {
                    let mut delta = (yaw - py).to_degrees();
                    delta = ((delta + 180.0).rem_euclid(360.0)) - 180.0;
                    assert!(
                        delta.abs() < 25.0,
                        "step {step} : « {} » a pivoté de {delta:.1}° en une frame — \
                         demi-tour brutal",
                        obj.name
                    );
                }
                prev[k] = (pos, Some(yaw));
            }
        }
    }

    /// Preuve du correctif « les créatures partent toutes dans la même direction
    /// et restent collées au mur » : le script de patrouille était déterministe
    /// **et identique** pour toutes les instances (cap initial 0° pour toutes,
    /// bruit de méandre fonction du seul `time` global — cf. la doc de
    /// `creature_wander_script`, paramètres `heading0`/`phase`). Les 5 créatures
    /// avançaient en bloc vers +Z jusqu'au mur, où le braquage anti-mur, lui
    /// aussi identique, ne les décollait pas. Ce test rejoue 2 s du vrai script
    /// de production avec la vraie physique et échoue si les directions de
    /// déplacement des créatures restent groupées (écart angulaire maximal
    /// entre deux déplacements < 60°).
    #[test]
    fn mmorpg_creatures_do_not_all_walk_in_the_same_direction() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        // Créature 20 (tortue-canon) exclue : tourelle délibérément stationnaire
        // (`creature_turret_script`, cf. sa doc) — elle pivote sur place mais ne
        // se déplace jamais, ce qui est le comportement voulu, pas le bug que ce
        // test traque (des créatures *censées patrouiller* qui restent bloquées
        // ensemble contre un mur).
        let creature_indices: Vec<usize> = app
            .scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.name.starts_with("Créature") && o.name != "Créature 20")
            .map(|(i, _)| i)
            .collect();
        assert!(
            creature_indices.len() >= 2,
            "la démo MMORPG doit contenir plusieurs créatures (trouvé {})",
            creature_indices.len()
        );
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let spawns: Vec<Vec3> = creature_indices
            .iter()
            .map(|&i| app.scene.objects[i].transform.position)
            .collect();

        let dt = 1.0 / 60.0;
        for _ in 0..(2 * 60) {
            app.sim_step(dt);
        }

        // Direction de déplacement (XZ) de chaque créature depuis son spawn.
        let headings: Vec<glam::Vec2> = creature_indices
            .iter()
            .zip(&spawns)
            .map(|(&i, spawn)| {
                let pos = app.scene.objects[i].transform.position;
                let d = glam::Vec2::new(pos.x - spawn.x, pos.z - spawn.z);
                assert!(
                    d.length() > 0.3,
                    "après 2 s, « {} » doit s'être déplacée (déplacement={d:?})",
                    app.scene.objects[i].name
                );
                d.normalize()
            })
            .collect();

        let max_angle = headings
            .iter()
            .enumerate()
            .flat_map(|(a, ha)| headings[a + 1..].iter().map(move |hb| ha.angle_to(*hb)))
            .fold(0.0_f32, |acc, angle| acc.max(angle.abs()));
        assert!(
            max_angle.to_degrees() > 60.0,
            "les créatures partent toutes dans la même direction (écart angulaire \
             maximal entre deux déplacements : {:.1}°, attendu > 60°)",
            max_angle.to_degrees()
        );
    }

    /// Preuve de la demande gameplay « la Créature 1 doit avoir une attaque et la
    /// faire parfois » (`scene::demos::creature_bite_script`) : un contact
    /// **continu** de 20 s avec le joueur doit infliger au moins une morsure, mais
    /// pas à chaque frame — contrairement au pattern des dangers existants
    /// (`if obj.triggered then damage(dps*dt) end`, dégâts fractionnaires à
    /// chaque tick), l'attaque se déclenche par salves discrètes (~`BITE_DAMAGE`
    /// nets), espacées d'au moins `BITE_COOLDOWN`. Le joueur est réaligné sur la
    /// créature après chaque pas (elle patrouille toujours, cf. `creature_wander_
    /// script`) pour garantir un contact ininterrompu sans dépendre de la
    /// trajectoire réelle.
    #[test]
    fn creature_1_bites_the_player_sometimes_not_on_every_contact_tick() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let creature_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Créature")
            .expect("la démo MMORPG doit contenir une « Créature »");
        let player_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Joueur")
            .expect("la démo MMORPG doit contenir un « Joueur »");

        // Isole la Créature 1 : depuis les créatures 6-10, d'autres attaques
        // (morsures 6/7, tirs 3/8/9/10) peuvent toucher le joueur pendant les
        // 20 s de contact — leurs chutes de vie se cumuleraient à la morsure
        // mesurée ici et fausseraient l'assertion « une salve ≈ 0.115 ». Les
        // masquer suffit : un objet invisible n'est jamais `triggered` (cf. le
        // filtre `visible` de `sim_step`) et les attaques à distance ignorent
        // les créatures masquées (cf. `update_creature_ranged_attacks`).
        for obj in app.scene.objects.iter_mut() {
            if obj.name.starts_with("Créature ") {
                obj.visible = false;
            }
        }

        let start = app.scene.objects[creature_idx].transform.position;
        app.scene.objects[player_idx].transform.position = start;
        app.hud_health = Some(1.0);
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        app.physics
            .as_mut()
            .unwrap()
            .set_position(player_idx, start);

        let dt = 1.0 / 60.0;
        let mut bites = 0u32;
        let mut prev_health = app.hud_health.unwrap();
        for step in 0..(20 * 60) {
            app.sim_step(dt);
            let health = app
                .hud_health
                .expect("damage() doit faire apparaître la vie du HUD");
            if health < prev_health - 1e-4 {
                bites += 1;
                let drop = prev_health - health;
                assert!(
                    (0.08..0.13).contains(&drop),
                    "step {step} : chute de vie {drop:.3} inattendue (attendu ≈ 0.115, \
                     une salve nette moins la régénération passive du tick, pas une \
                     fraction continue par frame)"
                );
            }
            prev_health = health;

            // Contact permanent : replace le joueur exactement sur la créature
            // (qui a continué de patrouiller ce tick) avant le prochain pas.
            let pos = app.scene.objects[creature_idx].transform.position;
            app.physics.as_mut().unwrap().set_position(player_idx, pos);
            app.scene.objects[player_idx].transform.position = pos;
        }

        assert!(
            bites > 0,
            "20 s de contact continu avec la Créature 1 auraient dû déclencher \
             au moins une morsure"
        );
        assert!(
            bites < 20,
            "{bites} morsures en 20 s pour un cooldown de 2,2 s — l'attaque semble \
             se déclencher en continu plutôt que « parfois »"
        );
    }

    /// Contre-épreuve de portée : le contact seul ne suffit pas à mordre — sans
    /// contact (joueur loin), la vie ne doit jamais baisser malgré 20 s de
    /// simulation (aucune tolérance de flakiness possible ici, contrairement au
    /// test précédent : `obj.triggered` est structurellement faux tout du long).
    #[test]
    fn creature_1_never_bites_without_contact() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let player_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Joueur")
            .expect("la démo MMORPG doit contenir un « Joueur »");
        // Loin de toute créature/mur (cf. les spawns `Vec3::new(±3.0, 0.0, ±3.0)`
        // et le pourtour à `half = 12.0` dans `Scene::mmorpg_demo`).
        app.scene.objects[player_idx].transform.position = Vec3::new(0.0, 1.0, 9.0);
        app.hud_health = Some(1.0);
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let dt = 1.0 / 60.0;
        for _ in 0..(20 * 60) {
            app.sim_step(dt);
        }
        assert_eq!(
            app.hud_health,
            Some(1.0),
            "sans contact, la vie ne doit jamais baisser (aucune créature ne mord à distance)"
        );
    }

    /// Verrouille la répartition des attaques par créature : au contact
    /// (`creature_bite_script`, script Lua + `trigger`) pour les n°1 (morsure),
    /// 6 (chauve-souris) et 7 (crabe) ; à distance (natif, par nom — cf.
    /// `creature_attack::RANGED_CREATURE_ATTACKS`) pour les n°3, 8, 9 et 10 ;
    /// et rien du tout pour les pacifiques n°2, 4 et 5. Vérifié statiquement
    /// sur les scripts (pas d'appel `damage(`) plutôt qu'en rejouant une scène
    /// de contact par créature — plus rapide, tout aussi précis.
    #[test]
    fn creature_attacks_are_scoped_to_the_intended_creatures() {
        let scene = crate::scene::Scene::mmorpg_demo();
        let by_name = |name: &str| {
            scene
                .objects
                .iter()
                .find(|o| o.name == name)
                .unwrap_or_else(|| panic!("la démo MMORPG doit contenir « {name} »"))
        };
        for name in ["Créature", "Créature 6", "Créature 7"] {
            let obj = by_name(name);
            assert!(
                obj.script.contains("damage("),
                "« {name} » devrait avoir une attaque au contact (cf. creature_bite_script)"
            );
            assert!(
                obj.trigger,
                "« {name} » doit avoir `trigger = true` pour que `obj.triggered` \
                 fonctionne dans son script d'attaque"
            );
        }
        for name in [
            "Créature 2",
            "Créature 3",
            "Créature 4",
            "Créature 5",
            "Créature 8",
            "Créature 9",
            "Créature 10",
            "Créature 11",
            "Créature 12",
            "Créature 13",
            "Créature 14",
            "Créature 15",
            "Créature 16",
            "Créature 17",
            "Créature 18",
            "Créature 19",
            "Créature 20",
        ] {
            let obj = by_name(name);
            assert!(
                !obj.script.contains("damage("),
                "« {name} » ne devrait pas attaquer via son script (script : {:?})",
                obj.script
            );
        }
        // Les attaques à distance sont natives, déclenchées par nom : chaque
        // créature de la table doit exister dans la démo (une entrée orpheline
        // serait une attaque silencieusement morte).
        for cfg_name in ["Créature 3", "Créature 8", "Créature 9", "Créature 10"] {
            by_name(cfg_name);
        }
    }

    /// Preuve (bug observé en jeu sur la créature MMORPG : « les bras et la tête
    /// partent en couille dès qu'elle tourne », silhouette dédoublée) : un script qui
    /// ne réécrit que `obj.ry` doit produire un cap **stable** d'un tick à l'autre,
    /// y compris au-delà de ±90°. Avant le correctif (`scripting::
    /// canonical_euler_xyz`), `to_euler(XYZ)` représentait un yaw de -117° comme
    /// (rx=180°, ry=-63°, rz=180°) ; le script écrasait `ry` seul et la
    /// recomposition gardait les flips ±180° de rx/rz → la rotation alternait entre
    /// -117° et -63° un tick sur deux (écart 2×(117−90) = 54°, jusqu'à 180° plein
    /// sud) — invisible en marche vers le « nord » (|cap| < 90°, aucun flip), d'où
    /// le symptôme « en ligne droite ça va, dès qu'il tourne ça casse ».
    #[test]
    fn script_rewriting_only_ry_keeps_a_stable_heading_beyond_90_degrees() {
        for target in [-179.0f32, -117.0, -95.0, 95.0, 150.0, 179.0] {
            let mut app = AppState::new();
            app.scene.objects.clear();
            app.scene.objects.push(SceneObject {
                script: format!("obj.ry = {target}"),
                ..Default::default()
            });
            let dt = 1.0 / 60.0;
            for tick in 0..6 {
                app.sim_step(dt);
                // Yaw lu en YXZ (yaw en premier : plage complète ±180°, pas de
                // représentation à flips comme le XYZ contraint à ±90° au milieu).
                let (yaw, _, _) = app.scene.objects[0]
                    .transform
                    .rotation
                    .to_euler(glam::EulerRot::YXZ);
                let mut diff = yaw.to_degrees() - target;
                diff = ((diff + 180.0).rem_euclid(360.0)) - 180.0;
                assert!(
                    diff.abs() < 0.01,
                    "tick {tick} : cap affiché {:.2}° pour obj.ry = {target}° — \
                     le cap doit rester exactement celui écrit par le script, \
                     sans alternance d'un tick à l'autre",
                    yaw.to_degrees()
                );
            }
        }
    }

    /// Sprint 111 (hot-reload) : `script_cache` est clé par hash du **contenu** du
    /// script (`scripting::script_key`), pas par identité d'objet — retoucher le
    /// texte d'un script en cours de Play (panneau « Scripts », ou IA) doit donc
    /// prendre effet dès le tick suivant, sans repasser par Stop/Play. Même principe
    /// que les textures, cf. `gfx::renderer::tests::invalidate_asset_textures_
    /// forces_a_reload_from_disk_on_the_next_sync` — mais ici aucune invalidation
    /// n'est nécessaire : la clé change d'elle-même avec le texte.
    #[test]
    fn editing_an_objects_script_mid_play_takes_effect_on_the_next_tick_without_restarting_play() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject {
            script: "obj.x = 1".into(),
            ..Default::default()
        });
        app.sim_step(0.1);
        assert_eq!(app.scene.objects[0].transform.position.x, 1.0);

        app.scene.objects[0].script = "obj.x = 2".into();
        app.sim_step(0.1);
        assert_eq!(
            app.scene.objects[0].transform.position.x, 2.0,
            "le nouveau texte du script doit s'appliquer dès le tick suivant, sans redémarrer Play"
        );
    }

    #[test]
    fn sim_step_advances_a_crossfade_towards_completion_and_stops() {
        use crate::scene::AnimationState;
        let mut app = AppState::new();
        app.scene.objects.clear();
        let mut anim = AnimationState {
            clip: "Idle".into(),
            ..Default::default()
        };
        assert_eq!(anim.blend, 1.0, "pas de transition en cours au départ");
        anim.set_clip("Run"); // démarre le fondu
        assert_eq!(anim.blend, 0.0);
        assert_eq!(anim.prev_clip, "Idle");
        app.scene.objects.push(SceneObject {
            animation: Some(anim),
            ..Default::default()
        });

        // CROSSFADE_SECONDS = 0.2s : un pas de 0.1s doit avancer blend à ~0.5, pas plus.
        app.sim_step(0.1);
        let anim = app.scene.objects[0].animation.as_ref().unwrap();
        assert!(
            (anim.blend - 0.5).abs() < 1e-4,
            "blend attendu ≈0.5 après 0.1s de fondu (durée 0.2s), obtenu {}",
            anim.blend
        );
        assert!(
            anim.prev_time > 0.0,
            "le clip quitté doit continuer d'avancer pendant le fondu"
        );

        // Encore 0.2s (au-delà de la durée du fondu) : blend clampé à 1.0, jamais au-delà.
        app.sim_step(0.2);
        let anim = app.scene.objects[0].animation.as_ref().unwrap();
        assert_eq!(anim.blend, 1.0, "blend ne doit jamais dépasser 1.0");

        // Transition terminée : encore un pas, prev_time ne doit plus avancer.
        let prev_time_after = anim.prev_time;
        app.sim_step(0.1);
        assert_eq!(
            app.scene.objects[0].animation.as_ref().unwrap().prev_time,
            prev_time_after,
            "prev_time ne doit plus bouger une fois la transition terminée"
        );
    }

    #[test]
    fn tank_controls_turn_then_thrust_move_the_player_along_its_own_facing() {
        // Bout en bout : A/D (rotation manuelle) et W/S (avance/recul) doivent piloter le
        // joueur indépendamment de la caméra, contrairement au joystick/flèches
        // (contrôles « tank »).
        let mut app = AppState::new();
        app.load_controller_demo();
        app.playing = true;
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("la démo contrôleur a un joueur pilotable");

        // D tenue (tourner à gauche, cf. doc `PlayerInput::key_turn`) : le yaw doit
        // augmenter par rapport à sa valeur de départ (0). Peu de pas : avec
        // `MANUAL_TURN_SPEED` (3 rad/s), rester bien en-deçà de π pour ne pas
        // « boucler » et fausser la lecture (`to_scaled_axis` ramène l'angle dans
        // (-π, π]).
        app.input_state.key_turn = 1.0;
        for _ in 0..5 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
            app.advance_play();
        }
        app.input_state.key_turn = 0.0;
        let yaw = app.scene.objects[pi]
            .transform
            .rotation
            .to_euler(EulerRot::YXZ)
            .0;
        assert!(
            yaw > 0.1,
            "D doit tourner le joueur vers la gauche, yaw={yaw}"
        );

        // Puis W tenue : le joueur doit avancer le long de cette orientation, pas vers
        // le -Z monde qu'utiliserait un déplacement caméra-relative.
        let p0 = app.scene.objects[pi].transform.position;
        app.input_state.key_thrust = 1.0;
        for _ in 0..30 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
            app.advance_play();
        }
        let moved = app.scene.objects[pi].transform.position - p0;
        let expected_dir = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        assert!(
            moved.length() > 0.3,
            "W doit faire avancer le joueur, déplacement={moved:?}"
        );
        assert!(
            moved.normalize().dot(expected_dir) > 0.8,
            "l'avance doit suivre l'orientation du joueur (yaw={yaw}), pas la caméra : \
             déplacement={moved:?}, attendu≈{expected_dir:?}"
        );
    }

    #[test]
    fn tank_controls_reversing_never_spins_the_player_around() {
        // Garde-fou : l'orientation doit rester fixe pendant S (recul), pas se
        // remettre à tourner vers le vecteur de vitesse (cf. docs/audits/app-mod.md).
        let mut app = AppState::new();
        app.load_controller_demo();
        app.playing = true;
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("la démo contrôleur a un joueur pilotable");
        let yaw0 = app.scene.objects[pi]
            .transform
            .rotation
            .to_euler(EulerRot::YXZ)
            .0;

        app.input_state.key_thrust = -1.0; // S tenue
        for _ in 0..90 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
            app.advance_play();
        }
        let yaw1 = app.scene.objects[pi]
            .transform
            .rotation
            .to_euler(EulerRot::YXZ)
            .0;
        assert!(
            (yaw1 - yaw0).abs() < 1e-3,
            "reculer (S) ne doit jamais faire tourner le personnage : yaw0={yaw0}, yaw1={yaw1}"
        );
    }

    /// Garde-fou du piège respawn + PV : un ennemi à plusieurs PV et
    /// `respawn_delay > 0` doit revenir avec ses PV d'origine, pas avec les 0 PV
    /// où il les a laissés (sinon il réapparaît « déjà vaincu » : re-masqué au
    /// premier coup, sans jamais encaisser sa barre de vie). Cf. `Combat::max_hp`
    /// (capture au premier coup dans `Scene::damage_attackable_by`) et
    /// `process_respawns` (restauration).
    #[test]
    fn a_respawning_enemy_comes_back_with_its_original_hp() {
        let mut app = AppState::new();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Brute".into(),
            combat: Some(crate::scene::Combat {
                attackable: true,
                hp: 3,
                ..Default::default()
            }),
            respawn_delay: 2.0,
            ..Default::default()
        });
        let i = app.scene.objects.len() - 1;

        // Trois coups pour le vaincre : les deux premiers blessent, le troisième
        // l'achève (masqué) — mise en file de respawn comme le fait `update_attack`
        // (cf. `app::combat`) au moment de la mise à mort.
        assert!(!app.scene.damage_attackable(i));
        assert!(!app.scene.damage_attackable(i));
        assert!(app.scene.damage_attackable(i), "3e coup = mise à mort");
        assert!(!app.scene.objects[i].visible, "vaincu ⇒ masqué");
        let delay = app.scene.objects[i].respawn_delay;
        app.respawn_queue.push((i, app.time + delay));

        // Délai non écoulé : rien ne bouge.
        app.process_respawns(app.time + delay * 0.5);
        assert!(!app.scene.objects[i].visible);

        // Délai écoulé : il réapparaît avec ses 3 PV d'origine…
        app.process_respawns(app.time + delay);
        assert!(app.scene.objects[i].visible, "délai écoulé ⇒ réapparu");
        assert_eq!(
            app.scene.objects[i].combat.as_ref().unwrap().hp,
            3,
            "le respawn doit restaurer les PV d'origine, pas laisser 0"
        );
        // …et redevient un adversaire entier : un coup le blesse sans le vaincre.
        assert!(
            !app.scene.damage_attackable(i),
            "après respawn, un seul coup ne doit plus suffire"
        );
        assert!(app.scene.objects[i].visible);
    }
}
