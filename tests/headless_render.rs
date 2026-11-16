use std::{
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn cube_example_has_a_complete_embedded_mesh_buffer() {
    use base64::Engine;

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/cube.gltf");
    let gltf: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let buffer = gltf["buffers"][0]["uri"].as_str().unwrap();
    let encoded = buffer
        .strip_prefix("data:application/octet-stream;base64,")
        .unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();

    assert_eq!(
        decoded.len(),
        gltf["buffers"][0]["byteLength"].as_u64().unwrap() as usize
    );
    assert_eq!(
        gltf["meshes"][0]["primitives"][0]["attributes"]["POSITION"],
        0
    );
    assert_eq!(gltf["accessors"][2]["count"], 36);
}

/// Exercises Bevy's actual offscreen renderer; it needs an installed wgpu adapter.
#[test]
#[ignore = "requires a working GPU or software graphics adapter"]
fn renders_the_example_cube_to_a_transparent_png() {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let output = std::env::temp_dir().join(format!(
        "glbviewer-cube-{}-{}.png",
        std::process::id(),
        now.as_nanos()
    ));
    let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/cube.gltf");
    let result = Command::new(env!("CARGO_BIN_EXE_glbviewer"))
        .args(["--output", output.to_str().unwrap(), "--size", "320x240"])
        .arg(model)
        .output()
        .expect("run glbviewer");

    assert!(
        result.status.success(),
        "glbviewer exited with {}\nstdout:\n{}\nstderr:\n{}",
        result.status,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let image = image::open(&output).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (320, 240));
    let visible_pixels = image.pixels().filter(|pixel| pixel.0[3] > 0).count();
    let transparent_pixels = image.pixels().filter(|pixel| pixel.0[3] == 0).count();
    assert!(visible_pixels > 100, "render contains no visible cube");
    assert!(
        transparent_pixels > 100,
        "render background is not transparent"
    );

    std::fs::remove_file(output).unwrap();
}
