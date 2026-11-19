//! B1's hardware parity tests are test-only: compiling the production library
//! must not require wgpu, naga, or a GPU.

#[cfg(test)]
mod validation;
