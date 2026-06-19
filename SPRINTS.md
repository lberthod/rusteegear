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

## Sprints à venir — alignés sur l'analyse

Ces sprints sont conçus pour **implémenter tous les points importants** relevés dans
[ANALYSE.md](ANALYSE.md) (§2 audit, §4 possibilités futures). Chaque ligne pointe vers
la recommandation qu'elle adresse.

| # | Sprint | Objectif | Couvre (réf. analyse) | État |
|---|---|---|---|---|
| 35b | Pipeline d'assets (fin) | Gestionnaire `asset://`, **import mobile (P10)**, fusion de meshes statiques | Audit P10 · §4 import mobile | ⬜ |
| 36 | Robustesse & boucle de simulation | Propager les `Result` d'init GPU/fenêtre + `log::error!` (anti-crash mobile) ; **découpler simulation et rendu** (boucle de mise à jour séparée, pas de temps fixe physique) | Audit P4 · « simulation pilotée par le rendu » · §2 reco 1–2 | ⬜ |
| 37 | Validation device & tests | Valider sur appareil réel les chaînes récentes (**PBR, instancing, resume**, joystick→script→APK) ; **étendre les tests** (matériaux, round-trip sérialisation, culling) | §2 reco 3–4 · HANDOFF Sprint 28 | ⬜ |
| 38 | Distribution signée | Override identité Android, **IPA signé en CI** (secrets), notarisation macOS, 3 artefacts signés par tag | Audit §6 · §4 distribution store | ⬜ |
| 39 | Rendu : finitions | **Ombres** câblées (`shadow.wgsl`), **textures** PBR (normal/metallic-roughness maps), post-process (bloom emissive) | §4 ombres/textures/rendu | ⬜ |
| 40 | Cible WebGPU / WASM | Export navigateur (`wgpu` le supporte déjà) — fort impact démonstration | §4 moyen terme | ⬜ |
| 41 | IA avancée & confort | IA « Ajouter à la scène » + édition ciblée, historique de prompts, glisser-déposer hiérarchie, gizmo multi rotate/scale | §4 IA · confort d'édition | ⬜ |
| 42 | (option) ECS léger | Migrer `Vec<SceneObject>` → entités typées si la complexité le justifie (à arbitrer contre la lisibilité) | §4 long terme | ⬜ |

---

## Correspondance analyse → sprint

| Point d'analyse | Sprint cible |
|---|---|
| P4 — panics d'init (crash mobile) | 36 |
| P10 — import d'assets sur mobile | 35b |
| Simulation pilotée par la cadence de rendu | 36 |
| Validation bout-en-bout sur device (PBR/instancing/resume) | 37 |
| Couverture de tests à étendre | 37 |
| Ombres (shadow mapping) | 39 |
| Textures PBR | 39 |
| Distribution store signée | 38 |
| WebGPU / WASM | 40 |
| IA approfondie | 41 |
| ECS léger | 42 (option) |
