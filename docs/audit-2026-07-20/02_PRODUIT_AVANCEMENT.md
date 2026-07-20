# Avancement produit — GDD vs réalité (2026-07-20)

*Photographie au commit `429a764` (2026-07-20) — à re-vérifier après le gel
`v0.1.0-alpha.1` et le premier retour de testeur externe.*

*Deux produits imbriqués : **RusteeGear** (moteur/éditeur 3D from-scratch, winit/wgpu/egui,
export macOS/Android/iOS/web) et **Le Hameau des Braises** (MMORPG-léger coop 2-16 joueurs,
salons jetables, compte Firebase).*

## Rythme

Pic d'activité : 80 commits le 13/07, 62 le 18/07, 71 le 19/07. Développement piloté par
cycles audit → plan de sprint → exécution → ré-audit, avec discipline de tests-preuves.
Plusieurs sessions Claude concurrentes travaillent souvent sur le même dépôt (thème
récurrent des audits — source de frictions réelles, cf. gel bloqué ci-dessous).

## Avancement par grand thème

| Thème | % | Livré | Manquant |
|---|---|---|---|
| **Moteur** | ~90 | PBR par objet, shadow map+PCF, spots, instancing+frustum culling, HDR/bloom, skinning+blending répliqué réseau, rendu zéro-alloc, physique rapier (CCD, couches, character controller), audio kira complet | SSAO, ombres cascadées, particules, IK, WebXR (Phases S/R, délibérément repoussées) |
| **Éditeur** | ~85 | Gizmos multi-objets, hiérarchie DnD, undo/redo, import glTF async, Build & Export 1-clic (DMG/APK/IPA), profiler FPS+GPU, hot-reload, pont `--pilot`, génération IA | Undo sur Inspecteur et import glTF, sous-groupes imbriqués, mode débutant |
| **Gameplay solo** | ~80 | Contrôleur sans script, 3 armes + soin, vie/mort/régén, 4 archétypes créatures, courbe de vagues (4 vagues, 7 chefs), 7 démos jouables | Voir écarts GDD ci-dessous |
| **Réseau/MMORPG** | ~70 | Serveur autoritaire headless, prédiction+réconciliation (auditée à 200 ms), multi-salons, spectateur, frags+assists, 3 classes, 4 modes de manche, contrat du jour, chat+mute, XP+anti-AFK, Firebase | `WeaponPickup` non sync réseau, salons publics/privés, présence en ligne jamais affichée, écran de fin minimal, parité mobile réduite |
| **Assets 3D** | ~90 livré | 482 GLB procéduraux (charte ≤3 teintes), 8 familles, gestionnaire GLB | **Avatar joueur = sphère placeholder** (`fairy_hero` non réintégré), pas de textures/normal maps |
| **Lua** | ~75 | mlua natif + rilua wasm, API objet complète (touch lifecycle, damage/spawn/add_item, HUD), 4 scripts officiels testés natif↔web | Lua **inerte sur wasm32** (limite web assumée) |

**Portage réel** : `.dmg`, `.apk` signé, `.ipa` signé fonctionnent ; démo web WebGPU
déployée et connectée à la prod. Limites web : Lua inerte, pas de musique streaming,
meshes skinnés non affichés.

## Sprints

- **Sprint 9 (clos le 19/07)** : découpage des god-modules — voir [01_ARCHITECTURE_DETTE.md](01_ARCHITECTURE_DETTE.md).
- **Depuis (20/07)** : polish Lua (touch lifecycle, `add_item()`), menu Démos réorganisé,
  ré-export `player_scene.json`, `default_settings.json`.
- **Chantier non commité en cours** : refonte des contrôles (caméra-relatif) — voir
  [06_CHANTIER_EN_COURS.md](06_CHANTIER_EN_COURS.md).
- Roadmaps moteur (Phases A→S) et réseau (sprints 50→117) : quasi tout clos ; ne restent
  ⬜ que les extensions Phase S et la Phase R (WebXR).

## Jalon bloquant : le gel `v0.1.0-alpha.1`

Préparation du premier test développeur externe (`sprint.19matin.md`) : **17/17 livrables
présents** (doctor.sh, QUICKSTART, FIRST_GAME, KNOWN_LIMITATIONS, examples, scénario et
formulaire de test). **Bloqué par** :
1. la resynchro de la scène embarquée (échec de test « Errant 62 ») ;
2. les sessions concurrentes qui écrivent encore dans l'arbre.

C'est l'action qui débloque le plus de valeur au moindre coût.

## Écarts GDD ↔ réalité (résiduels, vérifiés dans les audits 18-19/07)

- 🔴 **Tension n°7 « les deux jeux »** : le contenu riche (hameau fortifié, ménagerie) vit
  dans une démo d'éditeur ; la scène servie a remparts/faune/26 créatures mais le casting
  par archétype (Traqueuse/Meute/Colosse/Furtive) n'y est pas appliqué — créatures
  pilotées par 11 générateurs Lua de patrouille, pas par la grammaire `AiChaser`.
  Report acté (appliquer `Archetype` casserait le contrat de PV verrouillé).
- 🔴 **Jalon « preuve du fun » (vertical slice, GDD §18.6) non atteint** et non suivi dans
  la roadmap — conséquence directe de la tension n°7.
- 🟠 **Avatar = sphère** au lieu de `fairy_hero` (silhouettes de classe §10.3 illisibles).
- 🟡 `favorite_weapon` cité par le GDD comme « déjà persisté » **n'existe pas dans le code**.
- 🟡 Écran de fin de manche minimal (bannière + Rejouer) vs §9.2 (ligne par joueur, XP,
  contrat) ; bannière de vague, palier, marqueur allié hors-écran (§17) manquants.
- 🟡 Audio : allié à terre sans son dédié, éveil de créature absent (rangs 2-3).
- 🟡 Accessibilité (§16.6) quasi non instrumentée (daltonisme, taille HUD, secousses).

✅ Résolus depuis le 18/07 : PV par archétype, scaling de vagues par effectif
(`wave_window()`), config Firebase hors éditeur + bake à l'export, verrou CI décor, CI fmt.

## Priorités produit

La liste priorisée et séquencée fait autorité dans **[07_PLAN_ACTION.md](07_PLAN_ACTION.md)**
(source unique — ne pas maintenir de copie ici). En résumé : gel `v0.1.0-alpha.1`
(Vague 2), puis « preuve du fun » — réunification des deux jeux, `fairy_hero`, écran de
fin (Vague 4), puis parité réseau/mobile et accessibilité (Vague 5).
