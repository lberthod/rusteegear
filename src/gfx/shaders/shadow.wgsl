// Passe d'ombre : rend la profondeur de la scène depuis le point de vue de la lumière.
// Vertex seul (pas de fragment) ; réutilise les bind groups caméra+lumière (0) et modèle (1).

struct Light {
    dir: vec4<f32>,
    color: vec4<f32>,
    ambient: vec4<f32>,
    light_vp: mat4x4<f32>,
};
@group(0) @binding(1) var<uniform> light: Light;

struct Model {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    params: vec4<f32>,
    color: vec4<f32>,
};
@group(1) @binding(0) var<uniform> model: Model;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return light.light_vp * model.model * vec4<f32>(position, 1.0);
}
