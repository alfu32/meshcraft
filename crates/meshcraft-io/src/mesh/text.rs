use super::*;
use std::fmt::Write;

pub(super) fn read_obj(
    s: &str,
    resource: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    cancel: &AtomicBool,
) -> Result<Mesh, String> {
    let mut out = Mesh::default();
    let mut vertices = vec![];
    let mut vertex_colors = vec![];
    let mut materials = BTreeMap::new();
    let mut current = None;
    for (line_no, line) in s.lines().enumerate() {
        check(cancel)?;
        let line = line.split('#').next().unwrap_or("").trim();
        let mut fields = line.split_whitespace();
        let Some(kind) = fields.next() else { continue };
        let result = (|| -> Result<(), String> {
            match kind {
                "v" => {
                    let v = fields.map(number).collect::<Result<Vec<_>, _>>()?;
                    if v.len() < 3 {
                        return Err("vertex needs x y z".into());
                    }
                    vertices.push(Vec3::new(v[0], v[1], v[2]));
                    vertex_colors.push(if v.len() >= 6 {
                        color([v[3], v[4], v[5], 1.])
                    } else {
                        Rgba8::WHITE
                    });
                    if vertices.len() > MAX_PRIMITIVES * 3 {
                        return Err("too many OBJ vertices".into());
                    }
                }
                "f" | "l" => {
                    let ids = fields
                        .map(|f| {
                            let id = f
                                .split('/')
                                .next()
                                .unwrap_or("")
                                .parse::<i64>()
                                .map_err(|_| "invalid OBJ vertex index")?;
                            let id = if id < 0 {
                                vertices.len() as i64 + id
                            } else {
                                id - 1
                            };
                            if id < 0 || id >= vertices.len() as i64 {
                                Err("OBJ vertex index out of range")
                            } else {
                                Ok(id as usize)
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let c = current
                        .as_ref()
                        .and_then(|n| materials.get(n))
                        .copied()
                        .or_else(|| ids.first().map(|i| vertex_colors[*i]))
                        .unwrap_or(Rgba8::WHITE);
                    let p = ids.iter().map(|i| vertices[*i]).collect::<Vec<_>>();
                    if kind == "f" {
                        polygon(&mut out, &p, c)?;
                    } else {
                        if p.len() < 2 {
                            return Err("OBJ line needs two vertices".into());
                        }
                        out.segments.extend(p.windows(2).map(|p| ([p[0], p[1]], c)));
                    }
                }
                "mtllib" => {
                    for name in fields {
                        match resource(name) {
                            Ok(bytes) => {
                                let text = utf8(&bytes)?;
                                let mut name = None;
                                let mut rgba = [1.; 4];
                                for line in text.lines() {
                                    let mut f = line.split_whitespace();
                                    match f.next(){Some("newmtl")=>{if let Some(n)=name.take(){materials.insert(n,color(rgba));}name=f.next().map(str::to_owned);rgba=[1.;4];},Some("Kd")=>{for v in rgba.iter_mut().take(3){*v=number(f.next().ok_or("incomplete MTL color")?)?;}},Some("d")=>rgba[3]=number(f.next().ok_or("incomplete MTL alpha")?)?,Some("Tr")=>rgba[3]=1.-number(f.next().ok_or("incomplete MTL transparency")?)?,Some(k)if k.starts_with("map_")=>out.warn("OBJ texture maps are omitted; diffuse material colors are retained."),_=>{}}
                                }
                                if let Some(n) = name {
                                    materials.insert(n, color(rgba));
                                }
                            }
                            Err(e) => {
                                out.warn(format!("Material library {name} was not loaded: {e}"))
                            }
                        }
                    }
                }
                "usemtl" => current = fields.next().map(str::to_owned),
                "vn" | "vt" => out.warn(
                    "OBJ UVs and authored normals are omitted; normals are derived from winding.",
                ),
                "curv" | "surf" | "p" => {
                    out.warn("OBJ freeform surfaces, curves and point primitives are unsupported.")
                }
                _ => {}
            }
            Ok(())
        })();
        result.map_err(|e| format!("OBJ line {}: {e}", line_no + 1))?;
        if out.triangles.len() + out.segments.len() > MAX_PRIMITIVES {
            return Err("too many OBJ primitives".into());
        }
    }
    Ok(out)
}

pub(super) fn write_obj(m: &Mesh, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut s = String::from("# Meshcraft OBJ; RGB vertex colors; geometric normals\n");
    let mut index = 1;
    for (i, (p, c)) in m.triangles.iter().enumerate() {
        check(cancel)?;
        for p in p {
            writeln!(
                s,
                "v {} {} {} {} {} {}",
                p.x,
                p.y,
                p.z,
                c.red as f64 / 255.,
                c.green as f64 / 255.,
                c.blue as f64 / 255.
            )
            .unwrap();
        }
        let n = normal(*p);
        writeln!(
            s,
            "vn {} {} {}\nf {}//{} {}//{} {}//{}",
            n.x,
            n.y,
            n.z,
            index,
            i + 1,
            index + 1,
            i + 1,
            index + 2,
            i + 1
        )
        .unwrap();
        index += 3;
    }
    for (p, c) in &m.segments {
        check(cancel)?;
        for p in p {
            writeln!(
                s,
                "v {} {} {} {} {} {}",
                p.x,
                p.y,
                p.z,
                c.red as f64 / 255.,
                c.green as f64 / 255.,
                c.blue as f64 / 255.
            )
            .unwrap();
        }
        writeln!(s, "l {} {}", index, index + 1).unwrap();
        index += 2;
    }
    Ok(s.into_bytes())
}

pub(super) fn read_stl(bytes: &[u8], format: Format, cancel: &AtomicBool) -> Result<Mesh, String> {
    let binary = bytes
        .get(80..84)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        .filter(|n| *n <= MAX_PRIMITIVES)
        .is_some_and(|n| 84 + n * 50 == bytes.len());
    let mut out = Mesh::default();
    if format == Format::StlBinary || (format == Format::Stl && binary) {
        if bytes.len() < 84 {
            return Err("truncated binary STL header".into());
        }
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        if n > MAX_PRIMITIVES || bytes.len() != 84 + n * 50 {
            return Err("binary STL triangle count does not match file length".into());
        }
        for chunk in bytes[84..].as_chunks::<50>().0 {
            check(cancel)?;
            let mut points = [Vec3::ZERO; 3];
            for (i, p) in points.iter_mut().enumerate() {
                let at = 12 + i * 12;
                let read = |offset| {
                    f32::from_le_bytes(chunk[offset..offset + 4].try_into().unwrap()) as f64
                };
                *p = Vec3::new(read(at), read(at + 4), read(at + 8));
            }
            out.triangles.push((points, Rgba8::WHITE));
        }
    } else {
        let s = utf8(bytes)?;
        let mut points = vec![];
        let mut in_facet = false;
        let mut ended = false;
        if !s.trim_start().starts_with("solid") {
            return Err("ASCII STL must start with solid".into());
        }
        for line in s.lines() {
            check(cancel)?;
            let mut f = line.split_whitespace();
            match f.next() {
                Some("facet") => {
                    if in_facet {
                        return Err("nested STL facet".into());
                    }
                    in_facet = true;
                    points.clear();
                }
                Some("vertex") => {
                    if !in_facet || points.len() >= 3 {
                        return Err("invalid STL facet vertices".into());
                    }
                    points.push(Vec3::new(
                        number(f.next().ok_or("missing STL x")?)?,
                        number(f.next().ok_or("missing STL y")?)?,
                        number(f.next().ok_or("missing STL z")?)?,
                    ));
                }
                Some("endfacet") => {
                    if !in_facet || points.len() != 3 {
                        return Err("incomplete STL facet".into());
                    }
                    out.triangles
                        .push(([points[0], points[1], points[2]], Rgba8::WHITE));
                    in_facet = false;
                }
                Some("endsolid") => ended = true,
                _ => {}
            }
            if out.triangles.len() > MAX_PRIMITIVES {
                return Err("too many STL triangles".into());
            }
        }
        if in_facet || !ended {
            return Err("truncated ASCII STL".into());
        }
    }
    Ok(out)
}

pub(super) fn write_stl(m: &Mesh, format: Format, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    if format == Format::StlAscii {
        let mut s = String::from("solid meshcraft\n");
        for (p, _) in &m.triangles {
            check(cancel)?;
            let n = normal(*p);
            writeln!(s, " facet normal {} {} {}\n  outer loop", n.x, n.y, n.z).unwrap();
            for p in p {
                writeln!(s, "   vertex {} {} {}", p.x, p.y, p.z).unwrap();
            }
            s.push_str("  endloop\n endfacet\n");
        }
        s.push_str("endsolid meshcraft\n");
        Ok(s.into_bytes())
    } else {
        let mut b = vec![0; 80];
        b[..9].copy_from_slice(b"Meshcraft");
        b.extend_from_slice(&(m.triangles.len() as u32).to_le_bytes());
        for (p, _) in &m.triangles {
            check(cancel)?;
            for v in [normal(*p), p[0], p[1], p[2]] {
                for n in [v.x, v.y, v.z] {
                    let v = n as f32;
                    if !v.is_finite() {
                        return Err("STL coordinates exceed float32 range".into());
                    }
                    b.extend_from_slice(&v.to_le_bytes());
                }
            }
            b.extend_from_slice(&[0, 0]);
        }
        Ok(b)
    }
}
