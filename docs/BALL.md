# Ball — jeu VR (Meta Quest 3) + adversaire sur ordinateur

Code : `src/xr/ball/` (jeu, rendu dédié, physique, mains, parcours, réseau, relais).

## Jouer

| Qui | Comment |
|---|---|
| Joueur VR | APK « Ball » (`com.berthod.ball`, 90 Hz) : `VR_HZ=90 VR_SCENE=balles BUNDLE_ID=com.berthod.ball APP_NAME="Ball" ./packaging/build_quest.sh` |
| Joueur PC | `cargo run --release --bin ball_pc` (ou le binaire `target/release/ball_pc`) |

Le casque joue seul ; dès qu'un ordinateur se connecte, il apparaît dans la scène (avatar bleu)
et peut poser des murs (2 au plus, 8 s chacun, 2 s de recharge) qui arrêtent les boules — jamais
les cubes. Le casque compte les « tirs arrêtés ».

Commandes PC : ZQSD/WASD, clic droit maintenu ou flèches pour regarder, clic gauche ou Espace
pour poser un mur, Échap pour quitter.

## Réseau

- Le casque est maître (physique, score) : il envoie la scène 20 fois/s (`net::Snapshot`), le PC
  renvoie position + murs 30 fois/s (`net::PcState`). Messages `bincode` en trames WebSocket.
- Relais `src/bin/ball_relay.rs` : une place VR + une place PC, le nouveau venu remplace
  l'ancien, signal de présence toutes les 2 s.
- VPS : service systemd `ball-relay` (`127.0.0.1:7790`, binaire dans
  `~/rusteegear-server/target/release/ball_relay`), exposé par Caddy en
  `wss://ws.loicberthod.ch/ball` (bloc `ws.loicberthod.ch` : `handle /ball*` → 7790, le reste →
  serveur de jeu 7777 ; sauvegarde `Caddyfile.bak.20260924-200442`).
- Mise à jour du relais : `git pull` + `cargo build --release --bin ball_relay` dans
  `~/rusteegear-server`, puis `sudo systemctl restart ball-relay`. Changer le format des messages
  = incrémenter `net::PROTOCOL_VERSION` et redistribuer APK + PC.
- Local : `cargo run --release --bin ball_relay` puis `BALL_URL=ws://127.0.0.1:7790/ball` pour
  `quest_sim --scene balles` et `ball_pc`. `BALL_OFFLINE=1` : casque sans réseau.

## Vérifier sans casque

```bash
QUEST_SIM_BALL=2.2 QUEST_SIM_FRAMES=400 ./target/release/quest_sim --snapshot vr.png --scene balles &
./target/release/ball_pc --snapshot pc.png --wall --linger 8
```
