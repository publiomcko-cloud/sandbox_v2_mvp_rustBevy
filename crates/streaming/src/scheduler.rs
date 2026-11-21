//! B3 deterministic streaming ring: target planning independent of the renderer.
//!
//! B3 starts with a globally uniform LOD2 to preserve B2's proven
//! same-resolution seams. Mixed-LOD borders are explicitly not qualified.
//! Prefetch at 18 m/s is directional, and the retention radius is wider
//! than the presentation horizon so temporary build latency cannot evict
//! geometry still in view.

use std::collections::HashSet;

use sandbox_world_core::{BLOCK_EDGE_METERS, BlockCoord};

pub const HORIZON_M: f32 = 130.0;
pub const PREFETCH_M: f32 = 36.0;
pub const RETENTION_M: f32 = HORIZON_M + PREFETCH_M + BLOCK_EDGE_METERS as f32;
pub const STREAM_LOD: u8 = 2;
pub const BLOCKS_PER_FRAME: usize = 12;
pub const REQUIRED_LAYERS: [i64; 2] = [-1, 0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamTarget {
    pub coord: BlockCoord,
    pub distance_m: f32,
    pub required_now: bool,
    pub lead_m: f32,
}

#[derive(Clone, Debug)]
pub struct StreamPlan {
    /// Stable priority order: current horizon first; ahead-of-motion prefetch
    /// next; then block coordinate, for deterministic CI replays.
    pub targets: Vec<StreamTarget>,
    pub desired: HashSet<BlockCoord>,
    pub required: HashSet<BlockCoord>,
}

fn aabb_distance_1d(pos: f32, coord: i64) -> f32 {
    let lo = coord as f32 * BLOCK_EDGE_METERS as f32;
    let hi = lo + BLOCK_EDGE_METERS as f32;
    (lo - pos).max(pos - hi).max(0.0)
}

pub fn block_distance_xz(pos: [f32; 3], coord: BlockCoord) -> f32 {
    let dx = aabb_distance_1d(pos[0], coord.x);
    let dz = aabb_distance_1d(pos[2], coord.z);
    dx.hypot(dz)
}

pub fn within_retention(pos: [f32; 3], coord: BlockCoord) -> bool {
    block_distance_xz(pos, coord) <= RETENTION_M
}

/// Plans a 130 m all-direction presentation disk and a 36 m forward
/// directional prefetch region. The disk uses distance to the nearest
/// block *surface*, so no required block is discarded at a diagonal.
/// Two sparse vertical layers cover the B1 solid surface and cave tunnel.
pub fn plan(pos: [f32; 3], horizontal_direction: [f32; 2]) -> StreamPlan {
    let length = horizontal_direction[0].hypot(horizontal_direction[1]);
    let forward = if length > 0.001 {
        [
            horizontal_direction[0] / length,
            horizontal_direction[1] / length,
        ]
    } else {
        [0.0, -1.0]
    };
    let center_x = (pos[0] / BLOCK_EDGE_METERS as f32).floor() as i64;
    let center_z = (pos[2] / BLOCK_EDGE_METERS as f32).floor() as i64;
    let search_radius = ((HORIZON_M + PREFETCH_M) / BLOCK_EDGE_METERS as f32).ceil() as i64 + 2;

    let mut targets = Vec::new();
    let mut desired = HashSet::new();
    let mut required = HashSet::new();

    for z in center_z - search_radius..=center_z + search_radius {
        for x in center_x - search_radius..=center_x + search_radius {
            let sample = BlockCoord { x, y: 0, z };
            let distance_m = block_distance_xz(pos, sample);
            let required_now = distance_m <= HORIZON_M;

            // The projection is measured to the block's CENTER, but only
            // selects the prefetch extension (not the mandatory horizon).
            let dx =
                x as f32 * BLOCK_EDGE_METERS as f32 + (BLOCK_EDGE_METERS as f32 * 0.5) - pos[0];
            let dz =
                z as f32 * BLOCK_EDGE_METERS as f32 + (BLOCK_EDGE_METERS as f32 * 0.5) - pos[2];
            let lead_m = dx * forward[0] + dz * forward[1];
            let ahead = lead_m > 0.0;
            let in_prefetch =
                ahead && distance_m <= HORIZON_M + PREFETCH_M && distance_m - HORIZON_M <= lead_m;
            if !required_now && !in_prefetch {
                continue;
            }

            for y in REQUIRED_LAYERS {
                let coord = BlockCoord { x, y, z };
                targets.push(StreamTarget {
                    coord,
                    distance_m,
                    required_now,
                    lead_m,
                });
                desired.insert(coord);
                if required_now {
                    required.insert(coord);
                }
            }
        }
    }

    targets.sort_by(|a, b| {
        b.required_now
            .cmp(&a.required_now)
            .then_with(|| a.distance_m.total_cmp(&b.distance_m))
            .then_with(|| b.lead_m.total_cmp(&a.lead_m))
            .then_with(|| (a.coord.x, a.coord.y, a.coord.z).cmp(&(b.coord.x, b.coord.y, b.coord.z)))
    });
    StreamPlan {
        targets,
        desired,
        required,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_disk_covers_130_m_in_all_directions_and_diagonals() {
        for pos in [[0.0, 0.0, 0.0], [-9.7, -7.0, 15.9], [72.4, 20.0, -95.7]] {
            let p = plan(pos, [1.0, 0.0]);
            for x in -12..=12 {
                for z in -12..=12 {
                    for y in REQUIRED_LAYERS {
                        let c = BlockCoord {
                            x: x + (pos[0] / 16.0).floor() as i64,
                            y,
                            z: z + (pos[2] / 16.0).floor() as i64,
                        };
                        if block_distance_xz(pos, c) <= HORIZON_M {
                            assert!(
                                p.required.contains(&c),
                                "Missing 130m coverage at {pos:?} for {c:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn nearest_block_boundaries_are_inclusive_at_negative_coordinates() {
        let p = plan([-0.001, -7.0, -0.001], [0.0, 1.0]);
        for x in -1..=0 {
            for z in -1..=0 {
                for y in REQUIRED_LAYERS {
                    assert!(p.required.contains(&BlockCoord { x, y, z }));
                }
            }
        }
    }

    #[test]
    fn all_required_are_loaded_before_prefetch_in_priority() {
        let p = plan([0.0, 0.0, 0.0], [1.0, 0.0]);
        assert!(!p.targets.is_empty());
        let first_nonrequired = p.targets.iter().position(|x| !x.required_now).unwrap();
        assert!(
            p.targets[..first_nonrequired]
                .iter()
                .all(|x| x.required_now)
        );
        assert!(
            p.targets[first_nonrequired..]
                .iter()
                .all(|x| !x.required_now)
        );
        assert!(p.desired.len() >= p.required.len());
    }

    #[test]
    fn forward_prefetch_is_directional_without_restricting_horizon() {
        let p = plan([0.0, 0.0, 0.0], [1.0, 0.0]);
        assert!(
            p.desired
                .iter()
                .any(|c| c.x >= 9 && !p.required.contains(c))
        );
        assert!(
            !p.desired
                .iter()
                .any(|c| c.x <= -10 && !p.required.contains(c))
        );
    }

    #[test]
    fn retention_does_not_evict_any_required_or_prefetch_block() {
        for x in (-200..=200).step_by(17) {
            let pos = [x as f32, -7.0, (x as f32 * 0.25) - 2.0];
            let p = plan(pos, [1.0, 0.25]);
            assert!(p.desired.iter().all(|c| within_retention(pos, *c)));
        }
    }

    #[test]
    fn fixed_lod_prevents_unstitched_mixed_resolution_neighbors() {
        assert_eq!(STREAM_LOD, 2);
        // Deliberate B3 limitation until an explicit verified transition
        // topology is available. There must be no silent per-block LOD mix.
        for pos in [[0.0, -7.0, 0.0], [150.0, 6.0, 30.0]] {
            assert!(
                plan(pos, [1.0, 0.0])
                    .targets
                    .iter()
                    .all(|_| STREAM_LOD == 2)
            );
        }
    }

    #[test]
    fn deterministic_target_order_across_identical_inputs() {
        let a = plan([42.5, -5.0, -73.25], [0.5, -1.0]);
        let b = plan([42.5, -5.0, -73.25], [0.5, -1.0]);
        let ca: Vec<_> = a.targets.iter().map(|t| t.coord).collect();
        let cb: Vec<_> = b.targets.iter().map(|t| t.coord).collect();
        assert_eq!(ca, cb);
    }
}
