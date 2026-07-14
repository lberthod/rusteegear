//! Boucle de simulation (Sprint 105a-1, extrait de `app/mod.rs` — code
//! inchangé, seulement déplacé) : `advance_play`/`sim_step`, accumulateur à
//! pas fixe, interpolation de poses de rendu.

use glam::{EulerRot, Quat, Vec3};
use std::collections::HashMap;

use crate::time_compat::Instant;

#[cfg(not(target_arch = "wasm32"))]
use super::scripting;
use super::{AppState, multiplayer};

/// Angle de plongée (radians) de la caméra de suivi par défaut : resserré derrière
/// l'épaule du personnage plutôt que le recul plus « isométrique » d'avant (~35°,
/// `0.62`) — plus proche d'une vue façon jeu d'action à la troisième personne.
pub(super) const DEFAULT_CHASE_PITCH: f32 = 0.75;

/// Recul (mètres) de la caméra de suivi par défaut : plus proche que l'ancien 11.0,
/// pour un cadrage plus serré façon caméra d'épaule.
pub(super) const DEFAULT_CHASE_DISTANCE: f32 = 7.0;

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

impl AppState {
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
                    Some((a.clip.clone(), gain, panning))
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
                self.camera.target = p + Vec3::new(0.0, 0.8, 0.0);
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
            self.lua_vars.clear();
            self.respawn_queue.clear();
            self.time = 0.0;
            // Relit la qualité visée (modifiable dans le panneau Export sans redémarrer
            // l'app) : s'applique dès ce lancement de Play, pas seulement au build exporté.
            let cfg = crate::app::build_config::BuildConfig::load();
            self.render_quality = cfg.render_quality;
            self.bloom_enabled = cfg.bloom;
        }
        self.was_playing = self.playing;

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
            self.update_attack(dt);
            self.update_network_attacks(dt);
            self.update_fireballs(dt);
            // Vie individualisée des joueurs réseau (contact monstre, régénération
            // hors combat) puis soin coopératif — après les dégâts de ce tick, pour
            // qu'un soin ne soit pas aussitôt annulé par un contact déjà résolu
            // (cf. GAMEDESIGN_EN_LIGNE.md §3.1/§3.6).
            self.update_network_health(dt);
            self.update_network_heal(dt);
            // Réapparition des pièces bonus dont le délai est écoulé.
            let now = self.time;
            self.respawn_queue.retain(|&(i, at)| {
                if now >= at {
                    if let Some(o) = self.scene.objects.get_mut(i) {
                        o.visible = true;
                    }
                    false
                } else {
                    true
                }
            });
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
            // fois la courante vidée, ou déclenche la victoire à la dernière. N'a aucun
            // effet si la scène n'a pas de monstres à manches (self.wave == 0).
            self.update_waves();
        }

        // Position réseau du joueur local : appliquée *après* la physique (cf. sa
        // doc) pour ne pas être aussitôt écrasée par `sim_step`, qui recalculerait
        // sinon une position légèrement différente à partir de l'ancienne.
        self.apply_local_network_position();

        // Caméra qui suit le joueur — au niveau frame (lissage visuel), avec le dt réel.
        // Cible légèrement au-dessus du joueur (regarde le buste, voit plus loin devant).
        if self.scene.camera_follow
            && let Some(p) = self.player_position()
        {
            // Forme exponentielle `1 - e^(-k·dt)` plutôt que `k·dt` borné : le taux de
            // rattrapage devient indépendant du framerate (deux frames à 120 Hz lissent
            // exactement comme une à 60 Hz), là où la forme linéaire sur-amortissait à
            // bas FPS et créait de micro-à-coups de caméra sous gigue de frame.
            let t = 1.0 - (-dt * 6.0).exp();
            self.camera.target = self.camera.target.lerp(p + Vec3::new(0.0, 0.8, 0.0), t);
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
        }
        // Décroissance du flash de dégâts (~0,4 s), au niveau frame comme la caméra.
        if self.damage_flash > 0.0 {
            self.damage_flash = (self.damage_flash - dt * 2.5).max(0.0);
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
            // Scripting Lua indisponible sur wasm32 (`mlua`/`lua-src` ne ciblent pas
            // `wasm32-unknown-unknown`, cf. Cargo.toml et Sprint 114) : un objet
            // scripté reste inerte sur le web plutôt que de faire échouer la
            // compilation — le reste de la boucle (tap, collectibles, physique) est
            // inchangé au-dessus de ce bloc.
            #[cfg(target_arch = "wasm32")]
            continue;
            #[cfg(not(target_arch = "wasm32"))]
            {
                // Récupère (ou compile une seule fois) le chunk associé à cette source.
                let key = scripting::script_key(&obj.script);
                let func = match self.script_cache.get(&key) {
                    Some(f) => f.clone(),
                    None => match self.lua.load(&obj.script).into_function() {
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

        // 2. physique (écrase les poses des corps dynamiques)
        // Cibles de poursuite pour l'IA (`AiChaser`, cf. plus bas) : en solo, le
        // seul joueur local ; en réseau, **chaque** joueur réseau **vivant**
        // (GAMEDESIGN_EN_LIGNE.md §3.2 — avant ce correctif, un monstre ne
        // poursuivait jamais que le premier joueur à avoir rejoint, `self.
        // player_position()` désignant sur le serveur headless le premier objet
        // visible piloté trouvé, donc le joueur 1, jamais le 2e+ quelle que soit
        // sa proximité). `player_position()` reste utilisé tel quel en solo (pas
        // de joueurs réseau) : aucun changement de comportement pour ce cas.
        let candidate_targets: Vec<Vec3> = if self.network_players.is_empty() {
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
                for (idx, obj) in self.scene.objects.iter().enumerate() {
                    // Un monstre vaincu (invisible) ou d'une manche pas encore révélée
                    // ne poursuit pas (et n'a de toute façon pas de corps physique tant
                    // qu'il est masqué, cf. le filtre `visible` dans `Physics::build`).
                    if obj.ai_chaser.is_none() || !obj.visible {
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
                    by_target.entry(target_i).or_default().push((idx, dist_sq));
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
                        let speed = self.scene.objects[idx]
                            .ai_chaser
                            .as_ref()
                            .expect("filtré ci-dessus : cet objet a un ai_chaser")
                            .speed;
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
            phys.step(dt, &mut self.scene);
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

    #[test]
    fn sim_step_leaves_objects_without_animation_untouched() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject::default());
        app.sim_step(0.1);
        assert!(app.scene.objects[0].animation.is_none());
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
}
