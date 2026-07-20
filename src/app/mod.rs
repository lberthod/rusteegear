//! État applicatif **sans dépendance GPU** : scène, sélection, caméra, mode Play,
//! interaction pointeur. Le `Renderer` consomme cet état pour dessiner.

pub mod ai;
pub mod asset_ops;
mod autosave;
pub mod build_config;
mod combat;
mod console;
mod debug_draw;
mod demos;
mod fireball;
mod health;
pub mod input;
mod inventory;
pub mod locale;
pub mod multiplayer;
pub mod network_client;
mod persistence;
mod picking;
#[cfg(not(target_arch = "wasm32"))]
pub mod scripting;
// Backend Lua du player web (Sprint 137) : symétrique de `scripting` (mlua, natif),
// sur `rilua` (pur Rust, compile aussi nativement — `cfg(test)` en plus de wasm32
// permet les tests différentiels contre `mlua`, cf. `scripting_web::tests`).
mod creature_attack;
#[cfg(any(target_arch = "wasm32", test))]
mod scripting_web;
mod selection;
pub mod settings;
mod simulation;

use combat::{AttackCharge, AttackProjectile};

use crate::time_compat::Instant;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};

use glam::{Quat, Vec3};
#[cfg(not(target_arch = "wasm32"))]
use mlua::Lua;
#[cfg(target_arch = "wasm32")]
use rilua::LuaApiMut;

use crate::gfx::camera::OrbitCamera;
use crate::gfx::mesh::MeshData;
use crate::scene::{
    GameCamera, MeshKind, MobileControls, PointLight, Scene, SceneObject, Transform,
};

/// Instantané léger de la scène pour l'undo/redo (sans les meshes importés, lourds
/// et rarement modifiés) : objets + lumières + caméra de jeu + contrôles + groupes.
#[derive(Clone)]
struct SceneSnapshot {
    objects: Vec<SceneObject>,
    groups: Vec<String>,
    point_lights: Vec<PointLight>,
    mobile: MobileControls,
    camera_follow: bool,
    game_camera: Option<GameCamera>,
}

impl SceneSnapshot {
    fn capture(s: &Scene) -> Self {
        Self {
            objects: s.objects.clone(),
            groups: s.groups.clone(),
            point_lights: s.point_lights.clone(),
            mobile: s.mobile.clone(),
            camera_follow: s.camera_follow,
            game_camera: s.game_camera,
        }
    }
    fn restore(self, s: &mut Scene) {
        s.objects = self.objects;
        s.groups = self.groups;
        s.point_lights = self.point_lights;
        s.mobile = self.mobile;
        s.camera_follow = self.camera_follow;
        s.game_camera = self.game_camera;
    }
}

/// Résultat d'un import glTF effectué en thread de fond.
type ImportResult = Result<(String, MeshData, Vec3, Vec3), String>;

/// Rectangle `(x, y, largeur, hauteur)` d'un écran de téléphone (ratio 1080×2340,
/// ≈ 19.5:9) centré dans une zone `width × height`, avec une petite marge.
/// Sert à l'« Aperçu mobile » : même calcul en pixels (viewport GPU) et en points (UI egui).
pub fn device_rect(width: f32, height: f32, portrait: bool) -> (f32, f32, f32, f32) {
    let ar = if portrait {
        1080.0 / 2340.0
    } else {
        2340.0 / 1080.0
    };
    let margin = 0.94;
    let mut w = width * margin;
    let mut h = w / ar;
    if h > height * margin {
        h = height * margin;
        w = h * ar;
    }
    ((width - w) * 0.5, (height - h) * 0.5, w, h)
}

/// État des contrôles tactiles produit par l'overlay UI et lu par les scripts Lua.
#[derive(Default)]
pub struct PlayerInput {
    /// Axe du joystick virtuel, chaque composante dans [-1, 1].
    pub joy: (f32, f32),
    /// Boutons actuellement pressés (par nom).
    pub buttons: std::collections::HashSet<String>,
    /// Inclinaison (gyroscope/accéléromètre), chaque composante dans [-1, 1].
    /// Desktop : simulée aux flèches du clavier ; mobile : capteur natif (à brancher).
    pub tilt: (f32, f32),
    /// Déplacement clavier (ordinateur), relatif à la caméra : flèches **et** WASD
    /// (les deux jeux de touches sont équivalents depuis le passage au style
    /// « action moderne » — stick gauche/WASD = intention de déplacement relative
    /// à la caméra, le personnage tourne ensuite tout seul vers cette direction,
    /// cf. `AppState::advance_play`) ; chaque composante dans [-1, 1].
    pub key_move: (f32, f32),
    /// Élévation caméra libre (Espace = monte, C = descend), cf. `AppState::fly_cam`
    /// et `AppState::update_fly_cam` — sans effet ailleurs.
    pub fly_vertical: f32,
    /// Rotation clavier « tank » (A/D) : -1 = tourne à droite (A), +1 = tourne à
    /// gauche (D) — indépendante de la caméra, contrairement à `key_move`. Cf.
    /// `AppState::advance_play`.
    pub key_turn: f32,
    /// Avance/recul clavier « tank » (W/S), le long de l'orientation *actuelle* du
    /// personnage plutôt que de la caméra : +1 = W (avance), -1 = S (recul).
    pub key_thrust: f32,
    /// Avance/recul « tank » du **pavé tactile W/A/S/D** (cf. l'overlay mobile,
    /// `editor::mobile_overlay`) : canal séparé de `key_thrust` pour que le pavé
    /// (réécrit chaque frame, 0 au relâchement) n'écrase jamais l'état clavier,
    /// tenu par événements — les deux se cumulent via `thrust()`.
    pub touch_thrust: f32,
    /// Rotation « tank » du pavé tactile (A/D) — même principe que `touch_thrust`.
    pub touch_turn: f32,
    /// Stick gauche de la manette, zone morte + croix directionnelle déjà
    /// résolues (cf. `input::resolve_gamepad_input`) : déplacement **relatif à
    /// la caméra**, comme `joy`/`key_move`, cumulé avec eux avant
    /// `camera_relative_move` (style « action moderne » — stick gauche =
    /// intention de déplacement, le personnage tourne tout seul vers la
    /// direction résultante). Chaque composante dans [-1, 1].
    pub gamepad_move: (f32, f32),
    /// Axe horizontal du stick droit de la manette, zone morte déjà appliquée :
    /// orbite librement la caméra de jeu (`AppState::advance_play`), indépendant
    /// du personnage — stick droit = caméra, comme dans un TPS moderne. Sans
    /// manette, reste à 0 — aucun effet.
    pub gamepad_yaw: f32,
    /// Tangage caméra du stick droit de la manette (axe vertical, zone morte
    /// déjà appliquée) : consommé par la caméra de suivi (`update_effects`),
    /// stick vers le haut = regarder vers le haut. Sans manette, `gamepad_pitch`
    /// reste à 0 — aucun effet.
    pub gamepad_pitch: f32,
    /// Saut clavier (Espace) maintenu enfoncé.
    pub jump: bool,
    /// Attaque clavier (J) maintenue enfoncée.
    pub attack: bool,
    /// Tir de boule de feu clavier (K) maintenu enfoncé — cf. `app::fireball` ;
    /// le pendant tactile est le bouton nommé `Controller::fire_button`.
    pub fire: bool,
    /// Soin clavier (H) maintenu enfoncé — cf. `app::health` ; le pendant
    /// tactile est le bouton nommé `Controller::heal_button`. Sans effet en
    /// solo (pas d'allié) : n'a d'effet réel qu'en ligne, résolu côté serveur.
    pub heal: bool,
    /// Changement d'arme manette (Sprint 110) maintenu enfoncé — le cycle se
    /// déclenche sur le front montant dans `update_fireballs`, comme le bouton
    /// tactile `Controller::weapon_button` ; les pendants clavier (1/2/3)
    /// sélectionnent directement sans passer par cet état.
    pub weapon_cycle: bool,
}

impl PlayerInput {
    /// Avance/recul « tank » effectif : clavier (W/S) + pavé tactile, borné à [-1, 1].
    /// Toute la logique (prédiction locale `sim_step` **et** envoi réseau
    /// `network_move_axes`) passe par ici — mêmes contrôles au clavier, au tactile
    /// (APK) et en aperçu mobile desktop, sans qu'aucune source n'écrase l'autre.
    pub fn thrust(&self) -> f32 {
        (self.key_thrust + self.touch_thrust).clamp(-1.0, 1.0)
    }

    /// Rotation « tank » effective : pavé tactile mobile (`touch_turn`) + toute
    /// source externe qui pilote encore `key_turn` directement (pont de pilotage,
    /// cf. `pilot.rs`) — le clavier de bureau (A/D) n'y contribue plus depuis le
    /// passage au style « action moderne » (WASD alimente `key_move`, cf. `lib.rs`).
    pub fn turn(&self) -> f32 {
        (self.key_turn + self.touch_turn).clamp(-1.0, 1.0)
    }
}

/// Résultat d'une connexion Firebase en arrière-plan (`request_firebase_auth`) :
/// `(uid, id_token, xp cumulée si lisible)` — l'XP alimente la bannière
/// « palier atteint » (GDD §8.2), `None` si la lecture de progression échoue.
type FirebaseAuthResult = Result<(String, String, Option<u32>), String>;

pub struct AppState {
    pub scene: Scene,
    /// Projet ouvert (Sprint 3, manifeste `project.rusteegear.json`), posé par
    /// `open_project`/`create_project`. `None` en mode « scène seule »
    /// (comportement historique, toujours supporté) — cf. `crate::project`.
    pub current_project: Option<crate::project::ProjectRoot>,
    /// Confirmation de fermeture de projet en attente (Sprint 4) : même esprit
    /// que `confirm_quit`, mais pour « Fermer le projet » plutôt que quitter
    /// l'application — posé par `request_close_project` si `scene_dirty`.
    pub confirm_close_project: bool,
    /// Dernier autosave effectué cette session (Sprint 6) — `None` avant le
    /// premier. Sert à espacer les écritures de `AppState::AUTOSAVE_INTERVAL`
    /// (cf. `autosave::maybe_autosave`).
    last_autosave: Option<crate::time_compat::Instant>,
    /// Autosave à proposer en restauration au démarrage (Sprint 6), posé une
    /// fois par `lib::run()` juste après la création de l'app (cf.
    /// `AppState::pending_autosave_recovery`). `None` : rien à proposer, ou
    /// modale déjà traitée (Restaurer/Ignorer, cf. `gfx::renderer`).
    pub pending_autosave_recovery: Option<std::path::PathBuf>,
    /// Sélection « primaire » (gizmo, inspecteur, surbrillance forte).
    pub selection: Option<usize>,
    /// Ensemble sélectionné (inclut la primaire) pour les opérations groupées.
    pub selected: Vec<usize>,
    /// Presse-papiers d'objets (copier/coller).
    clipboard: Vec<SceneObject>,
    pub playing: bool,
    /// Caméra libre (« vol libre »/noclip) de l'éditeur, hors Play : bascule au clavier
    /// (G), déplacement aux flèches + Espace/C, cf. `update_fly_cam`. Sans effet en Play
    /// (remis à `false` à l'entrée en Play, la caméra de jeu prenant le relais).
    pub fly_cam: bool,
    /// En pause : reste en mode Play mais gèle la simulation (scripts, physique, temps).
    pub paused: bool,
    /// Demande de fermeture de l'application (menu Fichier → Quitter ou bouton
    /// de fermeture de la fenêtre, une fois la confirmation passée le cas échéant).
    pub should_quit: bool,
    /// Scène modifiée depuis la dernière sauvegarde (Phase C, `sprint.19matin.md`).
    /// Posé par `push_undo` (toute opération annulable) et par la détection
    /// d'édition de champs UI (cf. `ui_scene_fingerprint`) ; remis à faux par une
    /// sauvegarde réussie, un chargement de scène et `clear_history` (démarrage).
    /// Approximation assumée : annuler jusqu'à l'état sauvegardé laisse le drapeau posé.
    pub scene_dirty: bool,
    /// Confirmation de fermeture en attente : la fermeture a été demandée alors
    /// que `scene_dirty` est posé — l'éditeur affiche la modale
    /// Enregistrer / Quitter sans enregistrer / Annuler au lieu de quitter.
    pub confirm_quit: bool,
    /// Mode « player » : pas d'éditeur (panneaux egui), démarre en Play.
    pub player: bool,
    /// Langue du texte runtime affiché en Play (Sprint 130) — pas l'éditeur, dont l'UI
    /// reste en français (outil de développement). Persistée dans `Settings::locale`.
    pub locale: locale::Locale,
    /// État courant des contrôles tactiles (joystick + boutons), lu par les scripts.
    pub input_state: PlayerInput,
    /// Objet « tactile » touché cette frame (exposé une frame à son script via `obj.tapped`).
    tapped_obj: Option<usize>,
    /// Objet sous presse actuellement maintenue (du PointerDown au PointerUp) —
    /// pour `obj.touching` côté script. Contrairement à `tapped_obj` (vrai une
    /// seule frame au relâché, seulement pour un tap net sans glissé), reste
    /// `Some` sur toute la durée de la presse, quelle que soit la position
    /// courante du curseur.
    touched_obj: Option<usize>,
    /// Une seule frame : l'objet vient de recevoir la presse qui commence
    /// (`obj.touch_started`).
    touch_started_obj: Option<usize>,
    /// Une seule frame : la presse démarrée sur cet objet vient de se terminer
    /// (`obj.touch_ended`), qu'elle se relâche dessus ou après un glissé.
    touch_ended_obj: Option<usize>,
    /// Accumulateur de temps réel pour la simulation à **pas fixe** (découplée du rendu).
    sim_accumulator: f32,
    /// Multiplicateur du temps simulé : 1.0 = normal, 0 = figé, &gt;1 = accéléré.
    /// N'affecte que le `dt` consommé par la physique/les scripts, jamais le FPS affiché
    /// ni le pas fixe lui-même (`FIXED_DT` reste 1/60 s : seul le nombre de pas exécutés
    /// par frame change). Utile pour déboguer la physique et le réseau au ralenti.
    pub time_scale: f32,
    /// Pas unique demandé pendant la pause (cf. `request_step`) : consommé
    /// (remis à `false`) au tout début du prochain `advance_play`.
    step_requested: bool,
    /// Segments de debug drawing : (début, fin, couleur), accumulés pendant la
    /// frame par le picking et le gameplay (`debug_line`/`debug_box`/`debug_sphere`), lus et
    /// vidés par `Renderer::render` après dessin — jamais persistants au-delà d'une frame.
    pub debug_lines: Vec<(Vec3, Vec3, [f32; 3])>,
    /// Poses (position, rotation, échelle) de tous les objets après l'**avant-dernier**
    /// pas de simulation à pas fixe. Couplé à `sim_curr_poses` pour l'interpolation de
    /// rendu (cf. `advance_play`) : le rendu affiche un mélange des deux pondéré par
    /// l'accumulateur, au lieu de la dernière pose brute — sans quoi une frame affiche
    /// tantôt 0, tantôt 2 pas de simulation selon l'alignement rendu/60 Hz, un
    /// à-coup visible en continu (« judder »).
    sim_prev_poses: Vec<(Vec3, Quat, Vec3)>,
    /// Poses après le **dernier** pas de simulation — l'état « vrai » de la simulation,
    /// restauré avant chaque nouveau pas (cf. `restore_sim_poses` : les transforms
    /// affichés peuvent s'en écarter d'une fraction de pas à cause du mélange visuel).
    sim_curr_poses: Vec<(Vec3, Quat, Vec3)>,
    /// Poses telles qu'écrites par le **dernier** `blend_render_poses` : référence de
    /// `restore_sim_poses` pour distinguer « transform encore égal au mélange » (à
    /// restaurer) d'une écriture externe survenue depuis (à respecter). Vide = pas de
    /// mélange valide (début de Play, scène modifiée).
    sim_render_poses: Vec<(Vec3, Quat, Vec3)>,
    /// Temps de jeu (s) auquel tous les collectibles ont été ramassés (figé pour le HUD).
    win_time: Option<f32>,
    /// Partie perdue : le joueur a touché une zone mortelle (fige le jeu jusqu'au Stop).
    lost: bool,
    /// Score : nombre total de pièces ramassées dans la partie (bonus respawn inclus).
    score: u32,
    /// File d'événements de gameplay : noms émis pendant le tick courant
    /// (par un script via `emit("nom")`, ou par le moteur — ex. `score:N` à chaque
    /// point marqué), **délivrés aux scripts au tick fixe suivant** via
    /// `on_event("nom")` puis jetés — un événement se consomme en un tick, il ne
    /// s'accumule pas (sinon `on_event` resterait vrai pour toujours et la file
    /// grossirait sans borne). Le décalage d'un tick évite tout ordre de traitement
    /// intra-tick : peu importe quel objet émet ou écoute en premier dans la boucle
    /// des scripts, tous les auditeurs voient l'événement au même tick.
    game_events: Vec<String>,
    /// Zones de déclenchement actives au tick **précédent** (cf. `sim_step`) :
    /// indices d'objets `trigger` en contact avec le joueur au tick d'avant. Sert à
    /// détecter la sortie (`obj.exited`) — le tick où un objet quitte cet ensemble sans
    /// y être revenu — plutôt que de la déduire uniquement de `obj.triggered` qui ne
    /// dit que « en contact maintenant », jamais « vient de cesser de l'être ».
    trigger_prev: std::collections::HashSet<usize>,
    /// Créatures `Archetype::Furtive` déjà réveillées (Phase O Sprint 1,
    /// `sprint2audijeu0718.md`, GDD §5.4) : indices d'objets pour lesquels
    /// `Sfx::CreatureWake` a déjà été joué, pour ne le jouer **qu'une fois**
    /// par éveil — sans cette mémoire, le son rejouerait à chaque frame tant
    /// que le joueur reste à portée (`FURTIVE_DETECT_RANGE`), pas seulement
    /// au moment de la transition endormie → active. Même politique que
    /// `trigger_prev` ci-dessus (vidé à `restart_game`/à l'entrée en Play).
    furtive_awake: std::collections::HashSet<usize>,
    /// Variables de script persistantes, lues/écrites en Lua via
    /// `save.get("clé")`/`save.set("clé", valeur)` — contrairement à `game_events`,
    /// ne se vide jamais toute seule : c'est l'état que `runtime::savegame::SaveGame`
    /// capture/restaure (avec le score et les positions). Global au jeu, pas par objet
    /// : n'importe quel script peut lire ce qu'un autre a écrit le même tick ou avant.
    pub(crate) lua_vars: std::collections::HashMap<String, f64>,
    /// File de réapparition : (index de pièce, temps de jeu auquel la rendre visible).
    respawn_queue: Vec<(usize, f32)>,
    /// Sac du joueur : (sorte, quantité) par ordre de première découverte —
    /// cf. `app::inventory` (ramassage, empilement, utilisation des consommables).
    inventory: Vec<(crate::scene::ItemKind, u32)>,
    /// Niveau courant de la démo contrôleur (1-based).
    level: u32,
    /// « Aperçu mobile » : restreint la vue 3D à un écran de téléphone (letterbox).
    pub device_preview: bool,
    /// Orientation de l'aperçu mobile (portrait par défaut).
    pub device_portrait: bool,
    /// Région centrale 3D (hors panneaux) en pixels physiques `(x, y, w, h)`,
    /// remontée par l'éditeur ; base de l'aperçu mobile. `(0,0,0,0)` = plein écran.
    pub view_rect_px: (f32, f32, f32, f32),
    /// Barre de vie du HUD (0..1) pilotée par `set_health` ; `None` = pas de barre.
    pub hud_health: Option<f32>,
    /// Qualité de rendu visée (cf. `build_config::RenderQuality`) : relue depuis la
    /// config persistée à chaque entrée en Play, pilote le nombre de lumières
    /// ponctuelles envoyées au shader (perf en mode interactif « Basse » qualité).
    pub render_quality: crate::app::build_config::RenderQuality,
    /// Bloom activé pour ce build (`build_config::BuildConfig::bloom`) :
    /// relu comme `render_quality` ci-dessus. Combiné à
    /// `RenderQuality::bloom_enabled()` (opt-out automatique sur qualité « Basse ») —
    /// les deux doivent être vrais pour que le renderer calcule le bloom.
    pub bloom_enabled: bool,
    /// Intensité (1 = pic, décroît vers 0) du flash de dégâts (vignette rouge HUD),
    /// déclenché quand `hud_health` baisse. Purement cosmétique (retour de coup).
    pub damage_flash: f32,
    /// Intensité (1 = pic, décroît vers 0) du recul caméra à l'encaissement d'un
    /// coup (secousse brève) — même déclencheurs que `damage_flash`, décroissance
    /// séparée pour pouvoir ajuster l'un sans l'autre (Sprint 1, `sprint10audit.md`).
    pub camera_shake: f32,
    /// Coupe le recul caméra calculé par `camera_shake_offset` sans toucher
    /// `camera_shake` lui-même — persisté dans `Settings::reduce_shake`
    /// (PHASE I Sprint 1, accessibilité §16.6).
    pub reduce_shake: bool,
    /// Intensité (1 = pic, décroît vers 0) de la bannière « allié à terre »,
    /// déclenchée par `GameEvent::PlayerDown` d'un **autre** joueur réseau
    /// (GDD §5.3 : « la mort d'un allié est un événement de groupe » — jusqu'ici
    /// seule notre propre mort déclenchait un retour, `network_client.rs`).
    pub ally_down_flash: f32,
    /// Cause résumée de notre dernière mort (Sprint 2, `sprint10audit.md`,
    /// GDD §16.5), affichée par `editor::hud::defeated_banner` tant qu'on
    /// reste spectateur — `None` par défaut ou si le serveur n'a diffusé
    /// aucune cause (ex. vie mise à 0 sans dégât mémorisé).
    pub death_cause: Option<crate::net::protocol::DeathCause>,
    /// Intensité (1 = pic, décroît vers 0) de l'effet 3D d'attaque : téléporte et affiche
    /// brièvement l'objet `is_attack_fx` sur la cible touchée (rend le coup lisible).
    pub attack_flash: f32,
    /// Résumé par joueur de la dernière manche réseau décidée (Phase H,
    /// Sprint 1, GDD §9.2/§17.4), reçu via `GameEvent::Win`/`Lose`
    /// (`network_client::handle_server_msg`) — `None` avant la première
    /// manche décidée ou après `restart_game`. Affiché par
    /// `editor::hud::round_summary_banner` tant que présent.
    pub round_summary: Option<Vec<crate::net::protocol::RoundPlayerSummary>>,
    /// Issue de `round_summary` : `true` si diffusé par `GameEvent::Win`,
    /// `false` par `GameEvent::Lose` — sans signification si `round_summary`
    /// est `None`.
    pub round_summary_won: bool,
    /// Libellé du Contrat du jour rempli par la manche de `round_summary`
    /// (`GameEvent::Win::contract`, GDD §3.4/§3.5), `None` si aucun contrat
    /// n'a été rempli ou sur une défaite.
    pub round_contract_label: Option<&'static str>,
    /// Intensité (1 = pic, décroît vers 0) de la bannière de vague (Phase H,
    /// Sprint 2, GDD §17.2), déclenchée par `GameEvent::WaveStart` — même
    /// mécanisme que `ally_down_flash`.
    pub wave_banner_flash: f32,
    /// Numéro de la vague annoncée par la dernière `GameEvent::WaveStart`
    /// reçue, affiché tant que `wave_banner_flash > 0`.
    pub wave_banner_wave: u32,
    /// Intensité (1 = pic, décroît vers 0) de la bannière « palier atteint »
    /// (GDD §8.2/§17 : un palier = un déblocage nommé, affiché au moment où
    /// il tombe) — armée par `check_palier_atteint` à la réception du résumé
    /// de fin de manche, même mécanisme que `wave_banner_flash`.
    pub palier_flash: f32,
    /// Niveau-palier annoncé par la bannière ci-dessus (3, 6 ou 10), affiché
    /// tant que `palier_flash > 0`.
    pub palier_level: u32,
    /// Base visuelle (échelle, couleur) des objets avant application d'une
    /// silhouette de classe (v7, GDD §10.3, `apply_class_silhouette`) —
    /// mémorisée à la première application par indice d'objet, pour que
    /// réappliquer une silhouette (reconnexion, slot de fantôme réutilisé)
    /// reparte toujours du gabarit neutre au lieu de composer les facteurs.
    silhouette_base: std::collections::HashMap<usize, (Vec3, [f32; 3])>,
    /// XP cumulée du compte Firebase telle que connue du client : lue une
    /// fois à la connexion (`request_firebase_auth`, via `get_progress`),
    /// puis avancée localement à chaque résumé de manche — sert uniquement à
    /// détecter le franchissement d'un palier, la vérité comptable reste le
    /// serveur (`award_progress`). `None` = anonyme ou lecture échouée
    /// (aucune bannière plutôt qu'une fausse).
    pub firebase_xp: Option<u32>,
    /// Manche courante (1-based) d'un système de vagues (cf. `Combat::wave`) ; 0 = pas
    /// de système de manches dans la scène courante (les autres démos). Toutes les
    /// cibles de la manche courante vaincues ⇒ manche suivante révélée ; dernière
    /// manche vaincue ⇒ victoire (cf. `update_waves` dans `advance_play`).
    pub wave: u32,
    /// Objectif de la manche courante (Phase C, `sprint10audit.md`) : décide
    /// quelle condition de victoire/défaite `update_round` applique (cf.
    /// `app::combat`). Défaut `Vagues` (comportement historique, seul mode qui
    /// existait avant ce sprint) — zéro régression pour une scène/salon qui ne
    /// choisit pas de mode. Fixé côté salon multijoueur par `bin/server.rs::
    /// Lobby::objective` (propagé au `Join`, cf. `multiplayer::RoundObjective`) ;
    /// n'a d'effet que si la scène a un système de manches (`wave > 0`).
    pub objective: multiplayer::RoundObjective,
    /// Nombre de `GameEvent::PlayerDown` survenus depuis le début de la manche
    /// courante (Phase D, Sprint 9 de `sprint10audit.md` — contrat « Nuit
    /// blanche », GDD §3.4 : « gagnez sans qu'aucun Veilleur ne tombe »).
    /// Remis à zéro à chaque nouvelle manche (`AppState::new`, appelé par
    /// `Room::restart`) — jamais décrémenté : contrairement à `network_health`,
    /// une réanimation ultérieure ne doit pas effacer qu'une chute a eu lieu.
    pub player_down_count: u32,
    /// Nombre de réanimations **achevées** depuis le début de la manche
    /// courante (Phase D, Sprint 9 — contrat « La lande garde ses morts »,
    /// GDD §3.4 : « gagnez sans réanimation »). Distinct de `network_revive`
    /// (canal de réanimation *en cours*, purgé dès qu'elle se termine) : ce
    /// compteur, lui, persiste au-delà de la fin du canal.
    pub revives_completed: u32,
    /// La scène courante est-elle la démo contrôleur à niveaux (`Scene::controller_level`) ?
    /// Seule cette famille de scènes a un « niveau suivant » (cf. `next_level`) ; toute
    /// autre victoire (course infinie, tour, manches de zombies...) doit juste relancer
    /// la même scène au clic sur « Rejouer », pas basculer vers l'arène de combat.
    pub is_leveled_demo: bool,
    /// Temps restant (s) avant la prochaine attaque possible (cf. `Controller::attack_cooldown`).
    /// Sans ce temporisateur, maintenir le bouton défait instantanément tout ce qui entre
    /// en portée, sans le moindre risque — verrouillé par un test dédié.
    attack_cooldown_remaining: f32,
    /// Missile d'attaque en vol (cf. `Scene::attack_at` → tir à distance) : `None` = pas
    /// de tir en cours. L'impact réel (mise à mort) n'est résolu qu'à l'arrivée, pas au
    /// moment du tir — laisse le temps à la cible de continuer d'approcher, donc de
    /// mordre avant que le coup ne porte (le vrai risque qu'une résolution instantanée
    /// ne pouvait pas garantir, cf. audit_sprint.md).
    attack_projectile: Option<AttackProjectile>,
    /// Préparation d'attaque en cours (cf. `Controller::attack_windup`) : `None` = pas de
    /// tir en préparation. La cible est verrouillée dès l'appui, mais le missile ne part
    /// qu'une fois le temps de préparation écoulé — le joueur reste exposé pendant ce
    /// temps (aucune invulnérabilité), créant enfin un vrai risque en 1 contre 1 (cf.
    /// audit_sprint.md : le temps de vol du missile seul ne suffisait pas).
    attack_charge: Option<AttackCharge>,
    /// Reculs (knockback) en cours : (indice d'objet, vitesse horizontale, temps restant
    /// en s) — cf. `KNOCKBACK_SPEED`/`KNOCKBACK_DURATION`. Prioritaire sur le pilotage
    /// IA tant que le temps n'est pas écoulé (sinon la poursuite écraserait le recul dès
    /// la frame suivante).
    stagger: Vec<(usize, Vec3, f32)>,
    /// Joueurs réseau connectés (cf. `multiplayer.rs`) : indice de
    /// l'objet de scène que chacun pilote, dans `scene.objects`.
    network_players: HashMap<crate::net::protocol::PlayerId, usize>,
    /// Dernier `Input` reçu de chaque joueur réseau (remplacé, pas cumulé : le
    /// client renvoie son état complet à chaque message).
    network_inputs: HashMap<crate::net::protocol::PlayerId, multiplayer::NetworkInput>,
    /// Temps de recharge (s) restant avant la prochaine attaque possible de
    /// chaque joueur réseau (cf. `multiplayer::update_network_attacks`).
    network_attack_cooldowns: HashMap<crate::net::protocol::PlayerId, f32>,
    /// Vie individualisée de chaque joueur réseau (0..1, cf. `app::health`,
    /// GAMEDESIGN_EN_LIGNE.md §3.1) : remplace le champ scalaire unique
    /// (`hud_health`, pensé pour un seul joueur local) côté multijoueur — un
    /// joueur peut désormais mourir sans que la manche entière échoue pour tous.
    network_health: HashMap<crate::net::protocol::PlayerId, f32>,
    /// Frags individualisés par joueur réseau (cf. `app::health`) : nombre de
    /// monstres vaincus par **ce** joueur depuis sa connexion, toutes méthodes
    /// confondues (attaque au contact, boule de feu) — brique de progression
    /// pour un futur MMORPG (contribution individuelle, pas un score de salon
    /// partagé). Diffusé à tous via `EntityDelta::kills`, pas seulement au
    /// joueur concerné.
    network_kills: HashMap<crate::net::protocol::PlayerId, u32>,
    /// Assists individualisés par joueur réseau (GDD §8.3) : nombre de fois où
    /// **ce** joueur a porté un dégât à un monstre achevé par un autre joueur
    /// peu après (cf. `multiplayer::credit_assists_on_kill`) — distinct de
    /// `network_kills`, jamais incrémentés pour la même mise à mort (le
    /// tireur reçoit le frag, les autres contributeurs l'assist).
    network_assists: HashMap<crate::net::protocol::PlayerId, u32>,
    /// Dernier instant (`self.time`) où chaque joueur réseau a porté un dégât
    /// à chaque monstre encore vivant (indice d'objet → joueur → instant) —
    /// mémoire courte servant uniquement à décider qui a droit à un assist
    /// quand ce monstre meurt (cf. `multiplayer::ASSIST_WINDOW`), purgée à
    /// chaque mise à mort résolue (`credit_assists_on_kill`) pour ne jamais
    /// compter sur la vie suivante du même emplacement après respawn.
    damage_contributions: HashMap<usize, HashMap<crate::net::protocol::PlayerId, f32>>,
    /// Classe choisie par chaque joueur réseau au `Join` (cf.
    /// `multiplayer::PlayerClass`, GAMEDESIGN_MMORPG.md §3.2) — appliquée une
    /// fois pour toutes au spawn (vitesse, PV max), relue pour les
    /// modificateurs qui dépendent du tick courant (dégâts, soin, réanimation).
    network_classes: HashMap<crate::net::protocol::PlayerId, multiplayer::PlayerClass>,
    /// PV max de chaque joueur réseau (base `health::MAX_HEALTH` modulée par
    /// sa classe, ex. Éclaireur ×0,70) — remplace la constante plate partout
    /// où la vie est clampée ou testée à pleine vie (cf. `health::max_health_for`).
    network_max_health: HashMap<crate::net::protocol::PlayerId, f32>,
    /// Réanimation en cours (GDD §8.1, exclusivité Soutien) : pour chaque
    /// **soigneur**, la cible spectatrice qu'il canalise et le temps déjà
    /// accumulé (s) — remis à zéro si la cible change ou si le canal
    /// s'interrompt (cf. `health::update_network_revive`).
    network_revive: HashMap<crate::net::protocol::PlayerId, (crate::net::protocol::PlayerId, f32)>,
    /// Cooldown restant (s) par paire (indice de créature mordeuse, joueur réseau)
    /// — cf. `health::update_creature_bite`. Clé composite plutôt qu'un cooldown
    /// par créature seule : deux joueurs au contact de la même créature ne
    /// doivent pas partager un seul temporisateur (l'un mordu ne doit pas
    /// « protéger » l'autre).
    bite_cooldowns: HashMap<(usize, crate::net::protocol::PlayerId), f32>,
    /// Dernières sources de dégâts subies par chaque joueur réseau — type
    /// d'agresseur et indice de l'objet attaquant — bornées à
    /// `health::DEATH_CAUSE_WINDOW` (Sprint 2, `sprint10audit.md`) :
    /// consommées à la mort pour calculer `net::protocol::DeathCause`
    /// (diagnostic de mort, GDD §16.5), purgées ensuite (`health::
    /// compute_death_cause`).
    recent_damage: HashMap<
        crate::net::protocol::PlayerId,
        VecDeque<(crate::net::protocol::DeathCauseKind, usize)>,
    >,
    /// Boules de feu en vol (cf. `fireball.rs`) : simulées ici en solo **et** sur
    /// le serveur autoritaire (joueurs réseau) — un client connecté n'en simule
    /// aucune, il affiche celles du `Snapshot` (cf. `net_projectiles`).
    fireballs: Vec<fireball::Fireball>,
    /// Temps de recharge (s) restant par **objet tireur** (indice dans
    /// `scene.objects`) : la même table sert au joueur local (solo) et aux joueurs
    /// réseau (serveur) — validation côté simulation, un client qui spamme
    /// `fire: true` ne tire pas plus vite pour autant.
    fireball_cooldowns: HashMap<usize, f32>,
    /// Pool d'objets de scène (sphères émissives) réutilisés pour afficher les
    /// boules de feu — créés à la demande, masqués quand inutilisés, jamais
    /// retirés en cours de partie (retirer décalerait tous les indices).
    fireball_pool: Vec<usize>,
    /// Projectiles (position + arme) reçus du dernier `Snapshot` serveur (client
    /// connecté uniquement) : affichés tels quels via le pool.
    net_projectiles: Vec<(Vec3, usize)>,
    /// États des attaques à distance des créatures PNJ (pistolet à eau de la
    /// n°3, feu de la n°8, étincelle de la n°9, spore de la n°10) — un état
    /// par entrée de `creature_attack::RANGED_CREATURE_ATTACKS`, même indice.
    creature_ranged: Vec<creature_attack::RangedState>,
    /// Projectiles de créatures en vol (cf. `creature_attack::CreatureShot`).
    creature_shots: Vec<creature_attack::CreatureShot>,
    /// Pool d'affichage des projectiles de créatures, même principe que
    /// `fireball_pool`.
    creature_shot_pool: Vec<usize>,
    /// Projectiles de créature (position, direction, config) reçus du dernier
    /// `Snapshot` serveur (client connecté uniquement) — même principe que
    /// `net_projectiles`, affichés tels quels via le pool.
    net_creature_shots: Vec<(Vec3, Vec3, usize)>,
    /// Arme à distance équipée par le joueur local (indice dans
    /// `fireball::RANGED_WEAPONS`) : clavier 1/2/3, ou bouton tactile « Arme »
    /// qui cycle (cf. `Controller::weapon_button`). Envoyée au serveur à chaque
    /// `Input` quand ce client est en ligne.
    selected_weapon: usize,
    /// État du bouton tactile « Arme » à la frame précédente : le cycle ne se
    /// déclenche que sur le front montant (l'overlay réécrit `buttons` à chaque
    /// frame — sans ça, un appui ferait défiler toutes les armes en rafale).
    weapon_button_was_down: bool,
    /// Évènements de gameplay produits par la simulation (ex. monstre vaincu par
    /// une boule de feu) à diffuser aux clients — drainés par le serveur headless
    /// à chaque tick (cf. `take_net_events`). Reste vide hors serveur (le joueur
    /// solo entend ses sons directement, il n'a personne à prévenir).
    pending_net_events: Vec<crate::net::protocol::GameEvent>,
    /// Connexion au serveur multijoueur (cf. `network_client.rs`), si ce client
    /// a rejoint une partie en ligne. Desktop + Android seulement : `net::client`
    /// dépend de `tokio`, pas encore ciblé sur iOS (cf. `net/mod.rs`).
    #[cfg(not(target_os = "ios"))]
    net_client: Option<crate::net::client::NetClient>,
    /// Identifiant attribué par le serveur à ce client (`ServerMsg::Welcome`),
    /// une fois connecté. Sert à repérer sa propre entité dans les `Snapshot`
    /// reçus (cf. `net_local_interp` : le serveur reste maître même de la
    /// position du joueur local, `network_client::poll_network` se contente
    /// d'afficher ce qu'il reçoit).
    net_player_id: Option<crate::net::protocol::PlayerId>,
    /// Message de statut réseau à afficher dans l'UI (connecté/déconnecté/erreur).
    pub net_status: String,
    /// Autres joueurs réseau visibles par ce client, affichés comme des
    /// « fantômes » (objet de scène dont la position suit le dernier `Snapshot`
    /// reçu, interpolée — cf. `net::interpolation::RemoteEntity`), pas simulés
    /// localement (le serveur est autoritaire sur eux).
    remote_players: HashMap<crate::net::protocol::PlayerId, network_client::RemotePlayer>,
    /// `true` si un fantôme réseau (joueur distant ou créature diffusée par le
    /// serveur) a changé de visibilité depuis le dernier appel à
    /// `network_client::poll_network` — un fantôme masqué n'a pas de corps
    /// physique (cf. `runtime::physics::Physics::build`), donc chaque
    /// bascule doit reconstruire le monde physique pour que son collider
    /// apparaisse/disparaisse (même mécanisme que `App::update_waves` pour
    /// une manche révélée). Remis à `false` une fois le rebuild fait.
    net_visibility_dirty: bool,
    /// Horodatage du dernier `Snapshot` reçu couvrant chaque créature autoritaire
    /// (indexée par `SceneObject`, cf. `network_client::handle_server_msg`). Sert
    /// de filet de secours (`simulation::advance_play`) : si le serveur ne
    /// diffuse jamais (room jointe sans succès, scène désynchronisée) ou cesse
    /// de diffuser une créature donnée, on reprend sa simulation locale plutôt
    /// que de la laisser figée pour toujours — cf. `[[creature-freeze-...]]`.
    net_creature_last_snapshot: HashMap<usize, Instant>,
    /// Historique (2 derniers points) de la position du joueur **local** telle
    /// que rapportée par le serveur — même mécanisme d'interpolation que les
    /// fantômes des autres joueurs (`RemoteEntity`). Sert de référence
    /// autoritative pour la réconciliation (`apply_local_network_position`) :
    /// le joueur local reste piloté par prédiction immédiate (`sim_step`), le
    /// serveur ne corrige que si l'écart dépasse `interpolation::SNAP_THRESHOLD`.
    #[cfg(not(target_os = "ios"))]
    net_local_interp: crate::net::interpolation::RemoteEntity,
    /// Dernière vie connue du joueur local (0..1, cf. `app::health`,
    /// GAMEDESIGN_EN_LIGNE.md §3.1/§3.4) : lue telle quelle du dernier
    /// `Snapshot` reçu pour notre propre `PlayerId` — même principe que
    /// `RemotePlayer::health` pour les autres joueurs. `None` hors ligne ou
    /// avant le premier snapshot.
    #[cfg(not(target_os = "ios"))]
    net_local_health: Option<f32>,
    /// Frags individualisés connus du joueur local (brique de progression pour
    /// un futur MMORPG) — même principe que `net_local_health` : lu tel quel
    /// du dernier `Snapshot`. `None` hors ligne ou avant le premier snapshot.
    #[cfg(not(target_os = "ios"))]
    net_local_kills: Option<u32>,
    /// Assists individualisés connus du joueur local (Phase L Sprint 3,
    /// `sprint2audijeu0718.md`, GDD §8.3) — même principe que `net_local_kills`.
    #[cfg(not(target_os = "ios"))]
    net_local_assists: Option<u32>,
    /// Historique court (~1 s) des positions **prédites** du joueur local, une par
    /// frame (cf. `apply_local_network_position`). La position renvoyée par le
    /// serveur est en retard d'une latence aller-retour + un tick : la comparer à la
    /// position prédite *instantanée* la déclare « désynchronisée » dès qu'on bouge
    /// (écart ≈ vitesse × latence ≈ 1 m au-delà de `SNAP_THRESHOLD` à 4,5 m/s sur le
    /// VPS réel) — d'où une correction continue qui freinait et faisait trembler le
    /// personnage en pleine course. La position
    /// serveur est donc validée contre la **trajectoire récente** : si elle est
    /// proche d'un point où l'on est réellement passé, on est en phase (le serveur
    /// est juste en retard), pas de correction.
    #[cfg(not(target_os = "ios"))]
    net_local_history: std::collections::VecDeque<(crate::time_compat::Instant, Vec3)>,
    /// Horodatage du dernier `ClientMsg::Input` envoyé au serveur : `poll_network`
    /// est appelée une fois par frame de rendu, potentiellement bien au-dessus du
    /// tick serveur — ce champ sert à plafonner le débit d'envoi à
    /// `network_client::INPUT_SEND_INTERVAL` plutôt que d'envoyer un message par
    /// frame affichée.
    #[cfg(not(target_os = "ios"))]
    net_last_input_sent: Option<crate::time_compat::Instant>,
    /// Horodatage du dernier `ServerMsg` reçu, **quel qu'il soit** (`Welcome`,
    /// `Snapshot`, évènement… — tout message prouve que le serveur est vivant).
    /// Watchdog applicatif (`network_client::NET_SILENCE_TIMEOUT`) : le
    /// transport peut être à moitié mort (TCP half-open, façade Caddy qui gèle)
    /// sans que `NetClient::is_alive()` ne bascule — un silence prolongé est
    /// alors le seul symptôme. Armé dès la connexion (pas seulement au premier
    /// message), pour couvrir aussi un serveur qui accepte la socket mais ne
    /// répond jamais.
    #[cfg(not(target_os = "ios"))]
    net_last_server_msg: Option<crate::time_compat::Instant>,
    /// Paramètres `(url, nom, salon)` de la dernière connexion **réussie** —
    /// ce que la reconnexion automatique rejoue à l'identique après une
    /// coupure (cf. `network_client::poll_network`). `None` tant qu'on ne
    /// s'est jamais connecté, et remis à `None` par une déconnexion
    /// **volontaire** (`disconnect_from_server`) : quitter la partie ne doit
    /// jamais déclencher une reconnexion dans le dos du joueur.
    /// Quatrième champ (Sprint 3, `sprint10audit.md`) : la classe choisie
    /// (`multiplayer::PlayerClass::to_u8`) au `Join` initial — rejouée à
    /// l'identique par la reconnexion automatique, comme le reste du tuple.
    /// Cinquième champ (Sprint 21, `sprintreflecion.md`) : le mode de manche
    /// choisi (`multiplayer::RoundObjective::to_u8`), même principe.
    #[cfg(not(target_os = "ios"))]
    net_last_connect: Option<(String, String, String, u8, u8)>,
    /// Reconnexion automatique en cours, s'il y en a une (cf.
    /// `network_client::ReconnectState` : numéro de tentative, prochain essai,
    /// tentative de fond éventuellement en vol). `None` = connexion saine ou
    /// définitivement abandonnée.
    #[cfg(not(target_os = "ios"))]
    net_reconnect: Option<network_client::ReconnectState>,
    /// `uid` Firebase du joueur local une fois connecté (`sign_in`/`sign_up`,
    /// cf. `network_client`) : transmis au `Join` pour que le serveur puisse
    /// créditer la progression au bon compte. `None` = partie anonyme, sans
    /// compte.
    firebase_uid: Option<String>,
    /// Une requête Firebase (sign in/up) est en cours (évite d'en empiler
    /// plusieurs si l'utilisateur clique deux fois).
    firebase_busy: bool,
    /// Canal de résultat des requêtes Firebase (thread de fond, cf. les
    /// requêtes IA existantes) : `Ok(uid)` ou message d'erreur. Types
    /// universels (`String`) : pas besoin de gater ces champs par plateforme,
    /// seules les fonctions qui les produisent (`net::firebase::sign_in`/
    /// `sign_up`) le sont.
    /// `Ok((uid, id_token))` ou message d'erreur.
    firebase_tx: std::sync::mpsc::Sender<FirebaseAuthResult>,
    firebase_rx: std::sync::mpsc::Receiver<FirebaseAuthResult>,
    /// Jeton d'authentification Firebase (`?auth=...`), nécessaire pour poster
    /// un message de chat (écriture réservée aux comptes connectés). `None`
    /// tant qu'aucun `sign_in`/`sign_up` n'a réussi.
    firebase_id_token: Option<String>,
    /// Derniers messages de chat connus (dernier `request_refresh_chat`
    /// réussi) ; `ChatLine` est une représentation universelle (pas
    /// `net::firebase::ChatMessage`, absent des cibles mobiles), cf.
    /// `network_client`.
    pub chat_messages: Vec<network_client::ChatLine>,
    /// Une requête de chat (envoi ou rafraîchissement) est en cours.
    chat_busy: bool,
    chat_tx: std::sync::mpsc::Sender<Result<Vec<network_client::ChatLine>, String>>,
    chat_rx: std::sync::mpsc::Receiver<Result<Vec<network_client::ChatLine>, String>>,
    /// Dernier classement connu (dernier `request_refresh_leaderboard` réussi).
    pub leaderboard: Vec<network_client::LeaderboardLine>,
    /// Une requête de classement est en cours.
    leaderboard_busy: bool,
    leaderboard_tx: std::sync::mpsc::Sender<Result<Vec<network_client::LeaderboardLine>, String>>,
    leaderboard_rx: std::sync::mpsc::Receiver<Result<Vec<network_client::LeaderboardLine>, String>>,
    /// Derniers `uid` en ligne connus (dernier `request_refresh_online_players`
    /// réussi, cf. `net::firebase::list_online_players` — Phase L Sprint 1,
    /// `sprint2audijeu0718.md`). Présence globale par compte Firebase, pas
    /// filtrée par salon : la RTDB ne garde pas trace du salon dans
    /// `presence/<uid>`, seulement le dernier heartbeat.
    pub online_players: Vec<String>,
    /// Une requête de présence (rafraîchissement ou heartbeat) est en cours.
    online_players_busy: bool,
    online_players_tx: std::sync::mpsc::Sender<Result<Vec<String>, String>>,
    online_players_rx: std::sync::mpsc::Receiver<Result<Vec<String>, String>>,
    /// Grille de référence au sol affichée en mode édition.
    pub show_grid: bool,
    /// Aimantation : les translations/rotations au gizmo s'alignent sur un pas
    /// (0.5 en position, 15° en rotation — cf. `picking::maybe_snap`/`maybe_snap_angle`).
    pub snap: bool,
    /// Touche modificatrice tenue (Ctrl) pendant un glissé de gizmo (Sprint 112) :
    /// inverse temporairement `snap` — permet un ajustement fin ponctuel sans
    /// changer le réglage persistant, ou l'inverse (aimanter ponctuellement sans
    /// l'activer globalement). Positionné par la plateforme (`set_snap_modifier`),
    /// jamais persisté (état d'entrée pure, comme `additive`).
    snap_modifier: bool,
    /// Vue de debug du rendu : Éclairé/Normales/Profondeur.
    pub debug_view: DebugView,
    pub camera: OrbitCamera,

    viewport: (f32, f32),
    last_frame: Instant,
    /// Images par seconde lissées (moyenne mobile exponentielle), pour le bandeau d'état.
    fps: f32,
    /// Fenêtre du bilan de perf périodique (cf. `log_perf_window` dans
    /// `simulation.rs`) : début de la fenêtre courante et pire `dt` observé dedans.
    perf_window_start: Instant,
    perf_window_worst_dt: f32,
    /// Pire durée d'`advance_play` (simulation seule) sur la fenêtre courante —
    /// pas forcément la même frame que `perf_window_worst_dt`, c'est un
    /// indicateur de *quel côté* (sim vs rendu/présentation) chercher un à-coup.
    perf_window_worst_sim: f32,

    // --- état d'interaction pointeur ---
    dragging: bool,
    /// Pan forcé en cours (clic milieu / Maj+glisser) : déplace la caméra quel
    /// que soit l'outil actif, sans passer par le gizmo ni la sélection.
    pan_dragging: bool,
    last_cursor: Option<(f64, f64)>,
    press_cursor: Option<(f64, f64)>,

    // --- gizmo ---
    pub gizmo_mode: GizmoMode,
    /// Axe en cours de manipulation (0 = X, 1 = Y, 2 = Z).
    pub active_axis: Option<usize>,
    drag_start_t: f32,
    drag_start_angle: f32,
    drag_orig_pos: Vec3,
    drag_orig_rot: Quat,
    drag_orig_scale: Vec3,
    /// Positions d'origine de tous les objets sélectionnés (gizmo translate multi).
    drag_orig_positions: Vec<(usize, Vec3)>,
    /// Transforms d'origine de la sélection (gizmo rotate/scale multi, autour d'un pivot).
    drag_orig_transforms: Vec<(usize, Transform)>,
    /// Pivot commun (centroïde de la sélection) pour rotate/scale multi.
    drag_pivot: Vec3,
    /// Le prochain clic ajoute/retire de la sélection (Cmd/Maj enfoncé).
    additive: bool,
    /// Lumière ponctuelle sélectionnée (déplaçable au gizmo) ; exclusif avec `selection`.
    pub selected_light: Option<usize>,
    /// Lumière en cours de déplacement au gizmo (avec `active_axis`).
    drag_light: Option<usize>,

    // --- historique (snapshots de la liste d'objets) ---
    undo_stack: VecDeque<SceneSnapshot>,
    redo_stack: Vec<SceneSnapshot>,

    // --- scripting (indisponible sur wasm32, cf. Cargo.toml : `lua-src` ne
    // sait pas construire Lua pour `wasm32-unknown-unknown` — Sprint 114) ---
    #[cfg(not(target_arch = "wasm32"))]
    lua: Lua,
    /// Breakpoints Lua basiques (Sprint 128) — cf. `scripting::LuaBreakpoints`.
    #[cfg(not(target_arch = "wasm32"))]
    lua_breakpoints: scripting::LuaBreakpoints,
    /// Chunks Lua déjà compilés, indexés par hash de la source (évite de re-parser
    /// le même script à chaque frame).
    #[cfg(not(target_arch = "wasm32"))]
    script_cache: HashMap<u64, mlua::Function>,
    /// Backend Lua du player web (Sprint 137) — symétrique de `lua`/`script_cache`
    /// ci-dessus, sur `rilua` au lieu de `mlua` (cf. `scripting_web`). Pas de
    /// breakpoints ici : fonctionnalité éditeur, absente du player web.
    #[cfg(target_arch = "wasm32")]
    lua_web: rilua::Lua,
    #[cfg(target_arch = "wasm32")]
    script_cache_web: HashMap<u64, rilua::Function>,
    time: f32,

    // --- runtime Play ---
    was_playing: bool,
    play_snapshot: Vec<SceneObject>,
    physics: Option<crate::runtime::physics::Physics>,
    audio: crate::runtime::audio::Audio,

    // --- import glTF asynchrone ---
    import_tx: Sender<ImportResult>,
    import_rx: Receiver<ImportResult>,

    // --- chargement de scène asynchrone (Load) ---
    scene_load_tx: Sender<Result<Scene, String>>,
    scene_load_rx: Receiver<Result<Scene, String>>,
    /// Vrai après remplacement de la scène : le renderer doit reconstruire les meshes GPU importés.
    imported_dirty: bool,

    // --- génération de script par IA (asynchrone) ---
    ai_tx: Sender<(usize, Result<String, String>)>,
    ai_rx: Receiver<(usize, Result<String, String>)>,
    /// Une génération IA est en cours (désactive le bouton, affiche l'état).
    pub ai_busy: bool,
    // --- génération de scène entière par IA (asynchrone) ---
    ai_scene_tx: Sender<Result<Scene, String>>,
    ai_scene_rx: Receiver<Result<Scene, String>>,
    /// Mode de la génération de scène en cours : `true` = remplacer, `false` = ajouter.
    ai_scene_replace: bool,
}

/// Mode de manipulation du gizmo (touches W / E / R) ou outil de navigation
/// caméra (Main / Orbite / Loupe) — un seul outil actif à la fois, choisi dans
/// la barre d'outils.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
    /// 🖐 Main (touche Q) : glisser = pan de la caméra.
    Pan,
    /// 🔄 Orbite libre : glisser = yaw **et** pitch (contrairement à l'orbite
    /// par défaut, volontairement limitée au yaw).
    Orbit,
    /// 🔍 Loupe : glisser verticalement = zoom avant/arrière.
    Zoom,
}

impl GizmoMode {
    /// Outil de navigation caméra : pas de gizmo dessiné, pas de sélection au clic.
    pub fn is_nav(self) -> bool {
        matches!(self, GizmoMode::Pan | GizmoMode::Orbit | GizmoMode::Zoom)
    }
}

/// Vue de debug du rendu principal : remplace l'éclairage par une
/// visualisation directe d'une grandeur du pipeline. Encodé en `f32` (0/1/2) dans un canal
/// inutilisé de l'uniform d'éclairage (`SceneUniform::ambient.y`) plutôt que d'agrandir
/// l'uniform — cf. `write_uniforms` et `main.wgsl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugView {
    #[default]
    Shaded,
    /// Normales monde, remappées en couleur (`n * 0.5 + 0.5`).
    Normals,
    /// Profondeur NDC brute (0 = près du plan proche, 1 = loin) — non linéarisée.
    Depth,
}

impl DebugView {
    pub(crate) fn as_uniform(self) -> f32 {
        match self {
            DebugView::Shaded => 0.0,
            DebugView::Normals => 1.0,
            DebugView::Depth => 2.0,
        }
    }

    pub const ALL: [DebugView; 3] = [DebugView::Shaded, DebugView::Normals, DebugView::Depth];

    pub fn label(self) -> &'static str {
        match self {
            DebugView::Shaded => "Éclairé",
            DebugView::Normals => "Normales",
            DebugView::Depth => "Profondeur",
        }
    }
}

/// Longueur (monde) des axes / rayon des anneaux du gizmo. Partagée picking ↔ rendu.
pub const GIZMO_LEN: f32 = 1.0;

/// Nombre de segments par anneau de rotation du gizmo. Partagé picking ↔ rendu
/// pour garantir une géométrie identique des deux côtés.
pub const RING_SEGMENTS: usize = 48;

/// Direction unitaire d'un axe du gizmo.
pub fn axis_dir(axis: usize) -> Vec3 {
    match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    }
}

/// Base orthonormée (u, w) du plan perpendiculaire à un axe.
pub fn axis_basis(a: Vec3) -> (Vec3, Vec3) {
    let reference = if a.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let u = a.cross(reference).normalize();
    let w = a.cross(u).normalize();
    (u, w)
}

impl AppState {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let (scene_tx, scene_rx) = channel();
        let (ai_tx, ai_rx) = channel();
        let (ai_scene_tx, ai_scene_rx) = channel();
        let (firebase_tx, firebase_rx) = channel();
        let (chat_tx, chat_rx) = channel();
        let (leaderboard_tx, leaderboard_rx) = channel();
        let (online_players_tx, online_players_rx) = channel();
        // Volumes musique/SFX (Sprint 104) : lus une fois ici plutôt qu'attendre
        // que l'utilisateur ouvre la fenêtre Paramètres et bouge un slider —
        // `Editor` (qui possède `Settings`) et `AppState` (qui possède `Audio`)
        // sont construits indépendamment, sans moment commun où propager
        // l'un vers l'autre autrement qu'en relisant `Settings::load()` ici.
        let mut audio = crate::runtime::audio::Audio::new();
        let initial_settings = crate::app::settings::Settings::load();
        audio.set_music_volume(initial_settings.music_volume);
        audio.set_sfx_volume(initial_settings.sfx_volume);
        // Breakpoints Lua (Sprint 128) : hook installé une fois ici, sur l'instance
        // `Lua` partagée par tous les scripts d'objet — cf. la doc de
        // `scripting::LuaBreakpoints` pour ce que « pause » signifie concrètement
        // dans ce moteur (pas de coroutines pour les scripts d'objet).
        #[cfg(not(target_arch = "wasm32"))]
        let lua = Lua::new();
        #[cfg(not(target_arch = "wasm32"))]
        let lua_breakpoints = scripting::LuaBreakpoints::new();
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(e) = lua_breakpoints.install(&lua) {
            log::warn!("Breakpoints Lua indisponibles : {e}");
        }
        #[cfg(target_arch = "wasm32")]
        let mut lua_web = rilua::Lua::new().unwrap_or_else(|e| {
            // Jamais observé en pratique (`Lua::new()` échoue seulement si l'allocation
            // initiale du GC échoue) — `expect`-like mais loggé plutôt qu'une panique
            // muette, un état `Lua` par défaut minimal reste préférable à planter tout
            // le player web pour ça.
            log::error!("Initialisation rilua impossible : {e}");
            rilua::Lua::new_empty()
        });
        // GC incrémental désactivé (Sprint 137, constaté en prod : « string expected,
        // got collected string ») : `Table::raw_set`/`create_string` (l'API bas niveau
        // utilisée par `scripting_web`, seule disponible hors bytecode Lua) n'appliquent
        // aucun write barrier — une valeur fraîchement écrite dans une table déjà
        // « noire » (déjà scannée par un cycle incrémental précédent) peut donc être
        // ramassée avant d'être relue. `scripting_web::run_script_web` déclenche des
        // collectes complètes périodiques à la place (`full_gc`, cf. sa doc) : une
        // collecte complète repart de zéro et n'a pas cet écueil.
        #[cfg(target_arch = "wasm32")]
        lua_web.gc_stop();
        AppState {
            scene: Scene::demo(),
            selection: None,
            selected: Vec::new(),
            clipboard: Vec::new(),
            playing: false,
            fly_cam: false,
            paused: false,
            should_quit: false,
            scene_dirty: false,
            confirm_quit: false,
            player: false,
            locale: initial_settings.locale,
            input_state: PlayerInput::default(),
            tapped_obj: None,
            touched_obj: None,
            touch_started_obj: None,
            touch_ended_obj: None,
            sim_accumulator: 0.0,
            time_scale: 1.0,
            step_requested: false,
            debug_lines: Vec::new(),
            sim_prev_poses: Vec::new(),
            sim_curr_poses: Vec::new(),
            sim_render_poses: Vec::new(),
            win_time: None,
            lost: false,
            score: 0,
            game_events: Vec::new(),
            trigger_prev: std::collections::HashSet::new(),
            furtive_awake: std::collections::HashSet::new(),
            lua_vars: std::collections::HashMap::new(),
            respawn_queue: Vec::new(),
            inventory: Vec::new(),
            level: 1,
            device_preview: false,
            device_portrait: true,
            view_rect_px: (0.0, 0.0, 0.0, 0.0),
            hud_health: None,
            render_quality: crate::app::build_config::BuildConfig::load().render_quality,
            bloom_enabled: crate::app::build_config::BuildConfig::load().bloom,
            damage_flash: 0.0,
            camera_shake: 0.0,
            reduce_shake: initial_settings.reduce_shake,
            ally_down_flash: 0.0,
            death_cause: None,
            attack_flash: 0.0,
            round_summary: None,
            round_summary_won: false,
            round_contract_label: None,
            wave_banner_flash: 0.0,
            wave_banner_wave: 0,
            palier_flash: 0.0,
            palier_level: 0,
            silhouette_base: std::collections::HashMap::new(),
            firebase_xp: None,
            wave: 0,
            objective: multiplayer::RoundObjective::default(),
            player_down_count: 0,
            revives_completed: 0,
            is_leveled_demo: false,
            attack_cooldown_remaining: 0.0,
            attack_projectile: None,
            attack_charge: None,
            stagger: Vec::new(),
            network_players: HashMap::new(),
            network_inputs: HashMap::new(),
            network_attack_cooldowns: HashMap::new(),
            network_health: HashMap::new(),
            network_kills: HashMap::new(),
            network_assists: HashMap::new(),
            damage_contributions: HashMap::new(),
            network_classes: HashMap::new(),
            network_max_health: HashMap::new(),
            network_revive: HashMap::new(),
            bite_cooldowns: HashMap::new(),
            recent_damage: HashMap::new(),
            fireballs: Vec::new(),
            fireball_cooldowns: HashMap::new(),
            fireball_pool: Vec::new(),
            net_projectiles: Vec::new(),
            creature_ranged: creature_attack::default_states(),
            creature_shots: Vec::new(),
            creature_shot_pool: Vec::new(),
            net_creature_shots: Vec::new(),
            selected_weapon: 0,
            weapon_button_was_down: false,
            pending_net_events: Vec::new(),
            #[cfg(not(target_os = "ios"))]
            net_client: None,
            net_player_id: None,
            net_status: String::new(),
            remote_players: HashMap::new(),
            net_visibility_dirty: false,
            net_creature_last_snapshot: HashMap::new(),
            #[cfg(not(target_os = "ios"))]
            net_local_interp: crate::net::interpolation::RemoteEntity::default(),
            #[cfg(not(target_os = "ios"))]
            net_local_health: None,
            #[cfg(not(target_os = "ios"))]
            net_local_kills: None,
            #[cfg(not(target_os = "ios"))]
            net_local_assists: None,
            #[cfg(not(target_os = "ios"))]
            net_local_history: std::collections::VecDeque::new(),
            #[cfg(not(target_os = "ios"))]
            net_last_input_sent: None,
            #[cfg(not(target_os = "ios"))]
            net_last_server_msg: None,
            #[cfg(not(target_os = "ios"))]
            net_last_connect: None,
            #[cfg(not(target_os = "ios"))]
            net_reconnect: None,
            firebase_uid: None,
            firebase_busy: false,
            firebase_tx,
            firebase_rx,
            firebase_id_token: None,
            chat_messages: Vec::new(),
            chat_busy: false,
            chat_tx,
            chat_rx,
            leaderboard: Vec::new(),
            leaderboard_busy: false,
            leaderboard_tx,
            leaderboard_rx,
            online_players: Vec::new(),
            online_players_busy: false,
            online_players_tx,
            online_players_rx,
            show_grid: true,
            snap: false,
            snap_modifier: false,
            debug_view: DebugView::default(),
            camera: OrbitCamera::new(1.0),
            viewport: (1.0, 1.0),
            last_frame: Instant::now(),
            fps: 0.0,
            perf_window_start: Instant::now(),
            perf_window_worst_dt: 0.0,
            perf_window_worst_sim: 0.0,
            dragging: false,
            pan_dragging: false,
            last_cursor: None,
            press_cursor: None,
            gizmo_mode: GizmoMode::Translate,
            active_axis: None,
            drag_start_t: 0.0,
            drag_start_angle: 0.0,
            drag_orig_pos: Vec3::ZERO,
            drag_orig_rot: Quat::IDENTITY,
            drag_orig_scale: Vec3::ONE,
            drag_orig_positions: Vec::new(),
            drag_orig_transforms: Vec::new(),
            drag_pivot: Vec3::ZERO,
            additive: false,
            selected_light: None,
            drag_light: None,
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            lua,
            #[cfg(not(target_arch = "wasm32"))]
            lua_breakpoints,
            #[cfg(not(target_arch = "wasm32"))]
            script_cache: HashMap::new(),
            #[cfg(target_arch = "wasm32")]
            lua_web,
            #[cfg(target_arch = "wasm32")]
            script_cache_web: HashMap::new(),
            time: 0.0,
            was_playing: false,
            play_snapshot: Vec::new(),
            physics: None,
            audio,
            import_tx: tx,
            import_rx: rx,
            scene_load_tx: scene_tx,
            scene_load_rx: scene_rx,
            imported_dirty: false,
            ai_tx,
            ai_rx,
            ai_busy: false,
            ai_scene_tx,
            ai_scene_rx,
            ai_scene_replace: true,
            current_project: None,
            confirm_close_project: false,
            last_autosave: None,
            pending_autosave_recovery: None,
        }
    }

    /// Lance une génération de scène par IA (thread de fond). `replace` = remplace la
    /// scène ; sinon ajoute les objets générés à la scène actuelle.
    pub fn request_ai_scene(&mut self, req: ai::AiRequest, replace: bool) {
        if self.ai_busy {
            return;
        }
        self.ai_busy = true;
        self.ai_scene_replace = replace;
        let tx = self.ai_scene_tx.clone();
        std::thread::spawn(move || {
            let result = ai::generate_scene_json(&req).and_then(|j| Scene::from_ai_json(&j));
            let _ = tx.send(result);
        });
    }

    /// Lance une génération de script Lua par IA (thread de fond) pour l'objet `idx`.
    pub fn request_ai_script(&mut self, idx: usize, req: ai::AiRequest) {
        if self.ai_busy {
            return;
        }
        self.ai_busy = true;
        let tx = self.ai_tx.clone();
        std::thread::spawn(move || {
            let result = ai::generate_lua(&req);
            let _ = tx.send((idx, result));
        });
    }

    /// Applique un script généré par IA s'il est prêt (à appeler chaque frame).
    fn poll_ai(&mut self) {
        while let Ok((idx, result)) = self.ai_rx.try_recv() {
            self.ai_busy = false;
            match result {
                Ok(script) if idx < self.scene.objects.len() => {
                    self.push_undo();
                    self.scene.objects[idx].script = script;
                    log::info!("Script généré par IA appliqué à l'objet {idx}");
                }
                Ok(_) => {} // l'objet a disparu entre-temps
                Err(e) => log::error!("Génération IA : {e}"),
            }
        }
        while let Ok(result) = self.ai_scene_rx.try_recv() {
            self.ai_busy = false;
            match result {
                Ok(mut scene) => {
                    self.push_undo();
                    if self.ai_scene_replace {
                        self.scene = scene;
                        log::info!("Scène générée par IA appliquée");
                    } else {
                        // Ajout : on intègre les objets et lumières générés à la scène actuelle.
                        let n = scene.objects.len();
                        self.scene.objects.append(&mut scene.objects);
                        self.scene.point_lights.append(&mut scene.point_lights);
                        log::info!("{n} objet(s) ajouté(s) par IA à la scène");
                    }
                    self.imported_dirty = true;
                    self.clear_selection();
                }
                Err(e) => log::error!("Génération de scène IA : {e}"),
            }
        }
    }

    /// Indique (et réinitialise) si la scène vient d'être remplacée par un Load :
    /// le renderer s'en sert pour reconstruire ses meshes GPU importés.
    pub fn take_imported_dirty(&mut self) -> bool {
        std::mem::take(&mut self.imported_dirty)
    }

    /// Images par seconde lissées, pour le bandeau d'état de l'éditeur.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// Vrai quand l'app doit rendre en continu (animation Play ou interaction en cours) :
    /// la boucle d'événements reste en `Poll`. Sinon elle peut throttler (économie CPU).
    pub fn is_active(&self) -> bool {
        (self.playing && !self.paused) || self.dragging || self.active_axis.is_some()
    }

    /// Joue immédiatement un fichier son (bouton de test / scripts).
    pub fn play_audio(&mut self, path: &str) {
        self.audio.play(path);
    }

    /// Volume (0..1) de la piste musique/ambiance (Sprint 104, persisté dans
    /// `Settings::music_volume`).
    pub fn set_music_volume(&mut self, v: f32) {
        self.audio.set_music_volume(v);
    }

    /// Volume (0..1) de la piste effets sonores (Sprint 104, persisté dans
    /// `Settings::sfx_volume`).
    pub fn set_sfx_volume(&mut self, v: f32) {
        self.audio.set_sfx_volume(v);
    }

    /// Langue du texte runtime (Sprint 130, persistée dans `Settings::locale`).
    pub fn set_locale(&mut self, l: locale::Locale) {
        self.locale = l;
    }

    /// Réduction du screen-shake (PHASE I Sprint 1, persistée dans
    /// `Settings::reduce_shake`).
    pub fn set_reduce_shake(&mut self, v: bool) {
        self.reduce_shake = v;
    }

    pub fn set_gizmo_mode(&mut self, mode: GizmoMode) {
        self.gizmo_mode = mode;
    }

    /// Le prochain clic de sélection sera additif (Cmd/Maj enfoncé), positionné par la plateforme.
    pub fn set_additive(&mut self, additive: bool) {
        self.additive = additive;
    }

    /// Touche modificatrice de snap (Ctrl) tenue ou non, positionné par la
    /// plateforme à chaque mouvement de souris — cf. doc de `snap_modifier`.
    pub fn set_snap_modifier(&mut self, held: bool) {
        self.snap_modifier = held;
    }

    /// Snap effectif pour le glissé de gizmo en cours : `snap` inversé par la
    /// touche modificatrice tenue (Blender : Ctrl bascule temporairement l'état
    /// affiché par le bouton 🧲, sans le modifier).
    pub(crate) fn effective_snap(&self) -> bool {
        self.snap ^ self.snap_modifier
    }

    /// Demande la fermeture de l'application (traitée par la boucle d'événements).
    /// Si la scène a des modifications non sauvegardées en mode éditeur, ouvre la
    /// confirmation (`confirm_quit`) au lieu de quitter — le mode player n'édite
    /// pas la scène, il quitte toujours directement.
    pub fn request_quit(&mut self) {
        if self.scene_dirty && !self.player {
            self.confirm_quit = true;
        } else {
            self.should_quit = true;
        }
    }

    /// Bascule la caméra libre de l'éditeur (touche G) : permet de survoler toute la
    /// carte sans contrainte, hors Play — cf. `fly_cam`/`update_fly_cam`. Sans effet en
    /// Play (la caméra suit alors le joueur/la caméra de jeu).
    /// Bascule la pause en Play/Player (touche Échap, cf. `Phase J` de
    /// `sprintreflecion.md`) — même champ `paused` déjà gelé par `advance_play`
    /// et `is_active`, réutilisé tel quel plutôt qu'un second mécanisme de gel.
    /// Sans effet hors Play (rien à mettre en pause).
    pub fn toggle_pause(&mut self) {
        if self.playing {
            self.paused = !self.paused;
        }
    }

    pub fn toggle_fly_cam(&mut self) {
        if !self.playing {
            self.fly_cam = !self.fly_cam;
            log::info!(
                "Caméra libre : {}",
                if self.fly_cam {
                    "activée"
                } else {
                    "désactivée"
                }
            );
        }
    }

    /// Définit la caméra de jeu depuis le point de vue actuel (orbite éditeur).
    pub fn set_game_camera(&mut self) {
        self.push_undo();
        self.scene.game_camera = Some(GameCamera {
            target: self.camera.target.to_array(),
            yaw: self.camera.yaw,
            pitch: self.camera.pitch,
            distance: self.camera.distance,
        });
        log::info!("Caméra de jeu définie sur la vue actuelle");
    }

    /// Retire la caméra de jeu (la vue Play repart de l'orbite éditeur).
    pub fn clear_game_camera(&mut self) {
        self.push_undo();
        self.scene.game_camera = None;
    }

    /// Indices de scène des autres joueurs réseau (« fantômes ») : leur pose est posée
    /// chaque frame par l'interpolation réseau (cf. `poll_network`), jamais par la
    /// simulation locale — l'interpolation de rendu ne doit pas y toucher.
    fn remote_player_scene_indices(&self) -> std::collections::HashSet<usize> {
        self.remote_players
            .values()
            .map(|rp| rp.scene_index)
            .collect()
    }

    /// La partie est-elle perdue (joueur entré dans une zone mortelle) ?
    pub fn is_lost(&self) -> bool {
        self.lost
    }

    /// Temps à afficher au HUD chrono : figé à la victoire, sinon temps de jeu courant.
    /// `None` si la scène n'a pas de collectibles ou si on n'est pas en Play.
    pub fn hud_timer(&self) -> Option<f32> {
        if !self.playing || self.scene.collectibles().is_none() {
            return None;
        }
        Some(self.win_time.unwrap_or(self.time))
    }

    /// Objet « joueur » : pilotable (joystick/gyro) en priorité, sinon premier objet
    /// scripté, sinon premier objet. Base commune à `player_position` et à la résolution
    /// d'attaque (bouton/portée propres à cet objet).
    fn player_object(&self) -> Option<&SceneObject> {
        self.player_index().map(|i| &self.scene.objects[i])
    }

    /// Indice de l'objet « joueur » : cf. `player_object`. `pub` depuis le pont
    /// de pilotage (`crate::pilot`, verbe `player`).
    pub fn player_index(&self) -> Option<usize> {
        // `o.visible` : exclut un objet masqué (cf. `AppState::despawn_network_player`,
        // ou le gabarit caché par `spawn_network_player` une fois un vrai joueur
        // réseau présent) — sans ce filtre, un gabarit inerte resterait « le
        // joueur » pour l'IA/la victoire-défaite même après avoir été masqué,
        // cf. AUDIT_MMORPG.md.
        self.scene
            .objects
            .iter()
            .position(|o| o.visible && o.controller.as_ref().is_some_and(|c| c.input || c.gyro))
            .or_else(|| {
                // Exclut les monstres (`ai_chaser`) et cibles de combat
                // (`combat.attackable`) : ils portent aussi un script (leur
                // logique de dégâts/couleur), donc sans cette exclusion, un
                // monstre pouvait être désigné « le joueur » dès qu'aucun objet
                // pilotable n'était visible (ex. avant qu'un joueur réseau ne
                // rejoigne un serveur headless), cf. AUDIT_MMORPG.md.
                self.scene.objects.iter().position(|o| {
                    o.visible
                        && !o.script.trim().is_empty()
                        && o.ai_chaser.is_none()
                        && !o.combat.as_ref().is_some_and(|c| c.attackable)
                })
            })
        // Pas de repli sur « le premier objet de la scène » (retiré, cf.
        // AUDIT_MMORPG.md) : un tel repli désignait parfois un décor statique
        // (ex. le sol) comme « le joueur » — son AABB, souvent immense pour un
        // sol, chevauche alors tous les monstres et déclenche leurs scripts de
        // dégâts en même temps. `None` (aucun joueur trouvable) doit laisser
        // l'IA/les déclencheurs inactifs, pas désigner un objet au hasard.
    }

    /// Position du « joueur » : cf. `player_object`. `pub` depuis le pont
    /// de pilotage (`crate::pilot`, verbe `player`).
    pub fn player_position(&self) -> Option<Vec3> {
        self.player_object().map(|o| o.transform.position)
    }

    /// État live du cycle de vie du toucher pour un objet (touch_started,
    /// touching, touch_ended) — pour les indicateurs de l'Inspecteur, en
    /// Play/Pause, à côté de « Tactile (cliquable) ». Cf. `picking::handle_input`
    /// pour ce qui pose ces trois champs.
    pub fn touch_state_of(&self, idx: usize) -> (bool, bool, bool) {
        (
            self.touch_started_obj == Some(idx),
            self.touched_obj == Some(idx),
            self.touch_ended_obj == Some(idx),
        )
    }

    /// Données pour la mini-carte (overlay HUD/éditeur) : positions (x, z, plan
    /// horizontal — la hauteur n'y a pas sa place) du joueur local, des joueurs
    /// réseau et des créatures, plus les bornes du monde à cadrer. Recalculé
    /// chaque frame (peu d'objets en jeu, coût négligeable) plutôt que mis en
    /// cache : la position des joueurs/créatures change en continu.
    pub fn minimap_data(&self) -> MinimapData {
        let player = self
            .player_index()
            .and_then(|i| self.scene.objects.get(i))
            .map(|o| (o.transform.position.x, o.transform.position.z));
        let allies = self
            .remote_players
            .values()
            .filter_map(|rp| {
                self.scene
                    .objects
                    .get(rp.scene_index)
                    .map(|o| MinimapPoint {
                        x: o.transform.position.x,
                        z: o.transform.position.z,
                        label: rp.name.clone(),
                    })
            })
            .collect();
        // `active_wave` (demande utilisateur : « où sont les monstres de la
        // vague qui attaque ? ») — même filtre que `wave_hud`
        // (`o.combat.wave == self.wave`), pour désigner sur la carte les
        // ennemis de la manche en cours, distincts des créatures hors manche
        // (`combat.wave == 0` ou wave désactivé, `self.wave == 0`).
        let current_wave = self.wave;
        let creatures = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible && o.ai_chaser.is_some())
            .map(|o| MinimapCreature {
                x: o.transform.position.x,
                z: o.transform.position.z,
                active_wave: current_wave != 0
                    && o.combat.as_ref().is_some_and(|c| c.wave == current_wave),
            })
            .collect();
        // Bornes du monde : le sol conventionnel « Sol » (cf. assets/*.json), sa
        // `transform.scale` encodant un demi-extent horizontal — à défaut,
        // englobante de tous les objets, avec un repli fixe si la scène est vide.
        // Calculées avant `decor` ci-dessous : `thin_decor` a besoin de ces
        // bornes pour caler sa grille de dédoublonnage.
        let bounds = self
            .scene
            .objects
            .iter()
            .find(|o| o.name == "Sol")
            .map(|o| {
                let p = o.transform.position;
                let s = o.transform.scale;
                (p.x - s.x, p.z - s.z, p.x + s.x, p.z + s.z)
            })
            .unwrap_or_else(|| {
                let mut min_x = f32::MAX;
                let mut min_z = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_z = f32::MIN;
                for o in &self.scene.objects {
                    let p = o.transform.position;
                    min_x = min_x.min(p.x);
                    max_x = max_x.max(p.x);
                    min_z = min_z.min(p.z);
                    max_z = max_z.max(p.z);
                }
                if min_x > max_x {
                    (-50.0, -50.0, 50.0, 50.0)
                } else {
                    (min_x, min_z, max_x, max_z)
                }
            });
        // Décor repérable (eau, bâtiments, murs/remparts, forêt) : demande
        // utilisateur (« repères pour comprendre » la carte) — sans ça, la
        // mini-carte n'affiche que des points flottants sans contexte de
        // terrain. Exclut joueur/créatures/sol (déjà couverts ci-dessus/
        // ci-dessous) et tout objet dont `classify_decor` ne reconnaît pas le
        // nom/l'asset (la grande majorité du décor scatter, herbe/rochers
        // isolés — resterait un bruit visuel non catégorisable). `thin_decor`
        // dédoublonne ensuite (une forêt de centaines d'arbres ou une rive de
        // dizaines de tuiles d'eau produirait sinon un nuage de points
        // illisible, constaté en jeu).
        let raw_decor: Vec<MinimapDecor> = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible && o.ai_chaser.is_none() && o.name != "Sol")
            .filter_map(|o| {
                let asset_path = match o.mesh {
                    MeshKind::Imported(i) => self
                        .scene
                        .imported
                        .get(i as usize)
                        .map(|m| m.path.as_str())
                        .unwrap_or(""),
                    _ => "",
                };
                classify_decor(&o.name, asset_path).map(|kind| MinimapDecor {
                    x: o.transform.position.x,
                    z: o.transform.position.z,
                    kind,
                })
            })
            .collect();
        let span = (bounds.2 - bounds.0).max(bounds.3 - bounds.1).max(1.0);
        let decor_cell = (span / 24.0).max(1.0);
        let decor = thin_decor(raw_decor, bounds, decor_cell);
        MinimapData {
            player,
            allies,
            creatures,
            decor,
            decor_cell,
            bounds,
        }
    }

    /// Position monde de l'allié réseau à terre (vie <= 0, même seuil que
    /// `is_online_client`/`PlayerDown`) le plus proche du joueur local — pour
    /// le marqueur de direction hors-écran de `ally_down_banner` (Phase L
    /// Sprint 2, `sprint2audijeu0718.md`). `None` si aucun joueur local
    /// positionné ou aucun allié à terre : dans ce cas l'appelant n'affiche
    /// que la bannière texte, comme avant ce sprint.
    pub fn nearest_downed_ally_position(&self) -> Option<Vec3> {
        let player_pos = self.player_position()?;
        self.remote_players
            .values()
            .filter(|rp| rp.health.is_some_and(|h| h <= 0.0))
            .filter_map(|rp| self.scene.objects.get(rp.scene_index))
            .map(|o| o.transform.position)
            .min_by(|a, b| {
                a.distance_squared(player_pos)
                    .total_cmp(&b.distance_squared(player_pos))
            })
    }
}

mod minimap;
pub use minimap::*;

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Chemin du fichier de scène, dans le dossier personnel (cwd vaut "/" en mode .app).
fn scene_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{home}/motor3derust_scene.json")
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
