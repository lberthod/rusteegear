# Limitations connues — Developer Preview 1 (19 juillet 2026)

Cette page liste ce qui est **volontairement** absent, partiel ou non validé
dans cette préversion. Si vous butez sur un point listé ici, ce n'est pas un
bug à signaler : c'est un choix ou un chantier connu. Tout le reste mérite un
signalement (Aide → Diagnostic système pour le contexte à joindre).

## Matrice de support

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
  d'assets. Pas de gestionnaire de projets (créer/récents/dupliquer, prévu
  Sprint 4) ni de commande « Convertir en projet » pour migrer une scène seule
  et ses assets vers un vrai projet autonome. Voir `docs/SprintAudit12h24.md`
  (Sprint 3, section « cible long terme »).
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

- **Undo/redo partiel.** Annulables : création/suppression/duplication
  d'objets, groupes, manipulations au gizmo (y compris lumières), prefabs,
  outils d'assets. **Non annulables** : l'édition de champs dans l'Inspecteur
  (couleur, script, nom, physique…) et l'import glTF (l'objet créé reste
  supprimable, et cette suppression est annulable).
- **Sélection désactivée pendant Play** (décision produit) : repasser en
  Pause ou Stop pour sélectionner.
- **Génération IA (scène et scripts) : Experimental.** Nécessite une clé API
  externe ; qualité non garantie.

## Import 3D

- **Matériaux GLB : couleur de base uniquement** (`base_color_factor`).
  Pas de textures, normal maps ni émissifs sur les meshes importés — la
  direction artistique du moteur est « couleur par sommet » (cf. charte
  graphique). Squelettes et clips d'animation glTF sont, eux, supportés.

## Web Player

- **Lua : sous-ensemble portable** — détail et scripts garantis dans
  [LUA_PORTABLE.md](LUA_PORTABLE.md).
- **Canvas : taille figée à l'initialisation.** Redimensionner la fenêtre du
  navigateur après le chargement ne redimensionne pas encore le rendu
  (correctif en chantier) — recharger la page si besoin.
- **Musique en flux absente** (SFX fonctionnels).
- **WebGPU requis** : Chrome/Edge récents ; Safari/Firefox selon leur support.

## Qualité / tests

- Un test connu comme instable (`roguelike_demo_clears_rooms…`) est marqué
  `#[ignore]` — préexistant, suivi à part ; le lancer à la main :
  `cargo test roguelike_demo_clears -- --ignored`.
- Les goldens de rendu dépendent du GPU réel : un écart entre deux machines
  peut venir du matériel.
- Scènes au-delà de ~500 objets animés : hors cible de cette préversion.

## Réseau

- Le mode Player se connecte **automatiquement** au serveur public par défaut
  (c'est la démo partagée) ; `RUSTEEGEAR_OFFLINE=1` pour jouer hors-ligne
  (desktop). Un échec de connexion est affiché et les créatures reprennent
  leur simulation locale après 2,5 s sans nouvelles du serveur.
