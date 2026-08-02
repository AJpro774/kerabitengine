// Camera-facing particle billboards (M1). Soft circular alpha, additive-friendly.

struct Frame {
    view_proj: mat4x4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;

struct Particle {
    @location(0) pos_size: vec4<f32>,   // xyz world, w size
    @location(1) color: vec4<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    p: Particle,
) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let c = corners[vi];
    let world = p.pos_size.xyz
        + frame.camera_right.xyz * (c.x * p.pos_size.w)
        + frame.camera_up.xyz * (c.y * p.pos_size.w);
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.color = p.color;
    out.uv = uvs[vi];
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let d = length(in.uv * 2.0 - 1.0);
    let alpha = smoothstep(1.0, 0.35, d) * in.color.a;
    return vec4<f32>(in.color.rgb, alpha);
}
