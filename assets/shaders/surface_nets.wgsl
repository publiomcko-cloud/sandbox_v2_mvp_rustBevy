// B2 fixed-slot Surface Nets extraction.
// GPU density is produced by sdf_density.wgsl and remains GPU resident.
// Each cell owns one vertex; each candidate grid-edge owns six indices.
// Inactive slots become degenerate triangles; there is no GPU->CPU readback.
// B2 repairs same-LOD same-seed borders using extended Surface Nets cell vertices.
// B3 still needs mixed-LOD transitions, compaction and indirect drawing.

struct B2Params {
    base_x: i32,
    base_y: i32,
    base_z: i32,
    seed: u32,
    step_half: i32,
    dimension: u32,
    count: u32,
    reserved: u32,
    vertex_start: u32,
    vertex_end: u32,
    index_start: u32,
    index_end: u32,
    cells: u32,
    padding0: u32,
    padding1: u32,
    padding2: u32,
}

@group(0) @binding(0) var<uniform> p: B2Params;
@group(0) @binding(1) var<storage, read> density_samples: array<f32>;
@group(0) @binding(2) var<storage, read_write> vertex_data: array<f32>;
@group(0) @binding(3) var<storage, read_write> index_data: array<u32>;

const EDGE_PAIRS: array<vec2<u32>, 12> = array<vec2<u32>, 12>(
    vec2u(0u, 1u), vec2u(2u, 3u), vec2u(4u, 5u), vec2u(6u, 7u),
    vec2u(0u, 2u), vec2u(1u, 3u), vec2u(4u, 6u), vec2u(5u, 7u),
    vec2u(0u, 4u), vec2u(1u, 5u), vec2u(2u, 6u), vec2u(3u, 7u)
);

fn at(x: u32, y: u32, z: u32) -> f32 {
    return density_samples[(z * p.dimension + y) * p.dimension + x];
}

fn vertex_slot(x: u32, y: u32, z: u32) -> u32 {
    let vd = p.cells + 1u;
    return (z * vd + y) * vd + x;
}

@compute @workgroup_size(64)
fn emit_vertices(@builtin(global_invocation_id) gid: vec3<u32>) {
    let id = gid.x;
    // Vertex cells extend one negative-grid cell beyond the owned block.
    // Shared boundary cells are duplicated at identical SDF coordinates;
    // edges remain uniquely owned by their global starting grid point.
    let vd = p.cells + 1u;
    let total = vd * vd * vd;
    if (id >= total) {
        return;
    }

    let x = id % vd;
    let y = (id / vd) % vd;
    let z = id / (vd * vd);

    let vertex_offset = p.vertex_start + id * 8u;
    if (vertex_offset + 8u > p.vertex_end) {
        return;
    }

    // Samples include one halo sample on all sides.
    // Cell (0,0,0) starts at sample (1,1,1).
    var samples: array<f32, 8>;
    for (var k = 0u; k < 8u; k = k + 1u) {
        let dx = k & 1u;
        let dy = (k >> 1u) & 1u;
        let dz = (k >> 2u) & 1u;
        samples[k] = at(x + dx, y + dy, z + dz);
    }

    var intersection_sum = vec3f(0.0);
    var intersection_count = 0u;

    for (var edge = 0u; edge < 12u; edge = edge + 1u) {
        let pair = EDGE_PAIRS[edge];
        let a = pair.x;
        let b = pair.y;
        let da = samples[a];
        let db = samples[b];
        if ((da < 0.0) != (db < 0.0)) {
            let pa = vec3f(f32(a & 1u), f32((a >> 1u) & 1u), f32((a >> 2u) & 1u));
            let pb = vec3f(f32(b & 1u), f32((b >> 1u) & 1u), f32((b >> 2u) & 1u));
            let t = clamp(da / (da - db), 0.0, 1.0);
            intersection_sum = intersection_sum + mix(pa, pb, t);
            intersection_count = intersection_count + 1u;
        }
    }

    let spacing = 0.5 * f32(p.step_half);
    var center = vec3f(0.5);
    if (intersection_count > 0u) {
        center = intersection_sum / f32(intersection_count);
    }
    let position = (vec3f(f32(x), f32(y), f32(z)) - vec3f(1.0) + center) * spacing;

    // Surface is the zero-set; gradient points to positive (air) density.
    let gradient = vec3f(
        (samples[1] + samples[3] + samples[5] + samples[7]) -
            (samples[0] + samples[2] + samples[4] + samples[6]),
        (samples[2] + samples[3] + samples[6] + samples[7]) -
            (samples[0] + samples[1] + samples[4] + samples[5]),
        (samples[4] + samples[5] + samples[6] + samples[7]) -
            (samples[0] + samples[1] + samples[2] + samples[3])
    );
    var normal = vec3f(0.0, 1.0, 0.0);
    if (dot(gradient, gradient) > 0.000001) {
        normal = normalize(gradient);
    }

    // Bevy MeshAllocator currently interleaves xyz, normal xyz, uv xy.
    vertex_data[vertex_offset + 0u] = position.x;
    vertex_data[vertex_offset + 1u] = position.y;
    vertex_data[vertex_offset + 2u] = position.z;
    vertex_data[vertex_offset + 3u] = normal.x;
    vertex_data[vertex_offset + 4u] = normal.y;
    vertex_data[vertex_offset + 5u] = normal.z;
    vertex_data[vertex_offset + 6u] = position.x * 0.0625;
    vertex_data[vertex_offset + 7u] = position.z * 0.0625;
}

@compute @workgroup_size(64)
fn emit_indices(@builtin(global_invocation_id) gid: vec3<u32>) {
    let edge_id = gid.x;
    let cell_count = p.cells * p.cells * p.cells;
    if (edge_id >= 3u * cell_count) {
        return;
    }
    let base_index = p.index_start + 6u * edge_id;
    if (base_index + 6u > p.index_end) {
        return;
    }

    // Deterministically clear every unused fixed-capacity slot.
    for (var k = 0u; k < 6u; k = k + 1u) {
        index_data[base_index + k] = 0u;
    }

    let axis = edge_id / cell_count;
    let id = edge_id % cell_count;
    let x = id % p.cells;
    let y = (id / p.cells) % p.cells;
    let z = id / (p.cells * p.cells);

    let sx = x + 1u;
    let sy = y + 1u;
    let sz = z + 1u;
    let a_density = at(sx, sy, sz);
    var b_density = 0.0;
    if (axis == 0u) {
        b_density = at(sx + 1u, sy, sz);
    } else if (axis == 1u) {
        b_density = at(sx, sy + 1u, sz);
    } else {
        b_density = at(sx, sy, sz + 1u);
    }
    if ((a_density < 0.0) == (b_density < 0.0)) {
        return;
    }

    // Four cells incident to a sign-changing primal grid edge.
    var a = 0u;
    var b = 0u;
    var c = 0u;
    var d = 0u;
    // Per-block ownership: each primal edge starting at a grid point
    // [0, cells) belongs to this block, even when it lies on its low face.
    // The negative cell halo provides the adjacent four Surface Nets
    // vertices. No per-face omission or cross-block draw dependency.
    if (axis == 0u) {
        a = vertex_slot(x + 1u, y, z);
        b = vertex_slot(x + 1u, y + 1u, z);
        c = vertex_slot(x + 1u, y, z + 1u);
        d = vertex_slot(x + 1u, y + 1u, z + 1u);
    } else if (axis == 1u) {
        a = vertex_slot(x, y + 1u, z);
        b = vertex_slot(x + 1u, y + 1u, z);
        c = vertex_slot(x, y + 1u, z + 1u);
        d = vertex_slot(x + 1u, y + 1u, z + 1u);
    } else {
        a = vertex_slot(x, y, z + 1u);
        b = vertex_slot(x + 1u, y, z + 1u);
        c = vertex_slot(x, y + 1u, z + 1u);
        d = vertex_slot(x + 1u, y + 1u, z + 1u);
    }

    // Base winding a,b,c faces +X, -Y, +Z respectively.
    let ascending_density = a_density < 0.0;
    let reverse_winding = ascending_density == (axis == 1u);
    if (reverse_winding) {
        index_data[base_index + 0u] = a;
        index_data[base_index + 1u] = c;
        index_data[base_index + 2u] = b;
        index_data[base_index + 3u] = b;
        index_data[base_index + 4u] = c;
        index_data[base_index + 5u] = d;
    } else {
        index_data[base_index + 0u] = a;
        index_data[base_index + 1u] = b;
        index_data[base_index + 2u] = c;
        index_data[base_index + 3u] = c;
        index_data[base_index + 4u] = b;
        index_data[base_index + 5u] = d;
    }
}
