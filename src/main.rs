//! A minimal glTF/GLB viewer: `glbviewer <path/to/model.glb>`.

mod camera;
mod cli;
mod controls;
mod lighting;
mod model;

use bevy::prelude::*;

fn main() -> AppExit {
    let model = match cli::parse(std::env::args_os().skip(1)) {
        Ok(model) => model,
        Err(err) => {
            eprintln!("{err}");
            return AppExit::from_code(err.exit_code());
        }
    };

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: model.dir,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("glbviewer - {}", model.file),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((
            model::ModelPlugin { file: model.file },
            camera::CameraPlugin,
            lighting::LightingPlugin,
            controls::ControlsPlugin,
        ))
        .run()
}
