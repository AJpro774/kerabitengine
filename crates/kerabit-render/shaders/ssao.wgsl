// Half-resolution SSAO from the G-buffer (depth + world normal), then a
// depth-aware blur. Output R channel = visibility (1 = unoccluded).

struct SsaoKernel {
    samples: array<vec4<f32>, 16>,
    params: vec4<f32>, // radius, bias, power, unused
}

@group(1) @binding(0) var depth_tex: texture_depth_2d;
@group(1) @binding(1) var normal_tex: texture_2d<f32>;
@group(1) @binding(2) var<uniform> kernel: SsaoKernel;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    return fullscreen_vertex(vi);
}

fn load_depth(uv: vec2<f32>) -> f32 {
    let size = vec2<i32>(frame.screen.xy);
    let c = clamp(vec2<i32>(uv * frame.screen.xy), vec2<i32>(0), size - vec2<i32>(1));
    return textureLoad(depth_tex, c, 0);
}

// Interleaved gradient noise (Jimenez) — stable per pixel, no texture needed.
fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

@fragment
fn fs_ao(in: FullscreenOut) -> @location(0) vec4<f32> {
    let depth = load_depth(in.uv);
    if (depth >= 1.0) {
        return vec4<f32>(1.0);
    }
    let full = clamp(vec2<i32>(in.uv * frame.screen.xy), vec2<i32>(0), vec2<i32>(frame.screen.xy) - vec2<i32>(1));
    let n_world = textureLoad(normal_tex, full, 0).xyz;
    let n = normalize((frame.view * vec4<f32>(n_world, 0.0)).xyz);
    let p = view_pos_from_depth(in.uv, depth);

    // Random rotation around the normal.
    let angle = ign(in.position.xy) * 6.2831853;
    let rnd = vec3<f32>(cos(angle), sin(angle), 0.0);
    let t = normalize(rnd - n * dot(rnd, n));
    let b = cross(n, t);
    let tbn = mat3x3<f32>(t, b, n);

    let radius = kernel.params.x;
    let bias = kernel.params.y;
    var occlusion = 0.0;
    for (var i = 0; i < 16; i = i + 1) {
        let s = p + tbn * kernel.samples[i].xyz * radius;
        let clip = frame.proj * vec4<f32>(s, 1.0);
        let ndc = clip.xy / clip.w;
        let suv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        if (suv.x < 0.0 || suv.x > 1.0 || suv.y < 0.0 || suv.y > 1.0) {
            continue;
        }
        let sd = load_depth(suv);
        let scene_z = view_pos_from_depth(suv, sd).z;
        // View z is negative; scene surface in front of the sample means occluded.
        let range = smoothstep(0.0, 1.0, radius / max(abs(p.z - scene_z), 1e-4));
        occlusion += select(0.0, 1.0, scene_z >= s.z + bias) * range;
    }
    let ao = pow(1.0 - occlusion / 16.0, kernel.params.z);
    return vec4<f32>(ao, ao, ao, 1.0);
}

@group(1) @binding(3) var ao_src: texture_2d<f32>;
@group(1) @binding(4) var lin_samp: sampler;

@fragment
fn fs_blur(in: FullscreenOut) -> @location(0) vec4<f32> {
    let center_d = load_depth(in.uv);
    let texel = frame.screen.zw * 2.0; // half-res texel in uv
    var sum = 0.0;
    var wsum = 0.0;
    for (var y = -2; y <= 1; y = y + 1) {
        for (var x = -2; x <= 1; x = x + 1) {
            let o = vec2<f32>(f32(x) + 0.5, f32(y) + 0.5) * texel;
            let uv = in.uv + o;
            let d = load_depth(uv);
            let w = exp(-abs(d - center_d) * 400.0);
            sum += textureSample(ao_src, lin_samp, uv).r * w;
            wsum += w;
        }
    }
    let ao = sum / max(wsum, 1e-4);
    return vec4<f32>(ao, ao, ao, 1.0);
}
