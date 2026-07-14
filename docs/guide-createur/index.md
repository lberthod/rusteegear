# Créer son premier jeu avec RusteeGear

Ce guide s'adresse à quelqu'un qui n'a **jamais touché à Rust ni à ce projet**. Il
vous emmène de « l'éditeur est vide » à « j'ai un jeu jouable sur mon téléphone
Android », en passant uniquement par des boutons et des cases à cocher — aucune
ligne de code n'est nécessaire pour suivre ce guide.

Si vous cherchez plutôt une explication technique du moteur (architecture,
stack, pourquoi Rust), direction le [README](../../README.md) et
[architecture.md](../architecture.md).

## Avant de commencer

Lancez l'éditeur :

```bash
cargo run
```

Une fenêtre s'ouvre avec une scène 3D vide, une barre de menus en haut
(Fichier / Édition / Ajouter / Outils / Aide), et un bandeau d'état en bas
(FPS, nombre d'objets, mode). C'est tout l'éditeur — pas de deuxième
application à installer.

> Capture d'écran à venir ici (cf. section « Limite connue » en bas de page).

## Étape 1 — Créer une scène

Menu **Fichier ▸ ✨ Nouveau projet**. Une fenêtre s'ouvre avec trois choix :

| Carte | Ce que ça donne |
|---|---|
| **📄 Scène vide** | Rien du tout — pour construire votre propre niveau à partir de zéro. |
| **🕹 Démo contrôleur** | Un personnage déjà pilotable au joystick, avec un décor simple et des collisions. Bon point de départ pour comprendre les contrôles sans rien configurer. |
| **⚔ Niveau de combat** | Une scène avec des vagues de monstres qui poursuivent le joueur (façon *Call of Zombies*). Pour voir tout de suite à quoi ressemble un jeu « complet ». |

Pour ce guide, choisissez **📄 Scène vide** — on va tout construire pas à pas.

D'autres démos (donjon, duel, course infinie, MMORPG…) restent disponibles plus
bas dans le menu Fichier si vous voulez explorer d'autres styles de jeu une
fois ce guide terminé.

## Étape 2 — Ajouter un objet contrôlable

### Ajouter la forme

Menu **Ajouter ▸ 🃏 Ajouter (cartes)…** ouvre un panneau avec des icônes.
Cliquez sur **🧊 Cube** (ou n'importe quelle autre forme — capsule, sphère…).
Un objet apparaît au centre de la scène 3D, et se sélectionne automatiquement
(il devient surligné).

Ce panneau reste ouvert : vous pouvez cliquer plusieurs fois pour ajouter
plusieurs objets d'affilée, ou sur **💡 Ponctuelle** pour ajouter une lumière.
Fermez-le avec la croix quand vous avez ce qu'il vous faut.

### Le rendre pilotable

Sélectionnez l'objet que vous venez d'ajouter (cliquez dessus dans la scène
3D, ou dans la liste **Hiérarchie** à gauche). Le panneau **Inspecteur** à
droite affiche ses propriétés.

Dépliez la section **🧩 Composants mobiles (Android)** en bas de l'inspecteur,
et cochez **🕹 Input Receiver (joystick)**. Deux choses se produisent :

1. Un joystick virtuel apparaît automatiquement à l'écran en mode Jeu (Play).
2. L'objet sélectionné se déplace maintenant avec ce joystick, dans le plan
   du sol.

Survolez les champs (Métallique, Rugosité, Collider…) avec la souris si vous
voulez comprendre ce qu'ils font — chacun a une info-bulle qui explique son
effet en une phrase, sans jargon technique.

### Essayer

Cliquez sur **▶ Play** en haut de l'éditeur. Le joystick apparaît en bas à
gauche : cliquez-glissez dessus pour déplacer votre objet. **⏹ Stop** pour
revenir en mode édition (les changements de position pendant le Play ne sont
pas sauvegardés — c'est fait exprès, pour pouvoir tester sans risque).

## Étape 3 — Ajouter un HUD (interface à l'écran)

Toujours dans le menu **Ajouter ▸ 📱 UI mobile**, un sous-menu liste les
éléments d'interface mobile — joystick virtuel (déjà activé à l'étape
précédente), pavé directionnel, bouton tactile, zone tactile, **❤ Barre de
vie (HUD)**. Cliquez sur **❤ Barre de vie (HUD)** pour afficher une jauge de
santé en haut de l'écran en mode Jeu.

Pour des HUD plus élaborés (texte, image, bouton personnalisé relié à une
action), le bouton **🧩 Widgets HUD** de la barre d'outils (au-dessus de la
scène 3D) ouvre un éditeur dédié — hors du périmètre de ce premier guide,
mais la barre de vie/joystick/boutons suffisent pour un premier jeu jouable.

## Étape 4 — Exporter en APK (Android)

Menu **Fichier ▸ 📦 Build & Export…** (ou l'icône équivalente dans la barre
d'outils). Choisissez la cible **Android**, et cliquez sur **Build**.

La première fois, le panneau vous indique si des prérequis manquent (NDK
Android, cible de compilation) avec un lien pour les installer — suivez ces
indications, ce sont des installations classiques (pas spécifiques à
RusteeGear).

Une fois les prérequis en place, le bouton lance la construction dans un
journal affiché en direct dans le panneau. À la fin, le fichier `.apk` est
prêt (chemin affiché dans le journal) — transférez-le sur votre téléphone
Android pour l'installer et y jouer.

Si votre téléphone est branché en USB (mode développeur activé), le bouton
**Run Device** de la barre d'outils fait tout en un clic : build + installation
+ lancement automatique sur l'appareil.

## Et après ?

- Explorez les autres démos du menu Fichier pour voir d'autres styles de jeu
  déjà construits (donjon, duel, course infinie…) et comprendre comment ils
  sont assemblés.
- Le menu **Ajouter** propose bien plus que ce guide n'en couvre : lumières
  directionnelles/spot, caméra de suivi, physique (statique/dynamique),
  zones de déclenchement, sons.
- Pour aller plus loin (scripts, comportements personnalisés), une doc dédiée
  viendra compléter ce guide — pour l'instant, les démos du menu Fichier sont
  la meilleure source d'exemples concrets.

## Limite connue de ce guide

Ce guide est **texte seul** pour l'instant — pas de capture d'écran ni de GIF.
L'éditeur est une fenêtre native (pas une page web), et l'environnement qui a
rédigé ce guide n'a pas pu en capturer l'écran. Si vous suivez ce guide et que
certaines étapes ne correspondent pas à ce que vous voyez (l'éditeur a pu
changer depuis), un signalement est bienvenu.
