//! Fonctions de création de ressources GPU (textures, buffers, bind groups) utilisées
//! par `renderer.rs` lors du setup et de la synchronisation des textures/uniforms.
//! Extrait de `renderer.rs` (Sprint 113a) — aucun changement de comportement, les
//! signatures/corps sont identiques à ceux d'origine.

use super::renderer::{BLOOM_MIP_LEVELS, DEPTH_FORMAT, HDR_FORMAT, ModelUniform};

pub(super) fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Décode une image (disque ou `bundle://`) en RGBA8 + dimensions. `pub(crate)` :
/// aussi utilisé par `editor::hud` pour les widgets HUD `Image` (cf. Sprint 109),
/// pas seulement les textures de mesh de ce module.
pub(crate) fn load_rgba(path: &str) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = crate::assets::read_bytes(path)?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// Nombre de mips pour une texture `width`×`height` : `1 + log2(plus
/// grande dimension)`, la formule standard — 256 → 9 niveaux (256..1), 1×1 → 1 (rien
/// à générer). `leading_zeros` sur `u32` : direct, sans dépendance à une fonction
/// `log2` flottante (imprécisions d'arrondi à éviter sur un compte de niveaux entier).
pub(super) fn mip_count_for(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

/// Crée une texture RGBA8 + son bind group (groupe 3) prêt à lier, avec sa chaîne de
/// mips complète : sans elle, un objet texturé vu de loin agrège l'aliasing
/// du mip 0 au lieu de moyenner vers une version plus petite — c'est tout l'intérêt de
/// `mip_count_for`/de générer les niveaux suivants ici plutôt que de rester à 1 seul.
#[allow(clippy::too_many_arguments)]
pub(super) fn make_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    mipgen_pipeline: &wgpu::RenderPipeline,
    mipgen_layout: &wgpu::BindGroupLayout,
    mipgen_sampler: &wgpu::Sampler,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> wgpu::BindGroup {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mip_count = mip_count_for(width, height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("albedo"),
        size,
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        // `RENDER_ATTACHMENT` en plus de `TEXTURE_BINDING`/`COPY_DST` : chaque mip > 0
        // est rempli en le ciblant comme cible de rendu (blit), pas via `write_texture`
        // (qui n'a pas de filtre de réduction intégré).
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );

    if mip_count > 1 {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mipgen_encoder"),
        });
        let mut prev_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("mip_src"),
            base_mip_level: 0,
            mip_level_count: Some(1),
            ..Default::default()
        });
        for level in 1..mip_count {
            let target_view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("mip_dst"),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mipgen_bg"),
                layout: mipgen_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&prev_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(mipgen_sampler),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mipgen_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(mipgen_pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            prev_view = target_view;
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    // Vue par défaut (tous les mips) : c'est celle-ci que le shader échantillonne,
    // le sampler choisit/mélange le niveau selon les dérivées d'écran (`mipmap_filter`
    // du sampler, cf. `tex_sampler`).
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("tex_bg"),
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

/// Crée le buffer storage d'instances + son bind group (groupe 1) pour `capacity` objets.
pub(super) fn create_models_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    capacity: usize,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let size = (capacity * std::mem::size_of::<ModelUniform>()) as wgpu::BufferAddress;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("models_storage"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("models_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });
    (buf, bg)
}

pub(super) fn create_uniform(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(super) fn create_depth_view(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Texture HDR intermédiaire : cible de la passe principale avant tone
/// mapping. `width`/`height` explicites plutôt qu'une `SurfaceConfiguration` : réutilisée
/// aussi bien par le chemin fenêtré (taille de la fenêtre) que par les rendus headless
/// (taille demandée par l'appelant, indépendante de toute fenêtre).
pub(super) fn create_hdr_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hdr_color"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Chaîne de mips du bloom : une texture à `BLOOM_MIP_LEVELS` niveaux,
/// démarrant à moitié de la résolution HDR (`width`/`height` = celles de `hdr_view`) —
/// une vue par niveau (`base_mip_level` fixé, `mip_level_count: 1`), utilisable aussi
/// bien comme cible de rendu que comme texture échantillonnée (jamais les deux à la
/// fois dans la même passe).
pub(super) fn create_bloom_mip_views(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Vec<wgpu::TextureView> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bloom_chain"),
        size: wgpu::Extent3d {
            width: (width / 2).max(2),
            height: (height / 2).max(2),
            depth_or_array_layers: 1,
        },
        mip_level_count: BLOOM_MIP_LEVELS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    (0..BLOOM_MIP_LEVELS)
        .map(|level| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("bloom_mip_view"),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        })
        .collect()
}
