# `src/app/` — sous-modules divers d'`AppState`

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans chaque
fichier. Couvre les petits sous-modules d'`AppState` extraits de `app/mod.rs`
au Sprint 103a-1 : `build_config.rs`, `persistence.rs`, `debug_draw.rs`,
`console.rs`, `settings.rs`, `picking.rs`, `demos.rs`, `combat.rs`,
`asset_ops.rs`, `selection.rs`.

## Attribution par sprint

- **Sprint 20** — `build_config.rs` : config de build/export persistée (nom
  d'app, bundle id, version, numéro de build).
- **Sprint 33** — `settings.rs` : clé API DeepSeek pour la génération de
  scripts Lua par IA.
- **Sprint 39** — `build_config.rs` : options Android (orientation,
  min/target SDK, icône, splash) et rendu mobile (qualité, FPS cible, ombres,
  MSAA).
- **Sprint 50 / 51+** (`SPRINT_MMORPG.md`) — `combat.rs` : extraction du
  système de combat hors de `app/mod.rs`, en vue d'un futur serveur de jeu
  réseau qui devra le piloter en autorité.
- **Sprint 56** (`SPRINT_MMORPG.md`) — `settings.rs` : config Firebase pour
  les comptes multijoueur.
- **Sprint 65** — `demos.rs` : démo « MMORPG », dédiée au test multijoueur
  PC ↔ mobile.
- **Sprint 81** — `console.rs` : pas fixe de simulation à la demande (bouton
  « ⏭ » de la toolbar), même en pause.
- **Sprint 82** — `console.rs` : console développeur (`timescale`, `pause`,
  `tp`, `net_stats`, …).
- **Sprint 83** — `debug_draw.rs` (segments/box/sphere de debug) ;
  `picking.rs` (visualisation du rayon de picking envoyé).
- **Sprints 84-85** — `persistence.rs` : squelette/clips d'animation à
  l'import glTF.
- **Sprint 91** — `build_config.rs` : option bloom (`RenderQuality::
  bloom_enabled`, `BuildConfig::bloom`), avec opt-out automatique sur la
  qualité « Basse ».
- **Sprint 92** — `persistence.rs` : tangentes à l'import glTF.
- **Sprint 93** — `persistence.rs` : événement `score:N` émis par valeur
  traversée plutôt que par valeur finale.
- **Sprint 95** — `asset_ops.rs` : résolution `asset-id://<uuid>` avant de
  nommer la copie de texture optimisée.
- **Sprint 98** — `persistence.rs` : sauvegarde de partie (`SaveGame`,
  `capture_save`/`apply_save`).
- **Sprint 103a-1** — extraction de tous ces fichiers hors de `app/mod.rs`
  en sous-modules dédiés.

## Bugs réels trouvés en testant

- **Missile de combat, garantie de risque partielle** (`combat.rs`,
  `ATTACK_PROJECTILE_SPEED`) : l'attaque du joueur était à l'origine une
  résolution instantanée au moment du tir — maintenir le bouton défaisait
  n'importe quelle cible en portée sans le moindre risque, ce qui rendait le
  mode manches sans tension. Un premier correctif a ajouté un temps de
  recharge (`Controller::attack_cooldown`) pour empêcher le spam. La demande
  suivante (« comme des missiles ») a transformé l'attaque en missile homing
  qui vise la position courante de la cible et ne résout l'impact qu'à
  l'arrivée. En testant si ceci fermait aussi la faille du risque garanti en
  1 contre 1, la vérification a été honnête : non — un missile homing tiré
  dès l'entrée en portée arrive presque toujours avant qu'un poursuivant
  n'ait atteint sa propre portée de morsure, sauf à rendre le missile
  déraisonnablement lent. Le missile reste donc une vraie amélioration de
  lisibilité (le coup se voit voyager), mais ne remplace pas le levier de
  risque déjà identifié ; un vrai correctif demanderait un temps de
  préparation avant même le départ du projectile (ce qui a été ajouté plus
  tard, cf. `AttackCharge`/`Controller::attack_windup`). Détail dans
  `audit_sprint.md` §3-4.

- **Caméra de suivi : angle vertical libre instable** (`picking.rs`,
  `InputEvent::PointerMove`) : la rotation caméra pilotait à la fois le yaw
  et le pitch depuis le glisser souris. Constaté en jeu réel : un angle de
  plongée libre faisait basculer le sol/l'horizon au moindre geste,
  cassant le repère visuel attendu d'une caméra de suivi façon Zelda.
  Corrigé en figeant le pitch (contrôlé uniquement par le zoom/pinch,
  `InputEvent::Scroll`) et en ne laissant la souris piloter que la
  rotation horizontale.

- **`restart_game` et les objets ajoutés en cours de partie** (cf.
  AUDIT_MMORPG.md §4.2) : `play_snapshot` (utilisé pour restaurer la scène
  au redémarrage) ne connaît pas les objets ajoutés dynamiquement pendant la
  partie — joueurs réseau (`spawn_network_player`) et boules de feu. Sans
  nettoyage explicite, `network_players` et le pool de boules de feu
  pointaient vers des indices devenus obsolètes après restauration. Corrigé
  en vidant `network_players` (`clear_network_players`) et le pool de
  boules de feu (`clear_fireballs`) avant de reconstruire la physique.
