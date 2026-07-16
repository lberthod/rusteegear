# Audit du jeu MMORPG — RusteeGear (`motor3derust`)

> Audit réalisé le 16 juillet 2026, branche `main` (dernier commit `b026c3b`).
> **Dépôt non propre au démarrage** : des modifications locales non commitées
> (négociation de version de protocole + héros féérique + caméra) étaient
> présentes — vraisemblablement une session de travail concurrente. Cet audit
> les relit aussi (cf. §5) mais ne les modifie pas et ne committe rien.
> Complète [AUDIT_MMORPG.md](AUDIT_MMORPG.md) (7 juillet, Sprints 50→61) : ici,
> l'état du *jeu* tel qu'il est aujourd'hui — scène embarquée, boucle serveur
> multi-salons, client réseau avec reconnexion, progression Firebase.

---

## 1. Ce qui a été vérifié

- `cargo test` : **445 verts, 2 ROUGES, 3 ignorés** (cf. §2 — `main` est rouge).
- `cargo test --features net_tests` (couverture socket complète) : **467 verts,
  mêmes 2 rouges** que ci-dessus — toute la plomberie réseau réelle
  (server_loop, reconnexion, rate-limit, plafond IP, rejet de version) passe ;
  seuls les deux garde-fous de scène échouent.
- `cargo fmt --check` et `cargo clippy --all-targets -- -D warnings` : propres.
- Relecture ligne à ligne : `net/protocol.rs`, `net/server_loop.rs`,
  `net/interpolation.rs`, `src/bin/server.rs`, `app/multiplayer.rs`,
  `app/network_client.rs` (partie session/snapshot), et sondages ciblés dans
  `app/health.rs`, `app/fireball.rs`, `app/scripting.rs`, `app/simulation.rs`.
- Diff non commité relu en entier (protocole v1 + `JoinRejected`, scène
  joueur → héros féérique animé, caméra).

## 2. BLOQUANT — `main` est rouge : la scène exportée a perdu le gameplay tactile

Le commit `cf5a9c8` (« Ajout décors nature… + mise à jour scène joueur ») a
ré-exporté `assets/player_scene.json` et fait échouer **deux tests garde-fous**,
dont un conçu précisément pour ce scénario :

### 2.1 Boutons tactiles « Feu », « Arme », « Soin » perdus
`app::fireball::tests::the_embedded_scene_ships_monsters_and_the_fire_button`
échoue : `mobile.buttons` ne contient plus que `["Saut"]`, et le contrôleur du
joueur a `fire_button: ""` (idem arme/soin). Conséquences en jeu, pour tout
client tactile (APK Android, aperçu mobile desktop) :

- **plus de tir à distance** (la boule de feu, pourtant validée côté serveur) ;
- **plus de changement d'arme** ni de **soin coopératif** (GAMEDESIGN §3.6) ;
- la **caméra ne pivote plus derrière le joueur** : ce suivi est conditionné à
  `fire_button` non vide (cf. `simulation.rs`, réticule de visée).

Autrement dit : la moitié des mécaniques multijoueur (fire/weapon/heal,
resolues côté serveur) reste dans le code mais est devenue **inaccessible** dans
le jeu réellement exporté. Correctif : re-câbler les trois boutons dans la scène
(ou restaurer les champs perdus depuis `git show cf5a9c8^:assets/player_scene.json`)
puis re-exporter proprement.

### 2.2 Test « tout import est une créature skinnée » périmé par les décors
`scene::tests::the_embedded_scene_resolves_its_bundle_creatures` échoue :
il exige `skeleton.is_some()` pour **tous** les imports du bundle, or les décors
nature (`m20_nature_bridge.glb`…) sont — légitimement — des meshes statiques.
Le test est devenu trop strict au moment de l'ajout des décors ; à restreindre
aux imports effectivement référencés par des créatures (ou par un préfixe de
nom), sinon il masquera de vraies régressions au milieu de faux rouges.

> Ces deux rouges datent du même commit et sont donc **antérieurs** au travail
> non commité en cours — la CI (fmt+clippy+tests stricte) aurait dû les bloquer.

## 3. Constats serveur (`src/bin/server.rs`, `net/server_loop.rs`)

### 3.1 Moyen — `Join` invalide : le client croit être connecté, mais n'existe pas
`handle_message` rejette un `Join` aux champs invalides (pseudo vide, salon hors
charset) par un simple `log::warn!` — mais le transport a déjà envoyé `Welcome`.
Le client affiche « Connecté (joueur N) », envoie ses `Input`… et n'apparaît
dans aucun salon, sans le moindre retour. Depuis le diff en cours,
`ServerMsg::JoinRejected` existe : le réutiliser ici (rejet applicatif, pas
seulement rejet de version) donnerait enfin un diagnostic au joueur.

### 3.2 Moyen — double `Join` vers un autre salon : le joueur existe dans deux salons
Rien n'empêche un client (bug, trame forgée — le protocole ne borne que la
*première* trame) d'envoyer un second `Join` avec un autre code de salon.
`player_room.insert` écrase le routage, mais l'ancien salon garde le joueur dans
`lobby.names` : il **reçoit les snapshots et évènements des deux salons à la
fois** (entités mélangées à l'écran) jusqu'à l'éviction par timeout (60 s), et
son avatar orphelin reste simulé dans l'ancien salon. Garde simple : refuser
(ou traiter comme no-op) un `Join` dont l'id figure déjà dans `player_room`.

### 3.3 Moyen — un salon vidé en cours de manche tourne 20 minutes pour personne
La fermeture d'un salon vide (`to_close`) n'est évaluée **que** quand la manche
est décidée ou dépasse `MAX_DURATION`. Si tous les joueurs partent en cours de
manche, la room continue de simuler physique + IA + 20 créatures jusqu'à
20 minutes, pour personne. Fermer aussi une room `connected_ids().is_empty()`
non décidée (au tick suivant son dernier départ) suffirait.

### 3.4 Moyen — fin de manche : Firebase bloque la boucle de tick de TOUS les salons
`award_progress`/`post_leaderboard` font des requêtes HTTP **séquentielles et
bloquantes** dans la boucle de tick (2+ requêtes par joueur Firebase). Le
compromis est documenté (« au pire, le joueur perd le bonus d'une manche »), mais
l'effet est **inter-salons** : pendant ces secondes, aucun salon du process ne
tick — gel visible pour tous les joueurs connectés, pas seulement ceux de la
manche terminée. À déporter dans un thread éphémère (même patron canal que le
reste du code).

### 3.5 Mineur — anneau de spawn : collisions garanties à partir du 9ᵉ joueur
`spawn_network_player` place chaque joueur à `angle = n × TAU/8` sur un cercle
de 3 m : 8 positions distinctes seulement. Les joueurs 9+ (le scope revendiqué
va jusqu'à 16/salon) réapparaissent **exactement** sur la position de spawn d'un
prédécesseur — le cas « deux corps interpénétrés séparés par une impulsion
violente » que le commentaire du code dit précisément vouloir éviter. De plus,
`n = network_players.len()` peut se répéter avec le recyclage des emplacements
(départs/arrivées). Utiliser un compteur monotone et TAU/16, ou chercher un
angle libre.

### 3.6 Mineur — classement : même score pour tous, malgré des frags individualisés
`post_leaderboard` poste **le score du salon** pour chaque joueur, alors que les
frags individualisés (`network_kills`) existent et sont déjà diffusés dans les
snapshots. Un joueur AFK d'une manche gagnante est classé comme son MVP. Design
à trancher (score d'équipe assumé, ou basculer sur les kills individuels).

## 4. Constats client (`app/network_client.rs`, `net/interpolation.rs`)

Rien de cassé trouvé : la chaîne prédiction → réconciliation (historique de
trajectoire + `CORRECTION_PULL` + rattrapage à l'arrêt), l'interpolation
retardée (`RENDER_DELAY`), le watchdog de silence (8 s) et la reconnexion à
backoff plafonné sont cohérents entre eux et bien testés. La hiérarchie des
timeouts (2,5 s filet créatures < 8 s watchdog < 60 s serveur) est respectée.
Le nouveau `JoinRejected` est correctement traité en rejet *fatal* (reconnexion
désarmée).

## 5. Travail non commité en cours — un bug à signaler avant commit

Le diff local (protocole v1, héros féérique) est globalement bon (l'invariant
bincode du variant `Join` est documenté **et testé**, y compris le vieux format
forgé octet par octet). Mais :

### 5.1 Important — script d'animation du joueur : clés `save` partagées entre TOUS les clones
Le nouveau script Lua du joueur (`assets/player_scene.json`) mémorise sa
position via `save.get/set("player_anim_px"/"player_anim_pz")`. Or le store
`save` est **partagé par `AppState`, pas par objet** (choix documenté,
`scripting.rs`). Conséquences dès 2 joueurs :

- **côté serveur** : chaque joueur réseau est un clone du gabarit *avec ce
  script* ; tous lisent/écrivent les mêmes clés, donc chacun calcule sa
  « vitesse » comme la distance entre **deux joueurs différents** ÷ dt —
  quasi toujours > 0,15 m/s ⇒ tout le monde en « Walk » permanent, y compris à
  l'arrêt, et c'est ce clip erroné qui est diffusé à tous les écrans ;
- **côté client** : les fantômes clonent aussi le script
  (`ensure_remote_player`, `..template`) et partagent les clés avec le joueur
  local — même corruption, en plus du conflit d'écriture avec le clip répliqué
  par snapshot (`set_clip`).

Correctifs possibles : préfixer les clés par un identifiant d'objet (si exposé
aux scripts), ou calculer l'anim Idle/Walk côté Rust (dans `sim_step`, où la
vitesse réelle du corps est déjà connue via `physics.velocity(idx)`) plutôt
qu'en Lua. À traiter **avant** de committer le diff en cours.

### 5.2 Note — `PROTOCOL_VERSION` bumpe de facto le format
L'ajout du champ `protocol` au `Join` casse la compatibilité avec tout client
déjà déployé (APK, web) : c'est assumé et documenté dans le code (redéploiement
client+serveur ensemble, vérif `examples/smoke_vps.rs`), rappelé ici pour que
le déploiement ne soit pas oublié.

## 6. Ce qui est solide (à ne pas casser)

- Versioning de protocole : invariant bincode (variant 0, champ 1) documenté et
  verrouillé par test, y compris le décodage d'un `Join` pré-versioning forgé.
- Durcissement des entrées réseau : NaN/infinis filtrés, axes bornés, `weapon`
  clampé à la table, charset/longueur des champs de `Join` validés avant tout
  usage comme clé/URL.
- Anti-abus transport : taille de trame bornée (64 Kio), rate-limit
  messages+octets par connexion, plafond de connexions par IP.
- Autorité serveur réelle : cooldowns d'attaque et de tir, portée du soin, vie
  individualisée, éviction au timeout, recyclage des emplacements de joueurs
  (scène bornée au pic de joueurs simultanés).
- Progression Firebase protégée contre l'écrasement sur lecture échouée
  (commit `b026c3b`), avec tests purs hors gate réseau.

## 7. Priorités recommandées

1. **Réparer `main`** : re-câbler Feu/Arme/Soin dans la scène embarquée (§2.1)
   et ajuster le test des imports skinnés (§2.2) — la CI redevient verte.
2. Corriger les clés `save` du script d'animation avant de committer le diff en
   cours (§5.1).
3. `JoinRejected` pour les rejets applicatifs de `Join` (§3.1) et garde
   anti-double-`Join` (§3.2).
4. Fermeture des salons vides non décidés (§3.3) et Firebase hors boucle de
   tick (§3.4).
5. Anneau de spawn 16 positions (§3.5) ; trancher le design du classement (§3.6).
