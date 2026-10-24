//! Retained local-space scene meshes and transform-only GPU instances (OpenGL 3.3).
//! Resources are created, used and dropped on the raylib context's worker thread.
use super::*;
use meshcraft_core::{
    document::GeometryBundle,
    geometry::Affine3,
    selection::{SelectionDisplayPolicy, SelectionKind},
};
use raylib::prelude::{RaylibShader, Shader};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    mem::size_of,
};

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct SurfaceVertex {
    position: [f32; 3],
    color: [u8; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct LineVertex {
    start: [f32; 3],
    end: [f32; 3],
    color: [u8; 4],
}
#[derive(Clone, Debug, Default, PartialEq)]
struct MeshData {
    opaque: Vec<SurfaceVertex>,
    glass: Vec<SurfaceVertex>,
    lines: Vec<LineVertex>,
}
impl MeshData {
    fn key(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for vertices in [&self.opaque, &self.glass] {
            vertices.len().hash(&mut h);
            for v in vertices {
                v.position.map(f32::to_bits).hash(&mut h);
                v.color.hash(&mut h);
            }
        }
        self.lines.len().hash(&mut h);
        for l in &self.lines {
            l.start.map(f32::to_bits).hash(&mut h);
            l.end.map(f32::to_bits).hash(&mut h);
            l.color.hash(&mut h);
        }
        h.finish()
    }
    fn is_empty(&self) -> bool {
        self.opaque.is_empty() && self.glass.is_empty() && self.lines.is_empty()
    }
}
fn xyz(p: Vec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}
fn rgba(c: Rgba8) -> [u8; 4] {
    [c.red, c.green, c.blue, c.alpha]
}

#[derive(Clone)]
struct SourcePart {
    entity: SelectedEntity,
    origin: Vec3,
    data: Arc<MeshData>,
    key: u64,
}
#[derive(Clone)]
struct Placement {
    target: SelectionTarget,
    transform: Affine3,
}
struct PlannedGroup {
    data: Arc<MeshData>,
    key: u64,
    placements: Vec<Placement>,
}
#[derive(Default)]
struct SourceBuilder {
    origin: Option<Vec3>,
    data: MeshData,
}
impl SourceBuilder {
    fn relative(&mut self, p: Vec3) -> [f32; 3] {
        let origin = *self.origin.get_or_insert(p);
        xyz(p - origin)
    }
}
fn source_parts(
    g: &GeometryBundle,
    policy: &SelectionDisplayPolicy,
    object_wire: bool,
    colors: &BTreeMap<String, Rgba8>,
    suppressed: &std::collections::BTreeSet<String>,
) -> Vec<SourcePart> {
    let mut parts = BTreeMap::<SelectedEntity, SourceBuilder>::new();
    let edge_key = |a: Vec3, b: Vec3| {
        let key = |p: Vec3| [p.x, p.y, p.z].map(|v| if v == 0. { 0 } else { v.to_bits() });
        let a = key(a);
        let b = key(b);
        if a <= b { (a, b) } else { (b, a) }
    };
    let mut wire_edges = BTreeMap::new();
    let owner = |ext: &BTreeMap<String, Value>| {
        meshcraft_core::document::architecture::semantic_entity(ext)
            .unwrap_or(SelectedEntity::Occurrence)
    };
    let color = |ext: &BTreeMap<String, Value>, fallback| {
        meshcraft_core::document::materials::material_id(ext)
            .and_then(|id| colors.get(id))
            .copied()
            .unwrap_or(fallback)
    };
    for f in g.triangles.values() {
        let entity = owner(&f.extensions);
        let kind = if entity == SelectedEntity::Occurrence {
            SelectionKind::Face
        } else {
            entity.kind()
        };
        if !policy.can_draw(kind) {
            continue;
        }
        if policy.state(kind).wireframe || object_wire {
            // Architecture supplies its own boundaries; raw mesh wireframes expose
            // triangulation. Build it once in local space, then instance it too.
            if entity == SelectedEntity::Occurrence
                && let [Some(a), Some(b), Some(c)] = f.vertices.map(|r| g.vertex_position(r))
            {
                for [a, b] in [[a, b], [b, c], [c, a]] {
                    wire_edges
                        .entry(edge_key(a, b))
                        .or_insert(([a, b], color(&f.extensions, f.color)));
                }
            }
            continue;
        }
        if meshcraft_core::document::materials::material_id(&f.extensions)
            .is_some_and(|id| suppressed.contains(&procedural::surface_key(id, &f.extensions)))
        {
            continue;
        }
        let c = rgba(color(&f.extensions, f.color));
        let part = parts.entry(entity).or_default();
        if let [Some(a), Some(b), Some(corner)] = f.vertices.map(|r| g.vertex_position(r)) {
            let positions = [a, b, corner].map(|p| part.relative(p));
            let vertices = if c[3] == 255 {
                &mut part.data.opaque
            } else {
                &mut part.data.glass
            };
            vertices.extend(positions.map(|position| SurfaceVertex { position, color: c }));
        }
    }
    for e in g.segments.values() {
        let entity = owner(&e.extensions);
        let kind = if entity == SelectedEntity::Occurrence {
            SelectionKind::Edge
        } else {
            entity.kind()
        };
        if !policy.can_draw(kind) {
            continue;
        }
        if let [Some(a), Some(b)] = e.endpoints.map(|r| g.vertex_position(r)) {
            if entity == SelectedEntity::Occurrence {
                wire_edges.remove(&edge_key(a, b));
            }
            let part = parts.entry(entity).or_default();
            let start = part.relative(a);
            let end = part.relative(b);
            part.data.lines.push(LineVertex {
                start,
                end,
                color: rgba(color(&e.extensions, e.color)),
            });
        }
    }
    for (edge, c) in wire_edges.into_values() {
        let part = parts.entry(SelectedEntity::Occurrence).or_default();
        let start = part.relative(edge[0]);
        let end = part.relative(edge[1]);
        part.data.lines.push(LineVertex {
            start,
            end,
            color: rgba(c),
        });
    }
    parts
        .into_iter()
        .filter(|(_, part)| !part.data.is_empty())
        .map(|(entity, part)| SourcePart {
            entity,
            origin: part.origin.unwrap(),
            key: part.data.key(),
            data: Arc::new(part.data),
        })
        .collect()
}
#[cfg(test)]
fn plan(document: &Document, policy: &SelectionDisplayPolicy) -> Vec<PlannedGroup> {
    plan_with_procedural(document, policy, &std::collections::BTreeSet::new())
}
fn plan_with_procedural(
    document: &Document,
    policy: &SelectionDisplayPolicy,
    suppressed: &std::collections::BTreeSet<String>,
) -> Vec<PlannedGroup> {
    let mut sources = HashMap::new();
    let mut groups: Vec<PlannedGroup> = vec![];
    let mut by_hash = HashMap::<u64, Vec<usize>>::new();
    let colors = document.material_colors();
    for occurrence in document.scene_geometry_refs().unwrap_or_default() {
        if !policy.occurrence_visible(document, &occurrence) {
            continue;
        }
        let (Some(r), Some(g)) = (
            document.resolve_occurrence(&occurrence),
            document.effective_geometry(&occurrence),
        ) else {
            continue;
        };
        let wire = document.is_object_occurrence(&occurrence)
            && policy.state(SelectionKind::Object).wireframe;
        // Cache by source identity within this immutable snapshot. No world-space duplication.
        let parts = sources
            .entry((g as *const GeometryBundle as usize, wire))
            .or_insert_with(|| source_parts(g, policy, wire, &colors, suppressed));
        for part in parts.iter() {
            let at = by_hash.get(&part.key).and_then(|indices| {
                indices
                    .iter()
                    .copied()
                    .find(|i| groups[*i].data == part.data)
            });
            let at = at.unwrap_or_else(|| {
                let i = groups.len();
                groups.push(PlannedGroup {
                    key: part.key,
                    data: Arc::clone(&part.data),
                    placements: vec![],
                });
                by_hash.entry(part.key).or_default().push(i);
                i
            });
            groups[at].placements.push(Placement {
                target: SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: part.entity.clone(),
                },
                transform: r
                    .world_transform
                    .compose(Affine3::from_translation(part.origin)),
            });
        }
    }
    groups
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct Instance {
    rows: [[f32; 4]; 3],
    flags: [f32; 4],
}
impl Instance {
    fn new(transform: Affine3, external: bool, selected: bool) -> Self {
        let t = xyz(transform.translation);
        Self {
            rows: std::array::from_fn(|i| {
                [
                    transform.linear[i][0] as f32,
                    transform.linear[i][1] as f32,
                    transform.linear[i][2] as f32,
                    t[i],
                ]
            }),
            flags: [
                if transform.determinant() < 0. {
                    -1.
                } else {
                    1.
                },
                if external { 1. } else { 0. },
                if selected { 1. } else { 0. },
                0.,
            ],
        }
    }
}
// These repr(C) records contain only initialized arrays with no trailing/inter-field padding.
const _: () = {
    assert!(size_of::<SurfaceVertex>() == 16);
    assert!(size_of::<LineVertex>() == 28);
    assert!(size_of::<Instance>() == 64);
};

struct Buffer {
    id: u32,
    capacity: usize,
}
impl Buffer {
    fn upload<T>(values: &[T], dynamic: bool) -> Self {
        let bytes = std::mem::size_of_val(values);
        let size = i32::try_from(bytes).expect("GPU buffer chunks are bounded to i32 byte sizes");
        let id = unsafe { raylib::ffi::rlLoadVertexBuffer(values.as_ptr().cast(), size, dynamic) };
        assert_ne!(id, 0, "could not allocate GPU vertex buffer");
        Self {
            id,
            capacity: bytes,
        }
    }
    fn update<T>(&mut self, values: &[T]) {
        let bytes = std::mem::size_of_val(values);
        if bytes > self.capacity {
            *self = Self::upload(values, true);
        } else if bytes > 0 {
            unsafe {
                raylib::ffi::rlUpdateVertexBuffer(
                    self.id,
                    values.as_ptr().cast(),
                    i32::try_from(bytes).unwrap(),
                    0,
                )
            };
        }
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { raylib::ffi::rlUnloadVertexBuffer(self.id) }
    }
}
struct Chunk {
    vao: u32,
    _vertices: Buffer,
    count: i32,
    instance_binding: std::cell::Cell<u32>,
    indices: Option<Buffer>,
}
impl Drop for Chunk {
    fn drop(&mut self) {
        unsafe { raylib::ffi::rlUnloadVertexArray(self.vao) }
    }
}
struct Mesh {
    chunks: Vec<Chunk>,
}
// Split only at triangle boundaries. Each index addresses at most 65,535 distinct
// (position, color) vertices; material discontinuities therefore cannot bleed.
fn surface_chunks(corners: &[SurfaceVertex]) -> Vec<(Vec<SurfaceVertex>, Vec<u16>)> {
    let mut result = vec![];
    let mut vertices = vec![];
    let mut indices = vec![];
    let mut map = HashMap::new();
    for triangle in corners.as_chunks::<3>().0 {
        let keys = triangle
            .iter()
            .map(|v| (v.position.map(f32::to_bits), v.color))
            .collect::<Vec<_>>();
        let needed = keys.iter().filter(|k| !map.contains_key(*k)).count();
        if vertices.len() + needed > u16::MAX as usize || indices.len() >= 196_608 {
            result.push((std::mem::take(&mut vertices), std::mem::take(&mut indices)));
            map.clear();
        }
        for (vertex, key) in triangle.iter().zip(keys) {
            let index = *map.entry(key).or_insert_with(|| {
                let index = vertices.len() as u16;
                vertices.push(*vertex);
                index
            });
            indices.push(index);
        }
    }
    if !indices.is_empty() {
        result.push((vertices, indices));
    }
    result
}

impl Mesh {
    fn surfaces(vertices: &[SurfaceVertex]) -> Option<Self> {
        if vertices.is_empty() {
            return None;
        }
        let mut chunks = vec![];
        for (vertices, indices) in surface_chunks(vertices) {
            let mut mesh = Self::upload(&vertices, false).unwrap();
            let chunk = &mut mesh.chunks[0];
            unsafe {
                raylib::ffi::rlEnableVertexArray(chunk.vao);
            }
            let bytes = std::mem::size_of_val(indices.as_slice());
            let id = unsafe {
                raylib::ffi::rlLoadVertexBufferElement(
                    indices.as_ptr().cast(),
                    i32::try_from(bytes).unwrap(),
                    false,
                )
            };
            assert_ne!(id, 0, "could not allocate GPU index buffer");
            chunk.indices = Some(Buffer {
                id,
                capacity: bytes,
            });
            chunk.count = indices.len() as i32;
            unsafe {
                // The element binding belongs to this VAO. Leave it attached;
                // binding an element buffer with VAO 0 is invalid in core OpenGL.
                raylib::ffi::rlDisableVertexArray();
            }
            chunks.append(&mut mesh.chunks);
        }
        Some(Self { chunks })
    }
    fn lines(lines: &[LineVertex]) -> Option<Self> {
        // Six identical endpoint records per ribbon; gl_VertexID chooses its corners.
        // Chunk boundaries stay multiples of six, including meshes exceeding 65k vertices.
        if lines.is_empty() {
            return None;
        }
        let mut chunks = vec![];
        for lines in lines.chunks(16_384) {
            let vertices = lines.iter().flat_map(|l| [*l; 6]).collect::<Vec<_>>();
            chunks.append(&mut Self::upload(&vertices, true).unwrap().chunks);
        }
        Some(Self { chunks })
    }
    fn upload<T>(vertices: &[T], line: bool) -> Option<Self> {
        if vertices.is_empty() {
            return None;
        }
        let mut chunks = vec![];
        // Non-indexed chunks avoid raylib's 16-bit element limit; no primitive is split.
        for vertices in vertices.chunks(98_304) {
            let vao = unsafe { raylib::ffi::rlLoadVertexArray() };
            assert_ne!(vao, 0, "OpenGL 3.3 VAOs are required");
            unsafe {
                raylib::ffi::rlEnableVertexArray(vao);
            }
            let buffer = Buffer::upload(vertices, false);
            unsafe {
                raylib::ffi::rlEnableVertexBuffer(buffer.id);
                let stride = size_of::<T>() as i32;
                attribute(0, 3, raylib::ffi::RL_FLOAT as i32, false, stride, 0, 0);
                if line {
                    attribute(1, 3, raylib::ffi::RL_FLOAT as i32, false, stride, 12, 0);
                }
                attribute(
                    3,
                    4,
                    raylib::ffi::RL_UNSIGNED_BYTE as i32,
                    true,
                    stride,
                    if line { 24 } else { 12 },
                    0,
                );
                raylib::ffi::rlDisableVertexArray();
                raylib::ffi::rlDisableVertexBuffer();
            }
            chunks.push(Chunk {
                vao,
                _vertices: buffer,
                count: vertices.len() as i32,
                instance_binding: std::cell::Cell::new(0),
                indices: None,
            });
        }
        Some(Self { chunks })
    }
    fn draw(&self, instances: &Buffer, count: usize) {
        let count = i32::try_from(count).expect("instance counts fit OpenGL draw range");
        if count == 0 {
            return;
        }
        for chunk in &self.chunks {
            unsafe {
                raylib::ffi::rlEnableVertexArray(chunk.vao);
                if chunk.instance_binding.get() != instances.id {
                    raylib::ffi::rlEnableVertexBuffer(instances.id);
                    for i in 0..4 {
                        attribute(
                            4 + i,
                            4,
                            raylib::ffi::RL_FLOAT as i32,
                            false,
                            64,
                            i as i32 * 16,
                            1,
                        );
                    }
                    chunk.instance_binding.set(instances.id);
                }
                if chunk.indices.is_some() {
                    raylib::ffi::rlDrawVertexArrayElementsInstanced(
                        0,
                        chunk.count,
                        std::ptr::null(),
                        count,
                    );
                } else {
                    raylib::ffi::rlDrawVertexArrayInstanced(0, chunk.count, count);
                }
                raylib::ffi::rlDisableVertexArray();
                raylib::ffi::rlDisableVertexBuffer();
            }
        }
    }
}
unsafe fn attribute(
    index: u32,
    count: i32,
    kind: i32,
    normalized: bool,
    stride: i32,
    offset: i32,
    divisor: i32,
) {
    unsafe {
        raylib::ffi::rlSetVertexAttribute(index, count, kind, normalized, stride, offset);
        raylib::ffi::rlEnableVertexAttribute(index);
        raylib::ffi::rlSetVertexAttributeDivisor(index, divisor);
    }
}
struct Group {
    key: u64,
    data: Arc<MeshData>,
    opaque: Option<Mesh>,
    glass: Option<Mesh>,
    lines: Option<Mesh>,
    placements: Vec<Placement>,
    instances: Vec<Instance>,
    buffer: Buffer,
}
#[derive(Default, Debug, Clone, Copy)]
pub(super) struct Stats {
    pub geometry_uploads: usize,
    pub instance_uploads: usize,
    pub feedback_uploads: usize,
    pub groups: usize,
    pub instances: usize,
    pub surface_vertices: usize,
    pub indexed_vertices: usize,
    pub line_segments: usize,
}

pub(super) struct Feedback<'a> {
    pub outlines: &'a [([Vec3; 2], bool, Rgba8)],
    pub partial: &'a [([Vec3; 2], Rgba8)],
    pub bounds: &'a [[Vec3; 2]],
}
pub(super) struct Scene {
    pub procedural: procedural::Renderer,
    groups: Vec<Group>,
    surface: Shader,
    line: Shader,
    pub stats: Stats,
    overlay: Vec<Group>,
    feedback: [Option<Group>; 2],
}
impl Scene {
    pub fn new(rl: &mut raylib::RaylibHandle, thread: &raylib::RaylibThread) -> Self {
        Self {
            procedural: procedural::Renderer::default(),
            groups: vec![],
            overlay: vec![],
            feedback: [None, None],
            surface: rl.load_shader_from_memory(
                thread,
                Some(include_str!("../../../assets/shaders/mesh.vs")),
                Some(include_str!("../../../assets/shaders/scene.fs")),
            ),
            line: rl.load_shader_from_memory(
                thread,
                Some(include_str!("../../../assets/shaders/lines.vs")),
                Some(include_str!("../../../assets/shaders/lines.fs")),
            ),
            stats: Stats::default(),
        }
    }
    pub fn prepare(
        &mut self,
        scene: &RenderScene,
        selection: &lighting::SelectionIndex,
        rebuild: bool,
        feedback: Option<Feedback<'_>>,
    ) {
        if scene.preview.is_none() || matches!(scene.preview, Some(ToolPreview::Placement { .. })) {
            self.feedback[0] = None;
        }
        if scene.reference_edges.is_empty() {
            self.feedback[1] = None;
        }
        if rebuild {
            let plans = plan_with_procedural(
                &scene.document,
                &scene.display_policy,
                &self.procedural.surface_materials,
            );
            let mut kept = std::mem::take(&mut self.groups);
            for planned in plans {
                let old = kept
                    .iter()
                    .position(|g| g.key == planned.key && g.data == planned.data);
                let mut group = if let Some(i) = old {
                    kept.swap_remove(i)
                } else {
                    self.upload(planned.key, planned.data, true)
                };
                group.placements = planned.placements;
                self.groups.push(group);
            }
        }
        if rebuild || feedback.is_some() {
            for group in &mut self.groups {
                let instances = group
                    .placements
                    .iter()
                    .map(|p| {
                        Instance::new(
                            p.transform,
                            lighting::external(&scene.edit_context, &scene.document, &p.target),
                            selection.contains(&p.target),
                        )
                    })
                    .collect::<Vec<_>>();
                if instances != group.instances {
                    group.buffer.update(&instances);
                    group.instances = instances;
                    self.stats.instance_uploads += 1;
                }
            }
            if let Some(feedback) = feedback {
                self.prepare_overlay(feedback.outlines, feedback.partial, feedback.bounds);
            }
        }
        self.stats.groups = self.groups.len();
        self.stats.instances = self.groups.iter().map(|g| g.instances.len()).sum();
        self.stats.surface_vertices = self
            .groups
            .iter()
            .map(|g| g.data.opaque.len() + g.data.glass.len())
            .sum();
        self.stats.indexed_vertices = self
            .groups
            .iter()
            .flat_map(|g| [g.opaque.as_ref(), g.glass.as_ref()])
            .flatten()
            .flat_map(|m| &m.chunks)
            .map(|c| c._vertices.capacity / size_of::<SurfaceVertex>())
            .sum();
        self.stats.line_segments = self.groups.iter().map(|g| g.data.lines.len()).sum();
    }
    fn upload(&mut self, key: u64, data: Arc<MeshData>, model: bool) -> Group {
        if !data.is_empty() {
            if model {
                self.stats.geometry_uploads += 1;
            } else {
                self.stats.feedback_uploads += 1;
            }
        }
        let instance = Instance::new(Affine3::IDENTITY, false, false);
        Group {
            key,
            opaque: Mesh::surfaces(&data.opaque),
            glass: Mesh::surfaces(&data.glass),
            lines: Mesh::lines(&data.lines),
            data,
            placements: vec![],
            instances: vec![],
            buffer: Buffer::upload(&[instance], true),
        }
    }
    fn prepare_overlay(
        &mut self,
        outlines: &[([Vec3; 2], bool, Rgba8)],
        partial: &[([Vec3; 2], Rgba8)],
        bounds: &[[Vec3; 2]],
    ) {
        let mut sets: [MeshData; 3] = std::array::from_fn(|_| MeshData::default());
        for (e, selected, c) in outlines {
            sets[usize::from(*selected)].lines.push(LineVertex {
                start: xyz(e[0]),
                end: xyz(e[1]),
                color: rgba(*c),
            });
        }
        for (e, c) in partial {
            sets[1].lines.push(LineVertex {
                start: xyz(e[0]),
                end: xyz(e[1]),
                color: rgba(*c),
            });
        }
        for e in bounds {
            sets[2].lines.push(LineVertex {
                start: xyz(e[0]),
                end: xyz(e[1]),
                color: [45, 128, 245, 255],
            });
        }
        let mut old = std::mem::take(&mut self.overlay);
        for (i, data) in sets.into_iter().enumerate() {
            let key = data.key();
            let at = old.iter().position(|g| g.key == key && *g.data == data);
            let mut group = if let Some(at) = at {
                old.swap_remove(at)
            } else {
                self.upload(key, Arc::new(data), false)
            };
            let instance = Instance::new(Affine3::IDENTITY, false, i == 1);
            if group.instances != [instance] {
                group.buffer.update(&[instance]);
                group.instances = vec![instance];
            }
            self.overlay.push(group);
        }
    }
    pub fn draw_shadow(&mut self, shader: &mut Shader) {
        flush();
        vp(shader);
        float(shader, "depthOffset", 0.);
        unsafe {
            raylib::ffi::rlEnableShader(shader.id);
            raylib::ffi::rlDisableBackfaceCulling();
        }
        for g in &self.groups {
            if let Some(mesh) = &g.opaque {
                mesh.draw(&g.buffer, g.instances.len());
            }
        }
        finish();
    }
    pub fn set_light_matrix(&mut self, matrix: raylib::ffi::Matrix) {
        self.procedural.set_light(matrix);
        self.surface
            .set_shader_value_matrix(self.surface.get_shader_location("lightVP"), matrix);
    }
    pub fn draw_surfaces(&mut self, scene: &RenderScene, camera: OrbitCamera, shadow_texture: u32) {
        flush();
        vp(&mut self.surface);
        let s = &mut self.surface;
        s.set_shader_value(
            s.get_shader_location("towardLight"),
            ray_vec3(scene.lighting.direction()),
        );
        s.set_shader_value(s.get_shader_location("eye"), ray_vec3(camera.position()));
        for (name, value) in [
            ("depthOffset", 0.00015),
            ("ambient", scene.lighting.ambient),
            ("directional", scene.lighting.directional),
            ("shadowStrength", scene.lighting.shadow_strength),
            ("specular", scene.lighting.specular),
            ("depthBias", scene.lighting.depth_bias),
            ("normalBias", scene.lighting.normal_bias),
        ] {
            float(s, name, value);
        }
        s.set_shader_value(
            s.get_shader_location("shadows"),
            i32::from(scene.lighting.shadows),
        );
        s.set_shader_value(s.get_shader_location("shadowMap"), 1i32);
        unsafe {
            raylib::ffi::rlEnableShader(s.id);
            raylib::ffi::rlActiveTextureSlot(1);
            raylib::ffi::rlEnableTexture(shadow_texture);
            raylib::ffi::rlActiveTextureSlot(0);
            raylib::ffi::rlDisableBackfaceCulling();
        }
        for g in &self.groups {
            if let Some(mesh) = &g.opaque {
                mesh.draw(&g.buffer, g.instances.len());
            }
        }
        self.procedural
            .draw(scene, Some(camera), false, shadow_texture, false);
        unsafe {
            raylib::ffi::rlEnableShader(s.id);
            raylib::ffi::rlActiveTextureSlot(1);
            raylib::ffi::rlEnableTexture(shadow_texture);
            raylib::ffi::rlActiveTextureSlot(0);
            raylib::ffi::rlDisableBackfaceCulling();
        }
        // Opaque depth is complete before any shared translucent group is blended.
        unsafe {
            raylib::ffi::rlDisableDepthMask();
        }
        for g in &self.groups {
            if let Some(mesh) = &g.glass {
                mesh.draw(&g.buffer, g.instances.len());
            }
        }
        unsafe {
            raylib::ffi::rlEnableDepthMask();
            raylib::ffi::rlActiveTextureSlot(1);
            raylib::ffi::rlDisableTexture();
            raylib::ffi::rlActiveTextureSlot(0);
        }
        finish();
        self.procedural
            .draw(scene, Some(camera), false, shadow_texture, true);
    }
    pub fn draw_lines(&mut self, scene: &RenderScene, size: (f32, f32)) {
        flush();
        self.line_settings(
            size,
            scene.normal_line_width,
            scene.thick_line_width,
            scene.black_model_lines,
        );
        unsafe {
            raylib::ffi::rlEnableShader(self.line.id);
            raylib::ffi::rlDisableBackfaceCulling();
        }
        for g in &self.groups {
            if let Some(mesh) = &g.lines {
                mesh.draw(&g.buffer, g.instances.len());
            }
        }
        for (i, g) in self.overlay.iter().enumerate().take(2) {
            self.line.set_shader_value(
                self.line.get_shader_location("blackLines"),
                i32::from(scene.black_model_lines && i == 0),
            );
            unsafe {
                raylib::ffi::rlEnableShader(self.line.id);
            }
            if let Some(mesh) = &g.lines {
                mesh.draw(&g.buffer, 1);
            }
        }
        self.line_settings(size, 1.5, 1.5, false);
        unsafe {
            raylib::ffi::rlEnableShader(self.line.id);
            raylib::ffi::rlDisableDepthTest();
        }
        if let Some(g) = self.overlay.get(2)
            && let Some(mesh) = &g.lines
        {
            mesh.draw(&g.buffer, 1);
        }
        unsafe {
            raylib::ffi::rlEnableDepthTest();
        }
        finish();
    }
    pub fn draw_feedback(
        &mut self,
        edges: &[[Vec3; 2]],
        width: f32,
        size: (f32, f32),
        slot: usize,
    ) {
        let data = MeshData {
            lines: edges
                .iter()
                .map(|e| LineVertex {
                    start: xyz(e[0]),
                    end: xyz(e[1]),
                    color: [235, 118, 38, 255],
                })
                .collect(),
            ..Default::default()
        };
        if self.feedback[slot].as_ref().is_none_or(|g| *g.data != data) {
            self.feedback[slot] = Some(self.upload(data.key(), Arc::new(data), false));
            let instance = Instance::new(Affine3::IDENTITY, false, false);
            let group = self.feedback[slot].as_mut().unwrap();
            group.buffer.update(&[instance]);
            group.instances = vec![instance];
        }
        flush();
        self.line_settings(size, width, width, false);
        unsafe {
            raylib::ffi::rlEnableShader(self.line.id);
            raylib::ffi::rlDisableBackfaceCulling();
        }
        if let Some(g) = &self.feedback[slot]
            && let Some(mesh) = &g.lines
        {
            mesh.draw(&g.buffer, 1);
        }
        finish();
    }
    fn line_settings(&mut self, size: (f32, f32), normal: f32, selected: f32, black: bool) {
        vp(&mut self.line);
        self.line.set_shader_value(
            self.line.get_shader_location("viewportSize"),
            raylib::prelude::Vector2::new(size.0.max(1.), size.1.max(1.)),
        );
        self.line.set_shader_value(
            self.line.get_shader_location("lineWidths"),
            raylib::prelude::Vector2::new(normal.max(0.1), selected.max(0.1)),
        );
        self.line.set_shader_value(
            self.line.get_shader_location("blackLines"),
            i32::from(black),
        );
    }
}
fn flush() {
    unsafe {
        raylib::ffi::rlDrawRenderBatchActive();
    }
}
fn finish() {
    unsafe {
        raylib::ffi::rlDisableVertexArray();
        raylib::ffi::rlDisableVertexBuffer();
        raylib::ffi::rlDisableShader();
        raylib::ffi::rlEnableBackfaceCulling();
    }
}
fn vp(shader: &mut Shader) {
    let matrix = unsafe {
        raylib::ffi::MatrixMultiply(
            raylib::ffi::rlGetMatrixModelview(),
            raylib::ffi::rlGetMatrixProjection(),
        )
    };
    shader.set_shader_value_matrix(shader.get_shader_location("viewProjection"), matrix);
}
fn float(shader: &mut Shader, name: &str, value: f32) {
    shader.set_shader_value(shader.get_shader_location(name), value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshcraft_core::{
        document::{GeometryBundleId, InstanceEdits},
        operations::{objects::place_definition, transforms::transform_selection},
        selection::{SelectionMode, SelectionSession},
    };
    fn copies() -> (Document, Vec<meshcraft_core::document::OccurrenceRef>) {
        let mut d = Document::new();
        let first = d
            .add_cuboid("shared", Vec3::new(2., 3., 4.), Vec3::ZERO)
            .unwrap();
        let def = d.resolve_occurrence(&first).unwrap().definition.id;
        let mut paths = vec![first];
        for x in [5., 10.] {
            paths.push(
                place_definition(
                    &mut d,
                    &EditContext::Document,
                    def,
                    Vec3::X * x,
                    None,
                    false,
                )
                .unwrap(),
            );
        }
        (d, paths)
    }
    #[test]
    fn scene_plan_shares_effective_geometry_and_separates_modified_variants() {
        let (mut d, paths) = copies();
        let policy = SelectionDisplayPolicy::default();
        let source = plan(&d, &policy);
        assert_eq!(source.len(), 1);
        assert_eq!(source[0].placements.len(), 3);
        let mut replacement = d.effective_geometry(&paths[1]).unwrap().clone();
        replacement.id = GeometryBundleId(987_654);
        d.set_occurrence_edits(
            paths[1].clone(),
            InstanceEdits {
                geometry_replacement: Some(replacement.clone()),
                ..Default::default()
            },
        );
        assert_eq!(
            plan(&d, &policy).len(),
            1,
            "different bundle identities with identical display content share an upload"
        );
        replacement.vertices.values_mut().next().unwrap().position.y += 1.;
        d.set_occurrence_edits(
            paths[1].clone(),
            InstanceEdits {
                geometry_replacement: Some(replacement),
                ..Default::default()
            },
        );
        let changed = plan(&d, &policy);
        assert_eq!(changed.len(), 2);
        assert!(
            changed
                .iter()
                .any(|g| g.data == source[0].data && g.placements.len() == 2)
        );
        let mut selected = SelectionSession::default();
        selected.apply(
            [SelectionTarget {
                occurrence: paths[2].clone(),
                entity: SelectedEntity::Occurrence,
            }],
            SelectionMode::Replace,
        );
        transform_selection(
            &mut d,
            &mut selected,
            Affine3::from_translation(Vec3::Z * 7.),
            false,
        )
        .unwrap();
        let moved = plan(&d, &policy);
        assert_eq!(moved.len(), 2);
        assert!(moved.iter().any(|g| g.data == source[0].data));
        let transform = moved
            .iter()
            .flat_map(|g| &g.placements)
            .find(|p| p.target.occurrence == paths[2])
            .unwrap()
            .transform;
        assert_eq!(transform.translation.z, 7.);
    }
    #[test]
    fn visibility_and_wireframe_apply_without_redefining_materials_or_shared_sources() {
        let (d, _) = copies();
        let mut policy = SelectionDisplayPolicy::default();
        let base = plan(&d, &policy);
        assert!(!base[0].data.opaque.is_empty());
        policy.set_wireframe(SelectionKind::Object, true);
        let wire = plan(&d, &policy);
        assert!(wire[0].data.opaque.is_empty());
        assert!(
            wire[0].data.lines.len() > base[0].data.lines.len(),
            "wireframe adds the triangulation"
        );
        assert_eq!(wire.len(), 1);
        assert_eq!(
            wire[0].placements.len(),
            3,
            "wireframe geometry must be shared too"
        );
        policy.set_visible(SelectionKind::Object, false);
        assert!(plan(&d, &policy).is_empty());
    }
    #[test]
    fn indexed_mesh_chunks_preserve_large_geometry_winding_and_material_boundaries() {
        let mut corners = vec![];
        for i in 0..24_000 {
            for j in 0..3 {
                corners.push(SurfaceVertex {
                    position: [(i * 3 + j) as f32, j as f32, 0.],
                    color: [(i % 255) as u8, 50, 70, 255],
                });
            }
        }
        // Repeating a triangle shares vertices; a different material must not.
        corners.extend_from_within(0..3);
        let mut changed = [corners[0], corners[1], corners[2]];
        changed.iter_mut().for_each(|v| v.color = [1, 2, 3, 255]);
        corners.extend(changed);
        let chunks = surface_chunks(&corners);
        assert!(chunks.len() > 1);
        let restored = chunks
            .iter()
            .flat_map(|(vertices, indices)| {
                assert!(vertices.len() <= u16::MAX as usize);
                assert_eq!(indices.len() % 3, 0);
                indices.iter().map(|i| vertices[*i as usize])
            })
            .collect::<Vec<_>>();
        assert_eq!(corners, restored);
    }

    #[test]
    fn instance_rows_preserve_affine_positions_and_reflection_flags() {
        let transform = Affine3::from_linear_translation(
            [[-2., 0., 0.], [0., 3., 0.], [0., 0., 4.]],
            Vec3::new(5., 6., 7.),
        );
        let i = Instance::new(transform, true, true);
        let p = Vec3::new(2., 3., 4.);
        let output = i
            .rows
            .map(|r| r[0] * p.x as f32 + r[1] * p.y as f32 + r[2] * p.z as f32 + r[3]);
        assert_eq!(output, xyz(transform.transform_point(p)));
        assert_eq!(i.flags, [-1., 1., 1., 0.]);
    }

    #[test]
    #[ignore = "requires X11/OpenGL; validates persistent uploads and real shader output"]
    fn gpu_buffers_survive_navigation_selection_and_transform_only_edits() {
        let (mut rl, thread) = raylib::init()
            .size(640, 480)
            .hidden()
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut renderer = lighting::Renderer::new(&mut rl, &thread);
        assert_ne!(renderer.gpu.surface.id, unsafe {
            raylib::ffi::rlGetShaderIdDefault()
        });
        assert_ne!(renderer.gpu.line.id, unsafe {
            raylib::ffi::rlGetShaderIdDefault()
        });
        let (d, paths) = copies();
        let mut scene = RenderScene {
            document: Arc::new(d),
            grid_visible: false,
            ..Default::default()
        };
        scene.lighting.shadows = false;
        let mut camera = OrbitCamera::default();
        camera.fit(&scene.document);
        let mut target = rl.load_render_texture(&thread, 640, 480).unwrap();
        renderer.prepare(&mut rl, &thread, &scene);
        assert_eq!(renderer.gpu.stats.groups, 1);
        assert_eq!(renderer.gpu.stats.instances, 3);
        assert_eq!(
            renderer.gpu.stats.surface_vertices, 36,
            "three cubes must upload only one cube's triangles"
        );
        let vao = renderer.gpu.groups[0].opaque.as_ref().unwrap().chunks[0].vao;
        let uploads = renderer.gpu.stats.geometry_uploads;
        let placements = renderer.gpu.stats.instance_uploads;
        for i in 0..4 {
            camera.yaw += 0.2;
            renderer.prepare(&mut rl, &thread, &scene);
            draw_offscreen_scene(
                &mut rl,
                &thread,
                &mut target,
                camera,
                &scene,
                i,
                &mut renderer,
            );
            std::hint::black_box(copy_target_to_gpui_image(&target).unwrap());
            assert_eq!(renderer.gpu.stats.geometry_uploads, uploads);
            assert_eq!(renderer.gpu.stats.instance_uploads, placements);
        }
        // Reflection should preserve a symmetric solid's lighting/front-face tint.
        let original = Arc::clone(&scene.document);
        let baseline = copy_target_to_gpui_image(&target).unwrap();
        let probe = project_point(camera, (640., 480.), Vec3::new(1., 3., 2.)).unwrap();
        let px = (probe.0.round() as u32).min(639);
        let py = (probe.1.round() as u32).min(479);
        let mut mirrored = (*scene.document).clone();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: paths[0].clone(),
            entity: SelectedEntity::Occurrence,
        });
        transform_selection(
            &mut mirrored,
            &mut selection,
            Affine3::from_linear_translation(
                [[-1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
                Vec3::X * 2.,
            ),
            false,
        )
        .unwrap();
        scene.document = Arc::new(mirrored);
        renderer.prepare(&mut rl, &thread, &scene);
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            5,
            &mut renderer,
        );
        let reflected = copy_target_to_gpui_image(&target).unwrap();
        for channel in 0..3 {
            assert!(
                baseline.get_pixel(px, py)[channel].abs_diff(reflected.get_pixel(px, py)[channel])
                    < 6,
                "mirrored instances must preserve front-face tint"
            );
        }
        assert_eq!(uploads, renderer.gpu.stats.geometry_uploads);
        scene.document = original;
        scene.edit_context = EditContext::Occurrence(paths[0].clone());
        renderer.prepare(&mut rl, &thread, &scene);
        assert_eq!(
            renderer.gpu.groups[0]
                .instances
                .iter()
                .filter(|i| i.flags[1] > 0.5)
                .count(),
            2,
            "prototype isolation is per-instance GPU state"
        );
        assert_eq!(uploads, renderer.gpu.stats.geometry_uploads);
        scene.edit_context = EditContext::Document;
        scene.selection = vec![SelectionTarget {
            occurrence: paths[0].clone(),
            entity: SelectedEntity::Occurrence,
        }];
        renderer.prepare(&mut rl, &thread, &scene);
        assert_eq!(
            vao,
            renderer.gpu.groups[0].opaque.as_ref().unwrap().chunks[0].vao
        );
        assert_eq!(
            renderer.gpu.groups[0]
                .instances
                .iter()
                .filter(|i| i.flags[2] > 0.5)
                .count(),
            1
        );
        let after_selection = renderer.gpu.stats.geometry_uploads;
        let mut d = (*scene.document).clone();
        let mut selection = SelectionSession::default();
        selection.apply(scene.selection.clone(), SelectionMode::Replace);
        transform_selection(
            &mut d,
            &mut selection,
            Affine3::from_translation(Vec3::Z * 3.),
            false,
        )
        .unwrap();
        scene.document = Arc::new(d);
        renderer.prepare(&mut rl, &thread, &scene);
        assert_eq!(
            vao,
            renderer.gpu.groups[0].opaque.as_ref().unwrap().chunks[0].vao
        );
        assert_eq!(
            after_selection, renderer.gpu.stats.geometry_uploads,
            "moving instances reuploads transforms, not geometry"
        );
        let mut d = (*scene.document).clone();
        let mut variant = d.effective_geometry(&paths[1]).unwrap().clone();
        variant.vertices.values_mut().next().unwrap().position.y += 2.;
        d.set_occurrence_edits(
            paths[1].clone(),
            InstanceEdits {
                geometry_replacement: Some(variant),
                ..Default::default()
            },
        );
        scene.document = Arc::new(d);
        renderer.prepare(&mut rl, &thread, &scene);
        assert_eq!(renderer.gpu.stats.groups, 2);
        assert!(
            renderer
                .gpu
                .groups
                .iter()
                .any(|g| g.opaque.as_ref().unwrap().chunks[0].vao == vao && g.instances.len() == 2)
        );
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            10,
            &mut renderer,
        );
        let image = copy_target_to_gpui_image(&target).unwrap();
        assert!(
            image
                .pixels()
                .filter(|p| p[2] > 220 && p[1] > 90 && p[1] < 160 && p[0] < 80)
                .count()
                > 20,
            "selected instance lines must be orange"
        );
        // Camera modes and width/color preferences are uniforms, not mesh uploads.
        let uploads = renderer.gpu.stats.geometry_uploads;
        for orthographic in [false, true] {
            camera.orthographic = orthographic;
            scene.black_model_lines = !scene.black_model_lines;
            scene.normal_line_width = 4.;
            renderer.prepare(&mut rl, &thread, &scene);
            draw_offscreen_scene(
                &mut rl,
                &thread,
                &mut target,
                camera,
                &scene,
                11,
                &mut renderer,
            );
            assert_eq!(renderer.gpu.stats.geometry_uploads, uploads);
        }
        // A transparent occurrence inserted before an opaque one must still show it.
        let mut layered = Document::new();
        for (name, y, color) in [
            (
                "glass",
                2.5,
                Rgba8 {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 128,
                },
            ),
            (
                "solid",
                1.,
                Rgba8 {
                    red: 0,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                },
            ),
        ] {
            let path = layered
                .add_cuboid(name, Vec3::new(4., 0.1, 4.), Vec3::new(-2., y, -2.))
                .unwrap();
            let mut geometry = layered.effective_geometry(&path).unwrap().clone();
            for f in geometry.triangles.values_mut() {
                f.color = color;
                f.extensions.clear();
            }
            layered.set_occurrence_edits(
                path,
                InstanceEdits {
                    geometry_replacement: Some(geometry),
                    ..Default::default()
                },
            );
        }
        scene.document = Arc::new(layered);
        scene.selection.clear();
        camera.pitch = 1.2;
        camera.fit(&scene.document);
        renderer.prepare(&mut rl, &thread, &scene);
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            12,
            &mut renderer,
        );
        let layered = copy_target_to_gpui_image(&target).unwrap();
        let p = project_point(camera, (640., 480.), Vec3::new(0.7, 2.6, 0.4)).unwrap();
        let pixel = layered.get_pixel(p.0.round() as u32, p.1.round() as u32);
        assert!(
            pixel[2] > 80 && pixel[1] > 30,
            "glass must blend over the visible opaque object: {pixel:?}"
        );
        scene
            .display_policy
            .set_wireframe(SelectionKind::Object, true);
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(
            renderer
                .gpu
                .groups
                .iter()
                .all(|g| g.opaque.is_none() && g.glass.is_none())
        );
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            13,
            &mut renderer,
        );
        scene.document = Arc::new(Document::new());
        scene.selection.clear();
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(
            renderer.gpu.groups.is_empty(),
            "unreferenced GPU geometry is released"
        );
        println!("GPU geometry/instance upload counters and resource lifetime verified");
    }
    #[test]
    #[ignore = "requires X11/OpenGL; verifies line projection, width, clipping and selection"]
    fn gpu_lines_match_screen_projection_and_clip_at_the_near_plane() {
        let (mut rl, thread) = raylib::init()
            .size(640, 480)
            .hidden()
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut renderer = lighting::Renderer::new(&mut rl, &thread);
        let mut target = rl.load_render_texture(&thread, 640, 480).unwrap();
        let make_line = |ends: [Vec3; 2]| {
            let mut d = Document::new();
            let root = d.ensure_scene_root();
            let reference = |vertex| meshcraft_core::document::RawVertexRef {
                vertex,
                position_offset: Vec3::ZERO,
            };
            d.replace_definition_geometry_raw(
                root,
                ends.into_iter()
                    .map(|position| meshcraft_core::document::RawGeometryVertex {
                        position,
                        legacy_aliases: vec![],
                    })
                    .collect(),
                vec![meshcraft_core::document::RawGeometrySegment {
                    endpoints: [reference(0), reference(1)],
                    color: Rgba8 {
                        red: 255,
                        green: 0,
                        blue: 0,
                        alpha: 255,
                    },
                    extensions: BTreeMap::new(),
                    legacy_aliases: vec![],
                }],
                vec![],
            )
            .unwrap();
            d
        };
        let ends = [Vec3::new(-2., 2., 0.), Vec3::new(2., 2., 0.)];
        let mut scene = RenderScene {
            document: Arc::new(make_line(ends)),
            grid_visible: false,
            black_model_lines: false,
            normal_line_width: 4.,
            thick_line_width: 8.,
            ..Default::default()
        };
        scene.lighting.shadows = false;
        let mut camera = OrbitCamera::default();
        for orthographic in [false, true] {
            camera.orthographic = orthographic;
            renderer.prepare(&mut rl, &thread, &scene);
            draw_offscreen_scene(
                &mut rl,
                &thread,
                &mut target,
                camera,
                &scene,
                0,
                &mut renderer,
            );
            let pixels = copy_target_to_gpui_image(&target).unwrap();
            let red = pixels
                .enumerate_pixels()
                .filter(|(_, _, p)| p[2] > 220 && p[0] < 50 && p[1] < 50)
                .map(|(x, y, _)| (x as f32, y as f32))
                .collect::<Vec<_>>();
            assert!(red.len() > 40, "material-colored GPU line must be visible");
            let projected = ends.map(|p| project_point(camera, (640., 480.), p).unwrap());
            let length = (projected[0].0 - projected[1].0).hypot(projected[0].1 - projected[1].1);
            assert!(
                (red.len() as f32 / length - 4.).abs() < 1.5,
                "GPU line width must be four physical pixels in both projections"
            );
            for axis in [0, 1] {
                let coord = |(x, y): (f32, f32)| if axis == 0 { x } else { y };
                let lo = red.iter().copied().map(coord).fold(f32::INFINITY, f32::min);
                let hi = red
                    .iter()
                    .copied()
                    .map(coord)
                    .fold(f32::NEG_INFINITY, f32::max);
                let expected = projected.map(coord);
                assert!(
                    (lo - expected[0].min(expected[1])).abs() < 4.
                        && (hi - expected[0].max(expected[1])).abs() < 4.,
                    "GPU projection must match picking"
                );
            }
        }
        let uploads = renderer.gpu.stats.geometry_uploads;
        scene.black_model_lines = true;
        renderer.prepare(&mut rl, &thread, &scene);
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            1,
            &mut renderer,
        );
        let black = copy_target_to_gpui_image(&target).unwrap();
        assert!(
            black
                .pixels()
                .filter(|p| p[0] < 10 && p[1] < 10 && p[2] < 10)
                .count()
                > 40
        );
        assert_eq!(uploads, renderer.gpu.stats.geometry_uploads);
        scene.selection = scene
            .document
            .scene_root_definition()
            .unwrap()
            .geometry
            .segments
            .keys()
            .map(|id| SelectionTarget {
                occurrence: meshcraft_core::document::OccurrenceRef::scene_root(),
                entity: SelectedEntity::Segment(*id),
            })
            .collect();
        renderer.prepare(&mut rl, &thread, &scene);
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            2,
            &mut renderer,
        );
        let selected = copy_target_to_gpui_image(&target).unwrap();
        assert!(
            selected
                .pixels()
                .filter(|p| p[2] > 220 && p[1] > 100 && p[1] < 150 && p[0] < 80)
                .count()
                > 40,
            "partial primitive selection must use the orange shader overlay"
        );
        scene.selection.clear();
        scene.black_model_lines = false;
        camera.orthographic = false;
        let eye = camera.position();
        let forward = (camera.target - eye).try_normalized(1e-12).unwrap();
        let right = forward.cross(camera.up).try_normalized(1e-12).unwrap();
        for crossing in [true, false] {
            let a = eye - forward + right * (-0.5);
            let b = eye + forward * if crossing { 3. } else { -3. } + right * 0.5;
            scene.document = Arc::new(make_line([a, b]));
            renderer.prepare(&mut rl, &thread, &scene);
            draw_offscreen_scene(
                &mut rl,
                &thread,
                &mut target,
                camera,
                &scene,
                3,
                &mut renderer,
            );
            let frame = copy_target_to_gpui_image(&target).unwrap();
            let red = frame
                .pixels()
                .filter(|p| p[2] > 220 && p[0] < 50 && p[1] < 50)
                .count();
            if crossing {
                assert!(
                    red > 20,
                    "a line crossing the eye/near plane must be clipped, not discarded"
                );
            } else {
                assert_eq!(red, 0, "lines entirely behind the eye must be clipped away");
            }
        }
        // At long camera distances a constant NDC bias can pull a hidden line
        // through many model units of solid geometry. Test actual occlusion.
        camera.distance = 500.;
        camera.target = Vec3::new(0., 100., 0.);
        let eye = camera.position();
        let forward = (camera.target - eye).try_normalized(1e-12).unwrap();
        let right = forward.cross(camera.up).try_normalized(1e-12).unwrap();
        let up = right.cross(forward);
        for behind in [true, false] {
            let center = camera.target + forward * if behind { 2. } else { -2. };
            let mut d = make_line([center - right * 40., center + right * 40.]);
            let root = d.ensure_scene_root();
            let c = camera.target;
            d.append_colored_indexed_mesh_to_definition(
                root,
                vec![
                    c - right * 60. - up * 60.,
                    c + right * 60. - up * 60.,
                    c + right * 60. + up * 60.,
                    c - right * 60. + up * 60.,
                ],
                vec![],
                vec![([0, 1, 2], Rgba8::WHITE), ([0, 2, 3], Rgba8::WHITE)],
            )
            .unwrap();
            scene.document = Arc::new(d);
            renderer.prepare(&mut rl, &thread, &scene);
            draw_offscreen_scene(
                &mut rl,
                &thread,
                &mut target,
                camera,
                &scene,
                4,
                &mut renderer,
            );
            let frame = copy_target_to_gpui_image(&target).unwrap();
            let red = frame
                .pixels()
                .filter(|p| p[2] > 220 && p[0] < 50 && p[1] < 50)
                .count();
            if behind {
                assert_eq!(red, 0, "distant hidden edges must stay occluded");
            } else {
                assert!(
                    red > 40,
                    "a foreground line must remain visible at the same zoom"
                );
            }
        }
    }
}
