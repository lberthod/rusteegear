# RusteeGear — Plan de sprints issu de `reflexion.md`

> Traduit les 14 sections de [reflexion.md](reflexion.md) en phases/sprints exécutables. Convention
> identique à [sprint10audit.md](sprint10audit.md) et [sprintoptimation3daudit10h.md](sprintoptimation3daudit10h.md) :
> un sprint ≈ 1 à 3 jours, avec **Objectif · Tâches · Fichiers · Livrable vérifiable · Risques**.
> On ne démarre un sprint que si le précédent **de la même phase** est « vert ».

Retour : **[reflexion.md](reflexion.md)** (constat) · **[sprint10audit.md](sprint10audit.md)**
(gameplay/GDD) · **[sprintoptimation3daudit10h.md](sprintoptimation3daudit10h.md)** (rendu 3D).

Ce plan est **en aval** des deux autres : il valide, coordonne et déploie ce qu'ils produisent. Il
n'a de sens complet qu'une fois le code livré — mais certaines de ses phases (process,
documentation, produit) peuvent démarrer avant, en parallèle.

---

<a id="phase-a"></a>
## PHASE A — 🚨 Redéployer le VPS (hors-gating, avant tout le reste)

> Constat en direct (voir bandeau en tête de [reflexion.md](reflexion.md)) : le VPS de prod tourne
> sur `PROTOCOL_VERSION = 2` alors que le code local en est à `5` — **3 versions de retard,
> personne ne peut se connecter**. Ce n'est pas la Phase M (Sprints 15-16, qui reste le circuit
> normal pour les prochains déploiements) : c'est un correctif d'incident, à faire **maintenant**,
> sans attendre que E + F + G + L + H + I soient toutes vertes — ces phases restent utiles pour la
> suite, mais aucune ne doit bloquer la remise en service d'un serveur actuellement injoignable.
> Seule phase de ce plan à ne dépendre de rien du tout, y compris pas du code applicatif livré :
> elle redéploie ce qui est **déjà sur `main`**, tel quel.

### Sprint 0 — Redéployer immédiatement, hors du gating normal — ✅ résolu (vérifié 18 juillet 2026)
**Objectif** : rattraper l'écart `PROTOCOL_VERSION` 2 → 5 avant tout autre travail réseau, sans
attendre la Phase M (déploiement normal, qui reste le circuit standard pour la suite).
- [x] Vérifié que le code actuellement sur `main` compile, passe `cargo fmt --check`/`clippy -D
  warnings`/`cargo test` (déjà confirmé vert lors de la rédaction de ce plan).
- [x] VPS redéployé (ou remis à niveau par une autre voie — pas de commit de déploiement identifié
  dans l'historique local, mais l'effet est constaté) : `cargo run --example smoke_vps` contre
  `wss://ws.loicberthod.ch` **et** contre `ws://179.237.71.235:80` (défaut du script) répondent
  tous les deux `✅ Serveur VPS OK`, avec `Welcome`, snapshot de 27 entités (26 monstres), tir de
  projectile confirmé — **aucun message d'incompatibilité de protocole**, alors que l'incident
  original rapportait explicitement « version de protocole 5 incompatible (serveur : 2) ».
- [x] L'ordre normal des phases (E/F/G/H/I puis L puis M) peut reprendre — ce sprint d'urgence ne
  remplace pas la Phase M pour les *prochains* déploiements.
- **Fichiers** : déploiement, pas de code applicatif nouveau (redéploie l'existant).
- **Livrable vérifié** : un client à `PROTOCOL_VERSION = 5` se connecte au VPS sans message
  d'incompatibilité — confirmé en conditions réelles, pas juste supposé.
- **Risques résiduels** : le code actuellement en prod n'a pas nécessairement traversé la Phase L
  (playtest complet) — accepté par construction de ce sprint (statu quo pire que le risque de
  régression). Aucune trace de *qui* a redéployé ni *quand* exactement (pas de commit de
  déploiement, VPS non versionné dans ce dépôt) — à noter pour la Phase O (hygiène mémoire/process)
  si ça devient un problème de traçabilité récurrent.

---

## 🧭 Vue d'ensemble — dépendances et parallélisme

```
Phase A (🚨 Redéployer le VPS) ── AUCUNE dépendance, à faire EN PREMIER, hors-gating
                                   (incident en cours, ne dépend même pas du code applicatif livré)

Phase B (Coordination sessions)  ─┐  aucune dépendance, en continu dès maintenant
Phase C (Prochaines étapes prod) ─┘  (process/stratégie, pas de code)

Phase D (Doc à resynchroniser)   ── indépendante des autres phases de ce plan (touche des
                                     fichiers de documentation, pas de code) — démarrable dès
                                     que le contenu qu'elle vérifie existe

Code applicatif livré
        │
        ▼
Phase E (Gate technique) ──┬── Phase F (Réseau version & anti-triche)
                            ├── Phase G (Capacité skinnée en conditions réelles)
                            ├── Phase I (Sélecteur de salon/mode réseau)
                            ├── Phase J (Menu pause & redémarrage volontaire)
                            ├── Phase K (Terrain à relief réel) ──┐
                            └── Phase H (Sync mmorpg_demo → player_scene.json) ◄┘
                                     │            (E, F, G, I, J, K, H indépendantes entre elles
                                     │             côté code, parallélisables une fois le code
                                     │             prêt — H reste un préalable *bloquant* à L/M,
                                     │             et attend en plus le contenu de K pour que le
                                     │             nouveau relief soit exporté ; I ne bloque que
                                     │             le Sprint 14 de L, pas L entier)
                                     ▼
                            Phase L (Playtest réel) ── dépend de E + F + G + H (+ I pour Sprint 14)
                                     │
                                     ▼
                            Phase M (Déploiement) ── dépend de E + F + L + H
                                     │
                                     ▼
                            Phase N (Nettoyer README.md) ── dépend de L + M (+ I/J/K si livrées)
                                     │
                                     ▼
                            Phase O (Hygiène mémoire) ── dépend de TOUT, y compris N (dernière phase)
```

| Phase | Sprints | Dépend de | Parallèle possible avec | Fichiers touchés |
|---|---|---|---|---|
| **A — 🚨 Redéployer le VPS** | 0 | — (rien, à faire en premier) | tout, y compris le reste avant qu'il ne soit fini | déploiement, pas de code applicatif |
| **B — Coordination sessions** | 1 | — | tout, y compris avant le code applicatif | aucun (process) |
| **C — Prochaines étapes produit** | 2 | — | tout | aucun (réflexion/roadmap) |
| **D — Doc à resynchroniser** | 3 → 5 | le contenu qu'elle vérifie doit exister | E, F, G, H, I, J, K, B, C | `GDD_MMORPG.md`, `optimisation3D.Analys.md`, `ROADMAP_SPRINTS.md` |
| **E — Gate technique** | 6 → 7 | code applicatif livré | F, G, H, I, J, K | tout le dépôt (fmt/clippy/test), `tests/golden_*.rs` |
| **F — Réseau version & anti-triche** | 8 → 10 | champs réseau ajoutés côté serveur | E, G, H, I, J, K | `src/net/protocol.rs`, `src/bin/server.rs` |
| **G — Capacité skinnée réelle** | 11 → 12 | contenu skinné final + fix de capacité déjà en place | E, F, H, I, J, K | `src/gfx/renderer.rs` |
| **H — Sync scène jouable** | 18 → 19 | contenu de `mmorpg_demo` livré (au moins E/F/G), **et le relief de K si fait** | E, F, G, I, J | `assets/player_scene.json`, `src/editor/export.rs` |
| **I — Salon/mode réseau** | 20 → 21 | code applicatif livré (champs `lobby`/`objective` déjà dans le protocole) | E, F, G, H, J, K | `src/editor/windows.rs`, `src/net/client/native.rs`, `src/net/client/web.rs`, `src/app/network_client.rs` |
| **J — Menu pause & redémarrage** | 22 → 23 | code applicatif livré (`restart_game()` déjà existant) | E, F, G, H, I, K | `src/app/mod.rs`, `src/app/simulation.rs`, `src/editor/hud.rs`, `src/editor/mod.rs` |
| **K — Terrain à relief réel** | 24 → 26 | code applicatif livré | E, F, G, H, I, J | `src/gfx/mesh.rs`, `src/scene/mod.rs`, `src/scene/demos.rs`, `src/app/simulation.rs` |
| **L — Playtest réel** | 13 → 14 | **E, F, G, H** (Sprint 14 dépend en plus de **I**) | — | aucun (en jeu, pas de code) |
| **M — Déploiement** | 15 → 16 | **E, F, L, H** | — | déploiement VPS, pas de code applicatif |
| **N — Nettoyer README.md** | 27 | **L, M** (+ I/J/K si livrées) | — | `README.md` |
| **O — Hygiène mémoire** | 17 | **toutes les phases, y compris N** | — | fichiers mémoire (hors dépôt de code) |

**Fichiers partagés à surveiller** : `src/net/protocol.rs` est touché ailleurs par le travail réseau
en cours — coordonner l'ordre des merges avec la Phase F ci-dessous. `src/gfx/renderer.rs` est
touché ailleurs par le travail d'optimisation — coordonner avec la Phase G ci-dessous.
`assets/player_scene.json` est réécrit en bloc par chaque export (Phase H) — ne jamais l'éditer à la
main en parallèle d'un export en cours. `src/editor/windows.rs` (Phase I) est un fichier volumineux
déjà touché par le sélecteur de classe (Sprint 3 de `sprint10audit.md`) — vérifier son état avant
d'y ajouter les nouveaux champs. Les phases B, D, C de **ce** plan ne touchent aucun fichier de
code : elles peuvent tourner à tout moment sans risque de conflit, y compris avant que le reste soit
fini.

---

<a id="phase-b"></a>
## PHASE B — Coordination des sessions concurrentes (indépendante, en continu)

### Sprint 1 — Protocole de coordination avant merge — ✅ appliqué (vérifié 18 juillet 2026)
**Objectif** : éviter qu'une session écrase ou committe par-dessus le travail non fini d'une autre
(risque déjà rencontré sur ce dépôt).
- [x] Avant tout commit qui touche `src/scene/demos.rs`, `src/net/protocol.rs` ou
  `src/gfx/renderer.rs` : `git status` + vérifier les mtimes récents pour détecter un travail
  concurrent non commité.
- [x] Ne jamais committer par-dessus un état de build cassé par une autre session en cours ;
  attendre la fin ou convenir explicitement de l'ordre des commits.
- [x] Contrôle réel exécuté le 18 juillet 2026 : `git status` + `stat -f '%Sm'` sur les fichiers
  partagés + `ls -lat` sur les transcripts JSONL + `ps aux | grep motor3derust` ont détecté 8
  sessions actives à la même minute et un binaire `--player` en cours d'exécution, avec
  `src/gfx/renderer.rs`, `sprintreflecion.md` et `reflexion.md` modifiés dans les minutes
  précédentes — décision prise de **ne rien committer** tant que ces mtimes n'étaient pas
  stabilisés (voir mémoire [[concurrent-sessions-hazard]]).
- **Fichiers** : aucun (process).
- **Livrable** : une règle écrite et suivie, pas un artefact de code — se mesure à l'absence de
  conflits/écrasements sur les fichiers partagés listés ci-dessus. Confirmé en conditions réelles
  le 18 juillet 2026, pas juste rédigé sur le papier.
- **Risques** : aucun si respecté ; le risque inverse (l'ignorer) est justement ce qui a déjà causé
  un état de build cassé pendant ce plan.

---

<a id="phase-c"></a>
## PHASE C — Prochaines étapes produit (indépendante)

### Sprint 2 — Cadrage du périmètre après le cœur de boucle — ✅ terminé (2026-07-18)
**Objectif** : préparer la discussion produit qui suit la stabilisation des modes de manche/classes/
contrats, sans attendre que tout soit fini pour y réfléchir.
- [x] Noté où le Contrat du jour et la feuille de route long terme du moteur se recoupent, pour
  éviter un travail dupliqué — voir [ROADMAP_SPRINTS.md § Cadrage produit](ROADMAP_SPRINTS.md#-cadrage-produit--prochaines-étapes-après-le-cœur-de-boucle-sprintreflecionmd-phase-c-sprint-2).
- [x] Listé les arguments pour/contre rouvrir le périmètre exclu du GDD (artisanat/économie/
  guildes) une fois le cœur de boucle stable — pas une décision prise ici, juste les éléments
  préparés (même section).
- **Fichiers** : aucun (note de cadrage, dans `ROADMAP_SPRINTS.md` en commentaire).
- **Livrable** : un paragraphe de cadrage prêt à discuter, pas une implémentation.
- **Risques** : aucun — sprint purement réflexif.

---

<a id="phase-d"></a>
## PHASE D — Documentation à resynchroniser (indépendante des phases E/F/G/L/M de ce plan)

### Sprint 3 — Vérifier `GDD_MMORPG.md` §14 — ✅ terminé (18 juillet 2026)
**Objectif** : confirmer que le tableau d'avancement du GDD reflète bien l'état réel du code, pas un
état daté.
- [x] Relire `GDD_MMORPG.md` §14 et comparer à l'état réel du code (XP, roster HUD, audio).
- **Fichiers** : `GDD_MMORPG.md`.
- **Livrable** : §14 sans statut sous-évalué par rapport au code — 6 lignes sur 14 étaient
  sous-évaluées (sélecteur de classe, réanimation Soutien, assists, modes Survie/Boss/Escorte,
  contrat du jour, archétypes de créatures, feedback dégâts/mort) et corrigées avec preuve
  fichier:ligne. Point resté ouvert et noté explicitement : les modes de manche réseau sont
  codés/testés mais sans sélecteur UI (`objective: 0` en dur côté client).
- **Risques** : aucun — vérification, pas de nouveau développement.

### Sprint 4 — Vérifier `optimisation3D.Analys.md` — ✅ terminé (18 juillet 2026)
**Objectif** : confirmer que le document reflète des chiffres avant/après réels, pas seulement des
recommandations théoriques.
- [x] Comparer le tableau avant/après du document aux mesures réelles obtenues en Phase G de ce
  plan (voir plus bas).
- **Fichiers** : `optimisation3D.Analys.md`.
- **Livrable** : document confirmé à jour (mesures réelles du 18 juillet 2026, Phase F) ; ajout
  d'une mise en garde explicite — le pack « siège du hameau » (40 assets `siege_*`, complet) est
  postérieur à ce benchmark, décor statique non skinné confirmé mais pas encore intégré ni mesuré
  en conditions réelles (Phase G, Sprints 11-12, toujours non cochés).
- **Risques** : dépend chronologiquement d'avoir des mesures réelles — si elles ne sont pas encore
  disponibles, ce sprint ne peut confirmer que partiellement (à noter explicitement plutôt que
  deviner).

### Sprint 5 — Rattachement roadmap et nettoyage des anciens audits — ✅ terminé (18 juillet 2026)
**Objectif** : décider du statut des documents de sprint satellites et vérifier qu'aucune
information utile des anciens fichiers d'audit supprimés n'est perdue.
- [x] Décider si `sprint10audit.md` / `sprintoptimation3daudit10h.md` / `sprintreflecion.md`
  rejoignent la numérotation officielle de `ROADMAP_SPRINTS.md` ou restent des documents satellites.
- [x] Vérifier qu'aucune information utile de `AUDIT.md`, `AUDIT_MMORPG.md`, `HANDOFF.md` (supprimés,
  visibles en `git status`) n'a été perdue sans être reprise ailleurs.
- **Fichiers** : `ROADMAP_SPRINTS.md`.
- **Livrable** : décision actée dans `ROADMAP_SPRINTS.md` (note ajoutée en tête de fichier) —
  les 3 documents restent satellites (périmètre disjoint de la numérotation A→S, déjà
  cross-référencés par nom). Confirmé : le contenu utile des anciens `AUDIT*.md`/`HANDOFF.md`
  supprimés a été consolidé dans `GDD_MMORPG.md` §14 et ces satellites (commit `cb64951`), rien
  perdu sans reprise ailleurs.
- **Risques** : aucun — sprint documentaire.

---

<a id="phase-e"></a>
## PHASE E — Gate technique (dépend du code applicatif livré)

### Sprint 6 — CI stricte sur l'ensemble du dépôt — ✅ terminé (2026-07-18)
**Objectif** : faire passer le gate CI complet, pas seulement sur les fichiers récemment modifiés.
- [x] `cargo fmt --all --check` (`.github/workflows/ci.yml:36`) — aucune sortie, propre.
- [x] `cargo clippy --all-targets -- -D warnings` (`.github/workflows/ci.yml:39`) — 0 warning.
- [x] `cargo test` complet — 557 passed; 0 failed; 7 ignored (les 7 ignorés sont les tests marqués
  « outil, à lancer explicitement » type `sync_embedded_scene_*` et `tls_proof::wss_vps`, pas des
  échecs).
- **Fichiers** : aucun modifié — mesure seulement, dépôt déjà conforme.
- **Livrable** : CI verte de bout en bout, confirmée en local le 2026-07-18.
- **Risques** : lancé alors qu'une session concurrente avait `src/gfx/renderer.rs` modifié et un
  binaire `--player` en cours d'exécution (voir [[concurrent-sessions-hazard]]) — aucun commit
  effectué ici, uniquement des vérifications en lecture, donc pas de conflit possible.

### Sprint 7 — Régénération des golden tests si le rendu a changé — ✅ terminé (2026-07-18)
**Objectif** : s'assurer que les golden renders reflètent des changements volontaires et pas une
régression non voulue.
- [x] `cargo test --test golden_render` et `cargo test --test golden_skinning` (inclus dans le
  `cargo test` du Sprint 6) : les deux passent sans `UPDATE_GOLDEN=1`, donc le rendu courant — y
  compris avec les modifications non commitées de `src/gfx/renderer.rs` d'une session concurrente —
  correspond déjà aux images de référence sous `tests/`.
- [x] Bisection jugée inutile : aucune différence visuelle détectée, donc rien à distinguer entre
  changement attendu et régression.
- **Fichiers** : aucun — aucune image de référence régénérée, car aucune n'était périmée.
- **Livrable** : golden tests verts sans régénération (`golden_render` : 7 passed ;
  `golden_skinning` : 1 passed).
- **Risques** : si la session concurrente sur `renderer.rs` finalise des changements visuels après
  ce contrôle, relancer ce sprint avant la Phase L/M — ce constat n'est valable qu'à l'instant
  mesuré (2026-07-18, `src/gfx/renderer.rs` modifié à 12:08).

---

<a id="phase-f"></a>
## PHASE F — Réseau : version de protocole & anti-triche (dépend des champs réseau ajoutés)

### Sprint 8 — Bump de `PROTOCOL_VERSION` si nécessaire — ✅ terminé (2026-07-18)
**Objectif** : s'assurer qu'aucun champ réseau nouvellement ajouté (classe, mode de manche, contrat
du jour) ne l'a été sans incrémenter la version.
- [x] Vérifié `PROTOCOL_VERSION` (`src/net/protocol.rs:35`) : déjà à `5`, cohérent avec
  l'historique documenté juste au-dessus (v2 `class`, v3→v4 `cause`/`objective` de `Join`, v5
  `GameEvent::RoundObjective`) — aucun champ présent dans `ClientMsg`/`ServerMsg` n'est plus
  récent que ce commentaire, rien à incrémenter.
- [x] Câblage en cours par une session concurrente (Phase I, Sprint 21 : `connect_to_lobby`/
  `connect_to_server_as` gagnent un paramètre `objective` côté UI, `src/net/client/native.rs`) ne
  touche **pas** `src/net/protocol.rs` — c'est le champ `objective: u8` déjà versionné en v4 qui
  est enfin alimenté par autre chose que `0` en dur, pas un nouveau champ de trame. Confirmé par
  `git diff -- src/net/protocol.rs` vide pendant cette vérification.
- **Fichiers** : aucun modifié — `PROTOCOL_VERSION` déjà à jour.
- **Livrable** : version de protocole cohérente avec les champs réellement présents.
- **Risques** : si Phase I ajoute un jour un vrai nouveau champ de trame (pas seulement le
  câblage d'un champ existant), relancer ce sprint avant de merger.

### Sprint 9 — Audit de la discipline anti-triche sur les nouveaux champs — ✅ terminé (2026-07-18)
**Objectif** : vérifier que chaque nouveau champ réseau suit le pattern déjà en place pour
`PlayerClass` (`from_u8` avec repli sur Assaut plutôt qu'un panic).
- [x] `PlayerClass::from_u8` (`src/app/multiplayer.rs:62-68`) et `RoundObjective::from_u8`
  (`:187-194`) suivent déjà le pattern (`_ => valeur par défaut`, jamais de panic) — tests
  dédiés existants : `player_class_from_u8_falls_back_to_assault_for_unknown_values`,
  `round_objective_from_u8_falls_back_to_vagues_for_unknown_values`.
- [x] `DeathCause`/`DeathCauseKind` (cause de mort) et `Contract` (contrat du jour) vérifiés :
  ni l'un ni l'autre n'est décodé depuis des octets envoyés par le client — `DeathCause` est
  calculé côté serveur uniquement (`app::health::update_network_health`/`update_creature_bite`)
  et voyage serveur→client, jamais l'inverse ; `Contract` n'apparaît même pas dans
  `net::protocol` (calculé côté serveur depuis le jour UTC, `Contract::of_day`). Aucune surface
  d'attaque anti-triche pour ces deux-là : le serveur ne fait jamais confiance à une valeur
  cliente pour ces champs.
- **Fichiers** : aucun modifié — audit uniquement, discipline déjà en place.
- **Livrable** : confirmé — `class`/`objective` ont chacun un test de repli dédié sur le modèle
  demandé ; `cause`/`contrat du jour` n'ont pas besoin de cette protection (non client-contrôlés).
- **Risques** : aucun trouvé — à ne relancer que si un futur champ réseau est ajouté côté
  `ClientMsg`.

### Sprint 10 — Revue de sécurité ciblée avant déploiement — ✅ terminé (2026-07-18)
**Objectif** : dernier filet avant la Phase M (déploiement).
- [x] Revue manuelle ciblée sur les champs texte libres réseau (chat, salon, pseudo, uid
  Firebase) : `name`/`lobby`/`firebase_uid` (`ClientMsg::Join`) étaient déjà bornés et validés
  par `protocol::valid_join_fields` (Sprint 105a-2). **Trouvé** : `ChatMessage::text`
  (`src/net/firebase.rs`, chat Firebase RTDB) était le seul champ texte libre encore **sans
  borne de longueur** — un client buggé/malveillant pouvait poster un message de plusieurs Mo
  dans `/lobbies/{code}/chat`, stocké et rediffusé tel quel à tous les pairs du salon (coût
  RTDB, UI qui doit afficher la ligne).
- [x] Corrigé : `net::firebase::MAX_CHAT_LEN` (240 caractères) + `valid_chat_text()` (même
  discipline que `valid_join_fields` : rejet si vide après `trim` ou trop long), appelée dans
  `post_chat_message` (serveur de vérité) et côté client avant l'appel réseau
  (`AppState::request_send_chat_message`, `src/app/network_client.rs`) pour un message d'erreur
  immédiat plutôt qu'un aller-retour réseau inutile ; `egui::TextEdit` du champ de saisie
  (`src/editor/windows.rs`) borné par `.char_limit(MAX_CHAT_LEN)` en défense en profondeur
  côté UI. 3 tests dédiés ajoutés (`valid_chat_text_accepts_a_normal_message`,
  `_rejects_an_empty_or_blank_message`, `_rejects_an_oversized_message`).
- [x] Gate technique relancé après correctif : `cargo fmt --all --check` propre, `cargo clippy
  --all-targets -- -D warnings` propre, `cargo test --lib` 563 passed/0 failed/7 ignored,
  `cargo test --test golden_render --test golden_skinning` 8 passed.
- **Fichiers** : `src/net/firebase.rs`, `src/app/network_client.rs`, `src/editor/windows.rs`.
- **Livrable** : revue terminée, l'unique finding traité (pas seulement accepté).
- **Risques** : aucun résiduel identifié sur les champs texte libres actuels — à relancer si un
  nouveau champ texte libre est ajouté au protocole ou au chat.

---

<a id="phase-g"></a>
## PHASE G — Capacité skinnée en conditions réelles (dépend du contenu final de la scène)

### Sprint 11 — Re-test cumulé de la vue large/plongée — ✅ terminé (2026-07-18)
**Objectif** : vérifier l'effet combiné de toutes les optimisations de rendu, pas seulement chacune
isolément.
- [x] Sur `mmorpg_demo`, vue large/plongée (scénario qui a historiquement fait déborder
  `MAX_SKINNED_INSTANCES`), relevé `skinned_dropped`, `gpu_draw_calls`, FPS via
  `examples/phase_f_measure.rs` (vue large/plongée : `distance=90`, `yaw=0.7`, `pitch=1.1`,
  headless 1280×720, 60 échantillons après 5 de chauffe) : `gpu_draw_calls = 592`,
  `skinned_dropped = 0`, `76.4 FPS` équivalent (`13.10 ms/frame`), 887 objets de scène / 315
  meshes importés.
- **Fichiers** : aucun changement de code — mesure seulement.
- **Livrable** : chiffres cumulés post-optimisation, à comparer à la baseline mesurée initialement
  — `skinned_dropped == 0` confirmé, aucune régression de capacité détectée avec le contenu actuel.
- **Risques** : dépend que le travail d'instancing/culling soit réellement livré — sinon la mesure
  est prématurée.

### Sprint 12 — Revalidation de la marge avec le contenu final — ✅ terminé (2026-07-18)
**Objectif** : s'assurer que le contenu ajouté après coup (nouvelles créatures, décor) n'a pas re-fait
déborder la capacité skinnée.
- [x] Recompté les objets skinnés dans `mmorpg_demo()` avec le contenu final livré (script de
  comptage jetable, sans effet de bord) : **201 objets skinnés** sur 887 objets de scène — chiffre
  identique à la mesure du 18 juillet 2026 déjà documentée dans le commentaire de
  `MAX_SKINNED_INSTANCES` (`src/gfx/renderer.rs:166-172`). Le pack siège du hameau ajouté depuis
  (`siege_*.glb`, remparts/props statiques) n'introduit aucun nouvel objet skinné.
- [x] `MAX_SKINNED_INSTANCES` (`src/gfx/renderer.rs:173`, `256`) **laissé inchangé** : marge de
  ~55 confirmée toujours valide (201 skinnés / 256 de capacité), aucun ajustement nécessaire.
- **Fichiers** : `src/gfx/renderer.rs` (aucune modification — capacité déjà suffisante).
- **Livrable** : `skinned_dropped == 0` en vue large avec le contenu final — confirmé au Sprint 11.
- **Risques** : coordination avec un éventuel travail d'instancing du skinning encore en cours sur
  le même fichier — aucun conflit rencontré, le fichier n'a pas été modifié par ce sprint.

---

<a id="phase-h"></a>
## PHASE H — Synchroniser la scène jouable (`mmorpg_demo` → `player_scene.json`)

> Préalable **bloquant**, pas une simple bonne pratique : tout le contenu gameplay livré par
> `sprint10audit.md` (modes de manche, classes, archétypes, décor) et tout le rendu optimisé livré
> par `sprintoptimation3daudit10h.md` existent dans `Scene::mmorpg_demo()` et ses tests unitaires,
> mais **pas dans la scène réellement servie aux joueurs** (`assets/player_scene.json`, embarquée à
> la compilation via `include_str!`, `src/scene/demos.rs:8710-8715`). Sans cette phase, `cargo run
> -- --player`, les builds web/APK et le VPS déployé continuent de servir une carte périmée, quel
> que soit l'état du code source.

### Sprint 18 — Export `mmorpg_demo` vers `player_scene.json` — ✅ terminé (2026-07-18)
**Objectif** : faire exister dans la scène jouable tout ce qui a été construit dans les deux plans.
- [x] **Correction de trajectoire par rapport à la consigne initiale** : `Scene::mmorpg_demo()`
  n'est **pas** la source de vérité de la carte jouée — sa propre doc de fonction la décrit comme
  une arène minimale de test réseau PC↔mobile, sans boutons tactiles Feu/Arme/Soin ni monstres.
  Depuis le commit `823a074` (« le hameau fortifié devient la source de vérité »), c'est
  `Scene::hameau_gdd_demo()` qui joue ce rôle, avec les créatures/décor ambiant/pickups
  resynchronisés depuis `mmorpg_demo()` par-dessus. Un export brut de `mmorpg_demo()` via
  `bundle_scene_json` (bouton GUI ou équivalent headless) écrase silencieusement les boutons
  tactiles — confirmé en le faisant puis en voyant `the_embedded_scene_ships_monsters_and_the_fire_button`
  échouer ; reverté avant commit.
- [x] Utilisé la chaîne d'outils déjà existante (headless, `cargo test --lib -- --ignored
  --nocapture <nom>`), dans l'ordre : `sync_embedded_scene_hameau_from_the_demo` (environnement du
  hameau, préserve boutons/HUD/joueur), `sync_embedded_scene_creatures_from_the_demo`,
  `sync_embedded_scene_ambient_decor_from_the_demo`, `sync_embedded_scene_pickups_from_the_demo`,
  puis `bundle_missing_assets_referenced_by_the_embedded_scene` (0 asset manquant) — pas de
  session d'éditeur GUI nécessaire.
- [x] `touch src/assets.rs` (obligatoire : `include_dir!` ne détecte pas les changements de
  `assets/bundle/` seul) puis `cargo build` — vert (l'erreur de build croisée avec une autre
  session en cours sur `network_client.rs`/`native.rs` au même moment s'est résolue d'elle-même,
  sans rapport avec cet export).
- [x] `cargo test --lib` complet : **563 passed, 0 failed** (dont les 8 gardes-fous
  `the_embedded_*`), `cargo fmt --check` et `cargo clippy --all-targets -- -D warnings` verts.
- **Fichiers** : `assets/player_scene.json` + `assets/bundle/` (générés par les outils
  `sync_embedded_scene_*`, pas édités à la main) ; `src/editor/export.rs` (essai d'export direct
  de `mmorpg_demo()` ajouté puis reverté, aucune trace laissée dans le code final).
- **Livrable** : `assets/player_scene.json` reflète l'état courant du hameau fortifié + décor/
  créatures/pickups de `mmorpg_demo` — 614 objets/112 imports (hameau) puis 797 objets/240 imports
  après resync complet, identique en volume à l'état précédent mais contenu réellement resynchronisé
  (diff réel de ~8400 lignes, pas un no-op).
- **Risques résiduels** : le pack siège du hameau (`siege_*.glb`, 40 assets visés) et les modes de
  manche Survie/Escorte/Boss ne sont **pas** dans `hameau_gdd_demo()`/`mmorpg_demo()` au moment de
  cet export (aucune occurrence de `siege_`, `Survie`, `Escorte` dans le code source scanné) — la
  sélection de mode reste de toute façon gérée côté salon réseau (Phase I), pas par le contenu de
  la scène. Si le pack siège doit remplacer le décor de remparts actuel, ce sera un nouveau cycle
  Phase H (source à modifier : `hameau_gdd_demo()`, pas `player_scene.json` directement).

### Sprint 19 — Vérifier via `cargo run -- --player` — ✅ terminé (2026-07-18)
**Objectif** : confirmer, sur la vraie commande qu'utilise un joueur (README.md:450), que
l'export du Sprint 18 a bien fonctionné.
- [x] `cargo run -- --player` lancé : connexion multijoueur au VPS confirmée en log
  (« Multijoueur : connecté à wss://ws.loicberthod.ch », « bienvenue, joueur 5 »), aucune erreur
  au démarrage.
- [x] Vérification passive (logs + tests, pas d'inspection visuelle interactive — pas de bundle
  `.app` indexé pour cibler la fenêtre via l'automatisation disponible dans cette session ;
  vérification visuelle interactive laissée à l'utilisateur).
- **Fichiers** : aucun (vérification).
- **Livrable** : build `--player` démarre et se connecte sans erreur sur la scène resynchronisée ;
  parité de contenu confirmée par les gardes-fous `the_embedded_*` (tests, pas inspection visuelle
  manuelle des modes de manche/sélecteur de classe en jeu — à faire par l'utilisateur si souhaité).
- **Risques** : si un écart est trouvé à l'usage, ne pas le corriger uniquement dans
  `player_scene.json` (il serait re-écrasé au prochain export) — corriger la source
  (`hameau_gdd_demo`/`mmorpg_demo`) puis relancer les outils `sync_embedded_scene_*`.

---

<a id="phase-i"></a>
## PHASE I — Sélecteur de salon et de mode réseau

> Constat de la Section 3 de `reflexion.md` : le champ « Salon » visible dans la fenêtre
> Multijoueur ne contrôle que le chat Firebase, pas le salon de jeu — toutes les connexions
> réseau atterrissent dans `DEFAULT_LOBBY` (`"default"`) en dur, et le mode de manche
> (`RoundObjective`) part toujours à `0` (Vagues). Escorte/Boss existent côté serveur (Phase G de
> `sprint10audit.md`) mais ne sont jouables qu'en solo via les démos dédiées tant que cette phase
> n'est pas faite.

### Sprint 20 — Champ « Code de salon » pour le jeu (distinct du salon chat) — ✅ terminé (18 juillet 2026)
**Objectif** : permettre d'isoler plusieurs parties de test sur le même serveur, sans toucher au
champ « Salon » existant qui reste dédié au chat.
- [x] Nouveau champ « Code de partie » dans `multiplayer_window` (`src/editor/windows.rs`), à côté
  d'adresse/pseudo/classe — nommé distinctement du « Salon » du chat, porté par un nouveau champ
  `Editor::mp_room_code` (`src/editor/mod.rs`), pas la variable `mp_lobby_code` déjà prise par le
  chat.
- [x] Câblé jusqu'à `NetClient::connect_to_lobby` (le paramètre `lobby` existait déjà de bout en
  bout : `src/net/client/native.rs`, `src/net/client/web.rs`) via un nouveau paramètre `room: &str`
  sur `AppState::connect_to_server_as` (`src/app/network_client.rs`), à la place de
  `protocol::DEFAULT_LOBBY` codé en dur au site d'appel.
- [x] Vide = comportement actuel inchangé (`DEFAULT_LOBBY`) : `connect_to_server_as` retombe sur
  `protocol::DEFAULT_LOBBY` si `room.trim().is_empty()`, même repli que `ClientMsg::Join::lobby`
  côté protocole (`src/net/protocol.rs:52-56`).
- **Fichiers** : `src/editor/windows.rs`, `src/editor/mod.rs`, `src/app/network_client.rs`,
  `src/app/mod.rs` (champ `net_last_connect`, tuple étendu pour la reconnexion automatique),
  `src/net/client/native.rs`, `src/net/client/web.rs`, `src/bin/server.rs` (sites d'appel de test
  de `connect_to_lobby`).
- **Livrable** : CI (fmt+clippy `-D warnings`+`cargo test --lib`, 563 passed) verte sur le nouveau
  câblage ; `tests::two_clients_in_different_lobbies_land_in_separate_rooms`
  (`src/bin/server.rs`, préexistant, toujours vert) couvre déjà l'isolation par code de salon côté
  serveur — la vérification manuelle en session réseau réelle à 2 instances reste à faire par
  l'utilisateur si souhaité.
- **Risques** : `lobby_code` existante non réutilisée (nommage distinct `mp_room_code`), comme
  prévu — aucune confusion introduite avec le salon de chat.

### Sprint 21 — Sélecteur de mode de manche réseau (`RoundObjective`) — ✅ terminé (18 juillet 2026)
**Objectif** : rendre Survie/Escorte/Boss réellement jouables en salon réseau, pas seulement en
solo via les démos dédiées.
- [x] `egui::ComboBox` dans `multiplayer_window`, sur le modèle exact du sélecteur de classe
  (Sprint 3 de `sprint10audit.md`) — nouveau `RoundObjective::label()` (`src/app/multiplayer.rs`)
  pour les libellés affichés, désactivé une fois connecté comme le reste des champs de connexion.
- [x] Câblé jusqu'au champ `objective` de `ClientMsg::Join` (jusqu'ici `0` en dur à chaque site
  d'appel de `connect_to_lobby`, dans `native.rs` et `web.rs`) via un nouveau paramètre `objective:
  u8`/`RoundObjective` de bout en bout (`connect_to_lobby` → `connect_to_server_as` →
  `net_last_connect` pour la reconnexion automatique) — le serveur reste seul arbitre : le choix du
  **premier** joueur à rejoindre un salon vide fait foi (`Lobby::objective`, non modifié par ce
  sprint).
- **Fichiers** : `src/editor/windows.rs`, `src/editor/mod.rs`, `src/net/client/native.rs`,
  `src/net/client/web.rs`, `src/app/network_client.rs`, `src/app/mod.rs`, `src/app/multiplayer.rs`,
  `src/bin/server.rs` (sites d'appel de test).
- **Livrable** : CI verte (fmt+clippy `-D warnings`+`cargo test --lib`, 563 passed, dont
  `app::network_client::tests::round_objective_event_aligns_our_local_objective_with_the_room` et
  les tests de reconnexion `reconnection_gives_up_after_max_attempts_and_says_so`/
  `voluntary_disconnect_cancels_any_pending_reconnection` mis à jour pour le tuple à 5 éléments) ;
  une partie Survie/Escorte/Boss lancée depuis le sélecteur avec ≥2 clients réseau réels reste une
  vérification manuelle à faire par l'utilisateur si souhaité (couverte côté serveur par
  `tests::a_joining_client_learns_the_rooms_objective_over_the_wire`, préexistant).
- **Risques** : cohérence avec Sprint 20 respectée — le sélecteur de mode est désactivé dès la
  connexion établie (`ui.add_enabled_ui(!net_connected, …)`), donc un second joueur qui rejoint un
  salon déjà créé ne peut pas croire que son choix compte.

---

<a id="phase-j"></a>
## PHASE J — Menu pause et redémarrage volontaire

> Constat de la Section 4 de `reflexion.md` : `AppState::restart_game()`
> (`src/app/persistence.rs:16-36`) existe déjà et est câblé à un bouton « 🔄 Rejouer »
> (`src/editor/hud.rs:880-900`), mais **uniquement après une défaite** (`self.lost`, y compris la
> mort par chute dans une zone mortelle). Aucun menu pause/paramètres n'est accessible à la
> demande pendant la partie — `run_player_overlay` (`src/editor/mod.rs:484`) n'affiche que le HUD
> de jeu.

### Sprint 22 — État de pause compatible avec les timers de manche — ✅ terminé (2026-07-18)
**Objectif** : poser l'état de pause sans casser les mécaniques déjà chronométrées (Survie a un
minuteur de 180 s, `AppState::update_survie`).
- [x] Nouveau champ `AppState::paused: bool` (déjà existant, réutilisé — cf. `Livrable`),
  déclenché par une touche dédiée en mode Play/Player (Échap, `AppState::toggle_pause`,
  câblée dans `src/lib.rs`).
- [x] Geler la simulation pendant la pause sur le même principe que la fin de manche
  (`is_room_lost`/`win_time`) — pas de nouveau système de gel, réutilisé le point d'entrée déjà
  utilisé pour arrêter `advance_play` (`self.paused` était déjà gelé par `advance_play`, posé pour
  le Play/Pause de l'éditeur ; seul le déclenchement en Play/Player manquait).
- [x] Le chrono de `RoundObjective::Survie` ne continue pas à courir pendant la pause — testé
  (`app::tests::pausing_freezes_the_survie_timer`).
- **Fichiers** : `src/app/mod.rs`, `src/lib.rs` (déclenchement clavier), `src/app/persistence.rs`
  (`restart_game` lève aussi la pause).
- **Livrable** : mettre le jeu en pause en Survie à 10 s de la fin, attendre 30 s réelles, reprendre
  — la manche ne s'est pas terminée pendant la pause. Confirmé par test automatisé plutôt qu'en
  session manuelle (`cargo test --lib pausing_freezes_the_survie_timer`).
- **Risques** : coordination avec Phase I sur `src/app/network_client.rs` — non touché par ce
  sprint, aucun conflit constaté.

### Sprint 23 — Overlay du menu pause (Reprendre / Redémarrer) — ✅ terminé (2026-07-18)
**Objectif** : exposer `restart_game()` en dehors du chemin de défaite, plus une reprise simple.
- [x] Nouveau panneau HUD (`pause_menu`, sur le modèle de `defeated_banner`/`restart_button`,
  `src/editor/hud.rs`) avec deux boutons : **Reprendre** (referme le menu, aucun changement d'état
  hors lever `paused`) et **Redémarrer** (appelle `restart_game()`, déjà existant et testé).
- [x] Câblage dans `run_player_overlay` (`src/editor/mod.rs`), affiché uniquement si
  `AppState::paused` (Sprint 22), exclusif avec les bannières de fin de manche.
- **Fichiers** : `src/editor/hud.rs`, `src/editor/mod.rs`, `src/gfx/renderer.rs` (site d'appel),
  `src/app/locale.rs` (libellés FR/EN).
- **Livrable** : en Play/Player, ouvrir le menu pause, cliquer Redémarrer restaure la scène comme
  au chemin de défaite existant ; cliquer Reprendre continue la partie sans effet de bord. CI
  (fmt+clippy+`cargo test --lib`) verte hormis 2 échecs préexistants sans rapport
  (`assets/player_scene.json` en cours de réexport par une autre session concurrente).
- **Risques** : n'a pas dupliqué la logique de `restart_button` — réutilisé
  `crate::app::locale::restart_button_label`/`restart_game()` tels quels plutôt que réécrire un
  second chemin de redémarrage.

---

<a id="phase-k"></a>
## PHASE K — Terrain à relief réel (herbe partout, pentes, collines, tunnels, creux, lacs)

> Constat de la Section 5 de `reflexion.md` : `mesh::terrain()` (`src/gfx/mesh.rs:387-421`) ne
> génère qu'une grille 24×24 à relief sinusoïdal de faible amplitude (0,08) — un seul objet
> primitif, pas un système de terrain continu. Le sol actuel de `mmorpg_demo` est un scatter de
> décor (herbe/fougères/rochers) posé dessus, pas un heightmap couvrant toute la carte. Aucune
> primitive de tunnel/grotte/creux n'existe dans `MeshKind`.

### Sprint 24 — Heightmap paramétrable et couverture d'herbe continue
**Objectif** : remplacer la grille fixe par un terrain configurable, sans trou de sol visible.
- [ ] Étendre `mesh::terrain()` (ou nouveau générateur dédié) : taille de grille paramétrable,
  fonction de hauteur par bruit procédural (ou heightmap chargée), au lieu de l'amplitude/fréquence
  fixes actuelles.
- [ ] Couverture d'herbe continue sur tout le maillage — texture/couleur de sol ou scatter densifié
  (arbitrage perf à faire avec `optimisation3D.Analys.md` §3, déjà vigilant sur le fill-rate du
  feuillage).
- **Fichiers** : `src/gfx/mesh.rs`, `src/scene/mod.rs` (si nouveau `MeshKind`).
- **Livrable** : `mmorpg_demo` n'a plus de zone de sol nue visible en vue large.
- **Risques** : un terrain plus gros/plus dense augmente le nombre d'instances de décor — revalider
  avec la Phase G (capacité skinnée) et le culling par distance déjà en place si le scatter est
  densifié.

### Sprint 25 — Pentes, collines par zone, et collision IA vérifiée
**Objectif** : un relief qui varie par zone plutôt qu'un bruit uniforme, exploitable en gameplay.
- [ ] Variation d'amplitude/fréquence du relief par zone (pas la même fonction partout) pour créer
  des collines/pentes distinctes.
- [ ] Vérifier que le nouveau relief reste un obstacle réel pour les sondes IA et les joueurs —
  piège déjà documenté en mémoire projet : tout obstacle doit être visible au raycast à 0,6 m,
  sinon patrouille figée. Un relief prononcé change la hauteur de sol sous créatures/joueurs, pas
  seulement l'esthétique.
- **Fichiers** : `src/gfx/mesh.rs`, `src/scene/demos.rs`, `src/app/simulation.rs` (sondes IA).
- **Livrable** : au moins 2 zones visuellement distinctes (plate vs vallonnée) dans `mmorpg_demo`,
  aucune créature figée par le nouveau relief.
- **Risques** : le plus gros risque de cette phase — coordination avec `sprint10audit.md` Phase D
  (archétypes de créatures) si le fichier `src/app/simulation.rs` y est encore actif.

### Sprint 26 — Creux/tunnels et lacs intégrés au relief
**Objectif** : les éléments qu'un heightmap seul ne peut pas représenter.
- [ ] Creux/fosses et tunnels : géométrie non-heightmap insérée aux endroits voulus (un heightmap
  classique ne représente pas un surplomb) — traité comme une extension du terrain, pas une
  réécriture du système de hauteur.
- [ ] Lacs intégrés au relief : le niveau d'eau correspond à un creux réel du terrain (plutôt qu'un
  plan d'eau posé sur un sol plat comme aujourd'hui) pour que la rive suive la pente naturellement.
- **Fichiers** : `src/scene/demos.rs`, `src/gfx/mesh.rs`.
- **Livrable** : au moins un tunnel/creux et un lac dont la rive suit visiblement une pente du
  terrain, tests de collision/étanchéité de l'eau réutilisant le pattern déjà existant
  (`mmorpg_water_is_sealed_all_the_way_around_including_next_to_bridges`).
- **Risques** : le plus expérimental des trois sprints de cette phase — prévoir qu'il puisse être
  repoussé sans bloquer Sprint 24/25, qui apportent déjà l'essentiel du gain visuel (herbe
  continue + collines).

---

<a id="phase-l"></a>
## PHASE L — Playtest réel (dépend de E + F + G + H, Sprint 14 dépend en plus de I)

### Sprint 13 — Playtest d'équilibrage
**Objectif** : juger ce que les tests unitaires ne peuvent pas juger (feedback de dégâts, seuils de
culling, transitions de LOD).
- [ ] Session de jeu réelle, à plusieurs, latence réseau incluse, via `cargo run -- --player` sur
  `player_scene.json` (Phase H déjà faite) — pas seulement en mode Play dans l'éditeur sur
  `mmorpg_demo`, qui ne prouve pas que le joueur voit la même chose.
- **Fichiers** : aucun a priori — ce sprint peut générer des tickets de réglage fin en retour.
- **Livrable** : liste de réglages fins identifiés (ou confirmation que rien ne choque).
- **Risques** : ne pas démarrer avant que E + F + G + H soient verts — un playtest sur un build
  instable, non sécurisé, ou sur une carte pas encore synchronisée produirait des faux signaux.

### Sprint 14 — Session multijoueur sur les nouveaux modes de manche
**Objectif** : valider Survie/Escorte/Boss avec plusieurs clients réels, au-delà du script
`examples/load_test_client.rs`.
- [ ] Partie complète de chaque nouveau mode avec au moins 2 joueurs humains, via `cargo run --
  --player` (ou build déployé) pointant sur la scène synchronisée.
- **Fichiers** : aucun a priori.
- **Livrable** : chaque mode se termine correctement (victoire/défaite) en conditions réelles.
- **Risques** : dépend que les modes de manche soient effectivement livrés, jouables, **et
  présents dans `player_scene.json`** (Phase H) **et sélectionnables en salon réseau** (Phase I —
  sans elle, tout client réseau reste en Vagues quel que soit le contenu de `player_scene.json`).

---

<a id="phase-m"></a>
## PHASE M — Déploiement (dépend de E + F + L + H)

### Sprint 15 — Déployer l'optimisation 3D (isolée)
**Objectif** : déployer d'abord le changement à risque faible (pas de changement de protocole).
- [ ] **Préalable** : confirmer que la Phase H (export `mmorpg_demo` → `player_scene.json`) a bien
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

<a id="phase-n"></a>
## PHASE N — Nettoyer `README.md` (dépend de L + M, et de I/J/K si livrées)

> Constat de la Section 13 de `reflexion.md` : le nettoyage n'a de sens qu'une fois le déploiement
> (Phase M) fait, pour décrire ce qui est réellement en ligne — le faire plus tôt produirait un
> document à nouveau faux au sprint suivant. Déjà repéré comme périmé : § « Limites connues »
> (`README.md:384-395`, « Pas de rôles/classes » est faux depuis la Phase E de
> `sprint10audit.md`), § « Multijoueur en ligne (chantier en cours) » (`README.md:237`,
> sous-estime ce qui est livré), § « Fonctionnalités disponibles » (`README.md:163`, modes de
> manche/contrat du jour/archétypes absents), § « La suite — analyse & sprints » (`README.md:530`,
> à vérifier contre les liens morts déjà traités par la Phase D, portée initialement limitée à
> `GDD_MMORPG.md`).

### Sprint 27 — Relecture section par section, contre l'état réel du code
**Objectif** : un README qui décrit l'état effectivement déployé, sans réécriture globale.
- [ ] § Limites connues (`README.md:384-395`) : retirer « Pas de rôles/classes » (livré) ; retirer
  « Pas de sélection de salon dans l'UI » **seulement si** la Phase I est livrée ; vérifier chaque
  ligne restante contre le code (grep avant d'affirmer, même discipline que ce document).
- [ ] § Multijoueur en ligne (`README.md:237`) : relire le titre/l'intro « chantier en cours » à la
  lumière de ce qui est réellement stabilisé (assists, modes de manche, roster, chat, mute).
- [ ] § Fonctionnalités disponibles (`README.md:163`) : ajouter les modes Survie/Escorte/Boss, le
  contrat du jour, les archétypes de créatures, et — si livrées — le menu pause (Phase J) et le
  relief de terrain (Phase K).
- [ ] § La suite — analyse & sprints (`README.md:530`) : vérifier que les liens pointent vers les
  documents actuels (`auditGDD10h.md`/`sprint10audit.md`/`optimisation3D.Analys.md`/
  `sprintoptimation3daudit10h.md`/`reflexion.md`/`sprintreflecion.md`), aucun lien mort vers un
  fichier déjà supprimé que la Phase D n'aurait pas couvert (portée initiale limitée à
  `GDD_MMORPG.md`).
- **Fichiers** : `README.md`.
- **Livrable** : README relu section par section, chaque affirmation vérifiée contre le code au
  moment du nettoyage, aucun lien mort.
- **Risques** : démarrer ce sprint avant que L + M soient vertes — le document serait à refaire
  dès le prochain déploiement ; ne pas retirer une limite listée tant que la phase qui la lève
  n'est pas elle-même confirmée livrée (ex. ne pas retirer « pas de sélection de salon » si I n'est
  pas faite).

---

<a id="phase-o"></a>
## PHASE O — Hygiène de mémoire/process (dépend de toutes les phases, dernière)

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
| A | Un client à `PROTOCOL_VERSION = 5` se connecte au VPS sans message d'incompatibilité |
| B | Règle de coordination suivie sans incident sur les fichiers partagés |
| C | Cadrage produit rédigé, prêt à discuter |
| D | GDD §14, `optimisation3D.Analys.md` et `ROADMAP_SPRINTS.md` reflètent l'état réel |
| E | CI (fmt+clippy+test) verte, golden tests à jour et justifiés |
| F | `PROTOCOL_VERSION` cohérente, tous les nouveaux champs anti-triche testés, revue sécurité faite |
| G | `skinned_dropped == 0` en vue large avec le contenu final |
| L | Playtest d'équilibrage fait, tous les nouveaux modes validés à plusieurs joueurs réels |
| M | VPS à jour (optim puis gameplay), `smoke_vps` vert à chaque étape |
| H | `assets/player_scene.json` réexporté depuis `mmorpg_demo`, parité confirmée via `cargo run -- --player` |
| I | Code de salon et sélecteur de mode réseau fonctionnels, testés avec ≥2 clients réels |
| J | Menu pause accessible en Play/Player, Reprendre et Redémarrer fonctionnels, timers de manche non affectés |
| K | Herbe continue sans zone nue, au moins 2 zones de relief distinctes, un tunnel/creux et un lac intégrés |
| N | README relu section par section, aucune affirmation non vérifiée, aucun lien mort |
| O | Mémoires à jour avec les leçons opérationnelles |

## 📌 Conseils d'exécution

- **A d'abord, sans discussion** : c'est un incident en cours (VPS à `PROTOCOL_VERSION = 2` contre
  `5` en local), pas une phase planifiée — elle passe devant tout le reste, y compris devant B/C/D
  qui peuvent pourtant démarrer tout aussi tôt.
- **B, C, D peuvent démarrer immédiatement**, avant même que le code applicatif soit fini — aucune
  ne touche de fichier de code.
- **E, F, G, H, I, J, K ne démarrent qu'une fois le code visé livré**, mais sont indépendantes
  entre elles : paralléliser si plusieurs sessions sont disponibles, en coordonnant sur
  `src/net/protocol.rs` (F), `src/gfx/renderer.rs` (G), `assets/player_scene.json` (H),
  `src/editor/windows.rs` (I), `src/editor/hud.rs`/`src/editor/mod.rs` (J) et
  `src/scene/demos.rs`/`src/app/simulation.rs` (K) via la règle de la Phase B.
- **H n'est pas optionnelle** : c'est elle qui fait passer le travail de « existe dans le code et
  les tests » à « jouable par un vrai joueur » — ne pas la traiter comme un nettoyage de fin de
  plan. **Si K (terrain) est faite, H doit être relancée après** pour que le nouveau relief soit
  bien dans `player_scene.json`, pas seulement dans `mmorpg_demo`.
- **I conditionne uniquement le Sprint 14 de L** (playtest des nouveaux modes en réseau) — le
  Sprint 13 (playtest d'équilibrage général) peut se faire sans elle, en Vagues par défaut.
- **K (Sprint 26, tunnels/lacs) est la plus expérimentale** : Sprints 24-25 (herbe continue +
  collines) apportent déjà l'essentiel du gain visuel et peuvent être livrés seuls si le temps
  manque pour le Sprint 26.
- **L, M, N, O sont strictement séquentielles** derrière E+F+G+H(+I pour Sprint 14) : ne pas
  playtester un build non sécurisé/non capacité-validée/non synchronisé, ne pas déployer un build
  non playtesté, **ne pas nettoyer le README avant le déploiement réel** (Phase N), ne pas clore la
  mémoire avant tout le reste.
