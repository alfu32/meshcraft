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
        eye: Vec3,
    ) {
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
        for face in &self.faces {
            if selected(&scene.selection, &face.target) {
                for [a, b] in [
                    [face.points[0], face.points[1]],
                    [face.points[1], face.points[2]],
                    [face.points[2], face.points[0]],
                ] {
                    view.draw_line3D(ray_vec3(a), ray_vec3(b), Color::new(240, 116, 43, 255));
                }
            }
        }
        for edge in &self.edges {
            view.draw_line3D(
                ray_vec3(edge.points[0]),
                ray_vec3(edge.points[1]),
                if selected(&scene.selection, &edge.target) {
                    Color::new(245, 128, 46, 255)
                } else {
                    Color::BLACK
                },
            );
        }
        for vertex in &self.vertices {
            if selected(&scene.selection, &vertex.target) {
                view.draw_sphere(ray_vec3(vertex.point), 0.06, Color::new(227, 57, 49, 255));
            }
        }
    }
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
