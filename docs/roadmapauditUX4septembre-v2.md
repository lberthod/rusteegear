# Roadmap post-audit UX v2 — 4 septembre 2026 (soir)

Deuxième audit UX / ergonomie / fonctionnel de la journée, livré sur le commit
`5d8b0d2`, c'est-à-dire **après** l'exécution des vagues 0 à 5 de
`roadmapauditUX4septembre.md`. Objectif : vérifier ce qui a été livré en le
faisant tourner, et relever ce qui reste ou ce qui est apparu.

Méthode : CI et Pages vérifiés job par job (`gh run view`, 7 jobs CI + 2 jobs
Pages verts) ; éditeur compilé en `dev-fast`, emballé dans un bundle `.app`
jetable pour pouvoir le ramener au premier plan, piloté (`--pilot`) et cliqué
(événements CGEvent) avec captures de la vraie fenêtre ; démo web publique
ouverte dans Chrome/WebGPU en 1280×800 ; mode joueur desktop lancé avec un
`HOME` vierge ; trois relectures ciblées du code (éditeur, joueur/mobile/web,
documentation). Environ 110 constats bruts, consolidés ci-dessous.

Identifiants : S (serveur/réseau), J (joueur), E (éditeur), T (tactile),
W (web), D (documentation). Coûts en tailles de t-shirt, même format que les
roadmaps précédentes. « Constaté » = reproduit en direct ; « code » = vérifié
dans la source avec la référence.

## Constats bloquants

| ID | Constat | Preuve | Coût |
| --- | --- | --- | --- |
| S-01 | Serveur public : tout nouvel arrivant voit « Manche perdue » en moins de 30 s. Web (Invité0, joueur 21) et desktop (Invité7000, joueur 22) : bannière « Manche perdue — 0 frag(s), 0 assist(s), +0 XP », vie pleine, aucun ennemi rencontré. Sur le web, « Rejouer » déconnecte en silence (pastille « Hors ligne ») et garde la bannière. Le pont renvoie `lost: false` : l'état vient du serveur, pas du client. | Constaté 2× ; `pilot state` | M |
| J-01 | Toute l'interface joueur (accueil, pause, pastille, aide, carte) n'est construite que si `scene.mobile.any()`. Un jeu exporté sans contrôles tactiles n'a aucune UI. Corollaire constaté : stick et boutons Saut/Feu/Arme/Soin dessinés dans Chrome et dans la fenêtre macOS, et l'aide F1 affiche la section « Tactile » sur desktop. | `src/gfx/renderer/frame.rs:276`, `src/editor/mod.rs:1344` ; captures | M |
| J-02 | Solo sans enjeu : `pilot player` → `"health": null` avant et après 20 s de jeu ; `assets/player_scene.json` a `health_bar: false` et `attack_button: ""` ; `hud_health` n'est jamais alimenté depuis `net_local_health`. J n'attaque rien. La démo web affiche pourtant une barre verte : les deux builds ne montrent pas le même HUD. | Constaté ; `src/app/creature_attack.rs:561`, `src/app/network_client.rs:790` | S |
| E-01 | Premier lancement de l'éditeur (`HOME` vierge) : pas d'écran d'accueil, le hameau de 994 objets s'ouvre avec le titre « motor3derust_scene.json » alors que ce fichier n'existe pas, barre d'état « (sans projet) ». Cmd+S écrit alors en silence dans `~/motor3derust_scene.json`. Le point 1.7 de la roadmap précédente est marqué fait. | Constaté ; `src/lib.rs:905-940`, `src/app/persistence.rs:178-188` | M |
| E-02 | La modale « Travail non enregistré détecté » revient à chaque lancement tant qu'on ne sauvegarde pas (constaté « il y a 5 min » puis « il y a 28 min ») ; « Ignorer » n'est pas mémorisé ; le chemin brut est toujours affiché sous la date. | Constaté 2× | S |
| J-03 | Paramètres joueur : première section « Multijoueur — comptes (Firebase) » avec clé API et URL ; réglage éditeur « Rouvrir le dernier projet » exposé ; panneau translucide sans défilement, dessiné sous les boutons du menu pause, et l'aide F1 s'empile par-dessus. | Constaté (web) ; `src/editor/windows.rs:884,941,1076` | S |
| W-01 | Web : rien n'est persisté (`localStorage` vide après une partie) : pseudo, volumes, langue, remap. Tout le monde s'appelle « Invité0 ». Le stockage passe par `HOME`, inexistant sur wasm. | Constaté ; `src/app/settings.rs:265`, `src/assets.rs:285` | M |
| E-03 | Cmd+S pendant Play écrase la scène du projet avec l'état simulé ; Suppr / Cmd+D / Cmd+C-V-X restent actifs en Play et en mode joueur (depuis la pause, un joueur peut supprimer un objet). | `src/lib.rs:646,654,634-654`, `src/app/picking.rs:171` | S |

## Vague 0 — Un serveur où l'on peut jouer (M, 2 jours) — fait `a0e0156`

- [x] 0.1 Quand un joueur rejoint une manche déjà perdue (ou vide), le serveur relance la manche ou fait apparaître le joueur vivant ; le client n'affiche « Manche perdue » qu'après avoir été vivant dans cette manche (S-01) — cause réelle : le serveur relançait la manche dans le même tick que la défaite, sans l'annoncer ; désormais intermission de 8 s (`ROUND_INTERMISSION`), `GameEvent::RoundStart`, arrivant sans survivant = manche fraîche, `Lose` ignoré avant tout instantané de la manche
- [x] 0.2 « Rejouer » en ligne demande une nouvelle manche au serveur au lieu de déconnecter ; la bannière de défaite disparaît à la reconnexion (S-01) — `ClientMsg::RestartRound` (PROTOCOL_VERSION 7 → 8, déploiement VPS + clients à coupler)
- [x] 0.3 Vérification : `pilot net connect wss://ws.loicberthod.ch Bot` puis `pilot state` à 30 s doit donner une manche en cours et une vie non nulle ; à rejouer avec la démo web ouverte — couvert par 4 tests `net_tests` serveur + 4 tests client ; la vérification sur le serveur public reste à faire après le redéploiement du VPS

## Vague 0 bis — Manche jouable en production (M, même jour) — fait `6cd0ab1`

Constaté après le redéploiement du VPS avec la vague 0 : « Manche perdue »
toujours immédiate, connexion qui oscille, serveur à 73 % CPU. Mesuré avec
un serveur local et un client piloté (`pilot net connect` + `pilot state`).

- [x] 0b.1 La manche était décidée « défaite » ~6 s après l'arrivée d'un joueur idle : chasseurs et créatures mordeuses du hameau campent le point d'apparition. Grâce d'apparition de 5 s (`SPAWN_GRACE_S`, réseau et solo, contact + morsures + `damage()` Lua) et dégagement de la zone d'apparition à chaque manche côté serveur (`AppState::clear_spawn_area`, chasseurs repoussés à 12 m) — la zone n'est pas dégagée en solo, les scènes scriptées placent leurs monstres à dessein
- [x] 0b.2 Chaque relance rechargeait toute la scène embarquée (décompression + 482 modèles) : plusieurs secondes de silence sur le VPS, au-delà du délai toléré par les clients (8 s), donc déconnexions en boucle. Prototype de scène en cache (`Scene::embedded_player_cached`, `Clone` sur `Scene`), relance en millisecondes
- [x] 0b.3 CI : stub réseau iOS (`displayed_health`), `has_cmd` Windows, journal de l'éditeur publié par le job Linux même quand le pont ne répond pas (attente portée à 160 s) — cause lue au run suivant : panique de `xkbcommon-dl` au démarrage, la bibliothèque runtime `libxkbcommon-x11-0` manque sur le runner (job tenu par le lot « analyse comparative ») ; le job Windows compile désormais mais 12 tests lib échouent sur cette seule plateforme (autosave, `creature_attack`, morsures), à traiter avec ce même lot
- [ ] 0b.4 Le client considère la connexion perdue quand sa boucle ne tourne plus (fenêtre occultée sous macOS, onglet en arrière-plan) : l'état de la pastille dérive de la fraîcheur des messages. Comportement à revoir (maintenir le réseau hors rendu, ou afficher « en pause » plutôt que « hors ligne ») — hors périmètre de cette journée


## Vague 1 — Un mode joueur qui tient debout (M, 1 semaine) — fait `d8cda9a`

- [x] 1.1 Construire l'overlay joueur indépendamment de `mobile.any()` ; ne dessiner stick et boutons que si un événement tactile a été reçu (ou `cfg` mobile) ; l'aide choisit la section Tactile / Clavier sur le même critère (J-01)
- [x] 1.2 Vie initialisée en solo (`hud_health = Some(1.0)` dès qu'il y a un contrôleur), `health_bar: true` et `attack_button` remis dans la scène livrée, `hud_health` alimenté depuis `net_local_health` en ligne ; même HUD sur web et desktop (J-02) — cause de l'écart web/desktop : `scripting_web.rs` renseignait la vie du HUD sans condition ; parité rétablie
- [x] 1.3 Paramètres joueur réduits à Audio / Accessibilité / Langue / Clavier / Manette, dans une `ScrollArea`, opaques et exclusifs avec l'aide et le menu pause (J-03)
- [x] 1.4 Accueil joueur : jeu figé et HUD masqué derrière la modale (constaté : vague 1/4, monstres en mouvement, pastille « Hors ligne » visibles avant tout choix) ; boutons ⚙ et ? ; pseudo limité à 32 caractères avec compteur ; salon filtré `[A-Za-z0-9_-]` ; description des classes ; serveur modifiable dans une section repliable ; classe, salon et dernier choix mémorisés ; fenêtre 🌐 retirée du mode joueur (`src/editor/windows.rs:2571-2634`, `1131-1190`)
- [x] 1.5 Menu pause : « Menu principal » qui repose `welcome_pending` ; « Rejouer » renommé « Recommencer la partie » avec confirmation ; contrôles tactiles masqués pendant la pause (`src/editor/hud.rs:1108-1132`, `src/editor/mod.rs:1279-1283`)
- [x] 1.6 Tab maintenu = classement (constaté : Tab ne fait rien, egui consomme la touche) ; Paramètres uniquement via la pause (`src/lib.rs:679`) — Tab lue au niveau winit avant egui (qui la consommait pour la navigation de focus)
- [x] 1.7 Persistance web : `Settings::load/save` et `crash_log` sur `localStorage` derrière un adaptateur ; pseudo par défaut unique (« Invité » + 4 chiffres) (W-01) — `assets::persisted_read/write/remove` : fichier en natif, `localStorage` sur wasm ; pseudo invité à 4 chiffres réellement variés (l'ancien tirage ne donnait que des multiples de 1000)
- [x] 1.8 Raccourcis d'édition (Suppr, Cmd+D/C/V/X/A/Z) inactifs en mode joueur ; Cmd+S et 💾 inactifs en Play dans l'éditeur (E-03)
- [x] 1.9 Spectateur : message et caméra quand plus aucun allié n'est vivant ; bouton tactile « Saut » cycle les alliés ; texte composé depuis la touche réelle (`src/app/mod.rs:1694`, `src/lib.rs:664`)

## Vague 2 — Un réseau honnête (L, 2 semaines, redéploiement VPS) — fait `27efb65`

- [x] 2.1 Connexion asynchrone avec timeout 5 s (constaté par code : `ready_rx.recv()` bloque le thread de rendu, `src/net/client/native.rs:154`) — `Handshake { Pending, Open, Failed }` partagé natif/web, `CONNECT_TIMEOUT` 5 s
- [x] 2.2 Pastille dérivée de `net_connection_state()` et non de `is_connected()` + test de sous-chaîne « … » (constaté : « Connexion… » puis vert avant tout `Welcome`) (`src/editor/mod.rs:1120-1126`)
- [x] 2.3 Serveur plein → `JoinRejected { "serveur plein" }` avant fermeture, sans relance de reconnexion (`src/net/server_loop.rs:71-78`)
- [x] 2.4 Tout refus ou abandon de reconnexion ramène à l'accueil avec l'erreur affichée (`src/app/network_client.rs:900-905`)
- [x] 2.5 Bannière réseau multi-ligne limitée à 90 % de la largeur (`src/editor/hud.rs:1260`) ; statut « Connecté — N joueurs » au lieu de « joueur 22 »
- [x] 2.6 Ping/Pong dans le protocole (+1 de version), latence dans la pastille ; déploiement `scripts/deploy_vps.sh` — `Ping`/`Pong` ajoutés en fin d'énumération sans nouveau bump (la version 8 de la vague 0 n'est pas encore déployée) ; le redéploiement VPS reste à faire

## Vague 3 — Éditeur : premier lancement et sécurité du travail (M, 1 semaine) — fait `474ce61`

- [x] 3.1 Écran d'accueil éditeur sans projet : Premier jeu / Nouveau projet / Récents / Découvrir le hameau ; « Premier jeu » remonté au premier niveau du menu Fichier (E-01)
- [x] 3.2 Sans projet ouvert, Cmd+S ouvre « Enregistrer sous… » ; « Enregistrer sous… » devient la cible des Cmd+S suivants ; nom proposé = nom réel de la scène (`src/gfx/renderer/frame.rs:906`, `src/editor/menus.rs:756`)
- [x] 3.3 Modale d'autosave : « Ignorer » mémorisé pour ce fichier d'autosave ; chemin brut retiré ; en cas d'échec de restauration, proposer l'autosave précédente (E-02, `src/gfx/renderer/frame.rs:945`)
- [x] 3.4 Play/Stop conserve le contexte : caméra d'édition restaurée au Stop, sélection conservée, drapeau « modifié » remis à sa valeur d'avant Play, `playing = false` avant tout changement de scène (constaté : caméra et sélection perdues ; `src/app/simulation.rs:880-928`, `frame.rs:1247`)
- [x] 3.5 Undo complet : `SceneSnapshot` avec `light`, `sky`, `hud_layout`, `hud_widgets`, `imported` ; import glTF annulable et marqué modifié ; commandes console dans l'historique ; pas d'entrée d'undo sur un clic sans déplacement (constaté : « • » après un simple clic sur le sol) (`src/app/mod.rs:55-82`, `src/app/persistence.rs:465`, `src/app/picking.rs:63`)
- [x] 3.6 Clic dans la hiérarchie = sélection seule, cadrage au double-clic ou F (`src/editor/hierarchy.rs:188`) ; clic droit dans la vue 3D sélectionne l'objet visé avant d'ouvrir le menu (constaté : Cadrer/Dupliquer/Supprimer grisés) ; surbrillance de sélection atténuée pour les grands plans (constaté : tout le viewport jaune quand « Sol » est sélectionné) — atténuation de la surbrillance faite côté app (`highlight_of`, 0,25× au-delà d'une demi-diagonale de 20), pas dans le shader
- [x] 3.7 Confirmations : suppression de groupe (N objets désassignés), journal de crash, bake / limite de lumières ; préset qualité = une seule entrée d'undo ; échec de suppression de prefab remonté (`src/editor/hierarchy.rs:245`, `windows.rs:2247,1641-1652`, `frame.rs:1099`)
- [x] 3.8 Chemins d'erreur : projet récent introuvable affiché grisé avec « Localiser / Retirer » ; échec d'ouverture de projet en modale ; `settings.json` corrompu sauvegardé en `.bak` avec toast (`src/app/settings.rs:300-322,380-389`)

## Vague 4 — Vocabulaire, menus et documentation (S, 3 jours) — fait `b1507a0`

- [x] 4.1 Un seul libellé pour l'export (constaté : Fichier « Build & Export… », Outils « Build Android… », toolbar « Compiler l'APK », panneau « Run ») ; « Experimental » → « expérimental » ; « Diagnostic système » dans un seul menu ; « Guide export APK » pointant vers `docs/` (`src/editor/menus.rs:283,667,207,738-742`, `export.rs:477`)
- [x] 4.2 Chaînes internes retirées de l'UI (« roadmap UX 5.3 », « Sprint 126 ») ; anglicismes de l'inspecteur (Rigidbody, Input Receiver, Collider/Box, Impostors, Bake lighting) ; noms de touches lisibles dans le remap (« J » plutôt que « KeyJ ») avec détection de doublon et de touche réservée (`src/editor/windows.rs:1025,1678,1088-1106`, `src/app/input.rs:310`)
- [x] 4.3 Raccourcis affichés dans les menus Édition et Fichier (constaté : aucun) ; Annuler/Rétablir/Coller grisés quand vides ; Cmd+P Play/Stop, Cmd+Q Quitter, F1 ouvre les raccourcis dans l'éditeur ; Cmd+… laissé passer quand un champ texte a le focus (`src/editor/menus.rs:307-348`, `src/lib.rs:480-525,670`)
- [x] 4.4 Chaînes joueur localisées (Inventaire, Sac, Joueurs, Paramètres, Journal de crash) ; légende de la carte sans « M pour fermer » au tactile ; unités sur 💀 🏆 🤝 (`src/editor/hud.rs:348-503`, `src/app/locale.rs:339-347`)
- [x] 4.5 Docs : Test A de `TEST_SCENARIO.md` réalisable sans clone (Premier jeu embarqué, étape 10 « Test B ») ; tag de release après la vague 5 (la seule release est `v0.1.0-alpha.3` du 19 juillet, antérieure à toutes les vagues) ; KNOWN_LIMITATIONS à jour (undo inspecteur, écran allumé, spectateur, date) ; section README « Créer son premier jeu » sans le stub `guide-createur` ; référence API Lua complète (emit, on_event, find_tag, save, debug.line, reverb, raycast, overlap_sphere) — le tag de release n'est pas créé (à faire au moment du playtest)

## Vague 5 — Tactile réel (L, à tester sur appareil) — fait `e007e7d` (non vérifié sur appareil)

- [x] 5.1 Multi-touch : stick + orbite, stick + Feu simultanés (`src/lib.rs:586` désactive `handle_touch` en mode joueur ; egui ne suit qu'un pointeur) — rôles par doigt (`src/app/touch.rs`, 10 tests) ; egui ne voit toujours qu'un doigt pour ses fenêtres
- [x] 5.2 Stick flottant, zone morte 12 %, rayon relatif à l'écran, suivi du doigt hors du cercle (`src/editor/hud.rs:1422-1452`)
- [x] 5.3 Boutons ⏸ / Carte / ? à 44 pt minimum ; carte plein écran fermable au tactile (`Order::Foreground` au-dessus des boutons) ; pincer pour zoomer (`hud.rs:1170-1183`, `windows.rs:660-683`)
- [x] 5.4 Safe area : insets système réels et appliqués à tout le HUD, pas seulement au stick (`hud.rs:1349-1354,1166,1221`) ; orientation et barre d'état déclarées dans l'`Info.plist` iOS (`packaging/build_ios.sh`) — insets iOS/Android/web lus mais non vérifiés sur appareil
- [x] 5.5 Volume par défaut 0,6, touche et bouton muet ; police avec les émojis utilisés ou remplacement par du texte (`src/app/settings.rs:222`, `hud.rs:1175`) — glyphes non couverts remplacés (🧟→👹, 🟢→💚, 🩹→📝, 🤝→🛡), test de couverture de la fonte
- [x] 5.6 Souris en mode joueur : sensibilité appliquée à `touch_look`, glissé plein écran, curseur capturé pendant le jeu (`src/app/picking.rs:299`, `src/app/simulation.rs:1213`) — pointer lock web et capture du curseur non vérifiés sur appareil
- [x] 5.7 Doc de `dual_stick` réécrite (elle décrit encore le stick bridé à un axe) (`src/scene/mobile.rs:21-29`)

## Vague 6 — Confort éditeur (M, 1 semaine) — fait `21cfa21`

- [x] 6.1 Sélecteurs de fichiers asynchrones (`rfd::AsyncFileDialog`) ; ouverture et duplication de projet hors du thread UI avec état « Ouverture de X… » (`src/editor/menus.rs:241-776`, `src/app/persistence.rs:304-413`)
- [x] 6.2 Import glTF : « Import de X… » dans la barre d'état puis toast « X importé » ; autosave visible (« Sauvegarde auto à 14:32 ») ; « Copier le diagnostic » confirmé par toast ; sortie console non toastée (`src/app/persistence.rs:422-524`, `src/app/autosave.rs:56`, `src/editor/toasts.rs:99`) — l'autosave s'affiche en âge relatif (« 💾 auto il y a 2 min »), pas d'horloge locale dans le crate
- [x] 6.3 Export annulable (`child.kill()`) ; contrôle qualité APK cliquable vers l'objet fautif (`src/editor/export.rs:1059`, `readiness.rs:47`)
- [x] 6.4 Persistance des vues : grille, aimantation, outil, largeur des panneaux, position et taille de toutes les fenêtres (`src/app/mod.rs:1004`, `src/editor/mod.rs:971-1012`) — disposition egui complète persistée en RON (`editor_layout.ron`, sauvegarde toutes les 30 s et à la fermeture)
- [x] 6.5 États vides utiles : inspecteur (« Clique un objet ou Ajouter ▸ Cube »), filtre sans résultat, console (« tape help ») ; Échap annule un renommage, nom vide refusé (`src/editor/mod.rs:3061`, `hierarchy.rs:132,220-236`, `windows.rs:55`)
- [x] 6.6 Coquille web : `fail()` réaffiche le voile (`boot.style.display`), voile masqué sur la première frame du moteur plutôt qu'un délai, bouton ⛶ déplacé hors du « ? » du HUD, `contextmenu` neutralisé, timeout de téléchargement avec « Réessayer », `orientationchange` écouté (`packaging/web/index.html:40,148,186-196`) — non vérifié dans un navigateur avant le déploiement Pages suivant

## Vague 7 — Playtest (M, 3 jours + testeurs) — inchangée

- [ ] 7.1 Après les vagues 0 et 1 : 3 à 5 testeurs sur `docs/TEST_SCENARIO.md` corrigé, dont un non-développeur, avec le `.dmg` d'une release taguée
- [ ] 7.2 Résultats dans `docs/playtests/2026-09-XX.md` ; reprioriser les vagues 5 et 6 ; trancher la réapparition (GDD)

## À conserver tel quel

Toasts et compteur d'erreurs cliquable ; déduplication « ×N » des erreurs Lua ;
triptyque Enregistrer / Continuer sans enregistrer / Annuler et point unique
`SceneSwitch` ; confirmation de suppression de prefab ; barre d'état
« projet · fichier · • » ; inspecteur teinté en Play ; table unique des
raccourcis avec test de cohérence de `docs/CONTROLS.md` ; panneau Build &
Export ; menu pause en ligne « la partie continue sans vous » avec Se
déconnecter ; bannières réseau et reconnexion avec backoff ; diagnostic de mort
« Rattrapé par… » ; palette daltonienne doublée d'un pourcentage ; marqueur
d'allié hors écran ; détection WebGPU et barre de téléchargement honnête ;
écran maintenu allumé sur les trois plateformes ; « Premier jeu » embarqué et
stub de redirection daté pour l'ancien guide.
