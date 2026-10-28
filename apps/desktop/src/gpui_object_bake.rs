//! GPU vertex-shader readback. Never evaluates a different CPU tree algorithm.
use super::*;
use meshcraft_core::document::{
    GeometryBundle, OccurrenceRef, Segment, SegmentId, Triangle, TriangleId, Vertex, VertexId,
    VertexRef,
};

pub(in crate::gpui_app) fn capture(
    document: &Document,
    paths: &[OccurrenceRef],
    time: f32,
) -> Result<BTreeMap<OccurrenceRef, GeometryBundle>, String> {
    let mut result = BTreeMap::new();
    let mut total = 0usize;
    for path in paths {
        let material = document
            .procedural_object(path)
            .ok_or("Shader object no longer exists")?;
        let config = material
            .procedural
            .as_ref()
            .ok_or("Shader object has no program")?;
        let program = document
            .procedural_programs()
            .into_iter()
            .find(|p| p.id == config.program)
            .ok_or("Shader program is unavailable")?;
        if program.generation != "tree" {
            return Err("Unsupported object generator for GPU bake".into());
        }
        if program.parameters.iter().any(|p| p.kind == "texture") {
            return Err("Texture-dependent generators require texture baking, which is not supported by mesh baking yet".into());
        }
        let values = program.resolve(&config.parameters, &BTreeMap::new())?;
        let world = document
            .resolve_occurrence(path)
            .ok_or("Object placement is missing")?
            .world_transform;
        let bounds = document
            .procedural_object_local_bounds(path)
            .ok_or("Object bounds are missing")?;
        let catalog = document.materials();
        let base = config
            .base_material
            .as_ref()
            .and_then(|id| catalog.iter().find(|m| &m.id == id))
            .unwrap_or(&material);
        let c = base.color;
        let branches = tree_branch_count(&values).min(config.max_instances as usize);
        let leaves = (num(&values, "leaves", 3500.) as usize)
            .min((config.max_instances as usize).saturating_sub(branches));
        total += branches * 16 + leaves * 2;
        if total > 2_000_000 {
            return Err("Bake exceeds the two million triangle safety limit".into());
        }
        let (mut vertex, _) = shader_sources(&program);
        // Fragment color is sampled at the emitted triangle vertices. Discard is a
        // raster mask rather than topology; leaf cards remain cards in a mesh bake.
        let colors = program.fragment.replace("discard;", ";");
        vertex = vertex.replace(
            "void main() {",
            "out vec3 bakePosition; out vec4 bakeColor;\nvoid bakeMain() {",
        );
        vertex = vertex.replace(
            "proceduralVertex(position,normal,uv);",
            "proceduralVertex(position,normal,uv);bakePosition=position;",
        );
        vertex += &format!(
            "\n{colors}\nvoid main(){{bakeMain();bakeColor=proceduralColor(color,worldPosition,proceduralNormal,proceduralUv);}}\n"
        );
        let fragment = "#version 330\nout vec4 finalColor; void main(){finalColor=vec4(1);}";
        let mut shader =
            shader_compiler::compile_feedback(&vertex, fragment, &[c"bakePosition", c"bakeColor"])?;
        shader.set_shader_value_matrix(shader.get_shader_location("model"), matrix(world));
        shader.set_shader_value_matrix(
            shader.get_shader_location("viewProjection"),
            matrix(Affine3::IDENTITY),
        );
        shader.set_shader_value(shader.get_shader_location("time"), time);
        shader.set_shader_value(shader.get_shader_location("generation"), 2i32);
        shader.set_shader_value(shader.get_shader_location("treeBranches"), branches as i32);
        shader.set_shader_value(shader.get_shader_location("treeLeafCount"), leaves as i32);
        shader.set_shader_value(
            shader.get_shader_location("baseColor"),
            raylib::prelude::Vector4::new(
                c.red as f32 / 255.,
                c.green as f32 / 255.,
                c.blue as f32 / 255.,
                c.alpha as f32 / 255.,
            ),
        );
        bind_parameters(&mut shader, &program, &values);
        let domain = DomainGpu::new(vec![Domain {
            a: [
                bounds.min.x as f32,
                bounds.min.y as f32,
                bounds.min.z as f32,
            ],
            b: [
                bounds.max.x as f32,
                bounds.max.y as f32,
                bounds.max.z as f32,
            ],
            c: [0., 0., 0.],
            seed: [0., 0.],
        }]);
        let captured = (|| unsafe {
            raylib::ffi::rlEnableShader(shader.id);
            shader.set_shader_value(shader.get_shader_location("treePass"), 0i32);
            let mut data = shader_compiler::capture(domain.vao, 48, branches as i32)?;
            if leaves > 0 {
                shader.set_shader_value(shader.get_shader_location("treePass"), 1i32);
                data.extend(shader_compiler::capture(domain.vao, 6, leaves as i32)?);
            }
            Ok::<_, String>(data)
        })();
        unsafe {
            raylib::ffi::rlDisableShader();
        }
        result.insert(path.clone(), geometry(captured?)?);
    }
    Ok(result)
}

fn geometry(data: Vec<[f32; 7]>) -> Result<GeometryBundle, String> {
    let mut geometry = GeometryBundle::default();
    let mut vertices = BTreeMap::new();
    let mut edges = BTreeSet::new();
    for face in data.chunks_exact(3) {
        if face.iter().flatten().any(|v| !v.is_finite()) {
            return Err("Shader produced non-finite bake geometry".into());
        }
        let points: [Vec3; 3] = std::array::from_fn(|i| {
            Vec3::new(face[i][0] as f64, face[i][1] as f64, face[i][2] as f64)
        });
        if (points[1] - points[0])
            .cross(points[2] - points[0])
            .length_squared()
            < 1e-24
        {
            continue;
        }
        let color = |i: usize| {
            ((face.iter().map(|p| p[i]).sum::<f32>() / 3.).clamp(0., 1.) * 255.).round() as u8
        };
        let color = Rgba8::new(color(3), color(4), color(5), color(6));
        let references = std::array::from_fn(|i| {
            let p = points[i];
            let key = [p.x, p.y, p.z].map(|v| if v == 0. { 0 } else { v.to_bits() });
            let id = *vertices.entry(key).or_insert_with(|| {
                let id = VertexId(geometry.vertices.len() as u64 + 1);
                geometry.vertices.insert(
                    id,
                    Vertex {
                        id,
                        position: p,
                        legacy_aliases: vec![],
                    },
                );
                id
            });
            VertexRef {
                id,
                position_offset: Vec3::ZERO,
            }
        });
        let id = TriangleId(geometry.triangles.len() as u64 + 1);
        geometry.triangles.insert(
            id,
            Triangle {
                id,
                vertices: references,
                color,
                legacy_aliases: vec![],
                extensions: BTreeMap::new(),
            },
        );
        for [a, b] in [[0, 1], [1, 2], [2, 0]] {
            let a = references[a];
            let b = references[b];
            let key = if a.id < b.id {
                (a.id, b.id)
            } else {
                (b.id, a.id)
            };
            if edges.insert(key) {
                let id = SegmentId(geometry.segments.len() as u64 + 1);
                geometry.segments.insert(
                    id,
                    Segment {
                        id,
                        endpoints: [a, b],
                        color: Rgba8::new(0, 0, 0, 255),
                        legacy_aliases: vec![],
                        extensions: BTreeMap::new(),
                    },
                );
            }
        }
    }
    geometry.validate()?;
    Ok(geometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config_program(d: &Document, id: meshcraft_core::document::DefinitionId) -> String {
        d.procedural_object_definition(id)
            .unwrap()
            .procedural
            .unwrap()
            .program
    }
    #[test]
    #[ignore = "requires private X11/OpenGL 3.3; reads actual GPU tree shader output"]
    fn gpu_bake_tree_geometry_and_shader_deformation() {
        use meshcraft_core::{operations::objects::place_definition, selection::EditContext};
        let (_rl, _thread) = raylib::init()
            .size(64, 64)
            .hidden()
            .title("GPU object bake")
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut d = Document::new();
        let id = d.create_procedural_tree().unwrap();
        let path = place_definition(
            &mut d,
            &EditContext::Document,
            id,
            Vec3::new(5., 2., 3.),
            None,
            false,
        )
        .unwrap();
        let mesh = capture(&d, std::slice::from_ref(&path), 0.)
            .unwrap()
            .remove(&path)
            .unwrap();
        assert!(mesh.triangles.len() > 7000);
        assert!(!mesh.segments.is_empty());
        mesh.validate().unwrap();
        let next = capture(&d, std::slice::from_ref(&path), 1.)
            .unwrap()
            .remove(&path)
            .unwrap();
        assert_ne!(
            mesh.vertices, next.vertices,
            "animated vertex displacement must be captured"
        );
        let mut settings = d.procedural_object(&path).unwrap();
        settings
            .procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("leaves".into(), 0.into());
        d.set_procedural_object(id, Some(&path), settings).unwrap();
        let bare = capture(&d, &[path], 0.)
            .unwrap()
            .into_values()
            .next()
            .unwrap();
        assert!(bare.triangles.len() < mesh.triangles.len());
        assert!(bare.vertices.values().all(|v| v.position.is_finite()));
        let mut custom = d
            .procedural_programs()
            .into_iter()
            .find(|p| p.id == config_program(&d, id))
            .unwrap();
        custom.id = "bake-test-offset".into();
        custom.vertex =
            "void proceduralVertex(inout vec3 p,inout vec3 n,vec2 uv){p.y+=2.0;}".into();
        d.put_procedural_program(custom.clone()).unwrap();
        let path = d
            .scene_geometry_refs()
            .unwrap()
            .into_iter()
            .find(|p| d.procedural_object(p).is_some())
            .unwrap();
        let mut settings = d.procedural_object(&path).unwrap();
        settings.procedural.as_mut().unwrap().program = custom.id;
        d.set_procedural_object(id, Some(&path), settings).unwrap();
        let displaced = capture(&d, &[path], 0.)
            .unwrap()
            .into_values()
            .next()
            .unwrap();
        let min_y = |g: &GeometryBundle| {
            g.vertices
                .values()
                .map(|v| v.position.y)
                .fold(f64::INFINITY, f64::min)
        };
        assert!((min_y(&displaced) - min_y(&bare) - 2.).abs() < 1e-5);
    }
}
