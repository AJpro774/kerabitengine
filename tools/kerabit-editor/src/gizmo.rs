//! Translate / rotate / scale gizmos (screen-space handles over the 3D view).

use egui::{Color32, Pos2, Sense, Stroke, Ui};
use kerabit::{Quat, Vec3};

use kerabit_render::{Camera, Ray};

use crate::orbit::OrbitCamera;

const SNAP: f32 = 0.5;
const HANDLE_PX: f32 = 10.0;
const AXIS_LEN: f32 = 1.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

impl GizmoMode {
    pub fn label(self) -> &'static str {
        match self {
            GizmoMode::Translate => "Move (W)",
            GizmoMode::Rotate => "Rotate (E)",
            GizmoMode::Scale => "Scale (R)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn dir(self) -> Vec3 {
        match self {
            Axis::X => Vec3::X,
            Axis::Y => Vec3::Y,
            Axis::Z => Vec3::Z,
        }
    }

    fn color(self) -> Color32 {
        match self {
            Axis::X => Color32::from_rgb(220, 60, 60),
            Axis::Y => Color32::from_rgb(60, 200, 80),
            Axis::Z => Color32::from_rgb(60, 120, 230),
        }
    }
}

#[derive(Default)]
pub struct GizmoState {
    pub mode: GizmoMode,
    pub snap: bool,
    drag_axis: Option<Axis>,
    drag_start_mouse: Pos2,
    /// Translate: start position. Rotate: start quat. Scale: start scale.
    drag_start_at: Vec3,
    drag_start_rot: Quat,
    drag_start_scale: Vec3,
}

impl GizmoState {
    /// True while an axis handle drag is active.
    pub fn is_dragging(&self) -> bool {
        self.drag_axis.is_some()
    }
}

impl Default for GizmoMode {
    fn default() -> Self {
        Self::Translate
    }
}

pub struct GizmoEdit {
    pub at: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

/// Apply drag edits (no drawing). Returns true when the transform changed.
pub fn interact(
    ui: &mut Ui,
    rect: egui::Rect,
    response: &egui::Response,
    orbit: &OrbitCamera,
    state: &mut GizmoState,
    edit: &mut GizmoEdit,
) -> bool {
    let mut cam = orbit.to_camera();
    cam.set_aspect(rect.width() / rect.height().max(1.0));

    let Some(_origin) = project(edit.at, &cam, rect) else {
        return false;
    };

    let axes = [Axis::X, Axis::Y, Axis::Z];
    let mut tip_pos = [None; 3];
    for (i, axis) in axes.iter().enumerate() {
        let world_tip = edit.at + axis.dir() * AXIS_LEN * gizmo_world_scale(orbit);
        tip_pos[i] = project(world_tip, &cam, rect);
    }

    let mut changed = false;

    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pointer) = response.interact_pointer_pos() {
            state.drag_axis = None;
            let mut best = HANDLE_PX + 2.0;
            for (i, axis) in axes.iter().enumerate() {
                if let Some(tip) = tip_pos[i] {
                    let d = tip.distance(pointer);
                    if d < best {
                        best = d;
                        state.drag_axis = Some(*axis);
                    }
                }
            }
            if state.drag_axis.is_some() {
                state.drag_start_mouse = pointer;
                state.drag_start_at = edit.at;
                state.drag_start_rot = edit.rotation;
                state.drag_start_scale = edit.scale;
            }
        }
    }

    if response.dragged_by(egui::PointerButton::Primary) {
        if let (Some(axis), Some(pointer)) = (state.drag_axis, response.interact_pointer_pos()) {
            changed |= apply_drag(&cam, rect, orbit, state, axis, pointer, edit);
        }
    }

    if response.drag_stopped() {
        state.drag_axis = None;
    }

    let _ = response.interact(Sense::click());
    let _ = ui;
    changed
}

/// Draw gizmo handles on top of the 3D blit (call after the paint callback).
pub fn paint(ui: &mut Ui, rect: egui::Rect, orbit: &OrbitCamera, state: &GizmoState, edit: &GizmoEdit) {
    let mut cam = orbit.to_camera();
    cam.set_aspect(rect.width() / rect.height().max(1.0));
    let Some(origin) = project(edit.at, &cam, rect) else {
        return;
    };

    let painter = ui.painter_at(rect);
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let world_tip = edit.at + axis.dir() * AXIS_LEN * gizmo_world_scale(orbit);
        let Some(tip) = project(world_tip, &cam, rect) else {
            continue;
        };
        let stroke = Stroke::new(2.5_f32, axis.color());
        painter.line_segment([origin, tip], stroke);
        match state.mode {
            GizmoMode::Translate => {
                painter.circle_filled(tip, HANDLE_PX * 0.55, axis.color());
            }
            GizmoMode::Scale => {
                painter.rect_filled(
                    egui::Rect::from_center_size(tip, egui::vec2(HANDLE_PX, HANDLE_PX)),
                    2.0,
                    axis.color(),
                );
            }
            GizmoMode::Rotate => {
                painter.circle_stroke(tip, HANDLE_PX * 0.7, Stroke::new(2.0_f32, axis.color()));
            }
        }
    }
    painter.circle_filled(origin, 4.0, Color32::WHITE);
}

/// True if the primary pointer is over a gizmo handle (for pick suppression).
pub fn pointer_on_handle(
    pointer: Pos2,
    rect: egui::Rect,
    orbit: &OrbitCamera,
    edit: &GizmoEdit,
) -> bool {
    let mut cam = orbit.to_camera();
    cam.set_aspect(rect.width() / rect.height().max(1.0));
    let Some(origin) = project(edit.at, &cam, rect) else {
        return false;
    };
    if origin.distance(pointer) <= HANDLE_PX {
        return true;
    }
    let scale = gizmo_world_scale(orbit);
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let tip = edit.at + axis.dir() * AXIS_LEN * scale;
        if let Some(p) = project(tip, &cam, rect) {
            if p.distance(pointer) <= HANDLE_PX {
                return true;
            }
        }
    }
    false
}

fn apply_drag(
    cam: &Camera,
    rect: egui::Rect,
    orbit: &OrbitCamera,
    state: &GizmoState,
    axis: Axis,
    pointer: Pos2,
    edit: &mut GizmoEdit,
) -> bool {
    let delta = pointer - state.drag_start_mouse;
    match state.mode {
        GizmoMode::Translate => {
            let Some(axis_screen) = axis_screen_dir(cam, rect, edit.at, axis, orbit) else {
                return false;
            };
            let along = delta.x * axis_screen.x + delta.y * axis_screen.y;
            let world_per_px = gizmo_world_scale(orbit) * 0.02;
            let mut offset = axis.dir() * along * world_per_px;
            if state.snap {
                offset = snap_vec(offset);
            }
            let next = state.drag_start_at + offset;
            if next != edit.at {
                edit.at = next;
                return true;
            }
        }
        GizmoMode::Rotate => {
            let angle = delta.x * 0.01;
            let mut q = Quat::from_axis_angle(axis.dir(), angle);
            if state.snap {
                let step = 15.0_f32.to_radians();
                let snapped = (angle / step).round() * step;
                q = Quat::from_axis_angle(axis.dir(), snapped);
            }
            let next = (q * state.drag_start_rot).normalize();
            if (next.x - edit.rotation.x).abs()
                + (next.y - edit.rotation.y).abs()
                + (next.z - edit.rotation.z).abs()
                + (next.w - edit.rotation.w).abs()
                > 1e-5
            {
                edit.rotation = next;
                return true;
            }
        }
        GizmoMode::Scale => {
            let Some(axis_screen) = axis_screen_dir(cam, rect, edit.at, axis, orbit) else {
                return false;
            };
            let along = delta.x * axis_screen.x + delta.y * axis_screen.y;
            let mut s = state.drag_start_scale;
            let add = along * 0.01;
            match axis {
                Axis::X => s.x = (state.drag_start_scale.x + add).max(0.05),
                Axis::Y => s.y = (state.drag_start_scale.y + add).max(0.05),
                Axis::Z => s.z = (state.drag_start_scale.z + add).max(0.05),
            }
            if state.snap {
                s = snap_vec(s).max(Vec3::splat(0.05));
            }
            if s != edit.scale {
                edit.scale = s;
                return true;
            }
        }
    }
    false
}

fn snap_vec(v: Vec3) -> Vec3 {
    Vec3::new(
        (v.x / SNAP).round() * SNAP,
        (v.y / SNAP).round() * SNAP,
        (v.z / SNAP).round() * SNAP,
    )
}

fn gizmo_world_scale(orbit: &OrbitCamera) -> f32 {
    (orbit.distance * 0.12).clamp(0.35, 8.0)
}

fn axis_screen_dir(
    cam: &Camera,
    rect: egui::Rect,
    origin: Vec3,
    axis: Axis,
    orbit: &OrbitCamera,
) -> Option<egui::Vec2> {
    let a = project(origin, cam, rect)?;
    let b = project(origin + axis.dir() * gizmo_world_scale(orbit), cam, rect)?;
    let d = b - a;
    let len = (d.x * d.x + d.y * d.y).sqrt().max(1e-3);
    Some(egui::vec2(d.x / len, d.y / len))
}

fn project(world: Vec3, cam: &Camera, rect: egui::Rect) -> Option<Pos2> {
    let clip = cam.view_proj() * world.extend(1.0);
    if clip.w <= 1e-5 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if !ndc.x.is_finite() || !ndc.y.is_finite() {
        return None;
    }
    Some(Pos2::new(
        rect.center().x + ndc.x * rect.width() * 0.5,
        rect.center().y - ndc.y * rect.height() * 0.5,
    ))
}

/// Build a picking ray for a pointer inside `rect`.
pub fn picking_ray(orbit: &OrbitCamera, rect: egui::Rect, pointer: Pos2) -> Ray {
    let mut cam = orbit.to_camera();
    cam.set_aspect(rect.width() / rect.height().max(1.0));
    let (ndc_x, ndc_y) = kerabit_render::pointer_to_ndc(
        [pointer.x, pointer.y],
        [rect.min.x, rect.min.y],
        [rect.width(), rect.height()],
    );
    kerabit_render::ray_from_ndc(&cam, ndc_x, ndc_y)
}
