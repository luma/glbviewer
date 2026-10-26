//! A single shadow-casting "sun" plus a little ambient fill.

use bevy::{
    light::{CascadeShadowConfigBuilder, light_consts::lux},
    prelude::*,
};

use crate::{camera, model::ModelBounds};

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        // Low ambient fill so faces in shadow aren't pure black.
        app.insert_resource(GlobalAmbientLight {
            brightness: 200.0,
            ..default()
        })
        .add_systems(Startup, spawn_sun)
        .add_systems(
            Update,
            fit_shadows
                .run_if(resource_added::<ModelBounds>)
                .after(camera::frame_model),
        );
    }
}

/// The sun: high and off to one side. It stays fixed while the model rotates
/// beneath it.
fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: lux::AMBIENT_DAYLIGHT,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(2.0, 4.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// The default shadow cascades are sized for large outdoor scenes; fit a
/// single cascade tightly around the model for crisp shadows at any scale.
fn fit_shadows(
    mut commands: Commands,
    bounds: Res<ModelBounds>,
    sun: Single<Entity, With<DirectionalLight>>,
    camera: Single<&Transform, With<Camera3d>>,
) {
    // Cascade distances are measured from the camera, which looks at the
    // model's centre.
    let distance = camera.translation.length();
    let reach = bounds.radius * 1.5;
    commands.entity(*sun).insert(
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: (distance - reach).max(1e-3),
            maximum_distance: distance + reach,
            first_cascade_far_bound: distance + reach,
            ..default()
        }
        .build(),
    );
}
