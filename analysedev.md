# Analyse de développement — RusteeGear / « Le Hameau des Braises »

> Audit holistique du dépôt réalisé le 19 juillet 2026 : game design, level design, UX/HUD, inventaire/progression, architecture logicielle, pipeline 3D/direction artistique, documentation de sprint, code source, scènes/assets, état git, tests/CI, réseau. Photographie de l'état réel du projet à date, à mettre à jour si le contexte évolue significativement.

## Sommaire

1. [Vision du jeu (GDD)](#1-vision-du-jeu-gdd_mmorpgmd)
2. [Le moteur](#2-le-moteur-readmemd)
3. [Level design / cartes / monde](#3-level-design--cartes--monde)
4. [UX / ergonomie / HUD](#4-ux--ergonomie--hud)
5. [Inventaire / progression / économie](#5-inventaire--progression--économie-in-game)
6. [Architecture logicielle](#6-architecture-logicielle)
7. [Pipeline 3D / direction artistique](#7-pipeline-3d--direction-artistique)
8. [Documentation de sprint](#8-documentation-de-sprint--où-en-est-le-travail-produit)
9. [Structure du code](#9-structure-du-code-68-494-lignes-de-rust-dans-src)
10. [Scènes et assets — inventaire chiffré](#10-scènes-et-assets--inventaire-chiffré)
11. [État git / travaux en cours](#11-état-git--travaux-en-cours)
12. [Tests et CI](#12-tests-et-ci)
13. [Réseau / multijoueur](#13-réseau--multijoueur)
14. [Synthèse — écarts GDD ↔ code](#14-synthèse--écarts-gdd--code-les-plus-significatifs)
15. [Radar de maturité par dimension](#15-radar-de-maturité-par-dimension)

---

## 1. Vision du jeu (GDD_MMORPG.md)

**Pitch** : coop d'action en ligne, 2-16 joueurs (cible 3-4), vue 3e personne, sessions de 10-20 min (« nuits »). Personnage persistant (compte Firebase), salon jetable — **pas** de monde persistant partagé (verrouillé jusqu'au Sprint 50).

**Fiction** : le Hameau (village fortifié = arène de jeu), les Braises (feu qui attire les hordes), la Ménagerie (~45 créatures — attirées par le feu, pas « le mal » par nature).

**Boucles de jeu** :
- Seconde → combat
- Manche → 10-20 min, victoire/défaite de salon (la défaite paie aussi en XP)
- Soirée → 2-4 nuits
- Retour quotidien → contrat du jour, seed = jour UTC

**Modes de manche** :
- Vagues (« La Horde ») — ✅ en jeu, seul mode réellement branché en réseau
- Survie / Boss / Escorte — codés et testés côté logique (Phase C) mais **aucun sélecteur réseau** : `src/net/client/native.rs` envoie toujours `objective: 0` en dur

**Combat** : attaque de base à windup + missile homing courte portée, 3 armes à distance en triangle soutenu/précision/burst (Boule de feu / Éclair nerveux / Boulet burst), soin universel (touche H, 0,2 PV/s), Soin Soutien ×2,5. Économie de vie calibrée explicitement (contact −0,16/s, régen +0,05/s, soin +0,20/s).

**Créatures** : IA `AiChaser`, plafond 2 chasseresses/cible, rayon d'éveil 9 m, grammaire d'archétypes (Traqueuse / Meute / Colosse / Furtive) implémentée au code (`src/scene/mod.rs:537-571`, `src/app/simulation.rs`) **mais pas encore reflétée dans le casting réel de la scène servie**, qui reste peuplée par des patrouilles décoratives.

**Personnage joueur** : cible = fée riggée `fairy_hero`, mais **l'avatar actuellement servi est une sphère placeholder** depuis le commit `119a295` (17 juillet 2026), en attendant réintégration.

**Progression (vue GDD)** : 3 classes (Assaut / Éclaireur / Soutien), paliers nommés débloquant des options (jamais de puissance brute) — niv.3 Éclair, niv.6 Boulet, niv.10 2e emplacement classe. Barème XP recalibré (§8.3) après un bug historique où l'ancien barème rendait la progression quasi impossible (~100 nuits pour le 1er palier) : participation 150, frag/assist 5, victoire 75, contrat 250. Voir §5 pour la confrontation à l'implémentation réelle.

**Direction artistique (vue GDD)** : règle froid = décor / chaud-saturé = enjeu, une teinte par système (orange = joueur, cyan = mobilité, magenta = menace, vert = soin, violet = objectifs). Voir §7 pour le détail technique du pipeline.

**Exclusions fermes (§12)** : pas de monde persistant partagé, pas d'économie joueur-joueur, pas de guildes, pas de PvP par défaut, pas d'objets à stats/battle pass, pas de matchmaking par niveau. **Confirmées par le code** (§5) : aucune trace de boutique/craft/cosmétique.

**Tensions de design non résolues (§13, volontairement ouvertes)** :
- Le Soutien sera-t-il réellement joué ?
- Rayon d'éveil adapté aux grandes cartes ?
- Le contrat du jour suffit-il comme seul levier de rétention ?
- Viabilité du mode 16 joueurs ?
- L'Éclair est-il strictement meilleur que les autres armes ?
- Les vagues comme donnée de scène (risque de régression silencieuse)
- Scène servie vs scène vitrine qui divergent — « substantiellement résolue » Phase B (18 juillet 2026), réserve : fonctions de resynchro encore `#[ignore]`/manuelles

**Vertical slice visé (§18.6, « Nuit de référence »)** : scène unifiée, 3 vagues + 1 chef évolué, 3 classes jouables, roster + vie à l'écran, boutons tactiles réparés, feedback dégâts, écran de fin.

**État global (tableau §14)** : la quasi-totalité des systèmes cités sont ✅ en jeu (vie individuelle, IA, armes, multi-salons, XP/classement, décor + ménagerie dans la scène servie, vagues réelles à PV croissants, roster HUD, réanimation, assists, archétypes, chat/mute, feedback dégâts, audio partiel).

## 2. Le moteur (README.md)

RusteeGear est un moteur/éditeur 3D **écrit from scratch** en Rust (winit/wgpu/egui), ~32 000 lignes de cœur moteur, volontairement sans moteur tiers type Bevy (position argumentée dans le README, tableau comparatif dédié).

**Fonctionnalités moteur** : rendu wgpu (PBR, ombres, skinning GPU, ciel/brouillard), éditeur egui complet, physique rapier3d, audio kira, scripting Lua (mlua), HUD déclaratif, génération de scènes/scripts assistée par IA (DeepSeek), manettes (gilrs), export multi-plateforme (macOS .dmg, Android .apk, iOS, WebGPU wasm32).

**Pipeline d'assets** : 100 % procédural — **422 fichiers .glb** (README annonce 412, delta dû aux 10 nouvelles créatures en cours) générés par ~60 scripts Blender headless, aucun asset téléchargé. Détail complet en §7.

**Multijoueur** : serveur autoritaire Rust headless (`src/bin/server.rs`), protocole bincode compact, Firebase RTDB en backend annexe (comptes/chat/classement, pas de gameplay temps réel). Audit de latence 2026-07-12/13 documenté (fix your timestep, réconciliation par trajectoire, rattrapage doux).

**Roadmap moteur** : phases 0 à 135+, quasiment tout ✅/🟢. Phase S en cours (mixeur audio, forces de vent, pipeline assets, Zstd, versioning/RNG). Restent ⬜ : post-effets HDR, SSAO, variants de shaders, terrain sculpté, particules, ombres cascadées, IK deux-os, puis Phase R (WebXR).

**Limites assumées** : pas de PvP, pas de gestion salons publics/privés, menu pause minimal.

## 3. Level design / cartes / monde

**Trois familles de scènes dans `src/scene/demos.rs`, pas une seule carte.** Au-delà des deux fonctions MMORPG, le fichier contient une dizaine de démos techniques du moteur (`controller_demo`, `tower_demo`, `temple_run_demo`, `components_demo`, `zombies_demo`, `roguelike_demo`, `brawl_demo`, `boss_demo`, `escorte_demo`, `gameplay_demo`, `mobile_demo`) qui ne sont pas des niveaux jouables du MMORPG mais des bancs d'essai de mécaniques.

**`Scene::mmorpg_demo()`** (`demos.rs:2043`, `MMORPG_HALF=36`, carte 72×72 m) — **vitrine biome** : prairie centrale, forêt NE, lac + 2 rivières + coude, rizières en damier, promontoire rocheux, village hors les murs (`hamlet_*`), relief réel (`MeshKind::Terrain`) sur la marge ouest. Zéro usage de `siege_*`. C'est la scène « catalogue » qui montre la richesse du biome, pas l'arène de jeu.

**`Scene::hameau_gdd_demo()`** (`demos.rs:6455`, `HALF=24`, fort 48×48 m, sol codé en dur à 90×90 m) — **arène de manche réelle**, le hameau fortifié :
- Remparts en 8 pans nommés (`Rempart Nord Ouest` → `Rempart Ouest Sud`, l.6847-6917)
- Porte principale (brèche 5 m) + portes secondaires (`siege_portcullis.glb`, l.7175)
- Tours de garde (`siege_rampart_stairs.glb`, `siege_rampart_walk.glb`, `siege_rampart_torch.glb`)
- Village en enceinte (`hamlet_*`, 27 chemins distincts)
- Eau : Lac, Rivière ouest, Rivière sud (coordonnées exactes `demos.rs:7952-7973`), enrichie le 19 juillet de 74 instances `shore_*` et 27 `grotto_*` (`demos.rs:8094+`)
- Forêt en anneau (27→70 m autour du fort, scatter via `foret_scatter`/`faune_scatter`, `demos.rs:6549`/`6615`)
- **6 lisières de spawn de vagues** alignées avec les portes (« couloirs dégagés ±13° dans l'axe des 6 lisières », `demos.rs:6466`/`7407`)
- Zone d'exclusion partagée `excl_eau` (14→15 rectangles) protégeant à la fois les voies de circulation et la « clairière de l'Aînée » (autel du boss)

**La scène réellement servie (`assets/player_scene.json`) = `hameau_gdd_demo()` quasi telle quelle**, seules les 26/27 « Créatures » étant importées depuis `mmorpg_demo()` par filtre de nom (`starts_with("Créature")`, `scene/mod.rs:3325`). Le village hors les murs, le promontoire et les rizières de `mmorpg_demo()` sont **volontairement non réintroduits** — décision actée dans `docs/mapsJeuReflexionAnalyse.md` Phase A (18 juillet), pas un bug. C'est une divergence assumée entre « vitrine biome » et « arène de manche ».

**Vocabulaire spatial GDD (§7.1)**, respecté par construction dans le code :
| Espace | Fonction |
|---|---|
| Place (feu central) | Zone ouverte, dangereuse contre la Meute |
| Ruelles | Goulets 1-2 créatures de front |
| Cours / recoins | Poches défendables à une entrée, refuge soin/réanimation |
| Remparts | Verticalité légère, postes de tir Éclaireur |

Règles de construction vérifiables dans le code/mémoire : tout obstacle détectable au raycast à 0,6 m (sinon patrouille figée — piège documenté), aucun point à plus de ~8 s de course d'un espace ouvert, spawns aux lisières jamais dans le dos du joueur, anneau de spawn pour 16 positions distinctes.

**Rythme spatial d'une manche** : le joueur démarre dans/près du fort ; les vagues surgissent aux 6 lisières alignées sur les portes du rempart (cohérent avec la fiction — « la horde vient de la lande ») ; le repli naturel est la place/les cours intérieures. Carte compacte (48×48 m fort / 90×90 m sol total), cohérente avec le refus GDD de minimap permanente (§17.5, voir §4).

**Absence de niveau multiple / progression de cartes** : à ce stade, une seule arène de manche existe. Le GDD ne prévoit pas explicitement de rotation de cartes ; la variation vient des vagues (composition, budget PV) et du contrat du jour, pas de la géométrie.

## 4. UX / ergonomie / HUD

**Deux systèmes HUD coexistent**, l'un legacy codé en dur, l'autre déclaratif orienté données :

| Système | Fichier | Rôle |
|---|---|---|
| HUD legacy | `src/editor/hud.rs` (1435 lignes) | Barre de vie (`health_bar` l.753), HUD arme (`weapon_hud` l.220), compteur frags/assists (`kills_hud` l.274), inventaire d'armes (`weapon_inventory_panel` l.330), sac objets (`item_inventory_panel` l.390), roster multijoueur classé (`multiplayer_roster_panel` l.482), réticule (`crosshair` l.652), HUD de vague (`wave_hud` l.695), fin de manche (`round_summary_banner` l.1285), menu pause (`pause_menu` l.989), contrôles tactiles (`mobile_overlay` l.1091) |
| HUD déclaratif | `src/scene/hud_widgets.rs` | `Scene::hud_widgets` (Text/Image/Gauge/Button ancrés via `HudAnchor`) — une scène/niveau exporté peut définir son propre HUD sans toucher au moteur (Sprint 109) |

`HudLayout` (l.101) mémorise les décalages `[x,y]` de chaque overlay legacy, réglables en glissé (poignée 🖐, `hud_anchor`, `hud.rs:185`) via un mode « 👁 Aperçu HUD › 🖐 Repositionner » — bonne pratique d'ergonomie pour supporter différentes tailles d'écran sans recompiler.

**Feedback joueur** (retours visuels/sonores implémentés) :
- Flash rouge plein écran sur dégât (`damage_vignette`, l.155)
- Bannière « Vaincu » spectateur avec cause de mort détaillée (`defeated_banner`, l.862)
- Bannière « allié à terre » avec **marqueur de direction hors-écran** projeté sur le bord de l'écran (`ally_down_banner` + `offscreen_edge_position`, l.912-982, testé unitairement)
- Bannière nouvelle vague (`wave_start_banner`, l.1344)
- Écran de fin de manche détaillé par joueur (frags/assists/xp) + bannière contrat rempli (`round_summary_banner`, l.1285)
- Retour tactile générique (`touch_feedback`, anneau au point de contact, l.1065)

**Accessibilité — état réel vs prévu** :
- ✅ **Déjà en jeu** : pourcentage numérique à côté de la mini-barre de vie du roster (`health_percent_label`, l.457, testé) — repère non-couleur utile au daltonisme ; `reduce_shake` (`settings.rs:48`, `simulation.rs:126-130`, testé) coupe le recul de caméra au dégât ; `hud_scale` (`settings.rs:41`, borné 0.5-3.0, `clamp_hud_scale` l.748) agrandit tout le HUD.
- ⬜ **Manquant réellement** : un vrai mode daltonien à palette alternative (le repère numérique existant n'en tient pas lieu complètement) — objet de la **Phase K non commencée** (bloc 4 de `sprint2audijeu0718.md`).

**Contrôles tactiles mobiles** (`src/scene/mobile.rs`, config pure ; rendu dans `hud.rs::mobile_overlay` l.1091-1276) — trois schémas mutuellement exclusifs par priorité : pavé tank W/A/S/D (prioritaire), joystick vertical seul (`dual_stick`), joystick libre X/Y (`joystick`).

Décisions d'ergonomie notables et documentées :
- Stick droit de rotation **retiré sur retour explicite** (« tourner reste au clavier tant qu'aucun remplacement tactile n'est défini », `mobile.rs:26-29`)
- Boutons tactiles en **grille 2 colonnes** plutôt qu'une rangée extensible, pour ne jamais chevaucher le pavé tank (`hud.rs:1244-1256`)
- Lettres ASCII W/A/S/D au lieu de flèches ▲▼◀▶ — les glyphes triangle manquent dans la fonte egui Android et se rendent en carrés vides (`hud.rs:1146`/`218`)
- `safe_area` rétracte les contrôles d'une marge de 6 % (max 28 px) pour les encoches d'écran

**Irritants UX documentés (audits internes)** :
- `docs/ANALYSE_DESIGN_VISUEL.md` (15 juillet) §2 note une config `mobile` de scène testée sans d-pad ni barre de vie tactile, et un manque d'identité visuelle par classe (ex. distinguer le Soutien par un « sac cubique dorsal »)
- Le GDD (§17.5) **refuse explicitement une minimap permanente** — argument : carte compacte, danger toujours proche (éveil 9 m), feu communal visible de partout, portes qui s'embrasent à l'arrivée d'une vague, marqueurs hors-écran ponctuels (déjà implémentés via `ally_down_banner`)
- Une vraie minimap **existe** (`AppState::minimap_data()` `app/mod.rs:1314`, `minimap_window()` `editor/windows.rs:289`) mais reste strictement outil éditeur/dev, jamais appelée dans `run_player_overlay()` — cohérence délibérée avec le choix design ci-dessus

**Évaluation UX globale** : le HUD couvre bien les besoins fonctionnels immédiats (vie, arme, roster, feedback de dégât/mort/vague) et la double architecture (legacy + déclaratif) donne une trajectoire de migration propre plutôt qu'une dette. Le point faible reste l'accessibilité (daltonisme réel) et la validation empirique de l'ergonomie tactile (irritants notés mais pas encore tous corrigés).

## 5. Inventaire / progression / économie in-game

**Système d'inventaire réel, à trois familles distinctes** (`src/app/inventory.rs`, 224 lignes) :

1. **Pièces** (`collect_at`) — comptent au score/objectif de victoire, effet instantané
2. **Butins d'arme** (`weapon_pickup_at`) — équipent immédiatement (`WeaponPickup`) ; la **diffusion réseau** de ce pickup est la Phase M, non commencée (bloc 3 de `sprint2audijeu0718.md`)
3. **Sac** (`AppState::inventory: Vec<(ItemKind, u32)>`) — objets ramassés au contact, empilés par sorte, ordre = première découverte. `ItemKind` (`scene/mod.rs:677-686`) : `Potion`, `Baie` (soin), `Cle`, `Gemme` (collection). Consommation via panneau HUD 👜 (`use_item`), actif seulement si `heal() > 0` et si la scène a une barre de vie.

**Progression réelle côté serveur** (`src/bin/server.rs`) :
| Constante | Valeur | Ligne |
|---|---|---|
| `XP_PER_LEVEL` | 1000 | l.59 |
| `XP_PARTICIPATION` | 150 | l.470 |
| `XP_PER_FRAG_OR_ASSIST` | 5 | l.476 |
| `XP_VICTORY_BONUS` | 75 | l.480 |
| `XP_CONTRACT` | 250 | l.497 |

Cohérent avec le §8.3 du GDD.

**3 classes réellement codées, serveur uniquement** (`PlayerClass`, `src/app/multiplayer.rs:44-135`) — appliquées au spawn, jamais côté client (commentaire explicite « règle d'or anti-triche, GDD §5.7 : le serveur est seul juge ») :
- **Assaut** (défaut, zéro régression)
- **Éclaireur** (vitesse +25 %, saut +30 %, PV max −30 %)
- **Soutien** (vitesse −15 %, dégâts −30 %, soin ×2,5, **seule classe pouvant réanimer** via `can_revive()`)

Sélecteur de classe réseau robuste anti-triche (`to_u8`/`from_u8`, valeur hors table retombe sur Assaut).

**`favorite_weapon` : confirmé absent du code** (`grep -rn "favorite_weapon" src/` → 0 résultat). Uniquement cité dans `sprint2audijeu0718.md` Phase N comme travail non commencé — l'écart GDD↔code déjà identifié (le GDD affirme à tort que cette stat est déjà persistée) est donc confirmé au niveau code, pas seulement doc.

**Aucune boutique, craft, ni cosmétique** : `grep -rin "shop|boutique|craft|cosmetic|battle_pass" src/*.rs` → 0 résultat. Cohérent avec les exclusions fermes du GDD §12.

**Évaluation** : la progression est simple et volontairement plate (pas de puissance brute, juste des options — cohérent avec la philosophie coop du GDD), bien implémentée côté serveur avec anti-triche pris au sérieux. Le principal manque n'est pas un trou de design mais un trou d'instrumentation (stats jamais persistées comme `favorite_weapon`).

## 6. Architecture logicielle

**`AppState`** (`src/app/mod.rs:183+`) est le cœur partagé éditeur / serveur / player, organisé en strates :
- État de scène/sélection (`scene`, `selection`, `selected`, `clipboard`)
- Mode (`playing`, `fly_cam`, `paused`, `player`, `should_quit`, `locale`)
- Simulation à **pas fixe découplé du rendu** (`sim_accumulator`, `sim_prev_poses`/`sim_curr_poses`/`sim_render_poses` pour l'interpolation anti-judder)
- État de jeu (`win_time`, `lost`, `score`, `game_events` — file consommée un tick après émission, décalage volontaire pour un ordre déterministe)
- IA/gameplay (`trigger_prev`, `furtive_awake`)
- Scripting (`lua_vars`, persistant via `save.get/set`)
- Inventaire (`inventory`), aperçu mobile (`device_preview`)

**Boucle par frame** (`AppState::advance_play`, `src/app/simulation.rs:275`) : poll des chargements async (imports glTF, sons, IA, réseau) → calcul dt/FPS lissé (EMA) → transitions Edit↔Play (snapshot, `init_waves`, construction physique `Physics::build`, lancement audio autoplay spatialisé) → simulation à pas fixe. `src/lib.rs` (winit `ApplicationHandler`) pilote `renderer.render(&mut self.state)` (l.469) **séparément** du tick logique.

**Pas un ECS strict** — `Scene::objects: Vec<SceneObject>` (`scene/mod.rs:1174`/`748`) : chaque `SceneObject` porte directement transform/mesh/script/physics/collider/audio/color, avec des **composants optionnels** (`combat`, `controller`, `audio: Option<AudioSource>`) plutôt qu'un stockage par composant façon `World`/`Query` (Bevy-like). Modèle « objet monolithique à champs optionnels », plus proche d'un moteur de jeu classique (Unity-like) qu'un ECS data-oriented — choix cohérent avec la position anti-Bevy revendiquée dans le README.

**Articulation des sous-systèmes** :
- `gfx/` (rendu wgpu) — appelé uniquement depuis `renderer.render` dans la boucle winit
- `runtime/physics.rs` (rapier3d) — `Physics::build(&self.scene)` à l'entrée en Play, avancé dans `sim_step`
- `net/` — `poll_network()` appelé en tête d'`advance_play`, avant même le calcul du dt
- `app/` — toute la logique sans GPU

**Séparation client/serveur propre** : `src/bin/server.rs` réutilise directement `AppState::new()` (l.148/171/932/1125) en mode headless. Pas de duplication de logique de jeu : le serveur exécute la **même** simulation que le client/éditeur, sans renderer ni fenêtre.

**Points de couplage fragiles identifiés** :
| Fichier | Lignes | Risque |
|---|---|---|
| `src/app/mod.rs` | 4 371 | AppState central « fait beaucoup » par construction |
| `src/scene/demos.rs` | 10 820 | Authoring programmatique massif, un seul fichier pour toutes les scènes |
| `src/app/network_client.rs` | 3 228 | Volumineux |
| `src/app/simulation.rs` | 3 014 | Volumineux |

Contrepoint positif : extraction récente et bien ciblée en modules plus petits (`app/combat.rs`, `app/health.rs`, `app/fireball.rs`, `app/creature_attack.rs`) — discipline de refactoring progressive, pas un chantier arrêté.

**Évaluation architecture** : choix pragmatique et cohérent (moteur maison simple plutôt qu'ECS générique), bon découplage rendu/simulation/réseau, anti-triche pris au sérieux (autorité serveur pour les classes). Le risque principal est la concentration de logique dans quelques très gros fichiers (`app/mod.rs`, `demos.rs`) — gérable tant que le rythme d'extraction en modules dédiés se poursuit.

## 7. Pipeline 3D / direction artistique

**Charte graphique** (vit dans les conventions internes + `docs/mapsJeuReflexionAnalyse.md` §3, pas de fichier dédié unique) :
- ≤3 teintes par objet, aucune texture (`base_color_factor` seul)
- Un seul mesh joint par objet exporté (sauf skinné animé)
- Sol Blender à z=0, export Y-up glTF, échelle appliquée **avant** rotation (piège documenté : un twist appliqué avant le scale bascule l'axe voulu)
- Émissif glTF ignoré par le moteur (`src/scene/import.rs`) — le brillant vient de `obj.emissive` côté Rust, pas du glTF
- Règle GDD §10.1 « froid = décor, chaud/saturé = enjeu » appliquée : palette lande/hameau en pierre/bois froids vs joueur/projectiles/menaces en orange/magenta/cyan saturés
- Faune décorative (`fauna_*`) neutre et intouchable par construction (jamais `attackable`)

**Processus de génération procédurale**, illustré par `scripts/blender/gen_creature_pack63_67.py` (422 lignes, 5 créatures savane) :
1. Import de primitives partagées depuis `creature_kit.py` (`sphere`/`cylinder`/`cone`/`quad_bones`/`quad_walk_keys`/`build_creature`)
2. Scène propre (`fresh_scene()`)
3. Matériaux nommés par teinte RVB pure (ex. `material("Lion63Tawny", (0.72,0.55,0.28))`)
4. Corps construit sphère par sphère (chevauchement obligatoire des sphères de crinière pour éviter les trous vus en plongée — piège documenté et réutilisé d'un pack à l'autre)
5. Squelette généré via `quad_bones` (hiérarchie standard 4 pattes)
6. Clips d'animation par keyframes explicites sur les os (`Idle` 40 frames, `Walk` 24 frames, à 24 fps)
7. Export via `build_creature`

**Conventions de nommage par famille** (catalogue au 19 juillet, 422 fichiers `.glb`) :
| Préfixe | Volume | Usage |
|---|---|---|
| `creature_*`/`monster_*` | ~106+ | Bestiaire jouable, IA `AiChaser` ou script Lua |
| `hamlet_*` | 40 | Bâtiments/props du village |
| `siege_*` | 40 | Fortifications + props de mode |
| `nature_*` | 123 | Flore/pierre/signalétique |
| `fauna_*` | 29 | Faune décorative neutre |
| `item_*` | 30 | Objets ramassables |
| `grotto_*`/`shore_*` | 20+20 | Décor organique grotte/rive (métaballes, `organic_common.py`) |
| `fairy_hero` | 1 | Placeholder héroïne cible |

Dont 10 nouveaux `creature63-72` non commités (pack savane 63-67 + pack organique 68-72).

**État de l'animation** : squelette/skinning porté par `src/scene/import.rs` (`Skeleton`/`Joint` avec parent/enfant + pose de liaison inverse pour skinning GPU, `load_gltf_skeleton` l.168-234) — lit le **premier skin** du glTF, `Ok(None)` si pas de skin. Les scripts encodent l'animation par clips NLA nommés (`Idle`/`Walk` standard), un os par pièce à poids 1.0 (pas de skinning multi-os pondéré fin — simplicité assumée). **Aucun fallback keyframe d'objet** pour les assets animés du pack siège : squelette + clip uniquement, aucun filet.

**Contrôles qualité automatisés** (`scripts/blender/check_creatures.py`, 96 lignes, couvre `creature21-26` + `creature32-72`), exécutés sur le fichier **exporté** :
- Présence des pistes NLA `Idle`(40)/`Walk`(24)
- **Bouclage parfait** (comparaison des matrices de tous les os entre premier et dernier frame, tolérance 1e-3)
- **Garde au sol** (aucun vertex sous z=-0.001, sauf flotteurs assumés type fantôme/poissons) — lié au piège de dépénétration TriMesh documenté ailleurs dans le projet
- **Budget d'os** (`JOINT_CAPACITY = 128` côté moteur)

Sortie OK/ÉCHEC par créature, code de retour non nul en cas d'échec (utilisable en CI). Scripts jumeaux existants pour les autres packs (`check_hamlet_pack.py`, `check_organic_pack.py`, `check_siege_pack.py`).

**Évaluation pipeline 3D** : chaîne de production remarquablement disciplinée pour un moteur solo/petite équipe — génération 100 % procédurale, conventions strictes documentées, contrôle qualité automatisé et scriptable en CI. Le risque principal est la scalabilité humaine du système (chaque nouvelle créature = nouveau script Python + vérif manuelle), mais le taux de production (422 assets, +10 en une session) montre que le pipeline tient la charge.

## 8. Documentation de sprint — où en est le travail « produit »

`docs/` contient 24 fichiers `.md` + sous-dossiers `audits/`, `blender/`, `guide-createur/`.

Document le plus récent : **`docs/sprint2audijeu0718.md`** (18 juillet 2026), suite de `AUDIT_JEU_2026-07-18.md`. Couvre 9 phases (J→R) organisées en 5 blocs sans chevauchement de fichiers :

| Bloc | Phases | État |
|---|---|---|
| 1 | L (catalogue UI multijoueur, présence en ligne, marqueur allié) + R (CI goldens GPU macOS/Metal désormais bloquant) | ✅ mergé (`6c79635`) |
| 2 | J (écran de fin de manche détaillé) + O (audio manquant `Sfx::AllyDown`, `Sfx::CreatureWake`) | ✅ mergé (même commit) |
| 3 | M (WeaponPickup réseau) + Q (tests UI éditeur) | ⬜ non commencé |
| 4 | K (accessibilité : HUD scale, reduce shake, daltonien) + N (instrumentation `favorite_weapon`) | ⬜ non commencé |
| 5 | P (authentification `firebase_uid` par token + bump groupé `PROTOCOL_VERSION`) | ⬜ non commencé — sécurité |

Convention retenue : un seul bump de `PROTOCOL_VERSION` (actuellement **6**, `src/net/protocol.rs:42`) porté par la phase P en dernier, pour éviter des déploiements couplés client/VPS répétés.

**Autres documents notables** :
- `ANALYSE_DESIGN_VISUEL.md` — analyse critique du 15 juillet, à l'origine des correctifs GDD §6/§10.
- `AUDIT_JEU_2026-07-17.md` / `AUDIT_JEU_2026-07-18.md` — audits 4 dimensions (gameplay/rendu/réseau/qualité).
- `ROADMAP_SPRINTS.md` — source de vérité détaillée du moteur, sprint par sprint.
- `SPRINT3D_AUDIT_GAMEDESIGN.md` — audit de la scène servie face au GDD (18 juillet).
- `SPRINTNETWORK.md` / `SPRINT_MMORPG.md` — sprints réseau/latence et feuille de route multijoueur.
- `SPRINTS.md` — récap historique figé sprints 0→44.
- `architecture.md` — état des lieux courant du moteur.
- `auditGDD10h.md` → `sprint10audit.md` — audit d'écart GDD↔code (18 juillet), traduit en phases A-G exécutables.
- `creation3DBlendersuite.md` (pack siège), `creationAnimation3DBlendersuite.md` (volet animé), `creation3DBlenderOrganicSuite.md` (grottes/rives, 40/40 terminé), `integration_siege_scene.md`.
- `integrationMCPfuture.md` — proposition future d'un serveur MCP « RustyGear ».
- `mapsJeuReflexionAnalyse.md`, `optimisation3D.Analys.md`, `rapport_qualite_creatures_vs_hyper3d.md`, `reflexion.md`, `sprintcration3delement.md`, `sprintjeurefelxion.md`, `sprintoptimation3daudit10h.md`, `sprintreflecion.md`.

## 9. Structure du code (68 494 lignes de Rust dans `src/`)

**Fichiers les plus volumineux** :
- `src/scene/demos.rs` — 10 820 lignes (authoring programmatique des scènes)
- `src/app/mod.rs` — 4 371 lignes (AppState central)
- `src/scene/mod.rs` — 4 016 lignes (Transform, MeshKind, Scene, sérialisation)
- `src/gfx/renderer.rs` — 3 363 lignes
- `src/app/network_client.rs` — 3 228 lignes
- `src/app/simulation.rs` — 3 014 lignes
- `src/editor/mod.rs` — 2 486, `src/editor/windows.rs` — 2 343, `src/runtime/physics.rs` — 2 359
- `src/app/multiplayer.rs` — 1 829, `src/scene/import.rs` — 1 647, `src/bin/server.rs` — 1 609, `src/gfx/pipelines.rs` — 1 366

**Modules récents bien ciblés** : `src/app/combat.rs` (579, extrait pour réutilisation serveur/client), `src/app/health.rs` (1082), `src/app/fireball.rs` (1119), `src/app/creature_attack.rs` (1067), `src/net/protocol.rs` (741), `src/net/firebase.rs` (872), `src/net/server_loop.rs` (1144), `src/net/interpolation.rs` (372).

**Résumé par module** :
| Module | Rôle | État |
|---|---|---|
| `app/` | Logique sans GPU : AppState, picking, combat, multiplayer, health, fireball, settings, locale, scripting, inventaire | Mature |
| `gfx/` | Rendu wgpu : renderer, mesh, camera, pipelines WGSL, texcompress, LOD | Mature |
| `scene/` | Transform/MeshKind/Scene, groupes, lumières, import glTF, mobile.rs (contrôles tactiles), hud_widgets | Mature (demos.rs = gros volume d'authoring) |
| `runtime/` | Mode Play : physics (rapier3d), audio (kira), sfx (bips synthétisés), savegame, rng | Mature |
| `net/` | Multijoueur : protocol, server_loop, interpolation, firebase, client native/web | Mature, actif |
| `editor/` | UI egui desktop : hud.rs, windows.rs, menus.rs, hierarchy.rs, export.rs, readiness.rs | Mature mais sous-testé (voir §12) |
| `bin/` | server.rs (headless), glbviewer.rs (visualiseur GLB) | Mature |

**Dette technique marquée** : quasiment nulle — un seul `TODO` trouvé dans tout `src/` (`src/runtime/mod.rs:16`, vibration Android via JNI non implémentée, actuellement journalisée). Cohérent avec la discipline de tests-preuves et d'audits documentés du projet.

## 10. Scènes et assets — inventaire chiffré

**`assets/player_scene.json`** (1,48 Mo) : 983 objets au total, dont 315 meshes importés distincts. 27 objets avec composant `combat` (créatures attaquables), répartis par vague avec un budget de PV strictement croissant (vague 0 = 1 résiduel, vague 1 = 5, vague 2 = 6, vague 3 = 7, vague 4 = 8), conforme à la règle GDD §5.5. Le GDD annonce « 26 créatures attaquables » — léger écart avec le comptage brut (27), probablement lié à l'objet résiduel `wave:0`.

**`assets/models/`** : 422 fichiers `.glb` — répartition détaillée en §7. **Non commités** : creature63 à creature72 (10 nouvelles créatures + previews PNG).

**`scripts/blender/`** : 61 fichiers — détail du pipeline en §7. Nouveaux non commités : `gen_creature_pack63_67.py` et `gen_creature_pack68_72_organic.py`.

## 11. État git / travaux en cours

Activité très dense sur 18-19 juillet 2026 : durcissement CI goldens, Phase L (catalogue UI) + Phase O (audio) mergées, résumé de fin de manche (Phase J), diffusion `GameEvent::Win/Lose`, intégration du pack siège dans la scène servie, pack rives + grottes/rives organique, ajout du convoi Escorte manquant dans `mmorpg_demo`, terrain à relief réel (bassin, tunnel).

**Dernier commit** : `83fb16d` « Sprint composition de carte : eau (shore_*) et grotte (grotto_*) dans le hameau fortifié ».

**Non commité actuellement** (17 fichiers modifiés, +1069/-46) :
- `src/app/mod.rs` (+349) et `src/editor/windows.rs` (+423) — plus gros deltas, probablement la fenêtre Multijoueur enrichie / finition des phases J-L
- `src/scene/demos.rs`, `src/scene/mobile.rs`, `src/editor/hud.rs`, `src/editor/mod.rs`, `src/app/locale.rs`, `src/app/input.rs`, `src/lib.rs`, `src/gfx/renderer.rs` — deltas plus petits
- `Cargo.toml`, `README.md` (+91), `GDD_MMORPG.md` (+1), `packaging/build_dmg.sh`, `scripts/blender/check_creatures.py`, `assets/player_scene.json`, `docs/sprint2audijeu0718.md`
- **Non trackés** : 10 nouveaux modèles de créatures (creature63-72 + previews) et leurs 2 scripts générateurs

→ Session de travail active sur deux fronts : (a) extension du bestiaire de créatures, (b) finalisation du catalogue d'interface réseau, en cohérence avec les phases J/L/K.

## 12. Tests et CI

- `tests/` : `flora_assets.rs`, `golden_render.rs`, `golden_skinning.rs`, dossier `golden/` (images de référence).
- **655 tests inline `#[test]`** dans `src/` — volume substantiel. `cargo test --lib` ≈ 590-592 tests, `cargo test --features net_tests` ≈ 613 (sockets réels).
- **CI** (`.github/workflows/`) : `ci.yml`, `pages.yml`, `release.yml`
  - `check` (ubuntu) : fmt --check, clippy -D warnings, `cargo test --all-targets`, budget unwrap/expect/panic (`scripts/check_unwrap_budget.py`)
  - `net-tests` (ubuntu, séparé) : tests réseau à sockets réels
  - `golden` (macOS/Metal) : **récemment durci** (retrait `continue-on-error`, 18 juillet) après 15 runs verts consécutifs, désormais bloquant
  - `cross-build` : build-only sur aarch64-linux-android, aarch64-apple-ios, wasm32-unknown-unknown
- **Point faible identifié** : UI éditeur sous-testée — `hud.rs` 0 test, `menus.rs` 0 test, `windows.rs` 2 tests seulement. C'est l'objet de la Phase Q (non commencée).

## 13. Réseau / multijoueur

Fichiers clés : `src/net/protocol.rs` (`PROTOCOL_VERSION = 6`), `src/net/server_loop.rs`, `src/net/firebase.rs`, `src/net/interpolation.rs`, `src/net/client/{native,web}.rs`, `src/bin/server.rs`, plus `src/app/multiplayer.rs`, `src/app/network_client.rs`.

- Serveur autoritaire Rust headless partageant l'`AppState` avec l'éditeur desktop (pas de duplication de logique). Transport WebSocket + bincode (~540 octets/snapshot pour 20 entités).
- Reconnexion automatique avec backoff plafonné (le GDD §16.1 exige un affichage « Reconnexion… », jamais un retour brutal au menu).
- Multi-salons (`Room` dans `server.rs`) : code de salon, classes de joueur, contrat du jour, frags/assists diffusés en direct.
- **Limites assumées** : pas de dégâts joueur-contre-joueur, pas de sélecteur de mode réseau à la création, pas de protection contre collision de codes de salon, menu pause minimal.
- **Faille de sécurité connue et planifiée (Phase P)** : `firebase_uid` n'est aujourd'hui validé que sur la forme (charset), pas sur l'authenticité — un client modifié peut réclamer un uid arbitraire tant que la vérification par token n'est pas ajoutée côté serveur.
- Vertical slice réseau démontré : plusieurs appareils (desktop/APK/navigateur) se connectent en continu au même VPS (service systemd) et interagissent en temps réel.

## 14. Synthèse — écarts GDD ↔ code les plus significatifs

1. **Avatar joueur** : sphère placeholder servie au lieu de `fairy_hero` (depuis le 17 juillet 2026).
2. **Modes de manche** : Survie/Boss/Escorte codés mais non sélectionnables en réseau (`objective: 0` figé côté client natif).
3. **Archétypes de créatures** : implémentés au code mais pas reflétés dans le casting de la scène servie (patrouilles décoratives).
4. **Scène servie vs scène vitrine** : divergence historique « substantiellement résolue » mais resynchro encore manuelle/`#[ignore]`.
5. **Sécurité auth** : `firebase_uid` non vérifié par token — trou de sécurité identifié et planifié (Phase P), pas encore corrigé.
6. **Couverture de tests UI éditeur** : quasi nulle (`hud.rs`, `menus.rs`, `windows.rs`), phase de rattrapage (Q) pas commencée.
7. **`favorite_weapon`** : absent du code, alors que le GDD affirme à tort que cette stat est déjà persistée (Phase N doit l'instrumenter réellement).
8. **Accessibilité daltonisme** : repère numérique déjà présent (roster) mais pas de vrai mode de palette alternative (Phase K non commencée).
9. **WeaponPickup réseau** : logique locale existe (`inventory.rs`) mais pas encore diffusée entre joueurs (Phase M non commencée).
10. **Minimap** : existe côté outil éditeur mais volontairement absente côté joueur — cohérence design assumée, pas un manque.

## 15. Radar de maturité par dimension

| Dimension | Maturité | Commentaire |
|---|---|---|
| Moteur technique (rendu/physique/audio) | ●●●●● | Très mature, ~32k lignes, aucune dette TODO marquée |
| Réseau/multijoueur (cœur) | ●●●●○ | Autoritaire, reconnexion, salons — sécurité auth à finir (Phase P) |
| Pipeline 3D/asset procédural | ●●●●● | 422 assets, contrôle qualité automatisé en CI, conventions strictes |
| Level design (arène de manche) | ●●●●○ | Géométrie et règles solides, une seule carte à ce stade |
| Combat/progression (systèmes) | ●●●●○ | Classes, XP, économie de vie calibrées ; puissance plate assumée |
| Inventaire | ●●●○○ | Fonctionnel localement, sync réseau du pickup d'arme manquante |
| UX/HUD (fonctionnel) | ●●●●○ | Feedback riche (dégâts, mort, vague, fin de manche) |
| Accessibilité | ●●○○○ | HUD scale/reduce shake faits, daltonisme réel manquant |
| Tests UI éditeur | ●○○○○ | Quasi nul (`hud.rs`/`menus.rs`/`windows.rs`) |
| Contenu narratif/casting IA | ●●●○○ | Archétypes codés mais pas branchés sur la scène servie |
| Identité visuelle joueur | ●●○○○ | Avatar placeholder (sphère) au lieu de `fairy_hero` |

*Légende : ●●●●● = très mature/complet, ●○○○○ = amorce seulement.*
