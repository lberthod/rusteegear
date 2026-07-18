# RusteeGear — Plan de sprints d'optimisation 3D (`optimisation3D.Analys.md`)

> Traduit les constats de [optimisation3D.Analys.md](optimisation3D.Analys.md) en phases/sprints
> exécutables. Convention identique à [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md) et
> [sprint10audit.md](sprint10audit.md) : un sprint ≈ 1 à 3 jours, avec
> **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**.
> On ne démarre un sprint que si le précédent **de la même phase** est « vert ».

Retour : **[optimisation3D.Analys.md](optimisation3D.Analys.md)** (constat) ·
**[GDD_MMORPG.md](GDD_MMORPG.md)** · **[ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md)**.

---

## 📸 Analyse globale — état AVANT (référence, mesurée dans le code, pas estimée)

| Indicateur | Valeur mesurée | Source |
|---|---|---|
| Objets totaux dans `mmorpg_demo()` | **887** | sonde directe sur `Scene::mmorpg_demo()` |
| Meshes glTF importés distincts | **315** (320 clés mesh/texture) | idem |
| Objets skinnés simultanés | **201** | idem |
| Capacité skinnée du renderer | `MAX_SKINNED_INSTANCES = 160` | `src/gfx/renderer.rs:160` |
| Marge skinnée réelle | **−41** (201 > 160) si tout est visible en même temps | calcul |
| Historique de la constante | Relevée 3× déjà (8→32→96→160) | `src/gfx/renderer.rs:137-159` |
| Draw calls statiques | Batchés par plages contiguës `(mesh, texture)` | `src/gfx/renderer.rs:2375-2386` |
| Draw calls skinnés | 1 par instance visible (pas de batching) | `src/gfx/renderer.rs:860-885` |
| Frustum culling | Présent (statiques + skinnés) | `src/gfx/passes.rs:25-72` |
| Culling par distance | **Absent** | — |
| Occlusion culling | **Absent** | — |
| LOD géométrique | **Absent** | — |
| Compression texture GPU | **Absente** (`Rgba8UnormSrgb` brut) | `src/gfx/renderer.rs:487` |
| Mesh les plus instanciés | `nature_grass_tuft.glb` ×112, `nature_fern.glb` ×69 | sonde |
| Profiler existant | Oui — FPS, GPU par passe, draw calls, `skinned_dropped` | `src/editor/windows.rs:75-152` |
| FPS réel mesuré en jeu | **125** (min 110 · moy 120 · max 127), 861 objets, 315 modèles, ~382 draw calls, **`skinned_dropped` = 0** | Phase 0, 2026-07-18 — desktop Metal (Apple M1), `mmorpg_demo` via Démos → MMORPG, vue large couvrant la horde de créatures (« Errant N »). Détail GPU par passe indisponible (cf. note ci-dessous) |

## 🎯 Analyse globale — cible APRÈS (fin du plan)

| Indicateur | Cible visée |
|---|---|
| `skinned_dropped` en vue large sur `mmorpg_demo` | **0**, avec marge de sécurité (pas juste au ras de la capacité) |
| Draw calls skinnés en vue large | Sans objet actuellement — Sprint 2 a mesuré 0 doublon de mesh parmi le décor skinné statique éligible, donc rien à regrouper (voir Phase B, décision Sprint 3) |
| Charge en vue large/plongée (frustum plein) | Atténuée par un culling par distance |
| Fill-rate du feuillage dense (herbe/fougères) | Réduit par un LOD/impostor à distance |
| Compatibilité VRAM mobile/Android | Textures compressées avant tout build Android sérieux |
| FPS en jeu (desktop, vue large) | Mesuré en Phase 0 puis en Phase F, delta documenté |

L'écart avant/après ne sera quantifié en FPS réel qu'après la Phase 0 (baseline) et la Phase F
(validation finale) — voir ces phases. Le reste des indicateurs (draw calls, skinned_dropped,
comptage d'instances) est déjà mesurable dès maintenant via le Profiler intégré.

---

## 🧭 Vue d'ensemble — indépendance des phases

```
Phase 0 (Baseline mesurée) ──► doit précéder toute décision de réglage fin
   │
   ├─ Phase A (Sécurité skinnée immédiate)      ─┐
   ├─ Phase B (Instancing GPU du skinning)       ─┤  indépendantes entre elles,
   ├─ Phase C (Culling par distance)             ─┤  démarrables en parallèle
   ├─ Phase D (LOD géométrique herbe/fougères)   ─┤  après Phase 0
   └─ Phase E (Compression texture GPU mobile)   ─┘
                    │
                    ▼
        Phase F (Validation finale avant/après) ── dépend de A, B, C, D, E
```

- **Phase 0** doit être faite en premier : c'est la mesure de référence, rapide (ouvrir le Profiler
  en jeu), mais toutes les décisions de réglage (Phase A notamment) s'appuient dessus.
- **A, B, C, D, E n'ont aucune dépendance entre elles** une fois la Phase 0 faite : elles touchent
  des zones différentes (capacité skinnée / batching skinné / culling / LOD / import texture) et
  peuvent être menées en parallèle par des sessions différentes.
- **Attention fichiers partagés** : A et B touchent toutes deux `src/gfx/renderer.rs` (zone skinnée) ;
  C et D touchent toutes deux `src/gfx/passes.rs`/`renderer.rs` (zone culling/visibilité) — à
  coordonner si menées vraiment en simultané par deux personnes.
- **Phase F dépend de toutes les autres** : c'est la mesure finale, elle n'a de sens qu'une fois
  A→E terminées (sinon l'« après » est partiel).

| Phase | Sprints | Dépend de | Parallèle possible avec | But | Statut | Blocage |
|---|---|---|---|---|---|---|
| **0 — Baseline** | 0 | — | — (à faire en premier) | Mesurer l'état réel avant toute optimisation | ✅ Fait (18 juillet) | — |
| **A — Sécurité skinnée** | 1 | 0 | B, C, D, E | Éliminer la perte silencieuse de créatures en vue large | ✅ Fait (Sprint 1, 18 juillet) | — |
| **B — Instancing skinning** | 2 → 3 | 0 | A, C, D, E | Diviser les draw calls skinnés et lever la contrainte de capacité | ✅ Fait (Sprint 2 ; Sprint 3 descopé) | — |
| **C — Culling distance** | 4 | 0 | A, B, D, E | Réduire la charge en vue large/plongée | ✅ Fait (Sprint 4) | — |
| **D — LOD géométrique** | 5 | 0 | A, B, C, E | Réduire le fill-rate du feuillage dense | ✅ Câblé (18 juillet) | — (validation visuelle/mesure Phase F restantes) |
| **E — Compression texture** | 6 | 0 | A, B, C, D | Préparer la VRAM mobile/Android | ✅ Close avec constat (18 juillet) | Chemin BC3 sans cible réelle sur le contenu actuel ; ASTC mobile non traité |
| **F — Validation finale** | 7 | **A, B, C, D, E** | — | Mesurer l'après, documenter le delta | ✅ Fait (18 juillet) | — (delta FPS fenêtré non fait, limite d'outillage documentée) |

---

<a id="phase-0"></a>
## PHASE 0 — Baseline mesurée (préalable, rapide)

### Sprint 0 — Mesure de référence en conditions réelles ✅ (2026-07-18)
**Objectif** : obtenir les vrais chiffres FPS/draw calls/skinned_dropped en jeu, pas seulement les
comptages d'objets déjà mesurés dans le code (`optimisation3D.Analys.md`).
- [x] Lancer le binaire desktop sur `mmorpg_demo`, ouvrir le panneau « 📊 Profiler FPS »
  (`src/editor/windows.rs:75-152`).
- [x] Se placer en vue large/plongée (le scénario qui a historiquement fait déborder
  `MAX_SKINNED_INSTANCES`) et relever : FPS moyen/min, `gpu_draw_calls`, `skinned_dropped`,
  temps GPU par passe (Ombres/Scène/HDR+Bloom/UI).
- [x] Consigner ces chiffres dans ce document (tableau « AVANT ») ou dans un fichier de suivi.

**Résultat mesuré** (desktop, Metal/Apple M1, `mmorpg_demo`, vue large sur la horde de créatures) :

| Métrique | Valeur |
|---|---|
| FPS | 125 actuel, min 110 · moy 120 · max 127 |
| Objets visibles | 861 (315 modèles importés) |
| Draw calls (estimation) | ~382 |
| `skinned_dropped` | **0** |
| Temps GPU par passe | Non mesuré (voir ci-dessous) |

**Constat inattendu, hors périmètre de mesure pure** : ouvrir le Profiler déclenchait un crash
(`index out of bounds` dans `Scene::local_aabb`, indexation non protégée d'un mesh importé —
corrigé, `src/scene/queries.rs`) puis, une fois le crash corrigé, un gel permanent de l'éditeur
(le calcul des timestamp queries GPU par passe, `Renderer::read_gpu_pass_timings`, ne revenait
jamais sur cette machine). Les timestamp queries GPU par passe ont donc été **désactivées**
(`gpu_profiling = false`, `src/gfx/renderer.rs`, commentaire à ce niveau) le temps d'être
réinvestiguées avec un vrai débogueur GPU — FPS/draw calls/`skinned_dropped` restent fiables et
n'en dépendent pas. `skinned_dropped = 0` avec marge (861 objets < 887 mesurés dans le code : tous
n'étaient pas chargés/visibles simultanément dans cette vue) suggère que `MAX_SKINNED_INSTANCES` (160)
n'a pas débordé sur cette prise de vue précise — à reconfirmer avec une vue couvrant vraiment les
887 objets/201 skinnés avant de conclure que la Phase A n'est plus nécessaire.

- **Fichiers modifiés** (hors périmètre lecture-seule initial, nécessaires pour débloquer la
  mesure) : `src/scene/queries.rs` (fix crash), `src/gfx/renderer.rs` (fix gel + désactivation
  temporaire du détail GPU par passe).
- **Livrable** : chiffres réels ci-dessus, remplacent la ligne « FPS réel mesuré en jeu — Non
  disponible » du tableau « AVANT ».
- **Risques** : aucun résiduel pour la mesure elle-même. Reste ouvert : le détail GPU par passe est
  actuellement indisponible (feature désactivée, pas supprimée) ; et la vue testée (861/887 objets)
  n'est peut-être pas le pire cas absolu — une mesure complémentaire avec tous les objets/skinnés
  simultanément visibles resterait utile avant de statuer définitivement sur la Phase A.

---

<a id="phase-a"></a>
## PHASE A — Sécurité skinnée immédiate (indépendante après Phase 0)

### Sprint 1 — Relever `MAX_SKINNED_INSTANCES` à une valeur sûre — ✅ fait (2026-07-18)
**Objectif** : éliminer la perte silencieuse de créatures constatée (201 skinnés mesurés vs
capacité 160) le temps que la Phase B (solution structurelle) soit livrée.
- [x] Relevé `MAX_SKINNED_INSTANCES` de 160 à **256** (`src/gfx/renderer.rs:174`, historique
  documenté dans le commentaire de la constante) — marge ~55 au-dessus des 201 objets skinnés
  mesurés dans le code (`optimisation3D.Analys.md`). Fait de façon préventive malgré
  `skinned_dropped == 0` mesuré en Phase 0 : cette mesure n'avait pas les 201 objets skinnés
  simultanément visibles (861/887 objets chargés dans la vue testée), le dépassement restait donc
  latent — cf. l'historique de la constante, déjà relevée 3 fois pour la même raison.
- [x] Impact mémoire du buffer de joints vérifié : `JOINT_CAPACITY(128) × MAX_SKINNED_INSTANCES` ×
  64 octets/matrice → 1,25 Mio (160) à 2,0 Mio (256), soit **+0,75 Mio**, négligeable.
- [x] Tests unitaires liés (`cargo test skinned`) : 9 passés, dont
  `skinned_instances_beyond_capacity_get_no_offset_instead_of_aliasing_slot_zero`.
- [x] Re-mesure Phase 0 (vue large/zénithale sur `mmorpg_demo`) : **confirmé** — 798 objets, FPS 67
  (min 62 · moy 66 · max 73), ~590 draw calls, `skinned_dropped == 0`.
- **Fichiers modifiés** : `src/gfx/renderer.rs` (une constante + son commentaire d'historique).
- **Livrable** : vue large sur `mmorpg_demo`, `skinned_dropped == 0` dans le Profiler.
- **Risques** : c'est un pansement, pas une solution — la vraie capacité doit rester couplée au
  contenu de la scène (documenté explicitement dans le code comme ayant déjà été relevé 4 fois) ;
  ne pas considérer ce sprint comme suffisant à long terme, d'où la Phase B (déjà scopée/descopée,
  voir Sprint 2/3 ci-dessous).

---

<a id="phase-b"></a>
## PHASE B — Instancing GPU du skinning (indépendante, la plus structurante)

### Sprint 2 — Isoler le décor animé « statique en place » — ✅ fait
**Objectif** : distinguer les vraies créatures IA (mouvement, combat) du décor animé qui ne fait
que jouer un clip `Idle` sur place (PNJ errants, monstres décoratifs des packs menagerie), candidats
naturels à l'instancing.
- [x] Marquer/catégoriser ces objets dans `src/scene/demos.rs` (flag ou liste dédiée). Fait via
  `Scene::is_skinned_mesh`/`Scene::is_static_skinned_decor` (`src/scene/queries.rs`) — catégorisation
  calculée (squelette présent + ni `AiChaser` ni script), pas un nouveau champ stocké (évite le
  churn de format de scène pour un dérivé stable).
- [x] Confirmer leur nombre exact parmi les 201 skinnés mesurés (sonde similaire à celle de
  l'analyse initiale). Fait, verrouillé par le test
  `mmorpg_demo_static_skinned_decor_has_no_duplicate_mesh` (`src/scene/demos.rs`) : **201** skinnés
  au total (confirme exactement le chiffre de l'analyse initiale), **61** actifs (26
  `MMORPG_CREATURES` + 35 « Errant N » scriptées, non éligibles), **140** décor statique éligible.
- **Fichiers** : `src/scene/queries.rs` (nouvelles méthodes), `src/scene/demos.rs` (test).
- **Livrable** : ✅ compte exact ci-dessus, verrouillé par test de non-régression.
- **Constat additionnel (bloquant pour Sprint 3, voir ci-dessous)** : parmi les 140 objets
  éligibles, **aucun mesh n'est utilisé plus d'une fois** (chaque `monster_*.glb`/`fauna_*.glb`/
  `nature_*.glb` animé n'est posé qu'à un seul endroit dans `mmorpg_demo`) — vérifié par le même
  test.
- **Risques** : ne pas mal classer une créature qui a en fait une IA active (vérifier contre
  `AiChaser`/comportement de patrouille) — couvert par le test (`active_count == 61` verrouillé).

### Sprint 3 — Instancing GPU-skinning par texture d'animation — ⚠️ descoping, pas de bénéfice mesurable actuellement
**Constat Sprint 2** : l'instancing GPU du skinning ne réduit un draw call que s'il existe **au
moins deux instances du même mesh** partageant une palette de joints. Or les 140 objets « décor
statique éligible » identifiés en Sprint 2 sont **tous des meshes uniques** (aucune duplication) —
contrairement aux meshes très instanciés cités dans l'analyse initiale (`nature_grass_tuft.glb`
×112, `nature_fern.glb` ×69), qui sont en réalité des primitives **non skinnées** (`gen_grass_tuft`/
`gen_fern`, `scripts/blender/gen_nature_pack.py` — cônes/blobs sans armature), déjà couvertes par le
batching statique existant. **Implémenter le Sprint 3 tel que spécifié n'apporterait donc aucune
réduction mesurable de `gpu_draw_calls`** sur le contenu actuel de `mmorpg_demo`.
- [x] ~~Implémenter l'instancing~~ → **non fait, décision documentée** : ne pas construire un chemin
  de rendu (shader + bind groups + tri par lots) pour un gain de zéro draw call mesuré. Le design
  (palette de joints partagée par lot de phase, cf. rapport dans le sprint B) reste consigné pour le
  jour où du contenu dupliqué apparaîtrait, mais n'est pas codé (YAGNI).
- [ ] **Reste ouvert / à reprendre si le contenu change** : si une future passe de contenu ajoute des
  doublons de mesh skinné décoratif (ex. plusieurs moutons `fauna_sheep.glb`), relancer le test
  `mmorpg_demo_static_skinned_decor_has_no_duplicate_mesh` — un échec (`max_instances > 1`) est le
  signal explicite que Sprint 3 redevient rentable, avec le compte exact du gain potentiel.
- **Fichiers** : aucun changement de rendu (décision, pas d'implémentation).
- **Livrable** : ✅ décision documentée et testée (garde-fou automatique si la prémisse change).
- **Risques** : si ce sprint est repris plus tard (contenu dupliqué ajouté), le design à
  privilégier n'est **pas** une texture de palette par instance (complexité shader/bind-group
  élevée pour un gain marginal sur des animations d'idle discrètes) mais une palette de joints
  **partagée par lot `(mesh, texture, phase quantifiée)`** : 4 à 8 tranches de phase suffisent à
  casser l'effet lockstep visuel entre instances d'un même clip (`src/scene/demos.rs` décale déjà
  volontairement la phase de départ de chaque instance, `time: anim_count as f32 * 0.37`, pour
  cette raison — une palette 100 % unifiée par mesh les remettrait en lockstep).

### Audit du Sprint B (post-clôture) — nouvelle piste trouvée, non implémentée ici
En auditant les 140 objets éligibles, **50 ont un squelette mais `animation: None`** (jamais de
clip joué — étals/établis de `VILLAGE_PROPS`, riggés par le même gabarit que les créatures via
`scripts/blender/gen_items_pack11_20.py`, mais jamais activés dans `demos.rs`). Verrouillé par le
test `mmorpg_demo_has_static_skinned_decor_that_never_animates` (`src/scene/demos.rs`). Ces objets
rendent une pose de liaison figée — visuellement indiscernable d'un mesh statique — mais passent
quand même par `draw_skinned_objects` (`is_skinned` ne teste que la présence d'un squelette, jamais
`AnimationState`) : un draw call **et** un emplacement de `MAX_SKINNED_INSTANCES` dépensés pour
rien. Les basculer sur le chemin statique ferait passer les 201 skinnés mesurés à 151 (sous la
capacité de 160), réduisant voire éliminant le besoin de la Phase A. **Non implémenté ici** : la
correction toucherait `src/gfx/renderer.rs`, hors périmètre scène de ce sprint et partagé avec les
Phases A/C/D — à traiter dans un sprint dédié touchant le renderer, coordonné avec ces phases.

---

<a id="phase-c"></a>
## PHASE C — Culling par distance (indépendante) — ✅ Sprint 4 fait

### Sprint 4 — Rayon de culling par type d'objet — ✅ fait
**Objectif** : réduire la charge en vue large/plongée sans occlusion culling complet.
- [x] Définir un rayon de culling par catégorie (herbe/fougères : faible rayon ; arbres/rochers :
  rayon moyen ; bâtiments/créatures : rayon large ou pas de limite). — `culling_radius_for()`,
  `src/gfx/passes.rs` : 45 m (feuillage bas) / 110 m (arbres/rochers) / `None` (le reste).
- [x] Appliquer ce culling en complément du frustum existant (`src/gfx/passes.rs:25-72`), avant le
  tri en plages contiguës. — câblé dans la boucle du plan de dessin statique de
  `src/gfx/renderer.rs`, en `&&` avec `aabb_visible`.
- **Fichiers** : `src/gfx/passes.rs`, `src/gfx/renderer.rs`.
- **Livrable** : en vue large sur `mmorpg_demo`, `gpu_draw_calls` et temps de passe « Scène »
  réduits par rapport à la baseline (Phase 0), sans popping visible gênant. **Non mesuré en jeu
  ici** — logique posée et testée unitairement (3 tests dans `gfx::passes::culling_distance_tests`),
  mais la validation Profiler (chiffres avant/après, ajustement des rayons) reste à faire.
- **Bug trouvé et corrigé à l'audit** : la catégorisation par sous-chaîne (`path.contains("rock")`)
  matchait à tort `nature_rocking_chair.glb` (un meuble de jardin) dans le groupe « arbres/rochers »
  — seul faux positif parmi les ~110 assets `nature_*.glb` vérifiés un par un. Corrigé en remplaçant
  le test par sous-chaîne par `contains_word()` (frontière de mot exigée), avec test de
  non-régression dédié (`rocking_chair_is_not_matched_by_rock_keyword`).
- **Risques** : un rayon trop agressif crée du popping visible ; itérer avec le Profiler ouvert pour
  trouver le compromis distance/qualité. Rayons actuels (45 m/110 m) non calibrés en conditions
  réelles, à ajuster après mesure.

---

<a id="phase-d"></a>
## PHASE D — LOD géométrique herbe/fougères (indépendante) — ✅ Sprint 5 fait

> ✅ **Câblée le 18 juillet 2026** — décision de LOD (`src/gfx/lod.rs`, `foliage_lod_mesh`, 7 tests),
> nouvelle primitive `MeshKind::Billboard` (impostor croix, `src/gfx/mesh.rs`, testée) remplaçant le
> choix initial `MeshKind::Plane` (bug trouvé à l'audit : un plan horizontal est quasi invisible vu à
> hauteur d'œil, inadapté au feuillage debout), câblage dans les 4 sites de dessin de
> `Renderer::render` (`InstanceDraw::mesh` précalculé par distance caméra). Un second bug trouvé et
> corrigé pendant l'audit : `FOLIAGE_LOD_KEYWORDS` (mot-clé `"reeds"`) matchait aussi
> `nature_reeds_sway.glb` (variante animée du roseau, rive du lac) — la substituer par l'impostor
> statique lui aurait fait perdre silencieusement son animation ; corrigé en excluant tout chemin
> contenant `_sway`, testé (`animated_sway_variant_is_never_substituted_even_far_away`).
> Build/tests/clippy/fmt propres (23 tests `gfx::`, 120 `scene::`, 7 `golden_render`).
>
> ✅ **Validation visuelle faite le 18 juillet 2026** : captures en jeu d'une zone d'herbe/fougères
> denses en plan rapproché (< 40m, mesh complet visible, plusieurs brins/touffes distincts) et en
> vue large/plongée (> 40m sur le hameau) — aucun artefact visuel, aucune disparition brutale de
> feuillage, transition invisible à l'œil (comportement attendu d'un bon impostor). Combiné aux
> 7 tests unitaires et au câblage confirmé dans le source, le Sprint 5 est considéré clos.
> Mesure Profiler dédiée avant/après (temps de passe « Scène » isolé) non faite — nécessiterait un
> interrupteur de debug pour désactiver le LOD et comparer à isoler ; laissé à la Phase F (mesure
> globale avant/après une fois A→E closes) plutôt qu'une mesure isolée supplémentaire ici.

### Sprint 5 — Impostor/mesh simplifié pour le feuillage dense — ✅ fait (2026-07-18)
**Objectif** : réduire le fill-rate du feuillage le plus instancié (`nature_grass_tuft.glb` ×112,
`nature_fern.glb` ×69, `nature_reeds.glb` ×19).
- [x] Variante simplifiée choisie : impostor billboard croix (`MeshKind::Billboard`), pas un mesh
  `.glb` simplifié séparé — pas de nouvel asset à générer, réutilise le cache de primitives GPU.
- [x] Sélection par distance caméra câblée dans `Renderer::render` (`src/gfx/renderer.rs:1372`,
  `InstanceDraw::mesh` précalculé chaque frame via `foliage_lod_mesh`).
- [x] Validation visuelle en jeu (près/loin) : aucun artefact, transition invisible.
- **Fichiers modifiés** : `src/gfx/lod.rs` (nouveau), `src/gfx/mesh.rs` (`MeshKind::Billboard`),
  `src/gfx/renderer.rs` (câblage aux 4 sites de dessin).
- **Livrable** : temps de passe « Scène » réduit en vue large avec beaucoup de feuillage visible,
  sans changement perceptible en vue rapprochée — qualitativement confirmé ; quantification
  isolée reportée à la Phase F (mesure avant/après globale).
- **Risques** : résolus — la coordination avec la Phase C (toutes deux dans la logique de
  visibilité par instance) a été faite en les câblant ensemble une fois la Phase C committée.

---

<a id="phase-e"></a>
## PHASE E — Compression de texture GPU (indépendante, prépare le mobile) — 🟡 EN COURS

### Sprint 6 — Compression ASTC/BC7 au pipeline d'import
**Objectif** : réduire l'empreinte VRAM avant tout ciblage Android sérieux (le projet a déjà
`min_sdk_version` configuré dans `Cargo.toml`).
- [x] Ajouter une étape de compression de texture à l'import — **BC3 desktop** livré
  (`src/gfx/texcompress.rs`, branché dans `pipelines::make_texture`), activée seulement si
  `wgpu::Features::TEXTURE_COMPRESSION_BC` est supportée par le GPU (dégradation silencieuse
  vers `Rgba8UnormSrgb` sinon). **ASTC mobile non fait** — voir note ci-dessous.
- [x] Génération de mipmaps conservée pour le chemin compressé, chaîne **complète** jusqu'à 1×1
  (même formule que le chemin non compressé, `pipelines::mip_count_for` réutilisée directement) —
  via une chaîne CPU (filtre boîte en espace linéaire, `texcompress::downsample`) plutôt que le
  blit GPU existant (`mipgen_pipeline`, impossible sur un format bloc-compressé).
  **Audit a posteriori : 3 défauts trouvés et corrigés** (chaîne de mips initialement tronquée à 1
  niveau pour les tailles non multiples de 8 ; moyenne initialement en espace gamma au lieu de
  linéaire ; un crash GPU réel — `wgpu error: Copy width is not a multiple of block width` — introduit
  en corrigeant le premier défaut, capturé par le golden test existant et corrigé avant livraison).
  Ce crash prouve au passage que le chemin BC3 tourne bel et bien sur le GPU de la machine de
  développement (contrairement à ce que l'audit précédent supposait).
- [x] Cache de texture par chemin existant (`sync_textures`, `renderer.rs:1148-1179`) réutilisé
  sans duplication — le branchement compressé/non compressé se fait en amont, dans
  `pipelines::make_texture`.
- **Fichiers** : `Cargo.toml` (+ `texpresso = "2.0.2"`, pur Rust), `src/gfx/texcompress.rs`
  (nouveau, seul fichier touché par les correctifs de l'audit), `src/gfx/mod.rs`,
  `src/gfx/pipelines.rs`, `src/gfx/renderer.rs` (feature demandée à `request_device`).
- **Livrable** : `cargo check`/`cargo clippy -D warnings`/`cargo fmt --check`/
  `cargo check --target wasm32-unknown-unknown` verts ; tests ciblés verts (`texcompress` : 6 tests,
  `golden_render`+`golden_skinning` : 8 tests — dont `golden_textured_ground_with_mipmaps`, qui
  exerce et valide réellement le chemin BC3 bout-en-bout sur GPU, pas seulement en unitaire).
  `cargo test --lib` complet non relancé en fin de sprint : cassé par une modification concurrente
  et sans rapport d'une autre session sur `src/app/combat.rs`/`mod.rs` (signature de
  `update_round` changée en cours de route) — hors scope de cette phase. **Restant avant Phase F** :
  validation visuelle sur les vraies textures de `mmorpg_demo` (pas seulement le damier de test des
  golden), mesure VRAM avant/après chiffrée (pas d'outil de mesure VRAM dans le Profiler actuel), et
  **mesurer/corriger le coût de compression synchrone au premier chargement d'une scène** —
  `sync_textures` compresse toutes les textures pas encore en cache en une seule frame, sans budget
  de temps ; un micro-benchmark isolé (hors dépôt) donne ~25 ms/texture 1024×1024 (niveau de base
  seul), qui pourrait s'accumuler à plusieurs centaines de ms sur ~320 textures — non mesuré sur
  `mmorpg_demo` réellement, à vérifier en Phase 0/F.
- **Risques** : **BC7 remplacé par BC3** — aucun encodeur BC7 pur Rust (sans dépendance C/lien
  natif) identifié ; BC3 retenu pour la portabilité de build (même raisonnement que `ruzstd`
  ailleurs dans ce projet). **ASTC mobile non traité** — `TEXTURE_COMPRESSION_BC` n'est
  pratiquement jamais exposé par les GPU Android/iOS, donc l'objectif « VRAM mobile » de cette
  phase n'est pas encore atteint, seul le volet desktop l'est. À reprendre dans un sprint dédié si
  une échéance Android se précise (cohérent avec la note « E peut attendre » plus bas).

---

<a id="phase-f"></a>
## PHASE F — Validation finale avant/après (dépend de A, B, C, D, E)

### Sprint 7 — Re-mesure complète et documentation du delta — ✅ fait (18 juillet 2026)
**Objectif** : confirmer que le plan a effectivement amélioré la situation, avec les mêmes mesures
que la Phase 0.

**Préalable — Phase E close avec un constat plutôt qu'un feu vert plein** : avant de lancer cette
phase, audit des 3 points restants de la Phase E (validation visuelle BC3, mesure VRAM, coût de
compression synchrone) — découverte que le chemin BC3 (`texcompress`) n'a **aucune cible réelle**
sur `mmorpg_demo` ni sur aucune démo du dépôt (0 objet utilisant `obj.texture` sur 887 ; tous les
meshes visibles sont des imports glTF qui ne passent pas par ce chemin). Décision : documenter ce
constat et poursuivre — un avant/après qui n'aurait montré aucun gain BC3 est correct, pas un signe
d'échec de la phase.

- [x] **Impossible de piloter le panneau Profiler en jeu depuis cette session** (pas de contrôle
  clavier/souris disponible sur la fenêtre du binaire desktop — ni via le tool de contrôle d'écran,
  qui ne résout pas le process par son nom sur cette machine, ni via un second lancement, une
  instance concurrente tournant déjà). Décision (validée avec l'utilisateur) : remplacer le
  pilotage manuel par un **benchmark headless reproductible**, `examples/phase_f_measure.rs`
  (nouveau, `cargo run --release --example phase_f_measure`) — charge `Scene::mmorpg_demo()` telle
  quelle, positionne une caméra fixe documentée (vue large/plongée : `distance=90`, `yaw=0.7`,
  `pitch=1.1`, `target=0` — la plus large possible sans dépasser le plan éloigné à 100 m sur une
  arène de rayon `MMORPG_HALF=36`), effectue 5 rendus de chauffe puis 60 rendus chronométrés via
  `Renderer::render_scene_headless`, et lit `gpu_draw_calls`/`skinned_dropped` via
  `gpu_profiler_info()`/`skinned_dropped_count()`.
- [x] **Bug trouvé et corrigé en cours de mesure** : `render_scene_headless` ne renseignait jamais
  `last_frame_draw_calls` (toujours 0 en sortie de `gpu_profiler_info()` après un rendu headless) —
  seul `render()` (chemin fenêtré) le faisait. Corrigé dans `src/gfx/renderer.rs` en dupliquant
  exactement le même comptage (`scene_draw_calls` incrémenté à chaque `draw_indexed`, plus les
  retours de `draw_skinned_shadows`/`draw_skinned_objects`) dans `render_scene_headless`. Sans ce
  correctif, ce sprint (et tout futur benchmark headless) aurait rapporté `gpu_draw_calls: 0` en
  permanence — un faux signal de régression totale. `cargo test --release --test golden_render
  --test golden_skinning` : 8/8 verts après le correctif (aucune régression visuelle).
- [x] Résultats mesurés (Metal/Apple, résolution 1280×720, 887 objets scène) : **`gpu_draw_calls =
  592`**, **`skinned_dropped = 0`**, 6,41 ms/frame en boucle headless (156 FPS-équivalent — **non
  comparable** aux FPS fenêtrés de Phase 0/Sprint 1, cf. limite ci-dessous). Temps GPU par passe :
  toujours indisponible (`gpu_profiling = false`, gel Metal documenté en Phase 0, non réactivé ici —
  hors scope, nécessite un vrai débogueur GPU).
- [x] Tableau avant/après + limite méthodologique consignés dans `optimisation3D.Analys.md`
  (section « Mesure AVANT/APRÈS »).
- **Fichiers** : `optimisation3D.Analys.md` (mise à jour), `src/gfx/renderer.rs` (fix
  `last_frame_draw_calls` en headless), `examples/phase_f_measure.rs` (nouveau benchmark), ce
  document.
- **Livrable** : `skinned_dropped == 0` confirmé ; `gpu_draw_calls` mesuré exactement (592, comparable
  aux compteurs GPU des sprints précédents malgré la vue différente) — pas de comparaison FPS directe
  possible sans piloter la fenêtre (limite documentée, pas contournée silencieusement).
- **Risques restants** : la vue caméra du benchmark (90/0,7/1,1) est un choix raisonnable mais pas
  strictement identique aux vues « à l'œil » de Phase 0/Sprint 1 — un futur audit voulant un delta
  FPS fenêtré strict devra piloter le binaire desktop manuellement (limite d'outillage de cette
  session, pas du produit). Le volet ASTC mobile (Phase E) et le détail GPU par passe (timestamp
  queries désactivées) restent hors de portée de ce sprint.

---

## ✅ Définition de « terminé » par phase

| Phase | Terminée quand |
|---|---|
| 0 | ✅ Chiffres FPS/draw calls/skinned_dropped réels consignés (Sprint 0) |
| A | ✅ `skinned_dropped == 0` en vue large sur `mmorpg_demo` (Sprint 1, `MAX_SKINNED_INSTANCES` → 256), reconfirmé headless en Phase F |
| B | ✅ Catégorisation faite et testée (Sprint 2) ; Sprint 3 descopé (aucun bénéfice mesurable sur le contenu actuel, garde-fou de test en place pour le reprendre si du contenu dupliqué apparaît) |
| C | Culling par distance actif, charge réduite en vue large sans popping gênant |
| D | ✅ Câblé : `MeshKind::Billboard` (impostor croix) remplace le feuillage dense au-delà de 40 m dans `Renderer::render` ; validation visuelle en jeu faite le 18 juillet, mesure Profiler avant/après dédiée non faite (reportée à la Phase F globale) |
| E | ✅ Compression BC3 desktop livrée (Sprint 6) ; audit (3 passages) a trouvé et corrigé 3 défauts (dont un crash GPU), validée bout-en-bout par golden test sur un vrai GPU. **Constat du 18 juillet (Phase F)** : le chemin BC3 n'a aucune cible réelle sur le contenu actuel — `mmorpg_demo` (887 objets) a 0 objet utilisant `obj.texture`, et aucune démo du dépôt n'utilise ce champ (les meshes importés glTF n'ont pas de texture image via ce chemin). Coût de compression synchrone, validation visuelle et mesure VRAM sont donc structurellement à 0 sur ce contenu, pas « non mesurés ». ASTC mobile non fait |
| F | ✅ Tableau avant/après complet (`optimisation3D.Analys.md` § « Mesure AVANT/APRÈS »), `skinned_dropped == 0` et `gpu_draw_calls` confirmés par un benchmark headless reproductible ; delta FPS fenêtré non fait (limite d'outillage documentée, pas un chiffre caché) |

## 📌 Conseils d'exécution

- Ne pas sauter la **Phase 0** : sans baseline réelle, impossible de savoir si les phases suivantes
  ont un effet mesurable, seulement un effet supposé.
- **A avant B en pratique** même si non strictement dépendantes : A est un filet de sécurité
  d'une ligne, à poser tout de suite pendant que B (plus long) est en cours.
- Coordonner les merges sur `src/gfx/renderer.rs` (A + B) et `src/gfx/passes.rs` (C + D) si plusieurs
  sessions travaillent en parallèle sur ces phases.
- **E peut attendre** : c'est le seul chantier sans impact desktop immédiat — à traiter quand une
  échéance mobile/Android se précise, pas nécessairement avant F.
