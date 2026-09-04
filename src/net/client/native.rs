//! Transport WebSocket côté client — desktop/Android (`tokio`+`tokio_tungstenite`).
//! Cf. `super` pour le pourquoi de ce découpage natif/web.
//!
//! Même schéma que `server_loop` : un thread de fond dédié pousse les
//! `ServerMsg` reçus dans `inbox` (canal `std::sync::mpsc`), et `send` encode
//! un `ClientMsg` vers le serveur. La boucle `winit` n'a qu'à `try_recv()` sur
//! `inbox` une fois par frame, exactement comme elle le fait déjà pour les
//! imports glTF ou les réponses IA asynchrones.
//!
//! **Runtime `current_thread`** : une connexion réseau n'a besoin que d'un
//! thread pour attendre les octets qui arrivent, pas d'un pool de threads
//! ouvriers — `tokio::runtime::Runtime::new()` construit par défaut un
//! runtime **multi-thread** (un ouvrier par CPU logique, cf. docs/audits/
//! net.md pour le coût constaté). Un `current_thread` n'a pas de thread
//! ouvrier propre : il ne progresse que pendant qu'un thread appelle
//! `block_on` dessus — d'où le thread dédié ci-dessous, qui `block_on` la
//! boucle de vie entière de la connexion (pas seulement la connexion
//! initiale).

use std::cell::RefCell;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::super::protocol::{self, ClientMsg, ServerMsg};
use super::{CONNECT_TIMEOUT, CONNECT_TIMEOUT_REASON, Handshake};

/// Connexion réseau côté client à un salon RusteeGear.
pub struct NetClient {
    /// Messages reçus du serveur, à consommer par la boucle de jeu (non bloquant :
    /// `try_recv` une fois par frame).
    pub inbox: Receiver<ServerMsg>,
    outbox: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// Verdict de la poignée de main, poussé une seule fois par le thread
    /// réseau (roadmap post-audit UX v2 2026-09-04, 2.1) — sondé sans bloquer
    /// par `handshake()`, qui mémorise le résultat dans `handshake`.
    ready: Receiver<Result<(), String>>,
    /// Verdict mémorisé une fois reçu (`None` = encore en attente) : le canal
    /// ne livre son message qu'une fois, l'état doit survivre aux appels
    /// suivants.
    handshake: RefCell<Option<Handshake>>,
}

impl NetClient {
    /// Se connecte à `url` (ex. `"ws://127.0.0.1:7777"`) et envoie un
    /// `ClientMsg::Join` dès la socket ouverte. **Ne bloque pas** (roadmap
    /// post-audit UX v2 2026-09-04, 2.1) : rend la main dès la connexion
    /// lancée, cf. `handshake()` pour en suivre l'issue (et `wait_ready` pour
    /// les outils en ligne de commande qui préfèrent attendre).
    /// `firebase_uid` : `uid` obtenu par `net::firebase::sign_in`/`sign_up`, si le
    /// joueur s'est connecté avant de rejoindre ; `None` pour une partie
    /// locale/anonyme. Rejoint `protocol::DEFAULT_LOBBY` (le salon partagé par
    /// défaut) — cf. `connect_to_lobby` pour choisir un autre salon (cf.
    /// GAMEDESIGN_EN_LIGNE.md §3.3).
    pub fn connect(
        url: &str,
        name: &str,
        firebase_uid: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Classe fixée à Assaut (0) pour cet appel simple — cf.
        // `connect_to_lobby` pour choisir une classe (Sprint 3,
        // `sprint10audit.md`), utilisé par la fenêtre Multijoueur.
        Self::connect_to_lobby(url, name, firebase_uid, protocol::DEFAULT_LOBBY, 0, 0)
    }

    /// Comme `connect`, mais rejoint le salon `lobby` plutôt que le salon
    /// partagé par défaut (créé à la demande côté serveur s'il n'existe pas
    /// encore, cf. `bin/server.rs::Room`), choisit une `class` (cf.
    /// `net::protocol::ClientMsg::Join::class`, Sprint 3) et un `objective`
    /// (`RoundObjective::to_u8`, Sprint 21, `sprintreflecion.md` — seul le
    /// premier joueur à rejoindre un salon vide en fait foi côté serveur,
    /// `bin/server.rs::Lobby::objective`).
    pub fn connect_to_lobby(
        url: &str,
        name: &str,
        firebase_uid: Option<&str>,
        lobby: &str,
        class: u8,
        objective: u8,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (in_tx, in_rx) = channel::<ServerMsg>();
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let join = protocol::encode(&ClientMsg::Join {
            protocol: protocol::PROTOCOL_VERSION,
            name: name.to_string(),
            firebase_uid: firebase_uid.map(str::to_string),
            lobby: lobby.to_string(),
            class,
            objective,
        })?;
        // Mis en file avant même que le thread de fond n'existe : la pompe
        // sortante le trouvera prêt dès sa première itération.
        out_tx.send(join)?;

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let url = url.to_string();
        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            runtime.block_on(async move {
                // Poignée de main bornée (roadmap post-audit UX v2 2026-09-04,
                // 2.1) : sans ce délai, une IP filtrée (paquets SYN jetés sans
                // réponse) laissait `connect_async` attendre le timeout TCP du
                // système, ~75 s — et le thread de rendu avec lui, à l'époque
                // où `connect` bloquait dessus.
                let connected = match tokio::time::timeout(
                    CONNECT_TIMEOUT,
                    tokio_tungstenite::connect_async(&url),
                )
                .await
                {
                    Ok(res) => res,
                    Err(_elapsed) => {
                        let _ = ready_tx.send(Err(CONNECT_TIMEOUT_REASON.to_string()));
                        return;
                    }
                };
                let (ws, _response) = match connected {
                    Ok(v) => v,
                    Err(e) => {
                        let mut msg = e.to_string();
                        // Un « 308 Permanent Redirect » sur du ws:// est la
                        // signature d'une façade HTTPS (ex. Caddy) qui redirige
                        // le HTTP en clair vers le HTTPS — tungstenite ne suit
                        // pas les redirections, donc on guide l'utilisateur.
                        if url.starts_with("ws://") && msg.contains("308") {
                            msg.push_str(
                                " — ce serveur exige une connexion chiffrée : \
                                 remplacez ws:// par wss:// dans l'adresse",
                            );
                        }
                        let _ = ready_tx.send(Err(msg));
                        return;
                    }
                };
                // Même raison que côté serveur (`server_loop.rs`) : sans ça,
                // l'algorithme de Nagle retarde nos petites trames fréquentes
                // (`Input` à chaque frame) de plusieurs dizaines de ms.
                if let Err(e) = ws.get_ref().get_ref().set_nodelay(true) {
                    log::warn!("TCP_NODELAY impossible côté client : {e}");
                }
                let _ = ready_tx.send(Ok(()));
                let (mut sink, mut stream) = ws.split();

                let outbound = async {
                    while let Some(bytes) = out_rx.recv().await {
                        if sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                };
                let inbound = async {
                    while let Some(Ok(msg)) = stream.next().await {
                        if let Message::Binary(bytes) = msg
                            && let Ok(server_msg) = protocol::decode::<ServerMsg>(&bytes)
                            && in_tx.send(server_msg).is_err()
                        {
                            break;
                        }
                    }
                };
                tokio::select! {
                    _ = outbound => {}
                    _ = inbound => {}
                }
            });
            // Le thread se termine naturellement ici une fois la connexion
            // close : pas de nettoyage explicite à faire, `out_tx` (côté
            // `NetClient`) est la seule source de `out_rx`, qui se ferme
            // d'elle-même quand `NetClient` est droppé.
        });

        Ok(Self {
            inbox: in_rx,
            outbox: out_tx,
            ready: ready_rx,
            handshake: RefCell::new(None),
        })
    }

    /// Issue de la poignée de main, sans bloquer (contrat commun natif/web,
    /// cf. `super`) : `Pending` tant que le thread réseau n'a rien dit,
    /// puis `Open` ou `Failed(raison)` — mémorisé, les appels suivants
    /// rendent le même verdict.
    pub fn handshake(&self) -> Handshake {
        if let Some(h) = self.handshake.borrow().as_ref() {
            return h.clone();
        }
        let verdict = match self.ready.try_recv() {
            Ok(Ok(())) => Handshake::Open,
            Ok(Err(e)) => Handshake::Failed(e),
            Err(std::sync::mpsc::TryRecvError::Empty) => return Handshake::Pending,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Handshake::Failed("le thread réseau s'est arrêté avant la connexion".to_string())
            }
        };
        *self.handshake.borrow_mut() = Some(verdict.clone());
        verdict
    }

    /// Attend (en bloquant, au plus `timeout`) la fin de la poignée de main —
    /// pour les outils en ligne de commande (`examples/smoke_vps.rs`,
    /// `load_test_client.rs`) et les tests, qui n'ont pas de boucle de
    /// rendu à protéger. `Err(raison)` sur échec ou délai dépassé. Jamais
    /// appelé par le jeu lui-même, qui sonde `handshake()` frame après frame.
    pub fn wait_ready(&self, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match self.handshake() {
                Handshake::Open => return Ok(()),
                Handshake::Failed(e) => return Err(e),
                Handshake::Pending => {
                    if std::time::Instant::now() >= deadline {
                        return Err(CONNECT_TIMEOUT_REASON.to_string());
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    /// Envoie un message au serveur (non bloquant : mis en file, transmis par le
    /// thread réseau de fond).
    pub fn send(&self, msg: &ClientMsg) {
        if let Ok(bytes) = protocol::encode(msg) {
            let _ = self.outbox.send(bytes);
        }
    }

    /// `true` tant que le transport est vivant — contrat commun natif/web (cf.
    /// `super`). Ici : le thread de fond `block_on` la connexion entière et ne
    /// se termine qu'à sa fermeture (volontaire, perte réseau, serveur coupé) ;
    /// sa fin droppe `out_rx`, seule autre extrémité de `outbox` — `is_closed()`
    /// devient alors vrai sans aucun état supplémentaire à entretenir. Sans ce
    /// test, un client dont la connexion est morte continuait de `send()` dans
    /// un canal fermé en se croyant connecté pour toujours (cf.
    /// `AppState::is_connected`).
    pub fn is_alive(&self) -> bool {
        !self.outbox.is_closed()
    }
}

/// Tests-preuves du support TLS natif (`wss://`, feature `rustls-tls-webpki-roots`
/// de `tokio-tungstenite`, cf. Cargo.toml). `#[ignore]` : ils dépendent du VPS
/// réel (`ws.loicberthod.ch`) et du réseau — à lancer à la main :
/// `cargo test --lib tls_proof -- --ignored --nocapture`.
#[cfg(test)]
mod tls_proof {
    /// Le client natif ouvre bien une connexion chiffrée vers la façade Caddy.
    #[test]
    #[ignore]
    fn wss_vps() {
        let c = super::NetClient::connect("wss://ws.loicberthod.ch", "TestTLS", None)
            .expect("lancement de la connexion");
        match c.wait_ready(std::time::Duration::from_secs(10)) {
            Ok(()) => println!("OK: connexion wss établie"),
            Err(e) => panic!("échec wss: {e}"),
        }
    }
    /// Frapper la façade HTTPS en `ws://` non chiffré donne le 308 de Caddy,
    /// enrichi de l'indice « remplacez ws:// par wss:// ».
    #[test]
    #[ignore]
    fn ws_308_hint() {
        let c = super::NetClient::connect("ws://ws.loicberthod.ch", "TestTLS", None)
            .expect("lancement de la connexion");
        let e = match c.wait_ready(std::time::Duration::from_secs(10)) {
            Ok(()) => panic!("aurait dû échouer en ws:// (308 attendu)"),
            Err(e) => e,
        };
        println!("erreur: {e}");
        assert!(e.contains("wss://"));
    }
}

/// Poignée de main non bloquante et bornée (roadmap post-audit UX v2
/// 2026-09-04, 2.1) — vrai socket, derrière `net_tests` comme les autres.
#[cfg(all(test, feature = "net_tests"))]
mod handshake_tests {
    use std::time::{Duration, Instant};

    use super::super::{CONNECT_TIMEOUT, Handshake};
    use super::NetClient;

    /// Un port fermé : `connect` rend la main tout de suite (jamais bloquant),
    /// et la poignée de main finit `Failed` avec une raison lisible — pas un
    /// `Err` synchrone, pas un `Open` mensonger.
    #[test]
    fn a_refused_connection_fails_the_handshake_without_blocking_connect() {
        // Port réservé par un listener aussitôt fermé : personne n'écoute.
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        let t0 = Instant::now();
        let c = NetClient::connect(&format!("ws://127.0.0.1:{port}"), "Refusé", None)
            .expect("le lancement de la connexion ne peut pas échouer");
        assert!(
            t0.elapsed() < Duration::from_millis(500),
            "connect ne doit plus bloquer le temps de la poignée de main"
        );
        let verdict = c.wait_ready(Duration::from_secs(5));
        let Err(reason) = verdict else {
            panic!("un port fermé doit faire échouer la poignée de main");
        };
        assert!(!reason.is_empty());
        assert!(matches!(c.handshake(), Handshake::Failed(_)));
        assert!(!c.is_alive(), "le transport est mort après l'échec");
    }

    /// Un serveur qui accepte le TCP mais ne répond jamais à la poignée de
    /// main WebSocket (façade gelée, IP filtrée…) : `CONNECT_TIMEOUT` la
    /// tranche avec `CONNECT_TIMEOUT_REASON`, au lieu des ~75 s du timeout
    /// TCP système.
    #[test]
    fn a_silent_server_is_abandoned_after_the_connect_timeout() {
        // Listener jamais `accept`é : la connexion TCP aboutit (backlog du
        // noyau) mais aucune réponse HTTP/WebSocket ne viendra.
        let silent = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("ws://{}", silent.local_addr().expect("addr"));
        let c = NetClient::connect(&url, "Patient", None).expect("lancement");
        assert_eq!(c.handshake(), Handshake::Pending);
        let t0 = Instant::now();
        let verdict = c.wait_ready(CONNECT_TIMEOUT + Duration::from_secs(3));
        let elapsed = t0.elapsed();
        assert_eq!(
            verdict,
            Err(super::CONNECT_TIMEOUT_REASON.to_string()),
            "délai dépassé attendu"
        );
        assert!(
            elapsed >= CONNECT_TIMEOUT - Duration::from_millis(200)
                && elapsed < CONNECT_TIMEOUT + Duration::from_secs(2),
            "le délai doit être celui de CONNECT_TIMEOUT, pas celui du système : {elapsed:?}"
        );
        drop(silent);
    }
}
