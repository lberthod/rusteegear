# RusteeGear — Analyse : game design, interactions, charte graphique, personnages

> Analyse critique menée le 15 juillet 2026 sur l'état réel du jeu :
> `assets/player_scene.json` (carte embarquée), `src/gfx/renderer.rs`
> (pipeline de rendu), `src/app/*` (interactions), et les documents
> [GAMEDESIGN_EN_LIGNE.md](GAMEDESIGN_EN_LIGNE.md) /
> [GAMEDESIGN_MMORPG.md](GAMEDESIGN_MMORPG.md). Complète ces derniers sur un
> angle qu'ils ne couvrent pas : **l'identité visuelle et la lisibilité** du
> jeu, pas seulement ses mécaniques.

---

## 1. Game design — ce que la boucle raconte (et ne raconte pas)

**Forces.**
- La boucle « vagues → manche → XP » est complète et fermée : combat (mêlée
  à préparation + 3 armes à distance), danger réel (chasseurs multi-cibles,
  drain de vie au contact, ~6 s pour mourir), entraide (soin H), trace
  persistante (XP/frags Firebase). C'est rare d'avoir une boucle *entière*
  qui tourne à ce stade d'un moteur from scratch.
- L'équilibrage a déjà été confronté au réel (Sprints 85-86 : plafond de
  2 chasseurs/cible, portée de détection 9 m) — le tuning se fait sur
  signalement joueur, pas sur intuition.

**Faiblesses.**
- **Le jeu ne se raconte pas.** Aucun thème : « Créature 1-5 » poursuit
  « Joueur » dans une boîte de 24×24 m aux murs gris. Qui est-on ? Pourquoi
  ces créatures attaquent-elles ? Même trois lignes de fiction (un lieu, une
  menace, un but) donneraient une direction à toutes les décisions
  visuelles ci-dessous. C'est la décision la moins chère et la plus
  structurante de ce document.
- **L'arène est un rectangle vide.** 4 murs, 4 repères, une zone de vent :
  aucun couvert, aucun goulet, aucune verticalité. Or le gameplay émergent
  (kiting de l'Éclaireur, positionnement du Soutien, §3.2 de
  GAMEDESIGN_MMORPG.md) a besoin de géométrie pour exister. Piliers,
  plateformes basses et une zone dangereuse suffisent — pas besoin de
  level design complexe.
- **Le feedback de dégâts est asymétrique** : le joueur voit la vie des
  monstres baisser (impacts), mais son propre drain de vie au contact est
  silencieux (pas de flash écran, pas de recul, pas de son de blessure).
  Le danger est réel mais **imperceptible** — c'est probablement ce qui a
  produit le signalement « tout est attiré là-bas très rapidement » : le
  joueur ne comprenait pas ce qui le tuait.

## 2. Interactions — inventaire et lisibilité

**État réel.** Desktop : WASD + Espace (saut), J/K (attaque/arme), H (soin),
tir dans la direction du regard. Mobile : joystick virtuel + bouton « Saut »
(config `mobile` de la scène : pas de d-pad, pas de barre de vie tactile).
Web : mêmes touches, même serveur.

**Constats.**
- **Le mobile est en retard d'une génération de gameplay** : la carte
  embarquée n'expose que « Saut » alors que le jeu a désormais tir, changement
  d'arme et soin. Un joueur APK ne peut littéralement pas soigner. La config
  `mobile.buttons` doit suivre chaque nouvelle action — à intégrer au
  « definition of done » d'un sprint gameplay.
- **`hud_layout` existe mais tout est à (0,0)** : crosshair, kills, roster,
  weapon_hud ont des emplacements prévus et aucun placement réel. Cohérent
  avec la dette d'affichage déjà identifiée (GAMEDESIGN_MMORPG.md §3.1) —
  la priorité n°1 y reste la bonne.
- **Aucune affordance sur les interactions de proximité** : le soin (2,5 m)
  et le ramassage d'arme n'ont ni surbrillance de cible, ni indicateur de
  portée. Règle simple à adopter : *toute action contextuelle a un état
  visuel « à portée »* (contour émissif de l'allié soignable, halo au sol
  du pickup).

## 3. Charte graphique — état des lieux et proposition

**Ce que le moteur sait déjà faire** (pipeline vérifié dans `renderer.rs`) :
HDR + tonemapping, bloom multi-mip (intensité 0,6 sur la carte), ombres
portées, éclairage PBR (metallic/roughness), émissif par objet, brouillard et
ciel dégradé (Sprint 89), lumières ponctuelles/spots. **La palette d'outils
est au-dessus de ce que la direction artistique en exploite.**

**Palette actuelle de la carte embarquée** (RGB linéaire) :

| Élément | Couleur | Lecture |
|---|---|---|
| Ciel/brouillard | 0.07, 0.08, 0.10 | nuit bleu-noir uniforme (zénith = horizon) |
| Sol | 0.20, 0.28, 0.24 | vert-gris sombre |
| Murs | 0.30, 0.32, 0.38 | gris bleuté |
| Joueur | 0.95, 0.60, 0.25 | **orange chaud — seul point focal** |
| Repères | 0.50, 0.45, 0.62 | violet terne |
| Zone de vent | 0.25, 0.75, 0.85 | cyan |
| Boule de feu / Éclair / Boulet | orange vif / bleu clair / gris-violet | bien différenciés |
| Créatures | 1.0, 1.0, 1.0 (texture GLB) | **hors palette** |

**Diagnostic.** Il y a un embryon de charte cohérent — fond froid désaturé
(nuit bleue), acteurs chauds/saturés (joueur orange, feu) — mais il est
**accidentel et incomplet** : le ciel est plat (zénith = horizon = brouillard,
alors que le dégradé existe), les créatures blanches ne répondent à aucune
règle, et l'émissif (le meilleur allié du bloom déjà actif) n'est utilisé
nulle part dans la scène.

**Proposition de charte (formalise et pousse l'existant, zéro nouveau
système de rendu).**
- **Règle n°1 — froid = décor, chaud/saturé = enjeu.** Tout ce qui est
  inerte reste désaturé et froid ; tout ce qui agit ou peut être touché est
  saturé : joueurs (orange et couleurs de compte, §3.6 de
  GAMEDESIGN_MMORPG.md), projectiles, pickups, créatures.
- **Règle n°2 — le danger est émissif.** Yeux/marquages émissifs sur les
  créatures (rouge-magenta, absent de la palette actuelle donc réservé),
  projectiles émissifs, zone de vent émissive pulsée. Le bloom (déjà à 0,6)
  transforme ça en signalétique gratuite, lisible même sur petit écran APK.
- **Règle n°3 — un vrai ciel.** Zénith ≠ horizon (ex. zénith 0.05/0.06/0.10,
  horizon 0.12/0.09/0.14 — pointe de magenta au ras du sol) : la silhouette
  orange du joueur et les émissifs s'y détachent, et l'arène gagne une
  ambiance « nuit d'un autre monde » compatible avec la fiction à écrire (§1).
- **Règle n°4 — une teinte par système** : orange = joueur/feu, cyan =
  mobilité/vent/éclair, magenta = menace, vert = soin (à introduire avec le
  feedback du soin, aujourd'hui invisible), violet = objectifs/repères.
  Cette table doit vivre dans la scène, pas dans la tête de l'auteur.

## 4. Style de personnage

**État.** Le joueur est une **capsule orange** (mesh `Capsule`, roughness
0,6, aucun émissif, aucune animation). Les créatures sont des **GLB importés
et animés** (clip « Idle », déambulation Lua) — les monstres ont donc *plus*
d'identité visuelle que les joueurs, inversion du standard du genre où
l'avatar est le premier support d'identification (pilier « Identité » de
GAMEDESIGN_MMORPG.md §2).

**Recommandation — assumer le minimalisme géométrique, pas le combler.**
Le moteur a le skinning (pipeline skinned vérifié dans le renderer) mais
produire et maintenir des humanoïdes animés est un chantier d'asset,
pas de code. Une direction cohérente avec « from scratch, comprenable de
bout en bout » :
1. **Capsule + attributs** : une visière/bande émissive à la couleur du
   compte (persistance déjà prévue), l'arme portée visible (les meshes des
   3 armes existent comme pickups — les attacher au flanc de la capsule),
   une silhouette par classe (Assaut trapu ×1,1 ; Éclaireur élancé ×0,9 ;
   Soutien avec un « sac » cubique dorsal). Identité + lisibilité de classe
   à distance, sans un seul nouveau clip d'animation.
2. **L'animation passe par le squash & stretch procédural** (échelle au
   saut/atterrissage, inclinaison dans les virages — trois interpolations
   dans la simulation) plutôt que par du skinning d'avatar.
3. **Uniformiser les créatures dans la charte** : teinte de base désaturée
   froide + émissif magenta (yeux/dos). Le blanc pur actuel casse la règle
   n°1 et écrase la hiérarchie visuelle.

## 5. HUD / UI

- L'UI egui de l'éditeur est fonctionnelle mais le jeu n'a pas encore de
  **langage HUD** : quand le roster/frags/paliers arriveront (§3.1/§3.3 de
  GAMEDESIGN_MMORPG.md), fixer d'emblée 2-3 conventions — jauges = couleur
  du système concerné (vie en dégradé vert→rouge, soin vert), typographie
  unique, coins arrondis constants — évite un HUD patchwork accumulé sprint
  après sprint.
- **Priorité feedback** (moins cher qu'un panneau, plus urgent) : flash
  rouge bref en vignette quand le joueur prend des dégâts, tick sonore/visuel
  quand le soin agit, désaturation de l'écran en mode spectateur. Les trois
  s'appuient sur des états déjà diffusés par le serveur.

## 6. Synthèse — par où commencer

Dans l'ordre de levier décroissant, chaque item étant un petit sprint :

1. **Feedback de dégâts/soin/spectateur** (§1, §5) — le manque le plus
   dommageable en jeu réel aujourd'hui, et le moins coûteux.
2. **Parité mobile des actions** (§2) — un joueur APK doit pouvoir tirer,
   changer d'arme et soigner ; à verrouiller dans le process de sprint.
3. **Charte : ciel dégradé + émissifs menace/créatures** (§3, règles 2-3) —
   exploite bloom/émissif déjà en place, transforme l'ambiance à coût quasi
   nul.
4. **Trois lignes de fiction** (§1) — cadre toutes les décisions suivantes,
   y compris les noms (« Créature 3 » → un nom qui raconte).
5. **Capsule + attributs de classe** (§4) — à synchroniser avec le sprint
   classes (GAMEDESIGN_MMORPG.md §3.2).
6. **Géométrie d'arène** (piliers/couverts) (§1) — donne un terrain
   d'expression aux rôles avant même leur arrivée.
