//! B3 mixed-resolution planning and LOD-frontier safety contracts.
//!
//! Surface Nets needs a topology-aware dual-grid transition surface.
//! Merely changing per-block grid density DOES NOT seal a 2:1 seam.
//! This module computes a graded LOD0/1/2 ring and its seam masks, but
//! requires a GPU seam-mesh gate before it can enter the render path.
//! The current renderer stays at LOD2.

use std::collections::{HashMap, HashSet};
use sandbox_world_core::BlockCoord;
use crate::scheduler::{StreamPlan, block_distance_xz};

pub const NEAR_LOD0_M: f32 = 28.0;
pub const MID_LOD1_M: f32 = 80.0;
pub const LOD_HYSTERESIS_M: f32 = 4.0;

pub const FACE_NEG_X: u8 = 1 << 0;
pub const FACE_POS_X: u8 = 1 << 1;
pub const FACE_NEG_Y: u8 = 1 << 2;
pub const FACE_POS_Y: u8 = 1 << 3;
pub const FACE_NEG_Z: u8 = 1 << 4;
pub const FACE_POS_Z: u8 = 1 << 5;

pub fn target_lod(distance_m: f32) -> u8 {
    if distance_m <= NEAR_LOD0_M {
        0
    } else if distance_m <= MID_LOD1_M {
        1
    } else {
        2
    }
}

/// Hysteresis delays a change until outside the stable boundary band.
/// LOD 0 means highest geometric resolution; LOD2 is coarsest.
/// One classification step per call prevents a 0->2 jump on teleport.
pub fn next_lod(current_lod: u8, distance_m: f32) -> u8 {
    match current_lod {
        0 if distance_m > NEAR_LOD0_M + LOD_HYSTERESIS_M => 1,
        1 if distance_m < NEAR_LOD0_M - LOD_HYSTERESIS_M => 0,
        1 if distance_m > MID_LOD1_M + LOD_HYSTERESIS_M => 2,
        2 if distance_m < MID_LOD1_M - LOD_HYSTERESIS_M => 1,
        0..=2 => current_lod,
        _ => target_lod(distance_m),
    }
}

const NEIGHBORS: [(i64, i64, i64, u8); 6] = [
    (-1, 0, 0, FACE_NEG_X),
    (1, 0, 0, FACE_POS_X),
    (0, -1, 0, FACE_NEG_Y),
    (0, 1, 0, FACE_POS_Y),
    (0, 0, -1, FACE_NEG_Z),
    (0, 0, 1, FACE_POS_Z),
];

pub struct LodFrontier {
    pub desired: HashMap<BlockCoord, u8>,
    /// Face bitmask for each fine block which has a coarser neighbor.
    pub coarse_neighbor_faces: HashMap<BlockCoord, u8>,
    /// All inter-block mixed-resolution faces counted once.
    pub mixed_faces: usize,
}

/// Computes desired LODs and exposes every 2:1 frontier explicitly.
/// A missing neighbor is not treated as stitched or ready.
pub fn frontier(plan: &StreamPlan, pos: [f32; 3]) -> LodFrontier {
    let desired: HashMap<_, _> = plan.targets.iter()
        .map(|item| (item.coord, target_lod(block_distance_xz(pos, item.coord))))
        .collect();
    let mut coarse_neighbor_faces = HashMap::new();
    let mut mixed = HashSet::new();

    for (&block, &lod) in &desired {
        let mut mask = 0u8;
        for &(dx, dy, dz, face) in &NEIGHBORS {
            let neighbor = BlockCoord {
                x: block.x + dx,
                y: block.y + dy,
                z: block.z + dz,
            };
            let Some(&other) = desired.get(&neighbor) else { continue };
            if other > lod {
                mask |= face;
            }
            if other != lod {
                if (block.x, block.y, block.z) < (neighbor.x, neighbor.y, neighbor.z) {
                    mixed.insert((block, neighbor));
                }
            }
        }
        if mask != 0 {
            coarse_neighbor_faces.insert(block, mask);
        }
    }
    LodFrontier { desired, coarse_neighbor_faces, mixed_faces: mixed.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::plan;

    #[test]
    fn range_tiers_and_hysteresis() {
        assert_eq!([target_lod(0.0), target_lod(40.0), target_lod(125.0)], [0, 1, 2]);
        assert_eq!(next_lod(0, 30.0), 0);
        assert_eq!(next_lod(0, 33.0), 1);
        assert_eq!(next_lod(1, 26.0), 1);
        assert_eq!(next_lod(1, 23.0), 0);
        assert_eq!(next_lod(1, 82.0), 1);
        assert_eq!(next_lod(1, 85.0), 2);
        assert_eq!(next_lod(2, 78.0), 2);
        assert_eq!(next_lod(2, 75.0), 1);
        assert_eq!(next_lod(0, 200.0), 1);
    }

    #[test]
    fn every_neighboring_lod_is_adjacent_not_four_to_one() {
        for pos in [
            [0.0, -7.0, 0.0],
            [17.5, -7.0, -32.25],
            [-190.0, 8.0, 111.0],
        ] {
            let p = plan(pos, [1.0, 0.0]);
            let result = frontier(&p, pos);
            assert!(result.mixed_faces > 0);
            for (&coord, &lod) in &result.desired {
                for &(dx, dy, dz, _) in &NEIGHBORS {
                    let other = BlockCoord { x: coord.x + dx, y: coord.y + dy, z: coord.z + dz };
                    if let Some(&other_lod) = result.desired.get(&other) {
                        assert!(lod.abs_diff(other_lod) <= 1,
                            "4:1 seam forbidden {coord:?} lod={lod}, {other:?} lod={other_lod}");
                    }
                }
            }
        }
    }

    #[test]
    fn mixed_faces_never_silently_assumed_welded() {
        let pos = [0.0, -7.0, 0.0];
        let ring = plan(pos, [1.0, 0.0]);
        let result = frontier(&ring, pos);
        assert!(result.coarse_neighbor_faces.values().any(|mask| mask.count_ones() > 0));
        assert!(result.mixed_faces > 0);
        // The test validates LOD adjacency metadata, NOT actual surface
        // triangles. A nonzero mixed_faces demands a GPU transition mesh.
    }

    #[test]
    fn vertical_layers_carry_compatible_lod() {
        let pos = [0.0, -7.0, 0.0];
        let result = frontier(&plan(pos, [0.0, -1.0]), pos);
        for (&coord, &lod) in &result.desired {
            if coord.y == 0 {
                let below = BlockCoord { y: -1, ..coord };
                assert_eq!(result.desired.get(&below), Some(&lod));
            }
        }
    }
}
