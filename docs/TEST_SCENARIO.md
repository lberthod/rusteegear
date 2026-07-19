# Scénario de test — Developer Preview 1

À dérouler **dans l'ordre**, en notant pour chaque étape l'heure de début et de
fin (le [formulaire](TEST_FEEDBACK_FORM.md) a une ligne par étape). Si tu
bloques plus de 10 minutes sur une étape : note le blocage, passe à la
suivante. Un blocage n'est pas un échec de ta part — c'est exactement ce qu'on
cherche à mesurer.

Deux variantes :

- **Test A — expérience moteur** (sans toolchain Rust) : commence à l'étape 3
  avec le `RusteeGear.dmg` fourni (clic droit ▸ Ouvrir la première fois, le
  .dmg n'est pas signé).
- **Test B — expérience contributeur** : toutes les étapes.

Avant de signaler quoi que ce soit, jette un œil à
[KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) : si c'est listé, c'est connu.

| # | Étape | Ce qu'on doit voir | Temps cible |
| --- | --- | --- | --- |
| 1 | **Installer** : suivre [QUICKSTART.md](../QUICKSTART.md) §1-2 (rustup, clone, `./scripts/doctor.sh`) | doctor : « Environnement prêt » | < 10 min |
| 2 | **Compiler + lancer** : `cargo run --profile dev-fast` | Console : `RusteeGear 0.1.0` puis `GPU : …` ; l'éditeur s'ouvre sur le hameau | 1ʳᵉ compilation ~5-10 min (normal) ; lancements suivants < 30 s |
| 3 | **Ouvrir le projet exemple** : 📂 Ouvrir… → `examples/first_game/scene.json` | La scène de la [preview](../examples/first_game/preview.png) : sol vert, capsule orange, 3 caisses, cube bleuté, zone jaune, 3 pièces | < 30 s |
| 4 | **Importer un GLB** : 📥 Importer glTF… → `assets/models/creature.glb` (dans le clone) | Une créature apparaît dans la scène et dans la hiérarchie | < 2 min |
| 5 | **La placer + collider** : gizmo (W) pour la poser au sol ; Inspecteur → physique **Statique** | L'objet a un collider (le Joueur bute dessus en Play) | < 3 min |
| 6 | **Ajouter un objet scripté** : Ajouter → 🧊 Cube, puis Inspecteur → Script (Lua) : `obj.ry = obj.ry + 45 * dt` (ou coller `examples/scripts/rotate.lua`) | — | < 5 min |
| 7 | **Play** : WASD/flèches, Espace = saut ; ramasser les 3 pièces ; marcher sur la zone jaune | Ton cube tourne ; la zone devient verte ; chrono figé quand 3/3 pièces | < 3 min |
| 8 | **Stop puis vérifier** : les pièces réapparaissent, ton cube ajouté hors Play est toujours là | Règle : ce qui arrive *pendant* Play est jeté, ce qui est édité *hors* Play persiste | < 1 min |
| 9 | **Sauvegarder + rouvrir** : 💾 Enregistrer sous… → `~/Documents/mon_test.json` ; 📂 Ouvrir… dessus | Scène identique, script compris | < 2 min |
| 10 | **Exporter en web** : `PLAYER_BUILD=1 ./packaging/build_web.sh`, puis dézipper `target/export/RusteeGear-web.zip` et `python3 -m http.server 8080` dans le dossier ; ouvrir `http://localhost:8080` dans Chrome | Le jeu du hameau tourne dans le navigateur (WebGPU requis) | < 10 min (compilation wasm comprise) |

## En cas de pépin

- Chaque erreur devrait nommer le fichier/l'objet fautif et proposer une
  réparation — si un message t'a laissé sans piste, **copie-le** dans le
  formulaire, c'est une donnée précieuse.
- **Aide → 📋 Copier le diagnostic** copie version, commit, OS, GPU et les
  derniers logs : colle ça avec chaque signalement.
- La scène `examples/broken_scene/` est **volontairement cassée** (c'est un
  banc d'essai d'erreurs) — ne pas la signaler.
