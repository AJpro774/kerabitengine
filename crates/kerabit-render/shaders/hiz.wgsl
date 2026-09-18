// Hierarchical-Z pyramid: level 0 copies scene depth; each further level keeps
// the *closest* (minimum) depth of a 2×2 block so a ray that stays in front of
// a coarse texel can skip the whole block.

@group(0) @binding(0) var src_depth: texture_depth_2d;
@group(0) @binding(1) var src_hiz: texture_2d<f32>;

struct HizParams {
    // x = 1 when reading `src_depth` (level 0), 0 when reading `src_hiz`.
    mode: vec4<f32>,
}
@group(0) @binding(2) var<uniform> hiz: HizParams;

struct VsOut {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    out.position = vec4<f32>(pos[vi], 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dst = vec2<i32>(in.position.xy);
    if (hiz.mode.x > 0.5) {
        let size = vec2<i32>(textureDimensions(src_depth));
        let c = clamp(dst, vec2<i32>(0), size - vec2<i32>(1));
        return vec4<f32>(textureLoad(src_depth, c, 0), 0.0, 0.0, 1.0);
    }
    let size = vec2<i32>(textureDimensions(src_hiz));
    let base = dst * 2;
    var d = 1.0;
    for (var y = 0; y < 2; y = y + 1) {
        for (var x = 0; x < 2; x = x + 1) {
            let c = clamp(base + vec2<i32>(x, y), vec2<i32>(0), size - vec2<i32>(1));
            d = min(d, textureLoad(src_hiz, c, 0).r);
        }
    }
    return vec4<f32>(d, 0.0, 0.0, 1.0);
}
