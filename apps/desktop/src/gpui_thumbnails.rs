//! Cached orthographic isometric prototype previews, independent of the main camera.
use super::*;
use meshcraft_core::geometry::Affine3;
use std::hash::{Hash, Hasher};
#[derive(Default)]
pub(super) struct Ui {
    generation: Option<u64>,
    revision: Option<u64>,
    jobs: std::collections::VecDeque<(meshcraft_core::document::DefinitionId, u64, u64)>,
    active: Option<(
        meshcraft_core::document::DefinitionId,
        u64,
        u64,
        Arc<Document>,
    )>,
    runtime: Option<crate::script_runtime::Runtime>,
    retired: Vec<Arc<RenderImage>>,
    images: BTreeMap<meshcraft_core::document::DefinitionId, (u64, Arc<RenderImage>)>,
}
pub(super) fn image(
    shell: &MeshcraftShell,
    id: meshcraft_core::document::DefinitionId,
) -> Option<Arc<RenderImage>> {
    shell.thumbnails.images.get(&id).map(|(_, i)| i.clone())
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    _cx: &mut Context<MeshcraftShell>,
) {
    let generation = shell.session.document_generation();
    let revision = shell.session.document_revision();
    for image in shell.thumbnails.retired.drain(..) {
        let _ = window.drop_image(image);
    }
    if shell.thumbnails.generation == Some(generation)
        && shell.thumbnails.revision == Some(revision)
    {
        return;
    }
    if shell.thumbnails.generation != Some(generation) {
        for (_, i) in shell.thumbnails.images.values() {
            let _ = window.drop_image(i.clone());
        }
        shell.thumbnails.images.clear();
        shell.thumbnails.jobs.clear();
        shell.thumbnails.active = None;
    }
    shell.thumbnails.generation = Some(generation);
    shell.thumbnails.revision = Some(revision);
    let doc = &shell.cached_document;

    let colors = doc.material_colors();
    let mut active = std::collections::BTreeSet::new();
    for d in doc
        .definitions()
        .filter(|d| d.id != Document::SCENE_ROOT_DEFINITION_ID)
    {
        active.insert(d.id);
        let mut faces = vec![];
        let mut edges = vec![];
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        gather(
            doc,
            d.id,
            Affine3::IDENTITY,
            &[],
            &colors,
            &mut faces,
            &mut edges,
            &mut fingerprint,
            0,
        );
        let hash = fingerprint.finish();
        if shell
            .thumbnails
            .images
            .get(&d.id)
            .is_some_and(|(h, _)| *h == hash)
        {
            continue;
        }
        let needs_script = contains_script(doc, d.id, 0);
        let image = if needs_script {
            shell
                .thumbnails
                .images
                .get(&d.id)
                .map(|(_, i)| i.clone())
                .unwrap_or_else(|| render(&faces, &edges))
        } else {
            render(&faces, &edges)
        };
        if needs_script {
            shell.thumbnails.jobs.retain(|(id, _, _)| *id != d.id);
            shell.thumbnails.jobs.push_back((d.id, hash, generation));
        }
        if let Some((_, old)) = shell.thumbnails.images.insert(d.id, (hash, image.clone()))
            && !Arc::ptr_eq(&old, &image)
        {
            let _ = window.drop_image(old);
        }
    }
    shell.thumbnails.images.retain(|id, (_, image)| {
        if active.contains(id) {
            true
        } else {
            let _ = window.drop_image(image.clone());
            false
        }
    });
}
fn contains_script(
    doc: &Document,
    id: meshcraft_core::document::DefinitionId,
    depth: usize,
) -> bool {
    depth < 32
        && (doc.script(id).is_some_and(|s| s.enabled)
            || doc
                .definition(id)
                .and_then(|d| d.default_child_layout)
                .and_then(|id| doc.child_layout(id))
                .is_some_and(|l| {
                    l.entries
                        .iter()
                        .any(|e| contains_script(doc, e.definition, depth + 1))
                }))
}
/// Poll a single reusable worker; prototype previews never mutate live instances.
pub(super) fn tick(shell: &mut MeshcraftShell) -> bool {
    let mut ui = std::mem::take(&mut shell.thumbnails);
    let mut changed = false;
    if let Some(response) = ui.runtime.as_mut().and_then(|r| r.poll())
        && let Some((id, hash, generation, document)) = ui.active.take()
    {
        if response.key != hash.wrapping_add(generation.wrapping_mul(1_000_000_007)) {
            ui.runtime.as_mut().unwrap().submit(
                document.clone(),
                hash.wrapping_add(generation.wrapping_mul(1_000_000_007)),
                0.,
                0.,
            );
            ui.active = Some((id, hash, generation, document));
            shell.thumbnails = ui;
            return false;
        }
        let mut document = (*document).clone();
        document.install_script_frames(response.frames);
        if response.pending && response.errors.is_empty() {
            let document = Arc::new(document);
            ui.runtime.as_mut().unwrap().submit(
                document.clone(),
                hash.wrapping_add(generation.wrapping_mul(1_000_000_007)),
                0.,
                0.,
            );
            ui.active = Some((id, hash, generation, document));
        } else if response.errors.is_empty()
            && generation == shell.session.document_generation()
            && ui.images.get(&id).is_some_and(|(h, _)| *h == hash)
        {
            let mut faces = vec![];
            let mut edges = vec![];
            let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
            gather(
                &document,
                id,
                Affine3::IDENTITY,
                &document.scene_geometry_refs().unwrap_or_default(),
                &document.material_colors(),
                &mut faces,
                &mut edges,
                &mut fingerprint,
                0,
            );
            if let Some((_, old)) = ui.images.insert(id, (hash, render(&faces, &edges))) {
                ui.retired.push(old);
            }
            changed = true;
        }
    }
    if ui.active.is_none() {
        while let Some((id, hash, generation)) = ui.jobs.pop_front() {
            if generation != shell.session.document_generation()
                || !ui.images.get(&id).is_some_and(|(h, _)| *h == hash)
            {
                continue;
            }
            if let Ok((document, _)) = shell.cached_document.script_preview(id) {
                let document = Arc::new(document);
                let runtime = ui
                    .runtime
                    .get_or_insert_with(crate::script_runtime::Runtime::new);
                runtime.set_query_model(shell.cached_document.clone());
                runtime.submit(
                    document.clone(),
                    hash.wrapping_add(generation.wrapping_mul(1_000_000_007)),
                    0.,
                    0.,
                );
                ui.active = Some((id, hash, generation, document));
                break;
            }
        }
    }
    shell.thumbnails = ui;
    changed
}
#[allow(clippy::too_many_arguments)]
fn gather(
    doc: &Document,
    id: meshcraft_core::document::DefinitionId,
    transform: Affine3,
    paths: &[meshcraft_core::document::OccurrenceRef],
    colors: &BTreeMap<String, Rgba8>,
    faces: &mut Vec<([Vec3; 3], Rgba8)>,
    edges: &mut Vec<[Vec3; 2]>,
    hash: &mut impl Hasher,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    let Some(d) = doc.definition(id) else {
        return;
    };
    let script = doc.script(id);
    let frame = paths
        .iter()
        .find(|p| {
            doc.resolve_occurrence(p)
                .is_some_and(|r| r.definition.id == id)
        })
        .and_then(|p| doc.script_frame(p));
    // Authored geometry/source determine invalidation. Animated instance values do not.
    let bytes = serde_json::to_vec(&(
        &d.geometry,
        &d.frame,
        &script,
        d.extensions
            .get(meshcraft_core::document::scripts::parameters::VALUES_KEY),
        doc.prototype_ifc(id),
        d.domain_data.get("hotspots"),
        d.extensions.get("k3d.hotspots"),
    ))
    .unwrap_or_default();
    bytes.hash(hash);
    let g = frame
        .filter(|_| script.is_some())
        .map(|f| &f.geometry)
        .unwrap_or(&d.geometry);
    let transform = transform.compose(d.frame);
    let point = |v| g.vertex_position(v).map(|p| transform.transform_point(p));
    for t in g.triangles.values() {
        if let [Some(a), Some(b), Some(c)] = t.vertices.map(point) {
            let color = meshcraft_core::document::materials::material_id(&t.extensions)
                .and_then(|id| colors.get(id))
                .copied()
                .unwrap_or(t.color);
            [color.red, color.green, color.blue, color.alpha].hash(hash);
            faces.push(([a, b, c], color));
        }
    }
    for s in g.segments.values() {
        if let [Some(a), Some(b)] = s.endpoints.map(point) {
            edges.push([a, b]);
        }
    }
    if let Some(layout) = d.default_child_layout.and_then(|id| doc.child_layout(id)) {
        for e in &layout.entries {
            serde_json::to_vec(&(e.definition, e.transform))
                .unwrap_or_default()
                .hash(hash);
            gather(
                doc,
                e.definition,
                transform.compose(e.transform),
                paths,
                colors,
                faces,
                edges,
                hash,
                depth + 1,
            );
        }
    }
}
fn render(faces: &[([Vec3; 3], Rgba8)], edges: &[[Vec3; 2]]) -> Arc<RenderImage> {
    const W: u32 = 104;
    const H: u32 = 78;
    let bg = [42u8, 49, 58, 255];
    let mut image = ImageBuffer::from_pixel(W, H, Rgba([bg[2], bg[1], bg[0], 255]));
    let mut z = vec![f64::NEG_INFINITY; (W * H) as usize];
    let project = |p: Vec3| {
        [
            (p.x - p.z) * std::f64::consts::FRAC_1_SQRT_2,
            (p.x + p.z) * 0.408248290463863 - p.y * 0.816496580927726,
            (p.x + p.y + p.z) * 0.577350269189626,
        ]
    };
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for p in faces
        .iter()
        .flat_map(|(p, _)| p.iter())
        .chain(edges.iter().flat_map(|p| p.iter()))
    {
        let p = project(*p);
        for i in 0..2 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    if min[0].is_finite() {
        let scale = ((W - 12) as f64 / (max[0] - min[0]).max(1e-6))
            .min((H - 12) as f64 / (max[1] - min[1]).max(1e-6));
        let p = |v: Vec3| {
            let q = project(v);
            [
                (q[0] - (min[0] + max[0]) * 0.5) * scale + W as f64 * 0.5,
                (q[1] - (min[1] + max[1]) * 0.5) * scale + H as f64 * 0.5,
                q[2],
            ]
        };
        for (points, color) in faces {
            let t = points.map(p);
            let area = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1])
                - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
            if area.abs() < 1e-9 {
                continue;
            }
            let normal = (points[1] - points[0]).cross(points[2] - points[0]);
            let light = 0.45 + 0.55 * (normal.y.abs() / normal.length().max(1e-9));
            let x0 = t
                .iter()
                .map(|v| v[0])
                .fold(f64::INFINITY, f64::min)
                .floor()
                .max(0.) as u32;
            let x1 = t
                .iter()
                .map(|v| v[0])
                .fold(f64::NEG_INFINITY, f64::max)
                .ceil()
                .min(W as f64) as u32;
            let y0 = t
                .iter()
                .map(|v| v[1])
                .fold(f64::INFINITY, f64::min)
                .floor()
                .max(0.) as u32;
            let y1 = t
                .iter()
                .map(|v| v[1])
                .fold(f64::NEG_INFINITY, f64::max)
                .ceil()
                .min(H as f64) as u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let px = x as f64 + 0.5;
                    let py = y as f64 + 0.5;
                    let a =
                        ((t[1][0] - px) * (t[2][1] - py) - (t[1][1] - py) * (t[2][0] - px)) / area;
                    let b =
                        ((t[2][0] - px) * (t[0][1] - py) - (t[2][1] - py) * (t[0][0] - px)) / area;
                    let c = 1. - a - b;
                    if a >= 0. && b >= 0. && c >= 0. {
                        let depth = a * t[0][2] + b * t[1][2] + c * t[2][2];
                        let index = (y * W + x) as usize;
                        if depth > z[index] {
                            z[index] = depth;
                            let alpha = f64::from(color.alpha) / 255.;
                            let rgb = [color.blue, color.green, color.red];
                            let old = image.get_pixel(x, y).0;
                            image.put_pixel(
                                x,
                                y,
                                Rgba(std::array::from_fn(|i| {
                                    if i == 3 {
                                        255
                                    } else {
                                        (f64::from(rgb[i]) * light * alpha
                                            + f64::from(old[i]) * (1. - alpha))
                                            .round() as u8
                                    }
                                })),
                            );
                        }
                    }
                }
            }
        }
        for edge in edges {
            let [a, b] = edge.map(p);
            let n = ((b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil() as usize).max(1);
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let x = (a[0] + (b[0] - a[0]) * t).round() as i32;
                let y = (a[1] + (b[1] - a[1]) * t).round() as i32;
                if x >= 0 && y >= 0 && (x as u32) < W && (y as u32) < H {
                    let depth = a[2] + (b[2] - a[2]) * t;
                    let index = y as usize * W as usize + x as usize;
                    if depth >= z[index] - 0.02 / scale {
                        image.put_pixel(x as u32, y as u32, Rgba([29, 25, 22, 255]));
                    }
                }
            }
        }
    }
    Arc::new(RenderImage::new(vec![Frame::new(image)]))
}
