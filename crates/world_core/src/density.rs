//! Deterministic terrain density shared with sdf_density.wgsl.
//! Negative is solid, positive is air. This is a level-set field,
//! not an exact Euclidean distance near steep slopes.

use crate::{BLOCK_EDGE_METERS, BlockCoord};

pub const LOD0_CELLS: usize = 32;
pub const HALO: usize = 1;
pub const CAVE_RADIUS: f32 = 2.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityError {
    InvalidLod(u8),
    CoordinateOutOfRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DensityGrid {
    pub coord: BlockCoord,
    pub lod: u8,
    pub cells: usize,
    pub dim: usize,
    pub step_half: i32,
    pub base_half: [i32; 3],
}

impl DensityGrid {
    pub fn new(coord: BlockCoord, lod: u8) -> Result<Self, DensityError> {
        if lod > 2 {
            return Err(DensityError::InvalidLod(lod));
        }
        let cells = LOD0_CELLS >> lod;
        let dim = cells + 1 + 2 * HALO;
        let step_half = 1_i32 << lod;
        let mut base_half = [0_i32; 3];
        for (index, block) in [coord.x, coord.y, coord.z].into_iter().enumerate() {
            let base = block
                .checked_mul(BLOCK_EDGE_METERS)
                .and_then(|n| n.checked_mul(2))
                .ok_or(DensityError::CoordinateOutOfRange)?;
            let lower = base
                .checked_sub(step_half as i64)
                .ok_or(DensityError::CoordinateOutOfRange)?;
            let upper = base
                .checked_add((cells as i64 + 1) * step_half as i64)
                .ok_or(DensityError::CoordinateOutOfRange)?;
            i32::try_from(lower).map_err(|_| DensityError::CoordinateOutOfRange)?;
            i32::try_from(upper).map_err(|_| DensityError::CoordinateOutOfRange)?;
            base_half[index] =
                i32::try_from(base).map_err(|_| DensityError::CoordinateOutOfRange)?;
        }
        Ok(Self {
            coord,
            lod,
            cells,
            dim,
            step_half,
            base_half,
        })
    }

    pub fn len(self) -> usize {
        self.dim * self.dim * self.dim
    }

    pub fn offset(self, x: usize, y: usize, z: usize) -> Option<usize> {
        (x < self.dim && y < self.dim && z < self.dim).then_some((z * self.dim + y) * self.dim + x)
    }

    pub fn half_position(self, x: usize, y: usize, z: usize) -> Option<[i32; 3]> {
        self.offset(x, y, z)?;
        let indices = [x, y, z];
        Some(std::array::from_fn(|i| {
            self.base_half[i] + (indices[i] as i32 - HALO as i32) * self.step_half
        }))
    }

    /// 32-byte shader uniform: three i32 origins, u32 seed, i32 step,
    /// u32 dimension, u32 count and u32 padding.
    pub fn uniform_words(self, seed: u32) -> [u32; 8] {
        [
            self.base_half[0] as u32,
            self.base_half[1] as u32,
            self.base_half[2] as u32,
            seed,
            self.step_half as u32,
            self.dim as u32,
            self.len() as u32,
            0,
        ]
    }
}

#[derive(Debug)]
pub struct DensityBlock {
    pub grid: DensityGrid,
    pub seed: u32,
    /// Samples in x-fastest, then y, then z order, including one-sample halo.
    pub samples: Vec<f32>,
}

impl DensityBlock {
    pub fn generate(seed: u32, coord: BlockCoord, lod: u8) -> Result<Self, DensityError> {
        let grid = DensityGrid::new(coord, lod)?;
        let mut samples = Vec::with_capacity(grid.len());
        for z in 0..grid.dim {
            for y in 0..grid.dim {
                for x in 0..grid.dim {
                    let [hx, hy, hz] = grid.half_position(x, y, z).unwrap();
                    samples.push(density_half(seed, hx, hy, hz));
                }
            }
        }
        Ok(Self {
            grid,
            seed,
            samples,
        })
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<f32> {
        self.grid.offset(x, y, z).map(|idx| self.samples[idx])
    }
}

pub fn hash2(seed: u32, x: i32, z: i32) -> u32 {
    let mut h = seed ^ (x as u32).wrapping_mul(0x9e37_79b1) ^ (z as u32).wrapping_mul(0x85eb_ca77);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn hash01(seed: u32, x: i32, z: i32) -> f32 {
    (hash2(seed, x, z) & 1023) as f32 * (1.0 / 1023.0)
}
pub fn noise2_half(seed: u32, hx: i32, hz: i32, scale: i32) -> f32 {
    assert!(scale > 0);
    let gx = hx.div_euclid(scale);
    let gz = hz.div_euclid(scale);
    let tx = smooth(hx.rem_euclid(scale) as f32 / scale as f32);
    let tz = smooth(hz.rem_euclid(scale) as f32 / scale as f32);
    let a = lerp(hash01(seed, gx, gz), hash01(seed, gx + 1, gz), tx);
    let b = lerp(hash01(seed, gx, gz + 1), hash01(seed, gx + 1, gz + 1), tx);
    lerp(a, b, tz)
}
pub fn terrain_height_half(seed: u32, hx: i32, hz: i32) -> f32 {
    -3.0 + 11.0 * noise2_half(seed ^ 0x045d_9f3b, hx, hz, 128)
        + 5.0 * noise2_half(seed ^ 0x9e37_79b9, hx, hz, 32)
}
pub fn cave_center_half(seed: u32, hx: i32) -> (f32, f32) {
    let cy = -7.0 + 2.0 * noise2_half(seed ^ 0x04c1_1db7, hx, 0, 128);
    let cz = -3.0 + 6.0 * noise2_half(seed ^ 0x13a5_ba1d, hx, 0, 128);
    (cy, cz)
}
pub fn density_half(seed: u32, hx: i32, hy: i32, hz: i32) -> f32 {
    let y = hy as f32 * 0.5;
    let z = hz as f32 * 0.5;
    let height = terrain_height_half(seed, hx, hz);
    let (cy, cz) = cave_center_half(seed, hx);
    let dy = y - cy;
    let dz = z - cz;
    (y - height).max(CAVE_RADIUS - (dy * dy + dz * dz).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coord(x: i64, y: i64, z: i64) -> BlockCoord {
        BlockCoord { x, y, z }
    }

    #[test]
    fn dimensions_lods_and_halo() {
        for (lod, cells, dim) in [(0, 32, 35), (1, 16, 19), (2, 8, 11)] {
            let g = DensityGrid::new(coord(0, 0, 0), lod).unwrap();
            assert_eq!((g.cells, g.dim, g.len()), (cells, dim, dim * dim * dim));
            assert_eq!(g.half_position(0, 0, 0), Some([-(1 << lod); 3]));
            assert_eq!(g.half_position(dim - 1, 0, 0).unwrap()[0], 32 + (1 << lod));
            assert!(g.offset(dim, 0, 0).is_none());
        }
    }

    #[test]
    fn reproducible_and_seed_sensitive() {
        let a = DensityBlock::generate(42, coord(-3, -1, 4), 1).unwrap();
        let b = DensityBlock::generate(42, coord(-3, -1, 4), 1).unwrap();
        let c = DensityBlock::generate(43, coord(-3, -1, 4), 1).unwrap();
        assert_eq!(a.samples, b.samples);
        assert!(a.samples.iter().zip(&c.samples).any(|(x, y)| x != y));
    }

    #[test]
    fn all_axis_boundaries_share_identical_density_at_negative_coords() {
        for lod in 0..=2 {
            let a = DensityBlock::generate(1234, coord(-1, -1, -1), lod).unwrap();
            let edge = a.grid.cells + HALO;
            for axis in 0..3 {
                let mut next = [-1_i64; 3];
                next[axis] += 1;
                let b =
                    DensityBlock::generate(1234, coord(next[0], next[1], next[2]), lod).unwrap();
                for j in [0, a.grid.dim / 2, a.grid.dim - 1] {
                    for k in [0, a.grid.dim / 2, a.grid.dim - 1] {
                        let (a_pos, b_pos) = match axis {
                            0 => ([edge, j, k], [HALO, j, k]),
                            1 => ([j, edge, k], [j, HALO, k]),
                            _ => ([j, k, edge], [j, k, HALO]),
                        };
                        assert_eq!(
                            a.get(a_pos[0], a_pos[1], a_pos[2]),
                            b.get(b_pos[0], b_pos[1], b_pos[2])
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn shared_lod_vertices_sample_same_world_values() {
        let a = DensityBlock::generate(7, coord(1, -1, -2), 0).unwrap();
        let b = DensityBlock::generate(7, coord(1, -1, -2), 1).unwrap();
        let c = DensityBlock::generate(7, coord(1, -1, -2), 2).unwrap();
        for z in 1..=8 {
            for y in 1..=8 {
                for x in 1..=8 {
                    let p = a.get((x - 1) * 4 + 1, (y - 1) * 4 + 1, (z - 1) * 4 + 1);
                    assert_eq!(p, b.get((x - 1) * 2 + 1, (y - 1) * 2 + 1, (z - 1) * 2 + 1));
                    assert_eq!(p, c.get(x, y, z));
                }
            }
        }
    }

    #[test]
    fn continuous_cave_tunnel_has_open_core_and_solid_walls() {
        for meters in (-256..=256).step_by(8) {
            let hx = meters * 2;
            let (cy, cz) = cave_center_half(1234, hx);
            let hy = (cy * 2.0).round() as i32;
            let hz = (cz * 2.0).round() as i32;
            assert!(density_half(1234, hx, hy, hz) > 1.0);
            assert!(density_half(1234, hx, hy, hz + 16) < 0.0);
        }
    }

    #[test]
    fn solid_below_and_air_above() {
        for hx in [-128, -65, -1, 0, 1, 64, 128] {
            for hz in [-128, -1, 0, 1, 64] {
                let h = terrain_height_half(987, hx, hz);
                assert!((-3.0..=13.0).contains(&h));
                assert!(density_half(987, hx, 100, hz) > 0.0);
                assert!(density_half(987, hx, -100, hz) < 0.0);
            }
        }
    }

    #[test]
    fn unsupported_lod_and_integer_overflow_rejected() {
        assert_eq!(
            DensityGrid::new(coord(0, 0, 0), 3),
            Err(DensityError::InvalidLod(3))
        );
        assert_eq!(
            DensityGrid::new(coord(i64::MAX, 0, 0), 0),
            Err(DensityError::CoordinateOutOfRange)
        );
    }

    #[test]
    fn density_wgsl_module_parses_and_validates() {
        let shader = include_str!("../../../assets/shaders/sdf_density.wgsl");
        let module = naga::front::wgsl::parse_str(shader)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(shader)));
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator.validate(&module).expect("WGSL validation failed");
        assert!(module.entry_points.iter().any(|entry| entry.name == "main"));
    }

    #[test]
    fn shader_uniform_layout_stable() {
        let g = DensityGrid::new(coord(-1, 2, -3), 2).unwrap();
        assert_eq!(
            g.uniform_words(99),
            [(-32_i32) as u32, 64, (-96_i32) as u32, 99, 4, 11, 1331, 0]
        );
    }
}
