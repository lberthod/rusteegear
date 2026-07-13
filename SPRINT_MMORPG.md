# RusteeGear — Sprints MMORPG (multijoueur en ligne)

> Feuille de route dédiée au chantier **multijoueur**, en complément de
> [SPRINTS.md](SPRINTS.md) / [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md) (moteur solo).
> Numérotation à la suite : dernier sprint solo planifié = **49**, ce document
> commence donc à **50**.
>
> Légende : ✅ fait · 🟢 cœur fait (finitions reportées) · 🟡 en cours · ⬜ à faire · 🔴 bloqué.
> Chaque sprint a : **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**.
> Convention du projet : un sprint ≈ 1 à 3 jours, on ne démarre le suivant que si le
> précédent est « vert » (livrable validé). Commits attendus au format
> `Multi : <sprint> — <résumé>` pour rester grep-able (`git log --grep "^Multi"`).

---

## 0. Décision de scope (verrouillée le 2026-07-07)

| Question | Décision | Pourquoi |
|---|---|---|
| Échelle visée | **Petit multi en ligne, 2–16 joueurs par salon** (pas de monde persistant partagé) | Réaliste en solo ; réutilise le mode manches déjà livré (Sprints 45+, `attack_windup`, `AiChaser`) au lieu de repartir de zéro |
| Rôle de Firebase Realtime Database | **Backend annexe uniquement** : comptes, inventaire/progression persistante, chat, classement, présence des salons | RTDB n'a pas d'autorité serveur et n'a pas de SDK Rust natif — inadapté au transport temps réel (position/combat). Le gameplay temps réel passe par un **serveur de jeu Rust autoritaire** (WebSocket) |
| Autorité du gameplay | **Serveur** (headless, réutilise `scene`/`runtime`/physique existants) | Anti-triche de base : le client ne fait que prédire/afficher, jamais autorité |

> Si l'ambition change vers un vrai monde persistant à grande échelle, ce document ne
> s'applique plus tel quel : il faudrait revoir sharding de zones, base de données
> scalable dédiée et infra serveur — à retraiter en amont, pas en cours de route.

---

## 🧭 Vue d'ensemble des phases

| Phase | Sprints | But |
|---|---|---|
| **M — Préparation** | 50 | Isoler le code gameplay combat/manches de `app/mod.rs` avant d'y brancher le réseau |
| **N — Serveur & protocole** | 51 → 53 | Serveur headless autoritaire + protocole réseau + transport WebSocket |
| **O — Client réseau** | 54 → 55 | Prédiction/interpolation côté client + salons multijoueurs jouables |
| **P — Firebase RTDB (annexe)** | 56 → 59 | Comptes, progression persistante, chat/présence, classement |
| **Q — Robustesse & mise en prod** | 60 → 62 | Durcissement réseau, perf/charge, déploiement serveur |

---

## Suivi rapide (à cocher au fil de l'eau)

| # | Sprint | État |
|---|---|---|
| 50 | Extraction `app/combat.rs` | ✅ |
| 51 | Serveur headless (binaire, tick fixe) | ✅ |
| 52 | Protocole réseau & sérialisation | ✅ |
| 53 | Transport WebSocket + connexion client | ✅ |
| 54 | Prédiction client & interpolation | 🟢 |
| 55 | Salons multijoueurs (lobby, join/leave) | 🟢 |
| 56 | Firebase : comptes & auth | 🟢 |
| 57 | Firebase : inventaire/progression | 🟢 |
| 58 | Firebase : chat + présence | 🟢 |
| 59 | Firebase : classement (leaderboard) | 🟢 |
| 60 | Durcissement réseau & anti-triche de base | ✅ |
| 61 | Tests de charge & optimisation bande passante | ✅ |
| 62 | Déploiement serveur | ⬜ |
| 63 | Client réseau desktop & fenêtre Multijoueur (hors plan initial) | ✅ |
| 64 | Chat en jeu, branché sur le backend Firebase du Sprint 58 (hors plan initial) | ✅ |
| 65 | Classement en jeu, branché sur le backend Firebase du Sprint 59 (hors plan initial) | ✅ |

> Mettre à jour cette table à chaque sprint terminé — c'est la vue « où j'en suis »
> sans devoir relire tout le détail ci-dessous.

---

## PHASE M — Préparation

### Sprint 50 — Extraire le gameplay combat de `app/mod.rs` ✅ FAIT
**Objectif** : isoler attaque/manches/IA dans un module dédié pour pouvoir y brancher
le réseau sans travailler dans un fichier de ~4600 lignes.
- [x] Créé `src/app/combat.rs` (`pub(super)` — accessible par `app/mod.rs`, invisible
  ailleurs) : `AttackProjectile`, `AttackCharge`, constantes de vitesse/knockback,
  `attack_fx_index`, `max_wave`, `init_waves`, `update_waves`, et les deux méthodes
  qui portent toute la boucle d'attaque/manches : `update_attack(dt)` (cooldown →
  préparation → missile homing → impact/knockback) et `check_ring_outs()`.
  `AiChaser`/`Combat` restent des composants de données dans `scene/mod.rs` (inchangés :
  seule la *logique* qui les consomme a bougé).
- [x] `app/mod.rs` n'appelle plus que `self.update_attack(dt);` et
  `self.check_ring_outs();` dans `advance_play` — même ordre d'exécution qu'avant
  (refactor pur, aucun changement de comportement).
- [x] Les 83 tests existants passent toujours à l'identique (70 à l'époque de
  l'audit + tests ajoutés depuis).
- [x] Point d'extension pour le serveur réseau (Sprint 51+) : `update_attack`,
  `check_ring_outs`, `update_waves`/`init_waves` sont exactement les fonctions
  qu'un serveur headless devra appeler côté autorité, au lieu du client.
- **Fichiers** : `src/app/mod.rs` (4628 → 4311 lignes), nouveau `src/app/combat.rs`
  (339 lignes).
- **Livrable** : `cargo test` → **83/83 verts**, `cargo clippy --all-targets -D warnings`
  propre, `cargo fmt --check` propre.
- **Note** : dépôt vérifié propre (aucune édition concurrente, mtimes cohérents avec
  le dernier commit) avant de démarrer — cf. le risque noté ci-dessous, qui avait
  fait échouer une précédente tentative.

---

## PHASE N — Serveur & protocole

### Sprint 51 — Serveur de jeu headless ✅ FAIT
**Objectif** : un binaire serveur qui simule une partie **sans fenêtre ni GPU**, en
réutilisant `scene`/`runtime`/`combat.rs`.
- [x] Nouveau binaire `src/bin/server.rs` (convention Cargo `src/bin/*.rs`, pas de
  déclaration `[[bin]]` nécessaire dans `Cargo.toml`) : n'importe que
  `motor3derust::app::AppState` (donc, transitivement, `scene`/`runtime`/`app::combat`)
  — zéro appel à `gfx`, `editor`, `egui`, `winit` dans ce fichier. `AppState::advance_play`,
  `load_zombies_demo`, `has_won`/`is_lost`/`score`/`wave` étaient déjà `pub`, aucune
  API nouvelle à exposer pour ce sprint.
- [x] Boucle à tick serveur **20 Hz** (`SERVER_TICK`, découplée du pas fixe physique
  60 Hz interne à `advance_play`, cf. Sprint 45) : `advance_play()` appelé une fois par
  tick, `std::thread::sleep` pour le reste du budget de tick.
- [x] Test manuel : `cargo run --bin server` — la manche 1 se révèle, la physique/l'IA
  tournent, la défaite est détectée correctement à l'usure (aucune entrée joueur
  simulée pour l'instant → normal, le joueur ne se défend pas encore). Confirme que
  toute la chaîne (physique `rapier3d`, audio `kira` en repli silencieux, IA poursuite,
  manches, combat) s'initialise et boucle sans fenêtre ni crash.
- [x] `cargo fmt --check` et `cargo clippy --all-targets -D warnings` propres sur
  l'ensemble du projet avec le nouveau binaire.
- [ ] Reporté au Sprint 55 : un salon = une instance de `Scene` + liste de joueurs
  connectés (`PlayerId` → index) — pas de notion de salon multi-joueurs tant que le
  réseau (Sprints 52-53) n'existe pas.
- **Fichiers** : nouveau `src/bin/server.rs`.
- **Livrable** : `cargo run --bin server` fait tourner une manche jusqu'à son issue en
  console, sans fenêtre ; logs wave/score identiques en substance à la version desktop
  (mêmes fonctions `AppState`, aucune duplication de logique).
- **Risques levés** : `mlua`/`kira`/`rapier3d` compilent et s'exécutent sans
  `wgpu`/`winit` actifs — `Audio::new()` était déjà durci (Sprint 46, repli silencieux
  si aucun device audio), donc pas de crash en environnement sans son.
- **Note pour la suite** : dans ce MVP le joueur ne reçoit aucune entrée (pas de
  joystick/attaque simulés) — c'est voulu, le pilotage réel viendra du client réseau
  (Sprint 53). Un test de charge future (Sprint 61) devra simuler des inputs, pas
  laisser le joueur passif.

### Sprint 52 — Protocole réseau & sérialisation ✅ FAIT
**Objectif** : définir le format des messages client ↔ serveur avant d'ouvrir une socket.
- [x] `src/net/protocol.rs` : `ClientMsg` (Join, Input, Leave), `ServerMsg` (Welcome,
  PlayerJoined/Left, Snapshot, Event), `Snapshot`/`EntityDelta` (position + yaw +
  visible + santé optionnelle, indexés sur `scene.objects`), `GameEvent`
  (WaveStart/Defeated/Win/Lose).
- [x] Sérialisation via `serde` (déjà en dépendance) + **`bincode` 1.3** (ajouté à
  `Cargo.toml`) : `encode`/`decode` génériques, erreur typée `CodecError`.
  `serde_json` reste utilisable sur les mêmes types pour une inspection debug
  ponctuelle (déjà présent dans le projet, pas de dépendance en plus).
- [x] `EntityDelta` conçu comme un état minimal par entité (pas la place réservée
  pour l'implémentation *delta* proprement dite côté serveur — c'est-à-dire ne
  transmettre que les entités qui ont changé — qui viendra avec la boucle serveur
  au Sprint 53/55, mais le format s'y prête déjà : chaque entité est indépendante
  et optionnelle dans le `Vec`).
- [x] Tests unitaires : round-trip de chaque variant `ClientMsg`/`ServerMsg`
  (10 tests), plus un test de rejet d'octets invalides et un test de taille.
- **Fichiers** : nouveau `src/net/mod.rs`, `src/net/protocol.rs`, `Cargo.toml`
  (dépendance `bincode`), `src/lib.rs` (`pub mod net;`).
- **Livrable** : 10 tests de round-trip verts ; taille mesurée d'un snapshot de
  20 entités (16 joueurs + 4 monstres) = **536 octets, soit ~27 octets/entité**
  — largement sous l'objectif de 200 octets/joueur/tick.
- **Risques** : le format n'encode pas encore l'omission sélective des entités
  inchangées (ça reste à faire au niveau de la boucle serveur qui *construit* le
  `Snapshot`, pas du format lui-même) — à garder en tête au Sprint 55/61 si la
  mesure de charge dépasse le budget.

### Sprint 53 — Transport WebSocket + connexion client ✅ FAIT
**Objectif** : faire circuler le protocole du Sprint 52 sur un vrai réseau.
- [x] Ajouté `tokio` (rt-multi-thread/net/sync/macros/time), `tokio-tungstenite`,
  `futures-util`, gatés **desktop-only** dans `Cargo.toml` (même section que
  `rfd`/`ureq`) : la lib `cdylib` Android/iOS ne les compile pas — `src/net/mod.rs`
  gate `client`/`server_loop` derrière `#[cfg(not(any(target_os = "ios", target_os
  = "android")))]`, seul `protocol` (pas d'I/O) reste universel.
- [x] `src/net/server_loop.rs` (`NetServer`) et `src/net/client.rs` (`NetClient`) :
  chacun démarre son propre thread + runtime tokio et n'expose au reste du
  programme que des **canaux `std::sync::mpsc` synchrones** — même schéma que les
  imports glTF/requêtes IA déjà présents dans `app/mod.rs` (thread de fond + canal,
  poll non bloquant). `AppState`/`src/bin/server.rs` n'ont donc jamais besoin de
  connaître `tokio` directement.
  ⚠️ **Cohabitation tokio/winit non testée dans ce sprint** : le client réseau
  (`NetClient`) est écrit et testé isolément (test d'intégration), mais son
  branchement dans la boucle `winit` (`lib.rs`/`app/mod.rs`) est repoussé au
  Sprint 54, où la prédiction/interpolation ont de toute façon besoin d'un vrai
  branchement dans `advance_play`.
- [x] Serveur : accepte les connexions, lit le `ClientMsg::Join` initial, attribue un
  `PlayerId`, répond `ServerMsg::Welcome`, puis relaie `Input`/`Leave` vers `inbox`.
- [x] `src/bin/server.rs` intègre `NetServer` dans la vraie boucle de jeu (pas
  seulement en test isolé) : logue les messages reçus, diffuse un `Snapshot` de la
  position du joueur local à chaque tick.
- [x] Tests d'intégration bout-en-bout (2, dans `server_loop.rs`) : un `NetClient`
  rejoint, reçoit son `Welcome`, envoie un `Input` que le serveur reçoit avec le bon
  `PlayerId` ; deux clients obtiennent des identifiants distincts et reçoivent tous
  deux un `broadcast`. **Choix assumé** : validation par test automatisé plutôt que
  par ouverture manuelle de deux fenêtres graphiques — cet environnement n'a pas
  d'affichage pour observer visuellement « l'autre joueur bouger », et un test
  reproductible est de toute façon plus fiable qu'une vérification à l'œil.
- **Fichiers** : nouveaux `src/net/client.rs`, `src/net/server_loop.rs` ; modifiés
  `src/net/mod.rs`, `src/bin/server.rs`, `Cargo.toml`.
- **Livrable** : `cargo test` → **95/95 verts** (2 nouveaux tests réseau bout-en-bout),
  `cargo run --bin server` écoute réellement sur `127.0.0.1:7777` et diffuse un
  `Snapshot` par tick ; `clippy`/`fmt` propres.
- **Reste ouvert pour le Sprint 54/55** : brancher `NetClient` dans la boucle
  `winit` (pas encore fait ici, volontairement — la prédiction/interpolation du
  Sprint 54 en a besoin de toute façon) ; le pilotage du joueur depuis l'`Input`
  réseau reçu par le serveur n'est pas encore appliqué à `AppState` (juste logué).

---

## PHASE O — Client réseau

### Sprint 54 — Prédiction client & interpolation 🟢 (cœur livré, câblage UI reporté)
**Objectif** : rendre le jeu jouable malgré la latence réseau (le serveur est à 20 Hz,
le rendu à 60 Hz).
- [x] `src/net/interpolation.rs` : `RemoteEntity` (historique borné à 2 snapshots
  horodatés en temps **client local**, pas en tick serveur — reste correct quel que
  soit le jitter réseau) + `sample(now)` qui interpole position/yaw (chemin le plus
  court sur l'angle, pas de détour par 0 en cas de demi-tour) et clampe avant le
  premier / après le dernier snapshot (pas d'extrapolation hasardeuse).
- [x] `reconcile(predicted, authoritative)` : ne renvoie une correction que si
  l'écart dépasse `SNAP_THRESHOLD` (0,5 m) — sinon la prédiction locale reste
  telle quelle, pas de micro-saccade à chaque snapshot reçu.
- [x] 7 tests couvrant : aucun snapshot, un seul snapshot (pas d'interpolation),
  interpolation au mi-temps, clamp avant/après, angle qui traverse π (chemin
  court), et les deux branches de `reconcile` (ignoré / corrigé).
- [ ] **Reporté au Sprint 55** : câbler `NetClient` dans la boucle `winit`/`AppState`
  (envoi d'`Input` par frame, `RemoteEntity` par joueur distant affiché comme
  objet de scène supplémentaire, réconciliation appliquée au joueur local). Décision
  assumée : cet environnement n'a pas d'affichage graphique pour valider visuellement
  un tel câblage (cf. Sprint 53) ; le Sprint 55 câble de toute façon les salons
  multi-joueurs (plusieurs objets pilotables distincts), donc regrouper les deux
  évite un premier câblage à moitié refait une semaine plus tard.
- [ ] Test de latence artificielle : repoussé avec le câblage ci-dessus (nécessite
  un client réseau réellement intégré à la boucle de jeu pour être significatif).
- **Fichiers** : nouveau `src/net/interpolation.rs`.
- **Livrable obtenu** : la logique d'interpolation/réconciliation — la partie
  mathématique qui rend le mouvement lisse malgré la latence — est écrite et
  testée (102/102 tests verts au total, clippy/fmt propres). Le câblage bout-en-
  bout dans le jeu réel est la partie non couverte, reportée explicitement.
- **Risques** : le seuil `SNAP_THRESHOLD` (0,5 m) est une valeur de départ
  raisonnable mais non validée en conditions réelles de latence — à ajuster au
  Sprint 55 une fois testable dans le jeu.

### Sprint 55 — Salons multijoueurs (lobby, join/leave) 🟢 (cœur serveur fait, UI lobby reportée)
**Objectif** : brancher le réseau sur le mode manches existant, jouable à plusieurs.
- [x] `src/app/multiplayer.rs` (nouveau, `pub` — contrairement à `combat.rs`,
  privé, car `src/bin/server.rs` en a besoin depuis l'extérieur de la lib) :
  `spawn_network_player`/`despawn_network_player`/`set_network_input`/
  `network_player_object`/`network_snapshot`. Un joueur réseau = un clone du
  gabarit pilotable de la scène, ajouté comme objet indépendant (écarté du
  gabarit et des joueurs précédents pour éviter une interpénétration physique
  au spawn), avec son propre `NetworkInput`.
- [x] `sim_step` (`app/mod.rs`) route l'input par objet : un objet mappé à un
  joueur réseau utilise son `NetworkInput` propre ; l'objet « joueur local »
  (non mappé) continue d'utiliser `self.input_state` comme avant — **aucun
  changement de comportement pour le mode solo existant** (108 tests toujours
  verts sans modification).
- [x] `src/net/server_loop.rs` relaie maintenant aussi `ClientMsg::Join` au
  thread principal (pas seulement `Input`/`Leave` comme au Sprint 53), et
  émet un `Leave` synthétique à la fin de la connexion (déconnexion volontaire
  *ou* abrupte) — `despawn_network_player` étant idempotent, pas de risque de
  double traitement.
- [x] `src/bin/server.rs` (`handle_message`) : `Join` → `spawn_network_player` +
  broadcast `PlayerJoined` ; `Input` → `set_network_input` ; `Leave` →
  `despawn_network_player` + broadcast `PlayerLeft`. Diffuse
  `network_snapshot(tick)` (tous les joueurs réseau) à chaque tick au lieu du
  snapshot à un seul objet du Sprint 53.
- [x] Test bout-en-bout **à travers un vrai socket** (`src/bin/server.rs`,
  `joining_moving_and_leaving_through_the_real_socket`) : un `NetClient` réel
  rejoint, obtient un objet, le déplace via son `Input`, puis part — vérifie le
  câblage complet (pas seulement les méthodes `AppState` isolées). 6 tests
  supplémentaires dans `app::multiplayer` (spawn/despawn/inputs indépendants/
  snapshot).
- [ ] **Reporté** : écran de lobby egui (créer un salon / rejoindre par code,
  liste des joueurs avant lancement). Décision assumée : aucune valeur sans un
  client graphique réellement connecté au réseau (le client desktop actuel ne
  se connecte pas encore à `NetClient, cf. Sprint 54) — ajouter un écran avant
  d'avoir un client qui l'utilise serait du code mort, non testable ici de
  toute façon (pas d'affichage graphique dans cet environnement).
- [ ] **Reporté** : re-synchronisation d'un client qui rejoint en cours de
  manche avancée (`ServerMsg::FullState` dédié) — pour l'instant un nouveau
  joueur reçoit les prochains `Snapshot` réguliers (contenant déjà tous les
  joueurs réseau), donc se resynchronise en pratique en un tick ; un état
  complet dédié n'apporterait de valeur qu'avec des entités *non-joueur*
  (monstres) dans le snapshot, absentes pour l'instant (cf. limite ci-dessous).
- **Fichiers** : nouveau `src/app/multiplayer.rs` ; modifiés `src/app/mod.rs`
  (déclaration du module, champs `network_players`/`network_inputs`, routage
  d'input par objet dans `sim_step`), `src/net/server_loop.rs`,
  `src/bin/server.rs`.
- **Livrable** : **108 tests lib + 1 test bin verts** (aucune régression sur le
  mode solo), clippy/fmt propres ; un client réseau réel peut rejoindre,
  déplacer *son* objet indépendamment des autres, et partir proprement — validé
  par test automatisé de bout en bout à travers un vrai socket TCP/WebSocket
  local (pas seulement en mémoire).
- **Limites connues, assumées et documentées dans le code** (`multiplayer.rs`,
  `server.rs`) :
  1. La vie (`hud_health`) et les conditions de victoire/défaite restent
     celles de l'objet gabarit d'origine, pas individualisées par joueur —
     un vrai combat joueur-contre-joueur demande d'abord de donner à chaque
     joueur sa propre vie (extension naturelle de `Combat`, pas faite ici).
  2. `network_snapshot` ne transmet que les joueurs réseau, pas les monstres
     (`AiChaser`/`Combat`) — un client verrait donc les autres joueurs bouger
     mais pas les monstres qu'ils combattent. À étendre si/quand un client
     graphique réel consomme ces snapshots.
  3. Les monstres (`AiChaser`) poursuivent toujours *un seul* point cible
     (`player_position()`, heuristique du joueur local), pas le joueur réseau
     le plus proche — plusieurs joueurs connectés ne changent donc pas
     (encore) le comportement de l'IA.
  4. Pas d'écran de lobby ; pas de multi-salons (un seul salon = un seul
     `AppState` par processus serveur).

---

## PHASE P — Firebase Realtime Database (backend annexe)

> Rappel de scope : Firebase ne transporte **jamais** de position/combat en jeu ici —
> uniquement des données peu fréquentes (compte, chat, classement, présence).
> Pas de SDK Rust officiel Firebase → accès via l'API REST (déjà `ureq` en dépendance
> pour l'IA) + endpoint SSE de RTDB pour les mises à jour poussées (chat/présence).

### Sprint 56 — Comptes & authentification 🟢 (client REST fait, écran de connexion reporté)
**Objectif** : identifier les joueurs de façon persistante entre les sessions.
- [x] `src/net/firebase.rs` (nouveau, desktop-only comme `client.rs`/`server_loop.rs` —
  dépend de `ureq`, déjà gaté ainsi) : `sign_up`/`sign_in` (Firebase Auth REST,
  `signUp`/`signInWithPassword`) → `AuthSession { uid, id_token }` ;
  `set_profile_name`/`get_profile_name` (RTDB REST) pour le pseudo.
- [x] Logique de parsing séparée de l'I/O réseau (`parse_auth_response`,
  `parse_error_message`, `rtdb_url`) : testable sans identifiants Firebase réels.
  6 tests (réponse réussie, réponse malformée, message d'erreur Firebase, URL avec/
  sans slash final, URL avec/sans query string).
- [x] Doc de sécurité en tête de module (règles RTDB requises **avant** d'exposer le
  nœud en écriture — la clé API Web est publique par conception, ce n'est pas un
  secret côté client).
- [x] `Settings` (`app/settings.rs`) : `firebase_api_key`/`firebase_database_url`,
  persistés comme la clé DeepSeek existante. Champs ajoutés au panneau **Paramètres**
  de l'éditeur (`editor/mod.rs`), même convention que la section IA (auto-save au
  changement, indicateur configuré/non configuré).
- [ ] **Reporté** : écran de connexion/inscription en jeu (egui), stockage de la
  session, affichage du pseudo. Décision assumée, deux raisons : (1) aucun compte
  Firebase réel n'est configuré dans cet environnement pour valider un flux de
  connexion de bout en bout (contrairement au protocole réseau, testable sans
  service externe) ; (2) un écran de connexion est de l'UI `egui` non vérifiable
  visuellement ici (même limite que le lobby reporté au Sprint 55). Le client REST
  ci-dessus est en revanche complet et testé — brancher l'écran dessus est mécanique
  une fois qu'il y a un vrai projet Firebase à pointer.
- **Fichiers** : nouveau `src/net/firebase.rs` ; modifiés `src/app/settings.rs`,
  `src/editor/mod.rs` (panneau Paramètres), `src/net/mod.rs`.
- **Livrable** : 114 tests lib + 1 test bin verts, clippy/fmt propres ; `sign_up`/
  `sign_in`/`set_profile_name`/`get_profile_name` prêts à l'emploi dès qu'une clé API
  Web et une URL RTDB sont renseignées dans les Paramètres (non vérifié en conditions
  réelles, faute de projet Firebase de test disponible ici).
- **Risques** : les clés Firebase Web sont publiques par design (la sécurité vient des
  **règles RTDB**, pas du secret de la clé) — écrire les règles de sécurité *avant*
  d'exposer le nœud en écriture, pas après. Documenté dans le module, pas encore
  vérifié contre un vrai projet Firebase.

### Sprint 57 — Inventaire & progression persistante 🟢 (câblage serveur fait, non testé en conditions réelles)
**Objectif** : garder l'XP/objets d'un joueur entre les parties.
- [x] `PlayerProgress { level, xp }` (`net/firebase.rs`) ↔ `/users/{uid}/progress` ;
  `get_progress` traite `null` (nœud absent, premier lancement) comme niveau 1/0 XP,
  pas une erreur. `set_progress` prend un `auth_token` **explicite**, pas une
  `AuthSession` de joueur (cf. point suivant). 3 tests de parsing.
- [x] `ClientMsg::Join` gagne un champ `firebase_uid: Option<String>` (protocole
  modifié, tests mis à jour) : un client connecté à Firebase le transmet au join,
  pour que le serveur sache quel `uid` créditer. `None` = partie locale/anonyme,
  aucun changement de comportement.
- [x] **Qui écrit la progression ?** Documenté en détail dans `net::firebase` : le
  client ne doit jamais écrire sa propre progression avec son propre token (triche
  triviale sur les gains) — seul le **serveur de jeu**, avec un compte Firebase
  dédié (`FIREBASE_SERVER_EMAIL`/`FIREBASE_SERVER_PASSWORD`, distinct des comptes
  joueurs), doit pouvoir écrire `/users/*/progress`, via des règles RTDB dédiées.
  Vraie mise en prod : Firebase Admin SDK (hors scope, pas de crate Rust mature) ;
  ici, alternative REST « compte serveur + règles », documentée avec un exemple de
  règles JSON.
- [x] `src/bin/server.rs` : `connect_firebase_server()` lit 4 variables
  d'environnement au démarrage, se connecte une fois (`sign_in`), continue sans
  Firebase si absentes/en échec (log, pas de crash). `award_progress()` crédite le
  score de la manche en XP à chaque joueur réseau connu de Firebase, à la fin de la
  manche (victoire ou défaite) ; niveau = XP / 1000 (formule simple, à raffiner).
  Toute erreur Firebase est loguée, jamais fatale (la progression est un bonus).
- **Fichiers** : `src/net/firebase.rs`, `src/net/protocol.rs` (+ `client.rs`,
  `server_loop.rs` mis à jour pour le nouveau champ), `src/bin/server.rs`.
- **Livrable** : 117 tests lib + 1 test bin verts, clippy/fmt propres ; le serveur
  tourne identiquement sans les variables Firebase (vérifié manuellement — log
  « Firebase désactivé », aucune régression).
- **Non vérifié, assumé** : le flux complet (compte serveur réel, règles RTDB
  réellement déployées, écriture effective) n'a **pas** été testé contre un vrai
  projet Firebase — aucun projet de test disponible dans cet environnement. Le
  code est écrit pour être correct et dégrader proprement, mais « ça compile et la
  logique est testée unitairement » n'est pas la même chose que « vérifié en
  conditions réelles ». À valider dès qu'un projet Firebase est configuré.
- **Reporté** : lecture de la progression **au login** côté client (afficher
  niveau/XP en jeu) — dépend de l'écran de connexion reporté au Sprint 56 ; même
  raison (pas d'UI vérifiable ici).

### Sprint 58 — Chat de salon & présence 🟢 (REST fait, SSE temps réel reporté)
**Objectif** : communication texte + liste des joueurs en ligne.
- [x] `ChatMessage { sender, text, sent_at_ms }` sur `/lobbies/{code}/chat` :
  `post_chat_message` (POST, clé générée par RTDB — n'écrase jamais les messages
  existants), `list_chat_messages` (GET, tri par `sent_at_ms` croissant, `null` →
  liste vide plutôt qu'erreur si le salon n'a pas encore de messages).
- [x] `Presence { last_seen_ms }` sur `/presence/{uid}` : `set_presence` (heartbeat),
  `list_online_players`/`is_online` (seuil `PRESENCE_TIMEOUT_MS`, 15 s).
  **Limite assumée, documentée dans le module** : la présence RTDB « officielle »
  (`onDisconnect()`) est une fonctionnalité **liée à la connexion WebSocket du SDK
  JS/natif**, absente de l'API REST utilisée ici (pas de notion de connexion
  persistante en HTTP requête/réponse). Cette implémentation fait donc un
  **heartbeat périodique** plutôt qu'une vraie détection de déconnexion : un joueur
  qui perd la connexion brutalement reste « en ligne » jusqu'à expiration du seuil
  (15 s), au lieu d'une réaction immédiate. Acceptable à l'échelle visée (2-16
  joueurs/salon).
- [x] 6 tests (chat vide/trié/malformé, présence vide/multi-uid, seuil `is_online`).
- [ ] **Reporté, décision assumée** : écoute en **SSE** (flux temps réel, sans
  repoller) et **UI chat egui**. Deux raisons, cohérentes avec les sprints
  précédents : (1) le risque explicitement noté dans ce document
  (« SSE + `ureq` bloquant ne cohabitent pas bien avec `winit` ») se confirme à
  l'implémentation — un flux SSE ouvre une connexion HTTP tenue indéfiniment, ce
  qui demande la même gymnastique thread-dédié-avec-canal que `NetClient`/`NetServer`
  (Sprint 53), pas un simple appel `ureq` de plus ; (2) aucune UI n'est vérifiable
  visuellement dans cet environnement. Le **polling REST** (`list_chat_messages`/
  `list_online_players`) fonctionne dès maintenant et couvre un chat de salon à
  petite échelle (2-16 joueurs) sans le risque du SSE — un appel toutes les 1-2 s
  suffit largement à cette échelle, quitte à migrer vers du SSE plus tard si le
  volume de requêtes devient un problème réel (mesurable, pas anticipé).
- **Fichiers** : `src/net/firebase.rs`.
- **Livrable** : 123 tests lib + 1 test bin verts, clippy/fmt propres ; chat et
  présence utilisables dès maintenant par polling (non vérifié contre un vrai
  projet Firebase, même réserve qu'aux Sprints 56-57).
- **Risques** : la présence heartbeat suppose un appel `set_presence` régulier côté
  client — pas encore câblé (dépend de l'écran de connexion/lobby reporté), donc pas
  testable de bout en bout pour l'instant.

### Sprint 59 — Classement (leaderboard) 🟢 (backend + câblage serveur faits, UI reportée)
**Objectif** : classement global des meilleurs scores.
- [x] `LeaderboardEntry { name, score, achieved_at_ms }` sur `/leaderboard` :
  `post_leaderboard_entry` (POST, `auth_token` explicite — même raison qu'au
  Sprint 57, le score est une donnée compétitive, seul le serveur doit l'écrire),
  `get_top_leaderboard(limit)` (tri décroissant par score, tronqué à `limit`).
- [x] `src/bin/server.rs` (`post_leaderboard`) : appelée juste après
  `award_progress` en fin de manche (victoire ou défaite), pour chaque joueur
  réseau connu de Firebase. Mêmes garanties : jamais fatal, juste logué en cas
  d'échec.
- [x] 3 tests (vide, tri décroissant, entrée malformée).
- [ ] **Reporté** : écran classement en jeu (dépend du lobby/de la connexion,
  reportés aux Sprints 55-56 pour les mêmes raisons — UI non vérifiable ici).
- **Fichiers** : `src/net/firebase.rs`, `src/bin/server.rs`.
- **Livrable** : 126 tests lib + 1 test bin verts, clippy/fmt propres ; serveur
  vérifié sans régression (tourne identiquement sans Firebase configuré).
- **Risque non résolu, documenté dans le code** : `/leaderboard` grossit sans
  purge — chaque manche ajoute une entrée, jamais retirée, et `get_top_leaderboard`
  lit tout le nœud avant de trier/tronquer côté client (pas de requête RTDB
  indexée côté serveur ici). Acceptable tant que le volume reste faible ; à
  corriger (purge périodique ou requête indexée) avant une mise en production
  avec un usage soutenu.

---

## PHASE Q — Robustesse & mise en production

### Sprint 60 — Durcissement réseau & anti-triche de base ✅ FAIT
**Objectif** : le serveur ne fait confiance à aucune donnée client brute.
- [x] **Bug réel trouvé et corrigé** : `sanitize_network_input` (`app/multiplayer.rs`)
  rejette `NaN`/`Infinity` avant de mémoriser un `NetworkInput` — sans ce nettoyage,
  un `NaN` reçu du réseau (octets bincode arbitraires d'un client modifié, pas
  nécessairement un client légitime limité par des sliders egui) traverse
  `f32::clamp` sans être filtré (`NaN < min`/`NaN > max` sont tous deux faux) et se
  propage dans la position physique de l'objet, corrompant potentiellement la
  simulation pour tout le monde. Testé (`a_nan_input_from_the_network_never_
  corrupts_the_players_position`) : la position reste finie après plusieurs pas de
  simulation avec un input `NaN` malveillant.
- [x] **Attaque réseau + cooldown serveur** (nouveau, pas seulement un durcissement
  de l'existant) : `update_network_attacks` — chaque joueur réseau peut désormais
  frapper immédiatement à courte portée (`NETWORK_ATTACK_RANGE`, 1,2 m) quand son
  `Input.attack` est vrai, avec un temps de recharge (`NETWORK_ATTACK_COOLDOWN`,
  0,4 s) **validé côté serveur** — un client qui spamme `attack: true` à chaque
  tick ne peut pas frapper plus vite que ce temps de recharge. Testé
  (`server_rejects_attack_before_cooldown_elapsed`) : deux cibles à portée, un
  spam d'attaques rapprochées n'en vainc qu'une, la seconde ne tombe qu'après le
  temps de recharge écoulé.
- [x] **Timeout client** (`src/bin/server.rs`) : `CLIENT_TIMEOUT` (10 s) — un joueur
  sans le moindre message (même un `Input` inchangé, qu'un client légitime envoie à
  chaque tick) est retiré de la partie par `evict_timed_out_players`, appelée une
  fois par tick. Testé bout-en-bout à travers un vrai socket
  (`a_silent_client_is_evicted_after_the_timeout`, avec un timeout court injecté en
  paramètre pour ne pas attendre 10 s réelles dans le test).
- [ ] **Reporté** : reprise de la même place lors d'une reconnexion dans la fenêtre
  de grâce (nécessiterait une identité stable across reconnects, ex. `firebase_uid`
  comme clé plutôt que le `PlayerId` de session — un vrai chantier, pas fait ici).
  Un joueur qui timeout ou se déconnecte réapparaît aujourd'hui comme un nouveau
  joueur s'il se reconnecte.
- **Fichiers** : `src/app/multiplayer.rs`, `src/app/mod.rs` (appel dans
  `advance_play`), `src/bin/server.rs`.
- **Livrable** : 130 tests lib + 2 tests bin verts, clippy/fmt propres ; serveur
  vérifié sans régression (tourne identiquement pour une manche locale).
- **Limite documentée** : les attaques réseau (via `attack_at`) ne créditent pas
  encore de score/son (contrairement à l'attaque du joueur local) — cohérent avec
  la limite déjà documentée au Sprint 55 (santé/score pas encore individualisés
  par joueur réseau). Elles font en revanche bien progresser les manches
  (`update_waves` ne regarde que la visibilité des cibles, pas le score).
- **Risques** : périmètre volontairement limité, comme prévu — pas de détection
  d'aimbot/wallhack (hors scope d'un jeu 2-16 joueurs solo-dev), seulement la
  validation serveur des règles de jeu de base (mouvement, cooldown, présence).

### Sprint 61 — Tests de charge & optimisation bande passante ✅ FAIT
**Objectif** : confirmer que 16 joueurs/salon restent fluides et peu coûteux en réseau.
- [x] `examples/load_test_client.rs` : démarre un `NetServer` + `AppState` dans le
  même processus, connecte **16 bots** (`NetClient` réels, vrai socket local),
  chacun envoyant un `Input` à 20 Hz (mouvement sinusoïdal déphasé par bot, pas de
  dépendance à une crate `rand`) pendant 10 s (après 1 s de chauffe exclue de la
  mesure). Mesure le temps de traitement serveur par tick, la taille réelle des
  `Snapshot` diffusés et des `Input` envoyés (via `protocol::encode`, pas une
  estimation).
- [x] Mesuré (`cargo run --release --example load_test_client`, machine de dev) :
  ```
  Joueurs simultanés       : 16
  Temps de traitement/tick : moyenne 0.34-0.40 ms, max ~1.7-3.0 ms (budget 50 ms à 20 Hz)
  Taille moyenne d'un Snapshot (16 joueurs) : 368 octets
  Débit descendant (serveur -> 1 client)    : 6.76 Ko/s/joueur
  Débit montant total (16 joueurs)          : 4.11 Ko/s (0.26 Ko/s/joueur)
  Taille moyenne d'un Input                 : 14 octets
  ```
- [x] **Aucune optimisation nécessaire à cette échelle** : le temps de traitement
  serveur (~0,4 ms) utilise moins de 1 % du budget de tick (50 ms à 20 Hz), et le
  débit descendant (6,76 Ko/s/joueur) est ~30× sous un objectif large de
  200 Ko/s/joueur. Compression, fréquence adaptative et regroupement de messages
  (prévus dans ce sprint *si besoin*) ne sont donc pas justifiés à 16 joueurs —
  décision fondée sur la mesure, pas anticipée par prudence.
- **Fichiers** : nouveau `examples/load_test_client.rs`.
- **Livrable** : chiffres publiés ci-dessus (mesurés, pas estimés) ; `cargo test`
  toujours vert (130 lib + 2 bin), `clippy`/`fmt` propres.
- **Limite assumée** : mesuré sur une seule machine (client + serveur dans le même
  processus, latence réseau ~nulle) — ne mesure pas l'effet d'une vraie latence
  réseau (RTT) sur le débit ressenti, seulement le coût CPU/bande passante brut.
  Cohérent avec le scope de ce sprint (charge et bande passante, pas latence —
  déjà couverte par l'interpolation/réconciliation du Sprint 54).
- **Risques levés** : la mesure confirme les projections du Sprint 52 (encodage
  bincode compact) à l'échelle réellement visée par le projet.

### Sprint 62 — Déploiement serveur
**Objectif** : un serveur accessible depuis Internet, pas seulement en LAN.
- [ ] Choix d'hébergement (VPS simple pour commencer — pas besoin d'auto-scaling à
  cette échelle de 2-16 joueurs/salon).
- [ ] Packaging du binaire serveur (cross-compile Linux si dev sur macOS), script de
  déploiement dans `packaging/` (sur le modèle des scripts `.dmg`/`.apk` existants).
- [ ] Config réseau (port, TLS pour le WebSocket — `wss://` obligatoire si le client
  tourne sur un domaine HTTPS/mobile).
- [ ] Mettre à jour le README avec l'état multijoueur (comme le fait déjà le README
  pour l'état par plateforme).
- **Fichiers** : `packaging/deploy_server.sh` (nouveau), `README.md`.
- **Livrable** : deux joueurs sur des réseaux différents (pas le même LAN) jouent une
  manche ensemble via le serveur déployé.
- **Risques** : coûts d'hébergement récurrents (même modeste) — à valider avec le
  budget avant de laisser tourner en continu.

### Sprint 63 — Client réseau desktop & fenêtre Multijoueur ✅ FAIT (hors plan initial)
**Objectif** (demandé directement par l'utilisateur, pas dans le plan de sprints
d'origine : « pouvoir générer deux applications et jouer ensemble ») : brancher
enfin `NetClient` dans la boucle réelle de l'éditeur/du player, avec une UI pour
s'y connecter — le câblage explicitement reporté aux Sprints 54/55.
- [x] `src/app/network_client.rs` (nouveau, desktop-only comme `ai.rs`/
  `multiplayer.rs` réseau) : `connect_to_server`/`disconnect_from_server`/
  `is_connected`, et `poll_network` (appelée depuis `advance_play`, comme
  `poll_imports`/`poll_ai`) : envoie l'`Input` du joueur local à chaque frame,
  draine les `ServerMsg` reçus, affiche les **autres** joueurs comme des objets
  fantômes (`RemotePlayer`, position interpolée via `net::interpolation::
  RemoteEntity` du Sprint 54 — enfin utilisé). Le joueur local reste piloté par
  prédiction, inchangé.
- [x] **Extension de protocole nécessaire** : `EntityDelta` gagne un champ
  `player_id: Option<PlayerId>` — sans lui, un client ne pouvait pas distinguer
  « mon propre joueur » (à ne jamais écraser) des « autres joueurs » (à afficher
  en fantôme) dans un `Snapshot`, qui ne portait que des indices d'objets côté
  serveur. `AppState::network_snapshot` mis à jour pour le renseigner.
- [x] Fenêtre **🌐 Multijoueur** dans l'éditeur (`editor/mod.rs`) : adresse du
  serveur, pseudo, bouton Se connecter/Se déconnecter, statut — même convention
  que la fenêtre Paramètres (Sprint 56).
- [x] Test bout-en-bout à travers de vrais sockets (`app::network_client::tests`) :
  deux `AppState` (Alice, Bob) connectées au même serveur de test voient chacune
  **exactement un** fantôme (l'autre), jamais elles-mêmes.
- [x] **Bug critique trouvé et corrigé en testant réellement** (cf.
  AUDIT_MMORPG.md §4.5) : le serveur perdait la manche tout seul en 2,5-4,5 s,
  avant qu'un joueur n'ait eu le temps de se connecter — un monstre, puis le
  gabarit inerte, se faisaient désigner « le joueur » par l'heuristique
  `player_index` (qui retombait sur « premier objet scripté visible », et les
  monstres ont aussi un script). Corrigé : `player_index` exclut les monstres de
  ce repli et n'a plus de repli « premier objet quel qu'il soit » (pouvait
  désigner le sol) ; nouvelle méthode `AppState::hide_local_player_template()`,
  appelée par `src/bin/server.rs` avant `playing = true`. Test de régression :
  `waiting_for_the_first_player_never_drains_health_via_monster_scripts`.
- [x] **Câblage Firebase du Join** (ajouté juste après, même sprint) : la
  fenêtre Multijoueur gagne une section « Compte (optionnel) » (email/mot de
  passe, Se connecter/Créer un compte), visible seulement si une clé API et
  une URL Database sont renseignées dans les Paramètres. `AppState::
  request_firebase_sign_in`/`request_firebase_sign_up` (thread de fond, comme
  les requêtes IA existantes) résolvent un `uid`, mémorisé et **transmis au
  prochain `connect_to_server`** — sans ce câblage, `firebase_uid` restait
  toujours `None` et toute la progression/le classement des Sprints 57/59
  n'aurait jamais pu se relier à un vrai compte, même Firebase configuré.
  2 tests : le `uid` s'applique dès que la requête de fond résout (simulé,
  sans vrai projet Firebase), et `connect_to_server` le transmet bien au
  `Join` reçu côté serveur (vrai socket).
- **Fichiers** : nouveau `src/app/network_client.rs` ; modifiés
  `src/net/protocol.rs` (+ `interpolation.rs`, tests), `src/app/multiplayer.rs`,
  `src/app/mod.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs`, `src/bin/server.rs`.
- **Livrable** : 136 tests lib + 2 tests bin verts, clippy/fmt propres ; serveur
  réel vérifié stable indéfiniment en attente d'un joueur (15 s+ testées, contre
  2,5-4,5 s de défaite automatique avant correctif) ; app desktop relancée et
  vérifiée sans crash avec la nouvelle UI.
- **Reste non vérifié** : aucune fenêtre graphique vue tourner dans cet
  environnement (pas d'affichage) — la fenêtre Multijoueur, la connexion
  Firebase et le mouvement des fantômes en jeu réel restent à valider
  visuellement par l'utilisateur, en lançant deux instances de l'éditeur l'une
  contre l'autre.
- **Limite assumée** : pas de réconciliation pour le joueur local (le
  `Snapshot` le concernant est ignoré, cf. Sprint 54) — en cas de désaccord
  fort avec le serveur (triche, désync), le client ne se corrige pas encore.

### Sprint 64 — Chat en jeu (hors plan initial) ✅ FAIT
**Objectif** (demandé directement par l'utilisateur) : brancher un vrai chat dans la
fenêtre Multijoueur, sur le backend Firebase déjà écrit (Sprint 58) mais jamais
relié à une UI.
- [x] `AppState::request_send_chat_message`/`request_refresh_chat` (thread de fond,
  même schéma que les requêtes IA/Firebase existantes) : postent/lisent
  `/lobbies/{code}/chat` via `net::firebase::post_chat_message`/`list_chat_messages`.
  Envoyer un message nécessite un compte connecté (`firebase_id_token`, les
  règles RTDB réservent l'écriture aux comptes authentifiés) ; lire ne le
  nécessite pas.
- [x] Nouveau type universel `network_client::ChatLine { sender, text }` — évite
  d'exposer `net::firebase::ChatMessage` (absent des cibles mobiles) sur l'API
  publique d'`AppState` : `chat_messages: Vec<ChatLine>` reste un champ normal,
  sans gating par plateforme.
- [x] Fenêtre Multijoueur : section « Chat » (code de salon, historique
  défilant, champ de saisie + Envoyer, bouton Rafraîchir), visible dès que
  Firebase est configuré (indépendant d'être connecté au serveur de jeu — le
  chat passe par Firebase REST, pas par le WebSocket du protocole de jeu).
- [x] 2 tests : envoyer sans compte connecté est un no-op propre (pas de requête
  réseau démarrée) ; le résultat d'une requête Firebase simulée s'applique bien
  via `poll_network`.
- **Fichiers** : `src/app/network_client.rs`, `src/app/mod.rs` (champs),
  `src/editor/mod.rs` (fenêtre), `src/gfx/renderer.rs` (application des actions).
- **Livrable** : 137 tests lib + 2 tests bin verts, clippy/fmt propres ; app
  desktop relancée et vérifiée sans crash avec la nouvelle section.
- **Reste non vérifié** : comme le reste de l'UI, jamais vu tourner dans une
  vraie fenêtre ici (pas d'affichage) — et le chat n'a jamais été testé contre
  un vrai projet Firebase (même réserve que les Sprints 56-59). Pas de
  rafraîchissement automatique (bouton manuel) : un vrai auto-poll périodique
  reste à ajouter si l'usage le justifie.

### Sprint 65 — Classement en jeu (hors plan initial) ✅ FAIT
**Objectif** : même geste que le Sprint 64, pour le classement (backend Sprint 59,
jamais relié à une UI).
- [x] `AppState::request_refresh_leaderboard` (thread de fond) : lit
  `net::firebase::get_top_leaderboard` (lecture publique, pas besoin de compte —
  l'écriture reste réservée au serveur de jeu, `src/bin/server.rs`, cf. Sprint 59).
- [x] Nouveau type universel `network_client::LeaderboardLine { name, score }`,
  même raison qu'`ChatLine` (évite d'exposer `net::firebase::LeaderboardEntry`,
  absent des cibles mobiles, sur l'API publique d'`AppState`).
- [x] Section « Classement » dans la fenêtre Multijoueur : liste triée (déjà
  triée côté backend) + bouton Rafraîchir.
- [x] 1 test : le résultat d'une requête simulée s'applique bien via `poll_network`.
- **Fichiers** : `src/app/network_client.rs`, `src/app/mod.rs` (champs),
  `src/editor/mod.rs` (fenêtre), `src/gfx/renderer.rs` (application de l'action).
- **Livrable** : 138 tests lib + 2 tests bin verts, clippy/fmt propres ; app
  desktop relancée et vérifiée sans crash avec la nouvelle section.
- **Reste non vérifié** : même réserve que le Sprint 64 (pas d'affichage ici,
  pas de vrai projet Firebase testé).

---

## Correspondance décision de scope → sprint

| Point du scope (§0) | Sprint(s) couvrant | État |
|---|---|---|
| Extraction préalable du combat (bloquant, cf. AUDIT §7.4) | 50 | ✅ |
| Serveur autoritaire headless | 51 | ✅ |
| Protocole + transport réseau | 52, 53 | ✅ |
| Jouabilité malgré latence (prédiction/interpolation) | 54, 63 | ✅ (logique Sprint 54, câblage réel Sprint 63) |
| Salons 2-16 joueurs, réutilisation du mode manches | 55, 63 | ✅ (serveur Sprint 55, client Sprint 63) |
| Firebase RTDB — comptes | 56 | 🟢 (backend fait, UI connexion non branchée) |
| Firebase RTDB — progression persistante | 57 | 🟢 (backend + serveur faits) |
| Firebase RTDB — chat/présence | 58, 64 | ✅ (chat câblé Sprint 64 ; présence encore backend seul) |
| Firebase RTDB — classement | 59, 65 | ✅ (câblé Sprint 65) |
| Anti-triche de base (validation serveur) | 60 | ✅ |
| Validation charge/perf réseau | 61 | ✅ |
| Client réseau desktop + fenêtre Multijoueur | 63 | ✅ |
| Mise en production (hors LAN) | 62 | ⬜ |

---

## Notes de suivi (à compléter au fil des sprints)

> Ajouter ici, à chaque sprint terminé, une ligne courte : date, écarts vs
> l'estimation, décisions prises en cours de route. Objectif : que la prochaine
> session (ou une autre) comprenne le *pourquoi* sans relire tout l'historique git.

- **2026-07-07 — Sprints 50-61.** Enchaînés sans interruption dans une même session.
  Écart majeur vs l'estimation initiale : plusieurs sprints prévus avec UI egui
  (lobby, écran de connexion, chat, classement) ont été **volontairement reportés**
  — cet environnement d'exécution n'a pas d'affichage graphique pour les valider,
  et une UI écrite sans jamais être vue tourner est un risque, pas un gain. À la
  place, chaque sprint a livré son backend/logique, testé automatiquement
  (unitaire + bout-en-bout via de vrais sockets locaux quand pertinent). Sprints
  56-59 (Firebase) : code écrit et testé unitairement, mais **jamais vérifié
  contre un vrai projet Firebase** (aucun disponible ici) — à valider par
  l'utilisateur dès qu'un projet est configuré. Sprint 61 : chiffres **mesurés**
  (pas estimés) confirmant une large marge à 16 joueurs (voir sa section) —
  aucune optimisation réseau nécessaire à cette échelle.

- **2026-07-07 — Audit du chantier multijoueur ([AUDIT_MMORPG.md](AUDIT_MMORPG.md)).**
  Revue critique post-Sprint 61. Un bug réel trouvé et corrigé : un second
  `Join` du même client (rejeu réseau, bug client, trame forgée) créait un
  objet fantôme simulé indéfiniment sans jamais nettoyer le premier
  (`spawn_network_player` n'était pas idempotent) — corrigé, test de
  régression ajouté. Deux limites latentes également corrigées dans la foulée
  (peu coûteuses, avec test de régression, aucune régression sur les 130+2
  tests existants) : `AppState::clear_network_players()` oublie les joueurs
  réseau à chaque reset de scène (`restart_game`/transition Play→Edit), et
  `NetServer`/`NetClient` utilisent désormais un runtime tokio
  `current_thread` dédié (thread de fond qui le `block_on` en continu) au lieu
  d'un runtime multi-thread complet par connexion — mesuré : 30 threads OS au
  total pour le test de charge à 16 joueurs, contre plus de 150 estimés avant.
  Détail complet : [AUDIT_MMORPG.md](AUDIT_MMORPG.md).

- **2026-07-07 — Sprint 63 (hors plan, demandé par l'utilisateur).** Câblage
  réel de `NetClient` dans l'éditeur (fenêtre Multijoueur + `poll_network`),
  explicitement reporté aux Sprints 54/55 faute d'affichage graphique pour le
  valider — resté vrai ici aussi (pas de fenêtre vue tourner), mais un test
  bout-en-bout à deux `AppState` réelles a quand même trouvé un **bug bloquant
  pour l'objectif demandé** : le serveur perdait la manche tout seul avant
  qu'un joueur ne rejoigne (cf. AUDIT_MMORPG.md §4.5). Corrigé. Point notable :
  c'est le seul bug de toute la session trouvé en **exécutant** le serveur
  réel plutôt qu'en relisant le code ou en lançant les tests déjà écrits —
  aucun test antérieur ne faisait tourner une manche assez longtemps pour
  l'exposer. Prochaine étape naturelle si l'utilisateur veut aller plus loin :
  tester réellement deux instances de l'éditeur l'une contre l'autre (lui
  seul peut le faire, cet environnement n'a pas d'affichage).

### Sprint 80 — Vie individualisée, IA multi-cibles, soin coopératif ✅ FAIT
Traduction en code de [GAMEDESIGN_EN_LIGNE.md](GAMEDESIGN_EN_LIGNE.md) §3.1/
§3.2/§3.4/§3.6, dans l'ordre de priorité qu'il recommandait.

**§3.1 — Vie individualisée (nouveau `src/app/health.rs`).**
`AppState::network_health: HashMap<PlayerId, f32>` remplace, côté
multijoueur, le champ `hud_health` scalaire (pensé pour un joueur local
unique) : chaque joueur réseau a sa propre vie (0..1), drainée au contact
d'un monstre `AiChaser` visible et régénérée passivement hors contact. Un
joueur à 0 PV devient **spectateur** (objet masqué, mouvement/attaque/tir
ignorés côté serveur) sans mettre fin à la manche pour les autres —
`AppState::is_room_lost()` (défaite de *salon*, tous vaincus) remplace
`is_lost()` dans `src/bin/server.rs` dès qu'un salon a des joueurs réseau ;
`is_lost()` reste inchangé en solo, aucune régression. `EntityDelta::health`
(prévu de longue date, jamais rempli) porte désormais la vie de chaque
joueur ; `GameEvent::PlayerDown` prévient les clients d'une mort (son, une
fois, pour soi-même seulement).

**§3.2 — IA multi-cibles.** `chase_target` (point unique, `self.
player_position()` — désignait toujours le premier joueur réseau à avoir
rejoint sur un serveur headless) devient `candidate_targets: Vec<Vec3>` :
en solo inchangé, en réseau chaque joueur **vivant** et visible. Chaque
`AiChaser` recalcule sa cible la plus proche à chaque frame. Les 5 monstres
de la carte embarquée portent désormais `ai_chaser` (ils poursuivent
réellement, sinon la vie individualisée du §3.1 n'aurait aucun effet
observable).

**§3.6 — Soin coopératif.** Touche **H** ou bouton tactile **« Soin »**
(`Controller::heal_button`, nouveau) : maintenu, soigne en continu l'allié
vivant le plus proche et blessé à 2,5 m (0,2 PV/s), résolu et validé côté
serveur (`update_network_heal`) — universel (pas de gate par rôle, §3.5 non
traité ce sprint) et sans réanimation (mort = spectateur permanent pour la
manche, décision assumée).

**§3.4 — Vie/identité affichées : backend fait, HUD reporté.**
`RemotePlayer::health`/`AppState::net_local_health` mémorisent la dernière
vie reçue par joueur ; `AppState::multiplayer_roster()` expose `(nom, vie,
soi-même ?)`. Le panneau HUD lui-même est **reporté** : une autre session
travaillait en parallèle sur `src/editor/mod.rs`/`src/gfx/renderer.rs`
(Sprint 81, `time_scale`) au moment de ce sprint — cf. l'incident ci-dessous.

**Incident de session concurrente (géré, aucune perte).** Une autre session
Claude Code éditait ce dépôt en parallèle. Deux ajouts à
`src/net/protocol.rs` (`ClientMsg::Input::heal`, `GameEvent::PlayerDown`) ont
été silencieusement écrasés une première fois (le fichier réécrit sans ces
champs, alors que l'outil d'édition rapportait un succès) ; détecté en
relisant le fichier avant l'étape suivante, refait et vérifié par `grep`
immédiat après chaque édition sur ce fichier. Une collision plus visible
(erreurs de compilation) est ensuite survenue sur `editor/mod.rs`/
`renderer.rs` pendant que l'autre session y ajoutait `time_scale` — résolue
en attendant que cette session atteigne un état stable plutôt que de
continuer à enfiler des paramètres dans les mêmes signatures. Cf.
`concurrent-sessions-hazard` (mémoire du projet).

**Tests** : 10 nouveaux dans `health.rs` (contact, régénération, mort,
entrées ignorées après la mort, salon perdu seulement si tous vaincus, soin
en/hors portée, priorité au plus blessé), 2 dans `fireball.rs`/`mod.rs`
(tir bloqué après la mort, poursuite du joueur réseau le plus proche) — 209
tests au total, tous verts ; `cargo fmt`/`clippy --all-targets -D warnings`
propres.
**Fichiers** : `src/app/health.rs` (nouveau), `src/net/protocol.rs`,
`src/app/mod.rs`, `src/app/multiplayer.rs`, `src/app/fireball.rs`,
`src/app/network_client.rs`, `src/bin/server.rs`, `src/scene/mod.rs`,
`src/lib.rs`, `assets/player_scene.json`, `examples/*.rs`.
