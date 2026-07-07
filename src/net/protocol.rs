//! Messages échangés entre client et serveur (SPRINT_MMORPG.md, Sprint 52).
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

/// Message envoyé par un client au serveur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Première trame envoyée à la connexion : demande à rejoindre le salon.
    Join {
        name: String,
        /// `uid` Firebase (cf. `net::firebase::AuthSession`), si le joueur s'est
        /// connecté avant de rejoindre (Sprint 56/57) — sert au serveur pour
        /// créditer la progression de fin de manche au bon compte. `None` pour
        /// une partie locale/anonyme (pas de régression : identique à l'absence
        /// de compte).
        firebase_uid: Option<String>,
    },
    /// État des contrôles pour le tick courant (cf. `app::PlayerInput`, en plus
    /// compact — un client réseau ne pilote qu'un joueur, pas un overlay tactile
    /// complet). Envoyé à chaque tick client, même sans changement (le serveur ne
    /// mémorise pas d'état d'input entre deux messages).
    Input {
        move_x: f32,
        move_y: f32,
        attack: bool,
        jump: bool,
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
    /// Un joueur a quitté (volontairement ou par timeout, cf. Sprint 60).
    PlayerLeft { player_id: PlayerId },
    /// Delta d'état du monde depuis le dernier snapshot envoyé à ce client.
    Snapshot(Snapshot),
    /// Évènement ponctuel (pas un état continu) : manche suivante, victoire, défaite...
    Event(GameEvent),
}

/// Delta d'état du monde pour un tick donné : uniquement les entités dont l'état a
/// changé depuis le dernier snapshot envoyé à *ce* client (cf. §2 SPRINT_MMORPG.md,
/// Sprint 52) — pas l'état complet de la scène à chaque tick, pour limiter la bande
/// passante à N joueurs (Sprint 61 mesure et documente le coût réel).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// Numéro de tick serveur (monotone), pour que le client ignore un snapshot
    /// reçu hors ordre (réseau) plutôt que de reculer dans le temps.
    pub tick: u32,
    pub entities: Vec<EntityDelta>,
}

/// État minimal d'une entité de `scene.objects`, suffisant pour l'affichage/
/// l'interpolation côté client (Sprint 54) — pas la représentation complète de
/// `SceneObject` (mesh, script, composants...), qui reste une donnée de scène
/// locale au client, chargée une fois au join, pas retransmise à chaque tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
    /// `Some` uniquement pour les entités qui portent une vie (cf. `Combat::hp`) ;
    /// absent (donc `None`, pas sérialisé à 0 par défaut) pour un décor.
    pub health: Option<f32>,
}

/// Évènement ponctuel de gameplay, distinct d'un `Snapshot` (état continu) : le
/// client peut y réagir une fois (son, flash HUD) sans avoir à comparer deux états.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GameEvent {
    WaveStart { wave: u32 },
    Defeated { index: u32 },
    Win,
    Lose,
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
/// (Sprint 53 : WebSocket, une trame binaire par message).
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
            name: "Loïc".to_string(),
            firebase_uid: None,
        });
        round_trip(ClientMsg::Join {
            name: "Loïc".to_string(),
            firebase_uid: Some("uid-1234".to_string()),
        });
    }

    #[test]
    fn client_msg_input_round_trips() {
        round_trip(ClientMsg::Input {
            move_x: -0.7,
            move_y: 1.0,
            attack: true,
            jump: false,
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
            entities: vec![
                EntityDelta {
                    index: 0,
                    player_id: Some(1),
                    position: [1.0, 0.0, -2.5],
                    yaw: 0.78,
                    visible: true,
                    health: Some(0.75),
                },
                EntityDelta {
                    index: 7,
                    player_id: None,
                    position: [0.0, 0.0, 0.0],
                    yaw: 0.0,
                    visible: false,
                    health: None,
                },
            ],
        }));
    }

    #[test]
    fn server_msg_event_round_trips() {
        round_trip(ServerMsg::Event(GameEvent::WaveStart { wave: 2 }));
        round_trip(ServerMsg::Event(GameEvent::Defeated { index: 5 }));
        round_trip(ServerMsg::Event(GameEvent::Win));
        round_trip(ServerMsg::Event(GameEvent::Lose));
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
    /// pour documenter le coût réseau (objectif SPRINT_MMORPG.md : < 200 octets/joueur/
    /// tick). Pas une assertion stricte de taille exacte (le format peut évoluer) mais
    /// une garde-fou générale contre une régression grossière (ex. passer en JSON par
    /// erreur, ou dupliquer les données par inadvertance).
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
            })
            .collect();
        let snapshot = ServerMsg::Snapshot(Snapshot {
            tick: 999_999,
            entities,
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
}
