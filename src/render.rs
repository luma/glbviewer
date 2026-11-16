//! One-shot offscreen rendering and PNG output.

use std::{
    fs::{self, OpenOptions},
    path::Path,
};

use bevy::{
    app::AppExit,
    prelude::*,
    render::view::window::screenshot::{Screenshot, ScreenshotCaptured},
};

use crate::{camera::CaptureTarget, cli::CliOptions, model::ModelBounds};

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, request_capture);
    }
}

fn request_capture(
    mut commands: Commands,
    options: Res<CliOptions>,
    target: Option<Res<CaptureTarget>>,
    bounds: Option<Res<ModelBounds>>,
    mut rendered_frames: Local<u8>,
) {
    if options.output.is_none() || target.is_none() || bounds.is_none() {
        return;
    }

    if *rendered_frames == u8::MAX {
        return;
    }
    // Let scene assets and render pipelines settle before requesting the still.
    *rendered_frames += 1;
    if *rendered_frames < 3 {
        return;
    }
    *rendered_frames = u8::MAX;

    let path = options.output.clone().expect("checked above");
    commands
        .spawn(Screenshot::image(target.expect("checked above").0.clone()))
        .observe(
            move |event: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| match save_png(
                &event.image,
                &path,
            ) {
                Ok(()) => {
                    info!("Saved render to {}", path.display());
                    exit.write(AppExit::Success);
                }
                Err(err) => {
                    eprintln!("error: cannot save render to {}: {err}", path.display());
                    exit.write(AppExit::error());
                }
            },
        );
}

fn save_png(image: &Image, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dynamic = image.clone().try_into_dynamic()?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(err) = dynamic.write_to(&mut file, image::ImageFormat::Png) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(Box::new(err));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use bevy::render::render_resource::TextureFormat;

    use super::*;

    #[test]
    fn saves_transparent_png_and_refuses_to_overwrite() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let path = std::env::temp_dir().join(format!(
            "glbviewer-{}-{}.png",
            std::process::id(),
            now.as_nanos()
        ));
        let mut image = Image::new_target_texture(1, 1, TextureFormat::Rgba8UnormSrgb, None);
        image
            .data
            .as_mut()
            .unwrap()
            .copy_from_slice(&[10, 20, 30, 0]);

        save_png(&image, &path).unwrap();
        let saved = image::open(&path).unwrap().to_rgba8();
        assert_eq!(saved.get_pixel(0, 0).0, [10, 20, 30, 0]);
        assert!(save_png(&image, &path).is_err());

        fs::remove_file(path).unwrap();
    }
}
