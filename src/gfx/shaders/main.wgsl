// Shader principal : vertex (MVP) + fragment (Lambert paramétré + ombre portée).

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>, // position caméra (xyz) pour le spéculaire
    // Inverse de `view_proj` (Sprint 89), utilisé par `sky.wgsl` seul — WGSL permet à
    // un shader de ne déclarer qu'un préfixe du buffer réel (cf. `gizmo.wgsl`, qui ne
    // déclare même pas `eye`) ; recopié ici surtout pour la lisibilité du layout réel.
    inv_view_proj: mat4x4<f32>,
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
    // Ciel + brouillard (Sprint 89). `sky_horizon`/`sky_zenith` sont lus par `sky.wgsl`
    // (fond de scène), pas ici : recopiés pour que l'offset de `fog` reste correct.
    sky_horizon: vec4<f32>,
    sky_zenith: vec4<f32>,
    fog: vec4<f32>, // rgb = couleur, w = densité (0 = désactivé)
    // Ombres en cascade (analyse comparative 2026-09-04, court terme) : une
    // view-projection de lumière par cascade, ajustée sur une tranche du frustum
    // caméra (proche = serrée = nette, lointaine = large = floue mais couvrante).
    // `light_vp` ci-dessus reste la cascade 0 pour les shaders qui ne déclarent
    // que ce préfixe (shadow.wgsl, skinned.wgsl : passe d'ombre).
    cascade_vp: array<mat4x4<f32>, 3>,
    // xyz = distance caméra (m) au-delà de laquelle on passe à la cascade suivante
    // (x : 0→1, y : 1→2, z : fin de la 2 = plus d'ombre) ; w = 1 / taille texel.
    cascade_splits: vec4<f32>,
    // Extensions (démo Rivière & cascade), neutres à 0 — cf. `SceneUniform::extra`.
    extra: vec4<f32>,  // x = base brouillard de hauteur, y = décroissance, z = halo soleil, w = plan de réflexion
    extra2: vec4<f32>, // viewport de la passe principale (x, y, w, h) dans la cible
    extra3: vec4<f32>, // x = texture de réflexion liée, y = passe de réflexion (clip sous le plan)
};

// Brouillard exponentiel (Sprint 89), optionnellement **de hauteur** (`extra.y > 0`) :
// la densité est celle de `fog.w` à l'altitude `extra.x`, décroît au-dessus et
// épaissit en dessous (borné) — évaluée à mi-chemin entre l'œil et le fragment,
// approximation suffisante pour une vallée embrumée. `density = 0` → 0.
fn fog_amount_at(world_pos: vec3<f32>) -> f32 {
    let fog_dist = length(camera.eye.xyz - world_pos);
    var density = light.fog.w;
    if light.extra.y > 0.0 {
        let h_mid = 0.5 * (camera.eye.y + world_pos.y);
        density = density * clamp(exp(-(h_mid - light.extra.x) * light.extra.y), 0.0, 3.0);
    }
    return clamp(1.0 - exp(-fog_dist * density), 0.0, 1.0);
}
@group(0) @binding(1) var<uniform> light: Light;

struct Model {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    params: vec4<f32>, // x = surbrillance, yzw = metallic/roughness/emissive
    color: vec4<f32>,  // teinte (albédo)
    water: vec4<f32>,  // surface d'eau : x = genre (0 = non), y = vitesse, z = échelle, w = écume
};
// Tableau d'instances : indexé par @builtin(instance_index).
@group(1) @binding(0) var<storage, read> models: array<Model>;

@group(2) @binding(0) var shadow_map: texture_depth_2d_array;
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
    @location(6) alpha: f32,          // opacité (passe transparente ; 1 en opaque)
    @location(7) water: vec4<f32>,    // cf. Model.water (constant sur l'instance)
    @location(8) vcolor: vec3<f32>,   // couleur de sommet brute (masques du shader d'eau)
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let model = models[in.instance];
    let world = model.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world;
    out.world_normal = (model.normal * vec4<f32>(in.normal, 0.0)).xyz;
    // Surface d'eau : la couleur de sommet est un masque (écume/profondeur), pas
    // une teinte — `color` ne porte alors que la teinte de l'objet, le masque
    // voyage à part dans `vcolor`.
    out.color = select(in.color * model.color.rgb, model.color.rgb, model.water.x > 0.5);
    out.highlight = model.params.x;
    out.world_pos = world.xyz;
    out.uv = in.uv;
    out.material = model.params.yzw;
    out.alpha = model.color.a;
    out.water = model.water;
    out.vcolor = in.color;
    return out;
}

// ---------------------------------------------------------------------------
// Eau (`SceneObject::water`, démo « Rivière & cascade ») — entièrement
// procédurale, sans texture ni carte de normales : bruit de valeur animé
// dérivé en normale, Fresnel de Schlick entre le corps d'eau (teinte ×
// profondeur) et le reflet du ciel (dégradé horizon/zénith de la scène,
// berges sombres sous l'horizon), éclats du soleil, écume seuillée là où le
// masque de sommet (R) l'autorise. Trois genres (`water.x`) : 1 rivière/lac
// (UV en mètres, courant vers +u), 2 cascade (traînées étirées le long de v),
// 3 brume (nuage d'opacité fondu vers les bords d'un plan/impostor).
// ---------------------------------------------------------------------------
fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(0.1031, 0.1030));
    q = q + dot(q, q.yx + vec2<f32>(33.33, 33.33));
    return fract((q.x + q.y) * q.x);
}

// Bruit de valeur lissé (Hermite), continu et dérivable par différence finie.
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Hauteur de vaguelettes : quatre octaves qui dérivent le long de +x (sens du
// courant) à des vitesses croissantes, avec un léger clapot croisé en y.
fn wave_height(p: vec2<f32>, t: f32) -> f32 {
    var h = vnoise(p * 1.0 + vec2<f32>(-t * 0.9, t * 0.15)) * 0.5;
    h = h + vnoise(p * 2.3 + vec2<f32>(-t * 1.6, -t * 0.3)) * 0.25;
    h = h + vnoise(p * 5.1 + vec2<f32>(-t * 2.4, t * 0.9)) * 0.125;
    h = h + vnoise(p * 11.0 + vec2<f32>(-t * 3.5, -t * 1.2)) * 0.0625;
    return h;
}

// `dp1/dp2/duv1/duv2` : dérivées écran de la position monde et des UV, calculées
// par l'appelant en flux de contrôle **uniforme** (exigence WebGPU pour
// `dpdx`/`dpdy`, comme pour `textureSample` — cf. `shadow_factor`).
fn shade_water(
    in: VsOut,
    n_geo: vec3<f32>,
    front_facing: bool,
    dp1: vec3<f32>,
    dp2: vec3<f32>,
    duv1: vec2<f32>,
    duv2: vec2<f32>,
    light_dir: vec3<f32>,
    shadow: f32,
) -> vec4<f32> {
    let mode = in.water.x;
    let t = camera.eye.w * in.water.y;
    let scale = in.water.z;
    let foam_amt = in.water.w;
    let v = normalize(camera.eye.xyz - in.world_pos);
    let n_face = select(-n_geo, n_geo, front_facing);
    let fog_amount = fog_amount_at(in.world_pos);
    let ambient = mix(light.sky_horizon.rgb, light.sky_zenith.rgb, 0.7) * light.ambient.x;
    let sun = light.color.rgb * (1.0 - light.ambient.x);

    if mode > 2.5 {
        // Brume : opacité = bruit animé × fondu radial (UV 0..1 du plan/impostor).
        let c = in.uv - vec2<f32>(0.5, 0.5);
        let r2 = dot(c, c) * 4.0;
        let nz = vnoise(in.uv * 3.0 * scale + vec2<f32>(t * 0.2, -t * 0.6)) * 0.6
            + vnoise(in.uv * 7.0 * scale + vec2<f32>(-t * 0.35, -t * 1.3)) * 0.4;
        let a = in.alpha * (1.0 - smoothstep(0.15, 1.0, r2)) * smoothstep(0.3, 0.8, nz);
        var col = in.color * (ambient * 1.4 + sun * 0.5 * (0.5 + 0.5 * shadow));
        col = mix(col, light.fog.rgb, fog_amount);
        return vec4<f32>(col, a);
    }

    // Repère tangent depuis les dérivées écran (« cotangent frame », Schüler) :
    // aucune tangente par sommet nécessaire, marche sur tout maillage UV-mappé.
    let dp2perp = cross(dp2, n_face);
    let dp1perp = cross(n_face, dp1);
    let tt = dp2perp * duv1.x + dp1perp * duv2.x;
    let bb = dp2perp * duv1.y + dp1perp * duv2.y;
    let invmax = inverseSqrt(max(dot(tt, tt), dot(bb, bb)));
    let tbn = mat3x3<f32>(tt * invmax, bb * invmax, n_face);

    var p = in.uv * scale;
    var k = 0.55;
    if mode > 1.5 {
        // Cascade : traînées étirées le long de la chute (v) et défilement vers +v —
        // le bruit défile sur son axe x, on lui présente donc (v, u).
        p = vec2<f32>(in.uv.y * scale * 0.35, in.uv.x * scale * 2.5);
        k = 0.8;
    }
    let e = 0.02;
    let hx = wave_height(p + vec2<f32>(e, 0.0), t) - wave_height(p - vec2<f32>(e, 0.0), t);
    let hy = wave_height(p + vec2<f32>(0.0, e), t) - wave_height(p - vec2<f32>(0.0, e), t);
    var slope = vec2<f32>(hx, hy) / (2.0 * e) * k;
    if mode > 1.5 {
        slope = slope.yx; // `p` était transposé
    }
    let n_w = normalize(tbn * vec3<f32>(-slope.x, -slope.y, 1.0));

    let ndv = max(dot(n_w, v), 0.0);
    // Schlick, plancher relevé (0.06) et exposant adouci : l'eau réelle reflète
    // nettement plus qu'un diélectrique idéal (film de surface, écume fine,
    // diffusion) — sans quoi une rivière vue d'en haut est un aplat de fond.
    let fresnel = 0.06 + 0.94 * pow(1.0 - ndv, 4.0);
    let r = reflect(-v, n_w);
    let sky_t = smoothstep(-0.05, 0.6, r.y);
    var sky_col = mix(light.sky_horizon.rgb, light.sky_zenith.rgb, sky_t);
    // Sous l'horizon, le reflet est celui des berges et de la forêt, pas du ciel.
    sky_col = mix(sky_col * vec3<f32>(0.18, 0.24, 0.18), sky_col, smoothstep(-0.25, 0.1, r.y));
    // Réflexion planaire (`extra3.x`) : la scène vue par la caméra miroir, lue
    // aux coordonnées écran du fragment (groupe 3 = texture de réflexion pour
    // les objets eau), légèrement déformée par les vaguelettes ; pondérée par la
    // proximité du fragment au plan de réflexion (une rivière en pente s'en
    // éloigne peu à peu → retour progressif au ciel).
    if light.extra3.x > 0.5 && mode < 1.5 {
        let suv = (in.clip_position.xy - light.extra2.xy) / max(light.extra2.zw, vec2<f32>(1.0, 1.0));
        let distort = vec2<f32>(n_w.x, n_w.z) * vec2<f32>(0.03, 0.05);
        let ruv = clamp(suv + distort, vec2<f32>(0.002, 0.002), vec2<f32>(0.998, 0.998));
        let refl = textureSampleLevel(albedo_tex, albedo_samp, ruv, 0.0).rgb;
        let near_plane = 1.0 - smoothstep(0.6, 3.0, abs(in.world_pos.y - light.extra.w));
        sky_col = mix(sky_col, refl, near_plane);
    }
    let rl = max(dot(r, light_dir), 0.0);
    let glint = (pow(rl, 1200.0) * 8.0 + pow(rl, 80.0) * 0.4) * shadow;

    let depth = in.vcolor.g;
    let foam_mask = in.vcolor.r;
    var body_depth = depth;
    if mode > 1.5 {
        body_depth = 0.25;
    }
    let shallow = in.color * vec3<f32>(0.45, 0.62, 0.55);
    let deep = in.color * vec3<f32>(0.04, 0.14, 0.15);
    let body = mix(shallow, deep, body_depth)
        * (ambient * 1.2 + sun * (0.35 + 0.35 * max(dot(n_w, light_dir), 0.0) * shadow));

    // Écume : bruit seuillé, seuil abaissé par le masque de sommet × réglage.
    let fn1 = vnoise(p * 3.0 + vec2<f32>(-t * 1.3, t * 0.4));
    let fn2 = vnoise(p * 7.0 + vec2<f32>(-t * 2.1, -t * 0.7));
    let foam_n = fn1 * 0.6 + fn2 * 0.4;
    let cover = clamp(foam_mask * foam_amt, 0.0, 1.0);
    let th = mix(0.98, 0.42, cover);
    let foam = smoothstep(th, th + 0.14, foam_n) * smoothstep(0.0, 0.15, cover);
    let foam_col = vec3<f32>(0.92, 0.95, 0.97) * (ambient * 1.3 + sun * (0.55 + 0.45 * shadow));

    var col = mix(body, sky_col, fresnel) + glint * light.color.rgb;
    col = mix(col, foam_col, foam);
    var alpha = mix(in.alpha, 1.0, max(fresnel * 0.8, foam));
    if mode > 1.5 {
        alpha = alpha * mix(0.35, 1.0, depth); // bords de nappe effilochés
    }
    col = mix(col, light.fog.rgb, fog_amount);
    return vec4<f32>(col, alpha);
}

// Facteur d'ombre [0..1] : 1 = pleinement éclairé, 0 = dans l'ombre. Cascade
// choisie par la distance à la caméra (`cascade_splits`) ; `select` plutôt
// qu'un index dynamique dans l'uniform, pour rester trivialement portable
// (WebGPU/naga) et sans branche divergente autour de l'échantillonnage.
fn shadow_factor(world_pos: vec3<f32>) -> f32 {
    let view_dist = length(camera.eye.xyz - world_pos);
    let c1 = f32(view_dist > light.cascade_splits.x);
    let c2 = f32(view_dist > light.cascade_splits.y);
    let cascade = i32(c1 + c2);
    let lp0 = light.cascade_vp[0] * vec4<f32>(world_pos, 1.0);
    let lp1 = light.cascade_vp[1] * vec4<f32>(world_pos, 1.0);
    let lp2 = light.cascade_vp[2] * vec4<f32>(world_pos, 1.0);
    let lp = select(select(lp0, lp1, cascade == 1), lp2, cascade == 2);
    let proj = lp.xyz / lp.w;
    // Hors de la carte d'ombre → considéré éclairé. Calculé en booléen, PAS en
    // retour anticipé : `textureSampleCompare` (boucle PCF plus bas) doit rester
    // atteint en flux de contrôle **uniforme** (exigé par la validation WebGPU —
    // Chrome le rejette explicitement, contrairement aux backends natifs qui
    // l'acceptaient sans broncher, Sprint 114). Un retour anticipé basé sur
    // `world_pos` — qui varie par fragment — ferait sortir certains threads du
    // quad avant l'appel à la texture, ce qui casse le calcul des dérivées
    // implicites dont dépend l'échantillonnage.
    // Au-delà de la dernière cascade : plus d'ombre du tout (éclairé).
    let in_bounds = proj.x >= -1.0 && proj.x <= 1.0 && proj.y >= -1.0 && proj.y <= 1.0
        && proj.z <= 1.0 && view_dist <= light.cascade_splits.z;
    let uv = vec2<f32>(proj.x * 0.5 + 0.5, 0.5 - proj.y * 0.5);
    // Biais constant en profondeur normalisée : la plage de profondeur d'une
    // cascade grandit avec sa taille, donc le biais en mètres aussi — ce qui est
    // justement ce qu'il faut (les texels lointains sont plus gros).
    let bias = 0.0025;
    // PCF 5x5 pour adoucir le bord — toujours exécuté, même hors carte d'ombre
    // (le résultat est alors ignoré par le `select` final, cf. ci-dessus).
    var sum = 0.0;
    let texel = light.cascade_splits.w;
    for (var dx = -2; dx <= 2; dx = dx + 1) {
        for (var dy = -2; dy <= 2; dy = dy + 1) {
            let o = vec2<f32>(f32(dx), f32(dy)) * texel;
            sum = sum + textureSampleCompare(shadow_map, shadow_samp, uv + o, cascade, proj.z - bias);
        }
    }
    return select(sum / 25.0, 1.0, !in_bounds);
}

@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    // Dérivées écran pour le repère tangent du shader d'eau : ici, en tête de
    // fonction, donc en flux de contrôle uniforme (exigence WebGPU pour dpdx/dpdy).
    let dp1 = dpdx(in.world_pos);
    let dp2 = dpdy(in.world_pos);
    let duv1 = dpdx(in.uv);
    let duv2 = dpdy(in.uv);
    // Passe de réflexion planaire : rien sous le plan d'eau (le lit ne se
    // reflète pas ; `discard` = démotion, les dérivées ci-dessus restent valides).
    if light.extra3.y > 0.5 && in.world_pos.y < light.extra.w - 0.03 {
        discard;
    }

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

    // Surface d'eau : branche dédiée (après le dernier `textureSample`, donc
    // sans contrainte d'uniformité sur ce qui suit).
    if in.water.x > 0.5 {
        return shade_water(in, n, front_facing, dp1, dp2, duv1, duv2, light_dir, shadow);
    }

    let metallic = clamp(in.material.x, 0.0, 1.0);
    let roughness = clamp(in.material.y, 0.04, 1.0);
    let emissive = in.material.z;

    // Diffuse : atténuée pour les métaux (qui réfléchissent au lieu de diffuser).
    let kd = 1.0 - metallic;
    // Ambiante hémisphérique : teintée par le ciel (horizon/zénith) plutôt qu'un
    // lavage gris uniforme — une surface qui regarde vers le haut capte la teinte du
    // zénith, une surface qui regarde vers le bas celle de l'horizon. `light.ambient.x`
    // reste l'intensité globale (réglage scène), seule la couleur change.
    let hemi = clamp(dot(n, vec3<f32>(0.0, 1.0, 0.0)) * 0.5 + 0.5, 0.0, 1.0);
    let ambient_color = mix(light.sky_horizon.rgb, light.sky_zenith.rgb, hemi);
    let ambient_term = albedo * kd * ambient_color * light.ambient.x;
    let direct_term = albedo * kd * light.color.rgb * diffuse * (1.0 - light.ambient.x) * shadow;
    var color = ambient_term + direct_term;

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

    // Brouillard exponentiel (Sprint 89) : mélange vers `fog.rgb` en fonction de la
    // distance à la caméra — `density = 0` (par défaut) laisse `color` inchangée
    // (`fog_amount` reste à 0 quelle que soit la distance).
    let fog_amount = fog_amount_at(in.world_pos);
    color = mix(color, light.fog.rgb, fog_amount);
    // Alpha = opacité de l'objet : ignoré par le pipeline opaque (`REPLACE`), mélangé
    // par la passe transparente (`ALPHA_BLENDING`, cf. `transparent_pipeline`).
    return vec4<f32>(color, in.alpha);
}
