# RusteeGear — Game design du multijoueur, vers une logique MMORPG

> Document d'**audit + optimisation de game design**, distinct de
> [SPRINT_MMORPG.md](SPRINT_MMORPG.md) (feuille de route technique
> sprint par sprint) et [AUDIT_MMORPG.md](AUDIT_MMORPG.md) (revue de code
> réseau). Ici : *quel jeu est-ce qu'on fait jouer aux gens*, pas *comment le
> code est câblé*. Sert de référence avant de lancer de nouveaux sprints
> gameplay (numérotés à la suite dans SPRINT_MMORPG.md, cf. §7).
>
> Portée volontairement **petite** (§0 de SPRINT_MMORPG.md, verrouillé le
> 2026-07-07) : salons de 2-16 joueurs, pas de monde persistant partagé. Ce
> document emprunte des **mécaniques** au genre MMORPG (progression, rôles,
> loot, zones, économie sociale) sans en emprunter l'**échelle** — un MMO
> à sharding de zones et base de données distribuée est un projet différent,
> hors de portée d'un moteur solo.

---

## 1. Où en est le jeu aujourd'hui (état des lieux honnête)

Ce qui existe et fonctionne, vérifié en conditions réelles (VPS + APK + macOS) :

- **Boucle de jeu** : manches (`Combat::wave`) — les monstres de la manche N
  masqués jusqu'à ce que la manche N-1 soit vidée, victoire à la dernière.
  Un seul mode de boucle pour l'instant (vagues linéaires, pas de zones à
  explorer librement, pas d'objectifs parallèles).
- **Combat** : mêlée à préparation (`attack_windup`, missile homing verrouillé
  sur cible, `Controller::attack_mode` Single/Zone) + attaque à distance à
  trois armes (`app/fireball.rs`, Sprint 78-79 : Boule de feu / Éclair /
  Boulet, visée dans la direction du regard via `aim_yaw` réseau).
- **Monstres** : `AiChaser` (poursuite en ligne droite d'un point unique),
  `Combat::hp` (PV multiples), respawn à délai fixe. Diffusés en lecture
  seule aux clients (`player_id: None` dans `Snapshot`) depuis le Sprint 78.
- **Joueurs réseau** : chacun son objet cloné du gabarit (`spawn_network_player`),
  déplacement + attaque + tir validés côté serveur, prédiction/réconciliation
  client soignée (Sprints 66-77, latence VPS réelle ~200 ms).
- **Progression** : Firebase RTDB en backend annexe — comptes (`sign_in`/
  `sign_up`), XP/niveau (`PlayerProgress`, formule plate `XP_PER_LEVEL`),
  classement, chat de salon. Écrit une fois en fin de manche (`award_progress`).
- **Économie d'objets** : `WeaponPickup` (mêlée uniquement) — ramasser une arme
  au sol change le profil du joueur (portée/recharge/mode), pas d'inventaire,
  pas de choix stratégique persistant (l'arme se perd au reset de manche).

Ce qui **n'existe pas encore**, documenté dans le code et le README :

- **Vie non individualisée** : `hud_health` est un champ unique par `AppState`
  — la victoire/défaite du salon entier dépend d'*un* joueur gabarit, pas de
  chacun. Un joueur ne peut pas mourir sans que la manche entière échoue.
  C'est le trou le plus structurant : tant qu'il n'est pas comblé, aucune
  vraie coopération à enjeux individuels (ni PvP) n'est possible.
- **Pas de dégâts joueur-contre-joueur** : la boule de feu traverse les autres
  joueurs (conséquence directe du point précédent).
- **IA à cible unique** : `AiChaser` vise `self.player_position()` — le
  joueur *local* de l'`AppState` serveur, pas « le joueur réseau le plus
  proche ». À 2 joueurs réseau, un seul est jamais poursuivi.
- **Un seul salon** : `src/bin/server.rs` = un process = une manche. Pas de
  lobby, pas de sélection de salle, pas de matchmaking.
- **Pas de rôles/classes** : tous les joueurs partagent le même gabarit et
  les mêmes armes. Aucune spécialisation, aucune complémentarité d'équipe.
- **Inventaire non persistant** : les armes ramassées en jeu ne survivent pas
  à la manche ; seuls XP/niveau sont sauvegardés.

C'est un bon squelette de **shooter coop à vagues** (façon *Call of Zombies*
en encore plus simple). Ce document propose comment le faire évoluer vers
une **sensation MMORPG légère** : progression qui compte, rôles qui
comptent, un monde qui donne des raisons de revenir — sans jamais sortir
de l'échelle 2-16 joueurs/salon décidée au Sprint 50.

---

## 2. Principes de design retenus pour la suite

Quatre règles pour trancher les choix ci-dessous, à appliquer à toute
nouvelle mécanique avant de l'ajouter :

1. **La vie individualisée est le prérequis de tout le reste.** Rôles,
   PvP, loot personnel, mort permanente d'un perso dans un groupe qui
   continue — rien de tout ça n'a de sens tant qu'un joueur peut mourir
   sans que ça affecte que lui. C'est la dépendance bloquante n°1
   (cf. §3.1) : à traiter avant toute autre extension de ce document.
2. **Le serveur reste seul juge, toujours.** Chaque nouvelle mécanique
   (loot, classe, compétence) doit avoir sa résolution côté serveur
   headless, jamais côté client — c'est la garantie anti-triche déjà en
   place pour le mouvement/l'attaque/le tir, à ne jamais relâcher.
3. **Coopératif d'abord, compétitif ensuite si demandé.** Le PvP est *listé*
   ici (§3.7) mais reste optionnel — le jeu actuel (vagues de monstres) est
   fondamentalement coopératif ; en faire la priorité serait un pivot de
   direction, pas une optimisation. À activer seulement sur demande explicite.
4. **Petites itérations vérifiables**, comme le reste du projet : chaque
   proposition ci-dessous doit pouvoir se découper en sprints testables
   unitairement + en bout-en-bout via de vrais sockets, dans l'esprit de
   SPRINT_MMORPG.md.

---

## 3. Axes d'optimisation, par ordre de priorité

### 3.1 Vie individualisée par joueur — **bloquant, priorité absolue** ✅ FAIT (Sprint 80)

**Problème.** `AppState::hud_health: Option<f32>` est un champ scalaire,
pensé pour un joueur local unique. `is_lost()`/`has_won()` en dépendent :
en ligne, la manche entière capote si le gabarit d'origine (masqué, personne
ne le pilote) tombe à zéro — cf. le bug déjà corrigé une fois
(AUDIT_MMORPG.md §4.5), symptôme direct de ce même défaut de conception.

**Proposition.**
- Remplacer `hud_health: Option<f32>` par une table `player_health:
  HashMap<PlayerId, f32>` (+ un scalaire `local_health` conservé pour le
  mode solo, qui n'a pas de `PlayerId`). Chaque script `damage()`/`set_health()`
  cible désormais *l'objet appelant*, pas un état global.
- Un joueur réseau à 0 PV devient **spectateur** (objet masqué, entrées
  ignorées) au lieu de mettre fin à la manche — la manche continue tant
  qu'il reste au moins un joueur vivant.
- `EntityDelta::health` (déjà prévu dans le protocole, jamais rempli côté
  joueurs — seulement documenté comme limite connue) devient enfin non-`None`
  pour les joueurs : chaque client voit la vie de chacun (barre au-dessus du
  fantôme, cf. §3.4).
- Victoire de salon = dernière manche vidée **et** au moins un survivant ;
  défaite de salon = tous les joueurs à 0 PV (pas un seul).

**Pourquoi en premier.** Tout ce qui suit (rôles, PvP, loot personnel, mort
permanente) présuppose qu'un joueur individuel a un état de vie qui lui est
propre. Le faire maintenant évite de retoucher deux fois la même surface.

**Réalisé (Sprint 80, `src/app/health.rs`)** : `AppState::network_health:
HashMap<PlayerId, f32>`, une vie par joueur réseau (0..1), drainée au contact
d'un monstre `AiChaser` visible (~6 s pour mourir de plein PV à un seul
monstre) et régénérée passivement hors contact. Un joueur à 0 PV devient
spectateur (`visible = false`, entrées ignorées — mouvement, attaque, tir),
sans mettre fin à la manche pour les autres : `AppState::is_room_lost()`
(défaite de **salon**, tous vaincus) remplace `is_lost()` côté serveur
headless dès qu'un salon a des joueurs réseau ; `is_lost()` reste inchangé en
solo. `EntityDelta::health` porte désormais la vie de chaque joueur dans le
`Snapshot` (plus `None` par défaut), et `GameEvent::PlayerDown` prévient les
clients de chaque mort. `hud_health` (solo) est resté intact, aucune
régression. 10 tests de régression (contact, régénération, mort, entrées
ignorées, salon perdu seulement si tous vaincus).

---

### 3.2 IA multi-cibles (cf. `AiChaser`) ✅ FAIT (Sprint 80)

**Problème.** `chase_target = self.player_position()` (un seul point,
`src/app/mod.rs:2269`) — avec 2+ joueurs réseau, l'IA ignore tout le monde
sauf le gabarit local (qui, en headless, est masqué et immobile : cf.
`hide_local_player_template`). En pratique aujourd'hui, les monstres
`AiChaser` ne poursuivent donc **personne** de réel en multijoueur pur —
seule l'attaque à distance des joueurs contre eux fonctionne, pas le danger
qu'ils sont censés représenter.

**Proposition.**
- `chase_target` devient une fonction du monstre, pas un scalaire de
  l'`AppState` : pour chaque `AiChaser` visible, poursuivre le joueur
  (local ou réseau) **vivant** le plus proche, recalculé chaque frame —
  cohérent avec le principe déjà appliqué à `nearest_attackable` (le
  joueur choisit sa cible la plus proche ; l'IA doit pouvoir en faire autant).
- Prérequis : §3.1 (savoir qui est vivant) pour ne pas cibler un spectateur.
- Effet de gameplay : les monstres redeviennent une vraie menace en coop —
  aujourd'hui un groupe de joueurs réseau peut ignorer leur existence.

**Réalisé (Sprint 80)** : `chase_target` (point unique) remplacé par
`candidate_targets: Vec<Vec3>` — en solo, le joueur local (comportement
inchangé) ; en réseau, chaque joueur réseau **vivant** et visible. Chaque
`AiChaser` recalcule sa cible la plus proche à chaque frame. Les 5 monstres
de la carte embarquée (`assets/player_scene.json`) portent désormais
`ai_chaser` (ils poursuivent réellement, au lieu de rester des cibles de tir
immobiles) — sans quoi la vie individualisée du §3.1 n'aurait aucun effet
observable en jeu. Test de régression : un monstre loin de deux joueurs finit
par se rapprocher du plus proche, pas de celui arrivé en premier.

### 3.3 Multi-salons (lobby) ✅ FAIT (Sprint 82)

**Problème.** `src/bin/server.rs` sert un seul salon par process. Pour
plusieurs groupes simultanés, il faut lancer plusieurs processus/ports
manuellement — pas de découverte, pas de matchmaking.

**Proposition (portée mesurée, pas un vrai matchmaking MMO).**
- `NetServer` accepte toujours toutes les connexions, mais `main()`
  maintient `HashMap<LobbyCode, AppState>` au lieu d'un `AppState` unique —
  chaque `ClientMsg::Join` porte un `lobby: String` (défaut `"default"`,
  rétrocompatible) choisissant sa manche.
- Chaque salon a son propre tick, ses propres joueurs, sa propre victoire/
  défaite — copier `handle_message`/la boucle de `main` par salon plutôt que
  d'introduire un vrai scheduler de jeux : à cette échelle (2-16 × quelques
  salons), un `HashMap` + boucle séquentielle par tick suffit largement (déjà
  mesuré à 16 joueurs sur un seul salon, marge confortable, Sprint 61).
- Pas de découverte de salons dans l'UI pour l'instant (fenêtre Multijoueur
  actuelle : taper un code, comme `mp_lobby_code` déjà présent pour le chat
  Firebase — le réutiliser tel quel pour le lobby réseau serait cohérent).

**Réalisé (Sprint 82, `src/bin/server.rs::Room`)** : exactement la
proposition ci-dessus — `HashMap<String, Room>` (`Room` = `AppState` + état de
salon), `ClientMsg::Join::lobby` (nouveau champ, défaut `net::protocol::
DEFAULT_LOBBY` côté `NetClient::connect` — tous les clients actuels
continuent donc à se retrouver dans le même salon partagé, aucune régression
de comportement). `NetServer::broadcast()` va à TOUS les clients du process
(pas de notion de salon à ce niveau) — les envois par salon utilisent
`send_to` en boucle sur les joueurs de ce salon seulement. **Amélioration au
passage, pas prévue dans la proposition initiale** : une manche décidée
(victoire/défaite) ne termine plus le *process* entier (avant ce sprint,
`break` sortait de la boucle et arrêtait le binaire — systemd le relançait,
mais coupait la connexion de TOUS les joueurs, y compris ceux d'autres
salons sans rapport). Elle réinitialise désormais ce salon **en place**
(les joueurs encore connectés y sont re-spawnés) ; un salon vide (dernier
joueur parti) est fermé plutôt que de tourner pour personne. **Reporté** :
la sélection du salon dans l'UI (fenêtre Multijoueur) — `connect_to_lobby`
existe côté `NetClient`, mais `AppState::connect_to_server` (appelé par
l'éditeur) continue de rejoindre `DEFAULT_LOBBY` ; brancher un champ de
saisie dans `editor/mod.rs` est un prochain petit sprint, une fois ce
fichier stable (cf. la même réserve que pour §3.4). 4 tests de régression
(bout-en-bout via de vrais sockets) : isolation entre deux salons, fermeture
d'un salon vidé.

### 3.4 Vie et identité des autres joueurs à l'écran 🟢 backend fait (Sprint 80), HUD reporté

**Problème.** Un fantôme réseau (`ensure_remote_player`) affiche position +
orientation, mais pas son nom ni sa vie — on voit *quelqu'un* bouger, sans
savoir qui, ni s'il est en danger.

**Proposition.**
- Une fois §3.1 fait, afficher une petite barre de vie flottante au-dessus
  de chaque fantôme (et du joueur local) dans le HUD egui, positionnée par
  projection écran de `transform.position + Vec3::Y * hauteur` — même
  technique que le HUD d'arme actuel (Sprint 79), pas de nouveau système.
- Afficher le nom (`PlayerJoined::name`, déjà reçu et stocké dans
  `RemotePlayer`, jamais affiché) au-dessus du fantôme.
- Effet : un salon de 4+ joueurs devient lisible — qui est en danger, qui
  peut réanimer qui (cf. §3.6).

**Réalisé (Sprint 80)** : `RemotePlayer::health` et `AppState::net_local_health`
mémorisent la dernière vie reçue de chaque joueur (lue telle quelle du
`Snapshot`, pas interpolée — une vie n'a pas besoin d'être lissée comme un
mouvement) ; `AppState::multiplayer_roster()` expose la liste `(nom, vie,
soi-même ?)` pour l'UI. **Reporté** : le panneau HUD lui-même (barre
flottante par fantôme ou liste au coin de l'écran) — une autre session
travaillait au même moment sur `src/editor/mod.rs`/`src/gfx/renderer.rs`
(Sprint 81, `time_scale`), enfiler un paramètre de plus dans ces signatures
partagées au même instant était le genre de collision que
`concurrent-sessions-hazard` (mémoire du projet) demande d'éviter. Le
backend est prêt et testé ; brancher `multiplayer_roster()` sur un panneau
`egui` (liste texte simple, pas de projection 3D — plus sûr à écrire sans
pouvoir vérifier visuellement) est un prochain sprint autonome.

### 3.5 Rôles / classes légères

**Problème.** Tous les joueurs partagent le même `Controller` (mêmes
vitesse, saut, armes disponibles) — aucune complémentarité d'équipe, pas de
raison de jouer différemment d'un joueur à l'autre.

**Proposition (légère, pas un système de classes complet).**
- 3 profils choisis au `Join` (`ClientMsg::Join` gagne un champ `class: u8`,
  rétrocompatible en `0` par défaut) appliqués par `spawn_network_player` au
  clone du gabarit :
  - **Assaut** : `move_speed` standard, accès aux 3 armes à distance
    (§ Sprint 79), dégâts de mêlée normaux.
  - **Éclaireur** : `move_speed` +25 %, `jump_height` +30 %, mais PV max
    réduits — rapide, fragile, utile pour activer des déclencheurs éloignés.
  - **Soutien** : `move_speed` réduit, mais peut soigner un allié proche
    (nouvelle action, cf. §3.6) — n'attaque pas fort, mais maintient le
    groupe en vie.
- Réutilise entièrement l'infrastructure existante (`Controller` cloné par
  joueur, déjà indépendant par `spawn_network_player`) — aucun nouveau
  système réseau, juste des valeurs de départ différentes + une action.

### 3.6 Soin coopératif ✅ FAIT (Sprint 80, sans réanimation)

**Problème.** Aucune interaction joueur-vers-joueur n'existe : chacun joue
sa propre bulle de survie, sans jamais s'entraider.

**Proposition.**
- Nouvelle touche/bouton « Soin » (même schéma que `fire_button`/
  `weapon_button`) : maintenu à proximité d'un allié `player_health < max`
  (ou à 0, pour une **réanimation** après §3.1) transfère des PV au fil du
  temps — validé côté serveur (portée + recharge, même schéma que
  `NETWORK_ATTACK_RANGE`/`FIREBALL_COOLDOWN`).
- Sans le rôle Soutien (§3.5) équipé, l'action est absente ou dégradée
  (portée courte) — c'est ce qui donne un vrai poids au choix de rôle.
- Effet : premier vrai levier de coopération à enjeu (au-delà de « on tire
  chacun sur nos monstres à côté »).

**Réalisé (Sprint 80)** : touche **H** ou bouton tactile **« Soin »**
(`Controller::heal_button`) — maintenu, soigne en continu l'allié vivant le
plus proche et **blessé** (pas déjà au max) à 2,5 m, à 0,2 PV/s, résolu et
validé côté serveur (`update_network_heal`). **Écart vs la proposition
initiale** : universel (pas gated par un rôle Soutien, §3.5 non traité ce
sprint — un système de classes mérite sa propre UI de sélection à la
connexion, pas improvisée en passant) et **pas de réanimation** d'un allié
à 0 PV (mort = spectateur permanent pour cette manche, décision assumée pour
garder le scope contenu — §3.1 le documente). 4 tests de régression (soin en
portée, hors de portée, priorité au plus blessé plutôt qu'à un allié déjà
au max).

### 3.7 PvP — optionnel, à activer seulement sur demande

**Ne pas construire sans qu'on le demande explicitement** (principe #3,
§2) : le jeu actuel n'a ni équilibrage de dégâts entre joueurs, ni système
anti-abus dédié (un joueur ne peut pas actuellement griefer un allié). Si
demandé plus tard :
- Un mode de salon distinct (`GameMode::Pvp` sur le `Lobby`), pas une bascule
  globale — la coop reste le mode par défaut.
- Dégâts joueur-joueur activés uniquement dans ce mode (`fireball_impact`
  gagne un filtre par mode, aujourd'hui il exclut tout objet `controller.is_some()`
  sans condition).
- Nécessite en prérequis §3.1 (vie individualisée) — déjà couvert plus haut.

### 3.8 Loot et progression qui durent plus qu'une manche

**Problème.** `WeaponPickup` change le profil du joueur pour la manche en
cours seulement ; à la reconnexion, retour au profil de départ. Seuls
XP/niveau (Firebase) persistent, et sans effet de gameplay (un niveau 10 et
un niveau 1 jouent identiquement).

**Proposition (mesurée — pas un inventaire complet façon MMO).**
- Persister **l'arme à distance préférée** (`selected_weapon`, Sprint 79)
  par compte Firebase (`PlayerProgress` gagne un champ `favorite_weapon: u8`),
  restaurée au `Join` — un joueur retrouve son arme d'une session à l'autre.
- Le niveau (déjà calculé, `1 + xp / XP_PER_LEVEL`) débloque des **paliers
  simples** au lieu de rester cosmétique : niveau 5 débloque l'Éclair sans
  ramassage préalable, niveau 10 débloque le Boulet — donne un sens concret
  à la progression existante sans construire un système d'inventaire neuf.
- Rester délibérément **simple** : pas d'objets à stats aléatoires, pas
  d'enchantement, pas d'échange entre joueurs — hors de la portée « petit
  jeu coop » assumée au Sprint 50. Un vrai inventaire/craft serait un projet
  à part.

---

## 4. Ce qu'on ne fait *pas* (limites assumées du genre MMORPG ici)

Pour cadrer les attentes et éviter la dérive de scope — chacune de ces
exclusions découle directement de la décision de portée du Sprint 50 :

- **Pas de monde persistant partagé** entre salons : chaque salon est une
  instance de manche indépendante, comme aujourd'hui. Un vrai monde ouvert
  demanderait du sharding de zones — hors de portée solo.
- **Pas d'économie joueur-joueur** (échange, hôtel des ventes, monnaie) :
  la progression reste individuelle (XP/niveau, préférences), pas un
  marché à modérer.
- **Pas de guildes/persistance sociale au-delà du chat de salon** existant
  (Firebase) — une guilde demanderait sa propre modélisation de données et
  une UI dédiée, hors de portée immédiate.
- **Pas de PvP par défaut** (cf. §3.7) : coopératif d'abord.

---

## 5. Priorisation suggérée pour SPRINT_MMORPG.md

À traduire en sprints numérotés (80+) au moment de les attaquer, dans cet
ordre — chaque étape dépend de la précédente :

1. ~~**§3.1 Vie individualisée par joueur**~~ ✅ FAIT (Sprint 80)
2. ~~**§3.2 IA multi-cibles**~~ ✅ FAIT (Sprint 80)
3. **§3.4 Vie/identité affichées** 🟢 backend fait (Sprint 80) ; reste le
   panneau HUD lui-même — reporté le temps d'une collision d'édition
   concurrente sur `editor/mod.rs`/`gfx/renderer.rs` (Sprint 81, `time_scale`
   d'une autre session) ; sûr à reprendre une fois ce fichier stable.
4. **§3.5 Rôles légers** (non traité — mérite sa propre UI de sélection à la
   connexion, pas improvisée) ; ~~**§3.6 Soin coopératif**~~ ✅ FAIT (Sprint 80,
   en version universelle, sans gate de rôle — cf. sa section).
5. ~~**§3.3 Multi-salons**~~ ✅ FAIT (Sprint 82, backend + protocole ; sélection
   du salon dans l'UI reportée, même réserve que §3.4)
6. **§3.8 Progression qui dure** (indépendant, complète la boucle de retour)
7. **§3.7 PvP** — seulement si explicitement demandé

### Sprint 80 — Bilan

**Fait** : §3.1 (vie individualisée), §3.2 (IA multi-cibles), §3.6 (soin
coopératif universel), et le backend de §3.4 (vie/identité mémorisées côté
client, `multiplayer_roster()`). 10 nouveaux tests dans `src/app/health.rs`,
2 dans `src/app/fireball.rs`/`src/app/mod.rs` (209 tests au total, tous
verts), `cargo fmt`/`clippy -D warnings` propres.

**Reporté** : le panneau HUD du roster (§3.4, backend prêt), les rôles
(§3.5), le multi-salons (§3.3), la progression persistante (§3.8) et le PvP
(§3.7, sur demande seulement).

**Note opérationnelle** : une collision d'édition concurrente a été détectée
et gérée en cours de route (cf. `concurrent-sessions-hazard`, mémoire du
projet) — une autre session travaillait en parallèle sur
`src/editor/mod.rs`/`src/gfx/renderer.rs` (ajout de `time_scale`, Sprint 81).
Deux ajouts à `src/net/protocol.rs` (champ `heal`, variante `PlayerDown`) ont
été silencieusement écrasés une première fois et ont dû être refaits après
vérification (`grep` sur le fichier avant chaque étape suivante). Aucune
perte de travail au final, mais la prudence a ralenti ce sprint.

### Sprint 82 — Bilan

**Fait** : §3.3 (multi-salons). `src/bin/server.rs` a été volontairement
choisi pour ce sprint — aucun risque de collision avec la session parallèle,
qui travaillait sur `editor/mod.rs`/`gfx/renderer.rs`. `Room` (`AppState` +
état de salon) remplace le singleton précédent ; `ClientMsg::Join` gagne un
champ `lobby` (rétrocompatible : `NetClient::connect` envoie toujours
`DEFAULT_LOBBY`, seul `connect_to_lobby`, nouveau, en choisit un autre).
4 tests bout-en-bout (vrais sockets) : isolation entre deux salons distincts,
fermeture d'un salon vidé de tous ses joueurs. 220 tests lib + 4 tests bin,
tous verts.

**Reporté** : la sélection du salon dans l'UI (fenêtre Multijoueur) — même
réserve que §3.4, en attendant que `editor/mod.rs` soit stable côté session
parallèle.
