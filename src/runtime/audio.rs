//! Audio simple via kira : décodage en thread de fond + cache pour éviter tout lag.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};

use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::{AudioManager, AudioManagerSettings, DefaultBackend, Tween};

pub struct Audio {
    manager: Option<AudioManager>,
    playing: Vec<StaticSoundHandle>,
    /// Sons déjà décodés (réutilisés sans re-décoder).
    cache: HashMap<String, StaticSoundData>,
    /// Chemins demandés mais pas encore décodés (à jouer dès qu'ils arrivent).
    pending: HashSet<String>,
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
            pending: HashSet::new(),
            tx,
            rx,
        }
    }

    /// Joue un fichier : instantané si en cache, sinon décodage en fond puis lecture à l'arrivée.
    pub fn play(&mut self, path: &str) {
        if let Some(data) = self.cache.get(path).cloned() {
            self.start(data);
            return;
        }
        // Asset embarqué (player exporté) : décodage immédiat depuis la mémoire.
        if let Some(key) = crate::assets::strip_scheme(path) {
            match crate::assets::bundle_bytes(key) {
                Some(bytes) => {
                    match StaticSoundData::from_cursor(std::io::Cursor::new(bytes.to_vec())) {
                        Ok(data) => {
                            self.cache.insert(path.to_string(), data.clone());
                            self.start(data);
                        }
                        Err(e) => log::error!("Son embarqué '{key}' illisible : {e}"),
                    }
                }
                None => log::error!("Son embarqué introuvable : {key}"),
            }
            return;
        }
        // pas encore décodé : lancer un décodage en arrière-plan (une seule fois)
        if self.pending.insert(path.to_string()) {
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
            if self.pending.remove(&path) {
                self.start(data);
            }
        }
    }

    fn start(&mut self, data: StaticSoundData) {
        if let Some(m) = self.manager.as_mut() {
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
