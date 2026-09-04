# Contrôles — la référence unique

Source de vérité : la table `SHORTCUTS` de [src/app/shortcuts.rs](../src/app/shortcuts.rs),
qui alimente aussi la fenêtre **Aide › ⌨ Raccourcis clavier** de l'éditeur.
Un test (`docs_controls_lists_every_shortcut`) vérifie que chaque raccourci
de la table apparaît dans cette page : ajouter un raccourci sans le
documenter ici fait échouer `cargo test`.

Mac : `Cmd` ; Linux/Windows : `Ctrl`.

## Éditeur (hors Play)

| Touche | Action |
| --- | --- |
| `Q` | Outil Main (déplacer la vue) |
| `W` | Outil Déplacer (gizmo) |
| `E` | Outil Tourner (gizmo) |
| `R` | Outil Échelle (gizmo) |
| `T` | Outil Orbite (caméra) |
| `Y` | Outil Loupe (zoom) |
| `F` | Cadrer la sélection |
| `G` | Caméra libre (vol) — flèches + Espace/C pour monter/descendre |
| `Cmd+Z` / `Cmd+Maj+Z` | Annuler / Rétablir |
| `Cmd+D` | Dupliquer la sélection |
| `Cmd+C` / `Cmd+X` / `Cmd+V` | Copier / Couper / Coller |
| `Cmd+A` | Tout sélectionner |
| `Suppr` / `Retour arrière` | Supprimer la sélection |
| `Cmd+S` | Enregistrer (scène du projet ouvert, sinon `~/motor3derust_scene.json`) |
| `Cmd+Maj+S` | Enregistrer sous… |
| `Cmd+O` | Ouvrir une scène ou un projet… |
| `Cmd+N` | Nouveau projet… |

## Souris (éditeur)

| Geste | Action |
| --- | --- |
| Clic gauche | Sélectionner (tap < 4 px) ; `Cmd`/`Maj` + clic : sélection additive |
| Clic gauche + glisser | Tourner la caméra (horizontal) ; sur une poignée : gizmo |
| Clic milieu + glisser, ou `Maj` + glisser | Déplacer la vue (pan), quel que soit l'outil |
| Molette | Zoom |
| `Ctrl` pendant un glissé de gizmo | Inverser l'aimantation (snap) |
| Double-clic dans la hiérarchie | Renommer |

## Jeu (Play dans l'éditeur, mode Player, web)

| Touche | Action |
| --- | --- |
| `W A S D` ou flèches | Se déplacer (relatif à la caméra) |
| `Espace` | Sauter |
| `J` | Attaque de mêlée |
| `K` | Tirer (arme à distance) |
| `H` | Soigner l'allié blessé le plus proche |
| `1` `2` `3` | Choisir l'arme (Boule de feu / Éclair / Boulet) |
| `Échap` | Pause (Reprendre, Rejouer, Paramètres, Se déconnecter, Quitter) |
| `M` | Carte plein écran |
| `Tab` | Paramètres (mode Player uniquement) |
| Clic gauche + glisser | Tourner la caméra (deux axes en Play, sensibilité dans Paramètres) |

En mode Player (build joueur, web, mobile), les touches d'outils `Q T Y F G`
sont désactivées. Saut, attaque, tir, soin, pause et carte se remappent dans
**Paramètres › ⌨ Clavier** ; le déplacement reste WASD / flèches.

## Tactile (mobile, web sur écran tactile)

| Geste | Action |
| --- | --- |
| Stick gauche | Se déplacer (deux axes, relatif à la caméra) |
| Glisser sur la moitié droite de l'écran | Tourner la caméra |
| Boutons bas-droite | Saut / Feu / Arme / Soin (définis par la scène) |
| ⏸ (haut-droite) | Pause |
| Carte (haut-droite) | Carte plein écran |

## Manette

Remappable dans Paramètres › 🎮 Manette. Par défaut : stick gauche =
déplacement, stick droit = caméra, `Sud` saut, `Ouest` attaque, `Est` tir,
`Nord` soin, `Gâchette droite` changer d'arme, `Start` menu, `Select`
masquer le HUD.
