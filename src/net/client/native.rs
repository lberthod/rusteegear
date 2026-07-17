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

use std::sync::mpsc::{Receiver, channel};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::super::protocol::{self, ClientMsg, ServerMsg};

/// Connexion réseau côté client à un salon RusteeGear.
pub struct NetClient {
    /// Messages reçus du serveur, à consommer par la boucle de jeu (non bloquant :
    /// `try_recv` une fois par frame).
    pub inbox: Receiver<ServerMsg>,
    outbox: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
}

impl NetClient {
    /// Se connecte à `url` (ex. `"ws://127.0.0.1:7777"`) et envoie immédiatement un
    /// `ClientMsg::Join`. Bloquant le temps de la connexion TCP/WebSocket initiale
    /// (raisonnable au lancement/join d'un salon, pas dans la boucle de rendu).
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
        Self::connect_to_lobby(url, name, firebase_uid, protocol::DEFAULT_LOBBY)
    }

    /// Comme `connect`, mais rejoint le salon `lobby` plutôt que le salon
    /// partagé par défaut (créé à la demande côté serveur s'il n'existe pas
    /// encore, cf. `bin/server.rs::Room`).
    pub fn connect_to_lobby(
        url: &str,
        name: &str,
        firebase_uid: Option<&str>,
        lobby: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (in_tx, in_rx) = channel::<ServerMsg>();
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let join = protocol::encode(&ClientMsg::Join {
            protocol: protocol::PROTOCOL_VERSION,
            name: name.to_string(),
            firebase_uid: firebase_uid.map(str::to_string),
            lobby: lobby.to_string(),
            // Sélection de classe (GAMEDESIGN_MMORPG.md §3.2) pas encore
            // câblée à une UI — Assaut (0) pour tous, zéro régression tant
            // qu'aucun sélecteur n'existe (cf. `net::protocol::ClientMsg::Join::class`).
            class: 0,
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
                let (ws, _response) = match tokio_tungstenite::connect_async(&url).await {
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

        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => return Err("le thread réseau s'est arrêté avant la connexion".into()),
        }

        Ok(Self {
            inbox: in_rx,
            outbox: out_tx,
        })
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
        let c = super::NetClient::connect("wss://ws.loicberthod.ch", "TestTLS", None);
        match c {
            Ok(_) => println!("OK: connexion wss établie"),
            Err(e) => panic!("échec wss: {e}"),
        }
    }
    /// Frapper la façade HTTPS en `ws://` non chiffré donne le 308 de Caddy,
    /// enrichi de l'indice « remplacez ws:// par wss:// ».
    #[test]
    #[ignore]
    fn ws_308_hint() {
        let e = match super::NetClient::connect("ws://ws.loicberthod.ch", "TestTLS", None) {
            Ok(_) => panic!("aurait dû échouer en ws:// (308 attendu)"),
            Err(e) => e.to_string(),
        };
        println!("erreur: {e}");
        assert!(e.contains("wss://"));
    }
}
