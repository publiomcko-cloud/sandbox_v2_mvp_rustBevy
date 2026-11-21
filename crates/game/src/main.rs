//! B2: onscreen Bevy terrain, or headless GPU target for Windows service CI.
mod player;
mod visual_capture;

use avian3d::prelude::PhysicsPlugins;
use bevy::{
    app::ScheduleRunnerPlugin,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    render::RenderPlugin,
    window::ExitCondition,
    winit::WinitPlugin,
};
use sandbox_gpu_terrain::{GpuTerrainBootstrapPlugin, b2::GpuTerrainB2Plugin};
use std::time::Duration;

fn main() {
    let b0 = std::env::var("SANDBOX_STAGE").is_ok_and(|value| value.eq_ignore_ascii_case("B0"));
    let headless = !b0 && std::env::var("SANDBOX_B2_HEADLESS").as_deref() == Ok("1");
    let title = if b0 {
        "Sandbox V2 - Rust/Bevy B0 GPU bootstrap"
    } else {
        "Sandbox V2 - B2 GPU Surface Nets SDF terrain"
    };

    let mut app = App::new();
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: if headless {
                None
            } else {
                Some(Window {
                    title: title.into(),
                    resolution: (1920, 1080).into(),
                    ..default()
                })
            },
            exit_condition: if headless {
                ExitCondition::DontExit
            } else {
                ExitCondition::OnPrimaryClosed
            },
            ..default()
        })
        .set(RenderPlugin {
            synchronous_pipeline_compilation: headless,
            ..default()
        });
    if headless {
        // A Windows runner service executes in Session 0, where a Winit
        // window cannot reliably render frames. Render to a GPU image
        // instead, following Bevy's official headless_renderer example.
        app.add_plugins(plugins.disable::<WinitPlugin>());
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16)));
    } else {
        app.add_plugins(plugins);
    }
    app.add_plugins((
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
