//! Kerabit level editor — egui shell + 3D viewport for `.kerabit.json` scenes.
//!
//! Round-trips through [`kerabit::Scene::load`] / [`kerabit::Scene::save`].
//! No egui types leak into the public game API.

mod app;
mod gizmo;
mod orbit;
mod validation;
mod viewport;

use app::EditorApp;
use viewport::ViewportGpu;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Kerabit Editor"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Kerabit Editor",
        options,
        Box::new(|cc| {
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                let gpu = ViewportGpu::new(&rs.device, &rs.queue, rs.target_format);
                rs.renderer.write().callback_resources.insert(gpu);
            }
            Ok(Box::new(EditorApp::new()))
        }),
    )
}
