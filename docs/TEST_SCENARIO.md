# Scénario de test — Developer Preview 1

À dérouler **dans l'ordre**, en notant pour chaque étape l'heure de début et de
fin (le [formulaire](TEST_FEEDBACK_FORM.md) a une ligne par étape). Si tu
bloques plus de 10 minutes sur une étape : note le blocage, passe à la
suivante. Un blocage n'est pas un échec de ta part — c'est exactement ce qu'on
cherche à mesurer.

Deux variantes :

- **Test A — expérience moteur** (sans toolchain Rust, sans clone) : le
  `RusteeGear.dmg` seul. Commence à l'étape 3 ; les étapes 1, 2 et 10 sont
  réservées au Test B (l'étape 10 a un équivalent sans compilation, indiqué
  dans le tableau). Tout ce dont le Test A a besoin est **embarqué dans
  l'application** — aucun fichier du dépôt n'est requis.
- **Test B — expérience contributeur** : toutes les étapes, depuis le code
  source.

## Où prendre le `.dmg` (Test A)

Sur la page [GitHub Releases](https://github.com/lberthod/rusteegear/releases)
du dépôt. Première ouverture : clic droit ▸ Ouvrir (le `.dmg` n'est pas signé).

> ⚠️ Au 4 septembre 2026, la dernière release publiée est **v0.1.0-alpha.3 du
> 19 juillet 2026** : elle est **antérieure** aux vagues UX de septembre
> (menu Démos ▸ Premier jeu embarqué, toasts, écran d'accueil, aide en jeu…)
> que ce scénario suppose. Un nouveau tag doit être publié avant de lancer
> un Test A ; sans lui, la variante A ne peut suivre ce scénario qu'à partir
> d'un `.dmg` produit par `./packaging/build_dmg.sh` (ce qui la ramène au
> Test B).

Avant de signaler quoi que ce soit, jette un œil à
[KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) : si c'est listé, c'est connu.

| # | Étape | Ce qu'on doit voir | Temps cible |
| --- | --- | --- | --- |
| 1 | **Test B seulement — Installer** : suivre [QUICKSTART.md](../QUICKSTART.md) §1-2 (rustup, clone, `./scripts/doctor.sh`) | doctor : « Environnement prêt » | < 10 min |
| 2 | **Test B seulement — Compiler + lancer** : `cargo run --profile dev-fast` | Console : `RusteeGear 0.1.0` puis `GPU : …` ; l'éditeur s'ouvre sur le hameau au premier lancement (ensuite, sur le dernier projet ouvert) | 5-10 min la 1ʳᵉ fois (compilation, normal) ; < 30 s ensuite |
| 3 | **Ouvrir le projet exemple** : menu **Fichier ▸ 🎬 Démos ▸ ⭐ Commencer ▸ ⭐ Premier jeu** (embarqué : marche aussi depuis le `.dmg` seul ; Test B : équivalent à **📂 Ouvrir un projet…** → `examples/first_game`) | La scène de la [preview](../examples/first_game/preview.png) : sol vert, capsule orange, 3 caisses, cube bleuté, zone jaune, 3 pièces | < 30 s |
| 4 | **Ajouter un objet 3D** : Test A : menu **Ajouter ▸ ⚪ Sphère** ; Test B : **📥 Importer glTF…** → `assets/models/creature.glb` (dans le clone) | L'objet apparaît dans la scène et dans la hiérarchie, sélectionné | < 2 min |
| 5 | **Le placer + volume de collision** : gizmo (W) pour le poser au sol ; Inspecteur → physique **Corps statique** | L'objet a un volume de collision (le Joueur bute dessus en Play) | < 3 min |
| 6 | **Ajouter un objet scripté** : Ajouter → 🧊 Cube, puis Inspecteur → Script (Lua) : `obj.ry = obj.ry + 45 * dt` (c'est mot pour mot le script du cube bleuté de la scène — ouvre son Inspecteur pour le copier ; Test B : aussi `examples/first_game/scripts/rotating_object.lua`) | — | < 5 min |
| 7 | **Play** : WASD/flèches, Espace = saut ; ramasser les 3 pièces ; marcher sur la zone jaune | Ton cube tourne ; la zone devient verte ; chrono figé quand 3/3 pièces | < 3 min |
| 8 | **Stop puis vérifier** : les pièces réapparaissent, ton cube ajouté hors Play est toujours là | Règle : ce qui arrive *pendant* Play est jeté, ce qui est édité *hors* Play persiste | < 1 min |
| 9 | **Sauvegarder + rouvrir** : 💾 Enregistrer sous… → `~/Documents/mon_test.json` ; 📂 Ouvrir… dessus | Scène identique, script compris | < 2 min |
| 10 | **Test B seulement — Exporter en web** : `PLAYER_BUILD=1 ./packaging/build_web.sh`, puis dézipper `target/export/RusteeGear-web.zip` et `python3 -m http.server 8080` dans le dossier ; ouvrir `http://localhost:8080` dans Chrome. **Test A** : ouvre la démo publique <https://lberthod.github.io/rusteegear/> dans Chrome — c'est le résultat de ce même export | Le jeu du hameau tourne dans le navigateur (WebGPU requis) | B : < 10 min (compilation wasm comprise) ; A : < 2 min |

## En cas de pépin

- Chaque erreur devrait nommer le fichier/l'objet fautif et proposer une
  réparation — si un message t'a laissé sans piste, **copie-le** dans le
  formulaire, c'est une donnée précieuse.
- **Aide → 📋 Copier le diagnostic** copie version, commit, OS, GPU et les
  derniers logs : colle ça avec chaque signalement.
- La scène `examples/broken_scene/` est **volontairement cassée** (c'est un
  banc d'essai d'erreurs) — ne pas la signaler.
