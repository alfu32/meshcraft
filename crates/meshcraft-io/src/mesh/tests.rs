use super::*;
use meshcraft_core::selection::SelectionTarget;
use std::io::{Cursor, Write};

fn cancel() -> AtomicBool {
    AtomicBool::new(false)
}
fn decode_raw(b: &[u8], f: Format, o: &Options) -> Result<Mesh, String> {
    decode(
        b,
        f,
        o,
        &mut |_| Err("missing local resource".into()),
        &cancel(),
    )
}
fn tetra() -> Mesh {
    let p = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z];
    Mesh {
        triangles: [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]
            .map(|f| (f.map(|i| p[i]), Rgba8::new(120, 80, 40, 190)))
            .to_vec(),
        ..Default::default()
    }
}
fn bounds(m: &Mesh) -> [Vec3; 2] {
    let mut a = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut b = -a;
    for p in m
        .triangles
        .iter()
        .flat_map(|(p, _)| p)
        .chain(m.segments.iter().flat_map(|(p, _)| p))
    {
        a = Vec3::new(a.x.min(p.x), a.y.min(p.y), a.z.min(p.z));
        b = Vec3::new(b.x.max(p.x), b.y.max(p.y), b.z.max(p.z));
    }
    [a, b]
}
fn close(a: Vec3, b: Vec3) {
    assert!(a.distance(b) < 1e-6, "{a:?} != {b:?}");
}
fn zipped(name: &str, data: &str) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(vec![]));
    z.start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(data.as_bytes()).unwrap();
    z.finish().unwrap().into_inner()
}

#[test]
fn all_format_round_trips_have_correct_units_and_triangle_winding() {
    for f in [
        Format::Obj,
        Format::Stl,
        Format::StlAscii,
        Format::StlBinary,
        Format::Fbx,
        Format::Gltf,
        Format::Glb,
        Format::Dae,
        Format::Dxf,
        Format::ThreeMf,
        Format::Amf,
        Format::Ifc,
    ] {
        let mut m = tetra();
        m.transform(Affine3::from_translation(Vec3::new(3., 2., 1.)));
        let options = Options {
            meters_per_model_unit: 0.001,
            unit: if matches!(f, Format::Gltf | Format::Glb | Format::Ifc) {
                None
            } else {
                Some(Unit::MILLIMETER)
            },
            scale: 1.,
        };
        let (b, _) =
            encode(&m, f, &options, &cancel()).unwrap_or_else(|e| panic!("{f:?} export: {e}"));
        let n = decode_raw(&b, f, &options).unwrap_or_else(|e| panic!("{f:?} import: {e}"));
        assert_eq!(n.triangles.len(), 4, "{f:?}");
        let [a, b] = bounds(&n);
        close(a, Vec3::new(3., 2., 1.));
        close(b, Vec3::new(4., 3., 2.));
        let volume = n
            .triangles
            .iter()
            .map(|(p, _)| p[0].dot(p[1].cross(p[2])) / 6.)
            .sum::<f64>();
        assert!((volume - 1. / 6.).abs() < 1e-6, "{f:?}: {volume}");
    }
}
#[test]
fn line_only_formats_preserve_segments() {
    let mut m = Mesh::default();
    m.segments.push(([Vec3::X, Vec3::Z], Rgba8::WHITE));
    for f in [
        Format::Obj,
        Format::Dxf,
        Format::Gltf,
        Format::Glb,
        Format::Dae,
        Format::Ifc,
    ] {
        let (b, _) = encode(&m, f, &Options::default(), &cancel()).unwrap();
        let n = decode_raw(&b, f, &Options::default()).unwrap();
        assert_eq!(n.segments.len(), 1, "{f:?}");
        close(n.segments[0].0[0], Vec3::X);
        close(n.segments[0].0[1], Vec3::Z);
    }
    for f in [Format::Stl, Format::Fbx, Format::Amf, Format::ThreeMf] {
        assert!(encode(&m, f, &Options::default(), &cancel()).is_err());
    }
}
#[test]
fn obj_concave_negative_indices_lines_and_mtl() {
    let obj=b"mtllib colors.mtl\nv 0 0 0\nv 2 0 0\nv 2 2 0\nv 1 1 0\nv 0 2 0\nusemtl red\nf -5 -4 -3 -2 -1\nl 1 2 3\n";
    let m = decode(
        obj,
        Format::Obj,
        &Options::default(),
        &mut |_| Ok(b"newmtl red\nKd 1 0 0\nd 0.5\n".to_vec()),
        &cancel(),
    )
    .unwrap();
    assert_eq!(m.triangles.len(), 3);
    assert_eq!(m.segments.len(), 2);
    assert_eq!(m.triangles[0].1, Rgba8::new(255, 0, 0, 128));
    let area = m
        .triangles
        .iter()
        .map(|(p, _)| (p[1] - p[0]).cross(p[2] - p[0]).length() / 2.)
        .sum::<f64>();
    assert!((area - 3.).abs() < 1e-10);
}
#[test]
fn malformed_obj_and_stl_fail_instead_of_silently_truncating() {
    for b in [
        b"v 0 0 0\nf 1 2 3".as_slice(),
        b"v NaN 0 0",
        b"v 0 0 0\nf 0 0 0",
    ] {
        assert!(decode_raw(b, Format::Obj, &Options::default()).is_err());
    }
    assert!(
        decode_raw(
            b"solid x\nfacet normal 0 0 1\nvertex 0 0 0",
            Format::Stl,
            &Options::default()
        )
        .is_err()
    );
    let mut bytes = vec![0; 84];
    bytes[80..84].copy_from_slice(&100u32.to_le_bytes());
    assert!(decode_raw(&bytes, Format::StlBinary, &Options::default()).is_err());
}
#[test]
fn binary_stl_header_may_start_with_solid() {
    let (mut b, _) = encode(&tetra(), Format::StlBinary, &Options::default(), &cancel()).unwrap();
    b[..5].copy_from_slice(b"solid");
    assert_eq!(
        decode_raw(&b, Format::Stl, &Options::default())
            .unwrap()
            .triangles
            .len(),
        4
    );
}
#[test]
fn upstream_khronos_box_imports_scene_transforms_and_material() {
    let m = decode_raw(
        include_bytes!("../../tests/fixtures/Box.glb"),
        Format::Glb,
        &Options::default(),
    )
    .unwrap();
    assert_eq!(m.triangles.len(), 12);
    let [a, b] = bounds(&m);
    close(a, Vec3::new(-0.5, -0.5, -0.5));
    close(b, Vec3::new(0.5, 0.5, 0.5));
    assert_ne!(m.triangles[0].1, Rgba8::WHITE);
}
#[test]
fn legacy_ascii_fbx_fixture_imports() {
    let m = decode_raw(
        include_bytes!("../../tests/fixtures/legacy-wall.fbx"),
        Format::Fbx,
        &Options::default(),
    )
    .unwrap();
    assert!(!m.triangles.is_empty());
    assert!(
        decode_raw(b"Kaydara FBX Binary  \0", Format::Fbx, &Options::default())
            .unwrap_err()
            .contains("binary FBX")
    );
}
#[test]
fn gltf_nested_instances_mirrors_stride_and_external_buffer() {
    let mut data = vec![];
    for p in [[0f32, 0., 0., 99.], [1., 0., 0., 99.], [0., 1., 0., 99.]] {
        for v in p {
            data.extend_from_slice(&v.to_le_bytes());
        }
    }
    let json = serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"uri":"positions.bin","byteLength":48}],"bufferViews":[{"buffer":0,"byteLength":48,"byteStride":16}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"translation":[10,0,0],"children":[1,2]},{"mesh":0},{"mesh":0,"scale":[-1,1,1]}],"scenes":[{"nodes":[0]}],"scene":0});
    let m = decode(
        &serde_json::to_vec(&json).unwrap(),
        Format::Gltf,
        &Options::default(),
        &mut |uri| {
            assert_eq!(uri, "positions.bin");
            Ok(data.clone())
        },
        &cancel(),
    )
    .unwrap();
    assert_eq!(m.triangles.len(), 2);
    let [a, b] = bounds(&m);
    close(a, Vec3::new(9., 0., 0.));
    close(b, Vec3::new(11., 1., 0.));
    for (p, _) in m.triangles {
        assert!(normal(p).z > 0.);
    }
}
#[test]
fn gltf_rejects_out_of_bounds_and_sparse_without_panicking() {
    let (b, _) = encode(&tetra(), Format::Gltf, &Options::default(), &cancel()).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&b).unwrap();
    for count in [0, 10000000] {
        let mut v = original.clone();
        v["accessors"][0]["count"] = count.into();
        assert!(
            decode_raw(
                &serde_json::to_vec(&v).unwrap(),
                Format::Gltf,
                &Options::default()
            )
            .is_err()
        );
    }
    let mut v = original;
    v["bufferViews"][0]["byteLength"] = 999999.into();
    assert!(
        decode_raw(
            &serde_json::to_vec(&v).unwrap(),
            Format::Gltf,
            &Options::default()
        )
        .is_err()
    );
}
#[test]
fn three_mf_components_build_transforms_and_units_are_composed_once() {
    let xml = r#"<model xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" unit="centimeter"><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object><object id="2"><components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 2 0 0"/></components></object></resources><build><item objectid="2" transform="1 0 0 0 1 0 0 0 1 3 0 0"/></build></model>"#;
    let m = decode_raw(
        &zipped("3D/3dmodel.model", xml),
        Format::ThreeMf,
        &Options {
            meters_per_model_unit: 0.001,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(m.triangles.len(), 1);
    close(bounds(&m)[0], Vec3::new(50., 0., 0.));
    close(bounds(&m)[1], Vec3::new(60., 10., 0.));
}
#[test]
fn three_mf_cycles_invalid_indices_and_open_meshes_fail() {
    let xml = r#"<model><resources><object id="1"><components><component objectid="1"/></components></object></resources><build><item objectid="1"/></build></model>"#;
    assert!(
        decode_raw(
            &zipped("3D/3dmodel.model", xml),
            Format::ThreeMf,
            &Options::default()
        )
        .unwrap_err()
        .contains("cyclic")
    );
    let mut m = tetra();
    m.triangles.pop();
    assert!(
        encode(&m, Format::ThreeMf, &Options::default(), &cancel())
            .unwrap_err()
            .contains("manifold")
    );
    let mut m = tetra();
    m.triangles[0].0.swap(1, 2);
    assert!(encode(&m, Format::ThreeMf, &Options::default(), &cancel()).is_err());
}
#[test]
fn dae_concave_polylist_accessor_stride_and_node_transform() {
    let xml = r##"<COLLADA><asset><unit meter="0.01"/></asset><library_geometries><geometry id="g"><mesh><source id="p"><float_array>0 0 0 8 2 0 0 8 2 2 0 8 1 1 0 8 0 2 0 8</float_array><technique_common><accessor stride="4" count="5"/></technique_common></source><vertices id="v"><input semantic="POSITION" source="#p"/></vertices><polylist count="1"><input semantic="VERTEX" source="#v" offset="0"/><vcount>5</vcount><p>0 1 2 3 4</p></polylist></mesh></geometry></library_geometries><library_visual_scenes><visual_scene id="s"><node><translate>4 0 0</translate><node><instance_geometry url="#g"/></node></node></visual_scene></library_visual_scenes><scene><instance_visual_scene url="#s"/></scene></COLLADA>"##;
    let m = decode_raw(
        xml.as_bytes(),
        Format::Dae,
        &Options {
            meters_per_model_unit: 0.01,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(m.triangles.len(), 3);
    close(bounds(&m)[0], Vec3::new(4., 0., 0.));
    close(bounds(&m)[1], Vec3::new(6., 2., 0.));
}
#[test]
fn amf_vertices_are_scoped_per_object_and_units_respected() {
    let object = |x| {
        format!(
            "<object id=\"{x}\"><mesh><vertices><vertex><coordinates><x>{x}</x><y>0</y><z>0</z></coordinates></vertex><vertex><coordinates><x>{}</x><y>0</y><z>0</z></coordinates></vertex><vertex><coordinates><x>{x}</x><y>1</y><z>0</z></coordinates></vertex></vertices><volume><triangle><v1>0</v1><v2>1</v2><v3>2</v3></triangle></volume></mesh></object>",
            x + 1
        )
    };
    let xml = format!("<amf unit=\"millimeter\">{}{}</amf>", object(0), object(5));
    let m = decode_raw(
        xml.as_bytes(),
        Format::Amf,
        &Options {
            meters_per_model_unit: 0.001,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(m.triangles.len(), 2);
    close(bounds(&m)[1], Vec3::new(6., 1., 0.));
}
#[test]
fn dxf_lines_quads_polylines_units_and_unsupported_diagnostics() {
    let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n0\n20\n0\n11\n10\n21\n0\n0\n3DFACE\n10\n0\n20\n0\n11\n1\n21\n0\n12\n1\n22\n1\n13\n0\n23\n1\n0\nLWPOLYLINE\n70\n1\n10\n0\n20\n0\n10\n1\n20\n0\n10\n0\n20\n1\n0\nINSERT\n2\nunknown\n0\nENDSEC\n0\nEOF\n";
    let m = decode_raw(
        dxf.as_bytes(),
        Format::Dxf,
        &Options {
            meters_per_model_unit: 0.001,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(m.triangles.len(), 2);
    assert_eq!(m.segments.len(), 4);
    close(bounds(&m)[1], Vec3::new(10., 1., 0.));
    assert!(m.warnings.iter().any(|w| w.contains("INSERT")));
}
#[test]
fn ifc_export_contains_hierarchy_proxy_and_geometric_representation() {
    let (b, _) = encode(&tetra(), Format::Ifc, &Options::default(), &cancel()).unwrap();
    let s = utf8(&b).unwrap();
    for k in [
        "IFCPROJECT(",
        "IFCSITE(",
        "IFCBUILDING(",
        "IFCBUILDINGSTOREY(",
        "IFCBUILDINGELEMENTPROXY(",
        "IFCTRIANGULATEDFACESET(",
        "IFCRELCONTAINEDINSPATIALSTRUCTURE(",
    ] {
        assert!(s.contains(k));
    }
    assert!(!s.contains("IFCWALL("));
}
#[test]
fn ifc_local_placement_and_si_prefix_are_applied() {
    let s = "ISO-10303-21;DATA;#1=IFCCARTESIANPOINT((10.,0.,0.));#2=IFCAXIS2PLACEMENT3D(#1,$,$);#3=IFCLOCALPLACEMENT($,#2);#4=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1.,0.,0.),(0.,1.,0.)));#5=IFCTRIANGULATEDFACESET(#4,$,.F.,((1,2,3)),$);#6=IFCSHAPEREPRESENTATION($,'Body','Tessellation',(#5));#7=IFCPRODUCTDEFINITIONSHAPE($,$,(#6));#8=IFCBUILDINGELEMENTPROXY('a',$,'x',$,$,#3,#7,$,.NOTDEFINED.);#9=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);ENDSEC;END-ISO-10303-21;";
    let m = decode_raw(
        s.as_bytes(),
        Format::Ifc,
        &Options {
            meters_per_model_unit: 0.001,
            ..Default::default()
        },
    )
    .unwrap();
    close(bounds(&m)[0], Vec3::new(10., 0., 0.));
    close(bounds(&m)[1], Vec3::new(11., 1., 0.));
}
#[test]
fn document_export_resolves_nested_occurrences_and_selection() {
    let mut d = tetra().into_document().unwrap();
    let source = d.clone();
    let prototype = d.import_subscene(&source, "part", None, None).unwrap();
    let root = d.root_layout();
    let a = d
        .add_layout_entry(root, prototype, Affine3::from_translation(Vec3::X * 5.))
        .unwrap();
    let b = d
        .add_layout_entry(root, prototype, Affine3::from_translation(Vec3::X * 10.))
        .unwrap();
    let all = Mesh::from_document(&d, None, &cancel()).unwrap();
    assert_eq!(all.triangles.len(), 12);
    let mut selection = SelectionSession::default();
    selection.select(SelectionTarget {
        occurrence: OccurrenceRef::root(b),
        entity: SelectedEntity::Occurrence,
    });
    let selected = Mesh::from_document(&d, Some(&selection), &cancel()).unwrap();
    assert_eq!(selected.triangles.len(), 4);
    close(bounds(&selected)[0], Vec3::X * 10.);
    assert_ne!(a, b);
}
#[test]
fn cancellation_and_invalid_inputs_do_not_return_geometry() {
    let c = AtomicBool::new(true);
    assert!(
        decode(
            b"v 0 0 0",
            Format::Obj,
            &Options::default(),
            &mut |_| unreachable!(),
            &c
        )
        .is_err()
    );
    assert!(encode(&tetra(), Format::Obj, &Options::default(), &c).is_err());
    for value in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(
            encode(
                &tetra(),
                Format::Obj,
                &Options {
                    scale: value,
                    ..Default::default()
                },
                &cancel()
            )
            .is_err()
        );
    }
    assert!(
        decode_raw(
            b"<!DOCTYPE amf [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><amf>&x;</amf>",
            Format::Amf,
            &Options::default()
        )
        .is_err()
    );
}
#[test]
fn filename_units_and_scale_are_not_double_applied() {
    assert_eq!(
        Unit::from_filename(Path::new("part_mm.obj")),
        Some(Unit::MILLIMETER)
    );
    let o = Options {
        meters_per_model_unit: 0.001,
        unit: Some(Unit::MILLIMETER),
        scale: 2.,
    };
    let (b, _) = encode(&tetra(), Format::Obj, &o, &cancel()).unwrap();
    let m = decode_raw(&b, Format::Obj, &Options::default()).unwrap();
    close(bounds(&m)[1], Vec3::new(2., 2., 2.));
}
#[test]
fn raw_import_uses_exact_indexing_and_preserves_small_features() {
    let mut m = Mesh::default();
    m.segments
        .push(([Vec3::ZERO, Vec3::X * 1e-8], Rgba8::WHITE));
    m.segments.push(([Vec3::ZERO, Vec3::Y], Rgba8::WHITE));
    let doc = m.into_document().unwrap();
    let g = doc
        .effective_geometry(&OccurrenceRef::scene_root())
        .unwrap();
    assert_eq!(g.vertices.len(), 3);
    assert_eq!(g.segments.len(), 2);
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn local_resources_cannot_escape_source_directory() {
    let path = std::env::temp_dir().join(format!(
        "meshcraft-mesh-resource-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    let file = path.join("test.obj");
    std::fs::write(
        &file,
        "mtllib ../outside.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3",
    )
    .unwrap();
    let m = read_file(&file, &Options::default(), &cancel()).unwrap();
    assert!(m.warnings.iter().any(|s| s.contains("not loaded")));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn architectural_owner_selection_exports_only_its_attributed_geometry() {
    use meshcraft_core::document::architecture::{Element, Wall};
    use meshcraft_core::selection::SelectionKind;
    let mut d = Document::new();
    d.ensure_scene_root();
    let a = d
        .put_architecture(
            Document::SCENE_ROOT_DEFINITION_ID,
            None,
            Element::Wall(Wall::default()),
        )
        .unwrap();
    let b = d
        .put_architecture(
            Document::SCENE_ROOT_DEFINITION_ID,
            None,
            Element::Wall(Wall {
                start: Vec3::X * 20.,
                end: Vec3::X * 24.,
                ..Default::default()
            }),
        )
        .unwrap();
    let target = SelectionTarget {
        occurrence: OccurrenceRef::scene_root(),
        entity: SelectedEntity::SemanticHandle {
            kind: SelectionKind::Wall,
            id: a,
        },
    };
    let expected = d.architecture_mesh(&target).unwrap();
    let mut selection = SelectionSession::default();
    selection.select(target);
    let exported = Mesh::from_document(&d, Some(&selection), &cancel()).unwrap();
    assert_eq!(exported.triangles.len(), expected.faces.len());
    assert_eq!(exported.segments.len(), expected.edges.len());
    assert!(bounds(&exported)[1].x < 20.);
    assert!(!b.is_empty());
}
#[test]
fn export_uses_current_script_frame_without_running_the_script() {
    use meshcraft_core::document::scripts::{Drawable, ScriptDefinition, ScriptFrame, ScriptState};
    let mut d = Document::new();
    let path = d
        .add_scripted_object("generated", ScriptDefinition::default())
        .unwrap();
    let g = d.effective_geometry(&path).unwrap().clone();
    let drawable:Drawable=serde_json::from_value(serde_json::json!({"kind":"face","id":"triangle","points":[{"x":5,"y":0,"z":0},{"x":6,"y":0,"z":0},{"x":5,"y":1,"z":0}],"color":{"red":255,"green":0,"blue":0,"alpha":128}})).unwrap();
    let frame = ScriptFrame::build(&g, vec![drawable], ScriptState::default()).unwrap();
    d.install_script_frames(BTreeMap::from([(path, frame)]));
    let frozen = d.clone();
    let mesh = Mesh::from_document(&frozen, None, &cancel()).unwrap();
    assert_eq!(mesh.triangles.len(), 1);
    close(bounds(&mesh)[0], Vec3::X * 5.);
    assert_eq!(mesh.triangles[0].1.alpha, 128);
}
#[test]
fn gltf_retains_rgba_and_xml_attribute_limits_reject_hostile_tags() {
    let (b, _) = encode(&tetra(), Format::Glb, &Options::default(), &cancel()).unwrap();
    let m = decode_raw(&b, Format::Glb, &Options::default()).unwrap();
    assert_eq!(m.triangles[0].1, Rgba8::new(120, 80, 40, 190));
    let attrs = (0..200).map(|i| format!(" a{i}=\"x\"")).collect::<String>();
    let b = format!("<amf{attrs}/>");
    assert!(
        decode_raw(b.as_bytes(), Format::Amf, &Options::default())
            .unwrap_err()
            .contains("128 attribute")
    );
}

#[test]
fn ifc_keeps_named_occurrence_proxies_and_brep_outer_bound_orientation() {
    let mut d = Document::new();
    d.add_cuboid("First's box", Vec3::new(1., 1., 1.), Vec3::ZERO)
        .unwrap();
    d.add_cuboid("Second", Vec3::new(1., 1., 1.), Vec3::X * 4.)
        .unwrap();
    let mesh = Mesh::from_document(&d, None, &cancel()).unwrap();
    let (bytes, _) = encode(&mesh, Format::Ifc, &Options::default(), &cancel()).unwrap();
    let text = utf8(&bytes).unwrap();
    assert_eq!(text.matches("=IFCBUILDINGELEMENTPROXY(").count(), 2);
    assert!(text.contains("'First''s box'"));
    assert!(text.contains("'Second'"));
    assert_eq!(
        decode_raw(&bytes, Format::Ifc, &Options::default())
            .unwrap()
            .triangles
            .len(),
        24
    );
    let s = "ISO-10303-21;DATA;/* existing independent BREP fixture */#1=IFCCARTESIANPOINT((0.,0.,0.));#2=IFCCARTESIANPOINT((1.,0.,0.));#3=IFCCARTESIANPOINT((0.,1.,0.));#4=IFCPOLYLOOP((#1,#2,#3));#5=IFCFACEOUTERBOUND(#4,.F.);#6=IFCFACE((#5));#7=IFCCLOSEDSHELL((#6));#8=IFCFACETEDBREP(#7);ENDSEC;END-ISO-10303-21;";
    let mesh = decode_raw(s.as_bytes(), Format::Ifc, &Options::default()).unwrap();
    assert_eq!(mesh.triangles.len(), 1);
    assert!(normal(mesh.triangles[0].0).z < 0.);
}

#[test]
fn concave_triangulation_is_independent_of_coordinate_scale_and_origin() {
    for (offset, scale) in [(Vec3::ZERO, 1e-12), (Vec3::X * 1e9, 1.)] {
        let mut m = Mesh::default();
        let points = [[0., 0.], [2., 0.], [2., 2.], [1., 1.], [0., 2.]]
            .map(|[x, y]| offset + Vec3::new(x * scale, y * scale, 0.));
        polygon(&mut m, &points, Rgba8::WHITE).unwrap();
        assert_eq!(m.triangles.len(), 3);
    }
}
