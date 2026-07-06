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
| 54 | Prédiction client & interpolation | ⬜ |
| 55 | Salons multijoueurs (lobby, join/leave) | ⬜ |
| 56 | Firebase : comptes & auth | ⬜ |
| 57 | Firebase : inventaire/progression | ⬜ |
| 58 | Firebase : chat + présence | ⬜ |
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

### Sprint 54 — Prédiction client & interpolation
**Objectif** : rendre le jeu jouable malgré la latence réseau (le serveur est à 20 Hz,
le rendu à 60 Hz).
- [ ] **Joueur local** : appliquer l'input immédiatement côté client (prédiction),
  puis réconcilier silencieusement à réception du snapshot serveur (correction de
  position si écart, sans à-coup visible).
- [ ] **Autres joueurs/monstres** : interpolation entre les deux derniers snapshots
  reçus (pas de téléportation à chaque tick réseau).
- [ ] Test de régression : simuler une latence artificielle (delay configurable sur le
  socket client) et vérifier que le mouvement local reste fluide.
- **Fichiers** : `src/app/mod.rs`, `src/net/client.rs`, nouveau `src/net/interpolation.rs`.
- **Livrable** : jouable à 150 ms de latence simulée sans sensation de lag sur son
  propre joueur ; mouvement des autres joueurs lisse (pas de saccades au tick réseau).
- **Risques** : la réconciliation serveur mal réglée peut créer des micro-saccades —
  garder un seuil de correction (« snap » seulement au-delà d'un écart significatif).

### Sprint 55 — Salons multijoueurs (lobby, join/leave)
**Objectif** : brancher le réseau sur le mode manches existant, jouable à plusieurs.
- [ ] Écran de lobby minimal (egui) : créer un salon / rejoindre par code, liste des
  joueurs connectés avant le lancement.
- [ ] Le mode manches (`Combat`/`AiChaser`, vagues) tourne **sur le serveur** ; les
  clients ne font que prédire/afficher (cf. Sprint 0 du scope : autorité serveur).
- [ ] Gestion propre de la déconnexion d'un joueur en cours de manche (ne bloque pas
  les autres).
- [ ] Test manuel : 3-4 joueurs sur la même manche, un qui quitte en cours de partie.
- **Fichiers** : `src/editor/mod.rs` (écran lobby), `src/app/mod.rs`, `src/net/*`.
- **Livrable** : une manche « Call of Zombies » jouable à plusieurs, en local réseau
  (LAN), du lobby jusqu'au score final.
- **Risques** : resynchronisation d'un client qui rejoint en cours de manche (aucun
  snapshot initial complet reçu) — prévoir un message `ServerMsg::FullState` dédié au
  join tardif si besoin.

---

## PHASE P — Firebase Realtime Database (backend annexe)

> Rappel de scope : Firebase ne transporte **jamais** de position/combat en jeu ici —
> uniquement des données peu fréquentes (compte, chat, classement, présence).
> Pas de SDK Rust officiel Firebase → accès via l'API REST (déjà `ureq` en dépendance
> pour l'IA) + endpoint SSE de RTDB pour les mises à jour poussées (chat/présence).

### Sprint 56 — Comptes & authentification
**Objectif** : identifier les joueurs de façon persistante entre les sessions.
- [ ] Firebase Auth (REST : `signUp`/`signInWithPassword`) pour obtenir un `idToken`.
- [ ] RTDB : nœud `/users/{uid}/profile` (pseudo, date de création) protégé par les
  règles de sécurité RTDB (lecture publique du pseudo, écriture seulement par l'`uid`
  propriétaire, vérifiée via `auth != null && auth.uid === $uid`).
- [ ] Client : écran de connexion/inscription minimal (egui), token stocké en mémoire
  (pas de fichier en clair).
- [ ] Documenter noms de clé API/config dans les Paramètres (comme le modèle IA
  DeepSeek existant dans `src/app/ai.rs`) — jamais commit de clé en dur.
- **Fichiers** : nouveau `src/net/firebase.rs`, `src/editor/mod.rs` (écran connexion),
  `src/app/settings.rs`.
- **Livrable** : créer un compte, se reconnecter, pseudo affiché en jeu.
- **Risques** : les clés Firebase Web sont publiques par design (la sécurité vient des
  **règles RTDB**, pas du secret de la clé) — écrire les règles de sécurité *avant*
  d'exposer le nœud en écriture, pas après.

### Sprint 57 — Inventaire & progression persistante
**Objectif** : garder l'XP/objets d'un joueur entre les parties.
- [ ] Nœud RTDB `/users/{uid}/progress` (niveau, XP, objets débloqués).
- [ ] Écriture en fin de manche (score → XP), lecture au login.
- [ ] Résolution de conflit simple : dernière écriture serveur fait foi (le serveur de
  jeu du Sprint 51 pousse la progression, pas le client, pour éviter la triche côté
  client sur les gains).
- **Fichiers** : `src/net/firebase.rs`, `src/bin/server.rs` (appel Firebase en fin de
  manche), `src/app/combat.rs`.
- **Livrable** : rejouer une seconde session affiche bien le niveau/XP cumulés de la
  session précédente.
- **Risques** : le serveur de jeu doit détenir ses propres identifiants Firebase
  (service account / règles dédiées), différents du token utilisateur — à séparer
  proprement pour ne pas donner au client un accès en écriture directe à sa progression.

### Sprint 58 — Chat de salon & présence
**Objectif** : communication texte + liste des joueurs en ligne.
- [ ] Nœud RTDB `/lobbies/{code}/chat` (liste de messages horodatés) + `/presence/{uid}`
  (statut en ligne, `onDisconnect` RTDB pour nettoyer automatiquement à la déconnexion).
- [ ] Client : écoute en SSE (flux `EventSource` du REST RTDB) pour recevoir les
  nouveaux messages/présences sans polling.
- [ ] UI chat minimal dans le lobby (egui).
- **Fichiers** : `src/net/firebase.rs`, `src/editor/mod.rs`.
- **Livrable** : deux clients dans le même salon voient les messages de l'autre en
  temps quasi réel, et se voient apparaître/disparaître de la liste de présence.
- **Risques** : SSE + `ureq` (bloquant) ne cohabitent pas bien avec la boucle `winit` —
  probablement nécessaire de le faire tourner sur le même thread réseau que le
  WebSocket de jeu (`tokio`), pas un `ureq` bloquant de plus sur le thread principal.

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
| Jouabilité malgré latence (prédiction/interpolation) | 54 | ⬜ |
| Salons 2-16 joueurs, réutilisation du mode manches | 55 | ⬜ |
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
