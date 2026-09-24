//! Phase 0 de la roadmap VR (`docs/roadmapExportVRQuest24septembre.md`) : spike
//! « Hello Quest ». Prouve toute la chaîne technique sur le casque, sans encore
//! toucher au `Renderer` du moteur :
//!
//! 1. loader OpenXR Khronos + instance XR Android (`XR_KHR_android_create_instance`) ;
//! 2. `VkInstance`/`VkDevice` créés **par le runtime XR** (`XR_KHR_vulkan_enable2`)
//!    avec les extensions que wgpu-hal exige, puis importés dans wgpu
//!    (`Instance::from_hal` → `create_adapter_from_hal` → `create_device_from_hal`) ;
//! 3. swapchain XR à 2 couches (une par œil), images Vulkan enveloppées en
//!    `wgpu::Texture` (`texture_from_raw`, mémoire `External` : jamais libérée par wgpu) ;
//! 4. boucle de frame XR (wait → begin → locate_views → rendu wgpu → end) qui
//!    dessine des cubes fixes dans l'espace `STAGE` (sol réel du joueur).
//!
//! Critère de sortie (go / no-go) : cubes nets et stéréoscopiques, tête trackée,
//! 72 Hz sans saccade, retour propre au menu Quest. Le code « propre » (runtime XR
//! réutilisable par le `Renderer`) viendra en phase 1 ; ce fichier sert de
//! référence minimale qui marche.

use std::ffi::c_char;
use std::time::Duration;

use ash::vk::{self, Handle as _};
use glam::{Mat4, Quat, Vec3};
use openxr as xr;
use wgpu::hal;
use winit::platform::android::activity::{AndroidApp, MainEvent, PollEvent};

use super::math::{Fov, projection_from_fov, view_from_pose};

const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;
const VIEW_COUNT: u32 = 2;
const NEAR: f32 = 0.05;
const FAR: f32 = 100.0;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Vulkan 1.1 : multiview garanti en cœur (phase 2), et supporté par tous les Quest.
const VK_API_VERSION: u32 = vk::API_VERSION_1_1;

/// Point d'entrée VR, appelé par `android_main` quand la feature `vr` est active.
pub fn run(app: AndroidApp) {
    match run_inner(&app) {
        Ok(()) => log::info!("VR : session terminée proprement"),
        Err(e) => log::error!("VR : {e}"),
    }
}

fn err<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{ctx} : {e}")
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    session_info: xr::vulkan::SessionCreateInfo,
}

fn run_inner(app: &AndroidApp) -> Result<(), String> {
    // --- 1. OpenXR ---------------------------------------------------------------
    let platform = unsafe { xr::AndroidPlatformInfo::new(app.vm_as_ptr(), app.activity_as_ptr()) };
    let entry = unsafe { xr::Entry::load(&platform) }.map_err(err(
        "chargement du loader OpenXR (libopenxr_loader.so absent de l'APK ?)",
    ))?;
    let available = entry
        .enumerate_extensions()
        .map_err(err("extensions OpenXR"))?;
    if !available.khr_vulkan_enable2 {
        return Err("le runtime OpenXR n'expose pas XR_KHR_vulkan_enable2".into());
    }
    // Journalisé dès le spike pour la suite de la roadmap : phase 2 (fréquence,
    // foveation) et phase 8 (mains, Mouvéo).
    log::info!(
        "VR : extensions — hand_tracking={} refresh_rate={} foveation={}",
        available.ext_hand_tracking,
        available.fb_display_refresh_rate,
        available.fb_foveation
    );
    let mut enabled = xr::ExtensionSet::default();
    enabled.khr_vulkan_enable2 = true;
    enabled.khr_android_create_instance = true;
    let instance = entry
        .create_instance(
            &xr::ApplicationInfo {
                application_name: "RusteeGear VR",
                application_version: 0,
                engine_name: "RusteeGear",
                engine_version: 0,
                api_version: xr::Version::new(1, 0, 0),
            },
            &enabled,
            &[],
            &platform,
        )
        .map_err(err("création de l'instance OpenXR"))?;
    if let Ok(props) = instance.properties() {
        log::info!(
            "VR : runtime {} {}",
            props.runtime_name,
            props.runtime_version
        );
    }
    let system = instance
        .system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)
        .map_err(err("aucun casque détecté"))?;
    let blend_mode = instance
        .enumerate_environment_blend_modes(system, VIEW_TYPE)
        .map_err(err("modes de composition"))?
        .first()
        .copied()
        .ok_or("aucun mode de composition")?;

    // --- 2. Vulkan créé par OpenXR, importé dans wgpu -------------------------
    let sdk_version = app.config().sdk_version().max(0) as u32;
    let gpu = unsafe { create_gpu(&instance, system, sdk_version)? };

    // --- 3. Session, espace de référence, swapchain -----------------------------
    let (session, mut frame_waiter, mut frame_stream) =
        unsafe { instance.create_session::<xr::Vulkan>(system, &gpu.session_info) }
            .map_err(err("création de la session XR"))?;
    // STAGE : origine au sol, au centre de la zone de jeu (Guardian) — les cubes
    // apparaissent à hauteur réelle quelle que soit la taille du joueur.
    let stage = session
        .create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)
        .map_err(err("espace STAGE"))?;

    let views = instance
        .enumerate_view_configuration_views(system, VIEW_TYPE)
        .map_err(err("vues"))?;
    if views.len() != VIEW_COUNT as usize {
        return Err(format!("{} vues au lieu de 2", views.len()));
    }
    let width = views[0].recommended_image_rect_width;
    let height = views[0].recommended_image_rect_height;
    log::info!("VR : {width}×{height} px par œil");

    let (vk_format, color_format) = pick_swapchain_format(
        &session
            .enumerate_swapchain_formats()
            .map_err(err("formats de swapchain"))?,
    )?;
    let mut swapchain = session
        .create_swapchain(&xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT
                | xr::SwapchainUsageFlags::SAMPLED,
            format: vk_format.as_raw() as _,
            sample_count: 1,
            width,
            height,
            face_count: 1,
            array_size: VIEW_COUNT,
            mip_count: 1,
        })
        .map_err(err("création de la swapchain"))?;
    let images = swapchain
        .enumerate_images()
        .map_err(err("images de swapchain"))?;
    // Pour chaque image : une vue wgpu par œil (couche 0 = gauche, 1 = droite).
    let eye_views: Vec<[wgpu::TextureView; 2]> = images
        .iter()
        .map(|&raw| unsafe { wrap_swapchain_image(&gpu.device, raw, width, height, color_format) })
        .collect();

    let scene = CubeScene::new(&gpu.device, color_format, width, height);

    // --- 4. Boucle -----------------------------------------------------------------
    let mut events = xr::EventDataBuffer::new();
    let mut session_running = false;
    let mut exit_requested = false;
    let mut frames: u64 = 0;
    loop {
        let mut destroy = false;
        app.poll_events(
            Some(if session_running {
                Duration::ZERO
            } else {
                Duration::from_millis(100)
            }),
            |event| {
                if let PollEvent::Main(MainEvent::Destroy) = event {
                    destroy = true;
                }
            },
        );
        if destroy && !exit_requested {
            exit_requested = true;
            if session_running {
                let _ = session.request_exit();
            } else {
                break;
            }
        }

        while let Some(event) = instance
            .poll_event(&mut events)
            .map_err(err("événements XR"))?
        {
            match event {
                xr::Event::SessionStateChanged(e) => {
                    log::info!("VR : état de session {:?}", e.state());
                    match e.state() {
                        xr::SessionState::READY => {
                            session.begin(VIEW_TYPE).map_err(err("début de session"))?;
                            session_running = true;
                        }
                        xr::SessionState::STOPPING => {
                            session.end().map_err(err("fin de session"))?;
                            session_running = false;
                        }
                        xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => {
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                xr::Event::InstanceLossPending(_) => return Ok(()),
                _ => {}
            }
        }
        if !session_running {
            continue;
        }

        let frame_state = frame_waiter.wait().map_err(err("xrWaitFrame"))?;
        frame_stream.begin().map_err(err("xrBeginFrame"))?;
        if !frame_state.should_render {
            frame_stream
                .end(frame_state.predicted_display_time, blend_mode, &[])
                .map_err(err("xrEndFrame"))?;
            continue;
        }

        let (_, located) = session
            .locate_views(VIEW_TYPE, frame_state.predicted_display_time, &stage)
            .map_err(err("xrLocateViews"))?;
        let image_index = swapchain.acquire_image().map_err(err("acquire"))? as usize;
        swapchain
            .wait_image(xr::Duration::INFINITE)
            .map_err(err("wait_image"))?;

        let view_projs = [0, 1].map(|i| {
            let v = &located[i];
            let o = v.pose.orientation;
            let p = v.pose.position;
            let fov = Fov {
                left: v.fov.angle_left,
                right: v.fov.angle_right,
                up: v.fov.angle_up,
                down: v.fov.angle_down,
            };
            projection_from_fov(fov, NEAR, FAR)
                * view_from_pose(
                    Quat::from_xyzw(o.x, o.y, o.z, o.w),
                    Vec3::new(p.x, p.y, p.z),
                )
        });
        scene.render(&gpu.device, &gpu.queue, &eye_views[image_index], view_projs);

        swapchain.release_image().map_err(err("release"))?;
        let rect = xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di {
                width: width as i32,
                height: height as i32,
            },
        };
        let projection_views = [0, 1].map(|i| {
            xr::CompositionLayerProjectionView::new()
                .pose(located[i].pose)
                .fov(located[i].fov)
                .sub_image(
                    xr::SwapchainSubImage::new()
                        .swapchain(&swapchain)
                        .image_array_index(i as u32)
                        .image_rect(rect),
                )
        });
        frame_stream
            .end(
                frame_state.predicted_display_time,
                blend_mode,
                &[&xr::CompositionLayerProjection::new()
                    .space(&stage)
                    .views(&projection_views)],
            )
            .map_err(err("xrEndFrame"))?;

        frames += 1;
        if frames.is_multiple_of(720) {
            log::info!("VR : {frames} images rendues");
        }
    }
    Ok(())
}

/// Crée `VkInstance`/`VkDevice` via le runtime XR avec les extensions exigées par
/// wgpu-hal, puis les importe dans wgpu.
///
/// # Safety
/// FFI Vulkan/OpenXR brut. Les objets Vulkan ne sont jamais détruits par wgpu
/// (`drop_callback` no-op) : le processus Android se termine avec la session.
unsafe fn create_gpu(
    instance: &xr::Instance,
    system: xr::SystemId,
    sdk_version: u32,
) -> Result<Gpu, String> {
    let reqs = instance
        .graphics_requirements::<xr::Vulkan>(system)
        .map_err(err("exigences Vulkan du runtime"))?;
    let wanted = xr::Version::new(1, 1, 0);
    if wanted < reqs.min_api_version_supported
        || wanted.major() > reqs.max_api_version_supported.major()
    {
        return Err(format!(
            "le runtime exige Vulkan {}..{}",
            reqs.min_api_version_supported, reqs.max_api_version_supported
        ));
    }

    let vk_entry = unsafe { ash::Entry::load() }.map_err(err("chargement de libvulkan"))?;
    let flags = wgpu::InstanceFlags::empty();
    let instance_exts = hal::vulkan::Instance::desired_extensions(&vk_entry, VK_API_VERSION, flags)
        .map_err(err("extensions d'instance Vulkan"))?;
    let instance_ext_ptrs: Vec<*const c_char> = instance_exts.iter().map(|e| e.as_ptr()).collect();
    let app_info = vk::ApplicationInfo::default()
        .application_name(c"RusteeGear VR")
        .engine_name(c"RusteeGear")
        .api_version(VK_API_VERSION);
    let instance_ci = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&instance_ext_ptrs);
    #[allow(clippy::missing_transmute_annotations)]
    let get_proc = unsafe { std::mem::transmute(vk_entry.static_fn().get_instance_proc_addr) };
    let raw_instance = unsafe {
        instance.create_vulkan_instance(system, get_proc, &instance_ci as *const _ as *const _)
    }
    .map_err(err("xrCreateVulkanInstanceKHR"))?
    .map_err(|r| format!("vkCreateInstance : {:?}", vk::Result::from_raw(r)))?;
    let ash_instance = unsafe {
        ash::Instance::load(
            vk_entry.static_fn(),
            vk::Instance::from_raw(raw_instance as _),
        )
    };

    let physical = vk::PhysicalDevice::from_raw(
        unsafe { instance.vulkan_graphics_device(system, raw_instance) }
            .map_err(err("GPU du casque"))? as _,
    );
    let queue_family = unsafe { ash_instance.get_physical_device_queue_family_properties(physical) }
        .iter()
        .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        .ok_or("aucune file graphique Vulkan")? as u32;

    let hal_instance = unsafe {
        hal::vulkan::Instance::from_raw(
            vk_entry.clone(),
            ash_instance.clone(),
            VK_API_VERSION,
            sdk_version,
            None,
            instance_exts,
            flags,
            wgpu::MemoryBudgetThresholds::default(),
            false,
            Some(Box::new(|| ())),
        )
    }
    .map_err(err("import de l'instance Vulkan dans wgpu"))?;
    let exposed = hal_instance
        .expose_adapter(physical)
        .ok_or("wgpu refuse le GPU du casque")?;
    log::info!(
        "VR : GPU {} ({:?})",
        exposed.info.name,
        exposed.info.backend
    );

    let features = wgpu::Features::empty();
    let limits = exposed.capabilities.limits.clone();
    let device_exts = exposed.adapter.required_device_extensions(features);
    let device_ext_ptrs: Vec<*const c_char> = device_exts.iter().map(|e| e.as_ptr()).collect();
    let mut phd_features = exposed
        .adapter
        .physical_device_features(&device_exts, features);
    let priorities = [1.0_f32];
    let queue_ci = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family)
        .queue_priorities(&priorities)];
    let device_ci = phd_features.add_to_device_create(
        vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_ci)
            .enabled_extension_names(&device_ext_ptrs),
    );
    let raw_device = unsafe {
        instance.create_vulkan_device(
            system,
            get_proc,
            physical.as_raw() as _,
            &device_ci as *const _ as *const _,
        )
    }
    .map_err(err("xrCreateVulkanDeviceKHR"))?
    .map_err(|r| format!("vkCreateDevice : {:?}", vk::Result::from_raw(r)))?;
    let ash_device = unsafe {
        ash::Device::load(
            ash_instance.fp_v1_0(),
            vk::Device::from_raw(raw_device as _),
        )
    };

    let open = unsafe {
        exposed.adapter.device_from_raw(
            ash_device,
            Some(Box::new(|| ())),
            &device_exts,
            features,
            &limits,
            &wgpu::MemoryHints::Performance,
            queue_family,
            0,
        )
    }
    .map_err(err("import du device Vulkan dans wgpu"))?;

    let wgpu_instance = unsafe { wgpu::Instance::from_hal::<hal::api::Vulkan>(hal_instance) };
    let adapter = unsafe { wgpu_instance.create_adapter_from_hal(exposed) };
    let (device, queue) = unsafe {
        adapter.create_device_from_hal(
            open,
            &wgpu::DeviceDescriptor {
                label: Some("xr-device"),
                required_features: features,
                required_limits: limits,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            },
        )
    }
    .map_err(err("device wgpu"))?;

    Ok(Gpu {
        device,
        queue,
        session_info: xr::vulkan::SessionCreateInfo {
            instance: raw_instance,
            physical_device: physical.as_raw() as _,
            device: raw_device,
            queue_family_index: queue_family,
            queue_index: 0,
        },
    })
}

/// sRGB obligatoire : le compositeur du Quest traite la swapchain comme sRGB, un
/// format UNORM donnerait une image délavée.
fn pick_swapchain_format(formats: &[u32]) -> Result<(vk::Format, wgpu::TextureFormat), String> {
    [
        (
            vk::Format::R8G8B8A8_SRGB,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ),
        (
            vk::Format::B8G8R8A8_SRGB,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ),
    ]
    .into_iter()
    .find(|(vk_fmt, _)| formats.contains(&(vk_fmt.as_raw() as u32)))
    .ok_or_else(|| format!("aucun format sRGB dans la swapchain XR ({formats:?})"))
}

/// Enveloppe une image de swapchain XR en texture wgpu à 2 couches et renvoie une
/// vue par œil.
///
/// # Safety
/// `raw` doit être une image de la swapchain de `device`, vivante tant que les vues
/// renvoyées le sont.
unsafe fn wrap_swapchain_image(
    device: &wgpu::Device,
    raw: u64,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> [wgpu::TextureView; 2] {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: VIEW_COUNT,
    };
    let hal_texture = {
        let hal_device =
            unsafe { device.as_hal::<hal::api::Vulkan>() }.expect("device wgpu adossé à Vulkan");
        unsafe {
            hal_device.texture_from_raw(
                vk::Image::from_raw(raw),
                &hal::TextureDescriptor {
                    label: Some("xr-swapchain"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUses::COLOR_TARGET,
                    memory_flags: hal::MemoryFlags::empty(),
                    view_formats: vec![],
                },
                // L'image appartient au runtime XR : wgpu ne doit pas la détruire.
                Some(Box::new(|| ())),
                hal::vulkan::TextureMemory::External,
            )
        }
    };
    let texture = unsafe {
        device.create_texture_from_hal::<hal::api::Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("xr-swapchain"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
        )
    };
    [0, 1].map(|layer| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("xr-eye"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    })
}

// --- Scène de test : cubes éclairés + sol ------------------------------------------

const SHADER: &str = r#"
struct Eye { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> eye: Eye;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>, @builtin(instance_index) i: u32) -> Out {
    // (centre.xyz, demi-taille) — 5 cubes de 30 cm devant le joueur, 1 dalle de sol.
    var placements = array<vec4<f32>, 6>(
        vec4<f32>(0.0, 1.3, -1.2, 0.15),
        vec4<f32>(-0.6, 1.0, -1.5, 0.15),
        vec4<f32>(0.6, 1.0, -1.5, 0.15),
        vec4<f32>(-1.2, 1.5, -2.5, 0.15),
        vec4<f32>(1.2, 1.5, -2.5, 0.15),
        vec4<f32>(0.0, -0.01, 0.0, 3.0),
    );
    var colors = array<vec3<f32>, 6>(
        vec3<f32>(0.95, 0.55, 0.15),
        vec3<f32>(0.25, 0.6, 0.95),
        vec3<f32>(0.3, 0.85, 0.45),
        vec3<f32>(0.9, 0.3, 0.4),
        vec3<f32>(0.85, 0.8, 0.3),
        vec3<f32>(0.35, 0.38, 0.42),
    );
    let pl = placements[i];
    var scale = vec3<f32>(pl.w);
    if (i == 5u) { scale.y = 0.01; }
    let world = pl.xyz + p * scale;
    let light = normalize(vec3<f32>(0.4, 1.0, 0.3));
    var out: Out;
    out.pos = eye.view_proj * vec4<f32>(world, 1.0);
    out.color = colors[i] * (0.35 + 0.65 * max(dot(n, light), 0.0));
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;

struct CubeScene {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    eye_buffers: [wgpu::Buffer; 2],
    eye_groups: [wgpu::BindGroup; 2],
    depth: wgpu::TextureView,
}

impl CubeScene {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        use wgpu::util::DeviceExt as _;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("xr-hello"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xr-eye"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let eye_buffers = [0, 1].map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("xr-eye"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let eye_groups = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("xr-eye"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: eye_buffers[i].as_entire_binding(),
                }],
            })
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xr-hello"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("xr-hello"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                }],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("xr-cube"),
            contents: bytemuck::cast_slice(&cube_vertices()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("xr-depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Self {
            pipeline,
            vertices,
            eye_buffers,
            eye_groups,
            depth,
        }
    }

    /// Une passe par œil (le multiview viendra en phase 2).
    fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        targets: &[wgpu::TextureView; 2],
        view_projs: [Mat4; 2],
    ) {
        for (buffer, vp) in self.eye_buffers.iter().zip(view_projs) {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&vp.to_cols_array()));
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        for (target, group) in targets.iter().zip(&self.eye_groups) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("xr-eye"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.08,
                            b: 0.14,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..36, 0..6);
        }
        queue.submit([encoder.finish()]);
    }
}

/// Cube unité (−1..1) : 36 sommets (position, normale), faces sortantes en CCW.
fn cube_vertices() -> Vec<[f32; 6]> {
    // (normale, « haut » de la face) ; la « droite » s'en déduit : `right = up × n`
    // donne `right × up = n`, donc l'ordre (−,−) → (+,−) → (+,+) est CCW vu de dehors.
    let faces: [(Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::Y),
        (Vec3::NEG_X, Vec3::Y),
        (Vec3::Y, Vec3::Z),
        (Vec3::NEG_Y, Vec3::Z),
        (Vec3::Z, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y),
    ];
    let mut out = Vec::with_capacity(36);
    for (n, up) in faces {
        let right = up.cross(n);
        let c = |a: f32, b: f32| {
            let p = n + right * a + up * b;
            [p.x, p.y, p.z, n.x, n.y, n.z]
        };
        out.extend([
            c(-1., -1.),
            c(1., -1.),
            c(1., 1.),
            c(-1., -1.),
            c(1., 1.),
            c(-1., 1.),
        ]);
    }
    out
}
