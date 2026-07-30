//! CPU→GPU albedo texture cache (P5).

/// Handle into [`TextureCache`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub(crate) u32);

struct GpuTexture {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// Uploads RGBA8 albedo maps and builds per-texture bind groups (texture + sampler).
pub struct TextureCache {
    textures: Vec<GpuTexture>,
    white: TextureId,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl TextureCache {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("albedo-tex-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("albedo-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let mut cache = Self {
            textures: Vec::new(),
            white: TextureId(0),
            bind_group_layout,
            sampler,
        };
        cache.white = cache.upload_rgba8(device, queue, 1, 1, &[255, 255, 255, 255]);
        cache
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn white(&self) -> TextureId {
        self.white
    }

    /// Upload tightly packed RGBA8 (`width * height * 4` bytes).
    pub fn upload_rgba8(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> TextureId {
        let width = width.max(1);
        let height = height.max(1);
        let expected = width as usize * height as usize * 4;
        assert_eq!(
            rgba.len(),
            expected,
            "upload_rgba8: expected {expected} bytes, got {}",
            rgba.len()
        );

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let gpu_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("albedo"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = gpu_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("albedo-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let id = TextureId(self.textures.len() as u32);
        self.textures.push(GpuTexture {
            _texture: gpu_tex,
            bind_group,
        });
        id
    }

    pub fn bind_group(&self, id: TextureId) -> Option<&wgpu::BindGroup> {
        self.textures.get(id.0 as usize).map(|t| &t.bind_group)
    }
}
