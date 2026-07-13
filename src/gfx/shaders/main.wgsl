// Shader principal : vertex (MVP) + fragment (Lambert paramétré + ombre portée).

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>, // position caméra (xyz) pour le spéculaire
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct PointLight {
    pos_range: vec4<f32>, // xyz = position, w = portée
    color_int: vec4<f32>, // rgb = couleur, w = intensité
    spot: vec4<f32>,      // xyz = direction du cône, w = cos(demi-angle) ou -1 (point)
};

struct Light {
    dir: vec4<f32>,
    color: vec4<f32>,
    ambient: vec4<f32>,    // x = intensité ambiante
    light_vp: mat4x4<f32>, // view-projection de la lumière (shadow map)
    num_points: vec4<f32>, // x = nombre de lumières ponctuelles
    points: array<PointLight, 8>,
};
@group(0) @binding(1) var<uniform> light: Light;

struct Model {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    params: vec4<f32>, // x = surbrillance, yzw = metallic/roughness/emissive
    color: vec4<f32>,  // teinte (albédo)
};
// Tableau d'instances : indexé par @builtin(instance_index).
@group(1) @binding(0) var<storage, read> models: array<Model>;

@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;

@group(3) @binding(0) var albedo_tex: texture_2d<f32>;
@group(3) @binding(1) var albedo_samp: sampler;

struct VsIn {
    @builtin(instance_index) instance: u32,
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
    @location(5) material: vec3<f32>, // metallic, roughness, emissive
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let model = models[in.instance];
    let world = model.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world;
    out.world_normal = (model.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.color = in.color * model.color.rgb;
    out.highlight = model.params.x;
    out.world_pos = world.xyz;
    out.uv = in.uv;
    out.material = model.params.yzw;
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
    let n = normalize(in.world_normal);

    // Vue de debug (Sprint 83) : remplace l'éclairage par une lecture directe d'une
    // grandeur du pipeline. Encodée dans light.ambient.y (0 = éclairé, 1 = normales,
    // 2 = profondeur) — cf. `AppState::DebugView` / `write_uniforms` côté Rust.
    let debug_view = light.ambient.y;
    if debug_view > 1.5 {
        // Profondeur linéarisée (near = celui de OrbitCamera::view_proj, 0.1) : la
        // profondeur NDC brute écrase presque toute la scène près de 1.0 (non-linéaire),
        // illisible en debug. Normalisée sur `debug_far` (échelle visuelle des niveaux
        // RusteeGear — compacts, mobile — pas le plan lointain réel de la caméra, 100 :
        // avec 100, la quasi-totalité d'une scène typique resterait dans le même blanc
        // écrasé). 1.0 = proche (blanc), 0.0 = à `debug_far` ou au-delà (noir).
        let near = 0.1;
        let real_far = 100.0;
        let debug_far = 20.0;
        let z_ndc = in.clip_position.z;
        let z_view = (near * real_far) / (real_far - z_ndc * (real_far - near));
        let d = 1.0 - clamp((z_view - near) / (debug_far - near), 0.0, 1.0);
        return vec4<f32>(d, d, d, 1.0);
    }
    if debug_view > 0.5 {
        let n_color = n * 0.5 + vec3<f32>(0.5, 0.5, 0.5);
        return vec4<f32>(n_color, 1.0);
    }

    let light_dir = normalize(light.dir.xyz);
    let diffuse = max(dot(n, light_dir), 0.0);
    let shadow = shadow_factor(in.world_pos);
    let tex = textureSample(albedo_tex, albedo_samp, in.uv).rgb;
    let albedo = in.color * tex;

    let metallic = clamp(in.material.x, 0.0, 1.0);
    let roughness = clamp(in.material.y, 0.04, 1.0);
    let emissive = in.material.z;

    // Diffuse : atténuée pour les métaux (qui réfléchissent au lieu de diffuser).
    let kd = 1.0 - metallic;
    let lit = light.ambient.x + diffuse * (1.0 - light.ambient.x) * shadow;
    var color = albedo * kd * light.color.rgb * lit;

    // Spéculaire Blinn-Phong (puissance pilotée par la rugosité ; teinte = blanc
    // pour un diélectrique, albédo pour un métal). Approximation PBR légère, mobile-friendly.
    let v = normalize(camera.eye.xyz - in.world_pos);
    let h = normalize(light_dir + v);
    let spec_power = mix(8.0, 256.0, 1.0 - roughness);
    let spec = pow(max(dot(n, h), 0.0), spec_power) * diffuse * shadow * (1.0 - roughness);
    let spec_col = mix(vec3<f32>(1.0), albedo, metallic);
    color = color + spec * spec_col * light.color.rgb;

    // Lumières ponctuelles : diffus + spéculaire avec atténuation quadratique douce.
    let count = i32(light.num_points.x);
    for (var p = 0; p < count; p = p + 1) {
        let pl = light.points[p];
        let to_light = pl.pos_range.xyz - in.world_pos;
        let dist = length(to_light);
        let ld = to_light / max(dist, 0.001);
        // atténuation : 1 au centre, 0 au-delà de la portée (clamp lissé).
        let att = clamp(1.0 - dist / pl.pos_range.w, 0.0, 1.0);
        // Cône (spot) : atténuation douce du bord ; w < 0 → lumière ponctuelle (cône = 1).
        var cone = 1.0;
        if pl.spot.w >= 0.0 {
            let aligned = dot(-ld, normalize(pl.spot.xyz));
            cone = smoothstep(pl.spot.w, mix(pl.spot.w, 1.0, 0.5), aligned);
        }
        let atten = att * att * pl.color_int.w * cone;
        let d = max(dot(n, ld), 0.0);
        let ph = normalize(ld + v);
        let s = pow(max(dot(n, ph), 0.0), spec_power) * d * (1.0 - roughness);
        color = color + pl.color_int.rgb * atten * (albedo * kd * d + spec_col * s);
    }

    // Émission (l'objet brille de sa propre couleur) + surbrillance de sélection.
    color = color + albedo * emissive;
    color = color + in.highlight * vec3<f32>(0.35, 0.3, 0.0);
    return vec4<f32>(color, 1.0);
}
