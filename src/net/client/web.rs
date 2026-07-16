//! Transport WebSocket côté client — web (Sprint 116), `web_sys::WebSocket`.
//! Cf. la doc de `super` pour la différence de fond avec `native` (connexion
//! non bloquante : `connect` réussit dès que l'URL est syntaxiquement valide,
//! l'échec réel arrive plus tard via `is_connected()`/`net_status`).
//!
//! Les closures JS (`onopen`/`onmessage`/`onclose`/`onerror`) doivent rester en
//! vie aussi longtemps que le `WebSocket` peut encore les appeler : `NetClient`
//! les garde donc comme champs (`_on*`), jamais juste posées puis oubliées —
//! un `Closure` droppé libère son étoile côté JS, tout appel ultérieur du
//! navigateur planterait.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, channel};

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, ErrorEvent, MessageEvent, WebSocket};

use super::super::protocol::{self, ClientMsg, ServerMsg};

/// Connexion réseau côté client à un salon RusteeGear, portage web.
pub struct NetClient {
    /// Messages reçus du serveur, à consommer par la boucle de jeu (non bloquant :
    /// `try_recv` une fois par frame) — même contrat que côté natif.
    pub inbox: Receiver<ServerMsg>,
    ws: WebSocket,
    /// Vrai dès l'événement `open` du navigateur — avant ça, `send` mettrait en
    /// échec silencieusement côté navigateur (WebSocket pas encore `OPEN`).
    open: Rc<RefCell<bool>>,
    /// Vrai dès `onclose`/`onerror` — support du contrat `is_alive()` commun aux
    /// deux transports (cf. `super`). Indispensable ici : contrairement au natif
    /// (où la fin du thread de fond ferme `outbox`), le clone d'`in_tx` capturé
    /// par `onmessage` vit aussi longtemps que `NetClient` lui-même — `inbox` ne
    /// passera donc **jamais** `Disconnected`, seul ce drapeau permet de savoir
    /// que la connexion est morte. Couvre aussi l'échec de connexion initiale :
    /// `connect` ne bloque jamais sur cette cible (cf. doc de `connect`), un
    /// serveur injoignable se manifeste par un `onerror`/`onclose` différé.
    closed: Rc<RefCell<bool>>,
    /// `Join` encodé, envoyé dès l'ouverture (posé avant que la connexion ne soit
    /// établie, comme côté natif — `outbox` y jouait ce rôle là-bas).
    _on_open: Closure<dyn FnMut()>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

impl NetClient {
    /// Se connecte à `url` (ex. `"ws://127.0.0.1:7777"` ou `"wss://…"` — obligatoire
    /// depuis une page servie en HTTPS, le navigateur refuse un WebSocket non
    /// chiffré sur une origine sécurisée) et envoie `ClientMsg::Join` dès l'ouverture.
    /// **Ne bloque jamais** (cf. la doc de `super`) : contrairement à
    /// `native::NetClient::connect`, un `Ok` ici ne garantit pas que le serveur a
    /// répondu, seulement que l'URL est syntaxiquement valide et que le navigateur a
    /// accepté d'ouvrir la connexion.
    pub fn connect(
        url: &str,
        name: &str,
        firebase_uid: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::connect_to_lobby(url, name, firebase_uid, protocol::DEFAULT_LOBBY)
    }

    /// Comme `connect`, mais rejoint le salon `lobby` plutôt que le salon
    /// partagé par défaut.
    pub fn connect_to_lobby(
        url: &str,
        name: &str,
        firebase_uid: Option<&str>,
        lobby: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Sprint 113f : un navigateur refuse d'ouvrir un WebSocket `ws://` (non
        // chiffré) depuis une page servie en `https:` — `WebSocket::new` lève une
        // exception JS (« An insecure WebSocket connection may not be initiated
        // from a page loaded over HTTPS ») que `map_err` ci-dessous transformerait
        // en message technique peu clair. Détectée ici en amont pour un message
        // explicite, avant même de tenter la connexion.
        if let Some(win) = web_sys::window()
            && let Ok(protocol) = win.location().protocol()
            && protocol == "https:"
            && url.starts_with("ws://")
        {
            return Err(format!(
                "Connexion refusée : « {url} » n'est pas chiffré (ws://), mais cette page \
                 est servie en HTTPS — utilisez une adresse wss:// (le navigateur refuse \
                 tout WebSocket non chiffré depuis une page HTTPS)."
            )
            .into());
        }
        let ws = WebSocket::new(url).map_err(|e| js_error_to_string(&e))?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let join = protocol::encode(&ClientMsg::Join {
            name: name.to_string(),
            firebase_uid: firebase_uid.map(str::to_string),
            lobby: lobby.to_string(),
        })?;

        let (in_tx, in_rx) = channel::<ServerMsg>();
        let open = Rc::new(RefCell::new(false));
        let closed = Rc::new(RefCell::new(false));

        // `onopen` : le navigateur n'autorise `send` qu'une fois la poignée de main
        // WebSocket terminée — `Join` est donc envoyé ici, pas avant (contrairement
        // au thread natif, où le message était mis en file avant même la connexion).
        let ws_for_open = ws.clone();
        let open_flag = open.clone();
        let on_open = Closure::<dyn FnMut()>::new(move || {
            *open_flag.borrow_mut() = true;
            if let Err(e) = ws_for_open.send_with_u8_array(&join) {
                log::error!(
                    "Multijoueur (web) : envoi de Join échoué : {}",
                    js_error_to_string(&e)
                );
            }
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let Ok(buf) = e.data().dyn_into::<js_sys::ArrayBuffer>() else {
                // Le serveur n'envoie que du binaire (`protocol::encode`) — un
                // message texte signalerait un bug côté serveur, pas une entrée
                // utilisateur à valider ici.
                log::warn!("Multijoueur (web) : message non-binaire ignoré");
                return;
            };
            let bytes = js_sys::Uint8Array::new(&buf).to_vec();
            match protocol::decode::<ServerMsg>(&bytes) {
                Ok(msg) => {
                    let _ = in_tx.send(msg);
                }
                Err(e) => log::warn!("Multijoueur (web) : message illisible : {e}"),
            }
        });
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let closed_on_error = closed.clone();
        let on_error = Closure::<dyn FnMut(ErrorEvent)>::new(move |e: ErrorEvent| {
            log::warn!("Multijoueur (web) : erreur WebSocket : {}", e.message());
            *closed_on_error.borrow_mut() = true;
        });
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        let closed_on_close = closed.clone();
        let on_close = Closure::<dyn FnMut(CloseEvent)>::new(move |e: CloseEvent| {
            log::info!(
                "Multijoueur (web) : connexion fermée (code {}, « {} »)",
                e.code(),
                e.reason()
            );
            *closed_on_close.borrow_mut() = true;
        });
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        Ok(Self {
            inbox: in_rx,
            ws,
            open,
            closed,
            _on_open: on_open,
            _on_message: on_message,
            _on_error: on_error,
            _on_close: on_close,
        })
    }

    /// Envoie un message au serveur. Silencieusement ignoré si la connexion n'est
    /// pas encore ouverte (avant `onopen`) ou déjà fermée — même tolérance que le
    /// canal `outbox` côté natif, qui jette aussi les envois une fois le thread
    /// réseau terminé.
    pub fn send(&self, msg: &ClientMsg) {
        if !*self.open.borrow() {
            return;
        }
        if let Ok(bytes) = protocol::encode(msg)
            && let Err(e) = self.ws.send_with_u8_array(&bytes)
        {
            log::warn!(
                "Multijoueur (web) : envoi échoué : {}",
                js_error_to_string(&e)
            );
        }
    }

    /// `true` tant que le transport est vivant — contrat commun natif/web (cf.
    /// `super` et le champ `closed` pour le pourquoi de ce drapeau côté web).
    pub fn is_alive(&self) -> bool {
        !*self.closed.borrow()
    }
}

impl Drop for NetClient {
    /// Ferme proprement la connexion : sans ça, le navigateur la garde ouverte
    /// jusqu'au timeout du serveur (le `WebSocket` JS survit à son wrapper Rust
    /// tant que rien ne l'y force explicitement).
    fn drop(&mut self) {
        let _ = self.ws.close();
    }
}

fn js_error_to_string(e: &wasm_bindgen::JsValue) -> String {
    e.as_string()
        .or_else(|| {
            e.dyn_ref::<js_sys::Error>()
                .map(|err| String::from(err.message()))
        })
        .unwrap_or_else(|| "erreur inconnue".to_string())
}
