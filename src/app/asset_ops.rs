//! Pipeline d'assets pour l'export mobile : optimisation de textures, rassemblement
//! en `asset://`, conversion en puissances de 2, bake lighting. Extrait de `app/mod.rs`.

use super::AppState;

impl AppState {
    /// Réduit sur disque les textures dépassant `max_px` (côté le plus long), écrit
    /// une copie `…_opt.png` et met à jour les objets. Renvoie le nombre de textures réduites.
    pub fn optimize_textures(&mut self, max_px: u32) -> usize {
        use std::collections::HashMap;
        // chemins uniques utilisés par la scène
        let mut paths: Vec<String> = self
            .scene
            .objects
            .iter()
            .map(|o| o.texture.clone())
            .filter(|t| !t.is_empty())
            .collect();
        paths.sort();
        paths.dedup();

        let mut remap: HashMap<String, String> = HashMap::new();
        for path in paths {
            let Some(bytes) = crate::assets::read_bytes(&path) else {
                log::error!("Texture illisible {path}");
                continue;
            };
            let Ok(img) = image::load_from_memory(&bytes) else {
                log::error!("Texture non décodable {path}");
                continue;
            };
            let (w, h) = (img.width(), img.height());
            if w <= max_px && h <= max_px {
                continue;
            }
            let scale = max_px as f32 / w.max(h) as f32;
            let (nw, nh) = (
                ((w as f32 * scale) as u32).max(1),
                ((h as f32 * scale) as u32).max(1),
            );
            let resized = img.resize(nw, nh, image::imageops::FilterType::Lanczos3);
            let out = optimized_path(&path, max_px);
            // `asset://x.png` → écrit dans le dossier d'assets ; chemin disque → à côté.
            let write_path = match crate::assets::assets_dir() {
                Some(dir) if out.starts_with(crate::assets::ASSET_SCHEME) => dir
                    .join(out.trim_start_matches(crate::assets::ASSET_SCHEME))
                    .to_string_lossy()
                    .into_owned(),
                _ => out.clone(),
            };
            if let Err(e) = resized.save(&write_path) {
                log::error!("Échec écriture texture optimisée {write_path} : {e}");
                continue;
            }
            log::info!("Texture {path} ({w}×{h}) → {out} ({nw}×{nh})");
            remap.insert(path, out);
        }
        if remap.is_empty() {
            return 0;
        }
        self.push_undo();
        for o in &mut self.scene.objects {
            if let Some(new) = remap.get(&o.texture) {
                o.texture = new.clone();
            }
        }
        remap.len()
    }

    /// Rassemble les assets externes (textures, sons, modèles) dans le dossier de
    /// projet et réécrit les chemins en `asset://…` (portable). Renvoie le nombre réécrit.
    pub fn collect_assets(&mut self) -> usize {
        let is_external = |p: &str| !p.is_empty() && !crate::assets::is_known_scheme(p);
        let any = self.scene.objects.iter().any(|o| {
            is_external(&o.texture) || o.audio.as_ref().is_some_and(|a| is_external(&a.clip))
        }) || self.scene.imported.iter().any(|m| is_external(&m.path));
        if !any {
            return 0;
        }
        self.push_undo();
        let mut changed = 0;
        let mut import = |p: &mut String| {
            if is_external(p)
                && let Some(a) = crate::assets::import_to_assets(p)
            {
                *p = a;
                changed += 1;
            }
        };
        for o in &mut self.scene.objects {
            import(&mut o.texture);
            if let Some(a) = &mut o.audio {
                import(&mut a.clip);
            }
        }
        for m in &mut self.scene.imported {
            import(&mut m.path);
        }
        changed
    }

    /// Limite le nombre de lumières ponctuelles (optimisation mobile).
    pub fn limit_point_lights(&mut self, max: usize) {
        if self.scene.point_lights.len() > max {
            self.push_undo();
            self.scene.point_lights.truncate(max);
        }
    }

    /// Convertisseur de textures : redimensionne chaque texture aux **puissances de 2**
    /// inférieures (mip-mapping/compression GPU mobile). Écrit des copies `…_pot.png`
    /// et met à jour les objets. Renvoie le nombre de textures converties.
    pub fn convert_textures_pot(&mut self) -> usize {
        use std::collections::HashMap;
        let mut paths: Vec<String> = self
            .scene
            .objects
            .iter()
            .map(|o| o.texture.clone())
            .filter(|t| !t.is_empty())
            .collect();
        paths.sort();
        paths.dedup();

        // Plus grande puissance de 2 ≤ v (bornée à [1, 4096]).
        let pot = |v: u32| -> u32 {
            if v < 2 {
                return 1;
            }
            (1u32 << (31 - v.leading_zeros())).clamp(1, 4096)
        };

        let mut remap: HashMap<String, String> = HashMap::new();
        for path in paths {
            let Some(bytes) = crate::assets::read_bytes(&path) else {
                log::error!("Texture illisible {path}");
                continue;
            };
            let Ok(img) = image::load_from_memory(&bytes) else {
                log::error!("Texture non décodable {path}");
                continue;
            };
            let (w, h) = (img.width(), img.height());
            let (nw, nh) = (pot(w), pot(h));
            if nw == w && nh == h {
                continue; // déjà en puissances de 2
            }
            let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);
            let out = format!("{path}_pot.png");
            let write_path = match crate::assets::assets_dir() {
                Some(dir) if out.starts_with(crate::assets::ASSET_SCHEME) => dir
                    .join(out.trim_start_matches(crate::assets::ASSET_SCHEME))
                    .to_string_lossy()
                    .into_owned(),
                _ => out.clone(),
            };
            if let Err(e) = resized.save(&write_path) {
                log::error!("Échec écriture texture POT {write_path} : {e}");
                continue;
            }
            log::info!("Texture {path} ({w}×{h}) → {out} ({nw}×{nh}) [POT]");
            remap.insert(path, out);
        }
        if remap.is_empty() {
            return 0;
        }
        self.push_undo();
        for o in &mut self.scene.objects {
            if let Some(new) = remap.get(&o.texture) {
                o.texture = new.clone();
            }
        }
        remap.len()
    }

    /// Bake lighting : fige la contribution des lumières **ponctuelles** dans l'émission
    /// statique de chaque objet (selon distance/portée), puis les supprime. Réduit le
    /// nombre de lumières dynamiques (coût GPU mobile). Renvoie le nombre de lumières figées.
    pub fn bake_lighting(&mut self) -> usize {
        let lights = self.scene.point_lights.clone();
        if lights.is_empty() {
            return 0;
        }
        self.push_undo();
        for o in &mut self.scene.objects {
            let p = o.transform.position;
            let mut add = 0.0f32;
            for l in &lights {
                let lp = glam::Vec3::from(l.position);
                let d = (lp - p).length();
                if d < l.range {
                    let falloff = 1.0 - d / l.range; // atténuation linéaire simple
                    // Luminance approximative de la lumière.
                    let lum = (l.color[0] + l.color[1] + l.color[2]) / 3.0;
                    add += l.intensity * falloff * lum;
                }
            }
            o.emissive = (o.emissive + add * 0.6).clamp(0.0, 3.0);
        }
        let n = lights.len();
        self.scene.point_lights.clear();
        n
    }
}

/// Chemin de la copie optimisée d'une texture (`foo.png` → `foo_opt2048.png`).
/// Conserve le schéma `asset://`/`bundle://` éventuel ; sinon écrit à côté du fichier.
pub(super) fn optimized_path(path: &str, max_px: u32) -> String {
    // Une référence `asset-id://<uuid>` n'a pas de nom de fichier en soi —
    // la résoudre d'abord vers son `asset://<nom>` courant, sinon le nom dérivé serait
    // l'uuid tel quel (illisible, et incohérent d'une exécution à l'autre si l'asset
    // est renommé entre-temps).
    let path = crate::assets::resolve_asset_id(path).unwrap_or_else(|| path.to_string());
    let path = path.as_str();
    for scheme in [crate::assets::ASSET_SCHEME, crate::assets::SCHEME] {
        if let Some(key) = path.strip_prefix(scheme) {
            let stem = std::path::Path::new(key)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("texture");
            // Une copie optimisée d'un asset devient elle-même un asset de projet.
            return format!("{}{stem}_opt{max_px}.png", crate::assets::ASSET_SCHEME);
        }
    }
    let p = std::path::Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("texture");
    let parent = p.parent().and_then(|s| s.to_str()).unwrap_or("");
    let name = format!("{stem}_opt{max_px}.png");
    if parent.is_empty() {
        name
    } else {
        format!("{parent}/{name}")
    }
}
