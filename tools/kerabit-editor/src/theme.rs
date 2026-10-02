//! Optimistic chrome for the Kerabit editor.
//!
//! One warm family: deep cocoa panels, paper ink, a hopeful gold accent, and
//! coral only for press / warn. The 3D viewport stays a quiet rectangle.

use egui::{Color32, CornerRadius, RichText, Stroke, Ui};

/// Central palette. Prefer these over one-off hex values in widgets.
pub struct Palette;

impl Palette {
    /// Warm paper.
    pub const INK: Color32 = Color32::from_rgb(0xff, 0xf6, 0xea);
    /// Dark text on the gold active fill.
    pub const INK_ON_SUN: Color32 = Color32::from_rgb(0x2c, 0x16, 0x08);
    /// Secondary labels. Warm, still readable.
    pub const SAND: Color32 = Color32::from_rgb(0xf0, 0xc9, 0xa0);
    /// Hopeful gold. The only primary accent.
    pub const SUN: Color32 = Color32::from_rgb(0xff, 0xc2, 0x3d);
    /// Warm press / warn. Not a second theme.
    pub const CORAL: Color32 = Color32::from_rgb(0xff, 0x6a, 0x3d);
    /// Clear failure. Kept distinct from coral so errors stay serious.
    pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x4d, 0x5a);
    /// Non-primary multi-select marker (functional, not decoration).
    pub const MARKER: Color32 = Color32::from_rgb(0x8e, 0xc8, 0xff);
    pub const BG: Color32 = Color32::from_rgb(0x1c, 0x14, 0x10);
    pub const PANEL: Color32 = Color32::from_rgb(0x2a, 0x1c, 0x16);
    pub const RAISED: Color32 = Color32::from_rgb(0x3c, 0x28, 0x1c);
    pub const HOVER: Color32 = Color32::from_rgb(0x54, 0x36, 0x22);
    pub const SUNKEN: Color32 = Color32::from_rgb(0x12, 0x0e, 0x0b);
}

/// Apply the editor theme. Call once at startup.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Palette::PANEL;
    visuals.panel_fill = Palette::BG;
    visuals.extreme_bg_color = Palette::SUNKEN;
    visuals.faint_bg_color = Palette::RAISED;
    visuals.code_bg_color = Palette::SUNKEN;
    visuals.override_text_color = Some(Palette::INK);
    visuals.hyperlink_color = Palette::SUN;
    visuals.warn_fg_color = Palette::CORAL;
    visuals.error_fg_color = Palette::ERROR;
    visuals.window_stroke = stroke(1.0, with_alpha(Palette::SUN, 80));
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.selection.bg_fill = with_alpha(Palette::SUN, 56);
    visuals.selection.stroke = stroke(1.0, Palette::SUN);
    visuals.window_shadow = shadow(18, with_alpha(Palette::CORAL, 36));
    visuals.popup_shadow = shadow(22, Color32::from_black_alpha(120));
    visuals.slider_trailing_fill = true;
    visuals.indent_has_left_vline = true;

    let base = visuals.widgets.noninteractive;
    visuals.widgets.noninteractive = widget(base, Palette::BG, Palette::SAND, Stroke::NONE, 0.0);
    visuals.widgets.inactive = widget(
        base,
        Palette::RAISED,
        Palette::INK,
        stroke(1.0, with_alpha(Palette::CORAL, 55)),
        0.0,
    );
    visuals.widgets.hovered = widget(
        base,
        Palette::HOVER,
        Palette::SUN,
        stroke(1.5, Palette::CORAL),
        1.0,
    );
    visuals.widgets.active = widget(
        base,
        Palette::SUN,
        Palette::INK_ON_SUN,
        stroke(1.0, Palette::SUN),
        0.0,
    );
    visuals.widgets.open = widget(
        base,
        Palette::RAISED,
        Palette::SUN,
        stroke(1.0, with_alpha(Palette::SUN, 160)),
        0.0,
    );

    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        // Short enough that hover / press stay responsive.
        style.animation_time = 0.12;
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        if let Some(heading) = style.text_styles.get_mut(&egui::TextStyle::Heading) {
            heading.size = 18.0;
        }
    });
}

fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

fn widget(
    mut base: egui::style::WidgetVisuals,
    fill: Color32,
    fg: Color32,
    bg_stroke: Stroke,
    expansion: f32,
) -> egui::style::WidgetVisuals {
    base.bg_fill = fill;
    base.weak_bg_fill = fill.gamma_multiply(0.65);
    base.bg_stroke = bg_stroke;
    base.fg_stroke = stroke(1.0, fg);
    base.corner_radius = CornerRadius::same(6);
    base.expansion = expansion;
    base
}

fn shadow(blur: u8, color: Color32) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 8],
        blur,
        spread: 0,
        color,
    }
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Panel title: gold word plus a single rule. Same slot as a heading.
pub fn title(ui: &mut Ui, text: &str) {
    let resp = ui.label(RichText::new(text).heading().strong().color(Palette::SUN));
    let y = resp.rect.bottom() + 2.0;
    ui.painter().hline(
        ui.max_rect().x_range(),
        y,
        stroke(2.0, with_alpha(Palette::SUN, 210)),
    );
    ui.add_space(4.0);
}

/// Title that stays on a toolbar row (no full-width rule).
pub fn inline_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).heading().strong().color(Palette::SUN));
}

/// Inspector group label. One step down from [`title`].
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).strong().color(Palette::SAND));
}

/// Thin gold rule along the top or bottom edge of the current panel.
pub fn edge_rule(ui: &mut Ui, at_bottom: bool) {
    let rect = ui.max_rect();
    let y = if at_bottom { rect.bottom() } else { rect.top() };
    ui.painter()
        .hline(rect.x_range(), y, stroke(2.0, Palette::SUN));
}

/// Linear mix. `t` of 0 returns `from`.
pub fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgb(
        ch(from.r(), to.r()),
        ch(from.g(), to.g()),
        ch(from.b(), to.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_keeps_error_distinct_from_accent() {
        assert_ne!(Palette::SUN, Palette::ERROR);
        assert_ne!(Palette::CORAL, Palette::ERROR);
        assert_ne!(Palette::SUN, Palette::INK);
    }

    #[test]
    fn theme_applies_without_a_window() {
        let ctx = egui::Context::default();
        apply(&ctx);
        assert_eq!(ctx.style().visuals.hyperlink_color, Palette::SUN);
        assert_eq!(ctx.style().visuals.error_fg_color, Palette::ERROR);
        assert!(ctx.style().animation_time > 0.0 && ctx.style().animation_time < 0.25);
        assert!(ctx.style().visuals.widgets.hovered.expansion > 0.0);
        assert_eq!(ctx.style().visuals.widgets.active.expansion, 0.0);
    }
}
