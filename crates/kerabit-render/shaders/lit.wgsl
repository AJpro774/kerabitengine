// Lit pass: Lambertian diffuse + roughness-lite Blinn-Phong specular + soft sun shadows.
// Vertex layout (frozen): position f32x3, normal f32x3, uv f32x2.
// Instance layout: model columns @3–6, albedo @7, params (roughness) @8.

struct FrameUniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,      // xyz = camera world position
    light_dir: vec4<f32>,       // xyz = direction light travels (sun → scene)
    light_color: vec4<f32>,     // xyz = intensity * color
    ambient: vec4<f32>,         // xyz = ambient color
    light_view_proj: mat4x4<f32>,
    shadow_params: vec4<f32>,   // x = depth bias, y = inv map size
}

@group(0) @binding(0) var<uniform> frame: FrameUniforms;
@group(1) @binding(0) var albedo_tex: texture_2d<f32>;
@group(1) @binding(1) var albedo_samp: sampler;
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
    @location(8) params: vec4<f32>, // x = roughness (0 shiny … 1 matte)
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) albedo: vec4<f32>,
    @location(4) roughness: f32,
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
    // Uniform scale: rotate normals by upper-left 3x3 of model.
    let n_mat = mat3x3<f32>(
        model[0].xyz,
        model[1].xyz,
        model[2].xyz,
    );
    out.world_normal = n_mat * in.normal;
    out.uv = in.uv;
    out.albedo = instance.albedo;
    out.roughness = instance.params.x;
    return out;
}

fn sample_shadow(world_pos: vec3<f32>, ndotl: f32) -> f32 {
    let light_clip = frame.light_view_proj * vec4<f32>(world_pos, 1.0);
    // Orthographic: w ≈ 1; still divide for safety.
    let ndc = light_clip.xyz / light_clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, ndc.y * -0.5 + 0.5);
    let depth = ndc.z;

    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || depth < 0.0 || depth > 1.0) {
        return 1.0;
    }

    let inv_size = frame.shadow_params.y;
    let bias = frame.shadow_params.x + (1.0 - ndotl) * 0.0015;
    var shadow = 0.0;
    // 3×3 PCF — soft contact shadows without a second map.
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * inv_size;
            shadow += textureSampleCompare(shadow_map, shadow_samp, uv + offset, depth - bias);
        }
    }
    return shadow / 9.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    // Light vector toward the light (opposite of travel direction).
    let l = normalize(-frame.light_dir.xyz);
    let ndotl = max(dot(n, l), 0.0);

    let tex = textureSample(albedo_tex, albedo_samp, in.uv);
    let base = in.albedo * tex;
    let albedo = base.xyz;

    let shadow = sample_shadow(in.world_pos, ndotl);
    let lit = ndotl * shadow;

    let diffuse = frame.ambient.xyz * albedo
        + frame.light_color.xyz * albedo * lit;

    // Roughness-lite specular: high roughness → weak/broad highlight.
    let roughness = clamp(in.roughness, 0.04, 1.0);
    let shininess = mix(256.0, 8.0, roughness);
    let spec_strength = mix(0.55, 0.02, roughness);
    let v = normalize(frame.camera_pos.xyz - in.world_pos);
    let h = normalize(l + v);
    let ndoth = max(dot(n, h), 0.0);
    let specular = frame.light_color.xyz * (pow(ndoth, shininess) * spec_strength) * shadow;

    return vec4<f32>(diffuse + specular, base.w);
}
