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
use kerabit_render::{Camera, DrawItem, GpuState, Light, MeshId, SurfaceError, TextureId};
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
    light: Light,
    ambient: Color,
}

impl Kerabit {
    /// Start a builder with window title `title`.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            clear_color: Color::rgb(0.08, 0.09, 0.12),
            pending: Vec::new(),
            camera: Camera::perspective(60.0).look_at(vec3(5.0, 3.0, 7.0), kerabit_math::Vec3::ZERO),
            light: Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2),
            ambient: Color::rgb(0.15, 0.16, 0.18),
        }
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

    /// Set the directional sun light.
    pub fn light(mut self, light: Light) -> Self {
        self.light = light;
        self
    }

    /// Ambient term in the lit shader.
    pub fn ambient(mut self, color: Color) -> Self {
        self.ambient = color;
        self
    }

    /// Open a window and run `update` every frame until quit / close.
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
}

struct App<F> {
    title: String,
    clear_color: Color,
    pending: Vec<Entity>,
    camera: Camera,
    light: Light,
    ambient: Color,
    update: F,
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    world: World,
    physics: PhysicsWorld,
    audio: AudioEngine,
    renderables: HashMap<EntityId, Renderable>,
    input: InputState,
    ui: Ui,
    last_frame: Instant,
    quit: bool,
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
            .with_inner_size(winit::dpi::LogicalSize::new(960.0, 640.0));

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = Arc::new(window);
                match GpuState::new(window.clone(), self.clear_color) {
                    Ok(mut gpu) => {
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
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

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
                light: &mut self.light,
                ambient: &mut self.ambient,
                clear_color: &mut self.clear_color,
            };
            (self.update)(&mut ctx);
        }

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

        match gpu.render(
            &mut self.camera,
            &self.light,
            self.ambient,
            &draws,
            self.ui.commands(),
        ) {
            Ok(()) => {}
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
        let transform = Transform::from_trs(desc.translation, desc.rotation, desc.scale);
        let name = desc.name.clone();
        if let Some(parent) = desc.parent {
            parent_links.push((name.clone(), parent));
        }
        let id = world.spawn_named(name, transform);
        renderables.insert(
            id,
            Renderable {
                mesh: mesh_id,
                material: desc.material,
                albedo_texture,
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
        let Some(r) = renderables.get(&entity.id()) else {
            continue;
        };
        let model = entity.transform.world_matrix_cached();
        let mut item = DrawItem::new(r.mesh, model, r.material.albedo())
            .with_roughness(r.material.roughness_factor());
        if let Some(tex) = r.albedo_texture {
            item = item.with_texture(tex);
        }
        draws.push(item);
    }
    draws
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
            light: builder.light,
            ambient: builder.ambient,
            update,
            window: None,
            gpu: None,
            world: World::new(),
            physics: PhysicsWorld::new(),
            audio: AudioEngine::new(),
            renderables: HashMap::new(),
            input: InputState::new(),
            ui: Ui::new(),
            last_frame: Instant::now(),
            quit: false,
        };

        event_loop
            .run_app_on_demand(&mut app)
            .context("winit event loop exited with error")?;
        Ok(())
    })
}
