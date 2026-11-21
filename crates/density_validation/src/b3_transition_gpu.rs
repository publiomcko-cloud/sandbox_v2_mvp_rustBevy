//! Experimental GPU-side 2:1 dual-grid seam geometry validation.
#![cfg(test)]

use sandbox_world_core::{BlockCoord, density::DensityGrid};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::Duration,
};
use wgpu::util::DeviceExt;

const SEAM_SHADER: &str = include_str!("../../../assets/shaders/lod_transition.wgsl");

fn await_future<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::sleep(Duration::from_millis(2)),
        }
    }
}

fn pack_lods(current_lod: u8, negative_neighbor_lod: u8) -> (u32, u32) {
    let mut words = [0u32; 2];
    for z in 0..3 {
        for y in 0..3 {
            for x in 0..3 {
                let index = (z * 9 + y * 3 + x) as u32;
                let lod = if x == 0 {
                    negative_neighbor_lod
                } else {
                    current_lod
                };
                let bit = index * 2;
                let slot = (bit / 32) as usize;
                words[slot] |= (lod as u32) << (bit % 32);
            }
        }
    }
    (words[0], words[1])
}

#[test]
fn packed_seam_neighborhood_matches_3d_cell_coordinates() {
    for center in 0..=2 {
        for other in 0..=2 {
            let (a, b) = pack_lods(center, other);
            for z in 0..3 {
                for y in 0..3 {
                    for x in 0..3 {
                        let bit = (z * 9 + y * 3 + x) * 2;
                        let source = if bit < 32 { a >> bit } else { b >> (bit - 32) };
                        assert_eq!((source & 3) as u8, if x == 0 { other } else { center });
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "Requires dedicated Windows/Vulkan GTX 1050 Ti runner; diagnostic-only readback"]
fn b3_gpu_adaptive_dual_seam_emits_valid_cross_boundary_triangles() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = await_future(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("Vulkan adapter required");
    let info = adapter.get_info();
    assert!(matches!(
        info.device_type,
        wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu
    ));
    println!("B3 adaptive seam adapter: {}", info.name);
    let (device, queue) = await_future(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("Vulkan device required");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("B3 adaptive dual seam"),
        source: wgpu::ShaderSource::Wgsl(SEAM_SHADER.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("B3 seam shared B2 binding contract"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("B3 seam layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("B3 seam shader execution"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("emit_transition"),
        compilation_options: Default::default(),
        cache: None,
    });

    for (lod, neighbor_lod) in [(1, 2), (2, 1)] {
        let grid = DensityGrid::new(BlockCoord { x: 0, y: -1, z: 0 }, lod).unwrap();
        let cells = grid.cells as u32;
        let normal_vertices = (cells + 1).pow(3);
        let normal_indices = 18 * cells.pow(3);
        // Only -X face is active, with 2*32*32 candidate fine-grid edges.
        let seam_candidates = 2 * 32 * 32;
        let vertices_count = normal_vertices + seam_candidates * 4;
        let indices_count = normal_indices + seam_candidates * 6;
        let (word0, word1) = pack_lods(lod, neighbor_lod);

        let words: [u32; 16] = [
            grid.base_half[0] as u32,
            grid.base_half[1] as u32,
            grid.base_half[2] as u32,
            42,
            grid.step_half as u32,
            grid.dim as u32,
            grid.len() as u32,
            1,
            0,
            vertices_count * 8,
            0,
            indices_count,
            cells,
            0,
            word0,
            word1,
        ];
        let mut uniform_bytes = Vec::new();
        for word in words {
            uniform_bytes.extend_from_slice(&word.to_le_bytes());
        }
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("B3 seam uniform"),
            contents: &uniform_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let dummy_density = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B3 dummy read-only density"),
            size: grid.len() as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let vertex_bytes = (vertices_count * 8 * 4) as u64;
        let index_bytes = (indices_count * 4) as u64;
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B3 seam vertices"),
            size: vertex_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B3 seam indices"),
            size: index_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read_vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B3 seam test vertex staging"),
            size: vertex_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let read_indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B3 seam test index staging"),
            size: index_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("B3 seam shared binding group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dummy_density.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: vertices.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: indices.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("B3 seam compute diagnostic"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("B3 seam dispatch"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups((3 * seam_candidates).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&vertices, 0, &read_vertices, 0, vertex_bytes);
        encoder.copy_buffer_to_buffer(&indices, 0, &read_indices, 0, index_bytes);
        queue.submit(Some(encoder.finish()));
        let (vtx, vrx) = std::sync::mpsc::channel();
        let (itx, irx) = std::sync::mpsc::channel();
        read_vertices
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |status| {
                let _ = vtx.send(status);
            });
        read_indices
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |status| {
                let _ = itx.send(status);
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .expect("B3 seam GPU execution failed");
        vrx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        irx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();

        let vm = read_vertices
            .get_mapped_range(..)
            .expect("vertex readback mapping");
        let im = read_indices
            .get_mapped_range(..)
            .expect("index readback mapping");
        let floats: Vec<f32> = vm
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let index_values: Vec<u32> = im
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert!(
            index_values.iter().all(|v| *v < vertices_count),
            "out of bounds seam index"
        );
        let mut triangles = 0usize;
        let mut crossing = 0usize;
        for face in index_values[normal_indices as usize..].chunks_exact(3) {
            if face[0] == face[1] || face[1] == face[2] || face[0] == face[2] {
                continue;
            }
            triangles += 1;
            let mut x_low = false;
            let mut x_high = false;
            for &index in face {
                let start = index as usize * 8;
                assert!(floats[start..start + 6].iter().all(|v| v.is_finite()));
                x_low |= floats[start] < -0.00001;
                x_high |= floats[start] >= -0.00001;
            }
            if x_low && x_high {
                crossing += 1;
            }
        }
        println!(
            "B3 Vulkan 2:1 adaptive seam PASS lod={lod} other={neighbor_lod} triangles={triangles} crossing={crossing} index_slots={indices_count}"
        );
        assert!(
            triangles > 0,
            "GPU seam emitted no triangles for real B1 cave/SDF"
        );
        assert!(
            crossing > 0,
            "GPU seam did not connect vertices across the block border"
        );
        drop(vm);
        drop(im);
        read_vertices.unmap();
        read_indices.unmap();
    }
}
