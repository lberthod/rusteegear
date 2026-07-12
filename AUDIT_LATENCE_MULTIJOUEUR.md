# Audit — qualité du mode en ligne & optimisation de la latence

**Suivi** : le plan d'action (§3) a été transformé en sprints dans
`SPRINTNETWORK.md`. État au 2026-07-12 : §2.2 (débit d'`Input`), §2.3
(réconciliation lissée), §2.4 (délai d'interpolation des fantômes) et §2.1
(cohérence doc/code du `Snapshot`) sont **corrigés** (Sprints 66-68, 70). §2.6
(vérification géographique du VPS) et §2.5 (transport non-TCP) restent
ouverts — respectivement en attente d'accès réel au serveur de test, et
conditionnel à une perte de paquets mesurée (cf. Sprints 69 et 71).

Date : 2026-07-12. Portée : `src/net/` (protocole, transport WebSocket,
interpolation), `src/app/multiplayer.rs` (snapshot serveur), `src/app/
network_client.rs` (client réseau), `src/bin/server.rs` (boucle serveur
headless). Complète `AUDIT_MMORPG.md` (architecture générale, déjà audité)
sans le répéter — ici, focus uniquement sur ce qui affecte la latence perçue
et la qualité du mode en ligne.

Méthode : lecture du code réseau actuel (pas de nouveau test lancé pour cet
audit), recoupée avec les commentaires déjà présents dans le code qui
documentent des tests en conditions réelles (VPS, 2026-07-12).

## 1. Ce qui est déjà en place (acquis, à ne pas casser)

- **`TCP_NODELAY` activé des deux côtés** (`src/net/server_loop.rs:100`,
  `src/net/client.rs:81`, correctif du 2026-07-12) : sans lui, l'algorithme de
  Nagle retardait les petites trames fréquentes (`Input`/`Snapshot`) jusqu'à
  ~40 ms pour les regrouper — documenté comme « une bonne part de la latence
  perçue constatée en test réel ». C'est corrigé.
- **Prédiction + réconciliation du joueur local** (`network_client.rs:156-192`,
  correctif du 2026-07-12) : le joueur local est piloté en prédiction
  immédiate (comme en solo), le serveur ne corrige que si l'écart dépasse
  `SNAP_THRESHOLD` (0,5 m). Sans ça, le joueur local attendait un aller-retour
  réseau complet avant de bouger — documenté comme « poisseux, pas temps
  réel » aux ~150-250 ms mesurés vers le VPS réel.
- **Interpolation des fantômes distants** (`src/net/interpolation.rs`) entre
  les deux derniers snapshots reçus, plutôt qu'une téléportation à chaque
  tick réseau.
- **Runtime tokio `current_thread`** (un thread dédié par connexion, pas un
  pool multi-thread) : évite un gaspillage de ressources qui n'aide en rien
  la latence à cette échelle (2-16 joueurs), cf. `AUDIT_MMORPG.md` §4.3.
- **Snapshot compact** (`bincode`, pas JSON) : test de garde-fou
  (`snapshot_size_for_sixteen_players_stays_compact`, `protocol.rs:237`)
  maintient le budget < 200 octets/entité/tick.
- **Tick serveur à 60 Hz** (`SERVER_TICK = 16 ms`, `src/bin/server.rs:53`) :
  fréquence de simulation/diffusion cohérente avec un jeu d'action.

## 2. Problèmes identifiés (classés par impact sur la latence perçue)

### 2.1 — Le snapshot n'est pas un delta, malgré ce que dit sa propre doc

`net/protocol.rs:57-60` documente `Snapshot` comme « uniquement les entités
dont l'état a changé depuis le dernier snapshot envoyé à *ce* client ». En
réalité, `AppState::network_snapshot` (`app/multiplayer.rs:258-276`)
reconstruit et diffuse l'état de **tous** les joueurs réseau à **chaque**
tick, identique pour tous les clients (`NetServer::broadcast`, pas
`send_to` individualisé) — aucun delta par client, aucun suivi de « dernier
état envoyé à ce client ».

- **Impact latence** : aucun aujourd'hui à l'échelle visée (16 joueurs ≈ 3,2
  Ko/s par client à 60 Hz, largement sous tout budget raisonnable) — mais le
  commentaire est trompeur pour la suite du projet, et le coût grandira
  linéairement avec le nombre d'entités diffusées (monstres/décor,
  actuellement hors snapshot, cf. limite déjà documentée).
- **Recommandation** : soit implémenter le vrai delta par client (mémoriser
  le dernier `EntityDelta` envoyé à chaque `PlayerId`, ne renvoyer que ce qui
  a changé au-delà d'un epsilon), soit corriger le commentaire pour refléter
  la réalité (« état complet, pas un delta — suffisant tant que N ≤ 16 »).
  Le second est trivial ; le premier n'est utile que si le nombre d'entités
  diffusées grandit significativement.

### 2.2 — Débit d'`Input` non borné, calé sur le framerate d'affichage

`network_client.rs:97-128` (`poll_network`) envoie un `ClientMsg::Input` à
**chaque frame de rendu**, sans limite. Sur un écran 144 Hz, c'est 144
messages/s envoyés pour un serveur qui ne simule qu'à 60 Hz (`SERVER_TICK`,
`server.rs:53`) — les 84 messages/s excédentaires n'apportent rien (le
serveur ne les consomme qu'une fois par tick, `try_recv` videra la file mais
seul le dernier lu avant le tick compte, cf. `set_network_input` qui
remplace l'entrée précédente) et gaspillent bande passante + CPU réseau des
deux côtés, pour rien.

- **Impact latence** : indirect — pas de latence ajoutée par message, mais un
  gaspillage qui devient sensible en multi-joueurs (N clients × débit
  d'affichage variable) et complique le diagnostic de charge serveur.
  Recommandation clé pour "optimiser la latence" au sens large : ne pas
  confondre débit et latence, ce point est un problème de débit inutile.
- **Recommandation** : throttler l'envoi d'`Input` à la fréquence du tick
  serveur (ou légèrement au-dessus, ex. 30-60 Hz fixe, indépendant du
  framerate), avec un minuteur (`Instant`) dans `poll_network` — même schéma
  que `SERVER_TICK` côté serveur.

### 2.2bis — Envoi de l'`Input` même sans changement, à débit non borné

Le commentaire de `ClientMsg::Input` (`protocol.rs:28-31`) assume déjà que
l'input est renvoyé « à chaque tick client, même sans changement » — cohérent
avec le choix de ne pas mémoriser d'état d'input entre deux messages côté
serveur (simplicité). Ce choix reste raisonnable *si* le débit d'envoi est
borné au tick serveur (cf. 2.2) ; sans ce plafond, il amplifie exactement le
problème ci-dessus.

### 2.3 — Correction dure (« snap ») au lieu d'un lissage, sur la position du joueur local

`interpolation::reconcile` (`interpolation.rs:95-101`) retourne la position
serveur brute dès que l'écart dépasse 0,5 m — `apply_local_network_position`
(`network_client.rs:172-192`) l'applique alors **instantanément**, en une
frame. Sous latence/gigue élevée (les 150-250 ms mesurés en conditions
réelles, ou un pic de perte de paquets), ce seuil peut être franchi
régulièrement (pas seulement en cas de triche/désync, comme le documente le
commentaire de `apply_local_network_position:159-170`) — le joueur voit alors
son personnage **téléporter** plutôt que de glisser vers la position
correcte.

- **Impact latence perçue** : direct — une correction dure est l'un des
  artefacts les plus visibles d'un mauvais netcode, précisément dans les
  conditions (latence 150-250 ms) déjà mesurées comme réelles pour ce projet.
- **Recommandation** : remplacer le snap instantané par une correction lissée
  sur quelques frames (ex. `lerp` vers `authoritative` sur 100-150 ms au lieu
  d'un `=` immédiat) une fois le seuil dépassé — la fonction `reconcile`
  existe déjà et peut rester le déclencheur, seul le point d'application
  (`network_client.rs:184`) doit lisser au lieu d'écraser.

### 2.4 — Pas de délai d'interpolation (buffer de rendu) pour les fantômes distants

`RemoteEntity::sample` (`interpolation.rs:44-68`) interpole entre les deux
**derniers** snapshots reçus, échantillonnés à `Instant::now()` (temps de
réception locale). C'est correct tant que les snapshots arrivent à intervalle
régulier proche du tick serveur (16 ms) — mais dès qu'un paquet est retardé
au-delà de l'écart entre les deux derniers snapshots reçus, `sample` clampe
`t` à 1.0 (`interpolation.rs:48-52`) et **gèle** le fantôme sur le dernier
état connu jusqu'à l'arrivée du prochain — au lieu de continuer à interpoler
en douceur.

- **Impact latence perçue** : sous gigue réseau réelle (VPS, 150-250 ms de
  RTT mesurés), c'est un décrochage/saccade visible des autres joueurs à
  chaque paquet en retard — plus la connexion est instable, plus c'est
  fréquent.
- **Recommandation standard (netcode temps réel)** : introduire un petit
  délai de rendu fixe (ex. 100 ms derrière le dernier snapshot reçu, à
  ajuster empiriquement) et interpoler entre les snapshots qui *encadrent*
  cet instant passé, plutôt qu'entre les deux derniers reçus. Nécessite de
  garder un court historique (3-5 snapshots, pas juste 2) par entité
  distante — changement contenu dans `RemoteEntity`, sans toucher au
  protocole.

### 2.5 — Transport TCP (WebSocket) : tête de ligne bloquante sous perte de paquets

Le transport (`tokio-tungstenite` sur WebSocket/TCP) garantit l'ordre et la
fiabilité, ce qui est un choix raisonnable pour la simplicité et suffisant à
l'échelle visée — mais un seul paquet perdu bloque la réception de **tous**
les messages suivants (snapshots inclus) jusqu'à sa retransmission TCP,
ajoutant potentiellement un aller-retour complet de latence au pire moment
(réseau dégradé).

- **Impact latence** : uniquement en cas de perte de paquets réelle (pas
  mesuré ici, pas de test de perte simulée dans la suite actuelle). À
  l'échelle 2-16 joueurs sur un unique VPS, ce n'est probablement pas le
  facteur dominant face aux points 2.3/2.4 ci-dessus.
- **Recommandation** : ne rien changer maintenant (une bascule vers UDP/QUIC
  ou WebRTC data channels non fiables serait un chantier de transport entier,
  hors proportion pour le problème actuel) — mais le documenter comme limite
  connue si la latence reste un problème après les correctifs 2.2-2.4.

### 2.6 — RTT mesuré (150-250 ms) : probablement géographique, pas applicatif

Le commentaire de `network_client.rs:163` documente « ~150-250 ms de latence
réelle vers le VPS » comme fait mesuré. Ce chiffre est élevé pour un jeu
d'action même après tous les correctifs logiciels ci-dessus — il ressemble à
une latence de **distance géographique** (client-serveur sur des continents
différents) plutôt qu'à un problème de code.

- **Recommandation** : vérifier la localisation du VPS par rapport aux
  joueurs testés (`ping`/`traceroute` vers l'IP du serveur) avant d'investir
  davantage dans le netcode — un serveur mal placé géographiquement rend
  inutile toute optimization applicative au-delà d'un certain point. Si le
  projet vise un public dispersé, plusieurs régions de déploiement (ou un
  relais) seront nécessaires à terme ; hors scope de cet audit (infra, pas
  code).

## 3. Plan d'action priorisé

Du gain le plus visible pour le moins d'effort, au plus structurant :

1. **Lissage de la réconciliation** (§2.3) — un seul point de code à changer
   (`network_client.rs:184`), corrige directement l'artefact le plus visible
   (téléportation) dans les conditions déjà mesurées comme dégradées.
2. **Délai d'interpolation** (§2.4) — contenu dans `RemoteEntity`, corrige les
   saccades des fantômes sous gigue réseau.
3. **Plafonner le débit d'`Input`** (§2.2) — hygiène réseau, pas de latence
   ajoutée mais évite un gaspillage qui grandira avec le nombre de joueurs.
4. **Vérifier la localisation géographique du serveur de test** (§2.6) — pas
   du code, mais peut expliquer une part significative des 150-250 ms avant
   de chercher plus loin côté applicatif.
5. **Corriger le commentaire (ou implémenter le vrai delta) de `Snapshot`**
   (§2.1) — cohérence doc/code, pas urgent tant que N ≤ 16 et sans monstres
   diffusés.
6. **Transport non-TCP** (§2.5) — à ne considérer que si les points 1-4 ne
   suffisent pas et qu'une perte de paquets réelle est mesurée en conditions
   de jeu.

Aucun de ces points ne remet en cause l'architecture existante (serveur
autoritaire, prédiction + réconciliation, interpolation) — ce sont des
réglages/durcissements dans le même cadre, cohérents avec les correctifs déjà
appliqués le 2026-07-12.
