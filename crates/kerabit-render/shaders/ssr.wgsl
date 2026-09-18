// Specular resolve: screen-space reflections with Hi-Z assisted marching,
// falling back to the prefiltered environment where the ray misses.
// out = lit_color + spec_weight * mix(ibl, ssr, confidence)

@group(1) @binding(0) var color_tex: texture_2d<f32>;
@group(1) @binding(1) var spec_tex: texture_2d<f32>;
@group(1) @binding(2) var depth_tex: texture_depth_2d;
@group(1) @binding(3) var normal_tex: texture_2d<f32>;
@group(1) @binding(4) var hiz_tex: texture_2d<f32>;
@group(1) @binding(5) var lin_samp: sampler;
@group(1) @binding(6) var env_specular: texture_cube<f32>;

const COARSE_LEVEL: i32 = 2;
const COARSE_STEPS: i32 = 40;
const REFINE_STEPS: i32 = 6;
const THICKNESS: f32 = 0.6;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    return fullscreen_vertex(vi);
}

fn full_coord(uv: vec2<f32>) -> vec2<i32> {
    let size = vec2<i32>(frame.screen.xy);
    return clamp(vec2<i32>(uv * frame.screen.xy), vec2<i32>(0), size - vec2<i32>(1));
}

fn hiz_depth(uv: vec2<f32>, level: i32) -> f32 {
    let size = vec2<i32>(textureDimensions(hiz_tex, level));
    let c = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - vec2<i32>(1));
    return textureLoad(hiz_tex, c, level).r;
}

fn depth_at(uv: vec2<f32>) -> f32 {
    return textureLoad(depth_tex, full_coord(uv), 0);
}

struct Projected {
    uv: vec2<f32>,
    depth: f32,
    valid: bool,
}

fn project(p_view: vec3<f32>) -> Projected {
    let clip = frame.proj * vec4<f32>(p_view, 1.0);
    var out: Projected;
    out.valid = clip.w > 1e-4;
    let ndc = clip.xyz / max(clip.w, 1e-4);
    out.uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    out.depth = ndc.z;
    return out;
}

fn view_z_from_depth(uv: vec2<f32>, depth: f32) -> f32 {
    return view_pos_from_depth(uv, depth).z;
}

@fragment
fn fs_main(in: FullscreenOut) -> @location(0) vec4<f32> {
    let depth = depth_at(in.uv);
    let color = textureSample(color_tex, lin_samp, in.uv).rgb;
    if (depth >= 1.0) {
        return vec4<f32>(color, 1.0);
    }
    let weight = textureSample(spec_tex, lin_samp, in.uv).rgb;
    if (max(weight.r, max(weight.g, weight.b)) <= 1e-4) {
        return vec4<f32>(color, 1.0);
    }
    let nr = textureLoad(normal_tex, full_coord(in.uv), 0);
    let n_world = normalize(nr.xyz);
    let roughness = nr.w;

    let p_view = view_pos_from_depth(in.uv, depth);
    let p_world = world_pos_from_depth(in.uv, depth);
    let v_world = normalize(frame.camera_pos.xyz - p_world);
    let r_world = reflect(-v_world, n_world);
    let mips = f32(textureNumLevels(env_specular) - 1u);
    let ibl = textureSampleLevel(env_specular, lin_samp, r_world, roughness * mips).rgb;

    var reflected = ibl;
    if (frame.flags.z > 0.5 && roughness < 0.8) {
        let n_view = normalize((frame.view * vec4<f32>(n_world, 0.0)).xyz);
        let v_view = normalize(-p_view);
        let r_view = reflect(-v_view, n_view);
        // Rays toward the camera almost never hit visible geometry.
        let facing = dot(r_view, v_view);
        if (facing < 0.98) {
            let max_dist = 60.0;
            let step0 = max(0.08, -p_view.z * 0.02);
            var t = step0;
            var t_prev = 0.0;
            var hit = false;
            var hit_uv = in.uv;
            var i = 0;
            loop {
                if (i >= COARSE_STEPS || t > max_dist) { break; }
                let s = p_view + r_view * t;
                let pr = project(s);
                if (!pr.valid || pr.uv.x < 0.0 || pr.uv.x > 1.0 || pr.uv.y < 0.0 || pr.uv.y > 1.0) { break; }
                let scene_min = hiz_depth(pr.uv, COARSE_LEVEL);
                if (pr.depth > scene_min) {
                    // Potential hit inside this block: refine against full-res depth.
                    var lo = t_prev;
                    var hi = t;
                    for (var k = 0; k < REFINE_STEPS; k = k + 1) {
                        let mid = (lo + hi) * 0.5;
                        let sm = project(p_view + r_view * mid);
                        if (sm.depth > depth_at(sm.uv)) { hi = mid; } else { lo = mid; }
                    }
                    let sh = project(p_view + r_view * hi);
                    let scene_z = view_z_from_depth(sh.uv, depth_at(sh.uv));
                    let ray_z = (p_view + r_view * hi).z;
                    if (abs(scene_z - ray_z) < THICKNESS * (1.0 + hi * 0.05)) {
                        hit = true;
                        hit_uv = sh.uv;
                    }
                    if (hit) { break; }
                }
                t_prev = t;
                t = t * 1.18 + step0;
                i = i + 1;
            }
            if (hit) {
                let ssr = textureSample(color_tex, lin_samp, hit_uv).rgb;
                let edge = smoothstep(0.0, 0.1, hit_uv.x) * smoothstep(0.0, 0.1, 1.0 - hit_uv.x)
                    * smoothstep(0.0, 0.1, hit_uv.y) * smoothstep(0.0, 0.1, 1.0 - hit_uv.y);
                let rough_fade = 1.0 - smoothstep(0.4, 0.8, roughness);
                let confidence = edge * rough_fade;
                reflected = mix(ibl, ssr, confidence);
            }
        }
    }
    return vec4<f32>(color + weight * reflected, 1.0);
}
