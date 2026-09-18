//! Light storage buffer + froxel clustering compute pass.
//!
//! Lights are uploaded directional-first; the compute shader bins point lights
//! into a `CLUSTER_X × CLUSTER_Y × CLUSTER_Z` grid that `lit.wgsl` indexes per
//! fragment. Both the lit pass (read-only) and the compute pass (read-write)
//! bind the same buffers through different layouts.

use crate::light::{Light, LightKind, MAX_DIRECTIONAL_LIGHTS, MAX_LIGHTS};
use crate::uniforms::{GpuLight, CLUSTER_COUNT, CLUSTER_Z, MAX_LIGHTS_PER_CLUSTER};

const SHADER_CLUSTER: &str = include_str!("../shaders/cluster.wgsl");

pub struct LightBuffers {
    pub lights: wgpu::Buffer,
    pub grid: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    scratch: Vec<GpuLight>,
}

impl LightBuffers {
    pub fn new(device: &wgpu::Device, frame_buffer: &wgpu::Buffer, frame_wgsl: &str) -> Self {
        let lights = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lights"),
            size: (std::mem::size_of::<GpuLight>() * MAX_LIGHTS) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let grid = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light-cluster-grid"),
            size: (CLUSTER_COUNT as usize * std::mem::size_of::<[u32; 2]>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light-cluster-indices"),
            size: (CLUSTER_COUNT as usize
                * MAX_LIGHTS_PER_CLUSTER as usize
                * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let compute_entry = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            visibility: wgpu::ShaderStages::COMPUTE,
            ..buffer_entry(binding, wgpu::BufferBindingType::Storage { read_only })
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cluster-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ..buffer_entry(0, wgpu::BufferBindingType::Uniform)
                },
                compute_entry(1, true),
                compute_entry(2, false),
                compute_entry(3, false),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cluster-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lights.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: grid.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: indices.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cluster"),
            source: wgpu::ShaderSource::Wgsl(format!("{frame_wgsl}\n{SHADER_CLUSTER}").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cluster-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cluster-pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            lights,
            grid,
            indices,
            pipeline,
            bind_group,
            scratch: Vec::with_capacity(MAX_LIGHTS),
        }
    }

    /// Upload `lights` (directional first). Returns `(directional, point)` counts.
    pub fn upload(&mut self, queue: &wgpu::Queue, lights: &[Light]) -> (u32, u32) {
        self.scratch.clear();
        let mut dirs = 0u32;
        for l in lights.iter().filter(|l| l.kind == LightKind::Directional) {
            if dirs as usize >= MAX_DIRECTIONAL_LIGHTS {
                break;
            }
            self.scratch.push(GpuLight::from_light(l));
            dirs += 1;
        }
        let mut points = 0u32;
        for l in lights.iter().filter(|l| l.kind == LightKind::Point) {
            if self.scratch.len() >= MAX_LIGHTS {
                break;
            }
            self.scratch.push(GpuLight::from_light(l));
            points += 1;
        }
        if self.scratch.is_empty() {
            self.scratch.push(GpuLight::empty());
        }
        queue.write_buffer(&self.lights, 0, bytemuck::cast_slice(&self.scratch));
        (dirs, points)
    }

    /// Rebuild the cluster light lists for the frame uniforms already uploaded.
    pub fn encode_cluster(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("light-cluster"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(1, 1, CLUSTER_Z);
    }
}

/// Frame-group buffer entry visible to the graphics stages (read-only storage / uniform).
pub(crate) fn buffer_entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
