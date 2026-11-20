//! Optional Bevy-native render screenshot for automated B2 visual CI.
//! Saves either the headless GPU render target or the onscreen window.
//! In both cases screenshot readback happens only in diagnostic capture mode.
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use std::path::PathBuf;
use sandbox_gpu_terrain::b2::B2HeadlessTarget;

#[derive(Resource)]
struct VisualCaptureState {
    path: PathBuf,
    issued: bool,
    requested_at: f32,
}

pub fn configure(app: &mut App) {
    let Ok(path) = std::env::var("SANDBOX_B2_CAPTURE_PATH") else {
        return;
    };
    app.insert_resource(VisualCaptureState {
        path: PathBuf::from(path),
        issued: false,
        requested_at: 0.0,
    })
    .add_systems(Update, capture_frame);
}

fn capture_frame(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<VisualCaptureState>,
    mut exit: MessageWriter<AppExit>,
    headless_target: Option<Res<B2HeadlessTarget>>,
) {
    let elapsed = time.elapsed_secs();
    if !state.issued && elapsed >= 7.0 {
        state.issued = true;
        state.requested_at = elapsed;
        info!(
            "B2 visual CI: requesting screenshot at {}",
            state.path.display()
        );
        let capture = match headless_target {
            Some(target) => Screenshot::image(target.0.clone()),
            None => Screenshot::primary_window(),
        };
        commands
            .spawn(capture)
            .observe(save_to_disk(state.path.clone()));
    }

    if state.issued
        && elapsed - state.requested_at > 0.3
        && std::fs::metadata(&state.path).is_ok_and(|metadata| metadata.len() > 2000)
    {
        info!(
            "B2 visual CI: screenshot captured at {}",
            state.path.display()
        );
        exit.write(AppExit::Success);
    }

    if elapsed > 40.0 {
        error!("B2 visual CI: screenshot did not complete in 40 seconds");
        std::process::exit(3);
    }
}
