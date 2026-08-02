//! 3D viewport: offscreen kerabit-render lit pass + egui blit, picking, gizmos.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::Ui;
use kerabit::{Color, Light, Mat4, Quat, Scene, SceneEntity, SceneMesh, Vec3};
use kerabit_render::{
    pick_closest, Aabb, DrawItem, Mesh, MeshId, OffscreenLitRenderer, TextureId,
};

use crate::gizmo::{self, GizmoEdit, GizmoMode, GizmoState};
use crate::orbit::OrbitCamera;
use crate::selection::Selection;
use crate::undo::UndoStack;

/// Shared GPU resources living in egui-wgpu `callback_resources`.
pub struct ViewportGpu {
    pub renderer: OffscreenLitRenderer,
    mesh_ids: HashMap<MeshKey, MeshId>,
    texture_ids: HashMap<PathBuf, TextureId>,
}

impl ViewportGpu {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            renderer: OffscreenLitRenderer::new(device, queue, target_format, Color::BLACK),
            mesh_ids: HashMap::new(),
            texture_ids: HashMap::new(),
        }
    }

    fn mesh_id(
        &mut self,
        device: &wgpu::Device,
        key: &MeshKey,
        scene_dir: Option<&Path>,
    ) -> Option<MeshId> {
        if let Some(id) = self.mesh_ids.get(key) {
            return Some(*id);
        }
        let mesh = mesh_from_key(key, scene_dir)?;
        let id = self.renderer.upload_mesh(device, &mesh);
        self.mesh_ids.insert(key.clone(), id);
        Some(id)
    }

    fn texture_id(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &Path,
        scene_dir: Option<&Path>,
    ) -> Option<TextureId> {
        let abs = resolve_asset_path(path, scene_dir);
        if let Some(id) = self.texture_ids.get(&abs) {
            return Some(*id);
        }
        let tex = kerabit_assets::Texture::load_png(&abs).ok()?;
        let id = self
            .renderer
            .upload_texture_rgba8(device, queue, tex.width, tex.height, &tex.rgba);
        self.texture_ids.insert(abs, id);
        Some(id)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum MeshKey {
    Cube,
    Plane { bits: u32 },
    Path(PathBuf),
}

struct ViewportPaint {
    shared: Arc<Mutex<ViewportFrame>>,
    scene_dir: Option<PathBuf>,
}

struct ViewportFrame {
    width: u32,
    height: u32,
    clear: Color,
    ambient: Color,
    camera: kerabit_render::Camera,
    light: Light,
    draws: Vec<PreparedDraw>,
}

#[derive(Clone)]
struct PreparedDraw {
    key: MeshKey,
    model: Mat4,
    albedo: Color,
    roughness: f32,
    texture: Option<PathBuf>,
}

/// Editor-owned viewport controls + shared frame buffer for the GPU callback.
pub struct Viewport {
    pub orbit: OrbitCamera,
    pub gizmo: GizmoState,
    frame: Arc<Mutex<ViewportFrame>>,
    place_cube: bool,
    /// Snapshot of snap settings last applied from the toolbar (for persistence).
    pub snap_dirty: bool,
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            orbit: OrbitCamera::default(),
            gizmo: GizmoState::default(),
            frame: Arc::new(Mutex::new(ViewportFrame {
                width: 1,
                height: 1,
                clear: Color::rgb(0.08, 0.09, 0.12),
                ambient: Color::rgb(0.15, 0.16, 0.18),
                camera: OrbitCamera::default().to_camera(),
                light: Light::sun(Vec3::new(-0.35, -1.0, -0.25)).intensity(1.2),
                draws: Vec::new(),
            })),
            place_cube: false,
            snap_dirty: false,
        }
    }

    pub fn apply_snap_settings(&mut self, enabled: bool, size: f32) {
        self.gizmo.snap = enabled;
        self.gizmo.snap_size = size.max(0.05);
    }

    fn ui_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Gizmo");
            for mode in [GizmoMode::Translate, GizmoMode::Rotate, GizmoMode::Scale] {
                ui.selectable_value(&mut self.gizmo.mode, mode, mode.label());
            }
            ui.separator();
            let prev_snap = self.gizmo.snap;
            let prev_size = self.gizmo.snap_size;
            ui.checkbox(&mut self.gizmo.snap, "Snap");
            ui.add(
                egui::DragValue::new(&mut self.gizmo.snap_size)
                    .speed(0.05)
                    .range(0.05..=10.0)
                    .prefix("size "),
            );
            if self.gizmo.snap != prev_snap || (self.gizmo.snap_size - prev_size).abs() > 1e-6 {
                self.snap_dirty = true;
            }
            ui.separator();
            ui.checkbox(&mut self.place_cube, "Place cube (click)");
            ui.separator();
            ui.weak("RMB orbit · MMB pan · scroll zoom · LMB pick · Shift+LMB multi");
        });
    }

    /// Central panel: render + interact. Mutates `scene` / `selection`.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        scene: &mut Scene,
        selection: &mut Selection,
        scene_dir: Option<&Path>,
        undo: &mut UndoStack,
        mark_dirty: &mut dyn FnMut(),
        status: &mut String,
    ) {
        self.ui_toolbar(ui);

        ui.input(|i| {
            if i.key_pressed(egui::Key::W) {
                self.gizmo.mode = GizmoMode::Translate;
            }
            if i.key_pressed(egui::Key::E) {
                self.gizmo.mode = GizmoMode::Rotate;
            }
            if i.key_pressed(egui::Key::R) {
                self.gizmo.mode = GizmoMode::Scale;
            }
        });

        egui::Frame::dark_canvas(ui.style()).show(ui, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

            if response.dragged_by(egui::PointerButton::Secondary) {
                let d = response.drag_delta();
                self.orbit.orbit(d.x, d.y);
            }
            if response.dragged_by(egui::PointerButton::Middle) {
                let d = response.drag_delta();
                self.orbit.pan(d.x, d.y);
            }
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll.abs() > 0.0 {
                    self.orbit.zoom(scroll * 0.01);
                }
            }

            let multi = ui.input(|i| i.modifiers.shift || i.modifiers.command);

            if let Some(i) = selection.primary() {
                if i < scene.entities.len() {
                    let mut edit = GizmoEdit {
                        at: scene.entities[i].at,
                        rotation: scene.entities[i].rotation,
                        scale: scene.entities[i].scale,
                    };
                    let was_dragging = self.gizmo.is_dragging();
                    if gizmo::interact(ui, rect, &response, &self.orbit, &mut self.gizmo, &mut edit)
                    {
                        if self.gizmo.drag_began_this_frame {
                            undo.push(scene);
                        }
                        let delta = edit.at - scene.entities[i].at;
                        scene.entities[i].at = edit.at;
                        scene.entities[i].rotation = edit.rotation;
                        scene.entities[i].scale = edit.scale;
                        // Translate multi-selection together.
                        if self.gizmo.mode == GizmoMode::Translate
                            && selection.len() > 1
                            && delta.length_squared() > 0.0
                        {
                            for &j in selection.as_slice() {
                                if j != i && j < scene.entities.len() {
                                    scene.entities[j].at += delta;
                                }
                            }
                        }
                        mark_dirty();
                    }
                    if was_dragging && !self.gizmo.is_dragging() {
                        undo.end_gesture();
                    }
                }
            }

            if response.clicked_by(egui::PointerButton::Primary) && !self.gizmo.is_dragging() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    let on_gizmo = selection
                        .primary()
                        .and_then(|i| scene.entities.get(i))
                        .map(|e| {
                            gizmo::pointer_on_handle(
                                pointer,
                                rect,
                                &self.orbit,
                                &GizmoEdit {
                                    at: e.at,
                                    rotation: e.rotation,
                                    scale: e.scale,
                                },
                            )
                        })
                        .unwrap_or(false);

                    if !on_gizmo {
                        let ray = gizmo::picking_ray(&self.orbit, rect, pointer);
                        if self.place_cube {
                            if let Some(hit) = kerabit_render::ray_plane_y(ray, 0.5) {
                                undo.push(scene);
                                let mut at = hit;
                                if self.gizmo.snap {
                                    at = gizmo::snap_position(at, self.gizmo.snap_size);
                                    at.y = 0.5;
                                } else {
                                    at.y = 0.5;
                                }
                                let name = unique_name(scene, "cube");
                                scene.entities.push(SceneEntity {
                                    name,
                                    tags: Vec::new(),
                                    mesh: SceneMesh::Cube,
                                    material: kerabit::SceneMaterial {
                                        color: Color::ORANGE,
                                        roughness: 0.5,
                                        metallic: 0.0,
                                        texture: None,
                                    },
                                    at,
                                    rotation: Quat::IDENTITY,
                                    scale: Vec3::ONE,
                                    parent: None,
                                    components: Default::default(),
                                    extras: Default::default(),
                                });
                                selection.set_one(scene.entities.len() - 1);
                                self.place_cube = false;
                                mark_dirty();
                                *status = "Placed cube".into();
                            }
                        } else {
                            let worlds = world_matrices(&scene.entities);
                            let mut candidates = Vec::new();
                            for (idx, e) in scene.entities.iter().enumerate() {
                                let mesh = resolve_mesh(&e.mesh, scene_dir);
                                let local = Aabb::from_mesh(&mesh);
                                let world = local.transformed(worlds[idx]);
                                candidates.push((idx, world));
                            }
                            if let Some((idx, _)) = pick_closest(ray, &candidates, 1_000.0) {
                                if multi {
                                    selection.toggle(idx);
                                } else {
                                    selection.set_one(idx);
                                }
                                let n = selection.len();
                                *status = if n > 1 {
                                    format!("Selected {n} entities")
                                } else {
                                    format!("Selected \"{}\"", scene.entities[idx].name)
                                };
                            } else if !multi {
                                selection.clear();
                            }
                        }
                    }
                }
            }

            let pp = ui.ctx().pixels_per_point();
            let width = (rect.width() * pp).round().max(1.0) as u32;
            let height = (rect.height() * pp).round().max(1.0) as u32;
            let draws = build_draws(scene, scene_dir);
            let light = Light::sun(scene.light.direction)
                .intensity(scene.light.intensity)
                .color(scene.light.color);
            if let Ok(mut frame) = self.frame.lock() {
                frame.width = width;
                frame.height = height;
                frame.clear = scene.clear_color;
                frame.ambient = scene.ambient;
                frame.camera = self.orbit.to_camera();
                frame.light = light;
                frame.draws = draws;
            }

            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportPaint {
                    shared: Arc::clone(&self.frame),
                    scene_dir: scene_dir.map(|p| p.to_path_buf()),
                },
            ));

            // Gizmos must paint after the blit callback so handles stay visible.
            if let Some(i) = selection.primary() {
                if let Some(e) = scene.entities.get(i) {
                    gizmo::paint(
                        ui,
                        rect,
                        &self.orbit,
                        &self.gizmo,
                        &GizmoEdit {
                            at: e.at,
                            rotation: e.rotation,
                            scale: e.scale,
                        },
                    );
                }
            }

            // Selection markers for multi-select (small rings at entity origins).
            if selection.len() > 1 {
                let mut cam = self.orbit.to_camera();
                cam.set_aspect(rect.width() / rect.height().max(1.0));
                let painter = ui.painter_at(rect);
                for &i in selection.as_slice() {
                    if let Some(e) = scene.entities.get(i) {
                        if let Some(p) = project_point(e.at, &cam, rect) {
                            let primary = selection.primary() == Some(i);
                            let color = if primary {
                                egui::Color32::from_rgb(255, 220, 80)
                            } else {
                                egui::Color32::from_rgb(120, 180, 255)
                            };
                            painter.circle_stroke(
                                p,
                                if primary { 7.0 } else { 5.0 },
                                egui::Stroke::new(1.5_f32, color),
                            );
                        }
                    }
                }
            }
        });
    }
}

fn project_point(
    world: Vec3,
    cam: &kerabit_render::Camera,
    rect: egui::Rect,
) -> Option<egui::Pos2> {
    let clip = cam.view_proj() * world.extend(1.0);
    if clip.w <= 1e-5 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if !ndc.x.is_finite() || !ndc.y.is_finite() {
        return None;
    }
    Some(egui::Pos2::new(
        rect.center().x + ndc.x * rect.width() * 0.5,
        rect.center().y - ndc.y * rect.height() * 0.5,
    ))
}

impl egui_wgpu::CallbackTrait for ViewportPaint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(gpu) = resources.get_mut::<ViewportGpu>() else {
            return Vec::new();
        };
        let Ok(frame) = self.shared.lock() else {
            return Vec::new();
        };

        gpu.renderer.resize(device, frame.width, frame.height);
        gpu.renderer.clear_color = frame.clear;

        let scene_dir = self.scene_dir.as_deref();
        let mut draws = Vec::with_capacity(frame.draws.len());
        for d in &frame.draws {
            let Some(mesh_id) = gpu.mesh_id(device, &d.key, scene_dir) else {
                continue;
            };
            let mut item = DrawItem::new(mesh_id, d.model, d.albedo).with_roughness(d.roughness);
            if let Some(tex_path) = &d.texture {
                if let Some(tid) = gpu.texture_id(device, queue, tex_path, scene_dir) {
                    item = item.with_texture(tid);
                }
            }
            draws.push(item);
        }

        let mut camera = frame.camera.clone();
        gpu.renderer.encode_lit(
            device,
            queue,
            encoder,
            &mut camera,
            &frame.light,
            frame.ambient,
            &draws,
        );
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(gpu) = resources.get::<ViewportGpu>() {
            gpu.renderer.blit_into(render_pass);
        }
    }
}

fn build_draws(scene: &Scene, scene_dir: Option<&Path>) -> Vec<PreparedDraw> {
    let worlds = world_matrices(&scene.entities);
    let mut out = Vec::with_capacity(scene.entities.len());
    for (i, e) in scene.entities.iter().enumerate() {
        let key = mesh_key(&e.mesh, scene_dir);
        let texture = e
            .material
            .texture
            .as_ref()
            .map(|p| resolve_asset_path(p, scene_dir));
        out.push(PreparedDraw {
            key,
            model: worlds[i],
            albedo: e.material.color,
            roughness: e.material.roughness,
            texture,
        });
    }
    out
}

fn mesh_key(mesh: &SceneMesh, scene_dir: Option<&Path>) -> MeshKey {
    match mesh {
        SceneMesh::Cube => MeshKey::Cube,
        SceneMesh::Plane { size } => MeshKey::Plane {
            bits: size.to_bits(),
        },
        SceneMesh::Obj { path } | SceneMesh::Gltf { path } => {
            MeshKey::Path(resolve_asset_path(path, scene_dir))
        }
    }
}

fn mesh_from_key(key: &MeshKey, _scene_dir: Option<&Path>) -> Option<Mesh> {
    match key {
        MeshKey::Cube => Some(Mesh::cube()),
        MeshKey::Plane { bits } => Some(Mesh::plane(f32::from_bits(*bits))),
        MeshKey::Path(path) => {
            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("gltf") || e.eq_ignore_ascii_case("glb"))
            {
                kerabit_assets::load_gltf(path).ok().map(|g| g.mesh)
            } else {
                kerabit_assets::load_obj(path).ok()
            }
        }
    }
}

fn resolve_mesh(mesh: &SceneMesh, scene_dir: Option<&Path>) -> Mesh {
    match mesh {
        SceneMesh::Cube => Mesh::cube(),
        SceneMesh::Plane { size } => Mesh::plane(*size),
        SceneMesh::Obj { path } => {
            let p = resolve_asset_path(path, scene_dir);
            kerabit_assets::load_obj(&p).unwrap_or_else(|_| Mesh::cube())
        }
        SceneMesh::Gltf { path } => {
            let p = resolve_asset_path(path, scene_dir);
            kerabit_assets::load_gltf(&p)
                .map(|g| g.mesh)
                .unwrap_or_else(|_| Mesh::cube())
        }
    }
}

fn resolve_asset_path(path: &Path, scene_dir: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    if let Some(dir) = scene_dir {
        let joined = dir.join(path);
        if joined.exists() {
            return joined;
        }
    }
    path.to_path_buf()
}

fn unique_name(scene: &Scene, base: &str) -> String {
    let existing: Vec<&str> = scene.entities.iter().map(|e| e.name.as_str()).collect();
    if !existing.contains(&base) {
        return base.to_string();
    }
    for i in 2..10_000 {
        let candidate = format!("{base}_{i}");
        if !existing.contains(&candidate.as_str()) {
            return candidate;
        }
    }
    format!("{base}_{}", existing.len() + 1)
}

/// Parent-aware world matrices matching kerabit-world propagation (TRS local).
fn world_matrices(entities: &[SceneEntity]) -> Vec<Mat4> {
    let locals: Vec<Mat4> = entities
        .iter()
        .map(|e| Mat4::from_scale_rotation_translation(e.scale, e.rotation, e.at))
        .collect();
    let name_to_i: HashMap<&str, usize> = entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.name.as_str(), i))
        .collect();

    let mut worlds = locals.clone();
    let mut visiting = vec![0u8; entities.len()]; // 0 = unseen, 1 = stack, 2 = done

    fn resolve(
        i: usize,
        entities: &[SceneEntity],
        name_to_i: &HashMap<&str, usize>,
        locals: &[Mat4],
        worlds: &mut [Mat4],
        visiting: &mut [u8],
    ) {
        if visiting[i] == 2 {
            return;
        }
        if visiting[i] == 1 {
            worlds[i] = locals[i];
            visiting[i] = 2;
            return;
        }
        visiting[i] = 1;
        if let Some(parent) = entities[i].parent.as_deref() {
            if let Some(&pi) = name_to_i.get(parent) {
                resolve(pi, entities, name_to_i, locals, worlds, visiting);
                worlds[i] = worlds[pi] * locals[i];
                visiting[i] = 2;
                return;
            }
        }
        worlds[i] = locals[i];
        visiting[i] = 2;
    }

    for i in 0..entities.len() {
        resolve(
            i,
            entities,
            &name_to_i,
            &locals,
            &mut worlds,
            &mut visiting,
        );
    }
    worlds
}
