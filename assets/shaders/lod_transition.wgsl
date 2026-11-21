// Experimental adaptive dual Surface Nets 2:1 transition faces.
// This shader is a GPU topology candidate, NOT enabled in the B3 renderer.
// A per-block owner emits sign-changing fine-lattice Y/Z/X edges along
// its negative x/y/z faces. Each edge maps four adjacent finest-grid cells
// to their actual coarse/fine dual cells and triangulates their vertices.
// No CPU readback is needed by a future GPU-resident runtime integration.

struct SeamParams {
    base_x: i32,
    base_y: i32,
    base_z: i32,
    seed: u32,
    step_half: i32,
    dimension: u32,
    count: u32,
    reserved: u32,      // face_mask: bit0=-X, bit1=-Y, bit2=-Z
    vertex_start: u32,
    vertex_end: u32,
    index_start: u32,
    index_end: u32,
    cells: u32,
    padding0: u32,
    padding1: u32,      // packed 2-bit 3x3x3 LODs, indices 0..15
    padding2: u32,      // packed 2-bit 3x3x3 LODs, indices 16..26
}

@group(0) @binding(0) var<uniform> p: SeamParams;
@group(0) @binding(1) var<storage, read> density_samples: array<f32>;
@group(0) @binding(2) var<storage, read_write> vertex_data: array<f32>;
@group(0) @binding(3) var<storage, read_write> index_data: array<u32>;

const EDGE_PAIRS_SEAM: array<vec2<u32>, 12> = array<vec2<u32>, 12>(
    vec2u(0u, 1u), vec2u(2u, 3u), vec2u(4u, 5u), vec2u(6u, 7u),
    vec2u(0u, 2u), vec2u(1u, 3u), vec2u(4u, 6u), vec2u(5u, 7u),
    vec2u(0u, 4u), vec2u(1u, 5u), vec2u(2u, 6u), vec2u(3u, 7u)
);

fn hash2(seed: u32, x: i32, z: i32) -> u32 {
    var h = seed ^ (bitcast<u32>(x) * 0x9e3779b1u) ^ (bitcast<u32>(z) * 0x85ebca77u);
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    return h ^ (h >> 16u);
}
fn hash01(seed: u32, x: i32, z: i32) -> f32 {
    return f32(hash2(seed, x, z) & 1023u) * (1.0 / 1023.0);
}
fn smooth_curve(t: f32) -> f32 {
    return t * t * (3.0 - 2.0 * t);
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    return a + (b - a) * t;
}
fn floor_div(a: i32, divisor: i32) -> i32 {
    let q = a / divisor;
    let r = a % divisor;
    if (r < 0) {
        return q - 1;
    }
    return q;
}
fn noise2_half(seed: u32, hx: i32, hz: i32, scale: i32) -> f32 {
    let gx = floor_div(hx, scale);
    let gz = floor_div(hz, scale);
    let tx = smooth_curve(f32(hx - gx * scale) / f32(scale));
    let tz = smooth_curve(f32(hz - gz * scale) / f32(scale));
    let a = lerp(hash01(seed, gx, gz), hash01(seed, gx + 1, gz), tx);
    let b = lerp(hash01(seed, gx, gz + 1), hash01(seed, gx + 1, gz + 1), tx);
    return lerp(a, b, tz);
}
fn terrain_height_half(seed: u32, hx: i32, hz: i32) -> f32 {
    return -3.0
        + 11.0 * noise2_half(seed ^ 0x045d9f3bu, hx, hz, 128)
        + 5.0 * noise2_half(seed ^ 0x9e3779b9u, hx, hz, 32);
}
fn cave_center_y(seed: u32, hx: i32) -> f32 {
    return -7.0 + 2.0 * noise2_half(seed ^ 0x04c11db7u, hx, 0, 128);
}
fn cave_center_z(seed: u32, hx: i32) -> f32 {
    return -3.0 + 6.0 * noise2_half(seed ^ 0x13a5ba1du, hx, 0, 128);
}
fn density_half(seed: u32, hx: i32, hy: i32, hz: i32) -> f32 {
    let y = f32(hy) * 0.5;
    let z = f32(hz) * 0.5;
    let height = terrain_height_half(seed, hx, hz);
    let cy = cave_center_y(seed, hx);
    let cz = cave_center_z(seed, hx);
    let dy = y - cy;
    let dz = z - cz;
    return max(y - height, 2.6 - sqrt(dy * dy + dz * dz));
}


fn lod_at_cell(lower_half: vec3i) -> i32 {
    // Current block is center (index 13) of a 3x3x3 neighborhood.
    // Adjacent finest-grid cells can lie only at offsets -1 or 0.
    let dx = clamp(floor_div(lower_half.x - p.base_x, 32), -1, 1);
    let dy = clamp(floor_div(lower_half.y - p.base_y, 32), -1, 1);
    let dz = clamp(floor_div(lower_half.z - p.base_z, 32), -1, 1);
    let index = u32((dz + 1) * 9 + (dy + 1) * 3 + (dx + 1));
    let bit_offset = index * 2u;
    var word = p.padding1;
    var shift = bit_offset;
    if (bit_offset >= 32u) {
        word = p.padding2;
        shift = bit_offset - 32u;
    }
    return i32((word >> shift) & 3u);
}

struct DualVertex {
    position: vec3f,
    normal: vec3f,
};

fn dual_at(fine_cell_lower_half: vec3i) -> DualVertex {
    let lod = lod_at_cell(fine_cell_lower_half);
    let step = 1i << u32(lod);
    let lower = vec3i(
        floor_div(fine_cell_lower_half.x, step) * step,
        floor_div(fine_cell_lower_half.y, step) * step,
        floor_div(fine_cell_lower_half.z, step) * step
    );
    var values: array<f32, 8>;
    for (var k = 0u; k < 8u; k += 1u) {
        let pt = lower + vec3i(
            i32(k & 1u) * step,
            i32((k >> 1u) & 1u) * step,
            i32((k >> 2u) & 1u) * step
        );
        values[k] = density_half(p.seed, pt.x, pt.y, pt.z);
    }

    var intersection_sum = vec3f(0.0);
    var intersection_count = 0u;
    for (var edge = 0u; edge < 12u; edge += 1u) {
        let pair = EDGE_PAIRS_SEAM[edge];
        let a = pair.x;
        let b = pair.y;
        let da = values[a];
        let db = values[b];
        if ((da < 0.0) != (db < 0.0)) {
            let pa = vec3f(f32(a & 1u), f32((a >> 1u) & 1u), f32((a >> 2u) & 1u));
            let pb = vec3f(f32(b & 1u), f32((b >> 1u) & 1u), f32((b >> 2u) & 1u));
            let t = clamp(da / (da - db), 0.0, 1.0);
            intersection_sum += mix(pa, pb, t);
            intersection_count += 1u;
        }
    }
    var center = vec3f(0.5);
    if (intersection_count > 0u) {
        center = intersection_sum / f32(intersection_count);
    }
    let spacing = 0.5 * f32(step);
    let local_half = lower - vec3i(p.base_x, p.base_y, p.base_z);
    let position = vec3f(
        f32(local_half.x), f32(local_half.y), f32(local_half.z)
    ) * 0.5 + center * spacing;
    let gradient = vec3f(
        (values[1] + values[3] + values[5] + values[7]) -
            (values[0] + values[2] + values[4] + values[6]),
        (values[2] + values[3] + values[6] + values[7]) -
            (values[0] + values[1] + values[4] + values[5]),
        (values[4] + values[5] + values[6] + values[7]) -
            (values[0] + values[1] + values[2] + values[3])
    );
    var normal = vec3f(0.0, 1.0, 0.0);
    if (dot(gradient, gradient) > 0.000001) {
        normal = normalize(gradient);
    }
    return DualVertex(position, normal);
}

fn index_slot(face: u32, local_slot: u32) -> u32 {
    let lower_mask = p.reserved & ((1u << face) - 1u);
    let rank = countOneBits(lower_mask);
    return rank * (2u * 32u * 32u) + local_slot;
}

// Every transition face is sampled at its finer PARTICIPATING LOD.
// The fixed 32x32 slot grid allows LOD0/1 and LOD1/2 without changing
// dispatch size; non-aligned slots remain degenerate triangles.
@compute @workgroup_size(64)
fn emit_transition(@builtin(global_invocation_id) gid: vec3<u32>) {
    let candidate = gid.x;
    const N: u32 = 32u;
    const FACE_CANDIDATES: u32 = 2u * N * N;
    if (candidate >= 3u * FACE_CANDIDATES) { return; }
    let face = candidate / FACE_CANDIDATES;
    if ((p.reserved & (1u << face)) == 0u) { return; }

    let local_slot = candidate % FACE_CANDIDATES;
    let orientation = local_slot / (N * N);
    let uv = local_slot % (N * N);
    let u = i32(uv % N);
    let v = i32(uv / N);

    let base = vec3i(p.base_x, p.base_y, p.base_z);
    var previous = base;
    previous[face] -= 1;
    let own_lod = lod_at_cell(base);
    let other_lod = lod_at_cell(previous);
    let fine_step = 1i << u32(min(own_lod, other_lod));
    // A 2:1 seam must use the finer *participating* grid, not
    // arbitrary LOD0 samples on a LOD1/2 boundary.
    if ((u % fine_step) != 0 || (v % fine_step) != 0 ||
        u + fine_step > i32(N) || v + fine_step > i32(N)) {
        return;
    }
    var start = base;
    var edge_axis = 0u;
    if (face == 0u) {
        start.y += u;
        start.z += v;
        edge_axis = select(2u, 1u, orientation == 0u);
    } else if (face == 1u) {
        start.x += u;
        start.z += v;
        edge_axis = select(2u, 0u, orientation == 0u);
    } else {
        start.x += u;
        start.y += v;
        edge_axis = select(1u, 0u, orientation == 0u);
    }

    // At the intersection of two low faces, emit each primal edge
    // exactly once: lower face index (X then Y then Z) owns it.
    let on_x = (start.x == p.base_x);
    let on_y = (start.y == p.base_y);
    if ((face == 1u && on_x && edge_axis == 2u && (p.reserved & 1u) != 0u) ||
        (face == 2u && on_x && edge_axis == 1u && (p.reserved & 1u) != 0u) ||
        (face == 2u && on_y && edge_axis == 0u && (p.reserved & 2u) != 0u)) {
        return;
    }

    let slot = index_slot(face, local_slot);
    let base_vertex_count = (p.cells + 1u) * (p.cells + 1u) * (p.cells + 1u);
    let base_index_count = 18u * p.cells * p.cells * p.cells;
    let vertex_offset = p.vertex_start + base_vertex_count * 8u + slot * 32u;
    let index_offset = p.index_start + base_index_count + slot * 6u;
    if (vertex_offset + 32u > p.vertex_end || index_offset + 6u > p.index_end) {
        return;
    }
    for (var i = 0u; i < 6u; i += 1u) {
        index_data[index_offset + i] = 0u;
    }

    var end = start;
    if (edge_axis == 0u) { end.x += fine_step; }
    else if (edge_axis == 1u) { end.y += fine_step; }
    else { end.z += fine_step; }
    let da = density_half(p.seed, start.x, start.y, start.z);
    let db = density_half(p.seed, end.x, end.y, end.z);
    if ((da < 0.0) == (db < 0.0)) { return; }

    for (var k = 0u; k < 4u; k += 1u) {
        let a = (k & 1u) == 0u;
        let b = ((k >> 1u) & 1u) == 0u;
        var lower = start;
        if (edge_axis == 0u) {
            if (a) { lower.y -= fine_step; }
            if (b) { lower.z -= fine_step; }
        } else if (edge_axis == 1u) {
            if (a) { lower.x -= fine_step; }
            if (b) { lower.z -= fine_step; }
        } else {
            if (a) { lower.x -= fine_step; }
            if (b) { lower.y -= fine_step; }
        }
        let dual = dual_at(lower);
        let offset = vertex_offset + k * 8u;
        vertex_data[offset] = dual.position.x;
        vertex_data[offset + 1u] = dual.position.y;
        vertex_data[offset + 2u] = dual.position.z;
        vertex_data[offset + 3u] = dual.normal.x;
        vertex_data[offset + 4u] = dual.normal.y;
        vertex_data[offset + 5u] = dual.normal.z;
        vertex_data[offset + 6u] = dual.position.x * 0.0625;
        vertex_data[offset + 7u] = dual.position.z * 0.0625;
    }
    let vbase = base_vertex_count + slot * 4u;
    let reverse = (da < 0.0) == (edge_axis == 1u);
    if (reverse) {
        index_data[index_offset] = vbase;
        index_data[index_offset + 1u] = vbase + 2u;
        index_data[index_offset + 2u] = vbase + 1u;
        index_data[index_offset + 3u] = vbase + 1u;
        index_data[index_offset + 4u] = vbase + 2u;
        index_data[index_offset + 5u] = vbase + 3u;
    } else {
        index_data[index_offset] = vbase;
        index_data[index_offset + 1u] = vbase + 1u;
        index_data[index_offset + 2u] = vbase + 2u;
        index_data[index_offset + 3u] = vbase + 2u;
        index_data[index_offset + 4u] = vbase + 1u;
        index_data[index_offset + 5u] = vbase + 3u;
    }
}
