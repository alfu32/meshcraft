use super::*;
use base64::Engine;
use serde_json::json;

pub(super) fn read(
    bytes: &[u8],
    resource: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    cancel: &AtomicBool,
) -> Result<Mesh, String> {
    let g = ::gltf::Gltf::from_slice(bytes).map_err(|e| format!("glTF: {e}"))?;
    let mut buffers = vec![];
    let mut total = 0;
    for b in g.buffers() {
        check(cancel)?;
        let data = match b.source() {
            ::gltf::buffer::Source::Bin => g.blob.clone().ok_or("GLB binary chunk is missing")?,
            ::gltf::buffer::Source::Uri(uri) => {
                if let Some(uri) = uri.strip_prefix("data:") {
                    let (header, data) = uri.split_once(',').ok_or("invalid glTF data URI")?;
                    if !header.ends_with(";base64") {
                        return Err("glTF buffer data URI must use base64".into());
                    }
                    base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|e| format!("invalid base64 glTF buffer: {e}"))?
                } else {
                    resource(uri)?
                }
            }
        };
        total += data.len();
        if total > MAX_BYTES {
            return Err("glTF buffers exceed 512 MiB".into());
        }
        if data.len() < b.length() {
            return Err("glTF buffer is shorter than byteLength".into());
        }
        buffers.push(data);
    }
    for v in g.views() {
        if v.offset()
            .checked_add(v.length())
            .is_none_or(|end| end > buffers[v.buffer().index()].len())
        {
            return Err("glTF buffer view is out of bounds".into());
        }
    }
    for a in g.accessors() {
        if a.count() == 0 || a.count() > MAX_PRIMITIVES * 3 {
            return Err("glTF accessor count is outside supported bounds".into());
        }
        if a.sparse().is_some() {
            return Err("sparse glTF accessors are unsupported; expand them before import".into());
        }
    }
    let mut out = Mesh::default();
    let mut count = 0;
    fn visit(
        node: ::gltf::Node<'_>,
        parent: Affine3,
        buffers: &[Vec<u8>],
        out: &mut Mesh,
        depth: usize,
        count: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check(cancel)?;
        *count += 1;
        if depth > 64 || *count > 100_000 {
            return Err("glTF node hierarchy is cyclic or exceeds traversal limits".into());
        }
        let a = node.transform().matrix();
        let local = Affine3 {
            linear: [
                [a[0][0] as f64, a[1][0] as f64, a[2][0] as f64],
                [a[0][1] as f64, a[1][1] as f64, a[2][1] as f64],
                [a[0][2] as f64, a[1][2] as f64, a[2][2] as f64],
            ],
            translation: Vec3::new(a[3][0] as f64, a[3][1] as f64, a[3][2] as f64),
        };
        let t = parent.compose(local);
        if let Some(mesh) = node.mesh() {
            for p in mesh.primitives() {
                check(cancel)?;
                let reader = p.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
                let points = reader
                    .read_positions()
                    .ok_or("glTF primitive is missing readable POSITION data")?
                    .map(|p| t.transform_point(Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64)))
                    .collect::<Vec<_>>();
                let indices = if let Some(i) = reader.read_indices() {
                    i.into_u32().map(|i| i as usize).collect::<Vec<_>>()
                } else {
                    (0..points.len()).collect()
                };
                if indices.len() > MAX_PRIMITIVES * 3 || indices.iter().any(|i| *i >= points.len())
                {
                    return Err("glTF primitive indices are out of bounds".into());
                }
                let base = p
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_factor()
                    .map(f64::from);
                let vertex_colors = reader
                    .read_colors(0)
                    .map(|c| c.into_rgba_f32().collect::<Vec<_>>());
                if vertex_colors
                    .as_ref()
                    .is_some_and(|c| c.len() != points.len())
                {
                    return Err("glTF color accessor count differs from positions".into());
                }
                let c = |i: usize| {
                    vertex_colors.as_ref().map_or_else(
                        || color(base),
                        |colors| color(std::array::from_fn(|j| base[j] * colors[i][j] as f64)),
                    )
                };
                let mut triangle = |ids: [usize; 3]| {
                    let mut pts = ids.map(|i| points[i]);
                    if t.determinant() < 0. {
                        pts.swap(1, 2);
                    }
                    out.triangles.push((pts, c(ids[0])));
                };
                match p.mode() {
                    ::gltf::mesh::Mode::Triangles => {
                        if indices.len() % 3 != 0 {
                            return Err(
                                "glTF triangle index count is not divisible by three".into()
                            );
                        }
                        for v in indices.as_chunks::<3>().0 {
                            triangle([v[0], v[1], v[2]]);
                        }
                    }
                    ::gltf::mesh::Mode::TriangleStrip => {
                        for (i, v) in indices.windows(3).enumerate() {
                            triangle(if i % 2 == 0 {
                                [v[0], v[1], v[2]]
                            } else {
                                [v[1], v[0], v[2]]
                            });
                        }
                    }
                    ::gltf::mesh::Mode::TriangleFan => {
                        for v in indices[1.min(indices.len())..].windows(2) {
                            triangle([indices[0], v[0], v[1]]);
                        }
                    }
                    ::gltf::mesh::Mode::Lines => {
                        if indices.len() % 2 != 0 {
                            return Err("glTF line index count is odd".into());
                        }
                        for v in indices.as_chunks::<2>().0 {
                            out.segments.push(([points[v[0]], points[v[1]]], c(v[0])));
                        }
                    }
                    ::gltf::mesh::Mode::LineStrip | ::gltf::mesh::Mode::LineLoop => {
                        for v in indices.windows(2) {
                            out.segments.push(([points[v[0]], points[v[1]]], c(v[0])));
                        }
                        if p.mode() == ::gltf::mesh::Mode::LineLoop && indices.len() > 2 {
                            out.segments.push((
                                [points[*indices.last().unwrap()], points[indices[0]]],
                                c(indices[0]),
                            ));
                        }
                    }
                    _ => out.warn("glTF point primitives are omitted."),
                }
                if p.morph_targets().next().is_some() {
                    out.warn(
                        "glTF morph targets are not evaluated; static base geometry is imported.",
                    );
                }
                if vertex_colors.is_some() {
                    out.warn("glTF vertex colors are reduced to one base color per primitive face or segment.");
                }
                if p.material()
                    .pbr_metallic_roughness()
                    .base_color_texture()
                    .is_some()
                {
                    out.warn("glTF textures are omitted; base colors are retained.");
                }
                out.check_size()?;
            }
        }
        if node.skin().is_some() {
            out.warn("glTF skinning is not evaluated; static source geometry is imported.");
        }
        for child in node.children() {
            visit(child, t, buffers, out, depth + 1, count, cancel)?;
        }
        Ok(())
    }
    let scene = g
        .default_scene()
        .or_else(|| g.scenes().next())
        .ok_or("glTF has no scene")?;
    for node in scene.nodes() {
        visit(
            node,
            Affine3::IDENTITY,
            &buffers,
            &mut out,
            0,
            &mut count,
            cancel,
        )?;
    }
    if g.animations().next().is_some() {
        out.warn("glTF animation is omitted; initial node transforms are used.");
    }
    Ok(out)
}

pub(super) fn write(m: &Mesh, format: Format, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut groups = BTreeMap::<(u32, [u8; 4]), Vec<Vec3>>::new();
    for (p, c) in &m.triangles {
        check(cancel)?;
        groups
            .entry((4, [c.red, c.green, c.blue, c.alpha]))
            .or_default()
            .extend(p);
    }
    for (p, c) in &m.segments {
        groups
            .entry((1, [c.red, c.green, c.blue, c.alpha]))
            .or_default()
            .extend(p);
    }
    let mut binary = vec![];
    let mut views = vec![];
    let mut accessors = vec![];
    let mut primitives = vec![];
    let mut materials = vec![];
    if groups.len() > 4096 {
        return Err("glTF export exceeds the 4096 material-group limit".into());
    }
    for ((mode, c), points) in groups {
        check(cancel)?;
        let offset = binary.len();
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in &points {
            for (j, n) in [p.x, p.y, p.z].into_iter().enumerate() {
                let n = n as f32;
                if !n.is_finite() {
                    return Err("glTF coordinates exceed float32 range".into());
                }
                min[j] = min[j].min(n as f64);
                max[j] = max[j].max(n as f64);
                binary.extend_from_slice(&n.to_le_bytes());
            }
        }
        let view = views.len();
        views.push(
            json!({"buffer":0,"byteOffset":offset,"byteLength":binary.len()-offset,"target":34962}),
        );
        let accessor = accessors.len();
        accessors.push(json!({"bufferView":view,"componentType":5126,"count":points.len(),"type":"VEC3","min":min,"max":max}));
        let mat = materials.len();
        materials.push(json!({"name":format!("RGBA_{:02x}{:02x}{:02x}{:02x}",c[0],c[1],c[2],c[3]),"pbrMetallicRoughness":{"baseColorFactor":c.map(|v|v as f64/255.),"metallicFactor":0.,"roughnessFactor":0.8},"alphaMode":if c[3]<255{"BLEND"}else{"OPAQUE"},"doubleSided":true}));
        let mut attributes = json!({"POSITION":accessor});
        if mode == 4 {
            let offset = binary.len();
            for p in points.as_chunks::<3>().0 {
                let n = normal([p[0], p[1], p[2]]);
                for _ in 0..3 {
                    for v in [n.x, n.y, n.z] {
                        binary.extend_from_slice(&(v as f32).to_le_bytes());
                    }
                }
            }
            let view = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":binary.len()-offset,"target":34962}));
            let normal = accessors.len();
            accessors.push(
                json!({"bufferView":view,"componentType":5126,"count":points.len(),"type":"VEC3"}),
            );
            attributes["NORMAL"] = json!(normal);
        }
        primitives.push(json!({"attributes":attributes,"material":mat,"mode":mode}));
    }
    let mut doc = json!({"asset":{"version":"2.0","generator":"Meshcraft"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":primitives}],"materials":materials,"buffers":[{"byteLength":binary.len()}],"bufferViews":views,"accessors":accessors});
    if format == Format::Gltf {
        doc["buffers"][0]["uri"] = json!(format!(
            "data:application/octet-stream;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&binary)
        ));
        return serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string());
    }
    let mut json = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while binary.len() % 4 != 0 {
        binary.push(0);
    }
    let length = 12 + 8 + json.len() + 8 + binary.len();
    if length > u32::MAX as usize {
        return Err("GLB exceeds 4 GiB".into());
    }
    let mut result = b"glTF".to_vec();
    for n in [2, length as u32, json.len() as u32, 0x4e4f534a] {
        result.extend_from_slice(&n.to_le_bytes());
    }
    result.extend(json);
    for n in [binary.len() as u32, 0x004e4942] {
        result.extend_from_slice(&n.to_le_bytes());
    }
    result.extend(binary);
    Ok(result)
}
