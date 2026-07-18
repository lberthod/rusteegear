# PHASE C — Modes de manche : rapport de sprint (2026-07-18)

> Suite de [sprint10audit.md](sprint10audit.md) (Phase C, Sprints 5→8). Ce fichier documente
> ce qui a réellement été livré dans cette session, à quel niveau de confiance, et ce qui reste.
>
> **Mise à jour (auto-relecture ×2)** : deux passages de relecture après la livraison initiale ont
> trouvé (1) un bug réel dans le marqueur « salon neuf » de `Lobby::objective`, et (2) un vrai trou
> fonctionnel — le mode choisi n'était jamais renvoyé au client. Les deux sont corrigés ci-dessous,
> uniquement dans les fichiers réseau/protocole de cette phase (`src/bin/server.rs`,
> `src/net/protocol.rs`, `src/app/network_client.rs`, `src/app/multiplayer.rs`), sans toucher aux
> fichiers des autres phases.

## Fait

### Sprint 5 — Fondation `RoundObjective` ✅

- `RoundObjective` (`Vagues` / `Survie` / `Escorte` / `Boss`), `#[derive(..., Default)]` avec
  `Vagues` par défaut — [`src/app/multiplayer.rs`](src/app/multiplayer.rs), sur le même principe
  que `PlayerClass` (`from_u8`/`to_u8`, valeur hors table repliée sur `Vagues`, jamais de connexion
  refusée pour ça).
- `AppState::objective` (défaut `Vagues`, zéro régression) — [`src/app/mod.rs`](src/app/mod.rs).
- `ClientMsg::Join` gagne `objective: u8`, `PROTOCOL_VERSION` 4 (bump partagé avec le Sprint 2
  d'une autre session en cours, `GameEvent::PlayerDown::cause` — un seul bump groupé, cf. le
  commentaire sur `PROTOCOL_VERSION`) — [`src/net/protocol.rs`](src/net/protocol.rs). Câblé dans
  tous les points de construction/décodage du `Join` : `src/net/client/native.rs`,
  `src/net/client/web.rs`, `src/net/server_loop.rs` (décodage + relai), et les littéraux de test
  (`src/net/protocol.rs`, `src/net/server_loop.rs`, `src/app/network_client.rs`). Comme `class`
  avant le Sprint 3, pas encore câblé à un sélecteur UI : `objective: 0` (Vagues) envoyé par tous
  les clients pour l'instant.
- `bin/server.rs::Lobby::objective` (`Option<RoundObjective>`, `None` tant qu'aucun `Join` n'a
  jamais été traité par ce `Room`) : fixé au **premier** `Join` reçu, ignoré pour les joueurs
  suivants — un salon joue un seul mode pour toute sa durée de vie, comme son code. Réappliqué à
  `Room::app.objective` par `Room::restart()` (sinon chaque manche suivante retomberait sur
  `Vagues`, défaut d'`AppState::new()`).
- `AppState::update_round()` ([`src/app/combat.rs`](src/app/combat.rs)) : point d'entrée générique
  appelé depuis `advance_play` (`src/app/simulation.rs`, à la place de l'ancien appel direct à
  `update_waves()`), qui dispatche sur `self.objective`. `Escorte`/`Boss` (pas encore implémentés)
  retombent sur `update_waves()` plutôt que de laisser une manche ne jamais se terminer.
- `init_waves()` (révélation de la manche 1) reste identique et partagée par tous les modes — la
  divergence entre modes ne porte que sur la condition de victoire/défaite, pas sur l'entrée en jeu.

### Sprint 6 — Mode Survie ✅ (partiel : logique faite, HUD dédié non fait)

- `AppState::update_survie()` ([`src/app/combat.rs`](src/app/combat.rs)) : victoire à
  `SURVIE_DURATION_SECS` (180 s, `self.time` — remis à 0 à l'entrée en Play) écoulées ; contrairement
  à `update_waves`, vider la dernière manche **ne gagne pas** la partie, elle reboucle sur la manche 1
  (monstres re-révélés, physique reconstruite) pour maintenir la pression jusqu'au chrono. La défaite
  (wipe) reste détectée à côté par `AppState::is_room_lost` (générique, déjà utilisé par `Vagues`,
  aucun changement nécessaire).
- Testé : [`survie_mode_loops_the_wave_then_wins_once_the_timer_elapses`](src/app/mod.rs) (nouveau,
  vérifie que vider l'unique manche reboucle sans gagner, puis que le chrono écoulé déclenche
  `win_time` indépendamment de l'état des monstres) — vert.
- **Non fait** : HUD dédié (minuteur de survie). `hud::wave_hud` affiche déjà « Vague N/M » y compris
  en Survie (mode-agnostique, aucune régression), mais pas de minuteur ni de distinction visuelle
  Survie/Vagues. Volontairement pas touché : `src/editor/mod.rs` et `src/gfx/renderer.rs` avaient une
  refonte active d'une autre session en cours pendant tout ce sprint (câblage `DeathCause` du
  Sprint 2, puis sélecteur de classe du Sprint 3) — y toucher aurait risqué un conflit d'écriture sur
  des fichiers volumineux en cours de modification concurrente (cf. `sprint10audit.md`, plusieurs
  sessions actives simultanément le 2026-07-18 : Phase B terminée entre-temps, Phase E en cours,
  travail d'optimisation gfx également détecté). À faire dans un sprint séparé, une fois ces fichiers
  stables.

## Bug trouvé et corrigé (auto-relecture, après la première passe)

En repassant sur ce sprint, j'ai trouvé un vrai bug dans le marqueur « salon fraîchement créé »
utilisé au Sprint 5 pour décider si `objective` doit être fixé par ce `Join` : la première version
utilisait `room.lobby.last_seen.is_empty()`. Or `last_seen` redevient vide dès que **tous** les
joueurs quittent un salon — ce qui arrive couramment sans que le salon ne soit fermé (fermeture
réservée à la boucle `main`, uniquement quand la manche est *décidée* — victoire/défaite —, cf.
`a_room_closes_once_its_last_player_leaves`, test déjà existant qui documente précisément ce
comportement, corroboré en le lançant : `cargo test --bin server --features net_tests` — 17/17
verts avant et après le correctif). Conséquence concrète : un salon en `Survie`, vidé de tous ses
joueurs avant la fin des 180 s (déconnexions, tous partis), puis rejoint par un nouveau joueur
demandant `Vagues`,
voyait son mode réassigné **en pleine manche** — `wave`/scène restaient d'avant, mais
`update_round` se serait mis à appeler `update_waves` au lieu de `update_survie`, et la manche
aurait fini par déclarer victoire au lieu de reboucler.

**Corrigé** : `Lobby::objective` est devenu `Option<RoundObjective>` (`None` = « aucun `Join`
jamais traité par ce `Room` depuis sa création », un état qui ne redevient jamais vrai tant que le
`Room` vit) au lieu de dériver un marqueur de `last_seen`. Régression ajoutée :
[`a_room_keeps_its_objective_even_after_every_player_leaves_before_the_round_ends`](src/bin/server.rs)
(feature `net_tests`) — vert, avec les 16 autres tests `net_tests` de `bin/server.rs`. Seul
`src/bin/server.rs` a été modifié pour ce correctif (aucun fichier d'une autre phase touché).

## Deuxième passe d'audit — couverture de test renforcée

Relecture adversariale supplémentaire (recherche de trous de couverture, pas de nouveau bug trouvé
cette fois) : deux garanties de Sprint 5 n'avaient pas de test dédié bien qu'elles soient
implémentées correctement — comblé, toujours sans toucher aux fichiers d'une autre phase :

- `RoundObjective::from_u8`/`to_u8` (repli sur `Vagues` pour une valeur hors table, round-trip pour
  les 4 modes) n'avait pas l'équivalent des tests déjà existants pour `PlayerClass` — ajouté
  `round_objective_from_u8_falls_back_to_vagues_for_unknown_values` et
  `round_objective_to_u8_round_trips_through_from_u8` dans
  [`src/app/multiplayer.rs`](src/app/multiplayer.rs) (+ `RoundObjective::ALL`, même pattern que
  `PlayerClass::ALL`).
- Le repli d'`update_round` sur `update_waves` pour `Escorte`/`Boss` (Sprints 7/8 non implémentés)
  n'était vérifié qu'en lecture de code — ajouté
  `update_round_falls_back_to_vagues_for_the_not_yet_implemented_objectives` dans
  [`src/app/mod.rs`](src/app/mod.rs), qui vérifie que vider l'unique manche déclenche bien la
  victoire pour ces deux modes (pas de manche qui reste bloquée indéfiniment).

`cargo build --lib --bin server`, `cargo clippy --lib --bin server --no-deps` (aucun avertissement
sur les fichiers de ce sprint) et `rustfmt --check` sur tous les fichiers touchés : tous verts après
ce second passage.

**Verdict (avant la 3ᵉ passe ci-dessous, dépassé) :** le Sprint 5 (fondation) est maintenant couvert
et correct au niveau où je peux le vérifier sans lancer une vraie session Play/multijoueur. Le
Sprint 6 (Survie) est correct côté logique/tests mais reste sans vérification en conditions réelles
(aucun binaire lancé) et sans HUD dédié (gap déjà documenté, volontairement laissé pour ne pas
toucher aux fichiers d'éditeur/rendu en cours de refonte par une autre session). Sprints 7/8 non
commencés, comme annoncé.

## Troisième passe d'audit — trou fonctionnel trouvé et corrigé : le mode n'était jamais renvoyé au client

En creusant plus loin que la simple lecture de code côté serveur, j'ai vérifié une hypothèse
implicite de toute l'architecture réseau : **chaque client connecté exécute sa propre copie
locale de la logique de manche** (`AppState::advance_play` → `update_round`, cf. `app::combat`),
la même `AppState` que côté serveur, juste alimentée par les `Snapshot` pour la position/visibilité
des entités (`EntityDelta::visible` écrase directement `o.visible` en local — cf.
`app::network_client::handle_server_msg`, ligne ~848). `AppState::has_won()` (qui pilote l'écran de
victoire, `gfx/renderer.rs`) est lu **localement**, pas depuis un message serveur — `GameEvent::Win`/
`Lose` existent dans le protocole mais ne sont jamais diffusés (confirmé par grep, aucun `push`/
`send` de ces variants nulle part dans le code).

Conséquence : le Sprint 5 propageait `objective` **du client vers le serveur** (`ClientMsg::Join`)
mais jamais **du serveur vers les clients** une fois arbitré (`Lobby::objective`, premier `Join`
gagnant). Un client connecté à un salon `Survie` restait donc sur son défaut local `Vagues` — sa
propre `update_round` locale aurait appelé `update_waves` (pas `update_survie`), déclenchant un
écran de victoire **local et prématuré** dès que la dernière manche se vide localement, alors que
le serveur (autoritaire pour le score/XP/fin de manche réelle) continue de reboucler. Un bug de
Sprint 6 qui ne se serait révélé qu'en jouant réellement en multijoueur — exactement le type de
lacune que « couvrir tous les cas en lecture de code » ne peut pas voir tout seul.

**Corrigé**, dans les seuls fichiers réseau/protocole de cette phase :
- `GameEvent` gagne un variant `RoundObjective { objective: u8 }` ([`src/net/protocol.rs`](src/net/protocol.rs)),
  `PROTOCOL_VERSION` 4 → 5 (documenté en commentaire, comme les bumps précédents).
- `src/bin/server.rs` : après un `Join` réussi, en plus du `PlayerJoined` déjà diffusé, le serveur
  envoie désormais `GameEvent::RoundObjective` **au joueur qui vient de rejoindre**, reflétant
  `Lobby::objective` réellement arbitré pour ce salon.
- [`src/app/network_client.rs`](src/app/network_client.rs) : `handle_server_msg` gagne un cas pour
  ce nouvel évènement, qui aligne `AppState::objective` locale sur celle du serveur.

Testé bout-en-bout à travers un vrai socket —
[`a_joining_client_learns_the_rooms_objective_over_the_wire`](src/bin/server.rs) (feature
`net_tests`, connecte un vrai `NetClient` à un salon préréglé en `Survie`, vérifie la réception de
l'évènement avec la bonne valeur) — et en isolation côté client —
[`round_objective_event_aligns_our_local_objective_with_the_room`](src/app/network_client.rs). Les
deux verts, ainsi que l'ensemble `net_tests` (18/18) et la suite ciblée du crate.

**Verdict final** : Sprint 5 est maintenant complet dans les deux sens (client→serveur **et**
serveur→client), condition nécessaire pour que Sprint 6 (Survie) se comporte correctement en
multijoueur réel, pas seulement côté serveur. Reste non vérifiable sans lancer réellement le
binaire serveur + un client : le comportement visuel/HUD en conditions réelles (gap déjà noté).

## Sprints 7/8 — pris en charge par une autre session pendant cette même relecture

Pendant les passages d'audit ci-dessus, une **autre session** a commencé et visiblement terminé
Sprints 7 (Escorte) et 8 (Boss) directement dans `src/app/combat.rs`/`src/app/mod.rs`/
`src/scene/demos.rs` (`update_escorte`, `SceneObject::convoy`, tests
`update_round_boss_wins_when_its_single_wave_is_cleared`,
`update_round_escorte_wins_once_the_convoy_reaches_its_destination`,
`is_room_lost_true_when_the_escorte_convoy_is_destroyed_even_with_a_living_player`). `update_round`
a été étendu par cette session : `Escorte` a maintenant sa propre branche (`update_escorte`), `Boss`
retombe sciemment sur `update_waves` (une scène Boss est déjà « une seule manche contenant le
boss », cf. leur commentaire). Ce travail n'est **pas le mien** — je ne l'ai pas écrit, je ne l'ai
pas revu en détail, et je n'y ai plus touché une fois repéré (pour ne pas percuter une session
active sur le même fichier). La suite complète du crate (547 tests, 0 échec, cf. Vérification)
inclut leurs tests et les miens ensemble, tous verts au moment de la finalisation de ce rapport —
mais ça ne vaut ni relecture ni audit de leur code, seulement une confirmation que la compilation
et les tests existants passent. `sprint10audit.md` n'a pas encore ses cases Sprint 7/8 cochées ; je
les laisse à cette autre session plutôt que de les cocher pour un travail que je n'ai pas vérifié.

Statut réseau à noter : `Room::restart`/le Join initial propagent déjà `objective` pour ces deux
modes comme pour Vagues/Survie (Sprint 5, ci-dessus) — rien à refaire côté protocole pour eux.

## Vérification (état final, après les 3 passes d'audit)

- `cargo build --lib --bin server` : ✅.
- `cargo clippy --lib --bin server --no-deps` : aucun avertissement sur les fichiers de ce sprint.
- `cargo test --lib` (suite complète, état final incluant le travail Sprint 7/8 d'une autre
  session) : **547 passés / 0 échec** / 7 ignorés (outils de sync de scène, à lancer explicitement).
- `cargo test --bin server --features net_tests` : **18 passés / 0 échec**, dont les deux
  régressions ajoutées par ce sprint (`a_room_keeps_its_objective_even_after_every_player_leaves_
  before_the_round_ends`, `a_joining_client_learns_the_rooms_objective_over_the_wire`).
- `rustfmt --check` sur tous les fichiers touchés par ce sprint : propre.
- **Non vérifié** : partie Survie jouée en vrai (Play multijoueur avec un vrai salon, deux clients).
  Couverture actuelle : lecture de code + tests unitaires/intégration ciblés (dispatch
  `update_round`, bouclage de vague, déclenchement du chrono, non-régression de `Vagues`,
  propagation serveur→client de `objective` bout-en-bout via un vrai socket).

## Fichiers touchés (par cette session — pas les ajouts Sprint 7/8 d'une autre session)

`src/app/multiplayer.rs`, `src/app/mod.rs`, `src/app/network_client.rs`,
`src/net/protocol.rs`, `src/net/server_loop.rs`, `src/net/client/native.rs`,
`src/net/client/web.rs`, `src/bin/server.rs`, `sprint10audit.md` (cases Sprint 5/6 cochées).
`src/app/combat.rs`/`src/app/simulation.rs` ont aussi été touchés par cette session (fondation
Sprint 5/6) **et** par l'autre session (Sprint 7/8) — les deux corpus de changements coexistent
dans ces fichiers sans conflit (vérifié : build + suite complète verts).

## Note sur les sessions concurrentes

Cette session a travaillé sur un dépôt avec plusieurs autres sessions Claude Code actives en
parallèle pendant toute sa durée (Phase B/Sprint 4 assists, terminée et documentée dans
`sprintB10haudit.md` ; Sprint 2 diagnostic de mort et Sprint 3 sélecteur de classe, terminés en
cours de route dans `src/editor/mod.rs`/`src/gfx/renderer.rs`/`src/net/client/*.rs` ; Phase E
archétypes de créatures, tests rouges en cours de résolution puis verts en fin de session ; un
travail d'optimisation gfx également détecté via
`sprintD_optimisation10h.md`/`sprintoptimation3daudit10h.md` ; et, en toute fin de session, une
autre session a implémenté Sprints 7/8 de cette même Phase C directement dans `src/app/combat.rs`,
cf. section dédiée ci-dessus). Tous les fichiers partagés touchés ici (`src/net/protocol.rs`,
`src/bin/server.rs`, `src/app/multiplayer.rs`, `src/app/combat.rs`, `src/net/client/native.rs`,
`src/net/client/web.rs`) ont été édités par petites modifications ancrées (jamais de réécriture
complète de fichier), ce qui a permis de coexister sans perte : `PROTOCOL_VERSION` a fini à 5,
regroupant le champ `cause` (Sprint 2), `ClientMsg::Join::objective` (Sprint 5) et
`GameEvent::RoundObjective` (Sprint 5, retour serveur→client, auto-relecture) en bumps successifs
documentés.
Aucun commit n'a été fait par cette session — laissé à l'utilisateur, qui pourra choisir de
committer par petits lots (cf. le conseil déjà en mémoire projet sur ce dépôt).
