# Intégration du pack « siège du hameau » dans la scène servie

## Objectif

Les 40 assets `siege_*.glb` générés dans `creation3DBlendersuite.md` /
`creationAnimation3DBlendersuite.md` existent dans `assets/models/` mais ne
sont visibles ni dans l'éditeur ni en `cargo run -- -- player` : la scène
réellement chargée au démarrage (`Scene::embedded_player()`,
`src/scene/demos.rs`) vient de `assets/player_scene.json`, un JSON figé
compilé dans le binaire (`include_str!`), pas de `Scene::hameau_gdd_demo()`
(qui construit les remparts en `box_seg` plats et n'est reliée qu'aux menus
Démos de l'éditeur).

## Mécanisme existant (vérifié, pas inventé)

- `make_app()` (`src/lib.rs:720-745`) appelle `Scene::embedded_player()`
  (`src/scene/demos.rs:8932-8947`) pour l'éditeur ET le player — **un seul
  fichier source de vérité au runtime**.
- Le précédent d'intégration (pack hameau) est un test `#[ignore]`
  `sync_embedded_scene_hameau_from_the_demo` (`src/scene/mod.rs:3287`) : il
  reconstruit `assets/player_scene.json` en entier à partir de
  `Scene::hameau_gdd_demo()` (objets + imports), ne conservant de l'ancien
  fichier que `Joueur`, `groups`, `point_lights`, `mobile`, `camera_follow`,
  `game_camera`, `version`, `hud_layout`, `hud_widgets`. Il renumérote tous
  les imports en `bundle://mNN_<fichier>` (`i` = index dans le vecteur).
- `bundle://` est résolu via `include_dir!("assets/bundle")`
  (`src/assets.rs:13`) : les octets sont embarqués **à la compilation**
  depuis ce dossier disque. Faire tourner le test ne copie PAS les fichiers
  dans `assets/bundle/` — seul le JSON change. Il faut copier/renommer
  manuellement chaque `assets/models/<fichier>.glb` vers
  `assets/bundle/mNN_<fichier>.glb` selon la numérotation assignée, puis
  recompiler.

## Plan d'exécution

1. **Modifier `Scene::hameau_gdd_demo()`** (`src/scene/demos.rs:6417-8208`) :
   - Remplacer les 8 `box_seg` de remparts par les nouveaux modules
     (`siege_wall_segment`, `siege_wall_corner`, `siege_tower`,
     `siege_gate_closed`/`siege_gate_burning`) posés sur le même périmètre
     exact (`HALF=24`, `GATE_HALF=2.5`, `TRIM=5`) — même géométrie de
     couloirs de vague (`in_corridor`), juste habillée. Un petit helper
     `poser_run` répète le module de mur sur la longueur exacte d'un pan
     (échelle X non uniforme si la longueur ne divise pas rond par 4 m).
   - Tours (`siege_tower`) aux 2 coins pleins (Nord-Ouest, Sud-Est) ; les
     coins Nord-Est/Sud-Ouest restent des brèches ouvertes (inchangé).
   - Portes : Nord/Sud en `siege_gate_closed`, Est/Ouest en
     `siege_gate_burning` (les deux variantes doivent être présentes dans la
     carte, l'état réel « embrasé » au signal de vague reste un chantier de
     gameplay séparé, hors scope ici).
   - Remplacer « Feu communal » (`hamlet_bonfire.glb`) par
     `siege_communal_brazier.glb` à la même position (0,0) ; garder le
     réglage `emissive = 1.2` en fin de fonction.
   - Ajouter les ~28 assets restants en dressing ponctuel : chemin de ronde
     (torches, chemin de ronde décoratif, escaliers, bastions, module de
     créneau) sur les remparts ; poterne à la brèche Sud-Ouest ; herse,
     caisse de réserve, pieux, bannière de vague au(x) porte(s) ; chariot de
     braises sur le chemin entre porte Nord et place ; autel de l'Aînée +
     cage du chef dans la lande (zone dégagée, loin du camp de
     chasseurs/mare/vergers/prairies existants) ; tas de trophées près du
     camp de chasseurs ; trophée/portail/panneau/statue en dressing de
     place ; marqueurs de zone au sol sur les 6 lisières de vague existantes
     (mêmes coordonnées que les `marker()` déjà posés) ; corne d'alerte aux
     portes ; bannière de mode + fanion près de la place ; boulet en petit
     tas décoratif près d'une porte ; 10 assets de lande (rocher, arbre mort,
     ossements, menhir, broussaille, mare stagnante, ravine, poteau en
     ruine, cairn, brume) dispersés dans l'anneau extérieur, en évitant les
     zones déjà nommées (îlots, camp, mare aux nénuphars, prairies, verger).
   - Placement au meilleur effort (coordonnées choisies à la main pour rester
     à distance des zones existantes), pas un compactage garanti sans le
     moindre chevauchement visuel — c'est un paysage, pas une grille.

2. **Régénérer la scène embarquée** :
   `cargo test --lib scene::tests::sync_embedded_scene_hameau_from_the_demo -- --ignored`
   (chemin exact du test à vérifier) → réécrit `assets/player_scene.json`.

3. **Copier les fichiers bundle** : script qui lit le nouveau
   `player_scene.json`, extrait `imported[*].path` (`bundle://mNN_x.glb`),
   et copie `assets/models/x.glb` → `assets/bundle/mNN_x.glb` pour chaque
   entrée (écrase les anciens fichiers `mNN_*` désormais mal numérotés).

4. **Recompiler** (`cargo build`), vérifier avec `cargo run -- -- player` et
   dans l'éditeur qu'aucune erreur `poser()`/`load_gltf` n'apparaît dans les
   logs (fichier manquant, chemin faux) et que le rempart/la place ont bien
   changé visuellement.

5. **Faire tourner la suite de tests existante** touchée par ce changement
   (`the_embedded_scene_creatures_match_the_demo`, tests d'authoring des
   vagues, tests de synchro scène embarquée) pour s'assurer qu'aucune
   régression n'est introduite.

6. Commit.

## Hors scope explicite

- Le déclenchement réel des animations en jeu (porte qui s'ouvre à un
  événement, chariot qui avance en mode Escorte, cage du chef qui se libère)
  reste un chantier de gameplay séparé (`AnimationState::set_clip` côté
  code Rust) — l'intégration ne fait que poser les assets dans le décor,
  au repos (pose neutre/clip Idle par défaut).
- Aucun nouvel asset n'est généré ici : uniquement du placement de ceux déjà
  livrés.
