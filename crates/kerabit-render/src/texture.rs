//! CPU→GPU texture cache with albedo + normal material bind groups (M1).

use std::collections::HashMap;

/// Handle into [`TextureCache`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub(crate) u32);

struct GpuTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Uploads RGBA8 maps and builds per-(albedo, normal) bind groups.
pub struct TextureCache {
    textures: Vec<GpuTexture>,
    white: TextureId,
    flat_normal: TextureId,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    material_bgs: HashMap<(TextureId, TextureId), wgpu::BindGroup>,
}

impl TextureCache {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material-tex-bgl"),
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
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material-sampler"),
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
            flat_normal: TextureId(0),
            bind_group_layout,
            sampler,
            material_bgs: HashMap::new(),
        };
        cache.white = cache.upload_rgba8(device, queue, 1, 1, &[255, 255, 255, 255]);
        // Tangent-space flat normal (0.5, 0.5, 1.0) in linear space.
        cache.flat_normal =
            cache.upload_rgba8_linear(device, queue, 1, 1, &[128, 128, 255, 255]);
        cache
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn white(&self) -> TextureId {
        self.white
    }

    pub fn flat_normal(&self) -> TextureId {
        self.flat_normal
    }

    /// Upload tightly packed RGBA8 sRGB albedo (`width * height * 4` bytes).
    pub fn upload_rgba8(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> TextureId {
        self.upload_inner(
            device,
            queue,
            width,
            height,
            rgba,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "albedo",
        )
    }

    /// Upload tightly packed RGBA8 linear (normal maps — not sRGB).
    pub fn upload_rgba8_linear(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> TextureId {
        self.upload_inner(
            device,
            queue,
            width,
            height,
            rgba,
            wgpu::TextureFormat::Rgba8Unorm,
            "normal",
        )
    }

    fn upload_inner(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
        format: wgpu::TextureFormat,
        label: &str,
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
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
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
        let id = TextureId(self.textures.len() as u32);
        self.textures.push(GpuTexture {
            _texture: gpu_tex,
            view,
        });
        id
    }

    /// Ensure a bind group for albedo + normal exists (creates if missing).
    pub fn ensure_material_bind_group(
        &mut self,
        device: &wgpu::Device,
        albedo: TextureId,
        normal: TextureId,
    ) -> bool {
        let key = (albedo, normal);
        if self.material_bgs.contains_key(&key) {
            return true;
        }
        if self.textures.get(albedo.0 as usize).is_none()
            || self.textures.get(normal.0 as usize).is_none()
        {
            return false;
        }
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[albedo.0 as usize].view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[normal.0 as usize].view,
                    ),
                },
            ],
        });
        self.material_bgs.insert(key, bg);
        true
    }

    /// Look up a previously ensured material bind group (`&self` for render-pass use).
    pub fn material_bind_group(
        &self,
        albedo: TextureId,
        normal: TextureId,
    ) -> Option<&wgpu::BindGroup> {
        self.material_bgs.get(&(albedo, normal))
    }
}
