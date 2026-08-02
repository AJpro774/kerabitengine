// Post stack (M1): bright extract → separable blur → tonemap + bloom composite.

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = pos[vi];
    var out: VsOut;
    out.position = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 1.0 - (p.y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_extract(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(src_tex, src_samp, in.uv).rgb;
    let brightness = max(c.r, max(c.g, c.b));
    let kn = max(brightness - 0.85, 0.0);
    return vec4<f32>(c * kn * 1.5, 1.0);
}

struct BlurParams {
    // xy = texel size * direction (1,0) or (0,1)
    dir: vec4<f32>,
}

@group(1) @binding(0) var<uniform> blur: BlurParams;

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let d = blur.dir.xy;
    var c = textureSample(src_tex, src_samp, in.uv).rgb * 0.227027;
    c += textureSample(src_tex, src_samp, in.uv + d * 1.384615).rgb * 0.316216;
    c += textureSample(src_tex, src_samp, in.uv - d * 1.384615).rgb * 0.316216;
    c += textureSample(src_tex, src_samp, in.uv + d * 3.230769).rgb * 0.070270;
    c += textureSample(src_tex, src_samp, in.uv - d * 3.230769).rgb * 0.070270;
    return vec4<f32>(c, 1.0);
}

@group(0) @binding(2) var bloom_tex: texture_2d<f32>;

fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let hdr = textureSample(src_tex, src_samp, in.uv).rgb;
    let bloom = textureSample(bloom_tex, src_samp, in.uv).rgb;
    let mapped = aces_tonemap(hdr + bloom * 0.65);
    // Surface is often sRGB; leave gamma to the swapchain when format is *-srgb.
    return vec4<f32>(mapped, 1.0);
}
