# Analyse — Audit du 19 juillet 2026 à 12 h 24

> Photographie de l'état du projet à 12 h 24. Constat uniquement : le plan d'action
> détaillé (sprints et sous-phases) est dans [SprintAudit12h24.md](SprintAudit12h24.md).
> Toutes les affirmations vérifiables ont été recontrôlées sur le dépôt à la rédaction ;
> les preuves sont citées en place.

## Verdict immédiat

RusteeGear est maintenant dans un état beaucoup plus propre et crédible pour une préversion.
Les avancées précédemment non intégrées sont désormais :

- commitées ;
- poussées sur `origin/main` ;
- présentes dans un worktree propre ;
- couvertes par des tests.

Le projet se situe autour de **78–82 % d'une bêta testable par des personnes extérieures**.

---

## État Git

| Vérification | Résultat |
|---|---:|
| Branche | `main` |
| Synchronisée avec `origin/main` | ✅ |
| Fichiers modifiés | 0 |
| Fichiers non suivis | 0 |
| Worktree propre | ✅ |
| Dernier commit | `1d903e0` |
| Packs créatures 63–112 intégrés | ✅ |
| First Game intégré | ✅ |
| Pilot intégré | ✅ |

C'est une amélioration importante : le projet n'est plus dans un gros état intermédiaire
difficile à évaluer.

---

## État qualité

| Vérification | Résultat |
|---|---:|
| `cargo fmt --all -- --check` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | ✅ |
| Tests principaux de la bibliothèque | ✅ 618 réussis |
| Tests serveur | ✅ 11 réussis |
| First Game | ✅ 4 réussis |
| Exemple volontairement cassé | ✅ |
| Toutes les scènes exemples | ✅ |
| Assets flore | ✅ |
| Tests visuels | ✅ 8 réussis |
| Tests Pilot sans socket | ✅ 2 réussis |
| Tests Pilot TCP | ❌ 4 |
| Tests ignorés | 9 |

### Interprétation des quatre échecs Pilot

Les quatre tests Pilot échouent sur :

```text
pilot : liaison 127.0.0.1:0 impossible
Operation not permitted
```

Le code compile et les tests Pilot ne nécessitant pas de socket passent. L'environnement
d'exécution interdit l'ouverture d'un port TCP local.

**Vérifié sur le dépôt** : [tests/pilot_bridge.rs](../tests/pilot_bridge.rs) contient
6 tests — 4 tests TCP (qui ouvrent un `TcpStream` vers le pont) et 2 tests déterministes
`advance_steps_*` sans socket. Aucune feature `pilot_tests` n'existe encore dans
`Cargo.toml`.

Ce n'est donc pas la preuve que Pilot est fonctionnellement cassé. En revanche, c'est bien
un problème d'organisation des tests :

> `cargo test --all-targets` devrait pouvoir fonctionner dans un environnement qui
> interdit les sockets.

Les tests TCP Pilot doivent être séparés, comme le sont déjà les tests réseau.

---

## Avancement par domaine

| Domaine | Niveau | État |
|---|---:|---|
| Moteur 3D | 92 % | 🟢 |
| Démo MMORPG | 95 % | 🟢 |
| First Game | 90 % | 🟢 |
| Documentation d'entrée | 90 % | 🟢 |
| Diagnostic d'installation | 85 % | 🟢 |
| Tests du cœur | 92 % | 🟢 |
| Pilot externe | 75 % | 🟡 |
| Export | 72 % | 🟡 |
| Format de projet | 25 % | 🔴 |
| Protection des sauvegardes | 45 % | 🟡 |
| Multijoueur local graphique | 55 % | 🟡 |
| Préparation bêta | 78–82 % | 🟡 proche |

---

## Ce qui est maintenant acquis

### Phase A — Onboarding : ✅ presque terminée

Les éléments suivants sont intégrés :

- [QUICKSTART.md](../QUICKSTART.md) ;
- [First Game](../examples/first_game/README.md) ;
- [tutoriel de dix minutes](FIRST_GAME.md) ;
- modèle mental du moteur ([MENTAL_MODEL.md](MENTAL_MODEL.md)) ;
- limites connues ([KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md)) ;
- scénario de test ([TEST_SCENARIO.md](TEST_SCENARIO.md)) ;
- formulaire de retour ([TEST_FEEDBACK_FORM.md](TEST_FEEDBACK_FORM.md)) ;
- script de diagnostic (`scripts/doctor.sh`) ;
- exemple volontairement cassé.

Une nouvelle personne dispose désormais d'un chemin clair :

```text
Cloner
→ exécuter doctor.sh
→ lancer l'éditeur
→ ouvrir First Game
→ jouer
→ ajouter un objet
→ écrire un script
→ sauvegarder
```

C'est un changement qualitatif majeur.

### Phase B — Petite démo reproductible : ✅ presque terminée

`examples/first_game` est :

- autonome vis-à-vis des modèles externes ;
- construit uniquement avec des primitives ;
- documenté ;
- illustré (`preview.png`) ;
- accompagné de scripts lisibles ;
- testé en Play ;
- testé en sauvegarde/rechargement.

**Limite** : il s'agit encore d'un dossier contenant une scène
(`scene.json` + `scripts/`), pas d'un véritable projet reconnu par RusteeGear.

### Phase C — Automatisation externe : 🟡 avancée

Pilot ([docs/PILOT.md](PILOT.md)) permet d'automatiser :

- l'état de l'application ;
- Play, Pause, Stop et Step ;
- les entrées du joueur ;
- la console ;
- Lua ;
- la scène ;
- les captures ;
- les logs.

C'est une excellente base pour tester automatiquement le tutoriel et les exports.

**Travail restant** : séparer les tests Pilot derrière une feature `pilot_tests`
exécutée dans un job CI autorisant les sockets (détail dans
[SprintAudit12h24.md](SprintAudit12h24.md), Sprint 1).

---

## Les trois blocages structurants

### 1. Aucun véritable format de projet — 🔴 principal chantier

**Vérifié sur le dépôt** : aucune occurrence de `ProjectManifest` ni de
`project.rusteegear.json` dans `src/`. Le seul `PROJECT_ROOT` existant est la constante
`env!("CARGO_MANIFEST_DIR")` de [src/editor/export.rs](../src/editor/export.rs:14) —
c'est la racine du **dépôt du moteur**, pas celle d'un projet utilisateur.

Il manque toujours :

- `ProjectManifest` ;
- `project_root` (au sens projet utilisateur) ;
- `project.rusteegear.json` ;
- projets récents ;
- résolution des assets relativement au projet.

First Game s'ouvre comme un fichier :

```text
examples/first_game/scene.json
```

L'objectif reste :

```text
examples/first_game/
├── project.rusteegear.json
├── scenes/main.scene.json
├── scripts/
├── models/
├── textures/
├── audio/
└── build.json
```

Tant que cette structure n'existe pas, RusteeGear reste principalement un éditeur de
scènes à l'intérieur de son propre dépôt.

### 2. Release macOS toujours incohérente

**Vérifié sur le dépôt** :

- [packaging/build_dmg.sh:41](../packaging/build_dmg.sh:41) produit
  `target/release/bundle/dmg/RusteeGear.dmg` ;
- [.github/workflows/release.yml:27](../.github/workflows/release.yml:27) publie
  `target/release/bundle/dmg/Motor3DeRust.dmg`.

Le problème est encore présent malgré le commit mentionnant un correctif du bundle macOS.

**Conséquence** : le DMG peut être correctement construit, puis ne pas être trouvé au
moment de créer la Release GitHub.

**Correction** : dans le workflow, utiliser
`files: target/release/bundle/dmg/RusteeGear.dmg`, puis tester avec un tag alpha.

### 3. Sauvegarde utilisateur encore insuffisamment protégée

La sauvegarde et le rechargement fonctionnent. First Game le prouve par test.

**Vérifié sur le dépôt** : la sauvegarde de scène passe par un `std::fs::write` direct
([src/editor/export.rs:137](../src/editor/export.rs:137)). Il n'existe pas de système
complet pour :

- fichier temporaire ;
- remplacement atomique ;
- backup ;
- autosave ;
- restauration après crash ;
- avertissement de modifications non enregistrées.

Pour une bêta d'éditeur, c'est plus important qu'ajouter de nouveaux assets.

---

## Conclusion

À 12 h 24, RusteeGear est dans son meilleur état observé jusqu'ici :

- dépôt propre ;
- branche synchronisée ;
- onboarding intégré ;
- First Game validé ;
- cœur technique vert ;
- documentation utilisable ;
- automatisation Pilot disponible.

Le cap suivant n'est plus de créer la démonstration : **elle existe**. Le cap suivant est :

```text
Scène exemple
→ véritable projet
→ Release téléchargeable
→ sauvegarde sûre
→ testeur extérieur
```

Le projet est proche d'une alpha publique crédible. Les deux actions les plus urgentes
sont minuscules comparées au moteur : isoler les tests TCP Pilot et corriger le chemin
du DMG. Le plan de sprints détaillé est dans
[SprintAudit12h24.md](SprintAudit12h24.md).
