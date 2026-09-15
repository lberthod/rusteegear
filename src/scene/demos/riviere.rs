//! Démo « Rivière & cascade » — vitrine de rendu : une vallée boisée en U,
//! une rivière sinueuse qui descend d'un plateau par une cascade dans un
//! bassin, et une forêt dense sur les deux versants. Pas de combat ni
//! d'objectif : on se promène (joystick / WASD, saut).
//!
//! Géométrie de la vallée (terrain, nappes d'eau) générée par
//! `scripts/gen_riviere_cascade.py` (numpy, sans Blender) dans
//! `assets/models/riviere/`, avec un albédo « cuit » 2048² (herbe, humus,
//! roche sur les pentes, galets mouillés dans le lit). L'eau est rendue par la
//! branche `water` de `main.wgsl` (`SceneObject::water`). La forêt et les
//! accessoires de berge sont des modèles **générés** (`scripts/gen_riviere_vegetation.py` :
//! épicéas, pins, hêtres, bouleaux, herbes, fougères, rochers érodés) plus
//! quelques packs Blender copiés dans `assets/models/riviere/` (roseaux animés,
//! massettes, brume basse, faune),
//! posés au niveau du sol lu dans le maillage du terrain (`terrain_height`),
//! par un tirage déterministe (`Lcg`) — la scène est identique à chaque
//! chargement.
//!
//! `river_center`/`river_half_width`/`water_level` sont **recopiées** du
//! script Python : elles servent ici à exclure le lit et les berges des
//! plantations. Changer l'un sans l'autre plante des pins dans l'eau — le test
//! `riviere_demo_plants_nothing_in_the_water` le détecte.

use glam::{Quat, Vec3};

use super::creature_scripts::creature_bite_script;
use super::{
    AiChaser, AnimationState, Archetype, Combat, Controller, GameCamera, ImportedMesh, ItemKind,
    ItemPickup, Light, Locomotion, MeshKind, MobileControls, Scene, SceneObject, Sky, WeaponPickup,
    demo_obj,
};
use crate::runtime::physics::{ColliderShape, PhysicsKind};
use crate::scene::{BiteAttack, WaterKind, WaterSurface};

/// Côté du terrain (m) — `WORLD` du script.
const WORLD: f32 = 150.0;
/// Altitude du plateau amont — `H_UP` du script.
const H_UP: f32 = 9.0;
/// Arête de la falaise — `Z_LIP` du script.
const Z_LIP: f32 = -30.0;
/// Centre du bassin de réception — `POOL_Z` du script.
const POOL_Z: f32 = -24.5;

fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// x du centre du chenal (recopie de `river_center`).
pub(crate) fn river_center(z: f32) -> f32 {
    let lower = 3.0 * ((z - POOL_Z) * 0.075).sin() + 1.2 * ((z - POOL_Z) * 0.21 + 1.0).sin();
    let upper = 1.5 * ((z - Z_LIP) * 0.12).sin();
    let t = smoothstep(Z_LIP - 1.5, Z_LIP + 1.5, z);
    upper * (1.0 - t) + lower * t
}

/// Demi-largeur du chenal (recopie de `river_half_width`).
pub(crate) fn river_half_width(z: f32) -> f32 {
    let pool = (-((z - POOL_Z) / 4.5).powi(2)).exp();
    let lower = 3.6 + 3.4 * pool;
    let upper = 2.6;
    let t = smoothstep(Z_LIP - 1.5, Z_LIP + 1.5, z);
    upper * (1.0 - t) + lower * t
}

/// Niveau de l'eau (recopie de `water_level`).
pub(crate) fn water_level(z: f32) -> f32 {
    if z < Z_LIP {
        H_UP - 0.35
    } else {
        -0.02 * (z - (POOL_Z + 0.5)).max(0.0) - 0.35
    }
}

/// Distance transversale normalisée au chenal : 0 au centre, 1 sur la berge.
fn channel_dist(x: f32, z: f32) -> f32 {
    (x - river_center(z)).abs() / river_half_width(z)
}

/// Générateur congruentiel déterministe (même famille que les autres démos :
/// pas de `rand`, une scène reproductible à l'octet près).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// Hauteur du terrain importé en (x, z) : le maillage est une grille régulière
/// `(n × n)` rangée par rangée (z croissant puis x croissant, cf. le script),
/// interpolée bilinéairement. Hors de la grille : bord le plus proche.
pub(crate) fn terrain_height(mesh: &ImportedMesh, x: f32, z: f32) -> f32 {
    let verts = &mesh.data.vertices;
    let n = (verts.len() as f64).sqrt().round() as usize;
    if n < 2 || n * n != verts.len() {
        return 0.0;
    }
    let (min, max) = (mesh.aabb_min, mesh.aabb_max);
    let fx = ((x - min.x) / (max.x - min.x).max(1e-6) * (n - 1) as f32).clamp(0.0, (n - 1) as f32);
    let fz = ((z - min.z) / (max.z - min.z).max(1e-6) * (n - 1) as f32).clamp(0.0, (n - 1) as f32);
    let ix = (fx.floor() as usize).min(n - 2);
    let iz = (fz.floor() as usize).min(n - 2);
    let tx = fx - ix as f32;
    let tz = fz - iz as f32;
    let h = |ix: usize, iz: usize| verts[iz * n + ix].position[1];
    let h0 = h(ix, iz) * (1.0 - tx) + h(ix + 1, iz) * tx;
    let h1 = h(ix, iz + 1) * (1.0 - tx) + h(ix + 1, iz + 1) * tx;
    h0 * (1.0 - tz) + h1 * tz
}

/// Pente locale (tangente) par différence finie sur 1,5 m.
fn terrain_slope(mesh: &ImportedMesh, x: f32, z: f32) -> f32 {
    let e = 0.75;
    let dx = (terrain_height(mesh, x + e, z) - terrain_height(mesh, x - e, z)) / (2.0 * e);
    let dz = (terrain_height(mesh, x, z + e) - terrain_height(mesh, x, z - e)) / (2.0 * e);
    (dx * dx + dz * dz).sqrt()
}

/// Chemin d'un asset de la démo : dossier `assets/models/` sur disque en natif,
/// `embedded://` (compilé dans le `.wasm`, cf. `assets::embedded_bytes`) sur le web.
fn asset_path(file: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        format!("{}{file}", crate::assets::EMBEDDED_SCHEME)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file)
    }
}

/// Chargeur de glTF partagé : un même fichier n'est chargé qu'une fois, toutes
/// les instances pointent la même entrée de `Scene::imported`.
struct Loader {
    imported: Vec<ImportedMesh>,
}

impl Loader {
    fn load(&mut self, file: &str) -> Option<u32> {
        let path = asset_path(file);
        if let Some(i) = self.imported.iter().position(|m| m.path == path) {
            return Some(i as u32);
        }
        match crate::scene::import::load_gltf(&path) {
            Ok((data, aabb_min, aabb_max)) => {
                let mut mesh = ImportedMesh {
                    path,
                    data,
                    aabb_min,
                    aabb_max,
                    ..Default::default()
                };
                mesh.load_skinning();
                self.imported.push(mesh);
                Some((self.imported.len() - 1) as u32)
            }
            Err(e) => {
                log::error!("Démo rivière ({file}) : {e}");
                None
            }
        }
    }
}

/// Espèce plantée : fichier, échelle min/max, solide (tronc) ou non, teinte
/// (multiplie les couleurs du modèle : rochers assombris, feuillages variés).
struct Species {
    file: &'static str,
    scale: (f32, f32),
    solid: bool,
    tint: [f32; 3],
}

const fn sp(file: &'static str, lo: f32, hi: f32, solid: bool) -> Species {
    Species {
        file,
        scale: (lo, hi),
        solid,
        tint: [1.0, 1.0, 1.0],
    }
}

/// Un emplacement de monstre : espèce (mesh + tempérament de poursuite/
/// morsure) et poste fixe — cf. la doc du bloc « Monstres » plus bas pour le
/// contexte. Pas de `const fn` possible pour tout le tableau (`x` dépend de
/// `river_center`, une fonction ordinaire) : `monster_roster` reste un
/// tableau local construit à chaque appel de `riviere_demo`, comme l'était
/// `monster_spots` avant cette variation d'espèces.
struct MonsterSpot {
    /// Nom affiché (HUD, éditeur) — sert aussi de base au préfixe `save` Lua
    /// unique par instance (cf. la boucle de construction).
    species: &'static str,
    file: &'static str,
    x: f32,
    z: f32,
    scale: f32,
    /// Teinte multiplicative (`SceneObject::color`) : `Some(...)` seulement
    /// pour le renard (le distingue d'un coup d'œil du renard décoratif
    /// inoffensif de la faune ci-dessus) — les autres espèces gardent leurs
    /// couleurs de modèle d'origine (`None` ⇒ blanc neutre, cf. `demo_obj`).
    tint: Option<[f32; 3]>,
    hp: u32,
    speed: f32,
    archetype: Archetype,
    /// (cooldown, chance, dégâts) de la morsure — cf. `BiteAttack` et
    /// `creature_bite_script`.
    bite: (f32, f32, f32),
}

/// Arbres des versants et du plateau (pondérations cumulées, tirage sur `r`).
/// Modèles générés par `scripts/gen_riviere_vegetation.py` (épicéa = 10 m,
/// hêtre = 9 m à l'échelle 1). `near` : premier plan (au bord de la rivière,
/// côté caméra) → modèles détaillés ; sinon variantes `_lod` (≈ 6 fois moins
/// de triangles — ≈ 1 500 arbres, 5 passes de rendu par image, le budget
/// compte).
fn pick_tree(r: f32, high: bool, near: bool) -> Species {
    if !near {
        return if r < (if high { 0.85 } else { 0.6 }) {
            sp("riviere/epicea_lod.glb", 0.9, 1.6, true)
        } else {
            sp("riviere/hetre_lod.glb", 0.9, 1.4, true)
        };
    }
    if high {
        // Crêtes : conifères surtout.
        if r < 0.42 {
            sp("riviere/epicea_a.glb", 0.9, 1.6, true)
        } else if r < 0.8 {
            sp("riviere/epicea_b.glb", 0.9, 1.6, true)
        } else {
            sp("riviere/pin_a.glb", 1.0, 1.5, true)
        }
    } else if r < 0.27 {
        sp("riviere/epicea_a.glb", 0.8, 1.5, true)
    } else if r < 0.52 {
        sp("riviere/epicea_b.glb", 0.8, 1.5, true)
    } else if r < 0.64 {
        sp("riviere/pin_a.glb", 0.9, 1.4, true)
    } else if r < 0.78 {
        sp("riviere/hetre_a.glb", 0.9, 1.4, true)
    } else if r < 0.9 {
        sp("riviere/hetre_b.glb", 0.9, 1.4, true)
    } else {
        sp("riviere/bouleau_a.glb", 0.8, 1.25, true)
    }
}

/// Sous-bois et lisière : fougères, touffes d'herbe, quelques pierres.
fn pick_undergrowth(r: f32) -> Species {
    if r < 0.34 {
        sp("riviere/fougere.glb", 1.0, 1.7, false)
    } else if r < 0.62 {
        sp("riviere/herbe_touffe.glb", 1.2, 2.2, false)
    } else if r < 0.82 {
        sp("riviere/herbe_haute.glb", 1.0, 1.8, false)
    } else if r < 0.88 {
        sp("riviere/nature_mushrooms.glb", 1.0, 1.6, false)
    } else if r < 0.94 {
        sp("riviere/nature_clover_patch.glb", 2.0, 3.5, false)
    } else {
        sp("riviere/rocher_b.glb", 0.3, 0.7, false)
    }
}

/// Végétation et pierres de berge (roseaux animés, massettes, herbes, galets).
fn pick_bank(r: f32) -> Species {
    if r < 0.25 {
        sp("riviere/nature_reeds_sway.glb", 1.1, 1.7, false)
    } else if r < 0.42 {
        sp("riviere/nature_cattails.glb", 0.9, 1.4, false)
    } else if r < 0.52 {
        sp("riviere/nature_pampas_sway.glb", 1.0, 1.5, false)
    } else if r < 0.74 {
        sp("riviere/herbe_haute.glb", 1.0, 1.8, false)
    } else if r < 0.88 {
        sp("riviere/galet_a.glb", 0.45, 0.95, true)
    } else {
        sp("riviere/galet_b.glb", 0.45, 0.95, true)
    }
}

impl Scene {
    /// Vitrine de rendu « Rivière & cascade » (cf. la doc du module).
    pub fn riviere_demo() -> Self {
        let mut loader = Loader {
            imported: Vec::new(),
        };
        let mut objects: Vec<SceneObject> = Vec::new();

        // --- Terrain ---
        let Some(terrain_idx) = loader.load("riviere/terrain_vallee.glb") else {
            log::error!(
                "Démo rivière : terrain introuvable — lancer `python3 scripts/gen_riviere_cascade.py`"
            );
            return Scene::demo();
        };
        let terrain = loader.imported[terrain_idx as usize].clone();
        let mut sol = demo_obj("Vallée", MeshKind::Imported(terrain_idx), Vec3::ZERO);
        sol.physics = PhysicsKind::Static;
        sol.collider_shape = ColliderShape::TriMesh;
        sol.texture = asset_path("riviere/terrain_vallee_albedo.png");
        sol.roughness = 0.95;
        sol.group = "Vallée".into();
        objects.push(sol);

        // --- Enceinte invisible (14 septembre 2026, demande « un mur autour de
        // la carte ») : quatre parois statiques épaisses posées sur le bord du
        // terrain importé. Le maillage s'arrête net à ±WORLD/2 — au-delà, rien
        // sous les pieds : un joueur qui marchait, sautait ou se ruait
        // (`AppState::update_dash`) hors de la grille tombait dans le vide sans
        // fin. Invisibles (`visible = false` ne retire pas le collider d'un corps
        // **fixe**, cf. `physics::build` : seuls les kinématiques et capteurs
        // masqués sont ignorés), assez hautes pour qu'aucun saut ni relief ne
        // passe par-dessus, enfoncées bien sous le sol pour ne laisser aucune
        // fente au ras des berges. La ruée les voit aussi : son rayon sonde la
        // couche 0 (toutes les couches par défaut ici), et s'arrête à l'impact.
        // Les longueurs débordent de deux épaisseurs pour boucher les coins.
        {
            let (min, max) = (terrain.aabb_min, terrain.aabb_max);
            let thick = 2.0;
            // Paroi centrée sur l'arête : la moitié intérieure (1 m) mord sur le
            // dernier mètre du terrain, où `terrain_height` plaque déjà le relief
            // sur le bord — rien d'intéressant n'y est planté (cf. les marges de
            // plantation plus bas).
            let cx = (min.x + max.x) * 0.5;
            let cz = (min.z + max.z) * 0.5;
            let cy = (min.y + max.y) * 0.5;
            let height = (max.y - min.y) + 60.0;
            let len_x = (max.x - min.x) + 2.0 * thick;
            let len_z = (max.z - min.z) + 2.0 * thick;
            let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
                let mut w = demo_obj(name, MeshKind::Cube, pos);
                w.transform = w.transform.with_scale(scale);
                w.physics = PhysicsKind::Static;
                w.collider_shape = ColliderShape::Box;
                w.visible = false;
                w.group = "Enceinte".into();
                objects.push(w);
            };
            wall(
                "Mur Nord",
                Vec3::new(cx, cy, min.z),
                Vec3::new(len_x, height, thick),
            );
            wall(
                "Mur Sud",
                Vec3::new(cx, cy, max.z),
                Vec3::new(len_x, height, thick),
            );
            wall(
                "Mur Est",
                Vec3::new(max.x, cy, cz),
                Vec3::new(thick, height, len_z),
            );
            wall(
                "Mur Ouest",
                Vec3::new(min.x, cy, cz),
                Vec3::new(thick, height, len_z),
            );
        }

        // --- Eau ---
        let water = |loader: &mut Loader,
                     objects: &mut Vec<SceneObject>,
                     name: &str,
                     file: &str,
                     w: WaterSurface,
                     opacity: f32,
                     color: [f32; 3]| {
            if let Some(idx) = loader.load(file) {
                let mut o = demo_obj(name, MeshKind::Imported(idx), Vec3::ZERO);
                o.water = Some(w);
                o.opacity = opacity;
                o.color = color;
                o.roughness = 0.05;
                o.group = "Eau".into();
                objects.push(o);
            }
        };
        water(
            &mut loader,
            &mut objects,
            "Rivière amont",
            "riviere/eau_haute.glb",
            WaterSurface {
                kind: WaterKind::Riviere,
                flow_speed: 1.4,
                scale: 0.9,
                foam: 0.7,
            },
            0.72,
            [0.78, 0.9, 0.86],
        );
        water(
            &mut loader,
            &mut objects,
            "Cascade",
            "riviere/cascade.glb",
            WaterSurface {
                kind: WaterKind::Cascade,
                flow_speed: 5.5,
                scale: 1.0,
                foam: 1.0,
            },
            0.55,
            [0.9, 0.95, 1.0],
        );
        water(
            &mut loader,
            &mut objects,
            "Bassin et rivière aval",
            "riviere/eau_basse.glb",
            WaterSurface {
                kind: WaterKind::Riviere,
                flow_speed: 1.0,
                scale: 0.9,
                foam: 0.8,
            },
            0.7,
            [0.78, 0.9, 0.86],
        );

        // Embruns au pied de la chute : impostors (croix de plans) en brume
        // animée, plus un voile horizontal sur le bassin.
        let impact = Vec3::new(0.0, water_level(POOL_Z), Z_LIP + 3.2);
        for (i, (dx, dz, sx, sy)) in [
            (0.0, 0.6, 7.5, 6.5),
            (-2.4, 1.6, 5.5, 4.5),
            (2.6, 1.4, 5.5, 4.8),
            (0.4, 3.4, 6.0, 3.5),
        ]
        .into_iter()
        .enumerate()
        {
            let mut m = demo_obj(
                &format!("Embruns {}", i + 1),
                MeshKind::Billboard,
                impact + Vec3::new(dx, -0.3, dz),
            );
            m.transform = m.transform.with_scale(Vec3::new(sx, sy, sx));
            m.water = Some(WaterSurface {
                kind: WaterKind::Brume,
                flow_speed: 1.0 + 0.2 * i as f32,
                scale: 1.0,
                foam: 0.0,
            });
            m.opacity = 0.5;
            m.color = [0.9, 0.93, 0.96];
            m.group = "Eau".into();
            objects.push(m);
        }
        let mut voile = demo_obj(
            "Voile du bassin",
            MeshKind::Plane,
            impact + Vec3::new(0.0, 0.35, 2.5),
        );
        voile.transform = voile.transform.with_scale(Vec3::new(16.0, 1.0, 12.0));
        voile.water = Some(WaterSurface {
            kind: WaterKind::Brume,
            flow_speed: 0.5,
            scale: 1.5,
            foam: 0.0,
        });
        voile.opacity = 0.3;
        voile.color = [0.92, 0.94, 0.97];
        voile.group = "Eau".into();
        objects.push(voile);

        // --- Joueur ---
        // Petite créature ronde skinnée (`creature_ronde.glb`, modélisée/riggée/
        // animée à la main dans Blender via MCP — corps unique + oreilles/ailerons/
        // yeux/bras/pieds trapus, clips « Idle »/« Walk »/« Run » en dandinement)
        // plutôt qu'une capsule nue ou un humain : `Locomotion` (analyse comparative
        // 2026-09-04) choisit le clip et le mélange à partir de la vitesse
        // horizontale mesurée, sans script. Le collider physique reste une capsule
        // explicite (dérivée de l'AABB du mesh importé, pieds à l'origine) : le
        // rendu skinné ne change rien à la simulation.
        let start_z = 24.0;
        let start_x = river_center(start_z) + river_half_width(start_z) + 2.5;
        let start_y = terrain_height(&terrain, start_x, start_z) + 1.0;
        let joueur_mesh = loader.load("riviere/creature_ronde.glb");
        let mut joueur = demo_obj(
            "Joueur",
            joueur_mesh
                .map(MeshKind::Imported)
                .unwrap_or(MeshKind::Capsule),
            Vec3::new(start_x, start_y, start_z),
        );
        joueur.color = [1.0, 1.0, 1.0];
        joueur.tag = "joueur".into();
        joueur.physics = PhysicsKind::Kinematic;
        joueur.collider_shape = ColliderShape::Capsule;
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.4,
            attack_button: "Mêlée".into(),
            heal_button: "Soin".into(),
            block_button: "Bouclier".into(),
            dash_button: "Ruée".into(),
            ..Default::default()
        });
        if joueur_mesh.is_some() {
            joueur.animation = Some(AnimationState {
                clip: "Idle".into(),
                locomotion: Some(Locomotion {
                    walk_speed: 2.0,
                    run_speed: 4.4,
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        objects.push(joueur);

        // --- Forêt ---
        let mut rng = Lcg(0x5eed_2026_0911);
        let place = |loader: &mut Loader,
                     objects: &mut Vec<SceneObject>,
                     rng: &mut Lcg,
                     group: &str,
                     s: &Species,
                     x: f32,
                     z: f32,
                     sink: f32| {
            let Some(idx) = loader.load(s.file) else {
                return;
            };
            let y = terrain_height(&terrain, x, z) - sink;
            let n = objects.len();
            let mut o = demo_obj(
                &format!("{group} {n}"),
                MeshKind::Imported(idx),
                Vec3::new(x, y, z),
            );
            let sc = rng.range(s.scale.0, s.scale.1);
            o.transform = o.transform.with_scale(Vec3::splat(sc));
            o.transform.rotation = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU));
            if s.solid {
                o.physics = PhysicsKind::Static;
            }
            o.color = s.tint;

            // Pierres et bois : mats (le spéculaire par défaut les rendait laiteux).

            if s.file.contains("rocher") || s.file.contains("galet") || s.file.contains("tronc") {
                o.roughness = 0.92;
            }
            // Feuillages : variation de teinte par individu (plus sombre, plus
            // jaune, plus bleuté) — une forêt d'arbres tous identiques trahit le
            // clonage bien plus que la géométrie basse résolution.
            if group == "Arbre" || group == "Sous-bois" {
                let dark = rng.range(0.68, 1.0);
                let warm = rng.range(-0.08, 0.08);
                o.color = [
                    (s.tint[0] * dark * (1.0 + warm)).clamp(0.0, 1.0),
                    (s.tint[1] * dark).clamp(0.0, 1.0),
                    (s.tint[2] * dark * (1.0 - warm)).clamp(0.0, 1.0),
                ];
            }
            o.group = group.into();
            objects.push(o);
        };

        // Arbres : grille gigotée de 3,4 m ; clairière le long de la rivière,
        // forêt dense au-delà ; jamais dans le lit, sur la berge ni sur la falaise.
        let step = 4.0;
        let cells = (WORLD / step) as i32;
        let trees = |x: f32, z: f32, rng: &mut Lcg| -> Option<(Species, bool)> {
            if x.abs() > 73.0 || z.abs() > 73.0 {
                return None;
            }
            let d = channel_dist(x, z);
            if d < 1.9 {
                return None;
            }
            let h = terrain_height(&terrain, x, z);
            if h < water_level(z) + 0.3 || terrain_slope(&terrain, x, z) > 1.05 {
                return None;
            }
            let keep = if d < 3.2 {
                0.3
            } else if d < 6.0 {
                0.62
            } else {
                0.9
            };
            if rng.next() > keep {
                return None;
            }
            let r = rng.next();
            let near = d < 11.0 && z > -46.0;
            Some((pick_tree(r, h > 13.0, near), d < 9.0))
        };
        for iz in 0..cells {
            for ix in 0..cells {
                let x = -WORLD / 2.0 + (ix as f32 + 0.5 + rng.range(-0.45, 0.45)) * step;
                let z = -WORLD / 2.0 + (iz as f32 + 0.5 + rng.range(-0.45, 0.45)) * step;
                if let Some((mut s, near)) = trees(x, z, &mut rng) {
                    // Les troncs loin de la promenade n'ont pas besoin de collider.
                    s.solid = s.solid && near;
                    place(&mut loader, &mut objects, &mut rng, "Arbre", &s, x, z, 0.05);
                }
            }
        }

        // Sous-bois : bande de 1,3 à 11 demi-largeurs de chaque côté de la rivière.
        let ustep = 1.7;
        let uz = (WORLD / ustep) as i32;
        for iz in 0..uz {
            let z = -WORLD / 2.0 + (iz as f32 + 0.5) * ustep;
            if z.abs() > 73.0 {
                continue;
            }
            let half = river_half_width(z);
            let xc = river_center(z);
            let span = (half * 11.0 / ustep) as i32;
            for k in -span..=span {
                let x = xc + k as f32 * ustep + rng.range(-0.9, 0.9);
                let zz = z + rng.range(-0.9, 0.9);
                let d = channel_dist(x, zz);
                if !(1.3..11.0).contains(&d) || x.abs() > 73.0 {
                    continue;
                }
                if terrain_height(&terrain, x, zz) < water_level(zz) + 0.3
                    || terrain_slope(&terrain, x, zz) > 1.2
                {
                    continue;
                }
                if rng.next() > 0.62 {
                    continue;
                }
                let s = pick_undergrowth(rng.next());
                place(
                    &mut loader,
                    &mut objects,
                    &mut rng,
                    "Sous-bois",
                    &s,
                    x,
                    zz,
                    0.02,
                );
            }
        }

        // Berges : roseaux, massettes, mousse et galets au ras de l'eau.
        let bstep = 1.6;
        let bz = (WORLD / bstep) as i32;
        for iz in 0..bz {
            let z = -WORLD / 2.0 + (iz as f32 + 0.5) * bstep + rng.range(-0.5, 0.5);
            if z.abs() > 73.0 || (Z_LIP - 1.0..Z_LIP + 3.5).contains(&z) {
                continue;
            }
            for side in [-1.0, 1.0] {
                if rng.next() > 0.7 {
                    continue;
                }
                let d = rng.range(1.04, 1.45);
                let x = river_center(z) + side * d * river_half_width(z);
                let h = terrain_height(&terrain, x, z);
                if h > water_level(z) + 1.3 || h < water_level(z) - 0.2 {
                    continue;
                }
                let s = pick_bank(rng.next());
                // Les galets/rochers de berge sont enfoncés (ils ne « posent » pas).
                let sink = if s.file.contains("galet") { 0.12 } else { 0.03 };
                place(&mut loader, &mut objects, &mut rng, "Berge", &s, x, z, sink);
            }
        }

        // Rochers dans le lit (émergés), bois flotté, troncs moussus, souches.
        let rstep = 5.5;
        let rz = (WORLD / rstep) as i32;
        for iz in 0..rz {
            let z = -WORLD / 2.0 + (iz as f32 + 0.5) * rstep + rng.range(-2.0, 2.0);
            if z.abs() > 72.0 || (Z_LIP - 3.0..Z_LIP + 8.0).contains(&z) || rng.next() > 0.6 {
                continue;
            }
            let x = river_center(z) + rng.range(-0.8, 0.8) * river_half_width(z);
            let s = match rng.next() {
                r if r < 0.4 => sp("riviere/galet_a.glb", 0.9, 1.9, true),
                r if r < 0.8 => sp("riviere/galet_b.glb", 0.9, 1.9, true),
                _ => sp("riviere/tronc_mort.glb", 0.5, 0.9, false),
            };
            // Enfoncés dans le lit : seul le sommet émerge, l'eau les lèche.
            place(
                &mut loader,
                &mut objects,
                &mut rng,
                "Rivière",
                &s,
                x,
                z,
                0.35,
            );
        }
        for _ in 0..14 {
            let z = rng.range(-70.0, 70.0);
            if (Z_LIP - 4.0..Z_LIP + 8.0).contains(&z) {
                continue;
            }
            let side = if rng.next() < 0.5 { -1.0 } else { 1.0 };
            let x = river_center(z) + side * rng.range(1.6, 4.0) * river_half_width(z);
            if terrain_height(&terrain, x, z) < water_level(z) + 0.35 {
                continue;
            }
            let s = sp("riviere/tronc_mort.glb", 0.8, 1.3, true);
            place(
                &mut loader,
                &mut objects,
                &mut rng,
                "Sous-bois",
                &s,
                x,
                z,
                0.06,
            );
        }

        // Falaise : blocs moussus le long de l'arête et en éboulis, gros rochers
        // qui encadrent la lèvre et le bassin.
        let mut x = -46.0;
        while x < 46.0 {
            x += rng.range(2.4, 4.2);
            if x.abs() < 3.6 {
                continue;
            }
            let z = Z_LIP + rng.range(-0.8, 1.6) + 0.12 * (x.abs() - 5.0).max(0.0);
            let s = match rng.next() {
                r if r < 0.4 => sp("riviere/rocher_a.glb", 0.8, 1.9, true),
                r if r < 0.75 => sp("riviere/rocher_b.glb", 0.8, 1.9, true),
                _ => sp("riviere/rocher_c.glb", 0.7, 1.7, true),
            };
            place(
                &mut loader,
                &mut objects,
                &mut rng,
                "Falaise",
                &s,
                x,
                z,
                0.75,
            );
        }
        for (x, z, sc) in [
            (-3.9, Z_LIP - 0.6, 2.0),
            (3.9, Z_LIP - 0.4, 1.9),
            (-6.8, Z_LIP + 4.5, 1.7),
            (7.2, Z_LIP + 5.0, 1.8),
            (-8.5, POOL_Z + 2.0, 1.5),
            (8.8, POOL_Z + 1.0, 1.6),
        ] {
            let s = sp("riviere/rocher_a.glb", sc, sc, true);
            place(
                &mut loader,
                &mut objects,
                &mut rng,
                "Falaise",
                &s,
                x,
                z,
                0.6,
            );
        }

        // Grands hêtres penchés sur le bassin, nappes de brume basse le long de
        // la rivière (au ras de l'eau, dans les creux).
        for (x, z) in [(-10.5, POOL_Z + 6.0), (11.0, POOL_Z + 8.5)] {
            let s = sp("riviere/hetre_b.glb", 1.3, 1.5, true);
            place(&mut loader, &mut objects, &mut rng, "Berge", &s, x, z, 0.05);
        }
        for (i, (x, z, sc)) in [
            (0.0, POOL_Z + 1.5, 13.0),
            (-4.0, POOL_Z + 6.0, 10.0),
            (5.0, POOL_Z + 9.0, 9.0),
            (river_center(-5.0), -5.0, 9.0),
            (river_center(12.0), 12.0, 8.0),
            (river_center(40.0), 40.0, 10.0),
            (river_center(60.0), 60.0, 9.0),
        ]
        .into_iter()
        .enumerate()
        {
            let file = if i % 2 == 0 {
                "riviere/shore_low_fog.glb"
            } else {
                "riviere/siege_low_mist.glb"
            };
            if let Some(idx) = loader.load(file) {
                let mut m = demo_obj(
                    &format!("Brume {}", i + 1),
                    MeshKind::Imported(idx),
                    Vec3::new(x, water_level(z) + 0.05, z),
                );
                m.transform = m.transform.with_scale(Vec3::new(sc, sc * 0.6, sc));
                m.transform.rotation = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU));
                m.opacity = 0.45;
                m.color = [0.9, 0.93, 0.96];
                m.group = "Eau".into();
                objects.push(m);
            }
        }

        // Faune discrète : un héron au bassin, des canards, des cerfs en lisière.
        for (file, x, z, sc, sink) in [
            ("riviere/fauna_heron.glb", 6.5, POOL_Z + 5.5, 1.3, 0.0),
            (
                "riviere/fauna_duck.glb",
                river_center(8.0) + 1.0,
                8.0,
                1.2,
                0.0,
            ),
            (
                "riviere/fauna_duck.glb",
                river_center(9.5) - 0.8,
                9.5,
                1.1,
                0.0,
            ),
            (
                "riviere/fauna_deer.glb",
                river_center(45.0) - 9.0,
                45.0,
                1.4,
                0.0,
            ),
            (
                "riviere/fauna_deer.glb",
                river_center(47.0) - 12.0,
                47.0,
                1.3,
                0.0,
            ),
            (
                "riviere/fauna_rabbit.glb",
                river_center(20.0) + 7.0,
                20.0,
                1.2,
                0.0,
            ),
            (
                "riviere/fauna_fox.glb",
                river_center(-50.0) + 6.0,
                -50.0,
                1.2,
                0.0,
            ),
        ] {
            if let Some(idx) = loader.load(file) {
                let on_water = file.contains("duck");
                let y = if on_water {
                    water_level(z) - 0.05
                } else {
                    terrain_height(&terrain, x, z) - sink
                };
                let mut a = demo_obj(
                    &format!("Faune {}", objects.len()),
                    MeshKind::Imported(idx),
                    Vec3::new(x, y, z),
                );
                a.transform = a.transform.with_scale(Vec3::splat(sc));
                a.transform.rotation = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU));
                a.group = "Faune".into();
                objects.push(a);
            }
        }

        // --- Monstres (14 septembre 2026, PvE/PvP : kit de capacités 1-2-3-4 ;
        // bestiaire varié depuis le 14 septembre 2026 au soir) ---
        // Huit espèces plutôt que six clones du même renard : table
        // data-driven sur le patron de `MMORPG_CREATURES`
        // (`scene/demos/mmorpg/creatures.rs`) — une boucle unique construit
        // chaque `SceneObject` à partir d'une entrée `MonsterSpot`, au lieu
        // d'un bloc dédié par espèce. `x`/`z` ne sont pas `const` (calculés
        // via `river_center`, pas une fonction `const fn`) : ce tableau est
        // construit au premier appel de `riviere_demo`, pas figé à la
        // compilation comme `MMORPG_CREATURES`.
        //
        // Cinq des six postes historiques sont conservés tels quels (spots
        // #1/#2/#4/#5/#6 ci-dessous, mêmes coordonnées qu'avant cette
        // variation) ; le spot #3 (lisière est, peu caractéristique) cède la
        // place à quatre nouveaux postes pensés pour chaque nouvelle espèce
        // (berge peu profonde pour la grenouille, brume du bassin pour le
        // fantôme, plateau rocheux amont pour le golem, amas de champignons
        // du sous-bois pour le mordeur) — 9 monstres au total, comme les 6
        // d'origine. Échelles reprises telles quelles de la démo MMORPG
        // (`scene/demos/mmorpg/decor_data.rs`, table `MONSTER_DECOR`) où ces
        // mêmes fichiers sont déjà posés et calibrés visuellement : pas de
        // nouveau réglage à l'aveugle.
        //
        // Poursuite native (`AiChaser`, pas de script d'errance nécessaire,
        // cf. sa doc) ; seule la morsure est scriptée
        // (`creature_bite_script`), pour que le joueur **solo** (sans
        // serveur réseau) subisse aussi des dégâts — la résolution native de
        // `BiteAttack` (`app::health::update_creature_bite`) ne s'applique
        // qu'aux joueurs réseau, cf. sa doc. PV/vitesse/tempérament de
        // morsure varient par espèce (`Archetype::hp_multiplier` n'est PAS
        // appliqué automatiquement, cf. `scene/mod.rs` — chaque `hp` est
        // fixé en dur ci-dessous, même règle que `MMORPG_CREATURES`) : le
        // Golem encaisse le plus et frappe le plus fort mais reste le plus
        // lent (`Archetype::Colosse`, facile à semer), les Blobs et la
        // Grenouille sont les plus fragiles.
        let monster_roster: [MonsterSpot; 9] = [
            MonsterSpot {
                species: "Renard enragé 1",
                file: "riviere/fauna_fox.glb",
                x: river_center(-20.0) + 7.0,
                z: -20.0,
                scale: 1.3,
                tint: Some([0.55, 0.12, 0.08]),
                hp: 3,
                speed: 2.2,
                archetype: Archetype::Traqueuse,
                bite: (1.8, 0.5, 0.12),
            },
            MonsterSpot {
                species: "Maraudeur (orc)",
                file: "monster_orc.glb",
                x: river_center(-38.0) - 7.5,
                z: -38.0,
                scale: 0.9,
                tint: None,
                hp: 4,
                speed: 2.4,
                archetype: Archetype::Traqueuse,
                bite: (1.5, 0.55, 0.15),
            },
            MonsterSpot {
                species: "Grenouille des berges",
                file: "monster_frog.glb",
                // Berge peu profonde près du départ, jamais dans le lit
                // (même contrainte `channel_dist`/`water_level` que la
                // végétation, cf. `riviere_demo_plants_nothing_in_the_water`,
                // reprise pour les monstres par
                // `riviere_demo_monsters_never_spawn_in_or_too_close_to_the_water`).
                // Décalée de z=-8 à z=-5 (relecture du 15 septembre 2026) :
                // à -8, à seulement 12.0 m du Renard enragé 1 (poste
                // historique, z=-20), elle tombait dans la zone de
                // recouvrement de leurs portées d'éveil (9 m Traqueuse +
                // 5 m Furtive = 14 m de marge minimale requise, cf.
                // `CHASER_DETECT_RANGE`/`FURTIVE_DETECT_RANGE` dans
                // `app::simulation`) — double-pull quasi garanti dès qu'on
                // réveillait l'un des deux. À -5, la distance passe à 15.0 m.
                x: river_center(-5.0) + 7.0,
                z: -5.0,
                scale: 0.9,
                tint: None,
                hp: 2,
                // Vitesse EFFECTIVE = `speed * archetype.speed_multiplier()`
                // (`app::simulation::sim_step`, ×1.5 pour `Furtive`) — PAS la
                // vitesse finale en jeu (bug de relecture du 15 septembre
                // 2026 : l'ancienne valeur 1.6 était en fait déjà une
                // vitesse-cible, donnant un effectif de 2.4, aussi rapide que
                // le Maraudeur/orc — cassait l'intention « la Grenouille est
                // le maillon faible du roster »). 1.1 × 1.5 = 1.65 effectif :
                // la plus lente du roster mobile (Colosse mis à part, déjà
                // volontairement le plus lent par son propre multiplicateur
                // ×0.65, cf. le Golem ci-dessous). Même patron que le
                // Squelette `Furtive` de `scene/demos/roguelike.rs`
                // (speed=2.4, volontairement sous le Gobelin `Traqueuse` à
                // 3.2) et les créatures `Furtive` de
                // `scene/demos/mmorpg/creatures.rs` (chase_speed=2.8 contre
                // 3.0-3.4) : la vitesse de base saisie ici tient déjà compte
                // du ×1.5, ce n'est jamais la vitesse finale voulue.
                speed: 1.1,
                archetype: Archetype::Furtive,
                bite: (1.5, 0.45, 0.08),
            },
            MonsterSpot {
                species: "Fantôme de la brume",
                file: "monster_ghost.glb",
                // Nappe de brume basse en aval du bassin (même famille de
                // décor que les nappes `riviere/shore_low_fog.glb`/
                // `riviere/siege_low_mist.glb` posées le long de la rivière,
                // dont une exactement à x=river_center(12)+8, z=12, cf. la
                // boucle plus haut) — plutôt qu'au centre du bassin de
                // réception lui-même (relecture du 15 septembre 2026) : à
                // x=9.0/z=POOL_Z, il n'était qu'à 4.5 m du Renard enragé 1
                // (poste historique, x≈9.1/z=-20) — très en-dessous des 14 m
                // (9 m Traqueuse + 5 m Furtive) requis pour ne jamais
                // recouvrir sa portée d'éveil (double-pull garanti dès
                // qu'on réveillait l'un des deux). À x≈-5.0/z=12, la
                // distance passe à environ 35 m du Renard 1 (et reste ≥ 20 m
                // de tout autre monstre du roster, marge confortable au-delà
                // du minimum 14 m).
                x: river_center(12.0) - 7.0,
                z: 12.0,
                scale: 0.75,
                tint: None,
                hp: 2,
                // Même bug de vitesse effective que la Grenouille ci-dessus
                // (2.0 était déjà lu comme vitesse-cible, donnant un
                // effectif de 3.0 — plus rapide que TOUT le reste du
                // roster, y compris le Renard à 2.2 et l'Orc à 2.4). 1.4 ×
                // 1.5 = 2.1 effectif : furtif mais pas le monstre le plus
                // rapide de la démo.
                speed: 1.4,
                archetype: Archetype::Furtive,
                bite: (1.6, 0.5, 0.1),
            },
            MonsterSpot {
                species: "Golem des berges",
                file: "monster_goleling.glb",
                // Plateau amont rocheux, entre les postes #2 et #6 (renard
                // amont / renard aval) — sur la berge opposée (est plutôt
                // qu'ouest) et bien plus loin du centre du chenal que les
                // deux renards (relecture du 15 septembre 2026) : à l'ancien
                // poste (même berge, offset -8), il n'était qu'à 8.0 m de
                // l'Orc et 12.2 m du Renard enragé 2 — bien en-dessous des
                // 18 m (9 m + 9 m, deux `Traqueuse`/`Colosse`) requis. À
                // offset +18 (berge est), il reste à ~25 m des deux (marge
                // de sécurité `channel_dist`/`water_level` vérifiée aussi,
                // cf. `riviere_demo_monsters_never_spawn_in_or_too_close_to_the_water`
                // — la rive amont est presque plane, un offset trop faible
                // au-dessus du chenal y retombe sous le niveau d'eau du
                // plateau).
                x: river_center(-46.0) + 18.0,
                z: -46.0,
                scale: 0.9,
                tint: None,
                hp: 6,
                speed: 1.2,
                archetype: Archetype::Colosse,
                bite: (2.5, 0.6, 0.22),
            },
            MonsterSpot {
                species: "Blob des sous-bois (vert)",
                file: "monster_green_blob.glb",
                x: river_center(32.0) - 7.0,
                z: 32.0,
                scale: 1.3,
                tint: None,
                hp: 2,
                speed: 1.6,
                archetype: Archetype::Meute,
                bite: (1.4, 0.5, 0.09),
            },
            MonsterSpot {
                species: "Blob des sous-bois (rose)",
                file: "monster_pink_blob.glb",
                x: river_center(56.0) + 7.5,
                z: 56.0,
                scale: 1.3,
                tint: None,
                hp: 2,
                speed: 1.6,
                archetype: Archetype::Meute,
                bite: (1.4, 0.5, 0.09),
            },
            MonsterSpot {
                species: "Champignon mordeur",
                file: "monster_mushnub.glb",
                // Sous-bois, près d'un amas de champignons déjà planté
                // (`riviere/nature_mushrooms.glb`) — décalé de z=45 à
                // z=36.5, plus loin dans le sous-bois (offset +12 plutôt que
                // +7.5), relecture du 15 septembre 2026 : à l'ancien poste,
                // il n'était qu'à 11.0 m du Blob rose (z=56) — bien
                // en-dessous des 18 m (9 m + 9 m, deux `Traqueuse`/`Meute`)
                // requis. Au nouveau poste, ~20 m des deux Blobs.
                x: river_center(36.5) + 12.0,
                z: 36.5,
                scale: 1.0,
                tint: None,
                hp: 2,
                speed: 1.4,
                archetype: Archetype::Traqueuse,
                bite: (1.7, 0.5, 0.1),
            },
            MonsterSpot {
                species: "Renard enragé 2",
                file: "riviere/fauna_fox.glb",
                x: river_center(-58.0) - 7.0,
                z: -58.0,
                scale: 1.3,
                tint: Some([0.55, 0.12, 0.08]),
                hp: 3,
                speed: 2.2,
                archetype: Archetype::Traqueuse,
                bite: (1.8, 0.5, 0.12),
            },
        ];
        for (i, spot) in monster_roster.into_iter().enumerate() {
            let Some(idx) = loader.load(spot.file) else {
                continue;
            };
            let y = terrain_height(&terrain, spot.x, spot.z);
            let mut m = demo_obj(spot.species, MeshKind::Imported(idx), Vec3::new(spot.x, y, spot.z));
            m.transform = m.transform.with_scale(Vec3::splat(spot.scale));
            m.transform.rotation = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU));
            if let Some(tint) = spot.tint {
                m.color = tint;
            }
            m.tag = "monstre".into();
            m.group = "Monstre".into();
            m.physics = PhysicsKind::Kinematic;
            // Détection de contact pour la morsure (cf. `creature_bite_script`),
            // sans changer le collider (toujours solide).
            m.trigger = true;
            m.combat = Some(Combat {
                attackable: true,
                // `wave: 0` : pas de système de manches, actif dès le départ —
                // comme les créatures du hameau MMORPG (monde ouvert, pas une
                // arène par manches).
                hp: spot.hp,
                ..Default::default()
            });
            m.ai_chaser = Some(AiChaser {
                speed: spot.speed,
                archetype: spot.archetype,
            });
            let (bite_cooldown, bite_chance, bite_damage) = spot.bite;
            m.bite = Some(BiteAttack {
                cooldown: bite_cooldown,
                chance: bite_chance,
                damage: bite_damage,
            });
            // Réapparaît après un délai plutôt que de disparaître pour de bon :
            // même politique que les créatures du hameau MMORPG (contenu
            // renouvelé, pas un stock fini à épuiser une fois pour toutes).
            m.respawn_delay = 20.0;
            // Préfixe unique par **instance** (index global dans le roster),
            // pas par espèce : deux monstres de même espèce (les deux
            // renards, les deux blobs) ne doivent jamais partager leur
            // namespace `save` (cooldown de morsure notamment), sans quoi
            // l'un écraserait l'état de l'autre.
            let prefix = format!("m{i}_");
            m.script = creature_bite_script(
                &prefix,
                bite_cooldown,
                bite_chance,
                bite_damage,
                17.0 + i as f32 * 5.3,
            );
            objects.push(m);
        }

        // --- Boss (15 septembre 2026, évènement de fin de parcours) ---
        // « L'Aîné de la Cascade » : un dragon ancien retranché sur le plateau
        // amont, près de la chute — le seul monstre de la démo à réutiliser
        // `monster_dragon_evolved.glb` (inemployé ailleurs ici, donc
        // immédiatement reconnaissable dans le bestiaire). Nom, tag et groupe
        // dédiés (PAS "monstre"/"Monstre") : il ne doit pas se compter parmi
        // les 9 emplacements du roster ordinaire ci-dessus (cf.
        // `riviere_demo_monsters_are_varied_species` et consorts) — un
        // évènement de fin de parcours, pas un monstre de plus dans la
        // rotation. Phases (vitesse/attaque à distance selon le ratio de PV)
        // dans `app::boss`, PAS `RoundObjective::Boss` (un mode de salon
        // Hameau câblé sur les manches — `Combat::wave`, sans rapport avec ce
        // monde ouvert où `wave` vaut 0 partout, cf. la doc de `Combat::wave`
        // et celle du module `app::boss`). Le nom ci-dessous doit rester
        // identique à `app::boss::BOSS_NAME` — verrouillé par
        // `riviere_demo_boss_names_match_app_boss_module` ci-dessous (`scene`
        // ne dépend pas de `app`, d'où la chaîne recopiée plutôt qu'une
        // constante partagée, même convention que `creature_attack::
        // RANGED_CREATURE_ATTACKS` qui repère ses créatures par nom recopié
        // depuis `scene::demos::mmorpg`).
        //
        // Position hors du chenal comme le reste du roster (`channel_dist >=
        // 1.3`, cf. `riviere_demo_monsters_never_spawn_in_or_too_close_to_the_water`,
        // reprise pour le boss par
        // `riviere_demo_boss_is_placed_on_dry_ground_away_from_the_channel`) :
        // le plateau au bord du chenal, pas en son centre. Au point le plus
        // haut et le plus reculé de la vallée en U (plateau amont, très en
        // amont du Renard enragé 2, le monstre le plus reculé du roster
        // ordinaire, z=-58) — cohérent avec le statut d'évènement de fin de
        // parcours.
        //
        // (20.0, -40.0) — poste initial — n'était qu'à 6.9 m du Golem des
        // berges (x≈16.6, z=-46), tous deux `Archetype::Colosse` (portée
        // d'éveil 9 m chacun, cf. `CHASER_DETECT_RANGE`) : très en-dessous
        // des 18 m (9 m + 9 m) requis pour ne jamais recouvrir leurs portées
        // d'éveil — double-pull garanti dès qu'on réveillait l'un des deux,
        // alors que ce même fichier applique scrupuleusement cette règle à
        // toutes les autres paires du roster (cf. les docs ci-dessus, par
        // exemple sur la Grenouille ou le Champignon mordeur). Relecture du
        // 15 septembre 2026 : déplacé à (20.0, -70.0), toujours sur la même
        // berge est (même signe d'offset que l'ancien poste), à 24.2 m du
        // Golem (marge de 6.2 m au-delà du minimum requis) et à 29.2 m du
        // Renard enragé 2 — le monstre le plus proche restant, marge
        // confortable au-delà du minimum sur toutes les paires, dans le même
        // esprit que le reste du fichier.
        const BOSS_NAME: &str = "L'Aîné de la Cascade";
        const BOSS_LOOT_NAME: &str = "Arc de l'Aînée";
        let boss_x = 20.0;
        let boss_z = -70.0;
        let boss_y = terrain_height(&terrain, boss_x, boss_z);
        if let Some(idx) = loader.load("monster_dragon_evolved.glb") {
            let mut boss = demo_obj(
                BOSS_NAME,
                MeshKind::Imported(idx),
                Vec3::new(boss_x, boss_y, boss_z),
            );
            boss.transform = boss.transform.with_scale(Vec3::splat(2.4));
            boss.transform.rotation = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU));
            boss.tag = "boss".into();
            boss.group = "Boss".into();
            boss.physics = PhysicsKind::Kinematic;
            // Détection de contact pour la morsure (cf. `creature_bite_script`,
            // même dispositif que le reste du roster ci-dessus).
            boss.trigger = true;
            boss.combat = Some(Combat {
                attackable: true,
                // `wave: 0` : pas de système de manches, comme tout le reste
                // de cette démo — cf. la doc du bloc plus haut.
                hp: 60,
                ..Default::default()
            });
            // Vitesse/archétype d'authoring : réécrits chaque tick par
            // `app::boss::update_boss` selon la phase (ratio de PV) — ces
            // valeurs ne comptent que tant que le boss n'a pas encore encaissé
            // de dégât (avant le premier appel).
            boss.ai_chaser = Some(AiChaser {
                speed: 1.6,
                archetype: Archetype::Colosse,
            });
            boss.bite = Some(BiteAttack {
                cooldown: 2.0,
                chance: 0.55,
                damage: 0.28,
            });
            // Ne réapparaît que très rarement (pas de mécanisme probabiliste
            // dédié dans ce moteur, cf. `Combat`/`respawn_delay` : seulement 0
            // = jamais, ou une durée fixe) : 10 minutes plutôt qu'un farm
            // trivial, cohérent avec un évènement de fin de parcours.
            boss.respawn_delay = 600.0;
            boss.script = creature_bite_script("boss_", 2.0, 0.55, 0.28, 137.0);
            objects.push(boss);

            // Butin garanti à la mort (cf. `app::boss::update_boss_loot`) :
            // posé dès la construction, masqué — sa visibilité est ensuite
            // dérivée de celle du boss à chaque pas fixe, pas ramassable tant
            // que le boss est en vie. Arc (`WEAPONS[4]`), seule arme jamais
            // posée en butin ailleurs dans cette démo (les trois butins
            // d'arme ci-dessous couvrent Épée/Lance/Marteau) — distincte,
            // cohérent avec un butin de boss exclusif.
            if let Some(lidx) = loader.load("riviere/galet_a.glb") {
                let mut loot = demo_obj(
                    BOSS_LOOT_NAME,
                    MeshKind::Imported(lidx),
                    Vec3::new(boss_x + 1.6, boss_y + 0.1, boss_z + 1.2),
                );
                loot.transform = loot.transform.with_scale(Vec3::splat(0.55));
                loot.color = [0.85, 0.75, 0.35];
                loot.group = "Butin".into();
                loot.visible = false;
                loot.weapon_pickup = Some(WeaponPickup { weapon: 4 });
                objects.push(loot);
            }
        }

        // --- Second boss (15 septembre 2026, évènement de fin de parcours) ---
        // « Le Roi-Champignon du Sous-bois » : un patriarche fongique
        // retranché dans le sous-bois le plus profond et le plus reculé de
        // la vallée, en aval — à l'opposé géographique ET thématique de
        // « L'Aîné de la Cascade » ci-dessus (plateau rocheux amont, rive
        // est) : ici rive OUEST, au-delà du dernier poste du roster
        // ordinaire (Blob des sous-bois rose, z=56), jusqu'à la limite du
        // monde (z=75). Prolonge le thème champignons/sous-bois déjà amorcé
        // par le Champignon mordeur et `riviere/nature_mushrooms.glb` —
        // `monster_mushroom_king.glb`, inemployé ailleurs ici. Nom, tag et
        // groupe dédiés (PAS "monstre"/"Monstre", PAS "boss"/"Boss" non plus
        // : un second boss distinct, pas un doublon du premier) — ne doit se
        // compter ni parmi les 9 emplacements du roster ordinaire, ni comme
        // le premier boss (cf. `riviere_demo_boss_is_not_counted_among_the_
        // ordinary_monster_roster` et son pendant `riviere_demo_second_boss_
        // is_not_counted_among_the_ordinary_monster_roster_or_the_first_boss`
        // ci-dessous). Phases (vitesse/zone de spores/invocation de couvée
        // selon le ratio de PV) dans `app::boss2`, mécaniquement distinct du
        // premier boss (cf. sa doc) : aucun jet télégraphié, une zone de
        // dégâts au sol ancrée sur la position du joueur plutôt qu'un
        // projectile visé.
        //
        // Position hors du chenal comme le reste du roster (`channel_dist >=
        // 1.3`, même contrainte que le premier boss) : rive ouest à
        // channel_dist ≈ 5.0 (calculé, cf. `riviere_demo_second_boss_is_
        // placed_on_dry_ground_away_from_the_channel`), largement hors de
        // l'eau. Distance vérifiée à la construction (calcul manuel, 15
        // septembre 2026) aux deux voisins les plus proches du roster
        // ordinaire : ≈ 42 m du Champignon mordeur (x≈10.6, z=36.5) et
        // ≈ 24,9 m du Blob des sous-bois rose (x≈5.8, z=56, `Archetype::
        // Meute`) — au-dessus des 18 m (9 m Colosse + 9 m Meute) requis pour
        // ne jamais recouvrir leurs portées d'éveil (cf. `riviere_demo_
        // second_boss_stays_out_of_every_monsters_combined_aggro_range`),
        // avec une marge de ≈ 6,9 m au-delà du minimum — même ordre de
        // grandeur que la marge retenue pour le Golem des berges plus haut.
        const BOSS2_NAME: &str = "Le Roi-Champignon du Sous-bois";
        const BOSS2_LOOT_NAME: &str = "Sceptre du Roi Champignon";
        let boss2_x = river_center(70.0) - 18.0;
        let boss2_z = 70.0;
        let boss2_y = terrain_height(&terrain, boss2_x, boss2_z);
        if let Some(idx) = loader.load("monster_mushroom_king.glb") {
            let mut boss2 = demo_obj(
                BOSS2_NAME,
                MeshKind::Imported(idx),
                Vec3::new(boss2_x, boss2_y, boss2_z),
            );
            boss2.transform = boss2.transform.with_scale(Vec3::splat(2.2));
            boss2.transform.rotation = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU));
            boss2.tag = "boss2".into();
            boss2.group = "Boss2".into();
            boss2.physics = PhysicsKind::Kinematic;
            boss2.trigger = true;
            boss2.combat = Some(Combat {
                attackable: true,
                hp: 70,
                ..Default::default()
            });
            // Vitesse/archétype d'authoring : réécrits chaque tick par
            // `app::boss2::update_boss2` selon la phase — mêmes valeurs
            // d'avant-premier-coup que le premier boss ci-dessus.
            boss2.ai_chaser = Some(AiChaser {
                speed: 1.4,
                archetype: Archetype::Colosse,
            });
            boss2.bite = Some(BiteAttack {
                cooldown: 2.2,
                chance: 0.5,
                damage: 0.24,
            });
            boss2.respawn_delay = 600.0;
            boss2.script = creature_bite_script("boss2_", 2.2, 0.5, 0.24, 191.0);
            objects.push(boss2);

            // Butin garanti à la mort (cf. `app::boss2::update_boss2_loot`,
            // même patron que le premier boss) : Sceptre (`WEAPONS[5]`),
            // profil de mêlée dédié à ce boss (cf. la doc de `scene::WEAPONS`)
            // — `Scene::roguelike_demo` exclut délibérément cet indice de son
            // propre tirage, ce Sceptre reste exclusif à ce boss.
            if let Some(lidx) = loader.load("riviere/galet_a.glb") {
                let mut loot = demo_obj(
                    BOSS2_LOOT_NAME,
                    MeshKind::Imported(lidx),
                    Vec3::new(boss2_x + 1.4, boss2_y + 0.1, boss2_z - 1.3),
                );
                loot.transform = loot.transform.with_scale(Vec3::splat(0.55));
                loot.color = [0.65, 0.85, 0.35];
                loot.group = "Butin".into();
                loot.visible = false;
                loot.weapon_pickup = Some(WeaponPickup { weapon: 5 });
                objects.push(loot);
            }

            // Couvée de trois Champiblobs (cf. `app::boss2::BOSS2_MINION_TAG`) :
            // pré-placés tout près du boss, masqués dès la construction —
            // révélés progressivement en combat (2 en Prolifération, le 3e
            // en Éruption des spores) par `update_boss2`, jamais par un
            // script Lua de spawn (aucun système de spawn dynamique dans ce
            // moteur). Réutilise le mordeur de champignon déjà chargé
            // (`monster_mushnub.glb`, même mesh que le Champignon mordeur du
            // roster ordinaire un peu plus en amont) plutôt qu'un nouvel
            // asset : de petites créatures fongiques, cohérent avec la
            // couvée d'un boss champignon. `AiChaser`/`BiteAttack`/`Combat`
            // déjà branchés pour que la simulation générique les fasse vivre
            // dès que `update_boss2` les rend visibles — aucune logique d'IA
            // nouvelle à écrire pour eux. Tag/groupe dédiés : EXCLUS du
            // roster ordinaire (`tag == "monstre"`) et de ses tests de
            // variété d'espèces (cf. `riviere_demo_monsters_are_varied_
            // species` et consorts, qui filtrent strictement par ce tag).
            let minion_offsets: [(f32, f32); 3] =
                [(2.5, 2.0), (-2.0, 3.0), (3.0, -2.5)];
            for (i, (dx, dz)) in minion_offsets.into_iter().enumerate() {
                let Some(midx) = loader.load("monster_mushnub.glb") else {
                    continue;
                };
                let mx = boss2_x + dx;
                let mz = boss2_z + dz;
                let my = terrain_height(&terrain, mx, mz);
                let mut minion = demo_obj(
                    &format!("Champiblob {}", i + 1),
                    MeshKind::Imported(midx),
                    Vec3::new(mx, my, mz),
                );
                minion.transform = minion.transform.with_scale(Vec3::splat(0.85));
                minion.color = [0.7, 0.35, 0.75];
                minion.tag = "boss2_minion".into();
                minion.group = "Boss2".into();
                minion.physics = PhysicsKind::Kinematic;
                minion.trigger = true;
                // Masqué dès la construction : révélé uniquement par
                // `app::boss2::update_boss2` au fil du combat, cf. la doc
                // ci-dessus (pas de couvée active hors combat contre le
                // premier boss).
                minion.visible = false;
                minion.combat = Some(Combat {
                    attackable: true,
                    hp: 3,
                    ..Default::default()
                });
                minion.ai_chaser = Some(AiChaser {
                    speed: 1.7,
                    archetype: Archetype::Meute,
                });
                minion.bite = Some(BiteAttack {
                    cooldown: 1.6,
                    chance: 0.5,
                    damage: 0.1,
                });
                // Pas de réapparition individuelle (`respawn_delay` par
                // défaut à 0) : la couvée entière revient avec le boss, via
                // `update_boss2`, pas via la file de réapparition générique.
                let prefix = format!("boss2_minion{i}_");
                minion.script = creature_bite_script(&prefix, 1.6, 0.5, 0.1, 211.0 + i as f32 * 7.0);
                objects.push(minion);
            }
        }

        // --- Butin (armes de mêlée et soins à ramasser au contact) ---
        // Réutilise les rochers/galets déjà chargés comme socle visuel du
        // butin (pas de nouvel asset) : l'arme/l'objet est ramassé au contact
        // du rocher qui le « porte », comme un point de butin marqué au sol.
        let loot_weapons: [(f32, f32, usize); 3] = [
            // Épée près du départ : première amélioration facile à trouver.
            (river_center(20.0) - 6.0, 20.0, 1),
            // Lance au plateau amont, plus loin, plus risqué (monstres alentour).
            (river_center(-30.0) + 8.0, -30.0, 2),
            // Marteau (zone) près de la cascade : la pièce la plus tardive à
            // trouver, cohérent avec sa préparation/recharge les plus longues.
            (6.5, POOL_Z + 8.0, 3),
        ];
        for (i, (x, z, weapon)) in loot_weapons.into_iter().enumerate() {
            let Some(idx) = loader.load("riviere/galet_a.glb") else {
                continue;
            };
            let y = terrain_height(&terrain, x, z) + 0.1;
            let mut p = demo_obj(
                &format!("Butin arme {}", i + 1),
                MeshKind::Imported(idx),
                Vec3::new(x, y, z),
            );
            p.transform = p.transform.with_scale(Vec3::splat(0.55));
            p.color = [0.95, 0.82, 0.25];
            p.group = "Butin".into();
            p.weapon_pickup = Some(WeaponPickup { weapon });
            objects.push(p);
        }
        let loot_items: [(f32, f32, ItemKind, u32); 4] = [
            (river_center(6.0) + 3.0, 6.0, ItemKind::Baie, 2),
            (river_center(-12.0) - 3.5, -12.0, ItemKind::Baie, 2),
            (river_center(42.0) + 4.0, 42.0, ItemKind::Potion, 1),
            (-7.0, POOL_Z + 5.0, ItemKind::Potion, 1),
        ];
        for (i, (x, z, kind, count)) in loot_items.into_iter().enumerate() {
            let Some(idx) = loader.load("riviere/galet_b.glb") else {
                continue;
            };
            let y = terrain_height(&terrain, x, z) + 0.08;
            let mut p = demo_obj(
                &format!("Butin soin {}", i + 1),
                MeshKind::Imported(idx),
                Vec3::new(x, y, z),
            );
            p.transform = p.transform.with_scale(Vec3::splat(0.4));
            p.color = if kind == ItemKind::Potion {
                [0.85, 0.2, 0.35]
            } else {
                [0.3, 0.75, 0.25]
            };
            p.group = "Butin".into();
            p.item_pickup = Some(ItemPickup { kind, count });
            objects.push(p);
        }

        // Ordre de dessin ≈ du plus proche au plus lointain depuis le départ du
        // joueur (le renderer garde l'ordre de scène à l'intérieur d'un même
        // maillage) : le test de profondeur précoce rejette alors la plupart des
        // fragments de feuillage cachés — la scène est limitée par le surdessin
        // des feuilles, pas par les triangles. Le joueur reste avant le décor.
        let start = Vec3::new(start_x, start_y, start_z);
        let first_decor = objects
            .iter()
            .position(|o| o.controller.is_some())
            .map(|i| i + 1)
            .unwrap_or(0);
        objects[first_decor..].sort_by(|a, b| {
            a.transform
                .position
                .distance_squared(start)
                .total_cmp(&b.transform.position.distance_squared(start))
        });

        let groups = [
            "Vallée",
            "Eau",
            "Arbre",
            "Sous-bois",
            "Berge",
            "Rivière",
            "Falaise",
            "Faune",
            "Monstre",
            "Boss",
            "Boss2",
            "Butin",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let mut scene = Scene {
            objects,
            imported: loader.imported,
            groups,
            // Soleil bas derrière la cascade, un peu à gauche (contre-jour de fin
            // de journée) : halo dans le ciel au-dessus de la chute, éclats sur
            // toute la rivière, versants en contre-jour modelés par l'ambiante.
            light: Light {
                dir: [-0.28, 0.4, -0.87],
                color: [1.0, 0.86, 0.66],
                ambient: 0.4,
            },
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                // Ordre = grille d'action 2 colonnes bas-droite (cf. `TouchZones::layout`,
                // remplissage ligne par ligne, `col = i % 2`/`row = i / 2`). Avec 5
                // boutons (nombre impair), la dernière rangée n'a qu'une case — colonne
                // 0, PAS colonne 1 (la plus proche du bord droit) — donc le bouton mis en
                // dernier n'atterrit PAS près du pouce, contrairement à une intuition
                // "dernier = plus proche". Calcul exact (`ACTION_BUTTON`/`ACTION_SPACING`
                // = 64/8pt, pas de 72pt par case) : la case colonne 1/rangée 1 (indice
                // impair 1 ou 3) tombe exactement sur le bord droit historique du bouton
                // Saut solo (avant l'ajout des 4 autres boutons), et la case colonne
                // 1/rangée 1 (indice 3) est en plus la rangée pleine la plus basse — la
                // meilleure approximation disponible de la position historique. Saut est
                // donc placé en indice 3 (colonne 1, rangée 1, ~72pt au-dessus de sa
                // position solo d'origine mais alignée avec son bord droit), entre Mêlée
                // (indice 1, colonne 1, même bord droit mais une rangée plus haut) et
                // Bouclier (indice 4, seul sur la dernière rangée, colonne 0, aligné avec
                // le bas historique mais décalé de 72pt vers la gauche) — les trois
                // boutons les plus utilisés en combat rapproché occupent ainsi les trois
                // cases les plus proches du pouce. Ruée (indice 2) et Soin (indice 0,
                // situationnel, le moins fréquent) héritent des deux cases restantes,
                // les plus éloignées du pouce au repos.
                buttons: vec![
                    "Soin".into(),
                    "Mêlée".into(),
                    "Ruée".into(),
                    "Saut".into(),
                    "Bouclier".into(),
                ],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: Some(GameCamera {
                target: [start_x, start_y, start_z],
                yaw: 0.0,
                pitch: 0.18,
                distance: 9.0,
                ortho_height: 0.0,
                min_width: 0.0,
            }),
            // Brume légère (30 % à 80 m) : de la profondeur sans laver l'image.
            sky: Sky {
                horizon_color: [0.60, 0.60, 0.60],
                zenith_color: [0.24, 0.42, 0.76],
                fog_color: [0.66, 0.64, 0.62],
                fog_density: 0.0055,
                bloom_intensity: 0.25,
                // Soleil visible dans le ciel ; brume qui stagne dans la vallée
                // (pleine densité au niveau de l'eau, moitié 6 m plus haut).
                sun_glow: 0.8,
                fog_height_base: -1.0,
                fog_height_falloff: 0.11,
            },
            hud_layout: Default::default(),
            hud_widgets: Vec::new(),
            platformer: None,
            // Monde PvE/PvP (14 septembre 2026, kit de capacités 1-2-3-4) : HUD
            // de combat normal (vie, frags, roster), comme le hameau MMORPG —
            // `arcade_hud` ne cache plus rien ici, contrairement à la version
            // « vitrine de rendu » sans monstre d'avant ce jour.
            arcade_hud: false,
            // Souris capturée au clic (Échap libère), trackpad, doigt sur
            // mobile, molette = zoom — déjà le comportement par défaut d'une
            // scène non-arcade, ce champ n'a donc plus d'effet mais reste à
            // `true` par cohérence documentaire.
            arcade_free_camera: true,
            // 1 Mêlée / 2 Bouclier / 3 Sort / 4 Ruée (cf. `Scene::ability_bar`).
            ability_bar: true,
            hud_widgets_hidden: false,
            version: Scene::CURRENT_VERSION,
        };
        scene.ensure_default_animations();
        log::info!(
            "Démo rivière : {} modèles importés, {} objets (terrain : {} sommets)",
            scene.imported.len(),
            scene.objects.len(),
            terrain.data.vertices.len()
        );
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riviere_demo_has_terrain_water_forest_and_a_player() {
        let scene = Scene::riviere_demo();
        assert!(
            scene.imported.len() >= 8,
            "imports : {}",
            scene.imported.len()
        );
        let kinds: Vec<WaterKind> = scene
            .objects
            .iter()
            .filter_map(|o| o.water.map(|w| w.kind))
            .collect();
        assert!(kinds.contains(&WaterKind::Riviere));
        assert!(kinds.contains(&WaterKind::Cascade));
        assert!(kinds.contains(&WaterKind::Brume));
        let trees = scene.objects.iter().filter(|o| o.group == "Arbre").count();
        assert!(trees > 500, "forêt clairsemée : {trees} arbres");
        let player = scene
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("un joueur pilotable");
        assert!(player.transform.position.y > water_level(player.transform.position.z));
        assert!(
            scene.objects.iter().any(|o| !o.texture.is_empty()),
            "terrain texturé"
        );
    }

    /// L'enceinte (demande du 14 septembre 2026) : quatre parois statiques,
    /// invisibles mais **avec** collider (corps fixe), qui bordent exactement
    /// le terrain importé — un joueur, un saut ou une ruée ne peuvent plus
    /// sortir de la grille et tomber dans le vide. Vérifie que chaque bord du
    /// terrain est couvert par une paroi qui le dépasse en hauteur des deux
    /// côtés, et que rien de la scène n'est planté au-delà de l'enceinte.
    #[test]
    fn riviere_demo_is_fenced_by_invisible_static_walls() {
        let scene = Scene::riviere_demo();
        let terrain = scene
            .objects
            .iter()
            .find(|o| o.name == "Vallée")
            .and_then(|o| match o.mesh {
                MeshKind::Imported(i) => scene.imported.get(i as usize).cloned(),
                _ => None,
            })
            .expect("terrain importé");
        let (min, max) = (terrain.aabb_min, terrain.aabb_max);
        let walls: Vec<&SceneObject> = scene
            .objects
            .iter()
            .filter(|o| o.group == "Enceinte")
            .collect();
        assert_eq!(walls.len(), 4, "quatre parois attendues");
        for w in &walls {
            assert_eq!(w.physics, PhysicsKind::Static, "{} doit être fixe", w.name);
            assert!(!w.visible, "{} doit être invisible", w.name);
            let (lo, hi) = scene.world_aabb(w);
            assert!(
                lo.y < min.y - 5.0 && hi.y > max.y + 5.0,
                "{} ne couvre pas toute la hauteur du terrain : {lo:?}..{hi:?}",
                w.name
            );
        }
        // Chaque bord du terrain est recouvert par une paroi (l'arête est à
        // l'intérieur de sa boîte, sur toute sa longueur, coins compris).
        let covered = |p: Vec3| {
            walls.iter().any(|w| {
                let (lo, hi) = scene.world_aabb(w);
                (lo.x..=hi.x).contains(&p.x) && (lo.z..=hi.z).contains(&p.z)
            })
        };
        let mid_y = (min.y + max.y) * 0.5;
        for t in 0..=20 {
            let f = t as f32 / 20.0;
            let x = min.x + (max.x - min.x) * f;
            let z = min.z + (max.z - min.z) * f;
            assert!(covered(Vec3::new(x, mid_y, min.z)), "bord nord ouvert en x={x}");
            assert!(covered(Vec3::new(x, mid_y, max.z)), "bord sud ouvert en x={x}");
            assert!(covered(Vec3::new(min.x, mid_y, z)), "bord ouest ouvert en z={z}");
            assert!(covered(Vec3::new(max.x, mid_y, z)), "bord est ouvert en z={z}");
        }
        // Rien (joueur, arbres, monstres…) n'est posé hors de l'enceinte.
        for o in &scene.objects {
            if o.group == "Enceinte" {
                continue;
            }
            let p = o.transform.position;
            assert!(
                p.x >= min.x && p.x <= max.x && p.z >= min.z && p.z <= max.z,
                "{} hors de l'enceinte : {p:?}",
                o.name
            );
        }
    }

    #[test]
    fn riviere_demo_plants_nothing_in_the_water() {
        let scene = Scene::riviere_demo();
        for o in &scene.objects {
            if o.group != "Arbre" && o.group != "Sous-bois" {
                continue;
            }
            let p = o.transform.position;
            assert!(
                channel_dist(p.x, p.z) >= 1.3 && p.y >= water_level(p.z) + 0.25,
                "{} planté dans l'eau : {p:?}",
                o.name
            );
        }
    }

    /// Même garde-fou que `riviere_demo_plants_nothing_in_the_water`, pour
    /// le groupe "Monstre" (relecture du 15 septembre 2026 : rien
    /// n'empêchait jusqu'ici un monstre du roster d'être planté dans le lit
    /// de la rivière — ça ne cassait rien par hasard, la marge de hauteur du
    /// terrain suffisait à chaque poste actuel, mais rien ne le garantissait
    /// pour un futur ajout/déplacement). Réutilise les mêmes fonctions
    /// utilitaires (`channel_dist`, `water_level`) et le même seuil de
    /// sécurité (`channel_dist >= 1.3`) que la végétation.
    #[test]
    fn riviere_demo_monsters_never_spawn_in_or_too_close_to_the_water() {
        let scene = Scene::riviere_demo();
        let monsters: Vec<&SceneObject> = scene.objects.iter().filter(|o| o.group == "Monstre").collect();
        assert_eq!(monsters.len(), 9, "9 monstres attendus dans le groupe \"Monstre\"");
        for m in &monsters {
            let p = m.transform.position;
            assert!(
                channel_dist(p.x, p.z) >= 1.3 && p.y >= water_level(p.z) + 0.25,
                "{} planté dans l'eau ou trop près du lit : {p:?} (channel_dist={}, water_level={})",
                m.name,
                channel_dist(p.x, p.z),
                water_level(p.z)
            );
        }
    }

    /// Non-régression (relecture du 15 septembre 2026) : le boss doit rester
    /// hors de la portée d'éveil combinée de tout autre monstre du roster,
    /// comme le reste de ce fichier l'applique scrupuleusement à chaque paire
    /// (cf. les commentaires de `monster_roster` ci-dessus). Portées d'éveil
    /// dupliquées ici en constantes littérales — mêmes valeurs que
    /// `app::simulation::CHASER_DETECT_RANGE`/`FURTIVE_DETECT_RANGE` (privées,
    /// `scene` ne dépend pas de `app`), déjà dupliquées telles quelles dans
    /// les commentaires de `monster_roster`. Le boss est `Archetype::Colosse`
    /// (portée `CHASER_DETECT_RANGE`, comme le Golem) : un poste à moins de
    /// 18 m (9 m + 9 m) du Golem des berges rendrait un double-pull
    /// quasi garanti dès qu'on réveille l'un des deux.
    #[test]
    fn riviere_demo_boss_stays_out_of_every_monsters_combined_aggro_range() {
        const CHASER_DETECT_RANGE: f32 = 9.0;
        const FURTIVE_DETECT_RANGE: f32 = 5.0;
        let scene = Scene::riviere_demo();
        let boss = scene
            .objects
            .iter()
            .find(|o| o.group == "Boss")
            .expect("le boss doit être présent dans la démo Rivière");
        let boss_pos = boss.transform.position;
        let boss_range = CHASER_DETECT_RANGE; // Colosse.
        for m in scene.objects.iter().filter(|o| o.tag == "monstre") {
            let range = if m.ai_chaser.as_ref().unwrap().archetype == Archetype::Furtive {
                FURTIVE_DETECT_RANGE
            } else {
                CHASER_DETECT_RANGE
            };
            let required = boss_range + range;
            let dist = boss_pos.distance(m.transform.position);
            assert!(
                dist > required,
                "{} : {dist:.1} m du boss, en-dessous du minimum requis {required:.1} m (double-pull possible)",
                m.name
            );
        }
    }

    /// Même garde-fou que ci-dessus, pour le boss et son butin garanti : posés
    /// hors du chenal, sur la terre ferme du plateau (pas confondus avec le
    /// groupe "Monstre" — ils ont leurs propres groupes "Boss"/"Butin", cf.
    /// leur doc dans `riviere_demo`).
    #[test]
    fn riviere_demo_boss_is_placed_on_dry_ground_away_from_the_channel() {
        let scene = Scene::riviere_demo();
        let boss = scene
            .objects
            .iter()
            .find(|o| o.group == "Boss")
            .expect("le boss doit être présent dans la démo Rivière");
        let p = boss.transform.position;
        assert!(
            channel_dist(p.x, p.z) >= 1.3 && p.y >= water_level(p.z) + 0.25,
            "{} planté dans l'eau ou trop près du lit : {p:?}",
            boss.name
        );
        let loot = scene
            .objects
            .iter()
            .find(|o| o.name == "Arc de l'Aînée")
            .expect("le butin du boss doit être présent dans la démo Rivière");
        let lp = loot.transform.position;
        assert!(
            channel_dist(lp.x, lp.z) >= 1.3 && lp.y >= water_level(lp.z) + 0.25,
            "{} planté dans l'eau ou trop près du lit : {lp:?}",
            loot.name
        );
    }

    /// Le boss est un évènement de fin de parcours, pas un monstre de plus :
    /// il ne doit JAMAIS se compter parmi les 9 emplacements du roster
    /// ordinaire (groupe "Monstre"/tag "monstre") vérifiés par
    /// `riviere_demo_monsters_are_varied_species` et consorts — sans quoi ces
    /// comptages fixes casseraient silencieusement.
    #[test]
    fn riviere_demo_boss_is_not_counted_among_the_ordinary_monster_roster() {
        let scene = Scene::riviere_demo();
        assert!(
            scene.objects.iter().any(|o| o.group == "Boss"),
            "le boss doit exister dans son propre groupe"
        );
        assert!(
            scene.objects.iter().filter(|o| o.tag == "monstre").count() == 9,
            "le roster ordinaire doit rester à 9, le boss ne doit pas y être tagué"
        );
        assert!(
            scene.objects.iter().filter(|o| o.group == "Monstre").count() == 9
        );
    }

    /// Verrou de cohérence des chaînes (cf. la doc du bloc « Boss » de
    /// `riviere_demo`) : `scene` ne dépend pas de `app`, les noms du boss et
    /// de son butin sont donc recopiés en toutes lettres à deux endroits
    /// (`riviere_demo` et `app::boss`) plutôt que par une constante partagée
    /// — ce test les garde synchronisés (toute divergence casse `app::boss`,
    /// qui retrouve le boss par nom dans la scène).
    #[test]
    fn riviere_demo_boss_names_match_app_boss_module() {
        let scene = Scene::riviere_demo();
        assert!(
            scene
                .objects
                .iter()
                .any(|o| o.name == crate::app::boss::BOSS_NAME),
            "aucun objet nommé `app::boss::BOSS_NAME` dans la démo Rivière"
        );
        assert!(
            scene
                .objects
                .iter()
                .any(|o| o.name == crate::app::boss::BOSS_LOOT_NAME),
            "aucun objet nommé `app::boss::BOSS_LOOT_NAME` dans la démo Rivière"
        );
    }

    /// Pendant de `riviere_demo_boss_stays_out_of_every_monsters_combined_
    /// aggro_range`, pour le second boss (sous-bois aval).
    #[test]
    fn riviere_demo_second_boss_stays_out_of_every_monsters_combined_aggro_range() {
        const CHASER_DETECT_RANGE: f32 = 9.0;
        const FURTIVE_DETECT_RANGE: f32 = 5.0;
        let scene = Scene::riviere_demo();
        let boss2 = scene
            .objects
            .iter()
            .find(|o| o.group == "Boss2" && o.tag == "boss2")
            .expect("le second boss doit être présent dans la démo Rivière");
        let boss2_pos = boss2.transform.position;
        let boss2_range = CHASER_DETECT_RANGE; // Colosse.
        for m in scene.objects.iter().filter(|o| o.tag == "monstre") {
            let range = if m.ai_chaser.as_ref().unwrap().archetype == Archetype::Furtive {
                FURTIVE_DETECT_RANGE
            } else {
                CHASER_DETECT_RANGE
            };
            let required = boss2_range + range;
            let dist = boss2_pos.distance(m.transform.position);
            assert!(
                dist > required,
                "{} : {dist:.1} m du second boss, en-dessous du minimum requis {required:.1} m (double-pull possible)",
                m.name
            );
        }
        // Le premier boss (extrémité amont) ne doit évidemment pas non plus
        // se retrouver dans la portée d'éveil combinée du second (extrémité
        // aval) — même garde-fou, cas trivial vu la distance, mais vérifié
        // explicitement plutôt que supposé.
        let boss1 = scene
            .objects
            .iter()
            .find(|o| o.group == "Boss")
            .expect("le premier boss doit être présent dans la démo Rivière");
        let required = boss2_range + CHASER_DETECT_RANGE;
        assert!(boss2_pos.distance(boss1.transform.position) > required);
    }

    /// Pendant de `riviere_demo_boss_is_placed_on_dry_ground_away_from_the_
    /// channel`, pour le second boss et son butin garanti.
    #[test]
    fn riviere_demo_second_boss_is_placed_on_dry_ground_away_from_the_channel() {
        let scene = Scene::riviere_demo();
        let boss2 = scene
            .objects
            .iter()
            .find(|o| o.group == "Boss2" && o.tag == "boss2")
            .expect("le second boss doit être présent dans la démo Rivière");
        let p = boss2.transform.position;
        assert!(
            channel_dist(p.x, p.z) >= 1.3 && p.y >= water_level(p.z) + 0.25,
            "{} planté dans l'eau ou trop près du lit : {p:?}",
            boss2.name
        );
        let loot = scene
            .objects
            .iter()
            .find(|o| o.name == "Sceptre du Roi Champignon")
            .expect("le butin du second boss doit être présent dans la démo Rivière");
        let lp = loot.transform.position;
        assert!(
            channel_dist(lp.x, lp.z) >= 1.3 && lp.y >= water_level(lp.z) + 0.25,
            "{} planté dans l'eau ou trop près du lit : {lp:?}",
            loot.name
        );
        // Les trois Champiblobs (masqués mais déjà positionnés) doivent eux
        // aussi être posés sur la terre ferme.
        for m in scene.objects.iter().filter(|o| o.tag == "boss2_minion") {
            let mp = m.transform.position;
            assert!(
                channel_dist(mp.x, mp.z) >= 1.3 && mp.y >= water_level(mp.z) + 0.25,
                "{} planté dans l'eau ou trop près du lit : {mp:?}",
                m.name
            );
        }
    }

    /// Le second boss est, comme le premier, un évènement de fin de
    /// parcours : ni compté parmi les 9 emplacements du roster ordinaire, ni
    /// confondu avec le premier boss (groupe/tag distincts) — pendant de
    /// `riviere_demo_boss_is_not_counted_among_the_ordinary_monster_roster`.
    #[test]
    fn riviere_demo_second_boss_is_not_counted_among_the_ordinary_monster_roster_or_the_first_boss()
    {
        let scene = Scene::riviere_demo();
        assert!(scene.objects.iter().any(|o| o.group == "Boss2" && o.tag == "boss2"));
        assert_eq!(
            scene.objects.iter().filter(|o| o.tag == "monstre").count(),
            9,
            "le roster ordinaire doit rester à 9, le second boss (ni ses Champiblobs) ne doit pas y être tagué"
        );
        assert_eq!(scene.objects.iter().filter(|o| o.group == "Monstre").count(), 9);
        assert_eq!(
            scene.objects.iter().filter(|o| o.group == "Boss").count(),
            1,
            "un seul objet du groupe \"Boss\" : le second boss ne doit pas s'y confondre"
        );
        assert_eq!(
            scene.objects.iter().filter(|o| o.tag == "boss2_minion").count(),
            3,
            "3 Champiblobs attendus, tous dans le groupe \"Boss2\", jamais taggés \"monstre\""
        );
    }

    /// Même verrou de cohérence des chaînes que `riviere_demo_boss_names_
    /// match_app_boss_module`, pour le second boss (`app::boss2`).
    #[test]
    fn riviere_demo_second_boss_names_match_app_boss2_module() {
        let scene = Scene::riviere_demo();
        assert!(
            scene
                .objects
                .iter()
                .any(|o| o.name == crate::app::boss2::BOSS2_NAME),
            "aucun objet nommé `app::boss2::BOSS2_NAME` dans la démo Rivière"
        );
        assert!(
            scene
                .objects
                .iter()
                .any(|o| o.name == crate::app::boss2::BOSS2_LOOT_NAME),
            "aucun objet nommé `app::boss2::BOSS2_LOOT_NAME` dans la démo Rivière"
        );
        assert!(
            scene.objects.iter().any(|o| o.tag == crate::app::boss2::BOSS2_MINION_TAG),
            "aucun Champiblob taggé `app::boss2::BOSS2_MINION_TAG` dans la démo Rivière"
        );
    }

    #[test]
    fn terrain_height_is_bilinear_on_the_imported_grid() {
        let scene = Scene::riviere_demo();
        let terrain = &scene.imported[0];
        // Le plateau amont est plus haut que la vallée aval, le lit du bassin
        // plus bas que l'eau.
        assert!(terrain_height(terrain, 20.0, -60.0) > terrain_height(terrain, 20.0, 40.0) + 5.0);
        assert!(terrain_height(terrain, river_center(POOL_Z), POOL_Z) < water_level(POOL_Z) - 1.0);
    }

    /// Preuve de la demande gameplay du 14 septembre 2026 (« des monstres à
    /// tuer », « des objets à loot ») : la démo place bien des monstres
    /// attaquables/mordants et du butin ramassable, pas seulement de la faune
    /// et du décor.
    #[test]
    fn riviere_demo_has_monsters_and_loot() {
        let scene = Scene::riviere_demo();
        let monsters: Vec<&SceneObject> = scene
            .objects
            .iter()
            .filter(|o| o.tag == "monstre")
            .collect();
        assert!(!monsters.is_empty(), "aucun monstre placé");
        for m in &monsters {
            assert!(
                m.combat.as_ref().is_some_and(|c| c.attackable && c.hp > 0),
                "{} n'est pas attaquable",
                m.name
            );
            assert!(m.ai_chaser.is_some(), "{} n'a pas de poursuite", m.name);
            assert!(m.bite.is_some(), "{} ne mord pas", m.name);
            assert!(m.trigger, "{} ne détecte pas le contact", m.name);
            assert!(
                !m.script.is_empty(),
                "{} n'a pas de script de morsure solo",
                m.name
            );
        }
        let weapon_loot = scene
            .objects
            .iter()
            .filter(|o| o.weapon_pickup.is_some())
            .count();
        assert!(weapon_loot > 0, "aucune arme à ramasser");
        let heal_loot = scene
            .objects
            .iter()
            .filter(|o| {
                o.item_pickup
                    .is_some_and(|p| matches!(p.kind, ItemKind::Potion | ItemKind::Baie))
            })
            .count();
        assert!(heal_loot > 0, "aucun objet de soin à ramasser");
    }

    /// Garde-fou contre une régression silencieuse vers un seul mesh :
    /// bestiaire varié (14 septembre 2026 au soir) plutôt que six clones du
    /// même renard, cf. la doc du bloc « Monstres » de `riviere_demo`.
    #[test]
    fn riviere_demo_monsters_are_varied_species() {
        let scene = Scene::riviere_demo();
        let monsters: Vec<&SceneObject> = scene.objects.iter().filter(|o| o.tag == "monstre").collect();
        assert_eq!(monsters.len(), 9, "9 monstres attendus (8 espèces, le renard ×2)");
        let species: std::collections::HashSet<String> = monsters
            .iter()
            .filter_map(|m| match m.mesh {
                MeshKind::Imported(idx) => scene.imported.get(idx as usize).map(|im| im.path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            species.len(),
            8,
            "8 meshes distincts attendus (seul le renard est dupliqué) : {species:?}"
        );
    }

    /// Règles d'authoring du roster (sur le modèle de
    /// `mmorpg_demo_waves_follow_the_gdd_authoring_rules`) : PV et vitesse
    /// dans des bornes raisonnables, un Colosse encaisse strictement plus
    /// qu'une Traqueuse moyenne, et chaque monstre garde un préfixe (donc un
    /// script Lua) unique — deux instances de même espèce (les deux renards,
    /// les deux blobs) ne doivent jamais partager leur namespace `save`.
    #[test]
    fn riviere_demo_monster_roster_follows_authoring_rules() {
        let scene = Scene::riviere_demo();
        let monsters: Vec<&SceneObject> = scene.objects.iter().filter(|o| o.tag == "monstre").collect();
        for m in &monsters {
            let hp = m.combat.as_ref().unwrap().hp;
            assert!((1..=6).contains(&hp), "{} : PV hors bornes [1,6] : {hp}", m.name);
            let speed = m.ai_chaser.as_ref().unwrap().speed;
            assert!(
                speed > 0.0 && speed <= 3.0,
                "{} : vitesse de poursuite hors bornes : {speed}",
                m.name
            );
        }
        let colosse_hp: Vec<u32> = monsters
            .iter()
            .filter(|m| m.ai_chaser.as_ref().unwrap().archetype == Archetype::Colosse)
            .map(|m| m.combat.as_ref().unwrap().hp)
            .collect();
        assert!(!colosse_hp.is_empty(), "au moins un Colosse attendu (le golem)");
        let traqueuse_hps: Vec<u32> = monsters
            .iter()
            .filter(|m| m.ai_chaser.as_ref().unwrap().archetype == Archetype::Traqueuse)
            .map(|m| m.combat.as_ref().unwrap().hp)
            .collect();
        let traqueuse_avg = traqueuse_hps.iter().sum::<u32>() as f32 / traqueuse_hps.len() as f32;
        for hp in colosse_hp {
            assert!(
                hp as f32 > traqueuse_avg,
                "un Colosse ({hp} PV) doit encaisser plus qu'une Traqueuse moyenne ({traqueuse_avg})"
            );
        }
        let scripts: Vec<&str> = monsters.iter().map(|m| m.script.as_str()).collect();
        let unique: std::collections::HashSet<&str> = scripts.iter().copied().collect();
        assert_eq!(
            unique.len(),
            scripts.len(),
            "deux monstres partagent le même script (préfixe `save` en collision)"
        );
    }

    /// Non-régression par espèce (bestiaire varié, 14 septembre 2026 au
    /// soir) : chaque espèce apparaît avec le bon mesh, ses PV d'authoring,
    /// peut être blessée sans mourir avant son dernier PV puis vaincue
    /// (masquée) au dernier coup, et reste éligible au respawn
    /// (`respawn_delay > 0`). La restauration effective des PV **au**
    /// respawn (la file `AppState::respawn_queue`/`process_respawns`) est
    /// déjà couverte génériquement par
    /// `a_respawning_enemy_comes_back_with_its_original_hp`
    /// (`app::simulation_tests`), pas dupliquée ici par espèce.
    #[test]
    fn riviere_demo_each_species_spawns_takes_damage_and_dies() {
        let mut scene = Scene::riviere_demo();
        let expectations: [(&str, &str, u32); 8] = [
            ("Renard enragé 1", "fauna_fox.glb", 3),
            ("Maraudeur (orc)", "monster_orc.glb", 4),
            ("Grenouille des berges", "monster_frog.glb", 2),
            ("Fantôme de la brume", "monster_ghost.glb", 2),
            ("Golem des berges", "monster_goleling.glb", 6),
            ("Blob des sous-bois (vert)", "monster_green_blob.glb", 2),
            ("Blob des sous-bois (rose)", "monster_pink_blob.glb", 2),
            ("Champignon mordeur", "monster_mushnub.glb", 2),
        ];
        for (name, file_substr, hp) in expectations {
            let i = scene
                .objects
                .iter()
                .position(|o| o.name == name)
                .unwrap_or_else(|| panic!("espèce absente du roster : {name}"));
            assert_eq!(scene.objects[i].tag, "monstre", "{name} doit être tagué monstre");
            let mesh_path = match scene.objects[i].mesh {
                MeshKind::Imported(idx) => scene.imported[idx as usize].path.clone(),
                _ => panic!("{name} : mesh non importé"),
            };
            assert!(
                mesh_path.contains(file_substr),
                "{name} : mesh inattendu ({mesh_path}, attendu {file_substr})"
            );
            assert!(scene.objects[i].visible, "{name} doit apparaître visible");
            assert_eq!(
                scene.objects[i].combat.as_ref().unwrap().hp,
                hp,
                "{name} : PV d'authoring inattendus"
            );
            assert!(
                scene.objects[i].respawn_delay > 0.0,
                "{name} doit être éligible au respawn"
            );
            if hp > 1 {
                assert!(
                    !scene.damage_attackable_by(i, hp - 1),
                    "{name} : ne doit pas mourir avant son dernier PV"
                );
                assert!(
                    scene.objects[i].visible,
                    "{name} doit rester visible tant qu'il a des PV"
                );
            }
            assert!(
                scene.damage_attackable_by(i, 1),
                "{name} : le dernier coup doit l'achever"
            );
            assert!(!scene.objects[i].visible, "{name} vaincu doit être masqué");
        }
    }
}
