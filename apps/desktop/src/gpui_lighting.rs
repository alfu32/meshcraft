//! Retained/instanced model passes and a cached light-camera depth pass; helpers never cast shadows.
use super::*;
use raylib::prelude::{RaylibShader, RaylibShaderModeExt, Shader};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Lighting {
    pub ambient: f32,
    pub directional: f32,
    pub shadow_strength: f32,
    pub specular: f32,
    pub depth_bias: f32,
    pub normal_bias: f32,
    pub azimuth: f32,
    pub elevation: f32,
    pub shadows: bool,
}
impl Default for Lighting {
    fn default() -> Self {
        Self {
            ambient: 0.64,
            directional: 0.73,
            shadow_strength: 0.50,
            specular: 0.15,
            depth_bias: 0.00025,
            normal_bias: 0.037,
            azimuth: -35.0,
            elevation: 55.0,
            shadows: true,
        }
    }
}
impl Lighting {
    pub(super) fn direction(self) -> Vec3 {
        let (az, el) = (
            self.azimuth.to_radians() as f64,
            self.elevation.to_radians() as f64,
        );
        Vec3::new(az.sin() * el.cos(), el.sin(), az.cos() * el.cos())
    }
}
struct Face {
    points: [Vec3; 3],
    color: Rgba8,
    target: SelectionTarget,
    wireframe: bool,
    selected: bool,
}
fn face_outlines<'a>(faces: impl IntoIterator<Item = &'a Face>) -> Vec<([Vec3; 2], bool, Rgba8)> {
    let mut outlines =
        BTreeMap::<([u64; 3], [u64; 3]), ([Vec3; 2], usize, bool, bool, Rgba8)>::new();
    let key = |p: Vec3| [p.x, p.y, p.z].map(|v| if v == 0. { 0 } else { v.to_bits() });
    for face in faces {
        // Architecture publishes its boundary/reveal edges explicitly. Drawing
        // every tessellation edge exposes a grid around openings in wireframe.
        if matches!(
            face.target.entity,
            SelectedEntity::SemanticHandle {
                kind: meshcraft_core::selection::SelectionKind::Wall
                    | meshcraft_core::selection::SelectionKind::Slab
                    | meshcraft_core::selection::SelectionKind::Frame,
                ..
            }
        ) || (!face.selected && !face.wireframe)
        {
            continue;
        }
        for [a, b] in [
            [face.points[0], face.points[1]],
            [face.points[1], face.points[2]],
            [face.points[2], face.points[0]],
        ] {
            let (ka, kb) = (key(a), key(b));
            let edge = if ka <= kb { (ka, kb) } else { (kb, ka) };
            let entry = outlines
                .entry(edge)
                .or_insert(([a, b], 0, false, false, face.color));
            entry.1 += 1;
            entry.2 |= face.wireframe;
            entry.3 |= face.selected;
        }
    }
    outlines
        .into_values()
        .filter(|(_, count, wire, _, _)| *wire || *count == 1)
        .map(|(edge, _, _, selected, color)| (edge, selected, color))
        .collect()
}

struct Edge {
    color: Rgba8,
    points: [Vec3; 2],
    target: SelectionTarget,
    selected: bool,
}
struct Vertex {
    point: Vec3,
    target: SelectionTarget,
    selected: bool,
}

#[derive(Default)]
pub(super) struct SelectionIndex {
    exact: std::collections::BTreeSet<SelectionTarget>,
    objects: std::collections::BTreeSet<meshcraft_core::document::OccurrenceRef>,
}
impl SelectionIndex {
    fn new(selection: &[SelectionTarget]) -> Self {
        Self {
            exact: selection.iter().cloned().collect(),
            objects: selection
                .iter()
                .filter(|t| t.entity == SelectedEntity::Occurrence)
                .map(|t| t.occurrence.clone())
                .collect(),
        }
    }
    pub(super) fn exactly_contains(&self, target: &SelectionTarget) -> bool {
        self.exact.contains(target)
    }
    pub(super) fn contains(&self, target: &SelectionTarget) -> bool {
        if self.exact.contains(target) {
            return true;
        }
        let mut owner = target.occurrence.clone();
        loop {
            if self.objects.contains(&owner) {
                return true;
            }
            if owner.path.pop().is_none() {
                return false;
            }
        }
    }
}

fn selection_box(points: &[Vec3]) -> Vec<[Vec3; 2]> {
    let Some(first) = points.first().copied() else {
        return vec![];
    };
    let (mut lo, mut hi) = (first, first);
    for p in points {
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    let corners: [Vec3; 8] = std::array::from_fn(|i| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    });
    (0..8)
        .flat_map(|i| {
            [1, 2, 4]
                .into_iter()
                .filter(move |bit| i & bit == 0)
                .map(move |bit| [corners[i], corners[i | bit]])
        })
        .collect()
}

pub(super) struct Renderer {
    pub(super) placement: placement::Renderer,
    pub(super) gpu: gpu_mesh::Scene,
    depth: Shader,
    lit: Shader,
    map: RenderTexture2D,
    document: Option<Arc<Document>>,
    policy: meshcraft_core::selection::SelectionDisplayPolicy,
    faces: Vec<Face>,
    edges: Vec<Edge>,
    vertices: Vec<Vertex>,
    pub(super) handles: Vec<hotspots::Handle>,
    center: Vec3,
    radius: f64,
    last_light: Option<(f32, f32)>,
    last_selection: Vec<SelectionTarget>,
    pub(super) selection: SelectionIndex,
    outlines: Vec<([Vec3; 2], bool, Rgba8)>,
    selection_box: Vec<[Vec3; 2]>,
    direction_edges: Vec<[Vec3; 2]>,
    selected_points: Vec<Vec3>,
    handle_context: Option<EditContext>,
}
impl Renderer {
    pub fn new(rl: &mut raylib::RaylibHandle, thread: &raylib::RaylibThread) -> Self {
        let vertex = include_str!("../../../assets/shaders/scene.vs");
        let depth = rl.load_shader_from_memory(
            thread,
            Some(include_str!("../../../assets/shaders/mesh.vs")),
            Some(include_str!("../../../assets/shaders/shadow.fs")),
        );
        let lit = rl.load_shader_from_memory(
            thread,
            Some(vertex),
            Some(include_str!("../../../assets/shaders/scene.fs")),
        );
        Self {
            placement: placement::Renderer::new(rl, thread),
            gpu: gpu_mesh::Scene::new(rl, thread),
            depth,
            lit,
            map: rl
                .load_render_texture(thread, 2048, 2048)
                .expect("shadow target"),
            document: None,
            policy: meshcraft_core::selection::SelectionDisplayPolicy::default(),
            faces: Vec::new(),
            edges: Vec::new(),
            vertices: Vec::new(),
            handles: Vec::new(),
            center: Vec3::ZERO,
            radius: 10.0,
            last_light: None,
            last_selection: Vec::new(),
            selection: SelectionIndex::default(),
            outlines: Vec::new(),
            selection_box: Vec::new(),
            direction_edges: vec![],
            selected_points: vec![],
            handle_context: None,
        }
    }
    pub fn prepare(
        &mut self,
        rl: &mut raylib::RaylibHandle,
        thread: &raylib::RaylibThread,
        scene: &RenderScene,
    ) {
        self.placement.prepare(thread, scene.preview.as_ref());
        let changed = self
            .document
            .as_ref()
            .is_none_or(|doc| !Arc::ptr_eq(doc, &scene.document))
            || self.policy != scene.display_policy;
        if changed {
            self.rebuild(&scene.document, &scene.display_policy);
            self.last_light = None;
        }
        let feedback_changed = changed
            || self.last_selection != scene.selection
            || self.handle_context.as_ref() != Some(&scene.edit_context);
        if feedback_changed {
            self.update_selection(&scene.selection);
            self.handles =
                hotspots::handles(&scene.document, &scene.edit_context, &scene.selection);
            self.handles.retain(|h| {
                scene.display_policy.can_draw_target(
                    &scene.document,
                    &SelectionTarget {
                        occurrence: h.occurrence.clone(),
                        entity: SelectedEntity::SemanticHandle {
                            kind: meshcraft_core::selection::SelectionKind::Hotspot,
                            id: h.hotspot.id.clone(),
                        },
                    },
                )
            });
            self.handle_context = Some(scene.edit_context.clone());
        }
        let partial = if feedback_changed {
            self.edges
                .iter()
                .filter(|e| {
                    e.selected
                        && matches!(e.target.entity, SelectedEntity::Segment(_))
                        && !self.selection.contains(&SelectionTarget {
                            occurrence: e.target.occurrence.clone(),
                            entity: SelectedEntity::Occurrence,
                        })
                })
                .map(|e| (e.points, e.color))
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        if changed {
            self.gpu.procedural.prepare(rl, thread, scene);
        }
        self.gpu.procedural.tick();
        if changed && let Some(b) = self.gpu.procedural.bounds() {
            if self.vertices.is_empty() {
                self.center = (b.min + b.max) * 0.5;
                self.radius = 6.;
            }
            self.radius = self
                .radius
                .max(self.center.distance(b.min))
                .max(self.center.distance(b.max));
        }
        self.gpu.prepare(
            scene,
            &self.selection,
            changed,
            feedback_changed.then_some(gpu_mesh::Feedback {
                outlines: &self.outlines,
                partial: &partial,
                bounds: &self.selection_box,
            }),
        );
        let direction = scene.lighting.direction();
        let light = (scene.lighting.azimuth, scene.lighting.elevation);
        if !scene.lighting.shadows
            || (!changed && self.last_light == Some(light) && !self.gpu.procedural.animated)
        {
            return;
        }
        self.last_light = Some(light);
        let camera = Camera3D::orthographic(
            ray_vec3(self.center + direction * self.radius * 2.0),
            ray_vec3(self.center),
            RayVector3::new(0., 1., 0.),
            (self.radius * 2.6) as f32,
        );
        let light_vp;
        let (old_near, old_far) = unsafe {
            (
                raylib::ffi::rlGetCullDistanceNear(),
                raylib::ffi::rlGetCullDistanceFar(),
            )
        };
        unsafe {
            raylib::ffi::rlSetClipPlanes((self.radius * 0.5).max(0.01), self.radius * 3.5);
        }
        {
            let mut drawing = rl.begin_texture_mode(thread, &mut self.map);
            drawing.clear_background(Color::WHITE);
            let _view = drawing.begin_mode3D(camera);
            // Both sides cast, including open planar meshes. No guides/lines/preview here.
            unsafe {
                raylib::ffi::rlDisableBackfaceCulling();
                light_vp = raylib::ffi::MatrixMultiply(
                    raylib::ffi::rlGetMatrixModelview(),
                    raylib::ffi::rlGetMatrixProjection(),
                );
            }
            self.gpu.draw_shadow(&mut self.depth);
            self.gpu.procedural.draw(scene, None, true, 0, false);
            unsafe {
                raylib::ffi::rlEnableBackfaceCulling();
            }
        }
        unsafe {
            raylib::ffi::rlSetClipPlanes(old_near, old_far);
        }
        self.lit
            .set_shader_value_matrix(self.lit.get_shader_location("lightVP"), light_vp);
        self.gpu.set_light_matrix(light_vp);
    }
    fn rebuild(
        &mut self,
        document: &Arc<Document>,
        policy: &meshcraft_core::selection::SelectionDisplayPolicy,
    ) {
        self.policy = policy.clone();
        self.faces.clear();
        self.edges.clear();
        self.vertices.clear();
        let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = -min;
        let material_colors = document.material_colors();
        let material_color = |extensions: &BTreeMap<String, serde_json::Value>, fallback| {
            meshcraft_core::document::materials::material_id(extensions)
                .and_then(|id| material_colors.get(id))
                .copied()
                .unwrap_or(fallback)
        };
        for occurrence in document.scene_geometry_refs().unwrap_or_default() {
            let (Some(resolved), Some(geometry)) = (
                document.resolve_occurrence(&occurrence),
                document.effective_geometry(&occurrence),
            ) else {
                continue;
            };
            let world = |id| {
                geometry
                    .vertex_position(id)
                    .map(|p| resolved.world_transform.transform_point(p))
            };
            for face in geometry.triangles.values() {
                if let [Some(a), Some(b), Some(c)] = face.vertices.map(world) {
                    self.faces.push(Face {
                        points: if resolved.world_transform.determinant() < 0. {
                            [a, c, b]
                        } else {
                            [a, b, c]
                        },
                        color: material_color(&face.extensions, face.color),
                        wireframe: false,
                        selected: false,
                        target: SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: meshcraft_core::document::architecture::semantic_entity(
                                &face.extensions,
                            )
                            .unwrap_or(SelectedEntity::Triangle(face.id)),
                        },
                    });
                }
            }
            for edge in geometry.segments.values() {
                if let [Some(a), Some(b)] = edge.endpoints.map(world) {
                    self.edges.push(Edge {
                        color: material_color(&edge.extensions, edge.color),
                        points: [a, b],
                        selected: false,
                        target: SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: meshcraft_core::document::architecture::semantic_entity(
                                &edge.extensions,
                            )
                            .unwrap_or(SelectedEntity::Segment(edge.id)),
                        },
                    });
                }
            }
            for vertex in geometry.vertices.values() {
                let point = resolved.world_transform.transform_point(vertex.position);
                min = Vec3::new(min.x.min(point.x), min.y.min(point.y), min.z.min(point.z));
                max = Vec3::new(max.x.max(point.x), max.y.max(point.y), max.z.max(point.z));
                self.vertices.push(Vertex {
                    point,
                    selected: false,
                    target: SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::Vertex(vertex.id),
                    },
                });
            }
        }
        if !self.vertices.is_empty() {
            min.y = min.y.min(0.0);
            max.y = max.y.max(0.0);
            self.center = (min + max) * 0.5;
            self.radius = ((max - min).length() * 0.5 + 2.0).max(6.0);
        } else {
            self.center = Vec3::ZERO;
            self.radius = 10.0;
        }
        self.faces
            .retain(|face| policy.can_draw_target(document, &face.target));
        for face in &mut self.faces {
            face.wireframe = policy.target_wireframe(document, &face.target);
        }
        self.edges
            .retain(|edge| policy.can_draw_target(document, &edge.target));
        self.vertices
            .retain(|vertex| policy.can_draw_target(document, &vertex.target));
        self.handles.retain(|handle| {
            policy.can_draw_target(
                document,
                &SelectionTarget {
                    occurrence: handle.occurrence.clone(),
                    entity: SelectedEntity::SemanticHandle {
                        kind: meshcraft_core::selection::SelectionKind::Hotspot,
                        id: handle.hotspot.id.clone(),
                    },
                },
            )
        });
        self.document = Some(Arc::clone(document));
    }
    fn update_selection(&mut self, targets: &[SelectionTarget]) {
        self.selection = SelectionIndex::new(targets);
        self.last_selection = targets.to_vec();
        for face in &mut self.faces {
            face.selected = self.selection.contains(&face.target);
        }
        // Whole-object wireframes live in the shared local-space GPU line mesh.
        // Only explicitly selected faces need an additional boundary overlay.
        self.outlines = face_outlines(
            self.faces
                .iter()
                .filter(|f| !f.wireframe || self.selection.exact.contains(&f.target)),
        );
        for edge in &mut self.edges {
            edge.selected = self.selection.contains(&edge.target);
        }
        let mut points = Vec::new();
        points.extend(
            self.faces
                .iter()
                .filter(|f| f.selected)
                .flat_map(|f| f.points),
        );
        points.extend(
            self.edges
                .iter()
                .filter(|e| e.selected)
                .flat_map(|e| e.points),
        );
        for vertex in &mut self.vertices {
            // A selected object must not create a sphere draw call at every mesh vertex.
            vertex.selected = self.selection.exact.contains(&vertex.target);
            if vertex.selected {
                points.push(vertex.point);
            }
        }
        if let Some(document) = &self.document {
            for target in targets {
                if matches!(target.entity, SelectedEntity::Occurrence)
                    && let Some(b) = document.procedural_object_world_bounds(&target.occurrence)
                {
                    points.extend(meshcraft_core::document::procedural::bounds_corners(b));
                }
            }
        }
        self.selection_box = selection_box(&points);
        self.direction_edges = self
            .edges
            .iter()
            .filter(|e| {
                matches!(e.target.entity, SelectedEntity::Segment(_))
                    && self.selection.exact.contains(&e.target)
            })
            .map(|e| e.points)
            .collect();
        self.selected_points = self
            .vertices
            .iter()
            .filter(|v| v.selected)
            .map(|v| v.point)
            .collect();
    }

    pub fn draw(
        &mut self,
        view: &mut (impl RaylibDraw3D + RaylibDraw),
        scene: &RenderScene,
        camera: OrbitCamera,
        size: (f32, f32),
    ) {
        let height = size.1;
        let eye = camera.position();
        let settings = scene.lighting;
        let shader = &mut self.lit;
        shader.set_shader_value(
            shader.get_shader_location("towardLight"),
            ray_vec3(settings.direction()),
        );
        shader.set_shader_value(shader.get_shader_location("eye"), ray_vec3(eye));
        for (key, value) in [
            ("depthOffset", 0.00015),
            ("ambient", settings.ambient),
            ("directional", settings.directional),
            ("shadowStrength", settings.shadow_strength),
            ("specular", settings.specular),
            ("depthBias", settings.depth_bias),
            ("normalBias", settings.normal_bias),
        ] {
            shader.set_shader_value(shader.get_shader_location(key), value);
        }
        shader.set_shader_value(
            shader.get_shader_location("shadows"),
            i32::from(settings.shadows),
        );
        shader.set_shader_value(shader.get_shader_location("shadowMap"), 1i32);
        // Reserve a sampler outside raylib's default texture slot. Keep it bound across batch flushes.
        unsafe {
            raylib::ffi::rlActiveTextureSlot(1);
            raylib::ffi::rlEnableTexture(self.map.texture().id);
            raylib::ffi::rlActiveTextureSlot(0);
            raylib::ffi::rlEnableBackfaceCulling();
        }
        {
            let mut pass = view.begin_shader_mode(shader);
            let extent = (scene.grid_extent as f64 * scene.grid_spacing).max(self.radius * 2.0);
            let [a, b, c, d] = [
                Vec3::new(-extent, -0.01, -extent),
                Vec3::new(-extent, -0.01, extent),
                Vec3::new(extent, -0.01, extent),
                Vec3::new(extent, -0.01, -extent),
            ];
            triangle(&mut pass, [a, b, c], Color::new(188, 198, 208, 255));
            triangle(&mut pass, [a, c, d], Color::new(188, 198, 208, 255));
            // The ground is a top-facing receiver; authored open meshes remain two-sided.
            unsafe {
                raylib::ffi::rlDrawRenderBatchActive();
                raylib::ffi::rlDisableBackfaceCulling();
            }
        }
        self.gpu.draw_surfaces(scene, camera, self.map.texture().id);
        self.gpu.draw_lines(scene, size);
        // Draw direction feedback together above the surface, including coplanar model edges.
        unsafe {
            raylib::ffi::rlDrawRenderBatchActive();
            raylib::ffi::rlDisableDepthTest();
        }
        for edge in &self.direction_edges {
            direction_arrow(view, *edge, scene.thick_line_width, camera, height);
        }
        unsafe {
            raylib::ffi::rlDrawRenderBatchActive();
            raylib::ffi::rlEnableDepthTest();
            raylib::ffi::rlEnableBackfaceCulling();
        }
        for point in &self.selected_points {
            view.draw_sphere(ray_vec3(*point), 0.06, Color::new(227, 57, 49, 255));
        }
    }
}
/// Pixel-width ribbons keep model line widths portable across OpenGL implementations.
pub(super) fn stroke(
    view: &mut impl RaylibDraw3D,
    [a, b]: [Vec3; 2],
    color: Color,
    width: f32,
    camera: OrbitCamera,
    height: f32,
) {
    let size = (height * 2., height);
    let (Some(pa), Some(pb)) = (
        project_point(camera, size, a),
        project_point(camera, size, b),
    ) else {
        return;
    };
    let (dx, dy) = (f64::from(pb.0 - pa.0), f64::from(pb.1 - pa.1));
    let length = dx.hypot(dy);
    if length < 1.0e-6 {
        return;
    }
    let eye = camera.position();
    let forward = (camera.target - eye)
        .try_normalized(1.0e-12)
        .unwrap_or(Vec3::Z);
    let right = forward
        .cross(camera.up)
        .try_normalized(1.0e-12)
        .unwrap_or(Vec3::X);
    let up = right.cross(forward);
    let side = (right * (-dy) - up * dx) / length;
    let factor = f64::from(width) * (f64::from(camera.field_of_view).to_radians() * 0.5).tan()
        / f64::from(height.max(1.));
    let offset_a = side
        * (if camera.orthographic {
            f64::from(camera.distance)
        } else {
            (a - eye).dot(forward)
        } * factor);
    let offset_b = side
        * (if camera.orthographic {
            f64::from(camera.distance)
        } else {
            (b - eye).dot(forward)
        } * factor);
    triangle(view, [a - offset_a, b - offset_b, b + offset_b], color);
    triangle(view, [a - offset_a, b + offset_b, a + offset_a], color);
}

/// A constant-pixel arrowhead on each explicitly selected segment, following endpoint order.
fn direction_arrow(
    view: &mut impl RaylibDraw3D,
    [a, b]: [Vec3; 2],
    width: f32,
    camera: OrbitCamera,
    height: f32,
) {
    let (Some(pa), Some(pb)) = (
        project_point(camera, (height * 2., height), a),
        project_point(camera, (height * 2., height), b),
    ) else {
        return;
    };
    let length = f64::from((pb.0 - pa.0).hypot(pb.1 - pa.1));
    if length < 8. {
        return;
    }
    let midpoint = (a + b) * 0.5;
    let eye = camera.position();
    let forward = (camera.target - eye)
        .try_normalized(1e-12)
        .unwrap_or(-Vec3::Z);
    let right = forward
        .cross(camera.up)
        .try_normalized(1e-12)
        .unwrap_or(Vec3::X);
    let up = right.cross(forward);
    let depth = if camera.orthographic {
        f64::from(camera.distance)
    } else {
        (midpoint - eye).dot(forward)
    };
    let units = 2. * depth * (f64::from(camera.field_of_view).to_radians() * 0.5).tan()
        / f64::from(height.max(1.));
    let along = (b - a) / length;
    let perpendicular = (right * -f64::from(pb.1 - pa.1) - up * f64::from(pb.0 - pa.0)) / length;
    let arrow = 10_f64.min(length * 0.35);
    let tip = midpoint + along * (arrow * 0.5) - forward * (units * 0.1);
    let back = tip - along * arrow;
    for side in [-1., 1.] {
        stroke(
            view,
            [back + perpendicular * (side * arrow * units * 0.45), tip],
            Color::new(190, 55, 20, 255),
            width.max(2.),
            camera,
            height,
        );
    }
}

fn triangle(view: &mut impl RaylibDraw3D, [a, b, c]: [Vec3; 3], color: Color) {
    view.draw_triangle3D(ray_vec3(a), ray_vec3(b), ray_vec3(c), color);
}

pub(super) fn external(
    context: &EditContext,
    document: &Document,
    target: &SelectionTarget,
) -> bool {
    match context {
        EditContext::Document => false,
        EditContext::Occurrence(active) => !target.occurrence.path.starts_with(&active.path),
        EditContext::Definition(id) => document
            .resolve_occurrence(&target.occurrence)
            .is_none_or(|resolved| resolved.definition.id != *id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn architecture_wireframe_uses_explicit_edges_without_exposing_tessellation() {
        use meshcraft_core::{document::OccurrenceRef, selection::SelectionKind};
        let wall = meshcraft_core::document::architecture::Wall {
            end: Vec3::X * 6.,
            height: 4.,
            holes: vec![meshcraft_core::document::architecture::Hole {
                u0: 2.,
                u1: 4.,
                v0: 1.,
                v1: 3.,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mesh = meshcraft_core::document::architecture::Element::Wall(wall)
            .mesh()
            .unwrap();
        let mut faces = mesh
            .faces
            .iter()
            .map(|(t, color)| Face {
                points: t.map(|i| mesh.points[i]),
                color: *color,
                selected: true,
                wireframe: true,
                target: SelectionTarget {
                    occurrence: OccurrenceRef::scene_root(),
                    entity: SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Wall,
                        id: "wall".into(),
                    },
                },
            })
            .collect::<Vec<_>>();
        assert!(face_outlines(&faces).is_empty());
        assert!(
            !mesh.edges.is_empty(),
            "owner provides explicit boundary and reveal segments"
        );
        // Ordinary mesh wireframes still expose their triangulation.
        faces.truncate(1);
        faces[0].target.entity = SelectedEntity::Occurrence;
        assert_eq!(face_outlines(&faces).len(), 3);
    }
    #[test]
    fn lighting_defaults_match_the_requested_shader_preset() {
        let l = Lighting::default();
        assert_eq!((l.azimuth, l.elevation), (-35., 55.));
        assert_eq!((l.directional, l.ambient, l.specular), (0.73, 0.64, 0.15));
        assert_eq!(
            (l.shadow_strength, l.depth_bias, l.normal_bias),
            (0.5, 0.00025, 0.037)
        );
    }

    #[test]
    #[ignore = "requires a live X11/OpenGL display; run explicitly with --ignored --test-threads=1"]
    fn gpu_light_camera_casts_shadows_and_black_lines_survive_composition() {
        let (mut rl, thread) = raylib::init()
            .size(640, 480)
            .hidden()
            .log_level(TraceLogLevel::LOG_WARNING)
            .title("Meshcraft lighting validation")
            .build();
        let mut renderer = Renderer::new(&mut rl, &thread);
        assert_ne!(
            renderer.lit.id,
            unsafe { raylib::ffi::rlGetShaderIdDefault() },
            "lighting shader failed to compile/link"
        );
        assert_ne!(
            renderer.depth.id,
            unsafe { raylib::ffi::rlGetShaderIdDefault() },
            "depth shader failed to compile/link"
        );
        let mut document = Document::new();
        document
            .add_cuboid(
                "receiver",
                Vec3::new(6.0, 0.5, 6.0),
                Vec3::new(-3.0, 0.0, -3.0),
            )
            .unwrap();
        document
            .add_cuboid("caster", Vec3::new(1.5, 2.0, 1.5), Vec3::new(0.0, 0.5, 0.0))
            .unwrap();
        document
            .add_cuboid(
                "ground caster",
                Vec3::new(1.0, 1.5, 1.0),
                Vec3::new(2.0, 1.0, -4.0),
            )
            .unwrap();
        document
            .add_line_segment(
                "authored line",
                Vec3::new(-3.0, 1.0, -1.0),
                Vec3::new(-3.0, 1.0, 3.0),
            )
            .unwrap();
        let mut scene = RenderScene {
            document: Arc::new(document),
            grid_visible: false,
            ..Default::default()
        };
        let camera = OrbitCamera {
            target: Vec3::new(0.0, 0.7, 0.0),
            ..Default::default()
        };
        let mut target = rl.load_render_texture(&thread, 640, 480).unwrap();
        renderer.prepare(&mut rl, &thread, &scene);
        super::super::draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            0,
            &mut renderer,
        );
        let shadowed = super::super::copy_target_to_gpui_image(&target).unwrap();
        scene.lighting.shadows = false;
        renderer.prepare(&mut rl, &thread, &scene);
        super::super::draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            0,
            &mut renderer,
        );
        let unshadowed = super::super::copy_target_to_gpui_image(&target).unwrap();
        let boxes = [
            (Vec3::new(-3., 0., -3.), Vec3::new(3., 0.5, 3.)),
            (Vec3::new(0., 0.5, 0.), Vec3::new(1.5, 2.5, 1.5)),
            (Vec3::new(2., 1., -4.), Vec3::new(3., 2.5, -3.)),
        ];
        let hits_box = |origin: Vec3, direction: Vec3, min: Vec3, max: Vec3| -> Option<f64> {
            let mut near = 0.0_f64;
            let mut far = f64::INFINITY;
            for (p, d, a, b) in [
                (origin.x, direction.x, min.x, max.x),
                (origin.y, direction.y, min.y, max.y),
                (origin.z, direction.z, min.z, max.z),
            ] {
                if d.abs() < 1.0e-12 {
                    if p < a || p > b {
                        return None;
                    } else {
                        continue;
                    }
                }
                let (a, b) = ((a - p) / d, (b - p) / d);
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
            Some(near)
        };
        let mut intersection = 0;
        let mut union = 0;
        for y in (30..480).step_by(2) {
            for x in (0..640).step_by(2) {
                let (origin, ray) = camera
                    .ray(x as f32 + 0.5, y as f32 + 0.5, 640., 480.)
                    .unwrap();
                if ray.y >= -1.0e-9 {
                    continue;
                }
                let depth = (-0.01 - origin.y) / ray.y;
                if boxes
                    .iter()
                    .any(|(a, b)| hits_box(origin, ray, *a, *b).is_some_and(|d| d < depth))
                {
                    continue;
                }
                let point = origin + ray * depth;
                let truth = boxes
                    .iter()
                    .any(|(a, b)| hits_box(point, scene.lighting.direction(), *a, *b).is_some());
                let measured = i16::from(unshadowed.get_pixel(x, y)[1])
                    - i16::from(shadowed.get_pixel(x, y)[1])
                    > 8;
                if truth || measured {
                    union += 1;
                }
                if truth && measured {
                    intersection += 1;
                }
            }
        }
        let iou = intersection as f64 / union as f64;
        assert!(
            iou > 0.90,
            "ground shadow must align with analytic ray/box projection, IoU {iou:.3}"
        );
        println!("analytic ground shadow alignment IoU: {iou:.3}");

        let thin = unshadowed
            .pixels()
            .filter(|p| p[0] < 8 && p[1] < 8 && p[2] < 8)
            .count();
        scene.normal_line_width = 3.0;
        super::super::draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            0,
            &mut renderer,
        );
        let thick = super::super::copy_target_to_gpui_image(&target).unwrap();
        let thick_pixels = thick
            .pixels()
            .filter(|p| p[0] < 8 && p[1] < 8 && p[2] < 8)
            .count();
        assert!(
            thick_pixels > thin * 3 / 2,
            "model line width must affect actual pixels: {thin} vs {thick_pixels}"
        );
        let mut resolve = super::super::postprocess::Resolve::new(&mut rl, &thread);
        assert!(
            resolve.shader.get_shader_location("span") >= 0,
            "FXAA shader must compile"
        );
        target.texture_mut().set_texture_filter(
            &thread,
            raylib::consts::TextureFilter::TEXTURE_FILTER_BILINEAR,
        );
        let mut antialiased = rl.load_render_texture(&thread, 640, 480).unwrap();
        for quality in [
            postprocess::Antialiasing::Low,
            postprocess::Antialiasing::Medium,
            postprocess::Antialiasing::High,
        ] {
            resolve.draw(&mut rl, &thread, &target, &mut antialiased, quality);
            let image = super::super::copy_target_to_gpui_image(&antialiased).unwrap();
            let changed = image
                .pixels()
                .zip(thick.pixels())
                .filter(|(a, b)| (i16::from(a[1]) - i16::from(b[1])).abs() > 2)
                .count();
            let mean_error = image
                .pixels()
                .zip(thick.pixels())
                .map(|(a, b)| (f64::from(a[1]) - f64::from(b[1])).abs())
                .sum::<f64>()
                / (640. * 480.);
            assert!(
                mean_error < 5.,
                "FXAA must preserve orientation and flat shading: {mean_error}"
            );
            assert!(
                changed > 100,
                "shader antialiasing must smooth edges at {} quality, {changed} pixels",
                quality.label()
            );
        }
        let darkened = shadowed
            .pixels()
            .zip(unshadowed.pixels())
            .filter(|(a, b)| i16::from(b[1]) - i16::from(a[1]) > 8)
            .count();
        let mut ground_pixels = 0;
        let mut entity_pixels = 0;
        for (x, y, a) in shadowed.enumerate_pixels() {
            if i16::from(unshadowed.get_pixel(x, y)[1]) - i16::from(a[1]) <= 8 {
                continue;
            }
            let (origin, ray) = camera
                .ray(x as f32 + 0.5, y as f32 + 0.5, 640.0, 480.0)
                .unwrap();
            let hit = origin + ray * ((0.5 - origin.y) / ray.y);
            if (-3.0..=3.0).contains(&hit.x) && (-3.0..=3.0).contains(&hit.z) {
                entity_pixels += 1;
            } else {
                ground_pixels += 1;
            }
        }
        assert!(
            entity_pixels > 100,
            "no entity receiver shadow: {entity_pixels}"
        );
        assert!(
            ground_pixels > 30,
            "no ground receiver shadow: {ground_pixels}"
        );
        assert!(
            darkened > 100,
            "shadow map must darken ground/entity receivers: {darkened} pixels"
        );
        assert!(
            shadowed
                .pixels()
                .filter(|p| p[0] < 8 && p[1] < 8 && p[2] < 8)
                .count()
                > 40,
            "authored black line missing"
        );
        // Policy-only edits must invalidate the draw and shadow caches without cloning the model.
        let document = Arc::clone(&scene.document);
        let face_count = renderer.faces.len();
        scene
            .display_policy
            .set_visible(meshcraft_core::selection::SelectionKind::Face, false);
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(renderer.faces.is_empty());
        assert!(!renderer.edges.is_empty());
        assert!(Arc::ptr_eq(&document, &scene.document));
        scene
            .display_policy
            .set_visible(meshcraft_core::selection::SelectionKind::Face, true);
        scene
            .display_policy
            .set_wireframe(meshcraft_core::selection::SelectionKind::Object, true);
        renderer.prepare(&mut rl, &thread, &scene);
        assert_eq!(face_count, renderer.faces.len());
        assert!(renderer.faces.iter().all(|face| face.wireframe));
        scene
            .display_policy
            .set_visible(meshcraft_core::selection::SelectionKind::Object, false);
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(renderer.faces.is_empty());
        assert!(renderer.edges.is_empty());
        assert!(renderer.handles.is_empty());
        scene.display_policy = Default::default();
        scene.selection = scene
            .document
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .map(|occurrence| SelectionTarget {
                occurrence,
                entity: SelectedEntity::Occurrence,
            })
            .collect();
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(renderer.faces.iter().all(|f| f.selected));
        assert!(
            renderer.vertices.iter().all(|v| !v.selected),
            "object highlighting must not emit a marker per vertex"
        );
        assert!(
            renderer.outlines.is_empty(),
            "closed fully selected meshes must not redraw every triangle edge"
        );

        if let Ok(path) = std::env::var("MESHCRAFT_BENCH_PROJECT") {
            let document = meshcraft_io::project::read_project(&std::fs::read(path).unwrap())
                .unwrap()
                .document;
            let mut bench = RenderScene {
                document: Arc::new(document),
                grid_visible: false,
                ..Default::default()
            };
            bench.lighting.shadows = false;
            let mut camera = OrbitCamera::default();
            camera.fit(&bench.document);
            let mut target = rl.load_render_texture(&thread, 1280, 800).unwrap();
            renderer.prepare(&mut rl, &thread, &bench);
            println!("dense model retained GPU cache {:?}", renderer.gpu.stats);
            let all = renderer
                .faces
                .iter()
                .map(|f| f.target.clone())
                .chain(renderer.edges.iter().map(|e| e.target.clone()))
                .collect::<Vec<_>>();
            for (label, selection) in [("none", Vec::new()), ("all faces and edges", all)] {
                bench.selection = selection;
                renderer.prepare(&mut rl, &thread, &bench);
                super::super::draw_offscreen_scene(
                    &mut rl,
                    &thread,
                    &mut target,
                    camera,
                    &bench,
                    0,
                    &mut renderer,
                );
                let start = std::time::Instant::now();
                for _ in 0..6 {
                    renderer.prepare(&mut rl, &thread, &bench);
                    super::super::draw_offscreen_scene(
                        &mut rl,
                        &thread,
                        &mut target,
                        camera,
                        &bench,
                        0,
                        &mut renderer,
                    );
                    std::hint::black_box(super::super::copy_target_to_gpui_image(&target).unwrap());
                }
                println!(
                    "selection frame probe at 1280x800: {label}, {} targets, {:.2} ms/frame (software GPU including readback)",
                    bench.selection.len(),
                    start.elapsed().as_secs_f64() * 1000. / 6.
                );
            }
        }
        // Keep a directly inspectable artifact, converting GPUI BGRA back to PNG RGBA.
        let mut rgba = shadowed;
        for p in rgba.pixels_mut() {
            p.0.swap(0, 2);
        }
        rgba.save("/tmp/meshcraft-lighting-probe.png").unwrap();
        println!(
            "shadowed receiver pixels: {darkened} (ground {ground_pixels}, entity {entity_pixels}); image: /tmp/meshcraft-lighting-probe.png"
        );
    }
    #[test]
    #[ignore = "manual GPU throughput probe; requires an isolated X11/OpenGL display"]
    fn retained_geometry_throughput_probe() {
        use meshcraft_core::document::{
            DefinitionKind, RawGeometrySegment, RawGeometryTriangle, RawGeometryVertex,
            RawVertexRef,
        };
        let (mut rl, thread) = raylib::init()
            .size(960, 640)
            .hidden()
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut renderer = Renderer::new(&mut rl, &thread);
        let n: usize = std::env::var("MESHCRAFT_GPU_GRID")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        let copies: usize = std::env::var("MESHCRAFT_GPU_COPIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);
        let mut d = Document::new();
        let def = d.add_definition("shared grid", DefinitionKind::Mesh);
        let vertices = (0..=n)
            .flat_map(|z| {
                (0..=n).map(move |x| RawGeometryVertex {
                    position: Vec3::new(x as f64, 2., z as f64),
                    legacy_aliases: vec![],
                })
            })
            .collect();
        let reference = |vertex| RawVertexRef {
            vertex,
            position_offset: Vec3::ZERO,
        };
        let mut faces = vec![];
        for z in 0..n {
            for x in 0..n {
                let a = z * (n + 1) + x;
                let b = a + 1;
                let c = a + n + 1;
                let e = c + 1;
                for tri in [[a, c, b], [b, c, e]] {
                    faces.push(RawGeometryTriangle {
                        vertices: tri.map(reference),
                        color: Rgba8::WHITE,
                        legacy_aliases: vec![],
                        extensions: BTreeMap::new(),
                    });
                }
            }
        }
        let edges = (0..n)
            .flat_map(|i| {
                [
                    [i, i + 1],
                    [i * (n + 1), (i + 1) * (n + 1)],
                    [n * (n + 1) + i, n * (n + 1) + i + 1],
                    [i * (n + 1) + n, (i + 1) * (n + 1) + n],
                ]
            })
            .map(|e| RawGeometrySegment {
                endpoints: e.map(reference),
                color: Rgba8::BLACK,
                legacy_aliases: vec![],
                extensions: BTreeMap::new(),
            })
            .collect();
        d.replace_definition_geometry_raw(def, vertices, edges, faces)
            .unwrap();
        let columns = (copies as f64).sqrt().ceil() as usize;
        for i in 0..copies {
            meshcraft_core::operations::objects::place_definition(
                &mut d,
                &EditContext::Document,
                def,
                Vec3::new(
                    (i % columns) as f64 * n as f64 * 1.1,
                    0.,
                    (i / columns) as f64 * n as f64 * 1.1,
                ),
                None,
                false,
            )
            .unwrap();
        }
        let mut scene = RenderScene {
            document: Arc::new(d),
            grid_visible: false,
            ..Default::default()
        };
        scene.lighting.shadows = false;
        let mut camera = OrbitCamera::default();
        camera.fit(&scene.document);
        let mut target = rl.load_render_texture(&thread, 960, 640).unwrap();
        let start = Instant::now();
        renderer.prepare(&mut rl, &thread, &scene);
        println!("first prepare {:?}", start.elapsed());
        for _ in 0..2 {
            draw_offscreen_scene(
                &mut rl,
                &thread,
                &mut target,
                camera,
                &scene,
                0,
                &mut renderer,
            );
            std::hint::black_box(copy_target_to_gpui_image(&target).unwrap());
        }
        let start = Instant::now();
        let mut submitted = Duration::ZERO;
        for i in 0..6 {
            let begin = Instant::now();
            camera.yaw += 0.025;
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
            submitted += begin.elapsed();
            std::hint::black_box(copy_target_to_gpui_image(&target).unwrap());
        }
        println!(
            "Draw submission elapsed {:.3} ms/frame (includes driver waits); GPU cache {:?}",
            submitted.as_secs_f64() * 1000. / 6.,
            renderer.gpu.stats
        );
        println!(
            "{} instances, {} triangles, {} edges: {:.3} ms/frame including GPU/readback",
            copies,
            copies * n * n * 2,
            copies * n * 4,
            start.elapsed().as_secs_f64() * 1000. / 6.
        );
    }
}
