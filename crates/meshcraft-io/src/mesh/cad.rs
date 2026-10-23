use super::*;
use std::fmt::Write;
#[path = "ifc_properties.rs"]
mod properties;

pub(super) fn read_dxf(s: &str, options: &Options, cancel: &AtomicBool) -> Result<Mesh, String> {
    if s.starts_with("AutoCAD Binary DXF") {
        return Err("binary DXF is unsupported; save as ASCII DXF".into());
    }
    let mut lines = s.lines();
    let mut pairs = vec![];
    while let Some(code) = lines.next() {
        check(cancel)?;
        let value = lines.next().ok_or("truncated DXF group pair")?;
        pairs.push((
            code.trim()
                .parse::<u32>()
                .map_err(|_| "invalid DXF group code")?,
            value.trim(),
        ));
    }
    let mut out = Mesh::default();
    let mut i = 0;
    let mut section = "";
    let mut poly = vec![];
    let mut closed = false;
    while i < pairs.len() {
        check(cancel)?;
        if pairs[i].0 != 0 {
            i += 1;
            continue;
        }
        let kind = pairs[i].1;
        i += 1;
        let start = i;
        while i < pairs.len() && pairs[i].0 != 0 {
            i += 1;
        }
        let p = &pairs[start..i];
        if kind == "SECTION" {
            section = p
                .iter()
                .find(|(c, _)| *c == 2)
                .map(|(_, v)| *v)
                .unwrap_or("");
            continue;
        }
        if kind == "ENDSEC" {
            section = "";
            continue;
        }
        if section != "ENTITIES" {
            continue;
        }
        let get = |code: u32| {
            p.iter()
                .find(|(c, _)| *c == code)
                .map(|(_, v)| number(v))
                .transpose()
        };
        let point = |base: u32| -> Result<Vec3, String> {
            Ok(Vec3::new(
                get(base)?.ok_or("missing DXF x")?,
                get(base + 10)?.ok_or("missing DXF y")?,
                get(base + 20)?.unwrap_or(0.),
            ))
        };
        let c = if let Some(rgb) = get(420)? {
            let rgb = rgb as u32;
            Rgba8::new((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
        } else {
            Rgba8::WHITE
        };
        match kind {
            "3DFACE" | "SOLID" | "TRACE" => {
                let a = point(10)?;
                let b = point(11)?;
                let c0 = point(12)?;
                let d = if p.iter().any(|(code, _)| *code == 13) {
                    point(13)?
                } else {
                    c0
                };
                polygon(
                    &mut out,
                    &if kind == "3DFACE" {
                        if d == c0 {
                            vec![a, b, c0]
                        } else {
                            vec![a, b, c0, d]
                        }
                    } else if d == c0 {
                        vec![a, b, c0]
                    } else {
                        vec![a, b, d, c0]
                    },
                    c,
                )?;
            }
            "LINE" => out.segments.push(([point(10)?, point(11)?], c)),
            "LWPOLYLINE" => {
                let mut points = vec![];
                let elevation = get(38)?.unwrap_or(0.);
                for (j, (code, v)) in p.iter().enumerate() {
                    if *code == 10 {
                        let y = p[j + 1..]
                            .iter()
                            .take_while(|(c, _)| *c != 10)
                            .find(|(c, _)| *c == 20)
                            .ok_or("missing polyline y")?
                            .1;
                        points.push(Vec3::new(number(v)?, number(y)?, elevation));
                    }
                }
                for w in points.windows(2) {
                    out.segments.push(([w[0], w[1]], c));
                }
                if get(70)?.unwrap_or(0.) as u32 & 1 != 0 && points.len() > 2 {
                    out.segments.push(([*points.last().unwrap(), points[0]], c));
                }
                if p.iter()
                    .any(|(code, v)| *code == 42 && number(v).unwrap_or(0.) != 0.)
                {
                    out.warn("DXF bulge arcs are imported as straight polyline chords.");
                }
            }
            "POLYLINE" => {
                if get(70)?.unwrap_or(0.) as u32 & (16 | 64) != 0 {
                    return Err("DXF polygon/polyface mesh POLYLINE is unsupported; convert it to 3DFACE entities".into());
                }
                poly.clear();
                closed = get(70)?.unwrap_or(0.) as u32 & 1 != 0;
            }
            "VERTEX" => poly.push(point(10)?),
            "SEQEND" => {
                for w in poly.windows(2) {
                    out.segments.push(([w[0], w[1]], c));
                }
                if closed && poly.len() > 2 {
                    out.segments.push(([*poly.last().unwrap(), poly[0]], c));
                }
                poly.clear();
            }
            "CIRCLE" | "ARC" => {
                let center = point(10)?;
                let radius = get(40)?.ok_or("missing arc radius")?;
                let start = get(50)?.unwrap_or(0.).to_radians();
                let mut end = get(51)?.unwrap_or(360.).to_radians();
                while end <= start {
                    end += std::f64::consts::TAU;
                }
                let n = (((end - start).abs() / std::f64::consts::TAU * 64.).ceil() as usize)
                    .clamp(2, 4096);
                let mut previous = None;
                for j in 0..=n {
                    let a = start + (end - start) * j as f64 / n as f64;
                    let p = center + Vec3::new(a.cos() * radius, a.sin() * radius, 0.);
                    if let Some(p0) = previous {
                        out.segments.push(([p0, p], c));
                    }
                    previous = Some(p);
                }
                out.warn("DXF circles and arcs are tessellated to segments.");
            }
            "EOF" => {}
            _ => out.warn(format!("DXF entity {kind} is unsupported and was omitted.")),
        }
        if get(210)?.unwrap_or(0.) != 0.
            || get(220)?.unwrap_or(0.) != 0.
            || get(230)?.unwrap_or(1.) != 1.
        {
            return Err("DXF object-coordinate extrusion transforms are unsupported; convert the entity to world coordinates".into());
        }
        out.check_size()?;
    }
    if !pairs.iter().any(|(c, v)| *c == 0 && *v == "EOF") {
        return Err("DXF is missing EOF".into());
    }
    if options.unit.is_none()
        && let Some(i) = pairs.iter().position(|(c, v)| *c == 9 && *v == "$INSUNITS")
        && let Some((_, value)) = pairs.get(i + 1)
    {
        let meters = match *value {
            "0" => None,
            "1" => Some(0.0254),
            "2" => Some(0.3048),
            "4" => Some(0.001),
            "5" => Some(0.01),
            "6" => Some(1.),
            "13" => Some(1e-6),
            _ => return Err("unsupported DXF INSUNITS; supply an explicit import unit".into()),
        };
        if let Some(meters) = meters {
            out.scale(meters / options.meters_per_model_unit);
        }
    }
    Ok(out)
}

pub(super) fn write_dxf(m: &Mesh, unit: Unit, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut s = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1027\n9\n$INSUNITS\n70\n{}\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n",
        unit.dxf
    );
    let point = |s: &mut String, i: usize, p: Vec3| {
        writeln!(
            s,
            "{}\n{}\n{}\n{}\n{}\n{}",
            10 + i,
            p.x,
            20 + i,
            p.y,
            30 + i,
            p.z
        )
        .unwrap();
    };
    for (p, c) in &m.triangles {
        check(cancel)?;
        writeln!(
            s,
            "0\n3DFACE\n8\nMeshcraft\n420\n{}",
            ((c.red as u32) << 16) | ((c.green as u32) << 8) | c.blue as u32
        )
        .unwrap();
        for (i, p) in [p[0], p[1], p[2], p[2]].into_iter().enumerate() {
            point(&mut s, i, p);
        }
    }
    for (p, c) in &m.segments {
        check(cancel)?;
        writeln!(
            s,
            "0\nLINE\n8\nMeshcraft\n420\n{}",
            ((c.red as u32) << 16) | ((c.green as u32) << 8) | c.blue as u32
        )
        .unwrap();
        for (i, p) in p.iter().enumerate() {
            point(&mut s, i, *p);
        }
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    Ok(s.into_bytes())
}

pub(super) fn read_fbx(b: &[u8], cancel: &AtomicBool) -> Result<Mesh, String> {
    if b.starts_with(b"Kaydara FBX Binary") {
        return Err("binary FBX is unsupported; export ASCII FBX, OBJ or glTF instead".into());
    }
    let s = utf8(b)?;
    let mut out = Mesh::default();
    let mut rest = s;
    while let Some(at) = rest.find("Vertices:") {
        check(cancel)?;
        rest = &rest[at + 9..];
        let end = rest.find('}').ok_or("incomplete FBX vertices")?;
        let array = rest[..end]
            .split_once("a:")
            .ok_or("missing FBX vertex array")?
            .1;
        let values = numbers(array)?;
        if values.len() % 3 != 0 {
            return Err("invalid FBX vertex array".into());
        }
        let vertices = values
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| Vec3::new(p[0], p[1], p[2]))
            .collect::<Vec<_>>();
        rest = &rest[end + 1..];
        let at = rest
            .find("PolygonVertexIndex:")
            .ok_or("missing FBX polygons")?;
        rest = &rest[at + 19..];
        let end = rest.find('}').ok_or("incomplete FBX polygons")?;
        let array = rest[..end]
            .split_once("a:")
            .ok_or("missing FBX polygon array")?
            .1;
        let mut points = vec![];
        for v in array
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
        {
            check(cancel)?;
            let i = v.parse::<i64>().map_err(|_| "invalid FBX polygon index")?;
            let index = if i < 0 { -(i + 1) } else { i };
            points.push(
                *vertices
                    .get(index as usize)
                    .ok_or("FBX index out of range")?,
            );
            if i < 0 {
                polygon(&mut out, &points, Rgba8::WHITE)?;
                points.clear();
            }
            out.check_size()?;
        }
        if !points.is_empty() {
            return Err("unterminated FBX polygon".into());
        }
        rest = &rest[end + 1..];
    }
    if s.contains("Lcl Translation") || s.contains("Lcl Rotation") || s.contains("Lcl Scaling") {
        return Err(
            "ASCII FBX node transforms are unsupported; bake transforms or use glTF".into(),
        );
    }
    out.warn("FBX imports static ASCII polygon arrays only; materials, hierarchy, animation and skinning are omitted.");
    Ok(out)
}
pub(super) fn write_fbx(m: &Mesh, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut vertices = String::new();
    let mut indices = String::new();
    for (i, (p, _)) in m.triangles.iter().enumerate() {
        check(cancel)?;
        for p in p {
            write!(vertices, "{},{},{},", p.x, p.y, p.z).unwrap();
        }
        write!(indices, "{},{},{},", i * 3, i * 3 + 1, -(i as i64 * 3 + 3)).unwrap();
    }
    vertices.pop();
    indices.pop();
    Ok(format!("; FBX 7.4.0 generated by Meshcraft\nFBXHeaderExtension: {{\n FBXHeaderVersion: 1003\n FBXVersion: 7400\n}}\nDefinitions: {{\n Version: 100\n Count: 2\n ObjectType: \"Geometry\" {{ Count: 1 }}\n ObjectType: \"Model\" {{ Count: 1 }}\n}}\nObjects: {{\n Geometry: 100000, \"Geometry::Meshcraft\", \"Mesh\" {{\n  Vertices: *{} {{ a: {} }}\n  PolygonVertexIndex: *{} {{ a: {} }}\n  GeometryVersion: 124\n }}\n Model: 100001, \"Model::Meshcraft\", \"Mesh\" {{ Version: 232 }}\n}}\nConnections: {{\n C: \"OO\",100000,100001\n C: \"OO\",100001,0\n}}\n",m.triangles.len()*9,vertices,m.triangles.len()*3,indices).into_bytes())
}

// Small STEP value parser: references are distinct from numeric coordinates, and quoted
// strings/comments cannot inject entity terminators or references.
#[derive(Clone, Debug)]
enum Step {
    Ref(u64),
    Num(f64),
    List(Vec<Step>),
    Other(String),
}
impl Step {
    fn list(&self) -> Result<&[Step], String> {
        if let Self::List(v) = self {
            Ok(v)
        } else {
            Err("expected IFC aggregate".into())
        }
    }
    fn reference(&self) -> Result<u64, String> {
        if let Self::Ref(v) = self {
            Ok(*v)
        } else {
            Err("expected IFC reference".into())
        }
    }
    fn num(&self) -> Result<f64, String> {
        if let Self::Num(v) = self {
            Ok(*v)
        } else {
            Err("expected IFC number".into())
        }
    }
}
fn step_value(s: &str, depth: usize) -> Result<Step, String> {
    if depth > 64 {
        return Err("IFC aggregate nesting exceeds 64".into());
    }
    let s = s.trim();
    if let Some(s) = s.strip_prefix('#') {
        return Ok(Step::Ref(s.parse().map_err(|_| "invalid IFC reference")?));
    }
    if s.starts_with('(') && s.ends_with(')') {
        return Ok(Step::List(
            step_split(&s[1..s.len() - 1], ',')?
                .into_iter()
                .map(|s| step_value(s, depth + 1))
                .collect::<Result<_, _>>()?,
        ));
    }
    if s.starts_with([
        '-', '+', '.', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    ]) && !s.starts_with('.')
    {
        return number(s).map(Step::Num);
    }
    Ok(Step::Other(s.to_owned()))
}
fn step_split(s: &str, separator: char) -> Result<Vec<&str>, String> {
    let mut result = vec![];
    let mut start = 0;
    let mut depth = 0i32;
    let mut quoted = false;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\'' {
            if quoted && chars.peek().is_some_and(|(_, c)| *c == '\'') {
                chars.next();
                continue;
            }
            quoted = !quoted;
        }
        if quoted {
            continue;
        }
        match c {
            '(' => {
                depth += 1;
                if depth > 64 {
                    return Err("IFC nesting exceeds 64".into());
                }
            }
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err("unbalanced IFC aggregate".into());
                }
            }
            c if c == separator && depth == 0 => {
                result.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if quoted || depth != 0 {
        return Err("incomplete IFC string or aggregate".into());
    }
    if !s[start..].trim().is_empty() {
        result.push(&s[start..]);
    }
    Ok(result)
}
type Entities = BTreeMap<u64, (String, Vec<Step>)>;
fn entities(s: &str) -> Result<Entities, String> {
    let mut result = BTreeMap::new();
    for record in step_split(s, ';')? {
        let r = record.trim();
        if !r.starts_with('#') {
            continue;
        }
        let (id, r) = r[1..].split_once('=').ok_or("invalid IFC entity")?;
        let (name, args) = r.trim().split_once('(').ok_or("missing IFC arguments")?;
        let args = args.strip_suffix(')').ok_or("incomplete IFC entity")?;
        let id = id.trim().parse().map_err(|_| "invalid IFC ID")?;
        if result
            .insert(
                id,
                (
                    name.trim().to_ascii_uppercase(),
                    step_split(args, ',')?
                        .into_iter()
                        .map(|s| step_value(s, 0))
                        .collect::<Result<_, _>>()?,
                ),
            )
            .is_some()
        {
            return Err("duplicate IFC entity ID".into());
        }
    }
    Ok(result)
}
fn entity(e: &Entities, id: u64) -> Result<&(String, Vec<Step>), String> {
    e.get(&id)
        .ok_or_else(|| format!("IFC missing entity #{id}"))
}
fn arg(a: &[Step], n: usize) -> Result<&Step, String> {
    a.get(n).ok_or("missing IFC argument".into())
}
fn point(e: &Entities, id: u64) -> Result<Vec3, String> {
    let (kind, a) = entity(e, id)?;
    if !matches!(kind.as_str(), "IFCCARTESIANPOINT" | "IFCDIRECTION") {
        return Err("unsupported IFC point/direction".into());
    }
    let v = arg(a, 0)?.list()?;
    Ok(Vec3::new(
        arg(v, 0)?.num()?,
        arg(v, 1)?.num()?,
        v.get(2).map(Step::num).transpose()?.unwrap_or(0.),
    ))
}
fn placement(e: &Entities, id: u64, depth: usize) -> Result<Affine3, String> {
    if depth > 64 {
        return Err("cyclic or too deep IFC placement".into());
    }
    let (kind, a) = entity(e, id)?;
    if kind == "IFCLOCALPLACEMENT" {
        let parent = if let Some(Step::Ref(id)) = a.first() {
            placement(e, *id, depth + 1)?
        } else {
            Affine3::IDENTITY
        };
        return Ok(parent.compose(placement(e, arg(a, 1)?.reference()?, depth + 1)?));
    }
    if kind == "IFCAXIS2PLACEMENT3D" || kind == "IFCAXIS2PLACEMENT2D" {
        let origin = point(e, arg(a, 0)?.reference()?)?;
        let z = if let Some(Step::Ref(id)) = a.get(1) {
            point(e, *id)?
        } else {
            Vec3::Z
        };
        let x = if let Some(Step::Ref(id)) = a.get(2) {
            point(e, *id)?
        } else {
            Vec3::X
        };
        let z = z
            .try_normalized(1e-15)
            .ok_or("invalid IFC placement axis")?;
        let y = z
            .cross(x)
            .try_normalized(1e-15)
            .ok_or("invalid IFC placement reference")?;
        let x = y.cross(z);
        return Ok(Affine3 {
            linear: [[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]],
            translation: origin,
        });
    }
    Err(format!("unsupported IFC placement {kind}"))
}
fn ifc_geometry(
    e: &Entities,
    id: u64,
    out: &mut Mesh,
    t: Affine3,
    depth: usize,
    cancel: &AtomicBool,
) -> Result<(), String> {
    check(cancel)?;
    if depth > 64 {
        return Err("cyclic or too deep IFC representation".into());
    }
    let (kind, a) = entity(e, id)?;
    match kind.as_str() {
        "IFCPRODUCTDEFINITIONSHAPE" => {
            for v in arg(a, 2)?.list()? {
                ifc_geometry(e, v.reference()?, out, t, depth + 1, cancel)?;
            }
        }
        "IFCSHAPEREPRESENTATION" => {
            for v in arg(a, 3)?.list()? {
                ifc_geometry(e, v.reference()?, out, t, depth + 1, cancel)?;
            }
        }
        "IFCTRIANGULATEDFACESET" | "IFCPOLYGONALFACESET" => {
            let (k, points) = entity(e, arg(a, 0)?.reference()?)?;
            if k != "IFCCARTESIANPOINTLIST3D" {
                return Err("invalid IFC point list".into());
            }
            let points = arg(points, 0)?
                .list()?
                .iter()
                .map(|v| {
                    let v = v.list()?;
                    Ok(t.transform_point(Vec3::new(
                        arg(v, 0)?.num()?,
                        arg(v, 1)?.num()?,
                        arg(v, 2)?.num()?,
                    )))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let polygons = arg(
                a,
                if kind == "IFCTRIANGULATEDFACESET" {
                    3
                } else {
                    2
                },
            )?
            .list()?;
            for face in polygons {
                let indices = if kind == "IFCPOLYGONALFACESET" {
                    let (k, f) = entity(e, face.reference()?)?;
                    if k != "IFCINDEXEDPOLYGONALFACE" {
                        return Err("IFC polygonal faces with voids are unsupported".into());
                    }
                    arg(f, 0)?.list()?
                } else {
                    face.list()?
                };
                let p = indices
                    .iter()
                    .map(|i| {
                        let n = i.num()?;
                        if n < 1. || n.fract() != 0. {
                            return Err("invalid IFC face index".into());
                        }
                        points
                            .get(n as usize - 1)
                            .copied()
                            .ok_or("IFC face index out of bounds".into())
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                polygon(out, &p, Rgba8::WHITE)?;
                out.check_size()?;
            }
        }
        "IFCFACETEDBREP" => ifc_geometry(e, arg(a, 0)?.reference()?, out, t, depth + 1, cancel)?,
        "IFCCLOSEDSHELL"
        | "IFCOPENSHELL"
        | "IFCSHELLBASEDSURFACEMODEL"
        | "IFCGEOMETRICCURVESET"
        | "IFCGEOMETRICSET" => {
            for v in arg(a, 0)?.list()? {
                ifc_geometry(e, v.reference()?, out, t, depth + 1, cancel)?;
            }
        }
        "IFCFACE" => {
            let bounds = arg(a, 0)?.list()?;
            if bounds.len() != 1 {
                return Err("IFC BREP faces with voids are unsupported".into());
            }
            let (_, bound) = entity(e, arg(bounds, 0)?.reference()?)?;
            let (_, poly) = entity(e, arg(bound, 0)?.reference()?)?;
            let mut p = arg(poly, 0)?
                .list()?
                .iter()
                .map(|v| point(e, v.reference()?).map(|p| t.transform_point(p)))
                .collect::<Result<Vec<_>, _>>()?;
            if matches!(bound.get(1),Some(Step::Other(s)) if s==".F.") {
                p.reverse();
            }
            polygon(out, &p, Rgba8::WHITE)?;
        }
        "IFCPOLYLINE" => {
            let points = arg(a, 0)?
                .list()?
                .iter()
                .map(|v| point(e, v.reference()?).map(|p| t.transform_point(p)))
                .collect::<Result<Vec<_>, _>>()?;
            out.segments
                .extend(points.windows(2).map(|p| ([p[0], p[1]], Rgba8::WHITE)));
        }
        _ => out.warn(format!(
            "IFC representation {kind} is unsupported and was omitted."
        )),
    }
    Ok(())
}
pub(super) fn read_ifc(s: &str, options: &Options, cancel: &AtomicBool) -> Result<Mesh, String> {
    if !s.trim_start().starts_with("ISO-10303-21;") {
        return Err("expected IFC STEP text".into());
    }
    if !s.trim_end().ends_with("END-ISO-10303-21;") {
        return Err("IFC STEP file is incomplete".into());
    }
    let mut clean = String::with_capacity(s.len());
    let mut quoted = false;
    let mut i = 0;
    while i < s.len() {
        check(cancel)?;
        let c = s.as_bytes()[i];
        if c == b'\'' {
            clean.push('\'');
            i += 1;
            if quoted && s.as_bytes().get(i) == Some(&b'\'') {
                clean.push('\'');
                i += 1;
                continue;
            }
            quoted = !quoted;
            continue;
        }
        if !quoted && s[i..].starts_with("/*") {
            let end = s[i + 2..].find("*/").ok_or("unterminated IFC comment")?;
            i += end + 4;
            clean.push(' ');
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        clean.push(ch);
        i += ch.len_utf8();
    }
    let e = entities(&clean)?;
    let mut out = Mesh::default();
    let mut products = 0;
    for (&product, (kind, a)) in &e {
        check(cancel)?;
        if let Some(Step::Ref(rep)) = a.get(6)
            && e.get(rep)
                .is_some_and(|(k, _)| k == "IFCPRODUCTDEFINITIONSHAPE")
        {
            let t = if let Some(Step::Ref(id)) = a.get(5) {
                placement(&e, *id, 0)?
            } else {
                Affine3::IDENTITY
            };
            let triangles = out.triangles.len();
            let segments = out.segments.len();
            ifc_geometry(&e, *rep, &mut out, t, 0, cancel)?;
            out.parts.push(MeshPart {
                name: a
                    .get(2)
                    .map(|v| {
                        if let Step::Other(s) = v {
                            s.trim_matches('\'').replace("''", "'")
                        } else {
                            "Object".into()
                        }
                    })
                    .unwrap_or("Object".into()),
                triangles: triangles..out.triangles.len(),
                segments: segments..out.segments.len(),
                ifc: properties::read(&e, product),
            });
            products += 1;
            if kind != "IFCBUILDINGELEMENTPROXY" {
                out.warn("IFC semantic products are imported as geometry only.");
            }
        }
    }
    if products == 0 {
        for (&id, (k, _)) in &e {
            if matches!(
                k.as_str(),
                "IFCTRIANGULATEDFACESET" | "IFCPOLYGONALFACESET" | "IFCFACETEDBREP" | "IFCPOLYLINE"
            ) {
                ifc_geometry(&e, id, &mut out, Affine3::IDENTITY, 0, cancel)?;
            }
        }
    }
    if options.unit.is_none() {
        if e.values().any(|(k, _)| k == "IFCCONVERSIONBASEDUNIT") {
            return Err("IFC conversion-based units require an explicit import unit".into());
        }
        let mut meters = 1.;
        let units = e
            .values()
            .filter(|(k, _)| k == "IFCPROJECT")
            .filter_map(|(_, a)| a.get(8))
            .filter_map(|v| v.reference().ok())
            .filter_map(|id| e.get(&id))
            .filter(|(k, _)| k == "IFCUNITASSIGNMENT")
            .filter_map(|(_, a)| a.first().and_then(|v| v.list().ok()))
            .flatten()
            .filter_map(|v| v.reference().ok())
            .filter_map(|id| e.get(&id))
            .collect::<Vec<_>>();
        let units = if units.is_empty() {
            e.values().collect::<Vec<_>>()
        } else {
            units
        };
        for (kind, a) in units {
            if kind == "IFCSIUNIT" && matches!(a.get(1),Some(Step::Other(s)) if s==".LENGTHUNIT.") {
                meters = match a.get(2) {
                    Some(Step::Other(s)) if s == ".MILLI." => 0.001,
                    Some(Step::Other(s)) if s == ".CENTI." => 0.01,
                    Some(Step::Other(s)) if s == ".DECI." => 0.1,
                    Some(Step::Other(s)) if s == "$" => 1.,
                    _ => return Err("unsupported IFC SI length prefix".into()),
                };
            }
        }
        out.scale(meters / options.meters_per_model_unit);
    }
    out.warn("IFC supports tessellated faces, faceted BREPs, polylines, local placements and property sets; surface styles and other BIM relationships are omitted.");
    Ok(out)
}

pub(super) fn write_ifc(m: &Mesh, unit: Unit, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    // IFC output coordinates and SI units agree even for suffix-selected non-SI units.
    let mut m = m.clone();
    m.scale(unit.meters);
    let mut s = String::from(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('Meshcraft geometry proxy'),'2;1');\nFILE_NAME('meshcraft.ifc','',(''),(''),'Meshcraft','Meshcraft','');\nFILE_SCHEMA(('IFC4X3_ADD2'));\nENDSEC;\nDATA;\n",
    );
    s.push_str("#1=IFCCARTESIANPOINT((0.,0.,0.));\n#2=IFCAXIS2PLACEMENT3D(#1,$,$);\n#3=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-8,#2,$);\n#4=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n#5=IFCUNITASSIGNMENT((#4));\n#6=IFCPROJECT('0MC0000000000000000001',$,'Meshcraft',$,$,$,$,(#3),#5);\n#7=IFCLOCALPLACEMENT($,#2);\n#8=IFCSITE('0MC0000000000000000002',$,'Site',$,$,#7,$,$,.ELEMENT.,$,$,$,$,$);\n#9=IFCBUILDING('0MC0000000000000000003',$,'Building',$,$,#7,$,$,.ELEMENT.,$,$,$);\n#10=IFCBUILDINGSTOREY('0MC0000000000000000004',$,'Storey',$,$,#7,$,$,.ELEMENT.,0.);\n#11=IFCRELAGGREGATES('0MC0000000000000000005',$,$,$,#6,(#8));\n#12=IFCRELAGGREGATES('0MC0000000000000000006',$,$,$,#8,(#9));\n#13=IFCRELAGGREGATES('0MC0000000000000000007',$,$,$,#9,(#10));\n");
    let parts = if m.parts.is_empty() {
        vec![MeshPart {
            name: "Meshcraft geometry".into(),
            triangles: 0..m.triangles.len(),
            segments: 0..m.segments.len(),
            ifc: Default::default(),
        }]
    } else {
        m.parts.clone()
    };
    let mut id = 20u64;
    let mut products = vec![];
    for (i, part) in parts.iter().enumerate() {
        check(cancel)?;
        let triangles = &m.triangles[part.triangles.clone()];
        let segments = &m.segments[part.segments.clone()];
        let mut representations = vec![];
        if !triangles.is_empty() {
            let points = id;
            id += 1;
            write!(s, "#{points}=IFCCARTESIANPOINTLIST3D((").unwrap();
            for (i, (p, _)) in triangles.iter().enumerate() {
                check(cancel)?;
                for (j, p) in p.iter().enumerate() {
                    if i + j > 0 {
                        s.push(',');
                    }
                    write!(s, "({},{},{})", ifc_real(p.x), ifc_real(p.y), ifc_real(p.z)).unwrap();
                }
            }
            s.push_str("));\n");
            let faces = id;
            id += 1;
            write!(s, "#{faces}=IFCTRIANGULATEDFACESET(#{points},$,.F.,(").unwrap();
            for i in 0..triangles.len() {
                if i > 0 {
                    s.push(',');
                }
                write!(s, "({},{},{})", i * 3 + 1, i * 3 + 2, i * 3 + 3).unwrap();
            }
            s.push_str("),$);\n");
            writeln!(
                s,
                "#{id}=IFCSHAPEREPRESENTATION(#3,'Body','Tessellation',(#{faces}));"
            )
            .unwrap();
            representations.push(id);
            id += 1;
        }
        if !segments.is_empty() {
            let mut curves = vec![];
            for (p, _) in segments {
                check(cancel)?;
                for (j, p) in p.iter().enumerate() {
                    writeln!(
                        s,
                        "#{}=IFCCARTESIANPOINT(({},{},{}));",
                        id + j as u64,
                        ifc_real(p.x),
                        ifc_real(p.y),
                        ifc_real(p.z)
                    )
                    .unwrap();
                }
                writeln!(s, "#{}=IFCPOLYLINE((#{},#{}));", id + 2, id, id + 1).unwrap();
                curves.push(id + 2);
                id += 3;
            }
            let refs = curves
                .iter()
                .map(|id| format!("#{id}"))
                .collect::<Vec<_>>()
                .join(",");
            writeln!(
                s,
                "#{id}=IFCSHAPEREPRESENTATION(#3,'Axis','Curve3D',({refs}));"
            )
            .unwrap();
            representations.push(id);
            id += 1;
        }
        if representations.is_empty() {
            continue;
        }
        let reps = representations
            .iter()
            .map(|id| format!("#{id}"))
            .collect::<Vec<_>>()
            .join(",");
        let name = ifc_string(&part.name);
        writeln!(s,"#{id}=IFCPRODUCTDEFINITIONSHAPE($,$,({reps}));\n#{}=IFCBUILDINGELEMENTPROXY('{:022x}',$,'{name}',$,$,#7,#{id},$,.NOTDEFINED.);",id+1,i+8).unwrap();
        let product = id + 1;
        products.push(product);
        id += 2;
        properties::write(&mut s, &mut id, product, &part.ifc)?;
    }
    let refs = products
        .iter()
        .map(|id| format!("#{id}"))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(s,"#{id}=IFCRELCONTAINEDINSPATIALSTRUCTURE('{:022x}',$,$,$,({refs}),#10);\nENDSEC;\nEND-ISO-10303-21;",parts.len()+8).unwrap();
    Ok(s.into_bytes())
}
fn ifc_real(value: f64) -> String {
    let mut s = value.to_string();
    if !s.contains(['.', 'E', 'e']) {
        s.push('.');
    }
    s
}
fn ifc_string(value: &str) -> String {
    let mut result = String::new();
    for ch in value.chars() {
        match ch {
            '\'' => result.push_str("''"),
            '\\' => result.push_str("\\\\"),
            c if c.is_ascii() && !c.is_control() => result.push(c),
            c => {
                result.push_str("\\X2\\");
                for n in c.encode_utf16(&mut [0; 2]) {
                    write!(result, "{n:04X}").unwrap();
                }
                result.push_str("\\X0\\");
            }
        }
    }
    result
}
