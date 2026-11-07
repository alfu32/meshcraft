//! Sphere previews of materials for the Materials panel (T84).
//!
//! The panel asks for a picture of each material it shows. A request builds a
//! small private document (a sphere wearing the material) and queues it for the
//! graphics owner, which renders queued pictures with its own renderer, so the
//! main viewport's prepared scene is never disturbed. Pictures are cached by a
//! hash of everything that changes their look: editing a material renders it
//! again, and an unchanged material is never rendered twice.
use super::*;
use meshcraft_core::document::materials::Material;
use meshcraft_core::tools::primitives::PrimitiveKind;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Rendered size in pixels. Tiles show it at about half size, which smooths edges.
const PIXELS: u32 = 176;
/// Matches the tile background, so transparent materials show the panel through them.
pub(super) const BACKDROP: Rgba8 = Rgba8::new(43, 50, 59, 255);
/// Time the graphics owner spends on pictures between viewport frames.
const BUDGET: Duration = Duration::from_millis(40);
/// Texture loads finish asynchronously; a picture waits for them this many times.
const TEXTURE_ATTEMPTS: u32 = 120;
/// Pictures kept before the least recently shown are released.
const CAPACITY: usize = 240;

pub(super) struct Job {
    key: u64,
    scene: RenderScene,
    camera: OrbitCamera,
    attempts: u32,
}

#[derive(Default)]
pub(super) struct Queue {
    jobs: VecDeque<Job>,
    done: Vec<(u64, Arc<RenderImage>)>,
}

pub(super) struct Renderer {
    lighting: lighting::Renderer,
    target: RenderTexture2D,
}

/// Runs on the graphics owner between frames. Returns true when pictures remain.
pub(super) fn render_pending(
    rl: &mut raylib::RaylibHandle,
    thread: &raylib::RaylibThread,
    queue: &Mutex<Queue>,
    state: &mut Option<Renderer>,
) -> bool {
    let start = Instant::now();
    loop {
        let Some(mut job) = queue.lock().ok().and_then(|mut q| q.jobs.pop_front()) else {
            return false;
        };
        let renderer = match state {
            Some(renderer) => renderer,
            None => {
                let target = match rl.load_render_texture(thread, PIXELS, PIXELS) {
                    Ok(target) => target,
                    Err(error) => {
                        eprintln!("create material thumbnail target: {error}");
                        if let Ok(mut q) = queue.lock() {
                            q.jobs.clear();
                        }
                        return false;
                    }
                };
                state.insert(Renderer {
                    lighting: lighting::Renderer::new(rl, thread),
                    target,
                })
            }
        };
        renderer.lighting.prepare(rl, thread, &job.scene);
        draw_offscreen_scene(
            rl,
            thread,
            &mut renderer.target,
            job.camera,
            &job.scene,
            0,
            &mut renderer.lighting,
        );
        if renderer.lighting.gpu.textures.pending() && job.attempts < TEXTURE_ATTEMPTS {
            // Let the texture load while the viewport draws, then try again.
            job.attempts += 1;
            if let Ok(mut q) = queue.lock() {
                q.jobs.push_back(job);
            }
            return true;
        }
        if let Some(image) = copy_target_to_gpui_image(&renderer.target)
            && let Ok(mut q) = queue.lock()
        {
            q.done
                .push((job.key, Arc::new(RenderImage::new(vec![Frame::new(image)]))));
        }
        if start.elapsed() >= BUDGET {
            return queue.lock().is_ok_and(|q| !q.jobs.is_empty());
        }
    }
}

#[derive(Default)]
struct State {
    images: HashMap<u64, (u64, Arc<RenderImage>)>,
    requested: HashSet<u64>,
    /// Pictures that could not be built; the panel shows a colour swatch instead.
    failed: HashSet<u64>,
    retired: Vec<Arc<RenderImage>>,
    clock: u64,
}

/// The panel's picture cache. Rendering the panel only borrows the shell, so the
/// cache uses interior mutability to queue the pictures it is missing.
#[derive(Default)]
pub(super) struct Cache(RefCell<State>);

/// The picture of `material`, queueing it when it has not been rendered yet.
pub(super) fn image(shell: &MeshcraftShell, material: &Material) -> Option<Arc<RenderImage>> {
    let document = &shell.cached_document;
    let root = shell.texture_root();
    let key = key(document, material, root.as_deref());
    let mut state = shell.material_thumbnails.0.borrow_mut();
    state.clock += 1;
    let clock = state.clock;
    if let Some((used, image)) = state.images.get_mut(&key) {
        *used = clock;
        return Some(image.clone());
    }
    if state.failed.contains(&key) || !state.requested.insert(key) {
        return None;
    }
    match job(document, material, root, key) {
        Ok(job) => {
            if let Ok(mut queue) = shell.bridge.material_thumbnails.lock() {
                queue.jobs.push_back(job);
            }
            shell.bridge.request_redraw();
        }
        Err(_) => {
            state.requested.remove(&key);
            state.failed.insert(key);
        }
    }
    None
}

/// Collects finished pictures. True when the panel should be drawn again.
pub(super) fn tick(shell: &MeshcraftShell) -> bool {
    let done = shell
        .bridge
        .material_thumbnails
        .lock()
        .map(|mut q| std::mem::take(&mut q.done))
        .unwrap_or_default();
    if done.is_empty() {
        return false;
    }
    let mut state = shell.material_thumbnails.0.borrow_mut();
    let clock = state.clock;
    for (key, image) in done {
        state.requested.remove(&key);
        if let Some((_, old)) = state.images.insert(key, (clock, image)) {
            state.retired.push(old);
        }
    }
    if state.images.len() > CAPACITY {
        let mut ages = state
            .images
            .iter()
            .map(|(k, (t, _))| (*t, *k))
            .collect::<Vec<_>>();
        ages.sort_unstable();
        for (_, key) in ages.into_iter().take(state.images.len() - CAPACITY) {
            if let Some((_, image)) = state.images.remove(&key) {
                state.retired.push(image);
            }
        }
    }
    true
}

/// Releases replaced pictures from the window's image atlas.
pub(super) fn prepare(shell: &MeshcraftShell, window: &mut Window) {
    for image in shell.material_thumbnails.0.borrow_mut().retired.drain(..) {
        retire_viewport_image(window, image);
    }
}

/// Everything that changes how a material looks: the material itself, the
/// program it runs and the material that program builds on, and the folder its
/// texture path is relative to.
fn key(document: &Document, material: &Material, root: Option<&std::path::Path>) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut shown = material.clone();
    shown.id.clear();
    shown.name.clear();
    serde_json::to_string(&shown)
        .unwrap_or_default()
        .hash(&mut hasher);
    if let Some(config) = &material.procedural {
        if !meshcraft_core::document::procedural::is_builtin(&config.program)
            && let Some(program) = document
                .procedural_programs()
                .into_iter()
                .find(|p| p.id == config.program)
        {
            serde_json::to_string(&program)
                .unwrap_or_default()
                .hash(&mut hasher);
        }
        if let Some(base) = config
            .base_material
            .as_ref()
            .and_then(|id| document.materials().into_iter().find(|m| &m.id == id))
        {
            serde_json::to_string(&base)
                .unwrap_or_default()
                .hash(&mut hasher);
        }
    }
    if material.texture_path.is_some() {
        root.hash(&mut hasher);
    }
    hasher.finish()
}

fn job(
    source: &Document,
    material: &Material,
    texture_root: Option<std::path::PathBuf>,
    key: u64,
) -> Result<Job, String> {
    let (document, reach) = sphere_document(source, material)?;
    let camera = OrbitCamera {
        target: Vec3::new(0., 1., 0.),
        yaw: 35_f32.to_radians(),
        pitch: 20_f32.to_radians(),
        // Frame a sphere of radius `reach` with a small margin.
        distance: reach * 1.12 / (15_f32.to_radians()).sin(),
        field_of_view: 30.,
        ..Default::default()
    };
    Ok(Job {
        key,
        scene: RenderScene {
            document: Arc::new(document),
            grid_visible: false,
            guides_visible: false,
            black_model_lines: false,
            texture_root,
            antialiasing: postprocess::Antialiasing::Off,
            backdrop: Some(BACKDROP),
            ..Default::default()
        },
        camera,
        attempts: 0,
    })
}

/// A unit sphere centred one unit above the origin, wearing `material`, and the
/// radius the camera must frame (vegetation grows out of the sphere).
pub(super) fn sphere_document(
    source: &Document,
    material: &Material,
) -> Result<(Document, f32), String> {
    let mut document = Document::new();
    let mut reach = 1.;
    if let Some(config) = &material.procedural {
        let program = source
            .procedural_programs()
            .into_iter()
            .find(|p| p.id == config.program)
            .ok_or("Shader program is unavailable")?;
        if program.is_object_generator() {
            return Err("Shader object programs have no surface preview".into());
        }
        // Blades and leaves grow out of the sphere: frame their full length.
        let value = |id: &str| {
            config
                .parameters
                .get(id)
                .or_else(|| {
                    program
                        .parameters
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| &p.default)
                })
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.) as f32
        };
        reach += match program.generation.as_str() {
            "blades" => value("height"),
            "foliage" => value("height") + value("size"),
            _ => 0.,
        }
        .clamp(0., 3.);
        if !meshcraft_core::document::procedural::is_builtin(&program.id) {
            document.put_procedural_program(program)?;
        }
        if let Some(base) = config
            .base_material
            .as_ref()
            .and_then(|id| source.materials().into_iter().find(|m| &m.id == id))
        {
            document.put_material(base)?;
        }
    }
    document.put_material(material.clone())?;
    let center = Vec3::new(0., 1., 0.);
    let mesh = meshcraft_core::operations::primitives::build_primitive(
        PrimitiveKind::Sphere,
        &[center, center + Vec3::X],
        72,
    )?;
    let (_, faces) = document.append_colored_indexed_mesh_to_scene_root(
        mesh.vertices,
        Vec::new(),
        mesh.triangles
            .into_iter()
            .map(|t| (t, material.color))
            .collect(),
    )?;
    let occurrence = document
        .scene_geometry_refs()?
        .into_iter()
        .next()
        .ok_or("Preview sphere is missing")?;
    let mut selection = meshcraft_core::selection::SelectionSession::default();
    for face in faces {
        selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(face),
        });
    }
    meshcraft_core::operations::transforms::assign_material(
        &mut document,
        &selection,
        &material.id,
    )?;
    Ok((document, reach))
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshcraft_core::document::material_library::material_library;

    #[test]
    fn every_library_material_has_a_sphere_preview_and_its_own_picture_key() {
        let source = Document::new();
        let mut keys = HashSet::new();
        for material in material_library().into_iter().flat_map(|s| s.materials) {
            let (document, reach) = sphere_document(&source, &material)
                .unwrap_or_else(|e| panic!("{}: {e}", material.id));
            let geometry = document
                .scene_geometry_refs()
                .unwrap()
                .into_iter()
                .map(|p| document.effective_geometry(&p).unwrap().clone())
                .collect::<Vec<_>>();
            assert!(geometry.iter().flat_map(|g| g.triangles.values()).all(|t| {
                meshcraft_core::document::materials::material_id(&t.extensions)
                    == Some(material.id.as_str())
            }));
            assert!(
                geometry.iter().all(|g| g.segments.is_empty()),
                "no edge lines"
            );
            assert!(reach >= 1.);
            assert!(
                keys.insert(key(&source, &material, None)),
                "{}",
                material.id
            );
        }
    }

    #[test]
    fn picture_key_follows_the_look_not_the_name() {
        let mut document = Document::new();
        let material = Material {
            id: "paint".into(),
            name: "Paint".into(),
            color: Rgba8::new(200, 30, 30, 255),
            ..Default::default()
        };
        document.put_material(material.clone()).unwrap();
        let renamed = Material {
            name: "Red paint".into(),
            ..material.clone()
        };
        let recolored = Material {
            color: Rgba8::new(30, 30, 200, 255),
            ..material.clone()
        };
        let base = key(&document, &material, None);
        assert_eq!(base, key(&document, &renamed, None));
        assert_ne!(base, key(&document, &recolored, None));
    }
}
