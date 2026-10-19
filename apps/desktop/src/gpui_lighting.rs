//! One batched solid pass and a cached light-camera depth pass; helpers never cast shadows.
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
            ambient: 0.59,
            directional: 0.73,
            shadow_strength: 0.50,
            specular: 0.20,
            depth_bias: 0.00015,
            normal_bias: 0.004,
            azimuth: -35.0,
            elevation: 55.0,
            shadows: true,
        }
    }
}
impl Lighting {
    fn direction(self) -> Vec3 {
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
}
struct Edge {
    points: [Vec3; 2],
    target: SelectionTarget,
}
struct Vertex {
    point: Vec3,
    target: SelectionTarget,
}

pub(super) struct Renderer {
    depth: Shader,
    lit: Shader,
    map: RenderTexture2D,
    document: Option<Arc<Document>>,
    faces: Vec<Face>,
    edges: Vec<Edge>,
    vertices: Vec<Vertex>,
    center: Vec3,
    radius: f64,
    last_light: Option<(f32, f32)>,
}
impl Renderer {
    pub fn new(rl: &mut raylib::RaylibHandle, thread: &raylib::RaylibThread) -> Self {
        let vertex = include_str!("../../../assets/shaders/scene.vs");
        let depth = rl.load_shader_from_memory(
            thread,
            Some(vertex),
            Some(include_str!("../../../assets/shaders/shadow.fs")),
        );
        let lit = rl.load_shader_from_memory(
            thread,
            Some(vertex),
            Some(include_str!("../../../assets/shaders/scene.fs")),
        );
        Self {
            depth,
            lit,
            map: rl
                .load_render_texture(thread, 2048, 2048)
                .expect("shadow target"),
            document: None,
            faces: Vec::new(),
            edges: Vec::new(),
            vertices: Vec::new(),
            center: Vec3::ZERO,
            radius: 10.0,
            last_light: None,
        }
    }
    pub fn prepare(
        &mut self,
        rl: &mut raylib::RaylibHandle,
        thread: &raylib::RaylibThread,
        scene: &RenderScene,
    ) {
        let changed = self
            .document
            .as_ref()
            .is_none_or(|doc| !Arc::ptr_eq(doc, &scene.document));
        if changed {
            self.rebuild(&scene.document);
            self.last_light = None;
        }
        let direction = scene.lighting.direction();
        let light = (scene.lighting.azimuth, scene.lighting.elevation);
        if !scene.lighting.shadows || (!changed && self.last_light == Some(light)) {
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
            let mut view = drawing.begin_mode3D(camera);
            // Both sides cast, including open planar meshes. No guides/lines/preview here.
            unsafe {
                raylib::ffi::rlDisableBackfaceCulling();
                light_vp = raylib::ffi::MatrixMultiply(
                    raylib::ffi::rlGetMatrixModelview(),
                    raylib::ffi::rlGetMatrixProjection(),
                );
            }
            let mut pass = view.begin_shader_mode(&mut self.depth);
            for face in &self.faces {
                if face.color.alpha == 255 {
                    triangle(&mut pass, face.points, Color::WHITE);
                }
            }
            drop(pass);
            unsafe {
                raylib::ffi::rlEnableBackfaceCulling();
            }
        }
        unsafe {
            raylib::ffi::rlSetClipPlanes(old_near, old_far);
        }
        self.lit
            .set_shader_value_matrix(self.lit.get_shader_location("lightVP"), light_vp);
    }
    fn rebuild(&mut self, document: &Arc<Document>) {
        self.faces.clear();
        self.edges.clear();
        self.vertices.clear();
        let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = -min;
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
                        points: [a, b, c],
                        color: face.color,
                        target: SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Triangle(face.id),
                        },
                    });
                }
            }
            for edge in geometry.segments.values() {
                if let [Some(a), Some(b)] = edge.endpoints.map(world) {
                    self.edges.push(Edge {
                        points: [a, b],
                        target: SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Segment(edge.id),
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
        self.document = Some(Arc::clone(document));
    }
    pub fn draw(
        &mut self,
        view: &mut (impl RaylibDraw3D + RaylibDraw),
        scene: &RenderScene,
        camera: OrbitCamera,
        height: f32,
    ) {
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
            raylib::ffi::rlDisableBackfaceCulling();
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
            for face in &self.faces {
                let color = if selected(&scene.selection, &face.target) {
                    Color::new(242, 151, 61, face.color.alpha)
                } else {
                    ray_color(face.color)
                };
                triangle(&mut pass, face.points, color);
            }
        }
        unsafe {
            raylib::ffi::rlEnableBackfaceCulling();
            raylib::ffi::rlActiveTextureSlot(1);
            raylib::ffi::rlDisableTexture();
            raylib::ffi::rlActiveTextureSlot(0);
        }
        unsafe {
            raylib::ffi::rlDisableBackfaceCulling();
        }
        for face in &self.faces {
            if selected(&scene.selection, &face.target) {
                for [a, b] in [
                    [face.points[0], face.points[1]],
                    [face.points[1], face.points[2]],
                    [face.points[2], face.points[0]],
                ] {
                    stroke(
                        view,
                        [a, b],
                        Color::new(240, 116, 43, 255),
                        scene.thick_line_width,
                        camera,
                        height,
                    );
                }
            }
        }
        unsafe {
            raylib::ffi::rlDisableBackfaceCulling();
        }
        for edge in &self.edges {
            let selected = selected(&scene.selection, &edge.target);
            stroke(
                view,
                edge.points,
                if selected {
                    Color::new(245, 128, 46, 255)
                } else {
                    Color::BLACK
                },
                if selected {
                    scene.thick_line_width
                } else {
                    scene.normal_line_width
                },
                camera,
                height,
            );
        }
        unsafe {
            raylib::ffi::rlDrawRenderBatchActive();
            raylib::ffi::rlEnableBackfaceCulling();
        }
        for vertex in &self.vertices {
            if selected(&scene.selection, &vertex.target) {
                view.draw_sphere(ray_vec3(vertex.point), 0.06, Color::new(227, 57, 49, 255));
            }
        }
    }
}
/// Pixel-width ribbons keep model line widths portable across OpenGL implementations.
fn stroke(
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
        .cross(Vec3::Y)
        .try_normalized(1.0e-12)
        .unwrap_or(Vec3::X);
    let up = right.cross(forward);
    let side = (right * (-dy) - up * dx) / length;
    let factor = f64::from(width) * (f64::from(camera.field_of_view).to_radians() * 0.5).tan()
        / f64::from(height.max(1.));
    let offset_a = side * ((a - eye).dot(forward) * factor);
    let offset_b = side * ((b - eye).dot(forward) * factor);
    triangle(view, [a - offset_a, b - offset_b, b + offset_b], color);
    triangle(view, [a - offset_a, b + offset_b, a + offset_a], color);
}

fn selected(selection: &[SelectionTarget], target: &SelectionTarget) -> bool {
    selection.iter().any(|s| {
        s.occurrence == target.occurrence
            && (s.entity == target.entity || s.entity == SelectedEntity::Occurrence)
    })
}
fn triangle(view: &mut impl RaylibDraw3D, [a, b, c]: [Vec3; 3], color: Color) {
    view.draw_triangle3D(ray_vec3(a), ray_vec3(b), ray_vec3(c), color);
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
