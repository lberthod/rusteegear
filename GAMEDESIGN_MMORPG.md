# RusteeGear — Game design MMORPG (extension)

> Suite de [GAMEDESIGN_EN_LIGNE.md](GAMEDESIGN_EN_LIGNE.md), rédigée après
> les Sprints 80-89. Le document précédent a rempli sa mission : ses axes
> bloquants (§3.1 vie individualisée, §3.2 IA multi-cibles, §3.3 multi-salons,
> §3.6 soin coopératif) sont **faits et testés**. Ce document définit la
> couche suivante : ce qui transforme un shooter coop à vagues fonctionnel en
> un jeu à **sensation MMORPG** — identité de personnage, rôles, progression
> qui compte, raisons de revenir — sans jamais sortir de l'échelle verrouillée
> au Sprint 50 (salons 2-16 joueurs, pas de monde persistant partagé).
>
> Règles de design inchangées (GAMEDESIGN_EN_LIGNE.md §2) : serveur seul
> juge, coopératif d'abord, petites itérations testables. S'y ajoute ici :
> **la persistance passe par le compte Firebase, jamais par le salon** — un
> salon reste jetable, c'est le personnage qui dure.

---

## 1. Point de départ (acquis vérifiés)

Hérité et fonctionnel (voir GAMEDESIGN_EN_LIGNE.md §1 et SPRINT_MMORPG.md) :
vie individuelle par joueur + mode spectateur, IA qui poursuit le plus proche
(plafonnée à 2 chasseurs/cible, portée de détection 9 m), soin coop (touche H),
multi-salons par code de lobby, compteur de frags individuel diffusé à tous,
comptes/XP/chat/classement Firebase, prédiction client validée à ~200 ms réels.

Dettes d'affichage connues, prêtes côté backend : HUD roster
(`multiplayer_roster()`), compteur de frags (`displayed_kill_count()`),
sélection de salon (`connect_to_lobby`). Elles sont intégrées au plan §4 —
un game design n'existe que si le joueur le **voit**.

---

## 2. Le fantasme visé (pitch en une phrase)

« Mon personnage progresse à chaque manche, il a un rôle que mon groupe
reconnaît, et je reviens demain parce que quelque chose m'attend. »

Trois piliers, chacun mesurable :

1. **Identité** — je suis reconnaissable (nom, classe, apparence) et mes
   choix de build changent ma façon de jouer.
2. **Interdépendance** — la composition du groupe change le résultat d'une
   manche ; jouer à 4 rôles complémentaires bat 4 clones.
3. **Rendez-vous** — chaque session laisse une trace persistante (XP,
   déblocages, contrat du jour) qui donne une raison de relancer.

---

## 3. Axes de design, par ordre de priorité

### 3.1 Rendre visible ce qui existe déjà — HUD roster, frags, salon

**Problème.** Trois systèmes finis côté serveur sont invisibles en jeu. Tant
qu'un joueur ne voit ni la vie de ses alliés, ni les frags, ni dans quel
salon il joue, les piliers 1 et 2 n'ont aucun support perceptif.

**Proposition.**
- Panneau egui simple (liste au coin de l'écran, pas de projection 3D dans
  un premier temps) branché sur `multiplayer_roster()` : nom, barre de vie,
  frags, marqueur « c'est moi », grisé si spectateur.
- Champ « code de salon » dans la fenêtre Multijoueur, branché sur
  `connect_to_lobby` (même convention que `mp_lobby_code` du chat Firebase —
  idéalement le **même** champ pilote les deux, un seul code à partager).
- Événement visuel/sonore sur `GameEvent::PlayerDown` pour tous (aujourd'hui
  soi-même seulement) : un groupe doit savoir qu'un allié est tombé.

**Pourquoi en premier.** Coût minimal (backends prêts, plus de collision de
session sur `editor/mod.rs`), gain maximal : c'est le prérequis perceptif de
tout le reste. Aucune nouvelle mécanique ne devrait être ajoutée avant que
les mécaniques existantes soient lisibles à l'écran.

### 3.2 Classes légères — le choix qui crée l'interdépendance

Reprend GAMEDESIGN_EN_LIGNE.md §3.5 (non traité), précisé après le Sprint 80 :

- **3 classes au `Join`** (`ClientMsg::Join::class: u8`, défaut 0,
  rétrocompatible), appliquées par `spawn_network_player` :
  - **Assaut** (0, défaut) : valeurs actuelles — 3 armes à distance, mêlée
    normale. Le comportement d'aujourd'hui, donc zéro régression.
  - **Éclaireur** (1) : `move_speed` +25 %, saut +30 %, PV max −30 %.
    Son rôle émergera avec les objectifs de manche (§3.4) : activer,
    porter, attirer les chasseurs (kiting rendu viable par
    `MAX_ACTIVE_CHASERS_PER_TARGET`).
  - **Soutien** (2) : `move_speed` −15 %, dégâts −30 %, mais le soin (déjà
    universel, touche H) passe à 0,5 PV/s et 4 m pour lui, et lui seul
    obtient la **réanimation** (10 s de canal sur un spectateur, le ramène
    à 30 % PV). La réanimation — écartée au Sprint 80 — devient l'exclusivité
    qui justifie la classe.
- Le soin universel actuel **reste** (0,2 PV/s, 2,5 m) : le Soutien est un
  multiplicateur, pas un gate. Retirer une capacité déjà livrée serait une
  régression ressentie.
- UI de sélection : 3 boutons dans la fenêtre Multijoueur avant connexion
  (pas d'écran dédié) ; la classe est rappelée dans le roster (§3.1).
- Serveur seul juge, comme toujours : les modificateurs sont appliqués côté
  serveur au clone du gabarit, un client qui envoie `class: 1` avec la
  vitesse de l'Assaut est simplement corrigé par la validation existante.

**Mesure de succès** : sur une manche difficile, un groupe 1 Soutien +
2 Assaut + 1 Éclaireur doit survivre là où 4 Assaut échouent.

### 3.3 Progression persistante à paliers — l'XP qui change le jeu

Reprend GAMEDESIGN_EN_LIGNE.md §3.8, étendu :

- **Paliers de déblocage par niveau** (niveau déjà calculé, `1 + xp /
  XP_PER_LEVEL`) : niv. 3 → Éclair sans ramassage, niv. 6 → Boulet, niv. 10
  → 2e emplacement de classe (changer de classe entre deux manches sans
  reconnexion). Vérifiés **côté serveur** à partir du `PlayerProgress`
  Firebase (le serveur lit la progression au `Join` — jamais la parole du
  client).
- **`favorite_weapon` persistée** par compte, restaurée au `Join`.
- **XP par contribution réelle** : le compteur de frags individuel (déjà
  diffusé) remplace la division artificielle du score de salon dans
  `award_progress` — XP = base de participation + bonus par frag + bonus de
  victoire de salon. Le Soutien ne fragge pas : les PV soignés/réanimations
  créditent un compteur `assists` équivalent (même mécanique
  qu'`network_kills`, nouvelle brique symétrique).
- Rester plat et lisible : pas de stats d'objets, pas d'arbre de talents.
  Un palier = un déblocage nommé, affiché dans le HUD au moment où il tombe.

### 3.4 Variété de manches — objectifs au-delà de « videz les vagues »

**Problème.** Un seul mode de boucle (vagues linéaires) : les classes et la
coopération n'ont qu'un seul terrain d'expression, et la rétention (pilier 3)
s'épuise vite.

**Proposition (un `RoundObjective` par salon, choisi à la création du lobby).**
- **Vagues** (défaut) : mode actuel, inchangé.
- **Escorte** : un objet/PNJ lent à amener d'un point A à un point B pendant
  que les chasseurs affluent — réutilise `AiChaser` en le faisant cibler
  l'escorte en priorité ; l'Éclaireur détourne, le Soutien maintient.
- **Survie chronométrée** : tenir N minutes, respawn de monstres accéléré au
  fil du temps (`respawn` à délai déjà paramétrable) — le mode le plus simple
  à implémenter, bon premier candidat.
- **Boss de fin de manche** : un `AiChaser` à PV élevés + vitesse réduite +
  dégâts de contact doublés à la dernière vague — pas de nouveau système
  d'IA, des paramètres extrêmes sur l'existant.
- Chaque mode = même squelette réseau (`Snapshot`/`EntityDelta` inchangés),
  seule la condition de victoire de `Room` change. Un mode = un sprint.

### 3.5 Le rendez-vous quotidien — contrat du jour

**Problème.** Rien ne différencie aujourd'hui de demain : aucune raison
datée de relancer le jeu.

**Proposition (délibérément minimale).**
- Un **contrat du jour** global, dérivé de la date (seed = jour UTC, calculé
  identiquement par serveur et clients, zéro nouvelle infra) : « Survie en
  moins de 8 min », « Gagnez une Escorte sans aucun joueur à terre », etc.
- Récompense : bonus d'XP fixe, crédité une fois par compte et par jour
  (un champ `last_contract_day` dans `PlayerProgress` Firebase suffit,
  écrit par le serveur comme le reste).
- Affiché dans la fenêtre Multijoueur avant de rejoindre — c'est aussi un
  argument pour choisir son mode de manche (§3.4).

### 3.6 Identité visible — apparence minimale

- Une **couleur de personnage** par joueur, choisie à la connexion,
  persistée dans `PlayerProgress`, appliquée au matériau du clone par
  `spawn_network_player` et rappelée dans le roster. Un champ `u32` RGB,
  pas un système de cosmétiques.
- Optionnel plus tard : teinte débloquée par palier de niveau (§3.3) —
  premier cosmétique « gagné », coût quasi nul.

### 3.7 Ce qu'on ne fait toujours pas

Reconduit de GAMEDESIGN_EN_LIGNE.md §4, inchangé : pas de monde persistant
partagé, pas d'économie joueur-joueur, pas de guildes, pas de PvP par défaut
(§3.7 de l'ancien document reste la référence si demandé un jour). S'y
ajoute : pas d'arbre de talents, pas d'objets à stats, pas de saison/battle
pass — le contrat du jour (§3.5) est le plafond assumé de la rétention.

---

## 4. Priorisation pour SPRINT_MMORPG.md (sprints 90+)

Chaque ligne = 1-2 sprints testables (unitaires + bout-en-bout sockets) :

1. **§3.1 Rendre visible l'existant** — roster HUD, frags, sélection de
   salon, PlayerDown pour tous. Aucune dépendance, dette à solder d'abord.
2. **§3.2 Classes légères** — protocole (`class`), modificateurs serveur,
   UI de sélection, réanimation Soutien. Dépend de §3.1 (roster pour
   afficher la classe).
3. **§3.3 Progression à paliers** — paliers serveur, `favorite_weapon`,
   XP par contribution + `assists`. Dépend de §3.2 pour les assists Soutien.
4. **§3.4 Variété de manches** — Survie d'abord (le plus simple), puis
   Boss, puis Escorte. Indépendant, parallélisable avec §3.3.
5. **§3.5 Contrat du jour** — dépend de §3.4 (les contrats référencent les
   modes).
6. **§3.6 Couleur de personnage** — indépendant, sprint tampon idéal en cas
   de collision de session sur les gros fichiers.

Validation de bout de chaîne (à faire une fois §3.1-§3.3 livrés) : une
session réelle à 3-4 joueurs sur le VPS, classes mixtes, avec la question
unique « est-ce que la composition du groupe a changé le résultat ? » — le
même type d'audit en conditions réelles qui a produit les correctifs
d'équilibrage des Sprints 85-86.
