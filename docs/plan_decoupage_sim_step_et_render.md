# Plan de découpage — `sim_step` et `Renderer::render` (2026-09-03)

*Suite à la roadmap [docs/roadmapaudit3septembre.md](roadmapaudit3septembre.md), points 3.1 et
3.2, délibérément non commencés lors de cette session-là : les deux fonctions ont été lues
intégralement plutôt que découpées à l'aveugle, et ce document formalise ce que cette lecture a
révélé — les limites de découpage réelles, l'ordre de risque croissant, et le protocole de
vérification par lot. Même méthode que les 15 lots `AppState` et les 6 lots `build_ui` des
roadmaps précédentes : un lot = une extraction + build + Clippy + suite complète (dont
`net_tests`) + commit, jamais plusieurs lots à la fois.

## Pourquoi ces deux fonctions n'ont pas été traitées directement

- **`sim_step`** (`src/app/simulation.rs:824`, ~800 lignes) : scripts, spawn/destroy, régénération
  de vie, attaques à distance, ciblage IA (plafond d'agresseurs, portées de détection par
  archétype, éveil Furtive), pilotage physique du joueur, tout dans une seule fonction avec des
  emprunts mutables imbriqués (`self.scene.objects.iter_mut()` actif pendant que `self.scripting`,
  `self.touch`, `self.physics` sont aussi touchés). Une bonne partie de cette logique (IA,
  archétypes, ring-out) n'a **aucune couverture de test automatisée** — seul le jeu réel la
  vérifie.
- **`Renderer::render`** (`src/gfx/renderer/frame.rs:4`, 1125 lignes, **seule fonction du
  fichier**) : construction de l'UI egui, ~90 branches `if actions.X { app.Y() }` de dispatch,
  passes GPU (ombre, principale, bloom, tonemap, UI), gestion du profiler.

Un découpage mécanique sans plan explicite risquait soit d'introduire une régression de timing/
ordre invisible aux tests, soit de produire un diff trop gros pour être relu sérieusement.

## Méthode commune à toutes les vagues ci-dessous

Pour chaque lot :

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --features net_tests
cargo check --lib --target aarch64-apple-ios   # nécessite `rustup target add aarch64-apple-ios`
```

Pour un lot touchant `Renderer::render` (passes GPU) : ajouter

```bash
cargo test --test golden_render --test golden_skinning
```

Pour un lot touchant le ciblage IA / la boucle de scripts (`sim_step`) : les tests automatisés ne
suffisent pas — jouer une manche réelle (au moins `Scene::brawl_demo`, qui a un ring-out connu, et
une scène MMORPG avec plusieurs archétypes) avant de committer, pas seulement après coup. Noter
dans le message de commit ce qui a été rejoué manuellement.

Un seul lot par commit, poussé et vérifié en CI (6/6 vert) avant de passer au suivant — jamais
plusieurs extractions non vérifiées empilées.

---

## Partie A — `sim_step` (`src/app/simulation.rs`)

Structure réelle de la fonction, dans l'ordre où elle exécute (lignes au commit `9a39e10`, vont
dériver après le premier lot — se repérer par nom de variable/commentaire, pas par numéro) :

| # | Section | Lignes approx. | Risque |
|---|---|---|---|
| A1 | Avance des clips d'animation (`anim.time`, notifies croisés) | 826-864 | Faible |
| A2 | Zones de déclenchement (`triggered`/`exited`, `trigger_prev`) | 865-896 | Faible |
| A3 | Setup des accumulateurs (santé, `start_pos`, `tagged`) | 897-954 | Faible (glue) |
| A4 | Boucle de scripts (natif + wasm32, ~180 lignes) | 955-1136 | **Moyen** — gros mais mécanique |
| A5 | Application des sorties de script (spawn, items, dégâts, vibration, réverb) | 1137-1190 | Faible |
| A6 | Attaques à distance des créatures | ~1190 (`update_creature_ranged_attacks`) | Déjà une fonction séparée, rien à faire |
| A7 | Ciblage IA (cibles candidates, plafond par cible, portées Furtive/réseau) | ~1207-1580 | **Élevé** — le plus dense, le moins testé |
| A8 | Pilotage physique du joueur (joystick, saut, orientation) | ~1261-1416 | **Élevé** — imbriqué dans le même bloc `if let Some(phys) = &mut self.physics` que A7 |
| A9 | Application `player_facing`/`player_anim`, résolution physique | fin de fonction | Faible (glue) |

**A7 et A8 partagent le même bloc `if let Some(phys) = &mut self.physics { ... }`** — ce n'est pas
une erreur de lecture, ils sont réellement imbriqués l'un dans l'autre dans le code actuel. Les
séparer proprement fait partie du travail de A7/A8 ci-dessous, pas un prérequis distinct.

### Lot 3.1.A — `advance_animation_clips` (S)

```rust
/// Avance la lecture des clips (indépendant des scripts/tap actions) et renvoie
/// les marqueurs de notification franchis ce tick (`"anim:<nom>"`), à fusionner
/// dans `events_in` par l'appelant.
fn advance_animation_clips(scene: &mut Scene, dt: f32) -> Vec<String>
```

Fonction libre (pas de méthode `&mut self` nécessaire — ne touche que `scene`), placée dans
`simulation.rs` à côté de `sim_step`. Aucun changement de signature ailleurs. Test à ajouter dans
`simulation_tests.rs` : un objet avec `animation` dont le `time` avance de `dt`, et un cas avec un
marqueur (`notifies`) franchi qui produit bien `"anim:<nom>"` dans le vecteur retourné — logique
actuellement non testée isolément (seulement via le jeu complet).

### Lot 3.1.B — `run_object_scripts` (M)

La boucle de script est mécaniquement extractible : les emprunts qu'elle utilise
(`self.scene.objects.iter_mut()`, `self.scripting`, `self.touch`, `self.lua_vars`,
`self.debug_lines`, `self.physics.as_ref()`) restent valides à l'identique **à l'intérieur** d'une
méthode séparée, puisque ce sont des champs disjoints du même `self` — seul le site d'appel change
(un appel `self.run_object_scripts(...)` depuis `sim_step` au lieu du corps inline).

```rust
struct ScriptRunOutcome {
    events_out: Vec<String>,
    spawn_requests: Vec<(String, Vec3)>,
    item_add_requests: Vec<(crate::scene::ItemKind, u32)>,
    vibrations: Vec<f32>,
    reverb_requests: Vec<f32>,
    health: Option<f32>,
}

fn run_object_scripts(
    &mut self,
    dt: f32,
    time: f32,
    triggered: &std::collections::HashSet<usize>,
    exited: &std::collections::HashSet<usize>,
    events_in: &[String],
    tagged: &[(String, Vec3)],
    start_pos: &[Vec3],
    health_in: Option<f32>,
) -> ScriptRunOutcome
```

**Extraction pure** (copier-coller du corps de boucle actuel dans les deux branches
`#[cfg(target_arch = "wasm32")]`/`#[cfg(not(...))]`, sans changer une ligne de logique) — le risque
vient uniquement d'une erreur de câblage des accumulateurs dans `ScriptRunOutcome`, pas d'un
changement de comportement. Vérifier avec la suite complète **et** `net_tests` (les scripts
tournent aussi côté serveur headless). Pas de nouveau test nécessaire : le comportement est
inchangé, les tests existants de `simulation_tests.rs` sur les scripts continuent de le couvrir.

### Lot 3.1.C — `apply_script_outcomes` (S)

Regroupe A5 : application de `item_add_requests`, `spawn_requests` (avec rebuild physique
conditionnel), détection de coup encaissé (`damage_flash`/`camera_shake`), mise à jour
`hud_health`, reset des flags de tap, lecture des vibrations/réverb. Glue simple, extraction à
faible risque.

### Lot 3.1.D — pilotage physique du joueur (L, séparé de l'IA)

Extraire A8 (joystick/clavier/gyro → `phys.control`, orientation `player_facing`/`player_anim`)
**avant** A7 (IA), pour isoler le bloc `if let Some(phys) = &mut self.physics` en deux appels
successifs plutôt qu'un seul bloc mêlé :

```rust
fn drive_local_and_networked_players(&mut self, phys: &mut Physics, dt: f32)
    -> (Vec<(usize, f32)>, Vec<(usize, bool)>) // player_facing, player_anim
```

Risque élevé parce que cette section a des invariants de *feel* documentés en commentaire (vitesse
d'amortissement de rotation, priorité tank W/S vs auto-visée manette) sans test automatisé — à
vérifier en jouant réellement (déplacement clavier, manette, tactile) avant de committer.

### Lot 3.1.E — ciblage et poursuite IA (L, le plus risqué — à faire en dernier)

A7 : cibles candidates (convoi/joueurs réseau/solo), `chase_blocked` (visée gelée, sync serveur),
plafond de chasseurs actifs par cible, portées Furtive/réseau, éveil Furtive avec effet sonore,
chasse scriptée vs dynamique. C'est le cœur du chantier Chasse 4.1 (silhouettes de classe,
GAMEDESIGN_EN_LIGNE.md §3.2) — la section la plus documentée en commentaires *justement* parce que
c'est la plus subtile (cf. le commentaire sur `brawl_demo_rival_survives_two_hits_then_falls_on_
the_third`, un test qui casserait silencieusement si le ring-out était mal préservé).

Recommandation : ne PAS viser une extraction complète en un seul lot. Découper en sous-lots
séparés et testés indépendamment :
- E1 : calcul de `candidate_targets` et `chase_blocked` (lecture seule, extraction facile).
- E2 : le tri par cible + plafond `MAX_ACTIVE_CHASERS_PER_TARGET` (la logique la plus dense).
- E3 : l'application du mouvement (`phys.control`/chasse scriptée) et l'éveil Furtive.

Avant de committer E1-E3 : rejouer `Scene::brawl_demo` (ring-out), une scène MMORPG avec les 4
archétypes (Traqueuse/Meute/Colosse/Furtive) et au moins 2 joueurs réseau (plafond de chasseurs
par cible, calcul multi-cibles). Si le risque perçu en cours de route dépasse le bénéfice,
s'arrêter à un sous-lot déjà validé plutôt que de forcer la suite — c'est le même arbitrage qui a
fait reporter 2.4 (outils) dans la roadmap du 3 septembre.

### Lot 3.1.F — queue de fonction (S)

Reste de `sim_step` après le bloc physique (résolution `player_facing`/`player_anim`,
réconciliation). À regarder une fois A-E faits ; probable qu'il ne reste alors qu'un orchestrateur
de 60-80 lignes.

---

## Partie B — `Renderer::render` (`src/gfx/renderer/frame.rs`)

Contrairement à `sim_step`, cette fonction a des frontières de phase nettes — mais elle est seule
dans son fichier (aucune méthode sœur), donc chaque extraction crée une nouvelle méthode
`impl Renderer` dans `frame.rs` ou un fichier voisin du module `gfx/renderer/`.

| # | Section | Lignes approx. | Risque |
|---|---|---|---|
| B1 | Garde d'acquisition de surface (`get_current_texture`, `editor.take()`) | 4-41 | Nul — laisser tel quel |
| B2 | Construction UI mode Player (`editor.run_player_overlay`) | 54-122 | Faible |
| B3 | Construction UI éditeur (`editor.run(...)`, jusqu'aux `actions`) | 124-208 | Faible |
| B4 | Dispatch des ~90 actions UI (`if actions.X { app.Y() }`) | 212-629 | **Le plus gros, le plus sûr** |
| B5 | Avance simulation + sync GPU (`advance_play`, `sync_objects`/`sync_imported`/`sync_textures`) | 658-699 | Faible, déjà des appels à des méthodes existantes |
| B6 | Géométrie des gizmos | 701-834 | Faible — autonome |
| B7 | Géométrie des lignes de debug | 836-863 | Faible — autonome |
| B8 | Passe d'ombre (encoder GPU) | 865-941 | **Élevé** — vérifié par les goldens |
| B9 | Passe principale (ciel, grille, objets, gizmos, debug, skinné) | 946-1065 | **Élevé** — vérifié par les goldens |
| B10 | Bloom + tonemap | 1070-1092 | Faible — délègue déjà à `self.render_bloom`/`self.tonemap` |
| B11 | Peinture UI + comptage draw calls + submit + present | 1094-1128 | Faible |

### Lot 3.2.A — `build_gizmo_geometry` (S)

```rust
/// Construit et pousse la géométrie des gizmos (lumières, caméra de jeu, gizmo de
/// manipulation) dans `self.gizmo_vbuf`. Renvoie le nombre de sommets à dessiner
/// (0 en mode player/aperçu mobile, sans reconstruire quoi que ce soit).
fn build_gizmo_geometry(&mut self, app: &AppState) -> u32
```

Autonome : lit `app.scene`/`app.selection`/`app.gizmo_mode`/`app.selected_light`, écrit dans
`self.queue`/`self.gizmo_vbuf`. Extraction quasi triviale.

### Lot 3.2.B — `build_debug_geometry` (S)

Même idée pour les lignes de debug (`app.debug_lines`, vidées après construction).

### Lot 3.2.C — `apply_editor_actions` (M, mais le plus mécanique)

```rust
/// Traduit chaque action de l'UI éditeur (`EditorActions`) en appel `AppState`/
/// `Editor` correspondant — extraction pure de `render`, aucune logique nouvelle.
fn apply_editor_actions(&mut self, app: &mut AppState, editor: &mut Editor, actions: EditorActions)
```

C'est la section B4 : 417 lignes, mais une séquence plate de `if actions.champ { ... }`
indépendants les uns des autres — aucun état partagé entre deux branches, donc le risque de
régression est en réalité **plus bas** que la plupart des lots plus courts de ce document malgré
sa taille. Bon candidat pour tester la mécanique du découpage sur ce fichier avant de s'attaquer
aux passes GPU.

### Lot 3.2.D — construction UI (player + éditeur) (M)

B2 et B3 extraits séparément (deux méthodes, une par branche `app.player`), une fois B4 fait (pour
ne pas dupliquer le passage des `actions` en paramètre).

### Lot 3.2.E — passe d'ombre (L)

```rust
/// Passe de profondeur (carte d'ombre) : dessine `draw_plan` (objets statiques
/// visibles, groupés par mesh) puis les objets skinnés. Renvoie le nombre de
/// `draw_indexed` émis (accumulé dans `scene_draw_calls` par l'appelant).
fn render_shadow_pass(&self, encoder: &mut wgpu::CommandEncoder, app: &AppState) -> u32
```

**Vérification obligatoire par les goldens** (`golden_render`, `golden_skinning`) à chaque essai —
un rendu qui diffère d'un seul pixel de tolérance doit bloquer le commit, pas seulement une
relecture visuelle.

### Lot 3.2.F — passe principale (L, la plus risquée du fichier)

Même méthode que 3.2.E pour la passe principale (ciel, grille, objets, gizmos, debug, skinné) —
c'est la section qui accumule le plus de bind groups/pipelines actifs simultanément
(`camera_bind_group`, `shadow_bind_group`, `models_bind_group`, texture par lot). Faire 3.2.E
d'abord pour valider la méthode sur la passe la plus simple des deux.

### Lot 3.2.G — queue de fonction (S)

Bloom/tonemap (déjà des appels à des méthodes existantes, quasi rien à faire), peinture UI,
comptage des draw calls, soumission, présentation — probable qu'il ne reste alors qu'un
orchestrateur de 40-60 lignes appelant B2-B11 dans l'ordre.

---

## Ordre global recommandé

1. `sim_step` A1-A3 (S, faible risque, échauffement).
2. `render` B-A, B-B, B-C (S/M, faible risque, valide la méthode sur ce fichier).
3. `sim_step` B (M) — le plus gros extract-method mécanique de `sim_step`.
4. `render` D (M).
5. `sim_step` C, F (S).
6. `render` E puis F (L, goldens obligatoires) — commencer par la passe d'ombre, plus simple.
7. `sim_step` D (L, pilotage joueur — playtest manuel).
8. `sim_step` E, en sous-lots E1→E2→E3 (L, le plus risqué de tout le plan — playtest manuel
   systématique, s'arrêter à un sous-lot validé si le risque perçu augmente en cours de route).

Cet ordre place le sous-lot E (ciblage IA) en tout dernier, une fois la méthode éprouvée sur des
extractions plus sûres — cohérent avec la prudence qui a fait reporter ce chantier lors de la
session du 3 septembre plutôt que de le tenter sans plan.

**Décision utilisateur (2026-09-03, en cours d'exécution)** : après les 5 premiers lots (tous
mécaniques, faible risque), demandé explicitement si le pilotage joueur (D) et le ciblage IA (E)
devaient s'arrêter pour un playtest manuel avant de committer, comme ce document le recommande.
Réponse : continuer avec la seule vérification automatisée (tests + goldens), sans playtest
manuel — garantie plus faible que la recommandation initiale de ce plan pour ces deux lots
spécifiquement, accepté en connaissance de cause. Signalé dans les messages de commit de D et E.

## Suivi

| Lot | Description | Statut |
|---|---|---|
| 1. `sim_step` A | `advance_animation_clips` | ✅ `c61b22a`, [run 33790239666](https://github.com/lberthod/rusteegear/actions/runs/33790239666) 7/7 vert |
| 2. `render` B-A/B-B | `build_gizmo_geometry`, `build_debug_geometry` | ✅ `62b9ded`, [run 33795980310](https://github.com/lberthod/rusteegear/actions/runs/33795980310) 7/7 vert. `render` 1125 → ~950 lignes |
| 2. `render` B-C | `apply_editor_actions` (dispatch des ~90 actions) | ✅ `5504f68` — `render` 1125 → ~530 lignes cumulé |
| 3. `sim_step` B | `run_object_scripts` | ✅ `7736ffc`, [run 33798670774](https://github.com/lberthod/rusteegear/actions/runs/33798670774) 7/7 vert. `sim_step` ~800 → ~570 lignes cumulé |
| 4. `render` D | Construction UI (player + éditeur) | ✅ `f5f5a34`, [run 33800510694](https://github.com/lberthod/rusteegear/actions/runs/33800510694) 7/7 vert. `render` 1125 → ~370 lignes cumulé |
| 5. `sim_step` C | `apply_script_outcomes` | ✅ `ffac132`, [run 33801527708](https://github.com/lberthod/rusteegear/actions/runs/33801527708) 7/7 vert. `sim_step` ~800 → ~525 lignes cumulé. F (queue) reporté après D/E, non actionnable avant |
| 6. `render` E | Passe d'ombre (`render_shadow_pass`) | ✅ `5afc090`, [run 33802660171](https://github.com/lberthod/rusteegear/actions/runs/33802660171) 7/7 vert, goldens inchangés au pixel près. `render` 1125 → ~310 lignes cumulé |
| 6. `render` F | Passe principale | ⏳ |
| 7. `sim_step` D | Pilotage physique du joueur | ⏳ |
| 8. `sim_step` E1-E3 | Ciblage et poursuite IA | ⏳ |
