# Rivière & cascade — vitrine de rendu (eau animée, forêt, brume)

![Aperçu](img/riviere_cascade_preview.png)

Démo `Fichier → 🎬 Démos → 🌐 Modes multijoueur → 🏞 Rivière & cascade`
(console : `demo riviere` ; desktop : `--demo=riviere` ; web : `?scene=riviere`,
en ligne sur <https://water.loicberthod.ch>). Une vallée boisée en U, une
rivière sinueuse qui descend d'un plateau par une cascade dans un bassin, une
forêt dense sur les deux versants, de la brume au pied de la chute. Pas de
combat : on se promène (joystick / WASD, Espace pour sauter).

## Multijoueur (14 septembre 2026)

La démo est un monde en ligne à part entière, sur le même serveur autoritaire
que le hameau MMORPG (`src/bin/server.rs`) — pas un second protocole : le
**code de salon** décide seul quel monde un salon charge
(`net::protocol::RIVIERE_LOBBY = "riviere"`, `app::multiplayer::WorldKind`),
sans nouveau champ de protocole ni bump de `PROTOCOL_VERSION`. Un salon dont
le code vaut exactement `RIVIERE_LOBBY` construit `Scene::riviere_demo()`
(`bin/server.rs::Room::for_world`) au lieu du hameau embarqué ; tout autre
code garde le hameau, comme avant.

Côté client, `AppState::world` retient le monde **actuellement chargé**
localement (posé par `load_riviere_demo`/`use_embedded_scene*`) : un code de
salon vide au moment de se connecter (fenêtre Multijoueur de l'éditeur, ou
écran d'accueil « Jouer en ligne / seul ») rejoint alors le salon partagé du
monde en cours plutôt que toujours `DEFAULT_LOBBY`
(`app::network_client::resolve_lobby_code`). Charger la démo Rivière avant de
cliquer « Se connecter » suffit donc à retrouver les autres visiteurs de ce
monde, sans rien à choisir de plus. Limite assumée : pas de salon Rivière
*privé* avec un code arbitraire en v1 (comme le hameau, tous les visiteurs se
retrouvent dans ce salon unique).

**En ligne depuis le 14 septembre 2026**, vérifié à chaque étape :

- En local d'abord : serveur local + deux clients éditeur pilotés (`--pilot`),
  chacun `demo riviere` puis `pilot net connect ws://127.0.0.1:7777` avec un
  salon vide — les deux se retrouvent dans « riviere » et se voient (objet
  `Joueur réseau <nom>` visible de chaque côté).
- Puis en production : le serveur de `ws.loicberthod.ch` a été redéployé avec
  ce binaire (24 tests réseau rejoués *sur le VPS* avant le remplacement,
  ancien binaire sauvegardé, coupure de service ~2 s confirmée par
  l'utilisateur au préalable). Vérifié avec `cargo run --release --example
  smoke_vps wss://ws.loicberthod.ch` (hameau intact) et deux vrais clients
  connectés sur `wss://ws.loicberthod.ch`, salon « riviere ».
- Enfin sur le site : `water.loicberthod.ch` redéployé sans le garde-fou solo
  (`#[cfg(target_arch = "wasm32")]` retiré de `lib.rs`) et vérifié avec **deux
  vrais onglets Chrome** ouverts sur le site en ligne, chacun cliquant
  « Jouer en ligne » indépendamment — connectés, se voient.

**⚠️ Piège opérationnel** (pertinent pour quiconque retouche le serveur) : le
checkout git géré par `scripts/deploy_vps.sh` sur le VPS (`~/rusteegear-server`)
est resté sur l'ancien commit `main` — seul le **binaire compilé** a été
remplacé (build fait à la main dans `/mnt/data/riviere-mp-build/`, rsyncé
depuis ce dépôt, `feat/platformer-2d` mélangeant ce travail à du code non
trié d'une autre session). Un `deploy_vps.sh` normal (ou tout `git pull` +
rebuild dans ce dossier) écraserait ce binaire par l'ancien tant que ce
travail n'est pas mergé dans `main` — vérifier après coup que le salon
`riviere` route encore vers la vallée avant de considérer un déploiement
« normal » comme neutre.

### Kit de capacités 1-2-3-4 : retours à l'écran (14 septembre 2026 au soir)

Retour utilisateur après mise en ligne : « 1-2-3, J, K… ne donnent aucun
retour à l'écran ». Sondé sur le site en ligne (`window.__rusteegear_state`
après un `KeyboardEvent` sur le canvas), les touches arrivaient bien au moteur ;
trois vrais défauts en aval, tous corrigés et couverts par des tests :

- **Le sort (K/3) ne partait jamais** — solo comme en ligne, et pas seulement
  ici : `fireball_impact` éteignait le projectile dès sa naissance parce que le
  terrain « Vallée » (un seul grand maillage `Static`) contient tout point de
  tir dans son AABB monde. Les obstacles sont désormais détectés par un rayon
  physique sur le segment parcouru à chaque tick (`Physics::raycast`, même
  masque que la caméra et la ruée), les cibles gardent leur AABB gonflée.
  Test : `app::fireball::tests::a_fireball_survives_over_the_riviere_terrain_mesh`.
- **Aucun retour visible hors du clip d'animation** (petit, vu de dos) : une
  vraie barre de capacités (`editor::hud::ability_bar`, état pris dans
  `app::ability_hud::AbilityHud`) remplace en bas de l'écran l'arme équipée et
  l'ancienne ligne de texte qui se superposaient — chaque case (1/J mêlée,
  2 bouclier, 3/K sort, 4 ruée, H soin) **s'allume** tant que la capacité est
  en cours et se voile le temps de sa recharge (ruée, sort en solo ; les
  recharges serveur ne sont pas diffusées).
- **Les autres joueurs ne voyaient jamais un coup ou un sort** : le serveur ne
  diffusait que Walk/Idle. `multiplayer::update_network_ability_animations`
  élit le clip Block/Attack/Cast/Dash de chaque joueur réseau d'après son
  `Input` (mêmes minuteurs que le joueur local, un appui bref joue le clip en
  entier) — il part dans `EntityDelta::anim_clip` comme avant.

Au passage, une sonde réseau headless à deux clients sur le salon `riviere`
en production a montré 4 renards enragés sur 6 à y = −155 … −1881, tombant à
vitesse constante : un corps scripté (`resolve_scripted_moves`) dont le
collider finit **dans** le décor (bousculade, dépénétration) n'est plus retenu
par `move_shape` et traverse le sol pour toujours. Filet de sécurité : quand
un tel corps descend à pleine vitesse de chute, un rayon cherche le décor
fixe juste au-dessus de son origine et l'y repose. Test :
`runtime::physics::tests::a_scripted_body_pushed_into_the_floor_is_lifted_back_instead_of_falling_forever`.

Limite connue relevée pendant ces sondes : `MAX_CONNECTIONS_PER_IP` (4) compte
des connexions déjà fermées côté client tant que le serveur ne les a pas vues
tomber — plusieurs sondes successives depuis la même IP finissent refusées
(« Serveur plein ») ; un joueur seul n'est pas concerné.

### Jauges de vie et PvP (14 septembre 2026, fin de soirée)

Demande « rajoute des points de vie / jauge de vie sur les personnages et permet
le PvP ». Le PvP mêlée/tir existait déjà côté serveur dans cette scène (kit
1-2-3-4), mais rien n'était visible et un duel était en pratique impossible :

- **Jauges au-dessus des têtes** (`app::world_labels` → `hud::world_health_labels`,
  projetées par la matrice caméra comme le marqueur d'allié à terre) : pseudo +
  barre + points de vie (« 85 » sur 100) pour chaque autre joueur réseau, barre
  rouge + « 2/3 » pour chaque monstre à moins de 45 m ; flash blanc à chaque
  perte de vie. Le joueur local garde sa grande barre en haut de l'écran.
- **Vie des monstres diffusée** normalisée dans `EntityDelta::health` (était
  `None`) et recopiée dans `Combat::hp` côté client, sans changement de protocole.
- **Portée PvP** : deux `creature_ronde` ne s'approchent pas à moins de ≈ 2,1 m
  centre à centre (sonde en production) ; la portée de 1,2 m (`NETWORK_ATTACK_RANGE`,
  pensée contre l'AABB d'un monstre) rendait le duel impossible →
  `PVP_MELEE_RANGE` = 2,8 m, PvP seulement (hameau inchangé).
- **Réapparition PvP** (`health::update_network_respawn`) : un joueur vaincu
  dans une scène à kit revient au point de départ à pleine vie avec sa grâce
  d'apparition après 6 s ; dans les mondes coopératifs, seule la réanimation
  de Soutien relève un joueur, comme avant.

Vérifié en production (sonde à deux clients + navigateur) : B tué en mêlée
en ~8 s, réapparu 6 s plus tard, jauges « Joueur 6 · 30 » et « 3/3 » visibles
au-dessus des personnages, vie des monstres présente dans chaque snapshot.

### Parité tactile Bouclier/Ruée (15 septembre 2026)

Le tactile de cette démo n'exposait jusqu'ici qu'un seul bouton (Saut) : ni
Mêlée, Bouclier, Sort, Ruée ni Soin n'avaient de bouton tactile câblé, malgré
la barre de capacités 1/2/3/4/H déjà visible sur mobile (peintre pur sans
hit-test, cf. `editor::hud::ability_bar`) — un joueur tactile la voyait sans
jamais pouvoir y toucher. Corrigé :

- **Mêlée et Soin** : purement des données (`Controller::attack_button`/
  `heal_button` existaient déjà et étaient déjà câblés de bout en bout,
  comme `jump_button`) — juste renseignés sur le `Controller` de la scène.
- **Bouclier et Ruée** : aucun mécanisme équivalent n'existait. Deux nouveaux
  champs `Controller::block_button`/`dash_button` (même motif que les
  précédents), et le OR touche+clavier répliqué à chaque point de lecture
  local — `combat::update_dash`, `simulation::apply_ability_animations` (anim
  « Block »), `network_client::network_input_msg` (message serveur) — plutôt
  que dans `lib.rs::recompute_action_buttons`, qui ne tourne pas à chaque
  frame (seulement sur événement clavier/manette, cf. son commentaire) et
  raterait donc un joueur pur tactile.
- **Découverte en testant** (absente de la reconnaissance initiale) : le
  bouclier solo/hôte passe aussi par un troisième chemin, indépendant du
  réseau — le global Lua `blocking` (`scripting::run_script`/`run_script_web`,
  lu par `creature_bite_script` pour réduire directement les dégâts d'une
  morsure) lisait `input.block` brut, sans le OR tactile. Corrigé dans
  `simulation.rs` en construisant, une fois par frame et seulement quand
  nécessaire, une **copie** de `PlayerInput` avec `block` fusionné (jamais en
  place sur `input_state.block` : ça le rendrait « collant » après un
  relâchement du doigt, cf. le commentaire dans le code).
- **Grille d'action** : `TouchZones::layout` (`app/touch.rs`) bascule déjà en
  grille 2 colonnes dès qu'il y a plus d'un bouton — passer `MobileControls::
  buttons` de 1 à 5 noms suffit, aucun nouveau code de layout. La barre de
  capacités cosmétique, elle, se masque désormais sur tactile
  (`touch_ui_active() && mobile.any()`) : les boutons réels la remplacent au
  lieu de se superposer à elle sans être cliquables.

Tests : `combat::tests::holding_the_touch_dash_button_moves_the_player_forward`,
`simulation::tests::holding_the_touch_block_button_reduces_solo_bite_damage_from_creature_1`,
`network_client::tests::network_input_msg_sends_touch_block_and_dash_like_local_prediction`.

### Parité manette Bouclier/Ruée (15 septembre 2026)

Suite directe du point précédent : le tactile avait Bouclier/Ruée, la manette
non — `GamepadBindings`/`GamepadInput` n'avaient tout simplement pas ces deux
champs (contrairement à `Controller::block_button`/`dash_button` côté
tactile), donc aucun bouton manette ne pouvait déclencher ces actions, même
en remapping. Corrigé, plus simplement que le correctif tactile ci-dessus :

- `GamepadBindings::block`/`dash` (`app/settings.rs`, défauts `LeftTrigger`/
  `LeftTrigger2` — LB/LT, miroir de Changer d'arme sur RightTrigger) et
  `GamepadInput::block`/`dash` (`app/input.rs`, résolus par
  `resolve_gamepad_input`, inclus dans le tableau `bound` pour qu'un bouton
  de croix assigné à Bouclier/Ruée soit exclu du déplacement de secours,
  comme les 7 actions existantes).
- Un seul point d'intégration, contrairement au tactile : `lib.rs::
  recompute_action_buttons` fixe `inp.block`/`inp.dash` sur
  `keys.contains(&kb.block) || gp.block` (même motif que jump/attack/fire/
  heal), **pas** un OR à chaque point de lecture. Cette fonction tourne à
  chaque événement `gilrs` (bouton ou axe, cf. `poll_gamepad`), donc un OR
  simple suffit ici — au contraire du tactile qui n'a pas d'équivalent
  déclenché par un événement tactile et a dû répliquer le OR dans
  `combat::update_dash`/`simulation::apply_ability_animations`/
  `network_client::network_input_msg`.
- Vérifié explicitement que le troisième chemin découvert pour le tactile (le
  global Lua `blocking` lisant `self.input_state.block`) n'a besoin d'aucun
  changement : il lit déjà `input_state.block` posé par
  `recompute_action_buttons`, donc hérite automatiquement de la source
  manette une fois celle-ci fusionnée là — pas de copie de `PlayerInput`
  supplémentaire comme celle ajoutée pour le tactile.
- Panneau ⚙ Paramètres › 🎮 Manette (`editor::windows::settings_player_sections`) :
  deux lignes `gamepad_binding_row` de plus (Bouclier, Ruée), même fonction
  générique réutilisée telle quelle.

Tests : `app::input::tests::resolve_gamepad_input_reads_block_and_dash_defaults`,
`app::input::tests::a_dpad_button_bound_to_block_no_longer_moves_the_player`,
`app::settings::tests::an_old_settings_file_with_gamepad_but_without_block_dash_loads_with_default_bindings`.

## Ce qui a été ajouté au moteur

### Composant « Surface d'eau » (`SceneObject::water`)

Inspecteur → Matériau → **Surface d'eau**. Trois genres (`WaterKind`) :

| Genre | Usage | UV attendus |
| --- | --- | --- |
| Rivière / lac | plan d'eau courante ou calme | `u` = abscisse le long du courant (m), `v` = position transversale (m) — le motif défile vers +u |
| Cascade | nappe tombante | `u` transversal, `v` = longueur parcourue (m) — traînées étirées, défilement vers +v |
| Brume / embruns | plan ou impostor (`MeshKind::Billboard`) | 0..1 — nuage d'opacité animé, fondu vers les bords |

Réglages : **Courant** (vitesse de défilement), **Échelle** (finesse des
vaguelettes), **Écume** (quantité). Se combine avec **Opacité < 1** pour laisser
voir le lit (passe transparente).

Le rendu (`main.wgsl`, `shade_water`) est entièrement procédural — aucune
texture ni carte de normales : bruit de valeur à quatre octaves animé, dérivé
en normale dans un repère tangent reconstruit des dérivées écran ; Fresnel de
Schlick entre le corps d'eau (teinte de l'objet × profondeur) et le reflet du
ciel (dégradé horizon/zénith de la scène, berges sombres sous l'horizon) ;
éclats du soleil ; écume seuillée. Le temps d'animation est l'horloge du
renderer (`CameraUniform::eye.w`) : l'eau bouge aussi dans l'éditeur hors Play,
et reste figée en rendu headless (goldens déterministes ; `Renderer::set_anim_time`
pour un aperçu).

**Réflexion planaire** (`Renderer::render_reflection_pass`) : quand la scène
contient une surface « rivière », la scène opaque (ciel, statiques, skinnés)
est redessinée depuis la caméra **miroir** (symétrique par rapport au plan
d'eau, altitude lue au sommet de nappe le plus proche de la cible caméra) dans
une texture demi-résolution ; les fragments sous le plan sont rejetés (le lit
ne se reflète pas). Les objets eau lisent cette texture à leurs coordonnées
écran (elle remplace l'albédo, groupe 3), déformée par les vaguelettes et
mélangée au ciel selon le Fresnel — la forêt et la falaise se reflètent
réellement. Coût : une passe de plus (≈ la passe principale à ¼ des pixels).

**Atmosphère** (`Sky`) : `sun_glow` (disque + halo du soleil dans le ciel,
teinté par la lumière, passe dans le bloom), `fog_height_base` /
`fog_height_falloff` (brouillard **de hauteur** : la brume stagne dans la
vallée et s'éclaircit sur les crêtes ; l'horizon du ciel se voile de la couleur
du brouillard). Tous à 0 par défaut : aucune scène existante ne change.

**Masques par sommet** : le maillage porte une couleur `COLOR_0` lue comme
R = écume autorisée, G = profondeur relative (rivière) ou distance au bord
(cascade). L'importeur glTF lit désormais `COLOR_0` (multiplié par la teinte du
matériau, comme la spec) — sans effet sur les packs existants, qui n'en ont pas.

### Génération des assets (sans Blender)

```bash
python3 scripts/gen_riviere_cascade.py
```

Produit `assets/models/riviere/` : le terrain (grille 221² sommets, vallée,
plateau, falaise, lit et bassin creusés), son **albédo cuit** 2048² (herbe,
humus des sous-bois, roche sur les pentes, galets mouillés dans le lit, face de
berge sombre — une seule texture par objet dans le moteur, le mélange est fait à
la génération), et les trois nappes d'eau (amont, cascade balistique à deux
feuillets, bassin + aval).

```bash
python3 scripts/gen_riviere_vegetation.py
```

Végétation et rochers **générés** (numpy, couleurs par sommet) : épicéas à
verticilles de lames d'aiguilles et houppes (≈ 6 000 triangles, variante LOD
≈ 900), pin sylvestre, hêtres et bouleaux à ≈ 1 800 feuilles en losange dans
une couronne ellipsoïdale (normales « centre → feuille » pour un ombrage
arrondi), touffes d'herbe, fougères, rochers érodés (icosphère déplacée par
bruit, fissures, lichen, mousse sur les faces tournées vers le haut), galets
mouillés, tronc mort. Planche d'aperçu :
`cargo run --example gen_riviere_assets_preview --profile dev-fast`.

`Scene::riviere_demo` (`src/scene/demos/riviere.rs`) lit la hauteur du sol dans
le maillage importé (`terrain_height`) pour poser la forêt (modèles détaillés
au bord de la rivière côté caméra, `_lod` ailleurs), le sous-bois, les berges
(roseaux et pampas **animés** du pack Blender, massettes, herbes hautes,
galets), les rochers de rivière et de falaise, brumes basses et une faune
discrète — tirage déterministe, la scène est identique à chaque chargement.
Sur le web, `assets/models/riviere/` est compilé dans le `.wasm`
(`embedded://riviere/…`, ≈ 12 Mo). Les formules du chenal sont recopiées du script
Python ; le test `riviere_demo_plants_nothing_in_the_water` garantit qu'elles
restent synchronisées.

## Limites (honnêtes)

- Pas de normal mapping ni de réfraction ; la réflexion est planaire (un seul
  plan d'eau par image : la rivière amont, 9 m plus haut, retombe sur le ciel).
- Feuillages en triangles colorés par sommet (pas de textures alpha) : lisibles
  à distance et en mouvement, plus grossiers de près.
- Aperçu : `cargo run --example gen_riviere_preview --profile dev-fast`
  (GPU requis) écrit `docs/img/riviere_cascade_preview.png`.

## Mise en ligne (water.loicberthod.ch)

Site statique servi par Caddy sur le VPS partagé (même patron que
`mouveo.loicberthod.ch`, cf. `checkVPS/VPS-DEPLOY-GUIDE.md`, chemin B) :

```bash
./packaging/build_web.sh                      # wasm32 + wasm-bindgen + wasm-opt → packaging/web/pkg/
# dist = index.html (avec <script> qui force ?scene=riviere ET le commit
# substitué au placeholder __RUSTEEGEAR_COMMIT__, cf. packaging/web/index.html),
# favicon.svg, pkg/
rsync -az --delete -e "ssh -i ~/.ssh/loicberthodvps" dist/ ubuntu@<vps>:/mnt/data/water/dist/
```

Bloc Caddy `water.loicberthod.ch { root * /mnt/data/water/dist … file_server }`
ajouté le 11 septembre 2026 (sauvegarde `Caddyfile.bak.20260911-204518`). Le
`.wasm` pèse ≈ 36 Mo (moteur + assets de la vallée embarqués) ; premier
chargement ≈ 10-20 s selon la connexion. WebGPU requis (Chrome/Edge 113+,
Safari 18+). Sur un onglet en arrière-plan, le navigateur suspend
`requestAnimationFrame` : la page reste sur « Initialisation du GPU… » tant
que l'onglet n'est pas visible — ce n'est pas un blocage.

## Performances

Benchmark headless (`cargo run --example bench_riviere --profile dev-fast`,
`BENCH_RES=2560x1440`) : ≈ 14 ms/image pour la scène complète sur M5 Pro,
dont ≈ 60 % pour les arbres. La scène est limitée par le **surdessin des
feuillages** (fragments × MSAA × PCF 5×5), pas par les triangles : d'où les
feuilles moins nombreuses mais plus grandes, les variantes `_lod`, et le tri
des objets du plus proche au plus lointain depuis le départ du joueur (le
test de profondeur précoce rejette la plupart des fragments cachés). Mode
joueur natif (MSAA 4×, Retina) : ≈ 50 FPS en `dev-fast`, 120 FPS en build
optimisé.
