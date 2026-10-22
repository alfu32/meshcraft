use super::*;
use roxmltree::{Document as Xml, Node};
use std::fmt::Write;
use std::io::{Cursor, Read, Write as IoWrite};

fn parse(s: &str) -> Result<Xml<'_>, String> {
    // Bound attribute/namespace validation before roxmltree sees the input. Older
    // parser releases compare attributes pairwise; limiting each tag avoids quadratic
    // behavior on hostile XML without changing normal CAD documents.
    let b = s.as_bytes();
    let mut i = 0;
    let mut depth = 0usize;
    let mut nodes = 0usize;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if s[i..].starts_with("<!--") {
            i += s[i + 4..].find("-->").ok_or("unterminated XML comment")? + 7;
            continue;
        }
        if s[i..].starts_with("<![CDATA[") {
            i += s[i + 9..].find("]]>").ok_or("unterminated XML CDATA")? + 12;
            continue;
        }
        if s[i..].starts_with("<!") {
            return Err("XML declarations/DTDs are unsupported".into());
        }
        let start = i;
        let closing = b.get(i + 1) == Some(&b'/');
        let instruction = b.get(i + 1) == Some(&b'?');
        let mut quote = None;
        let mut attrs = 0;
        i += 1;
        while i < b.len() {
            let c = b[i];
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
            } else if matches!(c, b'\'' | b'"') {
                quote = Some(c);
            } else if c == b'=' {
                attrs += 1;
                if attrs > 128 {
                    return Err("XML tag exceeds the 128 attribute limit".into());
                }
            } else if c == b'>' {
                break;
            }
            i += 1;
            if i - start > 65536 {
                return Err("XML tag exceeds 64 KiB".into());
            }
        }
        if i >= b.len() {
            return Err("unterminated XML tag".into());
        }
        if !instruction {
            nodes += 1;
            if nodes > 2_000_000 {
                return Err("XML exceeds node limit".into());
            }
            if closing {
                depth = depth.saturating_sub(1);
            } else if b.get(i.wrapping_sub(1)) != Some(&b'/') {
                depth += 1;
                if depth > 256 {
                    return Err("XML nesting exceeds 256".into());
                }
            }
        }
        i += 1;
    }
    Xml::parse_with_options(
        s,
        roxmltree::ParsingOptions {
            nodes_limit: 2_000_000,
            allow_dtd: false,
            ..Default::default()
        },
    )
    .map_err(|e| format!("XML: {e}"))
}
fn children<'a, 'b>(n: Node<'a, 'b>, name: &'a str) -> impl Iterator<Item = Node<'a, 'b>> {
    n.children()
        .filter(move |n| n.is_element() && n.tag_name().name() == name)
}
fn child<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Result<Node<'a, 'b>, String> {
    n.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .ok_or_else(|| format!("missing XML {name}"))
}
fn attr(n: Node<'_, '_>, key: &str) -> Result<f64, String> {
    number(
        n.attribute(key)
            .ok_or_else(|| format!("missing XML attribute {key}"))?,
    )
}
fn index(n: Node<'_, '_>, key: &str) -> Result<usize, String> {
    n.attribute(key)
        .ok_or_else(|| format!("missing index {key}"))?
        .parse()
        .map_err(|_| format!("invalid index {key}"))
}
fn text_num(n: Node<'_, '_>, name: &str) -> Result<f64, String> {
    number(child(n, name)?.text().unwrap_or(""))
}
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn archive_entry(bytes: &[u8], extension: &str, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    // Inspect the bounded end record before allocating the archive directory. ZIP64
    // is unnecessary below our 512 MiB limit and is deliberately not accepted.
    let start = bytes.len().saturating_sub(65557);
    let eocd = (start..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| bytes.get(i..i + 4) == Some(b"PK\x05\x06".as_slice()))
        .ok_or("ZIP end record is missing")?;
    let count = u16::from_le_bytes(bytes[eocd + 10..eocd + 12].try_into().unwrap());
    if count > 10_000 {
        return Err("ZIP exceeds the 10000 entry limit (ZIP64 is unsupported)".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("ZIP: {e}"))?;
    if zip.len() > 10_000 {
        return Err("ZIP has too many entries".into());
    }
    fn load(
        zip: &mut zip::ZipArchive<Cursor<&[u8]>>,
        i: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, String> {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.size() > MAX_BYTES as u64 {
            return Err("decompressed model exceeds 512 MiB".into());
        }
        let mut data = vec![];
        let mut buffer = [0; 65536];
        loop {
            check(cancel)?;
            let n = f.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            if data.len() + n > MAX_BYTES {
                return Err("decompressed model exceeds 512 MiB".into());
            }
            data.extend_from_slice(&buffer[..n]);
        }
        Ok(data)
    }
    let mut matches = vec![];
    let mut relationships = None;
    for i in 0..zip.len() {
        check(cancel)?;
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.name().to_ascii_lowercase().ends_with(extension) {
            matches.push((i, f.name().to_owned()));
        }
        if f.name() == "_rels/.rels" {
            relationships = Some(i);
        }
    }
    let target = if extension == ".model" {
        if let Some(i) = relationships {
            let data = load(&mut zip, i, cancel)?;
            let doc = parse(utf8(&data)?)?;
            let r = doc
                .descendants()
                .find(|n| {
                    n.has_tag_name("Relationship")
                        && n.attribute("Type").is_some_and(|s| s.ends_with("/3dmodel"))
                })
                .ok_or("3MF root relationship has no model")?;
            if r.attribute("TargetMode") == Some("External") {
                return Err("external 3MF model relationships are unsupported".into());
            }
            Some(
                r.attribute("Target")
                    .ok_or("3MF root relationship has no target")?
                    .trim_start_matches('/')
                    .to_owned(),
            )
        } else {
            None
        }
    } else {
        None
    };
    let i = if let Some(target) = target {
        matches
            .iter()
            .find(|(_, n)| *n == target)
            .ok_or("3MF relationship model is missing")?
            .0
    } else {
        matches
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case("3D/3dmodel.model"))
            .or_else(|| matches.first())
            .ok_or("archive has no supported model XML")?
            .0
    };
    load(&mut zip, i, cancel)
}

pub(super) fn read(
    bytes: &[u8],
    format: Format,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<Mesh, String> {
    let data = if format == Format::ThreeMf {
        archive_entry(bytes, ".model", cancel)?
    } else if format == Format::Amf && bytes.starts_with(b"PK") {
        archive_entry(bytes, ".amf", cancel)?
    } else {
        bytes.to_vec()
    };
    let doc = parse(utf8(&data)?)?;
    match format {
        Format::ThreeMf => read_3mf(&doc, options, cancel),
        Format::Amf => read_amf(&doc, options, cancel),
        Format::Dae => read_dae(&doc, options, cancel),
        _ => unreachable!(),
    }
}
fn transform_3mf(n: Node<'_, '_>) -> Result<Affine3, String> {
    let Some(s) = n.attribute("transform") else {
        return Ok(Affine3::IDENTITY);
    };
    let v = numbers(s)?;
    if v.len() != 12 {
        return Err("3MF transform needs 12 coefficients".into());
    }
    Ok(Affine3 {
        linear: [[v[0], v[3], v[6]], [v[1], v[4], v[7]], [v[2], v[5], v[8]]],
        translation: Vec3::new(v[9], v[10], v[11]),
    })
}
fn read_3mf(doc: &Xml<'_>, options: &Options, cancel: &AtomicBool) -> Result<Mesh, String> {
    let root = doc.root_element();
    if root.tag_name().name() != "model" {
        return Err("expected 3MF model".into());
    }
    if root
        .attribute("requiredextensions")
        .is_some_and(|s| !s.trim().is_empty())
    {
        return Err("3MF required extensions are unsupported".into());
    }
    let unit = options
        .unit
        .or_else(|| Unit::parse(root.attribute("unit").unwrap_or("millimeter")))
        .ok_or("unsupported 3MF unit")?;
    let resources = child(root, "resources")?;
    let mut objects = BTreeMap::new();
    for o in children(resources, "object") {
        let id = index(o, "id")?;
        if objects.insert(id, o).is_some() {
            return Err("duplicate 3MF object ID".into());
        }
    }
    fn visit(
        id: usize,
        t: Affine3,
        objects: &BTreeMap<usize, Node<'_, '_>>,
        out: &mut Mesh,
        depth: usize,
        count: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check(cancel)?;
        *count += 1;
        if depth > 64 || *count > 100_000 {
            return Err("3MF components are cyclic or exceed traversal limits".into());
        }
        let o = *objects
            .get(&id)
            .ok_or("3MF component object does not exist")?;
        if let Ok(mesh) = child(o, "mesh") {
            let points = children(child(mesh, "vertices")?, "vertex")
                .map(|n| Ok(Vec3::new(attr(n, "x")?, attr(n, "y")?, attr(n, "z")?)))
                .collect::<Result<Vec<_>, String>>()?;
            for n in children(child(mesh, "triangles")?, "triangle") {
                check(cancel)?;
                let ids = [index(n, "v1")?, index(n, "v2")?, index(n, "v3")?];
                let mut p = [Vec3::ZERO; 3];
                for j in 0..3 {
                    p[j] = t.transform_point(
                        *points
                            .get(ids[j])
                            .ok_or("3MF triangle index out of bounds")?,
                    );
                }
                if t.determinant() < 0. {
                    p.swap(1, 2);
                }
                out.triangles.push((p, Rgba8::WHITE));
                if out.triangles.len() > MAX_PRIMITIVES {
                    return Err("3MF exceeds primitive limit".into());
                }
            }
        } else if let Ok(components) = child(o, "components") {
            for c in children(components, "component") {
                if c.attributes().any(|a| a.name() == "path") {
                    return Err("external 3MF component paths are unsupported".into());
                }
                visit(
                    index(c, "objectid")?,
                    t.compose(transform_3mf(c)?),
                    objects,
                    out,
                    depth + 1,
                    count,
                    cancel,
                )?;
            }
        } else {
            return Err("3MF object has neither mesh nor components".into());
        }
        Ok(())
    }
    let mut out = Mesh::default();
    let mut count = 0;
    for item in children(child(root, "build")?, "item") {
        visit(
            index(item, "objectid")?,
            transform_3mf(item)?,
            &objects,
            &mut out,
            0,
            &mut count,
            cancel,
        )?;
    }
    out.scale(unit.meters / options.meters_per_model_unit);
    if resources
        .children()
        .any(|n| n.is_element() && n.tag_name().name() != "object")
    {
        out.warn("3MF materials, colors and extensions are omitted.");
    }
    Ok(out)
}
fn read_amf(doc: &Xml<'_>, options: &Options, cancel: &AtomicBool) -> Result<Mesh, String> {
    let root = doc.root_element();
    if root.tag_name().name() != "amf" {
        return Err("expected AMF root".into());
    }
    if children(root, "constellation").next().is_some() {
        return Err("AMF constellations are unsupported; flatten instances before import".into());
    }
    let mut out = Mesh::default();
    for object in children(root, "object") {
        let mesh = child(object, "mesh")?;
        let points = children(child(mesh, "vertices")?, "vertex")
            .map(|n| {
                let c = child(n, "coordinates")?;
                Ok(Vec3::new(
                    text_num(c, "x")?,
                    text_num(c, "y")?,
                    text_num(c, "z")?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        for volume in children(mesh, "volume") {
            for tri in children(volume, "triangle") {
                check(cancel)?;
                let mut p = [Vec3::ZERO; 3];
                for (j, key) in ["v1", "v2", "v3"].iter().enumerate() {
                    let i = child(tri, key)?
                        .text()
                        .unwrap_or("")
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| "invalid AMF vertex index")?;
                    p[j] = *points.get(i).ok_or("AMF vertex index out of bounds")?;
                }
                out.triangles.push((p, Rgba8::WHITE));
                if out.triangles.len() > MAX_PRIMITIVES {
                    return Err("too many AMF triangles".into());
                }
            }
        }
    }
    if options.unit.is_none() {
        let unit = Unit::parse(root.attribute("unit").unwrap_or("millimeter"))
            .ok_or("unsupported AMF unit")?;
        out.scale(unit.meters / options.meters_per_model_unit);
    }
    out.warn(
        "AMF imports static triangles only; materials, curved faces and metadata are omitted.",
    );
    Ok(out)
}
fn dae_transform(node: Node<'_, '_>) -> Result<Affine3, String> {
    let mut t = Affine3::IDENTITY;
    for n in node.children().filter(Node::is_element) {
        let name = n.tag_name().name();
        if !matches!(name, "matrix" | "translate" | "rotate" | "scale") {
            continue;
        }
        let v = numbers(n.text().unwrap_or(""))?;
        let local = match name {
            "matrix" => {
                if v.len() != 16 {
                    return Err("Collada matrix needs 16 values".into());
                }
                if v[3] != 0. || v[7] != 0. || v[11] != 0. || v[15] != 1. {
                    return Err("projective Collada transforms are unsupported".into());
                }
                Affine3 {
                    linear: [[v[0], v[4], v[8]], [v[1], v[5], v[9]], [v[2], v[6], v[10]]],
                    translation: Vec3::new(v[12], v[13], v[14]),
                }
            }
            "translate" => {
                if v.len() != 3 {
                    return Err("Collada translate needs three values".into());
                }
                Affine3::from_translation(Vec3::new(v[0], v[1], v[2]))
            }
            "scale" => {
                if v.len() != 3 {
                    return Err("Collada scale needs three values".into());
                }
                Affine3 {
                    linear: [[v[0], 0., 0.], [0., v[1], 0.], [0., 0., v[2]]],
                    translation: Vec3::ZERO,
                }
            }
            _ => {
                if v.len() != 4 {
                    return Err("Collada rotate needs axis and angle".into());
                }
                let a = Vec3::new(v[0], v[1], v[2])
                    .try_normalized(1e-15)
                    .ok_or("invalid Collada rotation axis")?;
                let c = v[3].to_radians().cos();
                let s = v[3].to_radians().sin();
                let d = 1. - c;
                let (x, y, z) = (a.x, a.y, a.z);
                Affine3 {
                    linear: [
                        [c + x * x * d, x * y * d - z * s, x * z * d + y * s],
                        [y * x * d + z * s, c + y * y * d, y * z * d - x * s],
                        [z * x * d - y * s, z * y * d + x * s, c + z * z * d],
                    ],
                    translation: Vec3::ZERO,
                }
            }
        };
        t = t.compose(local);
    }
    Ok(t)
}
fn dae_mesh(node: Node<'_, '_>, cancel: &AtomicBool) -> Result<Mesh, String> {
    let mut sources = BTreeMap::new();
    for source in children(node, "source") {
        let id = source.attribute("id").ok_or("Collada source ID missing")?;
        let Some(array) = children(source, "float_array").next() else {
            continue;
        };
        let values = numbers(array.text().unwrap_or(""))?;
        let accessor = source.descendants().find(|n| n.has_tag_name("accessor"));
        let stride = accessor
            .and_then(|n| n.attribute("stride"))
            .unwrap_or("1")
            .parse::<usize>()
            .map_err(|_| "invalid Collada stride")?;
        let offset = accessor
            .and_then(|n| n.attribute("offset"))
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| "invalid Collada source offset")?;
        sources.insert(id, (values, stride, offset));
    }
    let vertices = children(node, "vertices")
        .filter_map(|n| {
            let id = n.attribute("id")?;
            let input =
                children(n, "input").find(|n| n.attribute("semantic") == Some("POSITION"))?;
            Some((id, input.attribute("source")?.trim_start_matches('#')))
        })
        .collect::<BTreeMap<_, _>>();
    let mut out = Mesh::default();
    for prim in node.children().filter(Node::is_element) {
        check(cancel)?;
        let kind = prim.tag_name().name();
        if !matches!(
            kind,
            "triangles" | "polylist" | "polygons" | "lines" | "linestrips"
        ) {
            if matches!(kind, "trifans" | "tristrips" | "spline") {
                out.warn(format!("Collada {kind} is unsupported."));
            }
            continue;
        }
        let mut stride = 1;
        let mut position = None;
        for input in children(prim, "input") {
            let offset = input
                .attribute("offset")
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| "invalid Collada input offset")?;
            if offset > 64 {
                return Err("Collada input stride exceeds limit".into());
            }
            stride = stride.max(offset + 1);
            let source = input
                .attribute("source")
                .unwrap_or("")
                .trim_start_matches('#');
            match input.attribute("semantic") {
                Some("VERTEX") => {
                    position = Some((
                        offset,
                        *vertices
                            .get(source)
                            .ok_or("missing Collada vertices source")?,
                    ))
                }
                Some("POSITION") => position = Some((offset, source)),
                _ => {}
            }
        }
        let (offset, source) = position.ok_or("Collada primitive has no position input")?;
        let (values, point_stride, point_offset) = sources
            .get(source)
            .ok_or("Collada position source missing")?;
        if *point_stride < 3 {
            return Err("Collada position stride must be at least three".into());
        }
        let mut vcounts = if kind == "polylist" {
            child(prim, "vcount")?
                .text()
                .unwrap_or("")
                .split_whitespace()
                .map(|s| s.parse::<usize>().map_err(|_| "invalid Collada vcount"))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
        } else {
            vec![].into_iter()
        };
        for p in children(prim, "p") {
            let ids = p
                .text()
                .unwrap_or("")
                .split_whitespace()
                .map(|s| s.parse::<usize>().map_err(|_| "invalid Collada index"))
                .collect::<Result<Vec<_>, _>>()?;
            if ids.len() % stride != 0 {
                return Err("Collada input indices do not match stride".into());
            }
            let points = ids
                .chunks_exact(stride)
                .map(|i| {
                    let at = i[offset]
                        .checked_mul(*point_stride)
                        .and_then(|n| n.checked_add(*point_offset))
                        .ok_or("Collada index overflow")?;
                    let v = values
                        .get(at..at.checked_add(3).ok_or("Collada index overflow")?)
                        .ok_or("Collada index out of bounds")?;
                    Ok(Vec3::new(v[0], v[1], v[2]))
                })
                .collect::<Result<Vec<_>, String>>()?;
            match kind {
                "triangles" => {
                    if points.len() % 3 != 0 {
                        return Err("incomplete Collada triangle".into());
                    }
                    for p in points.as_chunks::<3>().0 {
                        out.triangles.push(([p[0], p[1], p[2]], Rgba8::WHITE));
                    }
                }
                "lines" => {
                    if points.len() % 2 != 0 {
                        return Err("incomplete Collada line".into());
                    }
                    for p in points.as_chunks::<2>().0 {
                        out.segments.push(([p[0], p[1]], Rgba8::WHITE));
                    }
                }
                "linestrips" => out
                    .segments
                    .extend(points.windows(2).map(|p| ([p[0], p[1]], Rgba8::WHITE))),
                "polygons" => polygon(&mut out, &points, Rgba8::WHITE)?,
                _ => {
                    let mut at: usize = 0;
                    for count in vcounts.by_ref() {
                        let end = at
                            .checked_add(count)
                            .ok_or("Collada polygon count overflow")?;
                        let p = points.get(at..end).ok_or("Collada vcount out of bounds")?;
                        polygon(&mut out, p, Rgba8::WHITE)?;
                        at = end;
                    }
                    if at != points.len() {
                        return Err("Collada vcount differs from index count".into());
                    }
                }
            }
            out.check_size()?;
        }
        if children(prim, "ph").next().is_some() {
            return Err("Collada polygons with holes are unsupported".into());
        }
    }
    Ok(out)
}
fn read_dae(doc: &Xml<'_>, options: &Options, cancel: &AtomicBool) -> Result<Mesh, String> {
    if doc.root_element().tag_name().name() != "COLLADA" {
        return Err("expected COLLADA root".into());
    }
    let mut geometries = BTreeMap::new();
    for n in doc.descendants().filter(|n| n.has_tag_name("geometry")) {
        check(cancel)?;
        if let Ok(mesh) = child(n, "mesh") {
            geometries.insert(
                n.attribute("id").ok_or("Collada geometry ID missing")?,
                dae_mesh(mesh, cancel)?,
            );
        }
    }
    let mut out = Mesh::default();
    let mut visits = 0;
    fn visit(
        n: Node<'_, '_>,
        t: Affine3,
        g: &BTreeMap<&str, Mesh>,
        out: &mut Mesh,
        depth: usize,
        visits: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check(cancel)?;
        *visits += 1;
        if depth > 64 || *visits > 100_000 {
            return Err("Collada hierarchy exceeds traversal limits".into());
        }
        let t = t.compose(dae_transform(n)?);
        for inst in children(n, "instance_geometry") {
            let id = inst.attribute("url").unwrap_or("").trim_start_matches('#');
            let mut mesh = g
                .get(id)
                .ok_or("Collada instance geometry missing")?
                .clone();
            mesh.transform(t);
            out.triangles.extend(mesh.triangles);
            out.segments.extend(mesh.segments);
            for w in mesh.warnings {
                out.warn(w);
            }
            out.check_size()?;
        }
        if children(n, "instance_node").next().is_some()
            || children(n, "instance_controller").next().is_some()
        {
            return Err(
                "Collada instance_node/controllers are unsupported; bake them before import".into(),
            );
        }
        for n in children(n, "node") {
            visit(n, t, g, out, depth + 1, visits, cancel)?;
        }
        Ok(())
    }
    let scene = doc
        .descendants()
        .find(|n| n.has_tag_name("scene"))
        .and_then(|n| children(n, "instance_visual_scene").next())
        .and_then(|n| n.attribute("url"))
        .map(|s| s.trim_start_matches('#'));
    if let Some(id) = scene {
        let scene = doc
            .descendants()
            .find(|n| n.has_tag_name("visual_scene") && n.attribute("id") == Some(id))
            .ok_or("Collada visual scene missing")?;
        for n in children(scene, "node") {
            visit(
                n,
                Affine3::IDENTITY,
                &geometries,
                &mut out,
                0,
                &mut visits,
                cancel,
            )?;
        }
    } else {
        for (_, m) in geometries {
            out.triangles.extend(m.triangles);
            out.segments.extend(m.segments);
            for w in m.warnings {
                out.warn(w);
            }
        }
    }
    if options.unit.is_none() {
        let meters = doc
            .descendants()
            .find(|n| n.has_tag_name("unit"))
            .and_then(|n| n.attribute("meter"))
            .map(number)
            .transpose()?
            .unwrap_or(1.);
        if meters <= 0. {
            return Err("Collada unit must be positive".into());
        }
        out.scale(meters / options.meters_per_model_unit);
    }
    out.warn("Collada retains static meshes, linework and scene transforms; materials, animations and controllers are omitted. Source axes are retained.");
    Ok(out)
}

fn indexed(m: &Mesh) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let mut points = vec![];
    let mut map = BTreeMap::new();
    let mut faces = vec![];
    for (p, _) in &m.triangles {
        faces.push(p.map(|p| {
            let key = [p.x, p.y, p.z].map(|v| if v == 0. { 0 } else { v.to_bits() });
            *map.entry(key).or_insert_with(|| {
                let i = points.len();
                points.push(p);
                i
            })
        }));
    }
    (points, faces)
}
fn validate_closed(
    points: &[Vec3],
    faces: &[[usize; 3]],
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut edges = BTreeMap::<[usize; 2], (usize, i32)>::new();
    for f in faces {
        check(cancel)?;
        if f[0] == f[1]
            || f[1] == f[2]
            || f[2] == f[0]
            || normal(f.map(|i| points[i])) == Vec3::ZERO
        {
            return Err("3MF cannot export degenerate triangles".into());
        }
        for [a, b] in [[f[0], f[1]], [f[1], f[2]], [f[2], f[0]]] {
            let key = [a.min(b), a.max(b)];
            let e = edges.entry(key).or_default();
            e.0 += 1;
            e.1 += if a < b { 1 } else { -1 };
        }
    }
    if edges
        .values()
        .any(|(n, orientation)| *n != 2 || *orientation != 0)
    {
        return Err("3MF requires closed manifold meshes with consistent winding; repair open/non-manifold edges or export OBJ/STL".into());
    }
    Ok(())
}
pub(super) fn write(
    m: &Mesh,
    format: Format,
    unit: Unit,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let (points, faces) = indexed(m);
    let mut s = String::new();
    match format {
        Format::ThreeMf => {
            validate_closed(&points, &faces, cancel)?;
            if unit.name == "unitless" {
                return Err("3MF requires a physical output unit".into());
            }
            write!(s,"<?xml version=\"1.0\" encoding=\"UTF-8\"?><model xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" unit=\"{}\" xml:lang=\"en-US\"><resources><object id=\"1\" type=\"model\"><mesh><vertices>",unit.name).unwrap();
            for p in &points {
                check(cancel)?;
                write!(s, "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", p.x, p.y, p.z).unwrap();
            }
            s.push_str("</vertices><triangles>");
            for f in faces {
                write!(
                    s,
                    "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>",
                    f[0], f[1], f[2]
                )
                .unwrap();
            }
            s.push_str("</triangles></mesh></object></resources><build><item objectid=\"1\"/></build></model>");
            let mut zip = zip::ZipWriter::new(Cursor::new(vec![]));
            for (name, data) in [
                (
                    "[Content_Types].xml",
                    r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#,
                ),
                (
                    "_rels/.rels",
                    r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#,
                ),
                ("3D/3dmodel.model", s.as_str()),
            ] {
                check(cancel)?;
                zip.start_file(
                    name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(|e| e.to_string())?;
                zip.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
            }
            return zip
                .finish()
                .map(|c| c.into_inner())
                .map_err(|e| e.to_string());
        }
        Format::Amf => {
            let name = if unit.name == "foot" {
                "feet"
            } else {
                unit.name
            };
            if name == "centimeter" || name == "unitless" {
                return Err("AMF does not support this unit; use millimeter or meter".into());
            }
            write!(s,"<?xml version=\"1.0\"?><amf unit=\"{name}\" version=\"1.1\"><object id=\"0\"><mesh><vertices>").unwrap();
            for p in &points {
                check(cancel)?;
                write!(
                    s,
                    "<vertex><coordinates><x>{}</x><y>{}</y><z>{}</z></coordinates></vertex>",
                    p.x, p.y, p.z
                )
                .unwrap();
            }
            s.push_str("</vertices><volume>");
            for f in faces {
                write!(
                    s,
                    "<triangle><v1>{}</v1><v2>{}</v2><v3>{}</v3></triangle>",
                    f[0], f[1], f[2]
                )
                .unwrap();
            }
            s.push_str("</volume></mesh></object></amf>");
        }
        Format::Dae => {
            let mut positions = points.clone();
            for (p, _) in &m.segments {
                positions.extend(p);
            }
            let values = positions
                .iter()
                .flat_map(|p| [p.x, p.y, p.z])
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let faces = faces
                .iter()
                .flatten()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let lines = (points.len()..positions.len())
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            write!(s,"<?xml version=\"1.0\"?><COLLADA xmlns=\"http://www.collada.org/2005/11/COLLADASchema\" version=\"1.4.1\"><asset><created>2026-01-01T00:00:00Z</created><modified>2026-01-01T00:00:00Z</modified><unit name=\"{}\" meter=\"{}\"/><up_axis>Y_UP</up_axis></asset><library_geometries><geometry id=\"mesh\"><mesh><source id=\"positions\"><float_array id=\"positions-array\" count=\"{}\">{values}</float_array><technique_common><accessor source=\"#positions-array\" count=\"{}\" stride=\"3\"><param name=\"X\" type=\"float\"/><param name=\"Y\" type=\"float\"/><param name=\"Z\" type=\"float\"/></accessor></technique_common></source><vertices id=\"vertices\"><input semantic=\"POSITION\" source=\"#positions\"/></vertices>",xml_escape(unit.name),unit.meters,positions.len()*3,positions.len()).unwrap();
            if !m.triangles.is_empty() {
                write!(s,"<triangles count=\"{}\"><input semantic=\"VERTEX\" source=\"#vertices\" offset=\"0\"/><p>{faces}</p></triangles>",m.triangles.len()).unwrap();
            }
            if !m.segments.is_empty() {
                write!(s,"<lines count=\"{}\"><input semantic=\"VERTEX\" source=\"#vertices\" offset=\"0\"/><p>{lines}</p></lines>",m.segments.len()).unwrap();
            }
            s.push_str("</mesh></geometry></library_geometries><library_visual_scenes><visual_scene id=\"scene\"><node id=\"model\"><instance_geometry url=\"#mesh\"/></node></visual_scene></library_visual_scenes><scene><instance_visual_scene url=\"#scene\"/></scene></COLLADA>");
        }
        _ => unreachable!(),
    }
    Ok(s.into_bytes())
}
