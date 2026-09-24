//! Relais de **Ball** à deux joueurs (VR + ordinateur), cf.
//! `motor3derust::xr::ball::relay`. Sur le VPS : service systemd
//! `ball-relay` sur `127.0.0.1:7790`, exposé en `wss://ws.loicberthod.ch/ball`
//! par Caddy. En local : `cargo run --release --bin ball_relay`, puis
//! `BALL_URL=ws://127.0.0.1:7790/ball` pour le casque simulé et le PC.

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let addr = std::env::var("BALL_RELAY_ADDR").unwrap_or_else(|_| "127.0.0.1:7790".into());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime tokio");
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .unwrap_or_else(|e| panic!("écoute sur {addr} impossible : {e}"));
        log::info!("Ball relais : écoute sur {addr}");
        motor3derust::xr::ball::relay::serve(listener).await;
    });
}
