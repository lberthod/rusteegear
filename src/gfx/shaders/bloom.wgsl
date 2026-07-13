// Bloom (Sprint 91) : halo lumineux autour des zones très brillantes (émissifs,
// spéculaire HDR). Chaîne de mips down/upsample (dual filtering), même triangle plein
// écran que `sky.wgsl`/`tonemap.wgsl` pour chaque passe — un seul filtre bilinéaire par
// passe (`fs_sample`), le lissage vient du sampler linéaire, pas d'un noyau explicite :
// suffisant pour un halo doux, bien moins coûteux qu'un flou gaussien multi-tap.

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_samp: sampler;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_bloom(@builtin(vertex_index) i: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    out.clip_position = vec4<f32>(pos[i], 0.0, 1.0);
    out.uv = vec2<f32>(pos[i].x * 0.5 + 0.5, 1.0 - (pos[i].y * 0.5 + 0.5));
    return out;
}

// Seuil de luminance (Sprint 91) : seules les zones dont un canal dépasse 1.0 (déjà
// hors gamme LDR, donc « brillantes » par construction dans notre pipeline HDR)
// contribuent au bloom — un objet correctement exposé n'en gagne aucun.
const BLOOM_THRESHOLD: f32 = 1.0;

@fragment
fn fs_threshold(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(src_tex, src_samp, in.uv).rgb;
    let bright = max(c - vec3<f32>(BLOOM_THRESHOLD), vec3<f32>(0.0));
    return vec4<f32>(bright, 1.0);
}

// Utilisée à la fois pour descendre (cible plus petite que la source) et remonter la
// chaîne (cible plus grande) : le filtrage bilinéaire du sampler fait le travail dans
// les deux sens, seule la taille de la cible liée par l'appelant change de sens.
@fragment
fn fs_sample(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(src_tex, src_samp, in.uv).rgb, 1.0);
}
