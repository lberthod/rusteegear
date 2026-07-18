//! Messages échangés entre client et serveur.
//!
//! Sérialisation via `bincode` (compact, binaire) plutôt que JSON : ces messages
//! circulent potentiellement plusieurs fois par seconde et par joueur (snapshots),
//! là où `serde_json` (déjà utilisé ailleurs dans le projet pour les scènes,
//! peu fréquent) resterait lisible mais plus lourd. `serde_json::to_string` reste
//! disponible sur les mêmes types pour inspecter une trame en debug si besoin
//! (aucune dépendance supplémentaire : `serde_json` est déjà dans `Cargo.toml`).

use serde::{Deserialize, Serialize};

/// Identifiant de joueur, attribué par le serveur à la connexion (`ServerMsg::Welcome`).
pub type PlayerId = u32;

/// Version du protocole réseau, négociée au `Join` : le serveur rejette
/// proprement (`ServerMsg::JoinRejected`, avec un message clair) tout client
/// d'une autre version, au lieu de l'ancien comportement — un `decode` qui
/// échoue et une connexion fermée sans le moindre diagnostic. À incrémenter à
/// **chaque** changement incompatible de `ClientMsg`/`ServerMsg`/leurs types
/// imbriqués (champ ajouté sans `#[serde(default)]`, variant réordonné…).
/// ⚠️ Déploiement : un bump casse le format — client et serveur (VPS) doivent
/// être redéployés ensemble (vérifier ensuite avec `examples/smoke_vps.rs`).
///
/// v2 (GAMEDESIGN_MMORPG.md §3.2) : `ClientMsg::Join` gagne `class: u8`.
/// v3 → v4 (Sprint 2 et Sprint 5, `sprint10audit.md`, changements groupés dans
/// le même bump) : `GameEvent::PlayerDown` gagne `cause` (diagnostic de mort)
/// et `ClientMsg::Join` gagne `objective: u8` (mode de manche du salon, cf.
/// `RoundObjective`).
/// v5 (Sprint 5, `sprint10audit.md`, auto-relecture) : `GameEvent` gagne le
/// variant `RoundObjective { objective: u8 }` — le mode choisi n'était
/// propagé que du client vers le serveur (v4), jamais renvoyé au client une
/// fois arbitré par le salon (premier `Join` gagnant, cf. `Lobby::objective`) ;
/// sans ce sens retour, un client restait sur son défaut `Vagues` local alors
/// que le salon tournait en `Survie`.
/// v6 (Phase H, Sprint 1 — écran de fin de manche détaillé, GDD §9.2/§17.4) :
/// `GameEvent::Win`/`Lose` gagnent un résumé par joueur (`Vec<RoundPlayerSummary>`,
/// frags/assists/XP déjà calculés côté serveur mais jusqu'ici jamais renvoyés
/// au client), `Win` gagne en plus `contract: Option<u8>` (contrat du jour
/// rempli par cette manche, cf. `Contract::to_u8`) — sans quoi le client
/// n'avait aucun moyen d'afficher un détail de manche, seulement la bannière
/// minimale pilotée par sa propre simulation locale.
pub const PROTOCOL_VERSION: u32 = 6;

/// Code de salon utilisé quand `ClientMsg::Join::lobby` est vide — tous les
/// clients qui n'en précisent pas (cf. GAMEDESIGN_EN_LIGNE.md §3.3) s'y
/// retrouvent donc ensemble : le serveur route par code de salon, mais rien
/// ne change tant qu'aucune UI ne propose d'en choisir un autre.
pub const DEFAULT_LOBBY: &str = "default";

/// Longueur maximale (caractères) d'un pseudo (`ClientMsg::Join::name`) —
/// affiché aux autres joueurs, pas utilisé comme clé/chemin, donc plus
/// permissif en charset que `lobby`/`firebase_uid`, mais borné en taille
/// (Sprint 105a-2, durcissement des entrées réseau : sans cette borne, un
/// client buggé/malveillant pouvait envoyer un nom de plusieurs Mo, stocké
/// et rediffusé tel quel à tous les pairs).
pub const MAX_NAME_LEN: usize = 32;

/// Longueur maximale (caractères) d'un code de salon (`ClientMsg::Join::
/// lobby`) — devient directement une clé de `HashMap<String, Room>` côté
/// serveur (`bin/server.rs`), charset restreint (cf. `valid_join_fields`).
pub const MAX_LOBBY_LEN: usize = 32;

/// Longueur maximale (caractères) d'un `uid` Firebase (`ClientMsg::Join::
/// firebase_uid`) — un vrai uid Firebase fait ~28 caractères alphanumériques ;
/// cette borne est volontairement plus large pour ne pas coupler ce
/// validateur au format exact de Firebase, tout en restant loin de tout
/// usage légitime.
pub const MAX_FIREBASE_UID_LEN: usize = 128;

/// Valide les champs de `ClientMsg::Join` avant tout traitement côté serveur
/// (Sprint 105a-2) : longueur bornée pour les trois champs, et charset
/// restreint (alphanumérique + `-`/`_`) pour `lobby`/`firebase_uid` — tous
/// deux finissent, non échappés, dans un chemin d'URL Firebase RTDB
/// (`net::firebase::rtdb_url`) ou comme clé de `HashMap` ; un `/`, `?`, `#`
/// ou un `..` y aurait un effet indésirable (nœud RTDB différent de celui
/// prévu, confusion de salon). `name` (pseudo affiché, jamais utilisé comme
/// clé/chemin) n'a pas cette contrainte de charset, seulement une longueur
/// maximale.
pub fn valid_join_fields(
    name: &str,
    lobby: &str,
    firebase_uid: Option<&str>,
) -> Result<(), String> {
    let is_safe_token = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    if name.trim().is_empty() {
        return Err("le pseudo ne peut pas être vide".to_string());
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(format!("le pseudo dépasse {MAX_NAME_LEN} caractères"));
    }
    // `lobby` vide est légitime (repli sur `DEFAULT_LOBBY`, cf. sa doc) — ne
    // valide le charset que s'il est non vide.
    if !lobby.is_empty() {
        if lobby.chars().count() > MAX_LOBBY_LEN {
            return Err(format!(
                "le code de salon dépasse {MAX_LOBBY_LEN} caractères"
            ));
        }
        if !is_safe_token(lobby) {
            return Err("le code de salon contient des caractères non autorisés".to_string());
        }
    }
    if let Some(uid) = firebase_uid {
        if uid.chars().count() > MAX_FIREBASE_UID_LEN {
            return Err(format!(
                "l'identifiant Firebase dépasse {MAX_FIREBASE_UID_LEN} caractères"
            ));
        }
        if !is_safe_token(uid) {
            return Err("l'identifiant Firebase contient des caractères non autorisés".to_string());
        }
    }
    Ok(())
}

/// Message envoyé par un client au serveur.
///
/// **Invariant de compatibilité** (à préserver pour toujours) : `Join` reste
/// le variant 0 de cet enum et `protocol` son **premier** champ — bincode
/// sérialise l'indice du variant puis les champs dans l'ordre, c'est ce qui
/// permet à un serveur futur de lire au moins la version d'un vieux client
/// (et inversement) avant que le reste de la trame ne diverge, et donc de
/// répondre un `JoinRejected` intelligible plutôt qu'une erreur de décodage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Première trame envoyée à la connexion : demande à rejoindre un salon.
    Join {
        /// Version du protocole parlée par ce client (cf. `PROTOCOL_VERSION`
        /// et l'invariant de l'enum) : vérifiée par le serveur avant toute
        /// autre chose, `JoinRejected` si elle ne correspond pas.
        protocol: u32,
        name: String,
        /// `uid` Firebase (cf. `net::firebase::AuthSession`), si le joueur s'est
        /// connecté avant de rejoindre — sert au serveur pour créditer la
        /// progression de fin de manche au bon compte. `None` pour une partie
        /// locale/anonyme (identique à l'absence de compte, aucune régression).
        firebase_uid: Option<String>,
        /// Code du salon à rejoindre (créé à la demande s'il n'existe pas
        /// encore, cf. `bin/server.rs::Room`) — vide traité comme
        /// `DEFAULT_LOBBY` côté serveur (rétrocompatible : un client qui
        /// n'envoie rien de particulier atterrit dans le même salon partagé
        /// que tout le monde).
        lobby: String,
        /// Classe choisie (GAMEDESIGN_MMORPG.md §3.2, `PROTOCOL_VERSION` 2) :
        /// `0` = Assaut (défaut, les valeurs actuelles — zéro régression pour
        /// qui ne choisit pas), `1` = Éclaireur, `2` = Soutien. Une valeur
        /// hors table est traitée comme Assaut par `spawn_network_player`
        /// (jamais rejetée : un client futur avec une classe non reconnue ne
        /// doit pas perdre sa connexion pour ça).
        class: u8,
        /// Mode de manche choisi (Phase C, `sprint10audit.md`, `PROTOCOL_VERSION`
        /// 4) : `0` = Vagues (défaut, comportement historique — zéro régression
        /// pour qui ne choisit pas), `1` = Survie, `2` = Escorte, `3` = Boss.
        /// Fixé à la **création** du salon (premier `Join` d'un code encore
        /// inconnu, cf. `bin/server.rs::Lobby::objective`) — les joins suivants
        /// dans le même salon héritent du mode déjà choisi, comme `class` mais
        /// au niveau du salon plutôt que du joueur. Valeur hors table traitée
        /// comme Vagues par `RoundObjective::from_u8` (même principe que
        /// `class` : jamais de connexion refusée pour ça).
        objective: u8,
    },
    /// État des contrôles pour le tick courant (cf. `app::PlayerInput`, en plus
    /// compact — un client réseau ne pilote qu'un joueur, pas un overlay tactile
    /// complet). Envoyé à chaque tick client, même sans changement (le serveur ne
    /// mémorise pas d'état d'input entre deux messages).
    Input {
        move_x: f32,
        move_y: f32,
        /// Orientation du personnage (yaw, radians) telle que le client la
        /// prédit/affiche localement. Le serveur l'applique telle quelle (après
        /// nettoyage `NaN`/infini) : l'orientation n'a pas d'enjeu anti-triche
        /// (le collider est une capsule symétrique) mais elle décide de la
        /// direction du **tir** — le bloc d'orientation de `sim_step` est
        /// réservé au joueur local, ce champ est la seule source d'orientation
        /// pour un joueur réseau côté serveur/autres clients (cf.
        /// docs/audits/net.md pour le bug réel que son absence a causé).
        aim_yaw: f32,
        attack: bool,
        jump: bool,
        /// Tir de l'arme à distance sélectionnée (cf. `app::fireball`) : distinct
        /// de `attack` (coup au contact) — le serveur valide son propre temps de
        /// recharge, comme pour `attack`.
        fire: bool,
        /// Arme à distance sélectionnée (indice dans `app::fireball::
        /// RANGED_WEAPONS`, borné côté serveur) : envoyée à chaque tick comme le
        /// reste de l'état (pas un évènement « changement d'arme » à fiabiliser).
        weapon: u8,
        /// Soin d'un allié proche (cf. `app::health`, GAMEDESIGN_EN_LIGNE.md §3.6) :
        /// maintenu à portée d'un allié blessé, transfère des PV au fil du temps.
        /// Résolu et validé **côté serveur** (portée, cible, débit), comme le reste.
        heal: bool,
    },
    /// Déconnexion volontaire (quitte le salon proprement).
    Leave,
}

/// Message envoyé par le serveur à un ou plusieurs clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Réponse à `ClientMsg::Join` : attribue l'identifiant du joueur.
    Welcome { player_id: PlayerId },
    /// Un autre joueur a rejoint le salon.
    PlayerJoined { player_id: PlayerId, name: String },
    /// Un joueur a quitté (volontairement ou par timeout).
    PlayerLeft { player_id: PlayerId },
    /// Delta d'état du monde depuis le dernier snapshot envoyé à ce client.
    Snapshot(Snapshot),
    /// Évènement ponctuel (pas un état continu) : manche suivante, victoire, défaite...
    Event(GameEvent),
    /// Le serveur refuse le `Join` (version de protocole incompatible…) : envoyé
    /// juste avant de fermer la connexion, pour que le client puisse afficher la
    /// raison au lieu d'un échec silencieux. Ajouté en **fin** d'enum pour ne
    /// décaler aucun variant existant (même logique d'ordre bincode que
    /// l'invariant documenté sur `ClientMsg`). Rejet **fatal** côté client : ne
    /// jamais enchaîner sur une reconnexion automatique (le serveur nous
    /// refuserait en boucle), cf. `app::network_client::handle_server_msg`.
    JoinRejected { reason: String },
}

/// État des joueurs réseau pour un tick donné : **pas** un delta par client
/// malgré le nom — `AppState::network_snapshot` (`app/multiplayer.rs`)
/// diffuse l'état complet de tous les joueurs réseau à chaque tick, identique
/// pour tous les clients (`send_to` en boucle sur le salon, cf.
/// `src/bin/server.rs` — même contenu pour chacun, juste pas de delta
/// individualisé) — pas l'état complet de *la scène* non plus, seulement les
/// entités pilotées par un joueur réseau (les monstres/décor ne sont pas
/// encore diffusés, cf. `network_snapshot`). Un vrai delta par client
/// (mémoriser le dernier état envoyé à chaque `PlayerId`) resterait à faire
/// si le nombre d'entités diffusées grandissait significativement (cf.
/// docs/audits/net.md pour la mesure qui montre que ce n'est pas justifié
/// aujourd'hui).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// Numéro de tick serveur (monotone), pour que le client ignore un snapshot
    /// reçu hors ordre (réseau) plutôt que de reculer dans le temps.
    pub tick: u32,
    pub entities: Vec<EntityDelta>,
    /// Projectiles en vol (cf. `app::fireball`) : pas des entités de
    /// `scene.objects` (ils naissent et meurent en quelques secondes), donc pas
    /// des `EntityDelta` — chaque client les affiche via un petit pool local de
    /// sphères (cf. `AppState::sync_fireball_pool`), sans identité persistante
    /// à suivre d'un tick à l'autre.
    pub projectiles: Vec<ProjectileState>,
    /// Projectiles de créature en vol (morsure exceptée, purement au contact —
    /// cf. `app::creature_attack`) : même non-identité qu'un `ProjectileState` de
    /// joueur, `cfg` en plus (indice dans `creature_attack::RANGED_CREATURE_ATTACKS`,
    /// une table Rust compilée identique des deux côtés — sûr à référencer par
    /// indice comme `ProjectileState::weapon` le fait déjà pour les armes du joueur).
    #[serde(default)]
    pub creature_shots: Vec<CreatureShotState>,
}

/// Projectile de créature en vol pour un tick donné (cf. `Snapshot::creature_shots`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CreatureShotState {
    pub position: [f32; 3],
    pub dir: [f32; 3],
    /// Indice dans `app::creature_attack::RANGED_CREATURE_ATTACKS` — détermine
    /// vitesse/couleur/rayon à l'affichage côté client, cf. sa doc.
    pub cfg: u8,
}

/// Projectile en vol pour un tick donné : position + arme d'origine (l'aspect —
/// couleur, taille — dépend de l'arme, cf. `app::fireball::RANGED_WEAPONS`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectileState {
    pub position: [f32; 3],
    pub weapon: u8,
}

/// État minimal d'une entité de `scene.objects`, suffisant pour l'affichage/
/// l'interpolation côté client — pas la représentation complète de
/// `SceneObject` (mesh, script, composants...), qui reste une donnée de scène
/// locale au client, chargée une fois au join, pas retransmise à chaque tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityDelta {
    /// Indice dans `scene.objects`, stable pour la durée d'une manche (les objets
    /// ne sont ni ajoutés ni réordonnés en cours de partie côté serveur).
    pub index: u32,
    /// Joueur réseau propriétaire de cette entité, `None` pour une entité qui
    /// n'appartient à personne (réservé pour de futurs monstres/décor animé
    /// diffusés, cf. la limite documentée dans `AppState::network_snapshot` —
    /// pas encore le cas aujourd'hui, seuls les joueurs réseau sont diffusés).
    /// Sert au client à distinguer « mon propre joueur » (piloté localement en
    /// prédiction, jamais écrasé par le snapshot) des « autres joueurs »
    /// (affichés en fantômes interpolés) — sans ce champ, les deux étaient
    /// indiscernables une fois reçus.
    pub player_id: Option<PlayerId>,
    pub position: [f32; 3],
    /// Orientation autour de l'axe Y (radians) : suffisant pour un personnage/
    /// monstre au sol, évite d'envoyer un quaternion complet.
    pub yaw: f32,
    pub visible: bool,
    /// `Some` uniquement pour les entités qui portent une vie : les joueurs
    /// réseau (0..1, cf. `app::health`, GAMEDESIGN_EN_LIGNE.md §3.1 — vie
    /// individualisée par joueur, plus le champ scalaire unique d'avant) et les
    /// monstres synchronisés (`Combat::hp`, non normalisé). Absent (`None`,
    /// pas sérialisé à 0 par défaut) pour un décor sans vie.
    pub health: Option<f32>,
    /// Animation répliquée : nom du clip actuellement joué côté serveur
    /// (vide = objet non skinné ou pose de liaison, cf. `AnimationState::clip`).
    /// **Pas** de temps de lecture ici : chaque client avance déjà localement le
    /// temps de son propre `AnimationState` à chaque pas fixe, que l'objet soit
    /// local ou un fantôme réseau (cf. `AppState::sim_step`) — seul le *choix* du
    /// clip a besoin d'être répliqué, via `AnimationState::set_clip()` (fondu
    /// enchaîné inclus), pour que tous les écrans jouent la même animation.
    /// `String` plutôt qu'un indice numérique dans `ImportedMesh::clips` :
    /// robuste même si l'ordre des clips différait d'un client à l'autre, pour
    /// un coût négligeable tant que peu d'entités animées sont diffusées.
    #[serde(default)]
    pub anim_clip: String,
    /// Frags individualisés (GAMEDESIGN_EN_LIGNE.md — brique de progression pour
    /// un futur MMORPG) : nombre de monstres vaincus par **ce** joueur réseau
    /// depuis sa connexion (attaque au contact et boule de feu confondues, cf.
    /// `AppState::network_kills`). `Some` uniquement pour les entités-joueur,
    /// comme `health` — `None` pour les monstres/décor. Diffusé à tous (pas
    /// seulement au joueur concerné) : affiche le score de chacun dans le
    /// salon, pas seulement le sien — la contribution individuelle est un vrai
    /// signal social/compétitif en coopératif.
    #[serde(default)]
    pub kills: Option<u32>,
    /// Assists individualisés (Phase L Sprint 3, `sprint2audijeu0718.md` — GDD
    /// §8.3) : nombre de fois où **ce** joueur a porté un dégât à un monstre
    /// achevé par un autre joueur peu après (cf.
    /// `AppState::network_player_assists`/`credit_assists_on_kill`), distinct
    /// de `kills` ci-dessus (jamais incrémentés pour la même mise à mort). Même
    /// politique que `kills` : `Some` uniquement pour les entités-joueur,
    /// diffusé à tous.
    #[serde(default)]
    pub assists: Option<u32>,
}

/// Type d'agresseur à l'origine des derniers dégâts reçus avant une mort
/// (Sprint 2, `sprint10audit.md` — diagnostic de mort, GDD §16.5). Distingue
/// seulement les deux sources de dégâts réseau existantes côté serveur (cf.
/// `app::health::update_network_health`/`update_creature_bite`) : pas de
/// variant générique « inconnu », les deux fonctions qui tuent un joueur
/// réseau savent toujours laquelle des deux les a touchés.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeathCauseKind {
    /// Contact avec un monstre `AiChaser` (`MONSTER_CONTACT_DPS`).
    Monster,
    /// Morsure d'une créature scriptée (`SceneObject::bite`).
    Creature,
}

/// Résumé de la cause de mort d'un joueur réseau (Sprint 2) : type
/// d'agresseur dominant sur la courte fenêtre précédant la mort, et nombre
/// d'agresseurs *distincts* de ce type impliqués (ex. « Encerclé — 2
/// Traqueuses », GDD §16.5) — pas un historique complet, juste de quoi
/// afficher une cause lisible sans alourdir le protocole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathCause {
    pub kind: DeathCauseKind,
    pub distinct_attackers: u8,
}

/// Résumé d'un joueur en fin de manche (Phase H, Sprint 1, GDD §9.2/§17.4) :
/// frags/assists déjà calculés côté serveur (`network_player_score`/
/// `network_player_assists`, `bin/server.rs`), XP gagnée cette manche selon
/// le même barème que `award_progress` (`round_xp`) — un calcul dupliqué,
/// pas partagé, `round_xp` restant privé à `bin/server.rs` et ce module ne
/// dépendant pas de ce binaire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundPlayerSummary {
    pub player_id: PlayerId,
    pub name: String,
    pub frags: u32,
    pub assists: u32,
    pub xp: u32,
}

/// Évènement ponctuel de gameplay, distinct d'un `Snapshot` (état continu) : le
/// client peut y réagir une fois (son, flash HUD) sans avoir à comparer deux états.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameEvent {
    WaveStart {
        wave: u32,
    },
    Defeated {
        index: u32,
    },
    /// Manche gagnée (Phase H, Sprint 1) : `summary` un par joueur connecté du
    /// salon, `contract` le contrat du jour rempli par cette manche s'il y en
    /// a un (`Contract::to_u8`, `None` sinon — jamais rempli sur une défaite,
    /// cf. `Lose` qui n'a donc pas ce champ).
    Win {
        summary: Vec<RoundPlayerSummary>,
        contract: Option<u8>,
    },
    Lose {
        summary: Vec<RoundPlayerSummary>,
    },
    /// Un joueur réseau vient de tomber à 0 PV (cf. `app::health`,
    /// GAMEDESIGN_EN_LIGNE.md §3.1) : devient spectateur pour le reste de la
    /// manche (objet masqué, entrées ignorées côté serveur). Diffusé une fois,
    /// pas un état continu — chaque client y réagit une fois (son) plutôt que de
    /// comparer deux snapshots.
    PlayerDown {
        player_id: PlayerId,
        /// `None` si aucune source de dégâts n'a été mémorisée avant la mort
        /// (ex. import/scène de test qui met `network_health` à 0 directement) —
        /// le client retombe alors sur la bannière sans détail (comportement
        /// préexistant).
        cause: Option<DeathCause>,
    },
    /// Mode de manche du salon rejoint (Phase C, Sprint 5 — suite,
    /// `sprint10audit.md`) : envoyé une fois au joueur qui vient de rejoindre
    /// (`bin/server.rs`, juste après `ServerMsg::PlayerJoined`), pour que sa
    /// propre `AppState::objective` locale (utilisée par `update_round`, cf.
    /// `app::combat`) corresponde à celle, autoritaire, de `Room::app` côté
    /// serveur. Indispensable dès qu'un mode diverge de `Vagues` : chaque
    /// client exécute sa propre copie de la logique de manche pour son HUD/
    /// ses transitions locales (la visibilité des monstres, elle, reste
    /// strictement dictée par les `Snapshot`, cf. `EntityDelta::visible`) —
    /// sans ce message, un salon `Survie` verrait un client resté sur le
    /// défaut `Vagues` déclencher une victoire locale prématurée dès que la
    /// dernière manche se vide, alors que le serveur la reboucle. Encodé en
    /// `u8` comme `ClientMsg::Join::objective` (`RoundObjective::to_u8`),
    /// décodé avec le même repli sur `Vagues` pour une valeur hors table.
    RoundObjective {
        objective: u8,
    },
}

/// Erreur de (dé)sérialisation d'un message réseau.
#[derive(Debug)]
pub struct CodecError(bincode::Error);

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "erreur de codec réseau : {}", self.0)
    }
}
impl std::error::Error for CodecError {}

/// Encode un message en binaire compact (`bincode`) pour l'envoi sur le transport
/// (WebSocket, une trame binaire par message).
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, CodecError> {
    bincode::serialize(msg).map_err(CodecError)
}

/// Décode un message reçu du transport. Erreur si les octets ne correspondent pas
/// au type attendu (version de protocole incompatible, trame corrompue...).
pub fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, CodecError> {
    bincode::deserialize(bytes).map_err(CodecError)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(value: T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let bytes = encode(&value).expect("encodage");
        let back: T = decode(&bytes).expect("décodage");
        assert_eq!(value, back, "round-trip doit préserver la valeur");
    }

    #[test]
    fn client_msg_join_round_trips() {
        round_trip(ClientMsg::Join {
            protocol: PROTOCOL_VERSION,
            name: "Loïc".to_string(),
            firebase_uid: None,
            lobby: DEFAULT_LOBBY.to_string(),
            class: 0,
            objective: 0,
        });
        round_trip(ClientMsg::Join {
            protocol: PROTOCOL_VERSION,
            name: "Loïc".to_string(),
            firebase_uid: Some("uid-1234".to_string()),
            lobby: "salon-prive".to_string(),
            class: 2,
            objective: 1,
        });
    }

    #[test]
    fn server_msg_join_rejected_round_trips() {
        round_trip(ServerMsg::JoinRejected {
            reason: "version de protocole 999 incompatible (serveur : 1)".to_string(),
        });
    }

    /// Un `Join` au **vieux** format (d'avant le champ `protocol`, octets
    /// forgés à la main : variant 0 puis directement le pseudo) ne doit pas
    /// décoder silencieusement en un `Join` valide de la version courante —
    /// c'est la garantie que le versioning ne crée pas de faux positifs où un
    /// vieux client serait accepté avec des champs décalés.
    #[test]
    fn a_pre_versioning_join_does_not_decode_as_a_valid_join() {
        // Vieux format : [variant u32][len name u64]["A"][Option None u8][len lobby u64][]
        let mut old_join = Vec::new();
        old_join.extend_from_slice(&0u32.to_le_bytes()); // variant 0 = Join
        old_join.extend_from_slice(&1u64.to_le_bytes()); // name : 1 octet
        old_join.push(b'A');
        old_join.push(0); // firebase_uid : None
        old_join.extend_from_slice(&0u64.to_le_bytes()); // lobby : vide
        let decoded: Result<ClientMsg, _> = decode(&old_join);
        match decoded {
            // Refusé au décodage : parfait, la connexion se ferme comme avant.
            Err(_) => {}
            // Décodé malgré tout (les octets du pseudo ont été lus comme la
            // version) : la version ne doit alors JAMAIS être la courante,
            // sinon un vieux client passerait le contrôle avec des champs
            // corrompus.
            Ok(ClientMsg::Join { protocol, .. }) => {
                assert_ne!(
                    protocol, PROTOCOL_VERSION,
                    "un Join du vieux format ne doit pas se faire passer pour la version courante"
                );
            }
            Ok(other) => panic!("décodage inattendu d'un vieux Join : {other:?}"),
        }
    }

    #[test]
    fn client_msg_input_round_trips() {
        round_trip(ClientMsg::Input {
            move_x: -0.7,
            move_y: 1.0,
            aim_yaw: 1.57,
            attack: true,
            jump: false,
            fire: true,
            weapon: 2,
            heal: true,
        });
    }

    #[test]
    fn client_msg_leave_round_trips() {
        round_trip(ClientMsg::Leave);
    }

    #[test]
    fn server_msg_welcome_round_trips() {
        round_trip(ServerMsg::Welcome { player_id: 42 });
    }

    #[test]
    fn server_msg_player_joined_round_trips() {
        round_trip(ServerMsg::PlayerJoined {
            player_id: 3,
            name: "Zoé".to_string(),
        });
    }

    #[test]
    fn server_msg_player_left_round_trips() {
        round_trip(ServerMsg::PlayerLeft { player_id: 3 });
    }

    #[test]
    fn server_msg_snapshot_round_trips() {
        round_trip(ServerMsg::Snapshot(Snapshot {
            tick: 12345,
            creature_shots: vec![CreatureShotState {
                position: [2.0, 0.5, -1.0],
                dir: [0.0, 0.0, 1.0],
                cfg: 1,
            }],
            projectiles: vec![
                ProjectileState {
                    position: [1.0, 1.4, -3.0],
                    weapon: 0,
                },
                ProjectileState {
                    position: [0.0, 0.9, 4.2],
                    weapon: 2,
                },
            ],
            entities: vec![
                EntityDelta {
                    index: 0,
                    player_id: Some(1),
                    position: [1.0, 0.0, -2.5],
                    yaw: 0.78,
                    visible: true,
                    health: Some(0.75),
                    anim_clip: String::new(),
                    kills: Some(3),
                    assists: Some(1),
                },
                EntityDelta {
                    index: 7,
                    player_id: None,
                    position: [0.0, 0.0, 0.0],
                    yaw: 0.0,
                    visible: false,
                    health: None,
                    anim_clip: String::new(),
                    kills: None,
                    assists: None,
                },
            ],
        }));
    }

    #[test]
    fn server_msg_event_round_trips() {
        round_trip(ServerMsg::Event(GameEvent::WaveStart { wave: 2 }));
        round_trip(ServerMsg::Event(GameEvent::Defeated { index: 5 }));
        round_trip(ServerMsg::Event(GameEvent::Win {
            summary: vec![RoundPlayerSummary {
                player_id: 1,
                name: "Loïc".to_string(),
                frags: 3,
                assists: 1,
                xp: 245,
            }],
            contract: Some(2),
        }));
        round_trip(ServerMsg::Event(GameEvent::Lose {
            summary: vec![RoundPlayerSummary {
                player_id: 1,
                name: "Loïc".to_string(),
                frags: 0,
                assists: 0,
                xp: 150,
            }],
        }));
        round_trip(ServerMsg::Event(GameEvent::PlayerDown {
            player_id: 7,
            cause: Some(DeathCause {
                kind: DeathCauseKind::Monster,
                distinct_attackers: 2,
            }),
        }));
    }

    #[test]
    fn decode_rejects_garbage_bytes() {
        let garbage = [0xFFu8; 3];
        let result: Result<ServerMsg, _> = decode(&garbage);
        assert!(
            result.is_err(),
            "des octets invalides ne doivent pas décoder silencieusement"
        );
    }

    /// Mesure la taille d'un snapshot réaliste (16 joueurs + quelques monstres actifs)
    /// pour documenter le coût réseau (objectif : < 200 octets/joueur/tick). Pas une
    /// assertion stricte de taille exacte (le format peut évoluer) mais une garde-fou
    /// générale contre une régression grossière (ex. passer en JSON par erreur, ou
    /// dupliquer les données par inadvertance).
    #[test]
    fn snapshot_size_for_sixteen_players_stays_compact() {
        let entities: Vec<EntityDelta> = (0..16 + 4) // 16 joueurs + 4 monstres actifs
            .map(|i| EntityDelta {
                index: i,
                player_id: Some(i),
                position: [i as f32, 0.0, -(i as f32)],
                yaw: 0.3,
                visible: true,
                health: Some(0.9),
                anim_clip: String::new(),
                kills: Some(i),
                assists: Some(i / 2),
            })
            .collect();
        let snapshot = ServerMsg::Snapshot(Snapshot {
            tick: 999_999,
            creature_shots: Vec::new(),
            entities,
            // Quelques boules de feu en vol : le cas réaliste d'une manche animée.
            projectiles: vec![
                ProjectileState {
                    position: [3.0, 1.4, -2.0],
                    weapon: 1,
                };
                4
            ],
        });
        let bytes = encode(&snapshot).expect("encodage");
        let per_entity = bytes.len() as f32 / 20.0;
        println!(
            "Snapshot 20 entités : {} octets total, {:.1} octets/entité",
            bytes.len(),
            per_entity
        );
        // Budget large et documenté plutôt qu'un chiffre figé : alerte si le format
        // dérive fortement (ex. passage accidentel en JSON), pas un micro-seuil fragile.
        assert!(
            bytes.len() < 20 * 200,
            "snapshot de {} octets pour 20 entités dépasse largement le budget documenté \
             (< 200 octets/entité) : {bytes:?}",
            bytes.len()
        );
    }

    #[test]
    fn valid_join_fields_accepts_a_normal_join() {
        assert!(valid_join_fields("Alice", "salon1", Some("uid_abc-123")).is_ok());
        // `lobby` vide est légitime (repli sur DEFAULT_LOBBY).
        assert!(valid_join_fields("Alice", "", None).is_ok());
    }

    #[test]
    fn valid_join_fields_rejects_an_empty_or_oversized_name() {
        assert!(valid_join_fields("", "salon1", None).is_err());
        assert!(valid_join_fields("   ", "salon1", None).is_err());
        let too_long = "x".repeat(MAX_NAME_LEN + 1);
        assert!(valid_join_fields(&too_long, "salon1", None).is_err());
        // Pile à la limite : accepté.
        let exact = "x".repeat(MAX_NAME_LEN);
        assert!(valid_join_fields(&exact, "salon1", None).is_ok());
    }

    #[test]
    fn valid_join_fields_rejects_an_oversized_or_unsafe_lobby() {
        let too_long = "x".repeat(MAX_LOBBY_LEN + 1);
        assert!(valid_join_fields("Alice", &too_long, None).is_err());
        for bad in ["a/b", "a b", "a?b", "a#b", "../evil", "café"] {
            assert!(
                valid_join_fields("Alice", bad, None).is_err(),
                "« {bad} » ne devrait pas être un code de salon valide"
            );
        }
    }

    #[test]
    fn valid_join_fields_rejects_an_oversized_or_unsafe_firebase_uid() {
        let too_long = "x".repeat(MAX_FIREBASE_UID_LEN + 1);
        assert!(valid_join_fields("Alice", "salon1", Some(&too_long)).is_err());
        for bad in ["a/b", "a b", "a?b#c", "uid=1"] {
            assert!(
                valid_join_fields("Alice", "salon1", Some(bad)).is_err(),
                "« {bad} » ne devrait pas être un uid Firebase valide"
            );
        }
    }
}
