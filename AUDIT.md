# Audit technique — RusteeGear

> **Suivi des correctifs (2026-06-19).** Traités : **P1** (cache Lua), **P2** (rendu),
> **P3** (uniforms), **P4** (panics + indexation défensive), **P5** (undo `VecDeque`),
> **P6** (9 tests unitaires), **P7** (matrices de picking mises en cache),
> **P8** (chargement de scène asynchrone), **P9** (`RING_SEGMENTS` centralisé).
> Outillage : **CI GitHub Actions** (`.github/workflows/ci.yml` — fmt + clippy `-D warnings`
> + tests + cross-build Android/iOS), code rendu **clippy-clean**.
> Restant : **P10** (assets mobile).

**Date :** 2026-06-19
**Périmètre :** `src/` (~2400 lignes Rust), shaders WGSL, packaging.
**Verdict global :** base saine, bien commentée et bien découpée (état métier sans GPU dans `app/`, rendu pur dans `gfx/`, runtime isolé). Les axes d'amélioration concernent surtout les **performances** (rendu/scripting continus), la **robustesse** (panics, `unwrap`) et la **couverture de tests** (actuellement nulle).

---

## 1. Synthèse — priorités

| # | Sujet | Sévérité | Effort | Fichier |
|---|-------|----------|--------|---------|
| P1 | Recompilation Lua de chaque script à chaque frame | 🔴 Perf | Moyen | `app/mod.rs` |
| P2 | Rendu continu plein régime (double `request_redraw`, dt même hors Play) | 🟠 Perf/batterie | Faible | `lib.rs`, `app/mod.rs` |
| P3 | `model.inverse().transpose()` recalculé pour tous les objets chaque frame | 🟠 Perf | Faible | `gfx/renderer.rs` |
| P4 | Panics non gérés (`unwrap`/`expect`) sur des chemins d'init faillibles | 🟠 Robustesse | Moyen | `lib.rs`, `gfx/renderer.rs` |
| P5 | Undo : clone du `Vec` complet + `remove(0)` O(n) | 🟡 Perf/mémoire | Faible | `app/mod.rs` |
| P6 | Aucun test unitaire (math picking, ray/AABB, sérialisation) | 🟡 Qualité | Moyen | global |
| P7 | `view_proj()` / `.inverse()` recalculés dans des boucles de picking | 🟡 Perf | Faible | `app/mod.rs` |
| P8 | `reload_imported` synchrone bloque le thread UI au `Load` | 🟡 UX | Faible | `scene/mod.rs` |
| P9 | Code tactile dupliqué + magie de constantes (gizmo 320) | 🟢 Lisibilité | Faible | `lib.rs`, `gfx/renderer.rs` |
| P10 | Pas de chargement d'assets sur mobile (rfd désactivé, rien en remplacement) | 🟢 Fonctionnel | Moyen | `editor/mod.rs` |

---

## 2. Performances

### P1 — Le script Lua est recompilé à chaque frame, pour chaque objet 🔴
`run_script` (`app/mod.rs:589`) appelle `lua.load(src).exec()` à chaque frame. `load` **parse et compile** la source Lua à chaque appel. Avec N objets scriptés à 60 fps, c'est N×60 compilations/seconde de code identique.

**Recommandation :** précompiler les chunks. Conserver un cache `HashMap<u64 /*hash du script*/, mlua::Function>` (ou une `RegistryKey`), invalidé quand `obj.script` change. Idéalement, stocker le `Function` compilé sur l'objet/à côté à l'édition.
Bonus : éviter de recréer une `Table` neuve chaque frame — réutiliser une table persistante et n'écrire que les champs.

### P2 — Rendu continu à plein régime 🟠
- `lib.rs:118-119` : à chaque `RedrawRequested`, on rend **puis** on redemande un redraw ; en plus `about_to_wait` (`lib.rs:167`) redemande aussi. Le moteur tourne donc en boucle serrée en permanence, même scène statique, éditeur au repos. Le `PresentMode::Fifo` cale sur la vsync (ça ne « brûle » pas le GPU à 1000 fps) mais le CPU reconstruit l'UI egui + réécrit tous les uniforms à chaque frame → consommation/chauffe inutiles, critique sur mobile.
- `advance_play` (`app/mod.rs:419`) calcule `dt` via `Instant` même hors Play, et `last_frame` n'est mis à jour que là — correct, mais l'appel complet (poll imports, audio update, transitions) tourne chaque frame sans nécessité quand rien ne joue.

**Recommandation :** ne redessiner sur demande que lorsque c'est utile (`ControlFlow::Wait` + `request_redraw` ciblé sur événement/animation), ou au minimum supprimer le double `request_redraw` (garder soit `about_to_wait`, soit celui de `RedrawRequested`). En mode édition statique, passer en `Wait`.

### P3 — Matrice normale recalculée pour tout, chaque frame 🟠
`write_uniforms` (`gfx/renderer.rs:341-351`) fait `model.inverse().transpose()` par objet et réécrit le buffer pour **tous** les objets à chaque frame, même immobiles.

**Recommandation :**
- Pour des scales uniformes (cas courant ici), la matrice normale = la sous-matrice 3×3 du modèle ; éviter l'inverse.
- N'uploader que les objets dont le `Transform` a changé (drapeau « dirty » ou comparaison de hash). Combiné à P2, gros gain.
- Envisager un **seul** buffer modèle avec `has_dynamic_offset` plutôt qu'un buffer + bind group par objet (`sync_objects:307`), pour réduire le nombre de `write_buffer`/`set_bind_group`.

### P5 — Historique undo coûteux 🟡
`push_undo` (`app/mod.rs:139`) clone tout `scene.objects` à chaque modification, et `remove(0)` au-delà de 50 est O(n) (décale tout le Vec).

**Recommandation :** utiliser une `VecDeque` (`pop_front` O(1)). Pour de grandes scènes, passer à un journal de diffs (commande inverse) plutôt que des snapshots complets.

### P7 — Recalculs matriciels dans le picking 🟡
`ray` (`app/mod.rs:312`) fait `view_proj().inverse()` à chaque appel ; `project` recalcule `view_proj()` à chaque appel. `pick_ring` (`app/mod.rs:375`) appelle `project` jusqu'à 3×49 fois → 147 reconstructions de la view-proj pour un seul clic.

**Recommandation :** calculer `view_proj` (et son inverse) une fois en début d'interaction et le passer aux helpers.

---

## 3. Robustesse / gestion d'erreurs

### P4 — Panics sur chemins faillibles 🟠
- `lib.rs:93` `event_loop.create_window(attrs).unwrap()`
- `lib.rs:186/191` `EventLoop::new().unwrap()`, `run_app(...).unwrap()`
- `gfx/renderer.rs:86` `create_surface(...).unwrap()`, `:95` `.expect("Aucun adaptateur GPU")`, `:108` `.expect("Échec création du device")`

Sur desktop c'est tolérable ; sur **mobile** un panic = crash silencieux à froid sans diagnostic clair.

**Recommandation :** propager les erreurs d'init (`Result`) et logguer proprement (`log::error!`) avant une sortie contrôlée. Au minimum, messages d'erreur exploitables côté Android/iOS.

### Autres points
- `gfx/renderer.rs:501` `self.imported_gpu[i as usize]` : indexation directe par l'index stocké dans `MeshKind::Imported`. Si la liste `imported` et `imported_gpu` se désynchronisent (ex. `Load` qui `clear()` les GPU mais index invalide dans un fichier corrompu), risque de panic. Ajouter un `get(...)` défensif.
- `scene/mod.rs:116` `self.imported[i as usize]` même remarque pour `local_aabb`.
- `app/mod.rs:50` `*self.touches.values().next().unwrap()` : sûr car gardé par `len()==1`, mais fragile si la logique évolue.

---

## 4. Architecture / lisibilité

- **`advance_play` appelé depuis `render`** (`gfx/renderer.rs:415`) : la simulation (physique, scripts, audio) est pilotée par la cadence de rendu. Ça mélange logique et présentation et rend la simulation dépendante du framerate. Envisager une boucle de mise à jour séparée (idéalement pas fixe pour la physique).
- **P9 — Duplication tactile** : `lib.rs:51-57`, les branches `if self.orbiting` et `else` appellent toutes deux `PointerMove`. Simplifiable.
- **P9 — Constante magique** : `gfx/renderer.rs:255` buffer gizmo dimensionné « 320 » en dur alors que le besoin réel est `3*48*2 = 288` (translate/scale n'en utilise que 6). Dériver la taille d'une constante partagée `RING_SEGMENTS` (déjà répétée : `N=48` dans `app/mod.rs:376`, `app/mod.rs:438`, `renderer.rs:438`). Centraliser.
- **Scripts sur corps dynamiques** : en Play, les scripts s'exécutent (`app/mod.rs:458`) puis la physique **écrase** la pose des corps dynamiques (`physics.rs:116`). Pour un objet à la fois scripté et dynamique, le script est silencieusement inopérant sur la position. À documenter ou à arbitrer.
- **`default_physics`** (`scene/mod.rs:98`) duplique `PhysicsKind::None` ; `#[serde(default)]` suffirait si `PhysicsKind` dérivait `Default`.

---

## 5. Tests & outillage

### P6 — Aucun test 🟡
Des fonctions pures, déterministes et critiques ne sont pas couvertes :
- `ray_aabb` (`app/mod.rs:639`) — intersection slabs.
- `point_segment_dist` (`app/mod.rs:621`).
- `axis_basis` (`app/mod.rs:86`) — orthonormalité.
- Round-trip sérialisation `Scene` (save/load + `reload_imported`).
- `Transform::matrix`, conversions Euler↔Quat de l'inspecteur.

**Recommandation :** ajouter un module `#[cfg(test)]` par fichier. Ces tests sont rapides à écrire et protègent la math de picking lors des refactors.

### Outillage
- `clippy` n'est pas installé (`rustup component add clippy`) — à activer et faire passer en CI.
- Aucun CI détecté. Un workflow `cargo build --all-targets` + `cargo test` + `cargo clippy -- -D warnings` sur les 3 cibles (macOS/Android/iOS) sécuriserait les builds multiplateformes déjà fragiles.
- `.gitignore` : vérifier que `packaging/ios-xcode/build/` (volumineux, contient des caches `ModuleCache.noindex/`) est bien ignoré — il apparaît dans l'arbo et n'a rien à faire sous suivi git.

---

## 6. Mobile / packaging

- **P10** : sur iOS/Android, `rfd` est désactivé (normal), mais **aucun mécanisme de remplacement** pour importer un glTF ou un son → les fonctionnalités import/audio-fichier sont inaccessibles sur mobile. Prévoir des assets embarqués (`include_bytes!`) ou un picker natif.
- `scene_path` (`app/mod.rs:582`) écrit dans `$HOME/motor3derust_scene.json`. Sur iOS, `HOME` pointe vers le conteneur sandbox (OK) mais le commentaire « cwd vaut / en .app » mérite d'être validé sur device. Préférer un dossier Documents explicite.
- `required_limits: adapter.limits()` (`renderer.rs:102`) : bon choix (corrige le crash iOS mentionné dans l'historique git). À garder.

---

## 7. Quick wins (faible effort, bon ratio)

1. Supprimer le double `request_redraw` (P2) — 1 ligne.
2. `VecDeque` pour l'undo (P5).
3. Éviter `inverse().transpose()` pour scales uniformes (P3).
4. Centraliser `RING_SEGMENTS`/taille buffer gizmo (P9).
5. Cache du chunk Lua compilé (P1) — le plus gros gain perf.
6. Ajouter 5–6 tests unitaires sur la math (P6).
7. Indexations défensives `.get()` sur `imported`/`imported_gpu` (P4).

---

## Conclusion

Le code est **propre, idiomatique et lisible** ; le découpage état/rendu/runtime est exemplaire pour un projet de cette taille. Les gains les plus rentables sont : (1) **arrêter de recompiler le Lua chaque frame**, (2) **cesser le rendu/upload continu inconditionnel**, et (3) **introduire des tests** sur la couche math. Aucun problème de sécurité bloquant ; les panics d'init sont le principal risque de stabilité, surtout sur mobile.
