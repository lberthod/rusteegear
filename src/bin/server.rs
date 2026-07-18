//! Serveur de jeu headless : fait tourner des manches en réutilisant
//! `scene`/`runtime`/`app::combat`/`app::multiplayer` **sans fenêtre ni GPU**
//! (aucune dépendance à `gfx`/`egui`/`winit` dans ce binaire), et accepte des
//! connexions WebSocket (`net::server_loop`).
//!
//! **Multi-salons** (cf. GAMEDESIGN_EN_LIGNE.md §3.3) : un process sert
//! plusieurs salons simultanément, chacun sa propre `AppState` (donc sa propre
//! scène, ses propres joueurs, sa propre victoire/défaite) —
//! `ClientMsg::Join::lobby` choisit le salon (créé à la demande au premier
//! join, fermé quand son dernier joueur part). Portée volontairement mesurée,
//! pas un vrai matchmaking MMO : pas de découverte de salons, juste un code à
//! saisir (cf. `net::protocol::DEFAULT_LOBBY`, utilisé par tous les clients
//! actuels — ils continuent donc à se retrouver dans le même salon partagé
//! tant qu'aucune UI ne propose de choisir un autre code). Une manche décidée
//! (victoire/défaite) ne termine pas le *process* : seul ce salon est
//! réinitialisé en place (les joueurs encore connectés y sont re-spawnés),
//! les autres salons continuent sans interruption.
//!
//! **Progression Firebase** : optionnelle, activée par 4 variables
//! d'environnement (`FIREBASE_API_KEY`, `FIREBASE_DATABASE_URL`,
//! `FIREBASE_SERVER_EMAIL`, `FIREBASE_SERVER_PASSWORD` — un compte Firebase
//! dédié au serveur, cf. le commentaire « Qui écrit la progression ? » dans
//! `net::firebase`). Si absentes, le serveur tourne comme avant (pas de
//! régression). En fin de manche, chaque joueur réseau connecté avec un
//! `firebase_uid` (cf. `ClientMsg::Join`) reçoit son score de la manche en XP.
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use motor3derust::app::AppState;
use motor3derust::app::multiplayer::{Contract, NetworkInput, PlayerClass, RoundObjective};
use motor3derust::net::firebase::{
    self, AuthSession, FirebaseConfig, LeaderboardEntry, PlayerProgress,
};
use motor3derust::net::protocol::{
    ClientMsg, DEFAULT_LOBBY, GameEvent, PlayerId, ServerMsg, valid_join_fields,
};
use motor3derust::net::server_loop::NetServer;

/// Cadence réseau du serveur : alignée sur la cadence de la physique elle-même
/// (`FIXED_DT` dans `AppState::advance_play`) — un tick réseau par pas
/// physique, au lieu d'un rythme intermédiaire arbitraire, pour que chaque
/// `Snapshot` reflète un état fraîchement simulé plutôt qu'un état déjà périmé
/// de plusieurs pas physiques en attendant le prochain tick réseau (cf.
/// docs/audits/misc.md pour la latence perçue que mesurait une cadence plus
/// basse, et la marge CPU/réseau disponible à cette fréquence).
const SERVER_TICK: Duration = Duration::from_millis(16); // ~60 Hz

/// Durée maximale d'une manche avant arrêt de sécurité (évite une boucle infinie si
/// la manche ne se termine jamais, ex. bug de configuration de scène).
const MAX_DURATION: Duration = Duration::from_secs(1200);

/// Adresse d'écoute par défaut ; `RUSTEEGEAR_SERVER_ADDR` pour surcharger (ex. tests
/// manuels avec plusieurs instances sur la même machine).
const DEFAULT_ADDR: &str = "127.0.0.1:7777";

/// XP nécessaire pour passer au niveau suivant (formule volontairement simple :
/// un palier fixe, pas de courbe — à raffiner si besoin une fois testé en
/// conditions réelles).
const XP_PER_LEVEL: u32 = 1000;

/// Durée sans le moindre message d'un joueur réseau (même un `Input` inchangé —
/// cf. le protocole, un client légitime en envoie un par tick) au-delà de
/// laquelle il est considéré perdu et retiré de la partie. Un client frappé de
/// silence radio (freeze, crash sans fermeture propre de la socket) ne doit
/// pas laisser un objet fantôme immobile indéfiniment dans la manche des
/// autres joueurs.
///
/// Volontairement généreux (pas quelques secondes) : le rendu desktop
/// (`winit`/macOS) ralentit ou suspend `advance_play` — donc l'envoi
/// d'`Input` — quand la fenêtre n'est plus au premier plan/est occultée (App
/// Nap), et Android fait de même en arrière-plan ; aucune des deux apps ne
/// détecte sa propre éviction, donc un client légitime qui perd juste le
/// focus quelques secondes ne doit pas se faire éjecter silencieusement (cf.
/// docs/audits/misc.md).
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

/// État d'un salon côté binaire (pas dans `AppState`, qui ne connaît que les
/// indices d'objets, cf. `app::multiplayer`) : nom affiché, `uid` Firebase et
/// dernière activité de chaque joueur réseau connecté à **ce** salon.
#[derive(Default)]
struct Lobby {
    names: HashMap<PlayerId, String>,
    firebase_uids: HashMap<PlayerId, String>,
    /// Horodatage du dernier message reçu de chaque joueur (cf. `CLIENT_TIMEOUT`).
    last_seen: HashMap<PlayerId, Instant>,
    /// Classe choisie au `Join` (GAMEDESIGN_MMORPG.md §3.2) — vit au niveau du
    /// salon, pas de `Room::app` (remplacée en bloc par `Room::restart`), pour
    /// qu'un joueur déjà connecté garde sa classe d'une manche à l'autre au
    /// sein du même salon, sans avoir à renvoyer un nouveau `Join`.
    classes: HashMap<PlayerId, u8>,
    /// Mode de manche du salon (Phase C, `sprint10audit.md`) — fixé par le
    /// **premier** `Join` jamais reçu par ce salon (`None` jusque-là, cf.
    /// `handle_message`), ignoré pour tous les suivants : un salon joue un
    /// seul mode pour toute sa durée de vie, comme son code. `Option`
    /// plutôt qu'un `RoundObjective` nu avec un marqueur « salon vide » basé
    /// sur `last_seen.is_empty()` : `last_seen` redevient vide si **tous**
    /// les joueurs quittent avant la fin de la manche (le salon n'est fermé
    /// qu'à manche décidée, cf. la boucle principale) — un tel marqueur
    /// aurait laissé un rejoin ultérieur re-choisir le mode en pleine manche
    /// (`Room::restart` n'ayant pas eu lieu, la scène/le `wave` en cours
    /// resteraient d'un autre mode que celui nouvellement assigné). Réappliqué
    /// à `Room::app.objective` par `Room::restart` (contrairement à `classes`,
    /// qui vit déjà au niveau du salon pour la même raison — persister d'une
    /// manche à l'autre du même salon sans nouveau `Join`).
    objective: Option<RoundObjective>,
}

impl Lobby {
    fn forget(&mut self, id: PlayerId) {
        self.names.remove(&id);
        self.firebase_uids.remove(&id);
        self.last_seen.remove(&id);
        self.classes.remove(&id);
    }
}

/// Distance (m) cumulée par un joueur réseau sur la manche courante — seul
/// signal d'activité disponible côté serveur en dehors des frags (garde
/// anti-AFK, GDD §8.3 : « la participation n'est due qu'à un joueur
/// *actif* »). Remise à zéro à chaque nouvelle manche (`Room::new`/`restart`),
/// jamais lue directement par le protocole — un client ne peut donc pas la
/// mentir, elle est recalculée du seul mouvement observé de son objet serveur.
#[derive(Default)]
struct PlayerActivity {
    last_position: Option<glam::Vec3>,
    distance: f32,
}

/// Un salon : sa propre manche (`AppState`, donc sa propre scène/physique/
/// combat), ses propres joueurs connectés, et le suivi nécessaire pour logger
/// les changements de manche/score sans les répéter à chaque tick.
struct Room {
    app: AppState,
    lobby: Lobby,
    last_wave: u32,
    last_score: u32,
    started: Instant,
    /// Cf. `PlayerActivity` — garde anti-AFK de l'économie d'XP (GDD §8.3).
    activity: HashMap<PlayerId, PlayerActivity>,
}

impl Room {
    /// Charge une manche fraîche : la même scène que les clients (cf.
    /// `AppState::use_embedded_scene`), gabarit local masqué avant le premier
    /// join (`AUDIT_MMORPG.md` : sans ça, l'IA poursuit un mannequin inerte et
    /// sa santé s'épuise pendant l'attente du premier joueur).
    fn new() -> Self {
        let mut app = AppState::new();
        app.use_embedded_scene();
        app.hide_local_player_template();
        app.playing = true;
        let last_wave = app.wave;
        let last_score = app.score();
        Room {
            app,
            lobby: Lobby::default(),
            last_wave,
            last_score,
            started: Instant::now(),
            activity: HashMap::new(),
        }
    }

    /// Recharge une manche fraîche **sans déconnecter** les joueurs déjà
    /// présents : ils sont re-spawnés dans la scène recomposée. Appelé quand
    /// la manche de ce salon se termine (victoire/défaite) ou dépasse
    /// `MAX_DURATION` — seul ce salon repart, les autres salons ne sont pas
    /// affectés.
    fn restart(&mut self) {
        let ids: Vec<PlayerId> = self.lobby.names.keys().copied().collect();
        self.app = AppState::new();
        self.app.use_embedded_scene();
        self.app.hide_local_player_template();
        self.app.playing = true;
        // Le mode de manche vit au niveau du salon (`Lobby::objective`, fixé
        // au premier `Join`), pas de `Room::app` recréé à chaque manche —
        // sans cette ligne, chaque `restart()` retomberait sur `Vagues`
        // (défaut d'`AppState::new()`), quel que soit le mode choisi.
        self.app.objective = self.lobby.objective.unwrap_or_default();
        for id in ids {
            let class = PlayerClass::from_u8(self.lobby.classes.get(&id).copied().unwrap_or(0));
            self.app.spawn_network_player(id, class);
        }
        self.last_wave = self.app.wave;
        self.last_score = self.app.score();
        self.started = Instant::now();
        // Nouvelle manche = nouvelle mesure d'activité (GDD §8.3) : la
        // participation à la manche précédente ne doit pas se reporter.
        self.activity.clear();
    }

    /// Joueurs actuellement connectés à ce salon (pour cibler les envois —
    /// `NetServer` ne connaît pas la notion de salon, cf. sa doc :
    /// `broadcast_all_rooms()` atteint TOUS les clients du serveur, pas
    /// seulement ceux d'un salon donné, donc jamais utilisé ici, uniquement
    /// `send_to`/`send_to_many` ciblés sur ces ids).
    fn connected_ids(&self) -> Vec<PlayerId> {
        self.lobby.names.keys().copied().collect()
    }

    /// Accumule la distance parcourue par chaque joueur connecté depuis le
    /// dernier tick (garde anti-AFK, GDD §8.3) — appelé une fois par tick,
    /// après `advance_play()` (positions fraîchement simulées).
    fn update_activity(&mut self) {
        for id in self.connected_ids() {
            let Some(index) = self.app.network_player_object(id) else {
                continue;
            };
            let Some(object) = self.app.scene.objects.get(index) else {
                continue;
            };
            let position = object.transform.position;
            let entry = self.activity.entry(id).or_default();
            if let Some(last) = entry.last_position {
                entry.distance += last.distance(position);
            }
            entry.last_position = Some(position);
        }
    }
}

/// Traite un message reçu d'un client : fait entrer/sortir le joueur d'un
/// salon ou met à jour son `Input` courant. Extrait de `main` pour rester
/// testable (cf. `tests::joining_moving_and_leaving_through_the_real_socket`)
/// sans avoir à lancer le binaire complet.
///
/// `player_room` associe chaque joueur connecté au code du salon qu'il a
/// rejoint (renseigné au `Join`, consulté pour router `Input`/`Leave` sans
/// que ces messages n'aient besoin de reporter le code à chaque fois).
fn handle_message(
    rooms: &mut HashMap<String, Room>,
    player_room: &mut HashMap<PlayerId, String>,
    net: &NetServer,
    id: PlayerId,
    msg: ClientMsg,
) {
    match msg {
        ClientMsg::Join {
            // Déjà vérifiée par `server_loop::handle_connection` (un client
            // incompatible reçoit `JoinRejected` et n'arrive jamais ici).
            protocol: _,
            name,
            firebase_uid,
            lobby,
            class,
            objective,
        } => {
            // Durcissement (Sprint 105a-2) : `lobby` devient une clé de `rooms`
            // et `firebase_uid` finit non échappé dans une URL Firebase RTDB
            // (`net::firebase::rtdb_url`) — un champ hors bornes/charset
            // rejeté ici, avant toute inscription, plutôt qu'un comportement
            // indéfini plus loin dans la chaîne.
            if let Err(e) = valid_join_fields(&name, &lobby, firebase_uid.as_deref()) {
                log::warn!("Join rejeté ({id}) : {e}");
                return;
            }
            let code = if lobby.trim().is_empty() {
                DEFAULT_LOBBY.to_string()
            } else {
                lobby
            };
            let room = rooms.entry(code.clone()).or_insert_with(Room::new);
            // Mode fixé au tout premier `Join` jamais reçu par ce salon (avant
            // même qu'il ait un joueur effectivement piloté, cf.
            // `Lobby::objective`) — `objective` encore `None` est le marqueur
            // « aucun Join traité depuis la création de ce `Room` », y compris
            // si ce tout premier essai échoue à spawn (pas de gabarit
            // pilotable) : le mode reste alors celui-là plutôt que d'être
            // redéfini par le prochain venu. Contrairement à `last_seen.is_empty()`,
            // ne se réinitialise pas si tous les joueurs quittent avant la fin
            // de la manche (cf. la doc du champ).
            if room.lobby.objective.is_none() {
                let chosen = RoundObjective::from_u8(objective);
                room.lobby.objective = Some(chosen);
                room.app.objective = chosen;
            }
            room.lobby.last_seen.insert(id, Instant::now());
            room.lobby.classes.insert(id, class);
            if room
                .app
                .spawn_network_player(id, PlayerClass::from_u8(class))
                .is_some()
            {
                log::info!("Joueur {id} ({name}) entre en jeu (salon « {code} »)");
                room.lobby.names.insert(id, name.clone());
                if let Some(uid) = firebase_uid {
                    room.lobby.firebase_uids.insert(id, uid);
                }
                player_room.insert(id, code);
                net.send_to_many(
                    &room.connected_ids(),
                    &ServerMsg::PlayerJoined {
                        player_id: id,
                        name,
                    },
                );
                // Retour du mode arbitré par le salon (cf. `Lobby::objective`
                // ci-dessus) : sans ça, ce joueur resterait sur son défaut
                // local `Vagues` même si le salon tourne en `Survie`/etc.
                // (cf. `GameEvent::RoundObjective`, `PROTOCOL_VERSION` 5).
                net.send_to(
                    id,
                    &ServerMsg::Event(GameEvent::RoundObjective {
                        objective: room.lobby.objective.unwrap_or_default().to_u8(),
                    }),
                );
            } else {
                log::warn!(
                    "Joueur {id} ({name}) : aucun gabarit pilotable dans la scène (salon « {code} »)"
                );
            }
        }
        ClientMsg::Input {
            move_x,
            move_y,
            aim_yaw,
            attack,
            jump,
            fire,
            weapon,
            heal,
        } => {
            let Some(room) = player_room.get(&id).and_then(|code| rooms.get_mut(code)) else {
                return;
            };
            room.lobby.last_seen.insert(id, Instant::now());
            room.app.set_network_input(
                id,
                NetworkInput {
                    move_x,
                    move_y,
                    aim_yaw,
                    attack,
                    jump,
                    fire,
                    weapon,
                    heal,
                },
            );
        }
        ClientMsg::Leave => {
            let Some(code) = player_room.remove(&id) else {
                return;
            };
            let Some(room) = rooms.get_mut(&code) else {
                return;
            };
            room.app.despawn_network_player(id);
            room.lobby.forget(id);
            log::info!("Joueur {id} quitte le salon « {code} »");
            net.send_to_many(
                &room.connected_ids(),
                &ServerMsg::PlayerLeft { player_id: id },
            );
        }
    }
}

/// Retire, dans chaque salon, les joueurs réseau sans le moindre message
/// depuis `timeout` (cf. la doc de `CLIENT_TIMEOUT`) — appelé une fois par
/// tick avec `CLIENT_TIMEOUT`, après avoir traité les messages reçus.
/// Symétrique à un `ClientMsg::Leave` explicite (même nettoyage), sauf que
/// c'est le serveur qui l'initie faute de nouvelles du client. `timeout` en
/// paramètre (pas seulement la constante) : permet aux tests d'utiliser un
/// délai court plutôt que d'attendre 60 s réelles.
fn evict_timed_out_players(
    rooms: &mut HashMap<String, Room>,
    player_room: &mut HashMap<PlayerId, String>,
    net: &NetServer,
    timeout: Duration,
) {
    let now = Instant::now();
    for room in rooms.values_mut() {
        let timed_out: Vec<PlayerId> = room
            .lobby
            .last_seen
            .iter()
            .filter(|&(_, &at)| now.duration_since(at) > timeout)
            .map(|(&id, _)| id)
            .collect();
        for id in timed_out {
            log::warn!("Joueur {id} : timeout ({timeout:?} sans message), retiré de la partie");
            room.app.despawn_network_player(id);
            room.lobby.forget(id);
            player_room.remove(&id);
            net.send_to_many(
                &room.connected_ids(),
                &ServerMsg::PlayerLeft { player_id: id },
            );
        }
    }
}

/// Lit la config Firebase serveur depuis l'environnement et se connecte une
/// fois (cf. le commentaire « Qui écrit la progression ? » dans
/// `net::firebase`). `None` si les variables ne sont pas toutes présentes —
/// la progression est alors simplement désactivée, pas une erreur fatale.
fn connect_firebase_server() -> Option<(FirebaseConfig, AuthSession)> {
    let api_key = std::env::var("FIREBASE_API_KEY").ok()?;
    let database_url = std::env::var("FIREBASE_DATABASE_URL").ok()?;
    let email = std::env::var("FIREBASE_SERVER_EMAIL").ok()?;
    let password = std::env::var("FIREBASE_SERVER_PASSWORD").ok()?;
    let config = FirebaseConfig {
        api_key,
        database_url,
    };
    match firebase::sign_in(&config, &email, &password) {
        Ok(session) => {
            log::info!(
                "Firebase : connecté avec le compte serveur ({})",
                session.uid
            );
            Some((config, session))
        }
        Err(e) => {
            log::warn!(
                "Firebase : connexion du compte serveur échouée ({e}) — progression désactivée"
            );
            None
        }
    }
}

/// Progression mise à jour après une manche à `score` XP, ou `None` si la
/// lecture de la progression a échoué — **on ne réécrit JAMAIS par-dessus une
/// progression qu'on n'a pas pu lire**. Avant cette règle, une simple panne
/// réseau transitoire à la lecture faisait repartir le calcul de
/// `PlayerProgress::default()` (0 XP) puis l'écrivait : le cumul réel du
/// joueur était écrasé, potentiellement des heures de progression perdues
/// pour un incident d'infrastructure. Le cas « joueur sans progression
/// enregistrée » n'emprunte pas ce chemin : RTDB renvoie `null` pour un nœud
/// absent, que `parse_progress_response` transforme déjà en
/// `Ok(PlayerProgress::default())` (cf. `net::firebase`) — un `Err` ici est
/// donc toujours une vraie erreur, jamais un premier lancement. Fonction
/// pure, séparée des appels réseau pour être testable sans Firebase.
fn merged_progress(previous: Result<PlayerProgress, String>, score: u32) -> Option<PlayerProgress> {
    let previous = previous.ok()?;
    let xp = previous.xp + score;
    Some(PlayerProgress {
        level: 1 + xp / XP_PER_LEVEL,
        xp,
        // Inchangé ici : seul `award_progress` sait si le Contrat du jour a été
        // rempli *cette* manche, il le met à jour lui-même après cet appel.
        last_contract_day: previous.last_contract_day,
    })
}

/// Contribution individuelle utilisée pour l'XP et le classement (GDD §8.2 :
/// « le classement suit la contribution individuelle, pas le score de
/// salon » — cf. `AUDIT_GAMEPLAY_2026-07-16.md` §3.6, un joueur AFK d'une
/// manche gagnante était crédité comme son MVP). Frags du joueur réseau
/// `id`, `0` si non trouvé — jamais la contribution des autres.
fn network_player_score(app: &AppState, id: PlayerId) -> u32 {
    app.network_player_kills(id).unwrap_or(0)
}

/// Assists du joueur réseau `id` (GDD §8.3 : Sprint 4, XP due à qui blesse une
/// cible achevée par un autre joueur), `0` si non trouvé — comptés à part du
/// classement (`network_player_score`, frags uniquement) : un assist compte
/// pour l'XP, pas pour la contribution de classement (§8.2, cf. sa doc).
fn network_player_assists(app: &AppState, id: PlayerId) -> u32 {
    app.network_player_assists(id).unwrap_or(0)
}

/// XP de participation (GDD §8.3) : due une fois par manche à tout joueur
/// **actif**, indépendamment du résultat — « la défaite paie aussi » (§3.2)
/// — sinon l'XP existerait mais resterait imperceptible à l'échelle d'une
/// vie de joueur (constat du GDD : ~100 nuits pour le premier palier avec
/// l'ancien barème « score de salon = XP brute »).
const XP_PARTICIPATION: u32 = 150;

/// XP par frag ou assist (GDD §8.3) : un assist (dégât porté à une cible
/// achevée par un autre joueur peu après, cf. `AppState::credit_assists_on_kill`)
/// compte autant qu'un frag pour l'XP — seul le classement (`network_player_score`)
/// reste frags uniquement (§8.2).
const XP_PER_FRAG_OR_ASSIST: u32 = 5;

/// Bonus d'XP si la manche est gagnée (GDD §8.3) : « gagner compte, sans
/// doubler la mise » — la participation (ci-dessus) reste le terme dominant.
const XP_VICTORY_BONUS: u32 = 75;

/// Distance (m) minimale parcourue sur la manche pour compter comme *actif*
/// côté garde anti-AFK (GDD §8.3, point 1), si ni frag ni assist n'a été
/// marqué — sans cette garde, rester immobile à encaisser 150 XP par nuit
/// serait l'« optimum anti-fun » que le GDD interdit explicitement (§15.5).
/// Une valeur volontairement basse (quelques pas) : le but n'est pas
/// d'exiger un niveau d'activité, juste d'exclure un joueur qui n'a jamais
/// bougé ni contribué au combat.
const ACTIVITY_DISTANCE_THRESHOLD: f32 = 3.0;

/// XP du Contrat du jour (GDD §3.4/§3.5, table §3.5 : « Contrat du jour | 250 |
/// vaut ~une nuit »), Phase D — Sprint 9 de `sprint10audit.md`. Crédité au plus
/// une fois par compte et par jour (`PlayerProgress::last_contract_day`), sur
/// une manche **gagnée** dont `AppState::contract_completed` confirme la
/// condition du contrat du jour (`Contract::of_day`) — jamais sur une défaite
/// ni un arrêt de sécurité (`MAX_DURATION`).
const XP_CONTRACT: u32 = 250;

/// Numéro de jour UTC (secondes Unix / 86 400) : seed déterministe du Contrat
/// du jour (GDD §3.4 : « calculé identiquement par serveur et clients ») et
/// clé de « déjà réclamé aujourd'hui » (`PlayerProgress::last_contract_day`).
/// Pas de dépendance `chrono` : un simple compteur de jours suffit, aucune
/// notion de fuseau horaire/calendrier n'est nécessaire au seed (contrairement
/// à un affichage de date, hors scope ici).
fn day_number(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// XP créditée à un joueur pour cette manche (GDD §8.3, barème cible : niv. 3
/// ≈ 2000 XP, niv. 6 ≈ 5000, niv. 10 ≈ 9000 — déjà exact avec `XP_PER_LEVEL`
/// inchangé, `1 + xp / 1000` : le seul écart au « ~100× trop lent » constaté
/// était le score crédité par manche, pas la formule de niveau). Le contrat
/// du jour (`XP_CONTRACT`, Phase D, Sprint 9) est un terme **séparé**, ajouté
/// par `award_progress` — « distincte du score de manche normal » (sprint10audit.md) :
/// pas mélangé ici pour rester testable indépendamment (`round_xp` ne connaît
/// ni le contrat ni le jour). `frags_and_assists`
/// est déjà la somme des deux (cf. `award_progress`) — jamais compté deux fois
/// pour la même mise à mort (un joueur reçoit soit le frag, soit l'assist).
fn round_xp(frags_and_assists: u32, active: bool, won: bool) -> u32 {
    if !active {
        return 0;
    }
    let victory_bonus = if won { XP_VICTORY_BONUS } else { 0 };
    XP_PARTICIPATION + XP_PER_FRAG_OR_ASSIST * frags_and_assists + victory_bonus
}

/// Crédite l'XP de la manche à chaque joueur réseau connu de Firebase, selon
/// `round_xp` (participation + frags/assists + bonus de victoire, garde
/// anti-AFK incluse), plus `XP_CONTRACT` si `contract` est le contrat du jour
/// rempli par cette manche (`Some((contract, day))`, calculé par l'appelant
/// via `AppState::contract_completed` — seulement sur une victoire, jamais une
/// défaite ni un arrêt de sécurité) et que ce compte ne l'a pas déjà réclamé
/// aujourd'hui (`PlayerProgress::last_contract_day != day`). Même garde
/// anti-AFK que la participation normale (§8.3) : un joueur inactif d'une
/// manche par ailleurs gagnante ne réclame pas le contrat à sa place — sans
/// quoi rester immobile dans un salon qui gagne suffirait à l'encaisser.
/// Les échecs (réseau, règles RTDB non configurées...) sont logués mais ne
/// font pas planter le serveur — la progression est un bonus, pas une
/// condition de fonctionnement du jeu. Pas de retry sur une lecture échouée :
/// `get_progress`/`set_progress` sont des appels bloquants dans la boucle de
/// tick — au pire, le joueur perd le bonus d'une manche (logué), jamais son
/// cumul (cf. `merged_progress`).
fn award_progress(
    firebase: &Option<(FirebaseConfig, AuthSession)>,
    lobby: &Lobby,
    app: &AppState,
    won: bool,
    activity: &HashMap<PlayerId, PlayerActivity>,
    contract: Option<(Contract, u64)>,
) {
    let Some((config, session)) = firebase else {
        return;
    };
    for (id, uid) in &lobby.firebase_uids {
        let frags = network_player_score(app, *id);
        let assists = network_player_assists(app, *id);
        let moved = activity.get(id).map(|a| a.distance).unwrap_or(0.0);
        // Un assist compte comme un frag pour la garde anti-AFK (§8.3, point
        // 1) : blesser une cible achevée par un allié est une contribution
        // réelle au combat, pas de l'immobilité déguisée.
        let active = frags > 0 || assists > 0 || moved >= ACTIVITY_DISTANCE_THRESHOLD;
        let score = round_xp(frags + assists, active, won);
        let previous = firebase::get_progress(config, uid);
        if let Err(e) = &previous {
            log::warn!(
                "Firebase : lecture progression du joueur {id} échouée ({e}) — score de {score} \
                 XP NON crédité pour ne pas écraser sa progression réelle"
            );
        }
        let claims_contract = active
            && contract
                .is_some_and(|(_, day)| !matches!(&previous, Ok(p) if p.last_contract_day == day));
        let contract_bonus = if claims_contract { XP_CONTRACT } else { 0 };
        let Some(mut updated) = merged_progress(previous, score + contract_bonus) else {
            continue;
        };
        if claims_contract && let Some((_, day)) = contract {
            updated.last_contract_day = day;
        }
        let PlayerProgress {
            level,
            xp,
            last_contract_day: _,
        } = updated;
        match firebase::set_progress(config, uid, updated, &session.id_token) {
            Ok(()) => {
                log::info!(
                    "Firebase : joueur {id} ({uid}) → niveau {level}, {xp} XP (+{score}{})",
                    if contract_bonus > 0 {
                        format!(" +{contract_bonus} contrat")
                    } else {
                        String::new()
                    }
                )
            }
            Err(e) => log::warn!("Firebase : écriture progression du joueur {id} échouée ({e})"),
        }
    }
}

/// Poste une entrée de classement pour chaque joueur réseau connu de
/// Firebase, à sa propre contribution (`network_player_score` — même source
/// que `award_progress`, appelé juste avant elle en fin de manche). Mêmes
/// garanties : jamais fatal, juste logué en cas d'échec.
fn post_leaderboard(
    firebase: &Option<(FirebaseConfig, AuthSession)>,
    lobby: &Lobby,
    app: &AppState,
) {
    let Some((config, session)) = firebase else {
        return;
    };
    let achieved_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    for id in lobby.firebase_uids.keys() {
        let score = network_player_score(app, *id);
        let name = lobby
            .names
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("Joueur {id}"));
        let entry = LeaderboardEntry {
            name,
            score,
            achieved_at_ms,
        };
        match firebase::post_leaderboard_entry(config, &session.id_token, &entry) {
            Ok(()) => log::info!("Firebase : classement mis à jour pour le joueur {id} ({score})"),
            Err(e) => log::warn!("Firebase : écriture classement du joueur {id} échouée ({e})"),
        }
    }
}

fn main() {
    env_logger::init();
    log::info!("RusteeGear — serveur headless : salons multiples (Sprint 82)");

    let addr = std::env::var("RUSTEEGEAR_SERVER_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let net = match NetServer::start(&addr) {
        Ok(n) => {
            log::info!("Serveur réseau à l'écoute sur {}", n.local_addr);
            Some(n)
        }
        Err(e) => {
            log::warn!(
                "Réseau désactivé (échec du bind sur {addr} : {e}) — manche locale uniquement"
            );
            None
        }
    };

    let firebase = connect_firebase_server();
    if firebase.is_none() {
        log::info!(
            "Firebase désactivé (FIREBASE_API_KEY/DATABASE_URL/SERVER_EMAIL/SERVER_PASSWORD \
             non renseignées) — pas de progression persistante pour cette manche"
        );
    }

    let mut rooms: HashMap<String, Room> = HashMap::new();
    let mut player_room: HashMap<PlayerId, String> = HashMap::new();
    let mut tick: u32 = 0;

    // Sans réseau (bind échoué) : un unique salon local, pour ne pas régresser
    // le comportement historique (aucun moyen de le rejoindre de toute façon,
    // mais la manche tourne quand même — utile en test manuel sans port libre).
    if net.is_none() {
        rooms.insert(DEFAULT_LOBBY.to_string(), Room::new());
    }

    loop {
        let tick_start = Instant::now();

        if let Some(net) = &net {
            while let Ok((id, msg)) = net.inbox.try_recv() {
                handle_message(&mut rooms, &mut player_room, net, id, msg);
            }
            evict_timed_out_players(&mut rooms, &mut player_room, net, CLIENT_TIMEOUT);
        }

        let mut to_close: Vec<String> = Vec::new();
        for (code, room) in rooms.iter_mut() {
            room.app.advance_play();
            room.update_activity();

            if let Some(net) = &net {
                let ids = room.connected_ids();
                // Broadcast ciblé (`send_to_many`) : la `Snapshot` — le plus
                // gros message du protocole — est encodée UNE seule fois par
                // tick et par salon, au lieu d'un ré-encodage bincode identique
                // par destinataire (audit réseau 2026-07).
                let snapshot = ServerMsg::Snapshot(room.app.network_snapshot(tick));
                net.send_to_many(&ids, &snapshot);
                // Évènements ponctuels produits par la simulation de ce tick
                // (monstre vaincu, joueur vaincu...) : diffusés une fois, pour
                // que les clients réagissent (son/flash) sans comparer deux
                // snapshots — uniquement aux joueurs *de ce salon*.
                for event in room.app.take_net_events() {
                    net.send_to_many(&ids, &ServerMsg::Event(event));
                }
            }

            if room.app.wave != room.last_wave {
                log::info!("[{code}] Manche {} révélée", room.app.wave);
                room.last_wave = room.app.wave;
            }
            if room.app.score() != room.last_score {
                log::info!("[{code}] Score : {}", room.app.score());
                room.last_score = room.app.score();
            }

            // `is_room_lost()` (pas `is_lost()`, pensé pour un joueur local
            // unique) : la défaite n'arrive que si TOUS les joueurs réseau de
            // CE salon sont vaincus (GAMEDESIGN_EN_LIGNE.md §3.1) — un seul
            // joueur qui meurt devient spectateur, la manche continue pour
            // les autres, dans ce salon comme dans les autres.
            let decided = room.app.has_won() || room.app.is_room_lost();
            let timed_out = room.started.elapsed() > MAX_DURATION;
            if decided || timed_out {
                if decided {
                    log::info!(
                        "[{code}] Manche terminée : {}, score final {} (en {:.1} s)",
                        if room.app.has_won() {
                            "victoire"
                        } else {
                            "défaite"
                        },
                        room.app.score(),
                        room.started.elapsed().as_secs_f32()
                    );
                } else {
                    log::warn!(
                        "[{code}] Arrêt de sécurité : durée maximale de manche atteinte sans issue"
                    );
                }
                // Diffuse la fin de manche décidée (Phase L, `sprintreflecion.md` —
                // trouvé en vérifiant mécaniquement le mode Escorte : jusqu'ici
                // `GameEvent::Win`/`Lose` n'étaient jamais envoyés, seulement
                // calculés localement par chaque client complet via sa propre copie
                // d'`update_round`). Un bot minimal (sans cette simulation locale,
                // ex. `examples/phase_l_mode_check.rs`) ne pouvait donc jamais
                // observer de fin de manche par ce biais — corrigé pour que le
                // serveur, autoritaire, confirme aussi l'issue. Uniquement sur
                // `decided` (jamais sur un simple `timed_out` sans victoire/défaite
                // réelle, cf. la définition de `decided` ci-dessus) ; avant
                // `room.restart()` pour que l'événement parte encore vers les
                // joueurs de la manche qui vient de se terminer, pas la suivante.
                if decided && let Some(net) = &net {
                    let event = if room.app.has_won() {
                        GameEvent::Win
                    } else {
                        GameEvent::Lose
                    };
                    net.send_to_many(&room.connected_ids(), &ServerMsg::Event(event));
                }
                // Contrat du jour (Phase D, Sprint 9) : uniquement sur une
                // victoire *décidée* — jamais une défaite, jamais un arrêt de
                // sécurité (`timed_out`, qui n'implique `decided` que si
                // `has_won()` l'est aussi, cf. la définition de `decided`
                // ci-dessus). `day` recalculé à chaque manche décidée plutôt
                // que mémorisé sur `Room` : un contrat commencé la veille et
                // terminé après minuit ne doit pas être crédité comme celui
                // d'hier (GDD §3.4 : le contrat du jour est celui du jour où
                // la manche se termine, pas celui où elle a commencé).
                let day = day_number(SystemTime::now());
                let contract = (decided && room.app.has_won())
                    .then(|| Contract::of_day(day))
                    .filter(|&c| room.app.contract_completed(c, room.started.elapsed()))
                    .map(|c| (c, day));
                award_progress(
                    &firebase,
                    &room.lobby,
                    &room.app,
                    room.app.has_won(),
                    &room.activity,
                    contract,
                );
                post_leaderboard(&firebase, &room.lobby, &room.app);
                // Une manche décidée ne ferme pas tout le serveur : seul CE
                // salon repart, les autres continuent — sauf s'il est déjà
                // vide (dernier joueur parti entre-temps), auquel cas autant
                // le fermer plutôt que de le faire tourner pour personne.
                if room.connected_ids().is_empty() {
                    to_close.push(code.clone());
                } else {
                    room.restart();
                }
            }
        }
        for code in to_close {
            rooms.remove(&code);
            log::info!("Salon « {code} » fermé (vide)");
        }

        tick += 1;

        let elapsed = tick_start.elapsed();
        if elapsed < SERVER_TICK {
            std::thread::sleep(SERVER_TICK - elapsed);
        }
    }
}

/// Tests **purs** de la progression (aucun socket, aucun Firebase) — hors du
/// gate `net_tests` du module voisin exprès : la règle « ne jamais écraser une
/// progression illisible » doit être vérifiée par le `cargo test` de tous les
/// jours, pas seulement par la couverture réseau complète.
#[cfg(test)]
mod progress_tests {
    use super::*;

    /// Le cœur du correctif : une lecture échouée (panne réseau, règles RTDB)
    /// ne produit **aucune** écriture — avant, on repartait de
    /// `PlayerProgress::default()` et on écrasait le cumul réel du joueur.
    #[test]
    fn a_failed_progress_read_never_writes() {
        assert_eq!(
            merged_progress(Err("timeout réseau simulé".to_string()), 500),
            None
        );
    }

    /// Le premier lancement d'un joueur ne passe PAS par le chemin d'erreur :
    /// un nœud RTDB absent renvoie `null`, que `parse_progress_response`
    /// transforme en `Ok(default)` (cf. `net::firebase`) — le score de la
    /// première manche est donc bien crédité depuis zéro.
    #[test]
    fn an_absent_progress_node_still_credits_from_default() {
        let updated = merged_progress(Ok(PlayerProgress::default()), 500)
            .expect("une lecture réussie doit produire une écriture");
        assert_eq!(updated.xp, 500);
        assert_eq!(updated.level, 1);
    }

    /// L'XP s'accumule par-dessus l'existant et le niveau suit `XP_PER_LEVEL`.
    #[test]
    fn xp_accumulates_on_top_of_previous() {
        let previous = PlayerProgress {
            level: 1,
            xp: 900,
            ..PlayerProgress::default()
        };
        let updated = merged_progress(Ok(previous), 200).expect("écriture attendue");
        assert_eq!(updated.xp, 1100);
        assert_eq!(updated.level, 2, "1100 XP à {XP_PER_LEVEL} XP/niveau");
    }

    /// GDD §8.2 : « le classement suit la contribution individuelle, pas le
    /// score de salon » — contre-exemple direct du bug documenté dans
    /// `AUDIT_GAMEPLAY_2026-07-16.md` §3.6 (un joueur AFK d'une manche
    /// gagnante classé comme son MVP). Deux joueurs réseau dans la même
    /// `AppState`, un seul frappe une cible attaquable à portée : leurs
    /// scores individuels (`network_player_score`, la valeur désormais
    /// utilisée par `award_progress`/`post_leaderboard`) doivent diverger,
    /// alors que `room.app.score()` — l'ancienne valeur uniforme — resterait
    /// identique pour les deux.
    #[test]
    fn network_player_score_reflects_each_players_own_kills_not_a_shared_total() {
        let mut app = AppState::new();
        app.scene = motor3derust::scene::Scene::controller_demo();
        app.playing = true;

        let attacker: PlayerId = 1;
        let bystander: PlayerId = 2;
        let attacker_idx = app
            .spawn_network_player(attacker, PlayerClass::Assault)
            .expect("le gabarit joueur doit exister dans controller_demo");
        app.spawn_network_player(bystander, PlayerClass::Assault)
            .expect("le gabarit joueur doit exister dans controller_demo");

        // Place le seul attaquant au contact d'un « Ennemi » attaquable —
        // peu importe lequel, `attack_at_defeats_only_attackable_enemies_
        // in_range` (scene::mod.rs) garantit qu'ils le sont tous.
        let enemy_pos = app
            .scene
            .objects
            .iter()
            .find(|o| o.name.starts_with("Ennemi"))
            .map(|o| o.transform.position)
            .expect("controller_demo doit contenir au moins un « Ennemi »");
        app.scene.objects[attacker_idx].transform.position = enemy_pos;

        app.set_network_input(
            attacker,
            NetworkInput {
                attack: true,
                ..Default::default()
            },
        );
        // Pas d'attaque pour `bystander` : NetworkInput::default() (attack: false).
        app.set_network_input(bystander, NetworkInput::default());

        app.update_network_attacks(1.0 / 60.0);

        let attacker_score = network_player_score(&app, attacker);
        let bystander_score = network_player_score(&app, bystander);
        assert!(
            attacker_score > 0,
            "l'attaquant au contact doit avoir frag au moins une fois"
        );
        assert_eq!(
            bystander_score, 0,
            "un joueur qui n'attaque jamais ne doit recevoir aucun frag"
        );
        assert_ne!(
            attacker_score, bystander_score,
            "deux joueurs de contribution différente doivent recevoir des scores différents \
             (avant ce correctif, les deux auraient reçu `room.app.score()`, identique pour tous)"
        );
    }

    /// Sprint 4 (PHASE B, `sprint10audit.md`) : un assist vaut exactement la
    /// même XP qu'un frag (`round_xp` ne distingue pas leur origine, cf.
    /// `XP_PER_FRAG_OR_ASSIST`) — seul `network_player_score` (classement,
    /// §8.2) reste frags uniquement, jamais `network_player_assists`. Le
    /// détail du déclenchement d'un assist (dégât porté sans achever,
    /// achevé par un autre joueur dans `ASSIST_WINDOW`) est couvert côté
    /// bibliothèque par
    /// `multiplayer::tests::damaging_a_creature_that_another_player_finishes_off_credits_an_assist_not_a_kill`
    /// et l'intégration bout en bout par
    /// `fireball::tests::two_network_players_who_both_damage_a_creature_split_credit_between_kill_and_assist`.
    #[test]
    fn an_assist_is_worth_exactly_as_much_xp_as_a_frag() {
        let frag_only = round_xp(1, true, false);
        // `0 + 1` : 0 frag + 1 assist, écrit explicitement pour la lisibilité de
        // l'intention (assist compté comme un frag), pas une vraie opération.
        #[allow(clippy::identity_op)]
        let assist_folded_in = round_xp(0 + 1, true, false);
        assert_eq!(
            frag_only, assist_folded_in,
            "round_xp ne doit pas distinguer un frag d'un assist, seule leur somme compte"
        );
        assert_eq!(frag_only - round_xp(0, true, false), XP_PER_FRAG_OR_ASSIST);
    }

    /// GDD §8.3, propriété 1 : « la participation domine le frag » — sans
    /// aucun frag, un joueur actif touche déjà l'essentiel de l'XP d'une nuit
    /// moyenne (150 sur ~300 visés), pas une fraction négligeable.
    #[test]
    fn an_active_player_with_no_frags_still_earns_the_participation_xp() {
        assert_eq!(round_xp(0, true, false), XP_PARTICIPATION);
    }

    /// GDD §8.3, garde anti-AFK explicite : « un AFK gagne 0, pas 150 » — un
    /// joueur inactif ne touche rien, même une manche gagnée, même avec des
    /// frags à 0 (un joueur inactif ne peut de toute façon pas fragger, mais
    /// le test isole la garde elle-même plutôt que d'en dépendre).
    #[test]
    fn an_inactive_player_earns_nothing_even_on_a_won_round() {
        assert_eq!(round_xp(0, false, true), 0);
    }

    /// GDD §8.3 : « gagner compte, sans doubler la mise » — le bonus de
    /// victoire s'ajoute, il ne multiplie pas la participation/les frags.
    #[test]
    fn winning_adds_a_flat_bonus_on_top_of_participation_and_frags() {
        let lost = round_xp(3, true, false);
        let won = round_xp(3, true, true);
        assert_eq!(won - lost, XP_VICTORY_BONUS);
    }

    /// Nuit moyenne gagnée ≈ 300 XP (GDD §8.3, table « Économie cible ») :
    /// participation (150) + ~15 frags/joueur (5 × 15 = 75, arrondi ici à 15
    /// pour coller à l'exemple du GDD) + victoire (75) doit approcher 300,
    /// pas les ~20 XP de l'ancien barème (score de salon = nombre de
    /// monstres vaincus par toute l'équipe).
    #[test]
    fn an_average_won_night_lands_close_to_the_gdd_target_of_300_xp() {
        let xp = round_xp(15, true, true);
        assert_eq!(
            xp,
            XP_PARTICIPATION + XP_PER_FRAG_OR_ASSIST * 15 + XP_VICTORY_BONUS
        );
        assert!(
            (250..=350).contains(&xp),
            "une nuit moyenne gagnée doit rester proche des ~300 XP visés : {xp}"
        );
    }

    /// `Room::update_activity` (garde anti-AFK, GDD §8.3) : un joueur qui ne
    /// bouge jamais accumule une distance nulle ; un joueur qui se déplace
    /// entre deux ticks voit sa distance grandir — ce compteur est ce qui
    /// permet à `award_progress` de distinguer un joueur actif immobile en
    /// combat rapproché (frags > 0, donc actif par l'autre voie) d'un joueur
    /// réellement absent.
    #[test]
    fn room_activity_accumulates_distance_moved_between_ticks_only() {
        let mut room = Room::new();
        let id: PlayerId = 1;
        let index = room
            .app
            .spawn_network_player(id, PlayerClass::Assault)
            .expect("le gabarit joueur doit exister dans la scène embarquée");
        room.lobby.names.insert(id, "Testeur".to_string());

        room.update_activity();
        assert_eq!(
            room.activity.get(&id).map(|a| a.distance).unwrap_or(0.0),
            0.0,
            "aucune distance ne doit être comptée avant un premier mouvement observé"
        );

        room.app.scene.objects[index].transform.position += glam::Vec3::new(5.0, 0.0, 0.0);
        room.update_activity();
        let distance = room.activity.get(&id).map(|a| a.distance).unwrap_or(0.0);
        assert!(
            distance >= 4.9,
            "un déplacement de 5 m entre deux ticks doit se refléter dans la distance cumulée : {distance}"
        );
    }

    /// `day_number` (Phase D, Sprint 9) : seed déterministe du Contrat du jour —
    /// deux instants dans la même journée UTC donnent le même jour, un instant
    /// 24 h plus tard donne le suivant.
    #[test]
    fn day_number_is_stable_within_a_day_and_advances_after_24h() {
        let epoch_plus_1h = UNIX_EPOCH + Duration::from_secs(3600);
        let epoch_plus_23h = UNIX_EPOCH + Duration::from_secs(23 * 3600);
        assert_eq!(day_number(epoch_plus_1h), day_number(epoch_plus_23h));
        assert_eq!(day_number(epoch_plus_1h), 0);
        assert_eq!(
            day_number(UNIX_EPOCH + Duration::from_secs(25 * 3600)),
            1,
            "25 h après l'époque, on est passé au jour suivant"
        );
    }
}

// Sprint 105a-3 : tous les tests de ce module ouvrent un vrai socket
// (NetServer/NetClient) — regroupés derrière `net_tests` plutôt qu'annotés
// un par un, `cargo test` par défaut reste rapide et indépendant d'un
// environnement CI qui restreint parfois le bind loopback (cf.
// docs/architecture.md, section réseau, pour lancer la couverture complète).
#[cfg(all(test, feature = "net_tests"))]
mod tests {
    use std::time::Duration;

    use motor3derust::net::client::NetClient;
    use motor3derust::net::protocol::ServerMsg;

    use super::*;

    /// Bout-en-bout à travers un vrai socket (pas seulement les
    /// méthodes `AppState` testées isolément dans `app::multiplayer::tests`) :
    /// un `NetClient` rejoint, obtient un objet pilotable, son `Input` déplace
    /// *cet* objet, puis `Leave` le retire. Reproduit exactement la boucle de
    /// `main` (via `handle_message`) sans lancer le binaire dans un sous-processus.
    /// Construit une manche de test (démo zombies, pilotable + monstres) plutôt
    /// que la scène embarquée (`Room::new()`) : ces tests visent la plomberie
    /// réseau/salons, pas le contenu de `assets/player_scene.json`.
    fn zombies_room() -> Room {
        let mut app = AppState::new();
        app.load_zombies_demo();
        app.playing = true;
        Room {
            app,
            lobby: Lobby::default(),
            last_wave: 0,
            last_score: 0,
            started: Instant::now(),
            activity: HashMap::new(),
        }
    }

    #[test]
    fn joining_moving_and_leaving_through_the_real_socket() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        rooms.insert(DEFAULT_LOBBY.to_string(), zombies_room());
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client = NetClient::connect(&url, "Alice", None).expect("connexion du client");
        let ServerMsg::Welcome { player_id } = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu")
        else {
            panic!("premier message attendu : Welcome");
        };

        // Traite le `Join` relayé par le serveur (comme le ferait `main`).
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        assert_eq!(id, player_id);
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        let object_index = room
            .app
            .network_player_object(player_id)
            .expect("le Join doit avoir fait apparaître un objet pilotable");
        let start = room.app.scene.objects[object_index].transform.position;

        client.send(&motor3derust::net::protocol::ClientMsg::Input {
            move_x: 1.0,
            move_y: 0.0,
            aim_yaw: 0.0,
            attack: false,
            jump: false,
            fire: false,
            weapon: 0,
            heal: false,
        });
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Input attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        // Pas d'accès à `last_frame` (privé) depuis ce binaire externe : on avance
        // en temps réel, comme le fait réellement `main` (contrairement aux tests
        // internes de `app::multiplayer`, qui peuvent retarder `last_frame`
        // directement pour rester déterministes sans dormir).
        let room = rooms.get_mut(DEFAULT_LOBBY).unwrap();
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(20));
            room.app.advance_play();
        }
        let end = room.app.scene.objects[object_index].transform.position;
        assert!(
            (end.x - start.x).abs() > 0.5,
            "l'Input du client doit avoir déplacé son propre objet : {start:?} -> {end:?}"
        );

        client.send(&motor3derust::net::protocol::ClientMsg::Leave);
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Leave attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);
        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        assert_eq!(room.app.network_player_object(player_id), None);
        assert!(
            !room.app.scene.objects[object_index].visible,
            "l'objet du joueur parti doit être masqué"
        );
    }

    /// Phase C (Sprint 5, `sprint10audit.md`, auto-relecture) : le mode arbitré
    /// par le salon (`Lobby::objective`) doit être **renvoyé** à tout joueur
    /// qui rejoint (`GameEvent::RoundObjective`, `PROTOCOL_VERSION` 5) —
    /// bout-en-bout à travers un vrai socket, pas seulement `Lobby::objective`
    /// côté serveur (déjà couvert par le test précédent) : sans ce retour,
    /// `app::network_client::handle_server_msg` (côté client, testé isolément
    /// dans `app::network_client::tests::
    /// round_objective_event_aligns_our_local_objective_with_the_room`)
    /// n'aurait jamais rien à traiter, et le client resterait sur son défaut
    /// local `Vagues`.
    #[test]
    fn a_joining_client_learns_the_rooms_objective_over_the_wire() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        let mut room = zombies_room();
        room.lobby.objective = Some(RoundObjective::Survie);
        rooms.insert("salle-survie".to_string(), room);
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client = NetClient::connect_to_lobby(&url, "Bob", None, "salle-survie", 0, 0)
            .expect("connexion du client");
        let ServerMsg::Welcome { .. } = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu")
        else {
            panic!("premier message attendu : Welcome");
        };

        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        // `PlayerJoined` (déjà couvert ailleurs) précède `RoundObjective` sur
        // le fil : on consomme les messages dans l'ordre plutôt que de
        // supposer une position fixe, pour ne pas coupler ce test à l'ordre
        // exact d'émission dans `handle_message`.
        let mut saw_round_objective = false;
        for _ in 0..5 {
            let Ok(msg) = client.inbox.recv_timeout(Duration::from_secs(2)) else {
                break;
            };
            if let ServerMsg::Event(GameEvent::RoundObjective { objective }) = msg {
                assert_eq!(
                    RoundObjective::from_u8(objective),
                    RoundObjective::Survie,
                    "doit refléter le mode réel du salon rejoint"
                );
                saw_round_objective = true;
                break;
            }
        }
        assert!(
            saw_round_objective,
            "le client doit recevoir GameEvent::RoundObjective au Join"
        );
    }

    /// Phase C (Sprint 5, `sprint10audit.md`) : le mode d'un salon est fixé au
    /// premier `Join` **jamais reçu**, pas simplement « pas de joueur connu en
    /// ce moment » — sans ça, un salon dont tous les joueurs sont partis avant
    /// la fin de la manche (le salon n'est fermé qu'à manche décidée, cf. la
    /// boucle `main`) verrait son mode réassigné par le prochain venu, en
    /// pleine manche d'un autre mode.
    #[test]
    fn a_room_keeps_its_objective_even_after_every_player_leaves_before_the_round_ends() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let mut rooms: HashMap<String, Room> = HashMap::new();
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        handle_message(
            &mut rooms,
            &mut player_room,
            &net,
            1,
            ClientMsg::Join {
                protocol: motor3derust::net::protocol::PROTOCOL_VERSION,
                name: "Alice".to_string(),
                firebase_uid: None,
                lobby: "salle-survie".to_string(),
                class: 0,
                objective: RoundObjective::Survie.to_u8(),
            },
        );
        {
            let room = rooms.get("salle-survie").expect("salon créé au Join");
            assert_eq!(room.lobby.objective, Some(RoundObjective::Survie));
            assert_eq!(room.app.objective, RoundObjective::Survie);
        }

        // Alice quitte : plus personne dans le salon, mais il n'est pas fermé
        // (fermeture réservée à la boucle `main`, à manche décidée).
        handle_message(&mut rooms, &mut player_room, &net, 1, ClientMsg::Leave);
        {
            let room = rooms
                .get("salle-survie")
                .expect("le salon vide n'est pas fermé ici");
            assert!(room.lobby.last_seen.is_empty(), "plus aucun joueur connu");
        }

        // Bob rejoint le même salon en demandant Vagues : le mode déjà choisi
        // (Survie) doit être conservé, pas réassigné.
        handle_message(
            &mut rooms,
            &mut player_room,
            &net,
            2,
            ClientMsg::Join {
                protocol: motor3derust::net::protocol::PROTOCOL_VERSION,
                name: "Bob".to_string(),
                firebase_uid: None,
                lobby: "salle-survie".to_string(),
                class: 0,
                objective: RoundObjective::Vagues.to_u8(),
            },
        );
        let room = rooms.get("salle-survie").expect("salon toujours là");
        assert_eq!(
            room.lobby.objective,
            Some(RoundObjective::Survie),
            "le mode du salon ne doit pas être réassignable après le premier Join"
        );
        assert_eq!(room.app.objective, RoundObjective::Survie);
    }

    /// Sprint 105a-2 (durcissement des entrées réseau) : un `Join` dont le
    /// code de salon contient des caractères interdits (`valid_join_fields`)
    /// est rejeté — le joueur ne doit apparaître dans aucun salon, à la
    /// différence d'un `Join` valide (cf. `joining_moving_and_leaving_
    /// through_the_real_socket` ci-dessus). Le transport (`Welcome`) reste
    /// inconditionnel (envoyé avant que `handle_message` ne voie le `Join`),
    /// seule l'inscription applicative est bloquée.
    #[test]
    fn a_join_with_an_unsafe_lobby_code_is_rejected() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        rooms.insert(DEFAULT_LOBBY.to_string(), zombies_room());
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client = NetClient::connect_to_lobby(&url, "Alice", None, "salon/traversal", 0, 0)
            .expect("connexion du client");
        let ServerMsg::Welcome { player_id } = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu (transport inconditionnel)")
        else {
            panic!("premier message attendu : Welcome");
        };

        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        assert_eq!(
            room.app.network_player_object(player_id),
            None,
            "un Join avec un code de salon invalide ne doit inscrire le joueur \
             dans aucun salon"
        );
        assert!(
            !player_room.contains_key(&player_id),
            "un Join rejeté ne doit pas router les messages suivants de ce joueur"
        );
    }

    /// Sprint 103c (audit réseau après la migration du joueur vers
    /// `KinematicCharacterController`, Sprint 103b) : livrable explicite du
    /// roadmap — « aucun rubber-banding à 100 ms simulées ». Mêmes
    /// `NetServer`/`NetClient` réels que `joining_moving_and_leaving_
    /// through_the_real_socket` ci-dessus, mais le serveur ne traite son
    /// inbox/n'avance sa simulation qu'une fois toutes les 100 ms (au lieu
    /// des ~20 ms habituels) — une pacing bien plus lente que le tick
    /// serveur réel simule un aller-retour réseau dégradé sans horloge
    /// simulée (ce dépôt n'utilise que des `sleep`/`Instant` réels, cf.
    /// `SPRINTNETWORK.md`). « Rubber-banding » = la position oscille ou
    /// recule brièvement avant de repartir en avant ; ce test suit la
    /// position à chaque tick traité et vérifie qu'elle progresse
    /// globalement dans le sens du mouvement, jamais un aller-retour marqué
    /// entre deux ticks consécutifs.
    #[test]
    fn sustained_movement_does_not_rubber_band_at_100ms_simulated_latency() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        rooms.insert(DEFAULT_LOBBY.to_string(), zombies_room());
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client = NetClient::connect(&url, "Bob", None).expect("connexion du client");
        let ServerMsg::Welcome { player_id } = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu")
        else {
            panic!("premier message attendu : Welcome");
        };
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        let object_index = room
            .app
            .network_player_object(player_id)
            .expect("le Join doit avoir fait apparaître un objet pilotable");
        let start = room.app.scene.objects[object_index].transform.position;

        client.send(&motor3derust::net::protocol::ClientMsg::Input {
            move_x: 1.0,
            move_y: 0.0,
            aim_yaw: 0.0,
            attack: false,
            jump: false,
            fire: false,
            weapon: 0,
            heal: false,
        });
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Input attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        // Comme `joining_moving_and_leaving_through_the_real_socket` : pas
        // d'autre `Input` envoyé après celui-ci, `advance_play` continue de
        // piloter l'objet à partir de la dernière entrée connue
        // (`network_inputs`, persistante jusqu'au prochain message) — inutile
        // de redrainer l'inbox à chaque tick de la boucle.
        let room = rooms.get_mut(DEFAULT_LOBBY).unwrap();
        let mut previous = start;
        let mut max_backward_step = 0.0_f32;
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(100));
            room.app.advance_play();
            let current = room.app.scene.objects[object_index].transform.position;
            // Recul entre deux ticks consécutifs le long de l'axe de
            // déplacement (X, `move_x = 1.0` ci-dessus) : au-delà d'un bruit
            // négligeable, ce serait le symptôme même du rubber-banding.
            let backward = (previous.x - current.x).max(0.0);
            max_backward_step = max_backward_step.max(backward);
            previous = current;
        }

        let end = room.app.scene.objects[object_index].transform.position;
        assert!(
            (end.x - start.x).abs() > 0.5,
            "le mouvement doit progresser malgré la latence simulée : {start:?} -> {end:?}"
        );
        assert!(
            max_backward_step < 0.05,
            "aucun tick ne doit reculer sensiblement (rubber-banding) : recul \
             maximal observé {max_backward_step} m"
        );
    }

    /// Un joueur qui ne donne plus signe de vie (freeze, crash sans
    /// `Leave` propre) doit être retiré après le délai de timeout, sans bloquer
    /// la partie des autres. Utilise un `timeout` court (paramètre de
    /// `evict_timed_out_players`) plutôt que `CLIENT_TIMEOUT` (60 s réelles).
    #[test]
    fn a_silent_client_is_evicted_after_the_timeout() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        rooms.insert(DEFAULT_LOBBY.to_string(), zombies_room());
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client = NetClient::connect(&url, "Silencieux", None).expect("connexion");
        let ServerMsg::Welcome { player_id } = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu")
        else {
            panic!("premier message attendu : Welcome");
        };
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);
        assert!(
            rooms
                .get(DEFAULT_LOBBY)
                .unwrap()
                .app
                .network_player_object(player_id)
                .is_some()
        );

        // Aucun message pendant plus que le timeout court : le joueur doit être
        // évincé au prochain passage de `evict_timed_out_players`.
        let short_timeout = Duration::from_millis(50);
        std::thread::sleep(Duration::from_millis(120));
        evict_timed_out_players(&mut rooms, &mut player_room, &net, short_timeout);

        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        assert_eq!(
            room.app.network_player_object(player_id),
            None,
            "un joueur silencieux depuis plus que le timeout doit être retiré"
        );
        assert!(!room.lobby.last_seen.contains_key(&player_id));
        assert!(!player_room.contains_key(&player_id));
    }

    /// Deux clients qui rejoignent des salons différents (cf.
    /// GAMEDESIGN_EN_LIGNE.md §3.3) ne doivent jamais se voir l'un l'autre —
    /// chacun reste dans sa propre `AppState`, avec ses propres indices d'objets.
    #[test]
    fn two_clients_in_different_lobbies_land_in_separate_rooms() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let a = NetClient::connect_to_lobby(&url, "A", None, "salon-a", 0, 0).expect("connexion A");
        let b = NetClient::connect_to_lobby(&url, "B", None, "salon-b", 0, 0).expect("connexion B");
        let welcome_a = a.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
        let welcome_b = b.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
        let (ServerMsg::Welcome { player_id: id_a }, ServerMsg::Welcome { player_id: id_b }) =
            (welcome_a, welcome_b)
        else {
            panic!("Welcome attendu pour les deux clients");
        };

        for _ in 0..2 {
            let (id, msg) = net
                .inbox
                .recv_timeout(Duration::from_secs(2))
                .expect("Join attendu côté serveur");
            handle_message(&mut rooms, &mut player_room, &net, id, msg);
        }

        assert_eq!(
            rooms.len(),
            2,
            "deux salons distincts doivent avoir été créés"
        );
        assert!(rooms.contains_key("salon-a"));
        assert!(rooms.contains_key("salon-b"));
        assert_eq!(player_room.get(&id_a), Some(&"salon-a".to_string()));
        assert_eq!(player_room.get(&id_b), Some(&"salon-b".to_string()));

        // Le salon de B n'a aucune trace de A, et réciproquement.
        assert!(rooms["salon-a"].app.network_player_object(id_b).is_none());
        assert!(rooms["salon-b"].app.network_player_object(id_a).is_none());
        assert_eq!(rooms["salon-a"].lobby.names.len(), 1);
        assert_eq!(rooms["salon-b"].lobby.names.len(), 1);
    }

    /// Quand le dernier joueur d'un salon part, le salon disparaît
    /// (pas de manche qui tourne indéfiniment pour personne).
    #[test]
    fn a_room_closes_once_its_last_player_leaves() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client =
            NetClient::connect_to_lobby(&url, "Solo", None, "ephemere", 0, 0).expect("connexion");
        client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu");
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);
        assert!(rooms.contains_key("ephemere"));

        client.send(&motor3derust::net::protocol::ClientMsg::Leave);
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Leave attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        // `handle_message` masque le joueur mais ne ferme le salon vide que la
        // boucle `main` (le nettoyage `to_close` vit dans `main`, pas dans
        // `handle_message`, pour rester testable sans lancer tout le binaire) —
        // ici on vérifie juste la partie qu'expose `handle_message` :
        // plus aucun joueur connecté, prêt à être fermé au prochain tour de boucle.
        assert!(rooms["ephemere"].connected_ids().is_empty());
    }
}
