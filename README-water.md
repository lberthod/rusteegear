# water.loicberthod.ch — « Rivière & cascade », jeu 3D multijoueur PvE/PvP dans le navigateur

Document de passation (14 septembre 2026). Il résume **tout** ce qu'un développeur
doit savoir pour reprendre le projet : ce que c'est, comment c'est construit, où ça
tourne, comment on le déploie, comment on le teste, et ses limites. Le dépôt est
celui du moteur [RusteeGear](README.md) (`motor3derust`) : la démo Rivière en est
une scène et un monde multijoueur parmi d'autres, mais elle a son site et son
salon dédiés.

---

## 1. En une page

| Quoi | Où |
|---|---|
| Site public (client web WebAssembly + WebGPU) | https://water.loicberthod.ch (`?scene=riviere`) |
| Serveur de jeu autoritaire (WebSocket TLS) | `wss://ws.loicberthod.ch` → VPS, port local 7777 |
| Code (dépôt de base, moteur + toutes les démos) | https://github.com/lberthod/rusteegear, branche `main`, crate `motor3derust` |
| Code (dépôt de publication du projet water) | https://github.com/lberthod/water.loicberthod — miroir de `main` de rusteegear, poussé à chaque déploiement |
| Scène | `src/scene/demos/riviere.rs` (`Scene::riviere_demo`) |
| Monde réseau | salon `riviere` (`net::protocol::RIVIERE_LOBBY`) |
| Doc détaillée de la démo | [docs/RIVIERE_CASCADE.md](docs/RIVIERE_CASCADE.md) |
| Dernier état vérifié en production | commit `fa94e8f` (site et serveur), 14 sept. 2026 22h45 |

Ce que voit un joueur : une vallée forestière avec rivière, cascade et bassin,
un personnage « créature ronde » à la troisième personne, six renards enragés à
combattre (PvE), du butin (épée, lance, marteau, baies, potions), et les autres
visiteurs du site dans le même monde, qu'il peut combattre (PvP). Kit de
capacités 1-2-3-4, barre de vie, jauges au-dessus des têtes, mini-carte, liste des
joueurs. Tout tourne dans Chrome/Edge (WebGPU) sans installation.

### Les deux dépôts GitHub

Le projet vit dans le dépôt du moteur **rusteegear** (source de vérité, CI,
historique complet). Le dépôt **water.loicberthod** est un **miroir** de sa
branche `main` créé le 14 septembre 2026 pour que le projet du site ait sa
propre page GitHub : même arborescence, mêmes commits, ce README à la racine.
Il n'y a pas de code propre à water.loicberthod en dehors de rusteegear : toute
modification se fait dans rusteegear puis se pousse sur les deux remotes.

```bash
git clone https://github.com/lberthod/rusteegear.git   # git-lfs installé AVANT (assets .glb)
cd rusteegear
git remote add water https://github.com/lberthod/water.loicberthod.git
git push origin main && git push water main            # publier sur les deux
```

Si vous partez du miroir (`git clone …/water.loicberthod.git`), ajoutez
rusteegear comme remote `origin` pour récupérer les mises à jour du moteur.

---

## 2. Architecture

Un **seul crate Rust** (`motor3derust`, édition 2024, toolchain épinglée
`1.98.0` dans `rust-toolchain.toml`, cible `wasm32-unknown-unknown` incluse) qui
produit :

- **Le client** : `src/lib.rs` (application winit/wgpu/egui) compilé en natif
  (éditeur macOS, APK, IPA) **et** en WebAssembly (`packaging/web/`, mode
  « player » sans éditeur). Rendu wgpu (Metal/Vulkan/WebGPU), UI egui, physique
  rapier3d 0.33, scripts Lua (`mlua` en natif, `rilua` sur le web).
- **Le serveur headless** : `src/bin/server.rs` + `src/net/server_loop.rs`
  (tokio, WebSocket). Il fait tourner la **même** simulation (`AppState`) sans
  rendu, à 60 Hz, et diffuse des snapshots.
- **Le protocole** : `src/net/protocol.rs` (messages sérialisés en `bincode`,
  `PROTOCOL_VERSION = 9`).

Principe clé : **serveur autoritaire, scène partagée par valeur**. Le client et
le serveur construisent chacun `Scene::riviere_demo()` (même code, mêmes assets
embarqués), donc les indices d'objets coïncident et le snapshot ne transporte
que des deltas (position, orientation, visibilité, vie, clip d'animation) pour
les joueurs et les monstres. Le client n'est jamais maître d'un dégât.

### Modules à connaître (`src/`)

| Chemin | Rôle |
|---|---|
| `app/mod.rs` | `AppState` : tout l'état de jeu (scène, physique, joueurs réseau, HUD…). Gros fichier, chercher les `pub struct` |
| `app/simulation.rs` | `advance_play` → `sim_step` à pas fixe 1/60 s : scripts Lua, pilotage joueurs/IA, physique, capacités, animations |
| `app/multiplayer.rs` | joueurs réseau côté serveur : spawn (`spawn_network_player`), attaques (`update_network_attacks`, PvP), ruée, animations de capacités, `network_snapshot`, `WorldKind`, `PlayerClass` |
| `app/health.rs` | vie des joueurs réseau, dégâts (`apply_network_damage`), soin, réanimation Soutien, **réapparition PvP** (`update_network_respawn`), morsures |
| `app/fireball.rs` | tirs à distance (3 armes), impacts monstres/joueurs/décor |
| `app/combat.rs` | attaque locale (solo), ruée locale (`update_dash`) |
| `app/network_client.rs` | côté client : connexion, réception des snapshots, fantômes des autres joueurs (`RemotePlayer`), interpolation |
| `app/ability_hud.rs`, `app/world_labels.rs` | état de la barre de capacités et des jauges au-dessus des têtes (lus par le HUD) |
| `editor/hud.rs`, `editor/mod.rs` | dessin egui du HUD (vie, capacités, jauges monde, roster, mini-carte, bannières) |
| `gfx/` | renderer wgpu, shader `main.wgsl` (eau procédurale `shade_water`, réflexion planaire, ciel/brume), caméra |
| `runtime/physics/` | enveloppe rapier : construction des corps (`build.rs`), contrôleur kinématique joueur, corps scriptés (`control.rs`), requêtes (`query.rs`) |
| `scene/` | modèle de scène (`SceneObject`, `Combat`, `Controller`, `AnimationState`), importeur glTF, démos, scripts Lua générés (`demos/creature_scripts.rs`) |
| `net/` | protocole, boucle serveur, interpolation client, Firebase optionnel |
| `bin/server.rs` | salons (`Room`/`Lobby`), tick 16 ms, relances de manche, garde anti-AFK |

Autres documents utiles : [docs/architecture.md](docs/architecture.md),
[docs/MENTAL_MODEL.md](docs/MENTAL_MODEL.md), [docs/CONTROLS.md](docs/CONTROLS.md),
[docs/LUA_API.md](docs/LUA_API.md), [GDD_MMORPG.md](GDD_MMORPG.md) (game design du
monde « hameau », dont Rivière réutilise les règles réseau),
[docs/KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md).

---

## 3. La scène « Rivière & cascade »

Construite par `Scene::riviere_demo()` (`src/scene/demos/riviere.rs`), ~3 100
objets, 34 modèles importés. Groupes d'objets :

- **Vallée** : terrain (maillage `terrain_vallee.glb`, 48 841 sommets, albédo
  2048², couleurs de sommets) — un seul grand maillage `Static`.
- **Eau** : 4 nappes (`SceneObject::water`, `WaterKind` Rivière/Cascade/Brume)
  rendues par `shade_water` dans `main.wgsl`, avec une passe de réflexion planaire.
- **Forêt / végétation / Faune** : épicéas, hêtres, bouleaux, herbes, fougères,
  rochers, renards décoratifs — générés par script (voir § assets).
- **Monstre** : 6 « Renards enragés » (`fauna_fox.glb` teinté rouge).
- **Butin** : 3 armes de mêlée posées sur des galets (Épée près du départ,
  Lance au plateau amont, Marteau près de la cascade) ; baies ×2 et potions ×2.
- **Enceinte** : 4 murs invisibles autour de la vallée (corps fixes, collider
  actif même masqué) — plus de chute hors carte.
- **Joueur** : gabarit `creature_ronde.glb` (skinné, clips Idle/Walk/Attack/
  Cast/Block/Dash), `PhysicsKind::Kinematic`, contrôleur `input`.

### Assets

Tous dans `assets/models/riviere/`, suivis par **Git LFS** (`.gitattributes` :
`assets/models/**/*.glb`). Ils sont **embarqués dans le wasm** (`embedded://riviere/…`),
d'où les 37 Mo du binaire web. Générés sans Blender :

```bash
python3 scripts/gen_riviere_cascade.py      # terrain + albédo + nappes d'eau (numpy/PIL)
python3 scripts/gen_riviere_vegetation.py   # arbres, herbes, fougères, rochers
cargo run --example gen_riviere_preview --profile dev-fast   # aperçu PNG (GPU requis)
```

Les formules de la vallée (`river_center`, `river_half_width`, `water_level`)
sont **dupliquées** entre le script Python et `riviere.rs` : modifier les deux.
Sources Blender des personnages : `docs/blender/creature_ronde.blend`,
`docs/blender/heroine_griffes.blend`.

⚠️ Sur toute machine qui construit le serveur (VPS compris), `git lfs` doit
être installé **avant** le clone/pull, sinon les `.glb` sont des pointeurs texte
de 130 octets, la scène Rivière échoue à se construire (`Démo rivière : terrain
introuvable` dans les logs) et le salon `riviere` semble vide/solo.

---

## 4. Gameplay

### Contrôles (clavier ; tactile et manette : voir `docs/CONTROLS.md`)

| Touche | Action |
|---|---|
| WASD / flèches, Espace | déplacement, saut |
| **1** ou **J** | coup de mêlée (arme équipée ; G = même action, « griffe ») |
| **2** (tenu) | bouclier : dégâts reçus ×0,25 |
| **3** ou **K** | sort à distance (projectile) |
| **4** | ruée : 5 m en 0,5 s, recharge 1,5 s, bornée par un rayon (pas de traversée de mur) |
| **H** (tenu) | soin de soi 0,16 PV/s (Soutien : soigne aussi les alliés à portée) |
| M, Échap, 0 | carte, pause, son coupé |

Le kit 1-2-3-4 n'existe que dans les scènes avec `Scene::ability_bar = true`
(Rivière). Ailleurs, 1/2/3 changent d'arme à distance.

### Classes (`PlayerClass`, choisies à l'écran d'accueil)

| Classe | Vitesse | Saut | PV max | Spécificité |
|---|---|---|---|---|
| Assaut | ×1,0 | ×1,0 | ×1,0 | référence |
| Éclaireur | ×1,25 | ×1,30 | ×0,70 | déclenche les créatures furtives de loin |
| Soutien | ×0,85 | ×1,0 | ×1,0 | soin/réanimation des alliés, dégâts à distance réduits |

### PvE : les renards enragés

- 3 PV, tués par la mêlée (1 coup = 1 PV sauf armes plus lourdes) ou les tirs
  (Boule de feu 1, Éclair 1, Boulet 3).
- Poursuite native (`AiChaser`, vitesse 2,2, détection 9 m en réseau), morsure
  scriptée en Lua (`creature_bite_script`) : 0,12 PV, recharge 1,8 s, 50 % de
  chance ; contact monstre : 0,16 PV/s.
- Réapparition 20 s après la mort, à leur poste d'origine.
- Frags et assists individualisés (`network_kills`/`network_assists`, diffusés
  dans le snapshot, visibles dans le roster).

### PvP (scènes à kit uniquement — le hameau reste coopératif)

- **Mêlée** : 0,15 PV par coup (`PVP_MELEE_DAMAGE`), portée 2,8 m centre à
  centre (`PVP_MELEE_RANGE` — deux personnages ne peuvent pas s'approcher à
  moins de ≈ 2,1 m), recharge 0,4 s, validée côté serveur.
- **Tir** : impact d'un projectile sur un autre joueur vivant hors grâce
  (`Impact::Player` dans `fireball.rs`).
- **Bouclier** : ×0,25 sur tout dégât reçu, y compris PvP.
- **Grâce d'apparition** 5 s (intouchable), **réapparition** 6 s après la mort
  au point de départ à pleine vie (`update_network_respawn`).
- Pas d'équipes : tout autre joueur est une cible valide. Un joueur mort dans le
  hameau ne réapparaît pas, il attend une réanimation de Soutien (10 s de canal).

La vie est un flottant 0..1 (`MAX_HEALTH = 1.0`), affichée sur 100. Elle est
diffusée par joueur dans `EntityDelta::health`, celle des monstres normalisée
`hp / max_hp`.

### Ce qui est visible à l'écran (HUD, `editor/hud.rs`)

- Barre de vie du joueur local (haut), vignette rouge sur dégât.
- **Barre de capacités** 1/J · 2 · 3/K · 4 · H (bas) : la case s'allume tant que
  la capacité est en cours, se voile pendant la recharge (`ability_hud.rs`).
- **Jauges au-dessus des têtes** (`world_labels.rs`) : pseudo + barre + PV pour
  chaque autre joueur, barre rouge + « 2/3 » pour chaque monstre à moins de
  45 m, flash blanc sur perte de vie. Projection par `camera.view_proj()`.
- Roster « Joueurs » (vie, frags), mini-carte, pastille réseau (« En ligne ·
  x ms » = âge du dernier snapshot), bannières (allié à terre, palier, défaite).

---

## 5. Multijoueur

### Protocole (`src/net/protocol.rs`)

- Transport : une WebSocket par client (navigateur : `WebSocket` natif ;
  natif : `tokio-tungstenite`). Messages `bincode`.
- Client → serveur : `Join { protocol, name, firebase_uid, lobby, class,
  objective }`, puis `Input { move_x, move_y, aim_yaw, attack, jump, fire,
  weapon, heal, block, dash }` à chaque tick client (état complet, pas
  d'événements), `Leave`, `Ping`, `Chat`, `RestartRound`.
- Serveur → client : `Welcome { player_id }`, `PlayerJoined/Left`,
  `Snapshot { tick, entities, projectiles, creature_shots }` (~60/s),
  `Event(GameEvent)` (`RoundStart`, `WaveStart`, `Defeated`, `PlayerDown { cause }`,
  `Win/Lose`, `RoundObjective`), `JoinRejected { reason }`, `Pong`.
- **Toute modification de `ClientMsg`/`ServerMsg` qui casse la compatibilité
  impose un bump de `PROTOCOL_VERSION` et la redistribution de tous les clients.**
  Ajouter un champ `#[serde(default)]` à `EntityDelta` (comme `anim_clip`) ne
  casse rien.

### Salons et mondes (`src/bin/server.rs`)

- Un `Room` par code de salon, créé à la demande, `MAX_ROOMS = 16`, ~16 joueurs
  par salon. Le code `"riviere"` charge `Scene::riviere_demo()` ; **tout autre
  code** charge le hameau MMORPG (`WorldKind::Hameau`). Le client rejoint
  automatiquement le salon partagé du monde chargé localement quand le champ
  « Salon » est vide (`resolve_lobby_code`). Il n'y a pas de salon Rivière privé.
- Boucle : `SERVER_TICK = 16 ms` → `advance_play()` → `network_snapshot()`
  diffusé aux membres du salon. Relance de manche (`Room::restart`) quand tous
  les joueurs sont vaincus, ou après `MAX_DURATION`, ou sur `RestartRound`.
- Gardes : `MAX_CONNECTIONS_PER_IP = 4`, `MAX_TOTAL_CONNECTIONS = 256`, limiteur
  de débit par connexion, garde anti-AFK pour l'XP.
- Serveur = autorité sur : positions des joueurs (le client **n'a pas** de
  prédiction, il applique le snapshot), dégâts, projectiles, monstres, clips
  d'animation des joueurs. Client = autorité sur : son `aim_yaw` (orientation),
  ses propres animations de capacités (feedback immédiat), le HUD.

### Firebase (optionnel)

Progression persistante (XP, classement) si `FIREBASE_API_KEY`,
`FIREBASE_DATABASE_URL`, `FIREBASE_SERVER_EMAIL`, `FIREBASE_SERVER_PASSWORD`
sont définies pour le serveur. Absentes en production actuellement : le serveur
logue « Firebase désactivé », tout le reste fonctionne.

---

## 6. Le client web

- `packaging/web/index.html` : écran de chargement, détection WebGPU, aide
  clavier (remplacée pour `?scene=riviere`), plein écran, wake lock, message
  d'erreur GPU, et un placeholder `__RUSTEEGEAR_COMMIT__` substitué au build
  (« Build xxxxxxx » visible en bas de la page = preuve de déploiement).
- `?scene=riviere` fait charger la démo (`lib.rs` → `load_riviere_demo`) puis
  l'écran d'accueil multijoueur (pseudo, classe, salon, « Jouer en ligne » /
  « Jouer seul »). Sans paramètre, le hameau. `RUSTEEGEAR_OFFLINE=1` (variable
  de build) saute le serveur.
- Prérequis WebGPU : Chrome/Edge 113+, Safari 18+. Premier chargement 10-20 s
  (wasm 37 Mo, gzip par Caddy).
- Sonde de débogage : `window.__rusteegear_state` (chaîne `x=…;attack=…;fire=…`)
  et `window.__rusteegear_vars` ; on peut envoyer des touches avec
  `canvas.dispatchEvent(new KeyboardEvent('keydown', {code: 'KeyK'}))`.
- Pièges : un onglet en arrière-plan suspend `requestAnimationFrame` (page
  « figée » sur « Initialisation du GPU… ») ; dans un navigateur intégré
  throttlé, « En ligne · 1000 ms » n'est pas la latence réseau.

### Construire le client web

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127   # EXACTEMENT la version de Cargo.lock
brew install binaryen                               # wasm-opt (sinon wasm ~30 % plus gros)
./packaging/build_web.sh                            # → packaging/web/pkg/ (motor3derust_bg.wasm ≈ 37 Mo)
```

Un `.wasm` de ~15 Mo est **incomplet** (build lancé pendant un autre `cargo
build`) : les modèles embarqués manquent, la scène retombe sur des cubes.
Vérifier la taille avant de déployer, et la ligne « Démo rivière : 34 modèles
importés » dans la console du navigateur après.

---

## 7. Infrastructure (VPS)

Un VPS Ubuntu (noyau 6.8) partagé avec d'autres sites, accès SSH
`ubuntu@<ip>` avec la clé `~/.ssh/loicberthodvps`. L'adresse IP n'est
volontairement pas dans le dépôt : la passer via `RUSTEEGEAR_VPS_SSH`
(cf. `scripts/deploy_vps.sh`). `cargo` n'est pas dans le PATH d'un shell SSH
non interactif : `source ~/.cargo/env` d'abord.

| Élément | Valeur |
|---|---|
| Reverse proxy / TLS | Caddy, `/etc/caddy/Caddyfile` (Let's Encrypt automatique) |
| Bloc `water.loicberthod.ch` | `root * /mnt/data/water/dist`, `encode gzip`, `file_server`, en-têtes de sécurité |
| Bloc `ws.loicberthod.ch` | `reverse_proxy localhost:7777` (WebSocket) |
| Site statique | `/mnt/data/water/dist/` = `index.html` + `favicon.svg` + `pkg/` ; sauvegarde du déploiement précédent dans `/mnt/data/water/dist.bak/` |
| Serveur de jeu | unité systemd `rusteegear-server.service` : `User=ubuntu`, `WorkingDirectory=/home/ubuntu/rusteegear-server`, `ExecStart=…/target/release/server`, `Environment=RUSTEEGEAR_SERVER_ADDR=0.0.0.0:7777`, `Restart=always` |
| Checkout serveur | `~/rusteegear-server` = clone git de `main`, **git-lfs 3.4.1 installé** (assets matérialisés) |
| Logs | `sudo journalctl -u rusteegear-server -f` (ignorer le bruit ALSA : pas de carte son) |

Le serveur écoute en clair sur 7777 (Caddy termine le TLS) ; les vrais joueurs
passent tous par `wss://ws.loicberthod.ch`.

---

## 8. Procédures

### Déployer le site web (jamais GitHub Pages pour ce projet — consigne explicite)

```bash
# 1. arbre propre sur main, commit poussé (le hash affiché sur le site = HEAD)
./packaging/build_web.sh
ls -la packaging/web/pkg/motor3derust_bg.wasm        # ≈ 37 Mo attendu
# 2. assembler dist/ (ce dossier est ignoré par git)
mkdir -p dist && sed "s/__RUSTEEGEAR_COMMIT__/$(git rev-parse --short HEAD)/" packaging/web/index.html > dist/index.html
cp packaging/web/favicon.svg dist/ && rm -rf dist/pkg && cp -R packaging/web/pkg dist/pkg
# 3. sauvegarde distante puis rsync
ssh -i ~/.ssh/loicberthodvps ubuntu@<ip> 'rm -rf /mnt/data/water/dist.bak && cp -a /mnt/data/water/dist /mnt/data/water/dist.bak'
rsync -az --delete -e "ssh -i ~/.ssh/loicberthodvps" dist/ ubuntu@<ip>:/mnt/data/water/dist/
# 4. vérifier
curl -s https://water.loicberthod.ch/ | grep -o 'Build [0-9a-f]\{7\}'
curl -sI https://water.loicberthod.ch/pkg/motor3derust_bg.wasm | grep -i content-length
```

Rollback : `mv dist dist.broken && mv dist.bak dist` sur le VPS.

### Déployer le serveur

Scripté : `RUSTEEGEAR_VPS_SSH=ubuntu@<ip> scripts/deploy_vps.sh` (pull + build
release **sur le VPS** + restart + double smoke test). Ou à la main :

```bash
ssh -i ~/.ssh/loicberthodvps ubuntu@<ip>
source ~/.cargo/env && cd ~/rusteegear-server
git pull --ff-only && cargo build --release --bin server     # ≈ 2 min incrémental
sudo systemctl restart rusteegear-server && systemctl is-active rusteegear-server
```

Coupure ~2 s pour les joueurs connectés (ils sont déconnectés, la manche
repart). Pièges vécus : un fichier copié par `rsync -a` garde son mtime et cargo
peut croire le binaire à jour (`touch` le fichier) ; un checkout resté sur un
ancien commit ; git-lfs absent (voir § 3).

### Vérifier en production

```bash
cargo run --release --example smoke_vps wss://ws.loicberthod.ch   # hameau : projectile + monstres OK
cargo run --release --example probe_round -- wss://ws.loicberthod.ch Sonde 60   # 60 s sans trou de diffusion
```

Pour le salon Rivière, la technique éprouvée est une **sonde headless à deux
clients** : deux `NetClient::connect_to_lobby(url, nom, None, RIVIERE_LOBBY, 0, 0)`
dans un `examples/probe_*.rs` jetable, envoi d'`Input` toutes les 50 ms, lecture
des `Snapshot` (positions, `health`, `anim_clip`, `projectiles`). En 8 s on prouve
que deux joueurs se voient, qu'un tir part, qu'un coup blesse, qu'un mort
réapparaît — sans navigateur. Attention à `MAX_CONNECTIONS_PER_IP` (4) : des
sondes enchaînées finissent refusées (« Serveur plein ») quelques dizaines de
secondes.

En local : `RUSTEEGEAR_SERVER_ADDR=127.0.0.1:7777 cargo run --release --bin server`
puis la même sonde sur `ws://127.0.0.1:7777`, ou deux éditeurs pilotés
(`docs/PILOT.md`).

---

## 9. Tests et qualité

```bash
cargo test --profile dev-fast --lib          # ≈ 900 tests, ~45 s (une fois compilé)
cargo test --profile dev-fast --bin server   # 11 tests réseau avec vraies sockets
cargo clippy --profile dev-fast --lib --bins --tests   # doit être vide (CI bloquante)
```

CI GitHub Actions (`.github/workflows/ci.yml`) : `check` (fmt/clippy/tests),
`net-tests`, `golden` (rendu de référence), `cross-build` (wasm, Android, iOS),
`audit`, `editor-linux`, `editor-windows`. `pages.yml` publie la démo générique
sur GitHub Pages (hameau) — **pas** le site water. `release.yml` : DMG/APK sur tag
`v*` (secrets Android en attente, voir mémoire projet).

Tests de non-régression propres à Rivière (à garder verts) :
`riviere_demo_*` (scène), `a_fireball_survives_over_the_riviere_terrain_mesh`,
`a_scripted_body_pushed_into_the_floor_is_lifted_back_instead_of_falling_forever`,
`a_network_players_brief_attack_press_plays_the_attack_clip_for_all`,
`pvp_melee_reaches_a_player_standing_body_to_body_but_not_one_step_further`,
`a_player_killed_in_pvp_respawns_after_the_delay_only_in_ability_scenes`,
`the_snapshot_carries_normalized_monster_health`, `app::ability_hud`,
`app::world_labels`. Un test de synthèse sonore
(`runtime::sfx::…synth_variation…`) est aléatoirement instable, sans lien.

Technique de test physique headless sans GPU ni réseau : `AppState::new()` +
`load_riviere_demo()` + `playing = true` + boucle `advance_play()` avec
`sleep(16 ms)` ; entrées via `input_state` ; positions lues dans `scene.objects`.

---

## 10. Incidents résolus (pour ne pas les revivre)

| Symptôme | Cause | Correctif |
|---|---|---|
| Joueurs invisibles entre eux sur le site | git-lfs absent du VPS, `.glb` = pointeurs | `apt install git-lfs`, `git lfs pull`, restart |
| Fantômes qui flottent/téléportent à l'arrivée | y de spawn copié du gabarit sur terrain accidenté | raycast sol dans `spawn_network_player` |
| Ruée traverse les murs, rayon sol raté | broad-phase jetable sans colliders fixes après le 1er pas | BVH construite depuis les AABB (`query.rs`) |
| K/3 ne tire jamais | AABB du terrain contient tout point de tir | obstacles détectés par rayon physique |
| Coups/sorts invisibles pour les autres | seul Walk/Idle diffusé, puis clip écrasé chaque tick | `update_network_ability_animations` + `ability_anim_locked` |
| Renards à y = −1 800 | corps scripté enfoncé dans le décor, chute infinie | filet dans `resolve_scripted_moves` |
| Duel impossible | portée mêlée 1,2 m < distance minimale entre deux corps | `PVP_MELEE_RANGE` 2,8 m |
| Joueur tué reste mort | pas de réapparition hors réanimation Soutien | `update_network_respawn` (6 s, scènes à kit) |
| wasm de 15 Mo, scène en cubes | build web concurrent d'un autre `cargo build` | vérifier la taille avant rsync |

---

## 11. Limites connues et pistes

- Pas de prédiction côté client : la latence se voit sur ses propres
  déplacements (acceptable en Europe, ~60 snapshots/s).
- Recharges serveur (mêlée, sort en ligne) non diffusées : la barre de
  capacités ne voile que la ruée et le sort en solo.
- Le pseudo au-dessus d'un joueur arrivé avant vous s'affiche « Joueur N » (nom
  connu seulement via `PlayerJoined` postérieur au `Welcome`).
- Un seul salon Rivière public, pas d'équipes, pas de classement PvP.
- Réflexion planaire unique (la rivière amont, 9 m plus haut, reflète le ciel) ;
  feuillage en triangles colorés, pas de textures alpha.
- Manette : pas de bouclier/ruée ; tactile : boutons Feu/Arme/Saut seulement.
- `MAX_CONNECTIONS_PER_IP` compte un moment les connexions fermées.
- Firebase non configuré en production (pas de progression persistante).

---

## 12. Chronologie

- **11 sept. 2026** : scène Rivière & cascade (rendu eau, réflexion, forêt
  générée), mise en ligne sur water.loicberthod.ch (bloc Caddy, site statique).
- **14 sept. (journée)** : multijoueur (salon `riviere`, `WorldKind`), créature
  ronde skinnée, kit 1-2-3-4, roulade, enceinte invisible, correctifs physique,
  git-lfs sur le VPS, hauteur de spawn.
- **14 sept. (soir)** : touches avec retour à l'écran (barre de capacités, sort
  réparé, clips diffusés), filet anti-chute des corps scriptés, jauges de vie
  au-dessus des têtes, vie des monstres diffusée, PvP praticable et
  réapparition. Tout committé sur `main`, site et serveur à `fa94e8f`.
