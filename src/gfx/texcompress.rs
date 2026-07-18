//! Compression BC3 des textures d'albédo (Phase E, `sprintoptimation3daudit10h.md`
//! Sprint 6) : réduit l'empreinte VRAM d'un facteur ~4 par rapport à `Rgba8UnormSrgb`
//! quand le GPU expose `Features::TEXTURE_COMPRESSION_BC` — sinon `pipelines::
//! make_texture` reste sur le chemin non compressé existant (dégradation silencieuse,
//! même idiome que `TIMESTAMP_QUERY` dans `renderer.rs`). BC3 plutôt que BC1 : conserve
//! un canal alpha interpolé 8 bits, nécessaire pour les découpes du feuillage
//! (`nature_grass_tuft.glb`/`nature_fern.glb`) qui perdraient leur alpha progressif
//! avec l'alpha 1 bit de BC1.
//!
//! ASTC (mobile) n'est PAS traité ici : `TEXTURE_COMPRESSION_BC` n'est en pratique
//! jamais exposé par les GPU Android/iOS (Adreno/Mali/Apple GPU utilisent ASTC), donc
//! ce module n'a d'effet mesurable que sur desktop pour l'instant — cf. le document de
//! sprint pour le suivi de ce travail restant.
//!
//! Audit du 18 juillet 2026 (`sprintEoptimisation10h.md`) : deux défauts corrigés ici
//! après la livraison initiale — la chaîne de mips était tronquée à un seul niveau pour
//! toute texture non multiple de 8 (`Format::compress` de `texpresso` gère en réalité
//! très bien les blocs partiels via `num_blocks`, ceil-arrondi, donc plus besoin de
//! l'exiger nous-mêmes), et le filtre de réduction moyennait les octets sRGB bruts
//! au lieu d'un espace linéaire (assombrissait les mips par rapport au blit GPU du
//! chemin non compressé, qui décode/ré-encode sRGB automatiquement via le sampler).

use texpresso::{Format, Params, num_blocks};

/// En dessous de 4×4, compresser coûterait plus de VRAM que ça n'en économise (un bloc
/// BC3 fait 16 octets quelle que soit la taille réelle couverte, contre 4 octets/pixel
/// en RGBA8 brut) — pas une limite technique de `texpresso` (qui gère les blocs
/// partiels via un masque, cf. `Format::compress`), un choix d'efficacité mémoire.
pub(super) fn supports_compression(width: u32, height: u32) -> bool {
    width >= 4 && height >= 4
}

/// Décodage sRGB → linéaire (formule exacte, pas l'approximation gamma 2.2) — c'est ce
/// que le sampler GPU fait automatiquement en lisant une texture `*UnormSrgb`, donc ce
/// qu'il faut reproduire ici pour que la moyenne 2×2 de `downsample` soit physiquement
/// correcte plutôt qu'assombrie par une moyenne en espace gamma.
fn srgb_u8_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Ré-encodage linéaire → sRGB, symétrique de `srgb_u8_to_linear` — ce que le sampler
/// GPU fait automatiquement en écrivant vers une cible `*UnormSrgb`.
fn linear_to_srgb_u8(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Filtre boîte 2×2 en espace linéaire pour les canaux RGB (`srgb_u8_to_linear`/
/// `linear_to_srgb_u8`) — le canal alpha d'un format `*UnormSrgb` n'est PAS gamma-encodé
/// (seuls RGB le sont), il est donc moyenné directement. Réduction de moitié utilisée
/// pour bâtir la chaîne de mips côté CPU : les formats compressés ne peuvent pas être
/// `RENDER_ATTACHMENT`, donc le blit GPU de `pipelines::make_texture` ne s'applique pas
/// ici. Dimension impaire : le dernier pixel/ligne est ignoré (troncature), comme le
/// ferait un halving GPU standard — pas une nouvelle approximation introduite ici.
fn downsample(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let nw = (width / 2).max(1);
    let nh = (height / 2).max(1);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let mut rgb_acc = [0f32; 3];
            let mut a_acc = 0u32;
            for dy in 0..2 {
                for dx in 0..2 {
                    // `.min(width - 1)` : couvre le cas d'une dimension source à 1 seul
                    // pixel (nw/nh forcés à 1 par le `.max(1)` ci-dessus) sans lire hors
                    // limites — le pixel unique est alors simplement échantillonné 4 fois.
                    let sx = (x * 2 + dx).min(width - 1);
                    let sy = (y * 2 + dy).min(height - 1);
                    let idx = ((sy * width + sx) * 4) as usize;
                    for (c, acc) in rgb_acc.iter_mut().enumerate() {
                        *acc += srgb_u8_to_linear(rgba[idx + c]);
                    }
                    a_acc += rgba[idx + 3] as u32;
                }
            }
            let oidx = ((y * nw + x) * 4) as usize;
            for (c, acc) in rgb_acc.iter().enumerate() {
                out[oidx + c] = linear_to_srgb_u8(acc / 4.0);
            }
            out[oidx + 3] = (a_acc / 4) as u8;
        }
    }
    out
}

fn compress_bc3(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let size = Format::Bc3.compressed_size(width as usize, height as usize);
    let mut out = vec![0u8; size];
    Format::Bc3.compress(
        rgba,
        width as usize,
        height as usize,
        Params::default(),
        &mut out,
    );
    out
}

/// Équivalent compressé de `pipelines::make_texture` : mêmes paramètres d'entrée
/// (RGBA8 décodé + dimensions), même bind group de sortie (groupe 3, layout `tex_layout`
/// partagé) — appelable par `make_texture` sans changement des sites d'appel. Chaîne de
/// mips complète jusqu'à 1×1 (`pipelines::mip_count_for`, la même formule que le chemin
/// non compressé) : `Format::compress` gère les blocs partiels en bordure via un masque,
/// donc aucune dimension n'est jamais un obstacle à continuer la chaîne (contrairement à
/// une première version de ce module qui s'arrêtait dès qu'une dimension n'était plus
/// multiple de 8 — cf. audit du 18 juillet 2026 en tête de fichier).
pub(super) fn make_compressed_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> wgpu::BindGroup {
    let mip_count = super::pipelines::mip_count_for(width, height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("albedo_bc3"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bc3RgbaUnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let mut level_rgba = rgba.to_vec();
    let mut lw = width;
    let mut lh = height;
    for level in 0..mip_count {
        let compressed = compress_bc3(&level_rgba, lw, lh);
        // `num_blocks` (ceil-arrondi à 4) plutôt que `lw / 4`/`lh / 4` (troncature) :
        // `compressed_size`/`compress` de `texpresso` posent déjà cette même taille de
        // sortie pour les dimensions non multiples de 4 (blocs de bord partiellement
        // masqués) — un stride en troncature désynchroniserait la mise en page attendue
        // par `queue.write_texture` dès le premier mip non multiple de 4.
        let blocks_wide = num_blocks(lw as usize) as u32;
        let blocks_high = num_blocks(lh as usize) as u32;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &compressed,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(blocks_wide * Format::Bc3.block_size() as u32),
                rows_per_image: Some(blocks_high),
            },
            // wgpu exige que l'étendue de la copie soit un multiple exact du bloc
            // (`command::transfer::validate_texture_copy_range`, `copy_size.width.
            // is_multiple_of(block_width)`, sans exception pour un mip dont la taille
            // « virtuelle » (`lw`/`lh`) ne l'est pas) : il faut donc arrondir à la taille
            // « physique » du mip (`blocks_wide * 4`/`blocks_high * 4`), pas `lw`/`lh` —
            // exactement ce que `TextureDescriptor::mip_level_size(..).physical_size(..)`
            // calcule en interne pour valider cette même copie. Panique GPU trouvée par
            // `golden_textured_ground_with_mipmaps` (« Copy width is not a multiple of
            // block width ») avant cette correction — cf. audit du 18 juillet 2026.
            wgpu::Extent3d {
                width: blocks_wide * 4,
                height: blocks_high * 4,
                depth_or_array_layers: 1,
            },
        );
        if level + 1 < mip_count {
            let next_lw = (lw / 2).max(1);
            let next_lh = (lh / 2).max(1);
            level_rgba = downsample(&level_rgba, lw, lh);
            lw = next_lw;
            lh = next_lh;
        }
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("tex_bg_bc3"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_compression_requires_at_least_4x4_but_not_multiples_of_4() {
        assert!(!supports_compression(1, 1));
        assert!(!supports_compression(3, 4));
        assert!(!supports_compression(4, 3));
        assert!(supports_compression(4, 4));
        // Avant l'audit du 18 juillet 2026, ce cas était rejeté (5 n'est pas multiple de
        // 4) alors que `texpresso` gère très bien les blocs de bord partiels.
        assert!(supports_compression(5, 5));
        assert!(supports_compression(512, 512));
    }

    #[test]
    fn make_compressed_texture_builds_a_full_mip_chain_matching_the_uncompressed_path() {
        // Même formule que le chemin non compressé (`pipelines::mip_count_for`) : la
        // chaîne va jusqu'à 1×1, y compris pour une dimension non multiple de 8 (300 —
        // le cas que l'ancienne `compressible_mip_count` tronquait à 1 seul niveau).
        assert_eq!(super::super::pipelines::mip_count_for(1024, 1024), 11);
        assert_eq!(super::super::pipelines::mip_count_for(300, 300), 9);
        assert_eq!(super::super::pipelines::mip_count_for(4, 4), 3);
    }

    #[test]
    fn downsample_averages_in_linear_space_not_gamma_space() {
        // 2×2 image, canal rouge à 0 et 255 (les deux autres pixels aussi 0/255 pour
        // rester dans un cas extrême) : une moyenne naïve sur les octets sRGB donnerait
        // 127/128. La moyenne correcte (décodage sRGB → linéaire, moyenne, ré-encodage)
        // donne un résultat sensiblement plus clair (~188), parce que le point milieu en
        // espace linéaire (0.5) ré-encode à sRGB ~0.735, pas 0.5.
        #[rustfmt::skip]
        let rgba = [
            0, 0, 0, 255,       0, 0, 0, 255,
            255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let out = downsample(&rgba, 2, 2);
        assert_eq!(out.len(), 4);
        assert!(
            (180..=196).contains(&out[0]),
            "moyenne en espace gamma au lieu de linéaire (valeur: {})",
            out[0]
        );
        // L'alpha (déjà linéaire dans un format `*UnormSrgb`) n'a pas cette correction —
        // moyenne directe, 255 partout ici.
        assert_eq!(out[3], 255);
    }

    #[test]
    fn downsample_handles_odd_and_1px_dimensions_without_panicking() {
        let rgba_3x3 = [100u8; 3 * 3 * 4];
        let out = downsample(&rgba_3x3, 3, 3);
        assert_eq!(out.len(), 4); // 3/2 = 1 (troncature) sur chaque axe.

        let rgba_1x1 = [10u8, 20, 30, 40];
        let out = downsample(&rgba_1x1, 1, 1);
        assert_eq!(out.len(), 4);
        assert_eq!(out[3], 40); // alpha inchangé (un seul pixel, échantillonné 4×).
    }

    #[test]
    fn compress_bc3_roundtrip_preserves_alpha_within_tolerance() {
        let width = 8u32;
        let height = 8u32;
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for px in rgba.chunks_mut(4) {
            px[0] = 200;
            px[1] = 50;
            px[2] = 10;
            px[3] = 128;
        }
        let compressed = compress_bc3(&rgba, width, height);
        assert_eq!(
            compressed.len(),
            Format::Bc3.compressed_size(width as usize, height as usize)
        );
        let mut decoded = vec![0u8; rgba.len()];
        Format::Bc3.decompress(&compressed, width as usize, height as usize, &mut decoded);
        // BC3 est un format à perte : tolérance large, on vérifie juste que l'alpha
        // reste dans le voisinage de la valeur d'origine (pas un canal ignoré).
        for px in decoded.chunks(4) {
            assert!(
                (px[3] as i32 - 128).abs() <= 8,
                "alpha décompressé: {}",
                px[3]
            );
        }
    }

    #[test]
    fn compress_bc3_roundtrip_handles_a_non_multiple_of_4_dimension() {
        // 5×5 : rejeté par l'ancienne `supports_compression`, doit maintenant fonctionner
        // (blocs de bord partiels gérés par `texpresso` via un masque).
        let width = 5u32;
        let height = 5u32;
        let rgba = vec![77u8; (width * height * 4) as usize];
        let compressed = compress_bc3(&rgba, width, height);
        assert_eq!(
            compressed.len(),
            Format::Bc3.compressed_size(width as usize, height as usize)
        );
        let mut decoded = vec![0u8; rgba.len()];
        Format::Bc3.decompress(&compressed, width as usize, height as usize, &mut decoded);
        assert_eq!(decoded.len(), rgba.len());
    }
}
