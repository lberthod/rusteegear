//! Réglages utilisateur persistés : clé API DeepSeek pour la
//! génération de scripts Lua par IA, et config Firebase pour les comptes
//! multijoueur. Stockés dans `~/.motor3derust/settings.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Clé API DeepSeek (laisser vide pour désactiver la génération IA).
    #[serde(default)]
    pub deepseek_api_key: String,
    /// Modèle DeepSeek à utiliser (`deepseek-chat`, `deepseek-reasoner`, ou un id précis).
    #[serde(default = "default_model")]
    pub deepseek_model: String,
    /// Température de génération (0 = déterministe, 1 = créatif).
    #[serde(default = "default_temperature")]
    pub deepseek_temperature: f32,
    /// Clé API Web Firebase (Project Settings → Web API Key). Laisser vide pour
    /// désactiver les comptes/backend annexe multijoueur (cf. `net::firebase`).
    /// Publique par conception côté Firebase — la sécurité vient des **règles**
    /// RTDB, pas du secret de cette clé (cf. commentaire dans `net::firebase`).
    #[serde(default)]
    pub firebase_api_key: String,
    /// URL de la Realtime Database (ex. `https://xxx-default-rtdb.firebaseio.com`).
    #[serde(default)]
    pub firebase_database_url: String,
    /// Volume (0..1) de la piste musique/ambiance (Sprint 104, cf.
    /// `runtime::audio::Audio::set_music_volume`).
    #[serde(default = "default_volume")]
    pub music_volume: f32,
    /// Volume (0..1) de la piste effets sonores (Sprint 104, cf.
    /// `runtime::audio::Audio::set_sfx_volume`).
    #[serde(default = "default_volume")]
    pub sfx_volume: f32,
    /// Échelle des éléments HUD peints directement (barre de vie, indicateur de
    /// vague, HUD d'arme, frags, réticule, bannières) — PHASE I Sprint 1 (GDD
    /// §16.6, accessibilité minimale). 1.0 = taille actuelle ; ne change que la
    /// taille du texte/des jauges, jamais leur position à l'écran.
    #[serde(default = "default_hud_scale")]
    pub hud_scale: f32,
    /// Réduit à zéro l'amplitude du recul caméra (screen-shake, cf.
    /// `AppState::camera_shake_offset`) déclenché à l'encaissement d'un coup —
    /// PHASE I Sprint 1 (§16.6), pour les joueurs sensibles au mouvement de
    /// caméra.
    #[serde(default)]
    pub reduce_shake: bool,
    /// Remapping manette (Sprint 110) : quel bouton `gilrs` déclenche chaque action.
    #[serde(default)]
    pub gamepad: GamepadBindings,
    /// Langue du texte runtime affiché en Play (Sprint 130) — pas l'éditeur.
    #[serde(default)]
    pub locale: crate::app::locale::Locale,
    /// Pseudos mute localement dans le chat de salon (Sprint F-13,
    /// SPRINT_MMORPG.md §18.4.1) : purement client, pas partagé réseau — un
    /// joueur muté ici continue de voir ses messages chez les autres.
    #[serde(default)]
    pub muted_players: Vec<String>,
    /// Projets récemment ouverts (Sprint 4), le plus récent en premier —
    /// plafonné à [`Settings::MAX_RECENT_PROJECTS`]. Affiché dans le sous-menu
    /// Fichier → Projets récents et l'assistant « Nouveau projet ».
    #[serde(default)]
    pub recent_projects: Vec<RecentProject>,
}

/// Une entrée de la liste MRU (`Settings::recent_projects`).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct RecentProject {
    /// Nom déclaré par le manifeste au moment de la dernière ouverture (peut
    /// devenir périmé si le manifeste est renommé depuis — pas grave, juste
    /// un libellé d'affichage).
    pub name: String,
    /// Chemin du dossier racine du projet.
    pub path: String,
    /// Horodatage Unix (secondes) de la dernière ouverture — sert uniquement
    /// à trier la liste, jamais affiché brut à l'utilisateur.
    pub last_opened_unix_secs: u64,
}

/// Table de remapping manette → action, persistée et éditable dans les paramètres
/// (panneau « 🎮 Manette »). Chaque champ est un nom de `app::input::
/// GAMEPAD_BUTTON_NAMES` (pas un `gilrs::Button` directement : celui-ci n'implémente
/// pas `Serialize`, et un nom stable en JSON survit mieux à une évolution de la
/// dépendance qu'un discriminant d'enum sérialisé tel quel).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct GamepadBindings {
    pub jump: String,
    pub attack: String,
    pub fire: String,
    pub heal: String,
    /// Changement d'arme (cycle sur front montant, cf. `fireball::cycle_weapon`) —
    /// pendant manette des touches clavier 1/2/3 et du bouton tactile « Arme ».
    pub weapon: String,
    /// Ouvre/ferme la fenêtre Multijoueur (bascule sur front montant, cf.
    /// `App::recompute_action_buttons`) — le « menu » de la manette.
    pub menu: String,
    /// Masque/affiche les widgets HUD en Play (bascule sur front montant) —
    /// capture d'écran propre, spectacle, ou simple désencombrement.
    pub hud: String,
}

impl Default for GamepadBindings {
    /// South/West/East/North : disposition Xbox par défaut (A = Saut, X = Attaque,
    /// B = Tir, Y = Soin) — cohérente avec les voisins clavier J/K/H (Attaque/Tir/
    /// Soin groupés), sans obliger à une manette précise (les noms `gilrs`
    /// sont génériques par position, pas par étiquette de fabricant). Changer
    /// d'arme sur RightTrigger (bumper droit RB) : les quatre boutons de façade
    /// sont pris, et un bumper se presse sans lâcher le stick.
    fn default() -> Self {
        Self {
            jump: "South".into(),
            attack: "West".into(),
            fire: "East".into(),
            heal: "North".into(),
            weapon: "RightTrigger".into(),
            menu: "Start".into(),
            hud: "Select".into(),
        }
    }
}

fn default_model() -> String {
    "deepseek-chat".to_string()
}

fn default_temperature() -> f32 {
    0.2
}

fn default_volume() -> f32 {
    1.0
}

fn default_hud_scale() -> f32 {
    1.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            deepseek_api_key: String::new(),
            deepseek_model: default_model(),
            deepseek_temperature: default_temperature(),
            firebase_api_key: String::new(),
            firebase_database_url: String::new(),
            music_volume: default_volume(),
            sfx_volume: default_volume(),
            hud_scale: default_hud_scale(),
            reduce_shake: false,
            gamepad: GamepadBindings::default(),
            locale: crate::app::locale::Locale::default(),
            muted_players: Vec::new(),
            recent_projects: Vec::new(),
        }
    }
}

impl Settings {
    /// Chemin du fichier de réglages : `app_data_dir()/settings.json`, par
    /// plateforme (cf. `assets::app_data_dir` — Android via `set_android_data_dir`,
    /// sinon `~/.motor3derust/`, comme avant ce Sprint 1 côté desktop). Avant ce
    /// Sprint, cette fonction résolvait `$HOME` en dur : sur Android (où `$HOME`
    /// n'existe pas), elle renvoyait toujours `None`, donc `load()`/`save()`
    /// dégradaient silencieusement en no-op — le joueur mobile perdait tout
    /// réglage (Firebase, manette, volumes) à chaque redémarrage.
    fn path() -> Option<PathBuf> {
        Some(crate::assets::app_data_dir()?.join("settings.json"))
    }

    /// Charge les réglages depuis le disque ; à défaut de fichier existant, ceux embarqués à
    /// l'export (Sprint 3 de PHASE A — `assets::default_settings_json`, clé Firebase pré-remplie
    /// pour un `.app`/APK qui fonctionne sans saisie manuelle), ou sinon les valeurs par défaut.
    pub fn load() -> Self {
        let Some(p) = Self::path() else {
            return Self::from_bundled_defaults();
        };
        if p.exists() {
            return Self::load_from(&p);
        }
        let defaults = Self::from_bundled_defaults();
        // Persisté seulement si l'export a réellement embarqué une config par défaut : sinon
        // (développement, aucun `default_settings.json` dans le bundle), premier lancement
        // silencieux comme avant ce Sprint 3 — pas d'écriture avant un `save()` explicite.
        if crate::assets::default_settings_json().is_some() {
            defaults.save_to(&p);
        }
        defaults
    }

    /// Réglages d'un premier lancement : ceux embarqués à l'export s'il y en a (JSON partiel —
    /// seuls les champs Firebase sont écrits par `editor::export`, le reste retombe sur les
    /// `#[serde(default)]` de cette struct), sinon `Settings::default()`.
    fn from_bundled_defaults() -> Self {
        crate::assets::default_settings_json()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Comme `load`, mais avec un chemin de fichier explicite (isolation des
    /// tests — même patron que `assets::read_user_bytes_at`).
    fn load_from(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persiste les réglages (crée le dossier parent au besoin).
    pub fn save(&self) {
        if let Some(p) = Self::path() {
            self.save_to(&p);
        }
    }

    /// Comme `save`, mais avec un chemin de fichier explicite (isolation des
    /// tests — même patron que `assets::write_user_bytes_at`).
    fn save_to(&self, path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// `true` si `name` est mute localement (cf. `muted_players`).
    pub fn is_muted(&self, name: &str) -> bool {
        self.muted_players.iter().any(|m| m == name)
    }

    /// Mute `name` et persiste immédiatement. Sans effet si déjà mute.
    pub fn mute_player(&mut self, name: &str) {
        if !self.is_muted(name) {
            self.muted_players.push(name.to_string());
            self.save();
        }
    }

    /// Démute `name` et persiste immédiatement. Sans effet si pas mute.
    pub fn unmute_player(&mut self, name: &str) {
        let before = self.muted_players.len();
        self.muted_players.retain(|m| m != name);
        if self.muted_players.len() != before {
            self.save();
        }
    }

    /// Nombre maximal d'entrées conservées dans `recent_projects` (Sprint 4).
    pub const MAX_RECENT_PROJECTS: usize = 10;

    /// Logique pure d'insertion, séparée de `record_recent_project` pour rester
    /// testable sans déclencher `save()` (qui touche `$HOME` — même patron que
    /// `mute_player`/ses tests, cf. plus bas).
    fn upsert_recent_project(&mut self, name: &str, root: &Path, now_unix_secs: u64) {
        let path = root.to_string_lossy().into_owned();
        self.recent_projects.retain(|p| p.path != path);
        self.recent_projects.insert(
            0,
            RecentProject {
                name: name.to_string(),
                path,
                last_opened_unix_secs: now_unix_secs,
            },
        );
        self.recent_projects.truncate(Self::MAX_RECENT_PROJECTS);
    }

    /// Enregistre `root` en tête des projets récents et persiste immédiatement.
    /// Une réouverture du même chemin le remonte en tête plutôt que de
    /// dupliquer l'entrée. `now_unix_secs` est passé par l'appelant (plutôt que
    /// lu ici via `SystemTime::now()`) pour rester testable sans horloge réelle.
    pub fn record_recent_project(&mut self, name: &str, root: &Path, now_unix_secs: u64) {
        self.upsert_recent_project(name, root, now_unix_secs);
        self.save();
    }

    /// Projets récents dont le manifeste existe encore sur disque — un dossier
    /// supprimé, déplacé ou sur un volume débranché depuis la dernière
    /// ouverture est ignoré silencieusement plutôt que de proposer une entrée
    /// qui échouerait à l'ouverture (Sprint 4).
    pub fn existing_recent_projects(&self) -> Vec<&RecentProject> {
        self.recent_projects
            .iter()
            .filter(|p| {
                Path::new(&p.path)
                    .join(crate::project::MANIFEST_FILE)
                    .exists()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip JSON pur (`serde_json`, pas `Settings::save`/`load` qui
    /// touchent le vrai `$HOME` de la machine — à éviter dans un test).
    #[test]
    fn music_and_sfx_volume_round_trip_at_full_volume_by_default() {
        let settings = Settings::default();
        assert_eq!(settings.music_volume, 1.0);
        assert_eq!(settings.sfx_volume, 1.0);
        let json = serde_json::to_string(&settings).expect("sérialisable");
        let back: Settings = serde_json::from_str(&json).expect("désérialisable");
        assert_eq!(back.music_volume, 1.0);
        assert_eq!(back.sfx_volume, 1.0);
    }

    /// Sprint 104 : un `settings.json` déjà sur le disque d'un utilisateur
    /// (écrit par une version antérieure, sans `music_volume`/`sfx_volume`)
    /// doit continuer à charger, avec les valeurs par défaut pour les
    /// nouveaux champs — même garde-fou que `scene::tests::old_scene_
    /// without_new_fields_loads_with_defaults`.
    #[test]
    fn an_old_settings_file_without_volume_fields_loads_with_defaults() {
        let old_json = r#"{
            "deepseek_api_key": "sk-test",
            "deepseek_model": "deepseek-chat",
            "deepseek_temperature": 0.2,
            "firebase_api_key": "",
            "firebase_database_url": ""
        }"#;
        let settings: Settings = serde_json::from_str(old_json)
            .expect("un ancien settings.json sans les champs volume doit rester lisible");
        assert_eq!(settings.deepseek_api_key, "sk-test");
        assert_eq!(settings.music_volume, 1.0);
        assert_eq!(settings.sfx_volume, 1.0);
        assert_eq!(settings.gamepad, GamepadBindings::default());
    }

    /// Sprint 110 : un `settings.json` antérieur (sans le champ `gamepad`) doit
    /// continuer à charger, avec les bindings manette par défaut — même garde-fou
    /// que le test volume ci-dessus, pour le champ ajouté par ce sprint-ci.
    #[test]
    fn an_old_settings_file_without_gamepad_field_loads_with_default_bindings() {
        let old_json = r#"{
            "deepseek_api_key": "",
            "deepseek_model": "deepseek-chat",
            "deepseek_temperature": 0.2,
            "firebase_api_key": "",
            "firebase_database_url": "",
            "music_volume": 0.8,
            "sfx_volume": 0.8
        }"#;
        let settings: Settings = serde_json::from_str(old_json)
            .expect("un ancien settings.json sans `gamepad` doit rester lisible");
        assert_eq!(settings.gamepad, GamepadBindings::default());
    }

    /// Sprint F-13 : un `settings.json` antérieur (sans le champ `muted_players`)
    /// doit continuer à charger, avec une liste vide — même garde-fou que les
    /// tests ci-dessus pour les champs ajoutés par des sprints ultérieurs.
    #[test]
    fn an_old_settings_file_without_muted_players_field_loads_with_empty_list() {
        let old_json = r#"{
            "deepseek_api_key": "",
            "deepseek_model": "deepseek-chat",
            "deepseek_temperature": 0.2,
            "firebase_api_key": "",
            "firebase_database_url": ""
        }"#;
        let settings: Settings = serde_json::from_str(old_json)
            .expect("un ancien settings.json sans `muted_players` doit rester lisible");
        assert!(settings.muted_players.is_empty());
    }

    /// PHASE I Sprint 1 : un `settings.json` antérieur (sans `hud_scale`/
    /// `reduce_shake`) doit continuer à charger, avec l'échelle HUD à 1.0 et le
    /// screen-shake non réduit — même garde-fou que les tests ci-dessus pour
    /// les champs ajoutés par des sprints ultérieurs.
    #[test]
    fn an_old_settings_file_without_accessibility_fields_loads_with_defaults() {
        let old_json = r#"{
            "deepseek_api_key": "",
            "deepseek_model": "deepseek-chat",
            "deepseek_temperature": 0.2,
            "firebase_api_key": "",
            "firebase_database_url": "",
            "music_volume": 0.8,
            "sfx_volume": 0.8
        }"#;
        let settings: Settings = serde_json::from_str(old_json)
            .expect("un ancien settings.json sans champs d'accessibilité doit rester lisible");
        assert_eq!(settings.hud_scale, 1.0);
        assert!(!settings.reduce_shake);
    }

    /// PHASE I Sprint 1 : `hud_scale`/`reduce_shake` survivent à un aller-retour
    /// JSON, même patron que le test volume ci-dessus.
    #[test]
    fn hud_scale_and_reduce_shake_round_trip() {
        let settings = Settings {
            hud_scale: 1.5,
            reduce_shake: true,
            ..Settings::default()
        };
        let json = serde_json::to_string(&settings).expect("sérialisable");
        let back: Settings = serde_json::from_str(&json).expect("désérialisable");
        assert_eq!(back.hud_scale, 1.5);
        assert!(back.reduce_shake);
    }

    /// `mute_player`/`unmute_player` : ajoutent/retirent sans doublon, sans
    /// toucher `$HOME` (pas de `save()` observable ici, juste l'état en mémoire).
    #[test]
    fn mute_player_is_idempotent_and_unmute_removes_it() {
        let mut settings = Settings::default();
        assert!(!settings.is_muted("Grosse Bertha"));
        settings.muted_players.push("Grosse Bertha".to_string());
        assert!(settings.is_muted("Grosse Bertha"));
        // Second ajout manuel simulé : `mute_player` ne duplique pas l'entrée.
        if !settings.is_muted("Grosse Bertha") {
            settings.muted_players.push("Grosse Bertha".to_string());
        }
        assert_eq!(settings.muted_players.len(), 1);
        settings.muted_players.retain(|m| m != "Grosse Bertha");
        assert!(!settings.is_muted("Grosse Bertha"));
    }

    /// Dossier temporaire unique par test (pas de mutation de `$HOME`/état
    /// global — même patron que `assets::tests::temp_assets_dir`).
    fn temp_settings_path(tag: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "motor3derust_settings_test_{tag}_{:?}",
            std::thread::current().id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("settings.json")
    }

    /// Sprint 1 (config hors éditeur) : `save_to`/`load_from` doivent faire un
    /// aller-retour fidèle sur un répertoire simulé, sans toucher au vrai
    /// `$HOME`/dossier de données de la machine — c'est ce chemin explicite que
    /// `path()` emprunte désormais aussi sur Android via `assets::app_data_dir`
    /// (avant ce sprint, `path()` résolvait `$HOME` en dur et renvoyait `None`
    /// sur Android, dégradant `load()`/`save()` en no-op silencieux).
    #[test]
    fn save_to_then_load_from_round_trips_on_a_simulated_directory() {
        let path = temp_settings_path("round_trip");
        let settings = Settings {
            firebase_api_key: "AIzaTest".to_string(),
            firebase_database_url: "https://xxx-default-rtdb.firebaseio.com".to_string(),
            gamepad: GamepadBindings {
                menu: "LeftTrigger".to_string(),
                ..GamepadBindings::default()
            },
            ..Settings::default()
        };
        settings.save_to(&path);

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.firebase_api_key, "AIzaTest");
        assert_eq!(
            loaded.firebase_database_url,
            "https://xxx-default-rtdb.firebaseio.com"
        );
        assert_eq!(loaded.gamepad.menu, "LeftTrigger");
    }

    /// `load_from` sur un fichier absent (première utilisation) renvoie les
    /// valeurs par défaut plutôt que de paniquer — même garde-fou que
    /// `Settings::load()` côté disque réel.
    #[test]
    fn load_from_a_missing_file_returns_defaults() {
        let path = temp_settings_path("missing");
        let _ = std::fs::remove_file(&path);
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.firebase_api_key, "");
        assert_eq!(loaded.gamepad, GamepadBindings::default());
    }

    /// `upsert_recent_project` (Sprint 4) : la réouverture d'un projet déjà
    /// présent le remonte en tête sans dupliquer l'entrée — pas de `save()`
    /// ici, même patron que `mute_player_is_idempotent...` ci-dessus.
    #[test]
    fn upsert_recent_project_moves_a_known_path_to_the_front_without_duplicating() {
        let mut settings = Settings::default();
        settings.upsert_recent_project("Jeu A", std::path::Path::new("/tmp/a"), 100);
        settings.upsert_recent_project("Jeu B", std::path::Path::new("/tmp/b"), 200);
        assert_eq!(settings.recent_projects.len(), 2);
        assert_eq!(settings.recent_projects[0].name, "Jeu B");

        // Rouvrir A le remonte en tête, sans ajouter de troisième entrée.
        settings.upsert_recent_project("Jeu A", std::path::Path::new("/tmp/a"), 300);
        assert_eq!(settings.recent_projects.len(), 2);
        assert_eq!(settings.recent_projects[0].name, "Jeu A");
        assert_eq!(settings.recent_projects[0].last_opened_unix_secs, 300);
    }

    #[test]
    fn upsert_recent_project_caps_at_max_recent_projects() {
        let mut settings = Settings::default();
        for i in 0..(Settings::MAX_RECENT_PROJECTS + 5) {
            settings.upsert_recent_project(
                &format!("Jeu {i}"),
                &std::path::PathBuf::from(format!("/tmp/jeu-{i}")),
                i as u64,
            );
        }
        assert_eq!(
            settings.recent_projects.len(),
            Settings::MAX_RECENT_PROJECTS
        );
        // Le plus récemment ajouté (dernier de la boucle) doit rester en tête.
        assert_eq!(
            settings.recent_projects[0].name,
            format!("Jeu {}", Settings::MAX_RECENT_PROJECTS + 4)
        );
    }

    /// `existing_recent_projects` ignore silencieusement les dossiers dont le
    /// manifeste n'est plus sur disque (supprimé/déplacé depuis).
    #[test]
    fn existing_recent_projects_filters_out_vanished_manifests() {
        let dir = std::env::temp_dir().join("motor3derust-test-existing-recent-projects");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(crate::project::MANIFEST_FILE), "{}").unwrap();

        let mut settings = Settings::default();
        settings.upsert_recent_project("Encore là", &dir, 1);
        settings.upsert_recent_project(
            "Disparu",
            std::path::Path::new("/inexistant/nulle-part"),
            2,
        );

        let existing = settings.existing_recent_projects();
        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0].name, "Encore là");
    }
}
