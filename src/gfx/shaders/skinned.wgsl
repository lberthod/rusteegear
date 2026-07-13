// Vertex shader de skinning GPU (Sprint 86). Fragment stage **partagée** avec
// main.wgsl : ce module ne fournit que `vs_skinned_main`, le pipeline skinné utilise
// `fs_main` de main.wgsl pour l'étage fragment (même éclairage, aucune duplication) —
// wgpu autorise des modules vertex/fragment distincts tant que `VsOut` correspond
// exactement en bindings/types, donc cette struct est recopiée à l'identique ici.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct Model {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    params: vec4<f32>, // x = surbrillance, yzw = metallic/roughness/emissive
    color: vec4<f32>,
};
@group(1) @binding(0) var<storage, read> models: array<Model>;

// Palette de matrices de joints (Sprint 86) : `monde_du_joint * inverse_bind`, déjà
// composées côté CPU (`scene::import::compute_joint_matrices`) — le shader n'a plus qu'à
// mélanger selon les poids, jamais à composer de hiérarchie lui-même.
@group(4) @binding(0) var<storage, read> joint_matrices: array<mat4x4<f32>>;

struct VsIn {
    @builtin(instance_index) instance: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) highlight: f32,
    @location(3) world_pos: vec3<f32>,
    @location(4) uv: vec2<f32>,
    @location(5) material: vec3<f32>,
};

@vertex
fn vs_skinned_main(in: VsIn) -> VsOut {
    // Mélange linéaire des 4 matrices influentes (« linear blend skinning », le schéma
    // standard — pas de dual quaternion ici : plus simple, artefacts de « candy wrapper »
    // aux articulations très pliées, acceptables pour la portée actuelle du moteur).
    let skin = joint_matrices[in.joints.x] * in.weights.x
        + joint_matrices[in.joints.y] * in.weights.y
        + joint_matrices[in.joints.z] * in.weights.z
        + joint_matrices[in.joints.w] * in.weights.w;

    var out: VsOut;
    let model = models[in.instance];
    let skinned_pos = skin * vec4<f32>(in.position, 1.0);
    let world = model.model * skinned_pos;
    out.clip_position = camera.view_proj * world;
    out.world_pos = world.xyz;
    // Normale sous skinning : partie rotation de `skin` appliquée directement (pas
    // d'inverse-transpose du blend complet — coûteux par sommet, et les os d'un rig
    // squelettal ont presque toujours une échelle uniforme, donc l'erreur introduite
    // est négligeable en pratique). Puis la même normal-matrix d'objet que le chemin
    // statique (`model.normal`, déjà l'inverse-transpose du *model* — pas du skin).
    let skin_rot = mat3x3<f32>(skin[0].xyz, skin[1].xyz, skin[2].xyz);
    out.world_normal = (model.normal * vec4<f32>(skin_rot * in.normal, 0.0)).xyz;
    out.color = in.color * model.color.rgb;
    out.highlight = model.params.x;
    out.uv = in.uv;
    out.material = model.params.yzw;
    return out;
}
