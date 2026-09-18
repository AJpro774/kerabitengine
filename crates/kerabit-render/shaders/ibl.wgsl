// IBL precompute: equirect / procedural sky → cube face, GGX prefilter per
// roughness mip, and the split-sum BRDF LUT.

struct FaceParams {
    params: vec4<f32>,      // face, roughness, source mode (0 equirect, 1 sky), sample count
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    sky_ground: vec4<f32>,
}

@group(0) @binding(0) var<uniform> fp: FaceParams;
@group(0) @binding(1) var equirect: texture_2d<f32>;
@group(0) @binding(2) var source_cube: texture_cube<f32>;
@group(0) @binding(3) var samp: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

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

// Direction for a texel of cube face `face` (WebGPU cube face order +X -X +Y -Y +Z -Z).
fn face_dir(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let a = uv.x * 2.0 - 1.0;
    let b = 1.0 - uv.y * 2.0;
    switch face {
        case 0u: { return normalize(vec3<f32>(1.0, b, -a)); }
        case 1u: { return normalize(vec3<f32>(-1.0, b, a)); }
        case 2u: { return normalize(vec3<f32>(a, 1.0, -b)); }
        case 3u: { return normalize(vec3<f32>(a, -1.0, b)); }
        case 4u: { return normalize(vec3<f32>(a, b, 1.0)); }
        default: { return normalize(vec3<f32>(-a, b, -1.0)); }
    }
}

fn equirect_uv(d: vec3<f32>) -> vec2<f32> {
    let phi = atan2(d.x, -d.z);
    let theta = acos(clamp(d.y, -1.0, 1.0));
    return vec2<f32>(phi / 6.2831853 + 0.5, theta / 3.14159265);
}

fn sky_radiance(d: vec3<f32>) -> vec3<f32> {
    if (d.y >= 0.0) {
        let t = clamp(d.y, 0.0, 1.0);
        let w = t * t * (3.0 - 2.0 * t);
        return mix(fp.sky_bottom.xyz, fp.sky_top.xyz, w);
    }
    return mix(fp.sky_bottom.xyz, fp.sky_ground.xyz, sqrt(clamp(-d.y, 0.0, 1.0)));
}

@fragment
fn fs_to_cube(in: VsOut) -> @location(0) vec4<f32> {
    let d = face_dir(u32(fp.params.x), in.uv);
    if (fp.params.z > 0.5) {
        return vec4<f32>(sky_radiance(d), 1.0);
    }
    return vec4<f32>(textureSampleLevel(equirect, samp, equirect_uv(d), 0.0).rgb, 1.0);
}

fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}

fn hammersley(i: u32, n: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(n), radical_inverse_vdc(i));
}

fn importance_sample_ggx(xi: vec2<f32>, n: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 6.2831853 * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    let h = vec3<f32>(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(n.z) < 0.999);
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    return normalize(t * h.x + b * h.y + n * h.z);
}

@fragment
fn fs_prefilter(in: VsOut) -> @location(0) vec4<f32> {
    let n = face_dir(u32(fp.params.x), in.uv);
    let roughness = fp.params.y;
    let count = u32(max(fp.params.w, 1.0));
    if (count <= 1u || roughness <= 0.001) {
        return vec4<f32>(textureSampleLevel(source_cube, samp, n, 0.0).rgb, 1.0);
    }
    var color = vec3<f32>(0.0);
    var weight = 0.0;
    for (var i = 0u; i < count; i = i + 1u) {
        let xi = hammersley(i, count);
        let h = importance_sample_ggx(xi, n, roughness);
        let l = normalize(2.0 * dot(n, h) * h - n);
        let n_dot_l = max(dot(n, l), 0.0);
        if (n_dot_l > 0.0) {
            // Sample a blurrier source mip for rough lobes to reduce fireflies.
            color += textureSampleLevel(source_cube, samp, l, 0.0).rgb * n_dot_l;
            weight += n_dot_l;
        }
    }
    return vec4<f32>(color / max(weight, 1e-4), 1.0);
}

fn geometry_schlick_ggx_ibl(n_dot_v: f32, roughness: f32) -> f32 {
    let k = (roughness * roughness) / 2.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k);
}

@fragment
fn fs_brdf(in: VsOut) -> @location(0) vec4<f32> {
    let n_dot_v = max(in.uv.x, 1e-3);
    let roughness = max(in.uv.y, 0.02);
    let v = vec3<f32>(sqrt(1.0 - n_dot_v * n_dot_v), 0.0, n_dot_v);
    let n = vec3<f32>(0.0, 0.0, 1.0);
    var a = 0.0;
    var b = 0.0;
    let count = 128u;
    for (var i = 0u; i < count; i = i + 1u) {
        let xi = hammersley(i, count);
        let h = importance_sample_ggx(xi, n, roughness);
        let l = normalize(2.0 * dot(v, h) * h - v);
        let n_dot_l = max(l.z, 0.0);
        let n_dot_h = max(h.z, 0.0);
        let v_dot_h = max(dot(v, h), 0.0);
        if (n_dot_l > 0.0) {
            let g = geometry_schlick_ggx_ibl(n_dot_v, roughness) * geometry_schlick_ggx_ibl(n_dot_l, roughness);
            let g_vis = (g * v_dot_h) / (n_dot_h * n_dot_v + 1e-5);
            let fc = pow(1.0 - v_dot_h, 5.0);
            a += (1.0 - fc) * g_vis;
            b += fc * g_vis;
        }
    }
    return vec4<f32>(a / f32(count), b / f32(count), 0.0, 1.0);
}
