# Création 3D — pack « grottes & rives » en style organique (metaball)

## Objectif

Troisième salve d'assets après « hameau maison » (`hamlet_*.glb`, 40) et « siège du hameau » (`siege_*.glb`, 40), toutes deux en style primitives dures (cube/cylindre/cône). Celle-ci change délibérément de technique de modélisation : **style organique par métaballes fusionnées**, comme le prototype `scripts/blender/proto_creature62_fox_organic.py` (Option B du rapport qualité créatures) plutôt que l'assemblage de primitives à facettes. Les silhouettes rocheuses/naturelles y gagnent en rondeur crédible — une roche de rivière lissée par l'eau ou une stalactite n'ont pas la même lecture en cubes qu'en volumes fusionnés.

Grounding : le travail de terrain en cours (`sprintreflecion.md`, Phase K, Sprint 26 — *« Creux/fosses et tunnels : géométrie non-heightmap insérée aux endroits voulus »* et *« Lacs intégrés au relief : le niveau d'eau correspond à un creux réel du terrain »*) crée deux besoins concrets et encore non couverts par `nature_*` : du décor de grotte/tunnel et du décor de rive de lac au relief naturel. Aucun des deux prefixes actuels ne les couvre.

Charte graphique inchangée par ailleurs : mémoire [charte-graphique-assets-maison](../../.claude/projects/-Users-berthod-Desktop-motor3derust/memory/charte-graphique-assets-maison.md) — ≤3 teintes par objet, aucune texture, sol z=0/Y-up, pas de transparence (le moteur ignore l'alpha), vignette EEVEE 640×480.

## Technique organique : ce qui change par rapport au hameau/siège

Reprise directe de `proto_creature62_fox_organic.py`, adaptée au décor statique (pas de squelette — ces assets ne bougent pas) :

1. **Cœur organique par métaballes** : un objet `Metaball` (`bpy.data.metaballs.new`), des éléments `ELLIPSOID` rapprochés/surdimensionnés pour bien fusionner (`meta_elem()` du prototype), `resolution`/`render_resolution` ~0.04, `threshold` ~0.2-0.25 selon la taille. Converti en mesh (`bpy.ops.object.convert(target="MESH")`) puis `bpy.ops.object.shade_smooth()`.
2. **Accessoires nets** joints après coup si besoin (cristal facetté planté dans une paroi, poutre de bois d'un tunnel) — primitives classiques de `hamlet_common.py`, non lissées, même logique que « corps lisse + accessoires durs » du prototype.
3. **Garde-sol identique** au reste de la charte : aucun vertex sous z=0 (même vérification que `core.data.update(); min_z = min(...)`, translation si besoin) — le piège est le même qu'avec les blobs de `hamlet_common.blob()`, juste avec des métaballes au lieu d'icosphères.
4. **Export statique** : contrairement au prototype (qui exporte skin+clips pour une créature), ces assets suivent la convention `hamlet_common.export()` — un seul mesh joint, `transform_apply`, pas de skin/animation, `export_yup=True`.
5. **Nouveau module partagé** : `scripts/blender/organic_common.py`, qui expose `meta_elem()`/`build_metaball_core()` (généralisés depuis le prototype, sans le squelette) et importe la palette/les helpers de primitives de `hamlet_common.py` par réutilisation directe (pas de nouvelle palette — pierre/bois restent les mêmes teintes que le hameau et le siège).
6. **Coût à surveiller** : la résolution des métaballes n'est pas linéaire avec le nombre d'éléments — tester chaque asset individuellement (comme le prototype l'a fait pour le renard) avant de lancer un lot complet, et garder un budget vertices comparable aux props du hameau (~100-500) sauf pièces "hero" (arche d'entrée, paroi de fond) qui peuvent monter vers 800-1200.

## Nouveau préfixe

`grotto_*.glb` pour le lot souterrain, `shore_*.glb` pour le lot de rive — deux préfixes distincts plutôt qu'un seul générique, parce que ce sont deux biomes différents qui ne seront jamais posés ensemble dans la même zone. Vérifié sans collision (`ls assets/models/ | grep -E "grotto|shore"` vide).

## Liste complète des ~40 assets

### Grottes / souterrain (20) — `grotto_*.glb`

| Nom cible | Rôle | Technique |
|---|---|---|
| Arche d'entrée de grotte | Ouverture praticable, pièce hero du lot | métaball (masse rocheuse) + accessoires durs (gravats au pied) |
| Paroi de fond | Grand mur rocheux organique pour fermer un tunnel | métaball, silhouette large et basse |
| Stalactite (petite) | Suspendue au plafond | métaball fine et effilée |
| Stalactite (grande) | Idem, landmark vertical | métaball |
| Stalagmite (petite) | Au sol | métaball |
| Stalagmite (grande) | Au sol, landmark | métaball |
| Colonne (stalactite+stalagmite jointes) | Repère de couloir étroit | métaball, deux cônes fusionnés en un seul volume |
| Sol rocheux bosselé | Dalle de grotte irrégulière (pas plate) | métaball très aplatie |
| Éboulis | Tas de gravats organiques | métaball, plusieurs petits éléments groupés |
| Bloc effondré | Plafond écroulé, obstacle | métaball massif + arêtes dures cassées (accessoires) |
| Champignon lumineux (petit) | Flore souterraine bioluminescente | métaball + matériau émissif (vignette uniquement, cf. charte) |
| Champignon lumineux (grappe) | Variante groupée | métaball ×3-4 |
| Racine pendante | Racine de surface qui perce le plafond | métaball tordue, fine |
| Cristal de grotte | Amas cristallin | cœur métaball + facettes dures plantées (accessoires) |
| Flaque souterraine | Eau stagnante, sol | métaball très plate, matériau sombre |
| Poutre de soutènement | Structure humaine dans le tunnel | primitive dure classique (bois), pas organique — contraste voulu |
| Passage bas | Arche étroite à franchir courbé | métaball |
| Ossements de créature | Restes fossilisés au sol | métaball + accessoires durs (côtes, crâne stylisé) |
| Voile de moisissure | Surplomb texturé en surface | métaball aplatie, teinte verdâtre sombre |
| Goutte suspendue | Détail miniature, stalactite + reflet | métaball minuscule |

### Rives de lac (20) — `shore_*.glb`

| Nom cible | Rôle | Technique |
|---|---|---|
| Rocher de rive lissé | Galet géant arrondi par l'eau, pièce hero | métaball, silhouette très arrondie |
| Groupe de galets | 3-4 galets de tailles variées | métaball ×3-4 |
| Bois flotté | Tronc échoué, tordu | métaball allongée et sinueuse |
| Racine immergée | Racine de rive tordue, mi-émergée | métaball |
| Berge en pente douce | Talus végétalisé, suit le relief (Sprint 26) | métaball très aplatie et large |
| Berge abrupte rocheuse | Variante escarpée | métaball |
| Mousse de berge | Touffe humide, distincte de `nature_moss_boulder` | métaball petite, teinte verte sombre |
| Bulle/ondulation d'eau figée | Détail de surface stylisé | métaball très plate, quasi transparente visuellement (mais opaque, cf. charte) |
| Ponton rustique court | Jetée simple en bois | primitive dure classique, pas organique — contraste avec la roche |
| Amas de coquillages | Détail de rive | métaball, petits éléments groupés |
| Laisse de rive | Ligne de débris/algues déposée par l'eau | métaball aplatie et allongée |
| Poisson échoué | Petit détail narratif | métaball, silhouette fusiforme |
| Nid de rive | Détail de faune, vide (pas d'oiseau — `fauna_*` s'en charge) | métaball + brindilles dures |
| Souche à moitié immergée | Distincte de `nature_stump` (érosion, pas coupe nette) | métaball + accessoires durs (racines) |
| Vasque naturelle | Bassin creusé par l'érosion | métaball creusée (seuil de fusion réglé pour une dépression) |
| Cascade figée stylisée | Roche + filet d'eau, silhouette verticale | métaball (roche) + filet plat (eau, opaque) |
| Îlot rocheux | Petit rocher émergent, posable au milieu de l'eau | métaball |
| Berge à racines apparentes | Érosion qui expose des racines | métaball + racines dures entremêlées |
| Amas d'algues échouées | Tas organique sombre | métaball, teinte vert-brun |
| Brume basse de rive | Nappe stylisée au ras de l'eau (opaque, cf. charte fumée) | métaball très aplatie et large, teinte claire translucide-mais-opaque |

**Total : 20 + 20 = 40 assets.**

## Plan d'exécution par sprints

- **Sprint 0 — prototype et validation de la technique** : généraliser `proto_creature62_fox_organic.py` en `scripts/blender/organic_common.py` (fonctions réutilisables, sans le squelette). Tester sur 1 rocher de grotte ET 1 rocher de rive isolés, vérifier le rendu vignette et le budget vertices avant de lancer un lot complet — même prudence que pour `plank_wall`/`stone_coursing` au pack hameau. Écrire `check_organic_pack.py` (calqué sur `check_hamlet_pack.py`, scan `grotto_*`/`shore_*`).
- **Sprint 1 — grottes, pièces hero** (4 : arche d'entrée, paroi de fond, bloc effondré, colonne) : `gen_grotto_hero.py`.
- **Sprint 2 — grottes, formations rocheuses** (8 : stalactites ×2, stalagmites ×2, sol bosselé, éboulis, passage bas, cristal) : `gen_grotto_rocks.py`.
- **Sprint 3 — grottes, flore/détails** (8 : champignons ×2, racine pendante, flaque, poutre, ossements, moisissure, goutte) : `gen_grotto_decor.py`.
- **Sprint 4 — rives, pièces hero et rochers** (6 : rocher lissé, groupe de galets, îlot, berge douce, berge abrupte, vasque) : `gen_shore_rocks.py`.
- **Sprint 5 — rives, bois et détails organiques** (7 : bois flotté, racine immergée, souche immergée, berge à racines, cascade figée, laisse de rive, poisson échoué) : `gen_shore_decor.py`.
- **Sprint 6 — rives, petits props et ambiance** (7 : mousse, bulle d'eau, ponton, coquillages, nid, algues échouées, brume de rive) : `gen_shore_props.py`.
- **Sprint 7 — QA finale & revue** : `check_organic_pack.py` sur les 40 fichiers, revue visuelle groupée.

## Critères de validation

- Chaque asset organique passe par métaball → mesh → shade smooth, pas de facettage dur sauf accessoires volontairement nets.
- `check_organic_pack.py` : 0 échec (mesh joint, pas de vertex sous z=0, pas de texture).
- Palette ≤3 teintes/objet, réutilisant les constantes déjà définies dans `hamlet_common.py` (pierre, bois, eau sombre) — pas de nouvelle teinte sans besoin réel (ex. bioluminescence des champignons, à documenter si ajoutée).
- Vignette de contrôle par asset, inspectée visuellement.
- Nommage `grotto_*`/`shore_*`, sans collision avec les préfixes existants.
- Vérifier au moins un rocher de chaque lot côte à côte avec un rocher `nature_*` existant (ex. `nature_rock`) pour confirmer que le style organique est visuellement distinct et non redondant.

## Hors scope explicite

- L'intégration réelle dans le terrain (poser ces assets dans les tunnels/creux/lacs du Sprint 26 de `sprintreflecion.md`) est un chantier séparé, dépendant de l'avancement du système de relief lui-même (pas encore livré au moment de ce document).
- Aucun script n'est écrit ni exécuté par ce document — c'est un tour de planification, la génération réelle commence au Sprint 0 listé ci-dessus.
