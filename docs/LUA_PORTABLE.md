# Lua portable — ce qui marche partout, ce qui n'est pas garanti

Deux interpréteurs exécutent les scripts selon la plateforme : **mlua**
(Lua 5.4, natif : éditeur macOS, player desktop/Android/iOS, serveur) et
**rilua** (Lua 5.1 pur Rust, player **web**). L'API du moteur est portée à
l'identique sur les deux (`src/app/scripting.rs` / `scripting_web.rs`), et des
**tests différentiels** exécutent le même script sur les deux backends et
comparent les résultats (`cargo test official_scripts_match`).

## Supporté partout (couvert par les tests différentiels)

L'API du moteur :

```lua
obj.x, obj.y, obj.z          -- position
obj.rx, obj.ry, obj.rz       -- rotation (degrés)
obj.sx, obj.sy, obj.sz       -- échelle
obj.r, obj.g, obj.b          -- couleur
obj.tapped, obj.triggered, obj.exited
obj.anim                     -- clip d'animation
obj:destroy()

dt, time                     -- durée de frame, temps de jeu (secondes)
input.jx, input.jy, input.btn[...]

emit(nom), on_event(nom)     -- événements entre objets
spawn(prefab, x, y, z)
find_tag(tag)                -- → liste de positions {x, y, z}
save.get(clé), save.set(clé, valeur)
raycast(...), debug.line(...)
vibrate(ms), reverb(mix), set_health(v), damage(v)
```

Et le Lua « commun » aux deux versions : `math.*`, `string.*`, tables,
boucles, fonctions locales.

## Non garanti sur le web

- `io.*`, `os.*`, `package.*`, la bibliothèque `debug.*` de Lua — absents ou
  non portés (et sans objet dans un jeu : pas de système de fichiers côté
  navigateur) ;
- les nouveautés **Lua 5.3/5.4** : `goto`, division entière `//`, opérateurs
  bit à bit natifs, le type entier — le web est en Lua 5.1 ;
- les métatables avancées et `coroutine` : non couverts par les tests, à
  éviter dans un script destiné aux deux plateformes ;
- les **breakpoints** de l'éditeur : fonctionnalité éditeur, natif seulement.

En restant dans la section « supporté partout », un script tourne à
l'identique sur les cinq cibles.

## Les 4 scripts officiels

Publiés dans [`examples/scripts/`](../examples/scripts/), testés sur **les
deux backends** à chaque `cargo test` (résultats comparés à 1e-5 près) :

| Script | Montre | API clé |
| --- | --- | --- |
| `rotate.lua` | animation continue | `obj.ry`, `dt` |
| `move_between_points.lua` | va-et-vient | `time`, `math.sin` |
| `trigger_door.lua` | réaction à une zone | `obj.triggered` |
| `simple_enemy.lua` | poursuite du joueur | `find_tag`, `dt` |

Partez de l'un d'eux plutôt que d'une page blanche : ils sont garantis
portables.
