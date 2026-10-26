# glbviewer

A minimal desktop viewer for glTF 2.0 models (`.glb` and `.gltf`), built with
[Bevy](https://bevyengine.org/) 0.20.

The model is centred and framed automatically, lit by a single shadow-casting
"sun", and can be rotated with the mouse.

**Note:** This was hacked together for my own personal use and I've only tested it on my local Linux machine. That being said, if you have feature requests or hit issues open an issue :-)

## Usage

```sh
glbviewer path/to/model.glb
glbviewer path/to/model.gltf
```

The app takes exactly one argument: the path to the model file. A `.gltf` file
can reference external `.bin` buffers and textures; these are resolved relative
to the model's own directory.

### Controls

| Input              | Action                                  |
| ------------------ | --------------------------------------- |
| Left-drag left/right | Spin the model around the vertical axis |
| Left-drag up/down    | Tilt the model towards/away from you    |

Only the first scene in the file (`Scene0`) is shown.

## Building

You need a recent stable Rust toolchain (edition 2024, Rust 1.85+), installed
via [rustup](https://rustup.rs/).

On Linux, Bevy also needs a few system libraries. On Debian/Ubuntu:

```sh
sudo apt install g++ pkg-config libx11-dev libasound2-dev libudev-dev \
    libxkbcommon-x11-0 libwayland-dev libxkbcommon-dev
```

See Bevy's [Linux dependencies guide](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)
for other distributions.

Then:

```sh
# Run directly from the source tree
cargo run --release -- path/to/model.glb

# Or build the binary at target/release/glbviewer
cargo build --release
```

Use `--release`: debug builds of Bevy are noticeably slow to render. Release
builds use thin LTO and a single codegen unit, so a clean release build takes
several minutes.

## Installing

```sh
cargo install --path .
```

This builds in release mode and copies the binary to `~/.cargo/bin`, which
rustup adds to your `PATH`. Afterwards you can run `glbviewer` from any
directory. Re-run the same command to install an updated version, and use
`cargo uninstall glbviewer` to remove it.
