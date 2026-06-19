// Shader principal : vertex (MVP) + fragment (Lambert paramétré + ombre portée).

struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct Light {
    dir: vec4<f32>,
    color: vec4<f32>,
    ambient: vec4<f32>,    // x = intensité ambiante
    light_vp: mat4x4<f32>, // view-projection de la lumière (shadow map)
};
@group(0) @binding(1) var<uniform> light: Light;

struct Model {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    params: vec4<f32>, // x = surbrillance
    color: vec4<f32>,  // teinte (albédo)
};
@group(1) @binding(0) var<uniform> model: Model;

@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;

@group(3) @binding(0) var albedo_tex: texture_2d<f32>;
@group(3) @binding(1) var albedo_samp: sampler;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) highlight: f32,
    @location(3) world_pos: vec3<f32>,
    @location(4) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let world = model.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world;
    out.world_normal = (model.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.color = in.color * model.color.rgb;
    out.highlight = model.params.x;
    out.world_pos = world.xyz;
    out.uv = in.uv;
    return out;
}

// Facteur d'ombre [0..1] : 1 = pleinement éclairé, 0 = dans l'ombre.
fn shadow_factor(world_pos: vec3<f32>) -> f32 {
    let lp = light.light_vp * vec4<f32>(world_pos, 1.0);
    let proj = lp.xyz / lp.w;
    // hors de la carte d'ombre → considéré éclairé
    if proj.x < -1.0 || proj.x > 1.0 || proj.y < -1.0 || proj.y > 1.0 || proj.z > 1.0 {
        return 1.0;
    }
    let uv = vec2<f32>(proj.x * 0.5 + 0.5, 0.5 - proj.y * 0.5);
    let bias = 0.003;
    // PCF 3x3 pour adoucir le bord
    var sum = 0.0;
    let texel = 1.0 / 1024.0;
    for (var dx = -1; dx <= 1; dx = dx + 1) {
        for (var dy = -1; dy <= 1; dy = dy + 1) {
            let o = vec2<f32>(f32(dx), f32(dy)) * texel;
            sum = sum + textureSampleCompare(shadow_map, shadow_samp, uv + o, proj.z - bias);
        }
    }
    return sum / 9.0;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let light_dir = normalize(light.dir.xyz);
    let n = normalize(in.world_normal);
    let diffuse = max(dot(n, light_dir), 0.0);
    let shadow = shadow_factor(in.world_pos);
    let intensity = light.ambient.x + diffuse * (1.0 - light.ambient.x) * shadow;
    let tex = textureSample(albedo_tex, albedo_samp, in.uv).rgb;
    var color = in.color * tex * light.color.rgb * intensity;
    // surbrillance jaune additive pour l'objet sélectionné
    color = color + in.highlight * vec3<f32>(0.35, 0.3, 0.0);
    return vec4<f32>(color, 1.0);
}
