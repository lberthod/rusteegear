# Analyse comparative — RusteeGear face à Unity, Unreal Engine, Godot et Bevy

> Rapport du 4 septembre 2026. Photographie factuelle de RusteeGear (dépôt
> `motor3derust`, branche `main`, commit `5d8b0d2`) confrontée aux quatre
> moteurs de référence du marché. Les faits sur RusteeGear viennent d'une
> lecture directe du code (`src/`, `tests/`, `packaging/`, `.github/`) et des
> docs du dépôt ; les faits sur les moteurs tiers viennent de leur
> documentation publique à la date de rédaction (numéros de version donnés à
> titre indicatif, à re-vérifier avant toute citation externe).

## Sommaire

1. [Résumé exécutif](#1-résumé-exécutif)
2. [Périmètre et méthode](#2-périmètre-et-méthode)
3. [Fiche d'identité de RusteeGear](#3-fiche-didentité-de-rusteegear)
4. [Les moteurs comparés en une page chacun](#4-les-moteurs-comparés-en-une-page-chacun)
5. [Grille comparative par domaine](#5-grille-comparative-par-domaine)
6. [Analyse détaillée domaine par domaine](#6-analyse-détaillée-domaine-par-domaine)
7. [Ce que RusteeGear fait que les autres ne font pas](#7-ce-que-rusteegear-fait-que-les-autres-ne-font-pas)
8. [Radar de maturité](#8-radar-de-maturité)
9. [Positionnement : quand choisir quoi](#9-positionnement--quand-choisir-quoi)
10. [Risques et dette structurelle](#10-risques-et-dette-structurelle)
11. [Recommandations : quoi emprunter à qui](#11-recommandations--quoi-emprunter-à-qui)
12. [Annexes](#12-annexes)

---

## 1. Résumé exécutif

RusteeGear est un **moteur 3D compact écrit à la main en Rust** (≈ 79 600
lignes dans `src/`, dont ≈ 67 500 hors tests), livré avec un éditeur egui, un
runtime Lua, une physique rapier, un serveur multijoueur autoritaire headless
et des exports macOS, Android, iOS et web. Il a été démarré le 18 juin 2026 et
compte 619 commits en 11 semaines. Un vrai jeu coop en ligne, « Le Hameau des
Braises », tourne dessus.

**Comparé aux quatre références, le verdict tient en cinq points.**

- **Ce n'est pas un concurrent de Unity, Unreal ou Godot sur le périmètre
  fonctionnel**, et il ne prétend pas l'être. Il manque une hiérarchie de
  transforms, un vrai BRDF PBR, des ombres en cascade, des particules, la
  transparence, un LOD général, un navmesh, une machine à états d'animation,
  un système d'UI de jeu dédié et une API de plugins. Chacun de ces points est
  standard dans les trois moteurs grand public.
- **Sur le multijoueur « petit salon avec autorité serveur », il est plus
  avancé que Bevy et Godot en sortie de boîte**, et au niveau de ce que Unity
  offre avec Netcode for GameObjects : simulation identique client/serveur,
  prédiction, interpolation, réconciliation par historique de trajectoire,
  limitation de débit, salons, classes. Seul Unreal fait mieux nativement.
- **Sur l'outillage de test et de pilotage, il dépasse tous les autres pour
  sa taille** : 825 tests, goldens de rendu, tests différentiels Lua
  natif/web, budget d'`unwrap` vérifié en CI, et un pont TCP (`--pilot`) qui
  permet à un agent ou une CI de piloter l'éditeur en marche. Aucun des quatre
  moteurs ne livre cela clé en main.
- **Sa vraie force est sa lisibilité** : un développeur seul peut tenir le
  code entier en tête, sans ECS, sans ordonnanceur, sans build system
  propriétaire. C'est l'inverse exact des 20+ millions de lignes d'Unreal.
- **Sa vraie faiblesse est structurelle, pas fonctionnelle** : le
  `SceneObject` à 30 champs optionnels et l'absence de parent/enfant sont des
  choix qui simplifient aujourd'hui et bloqueront demain. C'est la première
  chose à corriger si le moteur doit dépasser les « petits jeux ».

**Positionnement recommandé** : « le moteur Rust pour livrer un petit jeu 3D
multijoueur en une semaine, sur mobile et web, avec un code qu'on comprend ».
Ne pas courir après Godot ; creuser l'écart sur le réseau, le pilotage
automatisé et l'export web.

---

## 2. Périmètre et méthode

**Ce qui a été inspecté dans le dépôt**

| Zone | Fichiers clés |
|---|---|
| Rendu | `src/gfx/renderer/{frame,shadows,post_process,resources,types}.rs`, `src/gfx/{pipelines,mesh,lod,texcompress}.rs`, 8 shaders WGSL |
| Scène et données | `src/scene/{mod,persistence,prefab,import,hud_widgets}.rs`, `src/project.rs`, `src/assets.rs` |
| Éditeur | `src/editor/{mod,windows,hud,hierarchy,menus,export}.rs`, `src/app/{selection,picking}.rs` |
| Scripting | `src/app/{scripting,scripting_web}.rs`, `docs/LUA_PORTABLE.md` |
| Simulation et gameplay | `src/app/{simulation,combat,fireball,creature_attack,health,inventory,multiplayer}.rs` |
| Physique, audio | `src/runtime/physics/*`, `src/runtime/{audio,sfx,savegame,rng}.rs` |
| Réseau | `src/net/{protocol,server_loop,interpolation,firebase}.rs`, `src/net/client/{native,web}.rs`, `src/bin/server.rs` |
| Build, CI | `Cargo.toml`, `rust-toolchain.toml`, `packaging/*.sh`, `.github/workflows/{ci,pages,release}.yml` |
| Tests et pilotage | `tests/*.rs`, `src/pilot.rs`, `src/bin/pilot.rs`, `docs/PILOT.md` |
| Docs | `README.md`, `docs/architecture.md`, `docs/KNOWN_LIMITATIONS.md`, `analysedev.md`, `GDD_MMORPG.md` |

**Méthode de comparaison.** Chaque domaine est noté sur cinq critères
possibles : présent et mature, présent mais partiel, absent volontairement
(documenté), absent, sans objet. Les notes du radar (§8) sont sur 5 et
reflètent l'usage réel pour un petit studio, pas la quantité de fonctionnalités.

**Ce que ce rapport n'est pas.** Ce n'est ni un benchmark de performance (aucune
mesure croisée n'a été faite), ni un audit du jeu (voir `analysedev.md` et
`docs/audits/`), ni une revue de code (voir `docs/roadmapaudit3septembre.md`).

---

## 3. Fiche d'identité de RusteeGear

| Attribut | Valeur |
|---|---|
| Langage | Rust 1.98.0 épinglé, édition 2024 |
| Taille | 79 557 lignes dans `src/` (118 fichiers), ≈ 67 500 hors tests |
| Modules de premier niveau | 12 : `app`, `assets`, `crash_log`, `editor`, `gfx`, `log_buffer`, `net`, `pilot`, `project`, `runtime`, `scene`, `time_compat` |
| Binaires | 4 : `motor3derust` (éditeur), `server` (headless), `pilot` (client de pilotage), `glbviewer` |
| Dépendances directes | 47 déclarations, 519 crates dans le lock |
| Briques externes | winit 0.30, wgpu 29, egui 0.34, glam, rapier3d 0.33, kira 0.12, mlua 0.11 (Lua 5.4), rilua 0.1 (Lua 5.1, web), gltf, tokio, tokio-tungstenite, bincode, gilrs |
| Modèle de scène | `Vec<SceneObject>` plat, groupes par nom, prefabs JSON |
| Rendu | Forward, HDR Rgba16Float, MSAA 4×, shadow map 2048 PCF 5×5, bloom, ACES, ciel dégradé, brouillard exponentiel |
| Éclairage | 1 directionnelle + ambiante + 8 ponctuelles/spots |
| Scripting | Lua par objet, API d'une trentaine de symboles, débogueur à points d'arrêt |
| Physique | rapier3d, Static/Dynamic/Kinematic, 6 formes de collider, heightfield, character controller, raycast, overlap sphere |
| Réseau | WebSocket + bincode, protocole v7, 60 Hz, autorité serveur, prédiction + interpolation + réconciliation, 16 salons × 16 joueurs |
| Plateformes | macOS (éditeur + player), Android APK, iOS, web WASM/WebGPU, serveur Linux |
| Tests | 825 fonctions `#[test]`, 11 fichiers d'intégration, 5 goldens de rendu, tests à socket réel derrière `net_tests` |
| CI | fmt, clippy `-D warnings`, tests, budget unwrap/expect/panic, orphelins de bundle, cross-builds, Pages, Release |
| Pipeline d'assets | 60+ scripts Blender headless, 482 GLB, ≈ 174 Mo en LFS |
| Licence | MIT |
| Historique | 619 commits depuis le 18 juin 2026 |

---

## 4. Les moteurs comparés en une page chacun

### 4.1 Unity (Unity Technologies)

Moteur généraliste C#, propriétaire (source consultable sous licence payante).
Versions Unity 6.x. Deux pipelines de rendu scriptables (URP mobile-first,
HDRP haut de gamme), GameObject + MonoBehaviour par défaut, DOTS/ECS
optionnel. Mecanim (machines à états, blend trees, IK), Shuriken et VFX
Graph, Shader Graph, Terrain, NavMesh, Timeline, Cinemachine, Addressables,
UI Toolkit. Réseau : Netcode for GameObjects et Netcode for Entities, Unity
Transport en UDP, services Relay/Lobby/Matchmaker payants à l'usage. Export
vers plus de 20 plateformes dont consoles. Asset Store immense. Modèle
économique : gratuit sous un plafond de revenus, puis licence par siège ;
le « Runtime Fee » a été retiré en 2024 après la réaction de la communauté.
Faiblesses connues : boîte noire, temps de domaine long, dépendance à
l'éditeur, instabilité historique de la feuille de route.

### 4.2 Unreal Engine (Epic Games)

Moteur AAA C++ avec Blueprints, source disponible sur GitHub. Versions 5.x.
Nanite (géométrie virtualisée), Lumen (GI dynamique), Virtual Shadow Maps,
Niagara (particules), Chaos (physique), MetaSounds, Control Rig, MetaHuman,
World Partition, Gameplay Ability System. Réplication réseau native au niveau
des acteurs (propriétés répliquées, RPC, autorité serveur, prédiction du
mouvement de personnage). Royalties de 5 % au-delà d'un seuil de revenus
bruts pour les jeux ; licence par siège hors jeu vidéo. Faiblesses connues :
poids colossal (dizaines de Go, builds longs, machine puissante exigée),
courbe d'apprentissage C++/UBT, projets difficiles à tenir pour une équipe
d'une personne, export web abandonné officiellement.

### 4.3 Godot (Godot Foundation)

Moteur libre MIT, C++, versions 4.x. Arbre de nœuds et scènes instanciables
comme modèle universel, GDScript, C#, GDExtension pour C++/Rust. Trois
rendus : Forward+ (Vulkan/D3D12/Metal), Mobile, Compatibility (OpenGL/
WebGL2). PBR complet, ombres en cascade, GI (SDFGI, VoxelGI), particules GPU,
AnimationTree, navigation, physique Godot Physics ou Jolt. Multijoueur de
haut niveau : `MultiplayerAPI`, `MultiplayerSpawner`/`Synchronizer`, RPC,
transports ENet, WebSocket, WebRTC. Export desktop, mobile, web ; consoles
via prestataires. Environ 1,5 à 2 millions de lignes de C++. Faiblesses
connues : performances 3D en retrait sur les grosses scènes, export web
limité à WebGL2, pas d'autorité serveur clé en main (le modèle est
« autorité par nœud », on construit le reste).

### 4.4 Bevy (communauté, Rust)

Moteur libre MIT/Apache en Rust, versions 0.1x, cadence de release
trimestrielle avec ruptures d'API à chaque version. ECS au cœur de tout,
ordonnanceur de systèmes parallèle, rendu wgpu PBR avec forward clustérisé,
ombres en cascade, SSAO, TAA, bloom, éclairage volumétrique, GI expérimentale
par ray tracing. Graphe d'animation, `bevy_ui`, hot-reload d'assets, export
wasm. **Pas d'éditeur officiel stable** (chantier en cours), **pas de réseau
intégré** (crates communautaires `lightyear`, `bevy_replicon`), physique par
`avian` ou `bevy_rapier`. Écosystème de crates très riche mais fragmenté.
Faiblesses connues : instabilité d'API, absence d'éditeur, courbe ECS.

### 4.5 Mention : Fyrox (Rust)

Moteur Rust avec éditeur, graphe de scène, rendu différé PBR, scripting en
Rust natif. Cité parce que c'est le comparable le plus proche en ambition
(« moteur complet avec éditeur, en Rust, par une très petite équipe »). Il
n'est pas détaillé dans les grilles ci-dessous.

---

## 5. Grille comparative par domaine

Légende : ✅ mature · 🟡 partiel · ⚪ absent volontairement (documenté) · ❌ absent · — sans objet.

### 5.1 Rendu

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Type de pipeline | Forward simple | Forward+/Deferred (URP/HDRP) | Deferred, Nanite/Lumen | Forward+ clustérisé, Mobile, Compat | Forward clustérisé, deferred optionnel |
| BRDF PBR | 🟡 Lambert + Blinn-Phong paramétré metal/rough | ✅ | ✅ | ✅ | ✅ |
| Normal maps, IBL | ❌ | ✅ | ✅ | ✅ | ✅ |
| Ombres directionnelles | 🟡 1 shadow map 2048, PCF | ✅ cascades | ✅ VSM | ✅ cascades | ✅ cascades |
| Ombres ponctuelles/spots | ❌ | ✅ | ✅ | ✅ | ✅ |
| GI dynamique | ❌ | 🟡 (HDRP) | ✅ Lumen | ✅ SDFGI/VoxelGI | 🟡 expérimental |
| HDR + tonemap | ✅ ACES | ✅ | ✅ | ✅ | ✅ |
| Bloom | ✅ | ✅ | ✅ | ✅ | ✅ |
| SSAO, TAA, DoF, motion blur | ❌ | ✅ | ✅ | ✅ | ✅ (sauf DoF partiel) |
| MSAA | ✅ 4× | ✅ | 🟡 | ✅ | ✅ |
| Transparence triée | ❌ (tout en `REPLACE`) | ✅ | ✅ | ✅ | ✅ |
| Instancing | ✅ par lot mesh+texture | ✅ | ✅ | ✅ | ✅ automatique |
| Frustum culling | ✅ CPU | ✅ | ✅ | ✅ | ✅ |
| Occlusion culling | ❌ | ✅ | ✅ | ✅ | 🟡 |
| LOD de mesh | 🟡 impostor herbe à 40 m seulement | ✅ | ✅ Nanite | ✅ | 🟡 |
| Particules | ❌ | ✅ Shuriken, VFX Graph | ✅ Niagara | ✅ GPU | 🟡 crates tierces |
| Terrain | 🟡 primitive à hauteur codée, heightfield rapier | ✅ éditeur, splatmaps | ✅ Landscape | ✅ via plugins | ❌ |
| Ciel, brouillard | ✅ dégradé + exp. | ✅ | ✅ volumétrique | ✅ | ✅ |
| Skinning GPU | ✅ 128 joints, 256 instances | ✅ | ✅ | ✅ | ✅ |
| Compression textures | 🟡 BC3 desktop | ✅ toutes | ✅ toutes | ✅ | 🟡 |
| Shaders custom | 🟡 éditer le WGSL du moteur | ✅ Shader Graph, HLSL | ✅ Material Editor | ✅ Visual Shader, GDShader | ✅ WGSL par matériau |
| Profilage GPU | ✅ timestamps par passe | ✅ | ✅ | ✅ | 🟡 |

### 5.2 Scène, entités, données

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Modèle | `Vec<SceneObject>` plat | GameObject + Components (ou ECS) | Actor + Components | Arbre de nœuds | ECS pur |
| Hiérarchie parent/enfant | ❌ groupes par nom seulement | ✅ | ✅ | ✅ | ✅ |
| Composants composables | ❌ champs optionnels figés | ✅ | ✅ | ✅ nœuds | ✅ |
| Prefabs | ✅ JSON, uuid stable, overrides | ✅ variants, nesting | ✅ Blueprints | ✅ scènes instanciées | 🟡 (scènes, `bsn` en cours) |
| Format de scène | JSON versionné | YAML | binaire `.uasset` | texte `.tscn` | RON/`bsn` |
| Undo/redo | 🟡 snapshot ×50, pas l'inspecteur | ✅ | ✅ | ✅ | — |
| Projet / assets isolés | 🟡 manifeste, assets globaux | ✅ | ✅ | ✅ | ✅ |
| Migration de format | ✅ champ `version` | ✅ | ✅ | ✅ | 🟡 |

### 5.3 Éditeur

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Éditeur intégré | ✅ egui, macOS validé | ✅ | ✅ | ✅ | ❌ (chantier) |
| Hiérarchie, inspecteur, gizmos | ✅ multi-sélection, snap, pivot commun | ✅ | ✅ | ✅ | — |
| Play/Pause/Stop | ✅ avec restauration | ✅ | ✅ PIE | ✅ | — |
| Caméra éditeur | 🟡 orbite seulement | ✅ orbite + vol | ✅ | ✅ | — |
| Navigateur d'assets | ✅ + visualiseur GLB dédié | ✅ | ✅ | ✅ | — |
| Éditeur de scripts intégré | ✅ Lua + points d'arrêt | ❌ (IDE externe) | 🟡 Blueprints | ✅ GDScript | — |
| Hot-reload assets | ✅ desktop | ✅ | ✅ | ✅ | ✅ |
| Hot-reload code | 🟡 Lua oui, Rust non | ✅ C# (domain reload) | ✅ Live Coding | ✅ GDScript | ❌ |
| Aperçu mobile | ✅ cadre téléphone, tactile simulé | ✅ Device Simulator | 🟡 | 🟡 | — |
| Génération IA de scènes/scripts | ✅ DeepSeek, expérimental | 🟡 Muse | ❌ | ❌ | ❌ |
| Éditeur de terrain | ❌ | ✅ | ✅ | 🟡 plugins | ❌ |
| Console, profiler, diagnostic | ✅ | ✅ | ✅ | ✅ | 🟡 |
| Linux/Windows | ❌ non validé | ✅ | ✅ | ✅ | ✅ |

### 5.4 Scripting et extensibilité

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Langage de gameplay | Lua 5.4 (natif), Lua 5.1 (web) | C# | C++, Blueprints | GDScript, C#, C++ | Rust |
| Surface d'API exposée | 🟡 ≈ 30 symboles | ✅ énorme | ✅ énorme | ✅ large | ✅ tout le moteur |
| Débogueur | ✅ points d'arrêt Lua natif | ✅ | ✅ | ✅ | 🟡 (debugger Rust) |
| Scripting visuel | ❌ | 🟡 (Visual Scripting retiré, Behavior Graph) | ✅ Blueprints | ❌ (retiré) | ❌ |
| API de plugins / extensions | ❌ | ✅ packages, Editor scripting | ✅ plugins C++ | ✅ GDExtension, addons | ✅ `Plugin` trait |
| Modifier le moteur lui-même | ✅ trivial, 67 k lignes | 🟡 licence source payante | ✅ mais 20 M+ lignes | ✅ 1,5 M+ lignes | ✅ |
| Cohérence natif/web du langage | ✅ tests différentiels mlua/rilua | ✅ | — | ✅ | ✅ |

### 5.5 Physique

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Moteur | rapier3d | PhysX | Chaos | Godot Physics / Jolt | avian / rapier |
| Types de corps | ✅ Static, Dynamic, Kinematic | ✅ | ✅ | ✅ | ✅ |
| Colliders | ✅ Box, Sphere, Capsule, TriMesh, ConvexHull, Heightfield | ✅ | ✅ | ✅ | ✅ |
| Character controller | ✅ KinematicCharacterController, tuning game-feel | ✅ | ✅ | ✅ | 🟡 |
| Raycast, overlap | ✅ exposés en Lua | ✅ | ✅ | ✅ | ✅ |
| Joints | ❌ alloués, jamais peuplés | ✅ | ✅ | ✅ | ✅ |
| Triggers/sensors | 🟡 drapeau scène, pas de sensors rapier | ✅ | ✅ | ✅ | ✅ |
| Couches et masques | ✅ | ✅ | ✅ | ✅ | ✅ |
| Pas fixe + interpolation | ✅ 1/60, anti-spirale | ✅ | ✅ | ✅ | ✅ |
| Déterminisme | 🟡 RNG maison, pas de seed sauvegardé | 🟡 | 🟡 | 🟡 | 🟡 |

### 5.6 Animation

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Import squelettes glTF | ✅ | ✅ | ✅ | ✅ | ✅ |
| Blending | 🟡 crossfade 2 clips 0,2 s | ✅ blend trees | ✅ | ✅ AnimationTree | ✅ graphe |
| Machine à états | ❌ (Lua ou Rust) | ✅ Mecanim | ✅ | ✅ | 🟡 |
| IK | ❌ | ✅ | ✅ Control Rig | ✅ | ❌ |
| Anim notifies | ✅ `anim:<nom>` vers Lua | ✅ | ✅ | ✅ | 🟡 |
| Morph targets | ⚪ hors scope | ✅ | ✅ | ✅ | ✅ |
| Réplication réseau | ✅ nom du clip | 🟡 | ✅ | 🟡 | — |

### 5.7 Audio

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Moteur | kira | FMOD-like interne | MetaSounds | interne | `bevy_audio` (rodio) / kira |
| Bus musique/SFX, ducking | ✅ | ✅ mixer | ✅ | ✅ | 🟡 |
| Spatialisation | 🟡 distance + panning stéréo | ✅ 3D | ✅ 3D, HRTF | ✅ 3D | 🟡 |
| Streaming | ✅ natif, ❌ web | ✅ | ✅ | ✅ | 🟡 |
| Effets | ✅ reverb, compresseur, EQ | ✅ | ✅ | ✅ | ❌ |
| Randomisation pitch/volume | ✅ | 🟡 scripts | ✅ | ✅ | ❌ |

### 5.8 UI de jeu (HUD)

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Système | egui (mode immédiat) | UI Toolkit, UGUI | UMG, Slate | Control nodes | `bevy_ui` |
| Widgets déclaratifs éditables | 🟡 texte/image/jauge/bouton ancrés | ✅ | ✅ | ✅ | 🟡 |
| Thème, styles | ❌ (egui par défaut) | ✅ USS | ✅ | ✅ Theme | 🟡 |
| Tactile virtuel intégré | ✅ joystick + boutons | 🟡 asset | 🟡 | 🟡 | ❌ |
| Localisation | 🟡 `locale.rs` | ✅ | ✅ | ✅ | ❌ |

### 5.9 Réseau et multijoueur

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Réseau intégré au moteur | ✅ | 🟡 package Netcode | ✅ natif | ✅ haut niveau | ❌ crates tierces |
| Transport | WebSocket (natif + navigateur) | UDP (Unity Transport), WebSocket | UDP | ENet, WebSocket, WebRTC | selon crate |
| Autorité serveur clé en main | ✅ même `AppState` que le client | 🟡 à configurer | ✅ | 🟡 à construire | 🟡 lightyear |
| Serveur headless sans GPU | ✅ binaire dédié | ✅ | ✅ | ✅ | ✅ |
| Prédiction client | ✅ | 🟡 (NGO limité, NfE oui) | ✅ CharacterMovement | ❌ à la main | 🟡 |
| Interpolation | ✅ | ✅ | ✅ | 🟡 | 🟡 |
| Réconciliation | ✅ par historique de trajectoire 1 s | 🟡 | ✅ | ❌ | 🟡 |
| Sérialisation | bincode, ≈ 540 o / snapshot 20 entités | propre | propre, delta | propre | selon crate |
| Anti-abus | ✅ rate limit, taille de frame, max/IP | 🟡 | 🟡 | ❌ | ❌ |
| Salons, classes | ✅ codes, 4 modes, 3 classes | 🟡 services payants | 🟡 | 🟡 | ❌ |
| Matchmaking, liste de salons | ❌ | ✅ service | ✅ EOS | ❌ | ❌ |
| Multijoueur navigateur | ✅ vérifié | 🟡 WebGL + WebSocket | ❌ | ✅ | 🟡 |
| Backend annexe | Firebase RTDB (comptes, chat, classement) | UGS | EOS | ❌ | ❌ |
| Test de charge | ✅ `examples/load_test_client.rs` | ❌ | ❌ | ❌ | ❌ |
| iOS | ❌ | ✅ | ✅ | ✅ | ✅ |

### 5.10 Assets et pipeline de contenu

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Formats 3D | glTF/GLB seulement | FBX, OBJ, glTF, USD… | FBX, USD, glTF… | glTF, FBX, OBJ, DAE | glTF |
| Matériaux importés | ⚪ couleur de base seulement, DA « couleur par sommet » | ✅ complet | ✅ complet | ✅ complet | ✅ complet |
| Textures | PNG, JPEG | toutes | toutes | toutes | toutes |
| Import asynchrone | ✅ | ✅ | ✅ | 🟡 | ✅ |
| Génération procédurale d'assets | ✅ 60+ scripts Blender, 482 GLB | ❌ (Asset Store) | ❌ (Quixel, Fab) | ❌ | ❌ |
| Store d'assets | ❌ | ✅ énorme | ✅ Fab | ✅ Asset Library | 🟡 crates |
| Bundle embarqué, compression | ✅ include_dir + zstd | ✅ Addressables | ✅ pak | ✅ pck | 🟡 |
| Références stables (uuid) | ✅ `asset-id://` | ✅ GUID | ✅ | ✅ UID | 🟡 |
| Assets par projet | ❌ dossier global | ✅ | ✅ | ✅ | ✅ |

### 5.11 Plateformes et distribution

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| macOS | ✅ éditeur + `.dmg` | ✅ | ✅ | ✅ | ✅ |
| Windows, Linux | ❌ non validés | ✅ | ✅ | ✅ | ✅ |
| Android | ✅ APK signé | ✅ | ✅ | ✅ | ✅ |
| iOS | ✅ player, réseau non validé | ✅ | ✅ | ✅ | ✅ |
| Web | ✅ WebGPU, démo publique | 🟡 WebGL, WebGPU expérimental | ❌ | ✅ WebGL2 | ✅ WebGPU/WebGL2 |
| Consoles | ❌ | ✅ | ✅ | 🟡 prestataires | ❌ |
| XR | ❌ | ✅ | ✅ | ✅ | 🟡 |
| Taille du runtime | non documentée | ≈ 20–40 Mo min | ≈ 100 Mo+ | ≈ 40–90 Mo | ≈ 10–30 Mo |
| Scripts de packaging | ✅ 4 shells + doctor | ✅ intégré | ✅ intégré | ✅ intégré | ❌ |
| Release automatisée | ✅ tag → dmg + apk | 🟡 Cloud Build | 🟡 | 🟡 | ❌ |

### 5.12 Qualité, tests, pilotage

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Tests du moteur | ✅ 825 | ✅ internes | ✅ internes | ✅ internes | ✅ |
| Tests d'un projet de jeu | ✅ 47 d'intégration, goldens | 🟡 Test Framework | 🟡 Automation, Gauntlet | 🟡 GUT (tiers) | ✅ cargo test |
| Goldens de rendu | ✅ 5, headless | 🟡 | ✅ | 🟡 | 🟡 |
| Pilotage externe de l'éditeur | ✅ pont TCP JSON (`--pilot`) | 🟡 Editor scripting | 🟡 Python Editor | 🟡 `--script` | ❌ |
| Budget `unwrap`/`panic` en CI | ✅ | — | — | — | ❌ |
| Journal de crash | ✅ local, sans envoi | ✅ Cloud Diagnostics | ✅ | 🟡 | ❌ |
| Docs | ✅ 46 fichiers, mais en français uniquement | ✅ | ✅ | ✅ | ✅ |

### 5.13 Licence, coût, gouvernance

| Critère | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Licence | MIT | propriétaire | source disponible, royalties 5 % | MIT | MIT / Apache-2.0 |
| Coût | 0 | 0 sous plafond, puis par siège | 0 sous seuil, puis 5 % | 0 | 0 |
| Télémétrie | aucune | oui | oui | non | non |
| Gouvernance | 1 personne | société cotée | société privée | fondation | communauté |
| Facteur bus | 1 | — | — | — | — |
| Stabilité d'API | aucune garantie | LTS 2 ans | par version majeure | semver | ruptures trimestrielles |

---

## 6. Analyse détaillée domaine par domaine

### 6.1 Rendu : un forward propre, sans le socle PBR moderne

Le renderer est cohérent et bien tenu : cible HDR, MSAA avec repli, ombres
PCF, bloom multi-mips, ACES, instancing par lot, culling CPU, aucune
allocation par frame, timestamps GPU par passe. C'est un très bon rendu
« stylisé mobile » et il correspond exactement à la direction artistique
choisie (couleur par sommet, trois teintes par objet).

Mais il s'arrête là où les quatre autres commencent. Le shader `main.wgsl`
se décrit lui-même comme une « approximation PBR légère » : c'est du
Lambert + Blinn-Phong modulé par metallic/roughness, sans GGX, sans IBL,
sans normal map. Une seule shadow map pour toute la scène, sans cascade,
donne des ombres floues au loin ou crénelées de près dès que la carte
dépasse quelques dizaines de mètres. Il n'y a pas de passe transparente
(tous les pipelines sont en `BlendState::REPLACE`), pas de particules, pas
d'occlusion, pas de LOD général, et l'unique LOD est un impostor d'herbe
codé en dur. Aucun compute shader.

**Conséquence pratique.** Tout jeu qui a besoin d'eau, de verre, de fumée,
d'un feu, d'une pluie ou d'un effet de sort volumétrique doit soit tricher
avec des meshes opaques, soit ajouter ces passes au moteur. Pour « Le Hameau
des Braises », dont le pitch repose sur un feu qui attire les hordes,
l'absence de particules est le manque le plus visible.

**Où se situe l'écart.** Godot Mobile et Unity URP « Low » restent
au-dessus : PBR complet, cascades, particules GPU, transparence. Bevy est
loin devant sur le rendu pur. RusteeGear est comparable à un moteur de jam
soigné, pas à un moteur de production.

### 6.2 Scène et entités : le choix « pas d'ECS » est défendable, le choix « pas de hiérarchie » ne l'est plus

Le README revendique un `Vec<SceneObject>` lisible sans courbe
d'apprentissage. C'est vrai, et c'est un vrai avantage sur Bevy pour un
débutant. Mais le `SceneObject` a aujourd'hui une trentaine de champs
optionnels (`controller`, `combat`, `ai_chaser`, `bite`, `weapon_pickup`,
`item_pickup`, `convoy`, `wind`, `deadly`, `respawn_delay`…), dont une bonne
moitié sont des composants **de gameplay du Hameau** et non du moteur. Chaque
nouveau type de jeu ajoutera des champs. C'est le schéma classique du
« god object » qu'Unity a résolu avec les Components en 2005 et Godot avec
les nœuds.

Plus grave : **il n'y a pas de parent/enfant**. Les groupes sont des chaînes
de caractères pour la hiérarchie de l'éditeur. On ne peut pas attacher une
arme à une main, une lampe à un chariot, un enfant à un véhicule, sans
recalculer les transforms à la main dans un script. Les quatre moteurs
comparés ont une hiérarchie de transforms, y compris Bevy (`Parent`/
`Children` + `GlobalTransform`). Le système de `Convoy` du mode Escorte est
précisément un contournement de cette absence.

Les points forts sont réels : prefabs avec uuid stable et overrides
par instance, sérialisation JSON versionnée avec migration, manifeste de
projet avec contrôle de traversée de chemin. Le format texte est un choix
sain (Unreal en binaire est un cauchemar de merge).

### 6.3 Éditeur : étonnamment complet pour la taille, mais mono-plateforme

L'éditeur egui couvre l'essentiel d'un Godot ou d'un Unity de base :
hiérarchie avec drag-drop, inspecteur, gizmos multi-objets à pivot commun,
snap, aligner/distribuer, couper/coller, Play/Pause/Stop avec restauration,
navigateur d'assets, prefabs, éditeur de widgets HUD, éditeur de script Lua
**avec points d'arrêt**, console, profiler CPU/GPU/mémoire, diagnostic,
journal de crash, aperçu mobile avec simulateur tactile, et une génération
IA de scènes et scripts. Le visualiseur GLB dédié pour parcourir 482 modèles
est un outil que beaucoup de studios écrivent eux-mêmes.

Trois manques comptent. L'undo ne couvre pas l'inspecteur (documenté, roadmap
UX 5.1), ce qui est disqualifiant pour un utilisateur venant de Unity où tout
est annulable. La caméra éditeur est orbitale seulement, sans vol WASD, ce qui
rend la composition d'une grande carte laborieuse (le projet a d'ailleurs
contourné en composant dans Blender via MCP). Et l'éditeur n'est validé que
sur macOS : c'est le point qui exclut d'emblée 90 % des utilisateurs
potentiels de Godot ou Unity.

Le mode immédiat d'egui est un bon choix pour un éditeur d'outil ; il l'est
moins pour l'UI de jeu (voir 6.8).

### 6.4 Scripting : Lua bien intégré, surface d'API étroite, zéro extensibilité Rust

L'intégration Lua est de qualité : cache de chunks par hachage de contenu
(hot-reload gratuit), débogueur, variables réactives par objet
(`obj.tapped`, `obj.triggered`, `obj.anim`), événements (`emit`/`on_event`),
requêtes physiques (`raycast`, `overlap_sphere`), sauvegarde clé/valeur,
spawn de prefabs. Le port web sur `rilua` avec tests différentiels contre
`mlua` est une pratique d'ingénierie remarquable que ni Unity (IL2CPP vs
Mono) ni Godot n'appliquent aussi systématiquement.

Mais l'API compte une trentaine de symboles. Il n'y a pas d'accès à la
caméra, aux lumières, au ciel, à l'audio (hors `reverb`), au réseau, aux
autres objets autrement que par `find_tag`, ni de création d'objets
procéduraux hors prefab. Un script Unity a accès à tout le moteur ; un
script GDScript aussi. Ici, tout ce qui n'est pas prévu passe par une
modification du moteur en Rust, ce qui est facile (67 k lignes) mais
impossible pour un créateur non-développeur.

Il n'y a **aucune API de plugin Rust** : pas de trait `Plugin`, pas de
chargement dynamique, pas de registre de composants. C'est cohérent avec la
philosophie « on modifie le moteur », mais c'est ce qui empêche un
écosystème.

### 6.5 Physique : au niveau, avec deux trous

rapier3d est un moteur de production (utilisé par Bevy et de nombreux
jeux). L'intégration est sérieuse : trois types de corps, six formes,
heightfield, couches/masques, CCD par objet, pas fixe avec accumulateur
borné, `KinematicCharacterController` avec tuning game-feel documenté
(freinage, contrôle aérien, gravité de descente 1,6×). C'est comparable à
Godot avec Jolt.

Deux trous : les **joints** (les sets sont alloués et jamais remplis, donc
pas de portes, ponts, chaînes, ragdolls) et les **triggers** qui sont un
drapeau de scène interrogé par les scripts plutôt que de vrais sensors
rapier (donc pas de trigger entre deux objets non-joueur).

### 6.6 Animation : le minimum viable

Import glTF skinné correct, skinning GPU, crossfade entre deux clips,
notifies vers Lua (`anim:<nom>`, utilisées pour les fenêtres de coup au
combat), réplication du nom du clip. C'est suffisant pour « idle / run /
attack ».

Il manque tout ce qui fait un personnage crédible dans les quatre autres
moteurs : blend tree (marche ↔ course selon la vitesse), machine à états
visuelle, couches additives (viser en courant), IK des pieds, root motion,
morph targets (hors scope, documenté). Bevy 0.15+ a un graphe d'animation ;
Godot a `AnimationTree` ; Unity et Unreal sont des références du domaine.

### 6.7 Audio : plus riche qu'attendu

kira avec deux bus, crossfade musical, ducking, reverb pilotable en Lua,
compresseur, EQ, normalisation de gain, streaming, randomisation par
déclenchement. C'est au niveau d'un Godot et au-dessus de Bevy. La
spatialisation reste 2,5D (distance + panning stéréo, pas de HRTF ni
d'atténuation directionnelle), ce qui suffit pour une vue à la 3e personne.
Le streaming musical manque sur le web.

### 6.8 UI de jeu : egui est un compromis, pas une solution

Le HUD (1 700 lignes) fait beaucoup : barre de vie, vignette de dégâts,
réticule, armes, manches, inventaire, roster, pastille réseau, pause,
joystick tactile, minimap. Les widgets HUD déclaratifs ancrés dans la scène
et éditables sans code sont une bonne idée.

Mais egui est une UI d'outil : pas de thème de jeu, pas d'animation de
transition, pas de police custom simple, pas de layout responsive au sens
d'UI Toolkit ou de Godot Control, un style « boîte grise » immédiatement
reconnaissable. Aucun des quatre moteurs n'utilise une UI de mode immédiat
pour le HUD de jeu. C'est un plafond pour la qualité perçue.

### 6.9 Réseau : la partie où RusteeGear est réellement compétitif

C'est le domaine où l'écart est en faveur du projet, et de loin par rapport
à sa taille.

Le serveur headless exécute **exactement** la même `AppState` que le client :
scène, physique, combat, manches. Pas de code dupliqué, pas de « logique
serveur » à part. Unreal fait cela nativement (le même `UWorld` en mode
dédié). Unity et Godot le permettent mais laissent le développeur assembler
autorité, prédiction et réconciliation. Bevy n'a rien intégré.

Le client fait prédiction locale, interpolation des entités distantes avec
délai de rendu, et une réconciliation par **historique de trajectoire**
(une position serveur située sur le chemin prédit est simplement « en
retard », pas une erreur) avec convergence douce au repos. C'est plus
élaboré que Netcode for GameObjects et que le `MultiplayerSynchronizer`
de Godot. Le serveur valide mouvement, cooldowns, armes, soins ; limite
débit, taille de frame, connexions par IP ; gère 16 salons.

Trois limites. Le transport est **WebSocket uniquement** (TCP), donc
head-of-line blocking sous perte de paquets ; Unity, Unreal et Godot
utilisent UDP pour le gameplay et gardent WebSocket pour le web. Il n'y a
**pas de matchmaking ni de liste de salons** (deux groupes peuvent prendre
le même code). Et le client iOS n'est pas validé.

### 6.10 Assets : un pipeline unique en son genre, mais fermé

Les 482 GLB sont générés par 60+ scripts Blender headless suivant une charte
stricte. Aucun autre moteur ne livre un pipeline procédural d'assets ; ils
livrent un store. C'est à la fois une force (reproductibilité, cohérence
visuelle, zéro problème de droits) et une limite (un créateur ne peut pas
importer un modèle texturé du marché : seule la couleur de base est lue,
décision documentée).

La gestion d'assets est saine (uuid stables, bundle zstd décompressé en
pur Rust pour le wasm, vérification d'orphelins en CI) mais les assets
restent globaux dans `~/.motor3derust/assets/`, partagés entre projets.
C'est un blocage pour tout usage à plusieurs projets.

### 6.11 Plateformes : la couverture est la vraie surprise

macOS, Android, iOS et web **avec multijoueur navigateur** depuis un même
crate, en 11 semaines, par une personne : c'est plus large que ce que Bevy
propose clé en main (pas de packaging) et comparable à Godot. L'export web
en WebGPU est en avance sur Godot (WebGL2) et Unity (WebGPU expérimental).
Le pipeline release (tag → dmg + apk sur GitHub Release, Pages pour la démo
et la doc) est propre.

Les trous : pas de Windows ni Linux validés (l'éditeur est donc macOS-only
de fait), pas de consoles, pas de XR, tailles de binaires non documentées.

### 6.12 Qualité et pilotage : au-dessus du lot

825 tests, dont des goldens de rendu headless, des tests de mode Play, des
tests de projet, des tests à socket réel, des tests différentiels
Lua natif/web. Un budget d'`unwrap`/`expect`/`panic` vérifié en CI. Un
vérificateur d'orphelins de bundle. Un toolchain épinglé avec les leçons
documentées dans le fichier lui-même.

Le pont `--pilot` mérite une mention à part : un canal TCP JSON local qui
permet à un agent, un script ou une CI de lire l'état, envoyer des entrées,
avancer la simulation pas à pas, capturer l'écran et évaluer du Lua dans
l'éditeur en marche. Il a été construit parce que winit/wgpu n'exposent pas
d'arbre d'accessibilité. Unity a l'Editor scripting et Unreal un Python
éditeur, mais aucun ne fournit un protocole externe de ce type prêt à
l'emploi. Pour un développement assisté par agent, c'est un avantage
décisif.

Faiblesse : la documentation, riche (46 fichiers), est en français
uniquement et mélange guides d'entrée et archéologie de sprints. Un
utilisateur non francophone n'a aucune porte d'entrée.

---

## 7. Ce que RusteeGear fait que les autres ne font pas

| Capacité | Détail | Comparable le plus proche |
|---|---|---|
| Même simulation solo, client et serveur, sans GPU | `src/bin/server.rs` instancie l'`AppState` de l'éditeur | Unreal (serveur dédié) |
| Réconciliation par historique de trajectoire | `src/net/interpolation.rs`, 1 s de chemin prédit | Aucun clé en main |
| Multijoueur navigateur en WebGPU sur le même serveur que le desktop | `src/net/client/web.rs`, démo publique | Godot (WebGL2) |
| Pont de pilotage externe de l'éditeur | `src/pilot.rs`, `docs/PILOT.md` | Aucun |
| Tests différentiels du langage de script natif/web | `scripting_web::tests` contre `mlua` | Aucun |
| Budget de panics en CI | `scripts/check_unwrap_budget.py` | Aucun |
| Pipeline d'assets procédural reproductible | `scripts/blender/gen_*.py` → 482 GLB | Aucun |
| Génération IA de scènes et scripts intégrée | `src/app/ai.rs`, DeepSeek | Unity Muse (payant) |
| Moteur entier lisible par une personne | 67 k lignes hors tests, 12 modules | Aucun des quatre |
| Zéro télémétrie, zéro licence, zéro runtime caché | MIT, binaire natif unique | Godot, Bevy |

---

## 8. Radar de maturité

Notes sur 5, pour l'usage « petit studio ou solo qui veut livrer un jeu ».
Elles mesurent l'utilité pratique, pas la quantité de fonctionnalités.

| Dimension | RusteeGear | Unity | Unreal | Godot | Bevy |
|---|---|---|---|---|---|
| Rendu | 2 | 5 | 5 | 4 | 4 |
| Modèle de scène | 2 | 5 | 4 | 5 | 4 |
| Éditeur | 3 | 5 | 5 | 5 | 0 |
| Scripting | 3 | 5 | 5 | 5 | 4 |
| Physique | 4 | 5 | 5 | 4 | 4 |
| Animation | 2 | 5 | 5 | 4 | 3 |
| Audio | 3 | 4 | 5 | 4 | 2 |
| UI de jeu | 2 | 5 | 5 | 5 | 2 |
| Réseau | 4 | 3 | 5 | 3 | 1 |
| Assets et pipeline | 3 | 5 | 5 | 4 | 2 |
| Plateformes | 3 | 5 | 4 | 4 | 3 |
| Tests et pilotage | 5 | 3 | 3 | 2 | 3 |
| Lisibilité, contrôle | 5 | 1 | 2 | 3 | 3 |
| Licence, coût | 5 | 3 | 3 | 5 | 5 |
| Écosystème, communauté | 0 | 5 | 5 | 5 | 4 |
| Stabilité, pérennité | 1 | 4 | 5 | 4 | 2 |
| **Moyenne** | **2,9** | **4,3** | **4,4** | **4,1** | **2,9** |

Lecture : RusteeGear et Bevy ont la même moyenne pour des raisons opposées.
Bevy est excellent sur le rendu et l'ECS mais n'a ni éditeur ni réseau ;
RusteeGear a un éditeur et un réseau mais un rendu et un modèle de scène de
prototype. Les trois moteurs grand public sont homogènes au-dessus de 4.

---

## 9. Positionnement : quand choisir quoi

| Situation | Choix conseillé | Pourquoi |
|---|---|---|
| Petit jeu coop 2-16 joueurs, mobile + web, une personne, style low-poly coloré | **RusteeGear** | Serveur autoritaire inclus, export web/mobile, code maîtrisé, aucun coût |
| Prototype de jeu en jam, en Rust, sans réseau | **Bevy** | Rendu bien meilleur, écosystème de crates, pas d'éditeur nécessaire en jam |
| Jeu 3D solo ou multi, équipe de 1 à 5, budget nul, visuel soigné | **Godot** | Éditeur mûr, PBR, particules, animation, Windows/Linux, MIT |
| Jeu mobile commercial avec store d'assets, monétisation, analytics | **Unity** | Écosystème, services, consoles, recrutement |
| Jeu AAA ou visuel photoréaliste, équipe > 10 | **Unreal** | Nanite, Lumen, réplication native, outils d'équipe |
| Expérience pédagogique « comprendre comment marche un moteur » | **RusteeGear** | Chaque étage est écrit à la main et documenté |
| Développement assisté par agent IA avec vérification automatique | **RusteeGear** | Pont `--pilot`, tests headless, goldens |
| Jeu nécessitant eau, feu, fumée, verre, foule, grand monde ouvert | Godot, Unity ou Unreal | RusteeGear n'a ni particules, ni transparence, ni LOD, ni streaming |
| Équipe sur Windows ou Linux | Tout sauf RusteeGear | Éditeur validé sur macOS uniquement |

---

## 10. Risques et dette structurelle

Classés par gravité pour l'avenir du moteur, pas pour le jeu actuel.

1. **Facteur bus égal à 1 et aucune communauté.** Aucun des quatre moteurs
   comparés ne dépend d'une seule personne. Sans au moins une documentation
   d'entrée en anglais et une API de plugins, le projet reste un moteur
   personnel.
2. **`SceneObject` en god object.** Trente champs optionnels, dont la
   moitié spécifiques au Hameau. Chaque nouveau genre de jeu alourdit le
   struct, la sérialisation, l'inspecteur et le protocole réseau. Le README
   présente cela comme une vertu ; c'est vrai jusqu'à ≈ 20 champs, plus
   maintenant.
3. **Absence de hiérarchie de transforms.** Contournée par `Convoy` et par
   des scripts. Plus la base de scènes grossit, plus la migration coûtera.
4. **egui pour le HUD de jeu.** Plafond de qualité visuelle et de
   personnalisation. Un remplacement plus tard imposera de réécrire 1 700
   lignes de HUD.
5. **Éditeur macOS-only.** wgpu/winit devraient compiler ailleurs, mais rien
   n'est validé. Chaque semaine sans CI Windows/Linux augmente le risque de
   régressions silencieuses (chemins, presse-papiers, GPU).
6. **Transport WebSocket pour le gameplay natif.** Acceptable à 16 joueurs
   sur fibre ; problématique sur mobile en 4G avec perte de paquets. UDP
   (ou WebTransport côté web) est la norme chez les quatre autres.
7. **Assets globaux entre projets.** Bloquant pour un deuxième jeu sur le
   moteur.
8. **Format d'API Lua non versionné.** Rien ne garantit qu'un script écrit
   aujourd'hui tourne dans six mois.
9. **Dépendance à `rilua` 0.1.x** pour tout le player web, crate jeune
   épinglée. Un abandon amont laisserait le web sans Lua.
10. **Documentation en français seulement, mélangée à l'historique.** Un
    tiers des 46 fichiers de `docs/` sont des audits et sprints.

---

## 11. Recommandations : quoi emprunter à qui

Organisées par horizon. Chaque ligne dit ce qu'on emprunte, à quel moteur, et
ce que ça débloque.

### Court terme (un à deux mois, sans changer l'architecture)

| Emprunt | Modèle | Débloque |
|---|---|---|
| Passe transparente triée (alpha blend, tri arrière-avant) | Tous | Eau, verre, fantômes, fondu de mort |
| Ombres en cascade (2 à 3 cascades) | Godot, Bevy | Grandes cartes sans ombres floues |
| Caméra éditeur en vol WASD + clic droit | Unity, Godot | Composer sans passer par Blender |
| Undo de l'inspecteur (déjà en roadmap UX 5.1) | Unity | Confiance de l'utilisateur |
| Blend 1D vitesse → idle/marche/course | Godot `AnimationTree` | Personnages crédibles avec trois clips |
| Joints rapier exposés dans l'inspecteur (fixe, révolution, sphérique) | Godot | Portes, ponts, pendules |
| Sensors rapier pour les triggers, entre tout couple d'objets | Tous | Pièges, zones, plaques de pression sans joueur |
| Validation CI Windows et Linux du build éditeur | Tous | 90 % des utilisateurs potentiels |
| `README.md` et `QUICKSTART.md` en anglais | Tous | Une porte d'entrée pour le monde |

### Moyen terme (trois à six mois, une refonte ciblée)

| Emprunt | Modèle | Débloque |
|---|---|---|
| Hiérarchie parent/enfant avec `GlobalTransform` calculé | Godot, Bevy | Attachements, véhicules, armes en main, fin de `Convoy` |
| Composants comme `Vec<Component>` typé par enum, hors du `SceneObject` | Unity, Godot | Extraire `combat`, `ai_chaser`, `bite`, `convoy`… du moteur vers le jeu |
| Particules GPU simples (émetteur, billboard, gravité, couleur sur durée) | Godot `GPUParticles3D` | Feu, fumée, sorts, pluie |
| PBR réel : GGX + Schlick + IBL depuis le ciel dégradé | Bevy `StandardMaterial` | Métaux crédibles, cohérence des matériaux importés |
| Assets par projet avec index | Unity, Godot | Deuxième jeu sur le moteur |
| UDP (QUIC ou `webrtc-unreliable`) pour le gameplay natif, WebSocket gardé pour le web | Unity Transport, Godot ENet | Mobile en réseau dégradé |
| Liste de salons publique et verrou sur les codes | Unity Lobby | Fin du conflit de codes |
| Machine à états d'animation déclarative en JSON, pilotable en Lua | Unity Mecanim | Moins de Lua pour les personnages |

### Long terme (six mois et plus, choix stratégiques)

| Emprunt | Modèle | Débloque |
|---|---|---|
| API de plugins Rust (trait `EnginePlugin` : systèmes, composants, panneaux d'éditeur) | Bevy `Plugin`, Godot GDExtension | Un écosystème ; les composants du Hameau deviennent un plugin |
| UI de jeu retenue avec thèmes (crate dédiée ou `bevy_ui`-like maison), egui gardé pour l'éditeur | Godot Control, Unity UI Toolkit | Qualité perçue du HUD |
| Normal maps et textures sur les GLB importés (option, DA conservée par défaut) | Tous | Ouverture aux assets du marché |
| Navmesh et pathfinding (crate `oxidized_navigation` ou `recast` port) | Unity NavMesh, Godot Navigation | IA qui contourne les murs |
| Versionnage de l'API Lua avec tests de compatibilité | Godot (semver) | Scripts pérennes |
| LOD général par distance sur `Imported` | Godot, Unity | Cartes plus grandes |

### Ce qu'il ne faut PAS emprunter

- **Un ECS.** Le positionnement « pas d'ECS » est le bon pour la cible.
  Une hiérarchie et des composants extraits suffisent.
- **Un pipeline de rendu différé ou une GI dynamique.** Hors cible mobile et
  web, et incompatible avec la direction artistique.
- **Un store d'assets.** Le pipeline Blender procédural est un différenciateur.
- **Un format de scène binaire.** Le JSON versionné est un avantage de merge.
- **La télémétrie.** L'absence en est un argument de vente.

---

## 12. Annexes

### 12.1 Chiffres bruts relevés

| Mesure | Valeur | Source |
|---|---|---|
| Lignes Rust dans `src/` | 79 557 | `wc -l` |
| Lignes de tests dans `src/` | ≈ 12 000 | modules `*_tests.rs` et `tests.rs` |
| Fonctions `#[test]` | 778 dans `src/`, 47 dans `tests/` | grep |
| Fichiers `.rs` | 118 | find |
| Shaders WGSL | 8 | `src/gfx/shaders/` |
| Dépendances directes / totales | 47 / 519 | `Cargo.toml`, `Cargo.lock` |
| Fichiers GLB | 482, ≈ 174 Mo | `assets/models/`, LFS |
| Scripts Blender | 60+ | `scripts/blender/` |
| Docs | 46 fichiers `.md` dans `docs/` | ls |
| Commits | 619 depuis le 18 juin 2026 | `git log` |
| Lumières ponctuelles max | 8 (2/4/8 selon qualité) | `src/scene/mod.rs`, `src/app/build_config.rs` |
| Shadow map | 2048², PCF 5×5 | `src/gfx/renderer/types.rs` |
| Instances skinnées max | 256, 128 joints | `src/gfx/renderer/types.rs` |
| Tick serveur | 16 ms | `src/bin/server.rs` |
| Protocole réseau | v7, bincode | `src/net/protocol.rs` |
| Salons / connexions | 16 salons, 256 connexions, 4 par IP | `src/bin/server.rs` |
| Snapshot 20 entités | ≈ 540 octets | doc inline |
| Undo | 50 entrées | `src/app/selection.rs` |

### 12.2 Documents du dépôt utilisés

- `README.md` : vision, fonctionnalités, comparaison Bevy, réseau.
- `docs/architecture.md` : boucle principale, simulation, rendu, scène, réseau.
- `docs/KNOWN_LIMITATIONS.md` : matrice de support, limites volontaires.
- `analysedev.md` : audit holistique du 19 juillet 2026.
- `GDD_MMORPG.md` : vision du jeu et exclusions de scope.
- `docs/PILOT.md`, `docs/LUA_PORTABLE.md`.
- `docs/roadmapaudit3septembre.md`, `docs/roadmapauditUX4septembre.md`.

### 12.3 Réserves sur les moteurs tiers

Les capacités attribuées à Unity, Unreal, Godot et Bevy reflètent leurs
versions courantes à la date de rédaction telles que connues de l'auteur ;
les numéros de version, les seuils de licence et l'état des fonctionnalités
expérimentales (WebGPU Unity, GI Bevy, Jolt Godot, éditeur Bevy) évoluent
vite et doivent être re-vérifiés sur les sites officiels avant toute
reprise dans un document externe. Aucun benchmark de performance croisé n'a
été réalisé.
