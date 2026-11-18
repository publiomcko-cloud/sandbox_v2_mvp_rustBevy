# B0 — Windows/Vulkan/GPU bootstrap

State: **implementation submitted to feature branch; CI and graphical validation must pass before B0 is accepted**.

## Scope and implementation boundary

- Rust 1.100.0 is fixed in rust-toolchain.toml; Bevy 0.20.1 and Avian 0.8.0 are fixed in Cargo.toml.
- The first Windows Actions run generates Cargo.lock, runs format/tests/release build, and attempts to commit the lock to the feature branch. If the repo blocks the action's write permission, preserve the artifact and commit Cargo.lock manually before merge.
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

Controls: W/A/S/D horizontal translation, Q/E vertical, 18 m/s, Esc/window close to quit. Test scene: two cubes sharing the compute-generated mesh, a ground circle, light and camera. Verify both cubes have actual faces after GPU compute rather than remaining collapsed at the origin.

## Evidence and gate

| Check | Required observation | State before CI/local testing |
| --- | --- | --- |
| Dependency compatibility | Bevy/Avian resolve and compile | NOT TESTED |
| Rust format + unit tests | Pass on Windows | NOT TESTED |
| Release Windows binary | Compiled and artifact uploaded | NOT TESTED |
| Compute mesh visual | GPU generated geometry is visible | NOT TESTED |
| No bulk readback | GPU buffer consumed directly by render pipeline | IMPLEMENTED IN PROTOTYPE; VISUAL UNVERIFIED |
| Frame diagnostics | FPS/frame-time logs produced | NOT TESTED |
| True 60 FPS / 130 m / 100 ms | Formal contract requires whole pipeline | NOT IN B0 |

This stage cannot be approved by headless CI alone. Manual Windows/Vulkan screenshot and logs are still needed to confirm actual displayed pixels and GPU behavior.

## Reference

GPU bootstrap adapted from Bevy 0.20.1 `examples/shader_advanced/compute_mesh.rs` and `assets/shaders/compute_mesh.wesl` (MIT OR Apache-2.0): https://github.com/bevyengine/bevy/tree/v0.20.1/examples/shader_advanced
