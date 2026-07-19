# Broken Scene — la scène volontairement cassée

Le jumeau défectueux de [first_game](../first_game/README.md) : chaque objet
porte une panne **délibérée**, pour vérifier (et montrer) comment RusteeGear
se comporte quand tout ne va pas bien. Le moteur ne doit **jamais** planter en
l'ouvrant — il doit nommer ce qui cloche et laisser éditer.

| Objet | Panne | Comportement attendu |
| --- | --- | --- |
| Statue absente | mesh importé dont l'asset n'existe pas (`asset://modele_disparu.glb`) | erreur console `Rechargement de asset://modele_disparu.glb échoué : …`, objet sans géométrie, scène utilisable |
| Cube en panne (exécution) | script Lua qui appelle `error(...)` à chaque frame | erreur console `Script 'Cube en panne (erreur d'exécution)' : …` (avec la ligne), les autres objets continuent |
| Cube en panne (syntaxe) | script Lua qui ne compile pas | erreur console `Compilation du script 'Cube en panne (erreur de syntaxe)' : …`, les autres objets continuent |
| Fantôme | référence de mesh hors bornes (`Imported: 42`) | objet simplement non dessiné, aucune panique |
| Témoin sain | script de rotation valide | tourne — la preuve que les pannes n'arrêtent pas la simulation |

## Utilisation

1. Menu **📂 Ouvrir…** → `examples/broken_scene/scene.json`
2. Ouvrir la **Console** de l'éditeur (les erreurs y sont capturées)
3. Cliquer **Play** : le témoin sain tourne, les erreurs défilent, rien ne plante.

Preuves automatisées : `cargo test --test broken_scene_example`.
