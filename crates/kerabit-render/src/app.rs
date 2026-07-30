//! Window event loop for lit mesh harnesses (P1/P2).

use std::sync::Arc;

use anyhow::{Context as _, Result};
use kerabit_color::Color;
use kerabit_math::{vec3, Mat4, Vec3};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::camera::Camera;
use crate::gpu::GpuState;
use crate::light::Light;
use crate::mesh::Mesh;
use crate::uniforms::DrawItem;

/// Scene description built once after GPU init.
struct Scene {
    camera: Camera,
    light: Light,
    ambient: Color,
    draws: Vec<DrawItem>,
}

enum SceneKind {
    /// P1: single orange cube.
    HardcodedCube,
    /// P2: gray plane + orange cube at different positions.
    TwoMeshes,
}

struct App {
    title: String,
    clear_color: Color,
    kind: SceneKind,
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    scene: Option<Scene>,
}

impl ApplicationHandler for App {
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
                        let scene = build_scene(&mut gpu, &self.kind);
                        self.scene = Some(scene);
                        self.gpu = Some(gpu);
                        self.window = Some(window);
                    }
                    Err(err) => {
                        eprintln!("kerabit-render: GPU init failed: {err:#}");
                        event_loop.exit();
                    }
                }
            }
            Err(err) => {
                eprintln!("kerabit-render: window creation failed: {err}");
                event_loop.exit();
            }
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
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed()
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                event_loop.exit();
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
                let (Some(gpu), Some(scene)) = (self.gpu.as_mut(), self.scene.as_mut()) else {
                    return;
                };
                match gpu.render(
                    &mut scene.camera,
                    &scene.light,
                    scene.ambient,
                    &scene.draws,
                    &crate::OverlayCommands::new(),
                ) {
                    Ok(()) => {}
                    Err(crate::SurfaceError::Lost | crate::SurfaceError::Outdated) => {
                        let size = self
                            .window
                            .as_ref()
                            .map(|w| w.inner_size())
                            .unwrap_or_default();
                        gpu.resize(size);
                    }
                    Err(crate::SurfaceError::OutOfMemory) => {
                        eprintln!("kerabit-render: out of GPU memory");
                        event_loop.exit();
                    }
                    Err(crate::SurfaceError::Timeout | crate::SurfaceError::Other) => {
                        // Transient; try again next frame.
                    }
                }
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

fn build_scene(gpu: &mut GpuState, kind: &SceneKind) -> Scene {
    let light = Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2);
    let ambient = Color::rgb(0.15, 0.16, 0.18);

    match kind {
        SceneKind::HardcodedCube => {
            let cube_id = gpu.upload_mesh(&Mesh::cube());
            Scene {
                camera: Camera::perspective(45.0).look_at(vec3(2.8, 2.0, 3.6), Vec3::ZERO),
                light,
                ambient,
                draws: vec![DrawItem::new(cube_id, Mat4::IDENTITY, Color::ORANGE)],
            }
        }
        SceneKind::TwoMeshes => {
            let cube_id = gpu.upload_mesh(&Mesh::cube());
            let plane_id = gpu.upload_mesh(&Mesh::plane(12.0));
            Scene {
                camera: Camera::perspective(60.0).look_at(vec3(5.0, 3.0, 7.0), Vec3::ZERO),
                light,
                ambient,
                draws: vec![
                    DrawItem::at(plane_id, Vec3::ZERO, Color::GRAY),
                    DrawItem::at(cube_id, vec3(0.0, 0.5, 0.0), Color::ORANGE),
                ],
            }
        }
    }
}

fn run_app(title: impl Into<String>, clear_color: Color, kind: SceneKind) -> Result<()> {
    let event_loop = EventLoop::new().context("failed to create winit event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        title: title.into(),
        clear_color,
        kind,
        window: None,
        gpu: None,
        scene: None,
    };

    event_loop
        .run_app(&mut app)
        .context("winit event loop exited with error")?;
    Ok(())
}

/// Open a window and render the hardcoded lit cube until closed (Escape quits).
pub fn run_hardcoded_cube(title: impl Into<String>, clear_color: Color) -> Result<()> {
    run_app(title, clear_color, SceneKind::HardcodedCube)
}

/// P2 harness: lit gray plane + orange cube at different positions (Escape quits).
pub fn run_two_meshes(title: impl Into<String>, clear_color: Color) -> Result<()> {
    run_app(title, clear_color, SceneKind::TwoMeshes)
}
