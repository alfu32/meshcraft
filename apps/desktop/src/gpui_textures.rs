//! Material images for the viewport. Files are decoded on a background thread, so a large image
//! never stalls a frame, then uploaded on the render thread with mipmaps and repeat wrapping.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
};

/// Larger images are scaled down before upload; tiles are seen at a few hundred pixels at most.
const MAX_SIZE: u32 = 4096;

/// Image file types the material editor offers and the viewport decodes.
pub(crate) const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tga"];

/// A relative texture path is relative to the project file, so a project and its images can move
/// together.
pub(crate) fn resolve(path: &str, project_dir: Option<&Path>) -> PathBuf {
    let path = Path::new(path);
    match project_dir {
        Some(dir) if path.is_relative() => dir.join(path),
        _ => path.to_owned(),
    }
}

/// How a picked file is stored: relative when it sits beside or below the project.
pub(crate) fn stored_path(file: &Path, project_dir: Option<&Path>) -> String {
    project_dir
        .and_then(|dir| file.strip_prefix(dir).ok())
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

type Decoded = Result<(u32, u32, Vec<u8>), String>;

enum Entry {
    Loading(mpsc::Receiver<Decoded>),
    Ready(u32),
    Failed(String),
}

#[derive(Default)]
pub(super) struct Cache {
    entries: HashMap<PathBuf, Entry>,
}

fn decode(path: &Path) -> Decoded {
    let image = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let image = if image.width().max(image.height()) > MAX_SIZE {
        image.resize(MAX_SIZE, MAX_SIZE, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let image = image.into_rgba8();
    Ok((image.width(), image.height(), image.into_raw()))
}

impl Cache {
    /// The GL texture for a file, once decoded. The first request starts loading it.
    pub fn get(&mut self, path: &Path) -> Option<u32> {
        let entry = self.entries.entry(path.to_owned()).or_insert_with(|| {
            let (send, receive) = mpsc::channel();
            let file = path.to_owned();
            std::thread::spawn(move || {
                let _ = send.send(decode(&file));
            });
            Entry::Loading(receive)
        });
        if let Entry::Loading(receive) = entry {
            match receive.try_recv() {
                Ok(Ok((width, height, pixels))) => *entry = upload(width, height, &pixels),
                Ok(Err(error)) => *entry = Entry::Failed(error),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    *entry = Entry::Failed(format!("{}: decoder stopped", path.display()))
                }
            }
        }
        match entry {
            Entry::Ready(id) => Some(*id),
            _ => None,
        }
    }

    /// True while an image is still decoding, so the viewport keeps drawing until it appears.
    pub fn pending(&self) -> bool {
        self.entries
            .values()
            .any(|e| matches!(e, Entry::Loading(_)))
    }

    pub fn errors(&self) -> Vec<String> {
        self.entries
            .values()
            .filter_map(|e| match e {
                Entry::Failed(error) => Some(format!("Texture unavailable: {error}")),
                _ => None,
            })
            .collect()
    }

    /// Frees images no material uses any more. A failed file is retried when it is used again.
    pub fn retain(&mut self, used: &std::collections::BTreeSet<PathBuf>) {
        self.entries.retain(|path, entry| {
            let keep = used.contains(path) && !matches!(entry, Entry::Failed(_));
            if !keep && let Entry::Ready(id) = entry {
                unsafe { raylib::ffi::rlUnloadTexture(*id) };
            }
            keep
        });
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        self.retain(&Default::default());
    }
}

fn upload(width: u32, height: u32, pixels: &[u8]) -> Entry {
    let format = raylib::ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32;
    let (w, h) = (width as i32, height as i32);
    let id = unsafe { raylib::ffi::rlLoadTexture(pixels.as_ptr().cast(), w, h, format, 1) };
    if id == 0 {
        return Entry::Failed("the GPU refused the image".into());
    }
    let mut levels = 1;
    unsafe {
        raylib::ffi::rlGenTextureMipmaps(id, w, h, format, &mut levels);
        for (param, value) in [
            (
                raylib::ffi::RL_TEXTURE_WRAP_S,
                raylib::ffi::RL_TEXTURE_WRAP_REPEAT,
            ),
            (
                raylib::ffi::RL_TEXTURE_WRAP_T,
                raylib::ffi::RL_TEXTURE_WRAP_REPEAT,
            ),
            (
                raylib::ffi::RL_TEXTURE_MIN_FILTER,
                raylib::ffi::RL_TEXTURE_FILTER_MIP_LINEAR,
            ),
            (
                raylib::ffi::RL_TEXTURE_MAG_FILTER,
                raylib::ffi::RL_TEXTURE_FILTER_LINEAR,
            ),
            (raylib::ffi::RL_TEXTURE_FILTER_ANISOTROPIC, 4),
        ] {
            raylib::ffi::rlTextureParameters(id, param as i32, value as i32);
        }
    }
    Entry::Ready(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picked_files_beside_the_project_are_stored_relative_and_resolve_back() {
        let dir = Path::new("/projects/cabin");
        let stored = stored_path(Path::new("/projects/cabin/textures/brick.jpg"), Some(dir));
        assert_eq!(stored, "textures/brick.jpg");
        assert_eq!(
            resolve(&stored, Some(dir)),
            Path::new("/projects/cabin/textures/brick.jpg")
        );
        // Files elsewhere, and any file before the project is saved, keep their full path.
        assert_eq!(
            stored_path(Path::new("/library/oak.png"), Some(dir)),
            "/library/oak.png"
        );
        assert_eq!(
            stored_path(Path::new("/library/oak.png"), None),
            "/library/oak.png"
        );
        assert_eq!(
            resolve("/library/oak.png", Some(dir)),
            Path::new("/library/oak.png")
        );
    }

    #[test]
    fn decoding_reports_unreadable_files_and_shrinks_huge_images() {
        assert!(decode(Path::new("/nonexistent/texture.png")).is_err());
        let dir = std::env::temp_dir().join(format!("meshcraft-texture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wide.png");
        image::RgbaImage::new(MAX_SIZE * 2, 8).save(&path).unwrap();
        let (width, height, pixels) = decode(&path).unwrap();
        assert_eq!((width, height), (MAX_SIZE, 4));
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
