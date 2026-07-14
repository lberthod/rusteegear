# `src/editor/` (mod.rs, windows.rs, hud.rs, export.rs, readiness.rs, menus.rs, hierarchy.rs)

Historique déplacé hors du code (Sprint 103a-3) — attribution par sprint et
récits de bugs réels, plutôt que la doc technique qui reste dans les
fichiers. Les sept fichiers sont regroupés ici : ce sont tous des sous-modules
de l'UI de l'éditeur, extraits de l'ancien `editor/mod.rs` monolithique au
Sprint 103a-2 (`windows.rs`, `hud.rs`, `menus.rs`, `hierarchy.rs`).

## Attribution par sprint

- **Sprint 19** — `export.rs` (`ExportPanel`) : panneau « Build & Export »
  (`.dmg`/`.apk`/`.ipa`) avec build en thread de fond et log streamé.
- **Sprint 32** — `readiness.rs` : « APK Readiness Check » (scène vide,
  éclairage, colliders manquants, textures trop grandes/introuvables,
  identité de bundle).
- **Sprint 39** — champs Android de `BuildConfig` (orientation, min/target
  SDK) dans `export.rs`/`readiness.rs`.
- **Sprint 65** — `mobile_multiplayer_overlay` (`windows.rs`) : overlay
  Multijoueur minimal pour le mode Player (mobile/APK).
- **Sprint 79** — cycle de l'arme équipée via un bouton tactile unique
  (`Controller::weapon_button`), remplacé plus tard par
  `weapon_inventory_panel` (liste complète, cf. bugs ci-dessous).
- **Sprint 81** — préréglages de time scale dans la toolbar + « ⏭ » (pas
  unique en pause) dans `mod.rs`.
- **Sprint 82** — Console intégrée à l'éditeur (`tool_windows` dans
  `windows.rs`) : historique de logs + commandes (`timescale`, `pause`,
  `step`, `tp`, `net_stats`…).
- **Sprint 83** — vue de debug dans la toolbar (Éclairé/Normales/Profondeur).
- **Sprint 84** — grille 2 colonnes des boutons tactiles mobiles
  (`mobile_overlay` dans `hud.rs`), cf. bug ci-dessous.
- **Sprint 91** — case à cocher Bloom dans les réglages de rendu de
  `export.rs`.
- **Sprint 93** — panneau « 👁 Aperçu HUD » (`HudPreview`, `hud_preview_window`
  dans `windows.rs`) : prévisualiser les overlays de jeu en Édition.
- **Sprint 98** — mode « 🖐 Repositionner » du panneau Aperçu HUD : rend les
  overlays glissables, position persistée dans `Scene::hud_layout`.
- **Sprint 100** — choix de forme de collider (TriMesh/ConvexHull) exposé
  dans l'inspecteur (`mod.rs`), en miroir de `ColliderShape` côté
  `runtime/physics.rs`.
- **Sprint 103a-2** — éclatement de l'ancien `editor/mod.rs` monolithique en
  sous-modules : `windows.rs` (fenêtres flottantes), `hud.rs` (HUD de jeu),
  `menus.rs` (barre de menus), `hierarchy.rs` (panneau hiérarchie).
- **GAMEDESIGN_EN_LIGNE.md §3.4** — `multiplayer_roster_panel` (`hud.rs`) :
  le tableau des joueurs en ligne existait déjà côté réseau
  (`network_client::multiplayer_roster`) mais n'avait jamais eu d'UI.
- **GAMEDESIGN_EN_LIGNE.md §3.1** — `defeated_banner` (`hud.rs`) : retour
  visuel persistant pour un joueur réseau à 0 PV.

## Bugs réels trouvés en testant

- **Icône Multijoueur invisible en plein écran Android** : en immersif
  (`NativeActivity`), la zone de rendu passe sous la barre de statut système.
  Un premier ancrage à 8 px du bord laissait l'icône 🌐 de
  `mobile_multiplayer_overlay` cachée dessous, donc impossible à toucher.
  Corrigé avec un décalage vertical plus généreux (`y=56`).

- **Frags (`kills_hud`) chevauchant l'overlay Multijoueur** : un premier
  réglage à `y=86` chevauchait encore le bord de l'overlay replié
  `mobile_multiplayer_overlay` (ancré à `y=56`, ~30 px de haut replié).
  Corrigé à `y=112`, qui laisse une vraie marge.

- **Texte du HUD d'arme illisible sur sol clair** : sans plaque de fond
  semi-transparente derrière le libellé de `weapon_hud`, le texte blanc/orange
  devenait illisible sur un sol clair (vert olive, sable...). Ajout d'une
  plaque unique sous les deux lignes de texte (plutôt qu'une par ligne, plus
  net visuellement).

- **Aucun repère visuel pour viser** : sans réticule, viser une cible avec la
  boule de feu n'avait aucun repère à l'écran — ne « faisait pas vrai jeu » en
  testant. Ajout de `crosshair`, discret, affiché seulement quand la scène a
  un contrôleur d'arme à distance.

- **Mort en multijoueur sans retour** : avant `defeated_banner`, un joueur
  réseau tombant à 0 PV n'avait qu'un flash rouge d'un tiers de seconde
  (`damage_flash`) puis l'objet devenait invisible en silence — écran
  figé/vide indiscernable d'un bug. Ajout d'une bannière « Vaincu —
  spectateur » persistante, distincte de `lose_banner` (défaite solo) car la
  manche continue pour les autres joueurs.

- **Score absent en multijoueur** : `collectibles_hud` (⭐ ramassés/total) ne
  s'affiche que si la scène a des collectibles, ce qui n'est pas le cas de la
  carte multijoueur — sans HUD dédié, aucun score de partie n'était jamais
  visible en ligne. Ajout de `kills_hud` (Frags), toujours affiché en Play.

- **Pavé tank W/A/S/D demandé sur APK et macOS** : l'ancienne croix
  directionnelle n'écrivait que `input.joy` (déplacement caméra-relatif), un
  simple doublon discret du joystick. Ajout d'un second schéma de contrôle
  tactile (`cfg.dpad`) calqué sur les touches clavier W/A/S/D du desktop.

- **Boutons WASD vides sur APK réel** : les triangles ▲▼◀▶ envisagés pour le
  pavé directionnel manquaient dans la fonte egui embarquée sur Android —
  rendus en carrés vides (capture d'écran utilisateur à l'appui). Remplacés
  par des lettres ASCII (W/A/S/D), qui existent dans toutes les fontes.
  Même contrainte de fonte pour `weapon_hud` (pas d'emoji dans son libellé).

- **Grille de boutons tactiles qui chevauchait le pavé tank (Sprint 84)** :
  avec 4 boutons (Saut/Feu/Arme/Soin) alignés sur une seule rangée, celle-ci
  débordait assez à gauche sur un téléphone de largeur courante pour
  chevaucher le pavé W/A/S/D — le « Sa » de Saut se retrouvait caché derrière
  le S du pavé. Corrigé avec une grille à 2 colonnes fixes qui pousse en
  hauteur plutôt qu'en largeur, quel que soit le nombre de boutons.

- **Feu/Arme/Soin invisibles dans l'inspecteur** : ces combos bouton tactile
  (`fireball.rs`) n'étaient réglables que directement dans le JSON de scène
  (`assets/player_scene.json`), sans aucune UI — contrairement au bouton de
  Saut. Exposés dans l'inspecteur avec le même widget combo-box.

- **Inventaire d'armes demandé en jeu réel** : le simple cycle au bouton
  tactile « Arme » (Sprint 79) ne permettait pas de voir tout son inventaire
  d'un coup. Remplacé par `weapon_inventory_panel`, un vrai panneau listant
  chaque arme (pastille de couleur + nom), avec sélection directe au clic.
