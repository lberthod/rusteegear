# HerRoad — course arcade façon Trackmania

Contre-la-montre en 3 tours sur un circuit en huit (pont, collines, virages relevés, 2 sauts,
portion de terre, portion de glace, 5 turbos, 6 points de passage), avec fantôme du meilleur
run, écart au meilleur tour à chaque point de passage et médailles Bronze → Auteur.

## Le monde

Heure dorée (soleil bas, ombres longues, brume tiède), relief généré autour du circuit (talus qui
portent la route, prairie vallonnée, montagnes à l'horizon), forêt et rochers réutilisant les
modèles glTF de la démo Rivière (~460 arbres, ~40 rochers, 120 FPS sur un M-series), tribunes avec
public et auvent au départ, banderoles, lampadaires, pont sur piliers. Chaussée détaillée : bordures
rouge/blanc, filets blancs, ornières, ligne médiane pointillée, damier d'arrivée.

## Ergonomie

- Écran **PRÊT ?** : le compte à rebours ne démarre qu'au premier geste (accélérer, freiner, diriger),
  on peut d'abord regarder le circuit (caméra C / −).
- HUD sur panneaux sombres : chrono, tour, point de passage, meilleur tour et record, écart au meilleur
  tour à chaque point de passage, vitesse avec jauge, indicateur TURBO.
- Alerte **SENS INTERDIT** après 1 s à contresens ; chute dans le vide ou sous le relief = retour
  automatique au dernier point de passage.
- Recommencer (X / +) et revenir au point de passage (Y) demandent un **appui prolongé** : un bouton
  parasite (manette qui bruite) ne casse jamais la course.
- L'aide des commandes s'affiche avant le départ puis disparaît après 12 s de course.

## Lancer

```bash
cargo run --release --bin herroad          # jeu autonome, plein écran joueur
cargo run --release -- --player --demo=herroad
# éditeur : console → « demo herroad », puis Play ; web : ?scene=herroad
```

## Commandes

| Action | Manette (Switch Pro / Joy-Con, Xbox, PlayStation) | Clavier |
|---|---|---|
| Accélérer | ZR (analogique) ou A | ↑ / W |
| Freiner, marche arrière | ZL (analogique) ou B | ↓ / S |
| Diriger | stick gauche ou croix | ← → / A D |
| Frein à main (dérapage) | R ou L | Espace |
| Retour au dernier point de passage | Y | Retour arrière |
| Recommencer la course | X ou + | Entrée / R |
| Changer de caméra (poursuite, éloignée, capot) | − ou clic stick droit | C |

Le record de temps total est gardé dans `~/.motor3derust/herroad_best.txt`.

## Architecture

| Fichier | Rôle |
|---|---|
| `src/racing/track.rs` | circuit : spline fermée → repères tous les 2 m, maillages route/barrières, sol et murs interrogeables (même géométrie que l'affichage) |
| `src/racing/car.rs` | voiture arcade (moteur, frein, adhérence, dérapage, gravité sur pente, vol, barrières) |
| `src/racing/race.rs` | compte à rebours, points de passage, tours, écarts, fantôme, médailles |
| `src/racing/terrain.rs` | relief autour du circuit : talus, prairie, montagnes, hauteur du sol pour la détection de chute |
| `src/racing/bot.rs` | pilote automatique de test (boucle les 3 tours en ~100 s, sert de repère aux médailles) |
| `src/scene/demos/herroad.rs` | scène 3D et HUD |
| `src/app/race.rs` | branchement moteur : pas de simulation, caméra de poursuite, HUD, sons, record |
| `assets/herroad/panel_*.png` | fonds sombres du HUD (natif seulement) |
| `examples/herroad_map.rs` | vue de dessus du circuit + tableau des fractions du tour |
| `examples/gen_herroad_preview.rs` | captures headless (`docs/img/herroad_*.png`) |

Pour modifier le circuit : éditer `TrackSpec::herroad()` (points de contrôle, sauts, zones),
relancer `herroad_map` pour voir le tracé, puis `cargo test racing` (le bot doit toujours finir).

## Limites connues

- La sensation de conduite (adhérence, rayon de braquage, poids du dérapage) est réglée par
  calcul et par le bot, pas encore validée à la manette : les constantes sont en tête de
  `car.rs` (`drive_on_ground`).
- Pas d'adversaires ni de multijoueur : contre-la-montre uniquement.
- Le build web (`?scene=herroad`) n'a pas été testé ; les panneaux du HUD n'y sont pas fournis.
- Pas de son de moteur : le moteur audio n'expose pas de boucle à hauteur variable (seuls des effets ponctuels).
