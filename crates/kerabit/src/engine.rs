//! [`Kerabit`] builder and winit/wgpu run loop (internals stay private).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use kerabit_audio::AudioEngine;
use kerabit_color::Color;
use kerabit_input::InputState;
use kerabit_math::vec3;
use kerabit_physics::PhysicsWorld;
use kerabit_render::{
    clamp_lights, Camera, DrawItem, GpuState, Light, MeshId, SurfaceError, TextureId,
};
use kerabit_world::{EntityId, Transform, World};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::{Window, WindowId};

// winit allows only one EventLoop per process. Reach (and similar games) call
// Kerabit::run more than once historically; we keep a single loop and re-enter
// with EventLoopExtRunOnDemand::run_app_on_demand. Prefer mid-run
// Context::apply_scene / clear_world for level transitions (E0) so the window
// and GPU stay alive.
thread_local! {
    static EVENT_LOOP: RefCell<Option<EventLoop<()>>> = const { RefCell::new(None) };
}

use crate::context::Context;
use crate::entity::Entity;
use crate::input_map::{map_key, map_mouse_button};
use crate::material::Material;
use crate::scene::SceneError;
use crate::ui::Ui;

/// Game-facing engine builder. Call [`Kerabit::run`] to open a window.
pub struct Kerabit {
    title: String,
    clear_color: Color,
    pending: Vec<Entity>,
    camera: Camera,
    /// Active lights (1..=[`MAX_LIGHTS`]). Index 0 is the primary / shadow sun when directional.
    lights: Vec<Light>,
    ambient: Color,
    window_size: (u32, u32),
    /// When set, dump RGBA PNG frames each tick (fixed 1/30 dt) for trailers.
    capture_dir: Option<std::path::PathBuf>,
}

impl Kerabit {
    /// Start a builder with window title `title`.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            clear_color: Color::rgb(0.08, 0.09, 0.12),
            pending: Vec::new(),
            camera: Camera::perspective(60.0).look_at(vec3(5.0, 3.0, 7.0), kerabit_math::Vec3::ZERO),
            lights: vec![Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2)],
            ambient: Color::rgb(0.15, 0.16, 0.18),
            window_size: (960, 640),
            capture_dir: None,
        }
    }

    /// Physical window size in pixels (default 960×640).
    pub fn window_size(mut self, width: u32, height: u32) -> Self {
        self.window_size = (width.max(1), height.max(1));
        self
    }

    /// Dump a PNG sequence under `dir` each frame (fixed 30 FPS dt). Creates `dir`.
    pub fn capture_frames(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.capture_dir = Some(dir.into());
        self
    }

    /// Framebuffer clear color.
    pub fn clear_color(mut self, color: Color) -> Self {
        self.clear_color = color;
        self
    }

    /// Queue an entity to spawn when the GPU is ready.
    pub fn spawn(mut self, entity: Entity) -> Self {
        self.pending.push(entity);
        self
    }

    /// Set the active camera.
    pub fn camera(mut self, camera: Camera) -> Self {
        self.camera = camera;
        self
    }

    /// Set the primary directional sun (replaces light slot 0; keeps extras).
    pub fn light(mut self, light: Light) -> Self {
        if self.lights.is_empty() {
            self.lights.push(light);
        } else {
            self.lights[0] = light;
        }
        self
    }

    /// Replace the full light list (truncated to [`MAX_LIGHTS`] = 4).
    ///
    /// Soft shadows use the first directional light only. Point lights are unshadowed.
    pub fn lights(mut self, lights: impl IntoIterator<Item = Light>) -> Self {
        self.lights = clamp_lights(&lights.into_iter().collect::<Vec<_>>());
        if self.lights.is_empty() {
            self.lights.push(Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2));
        }
        self
    }

    /// Ambient term in the lit shader.
    pub fn ambient(mut self, color: Color) -> Self {
        self.ambient = color;
        self
    }

    /// Open a window and run `update` every frame until quit / close.
    ///
    /// When [`Self::capture_frames`] is set, runs **headless** (no window) at a
    /// fixed 30 FPS and writes a PNG sequence — used for marketing trailers.
    ///
    /// Safe to call more than once in the same process: the winit event loop is
    /// created once and re-entered on later calls. For level transitions prefer
    /// [`Context::apply_scene`] / [`Context::load_scene`] inside a single `run`
    /// so the window and GPU are not torn down.
    ///
    /// Panics only if the event loop cannot be created; GPU failures print and exit.
    pub fn run<F>(self, update: F)
    where
        F: FnMut(&mut Context<'_>) + 'static,
    {
        if self.capture_dir.is_some() {
            if let Err(err) = run_headless_inner(self, update) {
                eprintln!("kerabit: {err:#}");
                std::process::exit(1);
            }
            return;
        }
        if let Err(err) = run_inner(self, update) {
            eprintln!("kerabit: {err:#}");
            std::process::exit(1);
        }
    }
}

pub(crate) struct Renderable {
    mesh: MeshId,
    material: Material,
    /// GPU handle for [`Material::albedo_texture`], if any.
    albedo_texture: Option<TextureId>,
    /// GPU handle for [`Material::normal_texture`], if any.
    normal_texture: Option<TextureId>,
}

struct App<F> {
    title: String,
    clear_color: Color,
    pending: Vec<Entity>,
    camera: Camera,
    lights: Vec<Light>,
    ambient: Color,
    update: F,
    window: Option<Arc<Window>>,
    window_size: (u32, u32),
    gpu: Option<GpuState>,
    world: World,
    physics: PhysicsWorld,
    audio: AudioEngine,
    renderables: HashMap<EntityId, Renderable>,
    input: InputState,
    ui: Ui,
    last_frame: Instant,
    quit: bool,
    capture_dir: Option<std::path::PathBuf>,
    capture_frame: u32,
}

impl<F> ApplicationHandler for App<F>
where
    F: FnMut(&mut Context<'_>),
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(winit::dpi::PhysicalSize::new(
                self.window_size.0,
                self.window_size.1,
            ));

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = Arc::new(window);
                match GpuState::new(window.clone(), self.clear_color) {
                    Ok(mut gpu) => {
                        if self.capture_dir.is_some() {
                            gpu.enable_frame_capture();
                        }
                        if let Err(err) = spawn_pending(self, &mut gpu) {
                            eprintln!("kerabit: failed to spawn scene: {err:#}");
                            event_loop.exit();
                            return;
                        }
                        self.gpu = Some(gpu);
                        self.window = Some(window);
                        self.last_frame = Instant::now();
                    }
                    Err(err) => {
                        eprintln!("kerabit: GPU init failed: {err:#}");
                        event_loop.exit();
                    }
                }
            }
            Err(err) => {
                eprintln!("kerabit: window creation failed: {err}");
                event_loop.exit();
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.input.add_mouse_delta(dx as f32, dy as f32);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(key) = map_key(&event.logical_key) {
                    self.input
                        .set_key(key, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = map_mouse_button(button) {
                    self.input
                        .set_mouse_button(button, state == ElementState::Pressed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input
                    .set_mouse_position(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Reserved for future zoom; consume so it does not fall through.
                let _ = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.tick_frame(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl<F> App<F>
where
    F: FnMut(&mut Context<'_>),
{
    fn tick_frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let real_dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        // Fixed 30 FPS when dumping frames so the loop encodes cleanly.
        let dt = if self.capture_dir.is_some() {
            1.0 / 30.0
        } else {
            real_dt
        };

        {
            self.ui.clear();
            let mut ctx = Context {
                dt,
                input: &self.input,
                world: &mut self.world,
                camera: &mut self.camera,
                physics: &mut self.physics,
                audio: &mut self.audio,
                ui: &mut self.ui,
                quit: &mut self.quit,
                gpu: self.gpu.as_mut(),
                renderables: &mut self.renderables,
                lights: &mut self.lights,
                ambient: &mut self.ambient,
                clear_color: &mut self.clear_color,
            };
            (self.update)(&mut ctx);
        }

        self.audio.maintain();
        self.input.end_frame();

        if self.quit {
            event_loop.exit();
            return;
        }

        self.world.update_world_matrices();

        let draws = build_draw_list(&self.world, &self.renderables);

        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };

        gpu.update_particles(dt);

        match gpu.render_lights(
            &mut self.camera,
            &self.lights,
            self.ambient,
            &draws,
            self.ui.commands(),
        ) {
            Ok(()) => {
                if let Some(dir) = self.capture_dir.as_ref() {
                    if let Some((w, h, rgba)) = gpu.take_captured_rgba() {
                        let path = dir.join(format!("frame_{:05}.png", self.capture_frame));
                        self.capture_frame += 1;
                        if let Err(err) = image::save_buffer(
                            &path,
                            &rgba,
                            w,
                            h,
                            image::ColorType::Rgba8,
                        ) {
                            eprintln!("kerabit: failed to write {}: {err}", path.display());
                        }
                    }
                }
            }
            Err(SurfaceError::Lost | SurfaceError::Outdated) => {
                let size = self
                    .window
                    .as_ref()
                    .map(|w| w.inner_size())
                    .unwrap_or_default();
                gpu.resize(size);
            }
            Err(SurfaceError::OutOfMemory) => {
                eprintln!("kerabit: out of GPU memory");
                event_loop.exit();
            }
            Err(SurfaceError::Timeout | SurfaceError::Other) => {}
        }
    }
}

fn spawn_pending<F>(app: &mut App<F>, gpu: &mut GpuState) -> Result<()> {
    let pending = std::mem::take(&mut app.pending);
    spawn_entities(&mut app.world, &mut app.renderables, gpu, pending)
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("{err}"))
}

/// Spawn descriptors into `world` + `renderables`, uploading meshes as needed.
pub(crate) fn spawn_entities(
    world: &mut World,
    renderables: &mut HashMap<EntityId, Renderable>,
    gpu: &mut GpuState,
    pending: Vec<Entity>,
) -> Result<Vec<EntityId>, SceneError> {
    let mut parent_links: Vec<(String, String)> = Vec::new();
    let mut ids = Vec::with_capacity(pending.len());

    for desc in pending {
        let mesh = desc.mesh.as_ref().ok_or_else(|| {
            SceneError::Spawn(format!("entity `{}` has no mesh", desc.name))
        })?;
        let mesh_id = gpu.upload_mesh(mesh.as_render());
        let albedo_texture = desc
            .material
            .albedo_texture()
            .map(|tex| gpu.upload_texture_rgba8(tex.width, tex.height, &tex.rgba));
        let normal_texture = desc
            .material
            .normal_texture()
            .map(|tex| gpu.upload_texture_rgba8_linear(tex.width, tex.height, &tex.rgba));
        let transform = Transform::from_trs(desc.translation, desc.rotation, desc.scale);
        let name = desc.name.clone();
        if let Some(parent) = desc.parent {
            parent_links.push((name.clone(), parent));
        }
        let id = world.spawn_named(name, transform);
        if let Some(entity) = world.get_mut_by_id(id) {
            entity.set_tags(desc.tags);
            entity.set_layer(desc.layer);
            entity.set_enabled(desc.enabled);
        }
        renderables.insert(
            id,
            Renderable {
                mesh: mesh_id,
                material: desc.material,
                albedo_texture,
                normal_texture,
            },
        );
        ids.push(id);
    }

    for (child, parent) in parent_links {
        if !world.attach(&child, &parent) {
            return Err(SceneError::Spawn(format!(
                "entity `{child}` parent `{parent}` not found"
            )));
        }
    }

    Ok(ids)
}

fn build_draw_list(
    world: &World,
    renderables: &HashMap<EntityId, Renderable>,
) -> Vec<DrawItem> {
    let mut draws = Vec::with_capacity(renderables.len());
    for entity in world.iter() {
        if !entity.is_enabled() {
            continue;
        }
        let Some(r) = renderables.get(&entity.id()) else {
            continue;
        };
        let model = entity.transform.world_matrix_cached();
        let mut item = DrawItem::new(r.mesh, model, r.material.albedo())
            .with_roughness(r.material.roughness_factor())
            .with_metallic(r.material.metallic_factor());
        if let Some(tex) = r.albedo_texture {
            item = item.with_texture(tex);
        }
        if let Some(tex) = r.normal_texture {
            item = item.with_normal_map(tex);
        }
        draws.push(item);
    }
    draws
}

fn run_headless_inner<F>(builder: Kerabit, mut update: F) -> Result<()>
where
    F: FnMut(&mut Context<'_>),
{
    let dir = builder
        .capture_dir
        .clone()
        .context("capture_dir required for headless record")?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create capture dir {}", dir.display()))?;

    let (width, height) = builder.window_size;
    let mut gpu = GpuState::new_headless(width, height, builder.clear_color)
        .context("headless GPU init failed")?;

    let mut world = World::new();
    let mut renderables = HashMap::new();
    let mut physics = PhysicsWorld::new();
    let mut audio = AudioEngine::new();
    let mut camera = builder.camera;
    let mut lights = builder.lights;
    let mut ambient = builder.ambient;
    let mut clear_color = builder.clear_color;
    let mut ui = Ui::new();
    let input = InputState::new();
    let mut quit = false;
    let mut frame_idx = 0u32;

    spawn_entities(&mut world, &mut renderables, &mut gpu, builder.pending)
        .map_err(|err| anyhow::anyhow!("{err}"))?;

    eprintln!(
        "kerabit: headless capture {}×{} → {} @ 30fps",
        width,
        height,
        dir.display()
    );

    // Safety cap: 30s wall of sim time even if quit is never set.
    const MAX_FRAMES: u32 = 30 * 30;
    while !quit && frame_idx < MAX_FRAMES {
        let dt = 1.0 / 30.0;
        ui.clear();
        {
            let mut ctx = Context {
                dt,
                input: &input,
                world: &mut world,
                camera: &mut camera,
                physics: &mut physics,
                audio: &mut audio,
                ui: &mut ui,
                quit: &mut quit,
                gpu: Some(&mut gpu),
                renderables: &mut renderables,
                lights: &mut lights,
                ambient: &mut ambient,
                clear_color: &mut clear_color,
            };
            update(&mut ctx);
        }
        audio.maintain();
        if quit {
            break;
        }

        world.update_world_matrices();
        let draws = build_draw_list(&world, &renderables);
        gpu.update_particles(dt);
        gpu.render_lights(&mut camera, &lights, ambient, &draws, ui.commands())
            .map_err(|e| anyhow::anyhow!("render failed: {e:?}"))?;

        if let Some((w, h, rgba)) = gpu.take_captured_rgba() {
            let path = dir.join(format!("frame_{frame_idx:05}.png"));
            image::save_buffer(&path, &rgba, w, h, image::ColorType::Rgba8)
                .with_context(|| format!("write {}", path.display()))?;
        }
        frame_idx += 1;
        if frame_idx % 30 == 0 {
            eprintln!("kerabit: captured {frame_idx} frames…");
        }
    }

    eprintln!("kerabit: wrote {frame_idx} frames to {}", dir.display());
    Ok(())
}

fn run_inner<F>(builder: Kerabit, update: F) -> Result<()>
where
    F: FnMut(&mut Context<'_>) + 'static,
{
    EVENT_LOOP.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(EventLoop::new().context("failed to create winit event loop")?);
        }
        let event_loop = slot.as_mut().expect("event loop slot just ensured");
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut app = App {
            title: builder.title,
            clear_color: builder.clear_color,
            pending: builder.pending,
            camera: builder.camera,
            lights: builder.lights,
            ambient: builder.ambient,
            update,
            window: None,
            window_size: builder.window_size,
            gpu: None,
            world: World::new(),
            physics: PhysicsWorld::new(),
            audio: AudioEngine::new(),
            renderables: HashMap::new(),
            input: InputState::new(),
            ui: Ui::new(),
            last_frame: Instant::now(),
            quit: false,
            capture_dir: builder.capture_dir.clone(),
            capture_frame: 0,
        };

        if let Some(dir) = app.capture_dir.as_ref() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("create capture dir {}", dir.display()))?;
        }

        event_loop
            .run_app_on_demand(&mut app)
            .context("winit event loop exited with error")?;
        Ok(())
    })
}
