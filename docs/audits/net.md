# `src/net/` (protocol.rs, firebase.rs, interpolation.rs, server_loop.rs, client.rs, mod.rs)

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans les fichiers.
Un seul document pour tout `src/net/` : c'est une seule couche (protocole,
transport, prédiction/interpolation, backend Firebase annexe) construite en
continu sur les mêmes sprints (`SPRINT_MMORPG.md`, `SPRINTNETWORK.md`,
`AUDIT_MMORPG.md`, `AUDIT_LATENCE_MULTIJOUEUR.md`).

## Attribution par sprint

- **Sprint 52** (`protocol.rs`) — messages `ClientMsg`/`ServerMsg`, codec
  `bincode` (`SPRINT_MMORPG.md`).
- **Sprint 53** (`server_loop.rs`, `client.rs`) — transport WebSocket
  (`tokio-tungstenite`), thread dédié + canaux `mpsc` de part et d'autre.
- **Sprint 54** (`interpolation.rs`, `protocol.rs::EntityDelta`) — prédiction
  client et interpolation des entités distantes.
- **Sprint 55** (`server_loop.rs`) — relais du `Join` initial au thread
  principal (`AppState::spawn_network_player`), `broadcast` du `Snapshot`.
- **Sprint 56** (`firebase.rs`) — comptes joueurs (email/mot de passe via
  l'API REST Firebase Auth).
- **Sprint 56/57** (`protocol.rs::ClientMsg::Join::firebase_uid`) — lien entre
  le compte Firebase et la session réseau.
- **Sprint 57** (`firebase.rs::PlayerProgress`) — progression persistante
  (niveau/XP), écrite uniquement par un compte serveur dédié (cf. « Qui écrit
  la progression ? » toujours dans le code).
- **Sprint 58** (`firebase.rs::ChatMessage`, `Presence`) — chat de salon et
  présence (heartbeat REST-only, pas de vrai `onDisconnect`).
- **Sprint 59** (`firebase.rs::LeaderboardEntry`) — classement global.
- **Sprint 60** (`protocol.rs::ServerMsg::PlayerLeft`) — déconnexion par
  timeout, en plus du départ volontaire.
- **Sprint 61** — mesure de la taille d'un `Snapshot` réel (16 joueurs, cf.
  ci-dessous) et décision de ne pas optimiser le format `anim_clip`
  (`String`, pas d'indice numérique) tant que peu d'entités animées sont
  diffusées.
- **Sprint 65** (`client.rs`) — `NetClient` compilé aussi pour Android
  (rejoindre un salon depuis un APK).
- **Sprint 67** (`interpolation.rs`) — `RENDER_DELAY`/`HISTORY_CAPACITY`,
  cf. bug réel ci-dessous.
- **Sprint 70** (`SPRINTNETWORK.md`, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.1) —
  discussion d'un vrai delta par client pour `Snapshot`, jugée non justifiée
  à l'échelle visée.
- **Sprint 79** (`protocol.rs::ClientMsg::Input::aim_yaw`) — cf. bug réel
  ci-dessous.
- **Sprint 82** (`protocol.rs::DEFAULT_LOBBY`, `ClientMsg::Join::lobby`,
  `client.rs::connect_to_lobby`) — introduction des salons multiples
  (`GAMEDESIGN_EN_LIGNE.md` §3.3), rétrocompatible avec le salon partagé
  unique d'avant.
- **Sprint 88** (`protocol.rs::EntityDelta::anim_clip`,
  `interpolation.rs::latest_anim_clip`) — réplication du choix d'animation.
- **Audit du 2026-07-07** (`AUDIT_MMORPG.md` §4.3, `server_loop.rs`,
  `client.rs`) — cf. bug réel ci-dessous (runtime tokio multi-thread inutile).

## Bugs réels trouvés en testant

- **Fantômes réseau figés vers -Z, tirs partant dans la mauvaise direction
  (audit du 2026-07-13, Sprint 79)** : `ClientMsg::Input::aim_yaw` existait
  déjà dans le protocole, mais le serveur ne s'en servait jamais pour faire
  pivoter les joueurs réseau — le bloc d'orientation de `sim_step` était
  réservé au joueur local. Résultat observable : les autres joueurs restaient
  visuellement figés dans leur orientation de spawn, et une boule de feu
  tirée par un joueur réseau partait dans cette même orientation de spawn au
  lieu de la direction visée. Corrigé en appliquant `aim_yaw` côté serveur
  pour tout joueur réseau (après nettoyage `NaN`/infini — pas d'enjeu
  anti-triche puisque le collider est une capsule symétrique, mais un enjeu
  réel pour la direction du tir).

- **Latence perçue anormalement haute sur les petites trames (`Input`)** :
  constatée en test réel sur le transport WebSocket. Cause : l'algorithme de
  Nagle (activé par défaut sur les sockets TCP) regroupe les petites écritures
  fréquentes pour réduire le nombre de paquets, retardant nos trames
  `Input`/`Snapshot` (quelques dizaines d'octets, plusieurs par seconde) de
  jusqu'à ~40 ms — exactement le pire cas pour ce trafic, qui a besoin de
  faible latence plus que de débit. Corrigé en activant `TCP_NODELAY` sur la
  socket, côté serveur (`server_loop.rs`) et côté client (`client.rs`).

- **Runtime tokio multi-thread inutile (audit du 2026-07-07,
  `AUDIT_MMORPG.md` §4.3)** : `server_loop.rs` et `client.rs` construisaient
  chacun leur runtime via `tokio::runtime::Runtime::new()`, qui crée par
  défaut un pool multi-thread (un thread ouvrier par CPU logique). À
  l'échelle visée (2-16 joueurs/salon, quelques sockets), ce travail est de
  l'attente réseau, pas du calcul parallèle : le pool réservait des threads
  pour rien. Corrigé en passant à
  `tokio::runtime::Builder::new_current_thread()` des deux côtés — un seul
  thread dédié `block_on` la boucle de vie entière de la connexion (pas
  seulement la connexion initiale, qui aurait laissé le reste tourner sans
  thread pour le faire progresser).

- **Snapshot figé sous gigue réseau réelle (Sprint 67, `SPRINTNETWORK.md`,
  `AUDIT_LATENCE_MULTIJOUEUR.md` §2.4)** : avant ce sprint, `RemoteEntity` ne
  gardait que les 2 derniers snapshots reçus et `sample` interpolait toujours
  entre `now` et le dernier point connu. Correct tant que les paquets
  arrivaient à intervalle régulier, mais dès qu'un paquet accusait un retard
  supérieur à cet intervalle, l'échantillon dépassait `latest.at` et se
  figeait sur le dernier état connu jusqu'à l'arrivée du paquet suivant —
  saccade visible en jeu réel, pas seulement en test synthétique à cadence
  parfaite. Corrigé en ajoutant `RENDER_DELAY` (100 ms) : les fantômes
  distants sont affichés à `now - RENDER_DELAY`, un instant qui reste presque
  toujours encadré par deux snapshots déjà reçus dans un historique élargi à
  `HISTORY_CAPACITY` (6, au lieu de 2) — au prix d'un léger délai d'affichage
  constant, largement préférable à une saccade intermittente.

## Mesures ponctuelles

- **Taille d'un `Snapshot` réaliste (Sprint 61)** : ~368 octets pour 16
  joueurs, soit environ 23 octets/joueur — très en dessous du budget large de
  200 octets/joueur/tick visé par `SPRINT_MMORPG.md`. A justifié de ne pas
  investir dans un vrai delta par client (mémoriser le dernier état envoyé à
  chaque `PlayerId`) ni dans un format plus compact pour `anim_clip`
  (`String` plutôt qu'un indice numérique dans `ImportedMesh::clips`) — à
  reconsidérer si le nombre d'entités diffusées grandissait significativement
  (monstres/décor animé, cf. `AUDIT_LATENCE_MULTIJOUEUR.md` §2.1).
