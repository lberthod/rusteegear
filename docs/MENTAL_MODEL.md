# Comment RusteeGear fonctionne

Une page, à lire avant tout le reste. Le détail technique vit dans
[architecture.md](architecture.md).

## Les données

```text
Scene                       ← un niveau ; un fichier .json (Ouvrir/Enregistrer)
 └── Scene Objects          ← tout ce qui existe dans la scène
      ├── Transform         ← position, rotation, échelle
      ├── Mesh              ← primitive (Cube, Sphere…) ou GLB importé
      ├── Collider          ← physique : None / Static / Dynamic / Kinematic
      ├── Controller        ← si « input » : l'objet devient le joueur
      ├── Script            ← code Lua inline, exécuté chaque frame en Play
      └── (Combat, Audio, Animation… — composants optionnels)
```

- La **lumière** (directionnelle + ponctuelles) est un réglage de la scène,
  pas un Scene Object.
- Les **assets** (GLB, sons, textures) importés sont copiés dans le dossier
  utilisateur `~/.motor3derust/assets/` et référencés par la scène
  (`asset-id://…`). Les scènes de démonstration n'en ont pas besoin : les
  primitives suffisent.
- Il n'y a pas encore de notion de « projet » au-dessus de la scène : une
  scène JSON **est** l'unité de travail (cf. limitations connues).

## Les quatre rôles

```text
Editor    → modifie les données        (l'application que tu lances)
Runtime   → simule les données         (physique, scripts, manches)
Renderer  → affiche les données        (wgpu : Metal / Vulkan / WebGPU)
Server    → simule sans afficher       (multijoueur autoritaire, optionnel)
```

Le **Player** (build exporté : web, .app, APK) = Runtime + Renderer sans
l'Editor, sur la scène embarquée à l'export.

## Play / Stop

```text
Play  → clone la scène éditée et lance la simulation dessus
Pause → fige la simulation (l'édition/sélection redevient possible)
Stop  → jette le clone : la scène revient à l'état d'avant Play
```

Conséquence : ce que tu modifies **pendant** Play est perdu au Stop — c'est
voulu (on expérimente en Play, on édite hors Play).

## Glossaire officiel

| Concept | Nom | À ne pas confondre avec |
| --- | --- | --- |
| un niveau / environnement | **Scene** | « projet », « monde » |
| un élément de scène | **Scene Object** | « entité », « acteur » |
| un composant optionnel d'objet | **Component** | — |
| le code utilisateur | **Script** (Lua) | « comportement » |
| l'application de création | **Editor** | — |
| le jeu exécuté / exporté | **Player** | « runtime », « build » |
| le serveur autoritaire | **Game Server** | le salon de chat Firebase |
| le moteur lui-même | **RusteeGear** | `motor3derust` (nom interne du crate) |
