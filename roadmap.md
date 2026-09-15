# Roadmap — Rivière & Cascade (water.loicberthod.ch)

> Compilation d'une session d'analyse (2026-09-15) portant sur le mode
> **Rivière & Cascade** uniquement. Ce document ne modifie aucun code — c'est
> une liste d'actions à trier/prioriser/implémenter plus tard. Référence de
> départ : commit `0c30a8e` (2026-09-15 11:48:34 +0200).

## 0. Décision de cadrage (actée)

- **Le Hameau des Braises est gelé** : plus de nouveau contenu/temps investi
  dessus pour l'instant. Priorité exclusive à Rivière & Cascade.
- Le gel n'implique **pas** de retirer du code : Hameau et Rivière partagent
  le même crate/binaire/build web, routés par `?scene=riviere` côté client
  ([packaging/web/index.html:102](packaging/web/index.html:102)) et par code
  de salon côté serveur (`RIVIERE_LOBBY` vs `DEFAULT_LOBBY`,
  [src/bin/server.rs:394-401](src/bin/server.rs:394)). Les séparer serait un
  gros chantier non justifié à ce stade.
- Vigilance à maintenir : le lien public partagé doit **toujours** porter
  `?scene=riviere` ; `DEFAULT_LOBBY`/`Room::new()` (→ Hameau) ne doivent
  jamais être exposés en avant sur le site.
- Optionnel, non fait : si on veut renforcer ce gel, on pourrait retirer/
  masquer tout lien visible vers le Hameau dans l'écran d'accueil web, ou
  ajouter un garde explicite empêchant d'atteindre `DEFAULT_LOBBY` sans code
  de salon volontaire. À trancher plus tard.

## 1. Chantiers de code en cours (état constaté pendant la session)

Le dépôt a évolué en parallèle de cette conversation (travail réalisé
ailleurs pendant la session) — état à re-vérifier avant d'agir dessus :

- ✅ **Déjà committé** (`e33ef81`, `0c30a8e`) : le correctif de perf
  scans-linéaires (`boss_idx_cache`, `find_named_object_cached`,
  [src/app/simulation.rs:856](src/app/simulation.rs:856)) + le correctif de
  chevauchement HUD des barres de boss + un banc d'essai headless
  (`examples/bench_riviere_growth.rs`, apparemment pas encore ajouté au repo
  — fichier non suivi (`??`) au moment de la rédaction).
- 🔄 **En cours, non committé** au moment de la rédaction : recyclage des
  « fantômes » de joueurs réseau déconnectés
  ([src/app/network_client.rs](src/app/network_client.rs), `ghost_free_list`)
  — évite que `scene.objects` grossisse sans limite sur un salon qui encaisse
  des connexions/déconnexions pendant des heures. Bien ciblé et cohérent avec
  le pattern de cache validé déjà utilisé pour le correctif boss (même
  philosophie : ne change jamais *quel* objet est trouvé, juste *comment*).
  Plus 3 petits fixes `id_salt` sur des `ScrollArea` de la fenêtre
  Multijoueur ([src/editor/windows.rs](src/editor/windows.rs)).
- **Action avant de committer quoi que ce soit** : lancer `cargo test` (≈900
  tests) sur l'état courant, vérifier que `bench_riviere_growth.rs` est
  intentionnel avant de l'ajouter au suivi git.

## 2. Bug UX trouvé en testant le site en production (à corriger)

**Constat** (testé en direct sur `https://water.loicberthod.ch/?scene=riviere`
le 2026-09-15) : la fenêtre de connexion **en jeu** (juste avant de cliquer
« Jouer ») affiche un indice de contrôles **faux** pour Rivière & Cascade :
*« J : attaquer · K : tirer »* au lieu des vrais contrôles (*1/J : mêlée,
2 : bouclier, 3/K : sort, 4 : ruée*).

- Cause : deux fonctions de libellés distinctes dans
  [src/app/locale.rs](src/app/locale.rs) —
  - `controls_hint()` ([locale.rs:489](src/app/locale.rs:489)) : **générique,
    jamais mis à jour pour Rivière**, appelée dans la fenêtre de connexion
    ([src/editor/windows.rs:3074](src/editor/windows.rs:3074)).
  - `ability_slot_names()` ([locale.rs:470](src/app/locale.rs:470)) :
    **correcte** (Mêlée/Bouclier/Sort/Ruée/Soin), mais utilisée ailleurs (la
    barre de capacités), pas dans cette fenêtre.
- Le correctif du 14 septembre (mentionné dans `docs/RIVIERE_CASCADE.md`)
  n'a corrigé que le texte du **boot HTML** (avant chargement du jeu,
  [packaging/web/index.html:100-105](packaging/web/index.html:100)) — pas ce
  second texte, plus tardif et plus visible puisqu'affiché au moment exact où
  le joueur va cliquer « Jouer ».
- **Fix suggéré** : rendre `controls_hint()` sensible à la scène courante
  (comme `ability_slot_names`), ou remplacer directement l'appel de
  [windows.rs:3074](src/editor/windows.rs:3074) par un texte dérivé
  d'`ability_slot_names()` quand `world == WorldKind::Riviere`. Fix trivial,
  faible risque.

## 3. Risques de déploiement identifiés

- Aucune CI pour le site public (`water.loicberthod.ch`) — tout le pipeline
  (`packaging/build_web.sh` → `rsync` → VPS/Caddy) est manuel/scripté en
  local, jamais vérifié automatiquement avant mise en ligne.
- **Piège déjà vécu deux fois** : un build concurrent produit un `.wasm`
  tronqué (~15 Mo au lieu de ~37 Mo) qui charge quand même, avec une scène
  réduite à des cubes — aucun check automatique de taille dans
  `build_web.sh`.
- Git LFS oublié sur le VPS → joueurs invisibles entre eux en prod (déjà
  arrivé, incident documenté).
- Rollback à un seul niveau (`dist.bak`), pas d'historique de versions
  déployées, **pas de healthcheck automatique** post-déploiement.
- Deux remotes GitHub à synchroniser à la main (`rusteegear` +
  `water.loicberthod`) → risque de désync doc/code publié.
- `MAX_ROOMS=16`, `MAX_TOTAL_CONNECTIONS=256`, `MAX_CONNECTIONS_PER_IP=4`
  ([src/bin/server.rs:48](src/bin/server.rs:48),
  [src/net/server_loop.rs:70,78](src/net/server_loop.rs:70)) : aucune alerte
  de capacité si le serveur sature — un pic de charge refuse juste les
  nouveaux joueurs, sans notifier l'opérateur.

## 4. Optimisation perf — mesures réelles et pistes

**Mesure en direct (2026-09-15)** : téléchargement du `.wasm` mesuré à
**~22 secondes** sur le test effectué (au-delà des 10-20 s estimés dans la
doc) — confirme que la taille du bundle est un vrai frein au premier lancement,
pas juste un chiffre théorique.

- Bundle `.wasm` ≈ 36-37 Mo, **tout embarqué** (assets Rivière inclus,
  ~12 Mo), aucun lazy-loading/streaming.
- Pas de LOD dynamique par distance caméra (seulement des variantes `_lod`
  placées manuellement pour les arbres) — le goulot mesuré est le surdessin
  de feuillage (~60 % du temps de frame, `cargo run --example bench_riviere`).
- **`RenderQuality` (Low/Medium/High,
  [src/app/build_config.rs:38](src/app/build_config.rs:38)) existe dans le
  moteur mais n'est réglable qu'à la compilation** (panneau Export de
  l'éditeur, [src/editor/export.rs:429](src/editor/export.rs:429)) — jamais
  exposé au joueur. Le build web déployé tourne donc en `Medium` fixe pour
  tout le monde, desktop haut de gamme comme mobile bas de gamme.
- **Vérifié pendant la session : le plumbing pour un réglage joueur existe
  déjà.** `Settings` ([src/app/settings.rs](src/app/settings.rs)) a déjà un
  pattern identique pour audio/accessibilité dans
  `settings_player_sections()`
  ([src/editor/windows.rs:1016](src/editor/windows.rs:1016)) : slider →
  `actions.xxx = Some(...)` → propagé en direct à `AppState` + persisté via
  `settings.save()`. Ajouter une section « Qualité graphique » au même
  endroit suit un patron déjà éprouvé — pas une idée en l'air.
- Pas de compression de texture GPU (pas de BC7/ASTC) — risque surtout pour
  un ciblage mobile/Android sérieux, moins critique pour le web desktop
  actuel.
- Réseau : snapshots ~60 Hz, format `EntityDelta` compact (`bincode`),
  conception raisonnable. Pas de prédiction client (limite assumée) — la
  latence se voit sur ses propres déplacements.

### Priorisation perf

**Quick wins :**
1. ✅ Fait (94240cc) — check de taille automatique dans `build_web.sh`
   (échec si `.wasm` < 30 Mo) — élimine un bug déjà vécu deux fois.
2. ✅ Fait (94240cc) — `RenderQuality` exposé comme réglage joueur dans les
   Paramètres web (`settings_player_sections`) au lieu d'être figé à la
   compilation.
3. ✅ Fait (0145e27, déployé en prod le 15 septembre 2026) — détection
   `navigator.maxTouchPoints` (web-sys, feature `Navigator`) dans
   `default_render_quality()` (`src/app/settings.rs`) pour pré-sélectionner
   `Low` sur un appareil tactile détecté dès le premier rendu, plutôt que
   `Medium` partout. Testé (`cargo test --lib settings`, 25 tests) et vérifié
   en prod (`curl` : build hash + taille wasm identiques au build local).

**Moyen terme :**
4. CI minimale : build `build_web.sh` + vérification de taille du `.wasm`
   sur chaque PR touchant les assets/scène Rivière.
5. Étendre le LOD manuel (arbres `_lod`) à un vrai culling par distance pour
   l'herbe/fougères denses (même goulot de surdessin que le hameau, déjà
   documenté dans `docs/optimisation3D.Analys.md`).
6. Chargement différé d'une partie des assets (faune/butin secondaires)
   plutôt que tout embarquer dans le `.wasm` initial.

**Long terme :**
7. Compression de texture GPU si un ciblage Android/mobile sérieux de
   Rivière est envisagé.
8. Monitoring/healthcheck automatique post-déploiement (script cron
   rejouant une sonde headless à deux clients sur `riviere` après chaque
   déploiement, avec alerte si échec).

## 5. UX — parcours actuel et frictions

**Parcours actuel** (lien partagé → premier combat) : détection WebGPU →
téléchargement (~22 s mesurés) → init GPU → écran d'accueil (pseudo, classe,
salon, « Jouer en ligne »/« Jouer seul ») → jeu. **5-6 interactions
minimum.**

**Frictions identifiées :**
- **Zéro playtest structuré à ce jour** : `docs/playtests/` est vide, le
  protocole existe (`docs/TEST_FEEDBACK_FORM.md`) mais n'a jamais tourné.
  Toute l'UX repose sur un audit interne, pas sur de vrais utilisateurs — le
  bug de contrôles (§2) en est un exemple concret : un vrai testeur l'aurait
  vu immédiatement.
- Bug d'indice de contrôles faux dans la fenêtre de connexion (§2, trouvé en
  testant le site en direct).
- Parité tactile (bouclier/ruée/sort) corrigée très récemment — non éprouvée
  en usage réel sur de vrais appareils.
- Kit de capacités 1-2-3-4 sans tutoriel in-game.
- Recharge de capacités non affichée visuellement en mode en ligne
  (seulement en solo).

### Priorisation UX

**Priorité 1 :**
1. Corriger le bug de contrôles faux (§2) — trivial, haute visibilité,
   touche 100 % des nouveaux joueurs.
2. Lancer enfin un playtest réel (3-5 testeurs non-développeurs, y compris
   mobile tactile) — le trou le plus structurant de toute l'analyse.

**Priorité 2 :**
3. Vérifier sur de vrais appareils tactiles le correctif bouclier/ruée
   récemment livré.
4. Étendre le retour visuel de recharge des capacités au mode en ligne (ou
   documenter clairement pourquoi certaines cases ne se voilent jamais).

**Priorité 3 :**
5. Raccourcir l'écran d'accueil pour un joueur pressé (classe/salon repliés
   par défaut, comme l'est déjà l'adresse serveur).
6. Tutoriel minimal in-game pour le kit 1-2-3-4 (bulle d'aide au premier
   spawn, ou surbrillance de la barre de capacités).
7. Message dédié pour l'état « onglet en arrière-plan = jeu figé » (le texte
   actuel « Initialisation du GPU… » est trompeur une fois le jeu déjà
   lancé).

## 6. Ordre d'implémentation suggéré

1. Vérifier/finaliser le chantier en cours (`ghost_free_list`) + `cargo test`
   avant de committer quoi que ce soit.
2. Corriger le bug de contrôles faux (§2) — rapide, gain immédiat.
3. Ajouter le check de taille automatique dans `build_web.sh` (§4.1) —
   rapide, évite une régression silencieuse déjà vécue deux fois.
4. Exposer `RenderQuality` en réglage joueur (§4.2) — plumbing déjà présent,
   effort modéré.
5. Planifier et exécuter un premier playtest réel (§5.1.2) — dépend surtout
   de disponibilité humaine, pas de code ; peut démarrer en parallèle des
   points ci-dessus.
6. Reprioriser la suite (CI, LOD, chargement différé, monitoring) une fois
   ces bases posées.
