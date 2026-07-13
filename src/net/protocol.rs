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
        /// Orientation du personnage (yaw, radians) telle que le client la
        /// prédit/affiche localement. Le serveur l'applique telle quelle (après
        /// nettoyage `NaN`/infini) : l'orientation n'a pas d'enjeu anti-triche
        /// (le collider est une capsule symétrique) mais elle décide de la
        /// direction du **tir** — sans elle, le serveur ne faisait jamais
        /// pivoter les joueurs réseau (le bloc d'orientation de `sim_step` est
        /// réservé au joueur local) : fantômes figés vers -Z et boule de feu
        /// partant dans l'orientation de spawn, pas là où le joueur regarde
        /// (audit du 2026-07-13, Sprint 79).
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
    /// Un joueur a quitté (volontairement ou par timeout, cf. Sprint 60).
    PlayerLeft { player_id: PlayerId },
    /// Delta d'état du monde depuis le dernier snapshot envoyé à ce client.
    Snapshot(Snapshot),
    /// Évènement ponctuel (pas un état continu) : manche suivante, victoire, défaite...
    Event(GameEvent),
}

/// État des joueurs réseau pour un tick donné : **pas** un delta par client
/// malgré le nom — `AppState::network_snapshot` (`app/multiplayer.rs`)
/// diffuse l'état complet de tous les joueurs réseau à chaque tick, identique
/// pour tous les clients (`NetServer::broadcast`, pas de `send_to`
/// individualisé). Suffisant à l'échelle visée (mesuré au Sprint 61 :
/// ~368 octets pour 16 joueurs, ~30x sous un budget large de 200 Ko/s/joueur)
/// — pas l'état complet de *la scène* non plus, seulement les entités pilotées
/// par un joueur réseau (les monstres/décor ne sont pas encore diffusés, cf.
/// `network_snapshot`). Un vrai delta par client (mémoriser le dernier état
/// envoyé à chaque `PlayerId`) resterait à faire si le nombre d'entités
/// diffusées grandissait significativement (cf. `AUDIT_LATENCE_MULTIJOUEUR.md`
/// §2.1, `SPRINTNETWORK.md` Sprint 70) — pas justifié aujourd'hui.
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
}

/// Projectile en vol pour un tick donné : position + arme d'origine (l'aspect —
/// couleur, taille — dépend de l'arme, cf. `app::fireball::RANGED_WEAPONS`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectileState {
    pub position: [f32; 3],
    pub weapon: u8,
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
    /// `Some` uniquement pour les entités qui portent une vie : les joueurs
    /// réseau (0..1, cf. `app::health`, GAMEDESIGN_EN_LIGNE.md §3.1 — vie
    /// individualisée par joueur, plus le champ scalaire unique d'avant) et les
    /// monstres synchronisés (`Combat::hp`, non normalisé). Absent (`None`,
    /// pas sérialisé à 0 par défaut) pour un décor sans vie.
    pub health: Option<f32>,
}

/// Évènement ponctuel de gameplay, distinct d'un `Snapshot` (état continu) : le
/// client peut y réagir une fois (son, flash HUD) sans avoir à comparer deux états.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GameEvent {
    WaveStart {
        wave: u32,
    },
    Defeated {
        index: u32,
    },
    Win,
    Lose,
    /// Un joueur réseau vient de tomber à 0 PV (cf. `app::health`,
    /// GAMEDESIGN_EN_LIGNE.md §3.1) : devient spectateur pour le reste de la
    /// manche (objet masqué, entrées ignorées côté serveur). Diffusé une fois,
    /// pas un état continu — chaque client y réagit une fois (son) plutôt que de
    /// comparer deux snapshots.
    PlayerDown {
        player_id: PlayerId,
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
        round_trip(ServerMsg::Event(GameEvent::PlayerDown { player_id: 7 }));
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
}
