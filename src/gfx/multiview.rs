//! Variantes **multiview** des shaders de scène (phase 2 de la roadmap VR) :
//! une seule passe dessine les deux yeux, chacun dans une couche d'une
//! texture-tableau (`multiview_mask = 0b11`), le GPU rejouant chaque draw call
//! avec `@builtin(view_index)` = 0 puis 1. Divise par deux le coût CPU
//! d'encodage de la scène en VR.
//!
//! Plutôt que dupliquer les shaders, la variante est **dérivée du source
//! existant** par `multiview_wgsl` : l'uniform `camera` (binding 0 du groupe 0)
//! devient une paire de caméras, et chaque point d'entrée recopie celle de
//! son œil dans une variable privée `camera` — le reste du shader est inchangé.
//! Un shader qui évolue fait évoluer sa variante ; les tests valident chaque
//! variante avec naga.

/// Déclaration de l'uniform caméra, identique dans tous les shaders de scène.
const CAMERA_DECL: &str = "@group(0) @binding(0) var<uniform> camera: Camera;";

/// Transforme un shader de scène en sa variante multiview. `camera_size` =
/// taille en octets de `CameraUniform` côté Rust : chaque shader ne déclare
/// qu'un **préfixe** de la structure (le ciel s'arrête à `inv_view_proj`), la
/// seconde caméra doit donc être placée à `camera_size`, pas à la taille du
/// préfixe local (`@size`).
///
/// Panique si le shader ne contient pas la déclaration attendue : une
/// dérive du source doit casser les tests, pas produire un shader faux.
pub(super) fn multiview_wgsl(src: &str, camera_size: usize) -> String {
    assert!(
        src.contains(CAMERA_DECL),
        "shader de scène sans `{CAMERA_DECL}` — variante multiview impossible"
    );
    let decl = format!(
        "struct MvCameras {{\n    @size({camera_size}) left: Camera,\n    right: Camera,\n}};\n\
         @group(0) @binding(0) var<uniform> mv_cameras: MvCameras;\n\
         var<private> camera: Camera;\n\
         fn mv_select(view: u32) -> Camera {{\n    if (view == 0u) {{ return mv_cameras.left; }}\n    return mv_cameras.right;\n}}"
    );
    let mut out = src.replacen(CAMERA_DECL, &decl, 1);

    // Points d'entrée : paramètre `view_index` ajouté en tête, caméra de l'œil
    // recopiée en première instruction.
    let mut search_from = 0;
    loop {
        let next = ["@vertex", "@fragment"]
            .iter()
            .filter_map(|tag| out[search_from..].find(tag).map(|i| search_from + i))
            .min();
        let Some(tag_at) = next else { break };
        let fn_at = tag_at
            + out[tag_at..]
                .find("fn ")
                .expect("`fn` après l'attribut d'entrée");
        let open = fn_at + out[fn_at..].find('(').expect("`(` des paramètres");
        out.insert_str(open + 1, "@builtin(view_index) mv_view: u32, ");
        let close = matching_paren(&out, open);
        let body = close + out[close..].find('{').expect("`{` du corps");
        out.insert_str(body + 1, "\n    camera = mv_select(mv_view);");
        search_from = body + 1;
    }
    out
}

/// Indice de la `)` qui ferme la `(` en `open` (attributs `@location(0)`
/// imbriqués dans la liste de paramètres).
fn matching_paren(s: &str, open: usize) -> usize {
    let mut depth = 0usize;
    for (i, c) in s[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return open + i;
                }
            }
            _ => {}
        }
    }
    panic!("parenthèse non fermée dans le shader");
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAMERA_SIZE: usize = std::mem::size_of::<crate::gfx::renderer::CameraUniform>();

    fn validate(name: &str, src: &str) {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{name} : WGSL invalide\n{}", e.emit_to_string(src)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name} : validation naga échouée : {e:?}"));
    }

    #[test]
    fn every_scene_shader_has_a_valid_multiview_variant() {
        for (name, src) in [
            ("main.wgsl", include_str!("shaders/main.wgsl")),
            ("sky.wgsl", include_str!("shaders/sky.wgsl")),
            ("particles.wgsl", include_str!("shaders/particles.wgsl")),
            ("skinned.wgsl", include_str!("shaders/skinned.wgsl")),
            ("gizmo.wgsl", include_str!("shaders/gizmo.wgsl")),
        ] {
            validate(name, src);
            let mv = multiview_wgsl(src, CAMERA_SIZE);
            validate(&format!("{name} (multiview)"), &mv);
            assert!(mv.contains("mv_cameras"), "{name}");
            let entries = src.matches("@vertex").count() + src.matches("@fragment").count();
            assert_eq!(
                mv.matches("camera = mv_select(mv_view);").count(),
                entries,
                "{name}"
            );
        }
    }

    #[test]
    fn the_second_camera_sits_at_the_full_uniform_size() {
        // Le ciel ne déclare que 3 champs (144 octets) : la seconde caméra doit
        // quand même être lue à 176 (taille réelle de `CameraUniform`).
        assert_eq!(CAMERA_SIZE, 176);
        let mv = multiview_wgsl(include_str!("shaders/sky.wgsl"), CAMERA_SIZE);
        assert!(mv.contains("@size(176) left: Camera"));
    }
}
