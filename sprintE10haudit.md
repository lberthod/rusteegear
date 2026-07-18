# Sprint E (10h) — Grammaire d'archétypes de créatures — Rapport final

> Exécute [sprint10audit.md](sprint10audit.md), Phase E — Sprints 10 et 11.
> Retour : [auditGDD10h.md](auditGDD10h.md) · [GDD_MMORPG.md](GDD_MMORPG.md) §5.4.

## Statut : ✅ Phase E terminée (Sprints 10 et 11)

Les 4 archétypes du GDD §5.4 (Traqueuse / Meute / Colosse / Furtive) existent désormais
comme un seul paramètre sur `AiChaser` — conformément à la contrainte du GDD
(« paramètres sur le même `AiChaser`, pas de nouveaux systèmes ») — et sont
distinguables en Play par leur comportement de poursuite.

## Ce qui a été fait

### `Archetype` (nouveau, `src/scene/mod.rs`)
- `pub enum Archetype { Traqueuse, Meute, Colosse, Furtive }`, `#[default] = Traqueuse`
  (comportement historique inchangé pour tout `AiChaser` qui ne précise pas d'archétype).
- `Archetype::speed_multiplier()` : Traqueuse `1.0`, Meute `1.25`, Colosse `0.65`,
  Furtive `1.5` (vitesse accrue une fois éveillée).
- `AiChaser` gagne un champ `archetype: Archetype` (`#[serde(default)]` — rétrocompatible
  avec toute scène JSON existante sans ce champ).

### Poursuite (`src/app/simulation.rs`, boucle de pilotage IA)
- Nouvelle constante `FURTIVE_DETECT_RANGE = 5.0` (< `CHASER_DETECT_RANGE` = 9 m).
- La Furtive reste immobile tant que la cible la plus proche est au-delà de
  `FURTIVE_DETECT_RANGE` — appliqué **en toute circonstance** (contrairement à
  `CHASER_DETECT_RANGE`, qui reste volontairement réseau-uniquement pour ne pas casser
  le ring-out de `brawl_demo`) : c'est ce délai d'éveil court, disponible aussi en solo,
  qui permet le contre-jeu « l'Éclaireur la déclenche de loin » du GDD.
- La vitesse effective de poursuite = `AiChaser::speed * archetype.speed_multiplier()`.
- Le plafond `MAX_ACTIVE_CHASERS_PER_TARGET` (2) et le rang par distance restent
  identiques pour tous les archétypes, y compris Meute — la coordination à plusieurs
  se fait donc « dans la limite du plafond existant », comme demandé par l'audit
  (pas de règle spéciale à contourner ni de risque de submersion instantanée).

### Casting par prefab (`src/scene/demos.rs`)
- `zombies_demo` : Rôdeur → Traqueuse, Coureur → Meute, Brute → Colosse.
- `roguelike_demo` : Gobelin → Meute, Squelette → Furtive, Ogre → Colosse.
- `brawl_demo` (rival unique) laissé en Traqueuse par défaut (comportement inchangé).
- `MONSTER_DECOR`/`mmorpg_demo` intentionnellement **non touchés** : purement décoratifs
  (aucun `ai_chaser`) ou pilotés par script Lua, hors du périmètre `AiChaser` de ce sprint
  (cf. note de scope ci-dessous).

### Tests (`src/app/mod.rs`)
Nouveau helper `chaser_distance_moved(x, archetype, steps)` isolant chaque scénario
dans son propre `AppState` à un seul chasseur — nécessaire car plusieurs chasseurs visant
le **même** joueur retombent sur `MAX_ACTIVE_CHASERS_PER_TARGET` et faussent toute
comparaison de vitesse (piège rencontré en écrivant le premier jet de test, corrigé).
- `furtive_archetype_stays_asleep_until_the_player_enters_its_shorter_wake_radius` :
  immobile à 7 m (hors `FURTIVE_DETECT_RANGE` mais sous `CHASER_DETECT_RANGE`), fonce et
  dépasse une Traqueuse à distance égale une fois sous 5 m.
- `creature_archetypes_produce_visibly_different_chase_speeds` : Colosse < Traqueuse <
  Meute sur la distance parcourue à distance/temps égaux.
- Suite `chaser`/`archetype` complète : **9 tests verts** (7 préexistants + 2 nouveaux),
  aucune régression sur le plafond, la portée réseau ou la poursuite multi-joueurs.

## Livrables vérifiés (définition de « terminé », Phase E)

- [x] Sprint 10 — Traqueuse et Meute visiblement différentes (vitesse standard vs
  accrue, coordination dans la limite du plafond existant).
- [x] Sprint 11 — Colosse (poursuite ralentie) et Furtive (éveil tardif à portée
  réduite, vitesse accrue une fois éveillée) ajoutées ; les 4 archétypes du GDD §5.4
  sont désormais distinguables en Play.
- [x] Tests IA existants non cassés (`src/app/mod.rs` autour des anciennes lignes
  2456-2559, désormais décalées par les nouveaux tests insérés juste avant).
- [x] `MAX_ACTIVE_CHASERS_PER_TARGET` reste valable pour tous les archétypes (aucune
  dérogation ajoutée).

## Vérification

- `cargo build --lib` : ✅.
- `cargo test --lib` : **519 passed; 0 failed; 7 ignored** (suite complète, pas
  seulement les tests archétypes).
- `cargo fmt --check` : ✅, propre.
- `cargo clippy --lib --tests -- -D warnings` : **non concluant cette session** — bloqué
  par du code mort dans `src/gfx/lod.rs` (fichier non versionné, en cours d'écriture par
  une autre session Claude Code active en parallèle sur ce dépôt pendant ce sprint, cf.
  note ci-dessous), sans rapport avec ce Sprint E. Aucun avertissement clippy détecté
  sur les fichiers de ce sprint avant que la compilation ne s'arrête sur ce fichier tiers.

## Portée non couverte (hors Sprint 10-11, à noter pour la suite)

- Le pack `MONSTER_DECOR` (~45 modèles `monster_*.glb`, casting suggéré par le GDD pour
  les 4 archétypes) reste purement décoratif — aucun `ai_chaser`/`combat`. Le convertir
  en spawns combattants (mode Vagues ou un futur `RoundObjective`) est un sprint à part,
  non demandé par l'audit Phase E.
- `mmorpg_demo` (26 créatures `creature*.glb`) reste piloté par script Lua
  (`creature_wander_script`/`creature_bite_script`), sans `AiChaser` : lui donner un
  archétype demanderait d'étendre les générateurs de script, hors périmètre de ce sprint.
- Les axes « PV réduits » (Meute) et « PV élevés, contact fort » (Colosse) du GDD §5.4
  relèvent de `Combat`, pas d'`AiChaser` — non traités ici (le sprint ne portait que sur
  la poursuite) ; actuellement les deux archétypes n'ajustent que la vitesse.

## Re-audit — a posteriori, sans toucher aux fichiers des autres phases

Relecture critique du travail ci-dessus, strictement limitée aux 4 fichiers de ce sprint
(`src/scene/mod.rs`, `src/scene/demos.rs`, `src/app/simulation.rs`, `src/app/mod.rs`) —
aucun fichier appartenant à une autre phase (A/B/C/D/F/G, en cours en parallèle) n'a été
rouvert ni modifié pour cette relecture.

### Bug trouvé et corrigé
- **Confusion terminologique dans `zombies_demo`** (`src/scene/demos.rs`) : le
  commentaire et le nom local `Kind` (profils d'auteur maison — Rôdeur/Coureur/Brute,
  stats/couleur/dégâts) utilisaient déjà le mot « archétype » avant ce sprint, pour un
  concept différent du nouveau `Archetype` (grammaire GDD §5.4) porté par le même objet.
  Risque réel pour un futur lecteur (humain ou agent) : confondre les deux, ou modifier
  l'un en croyant toucher l'autre. **Corrigé** : les 3 commentaires concernés
  reformulés en « profil(s) de monstres », `Archetype` réservé au terme « archétype » ;
  un commentaire ajouté sur `struct Kind` explicite la distinction. Recompilé, reformaté
  (`rustfmt --edition 2024`, propre) et retesté (`chaser`, `zombies_demo`,
  `roguelike_demo` : 13/13 verts) après ce changement.

### Points vérifiés, jugés corrects tels quels (pas de changement)
- **Gating Furtive vs réseau** : les deux portées de détection (`CHASER_DETECT_RANGE`
  réseau-uniquement, `FURTIVE_DETECT_RANGE` toujours actif) s'appliquent en séquence
  sans se contredire dans aucune combinaison solo/réseau × proche/loin — revérifié
  manuellement sur les 4 cas (< 5 m, 5-9 m, > 9 m solo, > 9 m réseau).
- **Multiplicateurs de vitesse** (Traqueuse 1.0, Meute 1.25, Colosse 0.65, Furtive 1.5) :
  écart volontairement marqué pour rester « distinguable en Play » sans instrumentation ;
  cumulés aux vitesses déjà différenciées par prefab (`k.speed`), l'écart Colosse/Meute
  peut devenir important (ex. Brute 1.8×0.65≈1.17 vs Gobelin 3.2×1.25=4.0) — à surveiller
  en playtest si un futur sprint équilibrage y touche, mais conforme à la demande du
  sprint (« sont tous distinguables en Play »), pas un bug.
- **`sprint10audit.md` décrit la Traqueuse comme « rapide et isolée »**, alors que
  `GDD_MMORPG.md` §5.4 (source de vérité citée en tête du fichier d'audit) la définit
  par des « valeurs standard ». Choix fait : suivre le GDD (multiplicateur 1.0), le
  wording du plan de sprint n'étant qu'une paraphrase informelle — aucune contradiction
  de fond, à noter pour qui relira ce sprint plus tard.
- **Casting Coureur→Meute et Gobelin→Meute** alors que ces prefabs spawnent seuls (un
  par manche/salle) : cohérent quand même, l'archétype module la *poursuite*
  individuelle (vitesse), pas le nombre de spawns — aucune règle du sprint n'exige un
  groupe physique pour porter l'archétype Meute.
- **`MAX_ACTIVE_CHASERS_PER_TARGET`** : confirmé identique pour les 4 archétypes,
  aucune dérogation ajoutée nulle part dans la boucle de pilotage.

### Reste à faire (hors ce sprint, à planifier séparément si voulu)
1. **PV réduits (Meute) / PV élevés (Colosse)** (`GDD §5.4`) : nécessite de toucher
   `Combat`, explicitement laissé de côté (cf. « Portée non couverte » ci-dessus) —
   pas un oubli, un choix de périmètre déjà documenté.
2. **Vérification visuelle en Play** : la différenciation n'a été validée que par tests
   unitaires sur la simulation (`cargo test`), pas par une session de jeu réelle (pas
   d'outil de preview navigateur applicable à ce jeu desktop Rust/wgpu) — recommandé
   avant de considérer Sprint 10-11 définitivement clos côté gameplay/lisibilité
   (silhouette + vitesse perçue « à 20 m, de nuit » comme l'exige le GDD).
3. **`cargo clippy` non concluant** (cf. section Vérification) — à relancer une fois
   que les fichiers tiers en cours d'écriture (`src/gfx/lod.rs`, `src/gfx/texcompress.rs`)
   seront stabilisés par leur session respective, pour confirmer que les 4 fichiers de
   ce sprint sont clippy-clean sous `-D warnings` (probable au vu de l'absence de tout
   diagnostic les concernant dans les runs partiels obtenus).

## Note — session concurrente pendant ce sprint

Une autre session Claude Code était active en parallèle sur ce dépôt pendant ce sprint
(travail visible : `ClientMsg::Join::objective`, `GameEvent::PlayerDown::cause`,
`AppState::death_cause`, `editor::run`/`run_player_overlay`, `src/gfx/lod.rs` — probablement
les Phases A/C/D du même plan). `cargo build`/`cargo test` ont dû être retentés plusieurs
fois le temps que ses modifications se stabilisent (fichiers `src/net/*`, `src/bin/server.rs`,
`src/app/health.rs`, `src/app/network_client.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs`,
`src/gfx/lod.rs`, non touchés par ce Sprint E si ce n'est l'ajout mécanique de
`..Default::default()` partout où `AiChaser { speed: ... }` était construit littéralement,
rendu nécessaire par le nouveau champ `archetype`). Aucun conflit de fichier réel : les
deux corpus de changements restent disjoints (cf. mémoire
`concurrent-sessions-hazard` — vérification appliquée avant chaque écriture/commit).
