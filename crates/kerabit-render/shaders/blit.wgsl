// Fullscreen blit: sample an offscreen color target into the egui pass.

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // Fullscreen triangle covering clip space.
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
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(src_tex, src_sampler, in.uv);
}
