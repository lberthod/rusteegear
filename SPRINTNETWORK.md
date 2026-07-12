# RusteeGear — Sprints réseau : latence & qualité du mode en ligne

Suite directe de `AUDIT_LATENCE_MULTIJOUEUR.md` (2026-07-12) : chaque sprint
ci-dessous correspond à un point du plan d'action priorisé de cet audit (§3),
dans le même ordre. Continue la numérotation de `SPRINT_MMORPG.md` (dernier
sprint livré : 65). Même format que ce document (Objectif / tâches / fichiers
/ livrable / risques), même discipline : un correctif = un test de régression
qui prouve le comportement, pas seulement « compile et tourne ».

## Suivi rapide

- [x] Sprint 66 — Lissage de la réconciliation du joueur local ✅ FAIT
- [x] Sprint 67 — Délai d'interpolation pour les fantômes distants ✅ FAIT
- [x] Sprint 68 — Plafonnement du débit d'`Input` client ✅ FAIT
- [ ] Sprint 69 — Vérification géographique du serveur de test (infra, pas code) — 🔜 nécessite un accès réel au VPS de test, non fait ici
- [x] Sprint 70 — Cohérence doc/code du `Snapshot` (option A : commentaire) ✅ FAIT
- [ ] Sprint 71 — Transport non-TCP (conditionnel, à ne lancer que si 66-68 ne suffisent pas)
- [x] Sprint 72 — Interpolation de rendu à pas fixe (fluidité visuelle) ✅ FAIT
- [x] Sprint 73 — Game feel du déplacement (freinage, air control, chute, rotation) ✅ FAIT
- [x] Sprint 74 — Réconciliation par trajectoire récente (fin du rubber-banding) ✅ FAIT
- [x] Sprint 75 — Convention d'axes de la poussée W/S client → serveur ✅ FAIT
- [x] Sprint 76 — Boutons tactiles/gyro dans l'Input réseau + pavé W/A/S/D ✅ FAIT
- [x] Sprint 77 — Rattrapage doux à l'arrêt + serveur VPS aligné sur le même code ✅ FAIT

---

### Sprint 66 — Lissage de la réconciliation du joueur local ✅ FAIT (révisé)
**Objectif** : remplacer la correction dure (« snap » instantané) du joueur
local par un lissage, pour supprimer l'artefact de téléportation visible dans
les conditions déjà mesurées comme dégradées (150-250 ms de RTT réel vers le
VPS, cf. `AUDIT_LATENCE_MULTIJOUEUR.md` §2.3).
- [x] **Version 1 (régression réelle, remplacée)** : une structure
  `NetworkCorrection { from, to, started }` faisait glisser (`Vec3::lerp`) la
  position entre deux points **figés** sur `CORRECTION_DURATION` (120 ms).
  **Bug trouvé en testant réellement l'app (2026-07-12, capture d'écran
  utilisateur : personnage bloqué/tremblant entre deux points)** :
  `Physics::step` (`runtime/physics.rs:207`) recopie la pose du corps rigide
  dans `transform.position` à **chaque** tick, avant `apply_local_network_
  position` — sync à sens unique physique → transform, jamais l'inverse.
  Toute correction figée sur `from`/`to` écrasait donc, frame après frame
  pendant toute la fenêtre de 120 ms, la vraie position fraîchement calculée
  à partir de l'input réel : le joueur semblait bloqué/trembler entre deux
  points, l'input étant purement ignoré pendant la correction.
- [x] **Version 2 (retenue)** : `NetworkCorrection`/`net_correction` supprimés.
  `apply_local_network_position` ne fait plus qu'un petit pas
  (`CORRECTION_PULL = 0.15`, `Vec3::lerp`) depuis la position **fraîche** de
  ce tick vers `server_pos`, recalculé à chaque appel — jamais une valeur
  mémorisée d'un appel précédent. Le mouvement piloté par l'input n'est donc
  plus jamais interrompu.
- [x] Tests de régression : `a_single_call_only_takes_a_small_step_toward_
  the_authoritative_position` (pas de saut en un appel),
  `repeated_calls_gradually_converge_toward_the_authoritative_position`
  (converge jusqu'à `SNAP_THRESHOLD`, pas au-delà — la correction s'arrête
  volontairement sous ce seuil), et surtout
  `a_correction_never_discards_local_movement_that_happened_between_calls`
  — verrouille précisément le premier bug trouvé : simule un mouvement local
  entre deux appels et vérifie qu'il n'est jamais écrasé par la correction.
- [x] **Second bug réel trouvé (mêmes captures d'écran, persistant après la
  v2 ci-dessus)** : même avec le petit pas par appel, la position affichée
  oscillait indéfiniment entre deux valeurs fixes sans jamais converger
  (log de diagnostic : `pos=(0.0,…) → (0.45,…)`, puis retour exact à
  `(0.0,…)` la frame suivante, en boucle). Cause : `Physics::step`
  (`runtime/physics.rs:207`) recopie la pose du corps rigide dans
  `transform.position` à *chaque* tick (sync à sens unique physique →
  transform, jamais l'inverse) — écrire uniquement dans `transform.position`
  n'a donc d'effet que pour la frame courante ; le tick physique suivant
  l'efface avant que la correction n'ait eu la moindre chance de persister.
  **Corrigé par une nouvelle méthode `Physics::set_position(index, pos)`**
  (`runtime/physics.rs`) qui écrit directement dans le corps rigide via
  `body.set_translation`, appelée depuis `apply_local_network_position` en
  plus de la mise à jour de `transform.position`. Testé
  (`a_local_position_correction_survives_the_next_physics_step` : simule un
  vrai tick physique après la correction et vérifie qu'elle n'est pas
  effacée). Vérifié en conditions réelles : la correction converge
  maintenant véritablement (`0.0 → 0.45 → 0.83 → 1.15 → …` au lieu d'osciller
  indéfiniment entre deux points).
- **Fichiers** : `src/app/network_client.rs`, `src/app/mod.rs` (champ
  `net_correction` retiré), `src/runtime/physics.rs` (nouvelle méthode
  `set_position`).
- **Livrable** : 149 tests lib verts, clippy/fmt propres (fichiers touchés).
  Déployé et vérifié en conditions réelles (VPS + app desktop + APK), y
  compris un diagnostic par log confirmant la convergence réelle.
- **Leçon** : comme pour §4.5 de `AUDIT_MMORPG.md`, ces deux bugs n'étaient
  visibles ni en lecture de code ni en tests unitaires isolés (chaque test
  passait à chaque étape) — seuls des tests bout-en-bout **réels**, avec un
  vrai personnage déplacé à la main puis un log de diagnostic en conditions
  réelles, les ont révélés. La cause racine du second (sync à sens unique
  physique → transform) n'était documentée nulle part avant cet incident.

### Sprint 67 — Délai d'interpolation pour les fantômes distants ✅ FAIT
**Objectif** : supprimer les gels/saccades des joueurs distants quand un
snapshot arrive en retard (gigue réseau), en interpolant systématiquement un
peu dans le passé plutôt qu'entre les deux derniers snapshots reçus
(`AUDIT_LATENCE_MULTIJOUEUR.md` §2.4).
- [x] `RemoteEntity` (`src/net/interpolation.rs`) : l'historique borné à 2
  entrées (`prev`/`latest`) est remplacé par un `VecDeque` borné à
  `HISTORY_CAPACITY` (6 snapshots) — assez pour retrouver les deux
  échantillons qui encadrent un instant `now - RENDER_DELAY`.
- [x] Constante `RENDER_DELAY` = 100 ms (`net::interpolation`). Nouvelle
  méthode `sample_delayed(now)` = `sample(now - RENDER_DELAY)` — `sample`
  elle-même reste inchangée dans sa signature (rétrocompatible avec les 7
  tests déjà existants, tous verts sans modification).
- [x] `poll_network` (`src/app/network_client.rs`) appelle désormais
  `rp.interp.sample_delayed(now)` pour les fantômes distants. `net_local_
  interp.sample(now)` (réconciliation du joueur local) reste sur `sample`
  **sans** délai — délayer la référence autoritative locale aurait fait
  dériver systématiquement la position prédite (toujours plus avancée) et
  déclenché des corrections inutiles, cf. commentaire de `sample_delayed`.
- [x] Comportement aux limites conservé : `sample_clamps_before_the_first_
  and_after_the_last_snapshot` toujours vert sans changement avec le nouvel
  historique.
- [x] Nouveaux tests : `sample_delayed_keeps_interpolating_smoothly_when_the_
  latest_packet_is_late` (preuve directe du gain : `sample(now)` se fige sur
  le dernier état connu quand le paquet est en retard, `sample_delayed(now)`
  continue d'interpoler) et `history_never_grows_past_its_capacity`.
- **Fichiers** : `src/net/interpolation.rs`, `src/app/network_client.rs`
  (point d'appel).
- **Livrable** : suite de tests étendue (+2 tests), clippy/fmt propres.
- **Risques** : `RENDER_DELAY` (100 ms) choisi par défaut cohérent avec le
  tick serveur (16 ms) mais non calibré en conditions réseau réelles (VPS) —
  à ajuster empiriquement si des saccades subsistent malgré ce sprint.

### Sprint 68 — Plafonnement du débit d'`Input` client ✅ FAIT
**Objectif** : ne plus envoyer un `ClientMsg::Input` à chaque frame de rendu
(débit non borné, calé sur le framerate d'affichage) mais à une fréquence
fixe alignée sur le tick serveur — hygiène réseau, aucun gain de latence
mais évite un gaspillage qui grandira avec le nombre de joueurs
(`AUDIT_LATENCE_MULTIJOUEUR.md` §2.2).
- [x] `poll_network` (`src/app/network_client.rs`) : nouveau champ `AppState::
  net_last_input_sent: Option<Instant>` — l'envoi n'a lieu que si
  `INPUT_SEND_INTERVAL` (16 ms, alignée sur `SERVER_TICK`,
  `src/bin/server.rs`) s'est écoulée depuis le dernier envoi.
- [x] Aucune régression sur la réactivité du joueur local : seul l'envoi
  réseau est throttlé, `sim_step` (prédiction) continue de tourner à chaque
  frame sans changement.
- [x] Test : `input_send_rate_is_capped_regardless_of_poll_network_call_rate`
  — 500 appels à `poll_network` en boucle serrée (sans dormir entre eux)
  produisent moins de 50 `Input` reçus côté serveur, pas 500.
- **Fichiers** : `src/app/network_client.rs`, `src/app/mod.rs` (nouveau champ).
- **Livrable** : 148 tests lib verts, clippy/fmt propres.
- **Risques levés** : aucun — changement isolé, pas de nouveau champ de
  protocole, pas de changement de comportement serveur.

### Sprint 69 — Vérification géographique du serveur de test (infra, pas code)
**Objectif** : établir si les 150-250 ms de RTT mesurés (`network_client.
rs:163`) viennent d'une distance géographique client-serveur plutôt que d'un
problème applicatif, avant d'investir davantage dans le netcode
(`AUDIT_LATENCE_MULTIJOUEUR.md` §2.6). Sprint sans code : mesure et décision.
- [ ] `ping`/`traceroute` vers l'IP du VPS de test depuis le poste utilisé
  pour les tests réels, comparé à la latence attendue pour la distance
  physique réelle (ex. table de latence RTT intercontinentale de référence).
- [ ] Si la latence mesurée est cohérente avec une distance géographique
  importante : documenter la conclusion (pas un bug logiciel) et évaluer,
  hors scope de ce sprint, si une région de déploiement plus proche du public
  visé est nécessaire à terme.
- [ ] Si la latence mesurée est **incohérente** avec la distance (trop
  élevée pour la distance réelle) : ouvrir une investigation dédiée
  (congestion, MTU, proxy intermédiaire...) — nouveau sprint à définir selon
  ce qui est trouvé.
- **Fichiers** : aucun (mesure), sauf mise à jour du commentaire
  `network_client.rs:163` avec la conclusion trouvée.
- **Livrable attendu** : chiffres mesurés publiés dans ce document ou dans
  `AUDIT_LATENCE_MULTIJOUEUR.md`, pas estimés — même discipline que la
  mesure de bande passante du Sprint 61 (`SPRINT_MMORPG.md`).
- **Risques** : dépend de l'accès réel au VPS de test et à des postes clients
  à des emplacements différents ; sans ça, ce sprint reste bloqué en l'état.

### Sprint 70 — Cohérence doc/code du `Snapshot` (option A retenue) ✅ FAIT
**Objectif** : lever la divergence entre la documentation de `Snapshot`
(« delta d'état... depuis le dernier snapshot envoyé à *ce* client »,
`src/net/protocol.rs:57-60`) et le code réel (`AppState::network_snapshot`,
`src/app/multiplayer.rs:258-276`, qui diffuse l'état complet de tous les
joueurs à chaque tick, identique pour tous les clients). Pas urgent tant que
N ≤ 16 et que les monstres/décor ne sont pas diffusés
(`AUDIT_LATENCE_MULTIJOUEUR.md` §2.1) — sprint de cohérence, pas de correctif
de latence.
- [x] **Option A retenue** (aucun changement à l'échelle actuelle, ni aux
  entités diffusées ni au nombre de joueurs visé, ne justifiait pas l'option B) :
  commentaire de `Snapshot` (`src/net/protocol.rs`) corrigé pour refléter la
  réalité — état complet des joueurs réseau à chaque tick, pas un delta par
  client — avec renvoi explicite vers la mesure du Sprint 61
  (`SPRINT_MMORPG.md` : ~368 octets/16 joueurs) et vers ce sprint pour
  l'option B si le besoin apparaît plus tard.
- [ ] Option B (vrai delta par client) : non retenue ce sprint, reste
  disponible si le nombre d'entités diffusées grandit significativement
  (cf. le commentaire mis à jour dans `protocol.rs`).
- **Fichiers** : `src/net/protocol.rs`.
- **Livrable** : commentaire cohérent avec le code, `cargo build`/tests
  inchangés (pas de code affecté), clippy/fmt propres.
- **Risques** : aucun — changement de documentation uniquement.

### Sprint 71 — Transport non-TCP (conditionnel)
**Objectif** : uniquement si les sprints 66-68 ne suffisent pas et qu'une
perte de paquets réelle est mesurée en conditions de jeu — remplacer ou
compléter le transport WebSocket/TCP actuel (tête de ligne bloquante sous
perte de paquets, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.5) par un transport qui ne
bloque pas les messages suivants en cas de perte d'un paquet (UDP, QUIC,
canaux de données WebRTC non fiables).
- [ ] **Préalable obligatoire** : mesurer un taux de perte de paquets réel en
  conditions de jeu (pas simulé) avant d'entamer ce sprint — sans cette
  mesure, ce chantier de transport entier serait injustifié à l'échelle
  visée (2-16 joueurs).
- [ ] Si justifié : évaluer le coût de migration (nouveau transport à
  implémenter à la fois client et serveur, cf. `src/net/server_loop.rs` et
  `src/net/client.rs`, tous deux construits autour de
  `tokio-tungstenite`/WebSocket) contre le gain réellement mesuré.
- [ ] Décision explicite à documenter avant tout code : ce sprint peut très
  bien conclure « pas justifié » sans qu'aucune ligne de transport ne change,
  comme le Sprint 61 a conclu qu'aucune optimisation de bande passante
  n'était nécessaire à cette échelle (`SPRINT_MMORPG.md`).
- **Fichiers** : potentiellement `src/net/server_loop.rs`, `src/net/client.rs`
  (nouveau transport), `Cargo.toml` (nouvelle dépendance) — uniquement si la
  mesure préalable justifie le chantier.
- **Livrable attendu** : soit une mesure documentée concluant que ce n'est
  pas nécessaire, soit un nouveau transport testé bout-en-bout (comme
  `client_joins_and_server_receives_its_input`, `src/net/server_loop.rs`).
- **Risques** : le plus gros chantier de cette liste — architecture de
  transport entière, à ne lancer que sur preuve mesurée, jamais par
  anticipation.

---

## Sprints 72-77 — Audit qualité du déplacement en ligne (2026-07-12 → 13)

Série issue de tests **réels** : captures vidéo analysées image par image
(traçage de la position du joueur sur 180 frames consécutives), captures
d'écran comparées au même instant sur deux appareils (macOS vs APK), le tout
contre le VPS réel (~200 ms de RTT). Chaque correctif est verrouillé par des
tests de régression ; commits `0aa0b5d` → `1f00598`.

### Sprint 72 — Interpolation de rendu à pas fixe ✅ FAIT (`0aa0b5d`)
**Symptôme** : déplacement « pas fluide » même à haut FPS. **Cause** : la
simulation avance par pas fixes de 1/60 s mais le rendu affichait la dernière
pose brute — selon l'alignement frame/tick, une frame montre 0 ou 2 pas de
simulation (« judder »). **Correctif** : le rendu affiche un mélange des deux
derniers pas pondéré par l'accumulateur (`blend_render_poses`), l'état exact
étant restauré avant chaque nouveau pas (`restore_sim_poses`). Les
téléportations (respawn, ancre FX) claquent sans traînée
(`TELEPORT_SNAP_PER_STEP`), les écritures externes du transform
(réconciliation réseau, tests) survivent à la restauration, les fantômes
réseau gardent leur interpolation serveur dédiée. Bonus : zone morte du
joystick remappée `[seuil,1]→[0,1]` (départ progressif) et lissage caméra en
`1-e^(-k·dt)` (indépendant du framerate).
**Fichiers** : `src/app/mod.rs`, `src/app/network_client.rs`.

### Sprint 73 — Game feel du déplacement ✅ FAIT (`e7695fe`)
Constantes documentées dans `runtime/physics.rs` : `BRAKE_FACTOR = 2.0`
(freinage 2× plus fort que l'accélération — arrêt net, fini l'effet
« savonnette »), `AIR_CONTROL = 0.35` (l'arc d'un saut s'engage à
l'impulsion), `FALL_GRAVITY_FACTOR = 1.6` (retombe plus vite qu'on ne monte,
hauteur de saut inchangée). Rotation du personnage en amorti exponentiel
(`rotate_towards_smooth`, indépendant du framerate) au lieu de vitesse
constante + arrêt sec ; rotation tank manuelle A/D à vitesse dédiée
(`MANUAL_TURN_SPEED = 3 rad/s` — `turn_speed` à 10 rad/s est un taux de
rattrapage, pas une vitesse tenue). Tests mesurant chaque effet.
**Fichiers** : `src/runtime/physics.rs`, `src/app/mod.rs`.

### Sprint 74 — Réconciliation par trajectoire récente ✅ FAIT (`718fb1d`)
**Symptôme (vidéo, mesuré)** : vitesse en dents de scie (2 à 12 px/frame à
entrée constante), dérive + tremblement ~1,5 s après chaque arrêt. **Cause** :
la position serveur date d'une latence + un tick — à 4,5 m/s elle est
*toujours* ~1 m derrière la prédiction, au-delà de `SNAP_THRESHOLD` (0,5 m) ;
la comparaison à la position instantanée déclenchait une traction arrière de
15 % à chaque frame pendant tout déplacement. **Correctif** : historique 1 s
des positions prédites (`net_local_history`) ; une position serveur proche
d'un point de la trajectoire récente = « en phase, juste en retard » → aucune
correction. Hors trajectoire (téléport, perte de paquets, triche) → corrigée
par petits pas comme avant.
**Fichiers** : `src/app/network_client.rs`, `src/app/mod.rs`.

### Sprint 75 — Convention d'axes de la poussée W/S ✅ FAIT (`04c0cc6`)
**Symptôme (jeu réel)** : en W/S, le déplacement part droit puis « repart
dans une autre direction » après quelques mètres. **Cause** : bug de signe —
`network_move_axes` envoyait la composante W/S en Z *monde*
(`-cos(yaw)`) alors que le serveur attend la convention *joystick*
(`move_y` positif = avant, il applique `vz = -move_y × vitesse`). Le serveur
simulait W inversé en Z ; dès que sa trajectoire divergeait au-delà du seuil
hors trajectoire récente, la réconciliation (Sprint 74) tirait le joueur à
contresens. Le test unitaire existant verrouillait la mauvaise valeur.
**Correctif** : `move_y = thrust × cos(yaw)` + test de bout en bout : pour
plusieurs yaw, la vitesse reconstruite par la convention serveur doit être
identique à la prédiction locale. Purement client — compatible avec un
serveur ancien.
**Fichiers** : `src/app/network_client.rs`.

### Sprint 76 — Boutons tactiles/gyro dans l'Input réseau + pavé W/A/S/D ✅ FAIT (`62cf640`, `619b5a6`)
Même famille que le Sprint 75 : le `ClientMsg::Input` ne partait que du
clavier. Trois sources utilisées par la prédiction locale restaient
invisibles pour le serveur : bouton tactile « Saut »
(`Controller::jump_button` via `input.buttons`), bouton « Attaque », et
l'inclinaison gyroscope. Le message est désormais construit par
`network_input_msg()` à partir des sources exactes de `sim_step`. Et la
croix directionnelle tactile devient un **pavé tank W/A/S/D** (lettres ASCII
— les glyphes ▲▼ manquaient de la fonte egui sur Android, carrés vides
constatés sur APK réel) : mêmes contrôles que le clavier, via des canaux
dédiés `touch_thrust`/`touch_turn` cumulés au clavier
(`PlayerInput::thrust()`/`turn()`) sans écrasement mutuel.
**Fichiers** : `src/app/network_client.rs`, `src/app/mod.rs`,
`src/editor/mod.rs`, `src/scene/mod.rs`.

### Sprint 77 — Rattrapage doux à l'arrêt + serveur aligné ✅ FAIT (`1f00598`)
**Symptôme (deux captures au même instant, macOS vs APK)** : à l'arrêt, les
positions relatives des joueurs diffèrent d'un appareil à l'autre.
**Cause** : `reconcile` ignore volontairement tout écart < 0,5 m, mais le
serveur (physique plus ancienne au moment du constat) s'arrêtait quelques
dizaines de cm plus loin que la prédiction — décalage permanent entre « où je
me vois » et « où les autres me voient ». **Correctif** : joueur immobile
(nouvelle méthode `Physics::velocity`, vitesse < 0,15 m/s) + écart > 3 cm ⇒
convergence douce (5 %/frame) vers la position serveur, uniquement à l'arrêt.
**Infra** : sources synchronisées sur le VPS, serveur recompilé sur place et
service `rusteegear-server` redémarré — serveur et clients tournent désormais
sur le même commit (mêmes physique et conventions des deux côtés, les
corrections deviennent marginales).
**Fichiers** : `src/app/network_client.rs`, `src/runtime/physics.rs`.
