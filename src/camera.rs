//! The viewing camera, moved back to fit the model once it is measured, and
//! its zoom.

use std::f32::consts::FRAC_PI_4;

use bevy::prelude::*;

use crate::model::ModelBounds;

/// Extra space around the model's bounding sphere when framing it.
const FRAME_MARGIN: f32 = 1.1;

/// Vertical field of view (radians) when not zoomed.
const FOV: f32 = FRAC_PI_4;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.15, 0.15, 0.18)))
            .init_resource::<Zoom>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    frame_model.run_if(resource_added::<ModelBounds>),
                    apply_zoom.run_if(resource_changed::<Zoom>),
                ),
            );
    }
}

/// How much the view is magnified: at 1 the model just fits the window, at 2
/// it appears twice as large, and so on.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Zoom(f32);

impl Zoom {
    const MIN: f32 = 0.1;
    const MAX: f32 = 20.0;

    /// Multiplies the magnification by `factor`, within the allowed range.
    pub fn zoom_by(&mut self, factor: f32) {
        self.0 = (self.0 * factor).clamp(Self::MIN, Self::MAX);
    }

    /// Returns to the view in which the model just fits the window.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl Default for Zoom {
    fn default() -> Self {
        Self(1.0)
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: FOV,
            ..default()
        }),
        Transform::from_xyz(0.0, 0.0, 5.0),
    ));
}

/// Moves the camera along +Z, looking at the pivot, until the model fits the
/// unzoomed view.
pub fn frame_model(
    bounds: Res<ModelBounds>,
    mut transform: Single<&mut Transform, With<Camera3d>>,
) {
    let distance = fit_distance(bounds.radius, FOV);
    **transform = Transform::from_xyz(0.0, 0.0, distance).looking_at(Vec3::ZERO, Vec3::Y);
}

/// Zooms by narrowing or widening the field of view rather than moving the
/// camera, so that zooming in can never put the camera inside the model.
pub fn apply_zoom(zoom: Res<Zoom>, mut projection: Single<&mut Projection, With<Camera3d>>) {
    if let Projection::Perspective(perspective) = &mut **projection {
        perspective.fov = zoomed_fov(FOV, zoom.0);
    }
}

/// Distance at which a sphere of `radius` fills a perspective view with
/// vertical field of view `fov` (radians), plus [`FRAME_MARGIN`].
fn fit_distance(radius: f32, fov: f32) -> f32 {
    radius / (fov / 2.0).sin() * FRAME_MARGIN
}

/// Field of view (radians) in which everything appears `magnification` times
/// the size it does with a field of view of `fov`.
fn zoomed_fov(fov: f32, magnification: f32) -> f32 {
    // On-screen size is inversely proportional to tan(fov / 2).
    2.0 * ((fov / 2.0).tan() / magnification).atan()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_fits_view() {
        // With a 90° FOV, a unit sphere just touches the frustum at sqrt(2).
        let distance = fit_distance(1.0, std::f32::consts::FRAC_PI_2);
        assert!((distance - 2f32.sqrt() * FRAME_MARGIN).abs() < 1e-5);
    }

    #[test]
    fn scales_with_radius() {
        let fov = 0.8;
        assert!((fit_distance(10.0, fov) - 10.0 * fit_distance(1.0, fov)).abs() < 1e-4);
    }

    #[test]
    fn zoom_multiplies_and_resets() {
        let mut zoom = Zoom::default();
        zoom.zoom_by(2.0);
        zoom.zoom_by(1.5);
        assert_eq!(zoom, Zoom(3.0));
        zoom.reset();
        assert_eq!(zoom, Zoom(1.0));
    }

    #[test]
    fn zoom_is_limited() {
        let mut zoom = Zoom::default();
        zoom.zoom_by(1e6);
        assert_eq!(zoom, Zoom(Zoom::MAX));
        zoom.zoom_by(1e-9);
        assert_eq!(zoom, Zoom(Zoom::MIN));
    }

    #[test]
    fn unzoomed_fov_is_unchanged() {
        assert!((zoomed_fov(FOV, 1.0) - FOV).abs() < 1e-6);
    }

    #[test]
    fn zoomed_fov_magnifies_the_view() {
        // A 90° view shows a height of 2 per unit of distance; magnified
        // four times it shows a height of 0.5.
        let fov = zoomed_fov(std::f32::consts::FRAC_PI_2, 4.0);
        assert!((2.0 * (fov / 2.0).tan() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn zoomed_fov_stays_valid_at_the_limits() {
        for magnification in [Zoom::MIN, Zoom::MAX] {
            let fov = zoomed_fov(FOV, magnification);
            assert!(fov > 0.0 && fov < std::f32::consts::PI);
        }
    }
}
