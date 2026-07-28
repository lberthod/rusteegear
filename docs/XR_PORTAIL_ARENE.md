# XR Portail — Arène MR passthrough (document évolutif)

> Document satellite, dans l'esprit de `sprint10audit.md`/`sprintreflecion.md` :
> périmètre disjoint de la feuille de route moteur principale
> ([ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)), pas renuméroté dans ses sprints
> A→S. Ce doc est **la source de vérité de ce projet précis** et se met à jour
> au fil de l'avancement (voir [Journal](#journal) en bas).
>
> À ne pas confondre avec [Phase R — WebXR](ROADMAP_SPRINTS.md#phase-r) du
> roadmap principal : Phase R vise une session XR **dans le navigateur**
> (spéculatif, après Phase Q). Ce document couvre un chantier différent —
> **OpenXR natif sur APK Quest**, hors périmètre wasm32.

**Statut** : **direction retenue** — transformation du moteur vers le XR/MR
lancée. Ce n'est plus un side-track optionnel : c'est l'axe de
développement actif de RusteeGear à partir de maintenant. Aucun code écrit
pour l'instant. Prochaine étape : spike Phase 0 (voir
[Plan par phases](#plan-par-phases)).

> Note de cadrage (gardée ici pour la traçabilité, pas pour re-débattre) :
> le risque principal identifié n'est pas technique — la faisabilité est
> réelle, avec du prior art vérifié — mais la **bande passante solo** face
> à deux projets ambitieux en parallèle (le shooter MR sur Unity/MRPortal,
> et maintenant cette transformation). Décision assumée : on y va quand
> même. Les garde-fous ci-dessous ([Plan par phases](#plan-par-phases),
> checkpoints go/no-go) existent pour protéger la qualité *dans* ce choix,
> pas pour le retarder.

---

## Table des matières

- [Vision](#vision)
- [Périmètre : APK vs Web](#perimetre)
- [Architecture cible](#architecture)
- [État de l'écosystème Rust OpenXR (recherche)](#ecosysteme)
- [Plan par phases](#plan-par-phases)
- [Budget de performance (standalone Quest)](#budget-perf)
- [Ergonomie / UX MR](#ux-mr)
- [Risques & inconnues à re-vérifier](#risques)
- [Journal](#journal)

---

<a id="vision"></a>
## Vision

Un shooter d'arène en **réalité mixte passthrough** sur Meta Quest :

1. Le joueur scanne sa vraie pièce ; le jeu identifie les **portes et
   fenêtres**.
2. Un **stencil MR** ne rend les ennemis visibles qu'à travers ces
   ouvertures — ils « rentrent dans notre réalité » depuis l'autre côté du
   mur, pas ailleurs dans la pièce.
3. Par-dessus ce mécanisme, une **boucle d'arène** classique (vagues,
   score, mort/victoire).
4. Premier ennemi : une **araignée procédurale**, 8 pattes avec IK,
   capable de changer de surface (sol/mur/plafond, monter sur une table)
   via raycast + réorientation du corps.
5. **Multijoueur** — plusieurs casques (et des spectateurs flat) dans la
   même arène.
6. Doit **tenir en standalone** sans déclencher le throttle thermique du
   Quest.

---

<a id="perimetre"></a>
## Périmètre : APK vs Web

**APK (OpenXR natif, Quest) — le vrai jeu.** Seul chemin viable pour le
passthrough, le stencil-portail et la détection de pièce. Concentre tout
l'effort XR de ce document.

**Web — rôle différent, pas une version XR.** Deux raisons de ne *pas*
viser une expérience immersive en navigateur pour ce projet :

1. Le build web de RusteeGear tourne sur **WebGPU** (wasm32/wgpu). WebXR+
   WebGPU est encore expérimental/derrière des flags dans la plupart des
   navigateurs (contrairement à WebXR+WebGL, mature) — parier dessus
   maintenant n'apporte rien de sûr.
2. « Porte/fenêtre détectée » n'a de sens qu'avec un casque passthrough
   scannant une vraie pièce. Un joueur laptop n'a pas de pièce à scanner.

Le web garde son rôle actuel : RusteeGear a **une seule simulation
partagée**, servie par un serveur autoritaire unique — desktop, APK et
navigateur se connectent déjà à la même partie. Le build web sert de
**client spectateur/second joueur flat** dans la même arène (clavier/
souris, vue classique), pendant que le(s) joueur(s) casqué(s) vivent le
portail en MR. Zéro travail XR supplémentaire côté web ; ça retombe sur le
multijoueur existant tel quel.

---

<a id="architecture"></a>
## Architecture cible

Principe directeur : **greffer**, pas réécrire. Les modules ci-dessous
existent déjà et sont réutilisés tels quels ou avec un delta ciblé — pas de
nouveau sous-système parallèle.

| Besoin | Aujourd'hui | Delta à apporter |
|---|---|---|
| Boucle fenêtre/événements | `src/lib.rs::App` (`winit::ApplicationHandler`) | Nouveau chemin d'entrée Android/OpenXR à côté de `run()`, pas en remplacement (desktop/mobile flat restent sur winit) |
| Rendu GPU | `src/gfx/renderer/` (`Renderer::render`), un seul `view_proj` (`OrbitCamera`) | Deux `view_proj` par frame (un par œil), pose venant du tracking HMD/OpenXR au lieu de la souris ; acquisition de frame pilotée par le swapchain OpenXR plutôt que la surface winit |
| Portail/stencil | `src/gfx/pipelines.rs` — plusieurs pipelines ont déjà un `DepthStencilState`, mais `StencilState::default()` (no-op) | Activer un vrai stencil write/test pour découper l'ouverture — brancher l'existant, pas un nouveau système |
| LOD araignée | `src/gfx/lod.rs` (existant, générique) | Réutiliser tel quel : moins d'os IK résolus + mesh moins détaillé selon distance |
| Simulation de jeu | `AppState::advance_play` (`src/app/simulation.rs`), pas fixe 1/60, indépendant du framerate d'affichage | IK araignée + gait de surface = un système de plus dans le pas fixe, déterministe, rejouable serveur/client comme le reste |
| Multijoueur | Serveur autoritaire unique, même simulation solo/client/serveur (pas de logique dupliquée) | Aucun changement d'architecture : l'araignée est un `SceneObject` de plus, répliquée comme le reste |
| Passthrough | — (nouveau) | `XR_FB_passthrough` : layer de composition dans `xrEndFrame`, alpha-cutout sur la layer projection pour l'effet portail — ne touche pas la boucle de rendu existante, s'ajoute à côté |
| Détection porte/fenêtre | — (nouveau) | v1 : placement **manuel** par le joueur (pointer + confirmer) au lieu du Scene API auto — voir [Risques](#risques) |

Nouveau module proposé : `src/xr/` (session OpenXR, swapchain, poses,
passthrough), consommé par `AppState`/`Renderer` de la même façon qu'un
input device de plus — sans dépendance GPU dans `AppState`, cohérent avec
la séparation existante (`Renderer` ne fait que consommer l'état, jamais
l'inverse).

---

<a id="ecosysteme"></a>
## État de l'écosystème Rust OpenXR (recherche, à re-vérifier avant implémentation)

- **Crate `openxr`** (openxrs, Ralith) : version 0.21.1 (janv. 2026),
  activement maintenu (~670k téléchargements). Wrapper **haut niveau tout
  fait** pour `XR_FB_passthrough` (`openxr::PassthroughFB`), pas seulement
  des bindings bruts. Init du loader Android (JNI `JavaVM`/`Activity`) géré
  nativement par le crate (`entry.rs`).
- **Trou confirmé** : aucun binding, ni haut niveau ni brut, pour
  `XR_FB_scene`/spatial anchors (détection sémantique porte/fenêtre) dans
  ce crate. Nécessiterait d'écrire les FFI à la main contre le XML
  Khronos — c'est le vrai chantier long, pas le passthrough.
- **Interop wgpu ↔ OpenXR (Vulkan)** : `wgpu-hal` expose
  `Instance::from_hal`/`Device::from_hal` pour partager les handles Vulkan
  avec OpenXR. Prior art actuel et pertinent : `matthewjberger/wgpu-example`
  tourne explicitement sur Quest 2/3/3S (build via `xbuild`, bundle
  `libopenxr_loader.so`). Piège connu : wgpu veut souvent du BGRA8, Quest
  donne du RGBA8 → texture intermédiaire + blit.
- **Passthrough (`XR_FB_passthrough`)** : niveau composition de layers dans
  `xrEndFrame` (pas le swapchain lui-même) — s'ajoute à une boucle Vulkan
  standard sans refonte d'architecture profonde une fois le swapchain
  stéréo en place.
- **Manifest/loader Android** : catégorie d'intent
  `com.oculus.intent.category.VR`, feature
  `android.hardware.vr.headtracking`, `libopenxr_loader.so` embarqué en
  `arm64-v8a` jniLibs (depuis le Meta OpenXR Mobile SDK).
- **Verdict chiffré** : ~3-6 semaines pour un portail passthrough
  fonctionnel *sans* détection auto Scene API. La détection auto peut
  facilement doubler ce chiffre en solo.
- Écosystème mouvant (crate publié janv. 2026, exemples datés de façon
  incertaine) — **à re-vérifier juste avant de démarrer** l'implémentation.

---

<a id="plan-par-phases"></a>
## Plan par phases

| Phase | Objectif | Dépend de | Statut |
|---|---|---|---|
| **0 — Spike swapchain** | Cube en stéréo affiché dans le casque Quest, APK qui tourne (go/no-go réel) | — | ⬜ |
| **1 — Passthrough + portail manuel** | Passthrough activé, joueur pointe/pose un portail à la main (ancre manuelle), alpha-cutout stencil | Phase 0 | ⬜ |
| **2 — Araignée procédurale (IK + gait)** | 8 pattes, IK réparti sur plusieurs frames, changement de surface par raycast | — (parallélisable dès maintenant, en desktop, indépendant de la XR) | ⬜ |
| **3 — Arène + vagues** | Boucle spawn/score/victoire branchée sur le combat existant (`src/app/combat.rs`) | Phase 1, Phase 2 | ⬜ |
| **4 — Multijoueur** | Araignée + portail répliqués via le serveur autoritaire existant | Phase 3 | ⬜ |
| **5 — Scene API auto** | Détection porte/fenêtre automatique (remplace le placement manuel de la Phase 1) | Phase 1, bindings FFI `XR_FB_scene` à écrire | ⬜ |
| **6 — Budget thermique standalone** | Résolution dynamique, cap dur du nombre d'araignées actives, profiling réel casque | Phase 3+ | ⬜ |

Les phases 2 (araignée) et 0/1 (XR) avancent **en parallèle** sans se
bloquer mutuellement.

---

<a id="budget-perf"></a>
## Budget de performance (standalone Quest)

- **Stencil/portail** : quasi gratuit — pipelines existants, juste
  activer le stencil test au lieu du no-op actuel.
- **IK araignée** : poste de coût CPU principal. Lever : répartir la
  résolution IK sur plusieurs frames (une patte par tick plutôt que les 8
  en même temps, invisible à l'œil, ÷~8 le coût). Cap dur du nombre
  d'araignées actives simultanément (budget fixe, pas de scaling
  dynamique).
- **LOD** : réutiliser `src/gfx/lod.rs` tel quel pour os IK + détail mesh
  selon distance.
- **Foveated rendering (FFR)** : géré au niveau compositeur/OpenXR, gain
  GPU quasi gratuit, à activer dès le swapchain en place.
- **Passthrough** : composité par le système, pas par notre pipeline GPU —
  le coût réel est ce qu'on choisit de dessiner *dans* le portail, pas la
  pièce entière.
- **Résolution dynamique** comme soupape thermique (resize swapchain à la
  volée) plutôt qu'un frame-drop brutal — à prévoir dès la Phase 0, pas en
  rustine plus tard.
- **Réseau** : une seule simulation partagée (pas de logique dupliquée) —
  l'IK doit rester déterministe côté serveur et rejouée côté client, pas
  resimulée indépendamment (sinon désync visible en multi).

---

<a id="ux-mr"></a>
## Ergonomie / UX MR

- **Placement du portail** : rayon manette/main qui accroche la surface,
  contour lumineux en temps réel, confirmation à la gâchette — pas de menu
  qui suit la tête (fatigue, casse l'immersion). UI ancrée au monde ou au
  poignet.
- **Pas de locomotion artificielle** : le joueur bouge dans sa vraie
  pièce ; toute translation/rotation de caméra non voulue (dash, snap-turn
  agressif) est la première cause de nausée en MR — le décor réel sert de
  référence stable et contredit le mouvement artificiel. Les ennemis
  viennent au joueur via le portail, pas l'inverse.
- **Sécurité physique = contrainte de gameplay** : télégraphier l'attaque
  tôt (son directionnel + lueur avant que la patte sorte) pour laisser le
  temps de réagir sans trébucher sur un vrai meuble.
- **Manette pour viser/tirer, main pour poser le portail et naviguer les
  menus** — mélanger selon l'action plutôt qu'un seul mode d'entrée pour
  tout.
- **Feedback thermique discret** : indicateur subtil quand la résolution
  dynamique baisse à cause de la chauffe, pas un popup qui casse
  l'immersion.

---

<a id="risques"></a>
## Risques & inconnues à re-vérifier

- **Scene API (`XR_FB_scene`) sans binding Rust** : le vrai chantier long.
  Dérisqué en v1 par le placement manuel (Phase 1) — la détection auto
  (Phase 5) n'est pas sur le chemin critique du « wow » initial.
  Alternative si le FFI à la main s'avère trop coûteux : port depuis le
  SDK Meta C/C++ officiel plutôt que suivre le XML Khronos brut — à
  évaluer en Phase 5.
- **Écosystème mouvant** : `openxr` crate, `matthewjberger/wgpu-example`,
  `indite` — versions et existence à revérifier juste avant chaque phase,
  pas seulement en amont.
- **Format swapchain BGRA8 (wgpu) vs RGBA8 (Quest/OpenXR)** : texture
  intermédiaire + blit à prévoir dès la Phase 0, pas découvert en cours de
  route.
- **Aucun test XR possible dans cet environnement d'exécution** (pas de
  casque physique ici) — toute validation de rendu stéréo/passthrough doit
  se faire sur device réel, hors de cette session.

---

<a id="journal"></a>
## Journal

- **2026-07-28** — Cadrage initial : vision, périmètre APK/Web, recherche
  écosystème OpenXR Rust (crate `openxr` 0.21.1, gap Scene API, prior art
  `wgpu-example`), architecture cible (greffe sur modules existants),
  budget perf, UX MR, plan en 7 phases. Aucun code écrit — prochaine étape
  Phase 0.
- **2026-07-28** — Décision : la transformation XR/MR devient l'axe de
  développement actif de RusteeGear (plus un side-track). Doc référencé
  depuis le README (§ La suite — analyse & sprints). Prochaine action
  concrète : démarrer la Phase 0 (spike swapchain OpenXR + wgpu).
- **2026-07-28** — Premier code Phase 0 : `src/xr/mod.rs` (instance + system
  OpenXR, feature `xr`/Android uniquement), écrit sans pouvoir compiler pour
  `aarch64-linux-android` dans l'environnement de rédaction. **Contrainte
  découverte en cours de route : aucun casque Quest disponible pour tester**
  — ni ici, ni chez le développeur pour l'instant. Le job CI `cross-build`
  (qui installe déjà le NDK pour builder la lib Android) a été étendu pour
  builder aussi avec `--features xr` : c'est la vérification de référence
  tant qu'aucun device n'est disponible — compile-checké, pas runtime-testé.
  Ne se déclenche que sur push `main`/pull request, pas sur un simple push
  de branche.
