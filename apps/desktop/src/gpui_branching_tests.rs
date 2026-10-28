//! Actual GLSL execution contracts; no CPU implementation of branch generation.
use super::*;
use meshcraft_core::{
    document::procedural::{self, BRANCHING},
    operations::objects::place_definition,
};

#[test]
fn branching_draw_counts_preserve_legacy_and_bound_work() {
    let legacy = procedural::tree_program();
    let modern = procedural::branching_program();
    let defaults = modern.resolve(&BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(object_branch_count(&legacy, &BTreeMap::new()), 105);
    assert_eq!(object_branch_vertices(&legacy), 48);
    assert_eq!(object_branch_count(&modern, &defaults), 92);
    assert_eq!(object_branch_vertices(&modern), 108);
    let mut values = defaults;
    values.insert("forks".into(), json!(1));
    assert_eq!(object_branch_count(&modern, &values), 22);
    values.insert("levels".into(), json!(0));
    assert_eq!(object_branch_count(&modern, &values), 1);
    values.extend([
        ("branches".into(), json!(12)),
        ("forks".into(), json!(8)),
        ("levels".into(), json!(6)),
    ]);
    assert_eq!(object_branch_count(&modern, &values), 16_384);
    for form in ["coral", "roots"] {
        values.insert("form".into(), json!(form));
        assert_eq!(object_leaf_count(&modern, &values), 0);
    }
}

#[test]
#[ignore = "requires private X11/OpenGL 3.3; exercises actual branching GLSL and bake readback"]
fn branching_shader_families_bounds_continuity_and_custom_rules() {
    let (_rl, _thread) = raylib::init()
        .size(64, 64)
        .hidden()
        .title("Branching shader contracts")
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let mut document = Document::new();
    let id = document.create_procedural_tree().unwrap();
    let path = place_definition(
        &mut document,
        &EditContext::Document,
        id,
        Vec3::ZERO,
        None,
        false,
    )
    .unwrap();
    let mut settings = Material::default();
    procedural::apply_preset(&mut settings, BRANCHING);
    settings.procedural.as_mut().unwrap().parameters.extend([
        ("leaves".into(), json!(0)),
        ("wind".into(), json!(0.0)),
        ("branches".into(), json!(4)),
        ("forks".into(), json!(2)),
        ("levels".into(), json!(3)),
    ]);
    let bake = |doc: &Document| {
        object_bake::capture(doc, std::slice::from_ref(&path), 0.)
            .unwrap()
            .remove(&path)
            .unwrap()
    };
    let mut family_bounds = vec![];
    for form in ["broadleaf", "conifer", "coral", "roots", "shrub", "weeping"] {
        settings
            .procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("form".into(), json!(form));
        document
            .set_procedural_object(id, None, settings.clone())
            .unwrap();
        let mesh = bake(&document);
        mesh.validate().unwrap();
        assert!(
            mesh.triangles.len() > 500,
            "{form} generated no useful geometry"
        );
        let bounds = document.procedural_object_local_bounds(&path).unwrap();
        for v in mesh.vertices.values() {
            assert!(v.position.is_finite());
            for (n, lo, hi) in [
                (v.position.x, bounds.min.x, bounds.max.x),
                (v.position.y, bounds.min.y, bounds.max.y),
                (v.position.z, bounds.min.z, bounds.max.z),
            ] {
                assert!(
                    n >= lo - 1e-4 && n <= hi + 1e-4,
                    "{form}: {n} outside {lo}..{hi}"
                );
            }
        }
        if form == "roots" {
            assert!(mesh.vertices.values().any(|v| v.position.y < -1.));
        }
        family_bounds.push(Aabb3::from_points(mesh.vertices.values().map(|v| v.position)).unwrap());
    }
    assert!(
        family_bounds.windows(2).all(|v| v[0] != v[1]),
        "families must have distinct geometry"
    );
    let config = settings.procedural.as_mut().unwrap();
    config.parameters.insert("form".into(), json!("broadleaf"));
    config.parameters.insert("growth".into(), json!(0.499));
    document
        .set_procedural_object(id, None, settings.clone())
        .unwrap();
    let early = bake(&document);
    settings
        .procedural
        .as_mut()
        .unwrap()
        .parameters
        .insert("growth".into(), json!(0.501));
    document
        .set_procedural_object(id, None, settings.clone())
        .unwrap();
    let late = bake(&document);
    let a: Vec<_> = early.vertices.values().map(|v| v.position).collect();
    let b: Vec<_> = late.vertices.values().map(|v| v.position).collect();
    // Growth stays near the preceding surface, instead of introducing full-sized limbs.
    assert!(b.iter().all(|p| a.iter().any(|q| p.distance(*q) < 0.15)));
    let config = settings.procedural.as_mut().unwrap();
    config.parameters.insert("growth".into(), json!(1.0));
    config.parameters.insert("branchDensity".into(), json!(0.0));
    document
        .set_procedural_object(id, None, settings.clone())
        .unwrap();
    let trunk = bake(&document);
    assert_eq!(
        trunk.triangles.len(),
        36,
        "zero survival leaves only the curved trunk"
    );
    let mut custom = procedural::branching_program();
    custom.id = "branching-test-rule".into();
    custom.vertex=custom.vertex.replace("void branchingRule(inout BranchRule rule, int level, int child, uvec2 path, uint key) {",
        "void branchingRule(inout BranchRule rule, int level, int child, uvec2 path, uint key) { rule.activation = 0.0;");
    document.put_procedural_program(custom.clone()).unwrap();
    settings.procedural.as_mut().unwrap().program = custom.id;
    settings
        .procedural
        .as_mut()
        .unwrap()
        .parameters
        .insert("branchDensity".into(), json!(1.0));
    document.set_procedural_object(id, None, settings).unwrap();
    assert_eq!(
        bake(&document).triangles.len(),
        36,
        "the editable GLSL rule controls topology"
    );
}

#[test]
#[ignore = "requires private X11/OpenGL 3.3; compares actual shader ancestry across arity/budget changes"]
fn branching_shader_path_identity_is_independent_of_draw_ordinals() {
    let (_rl, _thread) = raylib::init()
        .size(64, 64)
        .hidden()
        .title("Branch path stability")
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let program = procedural::branching_program();
    let (source, _) = shader_sources(&program);
    let source = source.replace("void main() {", "void unusedMain() {")
        + "\nout vec3 testStart; out vec3 testEnd; out float testAlive; void main(){vec3 c,frame;float radius;uint key;branchEvaluate(gl_InstanceID,testStart,c,testEnd,frame,radius,testAlive,key);gl_Position=vec4(0);}";
    let mut shader = shader_compiler::compile_feedback(
        &source,
        "#version 330\nout vec4 finalColor;void main(){finalColor=vec4(1);}",
        &[c"testStart", c"testEnd", c"testAlive"],
    )
    .unwrap();
    let domain = DomainGpu::new(vec![Domain {
        a: [0.; 3],
        b: [1.; 3],
        c: [0.; 3],
        seed: [0.; 2],
    }]);
    let mut values = program.resolve(&BTreeMap::new(), &BTreeMap::new()).unwrap();
    values.insert("branches".into(), json!(4));
    values.insert("forks".into(), json!(2));
    let mut sample = |values: &BTreeMap<String, Value>, count: usize| {
        bind_parameters(&mut shader, &program, values);
        unsafe {
            raylib::ffi::rlEnableShader(shader.id);
        }
        let data = unsafe { shader_compiler::capture(domain.vao, 3, count as i32) }.unwrap();
        unsafe {
            raylib::ffi::rlDisableShader();
        }
        data.as_chunks::<3>()
            .0
            .iter()
            .map(|v| v[0])
            .collect::<Vec<_>>()
    };
    let original = sample(&values, 29);
    let budget_prefix = sample(&values, 7);
    assert_eq!(&original[..7], budget_prefix.as_slice());
    values.insert("branches".into(), json!(6));
    let expanded = sample(&values, 43);
    // Same slot paths [0..3, 0..1], despite different breadth-first ordinals.
    for i in 0..4 {
        assert_eq!(original[1 + i], expanded[1 + i]);
        for j in 0..2 {
            assert_eq!(original[1 + 4 + i * 2 + j], expanded[1 + 6 + i * 2 + j]);
        }
    }
}

#[test]
#[ignore = "requires private X11/OpenGL 3.3; compares cached GPU vertices to the bake evaluator"]
fn branching_gpu_state_reuse_matches_direct_shader_vertices() {
    let (_rl, _thread) = raylib::init()
        .size(64, 64)
        .hidden()
        .title("GPU branch state parity")
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let program = procedural::branching_program();
    let vertex_feedback = |cached: bool| {
        let (source, _) = if cached {
            render_shader_sources(&program)
        } else {
            shader_sources(&program)
        };
        let source = source.replace("void main() {", "void unusedMain() {")
            + "\nout vec3 testPosition;out vec3 testNormal;out float testPart;void main(){vec2 uv;treeVertex(testPosition,testNormal,uv);testPart=treePart;gl_Position=vec4(0);}";
        shader_compiler::compile_feedback(
            &source,
            "#version 330\nout vec4 finalColor;void main(){finalColor=vec4(1);}",
            &[c"testPosition", c"testNormal", c"testPart"],
        )
        .unwrap()
    };
    let mut cached = vertex_feedback(true);
    let mut direct = vertex_feedback(false);
    let mut pass = branching_gpu::StatePass::new(&program).unwrap();
    let domain = DomainGpu::new(vec![Domain {
        a: [0.; 3],
        b: [1.; 3],
        c: [0.; 3],
        seed: [0.; 2],
    }]);
    let mut values = program.resolve(&BTreeMap::new(), &BTreeMap::new()).unwrap();
    values.extend([
        ("branches".into(), json!(4)),
        ("forks".into(), json!(2)),
        ("levels".into(), json!(2)),
    ]);
    let mut storage = None;
    for form in ["broadleaf", "conifer", "coral", "roots", "shrub", "weeping"] {
        values.insert("form".into(), json!(form));
        let count = object_branch_count(&program, &values);
        pass.run(&program, &values, 0.5, 0., domain.vao, count)
            .unwrap();
        if let Some(old) = storage {
            assert_eq!(old, pass.storage(), "GPU storage must be reused");
        }
        storage = Some(pass.storage());
        for (tree_pass, vertices, instances) in [(0, 108, count), (1, 6, 32)] {
            let capture_vertices = |shader: &mut Shader| {
                bind_parameters(shader, &program, &values);
                shader.set_shader_value(shader.get_shader_location("treePass"), tree_pass);
                shader.set_shader_value(shader.get_shader_location("treeBranches"), count as i32);
                shader.set_shader_value(shader.get_shader_location("branchStates"), 11i32);
                shader.set_shader_value(shader.get_shader_location("time"), 0.5f32);
                unsafe {
                    raylib::ffi::rlEnableShader(shader.id);
                }
                let data =
                    unsafe { shader_compiler::capture(domain.vao, vertices, instances as i32) }
                        .unwrap();
                unsafe {
                    raylib::ffi::rlDisableShader();
                }
                data
            };
            let a = capture_vertices(&mut direct);
            let b = capture_vertices(&mut cached);
            for (a, b) in a.iter().zip(&b) {
                for (a, b) in a.iter().zip(b) {
                    assert!(
                        (a - b).abs() < 2e-4,
                        "{form} cached/direct mismatch: {a} != {b}"
                    );
                }
            }
        }
    }
}
