// Fullscreen sky gradient (NDC Y: bottom → top). No vertex buffer — hard-coded triangle.

struct SkyUniforms {
    top: vec4<f32>,
    bottom: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sky: SkyUniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc_y: f32,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Fullscreen triangle covering clip space.
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VertexOutput;
    let p = pos[idx];
    out.clip_position = vec4<f32>(p, 1.0, 1.0); // far plane so lit geometry wins depth
    out.ndc_y = p.y;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = clamp(in.ndc_y * 0.5 + 0.5, 0.0, 1.0);
    // Smoothstep for a softer horizon band.
    let w = t * t * (3.0 - 2.0 * t);
    return vec4<f32>(mix(sky.bottom.xyz, sky.top.xyz, w), 1.0);
}
