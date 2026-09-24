# Roadmap — Export réalité virtuelle (Meta Quest & casques OpenXR Android)

*24 septembre 2026 — branche de départ : `feat/herroad`*

> Antécédent : [XR_PORTAIL_ARENE.md](XR_PORTAIL_ARENE.md) (28 juillet 2026) cadrait
> une arène MR passthrough et un squelette OpenXR jamais fusionné, faute de casque.
> Cette roadmap le remplace pour l'export VR ; la vision MR y reste documentée.

## 1. Est-ce possible ? — Oui, et le moteur est déjà bien placé

Un Meta Quest (2, 3, 3S, Pro) est un **appareil Android arm64** équipé d'un GPU
Adreno piloté en **Vulkan**, et toute application VR y passe par **OpenXR** (standard
Khronos, runtime fourni par Meta). Un jeu Quest = un **APK Android** dont le
manifeste le déclare « application VR », qui ouvre une session OpenXR et rend
deux images (une par œil) dans des swapchains fournies par le runtime.

Ce que RusteeGear a déjà et qui sert directement :

| Brique existante | Pourquoi elle compte pour la VR |
|---|---|
| APK Android via `cargo-apk`, `NativeActivity`, cible `aarch64-linux-android` (`packaging/build_apk.sh`) | Le Quest attend exactement ça : `.so` natif arm64 dans un APK signé |
| Rendu **wgpu 29** (Vulkan sur Android) | wgpu 29 expose l'interop bas niveau nécessaire : `Instance::from_hal`, `Adapter::create_device_from_hal`, `vulkan::Adapter::open_with_callback`, `vulkan::Device::texture_from_raw`, `Device::create_texture_from_hal` |
| `Features::MULTIVIEW` + `multiview_mask` dans wgpu 29 | Rendu des deux yeux en **une seule passe** (optimisation clé sur Quest) |
| Rendu **headless vers texture** (`gfx/renderer/headless.rs`) | Preuve que le renderer sait déjà dessiner ailleurs que dans la surface de la fenêtre — c'est ce que fait la VR (swapchain XR) |
| `InputEvent` agnostique (`app/input.rs`, dont le commentaire prévoit déjà « la VR (OpenXR) ») | Les manettes Touch se traduisent vers les mêmes actions de jeu |
| `RenderQuality` + presets perf mobile | Base du profil « VR » (budget GPU très serré) |
| API Lua `pose`/`hand` (projet Mouvéo, MediaPipe) | Le hand tracking du Quest peut alimenter **les mêmes signaux** → rééducation en VR |

**Précédents qui prouvent la voie** : `bevy_mod_openxr` (ex-`bevy_oxr`) fait tourner
Bevy — lui aussi basé sur wgpu — sur Quest avec exactement ce montage (instance
Vulkan créée par OpenXR, puis importée dans wgpu). La crate `openxr` (Ralith) gère
Android et peut compiler le loader Khronos en statique.

### Ce qui n'existe pas encore (le vrai travail)

1. **Démarrage piloté par OpenXR** : aujourd'hui wgpu crée seul son instance/device
   et une surface winit. En VR, c'est OpenXR (`XR_KHR_vulkan_enable2`) qui doit créer
   le `VkInstance`/`VkDevice`, que l'on importe ensuite dans wgpu.
2. **Boucle de frame XR** : `xrWaitFrame` → `xrBeginFrame` → `xrLocateViews`
   (pose + champ de vision asymétrique de chaque œil) → rendu → `xrEndFrame`.
   Pas de `surface.get_current_texture()` / `present()`.
3. **Renderer multi-vues** : `render()` suppose une caméra `OrbitCamera` unique et
   une surface. Il faut « N vues » (1 desktop, 2 VR) avec des matrices fournies de
   l'extérieur.
4. **Budget perf** : 72–90 Hz *stables*, ~2064×2208 px *par œil* sur Quest 3, GPU
   mobile à tuiles. La chaîne actuelle HDR → bloom → tonemap → egui est trop
   chère telle quelle.
5. **UI** : egui peint en surimpression écran — impossible en VR. Il faut un panneau
   dans le monde, piloté au rayon de manette.
6. **Caméra & confort** : la caméra orbitale 3ᵉ personne et le *camera shake*
   rendent malade en VR. Il faut un « rig » (origine de jeu + tête trackée),
   locomotion confortable, pas de secousses.
7. **Manifeste Quest + loader OpenXR** dans l'APK.

### Pourquoi natif OpenXR plutôt que WebXR

Le player web existe (water.loicberthod.ch) et le navigateur du Quest supporte
WebXR, mais WebXR s'appuie sur WebGL ; le lien WebXR ↔ WebGPU n'est pas
disponible de façon stable, et le player web RusteeGear vise WebGPU. Le natif
donne aussi les 72–120 Hz, le hand tracking complet, le foveated rendering et la
publication sur le Horizon Store. **WebXR reste une piste d'exploration tardive**
(phase 10).

---

## 2. Architecture cible

```
android_main (feature `vr`)
 └─ winit (cycle de vie Android, pas de surface)
     └─ XrRuntime  (src/xr/)
         ├─ instance OpenXR  + XR_KHR_android_create_instance
         ├─ XR_KHR_vulkan_enable2 → VkInstance / VkDevice
         │     └─ wgpu::Instance::from_hal / Adapter::create_device_from_hal
         ├─ session + espaces (LOCAL_FLOOR / STAGE, VIEW)
         ├─ swapchain couleur (+ profondeur) : tableau 2 couches (multiview)
         │     └─ vulkan::Device::texture_from_raw → wgpu::Texture
         ├─ actions (manettes Touch, mains) → XrInput
         └─ boucle : wait → begin → locate_views → Renderer::render_views → end
Renderer
 ├─ render()        : chemin actuel (fenêtre, 1 vue, egui écran)   — inchangé
 └─ render_views()  : N vues, cible externe, profil RenderQuality::Vr,
                      UI egui → texture → quad monde
```

Principes :
- **Feature Cargo `vr`** + APK séparé (`com.berthod.rusteegear.vr`) : zéro impact
  sur les builds desktop/mobile/web existants.
- La simulation (`AppState`, physique, Lua, réseau) ne change pas : elle reçoit la
  pose de tête et des manettes comme n'importe quelle entrée.
- Matrice de projection construite depuis les 4 angles `XrFovf` (asymétrique),
  convention wgpu z ∈ [0,1].

---

## 3. Prérequis matériels et comptes

- Un casque **Meta Quest 2, 3 ou 3S** + câble USB-C (données).
- **Compte développeur Meta** (organisation créée sur le dashboard développeur) et
  **mode développeur** activé via l'app mobile Meta Horizon.
- `adb` : déjà en place pour Android.
- ⚠️ **Pas de runtime OpenXR sur macOS** : l'itération se fait sur le casque (APK +
  `adb`). Option : un PC Windows avec Quest Link / SteamVR pour itérer en PCVR
  (même code OpenXR, backend Vulkan desktop).

---

## 4. Phases

Estimations en jours de développement effectifs, pour un dev qui connaît le moteur.

### Phase 0 — Spike « Hello Quest » (3–5 j) — *go / no-go technique*
- Dépendance `openxr` (feature `static` ou loader Khronos embarqué via
  `runtime_libs`), derrière `feature = "vr"`.
- Exemple `examples/xr_hello.rs` : instance XR → Vulkan → wgpu, swapchain, efface
  chaque œil d'une couleur différente, puis un cube fixe dans l'espace.
- Manifeste Quest minimal (voir phase 7) + install `adb`.
- **Critère de sortie** : cube stable et correctement stéréoscopique, tête trackée,
  72 Hz sans saccade, retour propre au menu Quest (bouton Meta).
- **Risque principal traité ici** : compatibilité des extensions Vulkan exigées par
  wgpu avec celles qu'OpenXR demande (`open_with_callback` pour les ajouter).

### Phase 1 — Backend XR dans le moteur (1–2 sem)
- Module `src/xr/` : `XrRuntime` (session, états `READY/FOCUSED/VISIBLE/STOPPING`,
  pause/reprise Android), swapchains, boucle de frame.
- `Renderer::new_xr(...)` qui reçoit device/queue importés ; pas de surface.
- Abstraction `View { view: Mat4, proj: Mat4, viewport }` ; `render_views(&[View])`
  réutilise les passes existantes (ombres calculées **une fois** pour les deux yeux).
- Uniform caméra par vue (bind group dynamique) — premier jet : **deux passes**,
  une par œil, sur les deux couches du swapchain.
- Chargement d'une scène du projet, en mode Player, dans le casque.
- **Sortie** : une démo existante (ex. Rivière) visible et jouable « à la tête »
  (sans manettes) dans le casque.

### Phase 2 — Performance VR (1–2 sem)
Budget : 13,9 ms/frame à 72 Hz (11,1 ms à 90 Hz), viser ≤ 10 ms GPU.
- `RenderQuality::Vr` : bloom off, réflexion planaire off, 1 cascade d'ombre 1024,
  LOD agressif, particules plafonnées.
- **Tonemap dans la passe principale** (pas de cible HDR intermédiaire + passe
  plein écran : très coûteux sur GPU à tuiles), MSAA 4× (quasi gratuit sur Adreno).
- **Multiview** : `Features::MULTIVIEW`, `multiview_mask = 0b11`, `@builtin(view_index)`
  dans `main.wgsl`/`skinned.wgsl`/`sky.wgsl`/`particles.wgsl`, caméra en tableau de 2.
- Textures **ASTC/ETC2** dans `gfx::texcompress` (le BC actuel n'existe pas sur Adreno).
- **Foveated rendering fixe** (`XR_FB_foveation`) et choix 72/90 Hz
  (`XR_FB_display_refresh_rate`).
- Mesures : `adb logcat` + OVR Metrics Tool (overlay FPS/GPU dans le casque).
- **Sortie** : Rivière à 72 Hz stable sur Quest 2, 90 Hz sur Quest 3.

### Phase 3 — Entrées VR (≈ 1 sem)
- Actions OpenXR : profils `oculus/touch_controller` + `khr/simple_controller`
  (repli) ; poses `grip`/`aim`, gâchettes, grip, sticks, A/B/X/Y, menu.
- Traduction vers les actions de jeu existantes (déplacement, saut, attaque,
  pause) — réutilise le remapping `settings`.
- Modèles 3D des manettes dans la scène, **retour haptique**
  (`xrApplyHapticFeedback`), recentrage.
- **Sortie** : Rivière/combat jouable entièrement aux manettes.

### Phase 4 — Caméra, locomotion, confort (≈ 1 sem)
- « Rig VR » : origine de jeu (suit le personnage) + pose de tête ; espace
  `LOCAL_FLOOR`/`STAGE` pour la hauteur réelle du joueur ; échelle 1 unité = 1 m.
- Modes : première personne ; « table » (vue 3ᵉ personne fixe, diorama) pour les
  jeux pensés en 3ᵉ personne.
- Locomotion : déplacement doux + **rotation par crans** (snap turn) +
  téléportation optionnelle ; **vignette** pendant le mouvement.
- `camera shake` et secousses désactivés en VR ; transitions de scène en fondu.
- **HerRoad en cockpit** : jeu assis, cockpit fixe = très bon candidat VR (peu de
  cinétose).
- **Sortie** : test de confort 15 min sans malaise sur 3 testeurs.

### Phase 5 — Interface en VR (≈ 1 sem)
- egui rendu dans une **texture** → quad placé dans le monde (ou calque
  `XrCompositionLayerQuad`, plus net pour le texte).
- Pointeur = rayon de la manette ; gâchette = clic → `egui::Event` pointeur.
- Menu pause, écran d'accueil, réglages VR (hauteur, main dominante, snap angle,
  vignette), HUD attaché au poignet.
- **Sortie** : tout le parcours Player (accueil → jeu → pause → quitter) sans retirer
  le casque.

### Phase 6 — Audio spatial & API Lua (3–5 j)
- Écouteur `kira` = pose de tête (orientation réelle, pas la caméra de jeu).
- Lua : `vr.active()`, `vr.head()`, `vr.controller("left"|"right")`,
  `vr.button(...)`, `vr.haptic(main, amplitude, durée)`, `vr.recenter()` ; documenté
  dans `docs/LUA_API.md`, no-op hors VR.

### Phase 7 — Packaging & éditeur (3–5 j)
- `packaging/build_quest.sh` (calqué sur `build_apk.sh`) : `--features vr`, identité
  `…​.vr`, signature release, `adb install` + lancement.
- Manifeste (`[package.metadata.android]` injecté par le script) :
  - catégorie d'intent `com.oculus.intent.category.VR` (+ `LAUNCHER`) ;
  - `uses-feature android.hardware.vr.headtracking` (`required=true`, `version=1`) ;
  - `meta-data com.oculus.supportedDevices` = `quest2|quest3|quest3s|questpro` ;
  - hand tracking (phase 8) : `oculus.software.handtracking` +
    permission `com.oculus.permission.HAND_TRACKING` ;
  - orientation paysage, `minSdk`/`targetSdk` alignés sur les exigences Meta
    **à re-vérifier au moment de la soumission** (elles évoluent).
- Loader OpenXR Khronos (`libopenxr_loader.so`) embarqué.
- Panneau Export : nouvelle cible **« Meta Quest · .apk »** (`Target::Quest` dans
  `editor/export.rs`), détection `adb` + casque branché, case « installer et lancer »,
  logs `adb` (réutilise `dump_adb_logs`).
- Contrôle de préparation (`editor/readiness.rs`) : avertir si la scène dépasse le
  budget VR (draw calls, triangles, textures non compressées).
- CI : job de compilation `aarch64-linux-android --features vr` (sans casque).

### Phase 8 — Mains & rééducation (1–2 sem, dans le périmètre, après la phase 3)
- `XR_EXT_hand_tracking` : 26 articulations par main → mêmes signaux Lua `hand`
  que MediaPipe (projet Mouvéo) ; les scripts de rééducation marchent tels quels.
- Pincement = clic ; mains visibles dans la scène.
- Intérêt fort pour PhysioTech : exercices en VR, mesure 3D réelle des amplitudes.

### Phase 9 — Publication et autres casques (continu)
- **Sideload** (`adb`) pour les tests ; **Meta Horizon Store** (canal de test puis
  public) : respect des VRC (performance, retour au menu, pas de crash, gestion du
  focus/pause), captures, fiche.
- Même APK OpenXR, manifestes adaptés : **Pico 4**, **Android XR** (Samsung),
  HTC Vive Focus.
- **PCVR** : Windows/Linux + SteamVR / Quest Link, backend Vulkan desktop — quasi
  gratuit une fois la phase 1 faite, et pratique pour itérer.

### Phase 10 — WebXR (exploration)
- Mettre water.loicberthod.ch en VR dans le navigateur du Quest. Dépend du support
  WebXR + WebGPU dans le navigateur (sinon backend WebGL2 de wgpu). À réévaluer
  après la phase 9.

---

## 5. Calendrier indicatif

| Phase | Durée | Cumul |
|---|---|---|
| 0 Spike | 3–5 j | ~1 sem |
| 1 Backend XR | 1–2 sem | ~3 sem |
| 2 Perf | 1–2 sem | ~5 sem |
| 3 Entrées | 1 sem | ~6 sem |
| 4 Confort | 1 sem | ~7 sem |
| 5 UI VR | 1 sem | ~8 sem |
| 6 Audio + Lua | 3–5 j | ~9 sem |
| 7 Packaging | 3–5 j | **~10 sem → export Quest utilisable** |
| 8 Mains (Mouvéo, dans le périmètre) | 1–2 sem | ~12 sem |
| 9–10 | continu | — |

Un **premier APK jouable dans le casque** (sans UI VR ni confort) arrive dès la
fin de la phase 1, soit environ 3 semaines.

## 6. Risques

| Risque | Impact | Parade |
|---|---|---|
| Interop wgpu-hal ↔ OpenXR fragile (API `unsafe`, change entre versions wgpu) | Bloquant | Spike phase 0 ; épingler wgpu ; isoler dans `src/xr/` ; s'inspirer de `bevy_mod_openxr` |
| Perf insuffisante (HDR/bloom, scènes lourdes) | Fort | Profil `Vr`, multiview, ASTC, foveation, avertissements éditeur |
| Cinétose | Fort | Snap turn, vignette, pas de shake, jeux assis (HerRoad) |
| Pas de runtime OpenXR sur Mac | Moyen | Itération sur casque ; PC Windows + Link en option |
| Exigences du Horizon Store évolutives | Moyen | Vérifier à la soumission ; sideload d'abord |
| egui peu lisible en VR | Moyen | Calque quad XR, police agrandie, UI simplifiée |

## 7. Décisions (prises le 24 septembre 2026)

1. **Casque cible : Meta Quest 3** (Adreno 740, 2064×2208 par œil, 90 Hz visé).
   Le Quest 2 n'est plus le plancher : le budget de la phase 2 se mesure sur Quest 3.
2. **Premier jeu porté : Rivière** (démo `riviere`, déjà multijoueur PvE/PvP) —
   critère de sortie des phases 1 à 4.
3. **Hand tracking inclus dans le périmètre** (Mouvéo / PhysioTech) : la phase 8
   n'est plus optionnelle. Elle est avancée juste après la phase 3 (les mains sont
   une entrée comme les manettes) et le manifeste de la phase 0 déclare déjà
   `oculus.software.handtracking` (`required=false`) +
   `com.oculus.permission.HAND_TRACKING`.

## 8. État — phase 0 (24 septembre 2026)

Code du spike écrit, compilé et empaqueté ; **reste le test dans le casque**.

| Élément | Fichier | État |
|---|---|---|
| Feature Cargo `vr` (`openxr` 0.22 `loaded`, `ash` =0.38.0, Android seulement) | `Cargo.toml` | ✅ |
| Projection stéréo asymétrique + vue depuis la pose (4 tests) | `src/xr/math.rs` | ✅ `cargo test --lib xr::` |
| Instance/device Vulkan créés par OpenXR puis importés dans wgpu, swapchain 2 couches, boucle de frame, 5 cubes + sol en espace `STAGE` | `src/xr/hello.rs` | ✅ compile (`clippy` propre), **à tester** |
| `android_main` bascule vers `xr::hello::run` si `vr` | `src/lib.rs` | ✅ APK téléphone inchangé |
| Manifeste Quest (catégorie VR, headtracking, mains, `supportedDevices`), loader Khronos 1.1.63 | `packaging/build_quest.sh` | ✅ APK debug 9,2 Mo |
| Scène de test partagée APK / simulateur | `src/xr/test_scene.rs` | ✅ |
| **Simulateur Meta Quest 3** (profil Quest 3/2, tête et manettes simulées, 6 tests) | `src/xr/sim.rs`, `src/bin/quest_sim.rs` | ✅ stéréo vérifiée (capture) |
| Job CI `cargo build --lib --target aarch64-linux-android --features vr` | `.github/workflows/ci.yml` | ✅ |
| **Phase 1 : Rivière en stéréo** (moteur complet, simulateur + APK) | cf. §10 | ✅ simulateur, casque à valider |

### Procédure de test (Quest 3)

1. **Mode développeur** : compte sur developers.meta.com (organisation), puis app
   Meta Horizon sur le téléphone → Appareils → le casque → Paramètres du casque →
   Mode développeur.
2. Brancher le casque en USB-C, **mettre le casque** et accepter « Autoriser le
   débogage USB » (cocher « Toujours autoriser »).
3. `~/Library/Android/sdk/platform-tools/adb devices` doit lister le casque.
4. `INSTALL=1 ./packaging/build_quest.sh` (construit, installe, lance).
5. Journaux : `~/Library/Android/sdk/platform-tools/adb logcat | grep -E 'motor3derust|OpenXR|panicked'`.
   Lignes attendues : `VR : runtime …`, `VR : GPU …`, `VR : 2064×2208 px par œil`
   (environ), `VR : état de session READY/SYNCHRONIZED/VISIBLE/FOCUSED`, puis
   `VR : 720 images rendues` toutes les ~10 s.
6. Relancer ensuite depuis le casque : Bibliothèque → filtre **Sources inconnues** →
   « RusteeGear VR ».

**Grille go / no-go** : 5 cubes colorés (orange devant à ~1,3 m de haut, bleu/vert
à hauteur de taille, rouge/jaune plus loin) + un sol gris ; vision nette, sans
dédoublement ; les cubes restent fixes quand on bouge la tête ; bouton Meta → menu
puis retour dans l'app ; « Quitter » ferme proprement (`VR : session terminée
proprement`).

En cas d'échec : conserver la sortie `logcat` complète — elle suffit à diagnostiquer
les étapes (loader, instance XR, Vulkan, session, swapchain).

## 9. Développer sans casque : le simulateur Quest (24 septembre 2026)

macOS n'a aucun runtime OpenXR ; pour ne pas bloquer chaque phase sur un test
casque, RusteeGear embarque son propre simulateur :

```bash
cargo run --release --bin quest_sim                     # fenêtre interactive
cargo run --release --bin quest_sim -- --snapshot s.png  # image stéréo hors écran (CI, agent)
```

- **Même code que l'APK** : `xr::math::EyeView` (projection asymétrique) et
  `xr::test_scene` sont appelés à l'identique par `xr::hello` (casque) et par
  `quest_sim` (desktop). Seule la source des poses change : `xrLocateViews` d'un
  côté, `xr::sim::SimHead` de l'autre.
- **Profil matériel réel** (`QuestProfile::QUEST3`) : 2064×2208 px par œil,
  FOV asymétriques, IPD 63 mm, 90 Hz (budget 11,1 ms). `2` bascule sur le
  profil Quest 2 (1832×1920, 72 Hz). Les FOV sont approximatifs : les remplacer
  par ceux que journalise l'APK au premier test.
- **Commandes** : clic gauche glissé = tête · ZQSD/WASD = marcher dans la pièce
  · Espace/C = se lever/s'accroupir · R = recentrer · Échap = quitter. Le titre
  affiche le temps par image comparé au budget (indicatif : GPU du Mac).
- **Manettes simulées** : `SimHead::controller_pose` (mains à hauteur de
  taille, suivant le lacet) — base de la phase 3.

### Ce que le simulateur remplace, ce qu'il ne remplace pas

| Vérification | Simulateur | Casque |
|---|---|---|
| Stéréo, parallaxe, FOV asymétriques, échelle 1 u = 1 m | ✅ (tests + capture) | confirmation |
| Rendu de Rivière en stéréo (P1), UI VR (P5), entrées (P3), confort de locomotion (P4), audio spatial et Lua (P6) | ✅ | confirmation finale |
| Interop OpenXR ↔ Vulkan ↔ wgpu (`xr::hello`) | ❌ | **obligatoire** (P0) |
| Performances Adreno 740 à 90 Hz (P2) | ordre de grandeur | **obligatoire** |
| Suivi des mains réel (P8), Guardian, exigences Horizon Store | ❌ | **obligatoire** |

**Décision (24 sept.)** : les phases 1 à 6 se développent et se vérifient sur le
simulateur ; le casque sert de validation par jalon (P0, puis fin de P2 et P8).
Prochaine étape technique : un trait `XrBackend` (poses d'yeux, cibles de rendu,
entrées) implémenté par `xr::hello` et par `quest_sim`, pour que le
`Renderer::render_views` de la phase 1 soit écrit une fois et tourne des deux côtés.

## 10. Phase 1 — Rivière en stéréo (24 septembre 2026) : ✅ sur simulateur

`cargo run --release --bin quest_sim` affiche désormais **la vraie partie
Rivière** (défaut ; `--scene cubes` pour la scène de test), et l'APK aussi
(`./packaging/build_quest.sh`, `VR_SCENE=cubes` pour le test pur de la phase 0).

| Élément | Fichier |
|---|---|
| `Renderer::new_external` : renderer sur un device fourni (runtime XR, simulateur) ; constructeur scindé (`assemble`) | `src/gfx/renderer/resources.rs`, `xr.rs` |
| `Renderer::render_views` : pipeline complet (ombres en cascade, ciel, eau + réflexion, skinning, translucides, particules, bloom, tone mapping) par œil ; ombres, culling, lumières calculés une fois depuis une caméra centrale englobante (`xr::rig::cull_camera`) ; caméra de jeu restaurée ; pas de *camera shake* | `src/gfx/renderer/xr.rs` |
| Passe principale partagée headless / VR (`encode_scene_pass`) — goldens inchangés | `src/gfx/renderer/headless.rs` |
| Rig : pièce du joueur posée 2,5 m derrière le personnage, sol sur le terrain (`AppState::ground_height_at`, rayon physique) | `src/xr/rig.rs` |
| Contenu VR commun APK / simulateur (`XrContent` : cubes ou partie) | `src/xr/content.rs` |
| APK de test en profil `dev-fast` (opt-level 1), 21,5 Mo | `packaging/build_quest.sh` |

Vérifié : 1 030 tests lib + goldens de rendu identiques, clippy (desktop et
Android `vr`), captures stéréo de Rivière contrôlées (parallaxe, ombres, eau).

### Mesures qui orientent la phase 2 (simulateur, Mac M5 Pro)

| Configuration | Temps par image (2 yeux) |
|---|---|
| Scène de cubes, 2064×2208 par œil | **1,8 ms** — la plomberie VR ne coûte rien |
| Rivière, 2064×2208 par œil | **37–39 ms** (budget 11,1 ms à 90 Hz) |
| Rivière, résolution × 0,7 | 33 ms |
| Rivière, résolution × 0,5 | 32 ms |
| Détail d'une image Rivière | simulation `advance_play` **16–24 ms**, préparation du rendu CPU 4,7 ms, scripts Lua 0,2 ms |

**Diagnostic** (profil `sample` de macOS) : le coût n'est pas le GPU mais
`Physics::resolve_scripted_moves` — le contrôleur de personnage cinématique de
rapier (détection de sol + *shape casts* contre le maillage du terrain) pour
chaque créature scriptée : ~15 ms par image. La mesure « physique » existante
(`sim_perf_ms`, 0,06 ms) ne l'inclut pas. Ce coût pèse aussi sur le jeu
desktop et web, pas seulement en VR.

**Phase 2 — priorités révisées** :
1. CPU d'abord : `resolve_scripted_moves` (créatures lointaines ou hors champ
   en mouvement simplifié, pas de détection de sol à chaque pas, budget de
   créatures actives) et l'inclure dans la mesure `sim_perf_ms`.
2. Découpler la simulation du rendu (simulation à 30–60 Hz, rendu à 90 Hz avec
   interpolation), comme sur un casque tout jeu doit le faire.
3. Seulement ensuite le GPU : multiview, MSAA 4× + tonemap dans la passe
   principale, ASTC, foveation, résolution de rendu (déjà mesurable :
   `QUEST_SIM_SCALE=0.7 cargo run --release --bin quest_sim`).

Dans le casque, l'APK Rivière actuel tournera donc bien en dessous de 72 Hz :
c'est attendu à ce stade (P1 = justesse du rendu, P2 = vitesse). Pour ce soir,
le test go / no-go de la phase 0 reste `VR_SCENE=cubes INSTALL=1
./packaging/build_quest.sh` ; l'APK Rivière est un aperçu.

## 11. Phase 2, partie 1 — CPU et profil VR (24 septembre 2026)

### Mesurer juste : `quest_sim --bench`

```bash
cargo run --release --bin quest_sim -- --bench 8        # Rivière, Quest 3
cargo run --release --bin quest_sim -- --bench 8 --scene cubes
QUEST_SIM_SCALE=0.7 QUEST_SIM_DRAW_DISTANCE=1 cargo run --release --bin quest_sim -- --bench 8
```

Hors écran, sans vsync, CPU et GPU en parallèle avec une image d'avance (comme
`xrWaitFrame`) : débit, CPU (simulation / rendu), draw calls. Les mesures du
§10 attendaient le GPU à chaque image (CPU + GPU additionnés) : pessimistes.
Fermer tout autre rendu GPU (navigateur, aperçu web) avant de mesurer —
l'écart peut aller du simple au double.

### Corrections

| Changement | Effet mesuré |
|---|---|
| **Corps scriptés au repos** (`Physics::resolve_scripted_moves`) : un objet posé, que son script ne déplace ni ne tourne, ne repasse plus par le `KinematicCharacterController` à chaque pas ; revérification étalée tous les 15 pas, réveil immédiat au premier déplacement/rotation, après `set_position`, et 2 pas de délai après `set_object_solid` (sol retiré). 4 tests. | `advance_play` Rivière : **16,8 → 2,4 ms** (médiane). Profite aussi au jeu desktop, au web et au **serveur** multijoueur. |
| Mesure `sim_perf_ms` « physique » : couvre désormais pilotage joueurs/IA + corps scriptés + pas rapier | le coût n'est plus invisible |
| **Profil de rendu VR** (`Renderer::new_external`) : distances de culling/LOD du feuillage × 0,6, ombres 1024, pas de réflexion planaire (eau sans reflet, une passe de toute la scène en moins par œil) | 2 549 → 1 343 draw calls par image stéréo |

### Résultat (Mac M5 Pro, Rivière, pleine résolution Quest 3)

| | Avant (§10) | Maintenant |
|---|---|---|
| Temps par image | 37–39 ms (mesure sérialisée) | **10,1–10,7 ms, 94–99 img/s** (budget 11,1 ms) |
| CPU par image | ~29 ms | **6,0 ms** (simulation 1,8, rendu 4,3) |
| Résolution × 0,7 | — | 8,9 ms |
| Sans ombres | — | 8,8 ms (ombres ≈ 1,3 ms) |
| Scène de cubes | 1,8 ms | 0,8 ms |

Le Mac tient donc 90 Hz ; **le Quest 3 non, pas encore** : son GPU (Adreno 740)
et son CPU sont nettement moins puissants qu'un M5 Pro. Estimation grossière,
à confirmer au casque : ×3 à ×5 sur les deux. D'où la suite.

### Phase 2, partie 2 — à faire (mesure au casque indispensable)

1. **Multiview** (`Features::MULTIVIEW`, `@builtin(view_index)`) : une seule
   passe de scène pour les deux yeux — divise par deux les draw calls et le
   coût CPU de rendu (4,3 ms → ~2,2).
2. **Simulation découplée du rendu** : pas fixe 60 Hz, rendu 72/90 Hz
   interpolé (le moteur a déjà `sim_poses` pour l'interpolation réseau).
3. **Résolution de rendu** adaptée au casque (0,7–0,8 × la recommandée, le
   compositeur agrandit) + **foveation fixe** (`XR_FB_foveation`).
4. Ombres : 1 cascade en VR ; MSAA 4× + tonemap dans la passe principale
   (supprime une cible HDR plein écran par œil, coûteuse sur GPU à tuiles) ;
   textures **ASTC**.
5. Rafraîchissement 72 Hz par défaut sur Quest, 90 Hz si mesuré tenable.

## 12. Phase 2, partie 2 — multiview, cache de classement, réglages casque

| Changement | Effet mesuré (Mac M5 Pro, Rivière, 2064×2208/œil) |
|---|---|
| **Multiview** : variantes des shaders de scène **dérivées automatiquement** du source (`gfx::multiview::multiview_wgsl` : `camera` → paire de caméras + `@builtin(view_index)`, validées par naga en test) ; pipelines multiview (`multiview_mask = 0b11`) ; une seule passe de scène vers une cible HDR à deux couches, tone mapping par œil. Activé si le GPU expose `Features::MULTIVIEW` (Metal : oui ; Vulkan 1.1 des Quest : oui). `QUEST_SIM_MULTIVIEW=0` pour comparer. | draw calls 1 343 → **769** ; image identique à 99 % au rendu œil par œil (différences = créatures en mouvement) ; gain de temps faible sur Mac, attendu plus net sur Adreno (multiview natif) — **à mesurer au casque** |
| **Cache de classement des modèles** (`Renderer::refresh_mesh_classes`) : rayon de culling et « feuillage dense » calculés une fois par modèle (43), plus par objet (3 104) et par image — profilé : recherches de mots-clés dans les chemins de fichiers | CPU de rendu **4,15 → 0,95 ms** — profite aussi au desktop et au web |
| APK : fréquence d'affichage demandée (`XR_FB_display_refresh_rate`, 72 Hz par défaut, `VR_HZ=90`), résolution de rendu réglable (`VR_RENDER_SCALE=0.8`), module `xr::quality` testé | — (casque) |
| Simulation découplée du rendu | **déjà en place** dans le moteur : pas fixe 60 Hz + interpolation (`blend_render_poses`) |

**Résultat** : 9,9–10 ms par image (100 img/s), **CPU 2,6 ms** (simulation 1,7, rendu 0,95) — contre 37–39 ms (mesure sérialisée) au matin du 24.
Le GPU est désormais le seul goulot : ~7 ms fixes (géométrie) + part liée à la
résolution (8,3 ms à × 0,7).

**Reste pour le casque** (non mesurable sur Mac) : foveation fixe
(`XR_FB_foveation`), textures ASTC, choix final 72/90 Hz et de la résolution
selon l'OVR Metrics Tool.

## 13. Phases 3 et 4 — manettes, déplacement, confort (24 septembre 2026)

Même code pour les vraies manettes (APK) et le simulateur.

| Élément | Fichier | Vérifié |
|---|---|---|
| État des manettes Touch (`XrInput`, poses poignée/visée, gâchettes, grips, sticks, A/B/X/Y, menu), correspondance vers les commandes du jeu, zone morte, vibrations sur fronts montants (coup encaissé : deux mains ; coup porté : main droite) | `src/xr/input.rs` | 4 tests |
| Actions OpenXR (APK) : jeu d'actions, correspondances Touch + profil générique de repli, poses via espaces d'action, vibrations `xrApplyHapticFeedback` | `src/xr/actions.rs`, `src/xr/hello.rs` | compile (Android, clippy) — **à tester au casque** |
| Manettes simulées au clavier/souris (ZQSD = stick gauche, A/E = crans, Espace = A, F/clic droit = gâchette…), `QUEST_SIM_HOLD=W,F` pour les captures | `src/bin/quest_sim.rs` | captures |
| Vue **première personne** (la pièce suit le personnage, personnage masqué) ⇄ **spectateur** (clic stick droit / V) | `src/xr/content.rs`, `src/xr/locomotion.rs` | captures |
| **Rotation par crans** 30° autour de la tête (hystérésis : un cran par poussée) | `xr::locomotion::SnapTurn` | 2 tests |
| **Vignette** de confort selon la vitesse (lissée), pipeline dédié | `src/gfx/renderer/xr.rs` | test naga + capture |
| Repères des manettes (boîtes filaires + pointeur) : lignes de debug, variante multiview du pipeline `gizmo` | `gfx::pipelines`, `gfx::multiview` | capture |
| Déplacement relatif au **regard** : `engine_move` + `AppState::vr_camera_yaw` (appliqué après la caméra de jeu, qui réimpose son lacet) | `xr::locomotion`, `app::simulation` | **test d'intégration sur le vrai moteur** (4 regards × avant/droite) |

Mesures dans le simulateur (1 s de jeu, `QUEST_SIM_HOLD`) : W → le personnage
avance vers le regard (z 24 → 14,2) ; D → vers la droite (x 3,5 → 9,3) ;
Q puis W → cran de 30° à gauche puis avance dans la nouvelle direction ; V →
vue spectateur. Performance inchangée (~10 ms/image stéréo sur Mac).

**Bug du jeu trouvé au passage (hors VR)** : dans Rivière, au clavier (W) et
à la manette, « avant » fait marcher le personnage **vers la caméra** quand
elle est alignée sur Z (`camera_relative_move` renvoie des coordonnées monde,
puis `vz = −my` inverse Z une seconde fois : réflexion du repère caméra ;
vérifié par rendu avant/après). **Corrigé le jour même** dans le moteur
(commit `51ef677`, `camera_relative_axes`, clavier + manette + tactile +
réseau, avec tests) ; `engine_move` suit désormais la convention corrigée
(θ = atan2(−Lx, −Lz), stick tel quel) et son test d'intégration sur le vrai
moteur cassera si elle change encore.

## 14. Phase 5 — interface en VR (24 septembre 2026)

| Élément | Fichier | Vérifié |
|---|---|---|
| Panneaux egui rendus **dans des textures** (un contexte egui + un `egui_wgpu::Renderer` par panneau), posés dans le monde en quads, dessinés dans chaque œil après la scène (sRGB correct, alpha prémultiplié) | `src/xr/ui.rs` | shader validé par naga |
| **Menu de pause** flottant (bouton menu de la manette gauche / Tab au simulateur), 1,3 m devant la tête, jeu en pause : Reprendre, Recentrer la vue, Vue 1re personne/spectateur, Rotation par crans 15/30/45°, Vignette oui/non, Rejouer la manche, Quitter (fin de session OpenXR / fermeture du simulateur) | `src/xr/content.rs` | capture |
| Pointage au **rayon de la manette droite** (rayon dessiné, curseur orange), gâchette = clic ; manettes relâchées pour le jeu tant que le menu est ouvert | `xr::ui::PanelPose::hit`, `VrUi::paint` | 3 tests purs + **test GPU** `tests/vr_menu.rs` (viser/appuyer/relâcher = un clic ; hors panneau = aucun) |
| **Affichage au poignet** gauche : vie (barre) et ennemis vaincus, tourné vers la tête | `xr::ui::PanelPose::facing` | capture |
| Couleurs de l'écran d'accueil (#12141a / #e8763b) | — | — |
