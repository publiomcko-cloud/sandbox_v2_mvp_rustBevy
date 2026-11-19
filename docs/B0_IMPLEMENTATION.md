# B0 — Windows/Vulkan/GPU bootstrap

State: **B0 GPU feasibility test validated by Windows CI and manual graphical execution (2026-11-19). Artifact packaging correction is awaiting a new CI pass.**

## Scope and implementation boundary

- Rust 1.100.0 is fixed in rust-toolchain.toml; Bevy 0.20.1 and Avian 0.8.0 are fixed in Cargo.toml.
- The committed Cargo.lock is tested with `--locked` on the organization's Windows self-hosted runner. CI #44410518237 passed formatting, core and workspace tests, release compilation, and artifact upload.
- GpuTerrainBootstrapPlugin uses Bevy's RenderApp, MeshAllocator slabs, compute shader and storage/index buffers. The shader creates a **fixed test cube** on the GPU and the render path consumes the same buffers. There is no GPU->CPU geometry readback. This is a pipeline feasibility proof, **not** SDF meshing, streaming, true indirect drawing, revision-gated GPU publication or terrain.
- sandbox-streaming includes only a pure-Rust publication-state invariant and tests; it is not integrated with RenderApp in B0.
- Avian PhysicsPlugins is loaded but B0 creates no colliders or rigid bodies.
- FrameTimeDiagnosticsPlugin and LogDiagnosticsPlugin provide preliminary runtime diagnostics, **not** the required CPU + GPU + presentation frame budget.
- Bevy's WESL is used for the compute shader; its contents use WGSL-compatible syntax.

## Reproduce

Requirements: Windows 10/11, modern graphics driver with compute + storage buffer support, Rustup + Cargo, PowerShell.

```powershell
git fetch origin
git switch feat/b0-rust-bevy-gpu-bootstrap-20261118
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-b0.ps1
```

Alternative after Cargo.lock has been committed:

```powershell
cargo test --workspace --locked
cargo run --release --locked -p sandbox-game
```

Run from the repo root because Bevy discovers assets relative to the current working directory.

### Prebuilt Windows artifact

The CI release artifact must contain:
```text
sandbox-game.exe
Cargo.lock
assets/shaders/compute_mesh.wesl
```

Extract it without changing that directory structure, open PowerShell in the extracted artifact root, and run `./sandbox-game.exe`. Do not run only a copied standalone EXE without its `assets` folder: that produces the ground circle but not GPU-generated cubes.

A prior artifact omitted `assets/shaders/compute_mesh.wesl`. The 2026-11-19 manual test confirmed both colored cubes become visible after placing the WESL file at the expected path. The packaging fix prevents this missing-asset failure, subject to new CI validation.

Controls: W/A/S/D horizontal translation, Q/E vertical, 18 m/s, Esc/window close to quit. Test scene: two cubes sharing the compute-generated mesh, a ground circle, light and camera. Verify both cubes have actual faces after GPU compute rather than remaining collapsed at the origin.

## Evidence and gate

| Check | Required observation | Evidence |
| --- | --- | --- |
| Dependency compatibility | Bevy/Avian resolve and compile | PASS — self-hosted CI #44410518237 |
| Rust format + unit tests | Pass on Windows | PASS — core and workspace tests in #44410518237 |
| Release Windows binary | Compiled and artifact uploaded | PASS — #44410518237 |
| Compute mesh visual | GPU-generated geometry is visible | PASS — manual Windows 11 + Vulkan + NVIDIA GTX 1050 Ti: both colored cubes visible, 2026-11-19 |
| No bulk readback | GPU buffer consumed directly by render pipeline | Code path confirmed; not independently profiled |
| Frame diagnostics | FPS/frame-time logs produced | PASS — static test roughly 60 FPS, with several observed frame times above 16.7 ms |
| Packaged assets | Executable and shader bundled | Fix committed; new CI validation required |
| True 60 FPS / 130 m / 100 ms | Formal contract requires whole pipeline | OUT OF SCOPE B0; not proven |

B0 validates the GPU compute-to-mesh rendering bootstrap, not 60 FPS under actual terrain, streaming, editing, water, or physics workloads. The GPU/WESL proof was visually validated after restoring the missing shader file.

## Reference

GPU bootstrap adapted from Bevy 0.20.1 `examples/shader_advanced/compute_mesh.rs` and `assets/shaders/compute_mesh.wesl` (MIT OR Apache-2.0): https://github.com/bevyengine/bevy/tree/v0.20.1/examples/shader_advanced
