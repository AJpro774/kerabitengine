// Depth-only directional shadow pass. Writes light-space depth into the shadow map.
// Vertex / instance layouts match lit.wgsl (position @0, model @3–6).

struct ShadowUniforms {
    light_view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> shadow: ShadowUniforms;

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
    @location(8) params: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );
    let world_pos = model * vec4<f32>(in.position, 1.0);
    return shadow.light_view_proj * world_pos;
}
