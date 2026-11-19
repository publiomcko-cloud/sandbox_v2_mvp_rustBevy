# B1 — SDF Rust, WGSL and density grids

Status: IMPLEMENTED CANDIDATE; CI and real Vulkan numeric parity must pass before B1 is accepted.

## Scope

- The CPU reference implementation is in crates/world_core/src/density.rs, with no Bevy dependency.
- A single u32 world seed and integer half-metre coordinates define the height field and a connected cave tunnel along the X direction. Negative density is solid, positive is air.
- World storage identifiers remain i64 block coordinates. B1 explicitly rejects dispatches outside the 32-bit half-metre GPU sample domain instead of wrapping silently. Large-world high/low coordinate support is deferred.
- The level set is a density approximation, not an exact Euclidean SDF near steep slopes and cave intersections; extraction must use the zero crossing, and future physical distance use requires a distance-correction strategy.
- Blocks are 16m. LOD0 samples 32^3 cells (0.5m), LOD1 16^3 (1m) and LOD2 8^3 (2m).
- Each grid includes one sample outside each face plus 33/17/9 shared boundary vertices: dimensions 35^3, 19^3, 11^3. Order is X-fastest, then Y, then Z.
- The GPU shader is assets/shaders/sdf_density.wgsl. Its parameters are 32 bytes: i32 base_x/y/z in half-metres, u32 seed, i32 step_half, u32 dimension/count and padding. It writes one f32 per sample, 64 threads per workgroup.
- CPU and WGSL use the same u32 avalanche hash and floor-division for negative coordinates. Floating point precision may differ slightly because GPU and CPU can choose different arithmetic contractions.

## Numeric gates

1. CPU deterministic seed reproducibility, negative-coordinate samples, terrain signs and connected cave tunnel.
2. Exact shared grid vertices across all block axes and between LOD0/1/2.
3. Shader must parse and validate with Naga 30.
4. Real GPU dispatch/readback in TEST ONLY, comparing three seeded blocks including negative coordinates, LOD0/1/2 and caves; each sample must have absolute density error <= 0.03 m. Any incompatible device or numeric disagreement fails this gate. This small diagnostic readback is not used in runtime terrain rendering.
5. All B0 workspace tests and Windows Release compile must still pass.

## Explicit limitations

- B1 density generation is a separate prototype and is not yet wired into the B0 render graph. B2 integrates GPU density with extraction and resident drawing.
- No GPU terrain mesh, streaming, seam stitching, runtime edits, water or physics are claimed in B1.
- The B0 visual test passed on 2026-11-19 with the two colored cubes after restoring the shader asset. A later CI run failed on an alternate Windows runner missing Visual Studio MSVC linker; this is an infrastructure issue, not B0 core/regression failure.
- B0 shader packaging was patched in commit 7647f70, but a successful packaged-artifact CI run remains pending.

## Reproduction

To run CPU numeric gates (Rust 1.100 / MSVC tools):

    cargo test --locked -p sandbox-world-core

To validate shader without a GPU:

    cargo test --locked -p sandbox-density-gpu-validation b1_shader_is_valid_wgsl_and_has_expected_entry_point

To run actual GPU parity on a machine with Vulkan:

    set WGPU_BACKEND=vulkan
    cargo test --locked -p sandbox-density-gpu-validation b1_cpu_gpu_density_parity_lod_seed_negative_coordinates_and_cave -- --ignored --nocapture

The B1 GitHub Actions workflow executes all of these checks and the complete workspace regression.


## Memory-aware validation separation (2026-11-19)

The original Windows B1 CI passed all ten CPU tests, then failed while compiling
Bevy dependencies for the shader test: `rustc-LLVM ERROR: out of memory`
(`bevy_reflect`, followed by a `windows` allocation failure). The GPU
parity test had not started. This was NOT a measured CPU/GPU density mismatch.

The Naga/wgpu test harness now lives in the standalone
`sandbox-density-gpu-validation` package. It has **no Bevy dependency**.
The self-hosted Windows CI compiles this lightweight package with one Cargo
job and debug symbols disabled, and then requires a real Vulkan GPU adapter
for parity. Independently, `b1-bevy-regression.yml` tests the full Bevy
workspace on hosted Windows hardware. No true B1 acceptance is claimed until
all gates pass.

Commands:

    cargo test -p sandbox-density-gpu-validation --locked b1_shader_is_valid_wgsl_and_has_expected_entry_point
    cargo test -p sandbox-density-gpu-validation --locked b1_cpu_gpu_density_parity_lod_seed_negative_coordinates_and_cave -- --ignored --nocapture


## Dedicated GPU runner label

The organization has at least two Windows runners. Generic Windows jobs can land on
`DESKTOP-T0U6S55`, which currently does not have the MSVC `link.exe` toolchain.
For reliable and reproducible real GPU parity, the B1 Windows workflow now
requires the custom label `b1-vulkan-gpu` in addition to the default Windows
labels. Assign this custom label only to the Windows machine that has a Vulkan GPU
and functioning Visual Studio C++ Build Tools (currently the previously qualified
`DESKTOP-SAFGO15`). Until the label is assigned, the GPU job will correctly
remain queued instead of producing a misleading failure on a nonqualified runner.
