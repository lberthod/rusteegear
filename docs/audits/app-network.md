# `src/app/network_client.rs`, `src/app/multiplayer.rs`, `src/app/fireball.rs`

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans les fichiers.
Les trois modules sont groupés ici : gameplay réseau (client, salons
multijoueurs) et attaque à distance dépendent l'un de l'autre au tick près
(input réseau → tir, snapshot → réconciliation) et partagent les mêmes bugs
de fond (prédiction/réconciliation, désynchronisation client-serveur).

## Attribution par sprint

### `network_client.rs`

- **Sprint 54** — `net::interpolation::RemoteEntity` (affichage des fantômes
  par interpolation), écrit avant d'être réellement câblé à la réconciliation.
- **Sprint 57** — `connect_to_server` transmet le `firebase_uid` connu au
  `Join`, pour créditer la bonne progression côté serveur.
- **Sprint 58** — Chat (`request_send_chat_message`/`request_refresh_chat`),
  écriture réservée aux comptes Firebase authentifiés (règles RTDB).
- **Sprint 59** — Classement global (`request_refresh_leaderboard`), lecture
  publique.
- **Sprint 65** — Client réseau porté sur Android (pas encore iOS).
- **Sprint 66 / « Sprint 66bis »** — Réconciliation du joueur local
  (`apply_local_network_position`, `CORRECTION_PULL`) : première version puis
  correctif, cf. bugs réels ci-dessous.
- **Sprint 67** — `sample_delayed`/`interpolation::RENDER_DELAY` (léger délai
  d'affichage des fantômes pour absorber la gigue réseau).
- **Sprint 68** — Plafond d'envoi de l'input (`INPUT_SEND_INTERVAL`), aligné
  sur le tick serveur.
- **Sprint 88** — Animation répliquée (clip poussé dans `AnimationState` des
  fantômes/joueurs réseau, sans interpolation).

### `multiplayer.rs`

- **Sprint 50** — `combat.rs`, même principe d'isolation de la surface de
  gameplay que ce module reprend pour le réseau.
- **Sprint 55** — Création du module (salons multijoueurs, `SPRINT_MMORPG.md`).
- **Sprint 60** — Durcissement de l'input réseau (`sanitize_network_input`) et
  temps de recharge serveur pour l'attaque au contact
  (`update_network_attacks`, `NETWORK_ATTACK_COOLDOWN`).

### `fireball.rs`

- **Sprint 78** — Boule de feu d'origine (arme unique).
- **Sprint 79** — Multi-armes (`RANGED_WEAPONS`) et direction du tir réseau
  pilotée par `aim_yaw` (au lieu de l'orientation serveur, jamais mise à jour
  jusque-là pour un joueur réseau).
- **Sprint 80** — Joueurs réseau comme tireurs (validation serveur du temps de
  recharge), spectateur à 0 PV (GAMEDESIGN_EN_LIGNE.md §3.1).
- **Sprint 85 / 86** — Plafond de chasseurs actifs et portée de détection
  réseau pour `ai_chaser` — fonctionnalité depuis retirée de la carte
  multijoueur embarquée (cf. bug réel ci-dessous).

## Bugs réels trouvés en testant

- **Correction de réconciliation figée entre deux points (Sprint 66 →
  66bis)** : la première version de `apply_local_network_position` mémorisait
  `from`/`to` une seule fois puis affichait une interpolation entre ces deux
  points figés pendant 120 ms. Or `Physics::step` recopie la pose du corps
  rigide dans `transform.position` à *chaque* tick, avant que cette fonction
  ne s'exécute — la correction figée écrasait donc, frame après frame, la
  vraie position fraîchement avancée par l'input réel. Rapporté par un
  utilisateur comme un personnage qui semble dupliqué/tremblant entre deux
  points, ignorant l'input pendant toute la fenêtre de correction. Corrigé en
  ne faisant jamais qu'un petit pas (`CORRECTION_PULL`) depuis la position
  **fraîche** du tick courant vers la position autoritative, jamais une
  valeur mémorisée.

- **La correction ne survivait pas au tick physique suivant** : même cause
  profonde que le bug précédent, sous un angle différent — écrire la
  correction uniquement dans `transform.position` sans passer par
  `Physics::set_position` n'avait d'effet que pour la frame courante ;
  `Physics::step` l'écrasait dès le tick suivant avec la position du corps
  rigide, resté inchangé. `set_position` corrige en écrivant sur le corps
  rigide lui-même (cf. aussi `docs/audits/physics.md`, même mécanisme touché
  par un bug analogue côté physique pure).

- **Traction arrière continue en pleine course, comparaison à la position
  instantanée** : la position renvoyée par le serveur date toujours d'une
  latence + un tick — en pleine course, elle est presque systématiquement en
  retard par rapport à la prédiction locale, au-delà de `SNAP_THRESHOLD`.
  Comparer à la seule position *instantanée* du serveur déclenchait donc une
  correction en continu pendant tout déplacement : vitesse en dents de scie,
  tremblement visible à l'arrêt, constaté en comparant l'enregistrement
  vidéo image par image. Corrigé en gardant un historique des positions
  prédites récentes (`net_local_history`) : si la position serveur est proche
  d'un point où l'on est réellement passé, le serveur est juste en retard —
  rien à corriger.

- **Décalage permanent à l'arrêt entre deux clients** : sous `SNAP_THRESHOLD`,
  `reconcile` ne corrige volontairement rien — mais le serveur (physique plus
  ancienne, freinage plus mou) s'arrête systématiquement quelques dizaines de
  cm plus loin que la prédiction locale. Constaté en comparant deux écrans au
  même instant (un client macOS et un APK) : les deux joueurs à l'arrêt
  n'étaient pas aux mêmes positions relatives d'un appareil à l'autre. Ajout
  d'un rattrapage doux (`IDLE_SETTLE_PULL`) qui ne s'active qu'à l'arrêt
  (vitesse sous `IDLE_SPEED_EPSILON`), imperceptible en mouvement.

- **Le déplacement clavier W/S ne partait jamais au serveur** : `key_thrust`
  n'était pas inclus dans le calcul de la direction envoyée
  (`network_move_axes`) — le serveur ne voyait donc jamais ce mouvement, et la
  réconciliation finissait par annuler la prédiction locale après quelques
  secondes, donnant l'impression que le déplacement au clavier « buguait ».

- **Avance W simulée à l'envers côté serveur** : une fois `key_thrust` inclus,
  la composante `move_y` était calculée en convention Z **monde**
  (`-yaw.cos()`) alors que le serveur attend la convention **joystick**
  (`move_y` positif = avant, il applique lui-même `vz = -move_y × vitesse`).
  La prédiction locale partait donc en avant tandis que le serveur simulait un
  mouvement vers l'arrière ; dès que l'écart sortait de la trajectoire récente,
  la réconciliation tirait le joueur à contresens de son propre mouvement.

- **Boutons tactiles et gyroscope invisibles pour le serveur** : le message
  réseau n'envoyait que `inp.jump`/`inp.attack` (clavier) — sur APK, sauter ou
  attaquer via les boutons tactiles nommés (`Controller::jump_button`/
  `attack_button`) restait invisible côté serveur : le joueur sautait à
  l'écran (prédiction) mais jamais dans la simulation autoritaire, d'où des
  corrections/incohérences visibles en ligne. Même oubli pour le gyroscope.

- **Un salon vide vidait la vie du HUD avant même la connexion d'un joueur**
  (`multiplayer.rs`) : sans exclure explicitement les monstres (`ai_chaser`/
  `combat.attackable`) du repli « premier objet scripté visible » de
  `player_index`, un monstre était désigné « le joueur » dès que les monstres
  devenaient visibles — ils se déclenchaient alors entre eux et vidaient
  `hud_health` (partagé) en quelques secondes, avant qu'un vrai joueur n'ait
  eu le temps de se connecter. Corrigé par `hide_local_player_template`,
  appelée par le serveur headless avant le premier `Join`.

- **Un second `Join` du même client créait un fantôme physique orphelin**
  (`multiplayer.rs`) : rien dans le protocole n'empêche un client d'envoyer un
  second `ClientMsg::Join` (rejeu réseau, bug client, ou trame forgée) — avant
  le correctif, `spawn_network_player` en profitait pour cloner un second
  objet sans jamais nettoyer le premier, qui restait simulé indéfiniment par
  la physique sans plus être référencé par `network_players` (donc invisible
  du `Snapshot`). Rendu idempotent : un `id` déjà connu renvoie l'objet
  existant.

- **Points d'apparition en ligne droite, joueurs dispersés** (`multiplayer.rs`)
  : les premiers points d'apparition des joueurs réseau s'éloignaient de +5 m
  en ligne du gabarit par joueur supplémentaire — sur une carte de taille
  modeste, les joueurs se retrouvaient trop loin les uns des autres pour se
  voir ou s'entraider dès la connexion. Remplacé par un cercle serré
  (`SPAWN_RADIUS`) autour du gabarit d'origine.

- **Fantômes réseau jamais visibles malgré des positions correctement reçues**
  (`multiplayer.rs`) : `hide_local_player_template` masque le gabarit
  d'origine avant le premier join — sans reset explicite, chaque joueur
  réseau nouvellement spawné héritait de ce `visible=false`, et
  `network_snapshot` diffusait cette invisibilité telle quelle. Deux vrais
  clients connectés recevaient bien les positions l'un de l'autre, mais aucun
  fantôme ne s'affichait jamais. Corrigé en forçant `template.visible = true`
  à chaque spawn, indépendamment de l'état du gabarit d'origine.

- **Salon de longue durée : la scène grossit sans borne** (`multiplayer.rs`) :
  sans recyclage des emplacements laissés par les joueurs partis
  (`despawn_network_player` ne fait que masquer l'objet, jamais le retirer
  pour ne pas décaler les indices), chaque `Join` poussait un nouveau clone
  dans `scene.objects` pour toujours. Sur un salon de longue durée avec
  beaucoup de va-et-vient, la scène grossissait indéfiniment et chaque
  `Physics::build` (reconstruit à chaque join/leave) devenait de plus en plus
  coûteux — perçu en jeu comme des à-coups/blocages de mouvement. Corrigé en
  réutilisant en priorité un emplacement orphelin avant d'en pousser un
  nouveau.

- **Scène de test sans sol, faux échec de test** (`fireball.rs`) : le premier
  jet de `scene_with_monster_ahead` n'avait pas de sol — le joueur (corps
  dynamique) tombait dans le vide avant que les tirs suivants ne partent,
  sous les cibles. `the_standard_weapon_needs_three_hits_on_the_boss` n'a
  alors compté qu'un seul impact sur trois attendus. Corrigé en ajoutant un
  sol statique à la scène de test.

- **Monstres poursuivants perçus comme un bug par un joueur solo réel**
  (`fireball.rs`, carte multijoueur embarquée) : malgré le plafond de
  chasseurs actifs et la portée de détection réseau ajoutés pour rendre
  `ai_chaser` utilisable en ligne, un joueur solo qui voyait des monstres se
  mettre à le poursuivre l'a signalé comme un dysfonctionnement (« les
  monstres bougent, tout bug »). Revenu à des cibles statiques sur cette
  carte — la vie individualisée (§3.1) reste prête dès qu'un vrai danger
  mobile sera réintroduit avec le bon contexte de jeu.
