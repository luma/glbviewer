//! Command-line argument handling.

use bevy::prelude::Resource;
use std::{ffi::OsString, fmt, io, path::PathBuf};

pub const USAGE: &str =
    "usage: glbviewer [--output <file.png>] [--size <WIDTHxHEIGHT>] <path/to/model.glb|.gltf>";

/// The model to view, split into the parts Bevy's asset system needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPath {
    /// Absolute directory containing the model; used as the asset root so a
    /// `.gltf` file can resolve its relative buffer and texture URIs.
    pub dir: String,
    /// File name of the model within `dir`.
    pub file: String,
}

/// Pixel dimensions for a saved render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderSize {
    pub width: u32,
    pub height: u32,
}

impl Default for RenderSize {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}

/// Parsed command-line options, independent of the argument parsing library.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct CliOptions {
    pub model: ModelPath,
    pub output: Option<PathBuf>,
    pub size: RenderSize,
}

#[derive(Debug)]
pub enum CliError {
    /// Wrong or malformed command-line arguments.
    Usage,
    /// The path could not be resolved (missing file, permissions, ...).
    Unreadable(PathBuf, io::Error),
    /// The path resolved to something without a file name, such as `/`.
    NotAFile(PathBuf),
    /// Bevy's asset root is a `String`, so the path must be valid UTF-8.
    NotUtf8(PathBuf),
}

impl CliError {
    /// Process exit code for this error, following the `sysexits.h` convention
    /// of 2 for a usage error.
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Usage => 2,
            _ => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage => f.write_str(USAGE),
            CliError::Unreadable(path, err) => {
                write!(f, "error: cannot open {}: {err}", path.display())
            }
            CliError::NotAFile(path) => write!(f, "error: {} is not a file", path.display()),
            CliError::NotUtf8(path) => {
                write!(f, "error: path is not valid UTF-8: {}", path.display())
            }
        }
    }
}

impl std::error::Error for CliError {}

/// Parses arguments (excluding the program name) and validates the model path.
///
/// This is deliberately a small, isolated parser so it can be replaced with
/// Clap without changing the options consumed by the app.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<CliOptions, CliError> {
    let mut args = args.into_iter();
    let mut model = None;
    let mut output = None;
    let mut size = None;
    let mut options_ended = false;

    while let Some(arg) = args.next() {
        if !options_ended && arg == "--" {
            options_ended = true;
            continue;
        }

        if !options_ended && arg == "--output" {
            if output.is_some() {
                return Err(CliError::Usage);
            }
            let Some(value) = args.next() else {
                return Err(CliError::Usage);
            };
            let path = PathBuf::from(value);
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
            {
                return Err(CliError::Usage);
            }
            output = Some(path);
            continue;
        }

        if !options_ended && arg == "--size" {
            if size.is_some() {
                return Err(CliError::Usage);
            }
            let Some(value) = args.next().and_then(|value| value.into_string().ok()) else {
                return Err(CliError::Usage);
            };
            size = Some(parse_size(&value).ok_or(CliError::Usage)?);
            continue;
        }

        if !options_ended && arg.to_string_lossy().starts_with('-') {
            return Err(CliError::Usage);
        }
        if model.replace(arg).is_some() {
            return Err(CliError::Usage);
        }
    }

    let model = model.ok_or(CliError::Usage)?;
    if size.is_some() && output.is_none() {
        return Err(CliError::Usage);
    }

    let model = resolve_model(PathBuf::from(model))?;
    Ok(CliOptions {
        model,
        output,
        size: size.unwrap_or_default(),
    })
}

fn parse_size(value: &str) -> Option<RenderSize> {
    let (width, height) = value.split_once('x')?;
    let size = RenderSize {
        width: width.parse().ok()?,
        height: height.parse().ok()?,
    };
    (size.width > 0 && size.height > 0).then_some(size)
}

fn resolve_model(path: PathBuf) -> Result<ModelPath, CliError> {
    let path = path
        .canonicalize()
        .map_err(|err| CliError::Unreadable(path, err))?;
    if !path.is_file() {
        return Err(CliError::NotAFile(path));
    }

    let (Some(dir), Some(file)) = (path.parent(), path.file_name()) else {
        return Err(CliError::NotAFile(path));
    };
    let (Some(dir), Some(file)) = (dir.to_str(), file.to_str()) else {
        return Err(CliError::NotUtf8(path));
    };

    Ok(ModelPath {
        dir: dir.to_owned(),
        file: file.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn valid_model() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap()
            .join("Cargo.toml")
    }

    #[test]
    fn rejects_wrong_argument_count() {
        assert!(matches!(parse(args(&[])), Err(CliError::Usage)));
        assert!(matches!(
            parse([
                valid_model().into_os_string(),
                valid_model().into_os_string()
            ]),
            Err(CliError::Usage)
        ));
    }

    #[test]
    fn rejects_missing_file() {
        let err = parse(args(&["/definitely/not/here.glb"])).unwrap_err();
        assert!(matches!(err, CliError::Unreadable(..)));
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn rejects_directory() {
        let dir = std::env::temp_dir();
        let err = parse([dir.into_os_string()]).unwrap_err();
        assert!(matches!(err, CliError::NotAFile(_)));
    }

    #[test]
    fn parses_render_options_in_any_order() {
        let model_path = valid_model();
        let output_path = std::env::temp_dir().join("render.png");
        let options = parse([
            OsString::from("--output"),
            output_path.clone().into_os_string(),
            OsString::from("--size"),
            OsString::from("1600x900"),
            model_path.clone().into_os_string(),
        ])
        .unwrap();

        assert_eq!(options.output, Some(output_path));
        assert_eq!(
            options.size,
            RenderSize {
                width: 1600,
                height: 900
            }
        );
        assert_eq!(options.model.file, "Cargo.toml");
    }

    #[test]
    fn accepts_render_options_after_the_model_path() {
        let options = parse([
            valid_model().into_os_string(),
            OsString::from("--size"),
            OsString::from("640x480"),
            OsString::from("--output"),
            OsString::from("render.png"),
        ])
        .unwrap();

        assert_eq!(options.output, Some(PathBuf::from("render.png")));
        assert_eq!(
            options.size,
            RenderSize {
                width: 640,
                height: 480
            }
        );
    }

    #[test]
    fn defaults_to_full_hd_and_keeps_viewer_mode() {
        let options = parse([valid_model().into_os_string()]).unwrap();
        assert_eq!(options.output, None);
        assert_eq!(options.size, RenderSize::default());
    }

    #[test]
    fn rejects_invalid_or_duplicate_render_options() {
        let model = valid_model().into_os_string();
        assert!(matches!(
            parse([
                OsString::from("--output"),
                OsString::from("out.jpg"),
                model.clone()
            ]),
            Err(CliError::Usage)
        ));
        assert!(matches!(
            parse([
                OsString::from("--output"),
                OsString::from("out.png"),
                OsString::from("--size"),
                OsString::from("0x10"),
                model.clone()
            ]),
            Err(CliError::Usage)
        ));
        assert!(matches!(
            parse([OsString::from("--size"), OsString::from("10x10"), model]),
            Err(CliError::Usage)
        ));
    }

    #[test]
    fn rejects_duplicate_flags() {
        let model = valid_model().into_os_string();
        assert!(matches!(
            parse([
                OsString::from("--output"),
                OsString::from("one.png"),
                OsString::from("--output"),
                OsString::from("two.png"),
                model.clone(),
            ]),
            Err(CliError::Usage)
        ));
        assert!(matches!(
            parse([
                OsString::from("--output"),
                OsString::from("one.png"),
                OsString::from("--size"),
                OsString::from("640x480"),
                OsString::from("--size"),
                OsString::from("800x600"),
                model,
            ]),
            Err(CliError::Usage)
        ));
    }

    #[test]
    fn splits_existing_file_into_dir_and_name() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let model = resolve_model(root.join("Cargo.toml")).unwrap();
        assert_eq!(model.dir, root.to_str().unwrap());
        assert_eq!(model.file, "Cargo.toml");
    }
}
