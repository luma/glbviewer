//! Mouse and keyboard controls.

use bevy::{
    input::mouse::{AccumulatedMouseMotion, MouseScrollUnit, MouseWheel},
    prelude::*,
};

use crate::{
    camera::{self, Zoom},
    model::Pivot,
};

/// Radians of rotation per pixel of mouse movement.
const ROTATE_SPEED: f32 = 0.01;

/// Magnification change per line scrolled (one notch of a typical wheel).
const ZOOM_PER_LINE: f32 = 1.1;

/// Scroll distance treated as one line when the device reports pixels, as
/// trackpads generally do.
const PIXELS_PER_LINE: f32 = 100.0;

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (rotate_model, zoom_view.before(camera::apply_zoom)));
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

/// Scrolling up zooms in and scrolling down zooms out; Backspace resets the
/// zoom.
fn zoom_view(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    mut zoom: ResMut<Zoom>,
) {
    // Converted event by event, as a mouse and a trackpad scrolled in the
    // same frame report in different units.
    let lines: f32 = wheel
        .read()
        .map(|event| scroll_lines(event.unit, event.y))
        .sum();

    if keys.just_pressed(KeyCode::Backspace) {
        zoom.reset();
    } else if lines != 0.0 {
        // Exponential, so that scrolling back the same amount undoes the zoom.
        zoom.zoom_by(ZOOM_PER_LINE.powf(lines));
    }
}

/// A scroll of `delta` in the given `unit`, in lines.
fn scroll_lines(unit: MouseScrollUnit, delta: f32) -> f32 {
    match unit {
        MouseScrollUnit::Line => delta,
        MouseScrollUnit::Pixel => delta / PIXELS_PER_LINE,
    }
}

#[cfg(test)]
mod tests {
    use bevy::input::{
        ButtonState, InputPlugin,
        keyboard::{Key, KeyboardInput},
        touch::TouchPhase,
    };

    use super::*;

    /// An app with just the camera and controls, updated once.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            camera::CameraPlugin,
            ControlsPlugin,
        ));
        app.update();
        app
    }

    /// Height of the camera's view per unit of distance from it; inversely
    /// proportional to how large things appear.
    fn view_height(app: &mut App) -> f32 {
        let mut camera = app
            .world_mut()
            .query_filtered::<&Projection, With<Camera3d>>();
        let Projection::Perspective(perspective) = camera.single(app.world()).unwrap() else {
            panic!("camera is not perspective");
        };
        2.0 * (perspective.fov / 2.0).tan()
    }

    fn scroll(app: &mut App, unit: MouseScrollUnit, y: f32) {
        app.world_mut().write_message(MouseWheel {
            unit,
            x: 0.0,
            y,
            window: Entity::PLACEHOLDER,
            phase: TouchPhase::Moved,
        });
    }

    #[test]
    fn wheel_zooms_and_backspace_resets() {
        let mut app = app();
        let unzoomed = view_height(&mut app);

        scroll(&mut app, MouseScrollUnit::Line, 1.0);
        app.update();
        let zoomed_in = view_height(&mut app);
        assert!((zoomed_in - unzoomed / ZOOM_PER_LINE).abs() < 1e-5);

        // The zoom holds while nothing is scrolled.
        app.update();
        assert_eq!(view_height(&mut app), zoomed_in);

        scroll(&mut app, MouseScrollUnit::Line, -3.0);
        app.update();
        assert!(view_height(&mut app) > unzoomed);

        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Backspace,
            logical_key: Key::Backspace,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        assert!((view_height(&mut app) - unzoomed).abs() < 1e-5);
    }

    #[test]
    fn scrolls_in_different_units_in_one_frame_all_count() {
        let mut app = app();
        let unzoomed = view_height(&mut app);

        scroll(&mut app, MouseScrollUnit::Line, 1.0);
        scroll(&mut app, MouseScrollUnit::Pixel, PIXELS_PER_LINE);
        app.update();
        let expected = unzoomed / ZOOM_PER_LINE.powi(2);
        assert!((view_height(&mut app) - expected).abs() < 1e-5);
    }

    #[test]
    fn pixels_are_converted_to_lines() {
        assert_eq!(scroll_lines(MouseScrollUnit::Line, -2.0), -2.0);
        assert_eq!(
            scroll_lines(MouseScrollUnit::Pixel, 2.5 * PIXELS_PER_LINE),
            2.5
        );
    }
}
