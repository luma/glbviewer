//! Mouse controls.

use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};

use crate::model::Pivot;

/// Radians of rotation per pixel of mouse movement.
const ROTATE_SPEED: f32 = 0.01;

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, rotate_model);
    }
}

/// Left-drag rotates the model: horizontal motion spins it around the world
/// Y axis, vertical motion tilts it around the world X axis (the camera's
/// horizontal axis, as the camera looks down -Z).
fn rotate_model(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut pivot: Single<&mut Transform, With<Pivot>>,
) {
    if !buttons.pressed(MouseButton::Left) || motion.delta == Vec2::ZERO {
        return;
    }
    let delta = motion.delta * ROTATE_SPEED;
    pivot.rotation =
        Quat::from_rotation_y(delta.x) * Quat::from_rotation_x(delta.y) * pivot.rotation;
}
