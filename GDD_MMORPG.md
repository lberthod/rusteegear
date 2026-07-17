# Le Hameau des Braises — Game Design Document

> **Nature de ce document.** GDD classique et autonome du jeu MMORPG-léger
> tournant sur RusteeGear : concept, univers, boucles de jeu, systèmes,
> game feel, level design, direction artistique. Il donne au jeu une
> identité de *jeu* (nom, ton, fiction), là où
> [GAMEDESIGN_EN_LIGNE.md](GAMEDESIGN_EN_LIGNE.md) et
> [GAMEDESIGN_MMORPG.md](GAMEDESIGN_MMORPG.md) sont des documents
> d'audit/priorisation (quoi construire ensuite, dans quel ordre), et
> [ANALYSE_DESIGN_VISUEL.md](ANALYSE_DESIGN_VISUEL.md) une analyse critique
> ponctuelle (15 juillet) dont les constats sont intégrés ici. En cas de
> conflit sur une valeur d'équilibrage ou un état d'avancement, le code et
> [SPRINT_MMORPG.md](SPRINT_MMORPG.md) font foi — ce GDD décrit la *cible*,
> eux décrivent le *chemin*.
>
> Tout ce qui est décrit ici respecte l'échelle verrouillée au Sprint 50 :
> **salons de 2 à 16 joueurs, pas de monde persistant partagé**. La
> « sensation MMORPG » vient du personnage qui dure (compte Firebase), pas
> d'un monde qui dure.

---

## 0. Résumé exécutif (le GDD en une page)

**Le jeu.** *Le Hameau des Braises* : un coop d'action en ligne (2-16
joueurs, pensé pour 3-4) où un groupe de Veilleurs féeriques défend chaque
« nuit » (une manche de 10-20 min) un village fortifié contre des hordes
attirées par son feu. Le personnage persiste (compte Firebase) ; le salon
est jetable.

**Les trois piliers**, chacun mesurable : **identité** (couleur de braise,
classe lisible à la silhouette, frags/assists individuels), **interdépendance**
(3 classes — Flamme/Feu follet/Foyer — dont la composition change le résultat),
**rendez-vous** (paliers de niveau nommés, contrat du jour, classement).

**Les six décisions structurantes** :
1. Le danger vient du *nombre*, jamais d'un individu (drain 6 s, plafond de
   2 chasseresses, éveil à 9 m) — §5.
2. Toute mécanique est latence-tolérante par construction (états maintenus
   et zones, jamais de timing serré entre clients) — §5.6.
3. Les paliers débloquent des *options*, jamais de la puissance — donc pas
   de matchmaking par niveau — §8.2.
4. Rien n'est livré tant que ce n'est pas perceptible à l'écran ET
   accessible au tactile — §6 et §16.
5. Le serveur est seul juge, sans exception — §5.7.
6. Chaque échelle de temps investie paie, chaque effort a son canal de
   reconnaissance, et jamais par un dark pattern — §15.

**Les trois chantiers de design les plus urgents** (issus des audits et de
l'état réel de la scène, détail au §14) : rendre le danger perceptible
(feedback des dégâts subis), réparer la parité mobile (boutons tactiles
perdus), et **réunifier les deux jeux** — le contenu riche (hameau,
ménagerie de 45 monstres) vit dans une démo d'éditeur que le serveur ne
sert pas, pendant que la carte en ligne ne joue aucune vague et que l'XP
rend les paliers ~100× trop lents (§5.5, §8.3, tension n°7).

---

## 1. Fiche d'identité

| | |
|---|---|
| **Titre de travail** | Le Hameau des Braises |
| **Genre** | Action coopérative en ligne à progression persistante (« MMORPG de poche ») |
| **Joueurs** | 2 à 16 par salon (jouable en solo, pensé pour 3-4) |
| **Plateformes** | macOS (éditeur + jeu), Android, iOS, navigateur (WebGPU) |
| **Session type** | 10-20 minutes (une manche), rejouable immédiatement |
| **Caméra** | 3e personne |
| **Moteur** | RusteeGear (Rust, wgpu — maison, from scratch) |
| **Modèle réseau** | Serveur autoritaire (VPS), prédiction client, ~200 ms tolérés |
| **Persistance** | Compte Firebase : XP, niveau, arme favorite, couleur, contrat du jour |

**Pitch.** Un hameau fortifié tient seul au milieu d'une lande envahie de
créatures. Chaque nuit, les braises du bourg attirent une nouvelle horde.
Vous êtes l'un des Veilleurs — des êtres féeriques liés au feu du village —
et chaque manche est une nuit à tenir, ensemble. Votre personnage, lui,
traverse les nuits : il gagne de l'expérience, débloque ses armes, se fait
un nom au classement.

**Fantasme joueur (le test des trois piliers, cf. GAMEDESIGN_MMORPG.md §2).**
« Mon personnage progresse à chaque manche, il a un rôle que mon groupe
reconnaît, et je reviens demain parce que quelque chose m'attend. »

**Le différenciateur assumé.** Ce jeu n'essaie pas de gagner sur la
profondeur des systèmes (il perdrait contre n'importe quel MMO) mais sur le
**rapport lisibilité/temps** : en 60 secondes on comprend tout, en 15
minutes on a vécu une nuit complète avec un rôle, en 3 soirées on a un
personnage. Chaque décision de ce document doit servir ce rapport.

---

## 2. Univers et ton

### 2.1 Fiction minimale

La fiction existe pour habiller les systèmes, jamais pour les contraindre.
L'analyse du 15 juillet l'a montré : « le jeu ne se raconte pas » était la
faiblesse la moins chère à corriger et la plus structurante — trois faits
suffisent, tout le reste est optionnel :

1. **Le Hameau** — un village médiéval fortifié (assets *Medieval Village
   Pack*), dernier lieu habité de la région. C'est l'arène : ses murs, ses
   ruelles et sa place sont le terrain de jeu, pas un décor lointain (§7).
2. **Les Braises** — le feu communal du hameau. C'est lui qui attire les
   hordes (justification des vagues) et lui qui ranime les Veilleurs entre
   deux nuits (justification du retour à chaque manche malgré la mort).
3. **La Ménagerie** — les créatures de la lande (assets *Ultimate Monsters
   Bundle*, ~45 espèces animées). Elles ne sont pas « le mal » : elles sont
   attirées, par instinct, comme des papillons par la flamme. Ce ton
   légèrement mélancolique autorise un bestiaire varié (bêtes, esprits,
   golems) sans exiger de lore par créature.

**Règle de nommage.** Rien à l'écran ne s'appelle par son nom technique.
« Créature 3 » n'existe pas pour le joueur : chaque espèce déployée porte un
nom de la lande (la Traqueuse, le Colosse, la Meute…, cf. §5.4), chaque mode
un nom de nuit (§4), chaque palier un nom de braise (§8.2). Le lore du jeu
*est* sa nomenclature — coût quasi nul, effet permanent.

### 2.2 Le joueur : un Veilleur

L'avatar **cible** est un être féerique (asset riggé `fairy_hero`) :
silhouette légère, lisible de loin, animations de course/attaque déjà en
place. **État actuel** (commit `119a295`, 17 juillet 2026) : le personnage
central servi est une **sphère**, placeholder assumé en attendant le retour
de `fairy_hero` dans la scène embarquée. Chaque Veilleur
porte une **couleur de braise** personnelle (teinte du matériau, persistée au
compte) — c'est l'identité visuelle minimale : dans un groupe de 4, on se
reconnaît à sa couleur avant de lire les noms. La couleur n'est jamais le
*seul* canal d'une information (accessibilité : toujours doublée du nom ou
d'une icône).

### 2.3 Direction de ton

- **Féerique crépusculaire, pas horrifique** : nuit bleutée, braises
  orangées, brouillard (ciel/brouillard du Sprint 89).
- **Mélancolique, pas grimdark** : les créatures sont attirées, pas
  haineuses ; on tient un feu, on ne purge pas un mal.
- **Aucun texte de lore obligatoire** : tout le récit tient dans le nom des
  choses. Un joueur qui ne lit rien vit la même histoire.

---

## 3. Boucles de jeu

### 3.1 Boucle seconde par seconde (le combat)

Se déplacer, viser, tirer/frapper, esquiver le contact, soigner un allié.
Détail au §5. **Sensation cible** : nerveuse mais indulgente — la mort vient
d'un encerclement qu'on a laissé se former (≈6 s de contact continu à pleine
vie), jamais d'un one-shot.

### 3.2 Boucle de manche (10-20 min) — une « nuit »

```
Fenêtre Multijoueur → choix classe + salon (code) → nuit (vagues/objectif)
→ victoire ou défaite de salon → écran de fin (frags, assists, XP gagnée)
→ rejouer immédiatement (le salon se réinitialise en place)
```

- **Victoire** : l'objectif de la nuit est rempli et au moins un Veilleur
  est debout.
- **Défaite** : tous les Veilleurs sont à terre (la mort individuelle rend
  spectateur, elle ne termine jamais la manche des autres).
- **La défaite paie aussi** : l'XP de participation est créditée même sur
  une nuit perdue (seul le bonus de victoire saute). Un jeu coop qui punit
  la défaite par zéro gain apprend aux joueurs à quitter les manches mal
  parties — l'exact contraire du pilier interdépendance.
- La manche est **jetable**, conformément à la règle « la persistance passe
  par le compte, jamais par le salon ».

### 3.3 Boucle de session (une soirée)

2 à 4 nuits enchaînées, idéalement avec le même groupe (même code de salon).
Le liant : le classement Firebase, le chat de salon, et la montée d'XP
visible entre les manches. **L'écran de fin de manche est une pièce de
design à part entière** (§9.2) : c'est là que la contribution de chacun
devient sociale.

### 3.4 Boucle de retour (jour après jour) — le pilier « rendez-vous »

- **Contrat du jour** (GAMEDESIGN_MMORPG.md §3.5) : un défi global dérivé de
  la date (seed = jour UTC, calculé identiquement par serveur et clients),
  récompensé une fois par compte et par jour. Habillage fiction :
  *« l'Almanach du Hameau »* — chaque jour, une page, un défi.
- **Paliers de niveau** : chaque déblocage nommé (§8.2) est une raison datée
  de relancer (« encore 2 manches et j'ai le Boulet »).

**Catalogue de contrats** (la table dans laquelle le seed du jour pioche —
chaque entrée doit être vérifiable côté serveur avec des compteurs déjà
existants, et jouable en une seule nuit) :

| Contrat | Vérification serveur | Ce qu'il fait jouer |
|---|---|---|
| *Nuit blanche* — gagnez sans qu'aucun Veilleur ne tombe | aucun `PlayerDown` sur la manche gagnée | prudence collective, le Foyer brille |
| *À l'aube juste* — gagnez une Horde en moins de 8 min | horodatage de manche | agressivité, la Flamme brille |
| *La lande garde ses morts* — gagnez sans réanimation | compteur d'assists de réanimation = 0 | tension inversée : le filet est interdit |
| *Main de braise* — 10 frags en mêlée seule | frags par source (contact vs projectile) | prise de risque au contact |
| *Le troupeau compte sur vous* — gagnez une Escorte convoi > 50 % PV | PV du convoi en fin de manche | protection, le Feu follet détourne |
| *Sobriété* — gagnez sans ramasser d'arme au sol | compteur de `WeaponPickup` | fidélité à son arme (et à son palier) |

Règles du catalogue : un contrat n'exige jamais un mode non livré (le
catalogue grandit avec §4), jamais une classe précise (il *favorise* une
classe, il n'exclut personne), et jamais plus d'une nuit de jeu.

### 3.5 La première nuit (onboarding)

Il n'y a ni tutoriel ni écran d'aide, et c'est un choix : **le mode solo est
le tutoriel**. La même carte, les mêmes créatures, les mêmes touches — sans
enjeu réseau. Trois garanties de conception le rendent suffisant :

1. **Tout s'apprend par l'affordance, pas par le texte** (§6.3) : ce qui est
   soignable brille, ce qui est ramassable a un halo, ce qui dort à 9 m est
   visiblement immobile.
2. **La première vague est un échauffement** : peu de créatures, espacées —
   la courbe (§5.5) commence toujours sous le niveau du joueur.
3. **Mourir enseigne** : 6 s de drain au contact avec feedback visible
   (§6.1), c'est assez lent pour que le joueur *comprenne* ce qui le tue —
   la leçon du signalement réel « tout est attiré là-bas très rapidement »,
   dont le diagnostic final était un déficit de perception, pas de physique.

---

## 4. Structure d'une nuit : les modes de manche

Un salon choisit son objectif à la création (`RoundObjective`,
GAMEDESIGN_MMORPG.md §3.4). Même squelette réseau pour tous ; seule la
condition de victoire change.

| Mode | Nom fiction | Règle | État |
|---|---|---|---|
| **Vagues** | *La Horde* | Vider N vagues successives ; la vague suivante n'apparaît qu'une fois la précédente éliminée. | ✅ En jeu (mode actuel) |
| **Survie** | *Tenir jusqu'à l'aube* | Survivre N minutes, respawn des créatures qui s'accélère. | Cible (le plus simple à livrer) |
| **Boss** | *L'Aînée de la lande* | Dernière vague : une créature unique, PV massifs, lente, contact doublé. | Cible |
| **Escorte** | *Le Convoi de braises* | Amener un chariot lent d'une porte du hameau à l'autre ; les créatures le ciblent en priorité. | Cible |

**Règles de conception des modes.**

- Chaque mode doit donner un terrain d'expression distinct aux trois classes
  (§8.1) : La Horde favorise l'Assaut, l'Escorte révèle l'Éclaireur
  (détourner) et le Soutien (maintenir), la Survie récompense la gestion
  d'espace de tout le groupe.
- Chaque mode doit avoir une **fin lisible à tout instant** : compteur de
  vagues, chrono, barre de progression du convoi — le joueur sait toujours
  à quelle distance de l'aube il est. Un mode dont on ne sent pas la fin
  approcher est un mode qu'on quitte en cours.
- Un mode = un sprint. Pas de mode « composite » (survie + escorte) tant que
  les quatre de base ne sont pas éprouvés en conditions réelles.

---

## 5. Combat

### 5.1 Arsenal du Veilleur

- **Mêlée** : attaque à préparation (`attack_windup`), mode Simple ou Zone.
- **Trois armes à distance** (visée dans la direction du regard), valeurs
  réelles de `RANGED_WEAPONS` :

| Arme | Dégâts | Recharge | DPS théorique | Portée | Rayon | Vitesse | Identité |
|---|---|---|---|---|---|---|---|
| **Boule de feu** | 1 | 0,9 s | 1,1 | ~18 m | 0,35 | 12 m/s | l'équilibrée — la référence |
| **Éclair** | 1 | 0,45 s | 2,2 | ~12 m | 0,22 | 20 m/s | la nerveuse — cadence double, mais petite et courte |
| **Boulet** | 3 | 1,8 s | 1,7 | ~16 m | 0,55 | 8 m/s | la lourde — burst (un « chef » à 3 PV tombe d'un coup), gros rayon qui pardonne la visée |

  Lecture d'équilibrage : le triangle est **soutenu / précision / burst**,
  et il ne tient que parce que le DPS *théorique* n'est pas le DPS *réel* —
  l'Éclair a le double de cadence mais le plus petit rayon et la plus
  courte portée : son DPS effectif dépend du taux de coups placés, donc de
  l'habileté et de la distance de combat. Le Boulet ne gagne pas au DPS
  mais à la **létalité par occasion** (un tir = un chef) et à la tolérance
  de visée. Si un playtest montre que l'Éclair domine à toutes les
  distances, la variable d'ajustement est sa *portée* (12 m → moins),
  jamais ses dégâts — le triangle vit sur la géométrie, pas sur les stats.
- **Soin (touche H / bouton tactile)** : maintenu près de l'allié vivant le
  plus blessé (2,5 m, 0,2 PV/s). Universel — tout Veilleur porte un peu de
  braise. Le Soutien (§8.1) le multiplie.
- **Ramassage d'armes** (`WeaponPickup`) : une arme trouvée au sol change le
  profil pour la manche en cours. Éphémère par design : le *drop* est le
  piment de la nuit, le *palier* est l'acquis du compte.

### 5.2 L'économie de la vie (les nombres qui font le jeu)

Les quatre débits de vie forment un système délibéré, à préserver dans leurs
**rapports** plus que dans leurs valeurs absolues (vie normalisée 0..1) :

| Flux | Valeur | /s de vie | Intention |
|---|---|---|---|
| Contact d'une créature | `MONSTER_CONTACT_DPS` | −0,16 | ~6 s pour mourir de pleine vie : une fenêtre de réaction, jamais un one-shot |
| Régénération passive (hors contact) | `REGEN_PER_S` | +0,05 | ~20 s pour tout récupérer seul : possible mais lent |
| Soin d'un allié | `HEAL_RATE_PER_S` | +0,20 | ~5 s pour tout rendre : **4× la régénération** |
| Soin du Soutien (cible §8.1) | | +0,50 | ~2 s : le métier du rôle |

Les rapports à défendre en équilibrage :

- **Soin allié ⩾ 4 × régénération passive** : se faire soigner doit toujours
  être nettement meilleur qu'attendre dans un coin — c'est ce rapport qui
  rend la coopération *rationnelle*, pas seulement sympathique.
- **Soin allié > drain d'un contact** (0,20 > 0,16) : un binôme dont l'un
  soigne pendant que l'autre encaisse *un* monstre tient indéfiniment ; à
  deux monstres au contact (0,32), il perd. C'est exactement le seuil voulu :
  le plafond de 2 chasseresses par cible (§5.4) garantit que ce cas-limite
  est la pire situation « équitable » possible.
- **Mourir (6 s) > tuer** : le temps de mettre à mort un joueur dépasse le
  temps qu'il faut au joueur pour éliminer ou fuir une menace isolée — le
  danger vient toujours du *nombre*, jamais d'un individu.

### 5.3 Vie, mort, et l'expérience du spectateur

- Vie individuelle par joueur, drainée au **contact** (AABB) d'une créature
  visible, régénérée passivement hors contact.
- À 0 PV : **spectateur** pour le reste de la nuit — avatar masqué, entrées
  ignorées, chat conservé. Fiction : la braise du Veilleur retourne au feu
  communal jusqu'à l'aube.
- **La mort d'un allié est un événement de groupe** : `PlayerDown` est
  diffusé à tous — signal visuel + sonore pour chacun, pas seulement pour
  la victime. Un groupe doit *sentir* qu'il perd quelqu'un.

**Le problème du spectateur, traité comme du design et pas comme un détail.**
Une nuit dure 10-20 min ; mourir à la 3e minute ne doit pas signifier 15
minutes de purgatoire, sinon le joueur quitte le salon (et son avatar
fantôme, l'anneau de spawn et le salon en pâtissent). Trois réponses,
en couches :

1. **Caméra de spectateur utile** : suivre un allié vivant au choix (cycle),
   pas un point fixe. Regarder un bon joueur jouer *est* du contenu — et le
   spectateur qui observe la carte devient les yeux du groupe (le chat est
   conservé pour ça).
2. **La réanimation du Soutien** (§8.1) est la vraie réponse systémique :
   tant qu'un Foyer est debout, aucune mort n'est définitive. 10 s de canal
   immobile au milieu d'une horde — *la* décision dramatique du jeu, et la
   raison d'être de la classe.
3. **Garde-fou de rythme** : si les modes longs (Survie) montrent en
   playtest des temps morts de spectateur > 5 min médians malgré la
   réanimation, la réponse sera un retour en jeu à la vague suivante avec
   PV réduits — pas un raccourcissement des manches. À trancher sur données
   (§11), pas préventivement.

### 5.4 Les créatures

L'IA est volontairement simple et lisible ; la variété vient du bestiaire
visuel (ménagerie ~45 espèces, clip Idle en décor, patrouilles) et des
paramètres, pas de comportements exotiques :

- **Poursuite du plus proche** (`AiChaser`), recalculée chaque frame.
- **Au plus 2 chasseresses par cible** (`MAX_ACTIVE_CHASERS_PER_TARGET`) :
  jamais d'encerclement instantané injuste — les autres guettent, visibles,
  menaçantes, immobiles. (Issu d'un signalement en conditions réelles,
  Sprint 85 — pas d'une intuition.)
- **Rayon d'éveil de 9 m** (`CHASER_DETECT_RANGE`) : une créature lointaine
  dort. Le joueur *voit* le danger dormant et choisit de le réveiller ou de
  le contourner — c'est le seul outil de « pull » du jeu, et il suffit.
- Dégâts au contact uniquement (pas de créatures à distance pour l'instant —
  simplicité assumée ; si un archétype distant arrive un jour, il sera un
  paramétrage du même squelette).

**Grammaire d'archétypes** (paramètres sur le même `AiChaser`, pas de
nouveaux systèmes). Chaque archétype doit être identifiable par sa
**silhouette et son gabarit** avant son comportement — à 20 m, de nuit.
Le casting puise dans le pack de monstres réel (`assets/models/monster_*`,
~45 modèles animés) :

| Archétype | Silhouette | Paramètres | Casting (assets réels) | Contre-jeu attendu |
|---|---|---|---|---|
| **Traqueuse** | moyenne | valeurs standard | démon, orc, ninja, tribal, sorcier | tout l'arsenal |
| **Meute** | petites, nombreuses | PV réduits, vitesse + | blobs (vert, rose, épineux), mushnub, glub, birb | attaque de zone, goulets (§7) |
| **Colosse** | massive, lente | PV élevés, contact fort | yéti, dragon, roi-champignon, alpaking | kiting, tir concentré — c'est aussi le boss |
| **Furtive** | fine, basse | éveil réduit (< 9 m) mais vitesse accrue éveillée | fantôme, crâne spectral, squidle, hywirl | vigilance, l'Éclaireur la déclenche de loin |

**Les chefs sont gratuits** : le pack fournit des variantes `_evolved` de
plusieurs espèces (goleling/goleling évolué, armabee, glub, mushnub,
dragon…). Règle de casting : **le chef à 3 PV d'une vague est la variante
évoluée d'une espèce déjà présente dans la vague** — même silhouette en
plus imposant, la hiérarchie du danger se lit sans un pixel d'UI ni un
nouvel asset.

**Règle de casting** : une vague mélange au plus 2 archétypes ; le 3e
n'apparaît que dans les vagues finales. La lisibilité du danger prime sur sa
variété — 45 skins ne veulent pas dire 45 comportements simultanés.

### 5.5 Courbe de difficulté d'une nuit

Le rythme cible d'une nuit est une **dent de scie ascendante**, pas une
rampe : chaque vague monte en intensité puis laisse un creux (fin de vague =
régénération, ramassage, repositionnement) avant la suivante. Règles :

- **Vague 1 sous le niveau du groupe** (échauffement, cf. §3.5) ; dernière
  vague au-dessus (elle doit coûter — victoire in extremis mémorable).
- **Le creux inter-vagues est sacré** : jamais de spawn surprise pendant la
  respiration. C'est le moment social (chat, soin, plan) — le seul moment
  où le Soutien peut réanimer sans héroïsme.
- **La difficulté scale par le *nombre*, pas par les stats** : plus de
  créatures par vague quand le salon compte plus de joueurs, plutôt que des
  PV gonflés — cohérent avec « le danger vient du nombre » (§5.2) et avec
  le plafond de 2 chasseresses (le surnombre étale la menace au lieu de la
  concentrer).

**Constat sur la carte livrée** (mis à jour à l'audit du 17 juillet 2026,
vérifié dans `player_scene.json` et `src/scene/demos.rs`) : la scène
embarquée servie (`use_embedded_scene()`) est resynchronisée depuis
`Scene::mmorpg_demo()` et joue désormais la dent de scie — **26 créatures
attaquables réparties en 4 vagues** (budgets de PV 5 / 8 / 11 / 16,
strictement croissants), avec **7 chefs à 3 PV** dont au moins un dès la
vague 2. **Le Boulet a maintenant les cibles qui le justifient** (« un chef
à 3 PV tombe d'un coup »). Ces règles sont verrouillées par le test
d'authoring `mmorpg_demo_waves_follow_the_gdd_authoring_rules`
(`src/scene/demos.rs`). L'ancien constat (« 20 cibles plates à `wave: 0`,
1 PV ») est résolu ; reste un chantier d'authoring : le **casting §5.4**
(archétypes Traqueuse/Meute/Colosse/Furtive sur le pack `monster_*`, chefs
`_evolved`) n'est pas encore appliqué à la scène servie — les vagues
actuelles sont peuplées par la ménagerie de patrouille (scripts de
déplacement décoratifs), pas par la grammaire d'archétypes de chasse.

**Règles d'authoring des vagues** (les vagues sont de la donnée de scène,
cf. tension n°6) : budget de PV strictement croissant par vague
(actuellement 5 / 8 / 11 / 16), au moins un chef à 3 PV dès la vague 2, la
dernière vague dépasse d'un tiers le budget de l'avant-dernière — c'est
elle qui doit coûter (§11) — et, dès que le casting §5.4 sera appliqué, au
plus 2 archétypes par vague. Précision de vocabulaire (audit du
17 juillet) : un *archétype* est une famille de chasse §5.4
(Traqueuse/Meute/Colosse/Furtive), **pas** un script de patrouille
(wander/soar/zigzag…) — une vague peut mélanger plus de deux scripts de
patrouille décoratifs sans violer la règle de casting.

### 5.6 La latence est une règle du jeu

Le jeu est validé à ~200 ms de latence réelle vers le VPS (tick serveur
60 Hz, prédiction locale, fantômes interpolés avec délai de rendu). Ce
n'est pas un paramètre technique : c'est une **contrainte de conception**
que chaque mécanique existante respecte déjà, et que toute mécanique future
doit respecter :

- **Aucune mécanique ne demande de réagir en < 200 ms à l'état d'un autre
  joueur ou d'une créature.** Le drain au contact (continu, 6 s), la mêlée
  à préparation (windup qui masque l'aller-retour), le soin et la
  réanimation (canaux maintenus) sont tous latence-tolérants par
  construction. Un « parry », une esquive à fenêtre courte ou un tir à
  hitscan réactif seraient injouables à 200 ms — exclus par principe, pas
  par manque d'ambition.
- **Les projectiles sont lents à dessein** (8-20 m/s) : leur temps de vol
  (0,6-2 s) est très supérieur à la latence, donc l'écart entre ce que le
  tireur voit et ce que le serveur résout reste une fraction du trajet —
  le jeu de visée reste équitable sans compensation de lag.
- **Toute nouvelle capacité se décrit comme un état maintenu ou une zone**,
  jamais comme un événement à timing serré entre deux clients.

### 5.7 Règle d'or anti-triche

**Le serveur est seul juge, sans exception** : mouvement, dégâts, soin,
portées, recharges, classes, paliers — tout est résolu ou validé côté
serveur headless. Un client qui ment est corrigé, pas cru. Aucune mécanique
de ce GDD ne peut être implémentée en dérogeant à cette règle.

---

## 6. Game feel et feedback (le chapitre qui manquait)

L'analyse du 15 juillet a identifié le déficit central du jeu actuel : **le
danger est réel mais imperceptible** — le drain de vie au contact est
silencieux (pas de flash, pas de recul, pas de son), alors que les dégâts
*infligés* aux créatures, eux, se voient. Ce chapitre érige les correctifs
en règles permanentes.

### 6.1 Les trois lois du feedback

1. **Tout dégât subi est perceptible sur trois canaux** : écran (flash/
   vignette rouge brève), monde (léger recul ou tressaillement de l'avatar),
   son (blessure). Le joueur ne doit jamais découvrir dans le HUD ce que
   son corps ne lui a pas déjà dit. Symétrique du feedback de dégâts
   infligés, déjà correct.
2. **Toute action contextuelle a un état « à portée »** : l'allié soignable
   a un contour émissif quand on est à 2,5 m, le pickup d'arme un halo au
   sol, la créature endormie une posture visiblement inerte. L'affordance
   remplace le tutoriel (§3.5).
3. **Tout événement de groupe a un signal partagé** : allié à terre
   (`PlayerDown`), palier atteint, vague vidée, contrat rempli — chacun de
   ces moments existe pour tous, visuellement et au son, ou n'existe pas.

### 6.2 Budget de lisibilité

Le bloom, l'émissif et les couleurs saturées sont un **budget**, pas une
décoration : ils sont réservés aux enjeux (§10). Si tout brille, rien ne
brille — chaque nouvel effet doit dire ce qu'il *retire* de l'attention du
joueur, pas seulement ce qu'il y ajoute.

### 6.3 Parité mobile — « definition of done »

Leçon d'une régression réelle (audit du 16 juillet : la scène ré-exportée
avait perdu les boutons tactiles Feu/Arme/Soin — la moitié des mécaniques
inaccessible sur APK, et la caméra de visée avec) :

- **Toute action a une touche ET un bouton tactile, dès son sprint de
  livraison.** La config `mobile.buttons` de la scène fait partie de la
  mécanique, pas de l'habillage. Un test garde-fou le verrouille (c'est lui
  qui a détecté la régression — il doit rester rouge tant que la scène ne
  re-câble pas les boutons).
- **Aucune mécanique n'exige plus de simultanéité que ce qu'un pouce
  permet.** Le mobile n'est pas un portage : c'est la plateforme la plus
  contrainte, donc c'est elle qui fixe le plafond d'inputs.

---

## 7. Level design : le hameau est du gameplay

L'arène historique (24×24 m, quatre murs gris, aucun couvert) ne permettait
pas au gameplay émergent d'exister — le kiting de l'Éclaireur et le
positionnement du Soutien ont besoin de géométrie. Le hameau fortifié la
fournit, à condition de le traiter en level designer et pas en décorateur :

### 7.1 Vocabulaire spatial

- **La place** (autour du feu communal) : l'espace ouvert — visibilité
  maximale, aucune protection. C'est là qu'on se regroupe, et là que la
  Meute est la plus dangereuse.
- **Les ruelles** : goulets à 1-2 créatures de front — le contre-jeu de la
  Meute, le piège face au Colosse (pas la place d'y reculer).
- **Les cours et recoins** : poches défendables à une entrée — refuge du
  soin et de la réanimation, nasse si on s'y attarde.
- **Les remparts/plateformes basses** : verticalité légère — postes de tir,
  chemins d'Éclaireur ; on y échappe au contact mais pas à l'attention des
  créatures (qui attendent en bas : hauteur = répit, pas d'invulnérabilité).

### 7.2 Règles de construction

1. **Tout obstacle est réel pour tout le monde** : chaque élément de décor
   doit être détectable par les sondes des créatures (raycast à 0,6 m de
   hauteur), sinon les patrouilles se figent dessus — un muret « visuel »
   qui bloque le joueur mais pas l'IA est un bug de design, pas un détail
   technique. Le level design se valide en jeu, pas à l'œil.
2. **Aucun point de la carte à plus de ~8 s de course d'un espace ouvert** :
   se faire piéger doit être une erreur de jugement, jamais une fatalité de
   géométrie.
3. **Les spawns de créatures aux lisières, jamais dans le dos** : les
   hordes viennent de la lande (portes, brèches des remparts) — cohérence
   fiction et équité (le danger a toujours une provenance lisible).
4. **L'anneau de spawn joueurs doit tenir 16 positions distinctes** (constat
   d'audit : 8 positions pour 16 joueurs = interpénétrations garanties dès
   le 9e) — un détail technique qui est en fait une promesse de design : le
   scope affiché « jusqu'à 16 » doit être vrai dès la seconde 0.

### 7.3 La vie du hameau (l'atout dormant du bundle)

Le bundle embarque une **faune paisible** encore inexploitée (mouton, cerf,
lapin, poule, écureuil, chouette, luciole, grenouille… — `m40`-`m57`).
C'est le moyen le moins cher de faire exister la fiction « dernier lieu
habité » entre deux vagues :

- **Placement narratif** : moutons et poules dans les cours, la chouette
  sur les remparts, les lucioles autour du feu communal — la luciole est
  littéralement le motif du jeu (une braise qui vole).
- **Règle stricte : la faune est neutre et intouchable.** Ni attaquable, ni
  attaquante, ignorée par le ciblage (`attackable: false`, pas d'`ai_chaser`)
  — elle appartient au registre « vivant neutre » de la charte (§10.1),
  jamais au registre « enjeu ». Un jeu où l'on peut tuer le mouton du
  hameau qu'on défend raconte l'inverse de sa fiction.
- **Budget** : la faune est la première variable d'ajustement du bilan de
  performance (§11) — décorative par définition, elle saute avant tout le
  reste si le pire-frame en combat en a besoin.

---

## 8. Personnage et progression

### 8.1 Les trois classes (choisies avant connexion, rappelées au roster)

| Classe | Fiction | Modificateurs (serveur) | Rôle de groupe |
|---|---|---|---|
| **Assaut** (défaut) | *Flamme* | Valeurs standard, 3 armes à distance | Éteindre la horde |
| **Éclaireur** | *Feu follet* | Vitesse +25 %, saut +30 %, PV max −30 % | Attirer, détourner, activer — kiting rendu viable par le plafond de 2 chasseresses et la géométrie du hameau (§7) |
| **Soutien** | *Foyer* | Vitesse −15 %, dégâts −30 % ; soin ×2,5 (0,5 PV/s, 4 m) ; **seul à réanimer** (10 s de canal, retour à 30 % PV) | Maintenir le groupe debout, effacer les morts |

Principes :

- **Le défaut est l'existant** : l'Assaut reproduit exactement les valeurs
  actuelles — zéro régression pour qui ne choisit pas.
- **Le Soutien multiplie, il ne gate pas** : le soin universel (0,2 PV/s)
  reste pour tous ; retirer une capacité livrée serait une régression
  ressentie. L'exclusivité du Soutien est la *réanimation*, pas le soin.
- **Chaque classe a un moment de gloire scripté par les systèmes, pas par
  un script** : la Flamme vide la dernière vague, le Feu follet emmène le
  Colosse loin du convoi, le Foyer réanime sous pression. Si un mode de
  manche ne produit naturellement aucun de ces moments pour une classe,
  c'est le mode qu'on retouche.

**Mesure de succès de l'interdépendance** : sur une nuit difficile, un
groupe 2 Flammes + 1 Feu follet + 1 Foyer survit là où 4 Flammes échouent.

### 8.2 Progression du compte (Firebase, écrite par le serveur uniquement)

- **XP par contribution réelle** : participation + frags individuels + bonus
  de victoire ; le Soutien crédite des **assists** (PV soignés,
  réanimations) symétriques aux frags. La participation garantit que la
  défaite paie (§3.2) ; les frags/assists garantissent que la contribution
  se voit.
- **Le classement suit la contribution individuelle** (frags + assists), pas
  le score de salon — tranché ici : l'audit du 16 juillet a montré qu'un
  joueur AFK d'une manche gagnante est aujourd'hui classé comme son MVP,
  ce qui contredit frontalement le pilier identité. Le score de salon reste
  un total d'équipe affiché en fin de manche, pas une entrée de classement.
- **Niveau plat et lisible** (`1 + xp / XP_PER_LEVEL`), paliers nommés :

| Palier | Déblocage | Nom fiction |
|---|---|---|
| Niv. 3 | Éclair sans ramassage | *« La foudre répond »* |
| Niv. 6 | Boulet | *« Le poids des braises »* |
| Niv. 10 | 2e emplacement de classe (changer entre deux nuits) | *« Double foyer »* |

- **Les paliers débloquent des options, jamais de la puissance brute** : un
  niveau 10 a plus d'outils qu'un niveau 1, pas plus de PV ni plus de
  dégâts par tir. Deux conséquences voulues : aucun matchmaking par niveau
  n'est nécessaire (un débutant dans un salon de vétérans n'est pas un
  poids mort), et le serveur peut mélanger tous les niveaux sans
  équilibrage dynamique.
- **Ce qui persiste aussi** : arme favorite (restaurée au Join), couleur de
  braise, classement, `last_contract_day`.
- **Plafond assumé** : pas d'arbre de talents, pas d'objets à stats, pas de
  battle pass. Un palier = un déblocage nommé, affiché dans le HUD au
  moment où il tombe. La progression doit tenir sur une carte postale.

### 8.3 L'économie de l'XP — les maths du rendez-vous

**Constat sur l'implémentation actuelle** (à corriger, c'est le point le
plus désaligné du jeu par rapport à son propre design) : l'XP créditée en
fin de manche est le *score de salon*, soit **1 point par monstre vaincu**
— une nuit typique en rapporte ~20 — avec `XP_PER_LEVEL = 1000`. Le premier
palier (niv. 3 = 2 000 XP) demanderait donc **~100 nuits**, soit ~25
soirées ; le niveau 10, plus de 400 nuits. La promesse « encore 2 manches
et j'ai le Boulet » est mathématiquement impossible : la progression existe
techniquement mais est **imperceptible à l'échelle d'une vie de joueur** —
deux ordres de grandeur d'écart entre l'intention et les nombres.

**Économie cible.** On fixe d'abord le *rythme vécu*, puis on en dérive les
valeurs (et non l'inverse) :

| Jalon | Rythme cible | XP cumulée requise |
|---|---|---|
| Niv. 3 — *La foudre répond* | fin de la 2e soirée (~6-8 nuits) | 2 000 |
| Niv. 6 — *Le poids des braises* | ~2 semaines de jeu régulier (~20 nuits) | 5 000 |
| Niv. 10 — *Double foyer* | ~1 mois (~40-50 nuits) — le plafond, atteint sans grind | 9 000 |

D'où le barème par nuit (cohérent avec « XP par contribution », §8.2) :

| Source | XP | Intention |
|---|---|---|
| Participation (nuit terminée, victoire ou non) | 150 | la défaite paie (§3.2) |
| Par frag ou assist | 5 | la contribution se voit (~15 créatures/joueur/nuit ≈ 75) |
| Bonus de victoire | 75 | gagner compte, sans doubler la mise |
| Contrat du jour | 250 | vaut ~une nuit : le rendez-vous a un prix réel |

Nuit moyenne gagnée ≈ **300 XP** → niv. 3 en ~7 nuits, niv. 10 en ~30-45
nuits selon l'assiduité au contrat. Trois propriétés à préserver quelle que
soit la retouche future des valeurs :

1. **La participation domine le frag** (150 vs ~75) : on progresse d'abord
   en *jouant des nuits*, pas en volant des kills — sinon le Soutien et
   l'Éclaireur sont taxés par leur propre rôle. **Garde anti-AFK
   obligatoire** (sinon cette règle crée l'optimum anti-fun que le §15.5
   interdit : rester immobile et encaisser 150 XP par nuit) : la
   participation n'est due qu'à un joueur *actif* — au moins un frag, un
   assist, ou un seuil minimal d'inputs/déplacement sur la manche, vérifié
   côté serveur comme tout le reste. Un AFK gagne 0, pas 150.
2. **Le contrat vaut à peu près une nuit** : assez pour être le prétexte du
   lancement quotidien, pas assez pour devenir une corvée obligatoire.
3. **Le plafond (niv. 10) s'atteint en un mois de jeu plaisir** : après,
   seuls le classement et les contrats restent — c'est le moment où le jeu
   assume d'être fini de progresser, conformément au plafond de rétention
   revendiqué (§12).

---

## 9. Le social à l'échelle du salon

### 9.1 En jeu

- **Salon = code partagé** (2-16 joueurs) : le même code pilote le lobby de
  jeu et le chat Firebase — un seul code à s'échanger.
- **Roster HUD** : nom, couleur, classe, barre de vie, frags/assists,
  marqueur « c'est moi », grisé si spectateur. C'est le support perceptif
  des piliers identité et interdépendance — *aucune mécanique n'existe pour
  le joueur tant qu'elle n'est pas à l'écran*. (Priorité 1 de la feuille de
  route pour cette raison exacte.)
- **Événements partagés** : cf. les trois lois du feedback (§6.1).

### 9.2 L'écran de fin de manche

Le moment social le plus dense du jeu, à concevoir comme tel et pas comme un
tableau de sortie :

- Une ligne par Veilleur : couleur, nom, classe, frags, **assists au même
  rang que les frags** (le Foyer lit sa contribution au même format que la
  Flamme — c'est la moitié de l'attractivité de la classe), XP gagnée.
- Le contrat du jour, s'il vient d'être rempli, s'affiche ici — c'est le
  moment où « reviens demain » se dit.
- Un seul bouton en avant : **Rejouer** (le salon se réinitialise en place).
  La friction entre deux nuits doit être nulle — c'est la boucle de session
  entière (§3.3) qui en dépend.

### 9.3 Ce que le social n'est pas

- **Classement Firebase** : la seule compétition du jeu de base. Le PvP
  n'existe pas par défaut (mode de salon distinct, uniquement si demandé
  explicitement un jour).
- Pas de guildes, pas d'échange d'objets, pas de monnaie — cf. exclusions
  (§12).

---

## 10. Direction artistique

Formalise la charte proposée par ANALYSE_DESIGN_VISUEL.md §3 — le pipeline
(HDR, tonemapping, bloom multi-mip, PBR, émissif, brouillard, ciel dégradé)
est déjà au-dessus de ce que la scène en exploite ; il s'agit d'une
discipline d'usage, pas de nouveaux systèmes de rendu.

### 10.1 La règle unique : froid = décor, chaud/saturé = enjeu

| Registre | Traitement | Exemples |
|---|---|---|
| **Inerte** (murs, sol, lande, ciel) | froid, désaturé, sombre | nuit bleu-noir, pierre grise, brouillard |
| **Vivant neutre** (créatures endormies, PNJ de décor) | palette du décor, teinte propre à l'espèce | la ménagerie s'intègre à la lande (plus jamais de blanc-texture hors palette) |
| **Enjeu** (joueurs, créatures éveillées, projectiles, pickups, le feu communal) | chaud et/ou saturé, émissif autorisé | braises des Veilleurs, boule de feu, halo des pickups, yeux/marques des créatures en chasse |

Corollaires :

- **L'éveil d'une créature se voit dans sa couleur** : endormie elle
  appartient au décor, éveillée elle rejoint le registre « enjeu »
  (marques émissives) — la mécanique du rayon de 9 m devient lisible sans
  un seul élément d'UI.
- **L'émissif est le canal du gameplay** : réservé aux enjeux, jamais
  décoratif — c'est le budget de lisibilité (§6.2) appliqué au rendu.
- **Le ciel travaille** : zénith ≠ horizon (le dégradé existe déjà) ; les
  lueurs chaudes du hameau contre la nuit froide de la lande donnent
  l'orientation gratuite — au centre, les braises ; au loin, le danger.

### 10.2 Une teinte par système (la table vit dans la scène, pas dans une tête)

Reprise de la règle n°4 de l'analyse visuelle — chaque système de jeu
possède sa teinte, et ne la partage avec aucun autre :

| Teinte | Système | Occurrences |
|---|---|---|
| **Orange** | joueur / feu / braises | Veilleurs, feu communal, Boule de feu |
| **Cyan** | mobilité / foudre | zone de vent, Éclair, traînées de l'Éclaireur |
| **Magenta** | menace éveillée | marques émissives des créatures en chasse (absent du décor : réservé) |
| **Vert** | soin / vie | tick de soin, jauge de vie pleine, halo de réanimation |
| **Violet** | objectifs / repères | convoi d'escorte, marqueurs de mode, pickups |

La couleur de braise personnelle (§2.2) module l'orange du Veilleur sans
quitter le registre chaud — l'identité individuelle ne casse jamais la
grammaire collective.

### 10.3 Lisibilité des classes : la silhouette d'abord

À 20 m de nuit, la classe d'un allié doit se lire **sans** le roster. Sur
la base commune cible `fairy_hero` (l'avatar servi est aujourd'hui une
sphère placeholder, cf. §2.2), trois variations à coût d'asset quasi nul
(échelles et attaches, pas de nouveaux rigs) :

- **Flamme** : gabarit de référence, l'arme à distance portée visible.
- **Feu follet** : silhouette élancée (~0,9×), traînée cyan en course —
  la vitesse se voit avant de se mesurer.
- **Foyer** : gabarit légèrement tassé (~1,1× de large), sacoche dorsale
  lumineuse — le point vert du groupe, celui vers qui on rampe.

Complément quasi gratuit hérité de l'analyse visuelle : l'**animation
secondaire procédurale** (squash & stretch au saut/atterrissage,
inclinaison dans les virages — trois interpolations dans la simulation)
donne du corps à tous les Veilleurs sans un clip de plus.

### 10.4 Audio (direction minimale)

Aucun système audio riche n'existe encore ; quand il arrive, l'ordre de
priorité est celui du feedback, pas de l'ambiance : 1) dégâts subis, 2)
allié à terre, 3) éveil de créature proche, 4) tir/impact, 5) fin de vague,
6) ambiance de nuit. Un jeu muet mais aux 5 premiers sons justes est
meilleur qu'une bande-son complète sans eux.

---

## 11. Playtest et mesures (le design se vérifie comme le code)

La méthode projet — tests-preuves, audits en conditions réelles sur le VPS,
tuning sur signalement — s'applique au design. Chaque système livré a sa
question de playtest et son instrument :

| Système | Question | Instrument |
|---|---|---|
| Classes | La composition change-t-elle le résultat ? (2F+1FF+1Fo survit où 4F échouent) | Session VPS 3-4 joueurs, classes imposées, même nuit rejouée |
| Feedback dégâts (§6.1) | Le joueur sait-il ce qui le tue ? | Signalements type « attiré très rapidement » : leur disparition est la mesure |
| Spectateur (§5.3) | Temps mort médian après mort < 5 min ? | Horodatage mort → réanimation/fin, côté serveur |
| Courbe (§5.5) | La dernière vague coûte-t-elle ? | Taux de victoire par vague ; une nuit gagnée sans aucun joueur sous 50 % PV est trop facile |
| Rendez-vous | Les gens reviennent-ils ? | `last_contract_day` : taux de comptes à contrat rempli 2 jours de suite |
| Perf | La ménagerie coûte-t-elle le combat ? | Bilan périodique en jeu (FPS, pire frame, part simulation) — la fluidité du combat prime sur la densité du décor |

Deux garde-fous méthodologiques hérités des audits :

- **Un signalement joueur vaut plus qu'une intuition de designer** — les
  deux meilleurs réglages du jeu (plafond de chasseresses, rayon d'éveil)
  viennent d'un seul retour de terrain diagnostiqué sérieusement.
- **Rien n'est « livré » tant que ce n'est pas perceptible à l'écran ET
  accessible au tactile** (§6.3) — les deux dettes structurelles relevées
  par les audits (backends sans HUD, scène sans boutons) sont le même
  défaut : du design invisible n'est pas du design.

---

## 12. Ce que ce jeu n'est pas (exclusions fermes)

Reconduites de GAMEDESIGN_EN_LIGNE.md §4 et GAMEDESIGN_MMORPG.md §3.7 :

- Pas de monde persistant partagé entre salons (pas de sharding, pas de
  base distribuée) — un salon est une nuit, jetable.
- Pas d'économie joueur-joueur (échange, hôtel des ventes, monnaie).
- Pas de guildes au-delà du chat de salon.
- Pas de PvP par défaut.
- Pas d'arbre de talents, pas d'objets à stats, pas de saison/battle pass —
  le contrat du jour est le plafond de la rétention.
- Pas de matchmaking par niveau — rendu inutile par « les paliers
  débloquent des options, pas de la puissance » (§8.2).

Chaque exclusion protège la promesse centrale : un jeu coop de poche qu'une
personne seule peut comprendre, maintenir et faire évoluer de bout en bout.

---

## 13. Tensions de design connues (à surveiller, pas à résoudre d'avance)

Un GDD honnête nomme ses paris. Ceux-ci sont assumés, avec leur signal
d'alerte :

1. **Le Soutien sera-t-il joué ?** Vitesse et dégâts réduits contre une
   exclusivité (réanimer) qui ne sert que quand tout va mal. Signal : si en
   playtest < 1 nuit sur 4 compte un Foyer volontaire, renforcer la
   *visibilité* de sa contribution (assists à l'écran de fin, §9.2) avant
   de toucher aux chiffres.
2. **Le rayon d'éveil de 9 m contre les grandes cartes.** Réglé pour
   l'arène de 24 m ; dans le hameau (plus vaste, cloisonné), il pourrait
   rendre les nuits trop calmes hors de la place. Signal : des vagues que
   le groupe « oublie » de finir. Réponse probable : rayon par archétype
   (la Furtive l'a déjà), pas d'augmentation globale.
3. **Le contrat du jour, plafond de rétention revendiqué.** Un seul défi
   quotidien global peut lasser vite. Le pari : la boucle de session
   (rejouer avec son groupe) porte la rétention, le contrat n'est que le
   prétexte du premier lancement. Signal contraire : des connexions d'un
   quart d'heure qui remplissent le contrat et repartent — alors le
   problème est la boucle de session, pas le contrat.
4. **16 joueurs est-il un vrai mode ?** Tout le design (rôles, feedback,
   spawns, place du hameau) est pensé à 3-4. À 16, le roster déborde,
   la place sature, les frags se diluent. Tant qu'aucune session réelle à
   16 n'a eu lieu, le « 2-16 » du scope est une capacité technique, pas une
   promesse de design — ne pas équilibrer pour ce cas avant de l'avoir vu.
5. **L'Éclair est-il strictement meilleur ?** DPS théorique double de la
   Boule de feu (§5.1), compensé uniquement par sa géométrie (portée 12 m,
   rayon 0,22). Le triangle tient tant que rater coûte ; si les joueurs
   expérimentés ne posent plus jamais l'Éclair, réduire sa *portée*, pas
   ses dégâts. Signal : la répartition d'`favorite_weapon` par niveau de
   compte (donnée déjà persistée — instrument gratuit).
6. **Les vagues sont de la donnée de scène** (`Combat::wave` par objet),
   pas du code : la courbe de difficulté (§5.5) se règle donc en éditant
   `player_scene.json` — force (itération sans recompiler le gameplay) et
   risque (une ré-export de scène peut casser l'équilibrage aussi
   silencieusement qu'elle a cassé les boutons tactiles). Signal : tout
   ré-export de scène passe par les tests garde-fous *et* une nuit de
   validation jouée.
7. **La scène servie et la scène vitrine divergent** (§5.5) : le contenu
   riche (hameau 72 m, ménagerie de 45 monstres) s'accumule dans
   `Scene::mmorpg_demo()` côté éditeur, pendant que le serveur sert la
   scène embarquée, plus pauvre et actuellement cassée au tactile. Chaque
   sprint « décor » qui enrichit la démo sans toucher la scène servie
   creuse l'écart. Signal : si un joueur en ligne ne peut pas croiser un
   monstre de la ménagerie, le sprint qui l'a ajouté n'a pas livré du
   *jeu*, il a livré de l'éditeur. Réponse : le pipeline d'authoring doit
   converger vers **une seule carte source de vérité** (la scène servie).

---

## 14. État d'avancement (résumé — le détail vit ailleurs)

| Système | État |
|---|---|
| Vie individuelle, spectateur, défaite de salon | ✅ En jeu |
| IA poursuite + plafond 2 chasseresses + rayon d'éveil | ✅ En jeu |
| 3 armes à distance, mêlée, soin universel (H) | ✅ En jeu (⚠️ boutons tactiles perdus par la scène ré-exportée — régression bloquante, cf. AUDIT_GAMEPLAY_2026-07-16.md §2) |
| Multi-salons par code | ✅ Backend (UI de saisie à brancher) |
| Frags individuels diffusés | ✅ Backend (HUD à brancher) |
| Comptes, XP, classement, chat (Firebase) | ✅ En jeu (classement = score de salon, à basculer sur la contribution individuelle, §8.2 ; **économie d'XP ~100× trop lente pour les paliers**, barème cible au §8.3) |
| Décor hameau + ménagerie animée + héroïne fée | ⚠️ Dans la démo d'éditeur (`mmorpg_demo`), **pas dans la scène que le serveur sert** — tension n°7 ; faune paisible encore inutilisée, §7.3 |
| Système de vagues (`Combat::wave`) | ✅ Codé et testé, **mais la carte livrée est toute à `wave: 0`, 1 PV** — aucune vague réelle, pas de chef à 3 PV pour le Boulet (§5.5) |
| Roster HUD, sélection de salon UI | 🔜 Priorité 1 (GAMEDESIGN_MMORPG.md §4) |
| Classes, réanimation Soutien | 🔜 Priorité 2 |
| Paliers, XP par contribution, assists | 🔜 Priorité 3 |
| Modes Survie / Boss / Escorte | 🔜 Priorité 4 |
| Contrat du jour, couleur de braise | 🔜 Priorités 5-6 |
| Feedback dégâts subis, affordances, charte (§6, §10) | 🔜 À planifier — non couvert par la feuille de route actuelle |

La séquence de livraison et ses dépendances sont pilotées par
[GAMEDESIGN_MMORPG.md](GAMEDESIGN_MMORPG.md) §4 et
[SPRINT_MMORPG.md](SPRINT_MMORPG.md) — ce tableau n'est qu'une photographie.

---

## 15. Gamification — l'architecture de la motivation

Les systèmes de récompense des chapitres précédents (XP, paliers, contrats,
frags/assists, classement) ne sont pas une liste : ils forment une
architecture. Ce chapitre la rend explicite, pour que chaque ajout futur
sache **quel besoin il nourrit, à quel horizon il paie, et quel effort il
reconnaît** — et pour interdire ce que la gamification ne doit jamais
devenir ici.

### 15.1 Les trois moteurs (et leur pilier)

Le jeu s'appuie sur les trois besoins classiques de la motivation
intrinsèque, chacun adossé à un pilier existant :

| Moteur | Pilier | Ce qui le nourrit dans le jeu |
|---|---|---|
| **Compétence** — « je progresse et je le sens » | Rendez-vous | maîtrise du triangle d'armes (§5.1), lecture des archétypes, paliers nommés, contrats qui exigent une exécution propre |
| **Autonomie** — « mes choix m'appartiennent » | Identité | choix de classe *avant* de voir celle des autres, arme favorite, couleur de braise, réveiller ou contourner une créature endormie (§5.4) |
| **Relation** — « on l'a fait ensemble » | Interdépendance | soin, réanimation, événements partagés (§6.1), écran de fin (§9.2), le même code de salon d'une soirée à l'autre |

**Règle d'arbitrage** : les récompenses extrinsèques (XP, déblocages) ne
doivent jamais écraser ces moteurs intrinsèques — l'XP *constate* une nuit
bien jouée, elle ne doit pas devenir la raison de jouer. Concrètement :
aucun système ne doit rendre optimal un comportement qui n'est pas
amusant (cf. l'invariant « la participation domine le frag », §8.3, qui
existe précisément pour ça).

### 15.2 Les horizons de récompense (chaque échelle de temps paie)

Un joueur doit trouver quelque chose qui le paie à *chaque* échelle de
temps où il investit. La pile complète, de la seconde au mois :

| Horizon | Ce qui paie | Système |
|---|---|---|
| **La seconde** | un tir placé, un contact esquivé, un tick de soin | feedback 3 canaux (§6.1) |
| **La minute** | une vague vidée, un chef abattu, une réanimation réussie | signal partagé de fin de vague, silhouette `_evolved` qui tombe |
| **La manche (10-20 min)** | victoire, frags/assists, XP, contrat rempli | écran de fin (§9.2) |
| **La soirée (2-4 nuits)** | montée d'XP visible, un cran de classement | roster, classement Firebase |
| **La semaine** | un palier nommé qui tombe | §8.2 — annoncé dans le HUD au moment exact où il tombe |
| **Le mois** | niveau 10 — la panoplie complète | plafond assumé (§8.3) |
| **Sans fin** | classement, contrats quotidiens, la maîtrise elle-même | §3.4, §9.3 |

**Règle de pertinence** : une récompense se déclenche pendant que l'effort
qu'elle paie est encore en mémoire de travail — le palier s'affiche à
l'instant où il tombe (pas au prochain menu), le contrat rempli s'annonce
sur l'écran de fin de la nuit qui l'a rempli, l'assist se crédite au tick
de soin qui a compté. Une récompense différée de plus d'un écran est une
récompense gaspillée.

### 15.3 La comptabilité de l'effort (aucun effort invisible)

Chaque *type* d'effort que le jeu demande doit avoir son canal de
reconnaissance — c'est l'audit systématique à refaire à chaque nouvelle
mécanique :

| Effort demandé | Canal de reconnaissance | État |
|---|---|---|
| Exécution (placer ses tirs) | frags individuels diffusés | ✅ backend |
| Sacrifice de DPS (soigner au lieu de tirer) | assists au même rang que les frags | 🔜 §8.2 |
| Prise de risque (kiter, réanimer sous pression) | réanimation créditée en assist ; le kiting reste sous-compté | ⚠️ trou connu, cf. ci-dessous |
| Constance (revenir chaque jour) | contrat du jour, `last_contract_day` | 🔜 §3.4 |
| Endurance (finir une nuit perdue) | XP de participation — la défaite paie | 🔜 §8.3 |
| Apprentissage (lire les archétypes, le triangle d'armes) | payé en performance, pas en points — c'est voulu (compétence intrinsèque) | ✅ par design |

**Trou assumé** : l'effort de *diversion* (le Feu follet qui promène un
Colosse pendant 40 s loin du groupe) n'a pas de compteur — le mesurer
proprement (temps d'aggro détourné ?) coûterait plus de complexité serveur
que sa reconnaissance ne vaut. On l'assume tant que l'écran de fin montre
la victoire *du groupe* en premier ; si les playtests montrent que les
Feux follets se sentent invisibles, la réponse est un événement ponctuel
(« Diversion ! » quand un Colosse change de cible à > 15 m du groupe), pas
une statistique de plus.

### 15.4 Objets et « qualité » sans rareté chiffrée

Le jeu exclut les objets à stats (§12) — mais la *sensation* de trouvaille,
elle, est voulue. La qualité d'un objet est ici **situationnelle et
spatiale**, jamais numérique :

- **Le drop du chef** : un chef `_evolved` abattu laisse une arme au sol
  garantie — le seul « loot dirigé » du jeu. La valeur de l'objet vient du
  *moment* (on vient de tomber le plus gros danger de la vague) et du
  *lieu* (il faut aller la chercher là où le chef est mort, parfois exposé).
- **Une arme au sol est un choix, pas un chiffre** : prendre l'Éclair
  quand on joue Boulet est un *pari* sur la vague suivante (Meute
  attendue ?), pas une montée de puissance. L'objet de qualité, c'est
  l'objet qui pose une question.
- **Trois états visuels obligatoires** pour tout objet interactif (règle
  UX, §16.3) : au repos (halo violet, §10.2), à portée (intensifié), saisi
  (éclat + son). Un objet qui ne signale pas sa saisissabilité n'existe pas.

### 15.5 Ce que la gamification ne fera jamais ici (anti-dark-patterns)

Verrouillé au même titre que les exclusions du §12 :

- **Pas de FOMO punitif** : un contrat manqué n'enlève rien et ne casse
  aucune « série » — il n'y a pas de compteur de streak. Rater un jour ne
  coûte que ce jour.
- **Pas de temps mort monétisé ni d'attente artificielle** : aucun timer,
  aucune énergie, aucune ressource qui se recharge hors jeu.
- **Pas d'aléatoire de récompense** : le drop du chef est garanti, les
  paliers sont déterministes, le contrat est le même pour tous. La seule
  variance du jeu est dans la partie, jamais dans la paie.
- **Pas d'optimum anti-fun** : si un comportement ennuyeux devient le plus
  rentable (camper un spawn, farmer la vague 1 en boucle), c'est un bug
  d'économie au même titre qu'un exploit — à corriger côté barème, pas à
  interdire côté règles.
- **Pas de comparaison forcée** : le classement est un onglet qu'on ouvre,
  jamais un écran qu'on subit ; l'écran de fin montre le groupe avant les
  lignes individuelles.

---

## 16. Game UX & ergonomie

Le game feel (§6) couvre la perception en combat ; ce chapitre couvre tout
le reste du trajet joueur — du lancement à la fin de soirée — avec des
budgets chiffrés, vérifiables comme le reste.

### 16.1 Time-to-fun et budget de friction

- **< 60 secondes du lancement au premier tir**, mesuré sur APK (la
  plateforme la plus lente). Le chemin critique est : lancement → fenêtre
  Multijoueur (pseudo mémorisé, arme favorite restaurée, classe
  pré-sélectionnée = celle de la dernière fois) → bouton Jouer.
- **Un seul écran entre le joueur et le jeu.** La fenêtre Multijoueur
  concentre tout (compte, salon, classe, contrat du jour) — aucun
  sous-menu obligatoire, jamais. Tout ce qui s'ajoute à cet écran doit
  déloger quelque chose ou se justifier.
- **Reprise après coupure invisible** : la reconnexion automatique existe
  (backoff plafonné) — côté UX, elle doit se présenter comme une pause
  (« Reconnexion… » sur l'écran de jeu), pas comme un retour au menu. Le
  joueur mobile qui reçoit un appel doit retrouver sa nuit, pas la refaire.

### 16.2 La qualité d'une session (le temps du joueur est la ressource rare)

- **La manche est l'unité de consentement** : 10-20 min annoncées, fin
  lisible en permanence (§4) — le jeu ne demande jamais un engagement dont
  la durée est cachée. Pas de sunk cost fabriqué : quitter en cours de
  nuit coûte la nuit, rien d'autre (pas de malus de compte, pas de
  « déserteur »).
- **La soirée a une sortie élégante** : l'écran de fin propose Rejouer en
  premier, mais le classement et le contrat rempli donnent au joueur un
  *point final satisfaisant* — une bonne UX de rétention sait aussi bien
  terminer une session qu'en relancer une. Un joueur qui part content
  revient ; un joueur retenu de force part pour de bon.
- **Le spectateur garde des mains** (§5.3) : caméra à choisir, chat — même
  mort, le joueur a des verbes. Zéro minute de jeu sans aucun verbe : c'est
  la définition d'une session de qualité.

### 16.3 Hiérarchie de l'information (les trois anneaux du HUD)

Du plus fréquent au plus rare, du centre vers les bords — chaque
information vit dans l'anneau de sa fréquence de consultation :

| Anneau | Information | Position | Fréquence |
|---|---|---|---|
| **Corps** (centre, périphérie proche) | vie propre (vignette), réticule, état de l'arme (recharge) | autour du personnage / réticule | continue, en vision périphérique |
| **Groupe** | roster (vie/classe/état des alliés), événements (`PlayerDown`, vague) | coin d'écran + bannières brèves | toutes les ~10 s |
| **Méta** | frags/assists, contrat, palier, code de salon | bords, ou visibles à la demande | entre les vagues |

Corollaires : rien de l'anneau méta ne s'anime pendant le combat (un
compteur qui clignote vole l'attention au danger) ; les bannières
d'événement durent < 2 s et ne se superposent jamais au réticule ; la vie
propre est lisible **sans regarder** (vignette + son, §6.1), la jauge
n'est que la confirmation.

### 16.4 Ergonomie tactile (le mobile fixe la barre)

Prolonge la parité mobile (§6.3) avec les contraintes physiques :

- **Zones des pouces** : joystick à gauche, actions (Saut/Feu/Arme/Soin) en
  arc sous le pouce droit — rien d'interactif dans le tiers supérieur de
  l'écran (zone du regard, pas des mains), rien sous les encoches
  (`safe_area`, prévu par la config de scène, aujourd'hui à `false`).
- **Cibles ≥ 9 mm** (~48 px à densité standard), espacées d'au moins une
  demi-cible — un raté de bouton en combat est une faute de l'interface,
  pas du joueur.
- **Le maintien est le geste de base** (soin, réanimation — états
  maintenus, §5.6) : compatible pouce posé. Aucun double-tap, aucun geste
  composé — un pouce, une action.
- **La barre de vie tactile existe dans la config de scène**
  (`health_bar`, aujourd'hui `false`) : à activer avec le HUD — sur mobile
  la vignette de dégâts compte double, l'écran est petit et souvent en
  plein soleil.

### 16.5 L'erreur est une leçon, jamais un mystère

- **Chaque mort est diagnosticable** : l'écran de spectateur affiche ce qui
  a tué (« Encerclé — 2 Traqueuses ») — la donnée existe côté serveur (qui
  draine, combien). Un joueur qui comprend sa mort la corrige ; un joueur
  qui ne la comprend pas désinstalle.
- **Les erreurs d'interface sont récupérables** : quitter un salon,
  changer de classe entre deux nuits, retaper un code — aucune action de
  menu n'est irréversible, donc aucune ne mérite de confirmation modale
  (les modales sont réservées à ce qui coûte : quitter une nuit en cours).
- **Les échecs réseau parlent le langage du joueur** : « Salon introuvable
  — vérifiez le code », jamais un code d'erreur. Le rejet applicatif
  (`JoinRejected`) existe côté protocole ; côté UX chaque cas de rejet a
  sa phrase et sa porte de sortie (retaper / réessayer / jouer solo).

### 16.6 Accessibilité (minimum viable, non négociable)

- Couleur jamais seule (déjà posé, §2.2) — s'applique aussi aux teintes
  système (§10.2) : la Furtive éveillée se lit aussi à sa posture.
- **Texte HUD ≥ 12 pt équivalent mobile**, contraste AA sur les fonds de
  nuit — la charte sombre (§10.1) rend le contraste facile à rater.
- **Pas de flash plein écran répété** : la vignette de dégâts est un
  assombrissement bref des bords, pas un stroboscope rouge.
- Les options d'accessibilité (taille du HUD, réduction des secousses)
  sont des réglages de la fenêtre Multijoueur, pas un menu caché.

### 16.7 Mesures UX (s'ajoutent au tableau du §11)

| Question | Instrument |
|---|---|
| Time-to-fun < 60 s sur APK ? | chrono lancement → premier tir, mesuré à chaque release |
| Un joueur neuf comprend-il sa première mort ? | playtest : « qu'est-ce qui vous a tué ? » — réponse correcte attendue dès la 2e mort |
| Les boutons tactiles sont-ils ratés en combat ? | taux de taps hors cible dans les zones d'action (mesurable côté client) |
| La reconnexion est-elle vécue comme une pause ? | signalements « j'ai perdu ma partie » après coupure — leur disparition est la mesure |

---

## 17. Catalogue des surfaces d'interface (l'inventaire complet des overlays)

Ce chapitre recense **toutes** les surfaces d'interface du jeu — celles qui
existent, celles à construire, celles qu'on refuse — chacune mappée sur ses
fondations réelles dans le code. C'est la spécification de référence : une
surface qui n'est pas dans ce catalogue ne s'ajoute pas sans y entrer, et
chaque surface vit dans l'anneau du §16.3 qui correspond à sa fréquence.

Fondations déjà en place, vérifiées : `HudLayout` réserve **six
emplacements** de widgets (`crosshair`, `weapon_hud`, `kills`,
`weapon_inventory`, `item_inventory`, `roster` — tous à (0,0) aujourd'hui,
aucun placé) ; Firebase couvre chat de salon (`list/post_chat_message`),
classement (`get_top_leaderboard`), **présence en ligne** (`set_presence`,
`list_online_players`) ; le réseau expose `multiplayer_roster()`,
`displayed_kill_count()`, `connect_to_lobby`, `JoinRejected` et les
`GameEvent`.

### 17.1 En jeu — permanents (toujours visibles)

| Surface | Contenu | Anneau | Fondations | État |
|---|---|---|---|---|
| **Réticule** | point de visée, direction du regard | corps | `hud_layout.crosshair` | slot prévu, à placer |
| **Vie propre** | vignette de dégâts (primaire) + jauge (confirmation) ; sur mobile, `health_bar` de la config de scène | corps | `network_health`, config `mobile.health_bar` (à activer) | à construire |
| **Arme équipée** | icône/nom de l'arme, état de recharge (le cooldown de 0,45-1,8 s doit se *voir*) | corps | `hud_layout.weapon_hud`, `selected_weapon` | slot prévu, à placer |
| **Sélecteur d'armes** | les 3 armes, l'équipée en avant, les non débloquées grisées avec leur palier (« Niv. 6 ») — la progression s'affiche là où elle manque | corps (bord) | `hud_layout.weapon_inventory`, `RANGED_WEAPONS` | slot prévu, à placer |
| **Sacoche** (inventaire léger) | objets de manche ramassés (`item_pickup`, 7 posés dans la scène) — jamais plus d'une rangée ; ce jeu n'a pas d'écran d'inventaire, la sacoche EST l'inventaire | corps (bord) | `hud_layout.item_inventory`, `ItemPickup` | slot prévu, à placer |
| **Roster de groupe** | par allié : couleur, nom, classe, barre de vie, état (vivant/à terre/spectateur) ; « c'est moi » marqué | groupe | `hud_layout.roster`, `multiplayer_roster()` | backend prêt — priorité 1 |
| **Frags/assists** | compteur personnel discret ; le détail complet attend l'écran de fin | méta | `hud_layout.kills`, `displayed_kill_count()` | backend prêt |
| **Progression de nuit** | vague N/M, chrono de Survie ou PV du convoi selon le mode — la « fin lisible en permanence » du §4 | méta | `Combat::wave`, `max_wave()` | à construire |

### 17.2 En jeu — contextuels (apparaissent, servent, disparaissent)

| Surface | Déclencheur | Contenu et règles | Fondations |
|---|---|---|---|
| **Bannière de vague** | vague vidée / suivante | « Vague 3 — l'aube approche » ; < 2 s, jamais sur le réticule (§16.3) | `GameEvent`, sfx `WaveStart` existant |
| **Allié à terre** | `PlayerDown` | bannière + **marqueur hors-écran** (flèche de bord vers le corps) — indispensable au Foyer pour trouver qui réanimer ; persiste tant que l'allié est à terre | `GameEvent::PlayerDown` diffusé |
| **Palier atteint** | niveau franchi en fin de manche | « *La foudre répond* — Éclair débloqué » ; règle de pertinence §15.2 : à l'instant du gain | `PlayerProgress` |
| **Contrat rempli** | condition du contrat validée | tampon sur l'écran de fin (§9.2), rappel discret en jeu au moment précis où la condition passe au vert | `last_contract_day` (à créer) |
| **Écran spectateur** | mort du joueur | désaturation, diagnostic de mort (« Encerclé — 2 Traqueuses », §16.5), caméra-allié cyclable, chat ouvert, état de la réanimation (« Foyer à 12 m ») | `network_health` à 0, entrées libérées |
| **Reconnexion** | coupure réseau | voile « Reconnexion… » par-dessus le jeu figé — jamais un retour au menu (§16.1) | backoff + watchdog existants |
| **Soin actif** | touche H maintenue à portée | lien visuel soigneur→soigné + tick (vert, §10.2) — le canal doit se voir des *deux* côtés | `update_network_heal` |

### 17.3 Hors combat — la fenêtre Multijoueur et ses onglets

Un seul conteneur (règle « un seul écran », §16.1), quatre onglets — le
premier est le seul obligatoire :

| Onglet | Contenu | Fondations | État |
|---|---|---|---|
| **Jouer** | pseudo/compte, code de salon (un seul champ pilote lobby réseau ET chat Firebase), choix de classe (3 boutons, dernière choisie mémorisée), contrat du jour, bouton Jouer | `connect_to_lobby`, `mp_lobby_code`, auth Firebase | connexion OK, salon/classe à brancher |
| **Salon** (chat + présence) | messagerie du salon (canal unique — pas de MP au lancement, cf. §17.5) + **joueurs en ligne** : qui est connecté maintenant, dans quel salon — c'est l'écran qui transforme « un code à partager » en « rejoins-moi » | `list/post_chat_message`, `set_presence`, `list_online_players` | backend complet, UI à faire |
| **Classement** | top global (contribution individuelle, §8.2) + sa propre ligne toujours visible même hors du top — un classement où on ne se trouve pas est un classement pour les autres | `get_top_leaderboard` | backend prêt |
| **Veilleur** | niveau + XP vers le prochain palier nommé, arme favorite, couleur de braise, options d'accessibilité (§16.6) | `PlayerProgress`, `favorite_weapon` | partiel |

### 17.4 L'écran de fin de manche

Déjà spécifié §9.2 — rappelé ici car c'est une surface à part entière :
groupe d'abord (victoire/défaite, total), lignes individuelles ensuite
(assists au rang des frags), contrat, XP animée vers le prochain palier,
un bouton Rejouer. C'est la seule surface où les chiffres ont le droit de
prendre toute la place — le combat est fini.

### 17.5 Les surfaces qu'on refuse (et pourquoi)

| Surface refusée | Pourquoi | L'alternative diégétique |
|---|---|---|
| **Minimap / carte d'écran** | la carte servie est compacte et le danger est toujours *proche* (éveil 9 m) ; une minimap vole l'anneau corps en permanence pour une info d'anneau méta | le feu communal visible de partout (orientation), les **portes qui s'embrasent** à l'arrivée d'une vague (provenance), les marqueurs hors-écran ponctuels (§17.2) |
| **Écran d'inventaire** | rien à gérer : pas d'objets à stats (§12) — un écran de gestion sans décisions est une taxe de temps | la sacoche en une rangée (§17.1), le sélecteur d'armes |
| **Journal de quêtes** | un seul contrat par jour, connu avant de jouer | le contrat affiché sur l'onglet Jouer et rappelé à l'écran de fin |
| **Messagerie privée / amis** | modération et complexité sociale hors de portée de l'échelle (cf. §12 : pas de persistance sociale au-delà du chat de salon) | la présence en ligne (§17.3) + le code de salon partagé hors du jeu — les amitiés vivent où elles vivent déjà |
| **Fil de dégâts (combat log)** | culture MMO chiffrée que ce jeu refuse (§8.2 : pas de stats d'objets → rien à théorycrafter) | le diagnostic de mort (§16.5), lisible par un humain |
| **Notifications push / hors jeu** | le rendez-vous doit rester une envie, pas une sonnerie (anti-FOMO, §15.5) | le contrat du jour se découvre en lançant le jeu |

### 17.6 Règles transverses du catalogue

1. **Chaque surface a un propriétaire de données serveur** : aucun overlay
   n'affiche un état que le serveur ne valide pas (§5.7) — le roster lit le
   snapshot, jamais une estimation locale.
2. **Les six slots de `HudLayout` sont le contrat** : un widget en jeu qui
   n'a pas de slot n'existe pas ; en créer un septième est une décision de
   design (ce catalogue), pas un patch d'affichage.
3. **Toute surface se replie** : le HUD complet doit pouvoir se masquer
   (captures, spectacle) et chaque panneau egui se ferme d'un geste — une
   surface qu'on ne peut pas fermer est une publicité.
4. **Le tactile d'abord, toujours** (§6.3, §16.4) : chaque surface de ce
   catalogue est conçue au format APK en premier — si elle tient sur un
   écran de téléphone au pouce, elle tiendra partout.

---

## 18. Public, production et garde-fous (ce qui manquait pour boucler)

Chapitre issu de l'audit du GDD lui-même : les sujets qu'un game design
« pertinent » doit trancher et que les 17 chapitres précédents laissaient
implicites.

### 18.1 Pour qui (public et positionnement)

- **Joueur primaire** : un petit groupe d'ami·es (2-4) qui veut une session
  coop *complète* en 15 minutes, multi-plateformes sans friction (l'un sur
  APK, l'autre sur navigateur, le troisième sur Mac — même serveur, même
  partie). Le comparable d'expérience n'est pas un MMO : c'est la partie
  rapide entre ami·es façon coop à vagues, avec un personnage qui dure en
  plus.
- **Joueur secondaire** : le curieux solo (démo web publique, un clic —
  c'est le canal d'acquisition réel du projet) — d'où le poids mis sur le
  time-to-fun (§16.1) et la première nuit sans tutoriel (§3.5).
- **Non-public assumé** : le joueur de MMO à la recherche de profondeur de
  build, d'économie ou de contenu de guilde — les exclusions du §12 le
  disent déjà ; le dire *en positif* évite de mal vendre le jeu.

### 18.2 Contrôles (la table de référence)

| Action | Desktop / Web | Mobile (APK/iOS) | Manette (défauts, remappables) | Note |
|---|---|---|---|---|
| Déplacement | WASD | joystick virtuel gauche | stick gauche (zone morte 15 %) | |
| Saut | Espace | bouton « Saut » | A / South | |
| Attaque mêlée | J | bouton « Attaque » | X / West | windup §5.1 |
| Tir | (regard) + touche de tir | bouton « Feu » | B / East | direction du regard (`aim_yaw`) |
| Changer d'arme | K | bouton « Arme » | R1 (bumper droit) | cycle les débloquées |
| Soin | H (maintenu) | bouton « Soin » (maintenu) | Y / North (maintenu) | canal §5.6 |
| Caméra / visée | souris | glisser hors joystick | stick droit : horizontal = visée (cumulé au stick gauche), vertical = tangage caméra | |
| Menu / fenêtre Multijoueur | Échap | bouton UI | Start (bascule) | |
| Masquer le HUD | — | — | Select (bascule) — l'alerte vitale reste affichée | |

La manette (gilrs, disposition Xbox — couvre les Logitech F310/F710 et
consorts) est **entièrement remappable** dans le panneau « 🎮 Manette » des
paramètres : les seize boutons standard (façade, L1/L2/R1/R2, Select/Start,
sticks cliqués, D-pad) sont assignables, un nom stable par bouton persisté
en JSON. Règle : les *défauts* suivent la disposition Xbox par position
(pas d'étiquette de fabricant), les gâchettes analogiques sont lues comme
des boutons (aucune mécanique n'exige de demi-pression, cohérent §5.6).

Cette table est le **contrat de parité** (§6.3) : une colonne vide = une
mécanique non livrée. (État actuel : la colonne mobile est cassée —
seul « Saut » subsiste dans la scène exportée, régression bloquante ; la
colonne manette est complète.)

### 18.3 Modèle économique et licence (pourquoi les anti-dark-patterns tiennent)

Le jeu est **gratuit, open source (MIT), sans aucune monétisation** — ni
achats, ni pub, ni compte payant. Ce n'est pas un détail : c'est ce qui
rend le §15.5 *structurellement* crédible (aucune pression de revenu ne
poussera jamais à monétiser l'attente ou la rareté). En contrepartie, le
budget « contenu » est le temps d'une seule personne : chaque système de ce
GDD est dimensionné pour être maintenu en solo — c'est la vraie contrainte
économique du projet, et elle explique chaque exclusion du §12.

### 18.4 Sécurité sociale et anti-abus (un jeu en ligne a des devoirs)

État vérifié : l'écriture du chat exige un compte (`auth != null`), le
transport a un rate-limit (déconnexion au-delà d'un budget de trames/s),
pseudo et code de salon sont validés côté serveur. Il manque, par ordre
d'urgence :

1. **Mute local** : masquer les messages d'un joueur pour soi — côté
   client, coût minimal, désamorce 90 % des incidents d'un chat de salon
   entre inconnus. Prérequis avant toute mise en avant publique du chat.
2. **Longueur et cadence des messages** plafonnées côté serveur (le
   rate-limit réseau ne couvre pas le spam Firebase).
3. **Pas de modération de contenu automatisée** (hors de portée solo,
   assumé) — en compensation : le chat reste *de salon* (on y est entre
   gens qui ont partagé un code, pas dans un canal mondial), et le pseudo
   est le seul contenu libre visible hors salon (classement) — il doit
   donc passer par la même validation de charset que le reste.
4. **Griefing gameplay** : impossible par construction (pas de dégâts
   entre joueurs, pas de collision punitive, pas d'objets volables) — le
   seul vecteur restant est l'AFK, traité au §8.3. À re-vérifier à chaque
   mécanique nouvelle : « un joueur hostile peut-il s'en servir contre son
   groupe ? » entre dans la definition of done.

### 18.5 Le cas solo et le sous-effectif (le jeu aux bornes)

Le design vise 3-4 joueurs ; les bornes doivent rester jouables :

- **Seul dans le salon** (le cas réel le plus fréquent aujourd'hui) : pas
  de réanimation possible → la mort est une défaite immédiate, assumée —
  le solo est le mode nerveux. Le scaling par le nombre (§5.5) donne à un
  joueur seul des vagues réduites ; le Foyer en solo est un choix
  volontairement sous-optimal (pas interdit : l'autonomie prime, §15.1).
- **À deux** : la première configuration où chaque classe a du sens — le
  duo Flamme+Foyer est le tutoriel naturel de l'interdépendance.
- **Départ en cours de nuit** : les vagues déjà lancées ne se réduisent
  pas (pas de re-scaling à chaud — exploitable et illisible) ; la nuit
  devient plus dure, c'est la règle connue de tous les coops.

### 18.6 Le jalon « preuve du fun » (vertical slice)

Avant d'étendre quoi que ce soit, une seule build doit prouver le jeu — la
**Nuit de référence** : la scène servie unifiée (tension n°7 résolue),
3 vagues + 1 chef `_evolved`, les 3 classes jouables, le roster et la vie
à l'écran, les boutons tactiles réparés, le feedback de dégâts subis
(§6.1), l'écran de fin. Critère de réussite unique, hérité du §11 : une
session VPS à 3-4 joueurs où la composition du groupe change le résultat
**et** où chacun peut raconter sa nuit sans regarder les chiffres. Tout ce
qui n'est pas dans cette liste (contrats, paliers, Escorte, couleurs) vient
*après* la preuve.

### 18.7 Langue et gouvernance du document

- **Langue** : le jeu parle français (noms fiction, UI, ce document) ; la
  démo web publique visant large, les textes d'UI passent par une table de
  chaînes dès le premier HUD — traduire plus tard ne doit pas exiger de
  retoucher du code. Les noms propres (Le Hameau des Braises, les classes)
  ne se traduisent pas.
- **Gouvernance** : ce GDD décrit la cible ; il n'est modifié que quand une
  *décision* change (pas à chaque livraison — l'avancement vit dans
  SPRINT_MMORPG.md et le tableau §14 n'est qu'une photographie datée).
  Toute contradiction découverte entre ce document et le code est soit un
  bug de code, soit une décision à acter ici — jamais un écart qu'on
  laisse vivre. Les valeurs chiffrées citées (§5.2, §5.1, §8.3) sont des
  *intentions calibrées* : le code reste la source de vérité de la valeur
  du jour, ce document celle du *rapport* à préserver.
