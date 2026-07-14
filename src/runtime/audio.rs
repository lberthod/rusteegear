//! Audio simple via kira : décodage en thread de fond + cache pour éviter tout lag.
//!
//! wasm32 (Sprint 115) : `kira` supporte nativement ce target (backend `cpal`
//! "wasm-bindgen", Web Audio sous le capot) — la quasi-totalité de ce module est
//! donc partagée entre plateformes sans `#[cfg]`. Deux exceptions structurelles,
//! pas de simple différence de comportement :
//! - **Musique en flux** (`play_music_streaming_gain`) : `kira::sound::streaming`
//!   s'exclut lui-même de `wasm32-unknown-unknown` (ouvre un vrai descripteur de
//!   fichier, absent du navigateur) — stub côté web, cf. sa doc plus bas.
//! - **Décodage en fond d'un chemin hors asset connu** (`play_gain`, branche
//!   `std::thread::spawn`) : pas de threads OS sur `wasm32-unknown-unknown` sans
//!   configuration spécifique (workers + atomics, hors scope) ; de toute façon pas
//!   de système de fichiers réel pour `StaticSoundData::from_file` sur le web —
//!   remplacé par un simple message d'erreur, cf. `play_gain`.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use glam::Vec3;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};

#[cfg(not(target_arch = "wasm32"))]
use kira::Panning;
#[cfg(not(target_arch = "wasm32"))]
use kira::sound::FromFileError;
#[cfg(not(target_arch = "wasm32"))]
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};

/// Convertit un gain linéaire (0..1) en décibels (kira). 0 → quasi-silence.
fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.001 {
        -60.0
    } else {
        20.0 * gain.log10()
    }
}

/// Panning stéréo (-1 = gauche, 0 = centre, 1 = droite) d'une source vue
/// depuis la caméra (Sprint 104) : projette le vecteur caméra→source sur
/// l'axe « droite » de la caméra (dérivé de son couple œil/cible, pas du
/// yaw brut — évite toute hypothèse de signe sur la convention de rotation).
/// Fonction pure, testable sans `AudioManager` — même esprit que `gain_to_db`.
pub fn camera_panning(eye: Vec3, target: Vec3, source: Vec3) -> f32 {
    let forward = target - eye;
    let Some(forward) = forward.try_normalize() else {
        return 0.0;
    };
    let right = forward.cross(Vec3::Y);
    let Some(right) = right.try_normalize() else {
        return 0.0;
    };
    let Some(to_source) = (source - eye).try_normalize() else {
        return 0.0;
    };
    to_source.dot(right).clamp(-1.0, 1.0)
}

/// Piste kira de destination d'un son (Sprint 104) : sépare musique/ambiance
/// (fichiers réels, potentiellement longs) des effets sonores synthétisés
/// (`sfx.rs`), pour un réglage de volume indépendant des deux (cf.
/// `Audio::set_music_volume`/`set_sfx_volume`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Track {
    Music,
    Sfx,
}

/// Handles des sons **en flux** en cours de lecture — `Vec` côté natif, absent
/// côté web (cf. la doc de `play_music_streaming_gain`). Un alias plutôt qu'un
/// deuxième `struct Audio` complet : c'est le seul champ dont le *type* diffère
/// entre les deux cibles, tout le reste du code est partagé tel quel.
#[cfg(not(target_arch = "wasm32"))]
type StreamingHandles = Vec<StreamingSoundHandle<FromFileError>>;
#[cfg(target_arch = "wasm32")]
type StreamingHandles = ();

pub struct Audio {
    /// Jamais relu après `new()` (tout passe désormais par `music_track`/
    /// `sfx_track`, Sprint 104) mais doit rester en vie : kira arrête la
    /// sortie audio dès que `AudioManager` est droppé.
    #[allow(dead_code)]
    manager: Option<AudioManager>,
    /// Piste musique/ambiance (fichiers réels, `play`/`play_gain`/
    /// `play_music_streaming_gain`) — `None` si `manager` est `None`.
    music_track: Option<TrackHandle>,
    /// Piste effets sonores (`play_bytes`, sons synthétisés de `sfx.rs`).
    sfx_track: Option<TrackHandle>,
    playing: Vec<StaticSoundHandle>,
    /// Sons **en flux** en cours de lecture (Sprint 104, `StreamingSoundData`) :
    /// type de handle distinct de `StaticSoundHandle`, pas de décodage complet
    /// en mémoire — évite le pic mémoire d'une musique longue entièrement
    /// décodée à l'avance. Absent sur wasm32, cf. `StreamingHandles`.
    streaming_playing: StreamingHandles,
    /// Sons déjà décodés (réutilisés sans re-décoder).
    cache: HashMap<String, StaticSoundData>,
    /// Chemins demandés mais pas encore décodés (avec leur gain), à jouer dès l'arrivée.
    pending: HashMap<String, f32>,
    tx: Sender<(String, StaticSoundData)>,
    rx: Receiver<(String, StaticSoundData)>,
}

impl Audio {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let mut manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
        {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("Audio indisponible : {e}");
                None
            }
        };
        // Deux pistes séparées (Sprint 104) : volume musique/SFX réglable
        // indépendamment (cf. `set_music_volume`/`set_sfx_volume`), sans quoi
        // tout passait par l'unique piste principale de kira. `add_sub_track`
        // n'échoue qu'en cas de limite de ressources atteinte (quelques
        // pistes sur des dizaines de milliers de possibles) — improbable ici,
        // mais on reste tolérant (`None` = repli silencieux sur l'absence
        // d'atténuation par piste, comme `manager` lui-même).
        let music_track = manager
            .as_mut()
            .and_then(|m| m.add_sub_track(TrackBuilder::new()).ok());
        let sfx_track = manager
            .as_mut()
            .and_then(|m| m.add_sub_track(TrackBuilder::new()).ok());
        Audio {
            manager,
            music_track,
            sfx_track,
            playing: Vec::new(),
            streaming_playing: StreamingHandles::default(),
            cache: HashMap::new(),
            pending: HashMap::new(),
            tx,
            rx,
        }
    }

    /// Joue un fichier à plein volume.
    pub fn play(&mut self, path: &str) {
        self.play_gain(path, 1.0);
    }

    /// Joue un fichier avec un gain (0..1) — utilisé pour l'atténuation spatiale.
    /// Instantané si en cache, sinon décodage en fond puis lecture à l'arrivée
    /// (desktop/mobile uniquement, cf. plus bas — sur le web, un asset se résout
    /// toujours via le chemin `is_known_scheme` juste au-dessus).
    pub fn play_gain(&mut self, path: &str, gain: f32) {
        if let Some(data) = self.cache.get(path).cloned() {
            self.start(data, gain);
            return;
        }
        // Asset embarqué/projet : décodage immédiat depuis la mémoire.
        if crate::assets::is_known_scheme(path) {
            match crate::assets::read_bytes(path) {
                Some(bytes) => match StaticSoundData::from_cursor(std::io::Cursor::new(bytes)) {
                    Ok(data) => {
                        self.cache.insert(path.to_string(), data.clone());
                        self.start(data, gain);
                    }
                    Err(e) => log::error!("Son '{path}' illisible : {e}"),
                },
                None => log::error!("Son introuvable : {path}"),
            }
            return;
        }
        // Chemin de fichier arbitraire (hors asset connu) : décodage en fond sur
        // desktop/mobile. Sur le web, il n'existe ni système de fichiers réel ni
        // thread OS accessible sans configuration spéciale (workers + atomics,
        // hors scope) — ce chemin n'est de toute façon jamais emprunté en pratique
        // par le player exporté (tous ses sons sont des assets embarqués).
        #[cfg(not(target_arch = "wasm32"))]
        if !self.pending.contains_key(path) {
            self.pending.insert(path.to_string(), gain);
            let tx = self.tx.clone();
            let p = path.to_string();
            std::thread::spawn(move || match StaticSoundData::from_file(&p) {
                Ok(data) => {
                    let _ = tx.send((p, data));
                }
                Err(e) => log::error!("Chargement audio '{p}' échoué : {e}"),
            });
        }
        #[cfg(target_arch = "wasm32")]
        log::error!(
            "Son '{path}' introuvable (pas un asset embarqué connu, pas de fichiers sur le web)"
        );
    }

    /// Joue un fichier de musique/ambiance **en flux** (Sprint 104,
    /// `StreamingSoundData`) plutôt que décodé en une fois en mémoire — cf.
    /// livrable « musique longue sans pic mémoire ». `panning` (-1..1, cf.
    /// `camera_panning`) positionne la source dans le champ stéréo. Pas de
    /// cache ni de décodage en fond ici : construire un `StreamingSoundData`
    /// n'ouvre qu'un décodeur (lecture de l'en-tête), pas un décodage complet
    /// — contrairement à `StaticSoundData::from_file`, l'appel reste léger et
    /// synchrone. Pas de réutilisation en cache possible non plus (un flux
    /// porte un état de lecture, pas `Clone`) — sans conséquence : une
    /// musique/ambiance de scène (`AudioSource`) se déclenche une fois à
    /// l'entrée en Play, jamais rejouée depuis un cache.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn play_music_streaming_gain(&mut self, path: &str, gain: f32, panning: f32) {
        let Some(track) = self.music_track.as_mut() else {
            return;
        };
        let panning = Panning(panning.clamp(-1.0, 1.0));
        let volume = Decibels(gain_to_db(gain));
        let data = if crate::assets::is_known_scheme(path) {
            match crate::assets::read_bytes(path) {
                Some(bytes) => StreamingSoundData::from_cursor(std::io::Cursor::new(bytes))
                    .map(|d| d.volume(volume).panning(panning)),
                None => {
                    log::error!("Son introuvable : {path}");
                    return;
                }
            }
        } else {
            StreamingSoundData::from_file(path).map(|d| d.volume(volume).panning(panning))
        };
        match data {
            Ok(data) => match track.play(data) {
                Ok(handle) => self.streaming_playing.push(handle),
                Err(e) => log::error!("Lecture audio (flux) échouée : {e}"),
            },
            Err(e) => log::error!("Son (flux) '{path}' illisible : {e}"),
        }
    }

    /// wasm32 : `kira::sound::streaming` n'existe pas pour ce target (ouvre un
    /// vrai descripteur de fichier, cf. la doc en tête de module) — musique/
    /// ambiance en flux indisponible sur le web pour l'instant. Un objet
    /// `AudioSource` de scène qui en dépend reste silencieux plutôt que de faire
    /// planter la compilation ; ré-implémentable plus tard en `StaticSoundData`
    /// chargée entière (accepter le pic mémoire) si le besoin se confirme.
    #[cfg(target_arch = "wasm32")]
    pub fn play_music_streaming_gain(&mut self, path: &str, _gain: f32, _panning: f32) {
        log::warn!("Musique/ambiance en flux indisponible sur le web : {path}");
    }

    /// Joue un son **généré en mémoire** (WAV synthétisé), mis en cache sous `key`.
    /// Sert aux effets sonores du jeu (ramassage, saut, victoire, défaite).
    /// `gain`/`playback_rate` (Sprint 108, randomisation) s'appliquent à la
    /// **lecture**, pas au contenu mis en cache : `bytes` n'est décodé qu'au
    /// premier appel pour une `key` donnée (cf. le cache ci-dessous) — faire
    /// varier le contenu resynthétisé à chaque appel n'aurait donc aucun
    /// effet après le premier ; varier gain/débit de lecture, si.
    /// `playback_rate` (1.0 = normal) modifie aussi la hauteur perçue du
    /// son, technique standard pour un effet procédural bon marché.
    pub fn play_bytes(&mut self, key: &str, bytes: &[u8], gain: f32, playback_rate: f32) {
        let data = if let Some(cached) = self.cache.get(key).cloned() {
            cached
        } else {
            match StaticSoundData::from_cursor(std::io::Cursor::new(bytes.to_vec())) {
                Ok(data) => {
                    self.cache.insert(key.to_string(), data.clone());
                    data
                }
                Err(e) => {
                    log::error!("SFX '{key}' illisible : {e}");
                    return;
                }
            }
        };
        let data = data.playback_rate(playback_rate as f64);
        self.start_on(data, gain, Track::Sfx);
    }

    /// À appeler chaque frame : récupère les sons décodés et joue ceux en attente.
    pub fn update(&mut self) {
        while let Ok((path, data)) = self.rx.try_recv() {
            self.cache.insert(path.clone(), data.clone());
            if let Some(gain) = self.pending.remove(&path) {
                self.start(data, gain);
            }
        }
    }

    /// Piste musique — utilisé par `play`/`play_gain`/`update` (fichiers réels).
    fn start(&mut self, data: StaticSoundData, gain: f32) {
        self.start_on(data, gain, Track::Music);
    }

    fn start_on(&mut self, data: StaticSoundData, gain: f32, track: Track) {
        let handle = match track {
            Track::Music => self.music_track.as_mut(),
            Track::Sfx => self.sfx_track.as_mut(),
        };
        if let Some(track) = handle {
            let data = data.volume(Decibels(gain_to_db(gain)));
            match track.play(data) {
                Ok(handle) => self.playing.push(handle),
                Err(e) => log::error!("Lecture audio échouée : {e}"),
            }
        }
    }

    /// Volume (0..1) de la piste musique/ambiance (Sprint 104, persisté dans
    /// `Settings::music_volume`) — s'applique en direct à tous les sons déjà
    /// en cours sur cette piste, sans avoir à les rejouer.
    pub fn set_music_volume(&mut self, v: f32) {
        if let Some(track) = self.music_track.as_mut() {
            track.set_volume(Decibels(gain_to_db(v.clamp(0.0, 1.0))), Tween::default());
        }
    }

    /// Volume (0..1) de la piste effets sonores (Sprint 104, persisté dans
    /// `Settings::sfx_volume`).
    pub fn set_sfx_volume(&mut self, v: f32) {
        if let Some(track) = self.sfx_track.as_mut() {
            track.set_volume(Decibels(gain_to_db(v.clamp(0.0, 1.0))), Tween::default());
        }
    }

    /// Arrête tous les sons en cours (sortie du mode Play).
    pub fn stop_all(&mut self) {
        for handle in &mut self.playing {
            handle.stop(Tween::default());
        }
        self.playing.clear();
        #[cfg(not(target_arch = "wasm32"))]
        {
            for handle in &mut self.streaming_playing {
                handle.stop(Tween::default());
            }
            self.streaming_playing.clear();
        }
        self.pending.clear();
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_panning_is_zero_straight_ahead_and_behind() {
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let target = Vec3::ZERO;
        assert!(
            camera_panning(eye, target, Vec3::ZERO).abs() < 1e-5,
            "une source pile sur la cible (droit devant) doit être centrée"
        );
        assert!(
            camera_panning(eye, target, Vec3::new(0.0, 0.0, 10.0)).abs() < 1e-5,
            "une source pile derrière la caméra, sur le même axe, reste centrée"
        );
    }

    #[test]
    fn camera_panning_is_maxed_to_each_side() {
        // Caméra en (0,0,5) regardant vers l'origine (-Z) : l'axe « droite »
        // pointe vers +X (forward=-Z, right=forward×Y=(-Z)×Y=+X, base directe).
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let target = Vec3::ZERO;
        let right = camera_panning(eye, target, eye + Vec3::new(1.0, 0.0, 0.0));
        let left = camera_panning(eye, target, eye + Vec3::new(-1.0, 0.0, 0.0));
        assert!(
            right > 0.9,
            "une source loin sur la droite doit paniquer à droite (right={right})"
        );
        assert!(
            left < -0.9,
            "une source loin sur la gauche doit paniquer à gauche (left={left})"
        );
    }

    #[test]
    fn volume_setters_never_panic_regardless_of_manager_availability() {
        // `AudioManager::new()` peut échouer (pas de matériel audio, CI/
        // sandbox) — `Audio` doit rester utilisable sans jamais paniquer,
        // que le manager (et donc les pistes) existent ou non.
        let mut audio = Audio::new();
        audio.set_music_volume(0.5);
        audio.set_sfx_volume(0.0);
        audio.play_music_streaming_gain("chemin/inexistant.mp3", 0.5, 0.0);
        audio.stop_all();
    }

    /// Sprint 108 : `play_bytes` accepte un gain/débit de lecture différents
    /// de 1.0 (variation aléatoire des effets sonores) sans jamais paniquer,
    /// que l'`AudioManager` soit disponible ou non (même esprit que le test
    /// ci-dessus). Passe par `sfx::play` (un vrai `Sfx`) plutôt qu'un WAV
    /// factice, et l'appelle deux fois pour exercer les deux chemins
    /// (décodage au premier appel, cache ensuite).
    #[test]
    fn play_bytes_with_varied_gain_and_pitch_never_panics() {
        let mut audio = Audio::new();
        crate::runtime::sfx::play(&mut audio, crate::runtime::sfx::Sfx::Jump);
        crate::runtime::sfx::play(&mut audio, crate::runtime::sfx::Sfx::Jump);
        audio.stop_all();
    }
}
