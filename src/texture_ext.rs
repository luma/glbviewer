//! Support for the `EXT_texture_webp` and `KHR_texture_basisu` glTF extensions.
//!
//! Bevy can decode WebP and KTX2 images, but its glTF loader doesn't understand
//! these two extensions: it only ever uses a texture's core `source` image, and
//! rejects files that leave it out or list the extensions as required.
//!
//! [`PatchingReader`] sits in front of the asset file reader and rewrites the
//! glTF JSON as it is read, so that each texture without a core `source` uses
//! its extension image instead. Textures that do have a core `source` (the
//! extensions' optional PNG/JPEG fallback) are left alone, so files Bevy could
//! already show look exactly as before.
//!
//! Bevy only transcodes UASTC Basis Universal images, not ETC1S ones, and one
//! undecodable texture fails the whole model. Textures whose only image is
//! ETC1S are therefore detached from their materials (with a warning), so the
//! rest of the model still shows.

use std::{
    borrow::Cow,
    collections::HashSet,
    path::{Path, PathBuf},
};

use base64::Engine;
use bevy::{
    asset::io::{
        AssetReader, AssetReaderError, AssetSourceBuilder, ErasedAssetReader, PathStream, Reader,
        VecReader, file::FileAssetReader,
    },
    prelude::*,
};
use serde_json::{Value, json};

const WEBP: &str = "EXT_texture_webp";
const BASISU: &str = "KHR_texture_basisu";
/// Extensions handled here, in order of preference when a texture has both
/// (WebP always decodes; a Basis image may be ETC1S).
const EXTENSIONS: [&str; 2] = [WEBP, BASISU];

/// A valid 1x1 PNG used as the image of textures that can't be shown. Nothing
/// samples it, as those textures are detached from their materials; it is only
/// there because glTF validation requires every texture to have an image.
const PLACEHOLDER_PNG: &str = "data:image/png;base64,\
    iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGP4DwQACfsD/fteaysAAAAASUVORK5CYII=";

const GLB_MAGIC: &[u8; 4] = b"glTF";
const GLB_JSON_CHUNK: u32 = 0x4E4F_534A;
const GLB_BIN_CHUNK: u32 = 0x004E_4942;

const KTX2_IDENTIFIER: &[u8; 12] = b"\xABKTX 20\xBB\r\n\x1A\n";
/// Bytes of a KTX2 file needed to read its supercompression scheme.
const KTX2_HEADER_LEN: usize = 48;
/// KTX2 `supercompressionScheme` for BasisLZ, which always holds ETC1S data.
const KTX2_BASIS_LZ: u32 = 1;

/// The default asset source, reading files under `dir` through a
/// [`PatchingReader`]. Must be registered before `AssetPlugin`.
pub fn asset_source(dir: String) -> AssetSourceBuilder {
    AssetSourceBuilder::new(move || Box::new(PatchingReader(Box::new(FileAssetReader::new(&dir)))))
}

/// Wraps another reader, rewriting `.gltf`/`.glb` files as described in the
/// module docs and passing everything else through untouched.
pub struct PatchingReader(pub Box<dyn ErasedAssetReader>);

impl AssetReader for PatchingReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        let mut reader = self.0.read(path).await?;
        if !is_gltf(path) {
            return Ok(reader);
        }
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        if let Some(patched) = self.patch(path, &bytes).await {
            bytes = patched;
        }
        Ok(Box::new(VecReader::new(bytes)))
    }

    async fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        self.0.read_meta(path).await
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        self.0.read_directory(path).await
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        self.0.is_directory(path).await
    }
}

impl PatchingReader {
    /// The rewritten file, or `None` if it needs no changes or can't be parsed
    /// (in which case Bevy's loader reports the problem).
    async fn patch(&self, path: &Path, bytes: &[u8]) -> Option<Vec<u8>> {
        let parts = split(bytes)?;
        let mut root: Value = serde_json::from_slice(parts.json).ok()?;
        if !needs_patch(&root) {
            return None;
        }

        let dir = path.parent().unwrap_or(Path::new(""));
        let bin = parts.glb_tail.and_then(bin_chunk);
        let targets = targets(&root);
        let mut unusable = HashSet::new();
        for target in targets.iter().filter(|target| target.extension == BASISU) {
            let header = self.image_header(&root, bin, dir, target.image).await;
            if header.as_deref().is_some_and(is_etc1s) {
                warn!(
                    "{}: texture {} only has an ETC1S Basis Universal image, which Bevy \
                     can't decode; showing the model without it",
                    path.display(),
                    target.texture,
                );
                unusable.insert(target.image);
            }
        }

        apply(&mut root, &targets, &unusable);
        Some(join(serde_json::to_vec(&root).ok()?, parts.glb_tail))
    }

    /// The first [`KTX2_HEADER_LEN`] bytes of image `image`, wherever it is
    /// stored, or `None` if they can't be found.
    async fn image_header(
        &self,
        root: &Value,
        bin: Option<&[u8]>,
        dir: &Path,
        image: usize,
    ) -> Option<Vec<u8>> {
        let image = root.get("images")?.get(image)?;
        let (data, offset, len): (Cow<[u8]>, usize, usize) =
            if let Some(uri) = image.get("uri").and_then(Value::as_str) {
                let data = self.uri_bytes(uri, dir).await?;
                let len = data.len();
                (data.into(), 0, len)
            } else {
                let view = root
                    .get("bufferViews")?
                    .get(usize::try_from(image.get("bufferView")?.as_u64()?).ok()?)?;
                let buffer = root
                    .get("buffers")?
                    .get(usize::try_from(view.get("buffer")?.as_u64()?).ok()?)?;
                let data = match buffer.get("uri").and_then(Value::as_str) {
                    Some(uri) => self.uri_bytes(uri, dir).await?.into(),
                    None => bin?.into(),
                };
                let offset = view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0);
                let len = view.get("byteLength")?.as_u64()?;
                (data, offset.try_into().ok()?, len.try_into().ok()?)
            };
        let end = offset.checked_add(len.min(KTX2_HEADER_LEN))?;
        data.get(offset..end).map(<[u8]>::to_vec)
    }

    /// The contents of a glTF URI: a data URI, or a file relative to `dir`.
    async fn uri_bytes(&self, uri: &str, dir: &Path) -> Option<Vec<u8>> {
        if let Some(data) = uri.strip_prefix("data:") {
            let (params, payload) = data.split_once(',')?;
            return if params.ends_with(";base64") {
                base64::engine::general_purpose::STANDARD
                    .decode(payload)
                    .ok()
            } else {
                Some(percent_encoding::percent_decode_str(payload).collect())
            };
        }
        let file = percent_encoding::percent_decode_str(uri)
            .decode_utf8()
            .ok()?;
        let path: PathBuf = dir.join(file.as_ref());
        let mut reader = self.0.read(&path).await.ok()?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.ok()?;
        Some(bytes)
    }
}

fn is_gltf(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gltf") || ext.eq_ignore_ascii_case("glb"))
}

/// A texture to point at the image from one of its extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    texture: usize,
    image: usize,
    extension: &'static str,
}

/// Whether the JSON uses either extension in a way Bevy can't handle: as a
/// required extension, or as a texture's only image.
fn needs_patch(root: &Value) -> bool {
    required(root).any(|ext| EXTENSIONS.contains(&ext)) || !targets(root).is_empty()
}

fn required(root: &Value) -> impl Iterator<Item = &str> {
    root.get("extensionsRequired")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// Textures with no core `source`, paired with their preferred extension image.
fn targets(root: &Value) -> Vec<Target> {
    let Some(textures) = root.get("textures").and_then(Value::as_array) else {
        return Vec::new();
    };
    textures
        .iter()
        .enumerate()
        .filter(|(_, texture)| texture.get("source").is_none())
        .filter_map(|(index, texture)| {
            EXTENSIONS.into_iter().find_map(|extension| {
                let image = texture
                    .get("extensions")?
                    .get(extension)?
                    .get("source")?
                    .as_u64()?;
                Some(Target {
                    texture: index,
                    image: image.try_into().ok()?,
                    extension,
                })
            })
        })
        .collect()
}

/// Points each target texture at its extension image, or at a placeholder if
/// that image is in `unusable` (detaching the texture from all materials), and
/// drops both extensions from `extensionsRequired`.
fn apply(root: &mut Value, targets: &[Target], unusable: &HashSet<usize>) {
    let mut placeholder = None;
    let mut detached = HashSet::new();
    for target in targets {
        let image = if unusable.contains(&target.image) {
            detached.insert(target.texture);
            *placeholder.get_or_insert_with(|| push_image(root, json!({ "uri": PLACEHOLDER_PNG })))
        } else {
            target.image
        };
        root["textures"][target.texture]["source"] = json!(image);
    }

    if !detached.is_empty()
        && let Some(materials) = root.get_mut("materials")
    {
        detach_textures(materials, &detached);
    }

    if let Some(object) = root.as_object_mut()
        && let Some(Value::Array(required)) = object.get_mut("extensionsRequired")
    {
        required.retain(|ext| !ext.as_str().is_some_and(|ext| EXTENSIONS.contains(&ext)));
        if required.is_empty() {
            object.remove("extensionsRequired");
        }
    }
}

/// Appends `image` to the `images` array and returns its index.
fn push_image(root: &mut Value, image: Value) -> usize {
    let images = root
        .as_object_mut()
        .expect("glTF root is an object")
        .entry("images")
        .or_insert_with(|| json!([]));
    let images = images.as_array_mut().expect("glTF images is an array");
    images.push(image);
    images.len() - 1
}

/// Removes every texture reference (a `...Texture` object with an `index`,
/// including those in material extensions) to one of `textures`.
fn detach_textures(value: &mut Value, textures: &HashSet<usize>) {
    match value {
        Value::Object(map) => {
            map.retain(|key, info| {
                let index = info.get("index").and_then(Value::as_u64);
                !(key.ends_with("Texture")
                    && index.is_some_and(|index| {
                        usize::try_from(index).is_ok_and(|index| textures.contains(&index))
                    }))
            });
            map.values_mut().for_each(|v| detach_textures(v, textures));
        }
        Value::Array(items) => items.iter_mut().for_each(|v| detach_textures(v, textures)),
        _ => {}
    }
}

/// Whether `header` starts a KTX2 file holding ETC1S (BasisLZ) data.
fn is_etc1s(header: &[u8]) -> bool {
    header.starts_with(KTX2_IDENTIFIER) && read_u32(header, 44) == Some(KTX2_BASIS_LZ)
}

/// A glTF file split into its JSON and, for a `.glb`, everything after the
/// JSON chunk (the optional binary chunk).
struct Parts<'a> {
    json: &'a [u8],
    glb_tail: Option<&'a [u8]>,
}

fn split(bytes: &[u8]) -> Option<Parts<'_>> {
    if !bytes.starts_with(GLB_MAGIC) {
        return Some(Parts {
            json: bytes,
            glb_tail: None,
        });
    }
    if read_u32(bytes, 16)? != GLB_JSON_CHUNK {
        return None;
    }
    let end = 20usize.checked_add(read_u32(bytes, 12)?.try_into().ok()?)?;
    Some(Parts {
        json: bytes.get(20..end)?,
        glb_tail: Some(&bytes[end..]),
    })
}

/// The data of the binary chunk at the start of `tail`, if there is one.
fn bin_chunk(tail: &[u8]) -> Option<&[u8]> {
    if read_u32(tail, 4)? != GLB_BIN_CHUNK {
        return None;
    }
    let len: usize = read_u32(tail, 0)?.try_into().ok()?;
    tail.get(8..8usize.checked_add(len)?)
}

/// Reassembles a file from new JSON and the [`Parts::glb_tail`] of the original.
fn join(mut json: Vec<u8>, glb_tail: Option<&[u8]>) -> Vec<u8> {
    let Some(tail) = glb_tail else {
        return json;
    };
    // Chunks must be 4-byte aligned; the JSON chunk is padded with spaces.
    json.resize(json.len().next_multiple_of(4), b' ');
    let total = 12 + 8 + json.len() + tail.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(GLB_MAGIC);
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&GLB_JSON_CHUNK.to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(tail);
    out
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let bytes = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A texture using `extension` for image 1, with a fallback core source of
    /// image 0 if `fallback` is set.
    fn texture(extension: &str, fallback: bool) -> Value {
        let mut texture = json!({ "extensions": { extension: { "source": 1 } } });
        if fallback {
            texture["source"] = json!(0);
        }
        texture
    }

    #[test]
    fn leaves_textures_with_a_core_source_alone() {
        let root = json!({ "textures": [texture(WEBP, true), { "source": 0 }] });
        assert!(targets(&root).is_empty());
        assert!(!needs_patch(&root));
    }

    #[test]
    fn targets_textures_without_a_core_source() {
        let root = json!({ "textures": [texture(WEBP, true), texture(BASISU, false)] });
        assert_eq!(
            targets(&root),
            [Target {
                texture: 1,
                image: 1,
                extension: BASISU
            }]
        );
    }

    #[test]
    fn prefers_webp_when_a_texture_has_both() {
        let root = json!({ "textures": [{ "extensions": {
            BASISU: { "source": 2 },
            WEBP: { "source": 3 },
        } }] });
        assert_eq!(
            targets(&root),
            [Target {
                texture: 0,
                image: 3,
                extension: WEBP
            }]
        );
    }

    #[test]
    fn points_textures_at_their_extension_image() {
        let mut root = json!({
            "extensionsRequired": [WEBP],
            "images": [{}, {}],
            "textures": [texture(WEBP, false)],
        });
        assert!(needs_patch(&root));
        let targets = targets(&root);
        apply(&mut root, &targets, &HashSet::new());
        assert_eq!(root["textures"][0]["source"], json!(1));
        assert!(root.get("extensionsRequired").is_none());
        assert!(!needs_patch(&root));
    }

    #[test]
    fn keeps_other_required_extensions() {
        let mut root = json!({ "extensionsRequired": [BASISU, "KHR_materials_unlit"] });
        assert!(needs_patch(&root));
        apply(&mut root, &[], &HashSet::new());
        assert_eq!(root["extensionsRequired"], json!(["KHR_materials_unlit"]));
    }

    #[test]
    fn detaches_unusable_textures_from_materials() {
        let mut root = json!({
            "images": [{}, {}],
            "textures": [texture(BASISU, false), { "source": 0 }],
            "materials": [{
                "pbrMetallicRoughness": {
                    "baseColorTexture": { "index": 0 },
                    "metallicRoughnessTexture": { "index": 1 },
                },
                "normalTexture": { "index": 0, "scale": 1.0 },
                "extensions": { "KHR_materials_clearcoat": { "clearcoatTexture": { "index": 0 } } },
            }],
        });
        let targets = targets(&root);
        apply(&mut root, &targets, &HashSet::from([1]));

        let placeholder = root["textures"][0]["source"].as_u64().unwrap() as usize;
        assert_eq!(placeholder, 2);
        assert_eq!(root["images"][placeholder]["uri"], json!(PLACEHOLDER_PNG));
        assert_eq!(
            root["materials"][0],
            json!({
                "pbrMetallicRoughness": { "metallicRoughnessTexture": { "index": 1 } },
                "extensions": { "KHR_materials_clearcoat": {} },
            })
        );
    }

    #[test]
    fn placeholder_is_a_decodable_png() {
        let data = PLACEHOLDER_PNG.split_once(',').unwrap().1;
        let png = base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap();
        let image = Image::from_buffer(
            &png,
            bevy::image::ImageType::MimeType("image/png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            bevy::image::ImageSampler::Default,
            bevy::asset::RenderAssetUsages::default(),
        )
        .unwrap();
        assert_eq!(image.size(), UVec2::ONE);
    }

    #[test]
    fn detects_etc1s_ktx2() {
        let mut header = [0u8; KTX2_HEADER_LEN];
        header[..12].copy_from_slice(KTX2_IDENTIFIER);
        assert!(!is_etc1s(&header), "no supercompression");
        header[44] = 2; // Zstandard, as used by UASTC
        assert!(!is_etc1s(&header));
        header[44] = 1; // BasisLZ
        assert!(is_etc1s(&header));
        assert!(!is_etc1s(&header[..40]), "truncated");
        assert!(!is_etc1s(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn plain_json_round_trips() {
        let parts = split(b"{}").unwrap();
        assert_eq!(parts.json, b"{}");
        assert_eq!(join(b"{\"a\":1}".to_vec(), parts.glb_tail), b"{\"a\":1}");
    }

    #[test]
    fn glb_round_trips_with_new_json() {
        // A GLB with a JSON chunk and a BIN chunk.
        let bin = [1u8, 2, 3, 4];
        let mut tail = Vec::new();
        tail.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        tail.extend_from_slice(&GLB_BIN_CHUNK.to_le_bytes());
        tail.extend_from_slice(&bin);
        let glb = join(b"{}".to_vec(), Some(&tail));
        assert!(glb.starts_with(GLB_MAGIC));

        let parts = split(&glb).unwrap();
        assert_eq!(parts.json, b"{}  ");
        assert_eq!(bin_chunk(parts.glb_tail.unwrap()), Some(&bin[..]));

        let rewritten = join(b"{\"asset\":1}".to_vec(), parts.glb_tail);
        assert_eq!(read_u32(&rewritten, 8), Some(rewritten.len() as u32));
        let parts = split(&rewritten).unwrap();
        assert_eq!(parts.json, b"{\"asset\":1} ");
        assert_eq!(bin_chunk(parts.glb_tail.unwrap()), Some(&bin[..]));
    }

    #[test]
    fn only_gltf_files_are_patched() {
        assert!(is_gltf(Path::new("a/b.glb")));
        assert!(is_gltf(Path::new("B.GLTF")));
        assert!(!is_gltf(Path::new("b.png")));
        assert!(!is_gltf(Path::new("glb")));
    }
}
