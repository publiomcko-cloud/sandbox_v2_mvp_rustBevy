//! B2 default: GPU-resident SDF terrain; opt in to B0 via SANDBOX_STAGE=B0.
mod player;
mod visual_capture;

use avian3d::prelude::PhysicsPlugins;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use sandbox_gpu_terrain::{GpuTerrainBootstrapPlugin, b2::GpuTerrainB2Plugin};

fn main() {
    let b0 = std::env::var("SANDBOX_STAGE").is_ok_and(|value| value.eq_ignore_ascii_case("B0"));
    let title = if b0 {
        "Sandbox V2 - Rust/Bevy B0 GPU bootstrap"
    } else {
        "Sandbox V2 - B2 GPU Surface Nets SDF terrain"
    };

    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: title.into(),
                resolution: (1920, 1080).into(),
                ..default()
            }),
            ..default()
        }),
        PhysicsPlugins::default(),
        FrameTimeDiagnosticsPlugin::default(),
        LogDiagnosticsPlugin::default(),
    ));
    if b0 {
        app.add_plugins(GpuTerrainBootstrapPlugin);
    } else {
        app.add_plugins(GpuTerrainB2Plugin);
    }
    visual_capture::configure(&mut app);
    app.add_systems(Update, player::fly_camera).run();
}
