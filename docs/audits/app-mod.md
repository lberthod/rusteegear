# `src/app/mod.rs`

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans le fichier.
`src/app/mod.rs` est le cœur de l'orchestration (`AppState`, `advance_play`/
`sim_step`, la boucle à pas fixe) ; les fichiers siblings issus du split du
Sprint 103a-1 (`selection.rs`, `picking.rs`, `persistence.rs`, `demos.rs`,
`asset_ops.rs`, `console.rs`, `debug_draw.rs`) ont leur propre historique.

## Attribution par sprint

- **Sprint 45** — Simulation à **pas fixe** découplée du rendu (`FIXED_DT`,
  `fixed_substeps`) : physique/scripts déterministes, indépendants du FPS.
- **Sprint 54** — Réconciliation réseau du joueur local contre
  `interpolation::SNAP_THRESHOLD` (`net_local_interp`).
- **Sprint 55** — Joueurs réseau connectés (`network_players`, cf.
  `multiplayer.rs`).
- **Sprint 57** — `firebase_uid` : crédite la progression au bon compte côté serveur.
- **Sprint 60** — `network_attack_cooldowns` (cf. `multiplayer::update_network_attacks`).
- **Sprint 65** — `net_client` : multijoueur sur desktop + Android (dépend de
  `tokio`, pas encore ciblé iOS).
- **Sprint 66bis** — `obj.triggered` (zones de déclenchement scriptées).
- **Sprint 68** — `net_last_input_sent` : plafonne le débit d'envoi d'`Input`
  à `network_client::INPUT_SEND_INTERVAL`.
- **Sprint 79** — Orientation des joueurs réseau pilotée par l'`aim_yaw` de
  leur `Input` plutôt que par un recalcul local.
- **Sprint 81** — `time_scale` (ralenti/accéléré) et `step_requested` (pas
  unique en pause, bouton « ⏭ »).
- **Sprint 83** — Debug drawing : `debug_lines`, `debug.line(...)` côté Lua,
  `DebugView` (Éclairé/Normales/Profondeur).
- **Sprint 85** — Bouclage des clips d'animation (`Clip::sample_joint`).
- **Sprint 87** — Lecture des clips squelettaux, fondu enchaîné entre clips,
  exposition Lua (`obj.anim`).
- **Sprint 91** — `bloom_enabled` (`build_config::BuildConfig::bloom`).
- **Sprint 93** — Événements de gameplay (`emit`/`on_event`, `game_events`).
- **Sprint 94** — Refactor à handles générationnels pour les objets de scène :
  **pas fait** — `obj:destroy()`/`spawn()` restent volontairement conservateurs
  (suppression douce, ajout en fin de tableau) tant que ce refactor n'existe pas.
- **Sprint 96** — Convention de nom de prefab unique (`unique_prefab_name`),
  réutilisée par les tests de ce fichier.
- **Sprint 97** — API Lua `obj:destroy()`, `spawn(prefab, x, y, z)`,
  `find_tag(tag)`, et vérification que les coroutines Lua standard fonctionnent
  sans câblage supplémentaire.
- **Sprint 98** — `save.get`/`save.set` (variables de script persistantes,
  `lua_vars`), intégré à `runtime::savegame::SaveGame`.
- **Sprint 99** — Marqueurs temporels d'animation (`anim:<nom>`), fenêtres de
  hit d'attaque pilotées par ces marqueurs.
- **Sprint 102** — `obj.exited` (sortie de zone), `raycast`/`overlap_sphere`
  exposés en Lua (capteur de sol, cône de vision).

## Bugs réels trouvés en testant

- **Réconciliation réseau qui freinait et faisait trembler le joueur en pleine
  course** : comparer la position confirmée par le serveur (en retard d'une
  latence aller-retour + un tick) à la position **prédite instantanée**
  déclarait le joueur « désynchronisé » dès qu'il bougeait (écart ≈ vitesse ×
  latence, ≈ 1 m au-delà du seuil à 4,5 m/s sur le VPS réel) — d'où une
  correction continue visible en vidéo. Corrigé en validant la position
  serveur contre la **trajectoire récente** du joueur (`net_local_history`) :
  si le point serveur est proche d'un point par lequel on est réellement
  passé, on est en phase (le serveur est juste en retard), pas de correction.

- **Dédoublement visuel du joueur réseau local** : appliquer la correction de
  position réseau *avant* le pas de physique se faisait aussitôt écraser par
  `sim_step`, qui recalculait une position légèrement différente à partir de
  l'ancienne — d'où un dédoublement visible. Corrigé en appliquant
  `apply_local_network_position` *après* la physique de la frame.

- **Réticule qui ne suivait pas l'orientation du joueur** : pour un personnage
  équipé d'une arme à distance, la caméra ne pivotait jamais derrière le
  joueur en tank (A/D) — seule la souris (absente au tactile) faisait
  tourner la caméra. Le réticule (fixe au centre de l'écran) pointait donc
  la direction de VUE, pas la direction de TIR (`aim_yaw`). Corrigé en
  faisant pivoter la caméra vers `aim_yaw` pour les personnages à arme à
  distance, en laissant les autres démos (joystick, plateformes)
  intentionnellement inchangées (caméra libre voulue, pas un défaut).

- **Vibrations en tournant un corps rigide en contact avec le décor** :
  imposer la rotation d'un personnage à chaque frame via
  `RigidBody::set_rotation` déstabilisait le solveur de contacts de rapier
  dès qu'on combinait beaucoup de rotation et de déplacement (mur, pilier).
  Corrigé en appliquant l'orientation directement sur `transform.rotation`
  **après** `phys.step()`, jamais sur le corps rigide — sans perte
  fonctionnelle, le collider (capsule) est de toute façon symétrique autour
  de l'axe Y.

- **Tenir S (recul « tank ») faisait tourner le personnage sur lui-même à
  180°** : le vecteur de vitesse pointant vers l'arrière était utilisé pour
  faire « faire face » le joueur à sa direction de déplacement, recalculé
  chaque frame à partir du nouveau cap — l'orientation partait en spirale.
  Corrigé en gardant l'orientation fixe pendant W/S (avance/recul « tank »),
  contrairement au reste du pilotage.

- **Déplacement en diagonale ~41 % plus rapide qu'en ligne droite** :
  `(mx, my)` était clampé axe par axe (`clamp(-1.0, 1.0)`) — en diagonale
  (ex. W+D), le vecteur `(1.0, 1.0)` a une longueur √2. Corrigé en bornant la
  **longueur** du vecteur combiné plutôt que chaque composante
  (`clamp_move_vector`).

- **« Cran » perceptible en sortant du rayon mort du joystick** : sans
  remappage, l'entrée sautait d'un coup de 0 à `threshold` en sortant de la
  zone morte. Corrigé en remappant `[threshold, 1]` vers `[0, 1]`
  (`apply_deadzone`), pour un départ continu depuis zéro.

- **Rotation « tank » manuelle réutilisant `Controller::turn_speed`** : ce
  taux (10 rad/s) est calibré comme un taux de *rattrapage* amorti
  (rapide au départ, doux à l'approche) pour l'orientation automatique — tenu
  en continu comme vitesse brute, il aurait fait tourner le personnage à
  ~570°/s, impossible à doser. Corrigé avec une constante dédiée
  (`MANUAL_TURN_SPEED`, 3 rad/s).

- **Plusieurs monstres convergeant tous en même temps sur l'unique joueur
  visible** : sans plafond, 4-5 monstres pouvaient acculer un joueur solo
  contre un mur en quelques secondes, sans fenêtre de riposte. Corrigé avec
  `MAX_ACTIVE_CHASERS_PER_TARGET` (2) : au-delà, les chasseurs en surnombre
  restent en place, remplacés dès qu'un des premiers meurt ou s'éloigne.
  Même avec ce plafond, un joueur **réseau** solo restait ciblé par
  *tous* les monstres de la carte au bout d'assez de temps (le plafond étale
  l'arrivée, il ne l'empêche pas) : ajout de `CHASER_DETECT_RANGE`,
  volontairement limité au cas réseau pour ne pas casser le ring-out des
  démos solo (`Scene::brawl_demo`, où un chasseur repoussé doit toujours
  revenir vers le joueur, même hors de cette portée).

- **Monstres qui se « déclenchaient » entre eux et vidaient la vie partagée
  sans joueur connecté** : sur un serveur headless, `player_index()` pouvait
  désigner un monstre (`ai_chaser`) comme « le joueur » dès qu'aucun objet
  pilotable n'était visible (avant qu'un joueur réseau ne rejoigne). Corrigé
  en excluant explicitement les monstres et les cibles de combat
  (`combat.attackable`) du repli sur « premier objet scripté » (cf.
  AUDIT_MMORPG.md).

- **Repli sur « le premier objet de la scène » comme joueur par défaut** :
  désignait parfois un décor statique (le sol) comme « le joueur » — son AABB,
  souvent immense, chevauchait alors tous les monstres et déclenchait leurs
  scripts de dégâts en même temps. Retiré : `player_index()` renvoie `None`
  plutôt que de désigner un objet au hasard (cf. AUDIT_MMORPG.md).

- **Déplacement incohérent dès que la caméra pivotait** : le joystick
  avançait toujours selon les mêmes axes du monde, indépendamment de
  l'orientation de la caméra. Corrigé avec `camera_relative_move` (façon
  caméra de suivi à la Zelda : « en haut » sur le joystick éloigne toujours
  le personnage de la caméra).

- **Caméra de suivi trop en retrait** : l'angle de plongée et le recul par
  défaut (`DEFAULT_CHASE_PITCH`/`DEFAULT_CHASE_DISTANCE`) ont été resserrés
  pour un cadrage plus proche d'un jeu d'action à la troisième personne,
  plutôt que le recul plus « isométrique » d'avant (~35°, recul 11 m).
