// Génération de mipmaps à l'import (Sprint 92) : blit plein écran d'un niveau vers le
// suivant (moitié résolution), par un simple échantillonnage bilinéaire — même
// technique de triangle plein écran que `sky.wgsl`/`bloom.wgsl`, dans un module séparé
// pour ne pas coupler la génération de mips (une fois par texture, à l'import) au
// pipeline de bloom (par frame, format HDR différent).

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_samp: sampler;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_mipgen(@builtin(vertex_index) i: u32) -> VsOut {
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

@fragment
fn fs_mipgen(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(src_tex, src_samp, in.uv);
}
