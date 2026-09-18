//! Temporal anti-aliasing: Halton jitter sequence + history reprojection/resolve.

use crate::post::HDR_FORMAT;

const SHADER_TAA: &str = include_str!("../shaders/taa.wgsl");
const JITTER_SAMPLES: u32 = 8;

/// Sub-pixel jitter for `frame` (Halton 2,3), in pixels within `[-0.5, 0.5]`.
pub fn jitter_pixels(frame: u32) -> [f32; 2] {
    let i = frame % JITTER_SAMPLES + 1;
    [halton(i, 2) - 0.5, halton(i, 3) - 0.5]
}

fn halton(mut index: u32, base: u32) -> f32 {
    let mut f = 1.0f32;
    let mut r = 0.0f32;
    while index > 0 {
        f /= base as f32;
        r += f * (index % base) as f32;
        index /= base;
    }
    r
}

/// Resolve writes a fixed `output` texture (stable view for the post stack) and
/// then copies it into `history` for the next frame.
pub struct TaaPass {
    bgl: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    history: Option<wgpu::Texture>,
    output: Option<(wgpu::Texture, wgpu::TextureView)>,
    bind_group: Option<wgpu::BindGroup>,
    width: u32,
    height: u32,
}

impl TaaPass {
    pub fn new(device: &wgpu::Device, frame_bgl: &wgpu::BindGroupLayout, frame_wgsl: &str) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("taa"),
            source: wgpu::ShaderSource::Wgsl(format!("{frame_wgsl}\n{SHADER_TAA}").into()),
        });
        let tex = |binding: u32, sample_type: wgpu::TextureSampleType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("taa-bgl"),
            entries: &[
                tex(0, wgpu::TextureSampleType::Float { filterable: true }),
                tex(1, wgpu::TextureSampleType::Float { filterable: true }),
                tex(2, wgpu::TextureSampleType::Float { filterable: true }),
                tex(3, wgpu::TextureSampleType::Depth),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("taa-layout"),
            bind_group_layouts: &[frame_bgl, &bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("taa-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
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
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("taa-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self {
            bgl,
            pipeline,
            sampler,
            history: None,
            output: None,
            bind_group: None,
            width: 1,
            height: 1,
        }
    }

    /// Bind the scene inputs; history / output targets are only recreated when the size changes.
    pub fn rebind(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        current: &wgpu::TextureView,
        velocity: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let (width, height) = (width.max(1), height.max(1));
        if self.output.is_none() || self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            let make = |label: &str, extra: wgpu::TextureUsages| {
                device.create_texture(&wgpu::TextureDescriptor {
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
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | extra,
                    view_formats: &[],
                })
            };
            let history = make("taa-history", wgpu::TextureUsages::COPY_DST);
            let output = make("taa-output", wgpu::TextureUsages::COPY_SRC);
            let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
            self.history = Some(history);
            self.output = Some((output, output_view));
        }
        let history_view = self
            .history
            .as_ref()
            .expect("history")
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(current),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&history_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(velocity),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
    }

    /// Resolved frame; the view stays valid until the next [`Self::rebind`].
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.output.as_ref().map(|(_, v)| v)
    }

    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder, frame_bind_group: &wgpu::BindGroup) {
        let (Some(bg), Some((output, output_view)), Some(history)) =
            (&self.bind_group, &self.output, &self.history)
        else {
            return;
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("taa-resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, frame_bind_group, &[]);
            pass.set_bind_group(1, bg, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_texture(
            output.as_image_copy(),
            history.as_image_copy(),
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_stays_within_half_pixel_and_varies() {
        let mut seen = std::collections::HashSet::new();
        for f in 0..JITTER_SAMPLES {
            let j = jitter_pixels(f);
            assert!(j[0].abs() <= 0.5 && j[1].abs() <= 0.5, "{j:?}");
            seen.insert((j[0].to_bits(), j[1].to_bits()));
        }
        assert_eq!(seen.len(), JITTER_SAMPLES as usize);
        assert_eq!(jitter_pixels(0), jitter_pixels(JITTER_SAMPLES));
    }
}
