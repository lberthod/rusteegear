# Sprint E — Compression de texture GPU (Phase E de `sprintoptimation3daudit10h.md`)

> Compte-rendu du Sprint 6 (« Compression ASTC/BC7 au pipeline d'import ») de la
> [Phase E](sprintoptimation3daudit10h.md#phase-e). Retour : **[optimisation3D.Analys.md](optimisation3D.Analys.md)**
> · **[sprintoptimation3daudit10h.md](sprintoptimation3daudit10h.md)**.

## Ce qui a été livré

Compression **BC3** (S3TC/DXT5) des textures d'albédo à l'import GPU, activée
uniquement quand le GPU expose `wgpu::Features::TEXTURE_COMPRESSION_BC` — sinon
dégradation silencieuse vers le chemin `Rgba8UnormSrgb` existant, inchangé.

- [x] Étape de compression de texture ajoutée au pipeline d'import GPU
      ([`src/gfx/texcompress.rs`](src/gfx/texcompress.rs), nouveau module).
- [x] Génération de mipmaps conservée pour les textures compressées, chaîne **complète**
      jusqu'à 1×1 — chaîne CPU (filtre boîte 2×2, corrigé en espace linéaire) plutôt que
      le blit GPU existant, les formats compressés ne pouvant pas être
      `RENDER_ATTACHMENT`. Corrigé après audit — voir « Audit a posteriori » plus bas
      pour les 3 défauts trouvés (dont un vrai crash GPU) et leurs correctifs appliqués.
- [x] Cache de texture par chemin existant (`sync_textures`,
      [`src/gfx/renderer.rs:1148-1179`](src/gfx/renderer.rs)) réutilisé tel quel — le
      branchement se fait dans `pipelines::make_texture`, en amont du cache, sans le
      dupliquer.
- [x] Feature GPU demandée à la création du device
      ([`src/gfx/renderer.rs`](src/gfx/renderer.rs), même idiome que
      `TIMESTAMP_QUERY` pour le profiler) : `& adapter.features()` garantit qu'elle
      n'est requise que si le GPU la supporte réellement.
- [x] `cargo check`, `cargo clippy -D warnings`, `cargo fmt --check` et
      `cargo check --target wasm32-unknown-unknown` verts. `cargo test` ciblé
      (`texcompress`, `golden_render`, `golden_skinning`) vert — voir note sur
      `cargo test --lib` complet dans « Ce qui n'a PAS été fait ».

## Décisions techniques et pourquoi

**BC3 plutôt que BC7.** Le sprint d'origine visait BC7 (meilleure qualité, ratio
identique à BC3 en 8 bpp) mais les encodeurs BC7 disponibles en Rust sont soit des
bindings C (`intel_tex_2`/ISPC, `bc7enc_rs`) qui ajoutent une dépendance de lien
native et un risque de build cross-plateforme, soit inexistants en pur Rust à ce
jour. `texpresso` (pur Rust, aucun lien C, compile nativement sur toutes les cibles
du projet dont `wasm32-unknown-unknown`) ne fait que BC1/BC2/BC3/BC4/BC5. **BC3
plutôt que BC1** : BC1 n'a qu'un alpha 1 bit (punch-through), qui aurait cassé les
découpes progressives du feuillage (`nature_grass_tuft.glb` ×112,
`nature_fern.glb` ×69) — BC3 garde un alpha interpolé 8 bits. Ratio obtenu : **4:1**
(32 bpp → 8 bpp) contre 8:1 pour BC1 — arbitrage qualité/mémoire documenté ici plutôt
que fait à l'aveugle.

**Chaîne de mips CPU, pas GPU.** `make_texture` existant génère les mips par blit GPU
(`RENDER_ATTACHMENT` + pipeline `mipgen`) — impossible sur un format bloc-compressé
(pas de rendu vers une texture compressée). `texcompress::downsample` refait un
filtre boîte 2×2 sur le RGBA8 source côté CPU avant de compresser chaque niveau,
jusqu'à 1×1 — la même formule que le chemin non compressé
(`pipelines::mip_count_for`, réutilisée directement plutôt que dupliquée). Coût CPU
ponctuel à l'import (une fois par texture, mise en cache ensuite comme avant), pas
par frame.

**Garde-fou dimensions : seulement un plancher, pas de contrainte de multiple.**
`texcompress::supports_compression` refuse une texture en dessous de 4×4 (en dessous,
compresser coûterait plus de VRAM que ça n'en économise — un bloc BC3 fait 16 octets
quelle que soit la taille réelle couverte). Au-dessus, `texpresso::Format::compress`
gère nativement les blocs de bord partiels via un masque (`num_blocks`, ceil-arrondi à
4) : aucune dimension n'est donc rejetée pour ne pas être multiple de 4 — un audit
initial avait imposé cette contrainte à tort avant de découvrir cette capacité de la
bibliothèque (cf. « Audit a posteriori », défaut n°1).

## Ce qui n'a PAS été fait (à traiter séparément)

- **ASTC mobile absent.** `TEXTURE_COMPRESSION_BC` n'est en pratique jamais exposé
  par les GPU Android (Adreno/Mali, ASTC natif) ni iOS — ce sprint n'a donc d'effet
  mesurable que sur les GPU desktop qui exposent la feature BC (la plupart des GPU
  discrets/intégrés récents). L'objectif « VRAM mobile/Android » de la Phase E n'est
  **pas atteint** par ce travail seul : il faudrait un chemin ASTC séparé
  (`wgpu::Features::TEXTURE_COMPRESSION_ASTC`), non implémenté ici — l'encodage ASTC
  n'a pas d'équivalent pur-Rust simple identifié (à rechercher : `astc-encoder`
  bindings C, ou compression pré-calculée hors ligne au lieu d'à l'import).
- **Pas de mesure VRAM avant/après.** Le Profiler existant (`src/editor/windows.rs`)
  n'expose pas de compteur mémoire GPU — seulement FPS/draw calls/temps de passe. Une
  mesure chiffrée du gain réel (attendu ~4× sur les textures compressées) nécessite
  soit un ajout au Profiler, soit un outil externe (Instruments/RenderDoc), à faire
  en Phase F si jugé utile.
- **Pas de vérification visuelle en jeu (`mmorpg_demo` en particulier).** Le chemin
  compressé a bien été validé sur un vrai GPU via les golden tests headless (voir
  « Audit a posteriori »), mais uniquement sur une texture de test (damier 64×64) —
  pas sur les vraies textures de personnages/créatures visées par le risque signalé
  dans le sprint d'origine. Le binaire desktop tournait déjà (session concurrente
  active pendant ce sprint, cf. `ps aux` au démarrage), donc pas de test lancé en jeu
  ici pour éviter toute interférence.
- **`cargo test --lib` complet non vert au moment de ce sprint — cause externe.** Un
  échec (`runtime::sfx::tests::synth_variation_...`) puis une erreur de compilation
  des tests (`app.update_round()` appelé sans le nouvel argument `dt: f32`) sont
  apparus en cours de session : une autre session travaillait en direct sur
  `src/app/combat.rs`/`src/app/mod.rs` (mtime de `combat.rs` à 10h49, en plein pendant
  cette session — cf. `concurrent-sessions-hazard` en mémoire). Aucun rapport avec
  `texcompress`/`pipelines`/`renderer.rs` (fichiers disjoints) : non corrigé ici,
  hors-scope de cette phase par construction (« sans toucher au fichier travaillé dans
  d'autres phases »). Les tests ciblés de cette phase (`texcompress`, golden
  render/skinning) compilent et passent indépendamment de ce problème externe.

## Fichiers touchés

| Fichier | Nature du changement |
|---|---|
| `Cargo.toml` | + dépendance `texpresso = "2.0.2"` |
| `src/gfx/texcompress.rs` | nouveau module — compression BC3 + mips CPU + 4 tests unitaires |
| `src/gfx/mod.rs` | + déclaration `mod texcompress;` |
| `src/gfx/pipelines.rs` | `make_texture` : branchement vers le chemin compressé si le GPU le supporte |
| `src/gfx/renderer.rs` | `TEXTURE_COMPRESSION_BC` ajoutée aux features demandées à `request_device` |

Diff additif uniquement : aucune ligne du chemin `Rgba8UnormSrgb` existant n'a été
modifiée au-delà de l'ajout d'un `if` en tête de `make_texture` — choisi
délibérément pour ne pas toucher au code déjà en cours de modification par ailleurs
dans `renderer.rs`/`pipelines.rs` au moment de ce sprint (plusieurs sessions actives
en parallèle sur ce dépôt, cf. `git status`/`ps aux` en début de session).

## Audit a posteriori (après livraison) — 3 défauts trouvés, **corrigés dans ce sprint**

Relecture ciblée du diff après coup (pas juste « ça compile/les tests passent »),
puis correction effective des défauts trouvés — tout dans `src/gfx/texcompress.rs`,
aucun autre fichier de cette phase ni d'une autre phase modifié pour ces correctifs.

1. **Chaîne de mips tronquée à un seul niveau pour toute texture non multiple de 8 —
   corrigé.** `compressible_mip_count` (première version) exigeait que **chaque
   niveau** reste multiple de 4, ce qui revenait à exiger un multiple de 8 dès le 1er
   niveau au-delà de la base. En creusant l'API de `texpresso`, `Format::compress`
   gère en réalité nativement les blocs de bord partiels via un masque
   (`num_blocks(n) = (n+3)/4`, ceil-arrondi — vérifié en lisant le code source de la
   dépendance, pas supposé) : cette restriction était donc inutile.
   **Correctif appliqué** : `compressible_mip_count` supprimée, `mip_count_for` de
   `pipelines.rs` réutilisée directement (même formule, même longueur de chaîne que
   le chemin non compressé) ; `supports_compression` ne garde qu'un plancher 4×4
   (raison mémoire, pas technique — voir plus haut). Vérifié par un nouveau test
   (`make_compressed_texture_builds_a_full_mip_chain_matching_the_uncompressed_path`,
   confirme que `mip_count_for(300, 300) == 9`, contre 1 seul niveau avant).
2. **Mips générés en espace gamma, pas linéaire — corrigé.** Le blit GPU du chemin
   non compressé (`mipgen.wgsl`) décode/moyenne/ré-encode en sRGB automatiquement (le
   sampler d'une vue `*UnormSrgb` fait ce travail). `downsample` moyennait les octets
   sRGB bruts directement — assombrissait les mips par rapport au chemin non
   compressé. **Correctif appliqué** : `srgb_u8_to_linear`/`linear_to_srgb_u8`
   (formule sRGB exacte, pas l'approximation gamma 2.2) autour de la moyenne 2×2 des
   canaux RGB — l'alpha, qui n'est **pas** gamma-encodé même dans un format
   `*UnormSrgb`, reste moyenné linéairement sans conversion (détail auquel il aurait
   été facile de se tromper). Vérifié par un nouveau test
   (`downsample_averages_in_linear_space_not_gamma_space`) qui distingue les deux
   comportements par la valeur numérique attendue (~188 en linéaire correct contre
   ~127/128 en gamma naïf pour une moyenne 0/255).
3. **Crash GPU réel, introduit par le correctif du défaut n°1 puis trouvé et corrigé
   avant livraison.** En étendant la chaîne de mips jusqu'à 1×1 (défaut n°1), les
   niveaux les plus petits (tailles non multiples de 4, ex. 2×2, 1×1) faisaient
   paniquer `queue.write_texture` avec `wgpu error: Copy width is not a multiple of
   block width` — capturé par le golden test existant
   `golden_textured_ground_with_mipmaps` (texture de test 64×64 → mip 1×1 atteint).
   Cause : wgpu exige que l'étendue d'une copie vers une texture compressée soit un
   multiple exact du bloc (`wgpu-core`, `validate_texture_copy_range` — vérifié en
   lisant le code source de `wgpu-core`, pas deviné), même pour le dernier mip d'une
   chaîne — pas d'exception « copie jusqu'au bord » comme je le supposais initialement.
   **Correctif appliqué** : l'étendue passée à `write_texture` utilise la taille
   « physique » du mip (`num_blocks(largeur) * 4`, arrondie au bloc supérieur), pas sa
   taille « virtuelle » (`lw`/`lh`) — exactement ce que
   `TextureDescriptor::mip_level_size(..).physical_size(..)` calcule en interne côté
   wgpu pour valider cette même copie. **Ce défaut est la preuve que la découverte du
   défaut n°1 a été vérifiée bout-en-bout sur un vrai GPU, pas seulement en unitaire**
   (voir point suivant).
4. **Correction d'une fausse affirmation de l'audit précédent : le chemin compressé
   TOURNE bien sur le GPU de cette machine.** Le premier passage d'audit affirmait
   (à tort) que cette machine (Mac Apple Silicon/Metal) n'exposait probablement pas
   `TEXTURE_COMPRESSION_BC`, et donc que le chemin BC3 n'avait jamais été exercé par
   un vrai rendu. Le crash du point 3 — provoqué précisément par ce chemin, sur cette
   machine, via un golden test headless — prouve le contraire : cette machine
   supporte bien la feature, et `golden_textured_ground_with_mipmaps` a bien
   compressé/décompressé/rendu une texture BC3 réelle. Après correctif, ce golden
   test **passe** (comparaison pixel avec tolérance contre l'image de référence,
   `tests/golden_render.rs::assert_matches_golden`) — validation bout-en-bout réelle,
   pas seulement des tests unitaires sur la logique pure.
5. **Coût de compression synchrone au premier chargement d'une scène — trouvé, non
   corrigé (nouveau, 3ᵉ passage d'audit).** `sync_textures`
   (`src/gfx/renderer.rs:1157-1179`) boucle sur tous les objets de la scène **sans
   budget de temps ni asynchronisme** : toute texture pas encore en cache est chargée
   et compressée dans le même appel, potentiellement toutes en une seule frame au
   premier affichage d'une scène (ex. bascule vers `mmorpg_demo`, ~320 clés
   mesh/texture). Avant ce sprint, ce coût était un `write_texture` quasi gratuit
   (mémoire) + un blit GPU (rapide). `Params::default()` de `texpresso` utilise
   `Algorithm::ClusterFit` (qualité correcte, pas le plus lent `IterativeClusterFit`
   ni le plus rapide `RangeFit`) — un micro-benchmark isolé (hors de ce dépôt,
   `cargo run --release` sur un projet jetable) donne, pour le niveau de base seul :
   ~1,6 ms (256×256), ~6,3 ms (512×512), ~24,7 ms (1024×1024) ; la chaîne de mips
   complète ajoute ~33 % à ce coût. Sur ~320 textures, même avec une majorité de
   petites tailles, l'accumulation peut représenter **plusieurs centaines de
   millisecondes à quelques secondes** de gel au premier chargement — je n'ai **pas**
   mesuré ce total exact sur `mmorpg_demo` (nécessiterait de lancer le binaire en
   conditions réelles, écarté pour ne pas interférer avec la session concurrente
   active, cf. plus haut), donc ce chiffre est une estimation par extrapolation du
   micro-benchmark, pas une mesure directe. **Non corrigé ici** : une vraie solution
   (budget de temps par frame façon streaming, ou compression sur un thread dédié)
   est un changement d'architecture plus large que ce qui est raisonnable à faire
   sans pouvoir tester en conditions réelles dans cette session — recommandé comme
   sprint de suivi si la Phase 0 (mesure FPS réelle) confirme un gel perceptible à
   l'ouverture de `mmorpg_demo`.

## Statut vs définition de « terminé » (Phase E)

> « Textures compressées en VRAM, qualité visuelle validée »
> (tableau récapitulatif de `sprintoptimation3daudit10h.md`)

**Nettement plus proche du vert qu'à la livraison initiale, mais pas encore complet —
et pas encore « parfait » comme le montre le point 5 ci-dessus.** L'audit a posteriori
a trouvé et corrigé 3 défauts réels dans `src/gfx/texcompress.rs` — dont un crash GPU
reproductible, désormais couvert par un golden test qui passe (validation bout-en-bout
sur un vrai GPU, pas seulement des tests unitaires) — et a identifié un 4ᵉ problème
(coût de compression synchrone au chargement) qui reste non corrigé, documenté plutôt
que caché. Restent, avant de considérer la Phase E réellement verte : (a) corriger ou
au moins mesurer précisément le point 5 (coût de chargement), (b) une validation
visuelle sur les vraies textures de `mmorpg_demo` (personnages/créatures, pas
seulement le damier de test), (c) une mesure VRAM avant/après chiffrée, (d) le volet
ASTC mobile, non traité par ce sprint.
