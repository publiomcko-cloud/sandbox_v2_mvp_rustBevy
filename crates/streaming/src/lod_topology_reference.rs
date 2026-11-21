//! CPU reference for an adaptive dual Surface Nets LOD seam.
//! NOT part of the production draw pipeline. A GPU seam renderer must
//! produce topologically equivalent faces before mixed LOD is enabled.
#![cfg(test)]

use sandbox_world_core::density::density_half;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct Cell {
    lower: [i32; 3],
    stride: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct TopologyReport {
    triangles: usize,
    mixed_side_triangles: usize,
    internal_edges: usize,
    nonmanifold_internal_edges: usize,
}

fn cell_for_fine_corner(point: [i32; 3], axis: usize, boundary: i32, low_stride: i32, high_stride: i32) -> Cell {
    let stride = if point[axis] < boundary { low_stride } else { high_stride };
    Cell {
        lower: point.map(|value| value.div_euclid(stride) * stride),
        stride,
    }
}

/// Reference: issue quads for sign-changing primal edges. The interior
/// owner uses its LOD stride; edges tangential to a mixed face use the
/// finer of the two strides. Each incident finest-cell coordinate maps
/// to the leaf cell of the appropriate LOD. Repeated coarse vertices
/// produce degenerate triangles, which are filtered below.
/// This is the dual analogue of adaptive/octree contouring, not MC tables.
fn reference_topology(
    border_axis: usize,
    border_half: i32,
    lower_stride: i32,
    upper_stride: i32,
    field: impl Fn([i32; 3]) -> f32,
) -> TopologyReport {
    assert!(lower_stride == upper_stride * 2 || upper_stride == lower_stride * 2);
    assert_eq!(border_half % lower_stride, 0);
    assert_eq!(border_half % upper_stride, 0);
    let margin = 24;
    let lower = [border_half - margin; 3];
    let upper = [border_half + margin; 3];
    let mut density_cache: HashMap<[i32; 3], f32> = HashMap::new();
    let mut sampled = |p: [i32; 3]| -> f32 {
        *density_cache.entry(p).or_insert_with(|| field(p))
    };

    let mut topology = HashMap::<(Cell, Cell), u32>::new();
    let mut triangle_count = 0usize;
    let mut mixed_triangles = 0usize;
    let tangent_axes: [usize; 3] = [0, 1, 2];

    for x in lower[0]..upper[0] {
        for y in lower[1]..upper[1] {
            for z in lower[2]..upper[2] {
                let source = [x, y, z];
                let owner_stride = if source[border_axis] < border_half {
                    lower_stride
                } else {
                    upper_stride
                };
                for edge_axis in tangent_axes {
                    let stride = if source[border_axis] == border_half && edge_axis != border_axis {
                        lower_stride.min(upper_stride)
                    } else {
                        owner_stride
                    };
                    if !source.iter().all(|c| c.rem_euclid(stride) == 0) {
                        continue;
                    }
                    let mut end = source;
                    end[edge_axis] += stride;
                    if end[edge_axis] >= upper[edge_axis] { continue; }
                    if (sampled(source) < 0.0) == (sampled(end) < 0.0) { continue; }
                    let orth: Vec<_> = (0..3).filter(|a| *a != edge_axis).collect();
                    let mut corners = [Cell { lower: [0; 3], stride: 1 }; 4];
                    for (i, (u, v)) in [(-1, -1), (0, -1), (-1, 0), (0, 0)].into_iter().enumerate() {
                        let mut fine_corner = source;
                        fine_corner[orth[0]] += u * stride;
                        fine_corner[orth[1]] += v * stride;
                        corners[i] = cell_for_fine_corner(
                            fine_corner, border_axis, border_half, lower_stride, upper_stride,
                        );
                    }
                    let triangles = [
                        [corners[0], corners[1], corners[2]],
                        [corners[2], corners[1], corners[3]],
                    ];
                    for tri in triangles {
                        let distinct: HashSet<_> = tri.into_iter().collect();
                        if distinct.len() != 3 { continue; }
                        triangle_count += 1;
                        let mixed = distinct.iter().any(|c| c.lower[border_axis] < border_half)
                            && distinct.iter().any(|c| c.lower[border_axis] >= border_half);
                        if mixed { mixed_triangles += 1; }
                        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                            let key = if a < b { (a, b) } else { (b, a) };
                            *topology.entry(key).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }

    let mut report = TopologyReport {
        triangles: triangle_count,
        mixed_side_triangles: mixed_triangles,
        ..Default::default()
    };
    let interior_margin = lower_stride.max(upper_stride) * 2;
    for ((a, b), count) in topology {
        let interior = [a, b].iter().all(|cell| {
            (0..3).all(|axis| {
                cell.lower[axis] >= lower[axis] + interior_margin &&
                cell.lower[axis] + cell.stride < upper[axis] - interior_margin
            })
        });
        if interior {
            report.internal_edges += 1;
            if count != 2 {
                report.nonmanifold_internal_edges += 1;
            }
        }
    }
    report
}

#[test]
fn planar_dual_seams_are_manifold_at_two_to_one_transition() {
    for axis in 0..3 {
        for (low, high) in [(4, 2), (2, 4)] {
            let report = reference_topology(axis, 0, low, high, |p| {
                let x = p[0] as f32 * 0.5;
                let y = p[1] as f32 * 0.5;
                let z = p[2] as f32 * 0.5;
                y - (1.0 + (0.17 * x).sin() + 0.23 * (0.2 * z).cos())
            });
            println!("Adaptive Surface Nets plane: axis={axis} {low}:{high}, {report:?}");
            assert!(report.triangles > 0);
            assert!(report.internal_edges > 0);
            assert_eq!(report.nonmanifold_internal_edges, 0,
                "Adaptive dual seam has open/non-manifold internal edges");
        }
    }
}

#[test]
fn seeded_b1_surface_and_cave_dual_seams_are_manifold_at_two_to_one_transition() {
    for axis in 0..3 {
        for (low, high) in [(4, 2), (2, 4)] {
            let report = reference_topology(axis, 0, low, high, |p| {
                density_half(42, p[0], p[1], p[2])
            });
            println!("Adaptive Surface Nets SDF: axis={axis} {low}:{high}, {report:?}");
            assert!(report.triangles > 0);
            assert!(report.internal_edges > 0);
            assert_eq!(report.nonmanifold_internal_edges, 0,
                "B1 field has open/non-manifold internal dual contour edges");
        }
    }
}
