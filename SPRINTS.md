# RusteeGear — Récapitulatif des sprints

> Vue d'ensemble **condensée** de tout l'historique d'exécution, du MVP jusqu'à l'état
> actuel, puis des sprints **à venir** dérivés de l'[analyse](ANALYSE.md).
> Le détail (objectif · tâches · fichiers · livrable · risques) reste dans
> [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md). Audit complet : [AUDIT.md](AUDIT.md).
>
> Légende : ✅ fait · 🟢 cœur fait (finitions reportées) · ⬜ à faire.

---

## Sprints réalisés (MVP → Sprint 35)

| # | Phase | Sprint | Apport principal | État |
|---|---|---|---|---|
| 0–6 | MVP | Moteur + éditeur + `.dmg` | Fenêtre winit, rendu wgpu, scène, primitives, export `.dmg` | ✅ |
| 7 | A | Refactor App/Renderer/Scene | Séparation logique (sans GPU) / rendu — socle du portage | ✅ |
| 8 | A | Gizmos souris | Translate / rotate / scale (W/E/R) | ✅ |
| 9 | A | Import glTF/GLB | Chargement asynchrone + recadrage auto | ✅ |
| 10 | A | Undo/Redo + duplication | Historique d'édition, Cmd+D | ✅ |
| 11 | B | Scripting Lua | `mlua` par objet (`obj.x/y/z…`, `dt`, `time`) | ✅ |
| 12 | B | Physique | `rapier3d` : statique/dynamique, gravité, collisions | ✅ |
| 13 | B | Audio | `kira` : son par objet, autoplay, décodage async | ✅ |
| 14 | C | Mode Player plein écran | Base mobile, scène jouable sans panneaux | ✅ |
| 15 | C | Entrées tactiles | 1 doigt orbit, 2 doigts pinch-zoom | ✅ |
| 16 | C | Build & signature iOS | `.ipa` signé, installé sur iPhone | 🟢 |
| 17 | C | Build Android | `cdylib` + `android_main`, `.apk` signé (cargo-apk) | ✅ |
| 18 | D | Profils build & app dev | Profils Cargo (LTO), bandeau FPS/GPU, cadence adaptative | ✅ |
| 19 | D | Panneau Build & Export | Export depuis l'éditeur, log streamé, pré-vol | ✅ |
| 20 | D | Config build persistée | Nom, bundle id, version, build # éditables | ✅ |
| 21 | D | Export APK 1-clic | Pré-vol, install device, révéler dossier | ✅ |
| 22 | D | Export IPA 1-clic | Signature configurable | ✅ |
| 23 | D | Presets & CI release | « Tout exporter », tag `v*` → artefacts attachés | ✅ |
| 24 | E | Assets embarqués | glTF + sons dans le player (`include_dir`, `bundle://`) | ✅ |
| 25 | E | Édition avancée | Multi-sélection, copier/coller, renommage inline | 🟢 |
| 26 | E | Matériaux & lumière | Couleur par objet + éclairage de scène éditable | 🟢 |
| 27 | E | Cycle de vie mobile | Resume (recréation surface), identité bundle macOS | 🟢 |
| 28 | F | Validation bout-en-bout | Filets de test, validation desktop | 🟢 |
| 29 | F | Édition complète | Multi-sélection 3D, gizmo multi-translate, réordonnancement | 🟢 |
| 30 | F | Ombres & textures | Shadow mapping directionnel + albédo texturé | 🟢 |
| 32 | F | Outils produit & menus pro | Barre de menus, console, profiler, Readiness Check APK, contrôles tactiles | 🟢 |
| 33 | — | Matériaux PBR & rendu avancé | PBR par objet (metallic/roughness/emissive), frustum culling CPU, rendu instancié | ✅ |
| 34 | — | Lumières & caméras | Lumières ponctuelles multiples (max 8), caméra de jeu définie par la scène | ✅ |
| 35 | — | Pipeline assets & opti mobile | Panneau « Optimisation mobile » : réduction réelle des textures, limite de lumières | 🟢 |

> Le **Sprint 31** (distribution complète) a été reporté et fusionné dans le Sprint 36 ci-dessous.

---

## Sprints à venir — maturité & robustesse (Sprints 36–37)

Dérivés de l'[analyse](ANALYSE.md) (§2 audit, §4 possibilités futures).

| # | Sprint | Objectif | Couvre (réf. analyse) | État |
|---|---|---|---|---|
| 36 | Distribution signée & validation device | Override identité Android, **IPA signé en CI** (secrets), notarisation macOS ; valider sur appareil réel (**PBR, instancing, resume**, joystick→script→APK) | Audit §6 · §2 reco 3 · §4 distribution | 🟢 |
| 37 | IA avancée & confort d'édition | IA « Ajouter à la scène » + édition ciblée, historique de prompts, glisser-déposer hiérarchie, gizmo multi rotate/scale | §4 IA · confort d'édition | 🟢 |

> **Transversal (à intégrer dans 36–37 ou en sprint dédié)** : **découpler simulation et
> rendu** (boucle de mise à jour séparée, pas de temps fixe physique), **durcir l'init**
> (`Result` + `log::error!` anti-crash mobile, audit P4), **étendre les tests**, et lever
> **P10** (import d'assets mobile). Pistes plus lointaines : **WebGPU/WASM**, **ECS léger**.

---

## PHASE G — Éditeur produit orienté Android (Sprints 38–42)

Objectif : atteindre l'**UI cible** d'un éditeur 3D Rust orienté export Android natif.
Promesse produit : *créer une scène → ajouter des contrôles mobiles → exporter un APK →
tester sur téléphone*. Détail complet dans [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md).

| # | Sprint | Apport principal | État |
|---|---|---|---|
| 38 | Menus complets & barre du haut | Fichier (Ouvrir/Sauver sous/Exporter APK/Paramètres projet…), Édition (Couper/Coller/Sélectionner tout/Grouper), toolbar (Pause/Stop/Snap/Grid/2D-3D/Build APK/Run Device), Aide (Guide APK, Diagnostic système) | 🟢 |
| 39 | Build Panel Android | Fenêtre dédiée : Application (nom/package/version/orientation/SDK/icône/splash), Rendu (Vulkan/qualité/FPS/ombres/MSAA), Assets (compression/nettoyage), Signature (debug/release), Actions (Build/Install/Run/Logs ADB) + Readiness Check enrichi | 🟢 |
| 40 | Menu Ajouter complet | Objet 3D (+ Terrain), Lumière (dir/point/spot/ambient), Caméra (principale/mobile), Physique (rigidbody/colliders/trigger), Audio (source/listener), UI (texte/bouton/joystick mobile/zone tactile/barre de vie) | 🟢 |
| 41 | Composants inspecteur mobiles | Mesh Renderer, Material, Mobile Touch Area + composants Android : Input Receiver, Touch Button, Virtual Joystick, Gyroscope, Vibration Feedback, Screen Safe Area | 🟢 |
| 42 | Menu Outils & optimisation mobile | Gestionnaire d'assets, Profiler mémoire, Gestionnaire scripts Lua, Bake lighting, Convertisseur textures ; Optimisation mobile complète (fusion meshes, LOD, occlusion culling, Mode performance Android) | 🟢 |

> **Déjà acquis (Sprints 32–35)** : barre de menus, console, profiler FPS, **APK Readiness
> Check**, contrôles tactiles, PBR, lumières multiples, caméra de jeu, réduction de textures.
> La Phase G **complète** ces briques jusqu'à l'UI cible.

**Boucle produit visée (sans ligne de commande) :**

```
Créer scène → Ajouter objets → Ajouter caméra → Ajouter joystick mobile
→ Build Panel Android → APK Readiness Check → Build APK → Installer & lancer sur téléphone
```

---

## Correspondance analyse / vision → sprint

| Point | Sprint cible |
|---|---|
| Distribution store signée (Android/iOS/macOS) | 36 |
| Validation device (PBR / instancing / resume) | 36 |
| P4 — panics d'init (crash mobile) | transversal (36) |
| P10 — import d'assets sur mobile | transversal / 42 (gestionnaire d'assets) |
| Simulation pilotée par la cadence de rendu | transversal |
| Couverture de tests à étendre | transversal (36) |
| IA approfondie + confort d'édition | 37 |
| Menus & toolbar produit | 38 |
| Build Panel Android (fenêtre dédiée) | 39 |
| Menu Ajouter complet (UI mobile) | 40 |
| Composants mobiles (gyroscope/vibration/safe area) | 41 |
| Optimisation mobile (LOD / occlusion / mode perf) | 42 |
| Ombres / textures PBR / WebGPU / ECS | pistes (voir ANALYSE §4) |
