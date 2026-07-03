# Audit sprint — moteur, architecture, gameplay

Résumé du travail d'audit et des corrections menées sur ce sprint : refonte de
`SceneObject` en composants optionnels, nouveau mode par manches (« Vagues de
zombies »), et audit gameplay du combat. Sert de journal de bord pour la suite.

## 1. Refonte architecturale — composants optionnels

**Constat de départ** : `SceneObject` accumulait des champs spécifiques à un
sous-ensemble d'objets (joueur pilotable, son, cible d'attaque...), à plat sur
la struct. Chaque nouvel objet d'une scène — décor, ennemi, pièce — traînait
ces champs même sans en avoir besoin. Le risque : plus le moteur grandit, plus
la struct devient ingérable (elle avait dépassé les 35 champs).

**Décision** : migration progressive vers des **composants optionnels**
(`Option<T>`), pas un ECS complet (pas de requêtes génériques par composant,
juste un regroupement logique). Compromis choisi par l'utilisateur plutôt
qu'un ECS complet, jugé disproportionné à ce stade.

| Composant | Champs regroupés | Commit |
|---|---|---|
| `Option<Controller>` | `input`, `gyro`, `move_speed`, `auto_run_speed`, `jump_button`, `jump_height`, `attack_button`, `attack_range`, `attack_cooldown` | `864eaad` |
| `Option<AudioSource>` | `clip`, `autoplay`, `spatial` | `2453ae9` |
| `Option<Combat>` | `attackable`, `is_attack_fx`, `wave` | `cec84e3` |

**Résultat** : `SceneObject` repassé sous les 25 champs plats. Chaque
migration a touché : générateurs de scène (démos), moteur physique
(`Physics::build`/`control`), résolution d'attaque, panneaux de l'éditeur
(checkboxes qui créent/suppriment le composant à la volée), export JSON, et
les tests associés — à chaque fois sans régression (suite de tests verte de
bout en bout).

**Bug trouvé en cours de route** : `#[derive(Default)]` sur `Controller` et
`AiChaser` ne reprenait **pas** les défauts serde (`#[serde(default = "fn")]`)
— un `Controller { ..Default::default() }` écrit en Rust divergeait
silencieusement d'une scène JSON désérialisée sans ces champs (vitesse à 0.0
au lieu de 3.0, etc.). Remplacé par des `impl Default` manuels alignés sur
les défauts serde, verrouillé par
`controller_and_ai_chaser_rust_default_matches_serde_default`.

**Candidats restants, non traités** : le trio matériau
(`metallic`/`roughness`/`emissive`) n'est **pas** un bon candidat — presque
tous les objets en ont besoin, grouper ce qui est déjà universel n'apporte
rien.

## 2. Nouveau mode : « Vagues de zombies » (style Call of Duty Zombies)

Remplace l'ancienne démo « Duel IA » (`ai_duel_demo` → `zombies_demo`,
commit `c5057bd`). Jeu **local contre l'ordinateur, sans réseau** (exigence
explicite de l'utilisateur — pas de multijoueur).

- **3 archétypes** de monstres (`AiChaser`, poursuite active recalculée
  chaque frame — pas une patrouille scriptée à trajectoire fixe) :
  - **Rôdeur** : vitesse 2.6, dégâts 0.8/s — basique.
  - **Coureur** : vitesse 4.6 (> joueur, 4.5) — inévitable en fuite, mais
    dégâts faibles (0.5/s).
  - **Brute** : vitesse 1.8 (facile à distancer), dégâts 2.2/s — punitive
    si acculée.
- **4 manches** progressives (3 → 7 monstres, 22 au total), difficulté
  croissante par densité et mixité des archétypes.
- **Nouveau champ moteur `Combat::wave`** (0 = pas de système de manches,
  scènes existantes inchangées) : les monstres d'une manche future restent
  masqués **et sans corps physique** tant qu'invisibles — sinon leur
  collider bloquerait le joueur comme un mur fantôme.
- **HUD** : indicateur « Vague N/M » + nombre de monstres restants.
- **Signal sonore** (`Sfx::WaveStart`) à chaque nouvelle manche — silencieux
  jusqu'au commit `a59a530`.

### Bug connexe trouvé et corrigé : bouton de fin de partie

Le bouton « Rejouer/Niveau suivant » appelait systématiquement
`next_level()` (bascule vers l'arène de combat, `controller_level`) sur
**toute** victoire, y compris course infinie, tour d'ascension ou zombies —
qui n'ont pas de « niveau suivant » et doivent juste relancer la même scène.
Nouveau champ `AppState::is_leveled_demo` isole ce comportement à la seule
démo contrôleur. Verrouillé par
`only_controller_demo_is_marked_as_leveled`.

## 3. Audit gameplay du combat

### Trouvaille n°1 — aucun temps de recharge sur l'attaque (commit `0a7bf99`)

Maintenir le bouton d'attaque défaisait **instantanément** tout ce qui
entrait en portée, sans le moindre risque — le mode manches, censé être une
expérience de survie tendue, était en pratique sans tension. Corrigé par un
nouveau champ `Controller::attack_cooldown` (0.5 s par défaut) et
`AppState::attack_cooldown_remaining`, qui bloque l'attaque même bouton
maintenu. Verrouillé par
`attack_cooldown_blocks_rapid_refire_but_allows_it_once_expired`.

### Trouvaille n°2 — portée d'attaque disproportionnée (commit `a59a530`)

Même avec la recharge posée, une **simulation automatisée** (bot qui fonce
sur le monstre le plus proche et attaque au rythme du cooldown) ne prenait
**jamais** un seul point de dégâts sur une partie complète. Portée resserrée
de 1.5 m à 0.7 m (arène de combat + zombies).

**Limite structurelle documentée, pas totalement corrigée** : la portée
d'attaque ne peut pas éliminer *tout* risque en duel frontal 1 contre 1 — le
cercle d'attaque (`attack_range + rayon_monstre`) contient toujours la boîte
de morsure du monstre (`≈ rayon_monstre`) dès que `attack_range > 0`, quelle
que soit sa taille. Le vrai risque du jeu vient d'affronter **plusieurs
monstres simultanément** pendant la fenêtre de recharge — ce que le système
de manches accentue déjà par construction (plusieurs monstres actifs par
manche). Verrouillé par un test géométrique déterministe
(`zombies_demo_attack_range_stays_close_to_monster_bite_reach`) plutôt
qu'une resimulation de bot, jugée trop fragile (elle testait autant la
navigation du bot autour des piliers — un obstacle *voulu* pour casser la
poursuite en ligne droite — que l'équilibrage réel).

### Trouvaille n°3 — un swing en zone vidait un groupe entier d'un coup (non commité séparément, cf. dernier commit du sprint)

Piste « tension à l'encerclement » creusée concrètement (pas juste en théorie
cette fois) : construction d'un scénario où un Rôdeur, un Coureur et une
Brute convergent ensemble sur un joueur immobile qui attaque en continu.
Résultat empirique surprenant : **les trois mouraient exactement sur la même
frame**, sans qu'aucun n'ait jamais mordu.

**Cause racine identifiée** : `attack_at` vainquait **toutes** les cibles à
portée en un seul appel (balayage de zone, pas une seule cible). Or le rayon
de mise à mort d'un monstre (`attack_range + son propre rayon`) grandit avec
sa taille — et les archétypes les plus gros (Brute) sont aussi les plus
lents. Ces deux effets se compensent presque exactement, si bien qu'un
groupe qui converge en cercle serré a tendance à entrer dans le rayon de
mise à mort de façon **synchronisée** plutôt qu'échelonnée : au lieu de
créer un risque, l'encerclement se faisait effacer d'un seul coup.

**Corrigé** : `attack_at` ne vainc désormais que **la cible la plus
proche**. Vider un groupe de 3 exige donc 3 coups distincts (donc 3 fenêtres
de recharge), pendant lesquelles les survivants du groupe ont une vraie
chance d'approcher et de mordre. Verrouillé par
`attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing` (test
déterministe sur la fonction, pas une resimulation de bot).

**Limite honnête, non résolue et documentée plutôt que masquée** : même
avec cette correction, rien ne garantit qu'un joueur immobile qui attaque en
continu prendra des dégâts en pratique — sans temps de préparation
(« wind-up ») sur l'attaque, le cercle d'attaque du joueur contient
toujours la boîte de morsure d'un monstre qui approche en ligne droite
(trouvaille n°2), donc gagner la course à l'engagement 1 contre 1 reste
structurellement favorable au joueur *même face à plusieurs monstres*, tant
qu'ils n'arrivent pas dans la même fenêtre de recharge. Un risque
véritablement garanti demanderait une refonte plus profonde (temps de
préparation sur l'attaque, ou pénalité de mouvement pendant le swing) — hors
périmètre de ce sprint.

## 4. Attaque transformée en missile (demande explicite : « comme des missiles »)

L'attaque, jusque-là une résolution instantanée au moment du tir, est
devenue un **missile homing** (`AttackProjectile`) : verrouille la cible la
plus proche à portée au moment du tir, puis vole vers sa position
**courante** (homing, pas une trajectoire figée) à vitesse constante
(`ATTACK_PROJECTILE_SPEED`, 10 m/s) ; l'impact (mise à mort réelle) n'est
résolu qu'à l'arrivée, pas au moment du tir. L'ancre visuelle
(`is_attack_fx`) sert maintenant à deux choses : un petit projectile
lumineux pendant le vol, puis l'éclat d'impact déjà en place (rétrécissement
progressif) une fois la cible atteinte.

**Tentative de vérifier si ceci referme la limite n°2/n°3 (risque garanti en
1 contre 1)** : construction d'un scénario où un monstre fonce vers le
joueur pendant le vol du missile, pour voir s'il peut mordre avant l'impact.
**Résultat, à nouveau honnête** : non, pas de façon fiable — un missile
homing tiré dès l'entrée en portée arrive presque toujours avant qu'un
monstre approchant en ligne droite n'ait atteint sa propre (bien plus
courte) portée de morsure, sauf à rendre le missile déraisonnablement lent
(la démonstration mathématique tient dans le commit). Le missile est donc
une **vraie amélioration de lisibilité et de sensation** (le coup se voit
voyager, pas de disparition instantanée), mais ne remplace pas le levier de
risque déjà identifié (plusieurs monstres à la fois pendant la recharge).
Verrouillé par `attack_is_a_missile_with_travel_time_not_an_instant_hit`,
qui teste précisément ce qui est vrai (vol progressif, homing) sans
prétendre garantir un risque que le mécanisme ne garantit pas.

### Pistes restantes pour la suite

- **Wind-up d'attaque** : introduire un court délai entre l'appui et le
  *lancement* du missile (le joueur reste vulnérable pendant ce temps,
  avant même que le projectile ne parte) serait le levier le plus direct
  pour garantir un vrai risque en 1 contre 1 — le vol du missile seul ne
  suffit pas (cf. section 4).
- Équilibrage vitesse/dégâts des 3 archétypes en jeu réel (pas seulement en
  simulation) : reste à valider en jouant, notamment si le Coureur (plus
  rapide que le joueur) se sent réellement menaçant une fois qu'on ne peut
  plus vider un groupe d'un seul coup.
- Pas de piste réseau/multijoueur envisagée : demande explicite de
  l'utilisateur de rester en local contre l'IA.

## État des tests

68/68 tests passent à l'issue de ce sprint (partis de 52 avant le début des
migrations de composants). Build release + relance manuelle vérifiés à
chaque commit, y compris les tout derniers (`attack_at` cible unique, puis
missile homing).
