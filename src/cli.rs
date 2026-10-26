//! Command-line argument handling.

use std::{ffi::OsString, fmt, io, path::PathBuf};

pub const USAGE: &str = "usage: glbviewer <path/to/model.glb|.gltf>";

/// The model to view, split into the parts Bevy's asset system needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPath {
    /// Absolute directory containing the model; used as the asset root so a
    /// `.gltf` file can resolve its relative buffer and texture URIs.
    pub dir: String,
    /// File name of the model within `dir`.
    pub file: String,
}

#[derive(Debug)]
pub enum CliError {
    /// Wrong number of arguments.
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

/// Parses the program arguments (excluding the program name).
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<ModelPath, CliError> {
    let mut args = args.into_iter();
    let (Some(path), None) = (args.next(), args.next()) else {
        return Err(CliError::Usage);
    };

    let path = PathBuf::from(path);
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

    #[test]
    fn rejects_wrong_argument_count() {
        assert!(matches!(parse(args(&[])), Err(CliError::Usage)));
        assert!(matches!(parse(args(&["a", "b"])), Err(CliError::Usage)));
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
    fn splits_existing_file_into_dir_and_name() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let model = parse([root.join("Cargo.toml").into_os_string()]).unwrap();
        assert_eq!(model.dir, root.to_str().unwrap());
        assert_eq!(model.file, "Cargo.toml");
    }
}
