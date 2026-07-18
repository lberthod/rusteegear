//! Données de mesh côté CPU + layout de vertex pour wgpu.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[derive(Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Sommet skinné : `Vertex` + jusqu'à 4 os influents et leurs poids
/// (convention glTF `JOINTS_0`/`WEIGHTS_0`, cf. `scene::import::VertexSkin`). Un type
/// **séparé** de `Vertex` plutôt qu'un ajout de champs à `Vertex` : ça laisse tous les
/// meshes statiques (primitives, imports glTF sans skin) et leur pipeline inchangés —
/// seul un mesh réellement skinné paie le coût des attributs supplémentaires.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
    /// Indices dans la palette de matrices de joints (cf. `Renderer`).
    /// `u32` plutôt que `u16` (format du glTF source) : format de vertex GPU plus simple
    /// et plus largement pris en charge, au prix de 8 octets non significatifs par sommet
    /// — négligeable face au reste du vertex.
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

impl SkinnedVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SkinnedVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 44,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32x4,
                },
                wgpu::VertexAttribute {
                    offset: 60,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[derive(Default)]
pub struct SkinnedMeshData {
    pub vertices: Vec<SkinnedVertex>,
    pub indices: Vec<u32>,
}

/// Mesh chargé côté GPU (buffers prêts à dessiner).
pub struct GpuMesh {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub num_indices: u32,
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, data: &MeshData) -> Self {
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        GpuMesh {
            vertex_buf,
            index_buf,
            num_indices: data.indices.len() as u32,
        }
    }

    /// Identique à `new`, pour un mesh skinné. `GpuMesh` lui-même ne connaît
    /// que des buffers bruts — indépendant du format de vertex, seul l'upload diffère.
    pub fn new_skinned(device: &wgpu::Device, data: &SkinnedMeshData) -> Self {
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skinned_vertices"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skinned_indices"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        GpuMesh {
            vertex_buf,
            index_buf,
            num_indices: data.indices.len() as u32,
        }
    }
}

/// Cube unitaire centré sur l'origine, normales par face.
pub fn cube(color: [f32; 3]) -> MeshData {
    // (normale, 4 coins dans le sens trigonométrique)
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // +X
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
        // -X
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        // +Y
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        // -Y
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ],
        ),
        // +Z
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        // -Z
        (
            [0.0, 0.0, -1.0],
            [
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
    ];

    // UV des 4 coins de chaque face (repère trigonométrique).
    let face_uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let base = vertices.len() as u32;
        for (k, pos) in corners.into_iter().enumerate() {
            vertices.push(Vertex {
                position: pos,
                normal,
                color,
                uv: face_uv[k],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    MeshData { vertices, indices }
}

/// Sphère UV de rayon 0.5 centrée sur l'origine.
pub fn sphere(color: [f32; 3]) -> MeshData {
    use std::f32::consts::PI;
    let sectors = 24u32;
    let stacks = 16u32;
    let radius = 0.5;

    let mut vertices = Vec::new();
    for i in 0..=stacks {
        let phi = PI * i as f32 / stacks as f32; // 0..π (du pôle nord au sud)
        let (sp, cp) = phi.sin_cos();
        for j in 0..=sectors {
            let theta = 2.0 * PI * j as f32 / sectors as f32;
            let (st, ct) = theta.sin_cos();
            let n = [sp * ct, cp, sp * st];
            vertices.push(Vertex {
                position: [radius * n[0], radius * n[1], radius * n[2]],
                normal: n,
                color,
                uv: [j as f32 / sectors as f32, i as f32 / stacks as f32],
            });
        }
    }

    let mut indices = Vec::new();
    let row = sectors + 1;
    for i in 0..stacks {
        for j in 0..sectors {
            let a = i * row + j;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    MeshData { vertices, indices }
}

/// Plan unitaire (1×1) dans le plan XZ, normale +Y. À mettre à l'échelle via le Transform.
pub fn plane(color: [f32; 3]) -> MeshData {
    let n = [0.0, 1.0, 0.0];
    let vertices = vec![
        Vertex {
            position: [-0.5, 0.0, -0.5],
            normal: n,
            color,
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.0, -0.5],
            normal: n,
            color,
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.0, 0.5],
            normal: n,
            color,
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.0, 0.5],
            normal: n,
            color,
            uv: [0.0, 1.0],
        },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    MeshData { vertices, indices }
}

/// Impostor « croix » (deux plans verticaux 1×1 perpendiculaires, base à `y=0`, sommet à
/// `y=1`) pour le feuillage dense vu de loin (Phase D, LOD géométrique) — technique
/// classique d'impostor d'herbe : contrairement à `plane()` (horizontal, quasi invisible
/// vu à hauteur d'œil), une croix verticale présente toujours une face visible sous un
/// angle de vue horizontal, quel que soit le yaw de l'instance. Pas de calcul de rotation
/// face-caméra par frame nécessaire : les deux plans à 90° suffisent avec le culling de
/// faces déjà désactivé sur le pipeline principal (`cull_mode: None`,
/// `src/gfx/pipelines.rs`), donc visibles des deux côtés.
pub fn billboard_cross(color: [f32; 3]) -> MeshData {
    let vertices = vec![
        // Plan A : dans le plan XY (normale +Z).
        Vertex {
            position: [-0.5, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            color,
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            color,
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            color,
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            color,
            uv: [0.0, 0.0],
        },
        // Plan B : dans le plan ZY (normale +X), perpendiculaire au plan A.
        Vertex {
            position: [0.0, 0.0, -0.5],
            normal: [1.0, 0.0, 0.0],
            color,
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.0, 0.0, 0.5],
            normal: [1.0, 0.0, 0.0],
            color,
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.0, 1.0, 0.5],
            normal: [1.0, 0.0, 0.0],
            color,
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.0, 1.0, -0.5],
            normal: [1.0, 0.0, 0.0],
            color,
            uv: [0.0, 0.0],
        },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    MeshData { vertices, indices }
}

/// Bornes locales (`x ∈ [-0.5,0.5]`) de la bande de collines à l'ouest — le seul
/// endroit où `mmorpg_terrain_local_height` renvoie une valeur non nulle, cf. sa
/// doc — utilisées par `runtime::physics::Physics::build` pour restreindre le
/// collider heightfield du relief à cette seule bande plutôt qu'à toute la carte.
/// Sans cette restriction, le `KinematicCharacterController` des créatures/du
/// joueur interagit avec un heightfield composite sur les ~99 % de carte par
/// ailleurs strictement plats — régression constatée sur
/// `app::simulation::tests::mmorpg_creature_never_gets_stuck_walking_into_a_wall`
/// (créature figée 43 % du temps, alors qu'elle ne s'approche jamais de cette
/// bande) quand tout le sol utilisait un seul heightfield global : un corps qui
/// ne touche jamais la bande ne doit voir AUCUN changement de comportement
/// physique par rapport à l'ancien sol `MeshKind::Plane`/cuboid plat. Légèrement
/// plus large que la zone réellement non nulle (jusqu'à x=-34,5 en mètres monde,
/// soit -0.4792 en local) pour ne jamais couper la retombée à 0 en plein milieu —
/// borne haute -0.46 (x=-33,1 m) très en-deçà du premier spawn de faune/créature
/// rencontré en balayant vers l'est (x=-31), avec la marge de 3 m déjà vérifiée.
pub const MMORPG_HILL_STRIP_X_LOCAL: (f32, f32) = (-0.5, -0.46);
/// Résolution (quads) de la bande restreinte : plus fine en Z (bande longue et
/// étroite) qu'en X (2,9 m de large seulement, cf. `MMORPG_HILL_STRIP_X_LOCAL`).
pub const MMORPG_HILL_STRIP_RES: (u32, u32) = (6, 96);

/// Résolution de grille (quads par côté) du maillage `MeshKind::Terrain` — partagée
/// par le maillage visuel (`terrain()`) et le collider heightfield physique
/// (`runtime::physics::Physics::build`, cas `MeshKind::Terrain`), qui DOIT
/// échantillonner `mmorpg_terrain_local_height` sur la même grille pour que le sol
/// visuel et le sol solide coïncident exactement (Sprint 24 de `sprintreflecion.md`,
/// Phase K). 96 : sur la carte MMORPG 72×72 m (Sprint 24), une cellule fait 0,75 m —
/// assez fin pour des collines lisses (vs 3 m avec l'ancienne grille 24×24,
/// visiblement facetté) sans exploser le budget de sommets (97² = 9 409 sommets,
/// 96² × 2 = 18 432 triangles — un seul objet, comparable à quelques dizaines
/// d'assets de décor importés déjà présents dans `mmorpg_demo`).
pub const TERRAIN_HEIGHTGRID_RES: u32 = 96;

/// Bascule douce 0→1 (Hermite cubique, dérivée nulle aux deux bords) :
/// `t = clamp((x-lo)/(hi-lo), 0, 1)`, retourne `3t²-2t³`. Brique de base pour
/// composer des zones de relief sans cassure (Sprint 24/25, Phase K).
fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Porte rectangulaire lissée sur un axe : ≈1 pour `v ∈ [lo,hi]`, retombe à 0 sur
/// une marge `margin` de part et d'autre (continue, dérivable par morceaux — donc
/// compatible avec les normales par différence finie de `heightgrid_mesh`).
fn band_gate(lo: f32, hi: f32, margin: f32, v: f32) -> f32 {
    smoothstep(lo - margin, lo, v) * (1.0 - smoothstep(hi, hi + margin, v))
}

/// Échantillonne une fonction de hauteur locale (`x,z ∈ [-0.5,0.5]` → hauteur
/// locale, à multiplier par `Transform.scale.y` — ou directement en mètres si cette
/// échelle vaut 1.0, cf. `mmorpg_terrain_local_height`) sur une grille `res×res` et
/// produit un maillage avec normales par différence finie centrée. Générique par
/// rapport à `terrain()` (Sprint 24, Phase K) : fonctionne pour n'importe quelle
/// fonction de hauteur, y compris une composition de `smoothstep` non triviale à
/// dériver analytiquement (contrairement à l'ancien relief sinusoïdal pur).
/// Utilisée à la fois ici (maillage visuel) ET par `runtime::physics::Physics::build`
/// (collider heightfield, cas `MeshKind::Terrain`) — impératif que les deux
/// utilisent la MÊME fonction de hauteur, sinon le sol visuel et le sol solide
/// divergent (joueur qui flotte ou s'enterre selon l'endroit de la carte).
pub fn heightgrid_mesh(res: u32, color: [f32; 3], height_fn: impl Fn(f32, f32) -> f32) -> MeshData {
    // Demi-pas de différence finie : un quart de cellule de grille, assez petit
    // pour capturer la variation locale sans être sensible au bruit numérique.
    let eps = 0.25 / res as f32;
    let mut vertices = Vec::with_capacity(((res + 1) * (res + 1)) as usize);
    for iz in 0..=res {
        for ix in 0..=res {
            let x = ix as f32 / res as f32 - 0.5;
            let z = iz as f32 / res as f32 - 0.5;
            let y = height_fn(x, z);
            let dhdx = (height_fn(x + eps, z) - height_fn(x - eps, z)) / (2.0 * eps);
            let dhdz = (height_fn(x, z + eps) - height_fn(x, z - eps)) / (2.0 * eps);
            let n = [-dhdx, 1.0, -dhdz];
            let inv = 1.0 / (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            vertices.push(Vertex {
                position: [x, y, z],
                normal: [n[0] * inv, n[1] * inv, n[2] * inv],
                color,
                uv: [ix as f32 / res as f32, iz as f32 / res as f32],
            });
        }
    }
    let mut indices = Vec::new();
    let row = res + 1;
    for iz in 0..res {
        for ix in 0..res {
            let a = iz * row + ix;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    MeshData { vertices, indices }
}

/// Hauteur locale (mètres, en supposant `Transform.scale.y = 1.0` — échelle Y
/// découplée de X/Z, cf. `Scene::mmorpg_demo`) dédiée au sol de `Scene::mmorpg_demo`
/// (carte 72×72 m, `Scene::MMORPG_HALF = 36.0`, dupliqué ici en dur : `gfx` ne dépend
/// pas de `scene`). `x,z ∈ [-0.5,0.5]` en coordonnées locales de la grille unitaire,
/// reconverties en mètres monde via `× 72`.
///
/// Relief NUL (exactement 0, pas juste « faible ») partout où le hameau, la forêt,
/// l'eau (lac + rivières), les rizières, la route principale, les chemins ou la
/// faune ambiante (`MMORPG_AMBIENT_FAUNA_SPAWNS` dans `demos.rs`, dont plusieurs
/// spawns sont délibérément posés « à l'ouest des plans d'eau », x∈[-31,-22]) sont
/// déjà placés à la main dans `mmorpg_demo` — des centaines d'objets y supposent un
/// sol à y≈0 (Sprint 24 de `sprintreflecion.md`, Phase K) ; repositionner ce
/// contenu était explicitement exclu du sprint. Collines visibles UNIQUEMENT sur
/// une bande étroite entre le mur de périmètre ouest (x=-36) et x=-34,5 — vérifiée
/// numériquement libre de tout point (décor placé à la main, spawn de faune/
/// créature) à ≥3 m près et de tout rectangle d'exclusion de `demos.rs`
/// (`EXCL_EAU_ROUTES`/`EXCL_ZONES_AMENAGEES`/`foret`) avant de choisir cette zone —
/// la marge ouest « évidente » x∈[-34,-28] envisagée initialement chevauchait en
/// fait les spawns de faune ambiante (-31,-30)/(-31,-20)/(-30,0), cf. l'échec
/// initial de `scene::demos::tests::
/// mmorpg_terrain_has_real_relief_but_stays_flat_under_placed_content`. Coupée à
/// hauteur de la route principale (z∈[12,3; 15,7], exclue avec marge) pour ne pas
/// la faire onduler, et retombe à 0 avant les murs Nord/Sud pour ne jamais créer de
/// rampe improvisée par-dessus un mur (1,8 m de haut) : l'amplitude maximale
/// (~1,3 m, `AMP`) n'est atteinte qu'au centre de la bande, loin des murs.
pub fn mmorpg_terrain_local_height(x: f32, z: f32) -> f32 {
    const WORLD_SIZE: f32 = 72.0; // = 2 × Scene::MMORPG_HALF
    const AMP: f32 = 2.0; // amplitude max des collines (m)

    let wx = x * WORLD_SIZE;
    let wz = z * WORLD_SIZE;

    // Bande de collines contre le mur ouest : pleine amplitude sur x∈[-35.5,-35.0],
    // retombée à 0 sur 0,5 m de chaque côté — marge choisie pour retomber à 0
    // EXACTEMENT à x=-36 (mur) et bien avant x=-34 (bord vérifié libre de tout
    // spawn de faune/créature à ≥3 m près, cf. la doc ci-dessus), vérifié
    // numériquement (`gfx::mesh::tests::mmorpg_hill_zone_is_zero_at_wall_and_water`).
    let x_gate = band_gate(-35.5, -35.0, 0.5, wx);
    // Coupure au droit de la route principale (z∈[12,3; 15,7]) : gardée plate.
    let road_cut = 1.0 - band_gate(9.0, 19.0, 3.0, wz);
    // Retombée avant les murs Nord/Sud (0 dès z=±36).
    let z_wall_taper = band_gate(-33.0, 33.0, 3.0, wz);
    let gate = x_gate * road_cut * z_wall_taper;
    if gate <= 0.0 {
        return 0.0;
    }

    // Relief : somme de sinusoïdes à fréquences/phases distinctes — continu et
    // dérivable, suffisant pour un aspect de collines naturel sans vrai bruit de
    // Perlin (Sprint 24).
    let n = 0.55 * (wx * 0.18 + 1.3).sin() * (wz * 0.14).cos()
        + 0.30 * (wx * 0.07 - wz * 0.09 + 2.1).sin()
        + 0.15 * (wx * 0.35 + wz * 0.30).sin();
    AMP * gate * n
}

/// Terrain : grille 1×1 (plan XZ) subdivisée avec un relief doux procédural.
/// Hauteur et normales par différence finie (`heightgrid_mesh`) ; à mettre à
/// l'échelle via le Transform. Depuis le Sprint 24 (Phase K), utilise directement
/// `mmorpg_terrain_local_height` : `MeshKind::Terrain` n'a qu'UN maillage partagé
/// (mis en cache par variante d'enum, cf. `gfx::pipelines`), donc pas de paramètre
/// par instance possible — `Scene::mmorpg_demo` est l'unique consommateur réel de
/// cette primitive (cf. `sprintreflecion.md`), la primitive éditeur (menus/
/// hiérarchie) affiche donc désormais le même relief plutôt qu'un bruit générique
/// sans rapport avec le contenu du jeu.
pub fn terrain(color: [f32; 3]) -> MeshData {
    heightgrid_mesh(TERRAIN_HEIGHTGRID_RES, color, mmorpg_terrain_local_height)
}

/// Cylindre unitaire : rayon 0.5, hauteur 1 (y de -0.5 à 0.5), axe +Y.
pub fn cylinder(color: [f32; 3]) -> MeshData {
    use std::f32::consts::PI;
    let sectors = 24u32;
    let radius = 0.5;
    let half = 0.5;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Paroi latérale : deux anneaux (bas/haut) avec normales radiales.
    for j in 0..=sectors {
        let theta = 2.0 * PI * j as f32 / sectors as f32;
        let (st, ct) = theta.sin_cos();
        let n = [ct, 0.0, st];
        let u = j as f32 / sectors as f32;
        vertices.push(Vertex {
            position: [radius * ct, -half, radius * st],
            normal: n,
            color,
            uv: [u, 1.0],
        });
        vertices.push(Vertex {
            position: [radius * ct, half, radius * st],
            normal: n,
            color,
            uv: [u, 0.0],
        });
    }
    for j in 0..sectors {
        let a = j * 2;
        let b = a + 1;
        let c = a + 2;
        let d = a + 3;
        indices.extend_from_slice(&[a, c, b, b, c, d]);
    }

    // Capuchons (centre + couronne) en haut (+Y) et en bas (-Y).
    for &(y, ny) in &[(half, 1.0f32), (-half, -1.0f32)] {
        let center = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, y, 0.0],
            normal: [0.0, ny, 0.0],
            color,
            uv: [0.5, 0.5],
        });
        let ring_start = vertices.len() as u32;
        for j in 0..=sectors {
            let theta = 2.0 * PI * j as f32 / sectors as f32;
            let (st, ct) = theta.sin_cos();
            vertices.push(Vertex {
                position: [radius * ct, y, radius * st],
                normal: [0.0, ny, 0.0],
                color,
                uv: [0.5 + 0.5 * ct, 0.5 + 0.5 * st],
            });
        }
        for j in 0..sectors {
            let a = ring_start + j;
            let b = ring_start + j + 1;
            // Orientation pour que la normale pointe vers l'extérieur.
            if ny > 0.0 {
                indices.extend_from_slice(&[center, b, a]);
            } else {
                indices.extend_from_slice(&[center, a, b]);
            }
        }
    }
    MeshData { vertices, indices }
}

/// Capsule unitaire : rayon 0.25, hauteur totale 1 (cylindre + deux demi-sphères), axe +Y.
pub fn capsule(color: [f32; 3]) -> MeshData {
    use std::f32::consts::PI;
    let sectors = 24u32;
    let cap_stacks = 8u32; // anneaux par demi-sphère
    let radius = 0.25;
    let cyl_half = 0.5 - radius; // partie cylindrique : 0.25 de chaque côté

    // Repère « visage » : patch sombre sur l'hémisphère haut (la tête), côté -Z local
    // — c'est la direction que `Physics::face_direction` fait correspondre au
    // déplacement (yaw=0 ⇒ le personnage avance vers -Z, cf. les tests
    // `camera_relative_move_*`). Sans ça, la capsule est parfaitement symétrique en
    // rotation : impossible de voir à l'écran vers où le personnage regarde une fois
    // que la rotation suit le déplacement.
    const FACE_COLOR: [f32; 3] = [0.08, 0.08, 0.08];
    const FACE_ROW_MIN: u32 = 1;
    const FACE_ROW_MAX: u32 = 5;
    const FACE_CENTER: f32 = 3.0 * PI / 2.0; // -Z
    const FACE_HALF_WIDTH: f32 = 0.55; // ± ~31°
    let angular_dist = |theta: f32| {
        let d = (theta - FACE_CENTER).rem_euclid(2.0 * PI);
        d.min(2.0 * PI - d)
    };

    let mut vertices = Vec::new();
    // Une grille (stack, sector) d'un pôle à l'autre, en décalant le centre des
    // hémisphères de ±cyl_half pour insérer le tube central.
    let rows = cap_stacks * 2 + 1;
    for i in 0..=rows {
        // phi : 0 (pôle haut) → π (pôle bas)
        let t = i as f32 / rows as f32;
        let phi = PI * t;
        let (sp, cp) = phi.sin_cos();
        // décalage vertical : hémisphère haut centré en +cyl_half, bas en -cyl_half
        let y_off = if cp >= 0.0 { cyl_half } else { -cyl_half };
        for j in 0..=sectors {
            let theta = 2.0 * PI * j as f32 / sectors as f32;
            let (st, ct) = theta.sin_cos();
            let n = [sp * ct, cp, sp * st];
            let is_face =
                (FACE_ROW_MIN..=FACE_ROW_MAX).contains(&i) && angular_dist(theta) < FACE_HALF_WIDTH;
            vertices.push(Vertex {
                position: [radius * n[0], radius * cp + y_off, radius * n[2]],
                normal: n,
                color: if is_face { FACE_COLOR } else { color },
                uv: [j as f32 / sectors as f32, t],
            });
        }
    }

    let mut indices = Vec::new();
    let row = sectors + 1;
    for i in 0..rows {
        for j in 0..sectors {
            let a = i * row + j;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    MeshData { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_marks_the_minus_z_face_with_a_distinct_color() {
        let base = [0.45, 0.7, 0.5];
        let data = capsule(base);
        let is_marked = |v: &Vertex| v.color != base;
        // Un sommet proche de -Z (avant, cf. `Physics::face_direction`), sur la tête
        // (z négatif dominant, y au-dessus du centre), doit porter la couleur du
        // visage — sinon on ne peut pas voir vers où le personnage regarde.
        assert!(
            data.vertices
                .iter()
                .any(|v| v.position[2] < -0.15 && v.position[1] > 0.0 && is_marked(v)),
            "aucun sommet marqué trouvé côté -Z (avant)"
        );
        // À l'opposé (+Z, dos), aucun sommet ne doit porter cette couleur.
        assert!(
            !data
                .vertices
                .iter()
                .any(|v| v.position[2] > 0.15 && v.position[1] > 0.0 && is_marked(v)),
            "un sommet marqué a été trouvé côté +Z (dos) — le repère visage doit rester devant"
        );
    }

    #[test]
    fn billboard_cross_has_two_perpendicular_quads_standing_on_the_ground() {
        let data = billboard_cross([0.2, 0.6, 0.2]);
        assert_eq!(data.vertices.len(), 8);
        assert_eq!(data.indices.len(), 12);
        // Base au sol (y=0), sommet à y=1 — cohérent avec un objet posé par son
        // `Transform::position` comme les autres primitives (`cube`, `plane`, ...).
        assert!(data.vertices.iter().all(|v| v.position[1] >= 0.0));
        assert!(data.vertices.iter().any(|v| v.position[1] == 1.0));
        // Les deux plans sont bien perpendiculaires (normales +Z et +X).
        assert!(data.vertices.iter().any(|v| v.normal == [0.0, 0.0, 1.0]));
        assert!(data.vertices.iter().any(|v| v.normal == [1.0, 0.0, 0.0]));
    }

    /// Sprint 24 (Phase K, `sprintreflecion.md`) : la bande de collines retombe à
    /// 0 EXACTEMENT au mur ouest (x=-36) et à la rive du lac (x=-28,
    /// `EXCL_EAU_ROUTES` dans `demos.rs`) — c'est cette propriété qui garantit
    /// qu'un corps qui n'entre jamais dans la bande (tout le contenu placé à la
    /// main de `mmorpg_demo`) ne voit aucune variation de hauteur de sol.
    #[test]
    fn mmorpg_hill_zone_is_zero_at_wall_and_water() {
        for wz in [-30.0_f32, -10.0, 0.0, 10.0, 25.0] {
            let at_wall = mmorpg_terrain_local_height(-36.0 / 72.0, wz / 72.0);
            // -34.0 : bord est de la bande, bien en-deçà (marge ≥3 m) du premier
            // spawn de faune/créature à x=-31 (cf. la doc de la fonction).
            let at_edge = mmorpg_terrain_local_height(-34.0 / 72.0, wz / 72.0);
            assert!(
                at_wall.abs() < 1e-4,
                "z={wz} : hauteur au mur ouest doit être ~0 (obtenu {at_wall})"
            );
            assert!(
                at_edge.abs() < 1e-4,
                "z={wz} : hauteur au bord est de la bande (x=-34) doit être ~0 \
                 (obtenu {at_edge})"
            );
        }
        // Et un relief bien réel (pas juste nul partout) au centre de la bande.
        let peak = mmorpg_terrain_local_height(-35.25 / 72.0, 0.0);
        assert!(
            peak.abs() > 0.5,
            "le centre de la bande de collines doit avoir un relief net (obtenu {peak})"
        );
    }

    /// Le relief doit rester exactement nul au droit de la route principale
    /// (z∈[12,3; 15,7]) même en pleine largeur de la bande de collines (x=-32) :
    /// sinon la route (posée à plat dans `demos.rs`) onduler ait sous les pieds
    /// des voyageurs à l'endroit précis où elle traverse la marge ouest.
    #[test]
    fn mmorpg_hill_zone_stays_flat_across_the_main_road() {
        let h = mmorpg_terrain_local_height(-35.25 / 72.0, 14.0 / 72.0);
        assert!(
            h.abs() < 1e-4,
            "la route principale doit rester plate même dans la bande de collines \
             (obtenu {h})"
        );
    }
}
