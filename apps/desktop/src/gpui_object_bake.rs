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
        if !program.is_object_generator() {
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
        let custom = program.generation == "custom";
        let (branches, leaves, instances) = if custom {
            let instances = program.custom_draw_instances(&values, config.max_instances);
            if instances == 0 {
                return Err("This shader object draws no instances".into());
            }
            (0, 0, instances)
        } else {
            let branches =
                object_branch_count(&program, &values).min(config.max_instances as usize);
            let leaves = object_leaf_count(&program, &values)
                .min((config.max_instances as usize).saturating_sub(branches));
            (branches, leaves, 0)
        };
        total += if custom {
            instances * program.vertex_count as usize / 3
        } else {
            branches * (object_branch_vertices(&program) / 3) + leaves * 2
        };
        if total > 2_000_000 {
            return Err("Bake exceeds the two million triangle safety limit".into());
        }
        let (mut vertex, _) = shader_sources(&program);
        // Fragment color is sampled at the emitted triangle vertices. Discard is a
        // raster mask rather than topology; leaf cards remain cards in a mesh bake.
        // The colour hooks run per vertex here, so fragment-only derivatives read
        // as flat and the fragment stage's view and light uniforms are declared.
        let colors = format!(
            "#define dFdx(x) ((x)*0.0)\n#define dFdy(x) ((x)*0.0)\n#define fwidth(x) ((x)*0.0)\nuniform vec3 eye;uniform vec3 towardLight;\n{}",
            fragment_hooks(&program).replace("discard;", ";")
        );
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
        if program.generation == "branching" {
            shader.set_shader_value(shader.get_shader_location("branchPixelScale"), 0f32);
        }
        shader.set_shader_value(
            shader.get_shader_location("generation"),
            if custom { 5i32 } else { 2i32 },
        );
        shader.set_shader_value(
            shader.get_shader_location("customVertexCount"),
            program.vertex_count as i32,
        );
        shader.set_shader_value(
            shader.get_shader_location("customInstanceCount"),
            instances as i32,
        );
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
            ..Default::default()
        }]);
        let captured = (|| unsafe {
            raylib::ffi::rlEnableShader(shader.id);
            if custom {
                return shader_compiler::capture(
                    domain.vao,
                    program.vertex_count as i32,
                    instances as i32,
                );
            }
            shader.set_shader_value(shader.get_shader_location("treePass"), 0i32);
            let mut data = shader_compiler::capture(
                domain.vao,
                object_branch_vertices(&program) as i32,
                branches as i32,
            )?;
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
    for face in data.as_chunks::<3>().0 {
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
    #[test]
    #[ignore = "requires private X11/OpenGL 3.3; reads actual GPU custom shader output"]
    fn gpu_bake_custom_object_geometry_counts_and_bounds() {
        use meshcraft_core::{
            document::procedural::{InstanceCount, Parameter, blank_object_program},
            operations::objects::place_definition,
            selection::EditContext,
        };
        let (_rl, _thread) = raylib::init()
            .size(64, 64)
            .hidden()
            .title("GPU custom bake")
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut d = Document::new();
        let mut program = blank_object_program();
        program.id = "custom.object.1".into();
        program.parameters.push(Parameter {
            id: "count".into(),
            kind: "integer".into(),
            default: 3.into(),
            label: String::new(),
            description: String::new(),
            unit: String::new(),
            min: Some(0.),
            max: None,
            step: None,
            options: BTreeMap::new(),
            extensions: BTreeMap::new(),
        });
        program.instances = Some(InstanceCount::Parameter("count".into()));
        // Stack the boxes: instanceId is a real per-instance input.
        program.vertex = program.vertex.replace(
            "position *= u_size;",
            "position *= u_size; position.y += float(instanceId) * u_size.y;",
        );
        // Bounds grow with the count, so the captured stack fits its own box.
        program.object_bounds.as_mut().unwrap().max[1] = "count * size.y".into();
        d.put_procedural_program(program.clone()).unwrap();
        let id = d.create_procedural_tree().unwrap();
        let settings = Material {
            name: "Stack".into(),
            procedural: Some(meshcraft_core::document::procedural::ProceduralMaterial {
                program: program.id.clone(),
                parameters: BTreeMap::from([("size".into(), serde_json::json!([2.0, 1.0, 2.0]))]),
                ..Default::default()
            }),
            ..Default::default()
        };
        d.set_procedural_object(id, None, settings).unwrap();
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
        mesh.validate().unwrap();
        // Three boxes of 12 triangles; touching faces stay separate triangles.
        assert_eq!(mesh.triangles.len(), 36);
        let ys = mesh.vertices.values().map(|v| v.position.y);
        let (low, high) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| {
            (a.min(y), b.max(y))
        });
        assert!(
            (low - 0.).abs() < 1e-5 && (high - 3.).abs() < 1e-5,
            "{low} {high}"
        );
        let xs = mesh.vertices.values().map(|v| v.position.x.abs());
        assert!((xs.fold(0., f64::max) - 1.).abs() < 1e-5);
        let bounds = d.procedural_object_local_bounds(&path).unwrap();
        assert_eq!((bounds.min.y, bounds.max.y, bounds.max.x), (0., 3., 1.));
        // Outward winding: each face normal points away from its box centre.
        for t in mesh.triangles.values() {
            let p = t.vertices.map(|v| mesh.vertices[&v.id].position);
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            let c = (p[0] + p[1] + p[2]) * (1. / 3.);
            // Triangles keep emission order: twelve per instance.
            let centre = Vec3::new(0., ((t.id.0 - 1) / 12) as f64 + 0.5, 0.);
            assert!(n.dot(c - centre) > 0., "inward face at {c:?}");
        }
        // A zero count draws nothing and is reported rather than baked empty.
        let mut none = d.procedural_object(&path).unwrap();
        none.procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("count".into(), 0.into());
        d.set_procedural_object(id, Some(&path), none).unwrap();
        assert!(capture(&d, &[path], 0.).is_err());
    }

    /// Fragment code with derivatives, view and light uniforms (bump mapping,
    /// reflections) still bakes: colour hooks run per vertex in the capture.
    #[test]
    #[ignore = "requires an OpenGL 3.3 context"]
    fn gpu_bake_custom_object_with_fragment_only_code() {
        use meshcraft_core::{
            document::procedural::blank_object_program, operations::objects::place_definition,
            selection::EditContext,
        };
        let (_rl, _thread) = raylib::init()
            .size(64, 64)
            .hidden()
            .title("GPU custom bake fragment code")
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut d = Document::new();
        let mut program = blank_object_program();
        program.id = "custom.object.1".into();
        program.fragment = "float bumps(){return fract(localPosition.x*7.0);}
vec3 proceduralShadingNormal(vec3 n,vec3 p,vec2 uv){
    float h=bumps()*0.05;vec3 dx=dFdx(p),dy=dFdy(p);
    return normalize(n-dFdx(h)*cross(dy,n)-dFdy(h)*cross(n,dx)+fwidth(h));
}
vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv){
    float facing=max(dot(normalize(eye-p),n),0.0)+dot(towardLight,n)*0.0;
    return vec4(c.rgb*(0.5+0.5*bumps())+facing*0.0,c.a);
}"
        .into();
        d.put_procedural_program(program.clone()).unwrap();
        let id = d.create_procedural_tree().unwrap();
        let settings = Material {
            procedural: Some(meshcraft_core::document::procedural::ProceduralMaterial {
                program: program.id.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };
        d.set_procedural_object(id, None, settings).unwrap();
        let path =
            place_definition(&mut d, &EditContext::Document, id, Vec3::ZERO, None, false).unwrap();
        let mesh = capture(&d, std::slice::from_ref(&path), 0.)
            .unwrap()
            .remove(&path)
            .unwrap();
        mesh.validate().unwrap();
        assert_eq!(mesh.triangles.len(), 12);
    }
}
