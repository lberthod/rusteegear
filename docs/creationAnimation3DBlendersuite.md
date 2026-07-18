# Création 3D — volet animé du pack « siège du hameau »

## Objectif

Ce document est un **addendum** à `creation3DBlendersuite.md` : il ne crée pas de nouveaux noms d'assets, il précise **lesquels des 40 assets `siege_*` déjà planifiés doivent être construits avec un squelette et un clip d'animation** plutôt qu'en géométrie statique pure, et comment.

Toute création reste sous la charte graphique : mémoire [charte-graphique-assets-maison](../../.claude/projects/-Users-berthod-Desktop-motor3derust/memory/charte-graphique-assets-maison.md).

## Contrainte technique confirmée (à respecter strictement)

Audit du pipeline d'import (`src/scene/import.rs`) : **le moteur ne lit que l'animation squelettique (armature + skin), aucun fallback keyframe d'objet.**

- `load_gltf_skeleton` (`import.rs:215-219`) renvoie `Ok(None)` dès qu'il n'y a pas de `skin` dans le fichier — un mesh statique n'a rien à squeletter.
- `load_gltf_clips` (`import.rs:472-488`) fait pareil : `let Some(skin) = doc.skins().next() else { return Ok(Vec::new()); }`. Un canal d'animation qui cible un nœud hors du skin est explicitement ignoré (`import.rs:499-502`, commentaire *« cible hors du skin (caméra, lumière…) : hors périmètre »*).

**Conséquence concrète** : impossible d'animer un objet en gardant juste des keyframes de transform sans squelette — même un simple battant de porte a besoin d'un mini-squelette (Root + 1 os par partie mobile), exactement comme les créatures. C'est déjà la recette utilisée par tout ce qui bouge dans le jeu (créatures, faune décorative, mécanismes `nature_*`) — on ne réinvente rien, on l'applique aux pièces du pack siège qui le méritent.

## Ce qui est déjà couvert (ne pas dupliquer)

Correction utile issue de l'audit : la plupart des mécanismes « animables évidents » ont déjà une version animée dans `nature_*` (technique squelette + clip « Idle », même recette) :
- `nature_watermill`/`nature_windmill` (roue/ailes qui tournent), `nature_drawbridge` (tablier qui s'ouvre), `nature_pendulum_clock` (balancier), `nature_catapult` (bras qui bascule), `nature_merry_go_round` (plateforme qui tourne), `nature_weaving_loom` (battant du métier à tisser), `nature_toll_gate` (barrière qui se lève), `nature_potters_wheel`/`nature_grindstone` (roue qui tourne), `nature_prayer_wheel`, `nature_mast_flag`, `nature_kite`, `nature_dock_crane`, `nature_rope_swing`.

Le pack siège **ne refait aucun de ces mécanismes** — il anime uniquement ses propres pièces (portes de rempart, chariot, bannières, brasero, etc.), qui n'ont pas d'équivalent existant.

## Liste des assets `siege_*` à construire animés (12, sur les 40 déjà planifiés)

| Asset (déjà dans creation3DBlendersuite.md) | Mouvement | Squelette | Clip |
|---|---|---|---|
| Porte de rempart (fermée) | Les deux vantaux pivotent (ouverture) | Root + `VantailGauche` + `VantailDroit` | Idle (boucle ouverte/fermée, 40f) |
| Porte de rempart (embrasée) | Même rig que ci-dessus + tremblement des flammes | Root + 2 vantaux + `Flamme1..2` | Idle (40f) |
| Poterne | Un seul vantail pivote | Root + `Vantail` | Idle (30f, rig simplifié de la porte) |
| Herse | Grille qui monte/descend (glissière verticale) | Root + `Grille` | Idle (40f) |
| Chariot de braises | Les 2 roues tournent | Root + `RoueGauche` + `RoueDroite` | Walk (24f, même convention que les créatures — le chariot avance dans le mode Escorte) |
| Brasero communal | Vacillement des flammes | Root + `Flamme1..3` | Idle (40f) |
| Bannière de vague | Le tissu ondule (reprise de la technique `nature_banner`) | Root + `Tissu1..2` | Idle (40f) |
| Bannière de mode (Escorte/Boss/Survie/Vagues, ×4 teintes) | Même rig que la bannière de vague, un seul script/squelette réutilisé pour les 4 variantes de couleur | Root + `Tissu1..2` | Idle (40f) |
| Fanion de couleur d'équipe | Même rig bannière, plus petit | Root + `Tissu` | Idle (30f) |
| Torche de rempart | Flamme qui vacille (version réduite du brasero) | Root + `Flamme` | Idle (30f) |
| Balise de spawn | Rune qui tourne + pulse d'échelle | Root + `Rune` | Idle (48f, boucle plus longue pour un effet moins répétitif) |
| Cage du chef | Barreaux/porte qui s'ouvre (libération du chef) | Root + `PorteCage` | Idle (40f, position "fermée" par défaut ; l'ouverture est déclenchée côté scène, pas par un clip "Walk" séparé) |

**Non retenus pour l'animation** (restent des assets statiques du pack siège) : segment de mur, segment d'angle, tour d'angle, module de créneau, chemin de ronde, escalier de rempart, bastion, boulet (sa trajectoire est gérée par le code de jeu, pas par un clip d'asset), autel de l'Aînée, tas de trophées, caisse de réserve, rangée de pieux, tous les assets de la catégorie Lande, corne d'alerte, marqueur de zone au sol (un cercle qui tourne ajoute peu et complique l'alignement au sol), statue commémorative, trophée de fin de manche, portail de fin, panneau directionnel — pas de besoin fonctionnel identifié dans le GDD pour justifier le coût d'un squelette sur ces pièces.

## Méthode

Reprendre exactement la recette de `creature_kit.py`/`gen_nature_animated.py`/`gen_fauna_decor_pack.py`, sans variante :
1. Squelette minimal : un os `Root` + un os par partie mobile (jamais plus que nécessaire — une porte à 2 vantaux a 2 os, pas plus).
2. Groupes de vertex à poids plein (100 %) par os, pas de skinning mélangé.
3. Keyframes sur les pose-bones (`rotation_euler`/`location`/`scale` selon le mouvement), baking dans une action poussée sur une piste NLA nommée selon la table ci-dessus (« Idle » ou « Walk »).
4. `ad.action = None` avant export (purge de la pose résiduelle qui écraserait sinon la piste NLA à l'évaluation — piège déjà documenté dans la charte).
5. Export glTF avec skin + animation (contrairement aux assets statiques du hameau/siège, ici `export_animations=True`/`export_skins=True`).
6. Un nouveau script partagé `scripts/blender/siege_common.py` (ou une extension de `hamlet_common.py`) porte les helpers squelette/bake, calqués sur `creature_kit.bake_clip`/`build_creature` mais adaptés à des props (pas de quadrupède standard) — un squelette sur-mesure par pièce, pas un squelette générique réutilisé partout.

## Plan d'exécution

Ce volet animé se glisse dans les sprints déjà prévus par `creation3DBlendersuite.md` plutôt que d'ouvrir une numérotation parallèle :
- Au **Sprint 1** (fortifications lot 1) : porte fermée, porte embrasée et poterne sont construites directement avec leur squelette dès la première génération (pas de passe statique puis re-fait).
- Au **Sprint 2** (fortifications lot 2) : la herse est construite animée.
- Au **Sprint 3** (props des modes lot 1) : chariot de braises, brasero, bannière de vague.
- Au **Sprint 4** (props des modes lot 2) : balise de spawn.
- Au **Sprint 5/6** (signalétique) : bannières de mode, fanion, torche, cage du chef.
- **Sprint 7 (QA finale)** : le script `check_siege_pack.py` doit être étendu (calqué sur `check_creatures.py`) pour vérifier, sur les 12 assets animés uniquement : présence du clip attendu, boucle parfaite (comparaison de la pose au premier et dernier frame, tolérance 1e-3), et budget d'os (`JOINT_CAPACITY` = 128 côté moteur — largement suffisant ici, chaque pièce a 1 à 4 os).

## Critères de validation

- Les 12 assets ci-dessus exportent avec skin + clip nommé correctement, les 28 autres restent des meshes statiques (pas de squelette inutile).
- QA étendue sans échec : clip présent, boucle fermée, budget d'os respecté, aucun vertex sous le sol (même règle que le reste du pack).
- Vignette de contrôle : EEVEE peut rendre une pose statique (frame 0) pour la vignette — pas besoin de prévisualiser l'animation elle-même dans l'image de contrôle, cohérent avec la convention déjà utilisée pour les créatures.
- Aucun mécanisme déjà couvert par `nature_*` n'est reconstruit.

## Hors scope explicite

- Le déclenchement réel des animations en jeu (ex. : ouverture de la porte sur événement de vague, libération du chef) est un chantier de gameplay séparé (code Rust, `AnimationState::set_clip`), pas un travail d'asset.
- La génération effective des scripts (`gen_siege_walls.py` etc.) et du squelette partagé (`siege_common.py`) n'est pas commencée par ce document — c'est un travail de sprint, comme convenu pour les deux packs précédents.
