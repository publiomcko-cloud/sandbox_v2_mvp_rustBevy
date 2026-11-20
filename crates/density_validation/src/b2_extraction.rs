//! B2 GPU extraction proof. Readback exists ONLY in this ignored test
//! to verify indices/vertices; the Bevy runtime never maps these buffers.

use sandbox_world_core::{BlockCoord, density::DensityGrid};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::Duration,
};

const DENSITY_SHADER: &str = include_str!("../../../assets/shaders/sdf_density.wgsl");
const SURFACE_SHADER: &str = include_str!("../../../assets/shaders/surface_nets.wgsl");

fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::sleep(Duration::from_millis(2)),
        }
    }
}

#[test]
#[ignore = "Requires physical Vulkan GPU; diagnostic-only small GPU->CPU readback"]
fn b2_gpu_surface_nets_writes_non_degenerate_indexed_geometry() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = wait(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("B2 requires Vulkan adapter");
    let info = adapter.get_info();
    assert!(matches!(
        info.device_type,
        wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu
    ));
    println!("B2 Vulkan device: {} ({:?})", info.name, info.device_type);
    let (device, queue) = wait(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("B2 Vulkan device unavailable");

    let density_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("B2 density"),
        source: wgpu::ShaderSource::Wgsl(DENSITY_SHADER.into()),
    });
    let extract_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("B2 extraction"),
        source: wgpu::ShaderSource::Wgsl(SURFACE_SHADER.into()),
    });
    let density_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("B2 density compute pipeline"),
        layout: None,
        module: &density_shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    // Exercise the same explicit four-binding layout as the Bevy B2 runtime.
    // Auto-inferred per-entry-point layouts would miss this class of bug.
    let surface_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("B2 shared Surface Nets layout regression"),
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
    let surface_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("B2 shared Surface Nets pipeline layout regression"),
        bind_group_layouts: &[Some(&surface_layout)],
        immediate_size: 0,
    });
    let vertices_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("B2 vertex compute pipeline"),
        layout: Some(&surface_pipeline_layout),
        module: &extract_shader,
        entry_point: Some("emit_vertices"),
        compilation_options: Default::default(),
        cache: None,
    });
    let indices_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("B2 index compute pipeline"),
        layout: Some(&surface_pipeline_layout),
        module: &extract_shader,
        entry_point: Some("emit_indices"),
        compilation_options: Default::default(),
        cache: None,
    });

    let mut output = std::collections::HashMap::new();
    for (coord, lod) in [
        (BlockCoord { x: 0, y: 0, z: 0 }, 2),
        (BlockCoord { x: -1, y: 0, z: 0 }, 2),
        (BlockCoord { x: 0, y: -1, z: 0 }, 2),
        (BlockCoord { x: 0, y: 0, z: -1 }, 2),
        (BlockCoord { x: 0, y: -1, z: -1 }, 2),
    ] {
        let grid = DensityGrid::new(coord, lod).unwrap();
        let cells = grid.cells as u32;
        let count = cells.pow(3);
        let vertex_count = (cells + 1).pow(3);
        let vertices_bytes = vertex_count as u64 * 8 * 4;
        let indices_bytes = count as u64 * 18 * 4;

        let mut bytes = Vec::with_capacity(64);
        let uniforms = [
            grid.base_half[0] as u32,
            grid.base_half[1] as u32,
            grid.base_half[2] as u32,
            42,
            grid.step_half as u32,
            grid.dim as u32,
            grid.len() as u32,
            0,
            0,
            vertex_count * 8,
            0,
            count * 18,
            grid.cells as u32,
            0,
            0,
            0,
        ];
        for word in uniforms {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        use wgpu::util::DeviceExt;
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("B2 validation params"),
            contents: &bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let density = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B2 validation samples"),
            size: grid.len() as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B2 validation vertices"),
            size: vertices_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B2 validation indices"),
            size: indices_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let v_staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B2 test-only vertex readback"),
            size: vertices_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let i_staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("B2 test-only index readback"),
            size: indices_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let density_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("B2 density group"),
            layout: &density_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: density.as_entire_binding(),
                },
            ],
        });
        // One explicit four-slot bind group is shared by both entry points,
        // including the unused slot for each individual compute stage.
        let surface_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("B2 shared Surface Nets group"),
            layout: &surface_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: density.as_entire_binding(),
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
            label: Some("B2 extraction regression"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Density"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&density_pipeline);
            pass.set_bind_group(0, &density_group, &[]);
            pass.dispatch_workgroups((grid.len() as u32).div_ceil(64), 1, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Surface vertices"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&vertices_pipeline);
            pass.set_bind_group(0, &surface_group, &[]);
            pass.dispatch_workgroups(vertex_count.div_ceil(64), 1, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Surface indices"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&indices_pipeline);
            pass.set_bind_group(0, &surface_group, &[]);
            pass.dispatch_workgroups((3 * count).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&vertices, 0, &v_staging, 0, vertices_bytes);
        encoder.copy_buffer_to_buffer(&indices, 0, &i_staging, 0, indices_bytes);
        queue.submit(Some(encoder.finish()));

        let (vtx, vrx) = std::sync::mpsc::channel();
        let (itx, irx) = std::sync::mpsc::channel();
        v_staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |s| {
                let _ = vtx.send(s);
            });
        i_staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |s| {
                let _ = itx.send(s);
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .expect("B2 GPU submission failed");
        vrx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        irx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();

        let v_mapping = v_staging.get_mapped_range(..).expect("vertex map");
        let i_mapping = i_staging.get_mapped_range(..).expect("index map");
        let values: Vec<f32> = v_mapping
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        let list: Vec<u32> = i_mapping
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert!(
            values.iter().all(|v| v.is_finite()),
            "nonfinite vertex data"
        );
        assert!(
            list.iter().all(|index| *index < vertex_count),
            "out-of-range index"
        );
        let triangles = list
            .chunks_exact(3)
            .filter(|triangle| {
                triangle[0] != triangle[1]
                    && triangle[1] != triangle[2]
                    && triangle[0] != triangle[2]
            })
            .count();
        if coord == (BlockCoord { x: 0, y: 0, z: 0 })
            || coord == (BlockCoord { x: 0, y: -1, z: -1 })
        {
            assert!(triangles > 0, "no nondegenerate triangles coord={coord:?}");
        }
        println!(
            "B2 Vulkan Surface Nets PASS coord={coord:?} lod={lod} triangles={triangles} vertices={count} index_slots={}",
            list.len()
        );
        // Preserve the diagnostic readback only in this test, allowing
        // verification of replicated Surface Nets cells at neighboring
        // block faces. Production B2 never copies geometry to the CPU.
        output.insert((coord.x, coord.y, coord.z), values);
        drop(v_mapping);
        drop(i_mapping);
        v_staging.unmap();
        i_staging.unmap();
    }

    // Compare all replicated cell vertices, not only SDF corner samples.
    // A local slot 0 on a low boundary is the same world-space cell as
    // slot 'cells' on the neighboring block's high boundary.
    let cells = DensityGrid::new(BlockCoord { x: 0, y: 0, z: 0 }, 2)
        .unwrap()
        .cells;
    let vd = cells + 1;
    let center = output.get(&(0, 0, 0)).unwrap();
    for axis in 0..3 {
        let coord = match axis {
            0 => (-1, 0, 0),
            1 => (0, -1, 0),
            _ => (0, 0, -1),
        };
        let neighbor = output.get(&coord).unwrap();
        let neighbor_origin = [coord.0 as f32 * 16.0, coord.1 as f32 * 16.0,
            coord.2 as f32 * 16.0];
        let slot = |p: [usize; 3]| ((p[2] * vd + p[1]) * vd + p[0]) * 8;
        for v in 0..vd {
            for u in 0..vd {
                let mut a = [0usize; 3];
                let mut b = [0usize; 3];
                match axis {
                    0 => { a[1] = u; a[2] = v; b[0] = cells; b[1] = u; b[2] = v; }
                    1 => { a[0] = u; a[2] = v; b[0] = u; b[1] = cells; b[2] = v; }
                    _ => { a[0] = u; a[1] = v; b[0] = u; b[1] = v; b[2] = cells; }
                };
                let ia = slot(a);
                let ib = slot(b);
                for component in 0..3 {
                    let delta = (center[ia + component] -
                        (neighbor[ib + component] + neighbor_origin[component])).abs();
                    assert!(delta <= 0.0002,
                        "B2 seam mismatch axis={axis} a={a:?} b={b:?} component={component} error={delta}");
                }
                for component in 3..6 {
                    let delta = (center[ia + component] -
                        neighbor[ib + component]).abs();
                    assert!(delta <= 0.0002,
                        "B2 seam normal mismatch axis={axis} a={a:?} b={b:?} component={component} error={delta}");
                }
            }
        }
        println!("B2 GPU same-LOD seam PASS axis={axis} replicated_cells={}", vd * vd);
    }
}
