use super::*;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(crate) struct GizmoVertex {
    pub(crate) position: [f32; 3],
    pub(crate) color: [f32; 3],
}

impl GizmoVertex {
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
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
pub(crate) struct CameraUniform {
    pub(super) view_proj: [[f32; 4]; 4],
    /// Position de la caméra (xyz), pour le terme spéculaire. w inutilisé.
    pub(super) eye: [f32; 4],
    /// Inverse de `view_proj` : déplie un point NDC du plan lointain en
    /// position monde, pour reconstruire la direction de vue dans `sky.wgsl` sans
    /// dépendre d'un dégradé fixe en espace écran (qui resterait immobile si la
    /// caméra pivote). Inutilisé par les autres shaders (`main.wgsl`/`skinned.wgsl`/
    /// `gizmo.wgsl` ne déclarent qu'un préfixe de cet uniform, WGSL l'autorise).
    pub(super) inv_view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(crate) struct ModelUniform {
    pub(super) model: [[f32; 4]; 4],
    pub(super) normal: [[f32; 4]; 4],
    pub(super) params: [f32; 4], // x = surbrillance (sélection)
    pub(super) color: [f32; 4],  // teinte (albédo) de l'objet
}

/// Une lumière ponctuelle côté GPU (std140 : deux vec4).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(super) struct PointLightU {
    pub(super) pos_range: [f32; 4], // xyz = position, w = portée
    pub(super) color_int: [f32; 4], // rgb = couleur, w = intensité
    pub(super) spot: [f32; 4],      // xyz = direction du cône, w = cos(demi-angle) ou -1 (point)
}

/// Éclairage de la scène (groupe 0, binding 1).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(crate) struct SceneUniform {
    pub(super) light_dir: [f32; 4],
    pub(super) light_color: [f32; 4],
    pub(super) ambient: [f32; 4], // x = intensité ambiante
    pub(super) light_vp: [[f32; 4]; 4],
    pub(super) num_points: [f32; 4], // x = nombre de lumières ponctuelles actives
    pub(super) points: [PointLightU; crate::scene::MAX_POINT_LIGHTS],
    /// Ciel + brouillard : ajoutés en fin de struct pour ne décaler aucun
    /// des offsets existants ci-dessus (moins de risque de désync avec les shaders qui
    /// ne déclarent qu'un préfixe de cet uniform).
    pub(super) sky_horizon: [f32; 4], // rgb, w inutilisé
    pub(super) sky_zenith: [f32; 4], // rgb, w inutilisé
    pub(super) fog: [f32; 4],        // rgb = couleur, w = densité
}

/// Paramètre du bloom (groupe dédié du `tonemap_pipeline`) : juste
/// l'intensité, dans son propre petit uniform plutôt que dans `SceneUniform` — le
/// tone mapping est une passe séparée avec son propre bind group, pas de raison de
/// lui faire porter tout `Light`/`Camera` pour un seul flottant.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub(super) struct BloomUniform {
    pub(super) intensity: [f32; 4], // x = intensité, yzw inutilisés (alignement std140)
}

pub(crate) const SHADOW_SIZE: u32 = 2048;

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Cible de rendu HDR : la scène (ciel, grille, objets, gizmos, debug
/// drawing, skinning) est dessinée dans cette texture intermédiaire — pas directement
/// dans le format d'affichage final — pour que les valeurs > 1 (émissifs, spéculaire
/// fort) restent représentables au lieu d'être écrêtées avant même le tone mapping.
/// `Rgba16Float` : suffisant pour la plage dynamique visée ici (contrairement à
/// `Rgba32Float`, filtrable nativement sans extension GPU supplémentaire).
pub(crate) const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Nombre de niveaux de la chaîne de mips du bloom : mip 0 = moitié de la
/// résolution HDR, chaque niveau suivant moitié du précédent. 4 est un compromis
/// raisonnable — assez pour un halo doux qui s'étend sur plusieurs pixels, sans
/// multiplier les passes plein écran par frame (2×(N-1) + 1 = 7 passes ici).
pub(crate) const BLOOM_MIP_LEVELS: u32 = 4;

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
pub(crate) const MAX_SKINNED_INSTANCES: usize = 256;
/// Taille en octets d'un créneau de la palette de joints — un objet skinné à la fois.
pub(crate) const JOINT_SLOT_BYTES: wgpu::BufferAddress =
    (JOINT_CAPACITY * std::mem::size_of::<[[f32; 4]; 4]>()) as wgpu::BufferAddress;
// Doit rester multiple de 256 (`minStorageBufferOffsetAlignment` WebGPU/wgpu) : le
// binding à offset dynamique de `skinned_model_layout` (pipelines.rs) sélectionne un
// créneau par `offset = slot * JOINT_SLOT_BYTES`.
const _: () = assert!(JOINT_SLOT_BYTES.is_multiple_of(256));

/// Descripteur d'une instance dans le plan de rendu (ordre = index dans le buffer storage).
pub(super) struct InstanceDraw {
    /// Index de l'objet dans `scene.objects` (texture relue au draw, sans clone).
    /// La scène n'est pas mutée entre la construction du plan et les passes de dessin.
    pub(super) obj: usize,
    /// Visible par la caméra (frustum culling) — la passe d'ombre l'ignore.
    pub(super) visible: bool,
    /// Mesh effectif à dessiner : `objs[obj].mesh`, sauf pour le feuillage dense LOD
    /// (Phase D, `foliage_lod_mesh`) au-delà du seuil de distance, où il devient
    /// `MeshKind::Billboard` — précalculé ici (pas relu depuis `scene.objects`, contrairement
    /// à `obj`) car il dépend de la distance caméra, recalculée à chaque frame où le plan est
    /// reconstruit, alors que le champ `mesh` de l'objet en scène reste inchangé.
    pub(super) mesh: MeshKind,
}

/// Nombre de marqueurs temporels écrits par frame (Sprint 112) : un avant chaque
/// passe mesurée plus un final, soit `GPU_PROFILER_MARKS - 1` intervalles nommés
/// dans `GpuProfiler::PASS_NAMES` (ombre / scène / HDR+bloom / UI).
pub(super) const GPU_PROFILER_MARKS: u32 = 5;

/// Timestamp queries GPU par passe (Sprint 112), actives seulement quand le
/// panneau Profiler est ouvert (`Editor::profiler_open`) — `write_timestamp` a un
/// coût réel (synchronisation GPU), pas question de le payer à chaque frame par
/// défaut, même si `Features::TIMESTAMP_QUERY_INSIDE_ENCODERS` est disponible.
/// `None` sur `Renderer` si l'adaptateur ne supporte pas cette feature (dégrade en
/// silence — le profiler FPS/mémoire reste utilisable sans elle).
pub(super) struct GpuProfiler {
    pub(super) query_set: wgpu::QuerySet,
    /// Résultats bruts (ticks GPU) résolus depuis `query_set` — recopiés ici pour
    /// pouvoir mapper `readback_buf` en lecture (`COPY_DST | MAP_READ` ne peut pas
    /// aussi servir de cible de résolution, `QUERY_RESOLVE` l'exige séparé).
    pub(super) resolve_buf: wgpu::Buffer,
    pub(super) readback_buf: wgpu::Buffer,
    /// Durée d'un tick GPU en nanosecondes (`Queue::get_timestamp_period`), fixe
    /// pour la durée de vie du device — convertit les deltas de ticks en ms.
    pub(super) period_ns: f32,
}

impl GpuProfiler {
    /// Noms des `GPU_PROFILER_MARKS - 1` intervalles, dans l'ordre où `render`
    /// écrit les marqueurs correspondants.
    pub(super) const PASS_NAMES: [&'static str; 4] =
        ["Ombres", "Scène", "HDR + Bloom", "UI (egui)"];

    pub(super) fn new(device: &wgpu::Device, period_ns: f32) -> Self {
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
    pub(super) surface: Option<wgpu::Surface<'static>>,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    pub(super) pipeline: wgpu::RenderPipeline,
    /// Fond de ciel, dessiné en premier dans la passe principale.
    pub(super) sky_pipeline: wgpu::RenderPipeline,
    /// Tone mapping HDR → LDR, dessiné après la passe principale.
    pub(super) tonemap_pipeline: wgpu::RenderPipeline,
    pub(super) tonemap_layout: wgpu::BindGroupLayout,
    pub(super) tonemap_sampler: wgpu::Sampler,
    /// Cible HDR de la passe principale en mode fenêtré — redimensionnée
    /// dans `resize()`, comme `depth_view`. Les chemins headless/test créent la leur en
    /// local (taille demandée par l'appelant, indépendante de la fenêtre).
    pub(super) hdr_view: wgpu::TextureView,
    /// Cible multi-échantillonnée de la passe principale, résolue vers `hdr_view` en fin
    /// de passe (`resolve_target`) — `None` si le MSAA est désactivé (qualité « Basse »,
    /// rendu headless, ou adaptateur GPU sans support à `msaa_samples` échantillons),
    /// auquel cas la passe dessine directement dans `hdr_view` comme avant. Redimensionnée
    /// dans `resize()` avec `hdr_view`.
    pub(super) msaa_color_view: Option<wgpu::TextureView>,
    /// Nombre d'échantillons MSAA de la passe principale (1 = désactivé). Fixé une
    /// fois à la création du renderer (`RenderQuality::msaa_samples`, cf. `new_impl`) —
    /// change de qualité en cours de partie ne reconstruit pas les pipelines/cibles,
    /// comme la plupart des moteurs qui exigent un redémarrage pour ce réglage.
    pub(super) msaa_samples: u32,
    /// Chaîne de bloom, cf. `render_bloom` — trois pipelines partageant
    /// `bloom_sample_layout` (seuil, downsample, upsample) et une petite texture à
    /// plusieurs mips en mode fenêtré (`bloom_mip_views`, redimensionnée dans
    /// `resize()` comme `hdr_view`).
    pub(super) bloom_threshold_pipeline: wgpu::RenderPipeline,
    pub(super) bloom_downsample_pipeline: wgpu::RenderPipeline,
    pub(super) bloom_upsample_pipeline: wgpu::RenderPipeline,
    pub(super) bloom_sample_layout: wgpu::BindGroupLayout,
    pub(super) bloom_intensity_buf: wgpu::Buffer,
    pub(super) bloom_mip_views: Vec<wgpu::TextureView>,
    pub(super) depth_view: wgpu::TextureView,
    pub(super) model_layout: wgpu::BindGroupLayout,

    pub(super) camera_buf: wgpu::Buffer,
    pub(super) light_buf: wgpu::Buffer,
    pub(super) camera_bind_group: wgpu::BindGroup,

    pub(super) meshes: HashMap<MeshKind, GpuMesh>,
    pub(super) imported_gpu: Vec<GpuMesh>,
    /// Mesh GPU skinné, aligné avec `imported_gpu`/`Scene::imported` :
    /// `None` pour un import statique (pas de skin), `Some` sinon. Séparé de
    /// `imported_gpu` plutôt qu'un enum : le mesh statique reste disponible même pour un
    /// objet skinné (utile si un jour un LOD non skinné est voulu), et la grande majorité
    /// des entrées n'ont simplement rien ici.
    pub(super) imported_gpu_skinned: Vec<Option<GpuMesh>>,
    /// Données d'instances de tous les objets (groupe 1, storage), indexées par `instance_index`.
    pub(super) models_buf: wgpu::Buffer,
    pub(super) models_bind_group: wgpu::BindGroup,
    pub(super) models_capacity: usize,
    /// Plan de rendu de la frame : un descripteur par objet, dans l'ordre du buffer d'instances.
    pub(super) draw_plan: Vec<InstanceDraw>,
    /// Objets skinnés : (indice scène, instance_index dans `models_buf`),
    /// hors du batching de `draw_plan` (chaque objet a sa propre palette de joints,
    /// dessiné individuellement par `draw_skinned_objects`). Leurs `ModelUniform` occupent
    /// la queue de `models_buf`, après les objets statiques de `draw_plan`.
    pub(super) draw_plan_skinned: Vec<(usize, u32)>,
    /// Tampons réutilisés chaque frame (évite deux allocations par frame).
    pub(super) order_scratch: Vec<usize>,
    pub(super) models_scratch: Vec<ModelUniform>,
    /// Nombre d'objets au dernier tri de `order_scratch` (re-tri paresseux).
    pub(super) last_sort_len: usize,
    /// Hash des entrées de rendu (objets + caméra) à la dernière reconstruction du plan
    /// de dessin : si inchangé, on saute le rebuild (skip au repos, sûr par construction).
    pub(super) last_render_hash: u64,

    pub(super) gizmo_pipeline: wgpu::RenderPipeline,
    pub(super) gizmo_vbuf: wgpu::Buffer,

    // --- debug drawing : mêmes pipeline/format que les gizmos, buffer
    //     séparé et redimensionnable (le nombre de segments n'est pas borné à l'avance,
    //     contrairement aux gizmos de manipulation). Vidé (`AppState::debug_lines`)
    //     après chaque frame de rendu.
    pub(super) debug_vbuf: wgpu::Buffer,
    pub(super) debug_capacity: usize,

    // --- grille de référence au sol (depth-testée, dans la passe principale) ---
    pub(super) grid_pipeline: wgpu::RenderPipeline,
    pub(super) grid_vbuf: wgpu::Buffer,
    pub(super) grid_count: u32,

    // --- ombres (shadow mapping) ---
    pub(super) shadow_view: wgpu::TextureView,
    pub(super) shadow_bind_group: wgpu::BindGroup,
    pub(super) shadow_pipeline: wgpu::RenderPipeline,

    // --- textures (groupe 3) ---
    pub(super) tex_layout: wgpu::BindGroupLayout,
    pub(super) tex_sampler: wgpu::Sampler,
    /// Bind groups de texture par chemin ; "" = texture blanche par défaut.
    pub(super) textures: HashMap<String, wgpu::BindGroup>,
    /// Génération de mipmaps à l'import : pipeline/layout/sampler dédiés,
    /// utilisés par `make_texture` pour chaque niveau au-delà du mip 0.
    pub(super) mipgen_pipeline: wgpu::RenderPipeline,
    pub(super) mipgen_layout: wgpu::BindGroupLayout,
    pub(super) mipgen_sampler: wgpu::Sampler,

    pub(super) editor: Option<Editor>,
    /// Nom du backend GPU réel (Metal / Vulkan / …), pour le bandeau d'état.
    pub(super) backend: String,

    // --- skinning GPU : palette de matrices de joints + pipeline dédié
    //     (vertex `skinned.wgsl`, fragment `fs_main` de main.wgsl **partagée**, même
    //     éclairage que le chemin statique). Dessine les objets skinnés de la scène
    //     (`render`/`render_scene_headless`, via `draw_plan_skinned`) ; `render_skinned_test`
    //     couvre en plus un chemin headless dédié à un seul mesh, hors scène.
    //     Groupe 1 dédié (`skinned_model_layout`) : `models` + joints fusionnés, pour
    //     tenir dans la limite WebGPU de 4 bind groups (cf. `pipelines.rs::build`).
    pub(super) skinned_pipeline: wgpu::RenderPipeline,
    /// Passe d'ombre des objets skinnés (audit du 17 juillet 2026) : profondeur seule
    /// depuis la lumière, vertex de skinning (`vs_skinned_shadow`) — sans lui, aucun
    /// objet skinné ne projetait d'ombre (la passe d'ombre n'itérait que `draw_plan`).
    pub(super) skinned_shadow_pipeline: wgpu::RenderPipeline,
    pub(super) skinned_model_layout: wgpu::BindGroupLayout,
    pub(super) joint_buf: wgpu::Buffer,
    /// Référence `models_buf` + `joint_buf` : à recréer si l'un des deux l'est
    /// (cf. `sync_objects`, seul site où `models_buf` change de capacité).
    pub(super) skinned_models_bind_group: wgpu::BindGroup,
    pub(super) joint_capacity: usize,
    // --- Tampons réutilisés du chemin skinning (audit perf, juillet 2026) : comme
    //     `order_scratch`/`models_scratch` pour le chemin statique, zéro allocation
    //     par frame une fois les capacités atteintes. ---
    /// Offsets dynamiques de la frame, dans l'ordre de `draw_plan_skinned`
    /// (rempli par `prepare_skinned_draws`, lu par `draw_skinned_objects`/
    /// `draw_skinned_shadows`).
    pub(super) skinned_offsets_scratch: Vec<Option<u32>>,
    /// Palette de joints d'**un** objet, recalculée par objet dans la boucle de
    /// `prepare_skinned_draws`.
    pub(super) joint_matrices_scratch: Vec<glam::Mat4>,
    /// Conversion `Mat4` → `[[f32; 4]; 4]` avant `write_buffer` (cf. `write_joint_matrices`).
    pub(super) joint_raw_scratch: Vec<[[f32; 4]; 4]>,
    /// Tampons internes de `compute_joint_matrices_into` (résolution par vagues).
    pub(super) skinning_scratch: crate::scene::import::SkinningScratch,
    /// Nombre d'objets skinnés **ignorés** (pas dessinés du tout) à la dernière frame
    /// faute de créneau (`slot >= MAX_SKINNED_INSTANCES`) — garde-fou visible du
    /// dépassement silencieux de capacité, exposé par `skinned_dropped_count`.
    pub(super) skinned_dropped_last_frame: u32,
    /// Chemins de texture dont le chargement a échoué : mémorisés pour ne pas
    /// réessayer (et re-logger) à chaque frame — le dessin retombe sur la texture
    /// blanche `""` déjà en cache via le repli des sites de draw, sans en recréer une.
    /// Vidé par `invalidate_asset_textures` (hot-reload : un fichier réparé redevient
    /// chargeable).
    pub(super) failed_textures: std::collections::HashSet<String>,

    // --- profiler GPU (Sprint 112) ---
    /// `None` si l'adaptateur ne supporte pas `TIMESTAMP_QUERY_INSIDE_ENCODERS`.
    pub(super) gpu_profiler: Option<GpuProfiler>,
    /// Durée (ms) de chaque passe mesurée à la dernière lecture réussie — vide tant
    /// qu'aucune frame n'a été profilée (panneau jamais ouvert, ou pas de support GPU).
    pub(super) gpu_pass_timings_ms: Vec<(&'static str, f32)>,
    /// Estimation du nombre de draw calls de la dernière frame (scène + ombre,
    /// cf. `Renderer::render`) — dérivée de `draw_plan`/`draw_plan_skinned`, pas
    /// comptée sur chaque site d'appel réel (bloom/tonemap/UI ajoutent quelques
    /// draws fixes non comptés ici, negligibles face au coût de la scène).
    pub(super) last_frame_draw_calls: u32,
}
