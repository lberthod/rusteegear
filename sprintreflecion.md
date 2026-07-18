# RusteeGear — Plan de sprints issu de `reflexion.md`

> Traduit les 11 sections de [reflexion.md](reflexion.md) en phases/sprints exécutables. Convention
> identique à [sprint10audit.md](sprint10audit.md) et [sprintoptimation3daudit10h.md](sprintoptimation3daudit10h.md) :
> un sprint ≈ 1 à 3 jours, avec **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**.
> On ne démarre un sprint que si le précédent **de la même phase** est « vert ».

Retour : **[reflexion.md](reflexion.md)** (constat) · **[sprint10audit.md](sprint10audit.md)**
(gameplay/GDD) · **[sprintoptimation3daudit10h.md](sprintoptimation3daudit10h.md)** (rendu 3D).

Ce plan est **en aval** des deux autres : il valide, coordonne et déploie ce qu'ils produisent. Il
n'a de sens complet qu'une fois le code livré — mais certaines de ses phases (process,
documentation, produit) peuvent démarrer avant, en parallèle.

---

## 🧭 Vue d'ensemble — dépendances et parallélisme

```
Phase D (Coordination sessions)  ─┐  aucune dépendance, en continu dès maintenant
Phase H (Prochaines étapes prod) ─┘  (process/stratégie, pas de code)

Phase E (Doc à resynchroniser)   ── indépendante des autres phases de ce plan (touche des
                                     fichiers de documentation, pas de code) — démarrable dès
                                     que le contenu qu'elle vérifie existe

Code applicatif livré
        │
        ▼
Phase A (Gate technique) ──┬── Phase B (Réseau version & anti-triche)
                            ├── Phase C (Capacité skinnée en conditions réelles)
                            ├── Phase J (Sync mmorpg_demo → player_scene.json)
                            └── Phase K (Sélecteur de salon/mode réseau)
                                     │            (A, B, C, J, K indépendantes entre elles,
                                     │             parallélisables une fois le code prêt —
                                     │             J est un préalable *bloquant* à F/G ; K ne
                                     │             bloque que le Sprint 14 de F, pas F entier)
                                     ▼
                            Phase F (Playtest réel) ── dépend de A + B + C + J (+ K pour Sprint 14)
                                     │
                                     ▼
                            Phase G (Déploiement) ── dépend de A + B + F + J
                                     │
                                     ▼
                            Phase I (Hygiène mémoire) ── dépend de TOUT (dernière phase)
```

| Phase | Sprints | Dépend de | Parallèle possible avec | Fichiers touchés |
|---|---|---|---|---|
| **D — Coordination sessions** | 1 | — | tout, y compris avant le code applicatif | aucun (process) |
| **H — Prochaines étapes produit** | 2 | — | tout | aucun (réflexion/roadmap) |
| **E — Doc à resynchroniser** | 3 → 5 | le contenu qu'elle vérifie doit exister | A, B, C, J, K, D, H | `GDD_MMORPG.md`, `optimisation3D.Analys.md`, `ROADMAP_SPRINTS.md` |
| **A — Gate technique** | 6 → 7 | code applicatif livré | B, C, J, K | tout le dépôt (fmt/clippy/test), `tests/golden_*.rs` |
| **B — Réseau version & anti-triche** | 8 → 10 | champs réseau ajoutés côté serveur | A, C, J, K | `src/net/protocol.rs`, `src/bin/server.rs` |
| **C — Capacité skinnée réelle** | 11 → 12 | contenu skinné final + fix de capacité déjà en place | A, B, J, K | `src/gfx/renderer.rs` |
| **J — Sync scène jouable** | 18 → 19 | contenu de `mmorpg_demo` livré (au moins A/B/C) | A, B, C, K | `assets/player_scene.json`, `src/editor/export.rs` |
| **K — Salon/mode réseau** | 20 → 21 | code applicatif livré (champs `lobby`/`objective` déjà dans le protocole) | A, B, C, J | `src/editor/windows.rs`, `src/net/client/native.rs`, `src/net/client/web.rs`, `src/app/network_client.rs` |
| **F — Playtest réel** | 13 → 14 | **A, B, C, J** (Sprint 14 dépend en plus de **K**) | — | aucun (en jeu, pas de code) |
| **G — Déploiement** | 15 → 16 | **A, B, F, J** | — | déploiement VPS, pas de code applicatif |
| **I — Hygiène mémoire** | 17 | **toutes les phases** | — | fichiers mémoire (hors dépôt de code) |

**Fichiers partagés à surveiller** : `src/net/protocol.rs` est touché ailleurs par le travail réseau
en cours — coordonner l'ordre des merges avec la Phase B ci-dessous. `src/gfx/renderer.rs` est
touché ailleurs par le travail d'optimisation — coordonner avec la Phase C ci-dessous.
`assets/player_scene.json` est réécrit en bloc par chaque export (Phase J) — ne jamais l'éditer à la
main en parallèle d'un export en cours. `src/editor/windows.rs` (Phase K) est un fichier volumineux
déjà touché par le sélecteur de classe (Sprint 3 de `sprint10audit.md`) — vérifier son état avant
d'y ajouter les nouveaux champs. Les phases D, E, H de **ce** plan ne touchent aucun fichier de
code : elles peuvent tourner à tout moment sans risque de conflit, y compris avant que le reste soit
fini.

---

<a id="phase-d"></a>
## PHASE D — Coordination des sessions concurrentes (indépendante, en continu)

### Sprint 1 — Protocole de coordination avant merge
**Objectif** : éviter qu'une session écrase ou committe par-dessus le travail non fini d'une autre
(risque déjà rencontré sur ce dépôt).
- [ ] Avant tout commit qui touche `src/scene/demos.rs`, `src/net/protocol.rs` ou
  `src/gfx/renderer.rs` : `git status` + vérifier les mtimes récents pour détecter un travail
  concurrent non commité.
- [ ] Ne jamais committer par-dessus un état de build cassé par une autre session en cours ;
  attendre la fin ou convenir explicitement de l'ordre des commits.
- **Fichiers** : aucun (process).
- **Livrable** : une règle écrite et suivie, pas un artefact de code — se mesure à l'absence de
  conflits/écrasements sur les fichiers partagés listés ci-dessus.
- **Risques** : aucun si respecté ; le risque inverse (l'ignorer) est justement ce qui a déjà causé
  un état de build cassé pendant ce plan.

---

<a id="phase-h"></a>
## PHASE H — Prochaines étapes produit (indépendante)

### Sprint 2 — Cadrage du périmètre après le cœur de boucle
**Objectif** : préparer la discussion produit qui suit la stabilisation des modes de manche/classes/
contrats, sans attendre que tout soit fini pour y réfléchir.
- [ ] Noter où le Contrat du jour et la feuille de route long terme du moteur se recoupent, pour
  éviter un travail dupliqué.
- [ ] Lister les arguments pour/contre rouvrir le périmètre exclu du GDD (artisanat/économie/
  guildes) une fois le cœur de boucle stable — pas une décision à prendre ici, juste préparer les
  éléments.
- **Fichiers** : aucun (note de cadrage, éventuellement dans `ROADMAP_SPRINTS.md` en commentaire).
- **Livrable** : un paragraphe de cadrage prêt à discuter, pas une implémentation.
- **Risques** : aucun — sprint purement réflexif.

---

<a id="phase-e"></a>
## PHASE E — Documentation à resynchroniser (indépendante des phases A/B/C/F/G de ce plan)

### Sprint 3 — Vérifier `GDD_MMORPG.md` §14
**Objectif** : confirmer que le tableau d'avancement du GDD reflète bien l'état réel du code, pas un
état daté.
- [ ] Relire `GDD_MMORPG.md` §14 et comparer à l'état réel du code (XP, roster HUD, audio).
- **Fichiers** : `GDD_MMORPG.md`.
- **Livrable** : §14 sans statut sous-évalué par rapport au code.
- **Risques** : aucun — vérification, pas de nouveau développement.

### Sprint 4 — Vérifier `optimisation3D.Analys.md`
**Objectif** : confirmer que le document reflète des chiffres avant/après réels, pas seulement des
recommandations théoriques.
- [ ] Comparer le tableau avant/après du document aux mesures réelles obtenues en Phase C de ce
  plan (voir plus bas).
- **Fichiers** : `optimisation3D.Analys.md`.
- **Livrable** : document reflétant des mesures réelles.
- **Risques** : dépend chronologiquement d'avoir des mesures réelles — si elles ne sont pas encore
  disponibles, ce sprint ne peut confirmer que partiellement (à noter explicitement plutôt que
  deviner).

### Sprint 5 — Rattachement roadmap et nettoyage des anciens audits
**Objectif** : décider du statut des documents de sprint satellites et vérifier qu'aucune
information utile des anciens fichiers d'audit supprimés n'est perdue.
- [ ] Décider si `sprint10audit.md` / `sprintoptimation3daudit10h.md` / `sprintreflecion.md`
  rejoignent la numérotation officielle de `ROADMAP_SPRINTS.md` ou restent des documents satellites.
- [ ] Vérifier qu'aucune information utile de `AUDIT.md`, `AUDIT_MMORPG.md`, `HANDOFF.md` (supprimés,
  visibles en `git status`) n'a été perdue sans être reprise ailleurs.
- **Fichiers** : `ROADMAP_SPRINTS.md`.
- **Livrable** : décision actée (même si c'est « rester satellite »), pas de contenu orphelin perdu.
- **Risques** : aucun — sprint documentaire.

---

<a id="phase-a"></a>
## PHASE A — Gate technique (dépend du code applicatif livré)

### Sprint 6 — CI stricte sur l'ensemble du dépôt
**Objectif** : faire passer le gate CI complet, pas seulement sur les fichiers récemment modifiés.
- [ ] `cargo fmt --all --check` (`.github/workflows/ci.yml:36`).
- [ ] `cargo clippy --all-targets -- -D warnings` (`.github/workflows/ci.yml:39`).
- [ ] `cargo test` complet.
- **Fichiers** : potentiellement tout le dépôt selon ce que les warnings/tests révèlent.
- **Livrable** : CI verte de bout en bout.
- **Risques** : dépend que le code visé soit effectivement livré — ne pas lancer ce sprint trop tôt
  sur du code partiel, le signal serait bruyant pour rien.

### Sprint 7 — Régénération des golden tests si le rendu a changé
**Objectif** : s'assurer que les golden renders reflètent des changements volontaires et pas une
régression non voulue.
- [ ] Sur un worktree propre, si le rendu a changé : `UPDATE_GOLDEN=1 cargo test --test golden_render`
  et `UPDATE_GOLDEN=1 cargo test --test golden_skinning`.
- [ ] Bisecter d'abord pour distinguer changement attendu vs régression avant de régénérer.
- **Fichiers** : images de référence sous `tests/` (chemin exact à confirmer en début de sprint).
- **Livrable** : golden tests verts, différences visuelles justifiées.
- **Risques** : régénérer sans bisecter au préalable masquerait une vraie régression sous une mise à
  jour de référence — à ne jamais faire sans avoir d'abord vérifié la cause du diff.

---

<a id="phase-b"></a>
## PHASE B — Réseau : version de protocole & anti-triche (dépend des champs réseau ajoutés)

### Sprint 8 — Bump de `PROTOCOL_VERSION` si nécessaire
**Objectif** : s'assurer qu'aucun champ réseau nouvellement ajouté (classe, mode de manche, contrat
du jour) ne l'a été sans incrémenter la version.
- [ ] Vérifier `PROTOCOL_VERSION` (`src/net/protocol.rs:29`, `4` au moment de cette analyse) contre
  les champs réellement présents dans le protocole.
- [ ] Incrémenter si nécessaire, avec un plan de déploiement couplé client/VPS (pas de client
  ancien parlant à un serveur incompatible silencieusement).
- **Fichiers** : `src/net/protocol.rs`.
- **Livrable** : version de protocole cohérente avec les champs réellement présents.
- **Risques** : coordination avec d'éventuels commits en cours sur ce même fichier — vérifier via
  Phase D (coordination sessions) avant de merger.

### Sprint 9 — Audit de la discipline anti-triche sur les nouveaux champs
**Objectif** : vérifier que chaque nouveau champ réseau suit le pattern déjà en place pour
`PlayerClass` (`from_u8` avec repli sur Assaut plutôt qu'un panic).
- [ ] Relire le décodage de chaque nouveau champ (cause de mort, mode de manche, contrat du jour)
  et confirmer qu'une valeur hors table ne fait jamais paniquer le serveur.
- **Fichiers** : `src/net/protocol.rs`, `src/bin/server.rs`.
- **Livrable** : test(s) dédié(s) par nouveau champ, sur le modèle des tests existants de
  `PlayerClass::from_u8`.
- **Risques** : un champ oublié dans cet audit est une vulnérabilité de déni de service potentielle
  contre le serveur — traiter comme bloquant, pas comme optionnel.

### Sprint 10 — Revue de sécurité ciblée avant déploiement
**Objectif** : dernier filet avant la Phase G (déploiement).
- [ ] `/security-review` ou revue manuelle ciblée sur les nouveaux messages réseau, en particulier
  les champs texte libres (chat, salon).
- **Fichiers** : `src/net/`, `src/bin/server.rs`.
- **Livrable** : revue terminée, findings traités ou explicitement acceptés.
- **Risques** : ne pas déployer (Phase G) tant que ce sprint n'est pas vert si des champs texte
  libres ont été ajoutés.

---

<a id="phase-c"></a>
## PHASE C — Capacité skinnée en conditions réelles (dépend du contenu final de la scène)

### Sprint 11 — Re-test cumulé de la vue large/plongée
**Objectif** : vérifier l'effet combiné de toutes les optimisations de rendu, pas seulement chacune
isolément.
- [ ] Sur `mmorpg_demo`, vue large/plongée (scénario qui a historiquement fait déborder
  `MAX_SKINNED_INSTANCES`), relever `skinned_dropped`, `gpu_draw_calls`, FPS via le Profiler.
- **Fichiers** : aucun changement de code — mesure seulement.
- **Livrable** : chiffres cumulés post-optimisation, à comparer à la baseline mesurée initialement.
- **Risques** : dépend que le travail d'instancing/culling soit réellement livré — sinon la mesure
  est prématurée.

### Sprint 12 — Revalidation de la marge avec le contenu final
**Objectif** : s'assurer que le contenu ajouté après coup (nouvelles créatures, décor) n'a pas re-fait
déborder la capacité skinnée.
- [ ] Recompter les objets skinnés dans `mmorpg_demo()` avec le contenu final livré.
- [ ] Ajuster `MAX_SKINNED_INSTANCES` (`src/gfx/renderer.rs:160`) si nécessaire, avec la même
  discipline de marge que les 3 relèvements précédents (documentés dans le code).
- **Fichiers** : `src/gfx/renderer.rs`.
- **Livrable** : `skinned_dropped == 0` en vue large avec le contenu final.
- **Risques** : coordination avec un éventuel travail d'instancing du skinning encore en cours sur
  le même fichier.

---

<a id="phase-j"></a>
## PHASE J — Synchroniser la scène jouable (`mmorpg_demo` → `player_scene.json`)

> Préalable **bloquant**, pas une simple bonne pratique : tout le contenu gameplay livré par
> `sprint10audit.md` (modes de manche, classes, archétypes, décor) et tout le rendu optimisé livré
> par `sprintoptimation3daudit10h.md` existent dans `Scene::mmorpg_demo()` et ses tests unitaires,
> mais **pas dans la scène réellement servie aux joueurs** (`assets/player_scene.json`, embarquée à
> la compilation via `include_str!`, `src/scene/demos.rs:8710-8715`). Sans cette phase, `cargo run
> -- --player`, les builds web/APK et le VPS déployé continuent de servir une carte périmée, quel
> que soit l'état du code source.

### Sprint 18 — Export `mmorpg_demo` vers `player_scene.json`
**Objectif** : faire exister dans la scène jouable tout ce qui a été construit dans les deux plans.
- [ ] Ouvrir `Scene::mmorpg_demo()` dans l'éditeur.
- [ ] Lancer l'Export (`src/editor/export.rs:129`, `bundle_scene_json`) qui réécrit
  `assets/player_scene.json` avec la scène courante + ses assets embarqués.
- [ ] Reconstruire (`cargo build`) pour que `include_str!` (`Scene::embedded_player`,
  `src/scene/demos.rs`) prenne le nouveau contenu.
- **Fichiers** : `assets/player_scene.json` (généré, pas édité à la main).
- **Livrable** : `assets/player_scene.json` reflète l'état courant de `mmorpg_demo` (modes de
  manche, classes, décor, archétypes).
- **Risques** : ne pas lancer cet export avant que le contenu de `mmorpg_demo` visé soit
  effectivement stable (coordination avec Phase D) — un export en pleine modification concurrente
  du fichier de scène produirait un `player_scene.json` incohérent.

### Sprint 19 — Vérifier via `cargo run -- --player`
**Objectif** : confirmer, sur la vraie commande qu'utilise un joueur (README.md:450), que
l'export du Sprint 18 a bien fonctionné.
- [ ] `cargo run -- --player` et vérifier en jeu que les nouveaux modes (Survie/Escorte/Boss), le
  sélecteur de classe et le décor livré sont bien présents — pas seulement en mode Play dans
  l'éditeur sur `mmorpg_demo`.
- **Fichiers** : aucun (vérification).
- **Livrable** : parité confirmée entre `mmorpg_demo` (code/tests) et la carte réellement jouée en
  mode Player.
- **Risques** : si un écart est trouvé ici, ne pas le corriger uniquement dans
  `player_scene.json` (il serait re-écrasé au prochain export) — corriger la source
  (`mmorpg_demo`) puis ré-exporter.

---

<a id="phase-k"></a>
## PHASE K — Sélecteur de salon et de mode réseau

> Constat de la Section 3 de `reflexion.md` : le champ « Salon » visible dans la fenêtre
> Multijoueur ne contrôle que le chat Firebase, pas le salon de jeu — toutes les connexions
> réseau atterrissent dans `DEFAULT_LOBBY` (`"default"`) en dur, et le mode de manche
> (`RoundObjective`) part toujours à `0` (Vagues). Escorte/Boss existent côté serveur (Phase C de
> `sprint10audit.md`) mais ne sont jouables qu'en solo via les démos dédiées tant que cette phase
> n'est pas faite.

### Sprint 20 — Champ « Code de salon » pour le jeu (distinct du salon chat)
**Objectif** : permettre d'isoler plusieurs parties de test sur le même serveur, sans toucher au
champ « Salon » existant qui reste dédié au chat.
- [ ] Nouveau champ dans `multiplayer_window` (`src/editor/windows.rs`), à côté d'adresse/pseudo/
  classe — nommé pour ne pas être confondu avec le « Salon » du chat (ex. « Code de partie »).
- [ ] Câblé jusqu'à `NetClient::connect_to_lobby` (le paramètre `lobby` existe déjà de bout en
  bout : `src/net/client/native.rs:59-78`, `src/net/client/web.rs`, `src/app/network_client.rs`)
  à la place de `protocol::DEFAULT_LOBBY` codé en dur.
- [ ] Vide = comportement actuel inchangé (`DEFAULT_LOBBY`), sur le modèle exact de
  `ClientMsg::Join::lobby` déjà pensé rétrocompatible côté protocole (`src/net/protocol.rs:52-56`).
- **Fichiers** : `src/editor/windows.rs`, `src/app/network_client.rs`, `src/net/client/native.rs`,
  `src/net/client/web.rs`.
- **Livrable** : deux paires d'instances connectées au même serveur avec des codes de salon
  différents ne se voient pas entre elles ; deux instances avec le même code se retrouvent
  ensemble.
- **Risques** : ne pas réutiliser la variable `lobby_code` existante (déjà prise par le chat) —
  nommage distinct pour ne pas perpétuer la confusion documentée en Section 3.

### Sprint 21 — Sélecteur de mode de manche réseau (`RoundObjective`)
**Objectif** : rendre Survie/Escorte/Boss réellement jouables en salon réseau, pas seulement en
solo via les démos dédiées.
- [ ] `egui::ComboBox` dans `multiplayer_window`, sur le modèle exact du sélecteur de classe
  (Sprint 3 de `sprint10audit.md`, déjà en place).
- [ ] Câblé jusqu'au champ `objective` de `ClientMsg::Join` (actuellement `0` en dur à chaque site
  d'appel de `connect_to_lobby`) — un seul choix envoyé par le **premier** joueur à rejoindre un
  salon vide fait foi pour toute sa durée de vie (`Lobby::objective`, comportement déjà correct
  côté serveur, juste jamais alimenté par autre chose que Vagues).
- **Fichiers** : `src/editor/windows.rs`, `src/net/client/native.rs`, `src/net/client/web.rs`,
  `src/app/network_client.rs`.
- **Livrable** : une partie Survie/Escorte/Boss lancée en choisissant le mode dans la fenêtre
  Multijoueur, avec au moins 2 clients réseau réels.
- **Risques** : cohérence avec Sprint 20 — un joueur qui rejoint un salon déjà créé ne doit pas
  pouvoir changer son mode (déjà le comportement serveur, à ne pas régresser côté UI en laissant
  croire au joueur que son choix compte s'il rejoint en second).

---

<a id="phase-f"></a>
## PHASE F — Playtest réel (dépend de A + B + C + J, Sprint 14 dépend en plus de K)

### Sprint 13 — Playtest d'équilibrage
**Objectif** : juger ce que les tests unitaires ne peuvent pas juger (feedback de dégâts, seuils de
culling, transitions de LOD).
- [ ] Session de jeu réelle, à plusieurs, latence réseau incluse, via `cargo run -- --player` sur
  `player_scene.json` (Phase J déjà faite) — pas seulement en mode Play dans l'éditeur sur
  `mmorpg_demo`, qui ne prouve pas que le joueur voit la même chose.
- **Fichiers** : aucun a priori — ce sprint peut générer des tickets de réglage fin en retour.
- **Livrable** : liste de réglages fins identifiés (ou confirmation que rien ne choque).
- **Risques** : ne pas démarrer avant que A + B + C + J soient verts — un playtest sur un build
  instable, non sécurisé, ou sur une carte pas encore synchronisée produirait des faux signaux.

### Sprint 14 — Session multijoueur sur les nouveaux modes de manche
**Objectif** : valider Survie/Escorte/Boss avec plusieurs clients réels, au-delà du script
`examples/load_test_client.rs`.
- [ ] Partie complète de chaque nouveau mode avec au moins 2 joueurs humains, via `cargo run --
  --player` (ou build déployé) pointant sur la scène synchronisée.
- **Fichiers** : aucun a priori.
- **Livrable** : chaque mode se termine correctement (victoire/défaite) en conditions réelles.
- **Risques** : dépend que les modes de manche soient effectivement livrés, jouables, **et
  présents dans `player_scene.json`** (Phase J) **et sélectionnables en salon réseau** (Phase K —
  sans elle, tout client réseau reste en Vagues quel que soit le contenu de `player_scene.json`).

---

<a id="phase-g"></a>
## PHASE G — Déploiement (dépend de A + B + F + J)

### Sprint 15 — Déployer l'optimisation 3D (isolée)
**Objectif** : déployer d'abord le changement à risque faible (pas de changement de protocole).
- [ ] **Préalable** : confirmer que la Phase J (export `mmorpg_demo` → `player_scene.json`) a bien
  été faite avant ce sprint — sinon le VPS déploie un binaire à jour qui sert quand même une carte
  périmée.
- [ ] Procédure existante : push GitHub → pull + build release sur le VPS → restart systemd →
  `examples/smoke_vps.rs`.
- **Fichiers** : déploiement, pas de code applicatif nouveau à ce stade.
- **Livrable** : VPS à jour, `smoke_vps` vert, aucune régression observée.
- **Risques** : les builds « player » se connectent automatiquement au VPS au lancement
  (`README.md:376-380`) — un déploiement cassé impacte immédiatement tous les joueurs connectés.

### Sprint 16 — Déployer le gameplay réseau
**Objectif** : déployer séparément le changement à risque plus élevé (changement de protocole),
pour isoler la source d'un problème éventuel.
- [ ] Même procédure que Sprint 15, après confirmation que Sprint 15 est stable.
- **Fichiers** : déploiement.
- **Livrable** : VPS à jour avec le nouveau protocole, clients à jour uniquement, `smoke_vps` vert.
- **Risques** : ne pas déployer avant que la version de protocole et la sécurité réseau soient
  entièrement validées.

---

<a id="phase-i"></a>
## PHASE I — Hygiène de mémoire/process (dépend de toutes les phases, dernière)

### Sprint 17 — Revue de mémoire post-exécution
**Objectif** : capturer les leçons opérationnelles de l'ensemble du travail, pas les documents de
sprint eux-mêmes.
- [ ] Identifier ce qui a été surprenant ou non évident pendant l'exécution (ex.
  `MAX_SKINNED_INSTANCES` relevé une 4e fois, un mode de manche révélant un piège d'IA non
  documenté, un conflit de session concurrent).
- [ ] Écrire/mettre à jour les mémoires correspondantes plutôt que de laisser l'information dans ces
  documents de sprint (qui, eux, sont des artefacts ponctuels).
- **Fichiers** : fichiers mémoire (hors dépôt de code).
- **Livrable** : mémoires à jour, documents de sprint archivés ou marqués terminés.
- **Risques** : aucun — dernier sprint, purement rétrospectif.

---

## ✅ Définition de « terminé » par phase

| Phase | Terminée quand |
|---|---|
| D | Règle de coordination suivie sans incident sur les fichiers partagés |
| H | Cadrage produit rédigé, prêt à discuter |
| E | GDD §14, `optimisation3D.Analys.md` et `ROADMAP_SPRINTS.md` reflètent l'état réel |
| A | CI (fmt+clippy+test) verte, golden tests à jour et justifiés |
| B | `PROTOCOL_VERSION` cohérente, tous les nouveaux champs anti-triche testés, revue sécurité faite |
| C | `skinned_dropped == 0` en vue large avec le contenu final |
| F | Playtest d'équilibrage fait, tous les nouveaux modes validés à plusieurs joueurs réels |
| G | VPS à jour (optim puis gameplay), `smoke_vps` vert à chaque étape |
| J | `assets/player_scene.json` réexporté depuis `mmorpg_demo`, parité confirmée via `cargo run -- --player` |
| K | Code de salon et sélecteur de mode réseau fonctionnels, testés avec ≥2 clients réels |
| I | Mémoires à jour avec les leçons opérationnelles |

## 📌 Conseils d'exécution

- **D, H, E peuvent démarrer immédiatement**, avant même que le code applicatif soit fini — aucune
  ne touche de fichier de code.
- **A, B, C, J, K ne démarrent qu'une fois le code visé livré**, mais sont indépendantes entre
  elles : paralléliser si plusieurs sessions sont disponibles, en coordonnant sur
  `src/net/protocol.rs` (B), `src/gfx/renderer.rs` (C), `assets/player_scene.json` (J) et
  `src/editor/windows.rs` (K) via la règle de la Phase D.
- **J n'est pas optionnelle** : c'est elle qui fait passer le travail de « existe dans le code et
  les tests » à « jouable par un vrai joueur » — ne pas la traiter comme un nettoyage de fin de
  plan.
- **K conditionne uniquement le Sprint 14 de F** (playtest des nouveaux modes en réseau) — le
  Sprint 13 (playtest d'équilibrage général) peut se faire sans elle, en Vagues par défaut.
- **F, G, I sont strictement séquentielles** derrière A+B+C+J(+K pour Sprint 14) : ne pas
  playtester un build non sécurisé/non capacité-validée/non synchronisé, ne pas déployer un build
  non playtesté, ne pas clore la mémoire avant le déploiement réel.
