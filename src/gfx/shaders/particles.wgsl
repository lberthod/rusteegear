// Particules (Sprint 132) : quads toujours face caméra, dépliés entièrement
// dans le vertex shader depuis un tampon d'instances (aucune géométrie CPU
// par particule) à partir de `camera.cam_right`/`cam_up` — la base orthonormée
// de la caméra, calculée une fois par frame côté CPU (`write_uniforms`,
// même idiome que `OrbitCamera::pan`). Non éclairées (pas de PBR ici, juste
// une teinte + un halo doux au bord du disque) : suffisant pour des
// étincelles/traînées/fumée, et beaucoup plus simple qu'une passe PBR pour
// un quad qui n'a pas de normale géométrique stable.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct Particle {
    center: vec3<f32>,
    size: f32,
    color: vec4<f32>,
};
@group(1) @binding(0) var<storage, read> particles: array<Particle>;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

// Deux triangles (6 sommets) couvrant un quad [-1,1]² en espace local —
// indexés par `vertex_index % 6`, aucun vertex/index buffer nécessaire.
const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VsOut {
    let p = particles[iid];
    let corner = CORNERS[vid % 6u];
    let half_size = max(p.size, 0.001) * 0.5;
    let world_pos = p.center
        + camera.cam_right.xyz * corner.x * half_size
        + camera.cam_up.xyz * corner.y * half_size;
    var out: VsOut;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.color = p.color;
    out.uv = corner * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Disque doux plutôt qu'un carré dur : `smoothstep` sur la distance au
    // centre en UV, sans texture (une seule teinte par particule suffit pour
    // étincelles/poussière/fumée — pas de flip-book ici, hors scope du sprint).
    let d = length(in.uv * 2.0 - vec2<f32>(1.0, 1.0));
    let falloff = smoothstep(1.0, 0.55, d);
    let alpha = in.color.a * falloff;
    if alpha <= 0.001 {
        discard;
    }
    return vec4<f32>(in.color.rgb, alpha);
}
