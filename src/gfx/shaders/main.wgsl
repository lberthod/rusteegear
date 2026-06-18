// Shader principal : vertex (MVP) + fragment (éclairage Lambert simple).

struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct Model {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    // x = facteur de surbrillance (objet sélectionné), reste réservé.
    params: vec4<f32>,
};
@group(1) @binding(0) var<uniform> model: Model;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) highlight: f32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let world = model.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world;
    out.world_normal = (model.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.color = in.color;
    out.highlight = model.params.x;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let n = normalize(in.world_normal);
    let diffuse = max(dot(n, light_dir), 0.0);
    let ambient = 0.25;
    let intensity = ambient + diffuse * 0.75;
    var color = in.color * intensity;
    // surbrillance jaune additive pour l'objet sélectionné
    color = color + in.highlight * vec3<f32>(0.35, 0.3, 0.0);
    return vec4<f32>(color, 1.0);
}
