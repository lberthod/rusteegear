# Audit d'écart GDD ↔ code réel — 2026-07-18

Objectif : lister précisément ce qu'il manque dans le jeu pour correspondre à `GDD_MMORPG.md`, avec preuves dans le code (`src/`). Méthode : lecture intégrale du GDD (1336 lignes) + ROADMAP_SPRINTS.md/SPRINT_MMORPG.md pour la trajectoire récente, exploration ciblée de `src/` système par système, croisement avec `git log`.

Convention de statut : **Fait** / **Partiel** / **Manquant**.

---

## 1. Modes de manche (§4 du GDD) — le plus gros trou

**Statut : Manquant (sauf Vagues)**

- Seul le mode Vagues existe : `Combat::wave`, `src/scene/mod.rs:454-470`, `max_wave()` dans `src/app/combat.rs:64`.
- `RoundObjective`, `Escorte`/`Convoi`, `Survie`, `Boss` : **aucune occurrence dans `src/`**. Recherche des symboles infructueuse (seuls des faux positifs comme `GizmoMode`, `AttackMode`).
- **À faire** : introduire un type `RoundObjective` (Vagues / Survie / Escorte / Boss), câbler la sélection de mode côté serveur (`src/bin/server.rs`) et la condition de victoire/défaite associée, puis l'UI de sélection.

## 2. Contrat du jour (§3.4)

**Statut : Manquant, confirmé par le code lui-même**

- Aucune trace de `last_contract_day` ou d'un système de contrat quotidien.
- `src/bin/server.rs:438-439` contient un commentaire explicite : pas de terme de contrat pour l'instant, à ajouter avec le premier `RoundObjective`.
- **À faire** : dépend du point 1 (modes de manche) — le contrat du jour est pensé comme une couche au-dessus des objectifs de manche.

## 3. Grammaire d'archétypes de créatures (§5.4)

**Statut : Manquant**

- Le GDD décrit 4 archétypes de comportement de chasse : Traqueuse / Meute / Colosse / Furtive.
- Aucune structure dédiée dans le code (`AiChaser` existe mais reste générique, pas de branchement par archétype). `src/app/simulation.rs` : plafond `MAX_ACTIVE_CHASERS_PER_TARGET = 2` (ligne 52) et rayon `CHASER_DETECT_RANGE = 9.0` (ligne 63) sont bien présents et conformes au GDD, mais c'est un comportement unique, pas 4 grammaires distinctes.
- **À faire** : ajouter un enum d'archétype par créature/prefab, différencier vitesse d'approche, distance de meute, camouflage (Furtive), charge (Colosse).

## 4. Assists (§8.3, économie XP)

**Statut : Partiel — formule prête, non branchée**

- `src/bin/server.rs:391-489` implémente déjà XP_PARTICIPATION=150, XP_PER_FRAG_OR_ASSIST=5, XP_VICTORY_BONUS=75, garde anti-AFK par distance (`ACTIVITY_DISTANCE_THRESHOLD=3.0`), classement par contribution individuelle (`network_player_score`, ligne 405-407).
- Commentaire à `src/bin/server.rs:416-420` : seuls les frags comptent aujourd'hui ; la formule additionne déjà la constante pour les assists mais rien ne les détecte/compte encore.
- **À faire** : détecter les assists côté serveur (dégât porté à une cible tuée par un autre joueur dans une fenêtre de temps) et les additionner au score existant.

## 5. Feedback dégâts subis / diagnostic de mort (§6.1, §16.5)

**Statut : Manquant (le trou de gameplay le plus visible)**

- Aucune preuve de flash/recul/son de contact à l'écran quand le joueur encaisse des dégâts.
- Le diagnostic de mort décrit au GDD (ex. « Encerclé — 2 Traqueuses ») n'a aucune trace dans le code.
- La bannière `PlayerDown` diffusée existe bien côté réseau (`src/app/network_client.rs:874-1455`, `src/net/protocol.rs:318`), mais c'est une notification d'état, pas un diagnostic explicatif.
- **À faire** : ajouter un système de feedback visuel/sonore au contact (vignette rouge, recul caméra), et calculer + afficher une cause de mort résumée (dernier type d'agresseur, nombre d'assaillants).

## 6. Sélecteur de classe en UI (§8, backend prêt)

**Statut : Partiel**

- `PlayerClass` (Assault/Éclaireur/Support) est complet côté modèle : `src/app/multiplayer.rs:44-103`, avec les modificateurs exacts du GDD et un décodage réseau anti-triche testé (`PlayerClass::from_u8`, lignes 59-66, tests 748-751).
- Aucun widget de sélection de classe trouvé dans `src/editor/windows.rs` ; les commentaires dans `network_client.rs`/`native.rs`/`web.rs` confirment explicitement que ce n'est pas encore câblé à une UI (défaut = Assaut).
- **À faire** : ajouter l'écran/le widget de choix de classe avant/pendant la connexion multijoueur.

## 7. Fenêtre Multijoueur — onglet Salon (chat + présence)

**Statut : Partiel, à vérifier plus précisément**

- Backend Firebase complet : `post_chat_message`/`list_chat_messages`, `set_presence`/`list_online_players`, `get_top_leaderboard` (`src/net/firebase.rs:421-545`), branché depuis `network_client.rs`.
- Le roster réseau **est** affiché en HUD (`multiplayer_roster_panel`, `src/editor/hud.rs:463-609`, trié par frags via `roster_display_order`, lignes 445-455) — ce point est donc plus avancé que ce que le GDD indique dans son tableau §14.
- L'onglet Salon (chat visible en jeu) n'a pas été vérifié positivement — à auditer précisément si prioritaire.
- Mute local (§18.4.1) : **absent**, aucune occurrence trouvée dans `src/`.
- **À faire** : vérifier/compléter l'UI de chat en jeu, ajouter le mute local par joueur.

## 8. Artisanat / Économie / Guildes / Quêtes-PNJ

**Statut : Manquant — conforme aux exclusions volontaires du GDD (§12)**

- Recherche de `guild|craft|artisanat|monnaie|currency|trade|marketplace` : aucun système réel dans `src/`. Ce n'est pas un écart, c'est cohérent avec le périmètre volontairement exclu par le GDD.
- Pas d'action requise sauf si le périmètre du GDD change.

## 9. Points où le GDD est en retard sur le code (à corriger dans le GDD, pas dans le jeu)

À signaler à l'équipe pour mise à jour du §14 (le GDD lui-même demande d'acter ces contradictions, §18.7) :

- **XP/économie (§8.3)** : marquée « 🔜 Priorité 3 » dans le GDD, mais implémentée quasi intégralement côté serveur (voir point 4).
- **Roster HUD multijoueur** : marqué « 🔜 Priorité 1 » dans le GDD, mais déjà câblé et affiché (`hud.rs:463-609`).
- **Audio** : le GDD affirme qu'« aucun système audio riche n'existe encore » (§10.4), alors que `src/runtime/audio.rs` et `src/runtime/sfx.rs` implémentent déjà streaming musical, mix de couches, reverb et gain. `Sfx::WaveStart` est câblé (`src/app/combat.rs:92,131`, `src/net/server_loop.rs:773`), mais les sons de feedback (dégâts subis, allié à terre, éveil de créature) n'ont pas été retrouvés — priorisation §10.4 non confirmée en l'état.

## 10. Systèmes solides, conformes au GDD

Pour situer l'effort restant, ce qui fonctionne déjà et n'a pas besoin de travail supplémentaire immédiat :

- **Combat cœur** : armes à distance/mêlée, plafond de chasseresses, économie de vie et exclusivité de réanimation (`src/app/health.rs:45-318`, tests `:727-872`).
- **Réseau/persistance** : serveur autoritaire (`src/net/server_loop.rs`), protocole (`src/net/protocol.rs`), reconnexion avec backoff, fantômes réseau désormais solides (commit `09babe0`).
- **Décor/scène** : hameau fortifié comme source de vérité, resynchronisé avec les pickups (commit `823a074`), ~45 modèles de créatures dans `src/scene/demos.rs`.
- **Éditeur** (`src/editor/`) : système massif et mature, hors périmètre direct du GDD mais essentiel à la production (profiler, export mobile, assistants d'authoring).
- **Mobile/HUD tactile** : `MobileConfig` avec boutons, barre de vie, zone sûre (`src/scene/mobile.rs:22-37`).

---

## Priorités recommandées (ordre d'impact)

1. **Feedback dégâts subis + diagnostic de mort** (§6.1/§16.5) — le trou le plus visible pour un joueur, effort modéré.
2. **Modes de manche (Survie/Escorte/Boss)** (§4) — le plus gros écart structurel, effort important.
3. **Sélecteur de classe en UI** (§8) — backend déjà prêt, effort faible.
4. **Assists** (§8.3) — formule déjà prête, effort faible à moyen.
5. **Contrat du jour** (§3.4) — dépend du point 2.
6. **Grammaire d'archétypes de créatures** (§5.4) — effort important, impact gameplay progressif.
7. **Mise à jour du GDD §14** pour refléter l'avance réelle sur XP, roster HUD et audio.

## Limites de cet audit

Non vérifiés directement (à creuser si critiques) :
- Contenu exact de `player_scene.json` et résultat du test `mmorpg_demo_waves_follow_the_gdd_authoring_rules` au jour J.
- État précis de l'onglet Salon (chat en jeu) de la fenêtre Multijoueur.
- Audit exhaustif de `src/runtime/sfx.rs` pour l'ordre de priorité des sons de feedback.
