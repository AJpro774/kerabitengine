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
mod selection;
mod settings;
mod undo;
mod validation;
mod viewport;

use std::path::PathBuf;

use app::EditorApp;
use viewport::ViewportGpu;

/// Site-aligned dark chrome: warm near-black panels, acid-lime accent.
fn apply_kerabit_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    let bg0 = egui::Color32::from_rgb(0x1a, 0x17, 0x14);
    let bg1 = egui::Color32::from_rgb(0x2a, 0x22, 0x1c);
    let ink = egui::Color32::from_rgb(0xf3, 0xeb, 0xe1);
    let muted = egui::Color32::from_rgb(0xb7, 0xa9, 0x9a);
    let accent = egui::Color32::from_rgb(0xe8, 0xff, 0x4a);
    let accent_ink = egui::Color32::from_rgb(0x1a, 0x17, 0x14);

    visuals.window_fill = bg0;
    visuals.panel_fill = bg0;
    visuals.extreme_bg_color = egui::Color32::from_rgb(0x0f, 0x0d, 0x0b);
    visuals.faint_bg_color = bg1;
    visuals.override_text_color = Some(ink);
    visuals.widgets.noninteractive.bg_fill = bg0;
    visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0_f32, muted);
    visuals.widgets.inactive.bg_fill = bg1;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x3a, 0x30, 0x26);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.widgets.active.bg_fill = accent;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, accent_ink);
    visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(0xe8, 0xff, 0x4a, 55);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.hyperlink_color = accent;
    visuals.warn_fg_color = egui::Color32::from_rgb(220, 160, 60);
    visuals.error_fg_color = egui::Color32::from_rgb(220, 80, 80);
    ctx.set_visuals(visuals);
}

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
            apply_kerabit_visuals(&cc.egui_ctx);
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
