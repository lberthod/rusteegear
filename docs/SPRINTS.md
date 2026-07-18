# RusteeGear — Récapitulatif des sprints

> Vue d'ensemble **condensée** de tout l'historique d'exécution, du MVP jusqu'à l'état
> actuel, puis des sprints **à venir**.
> Le détail (objectif · tâches · fichiers · livrable · risques) reste dans
> [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md).
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

## Sprints 36–37 — maturité & robustesse 🟢 (cœur livré)

Maturité & robustesse.

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

## PHASE H — Jouabilité mobile sans script & performance (Sprints 43–44) ✅

Objectif : rendre un objet **jouable au doigt sans écrire de Lua**, et alléger le
chemin de rendu. Détail dans [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md).

| # | Sprint | Apport principal | État |
|---|---|---|---|
| 43 | Contrôleur de personnage sans script | Composant **Input Receiver** (joystick → corps dynamique rapier, rotations bloquées, **collisions**), **saut** sur bouton tactile, **caméra qui suit** l'objet pilotable, **actions au tap** (changer couleur / masquer-ramasser), démo + JSON pré-généré, récap « scène embarquée » du Build Panel | ✅ |
| 44 | Optimisations rendu | **Culling/LOD des lumières** par distance caméra (8 plus proches), **0 allocation/frame** (tampons réutilisés), **re-tri d'ordre paresseux**, **plan de dessin par index** (0 clone de texture/frame) | ✅ |
| 43+ | Bonus jouabilité (mini-jeu) | Actions au tap **Grandir / Respawn** ; **collectibles** (gemmes tournantes) avec **score + chrono + « Gagné en X.Xs »** ; **zones mortelles** → **« Perdu »** (boucle win/lose, sans script) | ✅ |

---

## PHASE I — robustesse & découplage (Sprints 45–49)

Ce qui restait pour passer d'un **éditeur-produit jouable** à une **base robuste et distribuable**.

| # | Sprint | Objectif | Couvre | État |
|---|---|---|---|---|
| 45 | **Découpler simulation & rendu** | Boucle de mise à jour à **pas fixe** (1/60 s) pour la physique/scripts, indépendante du framerate (accumulateur + cap), testée 30/60/120 FPS | 🔴 P-rendu/sim | ✅ |
| 46 | **Durcir l'initialisation** | Init GPU/fenêtre/resume entièrement sur `Result` + `log::error!` ; caps de surface vides gérées ; **code de production sans `unwrap()`/`expect()`** | 🟠 Audit P4 | ✅ |
| 47 | **Tests étendus & skip-rebuild** | Couverture élargie (saut/collision contrôleur, round-trip composants, défauts) + **skip-rebuild du plan de dessin par hash des entrées** (sûr par construction : hash identique ⇒ sortie identique) | 🟡 perf + tests | ✅ |
| 48 | **Capteurs & assets mobiles** | **Gyroscope natif Android** (capteur réel → `tilt`), **vibration native**, **import d'assets sur mobile** (lever P10) | 🟠 P10 + mobile | ⬜ |
| 49 | **Distribution signée** | **IPA signé en CI** (secrets), **notarisation macOS**, signature *distribution* store (Android/iOS) | 🟢 distribution | ⬜ |

---

## 🌐 Multijoueur en ligne (Sprints 50–82, + réseau 80/82) ✅ cœur livré

VPS + WebSocket + Firebase (backend annexe) ; détail complet dans
[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md) § « Multijoueur en ligne ». ⚠️ Les numéros **80** et
**82** existent **deux fois** (une fois côté « réseau », une fois côté PHASE K solo) — le ROADMAP
précise le tronc à chaque occurrence, s'y référer en cas de doute.

| Sprints | Apport principal | État |
|---|---|---|
| 50 | Extraction du gameplay combat de `app/mod.rs` | ✅ |
| 51–53 | Serveur headless, protocole & sérialisation, transport WebSocket | ✅ |
| 54–55 | Prédiction client & interpolation, salons (lobby) | 🟢 (câblage UI au Sprint 63) |
| 56–59 | Comptes/auth, inventaire persistant, chat/présence, classement (Firebase) | 🟢 (backends faits, quelques écrans/temps réel différés) |
| 60–61 | Durcissement réseau & anti-triche de base, tests de charge & bande passante | ✅ |
| 62 | Déploiement serveur | ⬜ (infra) |
| 63–65 | Client réseau desktop (fenêtre Multijoueur), chat en jeu, classement en jeu | ✅ |
| 66–68 | Lissage réconciliation joueur local, délai d'interpolation fantômes, plafond débit `Input` | ✅ |
| 69 | Vérification géographique du serveur de test | ⬜ (infra, pas code) |
| 70 | Cohérence doc/code du `Snapshot` | ✅ |
| 71 | Transport non-TCP | ⬜ (conditionnel, non déclenché) |
| 72–77 | Interpolation pas fixe, game feel, réconciliation trajectoire, axes W/S, boutons tactiles/gyro réseau, rattrapage doux + VPS aligné | ✅ |
| 78–79 | Boule de feu + monstres sur la carte multi, visée réelle/multi-armes/changement d'arme | ✅ |
| 80 (réseau) / 82 (réseau) | Vie individualisée + IA multi-cibles + soin coopératif, multi-salons | ✅ |

---

## PHASE K — Filet de sécurité (Sprints 80–83) ✅

| # | Sprint | Apport principal | État |
|---|---|---|---|
| 80 | Golden tests de rendu | 🟢 |
| 81 | Temps maîtrisé (time scale, step frame) | 🟢 |
| 82 | Console développeur (cvars) | 🟢 |
| 83 | Debug drawing + vues buffers | 🟢 |

## PHASE L — Animation squelettale (Sprints 84–88) ✅

| # | Sprint | Apport principal | État |
|---|---|---|---|
| 84–86 | Données de squelette, échantillonnage de clips, skinning GPU | 🟢 |
| 87 | Intégration Play + blending + state machine | ✅ |
| 88 | Animation répliquée (réseau) | ✅ |

## PHASE M — Image (Sprints 89–92) ✅

| # | Sprint | Apport principal | État |
|---|---|---|---|
| 89 | Ciel + brouillard | ✅ |
| 90 | Cible HDR + tone mapping | ✅ |
| 91 | Bloom + réglages | ✅ |
| 92 | Mipmaps + tangentes | ✅ |

## PHASE N — Chaîne gameplay (Sprints 93–99) 🟢

| # | Sprint | Apport principal | État |
|---|---|---|---|
| 93 | File d'événements de gameplay (emit/on_event Lua) | ✅ |
| 94 | **Cycle de vie + handles générationnels (`slotmap`)** | ⬜ **— gap resté ouvert dans l'enchaînement** |
| 95 | GUID d'assets (`asset-id://`) + versioning de scènes | ✅ |
| 96 | Prefabs (sous-arbre JSON + overrides) | 🟢 (mécanisme fait, UI éditeur restante) |
| 97 | API Lua de scène (spawn/destroy/find_tag) | ✅ |
| 98 | Sauvegarde de partie (`user://`) | ✅ (Android non vérifié sur appareil) |
| 99 | Marqueurs temporels d'animation → événements | ✅ (démo de combat animée non câblée) |

> **Sprint 94 en suspens** : sauté entre 93 et 95, alors que le reste de la Phase N est livré.
> C'est le refactor le plus délicat annoncé (indices réseau + undo) — plus on construit dessus
> (prefabs, spawn/destroy Lua), plus il coûtera cher a posteriori. À traiter avant d'aller trop
> loin en Phase O.

---

## Sprints à venir — PHASE O : physique & feel (Sprints 100–103) ⬜

| # | Sprint | Objectif | État |
|---|---|---|---|
| 100 | Trimesh + convexe | ⬜ |
| 101 | CCD + couches de collision | ⬜ |
| 102 | Requêtes gameplay + trigger exit | ⬜ |
| 103 | **Character controller kinématique** — seul sprint qui menace l'acquis multijoueur, à faire **seul**, avec ré-audit complet de la prédiction réseau | ⬜ |

## Pistes suivantes (Phases P → R, détail dans ROADMAP_SPRINTS.md)

| Phase | Sprints | But | État |
|---|---|---|---|
| **P — Audio, HUD & confort** | 104 → 110 | Bus/panning audio, randomisation SFX, widgets HUD déclaratifs, manettes, hot-reload, snapping/profiler GPU, crash log + rustdoc | ⬜ |
| **Q — Web, la vitrine** | 111 → 114 | wasm32/WebGPU, assets & audio web, multijoueur navigateur, vitrine publique | ⬜ |
| **R — WebXR** | 115 → 117 | Spike session WebXR isolée, rendu stéréo + poses, tests XR automatisés (dépend de Q) | ⬜ |

---

## Correspondance analyse / vision → sprint

| Point | Sprint cible | État |
|---|---|---|
| IA approfondie + confort d'édition | 37 | 🟢 |
| Menus & toolbar produit | 38 | 🟢 |
| Build Panel Android (fenêtre dédiée) | 39 | 🟢 |
| Menu Ajouter complet (UI mobile) | 40 | 🟢 |
| Composants inspecteur mobiles | 41 | 🟢 |
| Optimisation mobile (mode perf, bake, POT) | 42 | 🟢 |
| Objet jouable au joystick + saut + collisions (sans script) | 43 | ✅ |
| Optimisations rendu (culling lumières, 0-alloc/frame) | 44 | ✅ |
| Simulation pilotée par la cadence de rendu (découplée, pas fixe) | 45 | ✅ |
| P4 — panics d'init (crash mobile) | 46 | ✅ |
| Couverture de tests + skip-rebuild (par hash) | 47 | ✅ |
| Gyroscope/vibration natifs + P10 (assets mobile) | 48 | ⬜ |
| Distribution store signée (IPA CI / notarisation) | 49 | ⬜ |
| Multijoueur en ligne (serveur, réseau, Firebase, combat) | 50 → 82 | ✅ cœur livré |
| Golden tests, temps maîtrisé, cvars, debug drawing | 80 → 83 (K) | 🟢 |
| Animation squelettale + réplication réseau | 84 → 88 (L) | ✅ |
| Ciel/fog, HDR/tone mapping, bloom, mipmaps | 89 → 92 (M) | ✅ |
| Événements, GUID/versioning, prefabs, API Lua scène, save, anim notifies | 93 → 99 (N) | 🟢 (94 ⬜) |
| Trimesh/CCD/requêtes/character controller | 100 → 103 (O) | ⬜ |
| Audio bus/HUD widgets/manettes/hot-reload/profiler/crash log | 104 → 110 (P) | ⬜ |
| WASM/WebGPU/multi navigateur/vitrine | 111 → 114 (Q) | ⬜ |
| WebXR (spike, stéréo, tests) | 115 → 117 (R) | ⬜ |
