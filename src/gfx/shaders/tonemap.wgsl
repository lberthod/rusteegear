// Tone mapping (Sprint 90) + composition du bloom (Sprint 91) : passe plein écran
// (triangle sans vertex buffer, même technique que `sky.wgsl`) qui lit la cible HDR
// (`Rgba16Float`, remplie par la passe principale — ciel, objets, gizmos, debug
// drawing, skinning), y ajoute le halo calculé par `bloom.wgsl` (déjà remonté à pleine
// résolution par le filtrage bilinéaire du sampler, cf. `Renderer::render_bloom`), puis
// écrit le résultat dans le format d'affichage final. Sans le tone mapping, toute valeur
// > 1 (émissifs, spéculaire fort) serait purement écrêtée au lieu de rouler en douceur
// vers le blanc.

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var hdr_samp: sampler;
@group(0) @binding(2) var bloom_tex: texture_2d<f32>;
struct BloomParams {
    // x = intensité (0 = désactivé, cf. `RenderQuality::bloom_enabled` — l'opt-out
    // mobile met cette valeur à 0 sans changer le shader). y = 1.0 si la surface
    // réelle n'a **pas** de format sRGB (ex. wasm32/WebGPU, Sprint 114) : le shader
    // doit alors encoder le gamma lui-même, sinon fait automatiquement par une vue
    // sRGB côté natif — cf. le commentaire de `Renderer::tonemap`. zw inutilisés.
    intensity: vec4<f32>,
};

// Encodage OETF sRGB standard (IEC 61966-2-1), appliqué canal par canal —
// utilisé seulement quand `bloom.intensity.y > 0.5` (cf. plus haut).
fn srgb_encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}
@group(0) @binding(3) var<uniform> bloom: BloomParams;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_tonemap(@builtin(vertex_index) i: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    out.clip_position = vec4<f32>(pos[i], 0.0, 1.0);
    // Origine texture en haut-gauche : y inversé par rapport au NDC (bas = -1).
    out.uv = vec2<f32>(pos[i].x * 0.5 + 0.5, 1.0 - (pos[i].y * 0.5 + 0.5));
    return out;
}

// Approximation ACES filmique (Narkowicz 2015) : un seul canal fragment, pas de LUT —
// bon compromis coût/qualité pour un moteur mobile-friendly. Roule en douceur vers le
// blanc au lieu d'écrêter, garde le contraste dans les tons moyens.
fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_tonemap(in: VsOut) -> @location(0) vec4<f32> {
    let hdr = textureSample(hdr_tex, hdr_samp, in.uv).rgb;
    let bloom_col = textureSample(bloom_tex, hdr_samp, in.uv).rgb;
    let combined = hdr + bloom_col * bloom.intensity.x;
    var mapped = aces_tonemap(combined);
    if bloom.intensity.y > 0.5 {
        mapped = srgb_encode(mapped);
    }
    return vec4<f32>(mapped, 1.0);
}
