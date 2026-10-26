//! Loading the glTF file, spawning its scene and measuring its bounds.

use bevy::{asset::LoadState, camera::primitives::Aabb, math::Affine3A, prelude::*};

/// Loads the glTF file `file` (relative to the asset root) and spawns its
/// default scene under a [`Pivot`].
pub struct ModelPlugin {
    pub file: String,
}

impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ModelFile(self.file.clone()))
            .add_systems(Startup, load_model)
            .add_systems(
                Update,
                (
                    spawn_scene.run_if(resource_exists::<PendingModel>),
                    measure_model.run_if(not(resource_exists::<ModelBounds>)),
                )
                    .chain(),
            );
    }
}

/// Entity the model hangs off; rotating it spins the model around its centre.
#[derive(Component)]
pub struct Pivot;

/// The spawned glTF scene, offset so its bounding-box centre sits on the pivot.
#[derive(Component)]
pub struct Model;

/// Bounding sphere of the model in pivot space, inserted once the model has
/// been measured and centred. Other plugins react to it being added.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ModelBounds {
    pub radius: f32,
}

#[derive(Resource)]
struct ModelFile(String);

/// The glTF file while it is loading.
#[derive(Resource)]
struct PendingModel(Handle<Gltf>);

fn load_model(mut commands: Commands, asset_server: Res<AssetServer>, file: Res<ModelFile>) {
    commands.spawn((Pivot, Transform::default(), Visibility::default()));
    commands.insert_resource(PendingModel(asset_server.load(file.0.clone())));
}

/// Waits for the glTF to load, then spawns its default scene (or first scene
/// if none is marked as default). Exits the app if the file cannot be used.
fn spawn_scene(
    mut commands: Commands,
    pending: Res<PendingModel>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    file: Res<ModelFile>,
    pivot: Single<Entity, With<Pivot>>,
    mut exit: MessageWriter<AppExit>,
) {
    let scene = match asset_server.load_state(&pending.0) {
        LoadState::NotLoaded | LoadState::Loading => return,
        LoadState::Failed(err) => Err(err.to_string()),
        LoadState::Loaded => gltfs
            .get(&pending.0)
            .and_then(|gltf| {
                gltf.default_scene
                    .clone()
                    .or_else(|| gltf.scenes.first().cloned())
            })
            .ok_or_else(|| "file contains no scenes".to_owned()),
    };
    commands.remove_resource::<PendingModel>();

    match scene {
        Ok(scene) => {
            commands
                .entity(*pivot)
                .with_child((Model, WorldAssetRoot(scene)));
        }
        Err(err) => {
            error!("failed to load {}: {err}", file.0);
            exit.write(AppExit::error());
        }
    }
}

/// Once the model's meshes have bounding boxes, centre the model on the pivot
/// and publish its size as [`ModelBounds`].
fn measure_model(
    mut commands: Commands,
    model: Single<(Entity, &mut Transform), With<Model>>,
    pivot: Single<&GlobalTransform, With<Pivot>>,
    children: Query<&Children>,
    meshes: Query<(&Aabb, &GlobalTransform)>,
) {
    let (model, mut model_transform) = model.into_inner();

    // Measure in pivot space so that rotating the model before it finished
    // loading doesn't skew the centring.
    let to_pivot = pivot.affine().inverse();
    let boxes = children
        .iter_descendants(model)
        .filter_map(|entity| meshes.get(entity).ok())
        .map(|(aabb, transform)| (*aabb, to_pivot * transform.affine()));

    let Some((center, radius)) = bounding_sphere(boxes) else {
        // Not spawned yet, or bounds not computed yet; try again next frame.
        return;
    };

    // The model is a direct child of the pivot, so shifting it by the centre
    // (in pivot space) puts that centre on the pivot.
    model_transform.translation -= center;
    commands.insert_resource(ModelBounds { radius });
}

/// Centre and radius of a sphere enclosing all the given local-space boxes
/// after transforming each one, or `None` if there are no boxes.
fn bounding_sphere(boxes: impl IntoIterator<Item = (Aabb, Affine3A)>) -> Option<(Vec3, f32)> {
    let mut min = Vec3::INFINITY;
    let mut max = Vec3::NEG_INFINITY;
    for (aabb, transform) in boxes {
        let (center, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        for i in 0..8 {
            let sign = Vec3::new(
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                if i & 4 == 0 { -1.0 } else { 1.0 },
            );
            let corner = transform.transform_point3(center + half * sign);
            min = min.min(corner);
            max = max.max(corner);
        }
    }
    // Still infinite only if there were no boxes.
    min.is_finite()
        .then(|| ((min + max) / 2.0, ((max - min).length() / 2.0).max(1e-3)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box() -> Aabb {
        Aabb::from_min_max(Vec3::splat(-1.0), Vec3::splat(1.0))
    }

    #[test]
    fn no_boxes_has_no_bounds() {
        assert_eq!(bounding_sphere([]), None);
    }

    #[test]
    fn single_box_at_origin() {
        let (center, radius) = bounding_sphere([(unit_box(), Affine3A::IDENTITY)]).unwrap();
        assert!(center.abs_diff_eq(Vec3::ZERO, 1e-6));
        assert!((radius - 3f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn applies_transforms_and_merges_boxes() {
        let left = Affine3A::from_translation(Vec3::new(-4.0, 0.0, 0.0));
        let right = Affine3A::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::IDENTITY,
            Vec3::new(10.0, 0.0, 0.0),
        );
        let (center, _) = bounding_sphere([(unit_box(), left), (unit_box(), right)]).unwrap();
        // x spans -5..12, y and z span -2..2.
        assert!(center.abs_diff_eq(Vec3::new(3.5, 0.0, 0.0), 1e-6));
    }
}
