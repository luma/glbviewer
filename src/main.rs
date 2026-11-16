//! A minimal glTF/GLB viewer: `glbviewer <path/to/model.glb>`.

use std::time::Duration;

mod camera;
mod cli;
mod controls;
mod lighting;
mod model;
mod render;
mod texture_ext;

use bevy::{
    app::ScheduleRunnerPlugin, asset::io::AssetSourceId, prelude::*, window::ExitCondition,
    winit::WinitPlugin,
};

fn main() -> AppExit {
    let options = match cli::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            return AppExit::from_code(err.exit_code());
        }
    };

    let save_render = options.output.is_some();
    let model_dir = options.model.dir.clone();
    let model_file = options.model.file.clone();
    let mut app = App::new();
    app.insert_resource(options)
        // Must come before `AssetPlugin` (in `DefaultPlugins`) to replace its
        // default file reader.
        .register_asset_source(
            AssetSourceId::Default,
            texture_ext::asset_source(model_dir.clone()),
        );

    let default_plugins = DefaultPlugins.set(AssetPlugin {
        file_path: model_dir,
        ..default()
    });
    if save_render {
        app.add_plugins(
            default_plugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
        app.add_plugins(render::CapturePlugin);
    } else {
        app.add_plugins(default_plugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("glbviewer - {model_file}"),
                ..default()
            }),
            ..default()
        }));
    }

    app.add_plugins((
        model::ModelPlugin { file: model_file },
        camera::CameraPlugin,
        lighting::LightingPlugin,
        controls::ControlsPlugin,
    ));
    app.run()
}
