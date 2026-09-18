//! The 3.0 scene pass chain, shared by the windowed / headless [`crate::GpuState`]
//! and the editor's [`crate::OffscreenLitRenderer`]:
//!
//! lights → clusters → cascaded shadows → depth prepass + G-buffer → Hi-Z →
//! SSAO → sky → lit (clustered PBR, CSM, SH irradiance) → specular resolve
//! (SSR / IBL) → *particles drawn by the caller into [`Self::lit_view`]* →
//! TAA resolve → [`Self::output_view`] (HDR, ready for bloom + tonemap).

use kerabit_color::Color;
use kerabit_math::Mat4;

use crate::camera::Camera;
use crate::environment::{Environment, EquirectImage, IblPipelines};
use crate::gbuffer::{draw_ranges, GBuffer, DEPTH_FORMAT};
use crate::light::Light;
use crate::lights::{buffer_entry, LightBuffers};
use crate::mesh_gpu::{MeshCache, MeshId};
use crate::post::HDR_FORMAT;
use crate::shadow::{fit_cascades, CascadeSet, ShadowMap};
use crate::sky::SkyPass;
use crate::ssao::SsaoPass;
use crate::ssr::{SsrInputs, SsrPass};
use crate::taa::{jitter_pixels, TaaPass};
use crate::texture::TextureCache;
use crate::uniforms::{
    pack_draw_batches, prepare_draws, DrawItem, FrameInputs, FrameUniforms, InstanceRaw,
    RenderSettings, MAX_INSTANCES,
};
use crate::vertex::Vertex;

pub(crate) const FRAME_WGSL: &str = include_str!("../shaders/frame.wgsl");
const SHADER_LIT: &str = include_str!("../shaders/lit.wgsl");
/// IBL strength used for the procedural sky when a scene sets no `environment`.
pub const DEFAULT_SKY_ENV_INTENSITY: f32 = 0.3;

/// Which HDR stage the post stack tonemaps (debug aid).
///
/// Set `KERABIT_RENDER_DEBUG` to a comma list of `lit` / `resolved` (stage to
/// show) and `no-ssao` / `no-ibl` / `no-ssr` / `no-taa` (features to disable).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DebugOutput {
    #[default]
    Final,
    /// Lit color before the specular resolve.
    Lit,
    /// After the specular resolve, before TAA.
    Resolved,
}

fn debug_config() -> (DebugOutput, RenderSettings) {
    let mut out = DebugOutput::Final;
    let mut settings = RenderSettings::default();
    if let Ok(spec) = std::env::var("KERABIT_RENDER_DEBUG") {
        for item in spec.split(',').map(str::trim) {
            match item {
                "lit" => out = DebugOutput::Lit,
                "resolved" => out = DebugOutput::Resolved,
                "no-ssao" => settings.ssao = false,
                "no-ibl" => settings.ibl = false,
                "no-ssr" => settings.ssr = false,
                "no-taa" => settings.taa = false,
                "" => {}
                other => eprintln!("kerabit-render: unknown KERABIT_RENDER_DEBUG item `{other}`"),
            }
        }
    }
    (out, settings)
}

pub struct SceneRenderer {
    width: u32,
    height: u32,
    pub mesh_cache: MeshCache,
    pub texture_cache: TextureCache,
    pub settings: RenderSettings,
    frame_buffer: wgpu::Buffer,
    frame_bgl: wgpu::BindGroupLayout,
    frame_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    lights: LightBuffers,
    shadow: ShadowMap,
    gbuffer: GBuffer,
    ssao: SsaoPass,
    ibl: IblPipelines,
    env: Environment,
    /// `Some(horizon)` while the environment is the procedural sky for that horizon color.
    sky_env_horizon: Option<Color>,
    sky: SkyPass,
    scene_bgl: wgpu::BindGroupLayout,
    scene_bind_group: wgpu::BindGroup,
    lit_pipeline: wgpu::RenderPipeline,
    _hdr: wgpu::Texture,
    hdr_view: wgpu::TextureView,
    _spec: wgpu::Texture,
    spec_view: wgpu::TextureView,
    ssr: SsrPass,
    taa: TaaPass,
    prev_view_proj: Mat4,
    frame_index: u32,
    debug_output: DebugOutput,
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let texture_cache = TextureCache::new(device, queue);

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-uniforms"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance-buffer"),
            size: (std::mem::size_of::<InstanceRaw>() * MAX_INSTANCES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lights = LightBuffers::new(device, &frame_buffer, FRAME_WGSL);

        let frame_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame-bgl"),
            entries: &[
                buffer_entry(0, wgpu::BufferBindingType::Uniform),
                buffer_entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer_entry(2, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer_entry(3, wgpu::BufferBindingType::Storage { read_only: true }),
            ],
        });
        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame-bg"),
            layout: &frame_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lights.lights.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lights.grid.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: lights.indices.as_entire_binding(),
                },
            ],
        });

        let shadow = ShadowMap::new(device);
        let gbuffer = GBuffer::new(device, &frame_bgl, FRAME_WGSL, width, height);
        let ssao = SsaoPass::new(
            device,
            &frame_bgl,
            FRAME_WGSL,
            &gbuffer.depth_view,
            &gbuffer.normal_view,
            width,
            height,
        );
        let ibl = IblPipelines::new(device, queue);
        let sky_horizon = Color::rgb(0.08, 0.09, 0.12);
        let env = ibl.build_from_sky(device, queue, sky_horizon, DEFAULT_SKY_ENV_INTENSITY);
        let sky = SkyPass::new(device, HDR_FORMAT);

        let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene-textures-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let lit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lit"),
            source: wgpu::ShaderSource::Wgsl(format!("{FRAME_WGSL}\n{SHADER_LIT}").into()),
        });
        let lit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lit-pipeline-layout"),
            bind_group_layouts: &[&frame_bgl, texture_cache.bind_group_layout(), &scene_bgl],
            push_constant_ranges: &[],
        });
        let lit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lit-pipeline"),
            layout: Some(&lit_layout),
            vertex: wgpu::VertexState {
                module: &lit_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &lit_shader,
                entry_point: Some("fs_main"),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            // Depth comes from the prepass: test only, so hidden fragments never shade.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let (hdr, hdr_view) = hdr_target(device, "hdr-color", width, height);
        let (spec, spec_view) = hdr_target(device, "hdr-spec-weight", width, height);
        let ssr = SsrPass::new(device, &frame_bgl, FRAME_WGSL);
        let taa = TaaPass::new(device, &frame_bgl, FRAME_WGSL);
        let scene_bind_group = make_scene_bind_group(device, &scene_bgl, &shadow, &ssao.view, &ibl.brdf_view);
        let (debug_output, settings) = debug_config();

        let mut this = Self {
            width,
            height,
            mesh_cache: MeshCache::new(),
            texture_cache,
            settings,
            frame_buffer,
            frame_bgl,
            frame_bind_group,
            instance_buffer,
            lights,
            shadow,
            gbuffer,
            ssao,
            ibl,
            env,
            sky_env_horizon: Some(sky_horizon),
            sky,
            scene_bgl,
            scene_bind_group,
            lit_pipeline,
            _hdr: hdr,
            hdr_view,
            _spec: spec,
            spec_view,
            ssr,
            taa,
            prev_view_proj: Mat4::IDENTITY,
            frame_index: 0,
            debug_output,
        };
        this.rebind_targets(device);
        this
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// HDR target holding lit color + resolved specular; callers draw particles here.
    pub fn lit_view(&self) -> &wgpu::TextureView {
        self.ssr.output_view.as_ref().unwrap_or(&self.hdr_view)
    }

    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.gbuffer.depth_view
    }

    /// Final HDR frame after [`Self::encode_finish`] (TAA output).
    pub fn output_view(&self) -> &wgpu::TextureView {
        match self.debug_output {
            DebugOutput::Final => self.taa.output_view().unwrap_or_else(|| self.lit_view()),
            DebugOutput::Lit => &self.hdr_view,
            DebugOutput::Resolved => self.lit_view(),
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.gbuffer.resize(device, width, height);
        self.ssao
            .resize(device, width, height, &self.gbuffer.depth_view, &self.gbuffer.normal_view);
        let (hdr, hdr_view) = hdr_target(device, "hdr-color", width, height);
        let (spec, spec_view) = hdr_target(device, "hdr-spec-weight", width, height);
        self._hdr = hdr;
        self.hdr_view = hdr_view;
        self._spec = spec;
        self.spec_view = spec_view;
        self.rebind_targets(device);
    }

    /// Environment from an equirectangular HDR (`None` restores the procedural sky).
    pub fn set_environment(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: Option<&EquirectImage<'_>>,
        intensity: f32,
    ) {
        match image {
            Some(img) => {
                self.env = self.ibl.build_from_equirect(device, queue, img, intensity);
                self.sky_env_horizon = None;
            }
            None => {
                let horizon = self.sky_env_horizon.unwrap_or(Color::rgb(0.08, 0.09, 0.12));
                self.env = self.ibl.build_from_sky(device, queue, horizon, intensity);
                self.sky_env_horizon = Some(horizon);
            }
        }
        self.rebind_targets(device);
    }

    fn rebind_targets(&mut self, device: &wgpu::Device) {
        self.scene_bind_group =
            make_scene_bind_group(device, &self.scene_bgl, &self.shadow, &self.ssao.view, &self.ibl.brdf_view);
        self.ssr.rebind(
            device,
            self.width,
            self.height,
            &SsrInputs {
                color: &self.hdr_view,
                spec: &self.spec_view,
                depth: &self.gbuffer.depth_view,
                normal: &self.gbuffer.normal_view,
                hiz: &self.gbuffer.hiz_view,
                env_specular: &self.env.specular_view,
            },
        );
        let lit = self.ssr.output_view.clone().expect("ssr output");
        self.taa.rebind(
            device,
            self.width,
            self.height,
            &lit,
            &self.gbuffer.velocity_view,
            &self.gbuffer.depth_view,
        );
    }

    /// Keep the procedural-sky environment in sync with the scene's horizon color.
    fn sync_sky_environment(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, horizon: Color) {
        let Some(current) = self.sky_env_horizon else {
            return;
        };
        if color_close(current, horizon) {
            return;
        }
        let intensity = self.env.intensity;
        self.env = self.ibl.build_from_sky(device, queue, horizon, intensity);
        self.sky_env_horizon = Some(horizon);
        self.rebind_targets(device);
    }

    /// Everything up to and including the specular resolve.
    #[allow(clippy::too_many_arguments)]
    pub fn encode_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &mut Camera,
        lights: &[Light],
        ambient: Color,
        clear_color: Color,
        draws: &[DrawItem],
    ) {
        camera.set_aspect(self.width as f32 / self.height.max(1) as f32);
        self.sync_sky_environment(device, queue, clear_color);

        let (dir_count, point_count) = self.lights.upload(queue, lights);
        let shadow_dir = Light::first_directional(lights)
            .map(|l| l.direction)
            .unwrap_or_else(|| kerabit_math::vec3(-0.35, -1.0, -0.25));
        let cascades: CascadeSet = fit_cascades(camera, shadow_dir);

        let mut proj = camera.projection_matrix();
        let mut jitter_ndc = [0.0f32; 2];
        if self.settings.taa {
            let j = jitter_pixels(self.frame_index);
            jitter_ndc = [2.0 * j[0] / self.width as f32, 2.0 * j[1] / self.height as f32];
            proj.z_axis.x -= jitter_ndc[0];
            proj.z_axis.y -= jitter_ndc[1];
        }
        let frame = FrameUniforms::build(&FrameInputs {
            camera,
            proj_jittered: proj,
            prev_view_proj: self.prev_view_proj,
            jitter_ndc,
            ambient,
            env_intensity: self.env.intensity,
            dir_light_count: dir_count,
            point_light_count: point_count,
            width: self.width,
            height: self.height,
            cascades: &cascades,
            sh: &self.env.sh,
            settings: self.settings,
        });
        queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));

        let white = self.texture_cache.white();
        let flat_n = self.texture_cache.flat_normal();
        let visible = prepare_draws(camera.view_proj(), camera.position(), draws, |id| {
            self.mesh_cache.local_aabb(id)
        });
        let (flat, ranges) = pack_draw_batches(&visible, white, flat_n);
        if !flat.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&flat));
        }
        let mesh_ranges: Vec<(MeshId, u32, u32)> = ranges
            .iter()
            .map(|&(mesh, _, _, start, count)| (mesh, start, count))
            .collect();
        for &(_, albedo, normal, _, _) in &ranges {
            let _ = self.texture_cache.ensure_material_bind_group(device, albedo, normal);
        }

        self.lights.encode_cluster(encoder);
        self.shadow.encode(queue, encoder, &cascades, &self.mesh_cache, &self.instance_buffer, &mesh_ranges);
        self.gbuffer
            .encode_prepass(encoder, &self.frame_bind_group, &self.mesh_cache, &self.instance_buffer, &mesh_ranges);
        if self.settings.ssr {
            self.gbuffer.encode_hiz(encoder);
        }
        if self.settings.ssao {
            self.ssao.encode(encoder, &self.frame_bind_group);
        }
        self.sky
            .encode(queue, encoder, &self.hdr_view, &self.gbuffer.depth_view, clear_color);

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lit-pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.hdr_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.spec_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.gbuffer.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.lit_pipeline);
            pass.set_bind_group(0, &self.frame_bind_group, &[]);
            pass.set_bind_group(2, &self.scene_bind_group, &[]);
            for &(mesh_id, albedo, normal, start, count) in &ranges {
                let Some(tex_bg) = self.texture_cache.material_bind_group(albedo, normal) else {
                    continue;
                };
                pass.set_bind_group(1, tex_bg, &[]);
                draw_ranges(&mut pass, &self.mesh_cache, &self.instance_buffer, &[(mesh_id, start, count)]);
            }
        }

        self.ssr.encode(encoder, &self.frame_bind_group);

        self.prev_view_proj = camera.projection_matrix() * camera.view_matrix();
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    /// TAA resolve; run after any transparent / particle draws into [`Self::lit_view`].
    pub fn encode_finish(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.taa.encode(encoder, &self.frame_bind_group);
    }

    pub fn frame_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.frame_bgl
    }
}

fn color_close(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 1e-3 && (a.g - b.g).abs() < 1e-3 && (a.b - b.b).abs() < 1e-3
}

fn hdr_target(device: &wgpu::Device, label: &str, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

fn make_scene_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    shadow: &ShadowMap,
    ao_view: &wgpu::TextureView,
    brdf_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let lin = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("scene-linear-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene-textures-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&shadow.array_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&shadow.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(ao_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&lin),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(brdf_view),
            },
        ],
    })
}
