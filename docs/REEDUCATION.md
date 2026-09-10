# PhysioTech.ch — Attrape-bulles (démo Rééducation, inspirée de Mouvéo)

Le jeu s'appelle **PhysioTech.ch** (titre du HUD et de la page web), créé par
[Antoine Quarroz](https://github.com/antoinequarroz) (Mouvéo, l'original) et
[Loïc Berthod](https://github.com/lberthod) (portage sur RusteeGear). Site
déployable : dépôt [lberthod/physiotek](https://github.com/lberthod/physiotek)
(page + wasm construits depuis ce dépôt).
Démo jouable `Fichier ▸ 🎬 Démos ▸ 🎮 Jeux jouables ▸ 🎯 Rééducation`, console
`demo reeduc`, desktop `motor3derust --player --demo=reeduc`, web
[`reeduc.html`](../packaging/web/reeduc.html) (`?scene=reeduc`). Portage, le
10 septembre 2026, du prototype web
[**Mouvéo**](https://github.com/antoinequarroz/mouveo-reeducation) (React +
MediaPipe Pose + canvas 2D, ~430 lignes) sur RusteeGear.

![Séance en cours : avatar, cible, halo et point suivi](img/reeducation_preview.png)

> **Démonstration technologique de vision par ordinateur, pas un outil
> médical** : aucune valeur médicale, thérapeutique ni scientifique, aucune
> validation clinique, aucun avis médical — comme Mouvéo l'annonçait déjà
> (« prototype de coaching, sans diagnostic ni mesure clinique »). L'utilisateur
> consulte un professionnel de santé et s'arrête en cas de douleur, vertige ou
> inconfort. Origine : [antoinequarroz/mouveo-reeducation](https://github.com/antoinequarroz/mouveo-reeducation).

## Ce que fait le jeu

Deux modes (`rd_mode`, bouton HUD « Mode » ou sélecteur de la page) :
**Attrape-bulles** (0, par défaut) et **Étoile filante** (1, décrit plus bas).

**Attrape-bulles.** Le patient se place face à la caméra (ou
pilote la main du mannequin au joystick), lance la partie, et de petites bulles
colorées apparaissent autour de ses épaules **actuelles** (elles suivent le
patient s'il se déplace), à une distance qui demande un geste (35 à 100 % de
la longueur du bras calibrée × amplitude) et toujours dans la partie de
l'écran que la caméra voit. Plus d'une fois sur deux, l'élément naît dans le demi-cercle du haut, jusqu'à
1,35 × le rayon au-dessus des épaules (borné au haut de l'image) : il faut
lever les bras. Le patient le touche avec l'une ou l'autre main avant qu'il
n'éclate (le halo rétrécit à mesure que la durée de vie s'écoule), avec le
**geste que sa couleur impose** :

| Couleur | Geste | Mesure |
| --- | --- | --- |
| Blanc | toucher | — |
| Bleu | toucher **poing fermé** | ouverture < 1,45 (bouts de doigts / taille de main) |
| Vert | toucher **main ouverte** | ouverture > 1,85 |
| **Cube rouge** (rouge sombre, halo cubique rouge qui pulse, tourne sur lui-même) | **ne pas toucher** : bombe | −20 points, série cassée ; expire sans pénalité |

L'ouverture est mesurée sur les 21 repères de la main (distance moyenne des
cinq bouts de doigts au poignet, rapportée à la distance poignet → base du
majeur). Un mauvais geste laisse l'élément en place et affiche la consigne
(« Bleu : fermez le poing… ») ; sans doigts suivis (mode démo, main trop
petite à l'image), bleu et vert s'attrapent comme un blanc. Répartition hors
bombes : 50 % blanc, 25 % bleu, 25 % vert. Une partie dure 60 s de temps actif (le chrono se
fige quand le corps sort du cadre).

Un élément naît toujours **loin des mains** : jusqu'à huit tirages, le premier
à plus de 0,55 m de chaque main suivie est retenu (sinon le plus éloigné) —
il faut aller le chercher (`HAND_CLEAR`, test `bubbles_spawn_away_from_the_hands`).

| Rythme | Nouvel élément toutes les | Durée de vie | Simultanés | Part de bombes |
| --- | --- | --- | --- | --- |
| Doux | 1,6 s | 6 s | 2 | 8 % |
| Moyen (défaut) | 1,1 s | 4,5 s | 3 | 12 % |
| Vif | 0,8 s | 3,2 s | 4 | 16 % |

Les éléments naissent aussi à **au moins 0,6 m les uns des autres**
(`ELEM_CLEAR`, jamais une bulle posée sur une bombe), et les bombes sont rares
par construction : **une seule en vol à la fois**, puis **3 à 8 s de repos**
sans bombe après chacune (touchée ou expirée, `rd_bomb_next`) — test
`bombs_come_one_at_a_time_with_a_rest_and_elements_keep_their_distance`.

**Score** : 10 points par bulle + 5 par palier de 3 dans la série (série
remise à zéro quand une bulle éclate ou qu'une bombe est touchée, score jamais
négatif), célébrations aux séries de 5 et 10 ;
**bilan** en fin de partie : points, bulles attrapées / éclatées, bombes touchées, précision,
1 à 3 étoiles, record et nombre de parties (conservés le temps de la session,
`save.*`). Les bulles naissent selon un tirage pseudo-aléatoire déterministe
(Park-Miller, identique sur les deux backends Lua), donc rejouable en test.

Les huit exercices structurés de Mouvéo (élévations, genou, pas, squat…) et
les trois exercices de doigts ont existé dans une version précédente de cette
démo (historique git jusqu'à `e5946fa`) ; ils ont été retirés à la demande pour
ne garder qu'un jeu de capture.

**Étoile filante** (`rd_mode` = 1) : une étoile dorée glisse lentement dans la
zone atteignable (courbe de Lissajous autour des épaules, vitesse selon le
rythme : 0,35 / 0,5 / 0,7 rad/s) ; garder une main dessus (rayon 1,25 × celui
d'une bulle) rapporte **10 points par seconde de contact**, plus 5 par palier de
3 s d'affilée ; la perdre après plus de 0,5 s compte une **perte** et casse la
série. Objets « Étoile » et « Halo étoile » (verts en contact, dorés sinon),
`caught` = secondes de contact, `missed` = pertes, `rd_maxcombo` = meilleure
série (s) ; consignes 20 / 21. Test `star_mode_scores_contact_seconds_and_counts_losses`.

**Avatar** : un **mannequin neutre** construit directement sur les repères
captés — tête, cou, torse et bassin (ellipsoïdes), membres en capsules,
articulations, pieds, mains et doigts — donc aux proportions exactes du
patient, sans accessoire ni personnage : c'est ce qu'on attend d'une
application de mouvement. Le bouton « Avatar » bascule sur le squelette de
bâtons (mêmes objets, affinés). Le moteur sait aussi retargeter une pose sur un
mesh skinné (`bone()`, cf. [LUA_API.md](LUA_API.md)) — les personnages
riggés essayés (héros à bouclier, ninja) ont été écartés pour cette démo.

Les **mains** n'existent pas dans Mouvéo : la page fait tourner en plus le
*Hand Landmarker* (21 repères par main, deux mains), et les deux mains suivies
viennent se poser sur les poignets du mannequin, couleur par doigt ; quand
elles sont suivies, c'est le centre de la paume qui attrape les bulles.

**Cadrage** : la caméra de jeu a une largeur minimale visible
(`GameCamera::min_width` = 3 m) : en portrait (téléphone) elle recule juste
assez pour garder le corps entier, bras levés. Le moteur publie aux scripts le
cadre réellement visible (`cam_visible_width` / `cam_visible_height`, unités
monde au niveau de la cible) et le script y ramène bulles, cubes et étoile
(`spawn_bounds`, test `bubbles_stay_inside_the_visible_frame_on_a_portrait_screen`) :
rien n'apparaît hors de l'image, quel que soit l'écran. Le corps capté est normalisé avant tout dessin — torse
hanches→épaules ramené à 0,9 m, hanches centrées à 1,35 m — donc le patient
garde la même taille à l'écran qu'il soit à 1 m ou 3 m de la caméra (les cibles,
relatives au corps, suivent) ; la caméra de jeu est reculée en conséquence.

**Qualité du suivi** : inférence dans un **Web Worker** (sur le thread
principal, elle bloquait le rendu du moteur 10 à 30 ms par image ; repli sur
le thread principal si le worker échoue), à 30 Hz (15 Hz dans Mouvéo) avec
repli automatique à 15 Hz si la machine ne suit pas ; côté moteur, chaque
repère **glisse à 60 Hz de la mesure précédente vers la nouvelle** sur la durée
observée entre deux images (`PoseFrame::sampled`, latence d'une image caméra),
puis un lissage léger (~30 ms) ; les tables Lua `pose`/`hand` ne sont
construites que pour les scripts qui les lisent ; un repère de visibilité
< 0,5 (hors cadre, extrapolé) n'est pas dessiné, et ses os non plus — c'est ce
qui faisait filer des cylindres hors de l'écran quand le patient était trop
près de la caméra.

**Score** : 10 points par cible + 5 par palier de 3 dans la série ; paliers
fêtés à 40 %, 70 % et 100 % de l'objectif ; **régularité** en fin de séance =
écart-type relatif des intervalles entre cibles (50–99 %) ; 1 à 3 étoiles selon
les répétitions faites ; **bilan** déclaratif douleur/fatigue (0–10) ;
**jardin de mobilité** (graine → jardin lumineux, un niveau par 150 points
cumulés) et record, conservés le temps de la session (`save.*`).

## Deux façons de jouer

| | Web `reeduc.html` (Chrome/Edge/Safari récents) | Éditeur, desktop `--player`, APK |
| --- | --- | --- |
| Entrée | **caméra** : MediaPipe Pose Landmarker + Hand Landmarker (wasm + modèles ~18 Mo, chargés d'un CDN au clic « Activer la caméra »), 33 + 2 × 21 repères à ~30 Hz ; les bulles s'attrapent avec la paume (doigts suivis) ou le poignet | **mode démo** : la main droite du mannequin se pilote au stick tactile (souris) ou aux flèches |
| Calibration | 1,8 s de corps visible sans bouger → ancres (épaules, bras) figées : centre et rayon de la zone des bulles | immédiate, silhouette virtuelle debout |
| Pause | automatique dès que le corps sort du cadre (ou pose périmée > 0,75 s) : chrono figé, consigne « Repositionnez-vous » ; **volontaire** par le bouton Pause de la page (`hud:pause` / `hud:reprendre`, bulles retirées sans pénalité) | jamais |
| Hanches hors cadre (assis, trop près) | torse et bassin dessinés sous les épaules, jambes effacées | — |
| Sans caméra | bascule d'elle-même en mode démo (refus, absence, CDN inaccessible) | — |

Le mode est **verrouillé au départ de la séance** (`pose.ok` à l'appui sur
▶ Commencer) : une caméra qui se coupe en cours de séance met en pause, elle ne
change pas de mode.

## Comment c'est construit (et ce qui diffère de Mouvéo)

Tout le gameplay est **en donnée de scène** — objets, scripts Lua, widgets HUD
(`Scene::reeducation_demo`, [src/scene/demos/reeducation.rs](../src/scene/demos/reeducation.rs)) :
la scène s'exporte, s'édite dans l'inspecteur (amplitudes, couleurs, textes,
boutons) et tourne à l'identique sur les cinq cibles, sur les deux backends
Lua (test `reeducation_scripts_run_on_the_web_backend`).

```text
reeduc.html ── caméra ──▶ MediaPipe ──▶ set_pose_landmarks(33 × 4)   (JS, web seulement)
                                              │
                               AppState::pose (app/pose.rs) ── vieillit à chaque pas
                                              │
                                   table Lua `pose` (script_ctx, 2 backends)
                                              │
   « Séance » (script directeur) ── save.rd_* ──▶ Cible / Halo / Point suivi / 13 Repères / 12 Os
                                              │
                                   hud_text(...) ──▶ widgets Text ; Button ──▶ on_event("hud:…")
```

| Mouvéo | RusteeGear | Pourquoi |
| --- | --- | --- |
| squelette 2D dessiné sur la vidéo miroir | mannequin 3D neutre construit sur les repères (ellipsoïdes + capsules, orientés par script), dans le plan `z = 0`, caméra de jeu fixe face au patient ; la vidéo reste en vignette miroir (HTML) | le moteur rend de la 3D, pas un flux vidéo ; la vignette garde le retour visuel « je me vois » |
| état React (`useRef`/`useState`), un composant de 430 lignes | machine à états (accueil, calibration, partie, bilan) dans un script Lua « directeur », état numérique dans `save.*` (préfixe `rd_`), les autres objets le relisent le même tick | pas de callbacks dans l'API script (un chunk par objet et par pas), et `save` est le seul état partagé |
| HUD React + Tailwind (score, série, chrono, consigne, bilan, sliders) | widgets HUD déclaratifs (`hud_text` pour les textes, `Button` → `hud:<action>` : Commencer, Rythme, Amplitude, Avatar) ; sur le web, la **page hôte** masque ces widgets (`set_hud_widgets_visible(false)`) et dessine son propre HUD à partir des variables `ui_*` | pas de sliders parmi les widgets ; hors web, les boutons restent visibles à tous les stades |
| voix de synthèse (`speechSynthesis`), vibration | page hôte : coach **Christophe** (mêmes clips mp3 + repli `speechSynthesis` fr-CH), `navigator.vibrate` sur capture ; moteur seul : `vibrate()` journalisé, pas de voix | pas d'API voix dans le moteur — c'est la page qui parle (cf. « Page hôte » ci-dessous) |
| `localStorage` (historique 30 séances, programme thérapeute) | page hôte : `physiotech-history` (30 séances) et `physiotech-program` (rythme, amplitude, durée, manches, assis, patient) ; moteur seul : `save.*` le temps de la session | la persistance web du moteur ne couvre pas encore `save.*` (cf. LUA_API.md) |
| démo tactile : un clic = cible atteinte | mode démo : point piloté au stick/flèches, le membre suit (coude, genou) | jouable et **testable** sans caméra, sur toutes les cibles |

Coordonnées : `x`, `y` MediaPipe normalisés (0..1, `y` vers le bas, image non
miroir) → monde `x = (0,5 − x) · 4 m`, `y = (1 − y) · 3,2 m` — la droite du
patient apparaît à droite de l'écran, comme dans une glace
(`reeducation::pose_to_world`). Seuil de cible : `max(0,2 m ; 16 % du membre)`,
anti-rebond 0,65 s entre deux cibles, comme dans l'original.

## Page hôte PhysioTech.ch (frontend repris de Mouvéo, 10 septembre 2026)

La page [`reeduc.html`](../packaging/web/reeduc.html) reprend toute l'interface
du composant React de Mouvéo (`app/mouveo-game.tsx`) en HTML, CSS et JS purs —
roadmap et correspondance détaillée dans
[roadmapMouveoFrontend10septembre.md](roadmapMouveoFrontend10septembre.md) :

- **Parcours** : accueil (séance simple ou parcours guidé, ambiance jardin /
  espace / océan, rythme, douleur avant séance) → préparation de l'espace →
  caméra et chargement → calibration (corps détecté, position stable, repères
  prêts) → compte à rebours 3-2-1 → partie (score, série, chrono, consigne,
  qualité du geste) → pause volontaire → repos entre les manches → bilan
  (points, étoiles, précision, meilleure série, régularité, douleur après,
  fatigue, conseil, enregistrer, partager).
- **Coach Christophe** : **une seule voix**, les enregistrements de Mouvéo
  (22 clips mp3, `packaging/web/voice/christophe/`, Chatterbox Multilingual
  V3, MIT, `NOTICE.txt`), jamais la synthèse vocale du navigateur : chaque
  phrase est ramenée au clip le plus proche (`clipFor`) — accueil, position,
  calibration, 3-2-1, départ, captures et séries, bombe, contact perdu, geste
  demandé, pause, reprise, repos, manche suivante, fin de séance ; les
  réglages (rythme, ambiance, amplitude, curseurs) n'ont pas de clip et
  restent muets. **File de lecture** : un seul clip à la fois, jamais coupé sauf
  par un événement de flux (compte à rebours, départ, pause, reprise, fin) ;
  les consignes (bombe, repositionnement, geste) passent en tête de file et
  attendent la fin du clip en cours ; les encouragements ne jouent que dans un
  silence ; un clip en cours ou déjà prévu n'est pas redemandé, et chaque clip a
  un délai minimal de répétition (0,4 / 3 / 5 s selon sa priorité).
- **Progression** (`localStorage`) : 30 séances, jardin de mobilité (un palier
  par 150 points cumulés), badges (premier pas, 100 bulles, précision 85 %,
  3 jours de suite), objectif hebdomadaire de 3 séances, vue « Progrès ».
- **Espace thérapeute** : programme (rythme, amplitude, durée de manche 30 à
  90 s, 1 à 4 manches, mode assis, prénom et code patient) appliqué au moteur
  au départ de la séance ; suivi (séances, dernière précision, dernière série,
  douleur récente sur 7 séances).
- **PWA** : `manifest.json`, `sw.js` (réseau d'abord + cache pour la
  page, le moteur et les voix ; cache d'abord pour MediaPipe), bouton
  Installer, pastille en ligne / hors ligne.

**Pont page ↔ moteur** (exports wasm de [src/lib.rs](../src/lib.rs), appliqués
dans `about_to_wait` comme la pose) :

| Sens | Export | Effet |
| --- | --- | --- |
| page → moteur | `push_hud_event(action)` | même file que les boutons HUD : `demarrer`, `pause`, `reprendre`, `arreter`, `rythme`, `amplitude`, `avatar` |
| page → moteur | `set_script_var(clé, nombre)` | pose une variable `save.*` : `rd_level` (1–3), `rd_amp` (55–95), `rd_duration` (s), `rd_world` (0 jardin, 1 espace, 2 océan — teinte fond et sol) |
| page → moteur | `set_hud_widgets_visible(bool)` | `Scene::hud_widgets_hidden` (runtime, jamais sérialisé) : widgets et rangée ⏸ 🔊 du moteur masqués |
| moteur → page | `window.__rusteegear_vars` | objet `{ ui_* }` remis à jour à chaque image : toutes les variables `save.ui_*` (`AppState::ui_vars`) |

Variables `ui_*` publiées par le script directeur : `ui_stage` (0 accueil,
1 calibration, 4 compte à rebours, 2 partie, 3 bilan), `ui_cam`, `ui_pose_ok`,
`ui_body_ok`, `ui_calib` (0–1), `ui_count` (3-2-1), `ui_points`, `ui_combo`,
`ui_maxcombo`, `ui_caught`, `ui_missed`, `ui_bombs`, `ui_left` (s),
`ui_duration`, `ui_status` (code de consigne), `ui_cel` / `ui_cel_gain`,
`ui_level`, `ui_amp`, `ui_avatar`, `ui_world`, `ui_best`, `ui_games`,
`ui_paused`, `ui_hand_l` / `ui_hand_r` (0 inconnu, 1 poing, 2 ouverte,
3 détendue). La page expose une sonde `window.__physiotech`
(`state`, `applyVars`, `setStage`…) pour le diagnostic et les tests.

**Vérifié** (10 septembre, pane navigateur et Chrome) : moteur rendu dans la
zone de scène, widgets du moteur masqués, `demarrer` reçu (accueil → compte à
rebours), parcours complet de la page piloté par des états moteur simulés
(compte à rebours, partie, capture, série de 5, bulle éclatée, pause, bilan,
enregistrement, parcours de 2 manches avec repos, vues Progrès et
Thérapeute, programme sauvegardé, mise en page mobile). Puis, en production
(physiotech.ch, onglet visible) : manche démo en temps réel, pause, reprise et
arrêt confirmés. Le mode caméra reste à essayer avec un vrai patient.

## Fichiers

- [src/app/pose.rs](../src/app/pose.rs) — `PoseFrame` (33 repères, fraîcheur), `HandFrame` (2 × 21 repères), `AppState::set_pose`/`set_hands`.
- [src/scene/import.rs](../src/scene/import.rs) — directions d'os imposées (`compute_joint_matrices_into_with`), `SceneObject::bone_dirs`, fonction Lua `bone()` ; [src/assets.rs](../src/assets.rs) — modèles `embedded://`.
- [src/lib.rs](../src/lib.rs) — export wasm `set_pose_landmarks`, `?scene=reeduc` / `--demo=reeduc`.
- [src/app/scripting.rs](../src/app/scripting.rs), [scripting_web.rs](../src/app/scripting_web.rs) — table `pose` (cf. [LUA_API.md](LUA_API.md)).
- [src/scene/demos/reeducation.rs](../src/scene/demos/reeducation.rs) — scène, scripts, widgets.
- [packaging/web/reeduc.html](../packaging/web/reeduc.html) — page caméra (publiée par `pages.yml` à côté de `index.html`).
- [examples/gen_reeduc_preview.rs](../examples/gen_reeduc_preview.rs) — régénère l'image ci-dessus (rendu headless, GPU requis).
- Tests : `app::pose`, `scripting::tests::script_reads_pose_landmarks_via_the_pose_table`,
  `scripting_web::tests::differential::{pose_table_matches_between_backends, reeducation_scripts_run_on_the_web_backend}`,
  `simulation::tests::reeducation_{demo_mode_is_playable_with_the_joystick_until_the_goal, camera_mode_calibrates_then_counts_reps_and_pauses_when_the_body_is_lost}`,
  `scene::demos::tests::reeducation_*`, `editor::hud::tests::reeducation_hud_glyphs_are_covered_by_the_embedded_fonts`.

## Limites et suites possibles

- **Caméra = web seulement.** Le moteur natif n'embarque ni capture caméra ni
  inférence ; un client desktop/APK pourrait recevoir les repères d'un petit
  service local (même contrat `33 × 4` flottants) via le pont de pilotage.
- **Persistance web seulement** : historique et programme thérapeute vivent
  dans le `localStorage` de la page hôte ; hors web (éditeur, desktop, APK),
  `save.*` ne survit pas à la session.
- **Précision** : celle de MediaPipe « lite » à 15 Hz, en 2D (la profondeur `z`
  est exposée mais inutilisée, comme dans Mouvéo) — aucune valeur clinique.
- Le suivi caméra n'a pas été essayé sur un vrai patient dans cette session :
  la page a été vérifiée sans caméra (mode démo) et le mode caméra par des poses
  synthétiques dans les tests.
