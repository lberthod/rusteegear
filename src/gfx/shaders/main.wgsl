// Shader principal : vertex (MVP) + fragment (éclairage Lambert paramétré).

struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct Light {
    dir: vec4<f32>,
    color: vec4<f32>,
    ambient: vec4<f32>, // x = intensité ambiante
};
@group(0) @binding(1) var<uniform> light: Light;

struct Model {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    // x = facteur de surbrillance (objet sélectionné), reste réservé.
    params: vec4<f32>,
    color: vec4<f32>, // teinte (albédo)
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
    // teinte par objet appliquée à la couleur du sommet
    out.color = in.color * model.color.rgb;
    out.highlight = model.params.x;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let light_dir = normalize(light.dir.xyz);
    let n = normalize(in.world_normal);
    let diffuse = max(dot(n, light_dir), 0.0);
    let intensity = light.ambient.x + diffuse * (1.0 - light.ambient.x);
    var color = in.color * light.color.rgb * intensity;
    // surbrillance jaune additive pour l'objet sélectionné
    color = color + in.highlight * vec3<f32>(0.35, 0.3, 0.0);
    return vec4<f32>(color, 1.0);
}
