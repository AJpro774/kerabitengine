// Shared frame block — prepended to every scene shader by the Rust side.
// Layout must match `uniforms.rs::FrameUniforms`.

struct FrameUniforms {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    inv_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    ambient: vec4<f32>,        // rgb ambient, w env intensity
    params: vec4<f32>,         // near, far, dir count, point count
    screen: vec4<f32>,         // w, h, 1/w, 1/h
    cluster: vec4<f32>,        // tiles x, tiles y, slices, slices / ln(far/near)
    jitter: vec4<f32>,         // xy current jitter (NDC)
    cascade_vp: array<mat4x4<f32>, 4>,
    cascade_splits: vec4<f32>,
    shadow_params: vec4<f32>,  // bias, 1/size, cascade count, blend band
    sh: array<vec4<f32>, 9>,
    flags: vec4<f32>,          // ssao, ibl, ssr, taa
}

struct GpuLight {
    pos_or_dir: vec4<f32>,   // xyz + w kind (0 dir, 1 point)
    color_range: vec4<f32>,  // rgb * intensity, w range
}

@group(0) @binding(0) var<uniform> frame: FrameUniforms;

// View-space position of a pixel from its non-linear depth and screen UV (y down).
fn view_pos_from_depth(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let p = frame.inv_proj * ndc;
    return p.xyz / p.w;
}

fn world_pos_from_depth(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let p = frame.inv_view_proj * ndc;
    return p.xyz / p.w;
}

// Fullscreen triangle helper shared by post-style passes.
struct FullscreenOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

fn fullscreen_vertex(vi: u32) -> FullscreenOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = pos[vi];
    var out: FullscreenOut;
    out.position = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 1.0 - (p.y * 0.5 + 0.5));
    return out;
}
