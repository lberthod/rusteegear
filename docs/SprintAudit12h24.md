# Plan de sprints — Audit du 19 juillet 2026 à 12 h 24

> Plan d'action issu de l'analyse [AnalyseAudit12h24.md](AnalyseAudit12h24.md).
> Chaque sprint est découpé en sous-phases avec un critère « terminé quand » vérifiable.
> Ordre pensé pour atteindre une alpha publique crédible : d'abord les deux corrections
> minuscules à fort impact (Pilot, DMG), puis le chantier structurant du format de
> projet, puis la sécurité des sauvegardes, enfin la bêta extérieure.

## Priorités immédiates

| Ordre | Travail | Effort | Impact |
|---:|---|---:|---:|
| 1 | Isoler les tests TCP Pilot | Faible | Élevé |
| 2 | Corriger le chemin du DMG | Très faible | Élevé |
| 3 | Créer une Release alpha | Faible | Très élevé |
| 4 | Ajouter le manifeste de projet | Moyen | Très élevé |
| 5 | Transformer le wizard | Moyen | Très élevé |
| 6 | Migrer First Game | Moyen | Très élevé |
| 7 | Sécuriser les sauvegardes | Moyen | Élevé |
| 8 | Tester avec des personnes extérieures | Faible | Décisif |

---

## Sprint 1 — Stabiliser Pilot

**Objectif** : rendre la suite standard indépendante des permissions réseau.

État des lieux : [tests/pilot_bridge.rs](../tests/pilot_bridge.rs) contient 6 tests —
4 TCP et 2 déterministes sans socket. Aucune feature `pilot_tests` dans `Cargo.toml`.

### Sous-phases

- **1.1 — Feature `pilot_tests`** : ajouter dans `Cargo.toml` :

  ```toml
  [features]
  pilot_tests = []
  ```

  et placer les quatre tests TCP (`pilot_bridge_drives_lua_play_scene_and_inputs_over_tcp`,
  `pilot_bridge_reports_lua_errors_and_survives_malformed_requests`,
  `pilot_bridge_full_editing_and_gameplay_session`,
  `pilot_bridge_options_and_demo_loading`) derrière
  `#[cfg_attr(not(feature = "pilot_tests"), ignore = "nécessite un socket TCP local — lancer avec --features pilot_tests")]`.
- **1.2 — Suite normale intacte** : conserver les deux tests déterministes
  (`advance_steps_*`) dans la suite standard, sans feature.
- **1.3 — Job CI Pilot** : ajouter un job dédié exécutant
  `cargo test --features pilot_tests --test pilot_bridge` dans un environnement
  autorisant les sockets.
- **1.4 — Sécurité du pont** : vérifier (test à l'appui) que le pont refuse les
  connexions non locales — liaison sur `127.0.0.1` uniquement.
- **1.5 — Documentation** : documenter dans [PILOT.md](PILOT.md) que Pilot est
  désactivé par défaut et comment lancer les tests TCP.

### Terminé quand

```bash
cargo test --all-targets
```

passe même si les sockets sont interdits, et le job CI Pilot est vert.

---

## Sprint 2 — Corriger et tester la Release

**Objectif** : une Release GitHub alpha téléchargeable et lançable.

État des lieux : [packaging/build_dmg.sh:41](../packaging/build_dmg.sh:41) produit
`RusteeGear.dmg` mais [.github/workflows/release.yml:27](../.github/workflows/release.yml:27)
publie `Motor3DeRust.dmg` — l'upload échouera.

### Sous-phases

- **2.1 — Chemin du DMG** : corriger le workflow en
  `files: target/release/bundle/dmg/RusteeGear.dmg`.
- **2.2 — Contenu des artefacts** : décider si le DMG contient l'éditeur ou le
  Player ; produire les deux si nécessaire (le chemin export de `build_dmg.sh`
  produit déjà `target/export/${OUTPUT_NAME}.dmg`).
- **2.3 — Tag alpha** : créer `v0.1.0-alpha.1` et laisser le workflow construire.
- **2.4 — Test sur machine propre** : télécharger les artefacts et les lancer sur
  une installation vierge (attention aux `.app` périmés sur les volumes montés
  `/Volumes/RusteeGear` — comparer la date de build).

### Livrables

```text
RusteeGear-Editor-v0.1.0-alpha.1.dmg
RusteeGear-FirstGame-v0.1.0-alpha.1.dmg
RusteeGear-FirstGame-v0.1.0-alpha.1-web.zip
```

### Terminé quand

La Release `v0.1.0-alpha.1` existe sur GitHub avec ses artefacts, et chacun démarre
sur une installation propre.

---

## Sprint 3 — Manifeste de projet

**Objectif** : que RusteeGear sache ce qu'est un « projet », pas seulement une scène.

État des lieux : aucune occurrence de `ProjectManifest` / `project.rusteegear.json`
dans `src/` ; le seul `PROJECT_ROOT` est la racine du dépôt du moteur
([src/editor/export.rs:14](../src/editor/export.rs:14)).

### Sous-phases

- **3.1 — Format du manifeste** : définir `project.rusteegear.json` :

  ```json
  {
    "format": 1,
    "name": "First Game",
    "main_scene": "scenes/main.scene.json",
    "build": "build.json"
  }
  ```

- **3.2 — Chargement et validation** : struct `ProjectManifest` (serde), version de
  format contrôlée, messages d'erreur lisibles (fichier manquant, JSON invalide,
  scène principale introuvable).
- **3.3 — Racine de projet** : notion de `project_root` dans le moteur ; résolution
  des assets (scènes, scripts, modèles, textures, audio) relativement au projet et
  non plus au dépôt.
- **3.4 — Ouverture par manifeste** : ouvrir un dossier contenant
  `project.rusteegear.json` charge la scène principale.
- **3.5 — Tests-preuves** : tests de chargement valide, format inconnu, chemins
  hors racine refusés, erreurs lisibles.

### Terminé quand

Un dossier avec manifeste s'ouvre comme un projet, ses assets se résolvent
relativement à sa racine, et la validation échoue proprement sur les cas d'erreur.

---

## Sprint 4 — Gestionnaire de projets

**Objectif** : le cycle de vie complet d'un projet depuis l'éditeur.

### Sous-phases

- **4.1 — Wizard de création** : transformer le wizard actuel en :

  ```text
  Nom du projet
  Emplacement
  Template
  Créer
  ```

  La création génère la structure complète (manifeste, `scenes/`, `scripts/`, …).
- **4.2 — Ouvrir / Fermer** : ouvrir un projet existant (sélection du dossier ou du
  manifeste) ; fermer proprement le projet courant.
- **4.3 — Projets récents** : liste persistée, accessible à l'accueil de l'éditeur.
- **4.4 — Confort** : Révéler dans le Finder ; Dupliquer le projet.
- **4.5 — Tests-preuves** : création depuis template → ouverture → scène principale
  chargée ; projets récents mis à jour.

### Terminé quand

On peut créer, ouvrir, fermer, retrouver et dupliquer un projet sans toucher au
système de fichiers à la main.

---

## Sprint 5 — Migrer First Game

**Objectif** : First Game devient le premier vrai projet RusteeGear.

### Sous-phases

- **5.1 — Restructuration** :

  ```text
  examples/first_game/
  ├── project.rusteegear.json
  ├── build.json
  ├── scenes/main.scene.json
  └── scripts/
  ```

- **5.2 — Chemins internes** : adapter les références de la scène et des scripts à
  la nouvelle arborescence.
- **5.3 — Documentation** : adapter [QUICKSTART.md](../QUICKSTART.md) et
  [FIRST_GAME.md](FIRST_GAME.md) :

  ```text
  Ouvrir le projet examples/first_game
  ```

  au lieu de « Ouvrir examples/first_game/scene.json ».
- **5.4 — Tests-preuves** : les 4 tests First Game passent sur la nouvelle
  structure ; ouverture par manifeste testée.

### Terminé quand

First Game s'ouvre comme un projet, le tutoriel est à jour, et les tests existants
passent sur la nouvelle arborescence.

---

## Sprint 6 — Sécuriser les sauvegardes

**Objectif** : aucun testeur extérieur ne doit pouvoir perdre son travail.

État des lieux : la sauvegarde de scène est un `std::fs::write` direct
([src/editor/export.rs:137](../src/editor/export.rs:137)) — une coupure en pleine
écriture corrompt le fichier.

### Sous-phases

- **6.1 — Sauvegarde atomique** : écrire dans un fichier temporaire du même dossier
  puis renommer (remplacement atomique).
- **6.2 — Backup** : conserver un `.backup` de la version précédente à chaque
  sauvegarde.
- **6.3 — Indicateur de modifications** : marqueur « non enregistré » visible dans
  l'éditeur.
- **6.4 — Confirmation avant fermeture** : avertissement si des modifications non
  enregistrées existent.
- **6.5 — Autosave** : sauvegarde périodique dans un emplacement dédié (pas par-dessus
  le fichier de l'utilisateur).
- **6.6 — Récupération au redémarrage** : au lancement, proposer de restaurer un
  autosave plus récent que la dernière sauvegarde manuelle.
- **6.7 — Tests-preuves** : atomicité (le fichier cible n'est jamais tronqué),
  backup présent, autosave restauré.

### Terminé quand

Tuer l'éditeur en pleine sauvegarde ne corrompt jamais la scène, et un crash ne fait
perdre au pire que l'intervalle d'autosave.

---

## Sprint 7 — Serveur local depuis l'éditeur

**Objectif** : le multijoueur local sans ligne de commande.

### Sous-phases

- **7.1 — Démarrer / arrêter** : lancer le serveur depuis l'éditeur ; arrêt propre.
- **7.2 — État visible** : panneau affichant l'état du serveur (port, joueurs
  connectés).
- **7.3 — Copier l'adresse** : bouton copiant l'adresse de connexion.
- **7.4 — Auto-connexion de l'hôte** : l'éditeur qui héberge se connecte
  automatiquement à son propre serveur.
- **7.5 — Code de salon** : rejoindre via un code de salon.
- **7.6 — Tests-preuves** : cycle démarrer → connecter → arrêter automatisé (via
  Pilot si possible), en respectant `PROTOCOL_VERSION`.

### Terminé quand

Deux instances sur la même machine jouent ensemble sans jamais ouvrir un terminal.

---

## Sprint 8 — Bêta extérieure

**Objectif** : valider avec 3–5 personnes extérieures.

### Sous-phases

- **8.1 — Kit testeur** : Release alpha (Sprint 2) + [QUICKSTART.md](../QUICKSTART.md)
  + [TEST_SCENARIO.md](TEST_SCENARIO.md) + [TEST_FEEDBACK_FORM.md](TEST_FEEDBACK_FORM.md).
- **8.2 — Scénario imposé** :

  1. suivre le Quickstart ;
  2. ouvrir First Game ;
  3. jouer ;
  4. ajouter un cube ;
  5. ajouter le script ;
  6. sauvegarder ;
  7. rouvrir ;
  8. exporter ;
  9. envoyer un retour.

- **8.3 — Collecte et tri** : centraliser les retours, classer
  bloquant / gênant / cosmétique.
- **8.4 — Boucle corrective** : corriger les bloquants, publier `v0.1.0-alpha.2` si
  nécessaire.

### Terminé quand

Au moins 3 personnes extérieures ont déroulé le scénario de bout en bout et leurs
retours sont triés et adressés.

---

## Vue d'ensemble

```text
Sprint 1 (Pilot)  ─┐
Sprint 2 (DMG)    ─┤  petites corrections, gros déblocage
                   ▼
Sprint 3 (manifeste) → Sprint 4 (gestionnaire) → Sprint 5 (migration First Game)
                   ▼
Sprint 6 (sauvegardes sûres) → Sprint 7 (serveur local)
                   ▼
Sprint 8 (bêta extérieure)
```

Voir le constat complet dans [AnalyseAudit12h24.md](AnalyseAudit12h24.md) et la
feuille de route générale dans [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md).
