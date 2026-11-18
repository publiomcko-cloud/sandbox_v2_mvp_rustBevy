//! B0 bootstrap: 3D window, editable camera controls, physics plugin, GPU mesh.
mod player;

use avian3d::prelude::PhysicsPlugins;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use sandbox_gpu_terrain::GpuTerrainBootstrapPlugin;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Sandbox V2 - Rust/Bevy B0 GPU bootstrap".into(),
                    resolution: (1920, 1080).into(),
                    ..default()
                }),
                ..default()
            }),
            PhysicsPlugins::default(),
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin::default(),
            GpuTerrainBootstrapPlugin,
        ))
        .add_systems(Update, player::fly_camera)
        .run();
}
