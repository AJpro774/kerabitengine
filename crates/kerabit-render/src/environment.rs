//! Image-based lighting: prefiltered specular cubemap, split-sum BRDF LUT, and
//! SH9 irradiance. Built on the GPU from an equirectangular HDR or from the
//! procedural sky gradient when a scene has no `environment`.

use kerabit_color::Color;

use crate::sky::zenith_from_horizon;

const SHADER_IBL: &str = include_str!("../shaders/ibl.wgsl");
pub const ENV_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const BRDF_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// Base resolution of the prefiltered specular cube (mip 0).
pub const SPECULAR_SIZE: u32 = 128;
/// Roughness levels stored as mips of the specular cube.
pub const SPECULAR_MIPS: u32 = 6;
const BRDF_SIZE: u32 = 128;

/// Pixels of an equirectangular HDR image (linear RGB, row-major, top row first).
pub struct EquirectImage<'a> {
    pub width: u32,
    pub height: u32,
    pub rgb: &'a [f32],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FaceParams {
    /// x = face index, y = roughness, z = source mode (0 equirect, 1 sky), w = sample count.
    params: [f32; 4],
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
    sky_ground: [f32; 4],
}

/// GPU resources shared by every environment (pipelines, LUT).
pub struct IblPipelines {
    equirect_bgl: wgpu::BindGroupLayout,
    to_cube: wgpu::RenderPipeline,
    prefilter: wgpu::RenderPipeline,
    brdf: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    pub brdf_view: wgpu::TextureView,
    _brdf: wgpu::Texture,
}

/// One environment: prefiltered specular cube + SH irradiance + intensity.
pub struct Environment {
    pub specular_view: wgpu::TextureView,
    _specular: wgpu::Texture,
    pub sh: [[f32; 4]; 9],
    pub intensity: f32,
}

impl IblPipelines {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ibl"),
            source: wgpu::ShaderSource::Wgsl(SHADER_IBL.into()),
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ibl-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let equirect_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ibl-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
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
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ibl-layout"),
            bind_group_layouts: &[&equirect_bgl],
            push_constant_ranges: &[],
        });
        let make = |label: &str, entry: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let to_cube = make("ibl-to-cube", "fs_to_cube", ENV_FORMAT);
        let prefilter = make("ibl-prefilter", "fs_prefilter", ENV_FORMAT);
        let brdf = make("ibl-brdf", "fs_brdf", BRDF_FORMAT);

        // Split-sum BRDF LUT is environment-independent: bake once.
        let brdf_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("brdf-lut"),
            size: wgpu::Extent3d {
                width: BRDF_SIZE,
                height: BRDF_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: BRDF_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let brdf_view = brdf_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let this = Self {
            equirect_bgl,
            to_cube,
            prefilter,
            brdf,
            sampler,
            brdf_view,
            _brdf: brdf_tex,
        };
        let (dummy_2d, dummy_cube) = this.dummy_sources(device);
        let params = crate::gbuffer::uniform_buffer(
            device,
            "ibl-brdf-params",
            &<FaceParams as bytemuck::Zeroable>::zeroed(),
        );
        let bg = this.bind_group(device, &params, &dummy_2d, &dummy_cube);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ibl-brdf-encoder"),
        });
        fullscreen_pass(&mut encoder, &this.brdf_view, &this.brdf, &bg);
        queue.submit(std::iter::once(encoder.finish()));
        this
    }

    fn dummy_sources(&self, device: &wgpu::Device) -> (wgpu::TextureView, wgpu::TextureView) {
        let tex2d = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl-dummy-2d"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ENV_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let cube = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl-dummy-cube"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ENV_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        (
            tex2d.create_view(&wgpu::TextureViewDescriptor::default()),
            cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            }),
        )
    }

    fn bind_group(
        &self,
        device: &wgpu::Device,
        params: &wgpu::Buffer,
        equirect: &wgpu::TextureView,
        cube: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ibl-bg"),
            layout: &self.equirect_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(equirect),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(cube),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Build an environment from an equirectangular HDR image.
    pub fn build_from_equirect(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &EquirectImage<'_>,
        intensity: f32,
    ) -> Environment {
        let mut rgba = Vec::with_capacity((image.width * image.height * 4) as usize);
        for px in image.rgb.chunks_exact(3) {
            rgba.extend_from_slice(&[
                f32_to_f16_bits(px[0]),
                f32_to_f16_bits(px[1]),
                f32_to_f16_bits(px[2]),
                f32_to_f16_bits(1.0),
            ]);
        }
        let equirect = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("env-equirect"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ENV_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &equirect,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&rgba),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 8),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        let equirect_view = equirect.create_view(&wgpu::TextureViewDescriptor::default());
        let sh = sh_from_equirect(image);
        self.build(device, queue, &equirect_view, None, sh, intensity)
    }

    /// Build an environment from the procedural sky gradient (horizon = `clear_color`).
    pub fn build_from_sky(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        horizon: Color,
        intensity: f32,
    ) -> Environment {
        let top = zenith_from_horizon(horizon);
        let ground = Color::rgb(horizon.r * 0.45, horizon.g * 0.45, horizon.b * 0.45);
        let sky = SkyColors { top, horizon, ground };
        let (dummy_2d, _) = self.dummy_sources(device);
        let sh = sh_from_sky(&sky);
        self.build(device, queue, &dummy_2d, Some(sky), sh, intensity)
    }

    fn build(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        equirect: &wgpu::TextureView,
        sky: Option<SkyColors>,
        sh: [[f32; 4]; 9],
        intensity: f32,
    ) -> Environment {
        let mode = if sky.is_some() { 1.0 } else { 0.0 };
        let (sky_top, sky_bottom, sky_ground) = match &sky {
            Some(s) => (s.top.to_array(), s.horizon.to_array(), s.ground.to_array()),
            None => ([0.0; 4], [0.0; 4], [0.0; 4]),
        };

        // Source cube (mip 0 of a scratch cube) rendered from the equirect / sky.
        let source = create_cube(device, "env-source", SPECULAR_SIZE, 1);
        let source_view = cube_view(&source, 0, 1);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ibl-build"),
        });
        let (dummy_2d, dummy_cube) = self.dummy_sources(device);
        let mut keep = Vec::new();
        for face in 0..6u32 {
            let params = crate::gbuffer::uniform_buffer(
                device,
                "ibl-face-params",
                &FaceParams {
                    params: [face as f32, 0.0, mode, 0.0],
                    sky_top,
                    sky_bottom,
                    sky_ground,
                },
            );
            let bg = self.bind_group(device, &params, equirect, &dummy_cube);
            let target = face_view(&source, face, 0);
            fullscreen_pass(&mut encoder, &target, &self.to_cube, &bg);
            keep.push((params, bg));
        }

        // Prefilter each roughness level into the specular cube mips.
        let specular = create_cube(device, "env-specular", SPECULAR_SIZE, SPECULAR_MIPS);
        for mip in 0..SPECULAR_MIPS {
            let roughness = mip as f32 / (SPECULAR_MIPS - 1) as f32;
            let samples = if mip == 0 { 1.0 } else { 64.0 };
            for face in 0..6u32 {
                let params = crate::gbuffer::uniform_buffer(
                    device,
                    "ibl-prefilter-params",
                    &FaceParams {
                        params: [face as f32, roughness, 0.0, samples],
                        sky_top,
                        sky_bottom,
                        sky_ground,
                    },
                );
                let bg = self.bind_group(device, &params, &dummy_2d, &source_view);
                let target = face_view(&specular, face, mip);
                fullscreen_pass(&mut encoder, &target, &self.prefilter, &bg);
                keep.push((params, bg));
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        drop(keep);

        Environment {
            specular_view: cube_view(&specular, 0, SPECULAR_MIPS),
            _specular: specular,
            sh,
            intensity,
        }
    }
}

/// IEEE half-float bits (round-to-nearest-even); HDR radiance never needs subnormal care.
pub(crate) fn f32_to_f16_bits(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x7f_ffff;
    if exp == 0xff {
        // Inf / NaN
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - e);
        let rounded = (m + 0x1000) >> 13;
        return sign | rounded as u16;
    }
    let half = ((e as u32) << 10) | (mant >> 13);
    // Round to nearest even on the dropped 13 bits.
    let round_bit = mant & 0x1000;
    let sticky = (mant & 0x0fff) != 0 || (half & 1) != 0;
    let half = if round_bit != 0 && sticky { half + 1 } else { half };
    sign | half as u16
}

#[derive(Clone, Copy)]
struct SkyColors {
    top: Color,
    horizon: Color,
    ground: Color,
}

fn sky_radiance(sky: &SkyColors, dir: [f32; 3]) -> [f32; 3] {
    let y = dir[1];
    let lerp = |a: Color, b: Color, t: f32| {
        [
            a.r + (b.r - a.r) * t,
            a.g + (b.g - a.g) * t,
            a.b + (b.b - a.b) * t,
        ]
    };
    if y >= 0.0 {
        let t = y.clamp(0.0, 1.0);
        let w = t * t * (3.0 - 2.0 * t);
        lerp(sky.horizon, sky.top, w)
    } else {
        lerp(sky.horizon, sky.ground, (-y).clamp(0.0, 1.0).sqrt())
    }
}

/// Project radiance onto the first 9 real SH basis functions.
fn sh_project(mut sample: impl FnMut(usize, usize) -> ([f32; 3], [f32; 3], f32), w: usize, h: usize) -> [[f32; 4]; 9] {
    let mut sh = [[0.0f32; 4]; 9];
    let mut total_weight = 0.0f32;
    for y in 0..h {
        for x in 0..w {
            let (dir, rgb, weight) = sample(x, y);
            let basis = sh_basis(dir);
            for (i, b) in basis.iter().enumerate() {
                sh[i][0] += rgb[0] * b * weight;
                sh[i][1] += rgb[1] * b * weight;
                sh[i][2] += rgb[2] * b * weight;
            }
            total_weight += weight;
        }
    }
    // Normalize so the weights integrate to 4π over the sphere.
    let norm = 4.0 * std::f32::consts::PI / total_weight.max(1e-6);
    for c in &mut sh {
        c[0] *= norm;
        c[1] *= norm;
        c[2] *= norm;
    }
    sh
}

fn sh_basis(d: [f32; 3]) -> [f32; 9] {
    let (x, y, z) = (d[0], d[1], d[2]);
    [
        0.282095,
        0.488603 * y,
        0.488603 * z,
        0.488603 * x,
        1.092548 * x * y,
        1.092548 * y * z,
        0.315392 * (3.0 * z * z - 1.0),
        1.092548 * x * z,
        0.546274 * (x * x - y * y),
    ]
}

fn sh_from_equirect(image: &EquirectImage<'_>) -> [[f32; 4]; 9] {
    // Subsample large maps; 64×32 directions is plenty for 9 coefficients.
    let sw = image.width.min(128) as usize;
    let shh = image.height.min(64) as usize;
    sh_project(
        |x, y| {
            let u = (x as f32 + 0.5) / sw as f32;
            let v = (y as f32 + 0.5) / shh as f32;
            let phi = u * std::f32::consts::TAU;
            let theta = v * std::f32::consts::PI;
            let dir = [theta.sin() * phi.sin(), theta.cos(), -theta.sin() * phi.cos()];
            let px = ((u * image.width as f32) as u32).min(image.width - 1);
            let py = ((v * image.height as f32) as u32).min(image.height - 1);
            let i = ((py * image.width + px) * 3) as usize;
            let rgb = [image.rgb[i], image.rgb[i + 1], image.rgb[i + 2]];
            (dir, rgb, theta.sin())
        },
        sw,
        shh,
    )
}

fn sh_from_sky(sky: &SkyColors) -> [[f32; 4]; 9] {
    let (sw, shh) = (64usize, 32usize);
    sh_project(
        |x, y| {
            let u = (x as f32 + 0.5) / sw as f32;
            let v = (y as f32 + 0.5) / shh as f32;
            let phi = u * std::f32::consts::TAU;
            let theta = v * std::f32::consts::PI;
            let dir = [theta.sin() * phi.sin(), theta.cos(), -theta.sin() * phi.cos()];
            (dir, sky_radiance(sky, dir), theta.sin())
        },
        sw,
        shh,
    )
}

fn create_cube(device: &wgpu::Device, label: &str, size: u32, mips: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ENV_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn cube_view(texture: &wgpu::Texture, base_mip: u32, mips: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("env-cube-view"),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        base_mip_level: base_mip,
        mip_level_count: Some(mips),
        ..Default::default()
    })
}

fn face_view(texture: &wgpu::Texture, face: u32, mip: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("env-face-view"),
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_mip_level: mip,
        mip_level_count: Some(1),
        base_array_layer: face,
        array_layer_count: Some(1),
        ..Default::default()
    })
}

fn fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ibl-pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        occlusion_query_set: None,
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_conversion_matches_known_values() {
        assert_eq!(f32_to_f16_bits(0.0), 0x0000);
        assert_eq!(f32_to_f16_bits(1.0), 0x3c00);
        assert_eq!(f32_to_f16_bits(-2.0), 0xc000);
        assert_eq!(f32_to_f16_bits(0.5), 0x3800);
        assert_eq!(f32_to_f16_bits(65504.0), 0x7bff);
        assert_eq!(f32_to_f16_bits(1.0e6), 0x7c00, "overflow → inf");
        assert_eq!(f32_to_f16_bits(0.333_251_95), 0x3555);
    }

    #[test]
    fn uniform_sky_projects_to_dc_only() {
        let c = Color::rgb(0.5, 0.5, 0.5);
        let sky = SkyColors {
            top: c,
            horizon: c,
            ground: c,
        };
        let sh = sh_from_sky(&sky);
        // L00 * c4 (0.886227) reconstructs the radiance: 0.5 / 0.886227 / ... ≈ 0.5 * sqrt(4π)*... check via irradiance.
        let irradiance = sh[0][0] * 0.886227 - sh[6][0] * 0.247708;
        assert!((irradiance - 0.5 * std::f32::consts::PI).abs() < 0.05, "{irradiance}");
        for c in &sh[1..] {
            assert!(c[0].abs() < 0.02, "higher bands should vanish: {sh:?}");
        }
    }

    #[test]
    fn brighter_zenith_lifts_upward_irradiance() {
        let sky = SkyColors {
            top: Color::rgb(1.0, 1.0, 1.0),
            horizon: Color::rgb(0.2, 0.2, 0.2),
            ground: Color::rgb(0.05, 0.05, 0.05),
        };
        let sh = sh_from_sky(&sky);
        // SH basis index 1 is proportional to +Y: it must be positive for a bright sky.
        assert!(sh[1][0] > 0.0);
    }
}
