# Limitations connues — Developer Preview 1 (mise à jour du 4 septembre 2026)

Cette page liste ce qui est **volontairement** absent, partiel ou non validé
dans cette préversion. Si vous butez sur un point listé ici, ce n'est pas un
bug à signaler : c'est un choix ou un chantier connu. Tout le reste mérite un
signalement (Aide → 📋 Copier le diagnostic pour le contexte à joindre).

## Matrice de support

Référence unique pour l'état des plateformes : le tableau « Plateformes —
état honnête » du [README](../README.md#plateformes) renvoie ici pour le
détail par fonction.

| Fonction | macOS Editor | Web Player | Android | iOS | Server |
| --- | --- | --- | --- | --- | --- |
| Rendu 3D | ✅ Oui | ✅ Oui (WebGPU, Chrome/Edge) | 🟡 Oui, non re-vérifié préversion | 🟡 Oui, non re-vérifié préversion | — (sans rendu) |
| Import GLB | ✅ Oui | ❌ Non (pas d'éditeur) | ❌ Non | ❌ Non | ❌ Non |
| Scripts Lua | ✅ Oui (Lua 5.4) | 🟡 Sous-ensemble ([LUA_PORTABLE.md](LUA_PORTABLE.md)) | ✅ Oui (5.4) | ✅ Oui (5.4) | ✅ Oui (5.4) |
| Multijoueur | ✅ Oui | ✅ Oui (vérifié 19/07) | 🟡 Oui, non re-vérifié | ❌ Non validé | ✅ Oui (autoritaire) |
| Export de builds | ✅ Oui (Web + macOS vérifiés ; APK/iOS non re-vérifiés) | ❌ Non | ❌ Non | ❌ Non | ❌ Non |
| Édition de scène | ✅ Oui | ❌ Non | ❌ Non | ❌ Non | ❌ Non |

Une case ❌ est **volontaire** (ex. : pas de bouton d'import sur le player
web — c'est un player, pas un éditeur). Une case 🟡 « non re-vérifié » builde
et fonctionnait historiquement, mais n'a pas été re-validée pour cette
préversion — les libellés du panneau Export le rappellent.

## Structure et données

- **Éditeur macOS uniquement.** Linux/Windows non testés (wgpu/Vulkan devrait
  bâtir, mais rien n'est garanti).
- **Système de projet partiel (Sprint 3, 19/07/2026).** Un dossier peut
  déclarer un manifeste `project.rusteegear.json` (nom + scène de démarrage) et
  s'ouvrir comme un projet (`AppState::open_project`, menu Fichier › Ouvrir…) ;
  la scène seule (comportement historique) reste supportée en parallèle. En
  revanche, les assets d'un projet **ne sont pas isolés** : ils vivent toujours
  dans le dossier utilisateur global `~/.motor3derust/assets/`, partagé entre
  tous les projets — pas encore de dossier `assets/` par projet ni d'index
  d'assets. Le gestionnaire de projets (Nouveau projet, récents, dupliquer,
  réouverture du dernier projet au démarrage) existe depuis le Sprint 4 et la
  roadmap UX du 04/09/2026 ; il manque encore une commande « Convertir en
  projet » pour migrer une scène seule et ses assets vers un vrai projet
  autonome. Voir `docs/SprintAudit12h24.md` (Sprint 3, section « cible long
  terme »).
- ~~**Fermeture sans alerte.**~~ **Corrigé (19/07/2026)** : fermer la fenêtre
  (ou Fichier › Quitter) avec des modifications non sauvegardées affiche
  désormais une confirmation Enregistrer / Quitter sans enregistrer / Annuler.
  Nuance : annuler (Ctrl+Z) jusqu'à revenir exactement à l'état sauvegardé
  laisse le drapeau « modifié » posé — l'alerte peut donc être posée à tort,
  jamais absente à tort.
- **Nom interne `motor3derust`.** Visible dans le nom du crate, la doc API
  publiée et le dossier `~/.motor3derust/`. Le produit s'appelle RusteeGear ;
  le renommage interne viendra plus tard (une migration de dossier utilisateur
  prématurée risquerait des pertes de données).

## Éditeur

- **Undo/redo.** Annulables : création/suppression/duplication d'objets,
  groupes, manipulations au gizmo (y compris lumières), prefabs, outils
  d'assets et, **depuis le 04/09/2026**, les éditions dans l'Inspecteur
  (couleur, script, nom, physique…) — une entrée d'historique par rafale de
  modifications, pas une par frappe. **Reste non annulable** : l'import glTF
  (l'objet créé reste supprimable, et cette suppression est annulable).
- **Retours à l'écran (04/09/2026, roadmap UX vague 1).** Sauvegarde,
  import, projet, erreurs de script : toasts en bas à droite + compteur ⛔
  dans la barre d'état ; badge ⛔ sur l'objet dont le script est en erreur.
  Le titre de fenêtre affiche projet, scène et le point « modifié ».
  Restent muets : les opérations d'assets en tâche de fond (texture
  illisible) hors toast, et l'export mobile hors du panneau Build.
- **Sélection désactivée pendant Play** (décision produit) : repasser en
  Pause ou Stop pour sélectionner.
- **Génération IA (scène et scripts) : expérimental.** Nécessite une clé API
  externe ; qualité non garantie.

## Import 3D

- **Matériaux GLB : couleur de base uniquement** (`base_color_factor`).
  Pas de textures, normal maps ni émissifs sur les meshes importés — la
  direction artistique du moteur est « couleur par sommet » (cf. charte
  graphique). Squelettes et clips d'animation glTF sont, eux, supportés.

## Web Player

- **Lua : sous-ensemble portable** — détail et scripts garantis dans
  [LUA_PORTABLE.md](LUA_PORTABLE.md).
- ~~**Canvas : taille figée à l'initialisation.**~~ **Corrigé (04/09/2026,
  roadmap post-audit UX 0.3)** : la page hôte réécrit la taille du canvas à
  chaque redimensionnement de la fenêtre et le rendu suit (vérifié dans
  Chrome 148 : 1280×720 → 900×500 sans rechargement).
- **Musique en flux absente** (SFX fonctionnels).
- **WebGPU requis** : Chrome/Edge récents ; Safari/Firefox selon leur support.

## Qualité / tests

- `creature_1_never_bites_without_contact` (`src/app/simulation_tests.rs`)
  échoue localement sur Apple Silicon (M5 Pro, 04/09/2026) et passe en CI
  Linux — dépendance plateforme non élucidée, à traiter avec les goldens
  ci-dessous. `synth_variation_does_not_repeat…` est un tirage aléatoire :
  rarement rouge, relancer.
- Les goldens de rendu dépendent du GPU réel : un écart entre deux machines
  peut venir du matériel.
- Scènes au-delà de ~500 objets animés : hors cible de cette préversion.

## Mode Player (desktop, web, mobile)

- **Écran d'accueil (04/09/2026, roadmap UX 2.1)** : pseudo mémorisé,
  classe, salon, puis « Jouer en ligne » (serveur public par défaut) ou
  « Jouer seul ». `RUSTEEGEAR_OFFLINE=1` (desktop) saute l'écran et joue
  hors-ligne. Une pastille ● En ligne / Connexion… / Hors ligne reste
  affichée en haut à droite ; les changements d'état (perte, reconnexion)
  passent en bannière 3,5 s. Pas de ping ni de perte de paquets affichés.
- **Pause** (Échap ou ⏸ tactile) : Reprendre, Rejouer, Paramètres, Se
  déconnecter, Quitter (natif). En ligne, la pause est locale — la manche
  continue sans vous, le menu le rappelle.
- **Tactile** : stick gauche à deux axes, orbite au glissé sur la moitié
  droite, boutons ⏸ et Carte en haut à droite. L'écran reste allumé pendant
  la partie sur le web, Android et iOS (04/09/2026). Pas encore : retour
  haptique (`vibrate()` ne fait que journaliser), orientation verrouillée.
- **Mort sans réapparition** (décision assumée) : jusqu'à la fin de manche,
  la caméra suit un allié vivant (Espace passe au suivant, la bannière
  « Vaincu » le nomme) ; sans allié vivant, elle reste sur le joueur. Pas de
  minuteur de réapparition — choix de game design à trancher au playtest.

## Réseau

- Un échec de connexion est affiché (pastille, bannière, fenêtre 🌐) et les
  créatures reprennent leur simulation locale après 2,5 s sans nouvelles du
  serveur. Reconnexion automatique avec repli progressif.
