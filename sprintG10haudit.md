# Sprint G — Rattrapage documentaire du GDD — clôture

> Exécution du Sprint 14 (Phase G) tel que défini dans
> [sprint10audit.md](sprint10audit.md#phase-g), lui-même issu de
> [auditGDD10h.md](auditGDD10h.md). Sprint purement documentaire : aucun code
> modifié, seul `GDD_MMORPG.md` (et le suivi dans `sprint10audit.md`) a bougé.

## Objectif

Le §14 de `GDD_MMORPG.md` (« État d'avancement ») sous-estimait ce qui est
déjà livré côté code — contraire à la règle de gouvernance §18.7 du GDD
(« toute contradiction découverte entre le document et le code est une
décision à acter, jamais un écart qu'on laisse vivre »).

## Écarts trouvés et corrigés

| Point du GDD | Constat avant | État réel vérifié | Correction |
|---|---|---|---|
| §8.3 — économie de l'XP | Marqué comme non corrigé (« ~100× trop lent ») | `round_xp` (`src/bin/server.rs:436-443`) applique déjà exactement le barème cible (150 participation + 5/frag + 75 victoire, garde anti-AFK à `ACTIVITY_DISTANCE_THRESHOLD`) — confirmé par le commit `539a883` (*Audit gameplay : mort partagée, classement individuel, XP calibrée, classes légères*) | §8.3, §14, §15.3 (ligne « Endurance ») et l'intro (§0) réécrits : seul le terme *assist* reste à 0, en attente de la Phase B |
| §14 — Roster HUD | « 🔜 Priorité 1 » | `multiplayer_roster_panel` (`src/editor/hud.rs:463-609`) est branché en dur dans `src/editor/mod.rs:530` et `:1901` — barres de vie, frags (💀), distinction spectateur grisé | Passé à ✅ « En jeu » |
| §14 — Frags individuels | « ✅ Backend (HUD à brancher) » | Déjà affichés dans le roster (`hud.rs:535`, colonne 💀) | Passé à ✅ « En jeu », référence précise ajoutée |
| §10.4 — Audio | « Aucun système audio riche n'existe encore » | `src/runtime/audio.rs` (moteur `kira` : ducking, spatialisation, streaming musique) et `src/runtime/sfx.rs` (`Sfx::Hit`, `Sfx::Defeat`, `Sfx::WaveStart` déjà câblés) forment un système complet | Reformulé en « état réel » ; noté que les rangs 2 (allié à terre) et 3 (éveil de créature) de la priorité de feedback restent muets |
| §14 — table entière | Structurée en « Priorité 1 à 6 », référençant un document supprimé (`GAMEDESIGN_MMORPG.md`) | N/A | Restructurée par phases A→G, alignée sur `sprint10audit.md`, avec pointeurs fichier:ligne pour chaque ligne 🔜 |
| §14 / §18.7 — pointeur de suivi | Ne citait que `SPRINT_MMORPG.md` | `sprint10audit.md` est la feuille de route active pour les écarts gameplay/GDD | Ajouté comme second pointeur, à côté de `SPRINT_MMORPG.md` (chantier réseau, toujours pertinent séparément) |
| §3.4, §3.5, §12 (fantasme joueur, contrat du jour, mode de manche, exclusions) | Citaient `GAMEDESIGN_MMORPG.md §2/§3.4/§3.5` et `GAMEDESIGN_EN_LIGNE.md §4/§3.7` — fichiers supprimés, renvois morts | Les deux documents n'existent plus (voir historique git) | Renvois retirés ou remplacés : contenu déjà auto-porté par le paragraphe qui le cite, sauf pour `RoundObjective` (§4) où le renvoi est remplacé par un pointeur vivant vers la Phase C de `sprint10audit.md` |
| `README.md` (limites connues, PvP) | Citait `GAMEDESIGN_EN_LIGNE.md §3.7` (supprimé) | Le point (« PvP hors périmètre par défaut ») est déjà couvert par `GDD_MMORPG.md` §12 | Renvoi basculé vers `GDD_MMORPG.md §12` |

## Ce qui n'a pas changé (toujours ouvert, confirmé en l'état)

- Décor hameau + ménagerie animée : toujours seulement dans `mmorpg_demo`, pas dans la scène servie (tension n°7).
- Système de vagues : toujours codé/testé mais la carte livrée reste à `wave: 0`.
- Régression tactile (scène ré-exportée, signalée le 16 juillet) : mention gardée mais assouplie — renvoyée vers `ROADMAP_SPRINTS.md` pour l'état courant plutôt que de citer un fichier d'audit supprimé. **Non vérifiée réellement** (nécessiterait de lancer le jeu en Play, hors périmètre d'un sprint documentaire) — reste un trou assumé de ce sprint.
- Sélecteur de classe, réanimation Soutien, assists, modes Survie/Boss/Escorte, contrat du jour, archétypes de créatures, salon/mute : toujours 🔜, désormais chacun explicitement pointé vers sa phase (`sprint10audit.md`).
- `SPRINT_MMORPG.md` a lui aussi des renvois morts vers `AUDIT_MMORPG.md`/`GAMEDESIGN_EN_LIGNE.md` (supprimés) — **non traité**, hors du mandat du Sprint 14 qui ne cible que `GDD_MMORPG.md`.

## Audit de clôture (2ème passe, sur demande) — un vrai bug trouvé et corrigé

Relecture intégrale du diff pour vérifier que le sprint est réellement complet, en restant strictement dans le périmètre déjà touché (`GDD_MMORPG.md`, `README.md`, `sprint10audit.md`, ce fichier) :

- **Bug introduit lors de la 1ère passe, corrigé** : la ligne §14 disait
  « Sélecteur de classe UI, réanimation Soutien | 🔜 Phase A, sprint 3 » —
  faux : relecture de `sprint10audit.md` (Sprint 3, lignes 66-74) confirme
  que ce sprint ne couvre que le sélecteur de classe, **pas** la
  réanimation. La réanimation Soutien (§8.1) n'apparaît dans **aucun** des
  14 sprints planifiés. Scindé en deux lignes : le sélecteur reste rattaché
  au Sprint 3, la réanimation est désormais marquée « **Non planifiée** ».
- **Ancres de section incohérentes, corrigées** : plusieurs lignes du
  tableau §14 pointaient vers `sprint10audit.md` sans ancre de phase
  (`#phase-a`, `#phase-b`, etc.) pendant que d'autres en avaient — toutes
  homogénéisées pour pointer vers la phase précise.
- **Liens et ancres vérifiés mécaniquement** : tous les fichiers cités
  (`.md`) existent sur disque, toutes les ancres `#phase-x` référencées
  correspondent à une ancre `<a id="phase-x">` réellement définie dans
  `sprint10audit.md`, structure de tableau §14 vérifiée (18 lignes × 4
  colonnes, pas de `|` cassé).
- **Constat hors périmètre, volontairement non traité** : un grep sur
  `src/**/*.rs` montre des dizaines de commentaires de code citant encore
  `GAMEDESIGN_EN_LIGNE.md`/`GAMEDESIGN_MMORPG.md` (fichiers supprimés) —
  bien plus large que ce sprint et touchant des fichiers `.rs` partagés
  avec les autres phases (B/C/D sur `src/bin/server.rs` notamment). Signalé
  ici pour mémoire mais **non corrigé**, conformément à la consigne de ne
  pas sortir du périmètre de ce sprint.

## Fichiers modifiés

- [GDD_MMORPG.md](GDD_MMORPG.md) — §0 (intro), §3.4, §4, §8.3, §10.4, §12, §14, §15.3.
- [README.md](README.md) — limites connues (renvoi PvP).
- [sprint10audit.md](sprint10audit.md) — Sprint 14 marqué ✅ terminé (2026-07-18), cases cochées.

## Vérification

Sprint purement documentaire (pas de code touché) : pas de build/test à lancer. Vérification faite par lecture directe du code source cité (`src/bin/server.rs`, `src/editor/hud.rs`, `src/editor/mod.rs`, `src/runtime/audio.rs`, `src/runtime/sfx.rs`), de l'historique git (`git log -- src/bin/server.rs`) pour dater la correction XP, et par relecture croisée de tous les liens/ancres du diff final.

## Bilan : le sprint est-il « parfait » ?

Oui, dans son périmètre strict (rattraper le §14 du GDD sur l'état réel du
code, sans dépendance aux autres phases). Deux limites assumées et
documentées restent hors de portée d'un sprint purement documentaire :
la vérification réelle de la régression tactile (demande de lancer le
jeu) et le nettoyage des renvois morts dans le code source (`src/**/*.rs`)
et dans `SPRINT_MMORPG.md`, qui appartiennent à un autre chantier.

## Prochaine étape suggérée

Phase A ou Phase C en priorité (impact joueur direct), comme recommandé par `sprint10audit.md` — Sprint G n'avait aucune dépendance et pouvait être traité indépendamment, ce qui est fait.
