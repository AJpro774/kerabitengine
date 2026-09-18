// Temporal anti-aliasing resolve: reproject history with motion vectors
// (camera reprojection for background), clamp to the 3×3 neighborhood,
// blend. With TAA disabled the pass is a plain copy.

@group(1) @binding(0) var current_tex: texture_2d<f32>;
@group(1) @binding(1) var history_tex: texture_2d<f32>;
@group(1) @binding(2) var velocity_tex: texture_2d<f32>;
@group(1) @binding(3) var depth_tex: texture_depth_2d;
@group(1) @binding(4) var lin_samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    return fullscreen_vertex(vi);
}

fn coord(uv: vec2<f32>) -> vec2<i32> {
    let size = vec2<i32>(frame.screen.xy);
    return clamp(vec2<i32>(uv * frame.screen.xy), vec2<i32>(0), size - vec2<i32>(1));
}

@fragment
fn fs_main(in: FullscreenOut) -> @location(0) vec4<f32> {
    let c = coord(in.uv);
    let current = textureLoad(current_tex, c, 0).rgb;
    if (frame.flags.w < 0.5) {
        return vec4<f32>(current, 1.0);
    }

    let depth = textureLoad(depth_tex, c, 0);
    var prev_uv: vec2<f32>;
    if (depth >= 1.0) {
        // Background: camera-only reprojection through the far plane.
        let world = world_pos_from_depth(in.uv, 1.0);
        let prev_clip = frame.prev_view_proj * vec4<f32>(world, 1.0);
        let prev_ndc = prev_clip.xy / max(prev_clip.w, 1e-4);
        prev_uv = vec2<f32>(prev_ndc.x * 0.5 + 0.5, 0.5 - prev_ndc.y * 0.5);
    } else {
        let vel = textureLoad(velocity_tex, c, 0).xy;
        // Current uv includes the jitter; remove it so static pixels line up.
        let unjittered = in.uv - frame.jitter.xy * vec2<f32>(0.5, -0.5);
        prev_uv = unjittered - vel;
    }
    if (prev_uv.x < 0.0 || prev_uv.x > 1.0 || prev_uv.y < 0.0 || prev_uv.y > 1.0) {
        return vec4<f32>(current, 1.0);
    }

    var history = textureSample(history_tex, lin_samp, prev_uv).rgb;

    // Neighborhood clamp against the current frame to reject stale history.
    var mn = current;
    var mx = current;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let s = textureLoad(current_tex, coord(in.uv + vec2<f32>(f32(x), f32(y)) * frame.screen.zw), 0).rgb;
            mn = min(mn, s);
            mx = max(mx, s);
        }
    }
    history = clamp(history, mn, mx);

    let blend = 0.9;
    return vec4<f32>(mix(current, history, blend), 1.0);
}
