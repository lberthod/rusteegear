//! Couche **rendu pur** (wgpu + egui). Ne contient aucun état métier : la scène,
//! la caméra et la sélection vivent dans `AppState` et sont passées à `render`.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use super::lod::foliage_lod_mesh;
use super::mesh::GpuMesh;
use super::passes::{
    aabb_visible, culling_radius_for, distance_visible, frustum_planes, is_skinned, mesh_key,
    render_input_hash,
};
#[cfg(test)]
use super::pipelines::mip_count_for;
use super::pipelines::{
    self, PipelineBundle, create_bloom_mip_views, create_depth_view, create_hdr_view,
    create_models_buffer, create_skinned_models_bind_group, load_rgba, make_texture,
};
use crate::app::{AppState, GIZMO_LEN, GizmoMode, RING_SEGMENTS, axis_basis, axis_dir};
use crate::editor::Editor;
use crate::scene::{MeshKind, Scene};
use crate::time_compat::Instant;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(super) struct GizmoVertex {
    pub(super) position: [f32; 3],
    pub(super) color: [f32; 3],
}

impl GizmoVertex {
    pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GizmoVertex>() as wgpu::BufferAddress,
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
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(super) struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    /// Position de la caméra (xyz), pour le terme spéculaire. w inutilisé.
    eye: [f32; 4],
    /// Inverse de `view_proj` : déplie un point NDC du plan lointain en
    /// position monde, pour reconstruire la direction de vue dans `sky.wgsl` sans
    /// dépendre d'un dégradé fixe en espace écran (qui resterait immobile si la
    /// caméra pivote). Inutilisé par les autres shaders (`main.wgsl`/`skinned.wgsl`/
    /// `gizmo.wgsl` ne déclarent qu'un préfixe de cet uniform, WGSL l'autorise).
    inv_view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(super) struct ModelUniform {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    params: [f32; 4], // x = surbrillance (sélection)
    color: [f32; 4],  // teinte (albédo) de l'objet
}

/// Une lumière ponctuelle côté GPU (std140 : deux vec4).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct PointLightU {
    pos_range: [f32; 4], // xyz = position, w = portée
    color_int: [f32; 4], // rgb = couleur, w = intensité
    spot: [f32; 4],      // xyz = direction du cône, w = cos(demi-angle) ou -1 (point)
}

/// Éclairage de la scène (groupe 0, binding 1).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(super) struct SceneUniform {
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4], // x = intensité ambiante
    light_vp: [[f32; 4]; 4],
    num_points: [f32; 4], // x = nombre de lumières ponctuelles actives
    points: [PointLightU; crate::scene::MAX_POINT_LIGHTS],
    /// Ciel + brouillard : ajoutés en fin de struct pour ne décaler aucun
    /// des offsets existants ci-dessus (moins de risque de désync avec les shaders qui
    /// ne déclarent qu'un préfixe de cet uniform).
    sky_horizon: [f32; 4], // rgb, w inutilisé
    sky_zenith: [f32; 4], // rgb, w inutilisé
    fog: [f32; 4],        // rgb = couleur, w = densité
}

/// Paramètre du bloom (groupe dédié du `tonemap_pipeline`) : juste
/// l'intensité, dans son propre petit uniform plutôt que dans `SceneUniform` — le
/// tone mapping est une passe séparée avec son propre bind group, pas de raison de
/// lui faire porter tout `Light`/`Camera` pour un seul flottant.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct BloomUniform {
    intensity: [f32; 4], // x = intensité, yzw inutilisés (alignement std140)
}

pub(super) const SHADOW_SIZE: u32 = 2048;

pub(super) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Cible de rendu HDR : la scène (ciel, grille, objets, gizmos, debug
/// drawing, skinning) est dessinée dans cette texture intermédiaire — pas directement
/// dans le format d'affichage final — pour que les valeurs > 1 (émissifs, spéculaire
/// fort) restent représentables au lieu d'être écrêtées avant même le tone mapping.
/// `Rgba16Float` : suffisant pour la plage dynamique visée ici (contrairement à
/// `Rgba32Float`, filtrable nativement sans extension GPU supplémentaire).
pub(super) const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Nombre de niveaux de la chaîne de mips du bloom : mip 0 = moitié de la
/// résolution HDR, chaque niveau suivant moitié du précédent. 4 est un compromis
/// raisonnable — assez pour un halo doux qui s'étend sur plusieurs pixels, sans
/// multiplier les passes plein écran par frame (2×(N-1) + 1 = 7 passes ici).
pub(super) const BLOOM_MIP_LEVELS: u32 = 4;

/// Skinning GPU : matrices par instance skinnée dans la palette de
/// joints — généreux pour un rig réel (Mixamo : ~50-65 os).
pub(super) const JOINT_CAPACITY: usize = 128;
/// Nombre d'objets skinnés distincts dessinables dans une même frame : un
/// créneau par instance dans `Renderer::joint_buf`, sélectionné au dessin par offset
/// dynamique. Augmenter est un changement d'une ligne si besoin (le buffer,
/// `pipelines.rs`, est déjà dimensionné à partir de cette constante).
///
/// Remonté de 8 à 32 (audit du 16 juillet 2026) : la démo MMORPG a 20 créatures
/// skinnées + le joueur (`assets/models/fairy_hero.glb`) visibles simultanément,
/// soit 21 instances — au-delà de l'ancienne capacité de 8, celles en trop étaient
/// dessinées avec la palette de joints d'un *autre* objet skinné (offset de repli
/// ambigu, cf. `write_joint_matrices`), pas simplement invisibles : le joueur (mesh
/// importé le plus loin dans l'ordre de tri par mesh/texture, donc le plus
/// susceptible de dépasser la capacité) apparaissait éclaté, transformé par le
/// squelette d'une créature au hasard.
///
/// Remonté de 32 à 96 (16 juillet 2026) : l'ajout des 40 assets animés du pack
/// « menagerie » (`gen_menagerie_pack.py`/`gen_menagerie_pack2.py` — petite
/// faune + mécanismes de décor, tous riggés avec un clip `Idle`) porte le total
/// skinné de la démo MMORPG à 20 créatures + 45 décors animés + le joueur, soit
/// 66 instances potentiellement visibles ensemble (vue quasi zénithale du test
/// `the_embedded_mmorpg_scene_gives_the_player_its_own_joint_offset`) — au-delà
/// de 66, le joueur ressortait du plan de dessin skinné (même symptôme qu'à 8).
/// 96 laisse de la marge pour du décor animé futur sans reproduire l'audit.
///
/// Remonté de 96 à 160 (menagerie de monstres, Ultimate Monsters Bundle) :
/// 45 nouveaux décors animés (`import_monster_pack.py`, riggés, clip `Idle`)
/// portent le total à 20 créatures + 90 décors animés + le joueur, soit 111
/// instances skinnées potentiellement visibles ensemble — 160 laisse à
/// nouveau de la marge (~49) sans reproduire l'audit ci-dessus.
///
/// Remonté de 160 à 256 (Phase A, `sprintoptimation3daudit10h.md`, 2026-07-18) :
/// la sonde sur `Scene::mmorpg_demo()` (`optimisation3D.Analys.md`) mesure 201
/// objets skinnés dans le contenu actuel, déjà au-delà de 160 — la mesure Phase 0
/// en jeu (vue large, `skinned_dropped == 0`) n'avait pas eu tous les 201 visibles
/// simultanément (861/887 objets chargés dans cette prise de vue précise), donc le
/// dépassement ne s'était pas encore manifesté à l'écran, mais restait latent (cf.
/// l'historique ci-dessus : ce cas s'est déjà produit 3 fois). 256 laisse à nouveau
/// une marge (~55) sans attendre de reproduire l'audit une 4e fois.
pub(super) const MAX_SKINNED_INSTANCES: usize = 256;
/// Taille en octets d'un créneau de la palette de joints — un objet skinné à la fois.
pub(super) const JOINT_SLOT_BYTES: wgpu::BufferAddress =
    (JOINT_CAPACITY * std::mem::size_of::<[[f32; 4]; 4]>()) as wgpu::BufferAddress;
// Doit rester multiple de 256 (`minStorageBufferOffsetAlignment` WebGPU/wgpu) : le
// binding à offset dynamique de `skinned_model_layout` (pipelines.rs) sélectionne un
// créneau par `offset = slot * JOINT_SLOT_BYTES`.
const _: () = assert!(JOINT_SLOT_BYTES.is_multiple_of(256));

/// Descripteur d'une instance dans le plan de rendu (ordre = index dans le buffer storage).
struct InstanceDraw {
    /// Index de l'objet dans `scene.objects` (texture relue au draw, sans clone).
    /// La scène n'est pas mutée entre la construction du plan et les passes de dessin.
    obj: usize,
    /// Visible par la caméra (frustum culling) — la passe d'ombre l'ignore.
    visible: bool,
    /// Mesh effectif à dessiner : `objs[obj].mesh`, sauf pour le feuillage dense LOD
    /// (Phase D, `foliage_lod_mesh`) au-delà du seuil de distance, où il devient
    /// `MeshKind::Billboard` — précalculé ici (pas relu depuis `scene.objects`, contrairement
    /// à `obj`) car il dépend de la distance caméra, recalculée à chaque frame où le plan est
    /// reconstruit, alors que le champ `mesh` de l'objet en scène reste inchangé.
    mesh: MeshKind,
}

/// Nombre de marqueurs temporels écrits par frame (Sprint 112) : un avant chaque
/// passe mesurée plus un final, soit `GPU_PROFILER_MARKS - 1` intervalles nommés
/// dans `GpuProfiler::PASS_NAMES` (ombre / scène / HDR+bloom / UI).
const GPU_PROFILER_MARKS: u32 = 5;

/// Timestamp queries GPU par passe (Sprint 112), actives seulement quand le
/// panneau Profiler est ouvert (`Editor::profiler_open`) — `write_timestamp` a un
/// coût réel (synchronisation GPU), pas question de le payer à chaque frame par
/// défaut, même si `Features::TIMESTAMP_QUERY_INSIDE_ENCODERS` est disponible.
/// `None` sur `Renderer` si l'adaptateur ne supporte pas cette feature (dégrade en
/// silence — le profiler FPS/mémoire reste utilisable sans elle).
struct GpuProfiler {
    query_set: wgpu::QuerySet,
    /// Résultats bruts (ticks GPU) résolus depuis `query_set` — recopiés ici pour
    /// pouvoir mapper `readback_buf` en lecture (`COPY_DST | MAP_READ` ne peut pas
    /// aussi servir de cible de résolution, `QUERY_RESOLVE` l'exige séparé).
    resolve_buf: wgpu::Buffer,
    readback_buf: wgpu::Buffer,
    /// Durée d'un tick GPU en nanosecondes (`Queue::get_timestamp_period`), fixe
    /// pour la durée de vie du device — convertit les deltas de ticks en ms.
    period_ns: f32,
}

impl GpuProfiler {
    /// Noms des `GPU_PROFILER_MARKS - 1` intervalles, dans l'ordre où `render`
    /// écrit les marqueurs correspondants.
    const PASS_NAMES: [&'static str; 4] = ["Ombres", "Scène", "HDR + Bloom", "UI (egui)"];

    fn new(device: &wgpu::Device, period_ns: f32) -> Self {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu_profiler_timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: GPU_PROFILER_MARKS,
        });
        let buf_size = u64::from(GPU_PROFILER_MARKS) * 8; // u64 par timestamp
        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_profiler_resolve"),
            size: buf_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_profiler_readback"),
            size: buf_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            query_set,
            resolve_buf,
            readback_buf,
            period_ns,
        }
    }
}

pub struct Renderer {
    /// `None` en rendu headless (tests de non-régression visuelle) — pas de
    /// fenêtre, pas de surface d'écran, pas d'UI egui.
    pub window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    pipeline: wgpu::RenderPipeline,
    /// Fond de ciel, dessiné en premier dans la passe principale.
    sky_pipeline: wgpu::RenderPipeline,
    /// Tone mapping HDR → LDR, dessiné après la passe principale.
    tonemap_pipeline: wgpu::RenderPipeline,
    tonemap_layout: wgpu::BindGroupLayout,
    tonemap_sampler: wgpu::Sampler,
    /// Cible HDR de la passe principale en mode fenêtré — redimensionnée
    /// dans `resize()`, comme `depth_view`. Les chemins headless/test créent la leur en
    /// local (taille demandée par l'appelant, indépendante de la fenêtre).
    hdr_view: wgpu::TextureView,
    /// Cible multi-échantillonnée de la passe principale, résolue vers `hdr_view` en fin
    /// de passe (`resolve_target`) — `None` si le MSAA est désactivé (qualité « Basse »,
    /// rendu headless, ou adaptateur GPU sans support à `msaa_samples` échantillons),
    /// auquel cas la passe dessine directement dans `hdr_view` comme avant. Redimensionnée
    /// dans `resize()` avec `hdr_view`.
    msaa_color_view: Option<wgpu::TextureView>,
    /// Nombre d'échantillons MSAA de la passe principale (1 = désactivé). Fixé une
    /// fois à la création du renderer (`RenderQuality::msaa_samples`, cf. `new_impl`) —
    /// change de qualité en cours de partie ne reconstruit pas les pipelines/cibles,
    /// comme la plupart des moteurs qui exigent un redémarrage pour ce réglage.
    msaa_samples: u32,
    /// Chaîne de bloom, cf. `render_bloom` — trois pipelines partageant
    /// `bloom_sample_layout` (seuil, downsample, upsample) et une petite texture à
    /// plusieurs mips en mode fenêtré (`bloom_mip_views`, redimensionnée dans
    /// `resize()` comme `hdr_view`).
    bloom_threshold_pipeline: wgpu::RenderPipeline,
    bloom_downsample_pipeline: wgpu::RenderPipeline,
    bloom_upsample_pipeline: wgpu::RenderPipeline,
    bloom_sample_layout: wgpu::BindGroupLayout,
    bloom_intensity_buf: wgpu::Buffer,
    bloom_mip_views: Vec<wgpu::TextureView>,
    depth_view: wgpu::TextureView,
    model_layout: wgpu::BindGroupLayout,

    camera_buf: wgpu::Buffer,
    light_buf: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    meshes: HashMap<MeshKind, GpuMesh>,
    imported_gpu: Vec<GpuMesh>,
    /// Mesh GPU skinné, aligné avec `imported_gpu`/`Scene::imported` :
    /// `None` pour un import statique (pas de skin), `Some` sinon. Séparé de
    /// `imported_gpu` plutôt qu'un enum : le mesh statique reste disponible même pour un
    /// objet skinné (utile si un jour un LOD non skinné est voulu), et la grande majorité
    /// des entrées n'ont simplement rien ici.
    imported_gpu_skinned: Vec<Option<GpuMesh>>,
    /// Données d'instances de tous les objets (groupe 1, storage), indexées par `instance_index`.
    models_buf: wgpu::Buffer,
    models_bind_group: wgpu::BindGroup,
    models_capacity: usize,
    /// Plan de rendu de la frame : un descripteur par objet, dans l'ordre du buffer d'instances.
    draw_plan: Vec<InstanceDraw>,
    /// Objets skinnés : (indice scène, instance_index dans `models_buf`),
    /// hors du batching de `draw_plan` (chaque objet a sa propre palette de joints,
    /// dessiné individuellement par `draw_skinned_objects`). Leurs `ModelUniform` occupent
    /// la queue de `models_buf`, après les objets statiques de `draw_plan`.
    draw_plan_skinned: Vec<(usize, u32)>,
    /// Tampons réutilisés chaque frame (évite deux allocations par frame).
    order_scratch: Vec<usize>,
    models_scratch: Vec<ModelUniform>,
    /// Nombre d'objets au dernier tri de `order_scratch` (re-tri paresseux).
    last_sort_len: usize,
    /// Hash des entrées de rendu (objets + caméra) à la dernière reconstruction du plan
    /// de dessin : si inchangé, on saute le rebuild (skip au repos, sûr par construction).
    last_render_hash: u64,

    gizmo_pipeline: wgpu::RenderPipeline,
    gizmo_vbuf: wgpu::Buffer,

    // --- debug drawing : mêmes pipeline/format que les gizmos, buffer
    //     séparé et redimensionnable (le nombre de segments n'est pas borné à l'avance,
    //     contrairement aux gizmos de manipulation). Vidé (`AppState::debug_lines`)
    //     après chaque frame de rendu.
    debug_vbuf: wgpu::Buffer,
    debug_capacity: usize,

    // --- grille de référence au sol (depth-testée, dans la passe principale) ---
    grid_pipeline: wgpu::RenderPipeline,
    grid_vbuf: wgpu::Buffer,
    grid_count: u32,

    // --- ombres (shadow mapping) ---
    shadow_view: wgpu::TextureView,
    shadow_bind_group: wgpu::BindGroup,
    shadow_pipeline: wgpu::RenderPipeline,

    // --- textures (groupe 3) ---
    tex_layout: wgpu::BindGroupLayout,
    tex_sampler: wgpu::Sampler,
    /// Bind groups de texture par chemin ; "" = texture blanche par défaut.
    textures: HashMap<String, wgpu::BindGroup>,
    /// Génération de mipmaps à l'import : pipeline/layout/sampler dédiés,
    /// utilisés par `make_texture` pour chaque niveau au-delà du mip 0.
    mipgen_pipeline: wgpu::RenderPipeline,
    mipgen_layout: wgpu::BindGroupLayout,
    mipgen_sampler: wgpu::Sampler,

    editor: Option<Editor>,
    /// Nom du backend GPU réel (Metal / Vulkan / …), pour le bandeau d'état.
    backend: String,

    // --- skinning GPU : palette de matrices de joints + pipeline dédié
    //     (vertex `skinned.wgsl`, fragment `fs_main` de main.wgsl **partagée**, même
    //     éclairage que le chemin statique). Dessine les objets skinnés de la scène
    //     (`render`/`render_scene_headless`, via `draw_plan_skinned`) ; `render_skinned_test`
    //     couvre en plus un chemin headless dédié à un seul mesh, hors scène.
    //     Groupe 1 dédié (`skinned_model_layout`) : `models` + joints fusionnés, pour
    //     tenir dans la limite WebGPU de 4 bind groups (cf. `pipelines.rs::build`).
    skinned_pipeline: wgpu::RenderPipeline,
    /// Passe d'ombre des objets skinnés (audit du 17 juillet 2026) : profondeur seule
    /// depuis la lumière, vertex de skinning (`vs_skinned_shadow`) — sans lui, aucun
    /// objet skinné ne projetait d'ombre (la passe d'ombre n'itérait que `draw_plan`).
    skinned_shadow_pipeline: wgpu::RenderPipeline,
    skinned_model_layout: wgpu::BindGroupLayout,
    joint_buf: wgpu::Buffer,
    /// Référence `models_buf` + `joint_buf` : à recréer si l'un des deux l'est
    /// (cf. `sync_objects`, seul site où `models_buf` change de capacité).
    skinned_models_bind_group: wgpu::BindGroup,
    joint_capacity: usize,
    // --- Tampons réutilisés du chemin skinning (audit perf, juillet 2026) : comme
    //     `order_scratch`/`models_scratch` pour le chemin statique, zéro allocation
    //     par frame une fois les capacités atteintes. ---
    /// Offsets dynamiques de la frame, dans l'ordre de `draw_plan_skinned`
    /// (rempli par `prepare_skinned_draws`, lu par `draw_skinned_objects`/
    /// `draw_skinned_shadows`).
    skinned_offsets_scratch: Vec<Option<u32>>,
    /// Palette de joints d'**un** objet, recalculée par objet dans la boucle de
    /// `prepare_skinned_draws`.
    joint_matrices_scratch: Vec<glam::Mat4>,
    /// Conversion `Mat4` → `[[f32; 4]; 4]` avant `write_buffer` (cf. `write_joint_matrices`).
    joint_raw_scratch: Vec<[[f32; 4]; 4]>,
    /// Tampons internes de `compute_joint_matrices_into` (résolution par vagues).
    skinning_scratch: crate::scene::import::SkinningScratch,
    /// Nombre d'objets skinnés **ignorés** (pas dessinés du tout) à la dernière frame
    /// faute de créneau (`slot >= MAX_SKINNED_INSTANCES`) — garde-fou visible du
    /// dépassement silencieux de capacité, exposé par `skinned_dropped_count`.
    skinned_dropped_last_frame: u32,
    /// Chemins de texture dont le chargement a échoué : mémorisés pour ne pas
    /// réessayer (et re-logger) à chaque frame — le dessin retombe sur la texture
    /// blanche `""` déjà en cache via le repli des sites de draw, sans en recréer une.
    /// Vidé par `invalidate_asset_textures` (hot-reload : un fichier réparé redevient
    /// chargeable).
    failed_textures: std::collections::HashSet<String>,

    // --- profiler GPU (Sprint 112) ---
    /// `None` si l'adaptateur ne supporte pas `TIMESTAMP_QUERY_INSIDE_ENCODERS`.
    gpu_profiler: Option<GpuProfiler>,
    /// Durée (ms) de chaque passe mesurée à la dernière lecture réussie — vide tant
    /// qu'aucune frame n'a été profilée (panneau jamais ouvert, ou pas de support GPU).
    gpu_pass_timings_ms: Vec<(&'static str, f32)>,
    /// Estimation du nombre de draw calls de la dernière frame (scène + ombre,
    /// cf. `Renderer::render`) — dérivée de `draw_plan`/`draw_plan_skinned`, pas
    /// comptée sur chaque site d'appel réel (bloom/tonemap/UI ajoutent quelques
    /// draws fixes non comptés ici, negligibles face au coût de la scène).
    last_frame_draw_calls: u32,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Renderer, String> {
        let size = window.inner_size();
        Self::new_impl(Some(window), size).await
    }

    /// Rendu headless : pas de fenêtre ni de surface d'écran (golden tests).
    /// `compatible_surface: None` à la création de l'adaptateur ; format fixe
    /// (`Rgba8UnormSrgb`) puisqu'il n'y a pas de surface pour en dicter un.
    pub async fn new_headless(width: u32, height: u32) -> Result<Renderer, String> {
        let size = winit::dpi::PhysicalSize::new(width.max(1), height.max(1));
        Self::new_impl(None, size).await
    }

    async fn new_impl(
        window: Option<Arc<Window>>,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> Result<Renderer, String> {
        let instance = wgpu::Instance::default();
        let surface = match &window {
            Some(w) => Some(
                instance
                    .create_surface(w.clone())
                    .map_err(|e| format!("Création de la surface impossible : {e}"))?,
            ),
            None => None,
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("Aucun adaptateur GPU trouvé : {e}"))?;

        // Profiler GPU (Sprint 112) : timestamp queries, demandées seulement si
        // l'adaptateur les supporte — sinon `required_features` resterait vide et
        // `GpuProfiler::new` ne serait jamais appelé (`gpu_profiler` reste `None`,
        // dégradation silencieuse comme pour `gilrs`/`notify`).
        let profiler_features =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        // Phase E (`sprintoptimation3daudit10h.md`, Sprint 6) : compression BC3 des
        // textures d'albédo, demandée seulement si l'adaptateur la supporte (desktop en
        // pratique — mobile/web n'exposent typiquement pas `TEXTURE_COMPRESSION_BC`,
        // même dégradation silencieuse que le profiler ci-dessus). Cf. `gfx::texcompress`.
        let texture_compression_features = wgpu::Features::TEXTURE_COMPRESSION_BC;
        let requested_features =
            (profiler_features | texture_compression_features) & adapter.features();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: requested_features,
                // Limites du GPU réel (iOS/mobile en ont de plus basses que les défauts).
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| format!("Échec création du device : {e}"))?;

        let gpu_profiler = requested_features
            .contains(profiler_features)
            .then(|| GpuProfiler::new(&device, queue.get_timestamp_period()));

        let backend = format!("{:?}", adapter.get_info().backend);
        // Ligne de démarrage attendue par un nouvel utilisateur (Phase A,
        // sprint.19matin.md) : quel GPU et quel backend, une seule ligne.
        log::info!("GPU : {} ({backend})", adapter.get_info().name);

        let (format, alpha_mode) = match &surface {
            Some(s) => {
                let caps = s.get_capabilities(&adapter);
                // GPU dégénéré (surface incompatible) : `caps.formats`/`alpha_modes` peuvent
                // être vides → on remonte une erreur claire au lieu de paniquer en indexant `[0]`.
                let format = caps
                    .formats
                    .iter()
                    .copied()
                    .find(|f| f.is_srgb())
                    .or_else(|| caps.formats.first().copied())
                    .ok_or_else(|| "Aucun format de surface supporté par le GPU".to_string())?;
                let alpha_mode =
                    caps.alpha_modes.first().copied().ok_or_else(|| {
                        "Aucun mode alpha de surface supporté par le GPU".to_string()
                    })?;
                (format, alpha_mode)
            }
            // Rendu headless : pas de surface pour dicter un format → fixe, stable d'une
            // machine à l'autre (comparaison de pixels des golden tests).
            None => (
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::CompositeAlphaMode::Opaque,
            ),
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // Fifo (vsync) : cale le rendu sur l'écran, fluide et peu gourmand.
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        if let Some(s) = &surface {
            s.configure(&device, &config);
        }

        // MSAA : uniquement en mode fenêtré (le rendu headless doit rester déterministe
        // pixel pour pixel pour les golden tests, cf. `render_scene_headless`), au niveau
        // visé par `RenderQuality` (`msaa_samples`, opt-out mobile sur « Basse » — même
        // logique que `bloom_enabled`), et seulement si l'adaptateur supporte réellement
        // ce nombre d'échantillons pour les deux formats de la passe principale (repli
        // silencieux à 1 sinon, ex. certains backends GLES/WebGL).
        let msaa_samples = if window.is_some() {
            let wanted = crate::app::build_config::BuildConfig::load()
                .render_quality
                .msaa_samples();
            let supported = wanted <= 1
                || (adapter
                    .get_texture_format_features(HDR_FORMAT)
                    .flags
                    .sample_count_supported(wanted)
                    && adapter
                        .get_texture_format_features(DEPTH_FORMAT)
                        .flags
                        .sample_count_supported(wanted));
            if supported { wanted } else { 1 }
        } else {
            1
        };

        let bundle = pipelines::build(
            &device,
            &queue,
            &config,
            size,
            window.as_ref(),
            msaa_samples,
        );
        let PipelineBundle {
            pipeline,
            sky_pipeline,
            tonemap_pipeline,
            tonemap_layout,
            tonemap_sampler,
            hdr_view,
            msaa_color_view,
            bloom_threshold_pipeline,
            bloom_downsample_pipeline,
            bloom_upsample_pipeline,
            bloom_sample_layout,
            bloom_intensity_buf,
            bloom_mip_views,
            depth_view,
            model_layout,
            camera_buf,
            light_buf,
            camera_bind_group,
            meshes,
            models_buf,
            models_bind_group,
            models_capacity,
            gizmo_pipeline,
            gizmo_vbuf,
            debug_vbuf,
            debug_capacity,
            grid_pipeline,
            grid_vbuf,
            grid_count,
            shadow_view,
            shadow_bind_group,
            shadow_pipeline,
            tex_layout,
            tex_sampler,
            textures,
            mipgen_pipeline,
            mipgen_layout,
            mipgen_sampler,
            editor,
            skinned_pipeline,
            skinned_shadow_pipeline,
            skinned_model_layout,
            joint_buf,
            skinned_models_bind_group,
        } = bundle;

        Ok(Renderer {
            window,
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            sky_pipeline,
            tonemap_pipeline,
            tonemap_layout,
            tonemap_sampler,
            hdr_view,
            msaa_color_view,
            msaa_samples,
            bloom_threshold_pipeline,
            bloom_downsample_pipeline,
            bloom_upsample_pipeline,
            bloom_sample_layout,
            bloom_intensity_buf,
            bloom_mip_views,
            depth_view,
            model_layout,
            camera_buf,
            light_buf,
            camera_bind_group,
            meshes,
            imported_gpu: Vec::new(),
            imported_gpu_skinned: Vec::new(),
            models_buf,
            models_bind_group,
            models_capacity,
            draw_plan: Vec::new(),
            draw_plan_skinned: Vec::new(),
            order_scratch: Vec::new(),
            models_scratch: Vec::new(),
            last_sort_len: usize::MAX,
            last_render_hash: 0,
            gizmo_pipeline,
            gizmo_vbuf,
            debug_vbuf,
            debug_capacity,
            grid_pipeline,
            grid_vbuf,
            grid_count,
            shadow_view,
            shadow_bind_group,
            shadow_pipeline,
            tex_layout,
            tex_sampler,
            textures,
            mipgen_pipeline,
            mipgen_layout,
            mipgen_sampler,
            editor,
            backend,
            skinned_pipeline,
            skinned_shadow_pipeline,
            skinned_model_layout,
            joint_buf,
            skinned_models_bind_group,
            joint_capacity: JOINT_CAPACITY,
            skinned_offsets_scratch: Vec::new(),
            joint_matrices_scratch: Vec::new(),
            joint_raw_scratch: Vec::new(),
            skinning_scratch: crate::scene::import::SkinningScratch::default(),
            skinned_dropped_last_frame: 0,
            failed_textures: std::collections::HashSet::new(),
            gpu_profiler,
            gpu_pass_timings_ms: Vec::new(),
            last_frame_draw_calls: 0,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        let Some(surface) = self.surface.as_ref() else {
            return; // rendu headless : pas de surface à reconfigurer
        };
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, &self.config, self.msaa_samples);
        (self.hdr_view, self.msaa_color_view) = create_hdr_view(
            &self.device,
            new_size.width,
            new_size.height,
            self.msaa_samples,
        );
        self.bloom_mip_views =
            create_bloom_mip_views(&self.device, new_size.width, new_size.height);
    }

    /// Recrée `debug_vbuf` en le doublant tant qu'il ne peut pas contenir `n` sommets,
    /// même politique de croissance que `create_models_buffer`.
    fn ensure_debug_capacity(&mut self, n: usize) {
        if n <= self.debug_capacity {
            return;
        }
        let cap = n.next_power_of_two().max(64);
        self.debug_vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("debug_vbuf"),
            size: (cap * std::mem::size_of::<GizmoVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.debug_capacity = cap;
    }

    /// Envoie la palette de matrices de joints d'**une** instance skinnée au GPU, dans son
    /// créneau `slot` du buffer partagé (offset dynamique, cf. commentaire
    /// sur `JOINT_SLOT_BYTES`). Tronque silencieusement (`log::warn!`) au-delà de
    /// `joint_capacity` plutôt que de paniquer ou d'écrire hors créneau — un rig
    /// anormalement gros dégraderait l'anim plutôt que de planter le rendu. `slot` au-delà
    /// de `MAX_SKINNED_INSTANCES` renvoie `None` : **aucune écriture** n'a lieu, l'offset
    /// `0` resterait sinon un offset valide (celui du véritable occupant du slot 0) —
    /// le renvoyer ici aurait fait dessiner cet objet avec la palette de joints d'un
    /// *autre* objet skinné (squelette différent) au lieu de rester simplement invisible.
    /// Bug observé en pratique : le joueur (mesh importé le plus loin dans l'ordre de tri
    /// par mesh/texture) dépassait la capacité dès que ≥ `MAX_SKINNED_INSTANCES` créatures
    /// skinnées étaient aussi visibles, et se faisait dessiner éclaté/scindé avec le
    /// squelette d'une créature quelconque.
    ///
    /// Renvoie l'offset dynamique (octets) à passer à `set_bind_group(1, &skinned_models_bind_group, &[offset])`,
    /// ou `None` si l'instance doit être sautée (pas de créneau disponible — les
    /// appelants comptabilisent ce cas dans `skinned_dropped_last_frame`, garde-fou
    /// visible du dépassement de `MAX_SKINNED_INSTANCES`).
    ///
    /// `raw` : tampon de conversion fourni par l'appelant (vidé puis rempli) — évite
    /// une allocation par objet skinné et par frame (audit perf, juillet 2026).
    fn write_joint_matrices(
        &self,
        slot: usize,
        matrices: &[glam::Mat4],
        raw: &mut Vec<[[f32; 4]; 4]>,
    ) -> Option<u32> {
        if slot >= MAX_SKINNED_INSTANCES {
            log::warn!(
                "skinning : créneau {slot} au-delà de la capacité ({MAX_SKINNED_INSTANCES}) — objet ignoré"
            );
            return None;
        }
        let n = matrices.len().min(self.joint_capacity);
        if matrices.len() > self.joint_capacity {
            log::warn!(
                "skinning : {} joints, capacité {} — le reste est ignoré",
                matrices.len(),
                self.joint_capacity
            );
        }
        raw.clear();
        raw.extend(matrices[..n].iter().map(|m| m.to_cols_array_2d()));
        let offset = slot as wgpu::BufferAddress * JOINT_SLOT_BYTES;
        self.queue
            .write_buffer(&self.joint_buf, offset, bytemuck::cast_slice(raw));
        Some(offset as u32)
    }

    /// Calcule et envoie au GPU la palette de joints de chaque objet skinné visible de la
    /// frame (`self.draw_plan_skinned`, déjà construit par `write_uniforms`),
    /// **avant** toute passe de rendu (cf. commentaire aux sites d'appel : `write_buffer`
    /// n'est pas ordonné avec les draw calls d'un encoder pas encore soumis). Remplit
    /// `self.skinned_offsets_scratch` avec les offsets dynamiques, dans l'ordre de
    /// `draw_plan_skinned`, à passer à
    /// `set_bind_group(1, &skinned_models_bind_group, &[offset])` lors du dessin réel dans la
    /// passe — `None` (mesh non importé/introuvable, pas de squelette, ou capacité de
    /// créneaux dépassée dans `write_joint_matrices`) signifie que `draw_skinned_objects`
    /// doit sauter l'objet plutôt que de le dessiner avec l'offset ambigu `0`, qui est
    /// *aussi* un offset valide pour l'objet réellement au slot 0.
    ///
    /// Audit perf (juillet 2026) : plus aucune allocation par frame ici — les tampons
    /// (`skinned_offsets_scratch`, `joint_matrices_scratch`, `joint_raw_scratch`,
    /// `skinning_scratch`) sont des champs réutilisés, et `draw_plan_skinned` est
    /// `mem::take`-é le temps de la boucle (au lieu de l'ancien `.clone()` par frame,
    /// simple contournement d'emprunt).
    fn prepare_skinned_draws(&mut self, scene: &Scene) {
        self.skinned_dropped_last_frame = 0;
        let plan = std::mem::take(&mut self.draw_plan_skinned);
        let mut offsets = std::mem::take(&mut self.skinned_offsets_scratch);
        let mut matrices = std::mem::take(&mut self.joint_matrices_scratch);
        let mut raw = std::mem::take(&mut self.joint_raw_scratch);
        let mut scratch = std::mem::take(&mut self.skinning_scratch);
        offsets.clear();
        for (slot, &(obj_idx, _instance)) in plan.iter().enumerate() {
            let obj = &scene.objects[obj_idx];
            let MeshKind::Imported(mesh_idx) = obj.mesh else {
                offsets.push(None);
                continue;
            };
            let Some(imported) = scene.imported.get(mesh_idx as usize) else {
                offsets.push(None);
                continue;
            };
            let Some(skeleton) = &imported.skeleton else {
                offsets.push(None);
                continue;
            };
            // Garde-fou visible (audit du 17 juillet 2026) : au-delà de la capacité de
            // créneaux, l'objet est **ignoré** (pas dessiné) — compté ici pour que le
            // dépassement ne reste pas qu'un `log::warn` (cf. `skinned_dropped_count`).
            if slot >= MAX_SKINNED_INSTANCES {
                self.skinned_dropped_last_frame += 1;
            }
            // Sans `AnimationState` (ou clip introuvable/vide) : pose de liaison figée,
            // pas une erreur — un mesh skinné a le droit de rester immobile (décor posé).
            let find_clip = |name: &str| imported.clips.iter().find(|c| c.name == name);
            let anim = obj.animation.as_ref();
            let clip = anim
                .filter(|a| !a.clip.is_empty())
                .and_then(|a| find_clip(&a.clip));
            let time = anim.map(|a| a.time).unwrap_or(0.0);
            // Fondu enchaîné : `blend < 1.0` tant qu'une transition est en
            // cours (cf. `AppState::sim_step`) — mélange avec le clip quitté au niveau
            // des poses locales, pas des matrices monde (`compute_joint_matrices_blended`).
            match anim.filter(|a| a.blend < 1.0 && !a.prev_clip.is_empty()) {
                Some(a) => crate::scene::import::compute_joint_matrices_blended_into(
                    skeleton,
                    find_clip(&a.prev_clip),
                    a.prev_time,
                    clip,
                    time,
                    a.blend,
                    &mut scratch,
                    &mut matrices,
                ),
                None => crate::scene::import::compute_joint_matrices_into(
                    skeleton,
                    clip,
                    time,
                    &mut scratch,
                    &mut matrices,
                ),
            };
            offsets.push(self.write_joint_matrices(slot, &matrices, &mut raw));
        }
        self.draw_plan_skinned = plan;
        self.skinned_offsets_scratch = offsets;
        self.joint_matrices_scratch = matrices;
        self.joint_raw_scratch = raw;
        self.skinning_scratch = scratch;
    }

    /// Nombre d'objets skinnés visibles **non dessinés** à la dernière frame préparée,
    /// faute de créneau de palette de joints (`slot >= MAX_SKINNED_INSTANCES`) —
    /// garde-fou visible du dépassement de capacité (audit du 17 juillet 2026) : `0`
    /// en régime normal ; une valeur non nulle signifie qu'il faut remonter
    /// `MAX_SKINNED_INSTANCES` (cf. sa doc). Note : le panneau de stats vit dans
    /// `src/editor` (hors de portée ici) — la stat est exposée côté renderer, prête à
    /// y être affichée.
    pub fn skinned_dropped_count(&self) -> u32 {
        self.skinned_dropped_last_frame
    }

    /// Dessine les objets skinnés de `self.draw_plan_skinned`, un draw individuel par
    /// objet (chacun avec sa propre palette de joints — pas de batching possible ici,
    /// contrairement aux objets statiques). `offsets` doit venir de
    /// `prepare_skinned_draws` sur la même frame, dans le même ordre — un `None` (mesh
    /// non importé/introuvable, pas de squelette, ou capacité de créneaux dépassée)
    /// **saute** l'objet plutôt que de le dessiner avec l'offset `0`, qui appartient à un
    /// autre objet skinné (cf. la doc de `write_joint_matrices` : le bug qui scindait le
    /// mesh du joueur venait exactement de dessiner malgré un `None`/absence de créneau).
    ///
    /// Renvoie le nombre de draw calls réellement émis (compteur de stats, cf.
    /// `last_frame_draw_calls`).
    fn draw_skinned_objects<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        scene: &Scene,
        offsets: &[Option<u32>],
    ) -> u32 {
        let mut draws = 0;
        for (&(obj_idx, instance_index), &offset) in self.draw_plan_skinned.iter().zip(offsets) {
            let Some(offset) = offset else {
                continue;
            };
            let obj = &scene.objects[obj_idx];
            let MeshKind::Imported(mesh_idx) = obj.mesh else {
                continue;
            };
            let Some(Some(gpu_mesh)) = self.imported_gpu_skinned.get(mesh_idx as usize) else {
                continue;
            };
            let tex = self
                .textures
                .get(&obj.texture)
                .unwrap_or(&self.textures[""]);
            pass.set_pipeline(&self.skinned_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.skinned_models_bind_group, &[offset]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(3, tex, &[]);
            pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(
                0..gpu_mesh.num_indices,
                0,
                instance_index..instance_index + 1,
            );
            draws += 1;
        }
        draws
    }

    /// Ombres des objets skinnés (audit du 17 juillet 2026) : dessine chaque objet de
    /// `draw_plan_skinned` dans la carte d'ombre avec `skinned_shadow_pipeline`
    /// (profondeur seule + vertex de skinning — sans ce chemin, un objet skinné ne
    /// projetait **aucune** ombre, la passe d'ombre n'itérant que le plan statique).
    /// Mêmes règles de saut que `draw_skinned_objects` (`offsets` de la même frame,
    /// `None` ⇒ objet sauté). Choix assumé : `draw_plan_skinned` est déjà filtré par le
    /// frustum caméra (contrairement au plan statique, rendu hors champ dans l'ombre) —
    /// une créature hors champ ne projette donc pas d'ombre dans le champ, imprécision
    /// mineure acceptée plutôt que de maintenir un second plan skinné non cullé.
    ///
    /// Renvoie le nombre de draw calls réellement émis (compteur de stats).
    fn draw_skinned_shadows<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        scene: &Scene,
        offsets: &[Option<u32>],
    ) -> u32 {
        let mut draws = 0;
        for (&(obj_idx, instance_index), &offset) in self.draw_plan_skinned.iter().zip(offsets) {
            let Some(offset) = offset else {
                continue;
            };
            let MeshKind::Imported(mesh_idx) = scene.objects[obj_idx].mesh else {
                continue;
            };
            let Some(Some(gpu_mesh)) = self.imported_gpu_skinned.get(mesh_idx as usize) else {
                continue;
            };
            pass.set_pipeline(&self.skinned_shadow_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.skinned_models_bind_group, &[offset]);
            pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(
                0..gpu_mesh.num_indices,
                0,
                instance_index..instance_index + 1,
            );
            draws += 1;
        }
        draws
    }

    /// Rendu headless d'**un** mesh skinné, en une seule instance (chemin de
    /// test/vérification dédié — pas piloté par `draw_plan_skinned`). `app` ne sert qu'à
    /// fournir caméra + lumière (`write_uniforms`) ; sa scène n'est pas dessinée ici.
    pub fn render_skinned_test(
        &mut self,
        app: &mut AppState,
        mesh: &crate::gfx::mesh::SkinnedMeshData,
        joint_matrices: &[glam::Mat4],
        model_transform: glam::Mat4,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        app.camera.aspect = width as f32 / (height as f32).max(1.0);
        self.write_uniforms(app);
        let mut raw = std::mem::take(&mut self.joint_raw_scratch);
        let joint_offset = self
            .write_joint_matrices(0, joint_matrices, &mut raw)
            .expect("slot 0 < MAX_SKINNED_INSTANCES, toujours valide");
        self.joint_raw_scratch = raw;

        let gpu_mesh = crate::gfx::mesh::GpuMesh::new_skinned(&self.device, mesh);

        let model_uniform = ModelUniform {
            model: model_transform.to_cols_array_2d(),
            normal: glam::Mat4::from_mat3(
                glam::Mat3::from_mat4(model_transform).inverse().transpose(),
            )
            .to_cols_array_2d(),
            params: [0.0, 0.0, 0.6, 0.0], // pas de surbrillance ; roughness 0.6, reste par défaut
            color: [1.0, 1.0, 1.0, 1.0],
        };
        self.queue
            .write_buffer(&self.models_buf, 0, bytemuck::bytes_of(&model_uniform));

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skinned_test_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skinned_test_depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        // Cible HDR, locale à cet appel — cf. `hdr_view` de `render()`.
        // Chemin headless : toujours mono-échantillon (`self.msaa_samples == 1`,
        // cf. `new_impl`), donc pas de cible multisample à gérer ici.
        let (hdr_view, _) = create_hdr_view(&self.device, width, height, self.msaa_samples);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("skinned_test_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("skinned_test_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &hdr_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
            pass.set_pipeline(&self.skinned_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.skinned_models_bind_group, &[joint_offset]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(3, &self.textures[""], &[]);
            pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..gpu_mesh.num_indices, 0, 0..1);
        }

        // Tone mapping : HDR → `view` (le format lu par `finish_and_read_rgba`).
        // Pas de bloom ici : ce chemin sert uniquement au golden test de
        // skinning, qui n'a pas besoin du post-effet — `hdr_view` réutilisée comme
        // source de bloom factice, neutralisée par une intensité à 0.
        self.tonemap(&mut encoder, &hdr_view, &hdr_view, 0.0, &view);

        self.finish_and_read_rgba(encoder, &target, width, height)
    }

    /// Transmet l'événement à l'UI. Retourne `true` s'il a été consommé par egui.
    pub fn on_ui_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        let (Some(window), Some(editor)) = (self.window.as_ref(), self.editor.as_mut()) else {
            return false; // rendu headless : pas d'UI
        };
        editor.on_window_event(window, event)
    }

    /// Réglages persistés courants (clé API, remapping manette…), `None` en rendu
    /// headless (pas d'`Editor`). Sprint 110 : lu par `App::gamepad_bindings`, qui
    /// n'a sinon aucun accès direct à `Editor` (privé à ce module).
    pub fn settings(&self) -> Option<&crate::app::settings::Settings> {
        self.editor.as_ref().map(|e| e.settings())
    }

    /// Bascule la fenêtre Multijoueur (bouton Start de la manette) — simple
    /// relais vers `Editor`, privé à ce module ; sans effet en headless.
    pub fn toggle_multiplayer_window(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_multiplayer_window();
        }
    }

    /// Bascule le HUD de Play (bouton Select de la manette) — même relais.
    pub fn toggle_play_hud(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_play_hud();
        }
    }

    /// Bascule l'overlay Paramètres minimal du mode Player (bouton Start de la
    /// manette ou touche Tab, en mode `--player`/mobile — Sprint 2) — même relais.
    pub fn toggle_player_settings(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_player_settings();
        }
    }

    /// Bascule la carte plein écran du mode Player (touche `M`) — même relais.
    pub fn toggle_player_map(&mut self) {
        if let Some(e) = self.editor.as_mut() {
            e.toggle_player_map();
        }
    }

    /// Garantit que le buffer d'instances peut contenir `n` objets (le recrée s'il faut).
    fn sync_objects(&mut self, scene: &Scene) {
        let n = scene.objects.len();
        if n > self.models_capacity {
            let cap = n.next_power_of_two().max(64);
            let (buf, bg) = create_models_buffer(&self.device, &self.model_layout, cap);
            self.models_buf = buf;
            self.models_bind_group = bg;
            self.models_capacity = cap;
            // `skinned_models_bind_group` référence `models_buf` par valeur : doit être
            // recréé avec le nouveau buffer, sinon le pipeline skinné continue de
            // dessiner avec l'ancien (erreur de validation ou instances obsolètes dès
            // que la scène dépasse la capacité initiale avec un mesh skinné présent).
            self.skinned_models_bind_group = create_skinned_models_bind_group(
                &self.device,
                &self.skinned_model_layout,
                &self.models_buf,
                &self.joint_buf,
            );
        }
    }

    /// Résout le `GpuMesh` d'un type de mesh (None si un modèle importé n'est pas encore chargé).
    fn resolve_mesh(&self, mesh: MeshKind) -> Option<&GpuMesh> {
        match mesh {
            MeshKind::Imported(i) => self.imported_gpu.get(i as usize),
            k => self.meshes.get(&k),
        }
    }

    /// Construit les `GpuMesh` des modèles importés pas encore chargés sur GPU.
    fn sync_imported(&mut self, scene: &Scene) {
        while self.imported_gpu.len() < scene.imported.len() {
            let m = &scene.imported[self.imported_gpu.len()];
            self.imported_gpu.push(GpuMesh::new(&self.device, &m.data));
            // Skinning GPU : mesh skinné en plus du statique si le glTF a un
            // skin (`ImportedMesh::skeleton`) — `None` sinon, la grande majorité des imports.
            let skinned = m
                .skinned_mesh_data()
                .map(|d| GpuMesh::new_skinned(&self.device, &d));
            self.imported_gpu_skinned.push(skinned);
        }
    }

    /// Hot-reload (Sprint 111) : vide le cache de textures (sauf la blanche par
    /// défaut, `""`, qui n'est pas chargée depuis un fichier) suite à un changement
    /// détecté dans le dossier d'assets de projet. `sync_textures` recharge alors
    /// depuis le disque au prochain appel — la nouvelle version d'un fichier
    /// retouché s'affiche donc sans redémarrer, quel que soit le schéma utilisé
    /// pour le référencer (`asset://`, `asset-id://`) : plus simple et robuste
    /// qu'une invalidation ciblée par chemin, qui devrait résoudre chaque forme
    /// vers le même fichier disque avant de savoir laquelle jeter.
    pub(crate) fn invalidate_asset_textures(&mut self) {
        self.textures.retain(|k, _| k.is_empty());
        // Les échecs mémorisés redeviennent tentables : un fichier réparé/ajouté
        // sur le disque doit pouvoir se charger au prochain `sync_textures`.
        self.failed_textures.clear();
    }

    /// Charge les textures référencées par la scène pas encore en cache.
    fn sync_textures(&mut self, scene: &Scene) {
        for obj in &scene.objects {
            if obj.texture.is_empty()
                || self.textures.contains_key(&obj.texture)
                || self.failed_textures.contains(&obj.texture)
            {
                continue;
            }
            let Some((rgba, w, h)) = load_rgba(&obj.texture) else {
                log::error!("Texture illisible : {}", obj.texture);
                // Repli : mémorise l'échec pour ne pas réessayer (ni re-logger) à
                // chaque frame — les sites de dessin retombent déjà sur la texture
                // blanche `""` quand le chemin est absent du cache, inutile d'en
                // recréer une 1×1 par chemin cassé comme avant (audit juillet 2026).
                self.failed_textures.insert(obj.texture.clone());
                continue;
            };
            let bg = make_texture(
                &self.device,
                &self.queue,
                &self.tex_layout,
                &self.tex_sampler,
                &self.mipgen_pipeline,
                &self.mipgen_layout,
                &self.mipgen_sampler,
                &rgba,
                w,
                h,
            );
            self.textures.insert(obj.texture.clone(), bg);
        }
    }

    /// Pousse les uniforms (caméra + matrices modèle + surbrillance) depuis l'état.
    /// N'écrit le buffer d'un objet que si sa pose ou sa surbrillance a changé.
    fn write_uniforms(&mut self, app: &AppState) {
        // Recul caméra (Sprint 1, `sprint10audit.md`) : décalage cosmétique du
        // rendu seulement (cf. doc `OrbitCamera::view_proj_shaken`), jamais de
        // `app.camera` lui-même.
        let shake = app.camera_shake_offset();
        let eye = app.camera.eye() + shake;
        let view_proj = app.camera.view_proj_shaken(shake);
        let camera_uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            // `view_proj` est toujours inversible (projection perspective + vue
            // rigide, jamais dégénérée) : pas de garde-fou nécessaire ici.
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&camera_uniform));

        // Éclairage de la scène + matrice de la carte d'ombre.
        let l = &app.scene.light;
        let mut dir = glam::Vec3::from_array(l.dir);
        if dir.length_squared() < 1e-6 {
            dir = glam::Vec3::Y;
        }
        dir = dir.normalize();
        // caméra orthographique placée « au niveau de la lumière », regardant l'origine.
        let up = if dir.x.abs() < 1e-3 && dir.z.abs() < 1e-3 {
            glam::Vec3::Z
        } else {
            glam::Vec3::Y
        };
        let view = glam::Mat4::look_at_rh(dir * 20.0, glam::Vec3::ZERO, up);
        let proj = glam::Mat4::orthographic_rh(-12.0, 12.0, -12.0, 12.0, 0.1, 60.0);
        let light_vp = proj * view;
        let mut points = [PointLightU {
            pos_range: [0.0; 4],
            color_int: [0.0; 4],
            spot: [0.0, -1.0, 0.0, -1.0],
        }; crate::scene::MAX_POINT_LIGHTS];
        // Culling/LOD : au-delà de la limite, on garde les lumières les plus proches
        // de la caméra (les plus visibles) plutôt que les premières de la liste. Le
        // plafond dépend de la qualité de rendu visée (perf en mode interactif « Basse »).
        let chosen = app
            .scene
            .nearest_point_lights(eye, app.render_quality.light_budget());
        let count = chosen.len();
        for (slot, &li) in points.iter_mut().zip(&chosen) {
            let pl = &app.scene.point_lights[li];
            slot.pos_range = [
                pl.position[0],
                pl.position[1],
                pl.position[2],
                pl.range.max(0.01),
            ];
            slot.color_int = [pl.color[0], pl.color[1], pl.color[2], pl.intensity];
            // Spot : direction normalisée + cos(demi-angle) ; w = -1 → lumière ponctuelle.
            let d = glam::Vec3::from_array(pl.spot_dir);
            let dir = if d.length_squared() > 1e-6 {
                d.normalize()
            } else {
                glam::Vec3::NEG_Y
            };
            let cos_cut = if pl.spot_angle > 0.0 {
                pl.spot_angle.to_radians().cos()
            } else {
                -1.0
            };
            slot.spot = [dir.x, dir.y, dir.z, cos_cut];
        }
        let scene_uniform = SceneUniform {
            light_dir: [l.dir[0], l.dir[1], l.dir[2], 0.0],
            light_color: [l.color[0], l.color[1], l.color[2], 0.0],
            // .y : vue de debug — canal inutilisé jusqu'ici, réutilisé plutôt
            // que d'agrandir l'uniform. Décodé dans `main.wgsl`.
            ambient: [l.ambient, app.debug_view.as_uniform(), 0.0, 0.0],
            light_vp: light_vp.to_cols_array_2d(),
            num_points: [count as f32, 0.0, 0.0, 0.0],
            points,
            sky_horizon: [
                app.scene.sky.horizon_color[0],
                app.scene.sky.horizon_color[1],
                app.scene.sky.horizon_color[2],
                0.0,
            ],
            sky_zenith: [
                app.scene.sky.zenith_color[0],
                app.scene.sky.zenith_color[1],
                app.scene.sky.zenith_color[2],
                0.0,
            ],
            fog: [
                app.scene.sky.fog_color[0],
                app.scene.sky.fog_color[1],
                app.scene.sky.fog_color[2],
                app.scene.sky.fog_density.max(0.0),
            ],
        };
        self.queue
            .write_buffer(&self.light_buf, 0, bytemuck::bytes_of(&scene_uniform));

        // Skip-rebuild : si les entrées de rendu (transforms/couleurs/sélection + caméra)
        // sont identiques à la frame précédente, le plan de dessin et le buffer d'instances
        // sont déjà à jour. Le hash capte TOUT changement pertinent → pas d'affichage figé.
        // (Les uniforms caméra/lumière ci-dessus sont toujours réécrits, ils sont bon marché.)
        let hash = render_input_hash(app);
        if hash == self.last_render_hash && !self.draw_plan.is_empty() {
            return;
        }
        self.last_render_hash = hash;

        // Instances ordonnées par (mesh, texture) pour permettre des draws groupés.
        // On bâtit en parallèle le buffer storage et le plan de rendu (même ordre).
        let planes = frustum_planes(app.camera.view_proj());
        // Culling par distance (Phase C, `sprintoptimation3daudit10h.md`) : complète le
        // frustum ci-dessus, sur la position caméra « pure » (pas le décalage cosmétique
        // de `write_uniforms`, qui ne doit affecter que le rendu, jamais la visibilité).
        let eye = app.camera.eye();
        let n = app.scene.objects.len();
        let order = &mut self.order_scratch;
        // Re-tri paresseux : l'ordre (groupé par mesh/texture pour le batching) ne dépend
        // pas des transforms ; on ne le recalcule que quand le nombre d'objets change.
        // Un ordre « périmé » reste une permutation valide de 0..n → rendu correct, au pire
        // batching sous-optimal jusqu'au prochain ajout/retrait.
        if self.last_sort_len != n {
            order.clear();
            order.extend(0..n);
            order.sort_by(|&a, &b| {
                let oa = &app.scene.objects[a];
                let ob = &app.scene.objects[b];
                mesh_key(oa.mesh)
                    .cmp(&mesh_key(ob.mesh))
                    .then_with(|| oa.texture.cmp(&ob.texture))
            });
            self.last_sort_len = n;
        }

        let models = &mut self.models_scratch;
        models.clear();
        self.draw_plan.clear();
        for &i in order.iter() {
            let obj = &app.scene.objects[i];
            // Skinning GPU : un objet skinné a sa propre palette de joints,
            // incompatible avec le batching par instances de ce plan — dessiné à part par
            // `draw_skinned_objects`, jamais ici (sinon il apparaîtrait deux fois).
            if is_skinned(&app.scene, obj.mesh) {
                continue;
            }
            let model = obj.transform.matrix();
            let highlight = app.highlight_of(i);
            // Matrice normale = inverse-transposée du bloc 3×3 (correct en scale non uniforme).
            let normal3 = glam::Mat3::from_mat4(model).inverse().transpose();
            models.push(ModelUniform {
                model: model.to_cols_array_2d(),
                normal: glam::Mat4::from_mat3(normal3).to_cols_array_2d(),
                params: [highlight, obj.metallic, obj.roughness, obj.emissive],
                color: [obj.color[0], obj.color[1], obj.color[2], 1.0],
            });
            let (lmin, lmax) = app.scene.local_aabb(obj.mesh);
            let radius = culling_radius_for(&app.scene, obj.mesh);
            let visible = obj.visible
                && distance_visible(eye, obj.transform.position, radius)
                && aabb_visible(&planes, model, lmin, lmax);
            // LOD géométrique (Phase D) : distance à la caméra « pure », comme le culling
            // par distance ci-dessus — jamais le décalage cosmétique de `write_uniforms`.
            let lod_mesh =
                foliage_lod_mesh(&app.scene, obj.mesh, eye.distance(obj.transform.position));
            self.draw_plan.push(InstanceDraw {
                obj: i,
                visible,
                mesh: lod_mesh,
            });
        }

        // Objets skinnés : leur ModelUniform occupe la queue de `models`,
        // après tous les objets statiques ci-dessus — `draw_skinned_objects` s'en sert
        // comme `base_instance` pour un draw individuel par objet (chacun avec sa propre
        // palette de joints, incompatible avec le batching des statiques).
        self.draw_plan_skinned.clear();
        for &i in order.iter() {
            let obj = &app.scene.objects[i];
            if !is_skinned(&app.scene, obj.mesh) || !obj.visible {
                continue;
            }
            let model = obj.transform.matrix();
            // Culling AABB approximatif : basé sur la pose de liaison (`aabb_min/max` de
            // l'import), pas sur l'enveloppe réelle de la pose animée — simplification
            // assumée (déplacement des os hors de cette boîte possible sur une anim
            // ample), commune même dans des moteurs de production comme premier jet.
            let (lmin, lmax) = app.scene.local_aabb(obj.mesh);
            if !aabb_visible(&planes, model, lmin, lmax) {
                continue;
            }
            let highlight = app.highlight_of(i);
            let normal3 = glam::Mat3::from_mat4(model).inverse().transpose();
            let instance_index = models.len() as u32;
            models.push(ModelUniform {
                model: model.to_cols_array_2d(),
                normal: glam::Mat4::from_mat3(normal3).to_cols_array_2d(),
                params: [highlight, obj.metallic, obj.roughness, obj.emissive],
                color: [obj.color[0], obj.color[1], obj.color[2], 1.0],
            });
            self.draw_plan_skinned.push((i, instance_index));
        }

        if !models.is_empty() {
            self.queue
                .write_buffer(&self.models_buf, 0, bytemuck::cast_slice(models));
        }
    }

    /// Chaîne de bloom : seuil (`hdr_source` → `mip_views[0]`), descente
    /// (`mip_views[i]` → `mip_views[i+1]`, remplace), puis remontée (`mip_views[i+1]` →
    /// `mip_views[i]`, additionne) — `mip_views[0]` porte le résultat final en sortie,
    /// à moitié résolution HDR, remonté à pleine taille par le filtrage bilinéaire du
    /// sampler quand `tonemap()` l'échantillonne.
    fn render_bloom(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        hdr_source: &wgpu::TextureView,
        mip_views: &[wgpu::TextureView],
    ) {
        let bind = |src: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom_bg"),
                layout: &self.bloom_sample_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.tonemap_sampler),
                    },
                ],
            })
        };
        let draw_into = |encoder: &mut wgpu::CommandEncoder,
                         pipeline: &wgpu::RenderPipeline,
                         bind_group: &wgpu::BindGroup,
                         target: &wgpu::TextureView| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        };

        let threshold_bg = bind(hdr_source);
        draw_into(
            encoder,
            &self.bloom_threshold_pipeline,
            &threshold_bg,
            &mip_views[0],
        );
        for i in 0..mip_views.len() - 1 {
            let bg = bind(&mip_views[i]);
            draw_into(
                encoder,
                &self.bloom_downsample_pipeline,
                &bg,
                &mip_views[i + 1],
            );
        }
        for i in (0..mip_views.len() - 1).rev() {
            let bg = bind(&mip_views[i + 1]);
            draw_into(encoder, &self.bloom_upsample_pipeline, &bg, &mip_views[i]);
        }
    }

    /// Passe de tone mapping + composition du bloom : lit
    /// `hdr_source` (`HDR_FORMAT`, rempli par la passe principale) et `bloom_source`
    /// (résultat de `render_bloom`, `mip_views[0]`), écrit le résultat dans `output`
    /// (format d'affichage final, `config.format`). Partagée par les trois chemins de
    /// rendu (`render`, `render_scene_headless`, `render_skinned_test`) : un seul
    /// endroit qui connaît la courbe ACES. `bloom_intensity` à 0 (opt-out mobile, cf.
    /// `RenderQuality::bloom_enabled`) neutralise `bloom_source` quel que soit son
    /// contenu — pas besoin d'une texture noire dédiée.
    fn tonemap(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        hdr_source: &wgpu::TextureView,
        bloom_source: &wgpu::TextureView,
        bloom_intensity: f32,
        output: &wgpu::TextureView,
    ) {
        // Le shader écrit une couleur linéaire (0..1 après tone mapping) en
        // supposant qu'une vue sRGB de `output` applique l'encodage gamma
        // automatiquement au moment de l'écriture (comportement standard des
        // formats *Srgb — c'est ce qui se passe côté natif, `config.format` y
        // est toujours srgb, cf. `new_impl`). Sur wasm32/WebGPU (Chrome, testé
        // Sprint 114), le canvas n'expose **aucun** format de surface srgb
        // (uniquement `Bgra8Unorm`) : sans ce correctif, l'image sortait
        // beaucoup trop sombre (quasi noire à l'écran) faute d'encodage —
        // `needs_srgb_encode` fait appliquer l'encodage **dans le shader** à la
        // place quand la surface réelle n'est pas srgb, quelle que soit la
        // plateforme (pas un `#[cfg(wasm32)]` : suit le format effectivement
        // choisi, robuste à un futur backend natif sans format srgb non plus).
        let needs_srgb_encode = if self.config.format.is_srgb() {
            0.0
        } else {
            1.0
        };
        self.queue.write_buffer(
            &self.bloom_intensity_buf,
            0,
            bytemuck::bytes_of(&BloomUniform {
                intensity: [bloom_intensity, needs_srgb_encode, 0.0, 0.0],
            }),
        );
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap_bg"),
            layout: &self.tonemap_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr_source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.tonemap_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bloom_source),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.bloom_intensity_buf.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tonemap_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.tonemap_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Lit les timestamp queries de la frame qu'on vient de soumettre (Sprint 112) et
    /// remplit `gpu_pass_timings_ms`. Appelée seulement quand le panneau Profiler est
    /// ouvert (`render`) : `map_async` + `device.poll(Wait)` bloque jusqu'à ce que le
    /// GPU ait fini — un vrai coût, acceptable pour un outil de dev opt-in, exclu du
    /// chemin de rendu par défaut. `resolve_query_set` renvoie des ticks GPU bruts
    /// (`u64`), convertis en ms via `period_ns` (`Queue::get_timestamp_period`).
    fn read_gpu_pass_timings(&mut self) {
        let Some(prof) = self.gpu_profiler.as_ref() else {
            return;
        };
        // Borné à 1s (au lieu de `wait_indefinitely`, Sprint 112 d'origine) : un
        // pilote/adaptateur qui ne relance jamais le callback de `map_async` gelait
        // l'éditeur sans retour possible dès l'ouverture du Profiler (rapporté
        // Phase 0 de `sprintoptimation3daudit10h.md`, 2026-07-18) — mieux vaut
        // renoncer à la mesure GPU de cette frame que geler l'app.
        let slice = prof.readback_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        let timeout = std::time::Duration::from_secs(1);
        let polled = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(timeout),
        });
        if polled.is_err() {
            log::warn!("read_gpu_pass_timings : device.poll a expiré, mesure GPU ignorée");
            return;
        }
        let Ok(Ok(())) = rx.recv_timeout(timeout) else {
            log::warn!("read_gpu_pass_timings : map_async n'a jamais répondu, mesure GPU ignorée");
            return;
        };
        let ticks: Vec<u64> = {
            let data = slice.get_mapped_range();
            // `chunks_exact(8)` garantit des tranches de 8 octets : la conversion en
            // `[u8; 8]` ne peut jamais échouer (Sprint 113b, audit unwrap/expect).
            data.chunks_exact(8)
                .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                .collect()
        };
        prof.readback_buf.unmap();
        self.gpu_pass_timings_ms = GpuProfiler::PASS_NAMES
            .iter()
            .enumerate()
            .filter_map(|(i, &name)| {
                let (t0, t1) = (*ticks.get(i)?, *ticks.get(i + 1)?);
                let ms = t1.saturating_sub(t0) as f32 * prof.period_ns / 1_000_000.0;
                Some((name, ms))
            })
            .collect();
    }

    /// Durée (ms) de chaque passe GPU mesurée à la dernière frame profilée, et
    /// estimation du nombre de draw calls (Sprint 112) — lu par le panneau
    /// « 📊 Profiler FPS ». Vide si le panneau n'a jamais été ouvert, ou si
    /// l'adaptateur ne supporte pas les timestamp queries.
    pub fn gpu_profiler_info(&self) -> (&[(&'static str, f32)], u32) {
        (&self.gpu_pass_timings_ms, self.last_frame_draw_calls)
    }

    pub fn render(&mut self, app: &mut AppState) {
        // 0. Acquérir la surface EN PREMIER. Si indisponible, on sort avant de lancer
        //    egui : sinon on jetterait le `textures_delta` de la frame (atlas de police),
        //    ce qui désynchronise le renderer egui (panic).
        let Some(surface) = self.surface.as_ref() else {
            return; // rendu headless : `render_scene_headless` est le chemin utilisé
        };
        use wgpu::CurrentSurfaceTexture as C;
        let frame = match surface.get_current_texture() {
            C::Success(t) | C::Suboptimal(t) => t,
            C::Outdated | C::Lost => {
                surface.configure(&self.device, &self.config);
                return;
            }
            C::Timeout | C::Occluded => return,
            C::Validation => {
                log::error!("surface validation error");
                return;
            }
        };
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        // Profiler GPU (Sprint 112) : n'écrit les timestamp queries que si le
        // panneau est ouvert **et** le device les supporte — cf. doc de `GpuProfiler`.
        //
        // Désactivé (2026-07-18, Phase 0 de `sprintoptimation3daudit10h.md`) : sur cette
        // machine (Metal, Apple M1), `read_gpu_pass_timings` ne revient jamais dans un
        // délai raisonnable dès que le Profiler est ouvert — chaque frame attend jusqu'au
        // timeout borné (1s, cf. sa doc), ce qui rend l'éditeur perçu comme figé tant que
        // le panneau reste ouvert. FPS/draw calls/`skinned_dropped` restent mesurables
        // sans cette fonctionnalité (non gatée par `gpu_profiling`, cf. plus bas) ; seul le
        // détail des temps GPU par passe (Ombres/Scène/HDR+Bloom/UI) est perdu. À
        // ré-investiguer avec un vrai débogueur GPU avant de réactiver.
        let gpu_profiling = false;

        // 1. Construire l'UI éditeur. En mode player : pas de panneaux, mais on
        //    dessine quand même les contrôles tactiles (joystick + boutons).
        // Calculé avant les appels mutant `app` (évite un conflit d'emprunt au site d'appel).
        let game_time = app.hud_timer();
        let score = app.score();
        let lost = app.is_lost();
        let won = app.has_won();
        let wave = app.wave;
        let mut restart = false;
        let mut resume = false;
        let mut player_net_actions = None;
        let full_output = if app.player {
            if app.scene.mobile.any() {
                let net_status = app.net_status.clone();
                let net_connected = app.is_connected();
                let weapon_label = app.selected_weapon_label();
                let defeated = app.is_locally_defeated();
                let kills = app.displayed_kill_count();
                let assists = app.displayed_assist_count();
                let weapon_inventory = app.ranged_weapon_display_info();
                let selected_weapon = app.selected_weapon();
                let item_inventory = app.inventory_items().to_vec();
                let roster = app.multiplayer_roster();
                let ally_marker = app
                    .nearest_downed_ally_position()
                    .map(|p| (app.camera.view_proj(), p));
                let minimap = app.minimap_data();
                let (output, actions) = editor.run_player_overlay(
                    &window,
                    &app.scene,
                    &mut app.input_state,
                    app.device_preview,
                    app.device_portrait,
                    app.hud_health,
                    app.damage_flash,
                    app.ally_down_flash,
                    ally_marker,
                    game_time,
                    score,
                    lost,
                    won,
                    wave,
                    &mut restart,
                    app.paused,
                    &mut resume,
                    &net_status,
                    net_connected,
                    weapon_label,
                    defeated,
                    app.death_cause,
                    kills,
                    assists,
                    &weapon_inventory,
                    selected_weapon,
                    &item_inventory,
                    &roster,
                    app.round_summary.as_deref(),
                    app.round_summary_won,
                    app.round_contract_label,
                    app.wave_banner_flash,
                    app.wave_banner_wave,
                    &minimap,
                    app.locale,
                );
                if let Some(i) = actions.select_weapon {
                    app.select_weapon(i);
                }
                if let Some(kind) = actions.use_item {
                    app.use_item(kind);
                }
                for action in &actions.hud_clicks {
                    app.push_hud_event(action);
                }
                player_net_actions = Some(actions);
                Some(output)
            } else {
                None
            }
        } else {
            let (gpu_pass_timings_ms, gpu_draw_calls) = self.gpu_profiler_info();
            let status = crate::editor::StatusInfo {
                fps: app.fps(),
                backend: &self.backend,
                ai_busy: app.ai_busy,
                grid: app.show_grid,
                snap: app.snap,
                debug_view: app.debug_view,
                gpu_pass_timings_ms,
                gpu_draw_calls,
                skinned_dropped: self.skinned_dropped_count(),
            };
            let net_status = app.net_status.clone();
            let net_connected = app.is_connected();
            let has_firebase_account = app.has_firebase_account();
            let weapon_label = app.selected_weapon_label();
            let defeated = app.is_locally_defeated();
            let kills = app.displayed_kill_count();
            let assists = app.displayed_assist_count();
            let weapon_inventory = app.ranged_weapon_display_info();
            let selected_weapon = app.selected_weapon();
            let item_inventory = app.inventory_items().to_vec();
            let roster = app.multiplayer_roster();
            let minimap = app.minimap_data();
            let ally_marker = app
                .nearest_downed_ally_position()
                .map(|p| (app.camera.view_proj(), p));
            // Détection d'édition de champs UI (Inspecteur…) pour le drapeau
            // « scène modifiée » : les widgets egui mutent la scène directement,
            // sans passer par `push_undo` — on compare une empreinte des parties
            // éditables juste avant/après la construction de l'UI de la frame.
            let ui_fingerprint_before = app.ui_scene_fingerprint();
            let (full_output, actions) = editor.run(
                &window,
                &mut app.scene,
                &mut app.selection,
                &mut app.selected,
                &mut app.selected_light,
                &mut app.playing,
                &mut app.paused,
                &mut app.time_scale,
                &mut app.gizmo_mode,
                &mut app.input_state,
                &mut app.device_preview,
                &mut app.device_portrait,
                &mut app.view_rect_px,
                app.hud_health,
                app.damage_flash,
                app.ally_down_flash,
                ally_marker,
                game_time,
                score,
                lost,
                won,
                wave,
                status,
                &net_status,
                net_connected,
                &app.chat_messages,
                has_firebase_account,
                &app.leaderboard,
                &app.online_players,
                weapon_label,
                defeated,
                app.death_cause,
                kills,
                assists,
                &weapon_inventory,
                selected_weapon,
                &item_inventory,
                &roster,
                app.round_summary.as_deref(),
                app.round_summary_won,
                app.round_contract_label,
                app.wave_banner_flash,
                app.wave_banner_wave,
                &minimap,
                app.locale,
                app.confirm_quit,
                app.current_project.is_some(),
                app.confirm_close_project,
                app.pending_autosave_recovery.as_deref(),
            );
            if app.ui_scene_fingerprint() != ui_fingerprint_before {
                app.scene_dirty = true;
            }
            if let Some(kind) = actions.use_item {
                app.use_item(kind);
            }
            if let Some(i) = actions.select_weapon {
                app.select_weapon(i);
            }
            for action in &actions.hud_clicks {
                app.push_hud_event(action);
            }
            if actions.save {
                app.save();
            }
            if let Some(path) = actions.save_path {
                app.save_to(&path);
            }
            if actions.load {
                app.load(); // asynchrone : la scène est remplacée plus tard (cf. take_imported_dirty)
            }
            if let Some(path) = actions.load_path {
                app.load_from(&path);
            }
            if let Some(picked_path) = actions.open_project_path {
                // Accepte soit le manifeste (sélecteur générique « Ouvrir… »,
                // Sprint 3), soit directement le dossier racine du projet
                // (« Ouvrir un projet… », projets récents — Sprint 4).
                let picked = std::path::Path::new(&picked_path);
                let dir = if picked.file_name().and_then(|n| n.to_str())
                    == Some(crate::project::MANIFEST_FILE)
                {
                    picked.parent().unwrap_or(picked)
                } else {
                    picked
                };
                match app.open_project(dir) {
                    Ok(_) => {
                        if let Some(project) = &app.current_project {
                            editor.note_recent_project(&project.name, &project.root);
                        }
                    }
                    Err(e) => log::error!("Ouverture du projet échouée : {e}"),
                }
            }
            if let Some(req) = actions.create_project {
                match app.create_project(&req.location, &req.name, req.template) {
                    Ok(_) => {
                        if let Some(project) = &app.current_project {
                            editor.note_recent_project(&project.name, &project.root);
                        }
                    }
                    Err(e) => log::error!("Création du projet échouée : {e}"),
                }
            }
            if actions.close_project {
                app.request_close_project();
            }
            // Réponses à la modale « modifications non sauvegardées » de
            // fermeture de projet (Sprint 4) — mêmes noms que la modale de
            // Quitter, cf. plus bas.
            if actions.close_project_cancel {
                app.confirm_close_project = false;
            }
            if actions.close_project_discard {
                app.close_project();
            }
            if actions.close_project_save {
                if let Some(project) = app.current_project.clone() {
                    let path = project.main_scene_path.to_string_lossy().into_owned();
                    app.save_to(&path);
                    // `save_to` ne baisse `scene_dirty` que sur succès : en cas
                    // d'échec, on reste ouvert plutôt que de fermer en perdant
                    // la scène — même garde que `quit_save`.
                    if !app.scene_dirty {
                        app.close_project();
                    }
                } else {
                    app.confirm_close_project = false;
                }
            }
            if actions.duplicate_project {
                match app.duplicate_project() {
                    Ok(dst) => log::info!("Projet dupliqué dans {}", dst.display()),
                    Err(e) => log::error!("Duplication du projet échouée : {e}"),
                }
            }
            if actions.reveal_project_in_finder
                && let Some(project) = &app.current_project
            {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg("-R")
                        .arg(&project.root)
                        .spawn();
                }
                #[cfg(not(target_os = "macos"))]
                {
                    log::info!(
                        "Révéler dans le Finder n'est disponible que sur macOS ({})",
                        project.root.display()
                    );
                }
            }
            // Réponses à la modale de récupération après crash (Sprint 6).
            if actions.restore_autosave
                && let Some(path) = app.pending_autosave_recovery.take()
                && let Err(e) = app.restore_autosave(&path)
            {
                log::error!("Restauration de l'autosave échouée : {e}");
            }
            if actions.dismiss_autosave_recovery {
                app.pending_autosave_recovery = None;
            }
            if let Some(path) = actions.import {
                app.import_gltf(&path);
            }
            if let Some(kind) = actions.add {
                app.add_object(kind);
            }
            if let Some(i) = actions.delete {
                app.delete_object(i);
            }
            if actions.duplicate {
                app.duplicate_selected();
            }
            if actions.new_scene {
                app.new_scene();
            }
            if actions.load_demo {
                app.load_mobile_demo();
            }
            if actions.load_gameplay {
                app.load_gameplay_demo();
            }
            if actions.load_controller {
                app.load_controller_demo();
            }
            if actions.load_tower {
                app.load_tower_demo();
            }
            if actions.load_temple_run {
                app.load_temple_run_demo();
            }
            if actions.load_components_demo {
                app.load_components_demo();
            }
            if actions.load_ai_duel {
                app.load_zombies_demo();
            }
            if actions.load_mmorpg {
                app.load_mmorpg_demo();
            }
            if actions.load_roguelike {
                app.load_roguelike_demo();
            }
            if actions.load_brawl {
                app.load_brawl_demo();
            }
            if actions.load_boss {
                app.load_boss_demo();
            }
            if actions.load_escorte {
                app.load_escorte_demo();
            }
            if actions.restart {
                restart = true;
            }
            if let Some((url, name, class, room, objective)) = actions.connect_to_server {
                app.connect_to_server_as(&url, &name, class, &room, objective);
            }
            if actions.disconnect_from_server {
                app.disconnect_from_server();
            }
            if let Some((email, password)) = actions.firebase_sign_in {
                let settings = editor.settings();
                app.request_firebase_sign_in(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    email,
                    password,
                );
            }
            if let Some((email, password)) = actions.firebase_sign_up {
                let settings = editor.settings();
                app.request_firebase_sign_up(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    email,
                    password,
                );
            }
            if let Some((lobby_code, sender_name, text)) = actions.send_chat_message {
                let settings = editor.settings();
                app.request_send_chat_message(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    lobby_code,
                    sender_name,
                    text,
                );
            }
            if let Some(lobby_code) = actions.refresh_chat {
                let settings = editor.settings();
                app.request_refresh_chat(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    lobby_code,
                );
            }
            if actions.refresh_leaderboard {
                let settings = editor.settings();
                app.request_refresh_leaderboard(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    10,
                );
            }
            if actions.refresh_online_players {
                let settings = editor.settings();
                app.request_refresh_online_players(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                );
            }
            if actions.presence_heartbeat {
                let settings = editor.settings();
                app.request_presence_heartbeat(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                );
            }
            if actions.align_ground {
                app.align_to_ground();
            }
            if actions.reset_transform {
                app.reset_transform();
            }
            if let Some(scope) = actions.save_as_prefab {
                let result = app.save_selected_as_prefab(scope);
                if let Err(e) = &result {
                    log::warn!("Création du prefab impossible : {e}");
                }
                editor.set_prefab_feedback(result);
            }
            if let Some(asset_id) = actions.instantiate_prefab {
                app.instantiate_prefab(&asset_id);
            }
            if actions.sync_prefab_instances {
                app.sync_prefab_instances();
            }
            if let Some((scope, name)) = actions.delete_prefab {
                crate::assets::delete_prefab(&scope, &name);
            }
            if actions.quit {
                app.request_quit();
            }
            // Réponses à la modale « modifications non sauvegardées ».
            if actions.quit_cancel {
                app.confirm_quit = false;
            }
            if actions.quit_discard {
                app.confirm_quit = false;
                app.should_quit = true;
            }
            if actions.quit_save {
                app.confirm_quit = false;
                app.save();
                // `save` ne baisse `scene_dirty` que sur succès : en cas d'échec
                // (disque plein, chemin illisible…), on reste ouvert plutôt que de
                // quitter en perdant la scène — l'erreur est visible dans la console.
                if !app.scene_dirty {
                    app.should_quit = true;
                }
            }
            if actions.launch_glb_viewer {
                crate::editor::launch_glb_viewer();
            }
            if actions.undo {
                app.undo();
            }
            if actions.redo {
                app.redo();
            }
            if actions.step_frame {
                app.request_step();
            }
            if let Some(cmd) = actions.console_command {
                let result = app.run_console_command(&cmd);
                log::info!("> {cmd}\n{result}");
            }
            if let Some(clip) = actions.play_audio {
                app.play_audio(&clip);
            }
            if let Some(v) = actions.music_volume {
                app.set_music_volume(v);
            }
            if let Some(v) = actions.sfx_volume {
                app.set_sfx_volume(v);
            }
            if let Some(l) = actions.locale {
                app.set_locale(l);
            }
            if let Some(v) = actions.reduce_shake {
                app.set_reduce_shake(v);
            }
            if let Some(down) = actions.move_in_list {
                app.move_selected_in_list(down);
            }
            if let Some((from, to)) = actions.reorder {
                app.reorder_object(from, to);
            }
            if actions.focus_selection {
                app.frame_selected();
            }
            if let Some((idx, req)) = actions.ai_generate {
                app.request_ai_script(idx, req);
            }
            if let Some((req, replace)) = actions.ai_generate_scene {
                app.request_ai_scene(req, replace);
            }
            if actions.set_game_camera {
                app.set_game_camera();
            }
            if actions.clear_game_camera {
                app.clear_game_camera();
            }
            if let Some(max) = actions.optimize_textures {
                let n = app.optimize_textures(max);
                log::info!("Optimisation : {n} texture(s) réduite(s) à ≤ {max} px");
            }
            if let Some(max) = actions.limit_lights {
                app.limit_point_lights(max);
            }
            if actions.convert_textures_pot {
                let n = app.convert_textures_pot();
                log::info!("Convertisseur : {n} texture(s) en puissances de 2");
            }
            if actions.bake_lighting {
                let n = app.bake_lighting();
                log::info!("Bake lighting : {n} lumière(s) ponctuelle(s) figée(s) en émission");
            }
            if actions.perf_mode {
                let t = app.optimize_textures(1024);
                app.limit_point_lights(4);
                log::info!("Mode performance Android : {t} texture(s) réduite(s), ≤ 4 lumières");
            }
            if let Some(preset) = actions.apply_quality_preset {
                app.apply_quality_preset(preset);
                log::info!("Préset qualité appliqué : {preset:?}");
            }
            if actions.collect_assets {
                let n = app.collect_assets();
                log::info!("Assets rassemblés : {n} chemin(s) → asset://");
            }
            if actions.cut {
                app.cut_selected();
            }
            if actions.copy {
                app.copy_selected();
            }
            if actions.paste {
                app.paste();
            }
            if actions.select_all {
                app.select_all();
            }
            if actions.group {
                app.group_selected();
            }
            if actions.ungroup {
                app.ungroup_selected();
            }
            if let Some(axis) = actions.align_axis {
                app.align_selection_axis(axis);
            }
            if let Some(axis) = actions.distribute_axis {
                app.distribute_selection_axis(axis);
            }
            if actions.toggle_grid {
                app.show_grid = !app.show_grid;
            }
            if actions.toggle_snap {
                app.snap = !app.snap;
            }
            if let Some(view) = actions.set_debug_view {
                app.debug_view = view;
            }
            Some(full_output)
        };

        if let Some(actions) = player_net_actions {
            if let Some((url, name, class, room, objective)) = actions.connect_to_server {
                app.connect_to_server_as(&url, &name, class, &room, objective);
            }
            if actions.disconnect_from_server {
                app.disconnect_from_server();
            }
        }

        // Bouton de fin de partie : « Niveau suivant » uniquement pour la démo contrôleur
        // à niveaux ; sinon « Rejouer » — y compris une victoire par manches (zombies) ou
        // par ligne d'arrivée (course infinie/tour), qui doivent juste relancer la scène.
        if restart {
            if app.has_won() && app.is_leveled_demo {
                app.next_level();
            } else {
                app.restart_game();
            }
        }
        // « Reprendre » du menu pause (Phase J) : lève la pause sans autre effet
        // de bord — `restart` gère déjà son propre cas ci-dessus.
        if resume {
            app.paused = false;
        }

        // 2. Comportements (Play), sync GPU, push des uniforms.
        // Chronométré pour le bilan de perf périodique (cf. `log_perf_window`) :
        // départage les à-coups côté simulation (scripts/physique/réseau) des
        // à-coups côté rendu/présentation (le reste de la frame).
        let sim_start = Instant::now();
        app.advance_play();
        app.note_sim_duration(sim_start.elapsed());
        // Une scène chargée en fond vient peut-être de remplacer l'actuelle :
        // reconstruire les meshes GPU importés depuis les nouvelles données.
        if app.take_imported_dirty() {
            self.imported_gpu.clear();
        }
        self.sync_objects(&app.scene);
        self.sync_imported(&app.scene);
        self.sync_textures(&app.scene);

        // Aperçu mobile : restreint la vue 3D à un écran de téléphone (letterbox).
        // L'aspect caméra doit suivre ce rectangle (sinon l'image serait étirée).
        let sw = self.config.width as f32;
        let sh = self.config.height as f32;
        let (dx, dy, dw, dh) = if app.device_preview {
            // Base : région centrale (hors panneaux) remontée par l'éditeur ; sinon plein écran.
            let (bx, by, bw, bh) = app.view_rect_px;
            let (bx, by, bw, bh) = if bw > 1.0 && bh > 1.0 {
                (bx, by, bw, bh)
            } else {
                (0.0, 0.0, sw, sh)
            };
            // Le viewport GPU est en Y depuis le haut, comme les coordonnées egui : pas d'inversion.
            let (rx, ry, rw, rh) = crate::app::device_rect(bw, bh, app.device_portrait);
            (bx + rx, by + ry, rw, rh)
        } else {
            (0.0, 0.0, sw, sh)
        };
        app.camera.aspect = dw / dh.max(1.0);
        self.write_uniforms(app);
        // Skinning GPU : joint_buf entièrement rempli AVANT la passe (comme
        // les lignes de debug ci-dessous) — `queue.write_buffer` n'est pas ordonné avec
        // les draw calls d'un encoder pas encore soumis, donc rien de tout ça ne peut
        // être fait entre deux `draw_indexed` de la passe principale plus bas.
        // Offsets dans `self.skinned_offsets_scratch` (tampon réutilisé, audit perf).
        self.prepare_skinned_draws(&app.scene);

        // Préparer les lignes du gizmo + marqueurs de lumières (jamais en player/aperçu mobile).
        let gizmo_count = if app.player || app.device_preview {
            0
        } else {
            let mut verts: Vec<GizmoVertex> = Vec::new();
            // Marqueur en croix 3D à chaque lumière ponctuelle, teinté par sa couleur.
            for pl in &app.scene.point_lights {
                let c = pl.position;
                let col = pl.color;
                let s = 0.3;
                for axis in 0..3 {
                    let d = axis_dir(axis) * s;
                    verts.push(GizmoVertex {
                        position: [c[0] - d.x, c[1] - d.y, c[2] - d.z],
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: [c[0] + d.x, c[1] + d.y, c[2] + d.z],
                        color: col,
                    });
                }
                // Spot : ligne depuis la lumière le long du cône (visualise l'orientation).
                if pl.spot_angle > 0.0 {
                    let dir = glam::Vec3::from_array(pl.spot_dir);
                    let dir = if dir.length_squared() > 1e-6 {
                        dir.normalize()
                    } else {
                        glam::Vec3::NEG_Y
                    };
                    let end = glam::Vec3::from_array(c) + dir * (pl.range * 0.4).max(0.5);
                    verts.push(GizmoVertex {
                        position: c,
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: end.to_array(),
                        color: col,
                    });
                }
            }
            // Marqueur cyan à la position de la caméra de jeu (si définie).
            if let Some(gc) = app.scene.game_camera {
                let pitch = gc.pitch.clamp(-1.54, 1.54);
                let eye = glam::Vec3::from_array(gc.target)
                    + glam::Vec3::new(
                        gc.distance * pitch.cos() * gc.yaw.sin(),
                        gc.distance * pitch.sin(),
                        gc.distance * pitch.cos() * gc.yaw.cos(),
                    );
                let col = [0.2, 0.85, 0.95];
                let s = 0.4;
                for axis in 0..3 {
                    let d = axis_dir(axis) * s;
                    verts.push(GizmoVertex {
                        position: [eye.x - d.x, eye.y - d.y, eye.z - d.z],
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: [eye.x + d.x, eye.y + d.y, eye.z + d.z],
                        color: col,
                    });
                }
            }
            // Gizmo de translation d'une lumière sélectionnée (3 axes colorés).
            if let Some(li) = app.selected_light
                && !app.gizmo_mode.is_nav()
                && let Some(pl) = app.scene.point_lights.get(li)
            {
                let o = glam::Vec3::from_array(pl.position);
                let colors = [[0.9, 0.25, 0.25], [0.25, 0.9, 0.3], [0.3, 0.45, 1.0]];
                for (axis, color) in colors.iter().enumerate() {
                    let end = o + axis_dir(axis) * GIZMO_LEN;
                    verts.push(GizmoVertex {
                        position: o.to_array(),
                        color: *color,
                    });
                    verts.push(GizmoVertex {
                        position: end.to_array(),
                        color: *color,
                    });
                }
            }
            // Gizmo de manipulation de l'objet sélectionné (aucun en outil de
            // navigation : Main/Orbite/Loupe n'éditent pas).
            if let Some(sel) = app.selection
                && !app.gizmo_mode.is_nav()
            {
                let o = app.scene.objects[sel].transform.position;
                let colors = [[0.9, 0.25, 0.25], [0.25, 0.9, 0.3], [0.3, 0.45, 1.0]];
                match app.gizmo_mode {
                    // Déplacer / Redimensionner : 3 segments d'axe.
                    GizmoMode::Translate | GizmoMode::Scale => {
                        for (axis, color) in colors.iter().enumerate() {
                            let end = o + axis_dir(axis) * GIZMO_LEN;
                            verts.push(GizmoVertex {
                                position: o.to_array(),
                                color: *color,
                            });
                            verts.push(GizmoVertex {
                                position: end.to_array(),
                                color: *color,
                            });
                        }
                    }
                    // Tourner : 3 anneaux (un par axe).
                    GizmoMode::Rotate => {
                        const N: usize = RING_SEGMENTS;
                        for (axis, color) in colors.iter().enumerate() {
                            let (u, w) = axis_basis(axis_dir(axis));
                            for j in 0..N {
                                let a0 = std::f32::consts::TAU * j as f32 / N as f32;
                                let a1 = std::f32::consts::TAU * (j + 1) as f32 / N as f32;
                                let p0 = o + (u * a0.cos() + w * a0.sin()) * GIZMO_LEN;
                                let p1 = o + (u * a1.cos() + w * a1.sin()) * GIZMO_LEN;
                                verts.push(GizmoVertex {
                                    position: p0.to_array(),
                                    color: *color,
                                });
                                verts.push(GizmoVertex {
                                    position: p1.to_array(),
                                    color: *color,
                                });
                            }
                        }
                    }
                    // Navigation (Main/Orbite/Loupe) : filtrée par le garde
                    // `is_nav()` ci-dessus, rien à dessiner.
                    GizmoMode::Pan | GizmoMode::Orbit | GizmoMode::Zoom => {}
                }
            }
            self.queue
                .write_buffer(&self.gizmo_vbuf, 0, bytemuck::cast_slice(&verts));
            verts.len() as u32
        };

        // Debug drawing : segments accumulés pendant la frame (picking,
        // gameplay), dessinés une fois puis vidés — jamais persistants d'une frame à
        // l'autre, contrairement aux gizmos de manipulation ci-dessus.
        let debug_count = {
            let verts: Vec<GizmoVertex> = app
                .debug_lines
                .iter()
                .flat_map(|&(a, b, color)| {
                    [
                        GizmoVertex {
                            position: a.to_array(),
                            color,
                        },
                        GizmoVertex {
                            position: b.to_array(),
                            color,
                        },
                    ]
                })
                .collect();
            app.debug_lines.clear();
            if !verts.is_empty() {
                self.ensure_debug_capacity(verts.len());
                self.queue
                    .write_buffer(&self.debug_vbuf, 0, bytemuck::cast_slice(&verts));
            }
            verts.len() as u32
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });

        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 0);
        }

        // Nombre de draw calls réellement émis par les passes ombre + scène (les
        // boucles ci-dessous l'incrémentent à chaque `draw_indexed`) — remplace
        // l'ancienne estimation `2 × (plan + plan skinné)`, qui surcomptait les
        // statiques (batchés en plages d'instances, pas un draw par objet).
        let mut scene_draw_calls: u32 = 0;

        // Passe d'ombre : profondeur de la scène depuis la lumière → carte d'ombre.
        {
            let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            spass.set_pipeline(&self.shadow_pipeline);
            spass.set_bind_group(0, &self.camera_bind_group, &[]);
            spass.set_bind_group(1, &self.models_bind_group, &[]);
            // Passe d'ombre : rend les objets hors champ (pas de frustum culling), mais
            // **ignore les objets invisibles** (ex. pièce ramassée) pour ne pas laisser
            // d'ombre fantôme. Groupé par mesh, scindé en plages de visibles consécutifs.
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = plan[i].mesh;
                let mut j = i + 1;
                while j < plan.len() && plan[j].mesh == mi {
                    j += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    spass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    spass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < j {
                        if !objs[plan[k].obj].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < j && objs[plan[k].obj].visible {
                            k += 1;
                        }
                        spass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                        scene_draw_calls += 1;
                    }
                }
                i = j;
            }
            // Objets skinnés dans la carte d'ombre (audit du 17 juillet 2026) : pipeline
            // dédié profondeur seule + skinning, cf. `draw_skinned_shadows` — avant, la
            // passe d'ombre n'itérait que `draw_plan` et aucun objet skinné ne projetait
            // d'ombre.
            scene_draw_calls +=
                self.draw_skinned_shadows(&mut spass, &app.scene, &self.skinned_offsets_scratch);
        }
        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 1);
        }

        {
            // La passe principale dessine dans `hdr_view` (HDR_FORMAT),
            // pas directement dans `view` — `self.tonemap()` fait le dernier maillon
            // vers le format d'affichage, après cette passe. Si le MSAA est actif
            // (`msaa_color_view`), la passe dessine dans la cible multi-échantillonnée et
            // se résout vers `hdr_view` (`resolve_target`) — sinon comportement inchangé.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.msaa_color_view.as_ref().unwrap_or(&self.hdr_view),
                    resolve_target: self.msaa_color_view.as_ref().map(|_| &self.hdr_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Aperçu mobile : la scène ne se dessine que dans le rectangle « téléphone ».
            // (Le clear remplit toute la surface → bandes sombres autour = letterbox.)
            pass.set_viewport(dx, dy, dw, dh, 0.0, 1.0);
            pass.set_scissor_rect(dx as u32, dy as u32, dw as u32, dh as u32);

            // Ciel : dessiné en premier, derrière tout le reste.
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.draw(0..3, 0..1);

            // Grille de référence au sol (depth-testée), en mode édition uniquement.
            if app.show_grid && !app.player && !app.device_preview {
                pass.set_pipeline(&self.grid_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
                pass.draw(0..self.grid_count, 0..1);
            }

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(1, &self.models_bind_group, &[]);

            // Rendu instancié : un draw par lot (mesh+texture), scindé en sous-plages
            // d'instances visibles consécutives (frustum culling).
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = plan[i].mesh;
                let tex_key = &objs[plan[i].obj].texture;
                let mut group_end = i + 1;
                while group_end < plan.len()
                    && plan[group_end].mesh == mi
                    && &objs[plan[group_end].obj].texture == tex_key
                {
                    group_end += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    let tex = self
                        .textures
                        .get(tex_key)
                        .unwrap_or_else(|| &self.textures[""]);
                    pass.set_bind_group(3, tex, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    // Plages contiguës d'instances visibles dans le lot.
                    let mut k = i;
                    while k < group_end {
                        if !plan[k].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < group_end && plan[k].visible {
                            k += 1;
                        }
                        pass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                        scene_draw_calls += 1;
                    }
                }
                i = group_end;
            }

            // Gizmo de l'objet sélectionné, par-dessus.
            if gizmo_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.gizmo_vbuf.slice(..));
                pass.draw(0..gizmo_count, 0..1);
            }

            // Debug drawing : même pipeline lignes, buffer dédié.
            if debug_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.debug_vbuf.slice(..));
                pass.draw(0..debug_count, 0..1);
            }

            // Objets skinnés : un draw individuel par objet, palettes déjà
            // envoyées au GPU par `prepare_skinned_draws` avant cette passe.
            scene_draw_calls +=
                self.draw_skinned_objects(&mut pass, &app.scene, &self.skinned_offsets_scratch);
        }
        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 2);
        }

        // Bloom : passes de seuil/downsample/upsample sautées entièrement
        // si désactivé (opt-out mobile, `RenderQuality::bloom_enabled`) — pas seulement
        // neutralisées côté shader, un vrai gain de perf sur le palier visé.
        let bloom_intensity = if app.bloom_enabled && app.render_quality.bloom_enabled() {
            app.scene.sky.bloom_intensity
        } else {
            0.0
        };
        if bloom_intensity > 0.0 {
            self.render_bloom(&mut encoder, &self.hdr_view, &self.bloom_mip_views);
        }
        // Tone mapping : HDR → `view` (format d'affichage réel), avant l'UI
        // (l'UI egui reste en LDR, peinte par-dessus l'image déjà tonemappée).
        self.tonemap(
            &mut encoder,
            &self.hdr_view,
            &self.bloom_mip_views[0],
            bloom_intensity,
            &view,
        );
        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 3);
        }

        // 3. Peindre l'UI egui par-dessus la scène (sauf en mode player).
        let extra = match full_output {
            Some(output) => editor.paint(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                [self.config.width, self.config.height],
                output,
            ),
            None => Vec::new(),
        };
        self.editor = Some(editor);

        // Nombre de draw calls des passes ombre + scène (cf. doc de
        // `last_frame_draw_calls`, bloom/tonemap/UI/ciel/grille/gizmos ajoutent
        // quelques draws fixes non comptés ici) : compté sur les `draw_indexed`
        // réellement émis — l'ancienne estimation `2 × (plan + plan skinné)`
        // surcomptait les statiques (batchés) et devinait au lieu de mesurer.
        self.last_frame_draw_calls = scene_draw_calls;

        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 4);
            encoder.resolve_query_set(&prof.query_set, 0..GPU_PROFILER_MARKS, &prof.resolve_buf, 0);
            let buf_size = u64::from(GPU_PROFILER_MARKS) * 8;
            encoder.copy_buffer_to_buffer(&prof.resolve_buf, 0, &prof.readback_buf, 0, buf_size);
        }

        self.queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        if gpu_profiling {
            self.read_gpu_pass_timings();
        }
        frame.present();
    }

    /// Rendu headless d'une scène dans une texture hors-écran : passe d'ombre + passe
    /// principale, **sans** grille, gizmos ni UI egui (golden tests de
    /// non-régression visuelle). Le pipeline utilisé — mêmes shaders, mêmes bind groups —
    /// est celui de [`Renderer::render`] : un shader qui dérive fait dériver les deux.
    /// Retourne les pixels RGBA8 (`width`×`height`, 4 octets/pixel, sans padding de ligne).
    pub fn render_scene_headless(
        &mut self,
        app: &mut AppState,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        self.sync_objects(&app.scene);
        self.sync_imported(&app.scene);
        self.sync_textures(&app.scene);
        app.camera.aspect = width as f32 / (height as f32).max(1.0);
        self.write_uniforms(app);
        // Skinning GPU : cf. commentaire équivalent dans `render()`.
        self.prepare_skinned_draws(&app.scene);

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        // Depth dédiée à la taille demandée (peut différer de `self.depth_view`, qui suit
        // la taille de la fenêtre en mode interactif). Même sample count que les
        // pipelines : mono-échantillon via `new_headless` (goldens), mais celui de la
        // fenêtre quand ce chemin sert de capture depuis un renderer fenêtré
        // (`screenshot_png`, pont de pilotage) — les pipelines y sont compilés en
        // MSAA, une cible mono-échantillon ferait échouer la validation wgpu.
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless_depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: self.msaa_samples,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        // Cibles HDR, locales à cet appel — cf. `hdr_view`/`msaa_color_view` de
        // `render()` : `msaa_color_view` n'est `Some` qu'en MSAA (renderer fenêtré).
        let (hdr_view, msaa_color_view) =
            create_hdr_view(&self.device, width, height, self.msaa_samples);
        // Chaîne de bloom, locale à cet appel — cf. `bloom_mip_views` de
        // `render()`.
        let bloom_mip_views = create_bloom_mip_views(&self.device, width, height);

        // Debug drawing : même logique que `render()` (préparer + vider avant
        // les passes, dessiner après les meshes texturés dans la passe principale).
        let debug_count = {
            let verts: Vec<GizmoVertex> = app
                .debug_lines
                .iter()
                .flat_map(|&(a, b, color)| {
                    [
                        GizmoVertex {
                            position: a.to_array(),
                            color,
                        },
                        GizmoVertex {
                            position: b.to_array(),
                            color,
                        },
                    ]
                })
                .collect();
            app.debug_lines.clear();
            if !verts.is_empty() {
                self.ensure_debug_capacity(verts.len());
                self.queue
                    .write_buffer(&self.debug_vbuf, 0, bytemuck::cast_slice(&verts));
            }
            verts.len() as u32
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("headless_encoder"),
            });

        // Compte des draw calls (Phase F, `sprintoptimation3daudit10h.md`) — même
        // logique que `render()`/`last_frame_draw_calls` : jusqu'ici ce chemin ne
        // renseignait jamais `last_frame_draw_calls`, donc `gpu_profiler_info()` après
        // un rendu headless retournait toujours 0, aucune régression de draw calls
        // n'aurait été détectable via un benchmark headless.
        let mut scene_draw_calls: u32 = 0;

        // Passe d'ombre — identique à celle de `render()`, sans les gizmos ni l'UI.
        {
            let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless_shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            spass.set_pipeline(&self.shadow_pipeline);
            spass.set_bind_group(0, &self.camera_bind_group, &[]);
            spass.set_bind_group(1, &self.models_bind_group, &[]);
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = plan[i].mesh;
                let mut j = i + 1;
                while j < plan.len() && plan[j].mesh == mi {
                    j += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    spass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    spass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < j {
                        if !objs[plan[k].obj].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < j && objs[plan[k].obj].visible {
                            k += 1;
                        }
                        spass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                        scene_draw_calls += 1;
                    }
                }
                i = j;
            }
            // Objets skinnés dans la carte d'ombre — même correctif que `render()`
            // (audit du 17 juillet 2026), appliqué ici aussi pour que les golden
            // tests capturent les ombres skinnées.
            scene_draw_calls +=
                self.draw_skinned_shadows(&mut spass, &app.scene, &self.skinned_offsets_scratch);
        }

        // Passe principale — identique à celle de `render()`, sans grille ni gizmos.
        // Dessine dans `hdr_view` ; `self.tonemap()` fait le dernier pas
        // vers `view`, juste avant la lecture des pixels.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless_main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    // MSAA : dessine dans la cible multi-échantillonnée et se résout
                    // vers `hdr_view` — même branchement que la passe principale de
                    // `render()`, sinon comportement inchangé (goldens mono-échantillon).
                    view: msaa_color_view.as_ref().unwrap_or(&hdr_view),
                    resolve_target: msaa_color_view.as_ref().map(|_| &hdr_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);

            // Ciel : même geste que dans `render()`.
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.draw(0..3, 0..1);

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(1, &self.models_bind_group, &[]);

            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = plan[i].mesh;
                let tex_key = &objs[plan[i].obj].texture;
                let mut group_end = i + 1;
                while group_end < plan.len()
                    && plan[group_end].mesh == mi
                    && &objs[plan[group_end].obj].texture == tex_key
                {
                    group_end += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    let tex = self
                        .textures
                        .get(tex_key)
                        .unwrap_or_else(|| &self.textures[""]);
                    pass.set_bind_group(3, tex, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < group_end {
                        if !plan[k].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < group_end && plan[k].visible {
                            k += 1;
                        }
                        pass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                        scene_draw_calls += 1;
                    }
                }
                i = group_end;
            }

            // Debug drawing.
            if debug_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.debug_vbuf.slice(..));
                pass.draw(0..debug_count, 0..1);
            }

            // Objets skinnés : cf. commentaire équivalent dans `render()`.
            scene_draw_calls +=
                self.draw_skinned_objects(&mut pass, &app.scene, &self.skinned_offsets_scratch);
        }

        // Cf. `render()` : `last_frame_draw_calls` sert de source unique à
        // `gpu_profiler_info()`, lu aussi bien après `render()` (panneau Profiler en
        // jeu) qu'après `render_scene_headless()` (mesures/benchmarks headless).
        self.last_frame_draw_calls = scene_draw_calls;

        // Bloom : cf. commentaire équivalent dans `render()`.
        let bloom_intensity = if app.bloom_enabled && app.render_quality.bloom_enabled() {
            app.scene.sky.bloom_intensity
        } else {
            0.0
        };
        if bloom_intensity > 0.0 {
            self.render_bloom(&mut encoder, &hdr_view, &bloom_mip_views);
        }
        // Tone mapping : HDR → `view` (le format lu par `finish_and_read_rgba`).
        self.tonemap(
            &mut encoder,
            &hdr_view,
            &bloom_mip_views[0],
            bloom_intensity,
            &view,
        );

        self.finish_and_read_rgba(encoder, &target, width, height)
    }

    /// Capture PNG de l'état courant de la scène (pont de pilotage externe, cf.
    /// `crate::pilot`) : rendu hors-écran via [`Renderer::render_scene_headless`],
    /// donc disponible aussi depuis un renderer **fenêtré** — la cible offscreen
    /// hérite alors du format de la surface (BGRA sur macOS/Metal, contrairement
    /// au RGBA imposé par `new_headless`), d'où le swizzle avant l'écriture PNG.
    pub fn screenshot_png(
        &mut self,
        app: &mut AppState,
        width: u32,
        height: u32,
        path: &std::path::Path,
    ) -> Result<(), String> {
        // `render_scene_headless` consomme `app.debug_lines` (vidées après dessin,
        // comme `render()`) : on les repose ensuite pour que la frame fenêtrée en
        // cours ne perde pas ses lignes de debug à cause d'une capture. L'aspect
        // caméra, lui aussi écrasé, est recalculé par `render()` à chaque frame —
        // rien à restaurer.
        let debug_lines = app.debug_lines.clone();
        let mut pixels = self.render_scene_headless(app, width, height);
        app.debug_lines = debug_lines;
        if matches!(
            self.config.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)
            .map_err(|e| format!("écriture de {} impossible : {e}", path.display()))
    }

    /// Copie `target` vers un buffer lisible CPU, soumet `encoder` et attend le résultat —
    /// partagé par tous les rendus headless (`render_scene_headless`, `render_skinned_test`).
    /// `encoder` doit déjà contenir toutes les passes de dessin dans `target` ;
    /// cette méthode ne fait que la copie finale + lecture.
    fn finish_and_read_rgba(
        &self,
        mut encoder: wgpu::CommandEncoder,
        target: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        // wgpu impose que `bytes_per_row` soit un multiple de
        // `COPY_BYTES_PER_ROW_ALIGNMENT` (256) → on copie avec ce padding puis on le retire.
        let bytes_per_pixel = 4u32;
        let unpadded = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless_readback"),
            size: (padded * height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        // Le callback de `map_async` est invoqué par `poll` ci-dessus : à ce stade le
        // résultat est déjà dans le canal.
        let _ = rx.recv();
        let mapped = slice.get_mapped_range();
        let mut out = vec![0u8; (unpadded * height) as usize];
        for y in 0..height {
            let src_start = (y * padded) as usize;
            let dst_start = (y * unpadded) as usize;
            out[dst_start..dst_start + unpadded as usize]
                .copy_from_slice(&mapped[src_start..src_start + unpadded as usize]);
        }
        drop(mapped);
        readback.unmap();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_count_for_matches_the_standard_formula() {
        // 1 + log2(plus grande dimension) — vérifié contre des puissances
        // de deux connues plutôt qu'en réimplémentant la formule dans le test.
        assert_eq!(mip_count_for(1, 1), 1); // rien à générer sous une texture 1×1
        assert_eq!(mip_count_for(2, 2), 2);
        assert_eq!(mip_count_for(256, 256), 9); // 256,128,64,32,16,8,4,2,1
        assert_eq!(mip_count_for(1024, 1024), 11);
        // Non carrée : la plus grande dimension domine (l'autre s'arrête avant 1×1,
        // ce qui reste correct — wgpu accepte des mips plus petits que 1 sur un axe
        // tant que l'autre n'est pas encore à 1).
        assert_eq!(mip_count_for(256, 64), 9);
        assert_eq!(mip_count_for(64, 256), 9);
    }

    /// Sprint 111 : preuve que `invalidate_asset_textures` force un rechargement
    /// depuis le disque au prochain `sync_textures`, plutôt que de continuer à
    /// servir la version déjà en cache — c'est tout le mécanisme du hot-reload
    /// (`lib.rs::poll_asset_hot_reload` appelle cette méthode dès qu'un événement du
    /// dossier d'assets arrive). Utilise un chemin disque brut (pas `asset://`) :
    /// `assets::read_bytes` le lit tel quel via `std::fs::read`, donc le test n'a
    /// besoin de toucher ni `$HOME` ni le dossier d'assets réel (cf. la garde-fou
    /// d'isolation des tests système, Sprint 105a-3).
    #[test]
    fn invalidate_asset_textures_forces_a_reload_from_disk_on_the_next_sync() {
        let mut renderer = match pollster::block_on(Renderer::new_headless(64, 64)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "invalidate_asset_textures_forces_a_reload_from_disk_on_the_next_sync : \
                     pas de GPU headless ({e}) — test sauté."
                );
                return;
            }
        };

        let dir = std::env::temp_dir().join(format!(
            "motor3derust_hot_reload_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("texture.png");
        image::save_buffer(&path, &[255, 0, 0, 255], 1, 1, image::ColorType::Rgba8).unwrap();

        let scene = crate::scene::Scene {
            objects: vec![crate::scene::SceneObject {
                texture: path.to_str().unwrap().to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        renderer.sync_textures(&scene);
        assert!(
            renderer.textures.contains_key(path.to_str().unwrap()),
            "la texture doit être en cache après le premier sync"
        );

        renderer.invalidate_asset_textures();
        assert!(
            !renderer.textures.contains_key(path.to_str().unwrap()),
            "invalidate_asset_textures doit vider l'entrée (sauf la blanche par défaut)"
        );
        assert!(
            renderer.textures.contains_key(""),
            "la texture blanche par défaut ne doit pas être jetée"
        );

        // Re-synchroniser recharge bien depuis le disque (le fichier n'a pas
        // changé ici, mais c'est exactement ce que ferait une retouche réelle : le
        // point important est qu'aucun état ne bloque le rechargement après coup).
        renderer.sync_textures(&scene);
        assert!(
            renderer.textures.contains_key(path.to_str().unwrap()),
            "sync_textures doit recharger l'entrée invalidée"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Audit du 16 juillet 2026 : le héros féérique (`assets/models/fairy_hero.glb`),
    /// intégré comme joueur dans une scène MMORPG à 20 créatures skinnées, s'affichait
    /// éclaté en morceaux disjoints — chaque partie du mesh transformée par le squelette
    /// d'une *autre* créature. Cause : au-delà de `MAX_SKINNED_INSTANCES`,
    /// `write_joint_matrices` renvoyait l'offset `0` sans rien écrire, et
    /// `draw_skinned_objects` dessinait quand même l'objet avec cet offset — qui est
    /// *aussi* l'offset légitime de l'objet réellement au slot 0. Ce test construit une
    /// scène avec plus d'objets skinnés que `MAX_SKINNED_INSTANCES` et vérifie que
    /// `prepare_skinned_draws` renvoie `None` (pas un offset aliasé) pour ceux en trop.
    #[test]
    fn skinned_instances_beyond_capacity_get_no_offset_instead_of_aliasing_slot_zero() {
        use crate::scene::import::{Joint, Skeleton};
        use crate::scene::{ImportedMesh, SceneObject};

        let mut renderer = match pollster::block_on(Renderer::new_headless(64, 64)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "skinned_instances_beyond_capacity_get_no_offset_instead_of_aliasing_slot_zero : \
                     pas de GPU headless ({e}) — test sauté."
                );
                return;
            }
        };

        // Un squelette minimal (une racine) suffit : seul le nombre d'instances
        // skinnées visibles compte pour ce bug, pas la richesse du rig.
        let skeleton = Skeleton {
            joints: vec![Joint {
                name: "Root".into(),
                parent: None,
                bind_local: glam::Mat4::IDENTITY,
                inverse_bind: glam::Mat4::IDENTITY,
            }],
        };
        let imported = ImportedMesh {
            skeleton: Some(skeleton),
            ..Default::default()
        };

        // Un objet skinné visible de plus que la capacité — avant le correctif, les
        // instances au-delà de `MAX_SKINNED_INSTANCES` dessinaient avec l'offset 0.
        let n = MAX_SKINNED_INSTANCES + 3;
        let scene = crate::scene::Scene {
            imported: vec![imported],
            objects: (0..n)
                .map(|_| SceneObject {
                    mesh: MeshKind::Imported(0),
                    visible: true,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };

        renderer.draw_plan_skinned = (0..n).map(|i| (i, i as u32)).collect();
        renderer.prepare_skinned_draws(&scene);
        let offsets = &renderer.skinned_offsets_scratch;

        assert_eq!(offsets.len(), n);
        let valid = MAX_SKINNED_INSTANCES;
        for (slot, offset) in offsets.iter().enumerate() {
            if slot < valid {
                assert!(
                    offset.is_some(),
                    "le slot {slot} (dans la capacité de {valid}) doit avoir un offset"
                );
            } else {
                assert!(
                    offset.is_none(),
                    "le slot {slot} dépasse la capacité de {valid} : doit être `None` \
                     (sauté), pas un offset qui aliaserait la palette de joints d'un \
                     autre objet skinné"
                );
            }
        }
        // Les offsets valides doivent tous être distincts (un créneau par objet) —
        // sinon deux objets partageraient la même palette de joints sans même
        // dépasser la capacité, même bug sous une autre forme.
        let mut valid_offsets: Vec<u32> = offsets[..valid].iter().filter_map(|o| *o).collect();
        valid_offsets.sort_unstable();
        valid_offsets.dedup();
        assert_eq!(
            valid_offsets.len(),
            valid,
            "les offsets des objets dans la capacité doivent être tous distincts"
        );

        // Garde-fou visible (audit du 17 juillet 2026) : les 3 objets au-delà de la
        // capacité ne doivent pas rester un simple `log::warn` — le compteur exposé
        // (`skinned_dropped_count`) doit les recenser, prêt à être affiché dans un
        // panneau de stats.
        assert_eq!(
            renderer.skinned_dropped_count(),
            3,
            "les objets skinnés ignorés faute de créneau doivent être comptés"
        );

        // Et il se réinitialise à chaque frame préparée : une frame qui tient dans la
        // capacité doit revenir à 0, pas cumuler les frames passées.
        renderer.draw_plan_skinned = vec![(0, 0)];
        renderer.prepare_skinned_draws(&scene);
        assert_eq!(
            renderer.skinned_dropped_count(),
            0,
            "le compteur d'objets ignorés doit repartir de zéro à chaque frame"
        );
    }

    /// P1 (audit du 17 juillet 2026) : les objets skinnés ne projetaient **aucune**
    /// ombre — la passe d'ombre n'itérait que `draw_plan` (statiques), jamais
    /// `draw_plan_skinned`. Preuve par le rendu : un triangle skinné horizontal
    /// au-dessus d'un sol nu doit créer une zone nettement sombre quelque part sur ce
    /// sol (détectée par balayage, sans dépendre de la position exacte de l'ombre —
    /// la géométrie de la fixture après import/skinning rend le calcul analytique
    /// fragile). Sans le correctif, le sol est un dégradé lisse sans aucune zone
    /// sombre : le test échoue. Passe par `render_scene_headless`, le même chemin que
    /// les golden tests — le correctif y est donc couvert aussi.
    #[test]
    fn skinned_objects_cast_a_shadow_on_the_ground() {
        use crate::scene::{ImportedMesh, SceneObject, import};

        let (width, height) = (160u32, 120u32);
        let mut renderer = match pollster::block_on(Renderer::new_headless(width, height)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "skinned_objects_cast_a_shadow_on_the_ground : pas de GPU ({e}) — sauté."
                );
                return;
            }
        };

        // Mesh skinné minimal : la fixture triangle + squelette de `import::tests`,
        // rechargée par `load_skinning` (même chemin que le vrai import).
        let bytes = import::tests::skinned_triangle_glb();
        let path = import::tests::write_temp_glb(&bytes, "renderer_skinned_shadow");
        let (data, aabb_min, aabb_max) =
            import::load_gltf(path.to_str().unwrap()).expect("glTF de test valide");
        let mut imported = ImportedMesh {
            path: path.to_str().unwrap().to_string(),
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        imported.load_skinning();
        let _ = std::fs::remove_file(&path);
        assert!(imported.skeleton.is_some(), "fixture : squelette attendu");

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            imported: vec![imported],
            objects: vec![
                // Sol : cube aplati et élargi, blanc par défaut.
                SceneObject {
                    mesh: MeshKind::Cube,
                    transform: crate::scene::Transform {
                        scale: glam::Vec3::new(10.0, 0.1, 10.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                // Triangle skinné : couché à l'horizontale 3 m au-dessus du sol,
                // normale vers le bas — sa face **arrière** regarde la lumière, ce que
                // le cull front de la passe d'ombre laisse passer (vérifié : l'autre
                // orientation est cullée, une fixture à un seul triangle n'est pas un
                // volume fermé). Émissif : vu de dessus, sa face avant tournée vers le
                // sol ne reçoit aucune lumière directe et rendrait sa silhouette aussi
                // sombre que l'ombre cherchée — l'émissif la garde brillante sans
                // changer quoi que ce soit à la profondeur écrite dans la carte
                // d'ombre, donc aucun faux positif du balayage ci-dessous.
                SceneObject {
                    mesh: MeshKind::Imported(0),
                    transform: crate::scene::Transform {
                        position: glam::Vec3::new(0.0, 3.0, 0.0),
                        rotation: glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                        scale: glam::Vec3::splat(2.0),
                    },
                    emissive: 2.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        // Lumière inclinée depuis +X (ombre décalée hors de la silhouette des
        // triangles vus de haut), ambiante réduite : l'ombre n'atténue que la part
        // directe, un plancher ambiant fort noierait le contraste recherché.
        app.scene.light.dir = [0.6, 1.0, 0.0];
        app.scene.light.ambient = 0.05;
        // Vue quasi zénithale : sol, triangle et zone d'ombre tous à l'écran.
        app.camera.target = glam::Vec3::ZERO;
        app.camera.distance = 10.0;
        app.camera.pitch = 1.4;
        app.camera.yaw = 0.0;

        let pixels = renderer.render_scene_headless(&mut app, width, height);

        // Projette un point monde en pixel (l'aspect de la caméra vient d'être fixé
        // par `render_scene_headless`).
        let vp = app.camera.view_proj();
        let to_px = |p: glam::Vec3| -> (u32, u32) {
            let clip = vp * p.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            (
                (((ndc.x * 0.5 + 0.5) * width as f32) as u32).min(width - 1),
                (((0.5 - ndc.y * 0.5) * height as f32) as u32).min(height - 1),
            )
        };
        // Luminance moyenne d'un carré 3×3 autour d'un pixel.
        let lum_at = |(cx, cy): (u32, u32)| -> f32 {
            let mut sum = 0.0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let x = (cx as i32 + dx).clamp(0, width as i32 - 1) as u32;
                    let y = (cy as i32 + dy).clamp(0, height as i32 - 1) as u32;
                    let i = ((y * width + x) * 4) as usize;
                    sum += pixels[i] as f32 + pixels[i + 1] as f32 + pixels[i + 2] as f32;
                }
            }
            sum / (9.0 * 3.0)
        };

        // Balayage du sol autour de l'origine : sans ombre skinnée, cette zone est un
        // dégradé lisse et bien éclairé (~150-215 mesurés, seuls le ciel/hors-sol
        // descendent plus bas mais sont hors de la zone balayée) ; l'ombre portée y
        // creuse une plage quasi noire (~2 mesuré, plancher ambiant). Les triangles
        // eux-mêmes, blancs et éclairés par-dessus, restent clairs même s'ils occluent
        // un point balayé — aucun faux positif possible à ce seuil.
        let mut dark_samples = 0;
        for ix in -22..=22 {
            for iz in -22..=22 {
                let p = glam::Vec3::new(ix as f32 * 0.2, 0.05, iz as f32 * 0.2);
                if lum_at(to_px(p)) < 90.0 {
                    dark_samples += 1;
                }
            }
        }
        assert!(
            dark_samples >= 10,
            "aucune zone d'ombre détectée sur le sol ({dark_samples} échantillons sombres) \
             — la passe d'ombre ne dessine probablement pas les objets skinnés"
        );
    }

    /// Même correctif que le test précédent, mais sur la scène embarquée **réelle**
    /// (`assets/player_scene.json`) : 20 créatures skinnées + le joueur
    /// (`fairy_hero.glb`) visibles ensemble, exactement le scénario qui produisait le
    /// héros éclaté en jeu avant le correctif de `MAX_SKINNED_INSTANCES`/
    /// `write_joint_matrices`.
    #[test]
    fn the_embedded_mmorpg_scene_gives_the_player_its_own_joint_offset() {
        let mut renderer = match pollster::block_on(Renderer::new_headless(64, 64)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "the_embedded_mmorpg_scene_gives_the_player_its_own_joint_offset : \
                     pas de GPU headless ({e}) — test sauté."
                );
                return;
            }
        };

        let mut app = AppState::new();
        app.scene = crate::scene::Scene::embedded_player();
        // Vue quasi zénithale englobant toute l'arène (BOUND ±11 dans les scripts de
        // créature) : sans ça, le culling frustum par défaut ne laisserait qu'une
        // poignée de créatures visibles et ce test ne dépasserait jamais l'ancienne
        // capacité de 8, passant même sans le correctif.
        app.camera.target = glam::Vec3::ZERO;
        app.camera.distance = 40.0;
        app.camera.pitch = 1.5;
        app.camera.yaw = 0.0;

        renderer.sync_objects(&app.scene);
        renderer.write_uniforms(&app);
        renderer.prepare_skinned_draws(&app.scene);
        let offsets = &renderer.skinned_offsets_scratch;

        let player_obj_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.tag == "joueur")
            .expect("objet joueur introuvable dans la scène embarquée");

        // Le joueur n'est plus forcément skinné (ex. primitive Sphere) : dans ce cas,
        // il est dessiné par le plan statique batché, pas par `draw_plan_skinned`, et
        // n'a donc pas de palette de joints à protéger — seule sa présence compte.
        if !is_skinned(&app.scene, app.scene.objects[player_obj_idx].mesh) {
            assert!(
                renderer.draw_plan.iter().any(|d| d.obj == player_obj_idx),
                "le joueur non skinné doit apparaître dans le plan de dessin statique"
            );
            return;
        }

        let slot = renderer
            .draw_plan_skinned
            .iter()
            .position(|&(obj_idx, _)| obj_idx == player_obj_idx)
            .expect("le joueur doit être un objet skinné visible du plan de dessin");

        assert!(
            offsets[slot].is_some(),
            "le joueur (slot {slot} sur {} objets skinnés visibles) n'a pas de créneau \
             de joints propre — il se ferait dessiner avec la palette d'une créature",
            renderer.draw_plan_skinned.len()
        );
    }
}
