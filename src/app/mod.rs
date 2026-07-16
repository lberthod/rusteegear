//! État applicatif **sans dépendance GPU** : scène, sélection, caméra, mode Play,
//! interaction pointeur. Le `Renderer` consomme cet état pour dessiner.

pub mod ai;
pub mod asset_ops;
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
use crate::scene::{GameCamera, MobileControls, PointLight, Scene, SceneObject, Transform};

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
    /// Déplacement clavier (ordinateur), relatif à la caméra : flèches uniquement
    /// (WASD pilote désormais des contrôles « tank », cf. `key_turn`) ; chaque
    /// composante dans [-1, 1].
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
    /// Avance/recul « tank » du stick gauche de la manette (Sprint 110), zone morte
    /// déjà appliquée — canal séparé de `key_thrust`/`touch_thrust`, cumulé avec eux
    /// via `thrust()`, même principe que les deux autres sources.
    pub gamepad_thrust: f32,
    /// Rotation « tank » du stick gauche de la manette — même principe que
    /// `gamepad_thrust`, cumulée via `turn()`.
    pub gamepad_turn: f32,
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
        (self.key_thrust + self.touch_thrust + self.gamepad_thrust).clamp(-1.0, 1.0)
    }

    /// Rotation « tank » effective : clavier (A/D) + pavé tactile + stick gauche
    /// manette (Sprint 110), borné à [-1, 1].
    pub fn turn(&self) -> f32 {
        (self.key_turn + self.touch_turn + self.gamepad_turn).clamp(-1.0, 1.0)
    }
}

pub struct AppState {
    pub scene: Scene,
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
    /// Demande de fermeture de l'application (menu Fichier → Quitter).
    pub should_quit: bool,
    /// Mode « player » : pas d'éditeur (panneaux egui), démarre en Play.
    pub player: bool,
    /// Langue du texte runtime affiché en Play (Sprint 130) — pas l'éditeur, dont l'UI
    /// reste en français (outil de développement). Persistée dans `Settings::locale`.
    pub locale: locale::Locale,
    /// État courant des contrôles tactiles (joystick + boutons), lu par les scripts.
    pub input_state: PlayerInput,
    /// Objet « tactile » touché cette frame (exposé une frame à son script via `obj.tapped`).
    tapped_obj: Option<usize>,
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
    /// Intensité (1 = pic, décroît vers 0) de l'effet 3D d'attaque : téléporte et affiche
    /// brièvement l'objet `is_attack_fx` sur la cible touchée (rend le coup lisible).
    pub attack_flash: f32,
    /// Manche courante (1-based) d'un système de vagues (cf. `Combat::wave`) ; 0 = pas
    /// de système de manches dans la scène courante (les autres démos). Toutes les
    /// cibles de la manche courante vaincues ⇒ manche suivante révélée ; dernière
    /// manche vaincue ⇒ victoire (cf. `update_waves` dans `advance_play`).
    pub wave: u32,
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
    /// Cooldown restant (s) par paire (indice de créature mordeuse, joueur réseau)
    /// — cf. `health::update_creature_bite`. Clé composite plutôt qu'un cooldown
    /// par créature seule : deux joueurs au contact de la même créature ne
    /// doivent pas partager un seul temporisateur (l'un mordu ne doit pas
    /// « protéger » l'autre).
    bite_cooldowns: HashMap<(usize, crate::net::protocol::PlayerId), f32>,
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
    #[cfg(not(target_os = "ios"))]
    net_last_connect: Option<(String, String, String)>,
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
    firebase_tx: std::sync::mpsc::Sender<Result<(String, String), String>>,
    firebase_rx: std::sync::mpsc::Receiver<Result<(String, String), String>>,
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

/// Mode de manipulation du gizmo (touches W / E / R).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
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
            player: false,
            locale: initial_settings.locale,
            input_state: PlayerInput::default(),
            tapped_obj: None,
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
            attack_flash: 0.0,
            wave: 0,
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
            bite_cooldowns: HashMap::new(),
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
            net_creature_last_snapshot: HashMap::new(),
            #[cfg(not(target_os = "ios"))]
            net_local_interp: crate::net::interpolation::RemoteEntity::default(),
            #[cfg(not(target_os = "ios"))]
            net_local_health: None,
            #[cfg(not(target_os = "ios"))]
            net_local_kills: None,
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
    pub fn request_quit(&mut self) {
        self.should_quit = true;
    }

    /// Bascule la caméra libre de l'éditeur (touche G) : permet de survoler toute la
    /// carte sans contrainte, hors Play — cf. `fly_cam`/`update_fly_cam`. Sans effet en
    /// Play (la caméra suit alors le joueur/la caméra de jeu).
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

    /// Indice de l'objet « joueur » : cf. `player_object`.
    fn player_index(&self) -> Option<usize> {
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

    /// Position du « joueur » : cf. `player_object`.
    fn player_position(&self) -> Option<Vec3> {
        self.player_object().map(|o| o.transform.position)
    }
}

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
mod tests {
    use super::*;
    // Sprint 105a-1 : `simulation`/`scripting` sont des sous-modules de `app`,
    // pas ré-exportés par `use super::*` (qui ne remonte que le contenu de
    // `app` lui-même) — import explicite des symboles `pub(super)` que ces
    // tests appellent directement (par nom, pas via `AppState::advance_play`).
    use super::scripting::run_script;

    /// Nom de prefab unique par appel (horloge + pid) : contrairement aux tests de
    /// `scene::tests` (convertis en dossier temporaire via `Scene::save_prefab_at`),
    /// celui-ci exerce `spawn()` côté Lua (`scripting.rs`), qui appelle la variante
    /// **globale** `Scene::instantiate_prefab` (pas de point d'injection de
    /// répertoire dans le binding Lua, et il ne devrait pas y en avoir un rien que
    /// pour les tests) — écrit donc réellement dans `~/.motor3derust/assets/prefabs/`,
    /// comme le ferait l'éditeur. Nom unique pour ne pas collisionner avec un vrai
    /// prefab de l'utilisateur.
    fn unique_test_prefab_name(tag: &str) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("test_{tag}_{}_{}", std::process::id(), nanos)
    }

    #[test]
    fn a_door_opens_on_score_3_without_direct_coupling() {
        // Bout en bout (App réel) : une « porte » scriptée
        // s'ouvre quand le score atteint 3, sans référencer ni les pièces ni le
        // joueur — elle n'écoute que l'événement `score:3` émis par le moteur
        // (`add_score`). Les 3 pièces sont sur le joueur : toutes ramassées le même
        // tick, précisément le cas où émettre seulement la valeur *finale* du score
        // ferait rater l'événement.
        let mut app = AppState::new();
        let mut scene = crate::scene::Scene::default();
        scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        for i in 0..3 {
            scene.objects.push(crate::scene::SceneObject {
                name: format!("Pièce {i}"),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0))
                    .with_scale(Vec3::splat(0.3)),
                tap_action: crate::scene::TapAction::Hide,
                ..Default::default()
            });
        }
        // Une 4e pièce hors de portée : sans elle, ramasser les 3 premières gagne la
        // partie le même tick — et le jeu **gèle** une fois gagné (cf. `advance_play`),
        // l'événement `score:3` ne serait jamais délivré. Le livrable vise une porte
        // qui s'ouvre *en cours de partie*, pas à l'écran de victoire.
        scene.objects.push(crate::scene::SceneObject {
            name: "Pièce lointaine".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(50.0, 1.0, 50.0))
                .with_scale(Vec3::splat(0.3)),
            tap_action: crate::scene::TapAction::Hide,
            ..Default::default()
        });
        scene.objects.push(crate::scene::SceneObject {
            name: "Porte".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 1.0, 0.0)),
            script: "if on_event('score:3') then obj.y = 10 end".into(),
            ..Default::default()
        });
        app.scene = scene;
        app.playing = true;
        for _ in 0..10 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert_eq!(app.score, 3, "les 3 pièces doivent être ramassées");
        let door = app
            .scene
            .objects
            .iter()
            .find(|o| o.name == "Porte")
            .unwrap();
        assert!(
            (door.transform.position.y - 10.0).abs() < 1e-4,
            "la porte devait s'ouvrir sur l'événement score:3 (y = {})",
            door.transform.position.y
        );
    }

    #[test]
    fn push_hud_event_reaches_scripts_prefixed_with_hud_via_on_event() {
        // Cf. `editor::hud::hud_widgets` : un widget `Button` cliqué appelle
        // `AppState::push_hud_event(action)`, qui doit se lire côté script exactement
        // comme un `emit()` Lua préfixé `hud:` — même file d'événements
        // (`AppState::game_events`), un script ne doit pas pouvoir distinguer les deux
        // sources.
        let mut app = AppState::new();
        let mut scene = crate::scene::Scene::default();
        scene.objects.push(crate::scene::SceneObject {
            name: "Porte HUD".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            script: "if on_event('hud:jump') then obj.y = 9.0 end".into(),
            ..Default::default()
        });
        app.scene = scene;
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        // Transition Edit→Play d'abord : elle vide `game_events` (nouvelle partie), donc
        // le clic HUD doit être poussé après, sans quoi il serait perdu avant même
        // d'atteindre un script.
        app.advance_play();
        app.push_hud_event("jump");
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let porte = app
            .scene
            .objects
            .iter()
            .find(|o| o.name == "Porte HUD")
            .unwrap();
        assert!(
            (porte.transform.position.y - 9.0).abs() < 1e-4,
            "le clic HUD devait se lire via on_event('hud:jump') (y = {})",
            porte.transform.position.y
        );
    }

    #[test]
    fn script_calling_obj_destroy_soft_deletes_via_visible_false() {
        // `obj:destroy()` doit se traduire par `visible = false` — une
        // suppression douce, pas un retrait de `scene.objects` (cf. la doc de
        // `run_script`, cette dernière casserait les indices retenus ailleurs).
        let mut app = AppState::new();
        let mut scene = crate::scene::Scene::default();
        scene.objects.push(crate::scene::SceneObject {
            name: "Éphémère".into(),
            script: "obj:destroy()".into(),
            ..Default::default()
        });
        app.scene = scene;
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert!(!app.scene.objects[0].visible, "l'objet devait être masqué");
        // Toujours dans `scene.objects` : ce n'est pas un vrai retrait.
        assert_eq!(app.scene.objects.len(), 1);
    }

    #[test]
    fn a_spawned_enemy_via_lua_joins_the_scene_and_can_be_found_by_tag() {
        // Un script peut faire apparaître un ennemi depuis un
        // prefab (`spawn`), et cet ennemi devient trouvable par `find_tag` (au tick
        // suivant : `find_tag` lit un instantané pris avant la boucle des scripts).
        // Verrou : ce test écrit dans le vrai `assets_dir()` (cf.
        // `REAL_ASSETS_DIR_TEST_LOCK`), à sérialiser avec les autres tests dans le
        // même cas (ex. `demos::tests::mmorpg_demo_landmarks_are_prefab_instances...`).
        let _guard = crate::assets::REAL_ASSETS_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let name = unique_test_prefab_name("ennemi97");
        let template = crate::scene::SceneObject {
            name: "Ennemi".into(),
            mesh: crate::scene::MeshKind::Cube,
            tag: "ennemi".into(),
            ..Default::default()
        };
        let asset_id = crate::scene::Scene::save_prefab(
            &template,
            &name,
            &crate::assets::PrefabScope::General,
        )
        .unwrap();

        let mut app = AppState::new();
        let mut scene = crate::scene::Scene::default();
        scene.objects.push(crate::scene::SceneObject {
            name: "Générateur".into(),
            script: format!("if time < 0.02 then spawn('{asset_id}', 3.0, 0.0, 4.0) end"),
            ..Default::default()
        });
        app.scene = scene;
        app.playing = true;
        for _ in 0..3 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert_eq!(
            app.scene.objects.len(),
            2,
            "le spawn doit ajouter exactement un objet"
        );
        let spawned = &app.scene.objects[1];
        assert_eq!(spawned.tag, "ennemi", "l'instance doit suivre le template");
        assert!((spawned.transform.position - Vec3::new(3.0, 0.0, 4.0)).length() < 1e-4);
    }

    /// Dossier temporaire unique par test (Sprint 105a-3, isolation des
    /// tests système) — même schéma que `assets::tests::temp_assets_dir` :
    /// aucune dépendance au vrai `$HOME`, sûr sous exécution parallèle.
    fn temp_save_dir(tag: &str) -> std::path::PathBuf {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        tag.hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        let dir =
            std::env::temp_dir().join(format!("rusteegear_appsave_test_{:x}", hasher.finish()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saving_and_loading_a_game_restores_score_position_and_lua_vars() {
        // La progression (score, positions, variables de
        // script) doit survivre à une sauvegarde puis un chargement — testé bout en
        // bout via `AppState::save_game_at`/`load_game_at`, qui écrivent réellement
        // sur disque (comme le ferait le jeu réel sur desktop ou Android), mais dans
        // un dossier temporaire isolé plutôt que le vrai `user://`.
        let dir = temp_save_dir("roundtrip");
        let slot = "roundtrip";
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::default();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            transform: Transform::from_pos(Vec3::new(3.0, 1.0, -2.0)),
            ..Default::default()
        });
        app.score = 7;
        app.lua_vars.insert("niveau".to_string(), 4.0);

        app.save_game_at(slot, &dir).expect("sauvegarde impossible");

        // Simule une reprise de partie : score/position/variables sont remis à zéro
        // avant le chargement (ex. l'app vient de redémarrer).
        app.score = 0;
        app.scene.objects[0].transform.position = Vec3::ZERO;
        app.lua_vars.clear();

        app.load_game_at(slot, &dir).expect("chargement impossible");

        assert_eq!(app.score, 7);
        assert_eq!(
            app.scene.objects[0].transform.position,
            Vec3::new(3.0, 1.0, -2.0)
        );
        assert_eq!(app.lua_vars.get("niveau"), Some(&4.0));
    }

    #[test]
    fn an_anim_notify_gates_the_combat_hit_window() {
        // Le coup ne doit « toucher » (ici : le script met
        // `in_window` à 1 via `save.set`) que pendant la fenêtre d'animation délimitée
        // par deux marqueurs (`hit_open`/`hit_close`), pas avant, pas après.
        let mut imported = crate::scene::ImportedMesh {
            name: "Guerrier".into(),
            ..Default::default()
        };
        imported
            .clips
            .push(crate::scene::import::Clip::without_tracks("attaque", 1.0));
        imported.notifies.insert(
            "attaque".to_string(),
            vec![
                (0.3, "hit_open".to_string()),
                (0.6, "hit_close".to_string()),
            ],
        );
        let mut scene = crate::scene::Scene::default();
        scene.imported.push(imported);
        scene.objects.push(crate::scene::SceneObject {
            name: "Guerrier".into(),
            mesh: crate::scene::MeshKind::Imported(0),
            animation: Some(crate::scene::AnimationState {
                clip: "attaque".into(),
                time: 0.0,
                speed: 1.0,
                prev_clip: String::new(),
                prev_time: 0.0,
                blend: 1.0,
            }),
            script: "\
                if on_event('anim:hit_open') then save.set('in_window', 1) end\n\
                if on_event('anim:hit_close') then save.set('in_window', 0) end"
                .into(),
            ..Default::default()
        });
        let mut app = AppState::new();
        app.scene = scene;
        app.playing = true;

        let advance_one_tick = |app: &mut AppState| {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
            app.advance_play();
        };

        // ~0.2 s : avant `hit_open` (0.3 s), la fenêtre ne doit pas encore être ouverte.
        for _ in 0..12 {
            advance_one_tick(&mut app);
        }
        assert_eq!(
            app.lua_vars.get("in_window"),
            None,
            "la fenêtre ne doit pas encore être ouverte avant 0.3 s"
        );

        // ~0.35 s : après `hit_open`, avant `hit_close` — fenêtre ouverte.
        for _ in 0..9 {
            advance_one_tick(&mut app);
        }
        assert_eq!(
            app.lua_vars.get("in_window"),
            Some(&1.0),
            "la fenêtre doit être ouverte entre 0.3 s et 0.6 s"
        );

        // ~0.8 s : après `hit_close` — fenêtre refermée.
        for _ in 0..27 {
            advance_one_tick(&mut app);
        }
        assert_eq!(
            app.lua_vars.get("in_window"),
            Some(&0.0),
            "la fenêtre doit être refermée après 0.6 s"
        );
    }

    #[test]
    fn script_setting_obj_anim_starts_a_crossfade() {
        // Exposition Lua : `obj.anim = "run"` doit atterrir dans
        // `AnimationState` via `set_clip`, avec le fondu enchaîné qu'il déclenche
        // (`prev_clip` retient l'ancien clip, `blend` repart à 0).
        use crate::scene::AnimationState;
        let lua = Lua::new();
        let src = "obj.anim = 'run'";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut anim = Some(AnimationState {
            clip: "idle".into(),
            time: 1.5,
            speed: 1.0,
            prev_clip: String::new(),
            prev_time: 0.0,
            blend: 1.0,
        });
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut anim,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let state = anim.unwrap();
        assert_eq!(state.clip, "run");
        assert_eq!(state.prev_clip, "idle");
        assert_eq!(state.blend, 0.0);
    }

    #[test]
    fn script_leaving_obj_anim_untouched_does_not_reset_clip() {
        // Sans écriture de `obj.anim` par le script, le clip courant ne doit pas être
        // relancé (sinon `set_clip` redémarrerait un fondu à chaque frame sans raison).
        use crate::scene::AnimationState;
        let lua = Lua::new();
        let src = "obj.x = obj.x"; // script sans rapport avec l'animation
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut anim = Some(AnimationState {
            clip: "run".into(),
            time: 0.4,
            speed: 1.0,
            prev_clip: String::new(),
            prev_time: 0.0,
            blend: 1.0,
        });
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut anim,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let state = anim.unwrap();
        assert_eq!(state.clip, "run");
        assert_eq!(state.time, 0.4);
        assert_eq!(state.blend, 1.0);
    }

    #[test]
    fn script_reacts_to_tap_and_changes_color() {
        // Au tap, l'objet vire au rouge.
        let lua = Lua::new();
        let src = "if obj.tapped then obj.r = 1.0; obj.g = 0.0; obj.b = 0.0 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [0.5, 0.5, 0.5];
        let input = PlayerInput::default();
        // pas de tap : couleur inchangée
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(col, [0.5, 0.5, 0.5]);
        // tap : passe au rouge
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            true,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(col, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn script_reacts_to_trigger() {
        // obj.y monte quand le joueur entre dans la zone.
        let lua = Lua::new();
        let src = "if obj.triggered then obj.y = 9.0 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let input = PlayerInput::default();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.y, 0.0);
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            true,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.y, 9.0);
    }

    #[test]
    fn script_reads_tilt() {
        let lua = Lua::new();
        let func = lua
            .load("obj.x = obj.x + tilt.x; obj.z = obj.z + tilt.y")
            .into_function()
            .unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let input = PlayerInput {
            tilt: (1.0, -1.0),
            ..Default::default()
        };
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert!((t.position.x - 1.0).abs() < 1e-5);
        assert!((t.position.z + 1.0).abs() < 1e-5);
    }

    #[test]
    fn script_sets_health() {
        let lua = Lua::new();
        let func = lua.load("set_health(0.5)").into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let input = PlayerInput::default();
        let mut health = None;
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut health,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(health, Some(0.5));
    }

    #[test]
    fn script_damage_is_relative_and_stacks_across_objects_same_frame() {
        // `damage(v)` doit soustraire de la vie déjà accumulée cette frame (par d'autres
        // objets), contrairement à `set_health` (valeur absolue) qui écraserait les dégâts
        // d'un ennemi précédent si un autre script s'exécutait après lui sans le vouloir.
        let lua = Lua::new();
        let func = lua.load("damage(0.3)").into_function().unwrap();
        let input = PlayerInput::default();
        // Aucun système de vie démarré : la base par défaut est pleine vie (1.0).
        let mut health = None;
        run_script(
            &lua,
            &func,
            &mut Transform::from_pos(Vec3::ZERO),
            &mut [1.0; 3],
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut health,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(health, Some(0.7));
        // Un deuxième objet inflige des dégâts la même frame : doit partir de 0.7, pas de 1.0.
        run_script(
            &lua,
            &func,
            &mut Transform::from_pos(Vec3::ZERO),
            &mut [1.0; 3],
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut health,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(
            (health.unwrap() - 0.4).abs() < 1e-5,
            "les dégâts de deux objets la même frame doivent s'additionner : {health:?}"
        );
        // Clampé à 0, ne descend pas en négatif.
        for _ in 0..10 {
            run_script(
                &lua,
                &func,
                &mut Transform::from_pos(Vec3::ZERO),
                &mut [1.0; 3],
                &mut None,
                0.016,
                0.0,
                &input,
                false,
                false,
                &[],
                &mut Vec::new(),
                &[],
                &mut Vec::new(),
                &mut false,
                &mut std::collections::HashMap::new(),
                &mut Vec::new(),
                &mut health,
                &mut Vec::new(),
                false,
                None,
                &mut Vec::new(),
            )
            .unwrap();
        }
        assert_eq!(health, Some(0.0));
    }

    #[test]
    fn controller_demo_enemy_scripts_compile_and_patrol() {
        // Les ennemis de la démo contrôleur sont scriptés (patrouille + pulsation rouge) :
        // vérifie que leurs scripts compilent et déplacent réellement l'objet dans le temps
        // (sinon un ennemi "mort" resterait immobile, silencieusement cassé).
        let scene = crate::scene::Scene::controller_demo();
        let enemies: Vec<_> = scene
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Ennemi"))
            .collect();
        assert!(enemies.len() >= 3, "au moins 3 ennemis dans la démo");
        let lua = Lua::new();
        for e in enemies {
            assert!(
                e.trigger && !e.deadly,
                "un ennemi doit infliger des dégâts progressifs (trigger), pas tuer \
                 instantanément (deadly) : {}",
                e.name
            );
            let func = lua.load(&e.script).into_function().unwrap();
            let mut t0 = e.transform;
            let mut col = e.color;
            let input = PlayerInput::default();
            run_script(
                &lua,
                &func,
                &mut t0,
                &mut col,
                &mut None,
                0.016,
                0.0,
                &input,
                false,
                false,
                &[],
                &mut Vec::new(),
                &[],
                &mut Vec::new(),
                &mut false,
                &mut std::collections::HashMap::new(),
                &mut Vec::new(),
                &mut None,
                &mut Vec::new(),
                false,
                None,
                &mut Vec::new(),
            )
            .unwrap();
            let mut t1 = e.transform;
            let mut col1 = e.color;
            run_script(
                &lua,
                &func,
                &mut t1,
                &mut col1,
                &mut None,
                0.016,
                1.0,
                &input,
                false,
                false,
                &[],
                &mut Vec::new(),
                &[],
                &mut Vec::new(),
                &mut false,
                &mut std::collections::HashMap::new(),
                &mut Vec::new(),
                &mut None,
                &mut Vec::new(),
                false,
                None,
                &mut Vec::new(),
            )
            .unwrap();
            assert!(
                (t0.position - t1.position).length() > 0.01,
                "l'ennemi {} doit bouger avec le temps",
                e.name
            );
        }
    }

    /// Scène synthétique minimale (sol + joueur + un danger `trigger`+`damage()` couvrant
    /// tout le sol) : isole la mécanique vie/dégâts de l'équilibrage d'un niveau réel.
    /// La démo contrôleur n'est pas réutilisée ici : sa patrouille est conçue pour un
    /// contact *intermittent* (l'ennemi s'éloigne), ce qui ne conviendrait pas pour
    /// tester un contact permanent sans coupler le test à ce détail d'équilibrage.
    fn synthetic_damage_scene() -> crate::scene::Scene {
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(16.0, 1.0, 16.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let mut danger = crate::scene::SceneObject {
            name: "Danger".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.5, 0.0))
                .with_scale(Vec3::splat(3.0)),
            trigger: true,
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            respawn_delay: 100.0,
            script: "if obj.triggered then damage(2.0 * dt) end".into(),
            ..Default::default()
        };
        danger.color = [1.0; 3];
        let mut fx = crate::scene::SceneObject {
            name: "FX Attaque".into(),
            mesh: crate::scene::MeshKind::Sphere,
            combat: Some(crate::scene::Combat {
                is_attack_fx: true,
                ..Default::default()
            }),
            visible: false,
            ..Default::default()
        };
        fx.color = [1.0; 3];
        crate::scene::Scene {
            objects: vec![sol, joueur, danger, fx],
            ..Default::default()
        }
    }

    #[test]
    fn sustained_enemy_contact_drains_health_and_ends_the_game() {
        // Bout en bout (App réel, pas juste `run_script`) : un contact **permanent** avec
        // un danger `trigger` + `damage()` doit finir par vaincre le joueur via le nouveau
        // check de défaite sur `hud_health <= 0`, malgré la régénération passive.
        let mut app = AppState::new();
        app.scene = synthetic_damage_scene();
        app.playing = true;
        let mut ended = false;
        for _ in 0..80 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.lost {
                ended = true;
                break;
            }
        }
        assert!(
            ended,
            "un contact soutenu doit finir par vaincre le joueur (vie = {:?})",
            app.hud_health
        );
    }

    #[test]
    fn attacking_defeats_enemy_and_stops_further_damage() {
        // Bout en bout : appuyer sur « Attaque » (bouton nommé) alors qu'un ennemi
        // `attackable` est à portée doit le vaincre (masquer) et augmenter le score.
        // Verrouille aussi la correction du filtre `triggered` (doit exclure les objets
        // invisibles) : un ennemi vaincu ne doit plus pouvoir blesser le joueur ensuite.
        let mut app = AppState::new();
        app.scene = synthetic_damage_scene();
        app.playing = true;
        app.input_state.buttons.insert("Attaque".into());
        // Laisse le temps à la préparation (attack_windup) puis au missile d'arriver
        // (l'attaque n'est plus instantanée, cf. `AttackCharge`/`AttackProjectile`).
        for _ in 0..10 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert_eq!(
            app.score, 1,
            "l'attaque doit vaincre l'ennemi à portée (score += 1)"
        );
        assert!(
            !app.scene
                .objects
                .iter()
                .find(|o| o.name == "Danger")
                .unwrap()
                .visible,
            "l'ennemi vaincu doit devenir invisible"
        );
        // Le joueur ne prend plus de dégâts une fois l'ennemi vaincu, même en restant
        // dessus (sans la correction du filtre `triggered`, le script du danger continuerait
        // à appeler `damage()` malgré `visible = false`).
        app.input_state.buttons.clear();
        let health_after_defeat = app.hud_health;
        for _ in 0..20 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert!(
            !app.lost,
            "un ennemi vaincu ne doit plus pouvoir vaincre le joueur (vie = {:?} → {:?})",
            health_after_defeat, app.hud_health
        );
    }

    #[test]
    fn attack_cooldown_blocks_rapid_refire_but_allows_it_once_expired() {
        // Trouvaille de l'audit gameplay : sans temps de recharge, maintenir le bouton
        // d'attaque défaisait instantanément tout ce qui entrait en portée, sans le
        // moindre risque — le combat était trivial. Verrouille le correctif : une
        // deuxième cible à portée n'est PAS vaincue dans la fenêtre de recharge, mais
        // l'est une fois celle-ci expirée.
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 50.0,
                attack_cooldown: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut cible1 = crate::scene::SceneObject {
            name: "Cible 1".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.5, 0.0)),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        cible1.color = [1.0; 3];
        // Hors de portée au départ : n'est PAS touchée par la première attaque (portée
        // 50 mais la cible 2 démarre à 100 unités). Téléportée à portée juste après.
        let mut cible2 = crate::scene::SceneObject {
            name: "Cible 2".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(100.0, 0.5, 0.0)),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        cible2.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![joueur, cible1, cible2],
            ..Default::default()
        };
        app.playing = true;
        app.input_state.buttons.insert("Attaque".into());

        // Tir sur la cible 1 (seule à portée), puis laisse le temps à la préparation
        // (attack_windup, défaut 0,25 s) et au missile d'arriver (le coup n'est plus
        // instantané, cf. `AttackCharge`/`AttackProjectile`) — sans dépasser la fenêtre
        // de recharge (0,5 s), sans quoi l'assertion suivante (cible 2 protégée par la
        // recharge) ne serait plus valide.
        for _ in 0..8 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert!(
            !app.scene.objects[1].visible,
            "cible 1 vaincue après l'arrivée du missile"
        );
        assert!(
            app.scene.objects[2].visible,
            "cible 2 encore debout (hors de portée)"
        );

        // La cible 2 entre à portée juste après (ex. un monstre qui s'approche) — toujours
        // dans la fenêtre de recharge de 0,5 s : le bouton reste enfoncé mais ne doit PAS
        // tirer un nouveau missile sur elle à cet instant.
        app.scene.objects[2].transform.position = Vec3::new(1.0, 0.5, 0.0);
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert!(
            app.scene.objects[2].visible,
            "sans recharge écoulée, aucun missile ne doit être tiré sur la cible 2"
        );

        // Laisse la recharge s'écouler (0,5 s) puis le missile arriver : l'attaque
        // suivante doit alors porter.
        for _ in 0..15 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert!(
            !app.scene.objects[2].visible,
            "la recharge écoulée et le missile arrivé, la cible 2 doit finir par être vaincue"
        );
    }

    #[test]
    fn attack_shows_and_hides_the_visual_fx_anchor() {
        // Une attaque qui porte doit rendre visible l'ancre `is_attack_fx`, la téléporter
        // sur la cible touchée, puis la faire disparaître une fois `attack_flash` retombé
        // à 0 — sinon l'effet resterait affiché indéfiniment après un coup.
        let mut app = AppState::new();
        app.scene = synthetic_damage_scene();
        let target_pos = app
            .scene
            .objects
            .iter()
            .find(|o| o.name == "Danger")
            .unwrap()
            .transform
            .position;
        app.playing = true;
        app.input_state.buttons.insert("Attaque".into());
        // Laisse le temps à la préparation puis au missile d'arriver (le coup n'est plus
        // instantané, cf. `AttackCharge`/`AttackProjectile`).
        for _ in 0..10 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }

        fn fx(app: &AppState) -> crate::scene::SceneObject {
            app.scene
                .objects
                .iter()
                .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
                .unwrap()
                .clone()
        }
        assert!(
            fx(&app).visible,
            "l'ancre FX doit être visible après un coup"
        );
        assert!(
            (fx(&app).transform.position - target_pos).length() < 1e-4,
            "l'ancre FX doit être téléportée sur la cible touchée"
        );
        assert!(app.attack_flash > 0.0);

        app.input_state.buttons.clear();
        for _ in 0..30 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.attack_flash <= 0.0 {
                break;
            }
        }
        assert_eq!(
            app.attack_flash, 0.0,
            "le flash d'attaque doit finir par retomber à 0"
        );
        assert!(
            !fx(&app).visible,
            "l'ancre FX doit disparaître une fois le flash retombé"
        );
    }

    #[test]
    fn auto_run_speed_advances_the_player_with_zero_input() {
        // Cœur du style « Temple Run » : un joueur `auto_run_speed > 0` doit avancer en +Z
        // même sans la moindre entrée (ni joystick, ni clavier) — contrairement au
        // déplacement classique (`move_speed` seul), purement piloté par l'entrée.
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::temple_run_demo();
        app.playing = true;
        // `input_state` reste à ses valeurs par défaut (aucune entrée).
        let z0 = app.player_position().unwrap().z;
        for _ in 0..40 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let z1 = app.player_position().unwrap().z;
        assert!(
            z1 > z0 + 1.0,
            "la course automatique doit avancer le joueur sans entrée (z0={z0}, z1={z1})"
        );
    }

    #[test]
    fn ai_chaser_actively_closes_distance_to_the_player() {
        // Cœur du « jeu local vs IA » : contrairement aux patrouilles scriptées à
        // trajectoire fixe (prévisibles, évitables par pattern), un `AiChaser` doit
        // se rapprocher réellement de la position courante du joueur, recalculée
        // chaque frame — une poursuite réactive.
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(30.0, 1.0, 30.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let mut chaser = crate::scene::SceneObject {
            name: "Chasseur".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(10.0, 0.5, 0.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 3.0 }),
            ..Default::default()
        };
        chaser.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![sol, joueur, chaser],
            ..Default::default()
        };
        app.playing = true;
        let dist0 = (app.scene.objects[2].transform.position - Vec3::new(0.0, 1.0, 0.0)).length();
        for _ in 0..60 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let player_pos = app.player_position().unwrap();
        let dist1 = (app.scene.objects[2].transform.position - player_pos).length();
        assert!(
            dist1 < dist0 - 1.0,
            "le chasseur doit se rapprocher du joueur (dist0={dist0}, dist1={dist1})"
        );
    }

    /// Vérifie que sur 3 chasseurs visant la même cible, seuls les
    /// `MAX_ACTIVE_CHASERS_PER_TARGET` (2) plus proches avancent réellement ;
    /// le 3e reste sur place ce tick (cf. GAMEDESIGN_EN_LIGNE.md).
    #[test]
    fn only_the_nearest_chasers_up_to_the_cap_advance_on_a_single_target() {
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(60.0, 1.0, 60.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            color: [1.0; 3],
            ..Default::default()
        };
        // Trois chasseurs à distances croissantes de la même cible : le
        // troisième (le plus loin) doit être celui relégué par le plafond.
        let chaser_at = |x: f32| crate::scene::SceneObject {
            name: format!("Chasseur {x}"),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(x, 0.5, 0.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 3.0 }),
            color: [1.0; 3],
            ..Default::default()
        };
        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![
                sol,
                joueur,
                chaser_at(6.0),
                chaser_at(10.0),
                chaser_at(14.0),
            ],
            ..Default::default()
        };
        app.playing = true;
        let start: Vec<Vec3> = (2..5)
            .map(|i| app.scene.objects[i].transform.position)
            .collect();
        for _ in 0..30 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let moved = |i: usize| (app.scene.objects[i].transform.position - start[i - 2]).length();
        assert!(
            moved(2) > 0.5,
            "le chasseur le plus proche doit avancer : déplacement {}",
            moved(2)
        );
        assert!(
            moved(3) > 0.5,
            "le 2e chasseur le plus proche doit aussi avancer : déplacement {}",
            moved(3)
        );
        assert!(
            moved(4) < 0.2,
            "au-delà du plafond, le 3e chasseur ne doit pas avancer ce tick : déplacement {}",
            moved(4)
        );
    }

    /// Même après le plafond par cible, avec une seule cible réseau vivante
    /// connectée, les chasseurs finissent par tous converger (le plafond étale
    /// l'arrivée dans le temps, il ne l'empêche pas). Vérifie
    /// qu'un chasseur **hors de portée de détection** (`CHASER_DETECT_RANGE`)
    /// reste totalement immobile face à un unique joueur réseau, même s'il
    /// serait autrement le seul/le plus proche (donc jamais relégué par le
    /// plafond). Un joueur **réseau**, pas local : la portée de détection est
    /// volontairement limitée au cas réseau (cf. le commentaire sur
    /// `CHASER_DETECT_RANGE` dans la boucle de pilotage IA) pour ne pas casser
    /// le ring-out de `Scene::brawl_demo` en solo.
    #[test]
    fn a_chaser_beyond_detection_range_never_moves_towards_a_lone_network_player() {
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(60.0, 1.0, 60.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let gabarit = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            color: [1.0; 3],
            ..Default::default()
        };
        // Bien au-delà de CHASER_DETECT_RANGE (9 m) : seule cible sur la carte,
        // donc jamais relégué par le plafond — sans la portée de détection, il
        // se rapprocherait quand même.
        let chaser = crate::scene::SceneObject {
            name: "Chasseur lointain".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(20.0, 0.5, 0.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 3.0 }),
            color: [1.0; 3],
            ..Default::default()
        };
        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![sol, gabarit, chaser],
            ..Default::default()
        };
        app.hide_local_player_template();
        app.spawn_network_player(1);
        app.playing = true;
        let start = app.scene.objects[2].transform.position;
        for _ in 0..60 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let moved = (app.scene.objects[2].transform.position - start).length();
        assert!(
            moved < 0.2,
            "un chasseur hors de portée de détection ne doit pas se rapprocher \
             de l'unique joueur réseau, aussi loin soit-il : déplacement {moved}"
        );
    }

    /// GAMEDESIGN_EN_LIGNE.md §3.2 (audit) : avant ce correctif, `chase_target`
    /// était un point unique (`self.player_position()`) — sur un serveur
    /// headless avec plusieurs joueurs réseau, cela désignait toujours le
    /// premier joueur à avoir rejoint (le premier objet visible piloté trouvé
    /// dans `scene.objects`), jamais le second même s'il était bien plus
    /// proche. Un monstre doit désormais poursuivre le joueur réseau **vivant**
    /// le plus proche, recalculé chaque frame.
    #[test]
    fn ai_chaser_pursues_the_nearest_network_player_not_just_the_first_joined() {
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(60.0, 1.0, 60.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut chaser = crate::scene::SceneObject {
            name: "Chasseur".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.5, -20.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 3.0 }),
            ..Default::default()
        };
        chaser.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![sol, joueur, chaser],
            ..Default::default()
        };
        app.playing = true;
        app.hide_local_player_template();
        let p1 = app.spawn_network_player(1).unwrap();
        let p2 = app.spawn_network_player(2).unwrap();
        let chaser_idx = 2; // sol=0, joueur(masqué)=1, chasseur=2, puis p1/p2 ajoutés ensuite.
        // Repositionne explicitement les deux joueurs (plutôt que de dépendre
        // de la géométrie de spawn de `spawn_network_player`, qui les place
        // proches l'un de l'autre sans garantir lequel est le plus près du
        // chasseur) : p1 loin de tout, p2 juste devant le chasseur.
        app.scene.objects[p1].transform.position = Vec3::new(0.0, 1.0, 30.0);
        app.scene.objects[p2].transform.position = Vec3::new(0.0, 1.0, -15.0);
        // Reconstruit la physique après avoir déplacé les objets « à la main » :
        // sans ça, les corps rigides (créés par `spawn_network_player` avec
        // l'ancienne position) écraseraient ce repositionnement dès le premier
        // pas de simulation (`Physics::step` recopie la pose du corps rigide
        // dans `transform`, jamais l'inverse).
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        let dist_to_p2_before = (app.scene.objects[chaser_idx].transform.position
            - app.scene.objects[p2].transform.position)
            .length();

        for _ in 0..60 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }

        let chaser_pos = app.scene.objects[chaser_idx].transform.position;
        let dist_to_p1 = (chaser_pos - app.scene.objects[p1].transform.position).length();
        let dist_to_p2 = (chaser_pos - app.scene.objects[p2].transform.position).length();
        assert!(
            dist_to_p2 < dist_to_p2_before - 1.0,
            "le chasseur doit se rapprocher du joueur réseau le plus proche (p2) : \
             avant={dist_to_p2_before}, après={dist_to_p2}"
        );
        assert!(
            dist_to_p2 < dist_to_p1,
            "le chasseur doit finir plus proche de p2 (le plus proche au départ) que de \
             p1 (le premier à avoir rejoint) : dist_p1={dist_to_p1}, dist_p2={dist_to_p2}"
        );
    }

    #[test]
    fn wave_system_reveals_next_wave_then_wins_on_the_last() {
        // 2 manches synthétiques d'un seul monstre chacune : ne doit révéler la manche 2
        // qu'une fois la manche 1 vidée, et gagner une fois la manche 2 vidée à son tour.
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 50.0, // portée large : le test cible la logique de manches, pas la précision d'attaque.
                attack_cooldown: 0.0, // pas de recharge : le test cible les manches, pas le rythme de combat.
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(30.0, 1.0, 30.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let mut m1 = crate::scene::SceneObject {
            name: "Monstre Vague1".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.5, 0.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 1.0 }),
            combat: Some(crate::scene::Combat {
                attackable: true,
                wave: 1,
                ..Default::default()
            }),
            ..Default::default()
        };
        m1.color = [1.0; 3];
        let mut m2 = crate::scene::SceneObject {
            name: "Monstre Vague2".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(-5.0, 0.5, 0.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 1.0 }),
            combat: Some(crate::scene::Combat {
                attackable: true,
                wave: 2,
                ..Default::default()
            }),
            ..Default::default()
        };
        m2.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![sol, joueur, m1, m2],
            ..Default::default()
        };
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play(); // entrée en Play : `init_waves` doit s'exécuter.

        assert_eq!(app.wave, 1, "démarre à la manche 1");
        assert!(
            app.scene.objects[2].visible,
            "manche 1 : le monstre 1 est révélé"
        );
        assert!(
            !app.scene.objects[3].visible,
            "manche 1 : le monstre 2 reste masqué"
        );

        // Attaque : tire sur le monstre de la manche 1 (portée large, toujours à portée),
        // puis laisse le temps au missile d'arriver (le coup n'est plus instantané, cf.
        // `AttackProjectile`) et à `update_waves` de détecter la manche vidée.
        app.input_state.buttons.insert("Attaque".into());
        for _ in 0..20 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.wave == 2 {
                // S'arrête dès la révélation de la manche 2, avant qu'un nouveau missile
                // (bouton toujours enfoncé) n'ait le temps de la vaincre aussi.
                break;
            }
        }
        app.input_state.buttons.clear();

        assert_eq!(app.wave, 2, "la manche 1 vidée doit révéler la manche 2");
        assert!(
            app.scene.objects[3].visible,
            "manche 2 : le monstre 2 est révélé"
        );
        assert!(
            app.win_time.is_none(),
            "pas encore gagné, la manche 2 reste à vider"
        );

        // Vainc le monstre de la manche 2 : dernière manche ⇒ victoire.
        app.input_state.buttons.insert("Attaque".into());
        for _ in 0..20 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        app.input_state.buttons.clear();

        assert!(
            app.win_time.is_some(),
            "toutes les manches vidées ⇒ victoire"
        );
    }

    #[test]
    fn zombies_demo_attack_range_stays_close_to_monster_bite_reach() {
        // Audit gameplay : la portée d'attaque totale (attack_range + rayon du monstre)
        // est un cercle qui **contient toujours** la boîte de morsure du monstre (rayon
        // ≈ son propre rayon) dès que `attack_range > 0` — un joueur qui fonce droit sur
        // un monstre gagnera donc structurellement la course à l'engagement, quelle que
        // soit sa vitesse. `attack_range` ne peut pas éliminer ce biais en 1 contre 1
        // frontal, seulement en réduire la marge (le vrai risque vient d'affronter
        // plusieurs monstres à la fois pendant la recharge). L'ancienne valeur (1,5 m)
        // donnait une marge de sécurité énorme (jusqu'à 4-5× le rayon du plus petit
        // monstre) ; verrouille qu'elle reste modeste désormais.
        let s = crate::scene::Scene::zombies_demo();
        let ctrl = s
            .objects
            .iter()
            .find_map(|o| o.controller.as_ref())
            .expect("un joueur pilotable");
        let smallest_monster_r = s
            .objects
            .iter()
            .filter(|o| o.ai_chaser.is_some())
            .map(|o| o.transform.scale.max_element() * 0.5)
            .fold(f32::INFINITY, f32::min);
        assert!(
            ctrl.attack_range <= smallest_monster_r + 0.5,
            "marge de sécurité trop généreuse : attack_range={} vs rayon du plus petit \
             monstre={smallest_monster_r} (marge > 0,5 m)",
            ctrl.attack_range
        );
    }

    #[test]
    fn attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing() {
        // Suite de l'audit gameplay : `attack_at` vainquait TOUTES les cibles à portée en
        // un seul appel (balayage de zone). Une expérimentation poussée (3 archétypes
        // convergeant en cercle serré sur un joueur immobile qui attaque en continu) a
        // montré qu'ils entraient dans le rayon de mise à mort de façon quasi synchronisée
        // — leur taille (donc leur propre rayon, qui élargit d'autant le rayon de mise à
        // mort perçu) compense presque exactement leur différence de vitesse. Résultat :
        // un groupe entier disparaissait en un seul coup, sans qu'aucun n'ait jamais mordu.
        // `attack_at` ne vainc désormais que la cible la plus proche : un groupe de 3
        // exige donc 3 coups (et donc 3 fenêtres de recharge), pas un seul.
        //
        // Limite honnête, documentée plutôt que masquée par un test fragile : ceci ne
        // garantit pas qu'un joueur qui reste immobile et attaque prendra des dégâts —
        // sans temps de préparation sur l'attaque, la portée d'attaque englobera toujours
        // la portée de morsure d'un monstre qui approche en ligne droite (cf.
        // `zombies_demo_attack_range_stays_close_to_monster_bite_reach`), donc gagner la
        // course à l'engagement 1 contre 1 reste structurellement favorable au joueur.
        // Un vrai risque garanti demanderait un temps de préparation sur l'attaque
        // (fenêtre de vulnérabilité avant que le coup ne porte) — hors du périmètre de ce
        // sprint, noté dans audit_sprint.md pour une prochaine itération.
        let mut s = crate::scene::Scene::default();
        s.objects.push(crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        for n in 0..3 {
            let mut m = crate::scene::SceneObject {
                name: format!("Monstre {n}"),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::new(0.2 * n as f32, 0.5, 0.0)),
                combat: Some(crate::scene::Combat {
                    attackable: true,
                    ..Default::default()
                }),
                ..Default::default()
            };
            m.color = [1.0; 3];
            s.objects.push(m);
        }
        // Les 3 sont groupés à moins de 0,5 m les uns des autres, largement à portée
        // d'une seule attaque à grand rayon.
        let hit = s.attack_at(Vec3::new(0.2, 0.5, 0.0), 5.0);
        assert_eq!(
            hit.len(),
            1,
            "une attaque ne vainc qu'une seule cible, pas tout le groupe"
        );
        let still_visible = s.objects[1..].iter().filter(|o| o.visible).count();
        assert_eq!(
            still_visible, 2,
            "les 2 autres cibles du groupe doivent survivre à ce coup"
        );
    }

    #[test]
    fn attack_mode_zone_clears_a_whole_cluster_in_one_swing() {
        // Contrepoint direct de `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing` :
        // ce dernier documente que le mode par défaut (`Single`) ne vainc qu'une cible à
        // la fois, précisément pour ne pas trivialiser un groupe convergent. Le mode
        // `AttackMode::Zone` (Marteau, cf. `Weapon::mode`) doit au contraire vaincre TOUT
        // le groupe d'un coup — c'est le point de payer une préparation/recharge plus
        // longues (cf. `WEAPONS`) : jamais d'état intermédiaire « 1 ou 2 des 3 vaincus ».
        let mut s = crate::scene::Scene::default();
        s.objects.push(crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.2, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 5.0,
                attack_cooldown: 1.0,
                attack_windup: 0.1,
                attack_mode: crate::scene::AttackMode::Zone,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        s.objects.push(joueur);
        for n in 0..3 {
            let mut m = crate::scene::SceneObject {
                name: format!("Monstre {n}"),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::new(0.2 * n as f32, 0.5, 0.0)),
                combat: Some(crate::scene::Combat {
                    attackable: true,
                    ..Default::default()
                }),
                ..Default::default()
            };
            m.color = [1.0; 3];
            s.objects.push(m);
        }

        let mut app = AppState::new();
        app.scene = s;
        app.playing = true;
        app.input_state.attack = true;
        let mut seen_counts: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for _ in 0..30 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            let visible = app.scene.objects[2..].iter().filter(|o| o.visible).count();
            seen_counts.insert(visible);
            if visible == 0 {
                break;
            }
        }
        assert!(
            seen_counts.contains(&0),
            "le mode Zone doit finir par vaincre tout le groupe"
        );
        assert!(
            !seen_counts.contains(&1) && !seen_counts.contains(&2),
            "jamais d'état intermédiaire \"1 ou 2 vaincus\" : la résolution doit toucher \
             les 3 cibles du groupe dans le même appel, pas une par une (vu={seen_counts:?})"
        );
    }

    /// Duel 1 contre 1 : sol statique, joueur pilotable (attaque à préparation) et un
    /// monstre-chasseur mordeur à 1 m. Le monstre a un **corps physique** (via
    /// `ai_chaser` + `visible`, cf. `Physics::build`) : contrairement aux dangers
    /// statiques de `synthetic_damage_scene`, sa collision solide repousse le joueur —
    /// c'est précisément la configuration où la morsure « centre dans l'AABB » échouait.
    fn duel_1v1_scene() -> crate::scene::Scene {
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 6.0,
                attack_cooldown: 0.5,
                attack_windup: 0.25,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(30.0, 1.0, 30.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        // À 1 m (rayon de morsure par défaut ≈ 0,5 m) et 4 m/s : atteint sa portée de
        // morsure en (1 - 0,5) / 4 = 0,125 s — avant la fin des 0,25 s de préparation,
        // donc avant même que le missile ne soit tiré.
        let mut monstre = crate::scene::SceneObject {
            name: "Monstre".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.5, 0.0)),
            trigger: true,
            ai_chaser: Some(crate::scene::AiChaser { speed: 4.0 }),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            script: "if obj.triggered then damage(5.0 * dt) end".into(),
            ..Default::default()
        };
        monstre.color = [1.0; 3];

        crate::scene::Scene {
            objects: vec![sol, joueur, monstre],
            ..Default::default()
        }
    }

    #[test]
    fn chasing_monster_with_solid_body_can_bite_the_player_on_contact() {
        // Régression du bug racine découvert par l'audit : la morsure testait « centre
        // du joueur dans l'AABB du monstre », or les colliders solides (joueur et
        // chasseur ont tous deux un corps rigide) empêchent toute interpénétration —
        // un monstre-chasseur ne mordait donc *jamais*, même en contact continu. Le
        // test de déclenchement est désormais une **intersection d'AABB** (cf.
        // `Scene::world_aabb_intersects`) : le contact suffit.
        let mut app = AppState::new();
        app.scene = duel_1v1_scene();
        app.playing = true;
        // Aucune attaque : on isole la collision physique pure (le joueur ne se défend
        // pas, le monstre doit finir par le mordre).
        app.input_state.attack = false;

        let mut took_damage = false;
        for _ in 0..40 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.hud_health.is_some() {
                took_damage = true;
                break;
            }
        }
        assert!(
            took_damage,
            "un monstre-chasseur au corps solide doit pouvoir mordre au contact, \
             malgré la répulsion physique qui interdit l'interpénétration des centres"
        );
    }

    #[test]
    fn attack_windup_finally_guarantees_risk_in_a_1v1() {
        // Clôt la limite documentée à répétition dans l'audit (le temps de vol du
        // missile seul ne suffisait pas à garantir un risque en 1 contre 1, cf.
        // `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing` et
        // `attack_is_a_missile_with_travel_time_not_an_instant_hit`) : un temps de
        // préparation (`Controller::attack_windup`) *avant même que le missile ne
        // parte* fonctionne, lui, indépendamment de la vitesse du missile — un monstre
        // déjà proche de sa propre portée de morsure au moment du tir peut mordre
        // pendant la préparation, avant qu'aucun projectile n'existe.
        let mut app = AppState::new();
        app.scene = duel_1v1_scene();
        app.playing = true;
        // Attaque maintenue dès la première frame : la préparation démarre aussitôt.
        app.input_state.attack = true;

        let mut bitten_before_kill = false;
        for _ in 0..40 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.hud_health.is_some() && app.scene.objects[2].visible {
                bitten_before_kill = true;
            }
            if !app.scene.objects[2].visible {
                break;
            }
        }
        assert!(
            !app.scene.objects[2].visible,
            "le missile doit finir par vaincre le monstre (sinon le duel ne se résout pas)"
        );
        assert!(
            bitten_before_kill,
            "un monstre déjà proche de sa portée de morsure doit pouvoir mordre pendant \
             la préparation de l'attaque, avant que le missile ne le vainque — gagner \
             un 1 contre 1 doit coûter de la vie"
        );
    }

    #[test]
    fn attack_is_a_missile_with_travel_time_not_an_instant_hit() {
        // L'attaque est désormais un missile homing avec un temps de vol (cf.
        // `AttackProjectile`), pas une résolution instantanée au tir : rend le coup
        // lisible en 3D (le missile se voit voyager, pas juste « la cible disparaît »).
        //
        // Limite honnête, re-vérifiée ici plutôt que survendue : le temps de vol NE
        // garantit PAS à lui seul un risque en 1 contre 1 — un missile homing tiré dès
        // l'entrée en portée arrive quasi toujours avant qu'un monstre qui fonce en
        // ligne droite n'ait eu le temps d'atteindre sa propre (bien plus courte) portée
        // de morsure, sauf à rendre le missile déraisonnablement lent. Le vrai risque
        // reste celui déjà documenté : affronter plusieurs monstres à la fois pendant la
        // recharge (cf. `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing`).
        // Ce test vérifie donc uniquement ce que le missile change réellement : un vol
        // progressif et homing, pas un « tout ou rien » au moment du tir.
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 6.0,
                attack_cooldown: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(30.0, 1.0, 30.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        // À 5 m : à portée du tir (6 m), le missile doit voyager plusieurs frames avant
        // d'arriver (pas de patrouille/chasse ici : isole le temps de vol lui-même).
        let mut monstre = crate::scene::SceneObject {
            name: "Monstre".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.5, 0.0)),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        monstre.color = [1.0; 3];
        let mut fx = crate::scene::SceneObject {
            name: "FX Attaque".into(),
            mesh: crate::scene::MeshKind::Sphere,
            combat: Some(crate::scene::Combat {
                is_attack_fx: true,
                ..Default::default()
            }),
            visible: false,
            ..Default::default()
        };
        fx.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![sol, joueur, monstre, fx],
            ..Default::default()
        };
        app.playing = true;
        app.input_state.attack = true;

        // Quelques pas : couvre la préparation (attack_windup, 0,25 s par défaut) sans
        // atteindre le temps de vol du missile (5 m à 10 m/s ≈ 0,5 s) — le monstre à 5 m
        // ne doit pas être vaincu si tôt.
        for _ in 0..6 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert!(
            app.scene.objects[2].visible,
            "le monstre à 5 m ne doit pas être vaincu dès la préparation/le tir : le \
             missile met du temps à arriver"
        );
        let fx_after_launch = app
            .scene
            .objects
            .iter()
            .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
            .map(|o| o.transform.position);

        // Quelques frames plus tard : le missile a progressé (pas téléporté).
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let fx_mid_flight = app
            .scene
            .objects
            .iter()
            .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
            .map(|o| o.transform.position);
        assert_ne!(
            fx_after_launch, fx_mid_flight,
            "l'ancre visuelle doit progresser vers la cible, pas rester figée"
        );

        // Laisse le temps au missile d'arriver.
        for _ in 0..20 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if !app.scene.objects[2].visible {
                break;
            }
        }
        assert!(
            !app.scene.objects[2].visible,
            "le missile doit finir par atteindre sa cible"
        );
    }

    #[test]
    fn damage_triggers_flash_that_fades_and_resets_on_stop() {
        // Retour visuel du coup : `damage_flash` doit monter à 1.0 dès la première baisse
        // de vie détectée, puis décroître frame après frame (pas rester bloqué au pic).
        let mut app = AppState::new();
        app.scene = synthetic_damage_scene();
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        // Le pic (1.0) est déclenché par le sim_step qui détecte le coup, mais cette même
        // frame applique déjà une frame de décroissance ensuite (comportement voulu : le
        // flash commence à s'estomper dès la frame du coup) — d'où la marge, pas `== 1.0`.
        let peak = app.damage_flash;
        assert!(
            peak > 0.8,
            "un coup doit déclencher un pic net du flash : {peak}"
        );
        // Sort du contact (sinon chaque frame retriggerait le pic à 1.0) pour vérifier la
        // décroissance en l'absence de nouveaux coups. Reconstruit le corps physique à sa
        // nouvelle position : sinon le pas de physique du même appel le ramènerait vers
        // l'ancienne pose (le corps rigide, lui, n'a pas bougé) et le contact reprendrait.
        if let Some(j) = app
            .scene
            .objects
            .iter_mut()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        {
            j.transform.position = Vec3::new(50.0, 0.5, 50.0);
        }
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert!(
            app.damage_flash < peak,
            "le flash doit continuer à décroître frame après frame hors contact"
        );
        // Sortir de Play remet tout à zéro (pas de flash résiduel visible en édition).
        app.playing = false;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert_eq!(
            app.damage_flash, 0.0,
            "le flash est effacé à la sortie de Play"
        );
    }

    #[test]
    fn controller_demo_lava_boil_script_preserves_collision_scale() {
        // La lave a un script de « bouillonnement » (pulsation de couleur) ajouté après coup ;
        // il ne doit surtout pas toucher à l'échelle Y, qui encode l'épaisseur de collision
        // nécessaire pour que la zone mortelle détecte un joueur debout (cf. test dédié dans
        // scene::tests). Une régression ici rendrait la lave inoffensive en silence.
        let scene = crate::scene::Scene::controller_demo();
        let lave = scene
            .objects
            .iter()
            .find(|o| o.name == "Lave")
            .expect("la lave existe");
        assert!(!lave.script.trim().is_empty(), "la lave doit être animée");
        let lua = Lua::new();
        let func = lua.load(&lave.script).into_function().unwrap();
        let mut t = lave.transform;
        let mut col = lave.color;
        let input = PlayerInput::default();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            3.7,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            t.scale, lave.transform.scale,
            "le script de la lave ne doit pas modifier l'échelle (collision)"
        );
        assert_eq!(
            t.position, lave.transform.position,
            "le script de la lave ne doit pas déplacer la mare"
        );
    }

    #[test]
    fn script_can_request_vibration() {
        let lua = Lua::new();
        let func = lua
            .load("if obj.tapped then vibrate(80) end")
            .into_function()
            .unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let input = PlayerInput::default();
        let mut vib = Vec::new();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            true,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut vib,
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(vib, vec![80.0]);
    }

    /// Sprint 121 : `reverb(mix)` — typiquement appelé depuis le script d'une
    /// zone `trigger` à l'entrée (`obj.triggered`) — empile la valeur demandée
    /// dans `reverb_out`, même mécanisme que `vibrate`/`vib_out` ci-dessus.
    #[test]
    fn script_can_request_reverb() {
        let lua = Lua::new();
        let func = lua
            .load("if obj.triggered then reverb(0.6) end")
            .into_function()
            .unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let input = PlayerInput::default();
        let mut reverb_out = Vec::new();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            true,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut reverb_out,
        )
        .unwrap();
        assert_eq!(reverb_out, vec![0.6]);
    }

    #[test]
    fn restart_game_restores_scene_and_clears_flags() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::controller_demo();
        app.play_snapshot = app.scene.objects.clone();
        // Simule une partie en cours : une gemme ramassée, perdu, chrono figé.
        if let Some(g) = app
            .scene
            .objects
            .iter_mut()
            .find(|o| o.tap_action == crate::scene::TapAction::Hide)
        {
            g.visible = false;
        }
        app.lost = true;
        app.win_time = Some(5.0);
        app.time = 5.0;

        app.restart_game();

        assert!(!app.lost, "défaite remise à zéro");
        assert!(app.win_time.is_none(), "victoire remise à zéro");
        assert_eq!(app.time, 0.0, "chrono remis à zéro");
        // Scopé aux gemmes (Hide) : d'autres objets sont légitimement invisibles par défaut
        // dans cette démo (ex. l'ancre `is_attack_fx`, masquée tant qu'aucun coup ne porte).
        assert!(
            app.scene
                .objects
                .iter()
                .filter(|o| o.tap_action == crate::scene::TapAction::Hide)
                .all(|o| o.visible),
            "toutes les gemmes redeviennent visibles"
        );
    }

    #[test]
    fn axis_basis_is_orthonormal() {
        for axis in 0..3 {
            let a = axis_dir(axis);
            let (u, w) = axis_basis(a);
            assert!((u.length() - 1.0).abs() < 1e-5);
            assert!((w.length() - 1.0).abs() < 1e-5);
            assert!(u.dot(a).abs() < 1e-5);
            assert!(w.dot(a).abs() < 1e-5);
            assert!(u.dot(w).abs() < 1e-5);
        }
    }
}
