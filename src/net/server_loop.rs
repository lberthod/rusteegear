//! Transport WebSocket côté serveur.
//!
//! `NetServer` accepte des connexions dans un thread dédié, et n'expose au
//! reste du programme que des canaux `std::sync::mpsc` **synchrones** — le
//! même schéma que les imports glTF ou les requêtes IA asynchrones déjà
//! présents dans `app/mod.rs` (thread de fond + canal, poll non bloquant côté
//! boucle principale). La boucle de jeu (`AppState`, `src/bin/server.rs`) n'a
//! donc jamais besoin de connaître `tokio`.
//!
//! **Runtime `current_thread`** : à l'échelle visée (2-16 joueurs/salon),
//! accepter des connexions et faire progresser une poignée de sockets est un
//! travail d'attente réseau, pas de calcul parallèle — un runtime multi-thread
//! (`tokio::runtime::Runtime::new()`) réserverait un thread ouvrier par CPU
//! logique pour rien (cf. docs/audits/net.md). Le thread dédié ci-dessous
//! `block_on` la boucle d'acceptation *et* toutes les connexions (via
//! `tokio::spawn`, ordonnancées coopérativement sur ce seul thread) pour toute
//! la durée de vie du serveur.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::protocol::{self, ClientMsg, PlayerId, ServerMsg};

/// Taille maximale (octets) d'une trame/message WebSocket entrant (Sprint
/// 105a-2, durcissement) — bien au-delà de tout `ClientMsg` légitime de ce
/// protocole (un `Input` tient sur quelques dizaines d'octets encodés en
/// `bincode`), très en-deçà des valeurs par défaut de tungstenite (64 Mio/
/// message, 16 Mio/trame) : filet de sécurité en amont du décodage, avant
/// même que `protocol::valid_join_fields` n'entre en jeu pour les champs
/// individuels d'un `Join`.
const MAX_WS_MESSAGE_BYTES: usize = 64 * 1024;

fn server_ws_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_WS_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_WS_MESSAGE_BYTES))
}

/// Rate limiting par connexion (Sprint 113c) : `MAX_WS_MESSAGE_BYTES` borne déjà la
/// taille d'un message *individuel*, mais rien n'empêchait jusqu'ici un client de les
/// enchaîner sans limite — un flood de petits messages valides passe outre ce filtre.
/// Fenêtre glissante d'une seconde approchée par deux seaux (cf. `RateLimiter` —
/// pas de fuite mémoire : quatre compteurs + un `Instant` par connexion).
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(1);
/// Un client légitime envoie au plus un `Input` par tick serveur (`SERVER_TICK` =
/// ~60 Hz, cf. `src/bin/server.rs`) ; ×2 pour absorber le jitter réseau/scheduling
/// sans pénaliser un client honnête proche de la limite.
const MAX_MESSAGES_PER_SEC: u32 = 120;
/// Un `Input`/`Leave` légitime tient sur quelques dizaines d'octets encodés en
/// `bincode` — réutilise `MAX_WS_MESSAGE_BYTES` comme budget *cumulé* par seconde
/// (pas par message) : très généreux pour du trafic légitime, mais empêche un
/// client d'atteindre `MAX_MESSAGES_PER_SEC` en enchaînant des trames proches du
/// maximum autorisé par message.
const MAX_BYTES_PER_SEC: usize = MAX_WS_MESSAGE_BYTES;

/// Connexions simultanées tolérées depuis une même adresse IP (Sprint 113c,
/// garde-fou anti-DoS basique — pas un WAF complet, cf. ROADMAP_SPRINTS.md). Assez
/// pour un joueur légitime avec plusieurs onglets/instances de test, pas assez pour
/// qu'une seule machine épuise les ressources du serveur en ouvrant des centaines de
/// sockets.
const MAX_CONNECTIONS_PER_IP: usize = 4;

/// Limiteur de débit à fenêtre glissante **approchée par deux seaux** (audit
/// réseau 2026-07) : l'ancienne fenêtre « réinitialisée en bloc » laissait un
/// client émettre jusqu'à ~2× `MAX_MESSAGES_PER_SEC` en concentrant un burst à
/// la toute fin d'une fenêtre puis un second juste après la remise à zéro. Ici,
/// le débit estimé sur la dernière seconde glissante compte le seau courant en
/// entier plus le seau précédent pondéré par la fraction de sa fenêtre encore
/// couverte (technique classique du « sliding window counter ») : le pire
/// dépassement possible tombe de ~2× à une approximation marginale, sans
/// horodater chaque message individuellement.
///
/// Struct pure (le temps est un paramètre, jamais `Instant::now()` en interne)
/// pour être testable de façon déterministe sans socket ni `sleep`.
struct RateLimiter {
    /// Début de la fenêtre courante (avance par pas de `RATE_LIMIT_WINDOW`).
    window_start: Instant,
    /// Compteurs de la fenêtre précédente (pondérés à l'estimation).
    prev_msgs: u32,
    prev_bytes: usize,
    /// Compteurs de la fenêtre courante (comptés en entier).
    curr_msgs: u32,
    curr_bytes: usize,
}

impl RateLimiter {
    fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            prev_msgs: 0,
            prev_bytes: 0,
            curr_msgs: 0,
            curr_bytes: 0,
        }
    }

    /// Fait basculer les seaux jusqu'à ce que `now` tombe dans la fenêtre
    /// courante : une fenêtre écoulée décale courant → précédent ; plus d'une
    /// fenêtre complète de silence périme les deux seaux d'un coup.
    fn roll(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.window_start);
        if elapsed >= RATE_LIMIT_WINDOW * 2 {
            self.prev_msgs = 0;
            self.prev_bytes = 0;
            self.curr_msgs = 0;
            self.curr_bytes = 0;
            self.window_start = now;
        } else if elapsed >= RATE_LIMIT_WINDOW {
            self.prev_msgs = self.curr_msgs;
            self.prev_bytes = self.curr_bytes;
            self.curr_msgs = 0;
            self.curr_bytes = 0;
            self.window_start += RATE_LIMIT_WINDOW;
        }
    }

    /// Enregistre un message de `len` octets reçu à `now` ; `true` si le débit
    /// estimé sur la dernière seconde glissante dépasse `MAX_MESSAGES_PER_SEC`
    /// ou `MAX_BYTES_PER_SEC` (l'appelant coupe alors la connexion).
    fn over_budget(&mut self, now: Instant, len: usize) -> bool {
        self.roll(now);
        self.curr_msgs = self.curr_msgs.saturating_add(1);
        self.curr_bytes = self.curr_bytes.saturating_add(len);
        // Part de la fenêtre précédente encore couverte par la seconde
        // glissante qui se termine à `now` : 1.0 au tout début de la fenêtre
        // courante, 0.0 à sa fin.
        let frac =
            now.duration_since(self.window_start).as_secs_f64() / RATE_LIMIT_WINDOW.as_secs_f64();
        let carry = (1.0 - frac).max(0.0);
        let msgs = f64::from(self.prev_msgs) * carry + f64::from(self.curr_msgs);
        let bytes = self.prev_bytes as f64 * carry + self.curr_bytes as f64;
        msgs > f64::from(MAX_MESSAGES_PER_SEC) || bytes > MAX_BYTES_PER_SEC as f64
    }
}

type IpCounts = Arc<Mutex<HashMap<IpAddr, usize>>>;

fn lock_ip_counts(counts: &IpCounts) -> std::sync::MutexGuard<'_, HashMap<IpAddr, usize>> {
    counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Décrémente le compteur de connexions de `ip` à la destruction (toutes les sorties
/// de `handle_connection`, y compris via `?`, doivent libérer leur créneau — un
/// `Drop` évite de dupliquer ce nettoyage sur chaque chemin de sortie).
struct IpGuard {
    counts: IpCounts,
    ip: IpAddr,
}

impl Drop for IpGuard {
    fn drop(&mut self) {
        let mut counts = lock_ip_counts(&self.counts);
        if let Some(n) = counts.get_mut(&self.ip) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                counts.remove(&self.ip);
            }
        }
    }
}

/// Plafond de l'inbox partagée (messages en attente côté thread principal,
/// toutes connexions confondues — audit réseau 2026-07) : l'ancien canal non
/// borné laissait la mémoire croître sans limite si le thread principal calait
/// (GC de scène, pause débogueur…). Dimensionnement : `MAX_MESSAGES_PER_SEC`
/// (120) × 16 joueurs ≈ 2000 messages/s au pire toléré par le rate limiting,
/// et la boucle de tick draine tout toutes les ~16 ms — 4096 messages
/// représentent ~2 s de calage complet avant les premières pertes, pour
/// quelques centaines de Kio au maximum. Au-delà, les messages sont jetés
/// (`try_send`, jamais bloquant : cf. la pompe entrante) avec un compteur
/// logué par connexion.
const INBOX_CAPACITY: usize = 4096;

/// Message reçu d'un client, avec l'identifiant du joueur qui l'a envoyé.
pub type Inbound = (PlayerId, ClientMsg);

type Outboxes = Arc<Mutex<HashMap<PlayerId, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>>;

/// Verrouille `outboxes` en récupérant le contenu même si le mutex est empoisonné
/// (Sprint 113b, durcissement) : `insert`/`remove`/lecture sur une simple `HashMap`
/// ne laissent rien d'incohérent en mémoire même interrompus par un panic — un seul
/// client fautif ne doit pas figer `send_to`/`broadcast_all_rooms` pour tous les autres
/// joueurs (et donc tout le thread de jeu principal) derrière un `.unwrap()` qui
/// re-paniquerait à chaque appel suivant.
fn lock_outboxes(
    outboxes: &Outboxes,
) -> std::sync::MutexGuard<'_, HashMap<PlayerId, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>> {
    outboxes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Serveur réseau : accepte des connexions WebSocket, décode les `ClientMsg` reçus
/// et les pousse dans `inbox` ; `send_to`/`send_to_many`/`broadcast_all_rooms`
/// encodent et poussent des `ServerMsg` vers un, plusieurs ou tous les clients
/// connectés.
pub struct NetServer {
    /// Messages reçus des clients, à consommer par le thread principal (non
    /// bloquant : `try_recv` une fois par tick).
    pub inbox: Receiver<Inbound>,
    outboxes: Outboxes,
    next_id: Arc<AtomicU32>,
    /// Adresse effectivement liée (utile en test : `"127.0.0.1:0"` laisse l'OS
    /// choisir un port libre).
    pub local_addr: SocketAddr,
}

impl NetServer {
    /// Démarre le serveur sur `addr` (ex. `"127.0.0.1:7777"`), avec le
    /// plafond de connexions par IP de production (`MAX_CONNECTIONS_PER_IP`).
    pub fn start(addr: &str) -> std::io::Result<Self> {
        Self::start_with_ip_cap(addr, MAX_CONNECTIONS_PER_IP)
    }

    /// Comme `start`, mais avec un plafond de connexions par IP explicite —
    /// pour les outils qui concentrent volontairement beaucoup de clients
    /// légitimes derrière une seule adresse (`examples/load_test_client.rs` :
    /// 16 bots depuis 127.0.0.1, que le plafond de production refusait dès le
    /// 5ᵉ). La production (`src/bin/server.rs`) passe toujours par `start` :
    /// le garde-fou anti-DoS n'y est pas affaibli.
    pub fn start_with_ip_cap(addr: &str, max_connections_per_ip: usize) -> std::io::Result<Self> {
        Self::start_inner(addr, max_connections_per_ip, INBOX_CAPACITY)
    }

    /// Cœur commun : `inbox_capacity` en paramètre uniquement pour que les
    /// tests puissent saturer une inbox minuscule sans envoyer 4096 messages
    /// réels (la production passe toujours `INBOX_CAPACITY`).
    fn start_inner(
        addr: &str,
        max_connections_per_ip: usize,
        inbox_capacity: usize,
    ) -> std::io::Result<Self> {
        let (tx, rx) = sync_channel::<Inbound>(inbox_capacity);
        let outboxes: Outboxes = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(AtomicU32::new(1));
        let ip_counts: IpCounts = Arc::new(Mutex::new(HashMap::new()));

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<std::io::Result<SocketAddr>>();
        let addr = addr.to_string();
        let accept_outboxes = outboxes.clone();
        let accept_next_id = next_id.clone();
        let accept_ip_counts = ip_counts.clone();
        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            runtime.block_on(async move {
                let listener = match TcpListener::bind(&addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let local_addr = listener.local_addr();
                let bind_ok = local_addr.is_ok();
                let _ = ready_tx.send(local_addr);
                if !bind_ok {
                    return;
                }
                loop {
                    let (stream, peer) = match listener.accept().await {
                        Ok(v) => v,
                        Err(e) => {
                            log::warn!("Connexion entrante refusée : {e}");
                            continue;
                        }
                    };
                    // Sans ça, l'algorithme de Nagle retarde nos petites trames
                    // fréquentes (`Input`/`Snapshot`, quelques dizaines d'octets,
                    // plusieurs par seconde) jusqu'à ~40 ms pour les regrouper —
                    // exactement le pire cas pour ce trafic (cf. docs/audits/net.md).
                    if let Err(e) = stream.set_nodelay(true) {
                        log::warn!("TCP_NODELAY impossible sur {peer} : {e}");
                    }

                    // Garde-fou anti-DoS basique (Sprint 113c) : refusée avant même la
                    // poignée de main WebSocket, moins de travail gaspillé qu'un refus
                    // après handshake pour une IP déjà au plafond.
                    {
                        let mut counts = lock_ip_counts(&accept_ip_counts);
                        let n = counts.entry(peer.ip()).or_insert(0);
                        if *n >= max_connections_per_ip {
                            log::warn!(
                                "Connexion refusée depuis {} : déjà {n} connexion(s) simultanée(s) (max {max_connections_per_ip})",
                                peer.ip()
                            );
                            continue;
                        }
                        *n += 1;
                    }
                    let ip_guard = IpGuard {
                        counts: accept_ip_counts.clone(),
                        ip: peer.ip(),
                    };

                    let tx = tx.clone();
                    let outboxes = accept_outboxes.clone();
                    let next_id = accept_next_id.clone();
                    tokio::spawn(async move {
                        let _ip_guard = ip_guard;
                        if let Err(e) = handle_connection(stream, peer, tx, outboxes, next_id).await
                        {
                            log::info!("Connexion {peer} terminée : {e}");
                        }
                    });
                }
            });
        });

        let local_addr = ready_rx.recv().map_err(|_| {
            std::io::Error::other("le thread réseau du serveur s'est arrêté avant le bind")
        })??;

        Ok(Self {
            inbox: rx,
            outboxes,
            next_id,
            local_addr,
        })
    }

    /// Envoie un message à un joueur précis ; sans effet s'il n'est plus connecté.
    pub fn send_to(&self, id: PlayerId, msg: &ServerMsg) {
        let Ok(bytes) = protocol::encode(msg) else {
            return;
        };
        if let Some(tx) = lock_outboxes(&self.outboxes).get(&id) {
            let _ = tx.send(bytes);
        }
    }

    /// Broadcast **ciblé** : envoie le même message à chaque joueur de `ids`,
    /// en ne l'encodant qu'UNE seule fois (les destinataires reçoivent un clone
    /// des octets, pas un ré-encodage bincode par joueur — à 16 joueurs × 60 Hz
    /// de `Snapshot`, l'ancien `send_to` en boucle refaisait 16 encodages
    /// identiques par tick, cf. docs/audits/net.md). À la différence de
    /// `broadcast_all_rooms` (tous les clients du process), la liste d'ids est
    /// fournie par l'appelant : le serveur multi-salons passe les joueurs du
    /// salon concerné et rien ne fuite vers les autres salons. Les ids absents
    /// (déconnectés entre-temps) sont ignorés, comme dans `send_to`.
    pub fn send_to_many(&self, ids: &[PlayerId], msg: &ServerMsg) {
        if ids.is_empty() {
            return;
        }
        let Ok(bytes) = protocol::encode(msg) else {
            return;
        };
        let outboxes = lock_outboxes(&self.outboxes);
        for id in ids {
            if let Some(tx) = outboxes.get(id) {
                let _ = tx.send(bytes.clone());
            }
        }
    }

    /// Envoie un message à **tous les clients du process, tous salons
    /// confondus** — d'où le nom : `NetServer` ne connaît pas les salons
    /// (routage applicatif dans `src/bin/server.rs`), un appel naïf ici
    /// fuiterait l'état d'un salon vers les autres. Le serveur multi-salons
    /// n'utilise QUE `send_to` en boucle sur les ids du salon concerné (cf.
    /// `server.rs`, qui documente ce choix) ; cette méthode ne convient qu'aux
    /// contextes mono-salon garantis (tests, `examples/load_test_client.rs`).
    pub fn broadcast_all_rooms(&self, msg: &ServerMsg) {
        let Ok(bytes) = protocol::encode(msg) else {
            return;
        };
        for tx in lock_outboxes(&self.outboxes).values() {
            let _ = tx.send(bytes.clone());
        }
    }

    /// Coupe la connexion du joueur `id` côté serveur ; sans effet s'il n'est
    /// plus connecté. Retirer son outbox droppe la dernière extrémité émettrice
    /// de son canal sortant : la pompe sortante de `handle_connection` se
    /// termine, le `select!` ferme la connexion, et le `Leave` synthétique de
    /// fin de connexion prévient le thread principal — exactement le même
    /// chemin qu'une perte de connexion réelle, ce qui en fait aussi l'outil
    /// des tests de reconnexion client (cf. `app::network_client`).
    pub fn disconnect(&self, id: PlayerId) {
        lock_outboxes(&self.outboxes).remove(&id);
    }

    /// Nombre de clients actuellement connectés.
    pub fn connected_count(&self) -> usize {
        lock_outboxes(&self.outboxes).len()
    }

    /// Identifiant qui serait attribué au prochain joueur à rejoindre (utile pour
    /// les tests / logs ; ne réserve rien).
    pub fn next_player_id(&self) -> PlayerId {
        self.next_id.load(Ordering::Relaxed)
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    tx: SyncSender<Inbound>,
    outboxes: Outboxes,
    next_id: Arc<AtomicU32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ws = tokio_tungstenite::accept_async_with_config(stream, Some(server_ws_config())).await?;
    let (mut sink, mut stream) = ws.split();

    // Première trame attendue : `ClientMsg::Join`. Toute autre trame, ou une
    // déconnexion avant d'avoir rejoint, met fin à la connexion.
    let first = stream.next().await.ok_or("connexion fermée avant Join")??;
    let join_bytes = match first {
        Message::Binary(b) => b,
        _ => return Err("première trame non binaire".into()),
    };
    let ClientMsg::Join {
        protocol: client_protocol,
        name,
        firebase_uid,
        lobby,
        class,
        objective,
    } = protocol::decode::<ClientMsg>(&join_bytes)?
    else {
        return Err("première trame n'est pas un Join".into());
    };

    // Version de protocole vérifiée avant toute autre chose (attribution d'id,
    // outbox, Welcome) : un client incompatible reçoit la raison en clair puis
    // la connexion se ferme — avant ce contrôle, il mourait dans un `decode`
    // silencieux sans aucun diagnostic. Limite assumée : un client d'avant le
    // versioning (sans champ `protocol`) échoue toujours au `decode` ci-dessus,
    // on ne peut rien lui répondre d'utile — le bénéfice vaut pour tous les
    // bumps futurs (cf. l'invariant documenté sur `ClientMsg`).
    if client_protocol != protocol::PROTOCOL_VERSION {
        let rejected = protocol::encode(&ServerMsg::JoinRejected {
            reason: format!(
                "version de protocole {client_protocol} incompatible (serveur : {}) — mettez \
                 le jeu à jour",
                protocol::PROTOCOL_VERSION
            ),
        })?;
        sink.send(Message::Binary(rejected.into())).await?;
        return Err(format!(
            "version de protocole incompatible ({client_protocol} ≠ {})",
            protocol::PROTOCOL_VERSION
        )
        .into());
    }

    let id = next_id.fetch_add(1, Ordering::Relaxed);
    log::info!("Joueur {id} ({name}) connecté depuis {peer}");

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    lock_outboxes(&outboxes).insert(id, out_tx);

    let welcome = protocol::encode(&ServerMsg::Welcome { player_id: id })?;
    sink.send(Message::Binary(welcome.into())).await?;

    // Relaie aussi le `Join` lui-même au thread principal (contrairement au
    // `Welcome`, géré ici) : c'est le signal qui doit faire apparaître le joueur
    // dans la partie (cf. `AppState::spawn_network_player`). Une défaillance
    // d'envoi ici (thread principal arrêté, ou inbox pleine parce qu'il est
    // calé — auquel cas ce join n'aurait de toute façon pas été traité) ne
    // doit pas empêcher la connexion de continuer, donc ni `?` ni blocage.
    let _ = tx.try_send((
        id,
        ClientMsg::Join {
            protocol: client_protocol,
            name,
            firebase_uid,
            lobby,
            class,
            objective,
        },
    ));

    // Pompe sortante : relaie les messages poussés par `send_to`/`broadcast_all_rooms`
    // (thread principal) vers la socket, jusqu'à fermeture du canal ou erreur
    // d'écriture.
    let outbound = async move {
        while let Some(bytes) = out_rx.recv().await {
            if sink.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    };

    // Pompe entrante : décode chaque trame en `ClientMsg` et la transmet au thread
    // principal via le canal synchrone borné (jamais bloquant : `try_send`, cf.
    // `INBOX_CAPACITY` — un envoi bloquant sur canal plein figerait le runtime
    // `current_thread`, donc TOUTES les connexions, exactement le deadlock à
    // éviter). Rate limiting (Sprint 113c, durci audit 2026-07) : fenêtre
    // glissante à deux seaux (`RateLimiter`) — état purement local à cette
    // tâche, pas besoin de le partager (chaque connexion a la sienne).
    let inbound_tx = tx.clone();
    let inbound = async move {
        let mut limiter = RateLimiter::new(Instant::now());
        // Messages jetés faute de place dans l'inbox (thread principal calé) —
        // logué avec parcimonie pour ne pas transformer l'incident en flood de
        // logs.
        let mut dropped: u64 = 0;
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Binary(bytes) = msg {
                if limiter.over_budget(Instant::now(), bytes.len()) {
                    log::warn!(
                        "Connexion {peer} (joueur {id}) coupée : rate limit dépassé \
                         ({} messages / {} octets dans la dernière seconde)",
                        limiter.curr_msgs,
                        limiter.curr_bytes
                    );
                    break;
                }
                if let Ok(client_msg) = protocol::decode::<ClientMsg>(&bytes) {
                    let is_leave = matches!(client_msg, ClientMsg::Leave);
                    match inbound_tx.try_send((id, client_msg)) {
                        Ok(()) => {}
                        // Inbox pleine : le thread principal ne draine plus. On
                        // jette le message plutôt que d'accumuler sans borne (un
                        // `Input` perdu est remplacé par le suivant ; un `Leave`
                        // perdu est rattrapé par le `Leave` synthétique de fin de
                        // connexion puis, au pire, par `CLIENT_TIMEOUT`).
                        Err(TrySendError::Full(_)) => {
                            dropped += 1;
                            if dropped == 1 || dropped.is_multiple_of(1000) {
                                log::warn!(
                                    "Connexion {peer} (joueur {id}) : inbox serveur pleine \
                                     ({INBOX_CAPACITY} messages), {dropped} message(s) jeté(s) — \
                                     thread principal calé ?"
                                );
                            }
                        }
                        // Thread principal terminé : plus personne à prévenir.
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                    if is_leave {
                        break;
                    }
                }
            }
        }
    };

    tokio::select! {
        _ = outbound => {}
        _ = inbound => {}
    }
    lock_outboxes(&outboxes).remove(&id);
    // Signale la déconnexion au thread principal, qu'elle soit volontaire (déjà
    // relayée par la pompe entrante) ou abrupte (perte de connexion) — envoyer un
    // second `Leave` dans le premier cas ne coûte rien : `despawn_network_player`
    // est idempotent (retirer un joueur déjà absent ne fait rien). `try_send` :
    // si l'inbox est pleine, `CLIENT_TIMEOUT` finit de nettoyer le fantôme.
    let _ = tx.try_send((id, ClientMsg::Leave));
    log::info!("Joueur {id} déconnecté");
    Ok(())
}

// Tests **purs** du limiteur de débit (aucun socket : le temps est un
// paramètre de `RateLimiter`) — hors du gate `net_tests` exprès, la propriété
// « pas de burst à ~2× la limite à cheval sur deux fenêtres » doit être
// vérifiée par le `cargo test` de tous les jours.
#[cfg(test)]
mod rate_limiter_tests {
    use super::*;

    /// Le cœur du correctif (audit 2026-07) : un burst juste sous la limite à
    /// la toute fin d'une fenêtre, suivi d'un second juste après le
    /// basculement, doit être coupé — l'ancienne remise à zéro en bloc
    /// laissait passer les deux (~2× `MAX_MESSAGES_PER_SEC` en ~20 ms).
    #[test]
    fn a_burst_straddling_two_windows_is_still_cut() {
        let t0 = Instant::now();
        let mut limiter = RateLimiter::new(t0);

        let end_of_first = t0 + RATE_LIMIT_WINDOW - Duration::from_millis(10);
        for _ in 0..100 {
            assert!(
                !limiter.over_budget(end_of_first, 10),
                "100 messages dans une fenêtre restent sous la limite ({MAX_MESSAGES_PER_SEC})"
            );
        }

        let start_of_second = t0 + RATE_LIMIT_WINDOW + Duration::from_millis(10);
        let cut = (0..100).any(|_| limiter.over_budget(start_of_second, 10));
        assert!(
            cut,
            "un second burst juste après le basculement de fenêtre doit être coupé \
             (l'ancienne fenêtre en bloc l'aurait laissé passer)"
        );
    }

    /// Un client honnête (~60 `Input`/s, un par tick) ne doit jamais être
    /// coupé, y compris au franchissement des fenêtres.
    #[test]
    fn a_steady_legitimate_client_is_never_cut() {
        let t0 = Instant::now();
        let mut limiter = RateLimiter::new(t0);
        for i in 0u64..180 {
            let now = t0 + Duration::from_millis(1000 * i / 60);
            assert!(
                !limiter.over_budget(now, 40),
                "un client à ~60 messages/s ne doit jamais être coupé (message {i})"
            );
        }
    }

    /// Le budget d'octets glisse lui aussi : des messages volumineux juste
    /// sous `MAX_BYTES_PER_SEC` en fin de fenêtre pèsent encore au début de
    /// la suivante.
    #[test]
    fn the_byte_budget_also_slides_across_windows() {
        let t0 = Instant::now();
        let mut limiter = RateLimiter::new(t0);

        let end_of_first = t0 + RATE_LIMIT_WINDOW - Duration::from_millis(10);
        for _ in 0..60 {
            assert!(
                !limiter.over_budget(end_of_first, 1090),
                "60 × 1090 octets restent sous MAX_BYTES_PER_SEC ({MAX_BYTES_PER_SEC})"
            );
        }

        let start_of_second = t0 + RATE_LIMIT_WINDOW + Duration::from_millis(10);
        let cut = (0..60).any(|_| limiter.over_budget(start_of_second, 1090));
        assert!(
            cut,
            "le report pondéré des octets de la fenêtre précédente doit couper ce second burst"
        );
    }

    /// Après plus d'une fenêtre complète de silence, les deux seaux sont
    /// périmés : un client qui reprend un trafic normal repart de zéro (pas
    /// de dette fantôme).
    #[test]
    fn an_idle_gap_clears_both_buckets() {
        let t0 = Instant::now();
        let mut limiter = RateLimiter::new(t0);
        for _ in 0..100 {
            assert!(!limiter.over_budget(t0, 10));
        }
        let after_gap = t0 + Duration::from_secs(3);
        for _ in 0..100 {
            assert!(
                !limiter.over_budget(after_gap, 10),
                "après > 2 fenêtres de silence, un burst sous la limite doit passer"
            );
        }
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

    use super::super::client::NetClient;
    use super::*;

    /// Bout-en-bout transport : un `NetClient` se
    /// connecte, envoie un `Join`, reçoit son `Welcome`, puis envoie un `Input` que
    /// le serveur doit recevoir avec le bon `PlayerId`. Vérifie la plomberie
    /// WebSocket + (dé)sérialisation sans dépendre d'une fenêtre graphique (aucun
    /// moyen d'ouvrir deux fenêtres winit dans cet environnement de test).
    #[test]
    fn client_joins_and_server_receives_its_input() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let client = NetClient::connect(&url, "Testeur", None).expect("connexion du client");

        let welcome = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("le client doit recevoir un Welcome");
        let ServerMsg::Welcome { player_id } = welcome else {
            panic!("premier message attendu : Welcome, reçu {welcome:?}");
        };

        // Le serveur relaie aussi le `Join` initial au thread principal (cf.
        // `AppState::spawn_network_player`) : c'est le premier message dans
        // `inbox`, avant l'`Input` envoyé ci-dessous.
        let (join_id, join_msg) = server
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("le serveur doit recevoir le Join du client");
        assert_eq!(join_id, player_id);
        assert_eq!(
            join_msg,
            ClientMsg::Join {
                protocol: protocol::PROTOCOL_VERSION,
                name: "Testeur".to_string(),
                firebase_uid: None,
                lobby: protocol::DEFAULT_LOBBY.to_string(),
                class: 0,
                objective: 0,
            }
        );

        client.send(&ClientMsg::Input {
            move_x: 0.5,
            move_y: -1.0,
            aim_yaw: 0.0,
            attack: true,
            jump: false,
            fire: false,
            weapon: 0,
            heal: false,
        });

        let (id, msg) = server
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("le serveur doit recevoir l'Input du client");
        assert_eq!(id, player_id);
        assert_eq!(
            msg,
            ClientMsg::Input {
                move_x: 0.5,
                move_y: -1.0,
                aim_yaw: 0.0,
                attack: true,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
            }
        );
    }

    /// Deux clients dans le même salon obtiennent des identifiants distincts, et un
    /// `broadcast` atteint les deux (préfigure la Snapshot diffusée à chaque tick).
    #[test]
    fn broadcast_all_rooms_reaches_every_connected_client() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let a = NetClient::connect(&url, "A", None).expect("connexion A");
        let b = NetClient::connect(&url, "B", None).expect("connexion B");

        let welcome_a = a.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
        let welcome_b = b.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_ne!(
            welcome_a, welcome_b,
            "deux joueurs doivent avoir des id distincts"
        );

        // Laisse le temps aux deux connexions de s'enregistrer dans `outboxes`
        // avant le broadcast (asynchrone, pas garanti terminé au retour de `recv`).
        let mut waited = Duration::ZERO;
        while server.connected_count() < 2 && waited < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
            waited += Duration::from_millis(10);
        }
        assert_eq!(server.connected_count(), 2);

        server.broadcast_all_rooms(&ServerMsg::Event(protocol::GameEvent::WaveStart {
            wave: 1,
        }));

        for c in [&a, &b] {
            let msg = c.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(
                msg,
                ServerMsg::Event(protocol::GameEvent::WaveStart { wave: 1 })
            );
        }
    }

    /// Broadcast ciblé (audit 2026-07) : `send_to_many` encode une seule fois
    /// mais ne doit atteindre QUE les ids listés — c'est la garantie qui le
    /// rend utilisable par le serveur multi-salons (contrairement à
    /// `broadcast_all_rooms`, qui fuiterait l'état d'un salon vers les autres).
    #[test]
    fn send_to_many_reaches_only_the_listed_recipients() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let a = NetClient::connect(&url, "A", None).expect("connexion A");
        let b = NetClient::connect(&url, "B", None).expect("connexion B");
        let c = NetClient::connect(&url, "C", None).expect("connexion C");

        let mut ids = Vec::new();
        for client in [&a, &b, &c] {
            let welcome = client.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
            let ServerMsg::Welcome { player_id } = welcome else {
                panic!("Welcome attendu, reçu {welcome:?}");
            };
            ids.push(player_id);
        }

        // Comme pour le broadcast : attend que les trois outboxes soient
        // enregistrées avant d'envoyer.
        let mut waited = Duration::ZERO;
        while server.connected_count() < 3 && waited < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
            waited += Duration::from_millis(10);
        }
        assert_eq!(server.connected_count(), 3);

        // Cible A et B — C joue le rôle d'un client d'un AUTRE salon.
        server.send_to_many(
            &ids[..2],
            &ServerMsg::Event(protocol::GameEvent::WaveStart { wave: 7 }),
        );

        for client in [&a, &b] {
            let msg = client.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(
                msg,
                ServerMsg::Event(protocol::GameEvent::WaveStart { wave: 7 })
            );
        }
        assert!(
            c.inbox.recv_timeout(Duration::from_millis(300)).is_err(),
            "un client hors de la liste de destinataires ne doit rien recevoir \
             (sinon, fuite inter-salons)"
        );
    }

    /// Inbox bornée (audit 2026-07) : quand le thread principal ne draine plus
    /// l'inbox, les messages excédentaires sont jetés (pas d'accumulation
    /// mémoire sans borne) **sans** couper la connexion ni bloquer la pompe —
    /// et le flux reprend dès que l'inbox est drainée. Capacité minuscule via
    /// `start_inner` pour saturer sans envoyer 4096 messages réels.
    #[test]
    fn a_full_inbox_drops_excess_messages_without_killing_the_connection() {
        let capacity = 2;
        let server = NetServer::start_inner("127.0.0.1:0", MAX_CONNECTIONS_PER_IP, capacity)
            .expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let client = NetClient::connect(&url, "Patient", None).expect("connexion du client");
        client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu");

        // L'inbox n'est PAS drainée : le `Join` relayé occupe déjà un créneau.
        // Dix `Input` (très en-deçà du rate limit) : au plus `capacity`
        // messages au total peuvent tenir, le reste doit être jeté.
        let input = ClientMsg::Input {
            move_x: 1.0,
            move_y: 0.0,
            aim_yaw: 0.0,
            attack: false,
            jump: false,
            fire: false,
            weapon: 0,
            heal: false,
        };
        for _ in 0..10 {
            client.send(&input);
        }
        std::thread::sleep(Duration::from_millis(300));

        let mut queued = 0;
        while server.inbox.try_recv().is_ok() {
            queued += 1;
        }
        assert!(
            queued <= capacity,
            "l'inbox bornée ne doit jamais retenir plus de {capacity} messages : {queued}"
        );

        // La connexion a survécu au débordement…
        assert!(
            client.is_alive(),
            "un débordement d'inbox ne coupe pas le client"
        );
        assert_eq!(server.connected_count(), 1);

        // …et le flux reprend maintenant que l'inbox est vide.
        client.send(&input);
        let (_, msg) = server
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("les messages doivent circuler à nouveau après drainage");
        assert_eq!(msg, input);
    }

    /// `start_with_ip_cap` doit accepter plus de clients d'une même IP que le
    /// plafond de production — c'est ce qui rend `examples/load_test_client.rs`
    /// (16 bots depuis 127.0.0.1) de nouveau exécutable, sans affaiblir le
    /// garde-fou de `start` (couvert par
    /// `per_ip_connection_limit_caps_simultaneous_sockets`).
    #[test]
    fn a_custom_ip_cap_allows_more_local_clients_than_production() {
        let over_default = MAX_CONNECTIONS_PER_IP + 2;
        let server = NetServer::start_with_ip_cap("127.0.0.1:0", over_default)
            .expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let mut clients = Vec::new();
        for i in 0..over_default {
            let c = NetClient::connect(&url, &format!("Bot{i}"), None)
                .unwrap_or_else(|e| panic!("connexion {i} attendue sous le plafond élargi : {e}"));
            c.inbox
                .recv_timeout(Duration::from_secs(2))
                .expect("Welcome attendu sous le plafond élargi");
            clients.push(c);
        }
    }

    /// Un client d'une autre version de protocole doit recevoir un
    /// `JoinRejected` avec une raison intelligible (« mettez le jeu à jour »)
    /// puis voir sa connexion fermée — pas l'ancien silence radio. Forge le
    /// `Join` à la main via une connexion tungstenite brute : `NetClient`
    /// envoie toujours la bonne version, il ne peut pas simuler ce cas. Le
    /// contre-test (version correcte → `Welcome`) est déjà couvert par
    /// `client_joins_and_server_receives_its_input`.
    #[test]
    fn an_outdated_client_is_rejected_with_a_clear_reason() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime de test");
        let reason = rt.block_on(async {
            let (ws, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("connexion brute");
            let (mut sink, mut stream) = ws.split();
            let join = protocol::encode(&ClientMsg::Join {
                protocol: 999,
                name: "TropVieux".to_string(),
                firebase_uid: None,
                lobby: String::new(),
                class: 0,
                objective: 0,
            })
            .expect("encodage");
            sink.send(Message::Binary(join.into()))
                .await
                .expect("envoi du Join forgé");
            while let Some(Ok(msg)) = stream.next().await {
                if let Message::Binary(bytes) = msg {
                    let decoded: ServerMsg = protocol::decode(&bytes).expect("ServerMsg");
                    let ServerMsg::JoinRejected { reason } = decoded else {
                        panic!("JoinRejected attendu, reçu {decoded:?}");
                    };
                    return reason;
                }
            }
            panic!("connexion fermée sans JoinRejected");
        });
        assert!(
            reason.contains("incompatible"),
            "la raison doit être intelligible : {reason}"
        );
        assert_eq!(
            server.connected_count(),
            0,
            "un client rejeté ne doit jamais être compté comme connecté"
        );
    }

    /// Une coupure décidée côté serveur (`NetServer::disconnect`, même chemin
    /// interne qu'une perte de connexion réelle) doit être **détectable** par
    /// le client via `is_alive()` — c'est la brique sur laquelle repose la
    /// reconnexion automatique (`app::network_client`). Avant `is_alive()`, le
    /// client n'avait aucun moyen de savoir que sa connexion était morte.
    #[test]
    fn a_server_side_disconnect_is_detected_by_the_client() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let client = NetClient::connect(&url, "Testeur", None).expect("connexion du client");
        let welcome = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu");
        let ServerMsg::Welcome { player_id } = welcome else {
            panic!("premier message attendu : Welcome, reçu {welcome:?}");
        };
        assert!(client.is_alive(), "transport vivant après le Welcome");

        server.disconnect(player_id);

        let mut waited = Duration::ZERO;
        while client.is_alive() && waited < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
            waited += Duration::from_millis(10);
        }
        assert!(
            !client.is_alive(),
            "le client doit détecter la fermeture de sa connexion"
        );
    }

    /// Une socket qui se ferme (client droppé, perte réseau) doit prévenir le
    /// thread principal par un `Leave` synthétique **immédiat** — sans lui,
    /// l'avatar du joueur resterait dans la partie jusqu'au timeout applicatif
    /// (60 s, cf. `src/bin/server.rs::CLIENT_TIMEOUT`).
    #[test]
    fn a_closed_socket_sends_a_synthetic_leave_to_the_main_thread() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let client = NetClient::connect(&url, "Éphémère", None).expect("connexion du client");
        let welcome = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu");
        let ServerMsg::Welcome { player_id } = welcome else {
            panic!("premier message attendu : Welcome, reçu {welcome:?}");
        };
        // Le Join relayé arrive en premier dans l'inbox serveur.
        let (join_id, _) = server
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        assert_eq!(join_id, player_id);

        drop(client); // fermeture abrupte de la socket, sans Leave volontaire

        let (id, msg) = server
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Leave synthétique attendu à la fermeture de la socket");
        assert_eq!(id, player_id);
        assert_eq!(msg, ClientMsg::Leave);
    }

    /// Sprint 113c : un client qui enchaîne les messages au-delà de
    /// `MAX_MESSAGES_PER_SEC` dans la fenêtre d'une seconde doit être coupé
    /// proprement (pas de panic serveur, juste une déconnexion), pas laissé libre
    /// de continuer à flooder indéfiniment.
    #[test]
    fn flooding_messages_disconnects_the_client_cleanly() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let client = NetClient::connect(&url, "Flooder", None).expect("connexion du client");
        client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("le client doit recevoir un Welcome");
        server
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("le serveur doit recevoir le Join");

        let mut waited = Duration::ZERO;
        while server.connected_count() < 1 && waited < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
            waited += Duration::from_millis(10);
        }
        assert_eq!(server.connected_count(), 1);

        // Bien au-delà de MAX_MESSAGES_PER_SEC (120), enchaînés sans pause : sur
        // localhost, largement sous la seconde de la fenêtre de rate limiting.
        for _ in 0..(MAX_MESSAGES_PER_SEC * 3) {
            client.send(&ClientMsg::Input {
                move_x: 0.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
            });
        }

        let mut waited = Duration::ZERO;
        while server.connected_count() > 0 && waited < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
            waited += Duration::from_millis(10);
        }
        assert_eq!(
            server.connected_count(),
            0,
            "le serveur doit avoir coupé la connexion qui a floodé"
        );
    }

    /// Sprint 113c : au-delà de `MAX_CONNECTIONS_PER_IP` connexions simultanées
    /// depuis la même adresse, les suivantes doivent être refusées (garde-fou
    /// anti-DoS basique) au lieu d'être acceptées sans limite.
    #[test]
    fn per_ip_connection_limit_caps_simultaneous_sockets() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let mut clients = Vec::new();
        for i in 0..MAX_CONNECTIONS_PER_IP {
            let c = NetClient::connect(&url, &format!("Client{i}"), None)
                .unwrap_or_else(|e| panic!("connexion {i} attendue sous le plafond : {e}"));
            c.inbox
                .recv_timeout(Duration::from_secs(2))
                .expect("Welcome attendu sous le plafond");
            clients.push(c);
        }

        let mut waited = Duration::ZERO;
        while server.connected_count() < MAX_CONNECTIONS_PER_IP && waited < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
            waited += Duration::from_millis(10);
        }
        assert_eq!(server.connected_count(), MAX_CONNECTIONS_PER_IP);

        // La connexion suivante, toujours depuis 127.0.0.1, dépasse le plafond : soit
        // la poignée de main échoue (TCP fermé avant le handshake WS), soit elle
        // n'obtient jamais de Welcome — dans les deux cas, le nombre de clients
        // effectivement connectés côté serveur ne doit pas dépasser le plafond.
        if let Ok(over_limit) = NetClient::connect(&url, "OverLimit", None) {
            let got_welcome = over_limit
                .inbox
                .recv_timeout(Duration::from_millis(500))
                .is_ok();
            assert!(
                !got_welcome,
                "une connexion au-delà du plafond par IP ne doit pas recevoir de Welcome"
            );
        }
        assert_eq!(
            server.connected_count(),
            MAX_CONNECTIONS_PER_IP,
            "le plafond par IP ne doit jamais être dépassé côté serveur"
        );
    }
}
