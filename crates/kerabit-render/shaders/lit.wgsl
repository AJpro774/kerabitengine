// Lit pass (3.0): PBR (albedo / roughness / metallic + optional normal map),
// clustered point lights, up to 4 directional lights with 4-cascade PCF shadows
// from the first, SSAO on ambient / IBL, SH irradiance. Specular IBL is written
// to a second target so the SSR resolve can replace it where a ray hits.
// Vertex layout (frozen): position f32x3, normal f32x3, uv f32x2.

@group(0) @binding(1) var<storage, read> lights: array<GpuLight>;
@group(0) @binding(2) var<storage, read> grid: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read> light_indices: array<u32>;

@group(1) @binding(0) var albedo_tex: texture_2d<f32>;
@group(1) @binding(1) var tex_samp: sampler;
@group(1) @binding(2) var normal_tex: texture_2d<f32>;

@group(2) @binding(0) var shadow_map: texture_depth_2d_array;
@group(2) @binding(1) var shadow_samp: sampler_comparison;
@group(2) @binding(2) var ao_tex: texture_2d<f32>;
@group(2) @binding(3) var lin_samp: sampler;
@group(2) @binding(4) var brdf_lut: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct InstanceInput {
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(7) prev_0: vec4<f32>,
    @location(8) prev_1: vec4<f32>,
    @location(9) prev_2: vec4<f32>,
    @location(10) prev_3: vec4<f32>,
    @location(11) albedo: vec4<f32>,
    @location(12) params: vec4<f32>, // x roughness, y metallic
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) albedo: vec4<f32>,
    @location(4) roughness: f32,
    @location(5) metallic: f32,
}

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = frame.view_proj * world_pos;
    out.world_pos = world_pos.xyz;
    let n_mat = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = n_mat * in.normal;
    out.uv = in.uv;
    out.albedo = instance.albedo;
    out.roughness = instance.params.x;
    out.metallic = instance.params.y;
    return out;
}

// --- Shadows ---------------------------------------------------------------

fn sample_cascade(index: i32, world_pos: vec3<f32>, ndotl: f32) -> f32 {
    let light_clip = frame.cascade_vp[index] * vec4<f32>(world_pos, 1.0);
    let ndc = light_clip.xyz / light_clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, ndc.y * -0.5 + 0.5);
    let depth = ndc.z;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || depth < 0.0 || depth > 1.0) {
        return 1.0;
    }
    let inv_size = frame.shadow_params.y;
    // Farther cascades cover more world per texel; scale the bias with them.
    let bias = (frame.shadow_params.x + (1.0 - ndotl) * 0.0015) * (1.0 + f32(index) * 0.75);
    var shadow = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * inv_size;
            shadow += textureSampleCompare(shadow_map, shadow_samp, uv + offset, index, depth - bias);
        }
    }
    return shadow / 9.0;
}

fn sample_shadow(world_pos: vec3<f32>, view_depth: f32, ndotl: f32) -> f32 {
    let count = i32(frame.shadow_params.z);
    var c = 0;
    for (var i = 0; i < 3; i = i + 1) {
        if (view_depth > frame.cascade_splits[i]) {
            c = i + 1;
        }
    }
    c = min(c, count - 1);
    var shadow = sample_cascade(c, world_pos, ndotl);
    // Blend into the next cascade near the split to hide the seam.
    let band = frame.shadow_params.w;
    if (c < count - 1) {
        let split = frame.cascade_splits[c];
        let t = clamp((view_depth - (split - band)) / band, 0.0, 1.0);
        if (t > 0.0) {
            shadow = mix(shadow, sample_cascade(c + 1, world_pos, ndotl), t);
        }
    }
    return shadow;
}

// --- Material ----------------------------------------------------------------

// Derivative-based TBN so we keep the frozen vertex layout (no tangents).
fn apply_normal_map(n: vec3<f32>, world_pos: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let map = textureSample(normal_tex, tex_samp, uv).xyz * 2.0 - 1.0;
    if (abs(map.x) < 0.02 && abs(map.y) < 0.02 && map.z > 0.98) {
        return n;
    }
    let dp1 = dpdx(world_pos);
    let dp2 = dpdy(world_pos);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let dp2perp = cross(dp2, n);
    let dp1perp = cross(n, dp1);
    var t = dp2perp * duv1.x + dp1perp * duv2.x;
    var b = dp2perp * duv1.y + dp1perp * duv2.y;
    let inv_max = inverseSqrt(max(dot(t, t), dot(b, b)));
    t = t * inv_max;
    b = b * inv_max;
    return normalize(t * map.x + b * map.y + n * map.z);
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (3.14159265 * d * d + 1e-5);
}

fn geometry_schlick_ggx(n_dot_x: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_x / (n_dot_x * (1.0 - k) + k + 1e-5);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

fn fresnel_schlick_roughness(cos_theta: f32, f0: vec3<f32>, roughness: f32) -> vec3<f32> {
    let fr = max(vec3<f32>(1.0 - roughness), f0) - f0;
    return f0 + fr * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

fn shade_light(
    n: vec3<f32>,
    v: vec3<f32>,
    l: vec3<f32>,
    radiance: vec3<f32>,
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32,
    shadow: f32,
) -> vec3<f32> {
    let n_dot_l = max(dot(n, l), 0.0);
    if (n_dot_l <= 0.0 || shadow <= 0.0) {
        return vec3<f32>(0.0);
    }
    let h = normalize(v + l);
    let n_dot_h = max(dot(n, h), 0.0);
    let n_dot_v = max(dot(n, v), 0.0);
    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    let d = distribution_ggx(n_dot_h, roughness);
    let g = geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
    let f = fresnel_schlick(max(dot(h, v), 0.0), f0);
    let specular = (d * g * f) / (4.0 * n_dot_v * n_dot_l + 1e-5);
    let kd = (vec3<f32>(1.0) - f) * (1.0 - metallic);
    let diffuse = kd * albedo / 3.14159265;
    return (diffuse + specular) * radiance * n_dot_l * shadow;
}

// Irradiance from 9 SH coefficients (Ramamoorthi & Hanrahan).
fn sh_irradiance(n: vec3<f32>) -> vec3<f32> {
    let c1 = 0.429043;
    let c2 = 0.511664;
    let c3 = 0.743125;
    let c4 = 0.886227;
    let c5 = 0.247708;
    let l00 = frame.sh[0].xyz;
    let l1m1 = frame.sh[1].xyz;
    let l10 = frame.sh[2].xyz;
    let l11 = frame.sh[3].xyz;
    let l2m2 = frame.sh[4].xyz;
    let l2m1 = frame.sh[5].xyz;
    let l20 = frame.sh[6].xyz;
    let l21 = frame.sh[7].xyz;
    let l22 = frame.sh[8].xyz;
    let e = c1 * l22 * (n.x * n.x - n.y * n.y)
        + c3 * l20 * n.z * n.z
        + c4 * l00
        - c5 * l20
        + 2.0 * c1 * (l2m2 * n.x * n.y + l21 * n.x * n.z + l2m1 * n.y * n.z)
        + 2.0 * c2 * (l11 * n.x + l1m1 * n.y + l10 * n.z);
    return max(e, vec3<f32>(0.0));
}

struct FsOut {
    @location(0) color: vec4<f32>,
    @location(1) specular: vec4<f32>,
}

@fragment
fn fs_main(in: VertexOutput) -> FsOut {
    var n = normalize(in.world_normal);
    n = apply_normal_map(n, in.world_pos, in.uv);

    let tex = textureSample(albedo_tex, tex_samp, in.uv);
    let base = in.albedo * tex;
    let albedo = base.xyz;
    let roughness = clamp(in.roughness, 0.04, 1.0);
    let metallic = clamp(in.metallic, 0.0, 1.0);

    let v = normalize(frame.camera_pos.xyz - in.world_pos);
    let n_dot_v = max(dot(n, v), 1e-4);
    let view_pos = (frame.view * vec4<f32>(in.world_pos, 1.0)).xyz;
    let view_depth = -view_pos.z;
    let screen_uv = in.clip_position.xy * frame.screen.zw;

    var ao = 1.0;
    if (frame.flags.x > 0.5) {
        ao = textureSample(ao_tex, lin_samp, screen_uv).r;
    }

    var color = frame.ambient.xyz * albedo * ao;

    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    let env_strength = frame.ambient.w * frame.flags.y;
    // Specular *weight* only (Fresnel × split-sum BRDF × AO × intensity); the
    // reflected radiance (IBL or SSR) is multiplied in by the specular resolve.
    var specular = vec3<f32>(0.0);
    if (env_strength > 0.0) {
        let ks = fresnel_schlick_roughness(n_dot_v, f0, roughness);
        let kd = (vec3<f32>(1.0) - ks) * (1.0 - metallic);
        color += kd * sh_irradiance(n) * albedo * ao * env_strength;
        let brdf = textureSample(brdf_lut, lin_samp, vec2<f32>(n_dot_v, roughness)).rg;
        specular = (ks * brdf.x + brdf.y) * ao * env_strength;
    }

    // Directional lights (first one casts cascaded shadows).
    let dir_count = i32(frame.params.z);
    for (var i = 0; i < 4; i = i + 1) {
        if (i >= dir_count) {
            break;
        }
        let light = lights[i];
        let l = normalize(-light.pos_or_dir.xyz);
        var shadow = 1.0;
        if (i == 0) {
            shadow = sample_shadow(in.world_pos, view_depth, max(dot(n, l), 0.0));
        }
        color += shade_light(n, v, l, light.color_range.xyz, albedo, roughness, metallic, shadow);
    }

    // Clustered point lights.
    let tile = vec2<u32>(in.clip_position.xy / (frame.screen.xy / frame.cluster.xy));
    let slice_f = log(max(view_depth, frame.params.x) / frame.params.x) * frame.cluster.w;
    let slice = min(u32(max(slice_f, 0.0)), u32(frame.cluster.z) - 1u);
    let tiles_x = u32(frame.cluster.x);
    let tiles_y = u32(frame.cluster.y);
    let cluster = min(tile.x, tiles_x - 1u) + min(tile.y, tiles_y - 1u) * tiles_x + slice * tiles_x * tiles_y;
    let range = grid[cluster];
    for (var k = 0u; k < range.y; k = k + 1u) {
        let light = lights[light_indices[range.x + k]];
        let to_light = light.pos_or_dir.xyz - in.world_pos;
        let dist = length(to_light);
        let l = to_light / max(dist, 1e-4);
        let lrange = max(light.color_range.w, 0.1);
        let atten = clamp(1.0 - dist / lrange, 0.0, 1.0);
        let radiance = light.color_range.xyz * (atten * atten);
        color += shade_light(n, v, l, radiance, albedo, roughness, metallic, 1.0);
    }

    var out: FsOut;
    out.color = vec4<f32>(color, base.w);
    out.specular = vec4<f32>(specular, 1.0);
    return out;
}
