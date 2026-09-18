// Light clustering: one invocation per froxel. Builds the per-cluster list of
// point lights whose sphere overlaps the cluster's view-space AABB.
// Layout: lights[0..dir_count) are directional (skipped), then point lights.

@group(0) @binding(1) var<storage, read> lights: array<GpuLight>;
@group(0) @binding(2) var<storage, read_write> grid: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read_write> light_indices: array<u32>;

const MAX_PER_CLUSTER: u32 = 64u;

fn slice_depth(slice: f32) -> f32 {
    let near = frame.params.x;
    let far = frame.params.y;
    return near * pow(far / near, slice / frame.cluster.z);
}

// View-space point on the ray through NDC xy at view depth `d` (positive).
fn view_point(ndc: vec2<f32>, d: f32) -> vec3<f32> {
    let p = frame.inv_proj * vec4<f32>(ndc, 1.0, 1.0);
    let far_pt = p.xyz / p.w;
    return far_pt * (d / -far_pt.z);
}

@compute @workgroup_size(16, 9, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let tiles_x = u32(frame.cluster.x);
    let tiles_y = u32(frame.cluster.y);
    let slices = u32(frame.cluster.z);
    if (gid.x >= tiles_x || gid.y >= tiles_y || gid.z >= slices) {
        return;
    }
    let cluster = gid.x + gid.y * tiles_x + gid.z * tiles_x * tiles_y;

    let x0 = f32(gid.x) / f32(tiles_x) * 2.0 - 1.0;
    let x1 = f32(gid.x + 1u) / f32(tiles_x) * 2.0 - 1.0;
    // Tile rows run top-down like fragment coordinates.
    let y0 = 1.0 - f32(gid.y + 1u) / f32(tiles_y) * 2.0;
    let y1 = 1.0 - f32(gid.y) / f32(tiles_y) * 2.0;
    let zn = slice_depth(f32(gid.z));
    let zf = slice_depth(f32(gid.z + 1u));

    var mn = vec3<f32>(1e30);
    var mx = vec3<f32>(-1e30);
    for (var c = 0u; c < 8u; c = c + 1u) {
        let sx = select(x0, x1, (c & 1u) != 0u);
        let sy = select(y0, y1, (c & 2u) != 0u);
        let sd = select(zn, zf, (c & 4u) != 0u);
        let p = view_point(vec2<f32>(sx, sy), sd);
        mn = min(mn, p);
        mx = max(mx, p);
    }

    let dir_count = u32(frame.params.z);
    let point_count = u32(frame.params.w);
    var count = 0u;
    let base = cluster * MAX_PER_CLUSTER;
    for (var i = dir_count; i < dir_count + point_count; i = i + 1u) {
        if (count >= MAX_PER_CLUSTER) {
            break;
        }
        let l = lights[i];
        let vp = (frame.view * vec4<f32>(l.pos_or_dir.xyz, 1.0)).xyz;
        let r = l.color_range.w;
        let closest = clamp(vp, mn, mx);
        let d = closest - vp;
        if (dot(d, d) <= r * r) {
            light_indices[base + count] = i;
            count = count + 1u;
        }
    }
    grid[cluster] = vec2<u32>(base, count);
}
