//! Interface VR (phase 5 de `docs/roadmapExportVRQuest24septembre.md`) : le rayon
//! de la manette converti en pointeur egui (`xr::ui::VrUi::paint`) clique
//! réellement un bouton — viser, appuyer, relâcher sur trois images, comme au
//! casque. Sur un vrai device wgpu headless ; **sauté** sans GPU (CI Linux),
//! même règle que `golden_render.rs`.

use motor3derust::xr::ui::{MENU, Pointer, VrUi};

fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

#[test]
fn aiming_and_pulling_the_trigger_clicks_a_menu_button() {
    let Some((device, queue)) = headless_device() else {
        eprintln!("vr_menu : pas de GPU headless — test sauté, pas en échec.");
        return;
    };
    let mut ui = VrUi::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let center = ui.panel_px(MENU) * 0.5;
    let clicks = std::cell::Cell::new(0);
    let frame = |ui: &mut VrUi, pos: Option<glam::Vec2>, pressed: bool| {
        ui.paint(
            &device,
            &queue,
            MENU,
            Pointer {
                pos_px: pos,
                pressed,
            },
            |root| {
                // Un seul gros bouton qui remplit le panneau.
                let size = root.available_size();
                if root
                    .add_sized(size, egui::Button::new("Reprendre"))
                    .clicked()
                {
                    clicks.set(clicks.get() + 1);
                }
            },
        );
    };
    frame(&mut ui, Some(center), false); // viser
    frame(&mut ui, Some(center), true); // gâchette enfoncée
    frame(&mut ui, Some(center), false); // relâchée → clic
    assert_eq!(clicks.get(), 1, "viser + appuyer + relâcher = un clic");
    frame(&mut ui, None, false); // rayon hors du panneau
    frame(&mut ui, None, true);
    frame(&mut ui, None, false);
    assert_eq!(clicks.get(), 1, "gâchette hors du panneau : aucun clic");
}
