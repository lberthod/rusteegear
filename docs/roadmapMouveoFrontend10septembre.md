# Roadmap — intégration du frontend Mouvéo dans PhysioTech.ch (10 septembre 2026)

Objectif : reprendre dans **PhysioTech.ch** (démo « Rééducation » de RusteeGear,
page `packaging/web/reeduc.html`, jeu **Attrape-bulles**) toute la logique
frontend du prototype **Mouvéo** d'Antoine Quarroz
(<https://github.com/antoinequarroz/mouveo-reeducation>, commit `20ad0c4`
« Improve mobile-first UI and add Vercel deployment », 10 septembre 2026) :
parcours de séance, coach vocal, historique, espace thérapeute, PWA, design
mobile-first — sans réintroduire les douze exercices (retirés sur demande le
10 septembre au soir : un seul jeu, Attrape-bulles).

Méthode : lecture complète de `app/mouveo-game.tsx` (796 lignes, React 19 +
MediaPipe Pose + canvas 2D), `globals.css`, `sw.js`, `manifest.json`,
pack voix `public/voice/christophe` ; comparaison avec l'état de PhysioTech au
commit `020805b`. Chaque fonctionnalité Mouvéo est rangée dans un sprint
ci-dessous ; les sept sprints ont été exécutés le 10 septembre au soir (moteur :
`src/lib.rs`, `src/scene/demos/reeducation.rs`, `src/editor/{hud,mod}.rs`,
`src/scene/mod.rs` ; page : `packaging/web/reeduc.html` réécrite, 1 570 lignes ;
PWA : `manifest.json`, `sw.js`, `favicon.svg`, `voice/`).

## Ce que Mouvéo fait et que PhysioTech ne faisait pas

| Domaine | Mouvéo (`mouveo-game.tsx`) | PhysioTech avant cette roadmap |
| --- | --- | --- |
| Parcours | `welcome → setup → loading → calibrate → countdown → playing → paused → rest → finished` | accueil → calibration → partie → bilan, dans le HUD du moteur ; pas de préparation, ni compte à rebours, ni pause volontaire, ni repos |
| Interface | React + Tailwind + shadcn, mobile-first, cartes, dock d'action, thème sombre `#07111f` / lime `#c4ff4a`, trois univers (jardin, espace, océan) | widgets HUD egui du moteur (texte + 4 boutons en bas à droite) |
| Coach | « Christophe » : 22 clips mp3 (Chatterbox, MIT) + repli `speechSynthesis` fr-CH, anti-répétition 6,5 s / 12 s, bouton test, coupure | aucune voix |
| Retour haptique | `navigator.vibrate` (45 ms, triple aux paliers) | `vibrate()` Lua journalisé, sans effet sur le web |
| Qualité du geste | carte « amplitude / stabilité / rythme » mise à jour toutes les 350 ms | consigne texte seulement |
| Bilan | points, étoiles, régularité, retour contrôlé, douleur avant/après, fatigue, conseil, enregistrer, partager | points, bulles, précision, étoiles, record (session seulement) |
| Progression | historique `localStorage` (30 séances), jardin de mobilité (5 paliers / 150 pts), 4 badges, série de jours, objectif hebdo (3 séances), vue « Progrès » | `save.rd_best` / `rd_games` perdus au rechargement |
| Thérapeute | programme (parcours 1–4 jeux, côté, amplitude, répétitions, mode assis), code patient, suivi douleur (7 séances) | boutons Rythme / Amplitude dans le HUD |
| PWA | `manifest.json`, `sw.js` (cache `mouveo-v11`, réseau d'abord), bouton Installer, pastille en ligne / hors ligne | rien |
| Agent | outil WebMCP `start_demo_session` | rien |

## Architecture retenue

Le moteur reste la seule source de vérité du jeu (scène + script Lua
directeur, identique sur les cinq cibles) ; la page web devient **l'hôte de
l'interface**, comme le composant React l'était pour le canvas 2D de Mouvéo.

```text
reeduc.html (interface PhysioTech, style Mouvéo)
   │  push_hud_event("demarrer" | "pause" | "reprendre" | "arreter" | …)
   │  set_script_var("rd_level" | "rd_amp" | "rd_duration" | "rd_world", n)
   │  set_hud_widgets_visible(false)        ← la page dessine le HUD à la place du moteur
   ▼
moteur wasm ── script directeur (save.rd_*) ── publie save.ui_* chaque image
   │
   └─▶ window.__rusteegear_vars = { ui_stage, ui_points, ui_combo, ui_left, … }  (lu par la page à 60 Hz)
```

Sans page hôte (éditeur, desktop `--player`, APK), rien ne change : le HUD
egui reste affiché et les boutons du moteur pilotent la séance.

## Sprint 1 — Pont page ↔ moteur (Rust, taille M) — fait

- [x] 1.1 Exports wasm `push_hud_event(action)` et `set_script_var(clé, valeur)` (mis en attente comme `set_pose_landmarks`, appliqués dans `about_to_wait`) ; `AppState::set_script_var`
- [x] 1.2 Export `set_hud_widgets_visible(bool)` : la page héberge le HUD, les widgets `Scene::hud_widgets` ne sont plus dessinés (drapeau runtime, jamais sérialisé)
- [x] 1.3 `window.__rusteegear_vars` : toutes les variables `save.ui_*` publiées à chaque image (objet JS), à côté de `__rusteegear_state`
- [x] 1.4 Script directeur : `ui_*` (étape, caméra, corps visible, calibration, compte à rebours, points, série, meilleure série, attrapées, éclatées, bombes, temps restant, consigne, célébration, rythme, amplitude, record, parties, pause, durée, état des mains)
- [x] 1.5 Événements `hud:pause` / `hud:reprendre` (chrono figé, bulles retirées sans pénalité) et `hud:arreter` ; durée de partie réglable (`rd_duration`, 60 s par défaut) ; compte à rebours 3-2-1 (étape 4) après la calibration et en mode démo ; meilleure série `rd_maxcombo`
- [x] 1.6 Ambiance `rd_world` (0 jardin, 1 espace, 2 océan) : le fond et le sol de la scène se teintent
- [x] 1.7 Tests : compte à rebours puis partie, pause volontaire, durée réglée, `ui_*` cohérentes avec `rd_*`, widgets masqués ; `cargo test --lib reeduc` sur les deux backends Lua ; `cargo clippy`

## Sprint 2 — Coquille PhysioTech (page, taille L) — fait

- [x] 2.1 Design system repris de `globals.css` : palette, cartes (`choice-card`, `exercise-tile`, `pain-card`), dock d'action, `pressable`, animations (`reward-pop`, `spark-fly`, `countdown-pop`), `prefers-reduced-motion`, safe areas, paysage mobile
- [x] 2.2 Écran **Accueil** : titre, choix Séance simple / Parcours guidé, ambiance (jardin, espace, océan), rythme, douleur avant séance (0–10, alerte ≥ 5), boutons Commencer / Mode démo
- [x] 2.3 Écran **Préparation** (« Préparez votre espace », trois consignes, caméra) puis chargement (caméra + modèles) avec message d'erreur inline
- [x] 2.4 Panneau **Calibration** (corps détecté / position stable / repères prêts, barre de progression) branché sur `ui_calib` et `ui_body_ok`
- [x] 2.5 **Compte à rebours** 3-2-1 (`ui_count`), **HUD de jeu** (score, série ×n, chrono, consigne, pastille « Repositionnez-vous »), boutons Pause / Arrêter, **écran Pause**, **Célébrations**
- [x] 2.6 **Bilan** : points, étoiles, bulles / éclatées / bombes / précision / meilleure série / régularité, conseil, douleur après + fatigue, Enregistrer, Nouvelle séance, Partager (`navigator.share` ou presse-papiers)
- [x] 2.7 Barre latérale (≥ 1024 px, défile seule ; sous 1024 px, empilée sous la scène) : parcours de séance, carte qualité du geste, carte séance, jardin, objectif hebdo, rappel « Bougez sans douleur »
- [x] 2.8 Outil WebMCP `start_demo_session` ; mode démo au stick tactile / flèches conservé

## Sprint 3 — Coach Christophe (taille S) — fait

- [x] 3.1 Pack voix copié dans `packaging/web/voice/christophe/` (22 mp3 + `NOTICE.txt`, Chatterbox Multilingual V3, MIT, persona original)
- [x] 3.2 Correspondance texte → clip adaptée à Attrape-bulles (prêt, position, calibré, 3-2-1, c'est parti, série, mission terminée, jeu suivant, repositionnez-vous, pause, reprise, bon contrôle) ; repli `speechSynthesis` fr-CH pour les autres phrases (bombe, poing, main ouverte)
- [x] 3.3 Anti-répétition (même cue : 12 s ; toute phrase : 6,5 s), bouton Christophe (on/off) + Tester, arrêt de la voix à la fin de séance
- [x] 3.4 Vibration sur capture (45 ms), triple aux séries de 5 et 10, longue sur bombe

## Sprint 4 — Progression (taille M) — fait

- [x] 4.1 Historique `localStorage` `physiotech-history` (30 séances : date, points, bulles, éclatées, bombes, précision, meilleure série, régularité, rythme, amplitude, durée, manches, douleur avant / après, fatigue)
- [x] 4.2 Régularité (écart-type relatif des intervalles de capture, 50–99 %) calculée côté page à partir des captures observées
- [x] 4.3 Jardin de mobilité (Graine → Jardin lumineux, 150 pts / palier), badges (Premier pas, 100 bulles, Précision 85 %, Rythme 3 jours), série de jours, objectif hebdo 3 séances
- [x] 4.4 Vue **Progrès** : séances, record, précision moyenne, liste des séances ; vide accueillant

## Sprint 5 — Espace thérapeute (taille M) — fait

- [x] 5.1 Programme `physiotech-program` : rythme, amplitude (55–95 %), durée de manche (30 / 45 / 60 / 90 s), nombre de manches (1–4), mode assis, nom et code patient ; Enregistrer
- [x] 5.2 Parcours multi-manches : repos entre les manches (écran « Respirez tranquillement », Continuer / Terminer ici), recalibration au retour caméra, parcours affiché dans la barre latérale
- [x] 5.3 Suivi patient : séances réalisées, dernière précision, dernière série, dernière douleur, douleur récente (7 séances), rappel « données déclaratives »

## Sprint 6 — PWA et mobile (taille S) — fait

- [x] 6.1 `manifest.json` (PhysioTech.ch, `standalone`, `#07111f`), `favicon.svg`, balises `theme-color` / `apple-mobile-web-app`
- [x] 6.2 `sw.js` : réseau d'abord + cache pour la page, `pkg/` (moteur, ~13 Mo, mis en cache à la première lecture), voix ; cache d'abord pour les modèles et le wasm MediaPipe (CDN) — jouable hors ligne après une première séance avec caméra
- [x] 6.3 Bouton Installer (`beforeinstallprompt`), pastille en ligne / hors ligne
- [x] 6.4 Mobile : orientation paysage conseillée, dock collé en bas, `100dvh`, `touch-action`

## Sprint 7 — Livraison (taille S) — fait

- [x] 7.1 `pages.yml` et recette VPS : copier `voice/`, `manifest.json`, `sw.js`, `favicon.svg` à côté de `reeduc.html` — VPS : `rsync` de `reeduc.html` (en `index.html`), `pkg/`, `voice/`, `sw.js`, `manifest.json`, `favicon.svg` vers `/mnt/data/mouveo/dist` (non redéployé dans cette session)
- [x] 7.2 `docs/REEDUCATION.md` : page hôte, pont, coach, progression, thérapeute, PWA ; `docs/LUA_API.md` : convention `ui_*`
- [x] 7.3 Vérification navigateur : moteur rendu dans la zone de scène, widgets masqués, `demarrer` reçu ; parcours complet piloté par des états moteur simulés (`window.__physiotech.applyVars`) — compte à rebours, partie, captures, série, pause, bilan, enregistrement, parcours 2 manches, Progrès, Thérapeute, mobile ; console sans erreur (hors `navigator.vibrate` sans geste) ; `cargo test --lib`, `cargo clippy`, `build_web.sh`. Puis, en production (physiotech.ch, onglet visible) : manche démo en temps réel (compte à rebours, chrono, bulles), pause (chrono figé), reprise, arrêt — sans erreur console. **Reste** : le mode caméra avec un vrai patient
- [x] 7.4 Mémoire projet mise à jour

## Non repris (et pourquoi)

- Les **douze exercices** de Mouvéo (`EXERCISES`, motifs `patterns`, mécaniques pop / trail / hold / rhythm / goalie / memory / mirror / platform, modes bras / genou / cheville / hanches, niveau adaptatif sur le retour contrôlé) : retirés le 10 septembre à la demande du propriétaire ; l'historique git jusqu'à `e5946fa` en garde une version. Le programme thérapeute prescrit donc des **manches** d'Attrape-bulles (rythme, amplitude, durée) plutôt qu'un enchaînement d'exercices.
- La pile **React / Next / Vite / Tailwind / shadcn** et l'hébergement **Cloudflare Sites / Vercel / D1 / Drizzle / Sign in with ChatGPT** (`app/chatgpt-auth.ts`, `db/`, `examples/d1`) : PhysioTech est une page statique sans étape de build JS, servie par Caddy ; le style est réécrit en CSS pur avec les mêmes valeurs.
- **MediaPipe servi localement** (`public/mediapipe/`, 30 Mo) : le CDN jsdelivr reste la source, mis en cache par le service worker ; à reconsidérer si le CDN pose problème.
- **Squelette 2D dessiné sur la vidéo** : PhysioTech rend un mannequin 3D ; la vidéo reste en vignette miroir.

## Retours du 10 septembre (soir, après mise en ligne) — fait

- [x] R.1 **Responsive sans zoom** : au-dessus de 1024 px, l'application tient dans la fenêtre (`body` en colonne de `100dvh`, scène étirée sur la hauteur restante, barre latérale qui défile seule, pied de page compact) ; police en `clamp(13px, …, 15px)` ; bouton « Plein écran » nommé (touche F) ; en portrait / zone étroite, la caméra de jeu recule d'elle-même (`GameCamera::min_width`, test `game_camera_backs_off_on_narrow_screens_to_keep_its_min_width`)
- [x] R.2 **Christophe parle à chaque action** : choix de séance, ambiance, rythme, amplitude, douleur, caméra, calibration, mode démo, chaque capture (clip « Série » tous les trois enchaînements, mot court entre-temps), bulle éclatée, bombe, geste demandé, corps perdu, pause, reprise, arrêt, repos, manche suivante, bilan lu (points, bulles, conseil), enregistrement, partage, vues Progrès / Thérapeute, programme enregistré, retour ; anti-répétition des consignes de jeu ramené à 2 s / 4 s
- [x] R.3 **Logo** : `favicon.svg` dans l'en-tête, l'écran de chargement, l'orbe d'accueil et le bilan (et déjà dans le manifeste PWA)
- [x] R.4 **Bombes en cubes noirs** : objets « Bombe i » (cube noir qui tourne) et « Halo bombe i » (cube rouge translucide) à la place des sphères ; textes « cubes noirs » dans le HUD moteur, la page et la doc
- [x] R.5 **Tout à l'écran à 100 %** : accueil en deux colonnes (choix de séance et douleur à gauche, ambiance et rythme à droite, titre réduit, dock collé au bas de la scène), barre latérale condensée (objectif de la semaine fusionné dans le jardin, jardin masqué pendant la séance, carte séance masquée au bilan), bilan en deux colonnes (métriques et conseil à gauche, douleur, fatigue et actions à droite). Mesuré à 1366×768 et 1440×820 : accueil, barre latérale et bilan sans défilement

## Retours du 10 septembre (nuit) — fait

- [x] N.1 **Une seule voix** : plus de clips mp3, tout passe par la synthèse vocale du navigateur avec une voix française choisie une fois (fr-CH, sinon Thomas / Google / Audrey / Amélie, sinon fr-FR) ; service worker sans les mp3 (`physiotech-v4`)
- [x] N.2 **Apparition loin des mains** : bulles et cubes naissent à ≥ 0,55 m de chaque main suivie (jusqu'à huit tirages, sinon le plus éloigné) — `HAND_CLEAR`, test `bubbles_spawn_away_from_the_hands`
- [x] N.3 **Second mode « Étoile filante »** : `rd_mode` (0 bulles, 1 étoile), bouton HUD « Mode », sélecteur sur l'accueil et dans le programme thérapeute (`program.game`), étoile en Lissajous autour des épaules à vitesse selon le rythme, 10 points par seconde de contact + paliers de 3 s, pertes comptées, consignes 20 / 21, `ui_mode` / `ui_star_hit`, libellés et bilan adaptés (secondes de contact, temps en contact, contacts perdus, étoiles à 40 / 25 / 10 s par minute), historique avec le mode ; test `star_mode_scores_contact_seconds_and_counts_losses`
- [x] N.4 **Voix : les enregistrements partout** (retour demandé) : plus de synthèse vocale, chaque phrase est ramenée au clip Christophe le plus proche (`clipFor`), les réglages sans clip restent muets ; service worker `physiotech-v5` avec les clips
- [x] N.5 **Bombes rouges** : le cube passe au rouge sombre (son halo l'était), textes « cubes rouges » dans le HUD, la page et la doc
- [x] N.6 **Voix sans doublons** (phrases doublées, hachées) : file de lecture à priorités (`enqueue`) — flux 3 interrompt, consignes 2 en tête de file, encouragements 1 seulement dans un silence ; clip en cours ou prévu jamais redemandé ; délai de répétition par priorité ; déclencheurs en double retirés (bombe parlait deux fois, « C'est parti » à chaque retour de consigne) ; « Deux / Un » attendent la fin de « Calibration terminée » en mode caméra ; vérifié par un faux lecteur audio dans la page (une lecture par événement, aucune coupure)
- [x] N.7 **Apparitions** : espacement ≥ 0,6 m entre éléments en vol (plus de bulle sur une bombe), une seule bombe à la fois, 3 à 8 s sans bombe après chacune, part de bombes 8 / 12 / 16 % (au lieu de 12 / 22 / 30), jusqu'à 4 bulles simultanées (objets « Bulle 1..4 »), cadence 1,6 / 1,1 / 0,8 s
- [x] N.8 **Navigateur mobile** : pendant la séance la scène prend toute la hauteur visible (`body.in-session`, remontée en haut), vignette caméra en haut à gauche sous le score, consigne compacte en bas, boutons Pause / Arrêter réduits, bouton plein écran masqué sur écran tactile (inutile sur iOS) ; en portrait la caméra de jeu recule juste assez pour le corps entier (`min_width` 3 m) et le moteur publie `cam_visible_width/height` pour que bulles, cubes et étoile restent dans l'image (test portrait)
- [x] N.9 **Plein écran sur iPad** (11 septembre) : le bouton n'est plus masqué pour tout écran tactile mais affiché dès que l'API plein écran existe (`fullscreenEnabled` ou préfixes WebKit) — iPad, Android, desktop ; iPhone reste sans (Safari ne le permet que pour les vidéos) ; libellé « Quitter le plein écran » une fois dedans, remonté au-dessus de la consigne en séance sur mobile
