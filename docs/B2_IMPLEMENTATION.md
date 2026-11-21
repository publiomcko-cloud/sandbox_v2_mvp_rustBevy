# B2 — GPU-resident Surface Nets (candidate)

## Goal and scope

B2 turns the B1 GPU density field into actual visible geometry without a
runtime GPU→CPU triangle readback. On Bevy 0.20.1 the normal MeshAllocator
allocates the vertex/index storage. The compute graph fills those slabs on
the SAME GPU and Bevy renders them in the camera pass.

Stages per static block:
1. B1 WGSL density compute → resident density storage (35³ / 19³ / 11³ samples).
2. Surface Nets GPU vertices (one per 3D cell): intersection average, SDF gradient normal.
3. Surface Nets GPU topology: six indices per candidate crossing grid edge.
4. Bevy camera draw reads the same vertex/index slabs directly.

No CPU mesh extraction, mesh readback or per-frame triangle upload occurs in
the B2 runtime. The CPU only creates fixed-size zero-filled allocation
placeholders (these do not contain surface geometry) and dispatch parameters.

## Prototype bounds

- Static 2 × 2 × 2 sample patch: eight blocks, each 16 m, seed 42.
- Default LOD1: 16³ cells/block at 1 m resolution. Optional LOD0=32³
  (0.5 m) and LOD2=8³ (2 m).
- Fixed-capacity allocation: cells³ vertices and 18·cells³ indices. All
  unused index slots become degenerate triangles; no prefix scan, compaction
  or indirect draw yet.
- Mesh vertex layout is Bevy's GPU slab order: position xyz, normal xyz,
  UV xy = 8 f32 per vertex. Explicit bounds checks prevent cross-slab writes.
- Shader-written vertex/index data remains resident and is not recomputed
  each frame; the density buffer is allocated per block and retained.
- The GPU-written surface has no reliable CPU placeholder AABB; static B2
  disables frustum culling.
- Neighbor edges are not yet stitched. B3 must prove zero seams between
  adjacent blocks and different LODs before 130 m streaming. No fog or skirts
  are used to conceal gaps.
- New revisions and atomic swap of old→new resident geometry are B4.
  Water, rocks, collisions and persistence are later stages.

## Execute visual demo

Build release from the B2 branch or download the B2 Windows artifact
from GitHub Actions, extract everything, and open PowerShell in its root.
Keep the entire assets/shaders directory beside sandbox-game.exe.

    .\sandbox-game.exe

Optional switches (set before launching):

    $env:SANDBOX_B2_LOD = "0"      # 0.5 m near-field
    $env:SANDBOX_B2_SCENE = "cave" # camera inside the tunnel
    $env:SANDBOX_STAGE = "B0"     # previous two-cube regression scene

Unset them between tests as desired:

    Remove-Item Env:SANDBOX_STAGE -ErrorAction SilentlyContinue

Control: WASD and Q/E, speed 18 m/s. The camera can enter the tunnel.
Observe that the generated mesh is hilly/volumetric rather than two cubes.

## Gates

- Rustfmt and complete unit tests.
- WGSL parse/validation for both B1 density and B2 Surface Nets.
- Windows Release and self-contained executable+shader artifact.
- Real GPU geometry smoke: verify valid and nondegenerate triangles on a
  hardware Vulkan adapter, without a readback in production.
- Interactive visual acceptance: actual terrain pixels, shader logs, no
  shader compilation errors, no reupload every frame.

## Acceptance boundaries

The eventual 60 FPS/16.7 ms frame contract, 130 m LOD horizon, 18 m/s gapless
movement, edits in 100 ms, physics, water, rocks, and seam-free transitions are
NOT demonstrated by B2 and must not be inferred from this static proof.
This implementation is a B2 candidate until CI and interactive GPU
visual gate pass.
