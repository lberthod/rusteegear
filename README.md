<div align="center">

# 🦀 RusteeGear

**Un moteur / éditeur de jeu 3D minimaliste « à la Unity », écrit _from scratch_ en Rust.**

winit · wgpu · egui — aucun moteur tiers.

![langage](https://img.shields.io/badge/Rust-1.95-orange?logo=rust)
![plateformes](https://img.shields.io/badge/macOS%20·%20Android%20·%20iOS-qui%20tournent-success?logo=apple)
![rendu](https://img.shields.io/badge/wgpu-Metal%20%7C%20Vulkan-blue)
![licence](https://img.shields.io/badge/licence-MIT-green)

**Tourne réellement sur les 3 plateformes** : éditeur complet sur macOS,
mode « player » tactile sur iPhone et Android.

### 🎮 [Essayer la démo dans le navigateur](https://lberthod.github.io/rusteegear/)

Aucune installation : WebGPU (Chrome/Edge récents), clavier (WASD + Espace/J/K/H),
connectée au **même serveur multijoueur** que le desktop/APK — tout le monde qui
ouvre ce lien atterrit dans la même partie. Doc API : [/doc/](https://lberthod.github.io/rusteegear/doc/motor3derust/).

</div>

---

## ✨ Vision

RusteeGear est un éditeur de jeu 3D léger et hackable. L'objectif n'est pas de
remplacer Unity, mais d'offrir une base **comprenable de bout en bout** : chaque
ligne du pipeline de rendu, de l'ECS-léger et de l'UI est écrite à la main, sans
boîte noire. Le projet est pensé pour grandir vers le **mobile (iOS / Android)**
grâce à la portabilité de `wgpu`.

---

## 🎯 Quel besoin RusteeGear adresse-t-il ?

Les moteurs grand public (Unity, Unreal, Godot) sont extraordinairement complets,
mais ce sont des **boîtes noires** : des millions de lignes, un runtime opaque, un
modèle de licence et de télémétrie que l'on subit, et une courbe d'apprentissage qui
porte sur *l'outil* plus que sur *les concepts*. Quand on veut **comprendre comment
un moteur fonctionne réellement** — comment un vertex part d'un `Vec<f32>` pour finir
en pixel à l'écran, comment un raycast sélectionne un objet, comment une boucle de
simulation reste stable — ces moteurs cachent justement ce qui est intéressant.

RusteeGear répond à un besoin précis :

- **Pédagogique & maîtrise totale.** Chaque étage du pipeline (fenêtre → événements →
  état → rendu GPU → UI) est écrit à la main, lisible en une après-midi, sans
  abstraction magique. C'est un moteur que l'on peut **tenir entièrement dans sa tête**.
- **Hackable et minimal.** ~32 000 lignes de Rust au total. Ajouter une primitive, un
  type de collider ou une variable de script se fait en quelques lignes, sans se battre
  contre un framework.
- **Portable par conception.** La logique (scène, caméra, entrées, picking) ne dépend
  **pas** du GPU ; seule la couche `gfx/` parle à `wgpu`. C'est ce découpage qui a permis
  de porter l'app sur **iOS et Android** sans réécrire le cœur.
- **Sans dépendance lourde ni runtime caché.** Pas de garbage collector, pas de moteur
  embarqué, pas de licence à négocier — un seul binaire natif.

Ce n'est **pas** un concurrent d'Unity. C'est un **socle compréhensible** pour
apprendre, prototyper et expérimenter le rendu temps réel et l'architecture moteur.

---

## 🦀 Pourquoi Rust ?

Un moteur de jeu cumule les contraintes que Rust adresse le mieux :

- **Performance native, prévisible.** Le rendu temps réel et la physique exigent un
  contrôle fin de la mémoire et zéro pause GC. Rust offre les performances du C/C++
  (pas de runtime, pas de ramasse-miettes, *zero-cost abstractions*) tout en restant
  expressif.
- **Sécurité mémoire sans coût à l'exécution.** Le *borrow checker* élimine à la
  compilation les classes de bugs qui hantent les moteurs C++ (use-after-free, data
  races, pointeurs pendants). Sur un système concurrent (rendu + chargements async +
  audio + physique), c'est décisif : ici, l'import glTF, le décodage audio et le
  chargement de scène tournent **sur des threads de fond** en toute sûreté, garantie
  par le type system (`Send`/`Sync`).
- **Un écosystème graphique de premier plan.** L'essentiel de la stack est écrit *en*
  Rust et de grande qualité : `wgpu` (abstraction GPU moderne : Metal / Vulkan / DX12 /
  WebGPU), `winit` (fenêtrage multiplateforme), `egui` (UI immédiate), `glam` (maths
  SIMD), `rapier3d` (physique), `kira` (audio). On bénéficie d'un alignement rare entre
  le langage et ses bibliothèques.
- **Portabilité réelle.** Un même cœur compile vers macOS, un `.so` Android (`cdylib` +
  `android_main`) et un binaire iOS — `cargo` et l'abstraction `wgpu` font le gros du
  travail.
- **Outillage moderne.** `cargo` (build, dépendances, tests), `clippy` (lints),
  `rustfmt` (format) et un système de modules clair rendent un projet de cette taille
  agréable à maintenir — et faciles à valider en CI.

En résumé : Rust permet d'écrire un moteur **bas niveau et performant** tout en gardant
la **fiabilité** et le **confort de développement** qu'on attendrait d'un langage de
plus haut niveau.

---

## ⚖️ From scratch sur Rust — et pas sur Bevy ?

[Bevy](https://bevyengine.org/) est l'excellent moteur de jeu de l'écosystème Rust :
ECS complet, ordonnanceur de systèmes, rendu PBR, plugins… Si l'objectif était de
**produire un jeu** le plus vite possible, Bevy (ou Godot, Fyrox) serait un choix
parfaitement légitime — et probablement supérieur.

Mais l'objectif de RusteeGear est exactement l'inverse : **comprendre et maîtriser le
moteur lui-même**. Or s'appuyer sur Bevy reviendrait à remplacer une boîte noire
(Unity) par une autre, certes en Rust. On hériterait de son ECS, de son ordonnanceur,
de son pipeline de rendu et de ses choix d'architecture — c'est-à-dire de précisément
ce que ce projet cherche à écrire à la main pour l'apprendre.

| Critère | RusteeGear (from scratch) | Bevy |
|---|---|---|
| **Objectif** | Comprendre/maîtriser un moteur | Produire des jeux efficacement |
| **Taille du cœur** | ~32 000 lignes, lisible d'un bout à l'autre | Très large, nombreux sous-systèmes |
| **Architecture** | Scène = `Vec<SceneObject>`, explicite | ECS complet + ordonnanceur de systèmes |
| **Rendu** | Pipeline `wgpu`/WGSL écrit à la main | Moteur de rendu intégré (PBR, etc.) |
| **Courbe d'apprentissage** | On apprend *les concepts* | On apprend *le framework* |
| **Contrôle** | Total (chaque ligne est à soi) | Cadré par les conventions du moteur |
| **Productivité jeu** | Faible (tout est à construire) | Élevée |
| **Boîte noire** | Aucune | Le moteur lui-même |

Concrètement, RusteeGear ne s'appuie **que** sur des briques *ciblées et
remplaçables* (`winit` pour la fenêtre, `wgpu` pour le GPU, `egui` pour l'UI,
`rapier3d`/`kira`/`mlua` pour le runtime) et **assemble lui-même** la boucle
d'événements, le pipeline de rendu, le picking, les gizmos, la sérialisation et le
mode Play. C'est ce qui rend la **comparaison pertinente** : on choisit la dépendance
pour *un problème précis et bien délimité*, jamais pour la structure générale du
moteur — qui, elle, reste l'objet même de l'apprentissage.

> En une phrase : **Bevy est un moteur ; RusteeGear est l'exercice consistant à en
> écrire un.** Les deux sont en Rust ; seul le second t'apprend ce qu'il y a dedans.

---

## 🎮 Fonctionnalités (disponibles aujourd'hui)

**Rendu**
- **Rendu 3D** temps réel via `wgpu`, shaders WGSL, depth buffer, **ombres** (shadow map + PCF).
- **Matériaux PBR** par objet (metallic / roughness / emissive) + spéculaire ; **textures** albédo.
- **Lumières** : directionnelle globale + ambiante, **lumières ponctuelles** et **spots** (cône) — jusqu'à 8.
- **Rendu instancié** (1 draw par lot mesh+texture) + **frustum culling** CPU + **culling/LOD des lumières** (les 8 plus proches de la caméra).
- **Chemin de rendu sans allocation par frame** (tampons réutilisés, plan de dessin par index, re-tri paresseux).
- **Caméra orbitale** ; présentation **vsync** + cadence adaptative (throttle CPU au repos).
- **Animation squelettale** : import glTF skinné, skinning GPU, **fondu enchaîné** entre
  clips (`obj.anim` pilotable en Lua), répliquée en multijoueur.
- **Ciel + brouillard** : dégradé horizon/zénith (suit l'orientation de la caméra) et
  brouillard exponentiel, réglables dans l'inspecteur de scène.

**Édition**
- **Primitives** cube / sphère / plan / cylindre / capsule / **terrain** **+ import glTF / GLB** (asynchrone).
- **Éditeur `egui`** : toolbar (Play/Pause/Stop) · hiérarchie · inspecteur · bandeau d'état.
- **Hiérarchie** : groupes (glisser-déposer), filtre, icônes & badges ; **renommage inline**.
- **Sélection** clic 3D / hiérarchie, **multi-sélection** ; **gizmos** translate/rotate/scale (**multi-objets**, pivot commun).
- **Agencement** : aligner / distribuer sur un axe, grouper / dégrouper.
- **Undo / Redo**, couper / copier / coller (Cmd+X/C/V), dupliquer (Cmd+D), tout sélectionner (Cmd+A).
- **Gestionnaire d'assets** (`asset://`, rassemblement + navigateur), **sérialisation** JSON.

**Runtime de jeu** (Play ▶ / Pause ⏸ / Stop ⏹, aperçu réinitialisable)
- **Physique** `rapier3d` (Statique / Dynamique) avec **collider explicite** (Auto/Box/Sphère/Capsule).
- **Audio** `kira` : son par objet, autoplay, **spatialisation** (volume selon distance), cache asynchrone,
  **bus musique/effets séparés + panning + streaming** (Sprint 104) et **randomisation pitch/volume** par déclenchement (Sprint 108).
- **Caméra de jeu** + **suivi** automatique de l'objet joueur.

**🎮 Mini-jeu jouable — _sans écrire une ligne de script_**
- **Personnage pilotable** (Input Receiver) : corps dynamique piloté au **joystick**, **collisions** avec le décor, rotations bloquées.
- **Saut** sur bouton tactile (gravité, retombe au sol), **vitesse** et **hauteur** réglables ; **caméra qui suit**.
- **Actions au tap** (sans Lua) : changer de couleur, **ramasser**, grandir, réapparaître au départ.
- **Boucle de jeu complète** : **collectibles** (gemmes tournantes) avec **score ⭐**, **chrono ⏱**, **« 🎉 Gagné en X.Xs ! »** ; **zones mortelles 💀** → **« Perdu ! »**.
- Démo prête à jouer (`Fichier → Démo contrôleur`) + scène **JSON pré-générée** ([assets/examples/demo_controleur.json](assets/examples/demo_controleur.json)).

**API de script Lua** (`mlua`, chunks compilés en cache)
```lua
-- Lecture/écriture par objet :
obj.x/y/z   obj.rx/ry/rz   obj.sx/sy/sz   obj.r/g/b
obj.tapped      -- touché au doigt cette frame
obj.triggered   -- le joueur est entré dans la zone (trigger)
obj.anim = "run"  -- change le clip joué (objets skinnés), fondu enchaîné automatique
-- Globales :
dt, time
input.jx, input.jy, input.btn.<nom>   -- joystick + boutons tactiles
tilt.x, tilt.y                         -- gyroscope (flèches sur desktop)
vibrate(ms)                            -- retour haptique
set_health(0..1)                       -- barre de vie du HUD
```

**Mobile**
- **Aperçu device** (cadre téléphone, portrait/paysage) façon simulateur tactile.
- **Contrôles tactiles** : joystick virtuel + boutons, **zones de déclenchement**, **barre de vie**.
- **macOS** (éditeur, `.dmg`), **Android** (`.apk`, identité surchargée), **iOS** — player tactile (resume).

**Manette** (`gilrs`, Sprint 110)
- **Stick gauche** → déplacement « tank », cumulé avec clavier/tactile ; **saut/attaque/tir/soin** au bouton.
- **Remapping** persisté, éditable dans **⚙ Paramètres › 🎮 Manette** (menu déroulant par action).

**HUD déclaratif** (Sprint 109)
- **Widgets** texte / image / jauge / bouton, **ancrés** dans la scène (`Scene::hud_widgets`), liés aux valeurs de jeu (vie, score, frags, manche).
- Édités dans le panneau **🧩 Widgets HUD**, sans code moteur ; un bouton cliqué émet un événement lisible en Lua (`on_event("hud:<action>")`).

**IA (DeepSeek)** — clé/modèle/température dans les Paramètres
- **Générer** ou **optimiser** un script Lua depuis une consigne ; **générer une scène** entière (remplacer/ajouter) ; historique des prompts.

**Outils** — Console (logs), Profiler FPS + mémoire + **GPU** (timestamp queries par passe, draw calls), **Contrôle qualité APK**, **Optimisation mobile** (réduction textures, limite de lumières), Diagnostic système, **journal de crash** (consultation/copie volontaire, aucun envoi automatique), **hot-reload** des assets retouchés en cours d'édition, **snap** position/rotation au gizmo (Ctrl pour inverser ponctuellement).

**Démos** — `Fichier → Démo mobile`, `Démo gameplay` (toute l'API scriptée) et **`Démo contrôleur`** (joueur jouable au joystick + saut + collisions, **sans script**).

---

## 🌐 Multijoueur en ligne (chantier en cours)

RusteeGear commence à devenir jouable **à plusieurs, en ligne**, sur le mode
manches « Call of Zombies » — un chantier séparé du moteur solo, suivi sprint
par sprint dans **[SPRINT_MMORPG.md](SPRINT_MMORPG.md)**. Trois décisions de
scope, prises dès le départ :

- **Échelle visée** : des salons de **2 à 16 joueurs**, pas un MMO à monde
  persistant (qui demanderait du sharding de zones et une infra bien plus
  lourde — hors de portée d'un projet solo).
- **Autorité serveur** : le gameplay (mouvement, manches, combat) est simulé
  par un **serveur de jeu Rust autoritaire**, jamais par les clients — anti-
  triche de base, chaque client ne fait qu'envoyer ses entrées et afficher
  l'état reçu.
- **Firebase Realtime Database en backend annexe seulement** (comptes,
  inventaire persistant, chat, classement — pas encore implémenté). Firebase
  RTDB n'a pas d'autorité serveur ni de SDK Rust natif : inadapté au transport
  temps réel du gameplay (position, coups), qui passe par un vrai serveur
  WebSocket.

### Comment ça marche

```
┌─────────────┐        WebSocket (bincode)        ┌──────────────────────────┐
│   Client     │ ───────────────────────────────▶  │   src/bin/server.rs      │
│ (RusteeGear, │  ClientMsg::Join / Input / Leave  │   (headless, sans GPU)   │
│  desktop)    │ ◀───────────────────────────────  │                          │
└─────────────┘   ServerMsg::Welcome / Snapshot /  │  AppState (la MÊME que   │
                   PlayerJoined / PlayerLeft /      │  l'éditeur desktop) +    │
                   Event                            │  app::multiplayer        │
                                                     └──────────────────────────┘
```

- **Le serveur est headless** (`src/bin/server.rs`) : il fait tourner une
  `AppState` — exactement le même moteur de simulation que l'éditeur desktop
  (`scene`, `runtime::physics`, `app::combat`) — mais sans fenêtre, sans GPU,
  sans `egui`/`winit`. C'est ce qui a motivé l'extraction du combat/manches
  (`app/combat.rs`, Sprint 50) hors du fichier principal : la même logique de
  jeu doit tourner *aussi bien* dans une fenêtre que dans ce binaire console.
- **Chaque joueur réseau = son propre objet piloté indépendamment**
  (`app::multiplayer::spawn_network_player`) : rejoindre clone l'objet
  « joueur » de la scène, et son `Input` (joystick/attaque/saut envoyé par le
  client) ne pilote que *cet* objet — le gameplay solo existant n'est pas
  affecté (aucune régression sur les tests existants).
- **Protocole compact** (`src/net/protocol.rs`) : messages sérialisés en
  `bincode`, pas JSON — un `Snapshot` (position/orientation/santé/visibilité)
  pour 20 entités tient dans ~540 octets, largement sous le budget réseau visé.
- **Mouvement lisse malgré la latence** (`src/net/interpolation.rs`) : les
  entités distantes sont interpolées légèrement dans le passé (`RENDER_DELAY`,
  robuste à la gigue), jamais téléportées à chaque tick réseau — et le joueur
  local est piloté en **prédiction immédiate**, réconciliée intelligemment
  avec le serveur (voir ci-dessous).
- **Attaque à distance + monstres sur la carte** (`src/app/fireball.rs`,
  Sprints 78-79) : touche **K** ou bouton tactile **« Feu »** (APK + aperçu
  desktop) ⇒ un projectile part **dans la direction que le joueur regarde**
  (l'orientation prédite part dans chaque `Input` — le serveur l'applique à
  l'objet et en fait la direction du tir), en ligne droite, et frappe le
  premier obstacle physique (un mur sert d'abri) ou monstre sur son chemin.
  **Trois armes** — Boule de feu (équilibrée), Éclair (rapide, portée
  courte), Boulet (lent, 3 dégâts) — via **1/2/3** au clavier ou le bouton
  tactile **« Arme »** (cycle), avec un HUD bas-centre qui affiche l'arme
  équipée et les raccourcis. La carte embarquée place 5 monstres à abattre
  (dont un « chef » à 3 PV, un seul coup de Boulet) ; le serveur autoritaire
  simule tirs et recharge (le spam d'un client modifié ne tire pas plus
  vite, l'indice d'arme est borné), diffuse monstres et projectiles (avec
  leur arme : couleur/taille fidèles sur tous les écrans) dans le `Snapshot`,
  et chaque mise à mort atteint tous les écrans (`GameEvent::Defeated` :
  son + flash immédiats).
- **Vie individualisée, monstres qui poursuivent vraiment, soin coopératif**
  (`src/app/health.rs`, Sprint 80 — voir [GAMEDESIGN_EN_LIGNE.md](GAMEDESIGN_EN_LIGNE.md)) :
  chaque joueur réseau a désormais sa propre vie (plus un champ unique par
  salon) — un joueur peut mourir (spectateur, objet masqué) sans mettre fin
  à la manche pour les autres, tant qu'il en reste un debout. Les monstres
  poursuivent le joueur réseau vivant le plus proche (avant ce sprint, ils
  ne visaient jamais que le premier arrivé). Touche **H** ou bouton tactile
  **« Soin »** : soigne en continu l'allié blessé le plus proche à portée,
  résolu côté serveur.
- **Multi-salons** (`src/bin/server.rs::Room`, Sprint 82) : un même serveur
  fait tourner plusieurs manches indépendantes en parallèle, choisies par un
  code de salon (`ClientMsg::Join::lobby`) — tous les clients actuels
  continuent de se retrouver dans le même salon partagé par défaut, aucune
  régression. Une manche décidée (victoire/défaite) ne coupe plus la
  connexion de tout le monde : seul son salon repart, les autres continuent.
- **Animation répliquée** (`EntityDelta::anim_clip`, Sprint 88) : le clip joué
  par un joueur ou un monstre réseau est répliqué (pas sa phase — chaque
  client avance déjà localement le temps de tout `AnimationState`, local ou
  distant) et poussé dans `AnimationState::set_clip()` sur les fantômes, avec
  le même fondu enchaîné qu'en solo.
- **Frags individualisés** : un compteur de monstres vaincus par joueur,
  diffusé à tous dans le `Snapshot` — brique de progression/compétition
  pensée pour un futur mode MMORPG (cf. GAMEDESIGN_EN_LIGNE.md).

### Un déplacement fluide, en solo comme en ligne (audit 2026-07-12/13)

Le déplacement a fait l'objet d'un audit complet, mesuré image par image sur
des captures vidéo réelles (VPS à ~200 ms de latence). Chaque correctif est
verrouillé par un test de régression :

- **Interpolation de rendu à pas fixe** (« fix your timestep ») : la
  simulation avance par pas fixes de 1/60 s, mais le rendu affiche un mélange
  des deux derniers pas pondéré par l'accumulateur — trajectoire continue à
  l'écran quel que soit le framerate (fini le « judder » 0 pas/2 pas par
  frame). Les téléportations claquent sans traînée, les écritures externes du
  transform sont respectées.
- **Game feel** (`runtime/physics.rs`) : freinage 2× plus fort que
  l'accélération (arrêt net, virages qui accrochent), autorité aérienne
  réduite à 35 % (arc de saut crédible), gravité de descente ×1,6 (saut vif,
  pas « lunaire »), rotation du personnage en amorti exponentiel indépendant
  du framerate, zone morte du joystick remappée (départ progressif).
- **Réconciliation par trajectoire récente** : la position renvoyée par le
  serveur date d'une latence — en pleine course elle est *toujours* ~1 m
  derrière la prédiction. La comparer à la position instantanée déclenchait
  une traction arrière permanente (rubber-banding filmé et mesuré : vitesse
  en dents de scie de 2 à 12 px/frame). Le client garde 1 s d'historique de
  sa trajectoire prédite : une position serveur *sur* cette trajectoire =
  simplement en retard, aucune correction ; *hors* trajectoire = vraie
  désynchronisation, corrigée par petits pas.
- **Rattrapage doux à l'arrêt** : sous le seuil de correction (0,5 m), un
  joueur immobile converge lentement (5 %/frame) vers la position serveur —
  tous les écrans (macOS, APK) affichent les mêmes positions au repos, sans
  toucher au ressenti en mouvement.
- **Mêmes entrées des deux côtés** : le `ClientMsg::Input` est construit à
  partir des sources exactes de la prédiction locale — clavier, **pavé
  tactile W/A/S/D** (contrôles tank, identiques au clavier), boutons
  tactiles nommés (Saut/Attaque) et gyroscope — avec la même convention
  d'axes (un bug de signe sur la poussée W/S envoyée au serveur faisait
  littéralement avancer le joueur *à l'envers* dans la simulation
  autoritaire).

### Essayer le serveur dès maintenant

```bash
cargo run --bin server              # écoute sur 127.0.0.1:7777, lance une manche
RUSTEEGEAR_SERVER_ADDR=0.0.0.0:9000 cargo run --bin server   # port/adresse au choix
```

Le binaire tourne en autonome et accepte de vraies connexions WebSocket —
validé par des tests d'intégration bout-en-bout (`cargo test`) qui ouvrent un
vrai socket local, **et en conditions réelles** : un serveur tourne en continu
sur un VPS (service systemd), et les builds « player » (desktop `--player` et
APK Android) s'y **connectent automatiquement** au lancement — deux appareils
se voient bouger, sauter et combattre en ligne. L'overlay Multijoueur permet
de se déconnecter ou de pointer vers un autre serveur (ex. localhost pendant
le développement). Historique sprint par sprint :
[SPRINT_MMORPG.md](SPRINT_MMORPG.md) puis [SPRINTNETWORK.md](SPRINTNETWORK.md)
(latence & qualité du déplacement en ligne).

### Limites connues (assumées, documentées dans le code)

- Pas de dégâts joueur-contre-joueur : la boule de feu traverse les autres
  joueurs (la vie est individualisée depuis le Sprint 80, mais le PvP reste
  un choix de design à part, cf. GAMEDESIGN_EN_LIGNE.md §3.7 — sur demande
  seulement).
- Pas de réanimation d'un allié à 0 PV : la mort est un statut de spectateur
  permanent pour le reste de la manche (décision assumée, Sprint 80).
- Pas de rôles/classes : tous les joueurs partagent le même profil (le soin
  coopératif est universel, pas réservé à une classe « Soutien »).
- Pas de sélection de salon dans l'UI : la fenêtre Multijoueur rejoint
  toujours le salon partagé par défaut, même si le serveur en gère plusieurs
  depuis le Sprint 82.

---

## 🗓️ Historique & avancement

| Phase | Sprints | État |
|---|---|---|
| **MVP** — moteur + éditeur + `.dmg` | 0 → 6 | ✅ |
| **A** — Fondations éditeur (refactor, gizmos, glTF, undo/dup) | 7 → 10 | ✅ |
| **B** — Runtime de jeu (Lua, physique, audio) + optimisations | 11 → 13 | ✅ |
| **C** — Portage mobile (Player, tactile, iOS, Android) | 14 → 17 | ✅ |
| **D** — App de dev & exports 1-clic (perf, panneau Export, config, presets, CI) | 18 → 23 | ✅ |
| **E** — Player complet & maturité (assets embarqués, multi-sélection, matériaux, resume) | 24 → 27 | 🟢 cœur |
| **F** — Validation, édition complète, **ombres & textures**, outils produit | 28 → 32 | 🟢 |
| **Rendu avancé & opti mobile** — PBR par objet, lumières/spots multiples, caméra de jeu, réduction textures | 33 → 35 | ✅ |
| Distribution signée (cœur) & IA/confort d'édition | 36 → 37 | 🟢 |
| **G** — Éditeur produit orienté Android (menus, Build Panel, menu Ajouter, composants mobiles, outils) | 38 → 42 | 🟢 |
| **H** — **Jouabilité mobile sans script** (contrôleur joueur, saut, collisions, actions au tap) & **perf rendu** | 43 → 44 | ✅ |
| **I** — Robustesse & découplage (pas fixe, init sans panic, tests + skip-rebuild) | 45 → 49 | 🟢 (48/49 mobile-only restants) |
| *(Multijoueur en ligne)* — salons, serveur autoritaire, Firebase annexe, latence, PvE réseau | 50 → 89 | 🟢 voir **[Multijoueur en ligne](#-multijoueur-en-ligne-chantier-en-cours)** |
| **K** — Filet de sécurité (golden tests rendu, temps maîtrisé — time scale/step, console dev, debug drawing) | 80 → 83 | ✅ |
| **L** — Animation squelettale (skinning glTF → blending → exposition Lua → réplication réseau) | 84 → 88 | ✅ |
| **M** — Image (ciel + brouillard, HDR/tone mapping, bloom, mipmaps + tangentes) | 89 → 92 | ✅ |
| **N** — Chaîne gameplay (événements, GUID d'assets, prefabs, API Lua de scène, sauvegarde, anim notifies) | 93 → 99 | 🟢 (94 cycle de vie/handles reporté ; 96 prefabs : UI éditeur restante) |
| **O** — Physique & feel (trimesh/convexe, CCD/couches, requêtes, 103a maintenabilité `app`/`editor`/`scene`, 103b character controller, 103c audit prédiction réseau) | 100 → 103c | ✅ |
| **P** — Audio (bus/panning/streaming, randomisation), HUD déclaratif, manettes + remapping, hot-reload, snapping + profiler GPU, crash log + rustdoc | 104 → 113 | ✅ (106/107 non numérotés, tampons non utilisés) |
| **Q** — Web, la vitrine (wasm32/WebGPU, assets & audio web, multijoueur navigateur) | 114 → 117 | ✅ (117 : reste à activer Pages une fois dans les réglages GitHub — non automatisable) |

> Récap propre + **logique des prochains sprints** : **[SPRINTS.md](SPRINTS.md)**.
> Détail sprint par sprint, **à jour en continu** : **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)**
> (c'est la source de vérité sur l'avancement — ce tableau n'en est qu'un résumé).
> Reprise du projet par un nouveau développeur : **[HANDOFF.md](HANDOFF.md)**.

### Plateformes — état honnête

| Cible | Livrable | Statut |
|---|---|---|
| **macOS** | `.dmg` (éditeur complet) | ✅ fonctionne — non signé (clic droit ▸ Ouvrir) |
| **Android** | `.apk` signé (arm64-v8a) | ✅ s'installe (`adb install`) et tourne en mode player |
| **iOS** | app signée, installée sur iPhone | ✅ tourne (scène animée + tactile) — signature développeur **personnelle** (pas App Store) |

L'**éditeur** (panneaux egui, gizmos, inspecteur) est **desktop**. Sur mobile, l'app
démarre en **mode player** : la scène jouable plein écran, caméra au doigt (1 doigt =
orbite, 2 doigts = zoom). iOS/Android ne sont pas signés pour une distribution store.

---

## 🚀 Démarrage rapide

```bash
cargo run                       # éditeur desktop
cargo run -- --player           # mode player (scène plein écran)
```

### Builds par plateforme
```bash
# macOS (.dmg) — cargo install cargo-bundle
./packaging/build_dmg.sh        # → target/release/bundle/dmg/RusteeGear.dmg

# Android (.apk) — NDK + cargo install cargo-apk (voir packaging/build_android.md)
./packaging/build_apk.sh        # → target/release/apk/motor3derust.apk

# iPhone branché — Xcode + brew install xcodegen (voir packaging/build_ios.md)
./packaging/install_ios_device.sh   # build + signature auto + install + lancement
```

> ⚠️ Aucune cible n'est signée pour distribution store. Le `.dmg` n'est pas signé
> (clic droit ▸ Ouvrir) ; l'`.apk` est signé clé debug ; l'iOS utilise votre certificat
> de développement personnel (installe sur un appareil enregistré).

### Commandes dans l'éditeur

| Action | Commande |
|---|---|
| Tourner la caméra | clic gauche + glisser (sur la vue 3D) |
| Zoomer | molette |
| Sélectionner un objet | clic sur l'objet, ou dans la hiérarchie |
| Ajouter un objet | boutons Cube / Sphère / Plan |
| Éditer / supprimer | panneau Inspecteur (droite) |
| Lancer / arrêter l'animation | ▶ Play / ⏹ Stop |
| Sauver / charger | 💾 Save / 📂 Load (`~/motor3derust_scene.json`) |

---

## 🎨 Créer son premier jeu

Vous ne codez pas et voulez juste construire un niveau jouable ? Le guide
**[docs/guide-createur/index.md](docs/guide-createur/index.md)** vous emmène pas
à pas, boutons et cases à cocher uniquement : créer une scène → ajouter un
objet pilotable → HUD (barre de vie, joystick) → export en `.apk` jouable sur
votre téléphone. Aucune ligne de code, aucun jargon Rust.

(La section « Pourquoi Rust ? » plus haut et l'architecture ci-dessous
s'adressent plutôt à quelqu'un qui veut comprendre/modifier le moteur
lui-même.)

---

## 🧱 Architecture

```
src/
├── lib.rs         # event loop winit + run() (desktop) + android_main (cdylib) + resume mobile
├── main.rs        # entrée desktop → motor3derust::run()
├── bin/server.rs  # serveur de jeu headless (multijoueur) — sans gfx/egui/winit
├── assets.rs      # assets embarqués (include_dir, schéma bundle://) pour le player exporté
├── app/           # logique sans GPU : AppState, picking, sélection, build_config
│   ├── combat.rs      # attaque, manches (extrait pour être réutilisé par le serveur)
│   └── multiplayer.rs # un joueur réseau = un objet piloté indépendamment
├── gfx/           # couche rendu wgpu (renderer, mesh, camera, gizmo, shaders WGSL)
├── scene/         # Transform, MeshKind, Scene, groupes, lumière, import glTF, sérialisation
├── runtime/       # mode Play : physics (rapier3d), audio (kira)
├── net/           # multijoueur : protocol (bincode), server_loop/client (WebSocket,
│                  # desktop only), interpolation (lissage malgré la latence)
└── editor/        # UI egui (toolbar, hiérarchie, inspecteur, panneau export) — desktop
```

Séparation nette **logique (`app`) / rendu (`gfx`)** : l'état (scène, caméra, entrées)
ne dépend pas du GPU, ce qui a rendu le portage mobile direct **et** permet à
`src/bin/server.rs` de réutiliser exactement la même simulation de jeu en
headless (aucune duplication de logique entre l'éditeur desktop et le
serveur). Le rendu repose sur `wgpu` (Metal / Vulkan / DX12 / WebGPU) — la clé
de la portabilité.
Détails et journal : **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)** (moteur),
**[SPRINT_MMORPG.md](SPRINT_MMORPG.md)** (multijoueur).

---

## 🧭 La suite — analyse & sprints

Le projet a été construit par **sprints incrémentaux**, un commit par étape validée.
L'historique propre et la **logique des prochains sprints** vivent dans :

- **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)** — **source de vérité**, à jour en continu :
  détail par sprint (objectif · tâches · fichiers · livrable), pour le moteur solo
  **et** le multijoueur (numérotation indépendante, cf. la section dédiée dans ce fichier).
- **[SPRINTS.md](SPRINTS.md)** — récap historique des sprints 0→44 (Phases A→H), figé.
- **[SPRINT_MMORPG.md](SPRINT_MMORPG.md)** / **[SPRINTNETWORK.md](SPRINTNETWORK.md)** —
  chantier **multijoueur en ligne** en détail, cf.
  **[Multijoueur en ligne](#-multijoueur-en-ligne-chantier-en-cours)** plus haut.
- **[HANDOFF.md](HANDOFF.md)** — reprise du projet par un nouveau développeur.

**Terminé — Phase P, audio/HUD/confort** (détail dans [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)) :
audio bus/panning/streaming (104) et randomisation pitch/volume (108),
maintenabilité `app`/tests système + `docs/architecture.md` (105a-1/2/3), widgets
de HUD déclaratifs sérialisés dans la scène (109), manettes + remapping via
`gilrs` (110), hot-reload assets (`notify`) + Lua (111), snapping éditeur
position/rotation + profiler GPU (112), journal de crash volontaire + `cargo doc`
publiable sur GitHub Pages (113), build wasm32/WebGPU — sol, joueur et
overlay tactile s'affichent dans Chrome, vérifié par lecture de pixels réels
(114), assets embarqués + audio (`kira`, SFX) fonctionnels sur le web —
démo contrôleur jouable au clavier dans le navigateur (115), multijoueur
navigateur — `web_sys::WebSocket` natif, vérifié en conditions réelles
contre le vrai serveur de production (116), page de démo + doc API
déployées en un seul site GitHub Pages (117, cf. le lien tout en haut de ce
README). **Phase Q terminée** — reste juste l'activation de Pages côté
GitHub (Settings → Pages → Source = *GitHub Actions*, un geste manuel qui ne
s'automatise pas depuis un fichier de workflow). Limitations connues : le
scripting Lua reste inerte sur wasm32, la musique/ambiance en flux n'est pas
encore portée sur le web (`kira::sound::streaming` exclut ce target), et les
meshes à animation squelettale ne s'affichent pas (limite de bind groups
WebGPU) — détail dans [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md). Puis R
(WebXR).

---

## 🛠️ Stack technique

| Besoin | Crate |
|---|---|
| Fenêtre / événements | `winit` |
| Rendu GPU | `wgpu` (WGSL) |
| Maths | `glam` |
| UI éditeur | `egui` + `egui-wgpu` + `egui-winit` |
| Sérialisation | `serde` + `serde_json` |
| Import 3D | `gltf` |
| Scripting | `mlua` (Lua 5.4) |
| Physique | `rapier3d` |
| Audio | `kira` |
| Assets embarqués (player) | `include_dir` |
| Sélecteur de fichiers (desktop) | `rfd` |
| Réseau multijoueur (desktop only) | `tokio` + `tokio-tungstenite` (WebSocket) + `bincode` (protocole) |
| Packaging | `cargo-bundle` (macOS) · `cargo-apk` (Android) · `xcodegen`+Xcode (iOS) |

> Export depuis l'éditeur : voir **[packaging/EXPORT.md](packaging/EXPORT.md)**.

---

## 📄 Licence

MIT — voir [LICENSE](LICENSE). Fais-en ce que tu veux. 🦀
