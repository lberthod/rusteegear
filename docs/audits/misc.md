# Fichiers divers (`server.rs`, `assets.rs`, `savegame.rs`, `mesh.rs`, `lib.rs`, `log_buffer.rs`)

Historique déplacé hors du code (Sprint 103a-3) pour six petits fichiers sans
lien entre eux — regroupés ici plutôt qu'un fichier quasi vide par module (cf.
`docs/audits/README.md`, choix de regroupement laissé au jugement).

## Attribution par sprint

- **`src/bin/server.rs`**
  - Sprints 51-55 (`SPRINT_MMORPG.md`) — serveur headless : réutilise
    `scene`/`runtime`/`app::combat`/`app::multiplayer` sans fenêtre ni GPU,
    connexions WebSocket via `net::server_loop`.
  - Sprint 57 — progression Firebase optionnelle (`award_progress`,
    `post_leaderboard`), activée par 4 variables d'environnement.
  - Sprint 60 — éviction des clients silencieux (`evict_timed_out_players`).
  - Sprint 61 — mesure de charge à 16 joueurs (marge CPU/réseau large même à
    20 Hz), qui a servi à justifier la hausse de cadence ci-dessous.
  - Sprint 82 (`GAMEDESIGN_EN_LIGNE.md` §3.3) — multi-salons (`Room`/`Lobby`),
    une manche décidée ne coupe plus tout le process.

- **`src/assets.rs`**
  - Sprint 24 — assets embarqués dans le binaire (`bundle://`).
  - Sprint 95 — références stables `asset-id://<uuid>` + manifeste
    (`register_asset`/`resolve_asset_id`/`rename_asset`), pour survivre à un
    renommage de fichier. `is_known_scheme` centralise à cette occasion un
    test `starts_with(SCHEME) || starts_with(ASSET_SCHEME)` jusque-là dupliqué
    à 4 endroits (import glTF, audio, dimensions de texture, collecte
    d'assets) — chacun aurait dû être mis à jour séparément pour reconnaître
    `asset-id://` sans ce point de passage unique.
  - Sprint 98 — schéma `user://` pour les sauvegardes de partie, distinct des
    assets de projet.

- **`src/runtime/savegame.rs`**
  - Sprint 98 — sauvegarde de partie (positions, score, variables Lua),
    persistée par slot sous `user://`.
  - Sprint 95 — `SaveGame::version`/`CURRENT_VERSION` alignés sur le
    versionnement déjà en place pour les scènes.
  - Sprint 80 (`ROADMAP_SPRINTS.md`) — `rand`/`thread_rng` explicitement
    écarté du moteur ; la description d'origine du Sprint 98 prévoyait un
    champ `seed`, jamais ajouté puisqu'aucun générateur aléatoire seedable
    n'existe encore à sauvegarder.

- **`src/gfx/mesh.rs`**
  - Sprint 86 — `SkinnedVertex`/`SkinnedMeshData`/`GpuMesh::new_skinned` :
    format de vertex séparé pour les meshes skinnés (joints/poids), afin de ne
    pas alourdir les meshes statiques.

- **`src/log_buffer.rs`**
  - Sprint 32 — logger « tee » (délègue à `env_logger` et conserve les
    dernières lignes dans un tampon circulaire pour la Console de l'éditeur).

## Bugs réels trouvés en testant

- **Cadence réseau du serveur trop basse (`server.rs`, `SERVER_TICK`)** : à
  20 Hz, chaque fantôme distant n'avait une position fraîche que toutes les
  50 ms, et `RemoteEntity::sample` interpole *entre* les deux derniers
  snapshots reçus — donc affichait toujours un état vieux d'au moins un tick,
  en plus du round-trip réseau réel. Constaté en test réel : latence perçue
  trop grande sur le mouvement des autres joueurs. Relevée à 50 Hz, puis
  alignée sur 60 Hz (la cadence de la physique elle-même, `FIXED_DT` dans
  `AppState::advance_play`) une fois le Sprint 61 ayant mesuré une large marge
  à 16 joueurs même à 20 Hz (30 threads OS, aucune limite CPU/réseau
  atteinte) — 60 Hz reste trivial à l'échelle testée (2 joueurs).

- **Timeout client trop court (`server.rs`, `CLIENT_TIMEOUT`)** : à 10 s, un
  client légitime se faisait éjecter dès qu'il perdait le focus quelques
  secondes. Cause : le rendu desktop (`winit`/macOS) ralentit ou suspend
  `advance_play` — donc l'envoi d'`Input` — quand la fenêtre n'est plus au
  premier plan/est occultée (App Nap), et Android fait de même en
  arrière-plan ; aucune des deux apps ne détecte sa propre éviction, donc
  c'était silencieux côté client. Relevé à 60 s (constaté en test réel).

- **Fin de manche coupait tout le serveur (`server.rs`, avant le Sprint 82)** :
  une manche décidée (victoire/défaite) arrêtait tout le *process* (systemd
  le relançait, mais coupait au passage la connexion de tout le monde, y
  compris d'autres joueurs sans rapport avec cette manche-là). Corrigé en
  isolant chaque salon dans sa propre `AppState` (`Room::restart`), pour que
  seul le salon concerné reparte.

- **Axe de déplacement WASD qui retombait à zéro (`lib.rs`,
  `axis_from_held`)** : l'ancien code assignait directement `v` (0.0 ou 1.0
  selon pressé/relâché) à l'axe pour la seule touche qui venait de changer,
  sans tenir compte de l'autre touche du même axe. Conséquence concrète :
  tenir A (gauche, axe=-1), appuyer D (droite, axe=+1) pendant que A est
  encore enfoncée, puis relâcher D — l'axe retombait à 0 (D relâchée écrit
  `v=0.0` sans condition) au lieu de revenir à -1 (A pourtant toujours
  enfoncée). Rendait les changements de direction rapides (fréquents en jeu)
  imprécis/saccadés. Corrigé en recalculant l'axe à partir de l'état actuel
  des **deux** touches à chaque changement.

- **Toucher qui faisait orbiter la caméra au lieu de déplacer le personnage
  (`lib.rs`, gestion tactile)** : sur l'APK, un appui immobile sur un bouton
  de la croix directionnelle (contrairement au joystick, elle ne génère
  quasiment aucun `TouchPhase::Moved` une fois le doigt posé) pouvait laisser
  l'état « survolé/enfoncé » d'egui en retard d'une frame — le toucher passait
  alors jusqu'à l'orbite caméra, qui bougeait la vue au lieu d'agir sur le
  contrôle. Corrigé par une garde explicite : en Player avec des contrôles
  tactiles actifs, un doigt n'orbite jamais la caméra, même si `consumed`
  (egui) ne l'a pas repéré cette frame précise.
