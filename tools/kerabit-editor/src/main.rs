//! Kerabit level editor — egui shell + 3D viewport for `.kerabit.json` scenes.
//!
//! Round-trips through [`kerabit::Scene::load`] / [`kerabit::Scene::save`].
//! No egui types leak into the public game API.
//!
//! Play mode: `kerabit-editor --play <path.kerabit.json>` opens the scene via
//! the public Kerabit API (`Scene` → `Kerabit::run`). The editor shell launches
//! that as a child of the same binary (no sidecar).

mod app;
mod gizmo;
mod orbit;
mod play;
mod validation;
mod viewport;

use std::path::PathBuf;

use app::EditorApp;
use viewport::ViewportGpu;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(path) = parse_play_arg(&args) {
        play::run(&path);
        return Ok(());
    }

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

fn parse_play_arg(args: &[String]) -> Option<PathBuf> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--play" {
            return iter.next().map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--play=") {
            return Some(PathBuf::from(path));
        }
    }
    None
}
