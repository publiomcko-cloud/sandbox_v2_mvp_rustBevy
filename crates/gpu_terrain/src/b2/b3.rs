//! B3 streaming integration for the B2 GPU-resident extraction path.
//!
//! Present whole 16m logical blocks; retain old meshes while adding new ones.
//! No GPU->CPU mesh readback and no mixed-LOD cracks (uniform LOD2 baseline).
//! Presentation is delayed until the GPU compute encoder has dispatched all
//! three passes. The CPU does not synchronously wait for GPU completion.

use super::{
    B2HeadlessTarget, B2Pipelines, B2ResidentBuffers, BuildSdfMesh, PendingB2, b2_counts,
    compute_b2, init_b2_pipelines, prepare_b2,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::{RenderTarget, primitives::Aabb},
    core_pipeline::schedule::camera_driver,
    mesh::Indices,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup,
        extract_component::ExtractComponentPlugin,
        mesh::allocator::MeshAllocatorSettings,
        render_resource::{BufferUsages, PrimitiveTopology, TextureFormat},
        renderer::RenderGraph,
    },
};
use sandbox_streaming::scheduler::{
    BLOCKS_PER_FRAME, STREAM_LOD, StreamPlan, plan, within_retention,
};
use sandbox_world_core::{BlockCoord, density::DensityGrid};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

const SEED: u32 = 42;
const MAX_RESIDENT_BLOCKS: usize = 1100;
const OPTIMIZED_BLOCKS_PER_FRAME: usize = 5;
const PLAN_UPDATE_DISTANCE_M: f32 = 1.5;
const PLAN_UPDATE_MAX_AGE_FRAMES: u64 = 12;
const DIRECTION_COSINE_THRESHOLD: f32 = 0.96;

/// MainWorld ↔ RenderWorld: renderer inserts IDs after all compute passes
/// have been encoded. MainWorld consumes readiness on a subsequent frame.
#[derive(Resource, Clone, Default)]
pub(super) struct B3ReadySet(pub Arc<Mutex<HashSet<AssetId<Mesh>>>>);

pub struct GpuTerrainB3Plugin;

impl Plugin for GpuTerrainB3Plugin {
    fn build(&self, app: &mut App) {
        let ready = B3ReadySet::default();
        app.insert_resource(ready.clone())
            .init_resource::<StreamWorld>()
            .add_plugins(ExtractComponentPlugin::<BuildSdfMesh>::default())
            .add_systems(Startup, setup_b3_scene)
            .add_systems(PostUpdate, stream_b3);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.world_mut().insert_resource(ready);
        render_app
            .init_resource::<PendingB2>()
            .init_resource::<B2ResidentBuffers>()
            .add_systems(RenderStartup, init_b2_pipelines)
            .add_systems(Render, prepare_b2)
            .add_systems(RenderGraph, compute_b2.before(camera_driver));
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .world_mut()
            .resource_mut::<MeshAllocatorSettings>()
            .extra_buffer_usages = BufferUsages::STORAGE;
    }
}

struct ActiveBlock {
    mesh: Handle<Mesh>,
    entity: Entity,
    visible: bool,
}

#[derive(Resource, Default)]
struct StreamWorld {
    blocks: HashMap<BlockCoord, ActiveBlock>,
    material: Option<Handle<StandardMaterial>>,
    previous_position: Option<Vec3>,
    frame: u64,
    dispatches: u64,
    evictions: u64,
    cached_plan: Option<StreamPlan>,
    plan_position: Vec3,
    plan_direction: Vec2,
    plan_frame: u64,
    plan_refreshes: u64,
    baseline_scheduler: bool,
    cpu_samples_us: Vec<u32>,
}

fn setup_b3_scene(
    mut commands: Commands,
    mut state: ResMut<StreamWorld>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    state.baseline_scheduler = std::env::var("SANDBOX_B3_SCHEDULER").as_deref() == Ok("baseline");
    state.cpu_samples_us = Vec::with_capacity(60);
    state.material = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.34, 0.59, 0.37),
        perceptual_roughness: 0.9,
        cull_mode: None,
        ..default()
    }));

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.6, 0.0)),
    ));

    let cave = std::env::var("SANDBOX_B2_SCENE").as_deref() == Ok("cave");
    let camera_transform = if cave {
        Transform::from_xyz(-8.0, -7.0, 2.0).looking_at(Vec3::new(6.0, -7.0, -3.0), Vec3::Y)
    } else {
        Transform::from_xyz(-17.0, 15.0, 14.0).looking_at(Vec3::new(4.0, 2.0, -7.0), Vec3::Y)
    };
    if std::env::var("SANDBOX_B2_HEADLESS").as_deref() == Ok("1")
        || std::env::var("SANDBOX_B3_HEADLESS").as_deref() == Ok("1")
    {
        let (width, height) = if std::env::var("SANDBOX_B3_RENDER_1080P").as_deref() == Ok("1") {
            (1920, 1080)
        } else {
            (1280, 720)
        };
        let mut image =
            Image::new_target_texture(width, height, TextureFormat::Rgba8UnormSrgb, None);
        image.asset_usage = RenderAssetUsages::RENDER_WORLD;
        let target = images.add(image);
        commands.insert_resource(B2HeadlessTarget(target.clone()));
        commands.spawn((
            Camera3d::default(),
            RenderTarget::Image(target.into()),
            camera_transform,
        ));
    } else {
        commands.spawn((Camera3d::default(), camera_transform));
    }
    info!(
        "B3 streaming stage initialized: 130m horizon, 36m directional prefetch, uniform LOD2, 18m/s route optional"
    );
}

fn stream_b3(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<Camera3d>>,
    mut state: ResMut<StreamWorld>,
    ready: Res<B3ReadySet>,
) {
    let cpu_start = Instant::now();
    let Some(transform) = camera.iter().next() else {
        return;
    };
    let position = transform.translation;
    let pos = [position.x, position.y, position.z];
    let direction = if let Some(previous) = state.previous_position {
        let motion = position - previous;
        if motion.x.hypot(motion.z) >= 0.001 {
            [motion.x, motion.z]
        } else {
            let forward = transform.forward();
            [forward.x, forward.z]
        }
    } else {
        let forward = transform.forward();
        [forward.x, forward.z]
    };
    state.previous_position = Some(position);
    state.frame += 1;

    // A 130m ring does not need 360 degree replanning on every 16ms
    // frame. Refresh after 1.5m / 12 frames / an appreciable turn.
    // Required coverage is still audited from the CURRENT camera
    // position once per second, rather than trusting the cached plan.
    let direction2 = Vec2::new(direction[0], direction[1]).normalize_or_zero();
    let moved = (position.xz() - state.plan_position.xz()).length();
    let heading_changed = state.plan_direction.dot(direction2) < DIRECTION_COSINE_THRESHOLD;
    let should_refresh = state.baseline_scheduler
        || state.cached_plan.is_none()
        || moved >= PLAN_UPDATE_DISTANCE_M
        || state.frame.saturating_sub(state.plan_frame) >= PLAN_UPDATE_MAX_AGE_FRAMES
        || heading_changed;
    if should_refresh {
        state.cached_plan = Some(plan(pos, direction));
        state.plan_position = position;
        state.plan_direction = direction2;
        state.plan_frame = state.frame;
        state.plan_refreshes += 1;
    }

    // New meshes were submitted to GPU by a previous render-graph pass.
    // Bevy's draw only sees them on a later frame, after their compute dispatch.
    if let Ok(mut dispatched) = ready.0.lock() {
        for block in state.blocks.values_mut() {
            if !block.visible && dispatched.remove(&block.mesh.id()) {
                commands.entity(block.entity).insert(Visibility::Visible);
                block.visible = true;
            }
        }
    }

    // Eviction is amortized on replanning frames, rather than scanning
    // ~800 resident mesh handles 60 times per second. The retention ring
    // is still 52 m larger than the mandatory 130 m horizon.
    if should_refresh {
        let stale: Vec<_> = state
            .blocks
            .keys()
            .copied()
            .filter(|c| !within_retention(pos, *c))
            .collect();
        for key in stale {
            if let Some(old) = state.blocks.remove(&key) {
                commands.entity(old.entity).despawn();
                meshes.remove(old.mesh.id());
                if let Ok(mut set) = ready.0.lock() {
                    set.remove(&old.mesh.id());
                }
                state.evictions += 1;
            }
        }
    }

    let material = state
        .material
        .as_ref()
        .expect("B3 material not initialized")
        .clone();
    let limit = if state.baseline_scheduler {
        BLOCKS_PER_FRAME
    } else {
        OPTIMIZED_BLOCKS_PER_FRAME
    };
    let mut new_count = 0_usize;
    let target_count = state.cached_plan.as_ref().expect("B3 plan").targets.len();
    for index in 0..target_count {
        if new_count >= limit || state.blocks.len() >= MAX_RESIDENT_BLOCKS {
            break;
        }
        let coord = state.cached_plan.as_ref().expect("B3 plan").targets[index].coord;
        if state.blocks.contains_key(&coord) {
            continue;
        }
        let Ok(grid) = DensityGrid::new(coord, STREAM_LOD) else {
            error!(?coord, "B3 block outside GPU SDF numeric range");
            continue;
        };
        let (vertex_count, index_count) = b2_counts(grid);
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; vertex_count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32; 3]; vertex_count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32; 2]; vertex_count])
        .with_inserted_indices(Indices::U32(vec![0_u32; index_count]));
        let handle = meshes.add(mesh);
        let entity = commands
            .spawn((
                BuildSdfMesh {
                    handle: handle.clone(),
                    grid,
                    seed: SEED,
                    seam_mask: 0,
                    neighbor_lod_words: [0; 2],
                },
                Mesh3d(handle.clone()),
                MeshMaterial3d(material.clone()),
                Visibility::Hidden,
                // The GPU may emit vertices one LOD2 cell (2m) beyond the
                // low block face. This manual bound replaces the zero-filled
                // placeholder mesh AABB and allows correct frustum culling.
                Aabb::from_min_max(Vec3::splat(-2.1), Vec3::splat(16.1)),
                Transform::from_xyz(
                    coord.x as f32 * 16.0,
                    coord.y as f32 * 16.0,
                    coord.z as f32 * 16.0,
                ),
            ))
            .id();
        state.blocks.insert(
            coord,
            ActiveBlock {
                mesh: handle,
                entity,
                visible: false,
            },
        );
        new_count += 1;
        state.dispatches += 1;
    }

    let elapsed_us = cpu_start.elapsed().as_micros().min(u32::MAX as u128) as u32;
    state.cpu_samples_us.push(elapsed_us);
    if state.frame % 60 == 0 {
        // This is deliberately a fresh plan. All reported required/missing
        // counts match the actual current position, not a cached origin.
        let audit = plan(pos, direction);
        let required = audit.required.len();
        let visible = audit
            .required
            .iter()
            .filter(|c| state.blocks.get(c).is_some_and(|b| b.visible))
            .count();
        let cpu_max_us = state.cpu_samples_us.iter().copied().max().unwrap_or(0);
        let cpu_avg_us = state.cpu_samples_us.iter().map(|x| *x as u64).sum::<u64>()
            / state.cpu_samples_us.len().max(1) as u64;
        state.cpu_samples_us.clear();
        info!(
            required,
            visible,
            missing = required - visible,
            resident = state.blocks.len(),
            scheduled = new_count,
            evictions = state.evictions,
            total_allocations = state.dispatches,
            x = position.x,
            y = position.y,
            z = position.z,
            cpu_max_us,
            cpu_avg_us,
            replans = state.plan_refreshes,
            baseline_scheduler = state.baseline_scheduler,
            "B3 horizon coverage (130m, two vertical layers, GPU mesh ready)"
        );
        if required > visible {
            warn!(
                missing = required - visible,
                "B3 incomplete horizon; gate remains HOLD"
            );
        }
    }
}
