# Sprint 19 juillet (matin) — Pré-test développeur externe : 5 phases

> Objectif : rendre le premier test externe **interprétable** (savoir si un
> blocage vient du moteur, de la doc, de l'installation, de l'ergonomie ou
> d'une fonction non prévue) — pas rendre RusteeGear complet.
>
> Ce document remplace la première version du 19/07 au matin : analyse critique
> des 30 recommandations du texte source (pertinence jugée **sur l'état réel du
> code**, vérifié ce matin), fusion des redondances, puis **5 phases retenues**
> avec sous-sprints. Tout le reste est explicitement reporté (§4).
>
> **État au 19/07/2026** — chaque tableau de phase porte une colonne
> « État & explication » : **A ✅ réalisée** (2 restes manuels : vrai compte
> macOS neuf, issue GitHub du test flaky — détail au journal §3 bis) ;
> **B ✅ réalisée** (first_game + QUICKSTART + FIRST_GAME + MENTAL_MODEL +
> index README, 4 tests-preuves + preview rendue — journal §3 ter ; reste le
> déroulé humain sur machine vierge) ; **C ✅ réalisée** (audits Play/Stop et
> undo prouvés, broken_scene, top-5 messages améliorés, exports web + .dmg
> vérifiés en vrai — journal §3 quater ; C2 partielle : piège
> `player_scene.json` différé ; l'alerte de fermeture, d'abord listée en
> limitation, a depuis été implémentée — cf. trouvaille C2 au journal) ;
> **D ✅ réalisée** (KNOWN_LIMITATIONS + matrice de support, LUA_PORTABLE +
> 4 scripts officiels testés sur les 2 backends, À propos « Developer
> Preview 1 — commit », marquage Experimental/non re-vérifié — journal
> §3 quinquies) ; **E ✅ réalisée** (scénario de test 10 étapes, formulaire,
> « Copier le diagnostic » avec anonymisation, commit injecté aux builds —
> journal §3 sexies). **Seul reste : la pose du tag `v0.1.0-alpha.1` (E5)**,
> checklist prête, bloquée tant que la scène embarquée n'est pas
> resynchronisée (« Errant 62 », session créatures) et que des chantiers
> écrivent encore dans l'arbre. La tâche « alerte de fermeture » est terminée
> (limitation corrigée) ; « migration demo_*.json » encore en cours.

## Sommaire

1. [Analyse critique des 30 recommandations](#analyse)
2. [Synthèse : ce qu'on garde, fusionne, reporte, rejette](#synthese)
3. [Les 5 phases (sous-sprints détaillés)](#phases)
   — avec journal d'exécution de la Phase A (§3 bis, preuves détaillées)
4. [Reporté à plus tard](#reporte)
5. [Faits code sur lesquels s'appuie ce plan](#faits)

---

<a id="analyse"></a>
## 1. Analyse critique des 30 recommandations

Notation pertinence : ★★★ indispensable au pré-test · ★★ utile · ★ marginal ou
prématuré. « Coût » = effort estimé pour CE dépôt, pas dans l'absolu.

| # | Recommandation | Pertinence | Coût | Jugement précis (état réel vérifié) |
| --- | --- | :-: | :-: | --- |
| 4 | Installation machine vierge | ★★★ | Moyen | **La plus critique.** `src/assets.rs` dépend de `~/.motor3derust/assets/` ; jamais testé hors de ton Mac. Tant que ce n'est pas fait, tout retour externe mesure ton environnement. |
| 2 | Projet exemple `first_game` | ★★★ | Moyen | Confirmé : `examples/` ne contient que des binaires de dev (`gen_*`, `smoke_vps`…). Aucun matériel pédagogique. Seule scène : le MMORPG embarqué — exactement l'anti-pattern décrit. |
| 3 | Parcours « zéro décision » | ★★★ | Faible | Le profil `dev-fast` **existe déjà** (Cargo.toml:52, commenté pour ça). Il manque juste le QUICKSTART qui l'exploite. Coût quasi nul, impact énorme. |
| 1 | Parcours de test précis 45-90 min | ★★★ | Faible | Juste. Fusionne naturellement avec 22 (chronos) et 24 (formulaire) : c'est un seul livrable « protocole de test », pas trois. |
| 26 | Version figée (tag alpha) | ★★★ | Faible | Indispensable vu le rythme de commits (3 sessions concurrentes possibles sur ce dépôt). Quasi gratuit. |
| 27 | CI verte avant test | ★★★ | Faible* | fmt+clippy stricte déjà en place. *Mais* : test roguelike flaky ~60-80 % sur HEAD (préexistant, documenté). Le corriger = coûteux ; l'`#[ignore]` + issue = 30 min. On prend la 2e option. |
| 17 | Sauvegarde / migration | ★★★ | Moyen | Renforcé par un piège **déjà documenté** du projet : l'export peut écraser `player_scene.json` (perte Feu/Arme/Soin). Ce n'est pas théorique ici, c'est arrivé. |
| 6 | Démarrage sans bruit | ★★★ | Moyen | Renforcé par un bug documenté : auto-connexion réseau en player build avec fallback silencieux qui fige les créatures. Un testeur conclura « moteur cassé ». |
| 16 | Scène volontairement cassée | ★★ | Faible | Très bon rapport coût/valeur : 1 dossier de données + 1 session de test. Révèle les panics avant que le testeur ne les trouve. |
| 15 | Messages d'erreur utiles | ★★ | Moyen | Juste, mais à **borner** : uniquement les 5 erreurs du parcours de test, pas les 11 listées. Le reste après le premier retour réel. |
| 19 | Audit Play/Pause/Stop | ★★ | Faible | C'est un audit, pas un dev. La règle « sélection désactivée en Play » est une décision produit déjà prise → l'écrire noir sur blanc. |
| 18 | Undo/redo essentiels | ★★ | Faible* | **Déjà implémenté** (pile de `SceneSnapshot`, `push_undo()` dans src/app/mod.rs). Le travail restant est un *audit de couverture* sur les actions du tutoriel, pas une construction. Le texte surestime le coût. |
| 30 | Page limitations connues | ★★ | Faible | Peu coûteux, gros gain de confiance, et c'est la soupape qui permet de reporter 7, 20, 28 honnêtement. |
| 13 | Matrice de support | ★★ | Faible | Une demi-journée. Évite les faux bugs (« pas de bouton import sur le web »). |
| 14 | Sous-ensemble Lua portable | ★★ | Faible* | Le texte croit qu'il faut le *définir* ; en réalité les **tests différentiels rilua/mlua existent déjà** (Cargo.toml, sprint 137 : même script, même résultat natif/web). Il reste à *documenter* ce que les tests garantissent. Coût divisé par 5. |
| 23 | Deux méthodes d'installation | ★★ | Faible | `build_dmg.sh` existe et fonctionne. Séparer test A (.dmg, ergonomie) / test B (source, contributeur) est la bonne idée du texte. Piège connu : `.app` périmés sur volumes montés → dater le build. |
| 22 | Chronométrer les étapes | ★★ | Faible | Oui, mais ce n'est pas une tâche autonome : c'est une colonne du protocole (fusion avec 1). |
| 24 | Formulaire de retour | ★★ | Faible | Oui — fusion avec 1 et 22 en un seul livrable « protocole ». |
| 5 | Script doctor | ★★ | Faible | Pertinent, et le dépôt a déjà la moitié de la logique : le module `readiness.rs` (« APK Readiness Check ») fait exactement ce genre de diagnostic pour l'export. Un `doctor.sh` minimal suffit (rustup, version, écriture, port). |
| 10 | Page modèle mental | ★★ | Faible | `docs/architecture.md` existe mais technique. Une page d'un écran suffit. |
| 25 | Bouton signaler un bug | ★ | Moyen | L'issue GitHub préremplie est du confort. La version qui compte : « Copier le diagnostic » (version/commit/OS/GPU/logs) — le crash-log existe déjà (`src/crash_log.rs`), on s'y adosse. |
| 12 | Masquer motor3derust | ★ | Faible | Visible surtout dans : titre de fenêtre, doc publiée `/doc/motor3derust/`, `~/.motor3derust/`. On masque le titre + logs (1 h) ; le crate et le dossier utilisateur : reportés, listés en limitation. La migration automatique proposée par le texte est **prématurée** (risque > gain avant tout test). |
| 11 | Terminologie stable | ★ | Élevé | Juste sur le fond, mais un chantier transversal (UI + docs + logs + code). Pour le pré-test : appliquer le glossaire aux **nouveaux** documents uniquement. |
| 21 | Export qui marche | ★★ | Faible* | `build_web.sh` + `packaging/EXPORT.md` + démo web **déjà déployée** sur GitHub Pages. Le texte suppose qu'il faut le construire ; il faut le **vérifier depuis un clone propre** et documenter la commande. |
| 9 | Tutoriel 10 minutes | ★★★ | Faible | Dépend de 2 (first_game). Doc pure, pas d'UI. Le script `obj.ry = obj.ry + 45 * dt` correspond à l'API réelle. |
| 29 | Trois niveaux de doc | ★★ | Faible | Vrai problème : `docs/` mélange 25+ fichiers d'audits/sprints/réflexion. Mais la solution pré-test n'est pas de tout restructurer : c'est QUICKSTART + FIRST_GAME à la racine + un index. Fusion avec 3 et 9. |
| 8 | Écran d'accueil | ★ | Élevé | UI nouvelle + notion de « projets récents » qui suppose un système de projet (7). Pour un test **guidé par un document écrit**, le QUICKSTART remplit ce rôle à coût nul. Reporté. |
| 7 | Système de projet `project.rgear` | ★ | Très élevé | Structurellement juste, mais c'est un chantier moteur (tout vit dans `~/.motor3derust/` + scène embarquée). Le faire *avant* un premier retour externe serait de l'ingénierie à l'aveugle. Reporté, listé en limitation n°1. |
| 20 | Projet hors du dépôt | ★ | Très élevé | Dépend entièrement de 7. Reporté avec lui. |
| 28 | Interface réduite / mode débutant | ★ | Élevé | Prématuré : on ne sait pas encore *ce qui* perd les gens — c'est justement l'objet du test. Marquer 3-4 items « Experimental » dans les menus suffit (1 h). Le mode Beginner/Advanced : après le premier retour. |

### Redondances du texte source

Le texte compte 30 points mais en réalité **~19 actions distinctes** :
- **1 + 22 + 24** = un seul livrable (protocole de test : scénario + chronos + formulaire).
- **3 + 9 + 29** = un seul chantier doc (QUICKSTART → FIRST_GAME → index).
- **6 + 15** se recouvrent (le bruit au démarrage EST une famille de messages d'erreur à traiter).
- **5 + 27** se recouvrent partiellement (doctor vérifie ce que la CI garantit côté dépôt).
- **12 + 11** : la cohérence de nom est un sous-cas de la terminologie.
- **8** est rendu inutile à court terme par **3** (un document guidé remplace l'écran d'accueil pour un test guidé).

---

<a id="synthese"></a>
## 2. Synthèse

| Catégorie | Recommandations |
| --- | --- |
| **Retenues dans les 5 phases** | 1, 2, 3, 4, 5, 6, 9, 13, 14 (doc seule), 15 (top-5), 16, 17, 18 (audit), 19, 21 (vérif), 22, 23, 24, 25 (version min), 26, 27 (avec `#[ignore]`), 29, 30, 12 (visible seulement) |
| **Reportées** (§4) | 7, 8, 11 (glossaire complet), 20, 28, 12 (crate + `~/.motor3derust/`), 25 (issue préremplie), 15 (au-delà du top-5) |
| **Rejetées en l'état** | Migration automatique `~/.motor3derust` → `~/.rusteegear` (risque de perte de données avant tout test, gain nul pour un testeur qui n'a aucun des deux dossiers) |

---

<a id="phases"></a>
## 3. Les 5 phases

Ordre = ordre d'exécution. Chaque phase a un critère de sortie binaire.
Convention projet : chaque sous-sprint livre une **preuve** (test, capture, journal).

---

### Phase A — Machine vierge (le socle ; recos 4, 5, 6, 27) — ✅ réalisée le 19/07 (2 restes manuels)

*Tant que A n'est pas verte, les phases B-E mesurent du sable.*

| Sous-sprint | Contenu | Preuve | État & explication |
| --- | --- | --- | --- |
| A1 | **Lancement sans `~/.motor3derust/`** : `mv ~/.motor3derust ~/.motor3derust.bak` puis `cargo run` — l'éditeur doit démarrer, créer le dossier proprement, afficher la scène embarquée. Test automatisé via l'override thread-local d'`assets_dir()`. | Test `cargo test` + lancement manuel journalisé | ✅ **Fait.** Lancement manuel réussi (ne recrée que `assets/`, dossier restauré ensuite) ; test permanent `first_launch_without_any_user_folder_still_opens_the_embedded_scene` ajouté dans `src/lib.rs` — dossier **inexistant** simulé, scène embarquée chargée, rien d'écrit sur disque. |
| A2 | **Chasse aux chemins personnels** : `grep -rn "Users/berthod\|/Volumes/" src/ assets/ packaging/` ; corriger tout chemin requis à l'exécution. | Grep vide (hors commentaires) | ✅ **Fait — aucun correctif nécessaire.** Code runtime (`src/`, `tests/`) propre. Occurrences restantes : scripts Blender de génération (outillage jamais exécuté par le moteur) et artefacts Xcode gitignorés. |
| A3 | **Compte vierge de bout en bout** : nouveau compte macOS → clone → `cargo build` → `cargo test` → `cargo run --profile dev-fast`. Chronométrer chaque étape (alimente E1). | Journal + chronos | 🟡 **Approché.** Clone propre (fichiers trackés seuls) : build `dev-fast` en **5 min 01** (Apple M4) ; binaire lancé avec `$HOME` entièrement vierge → démarre, crée proprement `.motor3derust/assets/`. Aucun fichier non versionné requis. **Reste manuel** : un vrai compte macOS neuf (création de compte non automatisable ici). |
| A4 | **Auto-connexion réseau maîtrisée** : opt-out hors-ligne + connexion annoncée en clair ; jamais le fallback silencieux qui fige les créatures (bug documenté). | Test player sans réseau, créatures animées | ✅ **Fait.** Le filet anti-gel (heartbeat 2,5 s, fix 16/07) existait ; ajouté `RUSTEEGEAR_OFFLINE=1` (desktop) + log explicite de la connexion par défaut ; échec déjà en `warn` + `net_status`. Web inchangé (démo partagée = promesse produit). Test `offline_is_requested_…` + lancements vérifiés dans les deux modes. |
| A5 | **Logs de démarrage propres** : par défaut quelques lignes (version, GPU/backend, prêt) ; le reste derrière `RUST_LOG=debug`. | Capture console avant/après | ✅ **Fait.** Démarrage : `RusteeGear 0.1.0` → `GPU : Apple M4 (Metal)` → manette. Warning egui sRGB rétrogradé via filtre par défaut (`info,egui_wgpu=error`), `RUST_LOG` reprend la main. Format de framebuffer intact (risque goldens). |
| A6 | **CI verte du testable** : `#[ignore]` + issue sur le test roguelike flaky ; `fmt`, `clippy`, `test`, build verts. | CI verte | 🟡 **Fait sauf issue GitHub.** Flaky `roguelike_demo_clears_rooms…` en `#[ignore]` justifié. `fmt --check` ✓, `clippy --all-targets` ✓, suite : **604 verts, 9 ignorés, 1 échec** — `the_embedded_scene_ambient_decor_matches_the_demo`, imputable aux modifs **non commitées d'une session concurrente** (`src/scene/demos.rs`, créatures 62+ sans resynchro de `player_scene.json` — resynchro volontairement non lancée ici, piège d'écrasement documenté). **Restes** : issue GitHub du flaky (action publique, avec accord), échec « Errant 62 » à résoudre par la session créatures. |
| A7 | **`scripts/doctor.sh`** : environnement uniquement (doctor = environnement, `readiness.rs` = contenu). Sortie ✓/✗ + réparation par échec. | Exécution sur le compte vierge de A3 | ✅ **Fait.** 8 vérifications (rustup, cargo, rustc ≥ 1.85/édition 2024, cible native, dépôt + `assets/bundle/`, `$HOME` écrivable, port 7777 optionnel) — 8/8 ✓ sur cette machine ; à rejouer sur le compte vierge du reste A3. |

**Sortie de phase** : sur un compte vierge, `doctor.sh` ✓ puis clone → build → run sans erreur ni warning visible. **État : atteinte sur environnement simulé ; à confirmer sur un vrai compte neuf.**

---

### Phase B — Matériel de démarrage (recos 2, 3, 9, 29 fusionnées) — ✅ réalisée le 19/07 (1 reste manuel)

| Sous-sprint | Contenu | Preuve | État & explication |
| --- | --- | --- | --- |
| B1 | **`examples/first_game/`** : sol, personnage, 3 cubes, lumière, objet animé (`rotating_object.lua`), zone de collision, mini-objectif. Dossier de **données** (scene.json + assets + scripts + README). Assets conformes à la charte graphique maison. | Ouverture < 30 s, scène lisible d'un coup d'œil | ✅ **Fait.** `scene.json` (10 objets, primitives uniquement — zéro asset externe, s'ouvre depuis n'importe quel clone), `scripts/*.lua` (copies lisibles des scripts inline), README avec `preview.png` (rendu headless réel via `examples/gen_first_game_preview.rs`). Cargo ignore le dossier (aucun `.rs` dedans ; `cargo build --examples` vérifié). **4 tests-preuves** dans `tests/first_game_example.rs` : chargement + conformité README, **simulation réelle** (script Lua tourne, pièce ramassée par `advance_play`), round-trip save/load, synchro inline ↔ `.lua`. |
| B2 | **`QUICKSTART.md`** (racine) : zéro décision — clone, `rustup update`, `doctor.sh`, `cargo run --profile dev-fast`, 3 clics vers first_game, Play. Avertir : 1re compilation longue vs suivantes. | Suivi à la lettre, chrono ≤ cible | ✅ **Fait.** Chiffres réels de A3 intégrés (~5 min 1re compilation sur M4, secondes ensuite), bannière de démarrage attendue décrite, dépannage express (dont `RUSTEEGEAR_OFFLINE=1`). **Reste manuel** : le dérouler sur le compte vierge (même reste que A3). |
| B3 | **`docs/FIRST_GAME.md`** — tutoriel 10 min : ouvrir l'exemple → ajouter un cube → gizmo → matériau → `obj.ry = obj.ry + 45 * dt` → Play → sauvegarder → (option) export web. Réussite visible < 5 min. | Parcours chronométré par soi-même | ✅ **Fait.** Les libellés UI sont ceux du code réel, vérifiés (menu **Ajouter → 🧊 Cube**, gizmo **W/E/R**, Inspecteur → **Matériau** / **Script (Lua)**, **💾 Enregistrer sous…**). La règle Play/Stop (restauration) est énoncée dès maintenant ; l'audit C1 la confirmera cas par cas. L'export est marqué « en re-vérification (Phase C) » — pas de promesse non tenue. Étapes 6-9 prouvées par les tests B1 (Play simulé, save/load). |
| B4 | **`docs/MENTAL_MODEL.md`** (1 écran) + mini-glossaire (8 termes officiels) appliqué **aux nouveaux docs uniquement**. | Relecture par un non-initié | ✅ **Fait.** Données (Scene → Scene Objects → composants), 4 rôles (Editor/Runtime/Renderer/Server), règle Play/Stop, glossaire avec la colonne « à ne pas confondre avec » (dont RusteeGear vs `motor3derust`). **Reste** : la relecture par un non-initié (= le testeur externe, de fait). |
| B5 | **Index de doc** : README pointe vers QUICKSTART (5 min) → FIRST_GAME (10 min) → MENTAL_MODEL. Les audits et sprints restent dans `docs/` mais ne sont plus le chemin d'entrée. | Diff README | ✅ **Fait.** Bloc « 🚀 Nouveau ici ? » inséré en tête de README, avant le renvoi historique vers ROADMAP/GDD, avec la phrase explicite « les audits et sprints sont l'historique, pas le chemin d'entrée ». |

**Sortie de phase** : un inconnu muni du seul QUICKSTART atteint « cube qui tourne » sans poser de question. **État : atteignable sur pièces (tout le parcours est écrit et prouvé par tests) ; la validation finale est le déroulé humain sur machine vierge.**

**Découverte de phase** : les scènes historiques `assets/examples/demo_*.json` utilisent l'ancien champ `input_receiver`, disparu du moteur — leur joueur n'est **plus pilotable** au format actuel. `first_game` utilise `controller.input` et un test l'empêche de vieillir pareil ; les deux `demo_*.json` sont à migrer ou retirer (hors phase).

---

### Phase C — Boucle essentielle fiabilisée (recos 16, 17, 18-audit, 19, 21-vérif) — ✅ réalisée le 19/07 (C2 partielle, dépend de la session concurrente)

*On ne fiabilise QUE ce que le parcours de test exerce.*

| Sous-sprint | Contenu | Preuve | État & explication |
| --- | --- | --- | --- |
| C1 | **Audit Play/Pause/Stop** : Play→Stop ; Play→Pause→Play→Stop ; modif pendant Play→Stop (restauré ?) ; suppression pendant Play ; erreur Lua pendant Play. Documenter persiste/restauré/verrouillé. Corriger uniquement les crashs et incohérences silencieuses. | Tableau de résultats dans le doc | ✅ **Fait, en tests headless** (le binaire GUI n'est pas pilotable par computer-use — piège documenté) : 7 tests dans `tests/play_mode_audit.rs`, tous verts **au premier passage** — le contrat tient. Résultats au journal §3 quater. Aucune incohérence trouvée, rien à corriger. |
| C2 | **Sauvegarde/réouverture** : modifier→sauver→fermer→rouvrir→identique ; fermeture sans sauvegarder→alerte ; JSON invalide→erreur propre sans crash. **Re-vérifier le piège d'écrasement de `player_scene.json` à l'export**. | Scénarios joués + diff de scène | 🟡 **Partielle.** Round-trip save/load prouvé (test B) ; JSON invalide → `Err` propre prouvé (test C1). **Trouvaille** : `CloseRequested => exit()` direct (src/lib.rs) — **aucune alerte de travail non sauvegardé** → limitation D3 + tâche séparée proposée (dirty-tracking = chantier). Le piège `player_scene.json` reste **différé** : la session concurrente occupe la scène embarquée (échec « Errant 62 », cf. A6). |
| C3 | **Audit undo/redo du parcours** : couverture de la pile `SceneSnapshot` sur : création, suppression, translate/rotate/scale, matériau, duplication, import. Trous → KNOWN_LIMITATIONS. | Tableau couvert/non couvert | ✅ **Fait (audit de code, 40+ sites `push_undo` recensés).** **Couvert** : création, suppression, duplication/coller, groupes, réordonner, align/reset, manipulations **gizmo** (1 snapshot par manipulation, lumières incluses), prefabs, IA, caméra de jeu, outils d'assets. **Non couvert** : **édition de champs dans l'Inspecteur** (couleur/matériau, script tapé, nom, physique…) et **import glTF** (`finish_import` sans `push_undo`). Pas de trou « dangereux » (la suppression est annulable) → les deux trous vont dans KNOWN_LIMITATIONS (D3). |
| C4 | **`examples/broken_scene/`** : asset manquant, script Lua en erreur, référence invalide. Le moteur ne panique pas, nomme l'objet fautif, laisse éditer. | Ouverture sans crash + messages | ✅ **Fait.** Scène de 7 objets (README avec tableau panne → comportement attendu, + un « témoin sain » qui tourne pour prouver que la simulation continue). Sûreté de l'index hors bornes vérifiée dans le code (`.get` au rendu et en physique). Test `tests/broken_scene_example.rs` : charge, joue 4 frames sans panique, témoin tourne, chaque erreur **nomme** l'objet/le chemin. |
| C5 | **Top-5 messages d'erreur** du parcours au format quoi/pourquoi/réparer. | Avant/après pour les 5 | ✅ **Fait (4 améliorés, 1 déjà bon).** (1) Erreur Lua : chunk nommé d'après l'**objet** (`script de « X »:1: …`) au lieu du call-site Rust illisible ; (2) asset manquant : + conséquence et réparation (« Réimportez… ») ; (3) scène JSON invalide : + chemin + piste + « la scène actuelle est conservée » ; (4) GLB invalide : + chemin + piste + « la scène n'a pas été modifiée » ; (5) port occupé : le message serveur existant était déjà complet (adresse + cause + conséquence), et `doctor.sh` prévient en amont. Preuves : assertions dédiées dans `broken_scene_example.rs`. |
| C6 | **Vérifier l'export** : `build_web.sh` → servi par `python3 -m http.server` et ouvert dans un navigateur ; `build_dmg.sh` → .dmg daté du jour. Aucune autre cible annoncée. | Build web ouvert + .dmg vérifié | ✅ **Fait.** `PLAYER_BUILD=1 build_web.sh` → `target/export/RusteeGear-web.zip` (15 Mo), servi et **joué dans le navigateur intégré** : WebGPU actif, hameau + HUD complets, connexion au serveur multijoueur réel (« bienvenue, joueur 43 »). `build_dmg.sh` → .dmg 19 Mo daté du jour. **Trouvaille croisée** : le 1er build (avant le correctif canvas que la session concurrente venait de poser dans lib.rs) affichait un canvas 0×0 — le rebuild avec son fix fonctionne ; séquelle restante : le canvas garde sa taille d'adoption (pas de suivi du resize), chantier de l'autre session. |

**Sortie de phase** : le parcours de test complet (E1) passe 3 fois de suite sans crash ni comportement inexpliqué. **État : les briques du parcours sont toutes prouvées individuellement ; le triple passage de bout en bout se fera avec E1 (scénario écrit).**

---

### Phase D — Honnêteté produit (recos 13, 14, 30, 12-visible, 28-marquage) — ✅ réalisée le 19/07

| Sous-sprint | Contenu | Preuve | État & explication |
| --- | --- | --- | --- |
| D1 | **Matrice de support** : rendu, import GLB, Lua, multijoueur, export, édition × macOS Editor, Web Player, Android, iOS, Server. Une absence est volontaire. | Tableau publié + limitations | ✅ **Fait.** Matrice en tête de `docs/KNOWN_LIMITATIONS.md`, calibrée sur les vérifications réelles : Web multijoueur « vérifié 19/07 » (C6), Android/iOS « non re-vérifié préversion » (honnête, pas de « Oui » gratuit), Server sans rendu. Liée depuis le bloc « Nouveau ici » du README. |
| D2 | **Sous-ensemble Lua documenté** + 3-4 scripts officiels couverts par un test natif **et** web. | Tests verts sur les 2 backends | ✅ **Fait.** `docs/LUA_PORTABLE.md` (API moteur portable, Lua commun 5.1/5.4, liste « non garanti web »). **4 scripts officiels** publiés dans `examples/scripts/` (`rotate`, `move_between_points`, `trigger_door`, `simple_enemy`) + test différentiel `official_scripts_match_between_backends` (`scripting_web.rs`) : chaque script lu **depuis le fichier publié** et exécuté sur mlua ET rilua, résultats comparés à 1e-5 (avec cas `triggered` vrai/faux et `find_tag`). Helpers différentiels paramétrés ajoutés (time/triggered/tagged). |
| D3 | **`KNOWN_LIMITATIONS.md`** : tout ce qui est volontairement absent, partiel ou non validé. | Page publiée, liée du README | ✅ **Fait.** Alimentée par les résultats réels des phases : trous d'undo (C3), fermeture sans alerte (C2, chantier lancé), canvas web figé (C6), test flaky ignoré (A6), `RUSTEEGEAR_OFFLINE=1` + filet 2,5 s (A4), sélection désactivée en Play (décision produit), nom interne `motor3derust`, **matériaux GLB = `base_color_factor` uniquement** (vérifié dans `scene/import.rs` — plus restrictif que la reco initiale « albedo/metallic/roughness »). |
| D4 | **Nom visible** : titre de fenêtre, logs, « À propos » → RusteeGear + `Developer Preview 1 — <commit>`. | Captures | ✅ **Fait.** Titre de fenêtre déjà « RusteeGear » (vérifié lib.rs:383) ; bannière logs faite en A5 ; « À propos » affiche désormais `Version 0.1.0 — Developer Preview 1` + `Commit : <RUSTEEGEAR_COMMIT>` (via `option_env!`, « build local » sinon — la variable sera injectée au build du tag E5, sans build.rs ni recompilation générale). |
| D5 | **Marquage Experimental** des fonctions hors périmètre. Pas de mode débutant. | Capture menus | ✅ **Fait.** « ✨ Générer une scène (IA)… — Experimental » (menu), « IA — génération de scripts (Experimental) » (fenêtre), et dans le panneau Export : `Android · .apk — non re-vérifié (préversion)` / `iOS · .ipa — non re-vérifié (préversion)` — cohérent avec la matrice D1. WebXR : inexistant dans les menus (rien à marquer). |

**Sortie de phase** : tout ce que le testeur peut rencontrer d'inachevé est soit marqué (menus/panneau Export), soit listé (KNOWN_LIMITATIONS). **Atteinte.**

---

### Phase E — Protocole de test et gel (recos 1+22+24 fusionnées, 23, 25-min, 26) — ✅ réalisée le 19/07 (E2/E5 : gel bloqué par les sessions concurrentes)

| Sous-sprint | Contenu | Preuve | État & explication |
| --- | --- | --- | --- |
| E1 | **Scénario écrit** (10 étapes) avec temps cible par étape, cibles calibrées sur les chronos réels. | Document remis au testeur | ✅ **Fait.** `docs/TEST_SCENARIO.md` : 10 étapes (doctor → compil → first_game → import `assets/models/creature.glb` → collider **Statique** (libellé UI vérifié) → script → Play → Stop → save/rouvrir → export web), temps cibles issus des mesures A3/C6, variantes Test A/B intégrées, consigne « bloqué > 10 min = noter et passer », renvois vers KNOWN_LIMITATIONS et broken_scene (« ne pas la signaler »). |
| E2 | **Deux tests découplés** : Test A « expérience moteur » (.dmg précompilé) ; Test B « expérience contributeur » (source). Un échec de compilation n'invalide pas le test A. | Les 2 artefacts prêts et datés | 🟡 **Prêt sauf artefacts du tag.** La séparation A/B est écrite dans le scénario et le formulaire. Des artefacts de répétition existent (zip web + .dmg du 19/07, C6), mais ceux **remis au testeur** doivent être rebâtis depuis le tag E5 — bloqués avec lui. |
| E3 | **Formulaire structuré** avant / par tâche / après + enregistrement d'écran proposé. | Formulaire prêt | ✅ **Fait.** `docs/TEST_FEEDBACK_FORM.md` : profil avant test, tableau une-ligne-par-étape (réussi, temps réel, blocage, message copié tel quel, compris comment réparer ?, difficulté 1-5), 9 questions après test (promesse perçue, moment le plus perdu, referait seul ?, motif d'abandon, doc utile/manquante…), diagnostic à joindre à chaque blocage. |
| E4 | **« Aide → 📋 Copier le diagnostic »** : version, commit, OS, GPU/backend, format de scène, derniers logs, sans données sensibles. | Collage dans une issue de test | ✅ **Fait.** `log_buffer::diagnostic_report()` (fonction testable) : version + Developer Preview 1, commit (`RUSTEEGEAR_COMMIT`), OS/arch, format de scène v2, 30 dernières lignes de log (bannière + ligne GPU comprises), **dossier personnel anonymisé en `~`**. Bouton dans le menu Aide (`ui.ctx().copy_text`, patron maison). Test `diagnostic_report_names_version_os_and_redacts_the_home_directory` ✓. |
| E5 | **Gel `v0.1.0-alpha.1`** : tag posé après A-D, CI verte dessus, .dmg construit **depuis le tag**, « À propos » affiche le commit. | Tag + release + .dmg | 🟡 **Préparé — pose du tag bloquée.** Fait : `build_dmg.sh`/`build_web.sh` exportent désormais `RUSTEEGEAR_COMMIT=$(git rev-parse --short HEAD)` → « À propos » et le diagnostic des builds de tag afficheront le commit exact (suivi `option_env!`, cargo rebuild au changement). Checklist de gel prête (journal §3 sexies). **Bloquants** : (1) resynchro de la scène embarquée (échec « Errant 62 », session créatures) ; (2) deux tâches de fond écrivent encore dans le dépôt (migration demo_*.json, alerte de fermeture) — on tague sur un arbre stable, pas pendant. |

**Sortie de phase** : on peut envoyer un mail au testeur avec 4 pièces : lien release, QUICKSTART, scénario, formulaire — et rien d'autre. **État : 3 pièces sur 4 prêtes (QUICKSTART, scénario, formulaire) ; la release attend le gel E5.**

---

## §3 bis — Journal d'exécution, Phase A (réalisée le 19/07/2026)

| Sous-sprint | Résultat | Preuve |
| --- | --- | --- |
| A1 | **Fait.** Test manuel : `mv ~/.motor3derust ~/.motor3derust.bak` → l'éditeur démarre proprement, ne recrée que `assets/`, aucune erreur ; dossier d'origine restauré ensuite. Test automatisé ajouté : `first_launch_without_any_user_folder_still_opens_the_embedded_scene` (src/lib.rs) — `assets_dir()` redirigé vers un dossier **inexistant**, la scène embarquée se charge, `list_assets()` ne liste que du `bundle://`, rien n'est écrit sur disque par le chargement. | Test vert + lancement journalisé |
| A2 | **Fait, aucun correctif nécessaire.** `grep "Users/berthod\|/Volumes/"` sur `src/` et `tests/` : zéro occurrence à l'exécution. Occurrences restantes : `scripts/blender/gen_*.py`/`import_*.py` (outillage de génération d'assets, jamais exécuté par le moteur) et `packaging/ios-xcode/build/` (artefacts Xcode, **gitignorés**). | Grep vide sur le code runtime |
| A3 | **Approché** (un vrai compte macOS neuf reste à faire à la main) : (1) dossier utilisateur vierge simulé — cf. A1 ; (2) clone propre `git clone . /tmp/rusteegear_clean` (fichiers trackés uniquement, sans les dizaines de GLB non commités) : **build `dev-fast` en 5 min 01** (Apple M4), binaire lancé avec un `$HOME` entièrement vierge → démarre, crée proprement `.motor3derust/assets/`. Aucun fichier non versionné requis. | Journal + chrono du build propre |
| A4 | **Fait.** Le filet anti-gel (heartbeat 2,5 s, fix du 16/07) existait déjà ; ajouté : `RUSTEEGEAR_OFFLINE=1` (desktop) qui saute l'auto-connexion, connexion annoncée en clair au démarrage (`Connexion au serveur … RUSTEEGEAR_OFFLINE=1 pour jouer hors-ligne`), échec déjà logué en warn + `net_status`. wasm inchangé (le web garde la partie partagée). Test : `offline_is_requested_by_any_value_except_absent_or_zero`. Vérifié en lançant le player dans les deux modes. | Logs des deux lancements + test vert |
| A5 | **Fait.** Démarrage avant : warning egui sRGB + rien d'identifiant. Après : `RusteeGear 0.1.0` → `GPU : Apple M4 (Metal)` → manette → prêt. Warning `egui_wgpu` rétrogradé via le filtre par défaut (`info,egui_wgpu=error`), `RUST_LOG` reprend la main si posé. Le format de framebuffer n'a pas été changé (risque goldens). | Captures console avant/après |
| A6 | **Fait (hors création d'issue).** `roguelike_demo_clears_rooms_one_at_a_time_to_victory` marqué `#[ignore]` avec justification et commande de relance manuelle — flaky préexistant documenté (60-80 % d'échec sur HEAD). Résultat suite complète : **604 verts, 9 ignorés, 1 échec** — `the_embedded_scene_ambient_decor_matches_the_demo` (« Errant 62 » absent de la scène embarquée), causé par les modifications **non commitées d'une session concurrente** sur `src/scene/demos.rs` (créatures 62+ sans resynchro de `player_scene.json`) ; la synchro n'a volontairement pas été lancée ici (elle réécrirait la scène embarquée en plein travail de l'autre session — piège documenté). `fmt --check` et `clippy --all-targets` verts. L'issue GitHub de suivi du flaky reste à créer (action publique, non faite sans accord). | fmt/clippy verts, suite verte hors échec imputable à la session concurrente |
| A7 | **Fait.** `scripts/doctor.sh` : rustup, cargo, rustc ≥ 1.85 (édition 2024), cible native, dépôt + `assets/bundle/`, `$HOME` écrivable, port 7777 (optionnel). Sortie ✓/✗ + commande de réparation par échec ; environnement = doctor, contenu = `readiness.rs`, sans doublon. 8/8 ✓ sur cette machine. | Exécution du script |

**Reste ouvert pour clore la sortie de phase** : le passage sur un *vrai* compte
macOS vierge (à faire à la main — création de compte non automatisable ici), et
la création de l'issue GitHub pour le test flaky.

## §3 ter — Journal d'exécution, Phase B (réalisée le 19/07/2026)

| Livrable | Détail | Preuve |
| --- | --- | --- |
| `examples/first_game/` | `scene.json` (Sol, Joueur pilotable `controller.input`, Caisses 1-3, Cube tournant scripté, Zone d'éveil `trigger`, Pièces 1-3 `tap_action: Hide`, lumière directionnelle + ponctuelle, `camera_follow`, `version: 2`) ; `scripts/rotating_object.lua` et `scripts/zone_signal.lua` (copies lisibles des scripts inline) ; `README.md` avec `preview.png`. Primitives uniquement — aucun asset externe, aucune dépendance à `~/.motor3derust/`. | 4 tests verts (`cargo test --test first_game_example`) |
| Tests-preuves (`tests/first_game_example.rs`) | (1) chargement par le vrai `Scene::load` + conformité au README (joueur pilotable, 3 collectibles comptés par `Scene::collectibles()`, zéro import) ; (2) **la scène joue** : 5 frames de `advance_play` réelles → le script Lua a tourné le cube, le joueur posé sur une pièce l'a ramassée ; (3) round-trip `Scene::save`/`Scene::load` sans perte (étape 9 du tutoriel) ; (4) synchro inline ↔ copies `.lua` (comparaison du code effectif, commentaires ignorés). | `4 passed` |
| `preview.png` | Rendu **headless réel** de la scène via le nouveau `examples/gen_first_game_preview.rs` (même pipeline que les goldens, caméra cadrée pour tout voir). Vérifié visuellement : les 10 objets sont identifiables d'un coup d'œil. | Image versionnée + générateur rejouable |
| `QUICKSTART.md` | 5 étapes zéro décision, chiffres réels de A3 (1re compilation ~5 min sur M4), `doctor.sh` intégré au parcours, dépannage express. | Relecture ; déroulé machine vierge à faire (reste A3) |
| `docs/FIRST_GAME.md` | 10 étapes ; chaque libellé UI vérifié dans le code (`menus.rs`, `mod.rs:1340-1360`, `mod.rs:1901/2100` : Ajouter → 🧊 Cube, gizmo W/E/R, Matériau, Script (Lua), 💾 Enregistrer sous…). Export marqué « en re-vérification (Phase C) ». | Grep des libellés + tests couvrant Play/save |
| `docs/MENTAL_MODEL.md` | 1 écran : données, 4 rôles, règle Play/Stop, glossaire 8 termes. | Relecture par le testeur externe (à venir) |
| Index README | Bloc « 🚀 Nouveau ici ? » : QUICKSTART → FIRST_GAME → MENTAL_MODEL, avant les renvois historiques. | Diff README |
| Découverte | `assets/examples/demo_controleur.json` / `demo_composants.json` : format périmé (`input_receiver` n'existe plus) → joueur non pilotable. À migrer ou retirer (hors phase) ; le test (1) protège first_game contre ce vieillissement. | Grep `input_receiver` : 0 occurrence dans `src/` |
| Hygiène | `cargo fmt --check` ✓, `clippy --all-targets` ✓, `cargo build --examples` ✓ (le dossier de données n'est pas compilé). | Sorties de commandes |

## §3 quater — Journal d'exécution, Phase C (réalisée le 19/07/2026)

### C1 — Tableau de résultats Play/Pause/Stop (7 tests, `tests/play_mode_audit.rs`)

| Scénario | Résultat prouvé |
| --- | --- |
| Modif d'objet pendant Play → Stop | **Restauré** (position d'avant Play) |
| `obj:destroy()` pendant Play → Stop | **Restauré** (l'objet réapparaît, script compris) |
| Pièce ramassée → Stop | **Restauré** (objectif réarmé à 0/3) |
| Pause | **Gelé** (scripts arrêtés), snapshot conservé ; reprise là où c'était ; Stop depuis « après pause » restaure |
| Erreur Lua (exécution **et** compilation) pendant Play | **Aucune panique**, objets sains continuent, erreurs loguées nommées |
| Play → Stop → éditer → Play | Le 2ᵉ snapshot vient de la scène **éditée** (pas de l'ancien) |
| JSON invalide au chargement | `Err` propre, pas de crash |

Règles confirmées pour la doc : *tout ce qui arrive pendant Play est jeté au
Stop ; tout ce qui est édité hors Play persiste.* (Déjà énoncé dans
FIRST_GAME.md §8 et MENTAL_MODEL.md — désormais prouvé.)

### C3 — Couverture undo/redo (audit des ~40 sites `push_undo`)

| Action | Annulable ? |
| --- | --- |
| Création / suppression / duplication / coller | ✅ |
| Groupes, réordonner, align au sol, reset transform | ✅ |
| Manipulation gizmo (translate/rotate/scale, 1 snapshot par geste, lumières incluses) | ✅ |
| Prefabs (instancier, synchroniser), scènes de démo, IA (script/scène), caméra de jeu | ✅ |
| Outils d'assets (optimisation textures, collecte, bake, présets) | ✅ |
| **Édition de champs Inspecteur** (couleur/matériau, script tapé, nom, physique, tag, éclairage scène/ciel) | ❌ → KNOWN_LIMITATIONS |
| **Import glTF** (`finish_import`) | ❌ → KNOWN_LIMITATIONS (l'objet créé reste supprimable, suppression annulable) |

### C2/C4/C5/C6 — Livrables et trouvailles

- `examples/broken_scene/` + `tests/broken_scene_example.rs` (2 pannes Lua,
  asset manquant, mesh hors bornes, témoin sain) — tout échoue proprement et
  **nommé**.
- Messages améliorés (src) : chunk Lua nommé d'après l'objet
  (`simulation.rs`), asset manquant avec réparation (`scene/persistence.rs`),
  échec de chargement scène avec chemin + piste (`app/persistence.rs`), GLB
  invalide avec chemin + piste (`app/persistence.rs`).
- Exports : `RusteeGear-web.zip` (15 Mo) **joué dans le navigateur** (WebGPU,
  hameau + HUD, connexion serveur réelle) ; `RusteeGear.dmg` (19 Mo) daté du
  jour. Le 1ᵉʳ build web (avant le correctif canvas posé par la session
  concurrente dans lib.rs) donnait un canvas 0×0 — corrigé au rebuild ;
  séquelle : pas de suivi du resize après adoption (chantier de l'autre
  session).
- Trouvaille C2 : fermeture sans alerte de sauvegarde (`CloseRequested =>
  exit()`) → limitation D3 + tâche « dirty-tracking » proposée. **Résolue le
  19/07** : drapeau `AppState::scene_dirty` (posé par `push_undo` + détection
  d'édition de champs UI via `ui_scene_fingerprint`, baissé par sauvegarde
  réussie/chargement/`clear_history`) et modale egui Enregistrer / Quitter
  sans enregistrer / Annuler sur `CloseRequested` et Fichier › Quitter
  (tests-preuves dans `app/selection.rs`). Vérification du piège
  `player_scene.json` à l'export : **différée** (session concurrente
  sur la scène embarquée).
- Hygiène : fmt ✓, clippy 0 warning ✓, 12 tests Phase B+C verts.

### Audit de clôture A+B+C (19/07, après les retouches de la session concurrente)

Re-vérification complète : tous les livrables présents (doctor.sh 8/8 ✓,
QUICKSTART/FIRST_GAME/MENTAL_MODEL, first_game + preview, broken_scene, zip
web 15 Mo + .dmg 19 Mo) ; toutes les modifications src survivantes malgré les
retouches concurrentes de lib.rs ; libellés UI du tutoriel re-confirmés dans
le code ; JSON des deux scènes valides ; suite lib **612 verts / 9 ignorés /
1 échec** (toujours `the_embedded_scene_ambient_decor…`, imputable à la
session concurrente — inchangé) ; **toutes** les suites d'intégration vertes,
**goldens de rendu compris** (mes changements n'ont pas dévié le rendu) ;
fmt ✓, clippy 0 warning.

## §3 quinquies — Journal d'exécution, Phase D (réalisée le 19/07/2026)

| Livrable | Détail | Preuve |
| --- | --- | --- |
| `docs/KNOWN_LIMITATIONS.md` | Matrice de support 6 fonctions × 5 cibles (calibrée sur les vérifications réelles, « non re-vérifié » assumé pour Android/iOS) + limitations par domaine, toutes issues des trouvailles des phases A-C. Liée du README (bloc « Nouveau ici »). | Page publiée |
| `docs/LUA_PORTABLE.md` | API moteur portable (obj.*, dt/time, input, emit/on_event, spawn, find_tag, save, raycast…), Lua commun aux deux versions, liste « non garanti web » (io/os/package, nouveautés 5.3/5.4, coroutines, breakpoints). | Page publiée |
| `examples/scripts/` (4 scripts officiels) | `rotate`, `move_between_points`, `trigger_door`, `simple_enemy` — sous-ensemble portable uniquement, commentés. | Test différentiel |
| Test `official_scripts_match_between_backends` | Chaque script lu **depuis le fichier publié**, exécuté sur mlua (natif) et rilua (web) avec les mêmes entrées (dont `triggered` vrai/faux et une cible `find_tag`), positions/rotations comparées à 1e-5. Helpers `run_native_at`/`run_web_at` paramétrés ajoutés à `scripting_web.rs`. | 3 tests différentiels verts |
| « À propos » enrichi | `Version 0.1.0 — Developer Preview 1` + `Commit : <RUSTEEGEAR_COMMIT \| build local>` (`option_env!` — la variable sera posée au build du tag E5). Titre de fenêtre déjà « RusteeGear ». | Code + capture à faire au tag |
| Marquage Experimental | Menu IA scène, fenêtre IA scripts, labels export Android/iOS « non re-vérifié (préversion) » — aucun test ne dépendait de ces libellés (vérifié). | Diff + grep |
| Trouvaille | Matériaux GLB importés = **`base_color_factor` seul** (pas de textures/normal maps sur les meshes importés, `scene/import.rs:52`) — documenté tel quel dans les limitations, plus précis que la reco d'origine. | Lecture du code |
| Incident croisé | Pendant la phase : casse transitoire `E0061` (46ᵉ paramètre ajouté à `editor.run` par la session « alerte de fermeture ») — résolue d'elle-même en ~90 s, aucun contournement fait ici. | Journal |
| Hygiène | fmt ✓, clippy 0 warning ✓, 12 tests intégration + 3 différentiels verts. | Sorties de commandes |

## §3 sexies — Journal d'exécution, Phase E (réalisée le 19/07/2026)

| Livrable | Détail | Preuve |
| --- | --- | --- |
| `docs/TEST_SCENARIO.md` | 10 étapes chronométrées (cibles issues des mesures réelles A3/C6), variantes Test A (.dmg) / Test B (source), GLB d'import fourni (`assets/models/creature.glb`, 476 Ko, tracké), libellés UI vérifiés dans le code (« Statique », « 📥 Importer glTF… »), consigne anti-enlisement (bloqué > 10 min → noter, passer), renvois KNOWN_LIMITATIONS + broken_scene. | Document prêt |
| `docs/TEST_FEEDBACK_FORM.md` | Avant (profil/machine/variante), tableau par étape (réussi, temps, blocage, message copié, compris la réparation ?, difficulté 1-5), 9 questions après test, écran + voix haute proposés, diagnostic joint à chaque blocage. | Document prêt |
| « Aide → 📋 Copier le diagnostic » | `log_buffer::diagnostic_report()` : version/Preview/commit/OS/arch/format de scène + 30 dernières lignes de log, **`$HOME` anonymisé en `~`**. Bouton menu Aide via `ui.ctx().copy_text` (patron existant windows.rs:2117). | Test `diagnostic_report_…_redacts_the_home_directory` ✓ |
| Injection du commit | `build_dmg.sh` + `build_web.sh` exportent `RUSTEEGEAR_COMMIT=$(git rev-parse --short HEAD)` (surchargable CI) → « À propos » et diagnostic des builds distribués affichent le commit exact. | Diff scripts |
| Hygiène | fmt ✓, clippy 0 warning ✓, 12 tests intégration + différentiels + diagnostic verts. | Sorties de commandes |

### Checklist de gel E5 (à dérouler d'un coup, sur arbre stable)

1. **Attendre** la fin des chantiers concurrents (session créatures, tâches de
   fond) — la tâche « alerte de fermeture » est déjà terminée et a corrigé la
   limitation correspondante.
2. **Resynchroniser la scène embarquée** (session créatures :
   `cargo test sync_embedded_scene_ambient_decor_from_the_demo -- --ignored`
   sur son travail commité) → l'échec « Errant 62 » disparaît.
3. **Committer par lots cohérents** (préversion testable / exemples / docs),
   puis `cargo fmt --check && cargo clippy --all-targets && cargo test`
   → 0 échec hors `#[ignore]`.
4. `git tag v0.1.0-alpha.1` + push ; CI verte sur le tag.
5. **Depuis le tag** : `./packaging/build_dmg.sh` et
   `PLAYER_BUILD=1 ./packaging/build_web.sh` (le commit s'injecte tout seul).
6. Vérifier : « À propos » affiche le commit du tag ; date du .dmg = jour même
   (piège des `.app` périmés sur volumes montés).
7. Release GitHub `v0.1.0-alpha.1` : .dmg + zip web en pièces jointes.
8. Mail au testeur avec les 4 pièces : lien release, QUICKSTART,
   TEST_SCENARIO, TEST_FEEDBACK_FORM.

## §3 septies — Audit final des 5 phases (19/07/2026, fin de journée)

**Verdict : prêt à geler, à deux dépendances externes près.**

- **Livrables** : 17/17 présents (doctor, QUICKSTART, FIRST_GAME, MENTAL_MODEL,
  KNOWN_LIMITATIONS, LUA_PORTABLE, TEST_SCENARIO, TEST_FEEDBACK_FORM,
  first_game + preview, broken_scene, 4 scripts officiels, 3 suites de tests) ;
  artefacts de répétition datés du jour (zip web 15 Mo, .dmg 19 Mo).
- **Preuves** : doctor 8/8 ✓ ; fmt ✓ ; clippy 0 warning ; suites d'intégration
  19/19 vertes (first_game 4, play_mode 7, broken_scene 1, goldens 7) ;
  suite lib **617 verts / 9 ignorés / 2 échecs**, tous deux hors périmètre du
  sprint :
  1. `the_embedded_scene_ambient_decor_matches_the_demo` (« Errant 62 ») —
     session créatures, connu depuis la Phase A, à résoudre par sa resynchro ;
  2. `console_pause_play_stop_step_drive_the_same_state_as_the_toolbar`
     (nouveau pont `--pilot`) — **flaky sous exécution parallèle**, passe 3/3
     en isolation ; tâche de fond proposée pour le rendre déterministe.
- **Chantiers concurrents intégrés pendant le sprint** : migration
  `demo_*.json` **committée** (`8acb9ef`, avec garde-fou
  `tests/examples_scenes.rs` sur le modèle de first_game) ; alerte de
  fermeture **implémentée** (limitation corrigée dans KNOWN_LIMITATIONS) ;
  pont de pilotage `--pilot` ajouté (documenté dans le QUICKSTART).

### Restant, dans l'ordre

1. **Résoudre « Errant 62 »** : la session créatures committe son travail et
   resynchronise la scène embarquée — dernier rouge légitime de la suite.
2. **Stabiliser le test console flaky** (tâche proposée) — ou décision
   explicite de l'accepter tel quel pour le tag.
3. **Committer les 22 fichiers en attente** par lots cohérents (préversion
   testable / exemples+docs / réglages build), après accord — rien n'a été
   commité par cette session.
4. **Dérouler la checklist de gel E5** (§3 sexies) : tag `v0.1.0-alpha.1`,
   CI verte dessus, builds depuis le tag, release, mail au testeur (4 pièces).
5. **Manuels, avant ou pendant le test** : parcours complet sur un vrai compte
   macOS vierge (A3/B2 — seule vérification non automatisable d'ici) ; issue
   GitHub du test roguelike flaky (action publique, sur accord).

---

<a id="reporte"></a>
## 4. Reporté à plus tard (après le premier retour externe)

Dans l'ordre de probabilité qu'ils remontent comme besoin réel :

1. **Système de projet `project.rgear` + projets hors dépôt** (recos 7, 20) — le chantier structurel n°1, mais à dimensionner *avec* le retour du testeur, pas avant.
2. **Écran d'accueil + projets récents** (reco 8) — dépend de 1.
3. **Renommage complet motor3derust → rusteegear** (reco 12, partie crate/dossier/doc API) avec migration de `~/.motor3derust/` — après 1, jamais avant un test.
4. **Terminologie unifiée dans l'UI et le code** (reco 11) — chantier transversal, guidé par le glossaire posé en B4.
5. **Mode Beginner/Advanced** (reco 28) — seulement si le retour montre que la richesse de l'UI perd le testeur.
6. **Messages d'erreur au-delà du top-5** (reco 15) — prioriser ceux que le testeur aura réellement rencontrés.
7. **Issue GitHub préremplie** (reco 25) — le diagnostic copiable (E4) suffit pour un testeur unique.
8. **Correction du test roguelike flaky** — issue ouverte en A6.

---

<a id="faits"></a>
## 5. Faits code sur lesquels s'appuie ce plan (vérifiés le 19/07)

- `[profile.dev-fast]` existe (Cargo.toml:52) — le QUICKSTART peut l'utiliser tel quel.
- Undo = pile de `SceneSnapshot` + `push_undo()` (src/app/mod.rs:712) — reco 18 est un audit, pas un développement.
- Backends Lua : `mlua` natif + `rilua` web, avec tests différentiels prévus dès le sprint 137 (Cargo.toml:98-103) — reco 14 est surtout de la documentation.
- `packaging/build_web.sh`, `build_dmg.sh`, `EXPORT.md` existent ; démo web déployée sur GitHub Pages — reco 21 est une vérification, pas une construction.
- `src/editor/readiness.rs` (« APK Readiness Check ») : diagnostic de contenu déjà en place — `doctor.sh` ne couvre que l'environnement, sans doublon.
- `src/crash_log.rs` + `src/log_buffer.rs` existent — support direct pour E4.
- `examples/` = binaires Cargo (`gen_*`, `smoke_vps`, …) — B1 doit éviter la collision avec `autoexamples`.
- `~/.motor3derust/assets/` = dépendance d'exécution centrale (src/assets.rs:241-251), avec override thread-local disponible pour les tests — base de A1.
- Pièges documentés en mémoire projet réutilisés : écrasement `player_scene.json` (C2), auto-connexion réseau figeant les créatures (A4), `.app` périmés sur volumes montés (C6/E2), test roguelike flaky (A6), goldens sur worktree propre (A6), sessions concurrentes (E5).
