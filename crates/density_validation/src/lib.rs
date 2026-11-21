//! B1's hardware parity tests are test-only: compiling the production library
//! must not require wgpu, naga, or a GPU.

#[cfg(test)]
mod validation;

#[cfg(test)]
mod b2_extraction;

#[cfg(test)]
mod b3_transition_gpu;
