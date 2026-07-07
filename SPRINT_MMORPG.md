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
| 59 | Firebase : classement (leaderboard) | ⬜ |
| 60 | Durcissement réseau & anti-triche de base | ⬜ |
| 61 | Tests de charge & optimisation bande passante | ⬜ |
| 62 | Déploiement serveur | ⬜ |

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

### Sprint 59 — Classement (leaderboard)
**Objectif** : classement global des meilleurs scores.
- [ ] Nœud RTDB `/leaderboard` (top scores, requête triée côté client via les query
  params REST `orderBy`/`limitToLast`).
- [ ] Écriture du score en fin de manche par le **serveur de jeu** (même raison
  qu'au Sprint 57 : pas d'écriture client directe sur une donnée compétitive).
- [ ] UI : écran classement accessible depuis le lobby.
- **Fichiers** : `src/net/firebase.rs`, `src/bin/server.rs`, `src/editor/mod.rs`.
- **Livrable** : un score de fin de manche apparaît dans le top classement affiché en jeu.
- **Risques** : volumétrie — si `/leaderboard` grossit sans limite, prévoir une purge
  (garder top N) plutôt qu'un nœud illimité.

---

## PHASE Q — Robustesse & mise en production

### Sprint 60 — Durcissement réseau & anti-triche de base
**Objectif** : le serveur ne fait confiance à aucune donnée client brute.
- [ ] Validation serveur de tous les `Input` reçus (bornes de vitesse, cooldown
  d'attaque réel côté serveur — pas seulement affiché côté client).
- [ ] Timeout/reconnexion : un client qui ne répond plus est retiré proprement du
  salon après délai ; un client qui se reconnecte dans la fenêtre de grâce reprend sa
  place.
- [ ] Tests dédiés (sur le modèle des tests gameplay existants nommés en intention,
  ex. `server_rejects_attack_before_cooldown_elapsed`).
- **Fichiers** : `src/net/server_loop.rs`, `src/app/combat.rs`, tests associés.
- **Livrable** : un client modifié pour spammer des inputs invalides ne casse pas la
  partie des autres joueurs (testé manuellement avec un client de test dédié).
- **Risques** : périmètre volontairement limité — pas de détection d'aimbot/wallhack
  ici (hors scope d'un jeu 2-16 joueurs solo-dev), seulement la validation serveur des
  règles de jeu de base.

### Sprint 61 — Tests de charge & optimisation bande passante
**Objectif** : confirmer que 16 joueurs/salon restent fluides et peu coûteux en réseau.
- [ ] Script de charge : N clients factices (headless, sans rendu) rejoignant un même
  salon, mesurant tick serveur et taille moyenne des snapshots.
- [ ] Optimisations si besoin : compression des deltas, fréquence réseau adaptative
  (baisser le tick si peu de changement), regroupement des messages.
- [ ] Documenter les chiffres obtenus (ticks/s soutenus, Ko/s par joueur) dans ce
  document, section Suivi.
- **Fichiers** : nouveau `examples/load_test_client.rs` (sur le modèle des exemples
  existants dans `examples/`), `src/net/protocol.rs`.
- **Livrable** : mesure chiffrée publiée (ex. « 16 joueurs, tick 20 Hz, X Ko/s/joueur,
  Y ms de traitement serveur par tick ») + éventuelles optimisations appliquées.
- **Risques** : sans ce sprint, le coût réseau réel n'est qu'une estimation du Sprint
  52 — ne pas sauter cette étape avant un déploiement public.

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

---

## Correspondance décision de scope → sprint

| Point du scope (§0) | Sprint(s) couvrant | État |
|---|---|---|
| Extraction préalable du combat (bloquant, cf. AUDIT §7.4) | 50 | ✅ |
| Serveur autoritaire headless | 51 | ✅ |
| Protocole + transport réseau | 52, 53 | ✅ |
| Jouabilité malgré latence (prédiction/interpolation) | 54 | 🟢 (logique faite, câblage → 55) |
| Salons 2-16 joueurs, réutilisation du mode manches | 55 | 🟢 (serveur fait, lobby UI → plus tard) |
| Firebase RTDB — comptes | 56 | ⬜ |
| Firebase RTDB — progression persistante | 57 | ⬜ |
| Firebase RTDB — chat/présence | 58 | ⬜ |
| Firebase RTDB — classement | 59 | ⬜ |
| Anti-triche de base (validation serveur) | 60 | ⬜ |
| Validation charge/perf réseau | 61 | ⬜ |
| Mise en production (hors LAN) | 62 | ⬜ |

---

## Notes de suivi (à compléter au fil des sprints)

> Ajouter ici, à chaque sprint terminé, une ligne courte : date, écarts vs
> l'estimation, décisions prises en cours de route. Objectif : que la prochaine
> session (ou une autre) comprenne le *pourquoi* sans relire tout l'historique git.

- _(vide pour l'instant — premier sprint à démarrer : **50**)_
