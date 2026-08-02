// Lit pass (M1): PBR-lite (albedo / roughness / metallic + optional normal map)
// with up to 4 lights (directional + point). Soft shadows from first directional.
// Vertex layout (frozen): position f32x3, normal f32x3, uv f32x2.
// Instance layout: model columns @3–6, albedo @7, params (roughness, metallic) @8.

struct GpuLight {
    pos_or_dir: vec4<f32>,   // xyz + w=kind (0 dir, 1 point)
    color_range: vec4<f32>,  // rgb * intensity, w = range (point)
}

struct FrameUniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    ambient: vec4<f32>,
    light_view_proj: mat4x4<f32>,
    shadow_params: vec4<f32>, // x bias, y inv map size, z light count
    lights: array<GpuLight, 4>,
}

@group(0) @binding(0) var<uniform> frame: FrameUniforms;
@group(1) @binding(0) var albedo_tex: texture_2d<f32>;
@group(1) @binding(1) var tex_samp: sampler;
@group(1) @binding(2) var normal_tex: texture_2d<f32>;
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;

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
    @location(7) albedo: vec4<f32>,
    @location(8) params: vec4<f32>, // x = roughness, y = metallic
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
    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = frame.view_proj * world_pos;
    out.world_pos = world_pos.xyz;
    let n_mat = mat3x3<f32>(
        model[0].xyz,
        model[1].xyz,
        model[2].xyz,
    );
    out.world_normal = n_mat * in.normal;
    out.uv = in.uv;
    out.albedo = instance.albedo;
    out.roughness = instance.params.x;
    out.metallic = instance.params.y;
    return out;
}

fn sample_shadow(world_pos: vec3<f32>, ndotl: f32) -> f32 {
    let light_clip = frame.light_view_proj * vec4<f32>(world_pos, 1.0);
    let ndc = light_clip.xyz / light_clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, ndc.y * -0.5 + 0.5);
    let depth = ndc.z;

    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || depth < 0.0 || depth > 1.0) {
        return 1.0;
    }

    let inv_size = frame.shadow_params.y;
    let bias = frame.shadow_params.x + (1.0 - ndotl) * 0.0015;
    var shadow = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * inv_size;
            shadow += textureSampleCompare(shadow_map, shadow_samp, uv + offset, depth - bias);
        }
    }
    return shadow / 9.0;
}

// Derivative-based TBN so we keep the frozen vertex layout (no tangents).
fn apply_normal_map(n: vec3<f32>, world_pos: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let map = textureSample(normal_tex, tex_samp, uv).xyz * 2.0 - 1.0;
    // Flat default (0.5,0.5,1) → (0,0,1); skip work when nearly flat.
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var n = normalize(in.world_normal);
    n = apply_normal_map(n, in.world_pos, in.uv);

    let tex = textureSample(albedo_tex, tex_samp, in.uv);
    let base = in.albedo * tex;
    let albedo = base.xyz;
    let roughness = clamp(in.roughness, 0.04, 1.0);
    let metallic = clamp(in.metallic, 0.0, 1.0);

    let v = normalize(frame.camera_pos.xyz - in.world_pos);
    var color = frame.ambient.xyz * albedo;

    let light_count = i32(frame.shadow_params.z);
    var used_shadow = false;

    for (var i = 0; i < 4; i = i + 1) {
        if (i >= light_count) {
            break;
        }
        let light = frame.lights[i];
        let kind = light.pos_or_dir.w;
        var l: vec3<f32>;
        var radiance = light.color_range.xyz;
        var shadow = 1.0;

        if (kind < 0.5) {
            // Directional: travel dir stored; light vector toward the light.
            l = normalize(-light.pos_or_dir.xyz);
            if (!used_shadow) {
                let ndotl = max(dot(n, l), 0.0);
                shadow = sample_shadow(in.world_pos, ndotl);
                used_shadow = true;
            }
        } else {
            let to_light = light.pos_or_dir.xyz - in.world_pos;
            let dist = length(to_light);
            l = to_light / max(dist, 1e-4);
            let range = max(light.color_range.w, 0.1);
            let atten = clamp(1.0 - dist / range, 0.0, 1.0);
            radiance = radiance * (atten * atten);
        }

        color += shade_light(n, v, l, radiance, albedo, roughness, metallic, shadow);
    }

    return vec4<f32>(color, base.w);
}
