//! The viewing camera, moved back to fit the model once it is measured.

use bevy::prelude::*;

use crate::model::ModelBounds;

/// Extra space around the model's bounding sphere when framing it.
const FRAME_MARGIN: f32 = 1.1;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.15, 0.15, 0.18)))
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, frame_model.run_if(resource_added::<ModelBounds>));
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.0, 5.0)));
}

/// Moves the camera along +Z, looking at the pivot, until the model fits.
pub fn frame_model(
    bounds: Res<ModelBounds>,
    camera: Single<(&mut Transform, &Projection), With<Camera3d>>,
) {
    let (mut transform, projection) = camera.into_inner();
    let fov = match projection {
        Projection::Perspective(p) => p.fov,
        _ => PerspectiveProjection::default().fov,
    };
    let distance = fit_distance(bounds.radius, fov);
    *transform = Transform::from_xyz(0.0, 0.0, distance).looking_at(Vec3::ZERO, Vec3::Y);
}

/// Distance at which a sphere of `radius` fills a perspective view with
/// vertical field of view `fov` (radians), plus [`FRAME_MARGIN`].
fn fit_distance(radius: f32, fov: f32) -> f32 {
    radius / (fov / 2.0).sin() * FRAME_MARGIN
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
}
