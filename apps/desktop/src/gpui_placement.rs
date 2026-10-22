//! Static GPU wireframe quads; placement motion changes only the transform uniform.
use super::*;
use raylib::prelude::{Mesh, RaylibShader, Shader};

pub(super) struct Renderer {
    shader: Shader,
    source: Option<Arc<Vec<[Vec3; 2]>>>,
    mesh: Option<Mesh>,
}

impl Renderer {
    pub(super) fn new(rl: &mut raylib::RaylibHandle, thread: &raylib::RaylibThread) -> Self {
        Self {
            shader: rl.load_shader_from_memory(
                thread,
                Some(include_str!("../../../assets/shaders/placement.vs")),
                Some(include_str!("../../../assets/shaders/placement.fs")),
            ),
            source: None,
            mesh: None,
        }
    }

    pub(super) fn prepare(&mut self, thread: &raylib::RaylibThread, preview: Option<&ToolPreview>) {
        let Some(ToolPreview::Placement { edges, .. }) = preview else {
            self.mesh = None;
            self.source = None;
            return;
        };
        if self
            .source
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, edges))
        {
            return;
        }
        self.mesh = None;
        self.source = Some(Arc::clone(edges));
        if edges.is_empty() {
            return;
        }
        let mut starts = Vec::with_capacity(edges.len() * 6);
        let mut ends = Vec::with_capacity(edges.len() * 6);
        let mut coordinates = Vec::with_capacity(edges.len() * 6);
        for [a, b] in edges.iter().copied() {
            for (along, side) in [
                (0., -1.),
                (1., -1.),
                (1., 1.),
                (0., -1.),
                (1., 1.),
                (0., 1.),
            ] {
                starts.push(ray_vec3(a));
                ends.push(ray_vec3(b));
                coordinates.push(raylib::prelude::Vector2::new(along, side));
            }
        }
        self.mesh = Some(
            Mesh::gen_mesh(&starts, &coordinates)
                .normals(&ends)
                .build(thread)
                .expect("placement quads have matched, non-indexed attributes"),
        );
    }

    pub(super) fn draw(
        &mut self,
        _view: &mut impl RaylibDraw3D,
        transform: meshcraft_core::geometry::Affine3,
        width: f32,
        size: (f32, f32),
    ) {
        let Some(mesh) = self.mesh.as_ref() else {
            return;
        };
        let m = transform.linear;
        let t = transform.translation;
        let model = raylib::ffi::Matrix {
            m0: m[0][0] as f32,
            m4: m[0][1] as f32,
            m8: m[0][2] as f32,
            m12: t.x as f32,
            m1: m[1][0] as f32,
            m5: m[1][1] as f32,
            m9: m[1][2] as f32,
            m13: t.y as f32,
            m2: m[2][0] as f32,
            m6: m[2][1] as f32,
            m10: m[2][2] as f32,
            m14: t.z as f32,
            m3: 0.,
            m7: 0.,
            m11: 0.,
            m15: 1.,
        };
        // Flush raylib's batch before binding our own immutable mesh/VAO. Raylib resumes its
        // default shader for subsequent helper batches; all resources remain on this GL thread.
        unsafe {
            raylib::ffi::rlDrawRenderBatchActive();
            let vp = raylib::ffi::MatrixMultiply(
                raylib::ffi::rlGetMatrixModelview(),
                raylib::ffi::rlGetMatrixProjection(),
            );
            self.shader.set_shader_value_matrix(
                self.shader.get_shader_location("mvp"),
                raylib::ffi::MatrixMultiply(model, vp),
            );
            self.shader.set_shader_value(
                self.shader.get_shader_location("viewportSize"),
                raylib::prelude::Vector2::new(size.0.max(1.), size.1.max(1.)),
            );
            self.shader
                .set_shader_value(self.shader.get_shader_location("lineWidth"), width.max(1.));
            let shader: &raylib::ffi::Shader = self.shader.as_ref();
            let mesh: &raylib::ffi::Mesh = mesh.as_ref();
            raylib::ffi::rlEnableShader(shader.id);
            raylib::ffi::rlDisableBackfaceCulling();
            raylib::ffi::rlEnableVertexArray(mesh.vaoId);
            raylib::ffi::rlDrawVertexArray(0, mesh.vertexCount);
            raylib::ffi::rlDisableVertexArray();
            raylib::ffi::rlEnableBackfaceCulling();
            raylib::ffi::rlDisableShader();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshcraft_core::geometry::Affine3;

    #[test]
    #[ignore = "requires X11/OpenGL; run explicitly with --ignored --test-threads=1"]
    fn placement_gpu_quads_follow_transforms_and_reuse_the_uploaded_mesh() {
        let (mut rl, thread) = raylib::init()
            .size(640, 480)
            .hidden()
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut renderer = lighting::Renderer::new(&mut rl, &thread);
        assert_ne!(renderer.placement.shader.id, unsafe {
            raylib::ffi::rlGetShaderIdDefault()
        });
        let mut target = rl.load_render_texture(&thread, 640, 480).unwrap();
        let edges = Arc::new(vec![[Vec3::new(-1., 0., 0.), Vec3::new(1., 0., 0.)]]);
        let mut scene = RenderScene {
            grid_visible: false,
            ..Default::default()
        };
        scene.lighting.shadows = false;
        let mut previous = None;
        for orthographic in [false, true] {
            for transform in [
                Affine3::from_translation(Vec3::new(-2., 2., 0.)),
                Affine3::from_linear_translation(
                    [[0., -1., 0.], [1., 0., 0.], [0., 0., 1.]],
                    Vec3::new(2., 3., 0.),
                ),
            ] {
                let camera = OrbitCamera {
                    orthographic,
                    ..Default::default()
                };
                scene.preview = Some(ToolPreview::Placement {
                    edges: Arc::clone(&edges),
                    transform,
                });
                renderer.prepare(&mut rl, &thread, &scene);
                let vao = renderer.placement.mesh.as_ref().unwrap().vaoId;
                if let Some(old) = previous {
                    assert_eq!(old, vao, "pointer/camera motion must reuse the upload");
                }
                previous = Some(vao);
                draw_offscreen_scene(
                    &mut rl,
                    &thread,
                    &mut target,
                    camera,
                    &scene,
                    0,
                    &mut renderer,
                );
                let frame = copy_target_to_gpui_image(&target).unwrap();
                let pixels = frame
                    .enumerate_pixels()
                    .filter(|(_, _, p)| p[2] > 200 && p[1] > 90 && p[1] < 150 && p[0] < 80)
                    .map(|(x, y, _)| (x as f32, y as f32))
                    .collect::<Vec<_>>();
                assert!(
                    pixels.len() > 30,
                    "placement shader must draw orange thick lines"
                );
                let projected = edges[0].map(|p| {
                    project_point(camera, (640., 480.), transform.transform_point(p)).unwrap()
                });
                for axis in [0, 1] {
                    let coord = |(x, y): (f32, f32)| if axis == 0 { x } else { y };
                    let expected_min = projected
                        .map(coord)
                        .into_iter()
                        .fold(f32::INFINITY, f32::min);
                    let expected_max = projected
                        .map(coord)
                        .into_iter()
                        .fold(f32::NEG_INFINITY, f32::max);
                    let actual_min = pixels
                        .iter()
                        .copied()
                        .map(coord)
                        .fold(f32::INFINITY, f32::min);
                    let actual_max = pixels
                        .iter()
                        .copied()
                        .map(coord)
                        .fold(f32::NEG_INFINITY, f32::max);
                    assert!(
                        (actual_min - expected_min).abs() < 4.
                            && (actual_max - expected_max).abs() < 4.,
                        "GPU placement must match picking projection"
                    );
                }
            }
        }
        scene.preview = None;
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(renderer.placement.mesh.is_none() && renderer.placement.source.is_none());
    }
}
