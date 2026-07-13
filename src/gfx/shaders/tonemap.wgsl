// Tone mapping (Sprint 90) : passe plein écran (triangle sans vertex buffer, même
// technique que `sky.wgsl`) qui lit la cible HDR (`Rgba16Float`, remplie par la passe
// principale — ciel, objets, gizmos, debug drawing, skinning) et écrit le résultat dans
// le format d'affichage final. Sans cette passe, toute valeur > 1 (émissifs, spéculaire
// fort) serait purement écrêtée au lieu de rouler en douceur vers le blanc.

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var hdr_samp: sampler;

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
    return vec4<f32>(aces_tonemap(hdr), 1.0);
}
