// Depth prepass + thin G-buffer: writes depth, world normal + roughness, and
// screen-space motion vectors. Vertex / instance layout is frozen (see lit.wgsl).

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
    @location(12) params: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) roughness: f32,
    @location(2) curr_clip: vec4<f32>,
    @location(3) prev_clip: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
    let prev = mat4x4<f32>(instance.prev_0, instance.prev_1, instance.prev_2, instance.prev_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    let prev_world = prev * vec4<f32>(in.position, 1.0);
    var out: VertexOutput;
    out.clip_position = frame.view_proj * world_pos;
    out.curr_clip = out.clip_position;
    out.prev_clip = frame.prev_view_proj * prev_world;
    let n_mat = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = n_mat * in.normal;
    out.roughness = instance.params.x;
    return out;
}

struct FsOut {
    @location(0) normal_rough: vec4<f32>,
    @location(1) velocity: vec2<f32>,
}

@fragment
fn fs_main(in: VertexOutput) -> FsOut {
    var out: FsOut;
    out.normal_rough = vec4<f32>(normalize(in.world_normal), clamp(in.roughness, 0.04, 1.0));
    // Unjitter the current position so velocity is purely motion.
    let curr = in.curr_clip.xy / in.curr_clip.w - frame.jitter.xy;
    let prev = in.prev_clip.xy / in.prev_clip.w;
    out.velocity = (curr - prev) * vec2<f32>(0.5, -0.5);
    return out;
}
