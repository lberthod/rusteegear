# Ton premier objet animé — tutoriel de 10 minutes

Prérequis : avoir suivi [QUICKSTART.md](../QUICKSTART.md) (éditeur lancé,
scène `examples/first_game/scene.json` ouverte). Chaque étape nomme le
bouton/menu exact tel qu'il apparaît dans l'éditeur.

## Étape 1 — Ouvrir le projet exemple

Menu **📂 Ouvrir…** → `examples/first_game/scene.json`. Tu vois : un sol vert,
une capsule orange (le joueur), trois caisses, un cube bleuté qui a un script,
une zone jaune, trois pièces dorées.

## Étape 2 — Ajouter un cube

Menu **Ajouter** → **🧊 Cube**. Il apparaît au centre de la scène et il est
sélectionné (surligné dans la hiérarchie à gauche).

## Étape 3 — Le déplacer avec le gizmo

Dans la barre d'outils, choisis **↔** (ou touche **W**) et fais glisser les
flèches du gizmo pour poser ton cube où tu veux — par exemple à côté de la
zone jaune. Les autres modes : **E** tourner, **R** redimensionner.

## Étape 4 — Changer sa couleur

Panneau **Inspecteur** (à droite) → section **Matériau** → choisis une
couleur. Tu peux aussi pousser « émissif » pour le faire briller.

## Étape 5 — Lui attacher un script

Toujours dans l'Inspecteur, section **Script (Lua)**, colle :

```lua
obj.ry = obj.ry + 45 * dt
```

(`obj.ry` = rotation de l'objet autour de l'axe vertical, en degrés ;
`dt` = durée de la frame — donc : 45°/seconde, quelle que soit la machine.)

## Étape 6 — Play

Clique **Play**. Déplace le joueur (WASD/flèches, Espace pour sauter).

## Étape 7 — Regarder

Ton cube tourne. Le cube bleu de la scène tourne aussi : c'est exactement le
même script (visible dans `examples/first_game/scripts/rotating_object.lua`).

## Étape 8 — Stop, puis comprendre ce qui persiste

Clique **Stop**. Règle importante : **Stop ramène la scène à l'état d'avant
Play** — les pièces ramassées réapparaissent, tout ce que tu as modifié
*pendant* Play est perdu. Ton cube, ajouté *hors* Play, est toujours là.
(On édite hors Play ; on expérimente en Play.)

## Étape 9 — Sauvegarder

Menu **💾 Enregistrer sous…** → choisis un emplacement, par exemple
`~/Documents/ma_premiere_scene.json`. Rouvre-la avec **📂 Ouvrir…** pour
vérifier : ton cube et son script sont dedans. Ne sauvegarde pas par-dessus
`examples/first_game/scene.json` — garde l'exemple intact.

## Étape 10 (option) — Aller plus loin

- Deuxième script : copie `examples/first_game/scripts/zone_signal.lua` sur un
  objet dont tu coches **Trigger** — il réagira au passage du joueur.
- Export (web, .app, APK) : voir `packaging/EXPORT.md`. *Non couvert par ce
  tutoriel : l'export web/desktop est en cours de re-vérification (Phase C du
  plan de préversion).*

## Ce que tu as vu en 10 minutes

L'éditeur (ajout, gizmo, inspecteur), le scripting Lua inline, le mode
Play/Stop et sa règle de restauration, la sauvegarde/réouverture. Le modèle
complet tient sur une page : [MENTAL_MODEL.md](MENTAL_MODEL.md).
