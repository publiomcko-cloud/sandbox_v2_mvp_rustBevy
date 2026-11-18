use bevy::prelude::*;

const HORIZONTAL_SPEED_MPS: f32 = 18.0;

/// B0 free-flight camera. This does not assert terrain or collision functionality.
pub fn fly_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    let mut velocity = Vec3::ZERO;

    if keys.pressed(KeyCode::KeyW) {
        velocity.z -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        velocity.z += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        velocity.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        velocity.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyE) {
        velocity.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyQ) {
        velocity.y -= 1.0;
    }

    for mut transform in &mut cameras {
        transform.translation +=
            velocity.normalize_or_zero() * HORIZONTAL_SPEED_MPS * time.delta_secs();
    }
}
