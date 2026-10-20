# Ball — jeu VR (Meta Quest 3) + adversaire sur ordinateur

Code : `src/xr/ball/` (jeu, rendu dédié, physique, mains, parcours, réseau, relais).

## Jouer

| Qui | Comment |
|---|---|
| Joueur VR | APK « Ball » (`com.berthod.ball`, 90 Hz) : `VR_HZ=90 VR_SCENE=balles BUNDLE_ID=com.berthod.ball APP_NAME="Ball" ./packaging/build_quest.sh` |
| Joueur PC | `cargo run --release --bin ball_pc` (ou le binaire `target/release/ball_pc`), puis taper le **code du salon** |

**Salon** : le casque tire un code à 4 chiffres (gardé dans `ball_room.txt`, `BALL_ROOM` pour le
fixer) et l'affiche au menu et au tableau (« En ligne · salon 4821 · … »). Le joueur PC le tape
dans la fenêtre puis Entrée, ou le passe par `--salon 4821` / `BALL_ROOM=4821`. Sans le code,
personne ne peut rejoindre ni éjecter le joueur VR (audit 2026-10-20, 1.7). Les APK d'avant les
salons (protocole 1) jouent toujours, dans un salon public qu'aucun PC à jour ne rejoint :
reconstruire l'APK.

Le casque joue seul ; dès qu'un ordinateur se connecte, il apparaît dans la scène (avatar bleu)
et peut poser des murs (2 au plus, 8 s chacun, 2 s de recharge) qui arrêtent les boules — jamais
les cubes. Le casque compte les « tirs arrêtés ».

Commandes PC : ZQSD/WASD, clic droit maintenu ou flèches pour regarder, clic gauche ou Espace
pour poser un mur, Échap pour quitter.

## Réseau

- Le casque est maître (physique, score) : il envoie la scène 20 fois/s (`net::Snapshot`), le PC
  renvoie position + murs 30 fois/s (`net::PcState`). Messages `bincode` en trames WebSocket.
- Relais `src/bin/ball_relay.rs` : par salon, une place VR + une place PC, le nouveau venu
  remplace l'ancien du même salon, signal de présence toutes les 2 s. Limites : messages de
  256 Kio, 120 messages/s par connexion, 64 connexions, 32 salons. Protocole 2 (`Msg::Join`
  avec le code) ; `Hello` version 1 accepté dans le salon public.
- VPS : service systemd `ball-relay` (`127.0.0.1:7790`, binaire dans
  `~/rusteegear-server/target/release/ball_relay`), exposé par Caddy en
  `wss://ws.loicberthod.ch/ball` (bloc `ws.loicberthod.ch` : `handle /ball*` → 7790, le reste →
  serveur de jeu 7777 ; sauvegarde `Caddyfile.bak.20260924-200442`). Repli **en clair** pour
  les appareils où le TLS échoue : bloc `http://ws.loicberthod.ch` (`/ball` → 7790, le reste
  redirigé vers https ; sauvegarde `Caddyfile.bak.20260924-202518`). Les clients essaient
  `wss://`, et `ws://` **seulement si l'échec est TLS** (certificat, poignée de main) ; une
  connexion en clair s'affiche « non chiffré » au casque et dans le titre de la fenêtre PC.
- Diagnostic sans câble : le casque affiche l'état de la liaison et, hors ligne, la cause exacte
  de l'échec (DNS, TLS, délai…) au menu et au tableau des scores.
- Mise à jour du relais : `git pull` + `cargo build --release --bin ball_relay` dans
  `~/rusteegear-server`, puis `sudo systemctl restart ball-relay`. Changer le format des messages
  = incrémenter `net::PROTOCOL_VERSION` et redistribuer APK + PC.
- Local : `cargo run --release --bin ball_relay` puis `BALL_URL=ws://127.0.0.1:7790/ball` pour
  `quest_sim --scene balles` et `ball_pc`. `BALL_OFFLINE=1` : casque sans réseau.

## Vérifier sans casque

```bash
BALL_ROOM=7342 QUEST_SIM_BALL=2.2 QUEST_SIM_FRAMES=400 ./target/release/quest_sim --snapshot vr.png --scene balles &
./target/release/ball_pc --snapshot pc.png --wall --linger 8 --salon 7342
```
