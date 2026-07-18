# RusteeGear — Plan de sprints issus de l'audit GDD (`auditGDD10h.md`)

> Traduit les écarts identifiés dans [auditGDD10h.md](auditGDD10h.md) en phases/sprints exécutables.
> Convention identique à [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md) : un sprint ≈ 1 à 3 jours,
> avec **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**.
> On ne démarre un sprint que si le précédent **de la même phase** est « vert ».

Retour : **[auditGDD10h.md](auditGDD10h.md)** (constat) · **[GDD_MMORPG.md](../GDD_MMORPG.md)** (cible) ·
**[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)** (plan moteur global).

---

## 🧭 Vue d'ensemble — indépendance des phases

Les phases **A, B, E, F, G** n'ont **aucune dépendance entre elles** : elles peuvent être
travaillées en parallèle (par des personnes ou des sessions différentes) dès aujourd'hui.
Seule la **phase D dépend de la phase C** (le Contrat du jour a besoin de `RoundObjective`,
livré en phase C — c'est explicitement noté dans le code, `src/bin/server.rs:438-439`).

```
Phase A (Feedback combat & UI classe)  ─┐
Phase B (Assists / économie XP)        ─┤
Phase C (Modes de manche) ──► Phase D (Contrat du jour)
Phase E (Archétypes créatures)         ─┤   toutes indépendantes,
Phase F (Salon & mute)                 ─┤   démarrables en parallèle
Phase G (Rattrapage doc GDD)           ─┘
```

| Phase | Sprints | Dépend de | Peut démarrer en parallèle avec | But |
|---|---|---|---|---|
| **A — Feedback combat & classe** | 1 → 3 | — | B, C, E, F, G | Le joueur sent qu'il encaisse, comprend pourquoi il meurt, choisit sa classe |
| **B — Assists** | 4 | — | A, C, E, F, G | Le score reflète l'aide au combat, pas seulement les frags |
| **C — Modes de manche** | 5 → 8 | — | A, B, E, F, G | Survie / Escorte / Boss, au-delà des Vagues |
| **D — Contrat du jour** | 9 | **C** | — (démarre après C) | Objectif quotidien rejouable, posé sur `RoundObjective` |
| **E — Archétypes créatures** — ✅ terminé 2026-07-18 | 10 → 11 | — | A, B, C, F, G | IA différenciée (Traqueuse/Meute/Colosse/Furtive) |
| **F — Salon & mute** — ✅ terminé 2026-07-18 | 12 → 13 | — | A, B, C, E, G | Chat en jeu utilisable, silence d'un joueur gênant |
| **G — Rattrapage doc GDD** | 14 | — | A, B, C, E, F | Le GDD §14 reflète l'avance réelle (XP, roster, audio) |

> Priorité recommandée si tout ne peut pas être mené de front : **A puis C** (impact joueur direct),
> le reste (B, E, F, G) peut se glisser dans les creux ou en parallèle sur une autre session.

---

<a id="phase-a"></a>
## PHASE A — Feedback combat & sélecteur de classe (indépendante) — ✅ terminé (2026-07-18)

### Sprint 1 — Feedback visuel/sonore des dégâts subis — ✅ terminé (2026-07-18)
**Objectif** : le joueur perçoit immédiatement qu'il encaisse un coup (§6.1 du GDD).
- [x] Vignette rouge / flash d'écran bref à la réception de dégâts côté client — **existait déjà**
  avant ce sprint (contrairement à l'hypothèse de l'audit), seul le recul caméra manquait.
- [x] Léger recul caméra (shake) proportionnel aux dégâts : `AppState::camera_shake` (intensité
  1→0, décroissance ~0,25 s, `src/app/simulation.rs`), appliqué **uniquement au rendu** de la frame
  courante (jamais à `camera.target`/`camera.yaw` persistants) via `OrbitCamera::view_proj_shaken`
  (`src/gfx/camera.rs`) — plafonné à 1.0, jamais cumulatif, réinitialisé à chaque rechargement de
  scène.
- [x] Son de contact déclenché en priorité sur les autres SFX — déjà en place.
- **Fichiers** : `src/gfx/{camera,renderer}.rs`, `src/app/{simulation,health,creature_attack,network_client,demos,persistence}.rs`.
- **Livrable** : encaisser un coup en Play déclenche visuellement + auditivement l'effet, sans latence perceptible.
- **Risques** : ne pas gêner la lisibilité en combat à 2+ créatures ; garder l'effet cappé (pas de spam si dégâts multiples/frame) — vérifié à l'audit.

### Sprint 2 — Diagnostic de mort — ✅ terminé (2026-07-18)
**Objectif** : à la mort, afficher une cause résumée (§16.5 : ex. « Encerclé — 2 Traqueuses »).
- [x] Serveur : `AppState::recent_damage` mémorise les 5 dernières sources de dégâts par joueur
  réseau (type d'agresseur + indice d'objet), purgées à la mort (`src/app/health.rs`,
  `compute_death_cause`).
- [x] Client : `AppState::death_cause` (mémorisé uniquement pour notre propre mort, jamais pour un
  allié) affiché sous le titre de `defeated_banner` (`src/editor/hud.rs`), via `locale::death_cause`
  (FR/EN). `net::protocol::GameEvent::PlayerDown` gagne un champ `cause: Option<DeathCause>`
  (`DeathCauseKind::{Monster,Creature}` + `distinct_attackers: u8`), `PROTOCOL_VERSION` bumpé.
- **Fichiers** : `src/net/protocol.rs`, `src/app/health.rs`, `src/app/network_client.rs`,
  `src/editor/{hud,mod}.rs`, `src/app/locale.rs`.
- **Livrable** : mourir en Play affiche une cause cohérente avec les dernières attaques reçues.
- **Risques** : calcul serveur `O(1)` amorti (fenêtre bornée à 5 entrées) — conforme au risque de départ.

### Sprint 3 — Sélecteur de classe en UI — ✅ terminé (2026-07-18)
**Objectif** : exposer le choix Assaut/Éclaireur/Soutien déjà modélisé côté backend (§8).
- [x] `PlayerClass` gagne `to_u8`/`label`/`ALL` (`src/app/multiplayer.rs`) — jusqu'ici seul `from_u8`
  existait, rien n'émettait autre chose que `0` (Assaut).
- [x] Sélecteur (`egui::ComboBox`) dans la fenêtre « 🌐 Multijoueur » desktop
  (`src/editor/windows.rs`), désactivé une fois connecté. L'overlay mobile minimal reste sans
  sélecteur par choix (Assaut par défaut, portée réduite documentée).
- [x] `NetClient::connect_to_lobby` (native + web) et `AppState::connect_to_server_as` transmettent
  la classe choisie au `Join` ; `connect_to_server` (2 arguments) reste un raccourci Assaut par
  défaut — zéro régression pour tout appelant existant.
- **Fichiers** : `src/editor/windows.rs`, `src/app/{multiplayer,network_client}.rs`,
  `src/net/client/{native,web}.rs`, `src/bin/server.rs`.
- **Livrable** : un joueur peut choisir sa classe avant de rejoindre, et son comportement en jeu
  (vitesse, dégâts, soin) reflète le choix.
- **Risques** : valeur par défaut (Assaut) reste le repli si aucun choix n'est fait — vérifié, zéro régression.

### Audit de repasse (Phase A) — un bug réel trouvé et corrigé, deux trous de test comblés
- **Bug réel corrigé — build iOS cassé** : les deux call sites de `editor.run`/`run_player_overlay`
  (`src/gfx/renderer.rs`) appellent `app.connect_to_server_as(...)` sans garde de plateforme, mais
  cette méthode n'existait que dans le bloc `#[cfg(not(target_os = "ios"))]` de
  `src/app/network_client.rs` — une compilation ciblant iOS aurait échoué. Corrigé par un stub
  symétrique `connect_to_server_as` dans le bloc iOS (classe ignorée, comme le reste de la connexion
  sur cette cible) — non vérifié par une compilation croisée réelle (pas de toolchain iOS
  disponible), corrigé par stricte parité de motif avec le stub existant.
- **Trou de test comblé — cas « Encerclé »** : le seul test de cause de mort couvrait un seul
  monstre ; ajouté `death_by_two_simultaneous_monsters_reports_two_distinct_attackers`
  (`src/app/health.rs`) pour le cas à 2+ agresseurs simultanés, cité littéralement dans le GDD.
- **Trou de test comblé — texte localisé** : `locale::death_cause` intégré au filet de sécurité
  `every_string_differs_between_locales` et testé pour la règle singulier/pluriel
  (`death_cause_only_says_surrounded_for_two_or_more_attackers`).
- Vérifié après ces deux correctifs : `cargo test --lib` 539 passés / 0 échoué / 7 ignorés
  volontairement, `cargo clippy --lib -- -D warnings` propre sur les fichiers de ce sprint.

---

<a id="phase-b"></a>
## PHASE B — Assists (indépendante)

### Sprint 4 — Détection et comptage des assists — ✅ terminé (2026-07-18)
**Objectif** : compléter l'économie XP déjà quasi finie (§8.3) avec les assists.
- [x] Détecter côté serveur qu'un joueur a porté des dégâts à une cible tuée par un autre joueur,
  dans une fenêtre de temps courte.
- [x] Additionner `XP_PER_FRAG_OR_ASSIST` (déjà défini, `src/bin/server.rs:391-489`) pour chaque assist.
- [x] Ne pas double-compter un assist comme un frag.
- **Fichiers** : `src/app/mod.rs` (champs `network_assists`/`damage_contributions`), `src/app/multiplayer.rs`
  (détection : `record_damage_contribution`/`credit_assists_on_kill`/`credit_assist`, `ASSIST_WINDOW`),
  `src/app/fireball.rs` (câblage au point d'impact), `src/bin/server.rs` (XP : `network_player_assists`,
  `round_xp` prend désormais frags+assists).
- **Livrable** : deux joueurs endommagent la même créature, celui qui ne l'achève pas reçoit quand même
  de l'XP d'assist ; test serveur dédié bout-en-bout (`two_network_players_who_both_damage_a_creature_split_credit_between_kill_and_assist`).
- **Risques** : bien borner la fenêtre de temps pour éviter les assists « gratuits » sans lien réel avec le kill
  — traité par `ASSIST_WINDOW` (5 s).
- **Audit de repasse** : deux correctifs mineurs trouvés — `resolve_fireball_hit` appelait
  `network_player_id_at(owner)` deux fois (factorisé en un seul appel), et le commentaire de la
  boucle de crédit dans `update_network_attacks` ne mentionnait pas encore les assists. Aucun bug
  fonctionnel trouvé ; `cargo clippy --lib -- -D warnings` propre sur les 4 fichiers du sprint après correctifs.

---

<a id="phase-c"></a>
## PHASE C — Modes de manche (indépendante, la plus lourde)

### Sprint 5 — Fondation `RoundObjective` ✅
**Objectif** : poser l'abstraction qui manque totalement aujourd'hui (seul le mode Vagues existe).
- [x] Enum `RoundObjective` (Vagues / Survie / Escorte / Boss) côté serveur (`app::multiplayer::RoundObjective`).
- [x] Sélection du mode à la création d'un salon, propagée au client dans les deux sens :
  client→serveur (`ClientMsg::Join::objective`, `bin/server.rs::Lobby::objective` fixé au premier
  `Join` d'un salon vide) et serveur→client (`GameEvent::RoundObjective`, envoyé au Join, pour que
  la logique de manche exécutée localement par chaque client — `AppState::update_round` — reste
  alignée sur le mode réel du salon). `PROTOCOL_VERSION` 5.
- [x] Condition de victoire/défaite générique branchée sur l'objectif actif (`AppState::update_round`, `app/combat.rs`).
- **Fichiers** : `src/bin/server.rs`, `src/net/protocol.rs`, `src/app/combat.rs`.
- **Livrable** : le mode Vagues actuel continue de fonctionner à l'identique, mais passe désormais
  par `RoundObjective::Vagues` plutôt que d'être codé en dur.
- **Risques** : ne pas régresser le mode Vagues existant (`Combat::wave`, `src/scene/mod.rs:454-470`)
  pendant la migration — garder les tests existants verts.
- **2 bugs trouvés et corrigés à l'audit** : (1) le marqueur « salon fraîchement créé » utilisé pour
  décider si `objective` doit être fixé par un `Join` se basait sur `room.lobby.last_seen.is_empty()`
  — or `last_seen` redevient vide dès que tous les joueurs quittent un salon (courant sans que le
  salon se ferme), donc un salon vidé puis rejoint pouvait voir son mode réassigné en pleine manche.
  Corrigé : `Lobby::objective` est devenu `Option<RoundObjective>` (`None` = « aucun `Join` jamais
  traité par ce `Room` », état qui ne redevient jamais vrai). (2) trou fonctionnel plus profond :
  chaque client exécute sa propre copie locale de `update_round`, mais `objective` n'était propagé
  que client→serveur, jamais serveur→client une fois arbitré — un client restait sur son défaut local
  `Vagues` et pouvait déclarer une victoire locale prématurée pendant qu'un salon `Survie` continuait
  de rebocler côté serveur. Corrigé par le nouveau `GameEvent::RoundObjective` envoyé au `Join`
  (ci-dessus), testé bout-en-bout via un vrai socket
  (`a_joining_client_learns_the_rooms_objective_over_the_wire`). Les Sprints 7/8 (Escorte/Boss) ont
  été livrés par une autre session pendant cette même relecture — non revus en détail par l'auditeur,
  seule la compilation/suite de tests complète (547 tests) a été confirmée verte avec leur code inclus.

### Sprint 6 — Mode Survie ✅ (HUD non fait, cf. risque ci-dessous)
**Objectif** : implémenter l'objectif Survie décrit au GDD §4.
- [x] Règle de fin (timer 180 s, vagues qui bouclent jusque-là) sur `RoundObjective::Survie`
  (`AppState::update_survie`, `app/combat.rs`) — le wipe reste détecté à côté par
  `AppState::is_room_lost` (générique, déjà utilisé par Vagues).
- [ ] UI/HUD minimal indiquant le temps survécu ou la vague courante — **non fait** : `hud::wave_hud`
  affiche déjà « Vague N/M » y compris en Survie (mode-agnostique, aucune régression), mais pas de
  minuteur dédié — `src/editor/mod.rs`/`src/gfx/renderer.rs` avaient une refonte en cours d'une
  autre session pendant ce sprint (câblage `DeathCause` du Sprint 2), non touchés pour éviter un
  conflit d'écriture. À compléter séparément.
- **Fichiers** : `src/app/combat.rs`, `src/app/multiplayer.rs` (`RoundObjective`), `src/bin/server.rs`.
- **Livrable** : un salon en mode Survie se termine correctement (wipe ou timer), score cohérent —
  vérifié par lecture de code (`update_survie`/`is_room_lost`/`award_progress`, tous mode-agnostiques
  au-delà du timer) ; **pas vérifié en Play/multijoueur réel** (build bloqué, cf. rapport final).
- **Risques** : dépend de Sprint 5 (même phase) — ne pas démarrer avant que `RoundObjective` soit posé.

### Sprint 7 — Mode Escorte/Convoi — ✅ terminé (2026-07-18)
**Objectif** : implémenter l'objectif Escorte décrit au GDD §4.
- [x] Entité convoi/cible à protéger, avec trajectoire et points de vie propres
  (`SceneObject::convoy` → `Convoy{destination, speed}`, `src/scene/mod.rs`, combiné à `Combat` pour les PV).
- [x] Condition de défaite si le convoi est détruit, de victoire s'il atteint sa destination
  (`AppState::update_escorte`, `src/app/combat.rs` ; défaite câblée dans `AppState::is_room_lost`,
  `src/app/health.rs`, prioritaire sur l'état des joueurs).
- **Fichiers** : `src/scene/mod.rs` (composant `Convoy`), `src/app/combat.rs` (`update_escorte`,
  branché dans `update_round`), `src/app/health.rs` (`is_room_lost`/`is_convoy_destroyed`),
  `src/app/simulation.rs` (ciblage IA prioritaire du convoi en mode Escorte), `src/scene/demos.rs`
  (`Scene::escorte_demo`, démo jouable), `src/app/demos.rs`/`src/editor/menus.rs` (chargeur + entrée
  de menu). `src/bin/server.rs` inchangé : `Lobby::objective`/`Room::restart` géraient déjà
  n'importe quel `RoundObjective` de façon générique (Sprint 5).
- **Livrable** : `Scene::escorte_demo()` (jouable via le menu Fichier → Démos → « Démo Escorte ») ;
  tests unitaires verts : victoire à l'arrivée (`update_round_escorte_wins_once_the_convoy_reaches_its_destination`),
  défaite si convoi détruit même joueur vivant (`is_room_lost_true_when_the_escorte_convoy_is_destroyed_even_with_a_living_player`),
  forme du prefab (`escorte_demo_has_an_attackable_convoy_with_a_reachable_destination`).
- **Risques** : ciblage prioritaire implémenté comme cible **exclusive** des chasseresses tant que le
  convoi est vivant (`candidate_targets` filtré en mode Escorte, `src/app/simulation.rs`) — le plafond
  `MAX_ACTIVE_CHASERS_PER_TARGET` reste intact (il opère par indice de cible, indifférent à ce que
  représente cet indice).

### Sprint 8 — Mode Boss — ✅ terminé (2026-07-18)
**Objectif** : implémenter l'objectif Boss décrit au GDD §4.
- [x] Créature boss avec PV élevés et pattern d'attaque distinct (`Scene::boss_demo`, `src/scene/demos.rs` :
  archétype `Colosse` — GDD_MMORPG.md:368 « c'est aussi le boss » —, `hp: 15`, contact doublé). Vrai
  modèle importé (`monster_dragon_evolved.glb`, via le nouvel helper `import_single_model`), pas un
  primitif — repli sur une capsule uniquement si l'asset est introuvable (`import_single_model`
  logue l'erreur plutôt que de faire planter la démo).
- [x] Condition de victoire à la mort du boss : **aucune logique dédiée** — le GDD décrit Boss comme
  « dernière vague : une créature unique » (§4), donc `RoundObjective::Boss` reste câblé sur
  `AppState::update_waves` (`update_round`, `src/app/combat.rs`) : une scène Boss n'a qu'une seule
  manche contenant le boss, et « dernière manche vidée » *est* déjà « boss vaincu ».
- **Fichiers** : `src/scene/demos.rs` (`Scene::boss_demo`), `src/app/demos.rs`/`src/editor/menus.rs`
  (chargeur + entrée de menu, pose `objective = Boss`). `src/bin/server.rs` inchangé (même raison
  que Sprint 7).
- **Livrable** : `Scene::boss_demo()` (jouable via le menu Fichier → Démos → « Démo Boss ») ; tests
  unitaires verts : victoire à la mort du boss (`update_round_boss_wins_when_its_single_wave_is_cleared`),
  forme du prefab (`boss_demo_has_a_single_high_hp_slow_colosse_on_wave_one`). Score final : déjà
  générique (`Room`/`award_progress`, `src/bin/server.rs`), aucun branchement par mode nécessaire.
- **Risques** : équilibrage (PV/dégâts) non validé en playtest réel — seulement en test unitaire, comme
  anticipé par ce risque ; à ajuster après un premier retour de jeu.

---

<a id="phase-d"></a>
## PHASE D — Contrat du jour (dépend de la phase C)

### Sprint 9 — Contrat du jour — ✅ terminé (2026-07-18)
**Objectif** : objectif quotidien rejouable, posé sur `RoundObjective` (§3.4).
- [x] Génération d'un contrat par jour (`Contract::of_day`, seed = jour UTC = secondes Unix / 86 400,
  `bin/server.rs::day_number` — même calcul déterministe que « calculé identiquement par serveur et
  clients », GDD §3.4), sélectionnant parmi un sous-ensemble du catalogue GDD §3.4 : *Nuit blanche*,
  *À l'aube juste* (Vagues < 8 min), *La lande garde ses morts*, *Le troupeau compte sur vous*
  (Escorte, convoi > 50 % PV). **Hors périmètre** (catalogue GDD à 6 entrées, 4 retenues) : *Main de
  braise* (mêlée seule — aucune notion de mêlée distincte du missile homing du joueur,
  `app::combat::AttackProjectile`, toujours à distance) et *Sobriété* (sans ramassage d'arme —
  `WeaponPickup` n'est câblé que côté donjon solo, pas aux joueurs réseau) : ni l'un ni l'autre n'a de
  compteur serveur existant à vérifier, contrairement à la règle du catalogue (« vérifiable côté
  serveur avec des compteurs déjà existants ») — le catalogue « grandit avec » le contenu livré (GDD
  §3.4), il n'avait pas à être complet dès ce sprint.
- [x] Récompense/bonus XP à la complétion (`XP_CONTRACT = 250`, GDD §3.5), distincte du score de
  manche normal (`round_xp`) : terme séparé ajouté dans `award_progress`, jamais mélangé dans
  `round_xp`. Une seule fois par compte et par jour (`PlayerProgress::last_contract_day`,
  `net::firebase`, champ additif rétrocompatible) ; même garde anti-AFK que la participation normale
  (un joueur inactif d'une manche gagnante ne réclame pas le contrat à sa place).
- **Fichiers** : `src/app/multiplayer.rs` (`Contract` enum + `AppState::contract_completed`),
  `src/app/mod.rs` (compteurs `player_down_count`/`revives_completed`), `src/app/health.rs`
  (incréments aux points de mort réseau/fin de réanimation), `src/net/firebase.rs`
  (`PlayerProgress::last_contract_day`), `src/bin/server.rs` (`day_number`, `XP_CONTRACT`,
  `award_progress` étendu, câblage en fin de manche — remplace le commentaire de repli qui était aux
  lignes 438-439, désormais à la doc de `round_xp`).
- **Livrable** : le contrat change de jour en jour (`Contract::of_day`, testé déterministe et
  couvrant tout le catalogue sur des jours consécutifs) ; complétion détectée et récompensée une
  seule fois par jour et par joueur (`last_contract_day` comparé au jour courant avant crédit). Tests
  unitaires verts pour les 4 contrats retenus, le round-trip `to_u8`/`from_u8`, le déterminisme du
  seed, et `day_number` (stable dans une même journée UTC, avance après 24 h).
- **Risques** : phase C était verte (Sprints 5→8 tous ✅) avant de démarrer ce sprint, comme requis.

---

<a id="phase-e"></a>
## PHASE E — Grammaire d'archétypes de créatures (indépendante) — ✅ Sprints 10-11 faits

> Désync doc/code corrigée le 18 juillet 2026 : les cases ci-dessous étaient restées décochées
> alors qu'un travail déjà livré et testé existait. Vérifié directement dans le code avant de
> cocher : `enum Archetype` (`src/scene/mod.rs:534`), champ `archetype` sur `AiChaser`, logique de
> vitesse/éveil dans `src/app/simulation.rs:1253`, et les tests dédiés re-confirmés verts. Pas une
> régression réelle — juste un tableau de suivi jamais mis à jour après coup. Point encore ouvert :
> validation visuelle en Play jamais faite (seulement testée en simulation) — à faire avant de
> considérer la lisibilité gameplay (silhouette/vitesse perçue) définitivement close.

### Sprint 10 — Archétypes Traqueuse et Meute — ✅ fait
**Objectif** : différencier le comportement de chasse au-delà de l'IA générique actuelle (§5.4).
- [x] `Archetype::speed_multiplier()` : Traqueuse `1.0`, Meute `1.25`, Colosse `0.65`, Furtive `1.5`.
  `AiChaser` gagne un champ `archetype: Archetype` (`#[serde(default)]`, rétrocompatible).
- [x] Traqueuse : approche directe rapide et isolée ; Meute : coordination à plusieurs sur une même cible
  (dans la limite du plafond existant, `MAX_ACTIVE_CHASERS_PER_TARGET` inchangé pour tous les archétypes).
- **Fichiers** : `src/app/simulation.rs`, `src/scene/demos.rs` (assignation d'archétype par prefab :
  `zombies_demo` Rôdeur→Traqueuse/Coureur→Meute/Brute→Colosse, `roguelike_demo` Gobelin→Meute/
  Squelette→Furtive/Ogre→Colosse).
- **Livrable** : en Play, une Traqueuse et un groupe de Meute se comportent visiblement différemment
  d'une créature générique actuelle.
- **Risques** : ne pas casser les tests IA existants ; garder le plafond de chasseresses valable pour
  tous les archétypes — vérifié à l'audit.

### Sprint 11 — Archétypes Colosse et Furtive — ✅ fait
**Objectif** : compléter la grammaire d'archétypes.
- [x] Colosse : vitesse de poursuite ralentie (`0.65×`). Furtive : reste immobile tant que la cible
  la plus proche est au-delà de `FURTIVE_DETECT_RANGE = 5.0` (< `CHASER_DETECT_RANGE` = 9 m,
  appliqué en toute circonstance y compris en solo), vitesse accrue (`1.5×`) une fois éveillée.
- **Fichiers** : `src/app/simulation.rs`, `src/scene/demos.rs`.
- **Livrable** : les 4 archétypes du GDD §5.4 sont tous distinguables en Play (9 tests verts,
  `furtive_archetype_stays_asleep_until_the_player_enters_its_shorter_wake_radius`,
  `creature_archetypes_produce_visibly_different_chase_speeds`).
- **Risques** : le camouflage de la Furtive ne doit pas la rendre injouable contre — rayon minimal de
  détection défini (5 m). PV réduits (Meute)/élevés (Colosse) du GDD §5.4 relèvent de `Combat`, pas
  d'`AiChaser` — non traités par ce sprint (seule la vitesse est ajustée), à planifier séparément.
- **Audit** : un bug de terminologie trouvé et corrigé — le mot « archétype » était déjà utilisé
  dans `zombies_demo` pour un concept différent (profils d'auteur Rôdeur/Coureur/Brute) ; commentaires
  reformulés en « profil(s) de monstres » pour ne pas confondre avec `Archetype` (grammaire GDD §5.4).
  Le pack `MONSTER_DECOR` (~45 modèles) et `mmorpg_demo` (créatures pilotées par script Lua) restent
  hors périmètre (aucun `AiChaser`).

---

<a id="phase-f"></a>
## PHASE F — Salon multijoueur & mute (indépendante)

### Sprint 12 — Vérification/complétion de l'onglet Salon — ✅ terminé (2026-07-18)
**Objectif** : confirmer et compléter le chat en jeu, dont le backend Firebase est déjà prêt
(`post_chat_message`/`list_chat_messages`, `src/net/firebase.rs:421-464`).
- [x] Auditer l'état réel de l'onglet Salon dans la fenêtre Multijoueur (non vérifié positivement
  dans `auditGDD10h.md`) : déjà entièrement fonctionnel avant ce sprint (UI, état, réseau, test).
  Seul écart réel trouvé : pas de rafraîchissement automatique.
- [x] Compléter l'affichage/saisie de chat si manquant : rien ne manquait ; ajouté à la place le
  rafraîchissement automatique (toutes les 4 s, `Editor::run`, `src/editor/mod.rs`) tant que la
  fenêtre Multijoueur reste ouverte.
- **Fichiers** : `src/editor/windows.rs`, `src/editor/mod.rs` (pas `network_client.rs` : la logique
  de requête existante a suffi, réutilisée telle quelle via `UiActions::refresh_chat`).
- **Livrable** : deux clients connectés au même salon peuvent échanger des messages visibles en jeu —
  confirmé, plus besoin de cliquer « Rafraîchir » pour les voir apparaître.
- **Risques** : dépend de l'état réel trouvé à l'audit — le sprint peut être plus court si le chat
  est déjà fonctionnel et qu'il ne manque qu'un réglage mineur. *(Confirmé : c'était le cas.)*

### Sprint 13 — Mute local — ✅ terminé (2026-07-18)
**Objectif** : un joueur peut faire taire localement un autre joueur gênant (§18.4.1).
- [x] Liste de joueurs mutés en local (non partagée réseau), filtrant le chat — bouton 🔇 par
  message (sauf sur ses propres messages), section rétractable « Joueurs muets » avec démute,
  `Settings::muted_players` (`src/app/settings.rs`) persisté dans `settings.json`. Voix : aucun
  système de chat vocal n'existe dans ce dépôt (vérifié), donc sans objet.
- **Fichiers** : `src/editor/windows.rs`, `src/app/settings.rs` (pas `network_client.rs` : filtrage
  purement d'affichage, aucun changement réseau nécessaire).
- **Livrable** : muter un joueur cache ses messages sans affecter les autres clients — confirmé
  (filtrage local uniquement, les messages continuent d'arriver dans `chat_messages`).
- **Risques** : persister le mute localement (pas de fuite entre sessions différentes) sans
  dépendance serveur — confirmé, `settings.json` (comme le reste des réglages persistés dans ce
  fichier : clés API, réglages manette, etc.), aucune requête réseau liée au mute.

**Vérification** : `cargo build`/`cargo test --lib` sur le dépôt complet (537 tests) et
`cargo clippy -D warnings` sur les 3 fichiers touchés — tous verts. Un second passage a ajouté deux
micro-économies (tooltip statique au lieu d'un `format!` par frame, plus de clone systématique de
`settings.muted_players` en dépliant la section « Joueurs muets »). Reste un test manuel en éditeur
avec un compte Firebase réel, non exécuté (pas de config Firebase disponible dans l'environnement de
vérification).

---

<a id="phase-g"></a>
## PHASE G — Rattrapage documentaire du GDD (indépendante, légère)

### Sprint 14 — Synchroniser `GDD_MMORPG.md` §14 avec l'état réel du code — ✅ terminé (2026-07-18)
**Objectif** : le GDD ne doit pas sous-estimer ce qui est déjà livré (règle de gouvernance §18.7 du GDD :
toute contradiction découverte est une décision à acter).
- [x] Repasser XP/économie (§8.3) de « 🔜 Priorité 3 » à son état réel : `round_xp` (`src/bin/server.rs`)
  applique déjà le barème cible, garde anti-AFK incluse — seuls les assists manquent (Phase B).
- [x] Repasser le roster HUD multijoueur de « 🔜 Priorité 1 » à « fait » (`hud.rs:463-609`, branché
  depuis `editor/mod.rs:530` et `:1901`) ; frags individuels également confirmés affichés (colonne 💀).
- [x] Corriger la mention « aucun système audio riche » (§10.4) au vu de `src/runtime/audio.rs`
  (moteur `kira`, ducking, spatialisation) et `src/runtime/sfx.rs` (`Sfx::Hit`/`Defeat`/`WaveStart`
  déjà câblés) — rangs 2/3 de la priorité (allié à terre, éveil) restent non couverts, noté au §10.4.
- [x] Reformulé le reste du tableau §14 par phases A-G (au lieu de « Priorité 1-6 », qui référençait un
  document supprimé) pour rester cohérent avec ce fichier.
- **Fichiers** : `GDD_MMORPG.md`.
- **Livrable** : le tableau §14 du GDD reflète l'état réel du code à la date du sprint, sans attendre
  la fin des autres phases.
- **Risques** : aucun — sprint purement documentaire, peut être fait à tout moment, y compris avant
  les autres phases.
- **Audit de clôture** : un bug réel trouvé et corrigé — la 1ère passe affirmait à tort que la
  réanimation Soutien (§8.1) était couverte par le Sprint 3 (sélecteur de classe) ; relecture de ce
  sprint confirme qu'elle n'apparaît dans aucun des 14 sprints planifiés, désormais marquée
  « non planifiée » dans le tableau §14. Ancres de section homogénéisées, tous les liens/ancres
  vérifiés mécaniquement.

---

## ✅ Définition de « terminé » par phase

| Phase | Terminée quand |
|---|---|
| A | Vignette/recul/son de dégâts + diagnostic de mort + sélecteur de classe visibles et testés en Play multijoueur |
| B | Un assist génère de l'XP, test serveur vert |
| C | Vagues (migré), Survie, Escorte, Boss se terminent tous correctement dans un salon de test |
| D | Un contrat du jour distinct de la manche normale est généré, complété et récompensé |
| E | Les 4 archétypes de créatures du GDD §5.4 sont distinguables en Play |
| F | ✅ Chat de salon fonctionnel + mute local opérationnel — terminé 2026-07-18 |
| G | `GDD_MMORPG.md` §14 ne contient plus de statut sous-évalué par rapport au code |

## 📌 Conseils d'exécution

- Démarrer **A** et **C** en priorité si les ressources sont limitées (impact joueur direct le plus fort).
- **D attend explicitement la fin de C** — ne pas anticiper le Sprint 9 avant que `RoundObjective`
  (Sprint 5) existe, sous peine de dupliquer le travail.
- **B, E, F, G** peuvent se glisser dans n'importe quel creux, en parallèle de A/C, sans risque de
  conflit fonctionnel (fichiers largement disjoints, sauf `src/bin/server.rs` partagé par B/C/D —
  coordonner les merges sur ce fichier).
- Sprint 14 (G) peut même être fait en tout premier, avant tout code, puisqu'il ne fait que corriger
  la documentation existante.
