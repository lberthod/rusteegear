# API Lua — référence de tout ce qu'un script peut lire et écrire

Un script vit **dans** l'objet (Inspecteur ▸ Script, ou la fenêtre 📝 Script).
Son corps entier est exécuté **du début à la fin, une fois par objet et par
pas de simulation** (dt fixe) tant que Play tourne : il n'y a **aucun callback
à définir** — pas de `update(dt)`, pas de `on_trigger()`. On lit des
drapeaux, on écrit des champs :

```lua
obj.ry = obj.ry + 45 * dt          -- tourne de 45°/s
if obj.triggered then emit("porte:ouvrir") end
if on_event("score:3") then obj:destroy() end
```

Les globales ci-dessous sont posées par `run_script` dans
[src/app/scripting.rs](../src/app/scripting.rs) (natif, Lua 5.4 via `mlua`)
et, à l'identique, par `run_script_web` dans
[src/app/scripting_web.rs](../src/app/scripting_web.rs) (web, Lua 5.1 via
`rilua`). Ce qui tient dans cette page tourne sur les cinq cibles ; les écarts
sont listés en fin de page. Pour ce qui relève du langage Lua lui-même (et non
du moteur) : [LUA_PORTABLE.md](LUA_PORTABLE.md).

## `obj` — l'objet qui porte le script

Champs **lus et réécrits** après chaque exécution : écrire dedans déplace,
tourne ou colore réellement l'objet.

| Champ | Type | Rôle |
| --- | --- | --- |
| `obj.x`, `obj.y`, `obj.z` | nombre (lecture/écriture) | Position dans le monde. |
| `obj.rx`, `obj.ry`, `obj.rz` | nombre (lecture/écriture) | Rotation en **degrés** (`ry` = autour de l'axe vertical). Le round-trip lecture → écriture est stable : un yaw pur reste dans `ry`. |
| `obj.sx`, `obj.sy`, `obj.sz` | nombre (lecture/écriture) | Échelle. |
| `obj.r`, `obj.g`, `obj.b` | nombre 0..1 (lecture/écriture) | Couleur de l'objet. |
| `obj.anim` | chaîne (lecture/écriture) | Clip d'animation en cours ; `obj.anim = "run"` démarre un fondu vers ce clip. Sans effet sur un objet non animé. |

Drapeaux **lecture seule**, posés par le moteur et remis à zéro à chaque pas :

| Champ | Vrai quand… |
| --- | --- |
| `obj.tapped` | l'objet vient d'être tapé/cliqué (une seule frame). |
| `obj.touch_started` | l'appui sur l'objet commence (une seule frame). |
| `obj.touching` | l'appui sur l'objet est maintenu (chaque frame). |
| `obj.touch_ended` | l'appui vient d'être relâché (une seule frame). |
| `obj.triggered` | le joueur est dans cette **🎯 Zone de déclenchement** (intersection des volumes, tant que ça dure). |
| `obj.exited` | le joueur **vient de sortir** de la zone (le pas où le contact cesse — pas « n'est pas dedans »). |

Méthode :

| Appel | Effet |
| --- | --- |
| `obj:destroy()` | Retire l'objet du jeu (il devient invisible et inactif, comme un monstre vaincu). Toujours avec `:` — c'est la forme qui marche partout. |

## Valeurs et tables globales

| Global | Type | Rôle |
| --- | --- | --- |
| `dt` | nombre | Durée du pas de simulation, en secondes (fixe). |
| `time` | nombre | Temps de jeu écoulé depuis le début du Play, en secondes. |
| `input.jx`, `input.jy` | nombre -1..1 | Axes du joystick virtuel (tactile) ou de son équivalent clavier. |
| `input.btn.<nom>` | `true` ou absent | Bouton tactile pressé (`if input.btn.B1 then … end`) ; un bouton non pressé est `nil`. |
| `tilt.x`, `tilt.y` | nombre -1..1 | Inclinaison gyroscope/accéléromètre (desktop : simulée aux flèches). |
| `save.get(clé)` | fonction → nombre ou `nil` | Lit une variable de sauvegarde. |
| `save.set(clé, valeur)` | fonction | Écrit une variable de sauvegarde (**nombres seulement**). Partagée entre tous les objets, conservée avant erreur, embarquée dans la sauvegarde de partie. |
| `debug.line(x1,y1,z1, x2,y2,z2, r,g,b)` | fonction | Trace un segment de débogage (couleur 0..1) visible une frame ; les appels s'accumulent. Remplace la bibliothèque `debug` standard de Lua. |

## Fonctions globales

| Fonction | Retour | Rôle |
| --- | --- | --- |
| `emit(nom)` | — | Émet un événement reçu par **tous** les scripts au pas **suivant** (l'ordre des objets ne compte donc jamais). |
| `on_event(nom)` | booléen | Vrai si l'événement `nom` a été reçu **ce pas**. |
| `spawn(prefab, x, y, z)` | — | Instancie un prefab (référence `asset-id://…`) à cette position, après la boucle des scripts. |
| `add_item(genre, n)` | — | Ajoute `n` objets au sac du joueur : `"potion"`, `"baie"`, `"cle"`, `"gemme"` (genre inconnu ignoré). |
| `find_tag(tag)` | table de `{x, y, z}` | Positions des objets visibles portant ce tag (instantané pris avant la boucle ; jamais de référence vivante). |
| `raycast(ox,oy,oz, dx,dy,dz, max [, masque])` | `{x, y, z, dist}` ou `nil` | Lance un rayon dans le monde physique ; `nil` si rien n'est touché ou hors Play. |
| `overlap_sphere(x,y,z, rayon [, masque])` | nombre | Compte les volumes de collision dans la sphère ; `0` hors Play. |
| `set_health(v)` | — | Fixe la barre de vie du HUD à `v` (0..1). |
| `damage(v)` | — | Retire `v` à la vie (cumulatif dans la frame, borné 0..1) — `damage(999)` pour une zone mortelle. |
| `reverb(mix)` | — | Réverbération du bus SFX (0..1, transition 0,5 s) ; le dernier appel du pas l'emporte. |
| `vibrate(ms)` | — | Demande un retour haptique — **aujourd'hui seulement journalisé**, sur toutes les cibles. |

## Événements émis par le moteur

Reçus par `on_event(...)` comme ceux d'`emit` :

| Nom | Quand |
| --- | --- |
| `score:<n>` | le score atteint `n` — un événement par valeur traversée (`score:1` puis `score:2` si deux pièces le même pas). |
| `hud:<action>` | un bouton du HUD (fenêtre 🧩 Widgets HUD) est cliqué. |
| `anim:<marqueur>` | un marqueur d'animation est franchi (ex. `anim:hit_open`) — reçu le pas même. |

## Ordre d'un pas de simulation

`time += dt` → animations (marqueurs `anim:*`) → calcul des zones touchées
(`triggered` / `exited`) → **scripts, objet après objet** → application de
`spawn`, `add_item`, `emit`, `vibrate`, `reverb`, vie du HUD → attaques →
physique. `obj:destroy()` s'applique dès le retour du script.

Bon à savoir :

- Une erreur Lua est journalisée (toast ⛔ et badge sur l'objet) et n'arrête
  pas les autres scripts ; les champs `obj.*` de l'objet fautif ne sont pas
  réécrits ce pas-là.
- Tous les scripts partagent la même instance Lua : une variable globale est
  visible par les autres objets et d'un pas à l'autre — préfère `local`, et
  `save.*` pour l'état voulu partagé.
- Le chunk est compilé une fois et mis en cache : modifier le script le
  recompile.

## Natif ↔ web : ce qui diffère

Les 18 globales ci-dessus existent avec les mêmes noms et arités sur les deux
interpréteurs, `raycast` et `overlap_sphere` compris. Les écarts réels :

| Sujet | Natif (`mlua`, Lua 5.4) | Web (`rilua`, Lua 5.1) |
| --- | --- | --- |
| Langage | Lua 5.4 complet (coroutines testées) | Lua 5.1 : pas de `goto`, `//`, opérateurs bit à bit, entiers — cf. [LUA_PORTABLE.md](LUA_PORTABLE.md) |
| Points d'arrêt de l'éditeur | Oui (arrête le script avant la ligne, pour ce pas) | Non (pas d'éditeur sur le web) |
| Messages d'erreur | Préfixés du nom de l'objet (`script de « Nom »:ligne`) | Message brut de l'interpréteur |
| `obj.destroy()` sans `:` | Erreur de type | Toléré — écris quand même `obj:destroy()` |
| Barre de vie | Affichée seulement si un script appelle `set_health`/`damage` | Toujours renseignée (1.0 par défaut) dès qu'un script tourne |
| `overlap_sphere` | Entier | Flottant (`tostring` peut différer) |
| Persistance de `save.*` | Fichier `user://save_<slot>.json` (`~/.motor3derust/save`) | Le temps de la session seulement (pas de dossier utilisateur dans le navigateur) |
| `spawn()` | Oui | Le dossier d'assets utilisateur n'existe pas : le prefab est introuvable et l'appel journalise une erreur |

Les quatre scripts d'[`examples/scripts/`](../examples/scripts/) sont exécutés
sur les deux interpréteurs à chaque `cargo test`
(`official_scripts_match_between_backends`) et leurs résultats comparés :
partir de l'un d'eux garantit la portabilité.
