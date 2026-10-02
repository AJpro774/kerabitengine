//! Shared field editors for the inspector and the environment panel.

use egui::Ui;
use kerabit::Color;

pub(super) fn color_to_rgb(c: Color) -> [f32; 3] {
    [c.r, c.g, c.b]
}

pub(super) fn vec3_drag(ui: &mut Ui, label: &str, v: &mut [f32; 3]) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut v[0]).speed(0.05).prefix("X "))
            .changed()
            || ui
                .add(egui::DragValue::new(&mut v[1]).speed(0.05).prefix("Y "))
                .changed()
            || ui
                .add(egui::DragValue::new(&mut v[2]).speed(0.05).prefix("Z "))
                .changed()
    })
    .inner
}
