// Ciel (Sprint 89) : dégradé horizon/zénith dessiné en premier, sans écriture de
// profondeur, derrière toute la géométrie de la scène. Triangle plein écran généré par
// l'indice de sommet (pas de vertex buffer) — technique standard pour un fond qui ne
// dépend que de la direction de vue, pas de la géométrie.
//
// Le dégradé suit la direction de vue reconstruite via `camera.inv_view_proj` plutôt
// qu'un dégradé fixe en espace écran : sinon le ciel resterait immobile pendant qu'on
// oriente la caméra (orbite éditeur ou joueur), un défaut visible immédiatement.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct PointLight {
    pos_range: vec4<f32>,
    color_int: vec4<f32>,
    spot: vec4<f32>,
};

// Doit correspondre exactement à `Light` dans `main.wgsl` (même buffer, groupe 0
// binding 1) : seuls `sky_horizon`/`sky_zenith` nous intéressent ici, mais WGSL exige
// de retrouver leur offset réel, donc tous les champs qui les précèdent sont recopiés.
struct Light {
    dir: vec4<f32>,
    color: vec4<f32>,
    ambient: vec4<f32>,
    light_vp: mat4x4<f32>,
    num_points: vec4<f32>,
    points: array<PointLight, 8>,
    sky_horizon: vec4<f32>,
    sky_zenith: vec4<f32>,
    fog: vec4<f32>,
};
@group(0) @binding(1) var<uniform> light: Light;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> VsOut {
    // Triangle qui déborde largement de l'écran des deux côtés : le rasteriseur ne
    // garde que la partie visible, moins de travail qu'un quad à deux triangles.
    var ndc = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    // z = 1 (plan lointain) : derrière toute géométrie réelle (profondeur NDC ∈ [0, 1]
    // pour ce moteur, cf. `camera.rs`), sans jamais l'emporter au depth test.
    out.clip_position = vec4<f32>(ndc[i], 1.0, 1.0);
    out.ndc = ndc[i];
    return out;
}

@fragment
fn fs_sky(in: VsOut) -> @location(0) vec4<f32> {
    let clip_far = vec4<f32>(in.ndc, 1.0, 1.0);
    let world_far = camera.inv_view_proj * clip_far;
    let world_far_pos = world_far.xyz / world_far.w;
    let dir = normalize(world_far_pos - camera.eye.xyz);
    // 0 = horizon, 1 = zénith. `smoothstep` plutôt qu'un `mix` linéaire sur `dir.y` brut :
    // évite une bande dure au ras de l'horizon, garde un ciel presque uniforme au zénith.
    let t = smoothstep(-0.05, 0.6, dir.y);
    let col = mix(light.sky_horizon.rgb, light.sky_zenith.rgb, t);
    return vec4<f32>(col, 1.0);
}
