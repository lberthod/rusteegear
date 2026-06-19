//! Audio simple via kira : décodage en thread de fond + cache pour éviter tout lag.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};

/// Convertit un gain linéaire (0..1) en décibels (kira). 0 → quasi-silence.
fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.001 {
        -60.0
    } else {
        20.0 * gain.log10()
    }
}

pub struct Audio {
    manager: Option<AudioManager>,
    playing: Vec<StaticSoundHandle>,
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
        let manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("Audio indisponible : {e}");
                None
            }
        };
        Audio {
            manager,
            playing: Vec::new(),
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
    /// Instantané si en cache, sinon décodage en fond puis lecture à l'arrivée.
    pub fn play_gain(&mut self, path: &str, gain: f32) {
        if let Some(data) = self.cache.get(path).cloned() {
            self.start(data, gain);
            return;
        }
        // Asset embarqué/projet : décodage immédiat depuis la mémoire.
        if path.starts_with(crate::assets::SCHEME) || path.starts_with(crate::assets::ASSET_SCHEME)
        {
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
        // pas encore décodé : lancer un décodage en arrière-plan (une seule fois)
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

    fn start(&mut self, data: StaticSoundData, gain: f32) {
        if let Some(m) = self.manager.as_mut() {
            let data = data.volume(Decibels(gain_to_db(gain)));
            match m.play(data) {
                Ok(handle) => self.playing.push(handle),
                Err(e) => log::error!("Lecture audio échouée : {e}"),
            }
        }
    }

    /// Arrête tous les sons en cours (sortie du mode Play).
    pub fn stop_all(&mut self) {
        for handle in &mut self.playing {
            handle.stop(Tween::default());
        }
        self.playing.clear();
        self.pending.clear();
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}
