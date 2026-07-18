# RusteeGear — Sprint 2 (18 juillet 2026) — finalisation des priorités restantes

> Suite de [AUDIT_JEU_2026-07-18.md](AUDIT_JEU_2026-07-18.md) (Phases A→I, sprint 1). Les Phases
> A, B, C, D, E, F, G y sont **✅ terminées et vérifiées** (tests + relectures). Ce document couvre
> ce qui reste réellement ouvert au 18 juillet au soir : Phase H (écran de fin de manche) et Phase I
> (accessibilité) du plan précédent, plus 5 nouvelles phases pour les points de dette/écarts non
> encore planifiés (catalogue d'UI, ramassage d'arme réseau, instrumentation de mesure, audio,
> sécurité `firebase_uid`, tests éditeur, CI goldens GPU).
>
> Convention identique aux documents précédents : un sprint ≈ 1 à 3 jours, avec
> **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**. Lettrage des phases poursuivi à
> partir de J (A→I déjà pris par `AUDIT_JEU_2026-07-18.md` §14).

---

## 🧭 Vue d'ensemble — blocs de phases sans AUCUN fichier partagé

Contrairement à une première estimation trop optimiste, ces 9 phases se recoupent beaucoup sur les
fichiers. Le tableau ci-dessous liste l'ensemble exact des fichiers de chaque phase (union de tous
ses sprints) — c'est la base de tout ce qui suit, pas une approximation :

| Phase | Fichiers (union de tous les sprints) |
|---|---|
| **J** | `src/net/protocol.rs`, `src/bin/server.rs`, `src/editor/hud.rs`, `src/editor/mod.rs` |
| **K** | `src/app/settings.rs`, `src/editor/windows.rs`, `src/editor/hud.rs`, `src/gfx/renderer.rs`, `src/gfx/camera.rs` |
| **L** | `src/editor/windows.rs`, `src/editor/hud.rs`, `src/net/protocol.rs`, `src/net/firebase.rs`, `src/app/network_client.rs` |
| **M** | `src/app/multiplayer.rs`, `src/app/simulation.rs`, `src/net/protocol.rs`, `src/bin/server.rs` |
| **N** | `src/app/mod.rs`, `src/bin/server.rs`, `src/net/firebase.rs` |
| **O** | `src/runtime/sfx.rs`, `src/app/network_client.rs`, `src/app/simulation.rs` |
| **P** | `src/bin/server.rs`, `src/net/protocol.rs`, `src/net/firebase.rs` |
| **Q** | `src/editor/hud.rs`, `src/editor/menus.rs`, `src/editor/windows.rs` |
| **R** | Configuration CI uniquement (`.github/workflows/*.yml`) — **aucun fichier source**, compatible avec toutes les autres à tout moment |

**Verdict** : `L` partage un fichier avec **six** des huit autres phases (tout sauf R) — elle ne peut
jamais tourner en même temps que quasiment personne. `J` partage avec cinq. `M`, `N`, `P` se bloquent
mutuellement deux à deux (`server.rs`/`protocol.rs`/`firebase.rs` communs) — un vrai triangle à trois
sommets qui, à lui seul, impose au moins 3 créneaux différents. Il est donc **faux** de dire que J, L,
M, N (ou davantage) peuvent démarrer les 4 en même temps — vous aviez raison de le relever.

### Les 5 blocs — dans cet ordre, chacun attend que le précédent soit fini et mergé

| Bloc | Phases dedans (aucune ne partage de fichier avec une autre du même bloc) | Fichiers touchés par ce bloc | Doit attendre la fin de |
|---|---|---|---|
| **1** | **L**, **R** | `windows.rs`, `hud.rs`, `protocol.rs`, `firebase.rs`, `network_client.rs` (+ CI config, sans rapport) | — (démarre tout de suite) |
| **2** | **J**, **O** | `protocol.rs`, `server.rs`, `hud.rs`, `editor/mod.rs`, `sfx.rs`, `network_client.rs`, `simulation.rs` | **Bloc 1** (J reprend `hud.rs`/`protocol.rs` juste libérés par L) |
| **3** | **M**, **Q** | `multiplayer.rs`, `simulation.rs`, `protocol.rs`, `server.rs`, `hud.rs`, `menus.rs`, `windows.rs` | **Bloc 2** (M reprend `protocol.rs`/`server.rs` juste libérés par J ; Q reprend `hud.rs`/`windows.rs`) |
| **4** | **K**, **N** | `settings.rs`, `windows.rs`, `hud.rs`, `renderer.rs`, `camera.rs`, `app/mod.rs`, `server.rs`, `firebase.rs` | **Bloc 3** (K reprend `windows.rs`/`hud.rs` juste libérés par Q ; N reprend `server.rs` juste libéré par M) |
| **5** | **P** (seule) | `server.rs`, `protocol.rs`, `firebase.rs` | **Bloc 4** (reprend `server.rs`/`firebase.rs` juste libérés par N) |

**R peut démarrer n'importe quand, y compris avant le Bloc 1** — elle ne touche aucun fichier source,
posée au Bloc 1 seulement parce qu'elle n'a besoin d'attendre rien.

### Pourquoi cet ordre précis (pas un autre)

- **L en tout premier, seule (avec R)** : c'est la phase la plus « connectée » du lot (elle touche
  cinq fichiers que quatre autres phases veulent aussi toucher) — la sortir du chemin en premier
  libère le plus de monde d'un coup.
- **J juste après, avec O** : une fois L partie, J ne se recoupe plus qu'avec O — qui ne partage
  justement rien avec J (`sfx.rs`/`network_client.rs`/`simulation.rs` vs
  `protocol.rs`/`server.rs`/`hud.rs`/`mod.rs`). J ne peut être casée avec aucune autre phase que O.
- **M et Q ensuite** : M ne peut tourner qu'une fois J et L parties (elle reprend `protocol.rs` et
  `server.rs`) ; Q ne peut tourner qu'une fois L et J parties (elle reprend `hud.rs`/`windows.rs`). M
  et Q ne partagent rien entre elles → même bloc.
- **K et N ensuite** : K ne peut tourner qu'une fois L (`windows.rs`), J (`hud.rs`) et Q
  (`hud.rs`/`windows.rs`) parties. N ne peut tourner qu'une fois L (`firebase.rs`), J (`server.rs`) et
  M (`server.rs`) parties. K et N ne partagent rien entre elles → même bloc.
- **P toute seule, en dernier** : elle partage `server.rs` avec J/M/N et `firebase.rs`/`protocol.rs`
  avec L/M/N — la dernière à devoir attendre tout le monde, et personne du Sprint 2 ne reste à lui
  associer une fois M et N passées.

C'est un minimum strict compte tenu des recoupements réels : impossible de faire tenir ces 9 phases
en moins de 5 créneaux successifs sans qu'au moins deux instances touchent le même fichier au même
moment.

### `PROTOCOL_VERSION` : un seul bump, au bon moment

`src/net/protocol.rs` est touché par L (Bloc 1), J (Bloc 2), M (Bloc 3) et P (Bloc 5) — dans cet
ordre précis grâce aux blocs ci-dessus, donc **jamais deux en même temps**. Mais chacune ajoute
potentiellement des champs à des messages réseau : ne faire bumper `PROTOCOL_VERSION` qu'une seule
fois, par la **dernière** phase à toucher ce fichier (P, Bloc 5) plutôt qu'à chaque bloc — sinon
quatre bumps successifs le même jour compliquent inutilement le déploiement couplé client/VPS (cf.
mémoire de session « Audit réseau 2026-07 »). Les blocs 1 à 4 qui touchent `protocol.rs` doivent
committer leurs champs sans toucher à la constante de version ; P s'en charge en dernier.

---

<a id="phase-j"></a>
## PHASE J — Écran de fin de manche détaillé (indépendante)

### Sprint 1 — Résumé par joueur (frags/assists/XP)
**Objectif** : remplacer la bannière minimale par un résumé conforme au §9.2/§17.4 du GDD.
- [x] Étendre l'événement de fin de manche (`GameEvent::Win`/`Lose`, `src/net/protocol.rs`) pour
  transporter un résumé par joueur — déjà calculé côté serveur (`network_player_score`,
  `network_player_assists`, `src/bin/server.rs`), juste jamais poussé au client sous cette forme.
  **Déjà livré** (sous l'étiquette « Phase H, Sprint 1 » dans les commentaires de code, écrite avant
  ce document) : `GameEvent::Win { summary, contract }`/`Lose { summary }` transportent
  `Vec<RoundPlayerSummary>` (`player_id`, `name`, `frags`, `assists`, `xp`,
  `src/net/protocol.rs:369`), peuplé côté serveur par `round_summary()` (`src/bin/server.rs:535`,
  mêmes sources que `award_progress`) et diffusé à `room.connected_ids()` juste avant
  `award_progress` (`src/bin/server.rs:822-838`). `PROTOCOL_VERSION` **non** bumpé (reste à 6, cf. la
  convention d'un seul bump groupé par P en fin de Bloc 5).
- [x] Afficher une ligne par joueur (nom, frags, assists, XP gagnée) sur l'écran Gagné/Perdu
  (`src/editor/hud.rs`), à la place de la bannière texte seule actuelle. **Déjà livré** :
  `round_summary_banner()` (`src/editor/hud.rs:1251`), une ligne par joueur via
  `locale::round_summary_row`, appelée depuis les deux points d'affichage (aperçu mobile
  `run_player_overlay` et vue Play de l'éditeur `run()`, `src/editor/mod.rs`). Côté client,
  `GameEvent::Win`/`Lose` alimentent `AppState::round_summary`/`round_summary_won`/
  `round_contract_label` (`src/app/network_client.rs:976-990`).
- **Fichiers** : `src/net/protocol.rs`, `src/bin/server.rs`, `src/editor/hud.rs`,
  `src/app/network_client.rs`, `src/app/mod.rs`, `src/editor/mod.rs`, `src/app/locale.rs` (au-delà des
  trois fichiers annoncés — la diffusion réseau + le stockage côté client n'existaient pas non plus).
- **Livrable** : un salon de test à 2+ joueurs affiche, à la fin de la manche, une ligne par joueur
  avec ses frags/assists/XP — pas seulement « Gagné !```/```Perdu ! ». ✅ Vérifié le 18 juillet 2026 :
  `cargo test --lib` (590 passés, dont `win_event_stores_the_round_summary_and_contract`,
  `lose_event_stores_the_round_summary_without_a_contract`), `cargo clippy --lib -- -D warnings` et
  `rustfmt --check` propres sur les fichiers listés.
- **Risques** : bump de `PROTOCOL_VERSION` si le format d'événement change — coordonner avec L/M/P
  (voir Frictions ci-dessus) pour un seul bump groupé. Non déclenché ici (format ajouté avant que ce
  document ne fixe la convention, mais compatible avec elle : aucun bump n'a eu lieu).

### Sprint 2 — Contrat du jour et bannière de vague
**Objectif** : combler les deux autres surfaces manquantes du §17.2 (contrat rempli, bannière de
vague au changement de manche).
- [x] Afficher `GameEvent::WaveStart` (déjà envoyé, `protocol.rs:350`) en bannière courte à l'écran —
  rien ne l'affiche aujourd'hui côté UI. **Déjà livré** : le serveur émet désormais
  `GameEvent::WaveStart { wave }` dès que `room.app.wave` change (`src/bin/server.rs:742-761`,
  `wave == 0` exclu — scène sans système de manches) ; côté client, `wave_start_banner()`
  (`src/editor/hud.rs:1309`) l'affiche, piloté par `AppState::wave_banner_flash`/`wave_banner_wave`
  (décroissance par frame, même mécanisme que `ally_down_flash`).
- [x] Afficher la complétion du contrat du jour (déjà calculée, `AppState::contract_completed`) en
  fin de manche, à côté du résumé du Sprint 1. **Déjà livré** : le serveur calcule le contrat rempli
  *avant* la diffusion de `GameEvent::Win` (`src/bin/server.rs:800-809`, uniquement sur victoire) et
  le transporte dans `GameEvent::Win::contract` ; `round_summary_banner()` l'affiche sous les lignes
  de joueurs si `contract_label` est renseigné (`src/editor/hud.rs:1295-1303`).
- **Fichiers** : `src/editor/hud.rs`, `src/editor/mod.rs`, `src/bin/server.rs`, `src/net/protocol.rs`
  (au-delà des deux fichiers annoncés — la diffusion serveur elle-même faisait partie du travail, pas
  seulement l'affichage).
- **Livrable** : une manche qui change révèle une bannière visible ; un contrat rempli s'affiche à la
  fin de la manche qui l'a complété. ✅ Vérifié le 18 juillet 2026 :
  `wave_start_event_arms_the_wave_banner` passe, `cargo test --lib` (590 passés).
- **Risques** : dépend du Sprint 1 pour rester cohérent visuellement (même zone d'écran). Confirmé
  cohérent : les deux bannières partagent le même mécanisme de décroissance par `intensity` et sont
  rendues dans la même zone `play_rect`/`area`.

---

<a id="phase-k"></a>
## PHASE K — Accessibilité minimale (dépendance théorique sur A, déjà levée)

### Sprint 1 — Taille HUD et réduction des secousses
**Objectif** : options minimales du §16.6 (taille HUD, réduction de screen-shake).
- [ ] Champs `hud_scale: f32`/`reduce_shake: bool` dans `Settings` (`src/app/settings.rs`), exposés à
  la fois dans `settings_window` (éditeur) et `player_settings_window`/`settings_essentials`
  (mode Player — réutiliser le mécanisme déjà commun aux deux, posé par la Phase A).
- [ ] Appliquer `hud_scale` aux tailles de police/tailles de widgets dans `src/editor/hud.rs` ;
  appliquer `reduce_shake` en atténuant/désactivant `camera_shake_offset`
  (`src/app/simulation.rs`)/`view_proj_shaken` (`src/gfx/camera.rs`).
- **Fichiers** : `src/app/settings.rs`, `src/editor/{windows,hud}.rs`, `src/gfx/{renderer,camera}.rs`.
- **Livrable** : les deux options changent visiblement le rendu, testées comme les autres réglages
  persistés (round-trip `save`/`load`, sur le modèle des tests de la Phase A Sprint 1).
- **Risques** : partage `src/editor/hud.rs` avec J et Q — coordonner l'ordre de merge (cf.
  Frictions). Comme ces réglages vivent dans la même fenêtre que Firebase/manette (Phase A), ils
  héritent automatiquement de la disponibilité en mode `--player`/mobile — aucun risque de retomber
  dans le trou documenté au §12 de l'audit précédent.

### Sprint 2 — Mode daltonien minimal
**Objectif** : ne plus faire reposer un signal de gameplay uniquement sur la couleur.
- [ ] Audit des couleurs porteuses de sens (silhouettes de classe §10.3, alertes HUD) + alternative
  non-couleur (forme, icône) pour au moins les cas critiques (vie basse, allié à terre, cible
  verrouillée).
- **Fichiers** : `src/editor/hud.rs`, `src/gfx/renderer.rs` (teintes de silhouette).
- **Risques** : effort le plus flou de cette liste — cadrer d'abord ce qui est réellement porteur de
  sens avant de coder, plutôt que d'ajouter des icônes partout.

---

<a id="phase-l"></a>
## PHASE L — Compléments du catalogue d'interface (§17) (indépendante)

### Sprint 1 — Fenêtre Multijoueur : onglets et présence en ligne
**Objectif** : combler le trou signalé au §5 de l'audit — `multiplayer_window()` est une seule liste
scrollable, et `list_online_players`/`set_presence` (déjà backés par Firebase,
`src/net/firebase.rs:500-541`) ne sont jamais affichés.
- [x] Découper `multiplayer_window()` (`src/editor/windows.rs`) en sections claires (onglets egui ou
  sections rétractables) : Connexion/Classe, Salon (chat existant), Classement, **Présence en ligne**
  (nouveau — liste des joueurs connectés au salon, via `list_online_players`). Fait : nouvelle
  section « Présence en ligne » ajoutée après Classement dans `multiplayer_window`
  (`src/editor/windows.rs`), alimentée par `AppState::online_players` (`request_refresh_online_players`/
  `poll_online_players`, `src/app/network_client.rs`) et un heartbeat périodique
  (`request_presence_heartbeat` → `net::firebase::set_presence`), même politique
  d'auto-rafraîchissement que le chat (`AUTO_PRESENCE_REFRESH_INTERVAL`, `src/editor/mod.rs`).
  **Correction du 18 juillet 2026 (auto-audit)** : le heartbeat tourne désormais sur un minuteur
  **indépendant** de la fenêtre Multijoueur (`mp_last_presence_heartbeat`, distinct de
  `mp_last_presence_refresh`) — la première version le liait à `panels.multiplayer` ouvert, comme le
  chat ; or fermer cette fenêtre en pleine partie faisait disparaître le joueur de la présence après
  `PRESENCE_TIMEOUT_MS` (15 s, `net::firebase`) alors qu'il jouait toujours. Le heartbeat tourne
  maintenant tant qu'un compte est connecté (`has_firebase_account`) et Firebase configuré, fenêtre
  ouverte ou non ; seul le rafraîchissement de la **liste** reste gaté par la fenêtre (inutile de la
  réinterroger quand personne ne la regarde). **Écart assumé par rapport au libellé de l'objectif** :
  `list_online_players`
  (`net::firebase.rs:500-541`) lit `/presence/<uid>`, une présence **globale par compte**, pas
  filtrée par salon (la RTDB ne garde pas trace du salon dans ce nœud) — la section affiche donc « qui
  a un compte connecté et actif », pas strictement « qui est dans ce salon ». Un vrai filtre par salon
  demanderait un nouveau nœud RTDB `lobbies/<code>/presence` (hors scope de ce sprint, pur
  réarrangement d'UI annoncé).
- **Fichiers** : `src/editor/windows.rs`, `src/editor/mod.rs`, `src/app/mod.rs`,
  `src/app/network_client.rs`, `src/gfx/renderer.rs` (au-delà de `windows.rs` seul annoncé — le
  déclaratif Firebase existait déjà mais n'était appelé nulle part, il fallait le brancher de bout en
  bout pour qu'une liste non vide s'affiche).
- **Livrable** : la fenêtre Multijoueur affiche qui est en ligne (présence de compte, cf. écart
  ci-dessus) dans une section dédiée et lisible séparément du chat. ✅
- **Risques** : ne pas casser le chat/mute déjà fonctionnels (Phase F de `sprint10audit.md`) — pur
  réarrangement d'UI, pas de changement de logique réseau attendu. Confirmé sans régression
  (`cargo test --all-targets` : 589 passés, `cargo test --features net_tests` : 613 passés).

### Sprint 2 — Marqueur allié hors-écran
**Objectif** : compléter `ally_down_banner()` (déjà existant, `hud.rs:861`) avec une indication de
direction pour un allié à terre hors du champ de vision.
- [x] Calculer la direction écran (flèche/indicateur de bord) vers l'allié à terre le plus proche,
  sur le modèle des indicateurs hors-écran standards (projection de la position 3D sur les bords du
  viewport 2D). Fait : `AppState::nearest_downed_ally_position` (`src/app/mod.rs`, filtre `health <=
  0.0`, plus proche du joueur local) + `offscreen_edge_position`/flèche triangulaire dans
  `ally_down_banner` (`src/editor/hud.rs`) — projection via la vraie `Mat4::view_proj()` de la
  caméra (pas une approximation d'angle), avec le cas `w <= 0` (allié derrière la caméra) géré par
  inversion de signe, technique standard d'indicateur hors-écran. Câblage `ally_marker: Option<(Mat4,
  Vec3)>` à travers `editor::run`/`run_player_overlay`/`build_ui` depuis `gfx/renderer.rs`. 4 tests
  (`offscreen_edge_position_*`, `nearest_downed_ally_position_ignores_ghosts_still_alive`).
- **Fichiers** : `src/editor/hud.rs`, `src/app/mod.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs`.
- **Livrable** : un allié à terre hors champ affiche une flèche indiquant sa direction, en plus de la
  bannière texte déjà existante. ✅
- **Risques** : lisibilité en combat dense — plafonné à un seul marqueur (l'allié le plus proche,
  `nearest_downed_ally_position` ne renvoie qu'une position), pas un par allié à terre. Le marqueur
  ne s'affiche que pendant la fenêtre `ally_down_flash > 0` (même déclencheur que la bannière
  texte) — pas d'état persistant séparé à maintenir.

### Sprint 3 — Détail frags/assists au HUD
**Objectif** : `kills_hud()` (`hud.rs:271`) n'affiche qu'un compteur `kills: u32` — ajouter le détail
assists déjà calculé côté serveur.
- [x] Étendre l'affichage pour montrer frags et assists séparément (ex. « 3 💀 · 2 🤝 »). Fait :
  `kills_hud` (`src/editor/hud.rs`) prend désormais `assists: u32` et affiche
  `locale::kills_and_assists` (icônes seules, pas de traduction — cf. sa doc). **Vérifié avant de
  coder, comme prévu par le risque annoncé** : `network_player_assists` (`app::multiplayer`) existait
  déjà côté serveur mais son résultat n'était jamais diffusé — `EntityDelta` (`src/net/protocol.rs`)
  n'avait qu'un champ `kills`, pas `assists`. A donc fallu répliquer la même chaîne que `kills` :
  nouveau champ `EntityDelta::assists: Option<u32>` (`#[serde(default)]`), peuplé dans
  `network_snapshot` (`src/app/multiplayer.rs`) depuis `network_assists` (déjà tenu à jour par
  `credit_assists_on_kill`, rien à changer côté calcul), puis lu côté client
  (`RemotePlayer::assists`/`AppState::net_local_assists`/`displayed_assist_count`,
  `src/app/network_client.rs`). Pas de bump de `PROTOCOL_VERSION` (`src/net/protocol.rs`, actuellement
  6) : convention du Bloc 1 de ce document — un seul bump groupé, porté par la Phase P en dernier.
- **Fichiers** : `src/editor/hud.rs`, `src/app/network_client.rs`, `src/app/locale.rs` (nouveau
  `kills_and_assists`, l'ancien `kills(locale, u32)` devenu mort a été supprimé plutôt que laissé
  inutilisé), `src/net/protocol.rs`, `src/app/multiplayer.rs`, `src/app/mod.rs`, `src/gfx/renderer.rs`
  (au-delà de `hud.rs`/`network_client.rs` annoncés — la réplication réseau live n'existait pas du
  tout, seul le résumé de fin de manche (`RoundPlayerSummary::assists`, Phase J) transportait déjà un
  total assists, mais pas en continu pendant la manche).
- **Livrable** : le HUD distingue visuellement frags et assists. ✅ Nouveau test
  `network_snapshot_reports_each_players_live_assist_count` (`src/app/multiplayer.rs`) prouve la
  diffusion en direct ; `cargo test --features net_tests` (613 passés, dont le test à sockets réels
  `two_connected_clients_see_the_same_creature_position_kill_and_bite_damage`) confirme le round-trip
  bincode sur un vrai socket.
- **Risques** : aucun de notable — extension d'un widget déjà existant. Confirmé : `cargo clippy
  --all-targets -- -D warnings`, `cargo fmt --all --check` et `python3 scripts/check_unwrap_budget.py`
  tous au vert après ce sprint.

---

<a id="phase-m"></a>
## PHASE M — `WeaponPickup` réseau (indépendante)

### Sprint 1 — Synchronisation du ramassage d'arme entre joueurs réseau
**Objectif** : le ramassage d'arme (`WeaponPickup`, aujourd'hui donjon solo uniquement,
`src/app/simulation.rs:510-522`) doit se propager aux autres joueurs du salon.
- [ ] Nouveau message réseau (`ClientMsg`/`GameEvent`, `src/net/protocol.rs`) signalant qu'un joueur a
  ramassé une arme à une position/objet donné ; le serveur valide (l'objet existe, n'a pas déjà été
  ramassé) et diffuse aux autres clients du salon pour qu'ils masquent le pickup et, si pertinent,
  reflètent l'équipement du joueur qui l'a ramassé (au moins visuellement, pas nécessairement son
  arsenal complet).
- **Fichiers** : `src/app/multiplayer.rs`, `src/app/simulation.rs`, `src/net/protocol.rs`,
  `src/bin/server.rs`.
- **Livrable** : deux clients connectés au même salon voient tous deux un pickup disparaître dès
  qu'un des deux le ramasse (pas seulement celui qui l'a ramassé).
- **Risques** : bump de `PROTOCOL_VERSION` (coordonner avec J/L/P, cf. Frictions) ; valider
  côté serveur pour respecter la règle d'or anti-triche (§5.7 du GDD, déjà vérifiée tenue ailleurs) —
  ne pas faire confiance à la position/l'identité de pickup envoyée par le client sans vérification.

---

<a id="phase-n"></a>
## PHASE N — Instrumentation `favorite_weapon` (indépendante)

### Sprint 1 — Persister l'arme favorite par joueur
**Objectif** : le GDD (§11, §13 tension 5) cite `favorite_weapon` comme « instrument de mesure
gratuit déjà persisté » pour trancher la tension « l'Éclair est-il strictement meilleur ? » — cette
donnée n'existe pas dans le code, l'affirmation du GDD était fausse.
- [ ] Compter, côté serveur, l'arme la plus utilisée par joueur sur une manche (ou une fenêtre de
  temps) et la persister dans `PlayerProgress` (`src/net/firebase.rs`), au même titre que
  `last_contract_day`.
- **Fichiers** : `src/app/mod.rs` (compteur par arme, sur le modèle de `player_down_count`), `src/bin/server.rs`
  (calcul en fin de manche), `src/net/firebase.rs` (`PlayerProgress::favorite_weapon`, champ additif
  rétrocompatible).
- **Livrable** : `PlayerProgress::favorite_weapon` reflète l'arme la plus utilisée par le joueur,
  vérifiable en base Firebase après une manche de test.
- **Risques** : ne pas confondre « arme équipée en fin de manche » avec « arme la plus utilisée » —
  vraiment compter les tirs/dégâts par arme, pas juste l'état final. Décision de scope à trancher
  avant de coder : compteur de tirs, de dégâts, ou de frags par arme (le GDD ne précise pas lequel).

---

<a id="phase-o"></a>
## PHASE O — Audio manquant (indépendante)

### Sprint 1 — Son dédié « allié à terre » et « éveil de créature »
**Objectif** : combler les rangs 2 et 3 de la priorité §10.4 du GDD, déjà documentés comme non
couverts par le GDD lui-même.
- [x] Nouvelle variante `Sfx::AllyDown` (`src/runtime/sfx.rs`), jouée à la place de `Sfx::Lose` côté
  client quand la bannière `ally_down_banner` s'affiche pour un **allié** (pas soi-même) —
  `src/app/network_client.rs:934-937`. Fait : trois tons descendants (392/294/220 Hz), distincts de
  `Lose` (330/247 Hz) — même famille « mauvaise nouvelle », signature acoustique différente
  (vérifié par test, cf. Livrable).
- [x] Nouvelle variante `Sfx::CreatureWake`, jouée au moment où une créature `Furtive` sort de son
  état endormi (`src/app/simulation.rs:1251-1256`, aujourd'hui aucun appel `sfx::play` à ce site).
  Fait : sting bref et montant (260/420/560 Hz). La difficulté réelle n'était pas le son lui-même
  mais **détecter la transition** endormie → active (le code existant ne fait que recalculer chaque
  frame « à portée ou pas », sans mémoire d'état) : nouveau registre `AppState::furtive_awake`
  (`HashSet<usize>`, `src/app/mod.rs`), rempli au premier tick où une `Furtive` franchit
  `FURTIVE_DETECT_RANGE`, jamais réarmé ensuite (éveillée une fois = éveillée pour le reste de la
  partie, même politique que `trigger_prev`) — vidé à `restart_game`
  (`src/app/persistence.rs`) et à l'entrée en Play (`src/app/simulation.rs`, même sites que
  `trigger_prev.clear()`). Les indices nouvellement éveillés sont collectés dans un `Vec` local
  pendant la boucle de pilotage des chasseurs (qui emprunte `self.scene.objects`), puis appliqués
  (`furtive_awake.insert` + `sfx::play`) juste après — évite tout conflit d'emprunt avec
  `self.audio`/`self.furtive_awake`.
- **Fichiers** : `src/runtime/sfx.rs`, `src/app/network_client.rs`, `src/app/simulation.rs`,
  `src/app/mod.rs` (nouveau champ `furtive_awake`), `src/app/persistence.rs` (reset à `restart_game`,
  au-delà des trois fichiers annoncés — nécessaire pour que l'éveil de la scène d'origine puisse se
  resignaler après un « Rejouer »).
- **Livrable** : un allié à terre déclenche un son distinct de la propre défaite du joueur ; une
  créature Furtive qui s'éveille émet un son perceptible. ✅ Tests :
  `ally_down_and_creature_wake_are_acoustically_distinct_from_related_sfx` (`src/runtime/sfx.rs` —
  clés ET WAV générés distincts, pas seulement les enums) et
  `a_furtive_is_marked_awake_exactly_once_when_it_crosses_its_wake_radius` (`src/app/mod.rs` —
  aucun éveil enregistré tant qu'endormie, exactement un éveil enregistré après franchissement,
  jamais dupliqué sur les ticks suivants).
- **Risques** : aucun de notable — extension additive de l'enum `Sfx`, pas de changement de logique
  de jeu. Confirmé : `cargo test --all-targets` (592 passés), `cargo clippy --all-targets -- -D
  warnings`, `cargo fmt --all --check`, `check_unwrap_budget.py` tous au vert après ce sprint.
  **Auto-audit (18 juillet 2026)** : limite mineure assumée, pas corrigée — `furtive_awake` n'est
  jamais réarmé pour un objet donné (cf. sa doc), donc une `Furtive` déjà éveillée puis **vaincue et
  respawnée** (`Combat::respawn_delay > 0`) ne rejouerait pas `Sfx::CreatureWake` à son second éveil.
  Sans impact sur le contenu actuel : les créatures de vague (dont l'archétype `Furtive` utilisé,
  `scene::demos.rs`, « Squelette ») ont `respawn_delay = 0.0` (vérifié dans `demos.rs` — `combat.rs`
  ne remet en file que si `respawn_delay > 0.0`), donc jamais respawnées ; à revisiter seulement si
  une future scène équipe une `Furtive` d'un vrai respawn. Le comportement de poursuite lui-même
  (indépendant de `furtive_awake`, piloté uniquement par la distance à chaque frame) n'est pas
  affecté — c'est uniquement le signal sonore du second éveil qui manquerait.

---

<a id="phase-p"></a>
## PHASE P — Authentification `firebase_uid` (indépendante, sécurité)

### Sprint 1 — Vérification de token côté serveur
**Objectif** : aujourd'hui, `firebase_uid` (`ClientMsg::Join`) n'est validé que sur la forme
(charset, `protocol.rs:62`), jamais sur l'authenticité — un client modifié peut réclamer un uid
arbitraire et créditer la progression d'autrui.
- [ ] Le client envoie un ID token Firebase (obtenu après authentification) en plus de l'uid ; le
  serveur vérifie ce token auprès de Firebase (endpoint de vérification côté Admin SDK ou appel REST
  équivalent) avant d'associer les gains de la manche à cet uid.
- **Fichiers** : `src/net/protocol.rs` (nouveau champ), `src/net/firebase.rs` (vérification),
  `src/bin/server.rs` (rejet si le token ne correspond pas à l'uid déclaré).
- **Livrable** : un client qui envoie un uid sans token valide correspondant ne voit pas sa
  progression créditée (test simulant un uid usurpé).
- **Risques** : dépendance réseau supplémentaire vers Firebase à chaque connexion (latence, panne) —
  prévoir un repli explicite (jouer en "invité" sans progression persistée) plutôt qu'un blocage dur
  de la connexion si la vérification échoue pour une raison technique (pas d'usurpation).

### Sprint 2 — Déploiement couplé
**Objectif** : ce changement de protocole nécessite un déploiement VPS + client synchronisé (comme
documenté dans la mémoire de session « Procédure de déploiement VPS » et « Audit réseau 2026-07 »).
- [ ] Bump `PROTOCOL_VERSION`, suivre la procédure de déploiement couplé existante.
- **Fichiers** : selon la procédure standard déjà documentée.
- **Risques** : point déjà identifié comme risque opérationnel dans les audits précédents (rien
  n'automatise le bump couplé) — suivre la procédure manuelle à la lettre.

---

<a id="phase-q"></a>
## PHASE Q — Tests de l'UI éditeur (indépendante, dette pure)

### Sprint 1 — `windows.rs` (le mieux loti, seulement 2 tests aujourd'hui)
**Objectif** : couvrir au moins la logique non-UI extractible (validation de champs, calculs) de
`windows.rs`, sans nécessairement tester le rendu egui lui-même.
- [ ] Identifier les fonctions pures dans `windows.rs` (ex. formatage, validation d'entrée) et les
  tester isolément.
- **Fichiers** : `src/editor/windows.rs`.
- **Livrable** : couverture de test en hausse mesurable sur ce fichier.
- **Risques** : ne pas essayer de tester le rendu egui pixel par pixel — se concentrer sur la logique.

### Sprint 2 — `hud.rs` (0 test aujourd'hui, ~1064 lignes)
**Objectif** : idem, sur les fonctions de calcul (ex. `damage_vignette` intensité, positions de
widgets) plutôt que le rendu.
- **Fichiers** : `src/editor/hud.rs`.
- **Risques** : partage ce fichier avec J et K (cf. Frictions) — faire ce sprint **avant** J/K pour
  poser un filet de non-régression sur lequel elles pourront s'appuyer, ou après si elles sont déjà
  mergées (coordination, pas blocage).

### Sprint 3 — `menus.rs` (0 test aujourd'hui, ~623 lignes)
**Objectif** : idem.
- **Fichiers** : `src/editor/menus.rs`.
- **Risques** : aucun de notable, fichier le moins disputé de ce sprint.

---

<a id="phase-r"></a>
## PHASE R — CI goldens avec GPU (indépendante, filet de sécurité)

### Sprint 1 — Job CI optionnel avec GPU
**Objectif** : les goldens de rendu (`golden_render.rs`, `golden_skinning.rs`) sont sautés en CI
(pas de GPU sur `ubuntu-latest`) — une régression de shader passe la CI au vert aujourd'hui.
- [x] Ajouter un job CI optionnel (runner macOS avec Metal, ou self-hosted avec GPU) qui lance les
  tests goldens et publie le résultat comme informatif (pas bloquant au départ, pour éviter un flake
  qui bloquerait tout le monde avant d'être éprouvé). **Déjà livré** : job `golden`
  (`.github/workflows/ci.yml`, `runs-on: macos-latest`, lance
  `cargo test --test golden_render --test golden_skinning`) — introduit au commit `48ae056`
  (« CI : tests réseau à sockets réels et goldens GPU sur macOS ») avant ce sprint, vérifié présent
  et fonctionnel le 18 juillet 2026 (audité via `gh run list`/`gh api` : 15 exécutions consécutives
  sur `main`, l'étape « Tests goldens » elle-même en `success`, pas seulement le job masqué).
- [x] **Durci le 18 juillet 2026** : `continue-on-error: true` retiré (`.github/workflows/ci.yml`)
  après avoir confirmé les 15 runs verts consécutifs ci-dessus — le job bloque désormais la CI comme
  les autres.
- **Fichiers** : configuration CI (`.github/workflows/*.yml`).
- **Livrable** : un job CI visible qui tourne les goldens sur un vrai GPU à chaque push, désormais
  bloquant. ✅
- **Risques** : coût/disponibilité d'un runner GPU — cadrer le budget avant de s'engager sur un
  runner self-hosted permanent. Un futur run rouge bloquera maintenant toute la CI (plus de filet
  informatif) : si le GPU des runners GitHub dérive du GPU local ayant produit les goldens au-delà
  de la tolérance (`CHANNEL_TOLERANCE`/`MAX_DIFFERING_RATIO`, `tests/golden_render.rs`), régénérer
  les références (`UPDATE_GOLDEN=1`) plutôt que de rétablir `continue-on-error`.

---

## ✅ Définition de « terminé » par phase

| Phase | Terminée quand |
|---|---|
| J | Résumé par joueur + bannière de vague + contrat rempli visibles en fin/pendant de manche |
| K | Taille HUD et réduction de secousses fonctionnelles et testées ; mode daltonien minimal pour vie/allié à terre/cible |
| L | Fenêtre Multijoueur sectionnée avec présence en ligne ; marqueur allié hors-écran ; détail frags/assists au HUD |
| M | Un ramassage d'arme par un joueur réseau est visible par les autres joueurs du même salon |
| N | `PlayerProgress::favorite_weapon` alimenté et vérifiable en base |
| O | Son dédié pour allié à terre et éveil de créature |
| P | `firebase_uid` vérifié par token, déploiement couplé effectué |
| Q | `hud.rs`/`menus.rs`/`windows.rs` ont chacun une couverture de test non nulle sur leur logique extractible |
| R | Job CI GPU des goldens visible et fonctionnel (même informatif) |

## 📌 Conseils d'exécution

- **Suivre les 5 blocs dans l'ordre indiqué en tête de document** (L+R → J+O → M+Q → K+N → P) — c'est
  la seule séquence qui garantit qu'aucun fichier n'est jamais touché par deux instances en même
  temps. Ne pas démarrer un bloc avant que le précédent soit **fini et mergé**, pas seulement
  « presque fini ».
- Un seul bump de `PROTOCOL_VERSION`, porté par **P** (dernière phase du dernier bloc à toucher
  `protocol.rs`) — les blocs 1 à 4 ajoutent leurs champs sans toucher la constante de version.
- Si les ressources sont limitées et qu'il faut choisir où investir en premier plutôt que de dérouler
  les 5 blocs dans l'ordre : **J** a l'impact joueur le plus direct (écran de fin de manche) et
  **P** l'impact sécurité le plus fort si la progression prend de la valeur — mais l'un comme l'autre
  restent contraints par leur bloc (J ne peut pas démarrer avant la fin de L ; P ne peut démarrer
  qu'en tout dernier).
- R (CI goldens GPU) est la seule phase réellement libre de tout ordre — à glisser dans n'importe
  quel creux, y compris avant le Bloc 1, puisqu'elle ne touche aucun fichier source du dépôt.
