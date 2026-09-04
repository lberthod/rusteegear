# Contrôles — la référence unique

Source de vérité : la table `SHORTCUTS` de [src/app/shortcuts.rs](../src/app/shortcuts.rs),
qui alimente aussi la fenêtre **Aide › ⌨ Raccourcis clavier** de l'éditeur.
Un test (`docs_controls_lists_every_shortcut`) vérifie que chaque raccourci
de la table apparaît dans cette page : ajouter un raccourci sans le
documenter ici fait échouer `cargo test`.

Mac : `Cmd` ; Linux/Windows : `Ctrl`. Les menus Fichier et Édition affichent
ces raccourcis à droite de chaque entrée, lus dans la même table.

`Cmd+S`, `Cmd+Maj+S`, `Cmd+O`, `Cmd+N`, `Cmd+P` et `Cmd+Q` répondent même
pendant la saisie dans un champ texte ; `Cmd+C` / `Cmd+V` / `Cmd+X` / `Cmd+A`
/ `Cmd+Z` y restent ceux du champ.

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
| `Cmd+P` | Play / Stop (comme les boutons ▶ / ⏹ de la barre d'outils) |
| `Cmd+Q` | Quitter — confirmation si modifications non enregistrées |
| `F1` | Fenêtre Raccourcis clavier (en Play : aide en jeu) |

## Souris (éditeur)

| Geste | Action |
| --- | --- |
| Clic gauche | Sélectionner (tap < 4 px) ; `Cmd`/`Maj` + clic : sélection additive |
| Clic gauche + glisser | Tourner la caméra (horizontal) ; sur une poignée : gizmo |
| Clic droit | Menu contextuel : cadrer, dupliquer, supprimer, ajouter |
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
| `Échap` | Pause (Reprendre, Recommencer la partie, Paramètres, Menu principal, Se déconnecter, Quitter) |
| `M` | Carte plein écran |
| `Tab` | Classement des joueurs, tant que la touche est maintenue (mode Player) ; les Paramètres s'ouvrent depuis le menu pause |
| `Espace` (vaincu) | Allié spectateur suivant |
| `F1` | Aide en jeu (contrôles et objectif) |
| `0` | Couper / remettre le son (mémorisé dans les Paramètres) |
| Clic gauche + glisser | Tourner la caméra (deux axes en Play, sensibilité dans Paramètres) |
| Souris (mode Player, desktop) | Le curseur est capturé pendant la partie : bouger la souris tourne la caméra sans cliquer ; `Échap`, la pause, l'accueil, les Paramètres, l'aide, la carte ou la défaite le rendent |

En mode Player (build joueur, web, mobile), les touches d'outils `Q T Y F G`
sont désactivées. Saut, attaque, tir, soin, pause et carte se remappent dans
**Paramètres › ⌨ Clavier** ; le déplacement reste WASD / flèches.

## Tactile (mobile, web sur écran tactile)

Le stick et les boutons n'apparaissent que sur un écran tactile (Android, iOS,
ou dès qu'un doigt a touché l'écran) ; les boutons ⏸ / Carte / ? restent
disponibles à la souris.

Chaque doigt est suivi séparément : stick + caméra, ou stick + Feu, en
même temps. Le stick est flottant — il apparaît là où le pouce se pose dans
la moitié gauche de l'écran (sous la barre de vie), avec une zone morte de
12 % et un rayon proportionnel à l'écran (44 à 90 pt) ; le doigt garde le
contrôle même en sortant du cercle.

| Geste | Action |
| --- | --- |
| Poser le pouce à gauche | Stick flottant : se déplacer (deux axes, relatif à la caméra) |
| Glisser sur la moitié droite de l'écran | Tourner la caméra (en même temps que le stick) |
| Boutons bas-droite | Saut / Feu / Arme / Soin (définis par la scène), tenables avec le stick |
| Pincer sur la carte | Zoomer la carte plein écran |
| Saut (vaincu) | Allié spectateur suivant |
| ⏸ (haut-droite) | Pause |
| 🔇 (haut-droite) | Couper / remettre le son |
| Carte (haut-droite) | Carte plein écran |
| ✖ (carte) | Fermer la carte |
| ? (haut-droite) | Aide en jeu |

## Manette

Remappable dans Paramètres › 🎮 Manette. Par défaut : stick gauche =
déplacement, stick droit = caméra, `Sud` saut, `Ouest` attaque, `Est` tir,
`Nord` soin, `Gâchette droite` changer d'arme, `Start` menu, `Select`
masquer le HUD.
