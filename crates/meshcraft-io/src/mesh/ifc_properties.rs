//! IFC4X3_ADD2 property values, independent of geometry representations.
use super::*;
use meshcraft_core::document::ifc::{self, Kind, Properties, Property};
use serde_json::Value;
fn emit(s: &mut String, id: &mut u64, record: String) -> u64 {
    let n = *id;
    *id += 1;
    writeln!(s, "#{n}={record};").unwrap();
    n
}
fn text(s: &str) -> String {
    format!("'{}'", ifc_string(s))
}
fn value(v: &Value, kind: &str) -> Result<String, String> {
    if v.is_null() {
        return Ok("$".into());
    }
    if !ifc::catalog()
        .types
        .iter()
        .any(|t| t.eq_ignore_ascii_case(kind))
    {
        return Err(format!("Unknown IFC value type {kind}"));
    }
    let v = match v {
        Value::String(s) => {
            if kind == "IfcLogical" && s == "UNKNOWN" {
                ".U.".into()
            } else {
                text(s)
            }
        }
        Value::Bool(v) => {
            if *v {
                ".T.".into()
            } else {
                ".F.".into()
            }
        }
        Value::Number(n) => {
            if kind == "IfcInteger" {
                n.to_string()
            } else {
                ifc_real(n.as_f64().ok_or("Invalid IFC number")?)
            }
        }
        Value::Array(values) => format!(
            "({})",
            values
                .iter()
                .map(|v| match v {
                    Value::Number(n) => Ok(ifc_real(n.as_f64().ok_or("Invalid IFC array number")?)),
                    Value::String(v) => Ok(text(v)),
                    _ => Err("Unsupported IFC aggregate member"),
                })
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ),
        _ => {
            return Err(
                "IFC scalar values must be text, boolean, numeric or a typed aggregate".into(),
            );
        }
    };
    Ok(format!("{}({v})", kind.to_uppercase()))
}
fn unit(s: &mut String, id: &mut u64, unit: Option<&str>) -> Result<String, String> {
    let Some(u) = unit else {
        return Ok("$".into());
    };
    let (kind, prefix, name) = match u {
        "m" => ("LENGTHUNIT", "$", "METRE"),
        "mm" => ("LENGTHUNIT", ".MILLI.", "METRE"),
        "cm" => ("LENGTHUNIT", ".CENTI.", "METRE"),
        "m2" | "m²" => ("AREAUNIT", "$", "SQUARE_METRE"),
        "m3" | "m³" => ("VOLUMEUNIT", "$", "CUBIC_METRE"),
        "s" => ("TIMEUNIT", "$", "SECOND"),
        "kg" => ("MASSUNIT", ".KILO.", "GRAM"),
        "K" => ("THERMODYNAMICTEMPERATUREUNIT", "$", "KELVIN"),
        _ => {
            return Err(format!(
                "IFC export unit {u} is not a supported SI symbol; use m, mm, cm, m2, m3, s, kg or K"
            ));
        }
    };
    Ok(format!(
        "#{}",
        emit(s, id, format!("IFCSIUNIT(*,.{kind}.,{prefix},.{name}.)"))
    ))
}
fn property(
    s: &mut String,
    id: &mut u64,
    name: &str,
    p: &Property,
    depth: usize,
) -> Result<u64, String> {
    if depth > 16 {
        return Err("IFC complex property nesting exceeds 16".into());
    }
    p.validate()?;
    let n = text(name);
    let d = if p.description.is_empty() {
        "$".into()
    } else {
        text(&p.description)
    };
    let typed = |v: &Value| value(v, &p.data_type);
    let list_of = |v: &Value, data_type: &str| -> Result<String, String> {
        let Some(a) = v.as_array().filter(|a| !a.is_empty()) else {
            return Ok("$".into());
        };
        Ok(format!(
            "({})",
            a.iter()
                .map(|v| value(v, data_type))
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ))
    };
    let list = |v: &Value| list_of(v, &p.data_type);
    let record = match p.kind {
        Kind::Single => format!(
            "IFCPROPERTYSINGLEVALUE({n},{d},{},{})",
            typed(&p.value)?,
            unit(s, id, p.unit.as_deref())?
        ),
        Kind::Enumerated => {
            let reference = if p.enumeration.is_some() || p.unit.is_some() {
                let choices = p
                    .enumeration
                    .as_ref()
                    .map_or_else(|| p.value.clone(), |v| serde_json::json!(v));
                let list = list(&choices)?;
                let unit = unit(s, id, p.unit.as_deref())?;
                format!(
                    "#{}",
                    emit(s, id, format!("IFCPROPERTYENUMERATION({n},{list},{unit})"))
                )
            } else {
                "$".into()
            };
            format!(
                "IFCPROPERTYENUMERATEDVALUE({n},{d},{},{reference})",
                list(&p.value)?
            )
        }
        Kind::List => format!(
            "IFCPROPERTYLISTVALUE({n},{d},{},{})",
            list(&p.value)?,
            unit(s, id, p.unit.as_deref())?
        ),
        Kind::Bounded => format!(
            "IFCPROPERTYBOUNDEDVALUE({n},{d},{},{},{},{})",
            typed(&p.value["upper"])?,
            typed(&p.value["lower"])?,
            unit(s, id, p.unit.as_deref())?,
            typed(&p.value["setPoint"])?
        ),
        Kind::Table => format!(
            "IFCPROPERTYTABLEVALUE({n},{d},{},{},{},{},{},.{}.)",
            list(&p.value["defining"])?,
            list_of(
                &p.value["defined"],
                p.secondary_data_type.as_deref().unwrap_or(&p.data_type)
            )?,
            p.expression.as_deref().map(text).unwrap_or("$".into()),
            unit(s, id, p.unit.as_deref())?,
            unit(s, id, p.secondary_unit.as_deref())?,
            p.interpolation.as_deref().unwrap_or("NOTDEFINED")
        ),
        Kind::Reference => {
            let v = &p.value;
            let usage = p.usage_name.as_deref().map(text).unwrap_or("$".into());
            if v.is_null() {
                return Ok(emit(
                    s,
                    id,
                    format!("IFCPROPERTYREFERENCEVALUE({n},{d},{usage},$)"),
                ));
            }
            if !["IfcLabel", "IfcDocumentReference"].contains(&p.data_type.as_str()) {
                return Err(format!(
                    "Reference type {} is retained in project metadata but needs a BIM object graph for IFC export",
                    p.data_type
                ));
            }
            let location = v.as_str().or_else(|| v["location"].as_str()).unwrap_or("");
            let r = emit(
                s,
                id,
                format!(
                    "IFCDOCUMENTREFERENCE({},{},{},$,$)",
                    text(location),
                    v["identification"].as_str().map(text).unwrap_or("$".into()),
                    v["name"].as_str().map(text).unwrap_or("$".into())
                ),
            );
            format!("IFCPROPERTYREFERENCEVALUE({n},{d},{usage},#{r})")
        }
        Kind::Complex => {
            let children: BTreeMap<String, Property> =
                serde_json::from_value(p.value.clone()).map_err(|e| e.to_string())?;
            let refs = children
                .iter()
                .map(|(n, p)| property(s, id, n, p, depth + 1).map(|id| format!("#{id}")))
                .collect::<Result<Vec<_>, _>>()?;
            if refs.is_empty() {
                return Err("An IFC complex property needs at least one child".into());
            }
            format!(
                "IFCCOMPLEXPROPERTY({n},{d},{},({}))",
                text(p.usage_name.as_deref().unwrap_or("Meshcraft")),
                refs.join(",")
            )
        }
        Kind::Quantity => {
            let kind = match p.data_type.as_str() {
                "Q_LENGTH" | "IfcLengthMeasure" => "LENGTH",
                "Q_AREA" | "IfcAreaMeasure" => "AREA",
                "Q_VOLUME" | "IfcVolumeMeasure" => "VOLUME",
                "Q_WEIGHT" | "IfcMassMeasure" => "WEIGHT",
                "Q_TIME" | "IfcTimeMeasure" => "TIME",
                "Q_NUMBER" => "NUMBER",
                _ => "COUNT",
            };
            format!(
                "IFCQUANTITY{kind}({n},{d},{},{},$)",
                unit(s, id, p.unit.as_deref())?,
                ifc_real(
                    p.value
                        .as_f64()
                        .ok_or("Quantities require numeric values")?
                )
            )
        }
    };
    Ok(emit(s, id, record))
}
pub(super) fn write(
    s: &mut String,
    id: &mut u64,
    product: u64,
    properties: &Properties,
) -> Result<(), String> {
    ifc::validate(properties)?;
    for (set, values) in properties {
        for quantities in [false, true] {
            let mut refs = vec![];
            for (name, p) in values
                .iter()
                .filter(|(_, p)| (p.kind == Kind::Quantity) == quantities)
            {
                refs.push(format!("#{}", property(s, id, name, p, 0)?));
            }
            if refs.is_empty() {
                continue;
            }
            let n = *id;
            let record = if quantities {
                format!(
                    "IFCELEMENTQUANTITY('{n:022x}',$,{},$,$,({}))",
                    text(set),
                    refs.join(",")
                )
            } else {
                format!(
                    "IFCPROPERTYSET('{n:022x}',$,{},$,({}))",
                    text(set),
                    refs.join(",")
                )
            };
            let set_id = emit(s, id, record);
            let n = *id;
            emit(
                s,
                id,
                format!("IFCRELDEFINESBYPROPERTIES('{n:022x}',$,$,$,(#{product}),#{set_id})"),
            );
        }
    }
    Ok(())
}
fn string(v: &Step) -> String {
    if let Step::Other(s) = v {
        let s = s
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .unwrap_or(s)
            .replace("''", "'");
        let mut result = String::new();
        let mut rest = s.as_str();
        while let Some(i) = rest.find("\\X2\\") {
            result.push_str(&rest[..i]);
            let Some(end) = rest[i + 4..].find("\\X0\\") else {
                break;
            };
            let raw = &rest[i + 4..i + 4 + end];
            let utf16 = raw
                .as_bytes()
                .as_chunks::<4>()
                .0
                .iter()
                .filter_map(|v| {
                    std::str::from_utf8(v)
                        .ok()
                        .and_then(|v| u16::from_str_radix(v, 16).ok())
                })
                .collect::<Vec<_>>();
            result.push_str(&String::from_utf16_lossy(&utf16));
            rest = &rest[i + 4 + end + 4..];
        }
        result.push_str(rest);
        result
    } else {
        String::new()
    }
}
fn scalar(v: &Step) -> (String, Value) {
    match v {
        Step::Num(n) => ("IfcReal".into(), serde_json::json!(n)),
        Step::Other(s) => {
            if let Some((t, args)) = s.split_once('(')
                && let Some(args) = args.strip_suffix(')')
            {
                let parsed = step_value(args, 0).unwrap_or(Step::Other(args.into()));
                let (_, mut value) = scalar(&parsed);
                let dtype = ifc::catalog()
                    .types
                    .iter()
                    .find(|k| k.eq_ignore_ascii_case(t))
                    .cloned()
                    .unwrap_or(t.into());
                if ifc::catalog()
                    .type_kinds
                    .get(&dtype)
                    .is_some_and(|k| k == "INTEGER")
                    && let Some(n) = value.as_f64().filter(|n| n.fract() == 0.)
                {
                    value = serde_json::json!(n as i64);
                }
                return (dtype, value);
            }
            (
                "IfcLabel".into(),
                match s.as_str() {
                    "$" => Value::Null,
                    ".T." => Value::Bool(true),
                    ".F." => Value::Bool(false),
                    ".U." => Value::String("UNKNOWN".into()),
                    _ => Value::String(string(v)),
                },
            )
        }
        Step::List(values) => (
            "IfcComplexNumber".into(),
            Value::Array(values.iter().map(|v| scalar(v).1).collect()),
        ),
        _ => ("IfcLabel".into(), Value::Null),
    }
}
fn read_unit(e: &Entities, v: &Step) -> Option<String> {
    let (_, u) = e.get(&v.reference().ok()?)?;
    let kind = u.get(3).map(string).unwrap_or_default();
    let prefix = u.get(2).map(string).unwrap_or_default();
    match (kind.as_str(), prefix.as_str()) {
        (".METRE.", ".MILLI.") => Some("mm".into()),
        (".METRE.", ".CENTI.") => Some("cm".into()),
        (".METRE.", _) => Some("m".into()),
        (".SQUARE_METRE.", _) => Some("m2".into()),
        (".CUBIC_METRE.", _) => Some("m3".into()),
        (".SECOND.", _) => Some("s".into()),
        (".GRAM.", ".KILO.") => Some("kg".into()),
        (".KELVIN.", _) => Some("K".into()),
        _ => None,
    }
}

fn read_property(e: &Entities, id: u64, depth: usize) -> Option<(String, Property)> {
    if depth > 16 {
        return None;
    }
    let (kind, a) = e.get(&id)?;
    let name = string(a.first()?);
    let mut p = Property {
        description: a
            .get(1)
            .map(string)
            .filter(|s| s != "$")
            .unwrap_or_default(),
        ..Default::default()
    };
    let v = a.get(2)?;
    let values = |v: &Step| {
        v.list()
            .ok()
            .map(|a| a.iter().map(scalar).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    match kind.as_str() {
        "IFCPROPERTYSINGLEVALUE" => {
            (p.data_type, p.value) = scalar(v);
        }
        "IFCPROPERTYENUMERATEDVALUE" | "IFCPROPERTYLISTVALUE" => {
            p.kind = if kind == "IFCPROPERTYLISTVALUE" {
                Kind::List
            } else {
                Kind::Enumerated
            };
            let list = values(v);
            p.data_type = list
                .first()
                .map(|v| v.0.clone())
                .unwrap_or("IfcLabel".into());
            p.value = Value::Array(list.into_iter().map(|v| v.1).collect());
            if p.kind == Kind::Enumerated
                && let Some((_, enumeration)) = a
                    .get(3)
                    .and_then(|v| v.reference().ok())
                    .and_then(|id| e.get(&id))
            {
                p.enumeration = Some(
                    enumeration
                        .get(1)
                        .map(values)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|v| v.1)
                        .collect(),
                );
            }
        }
        "IFCPROPERTYBOUNDEDVALUE" => {
            p.kind = Kind::Bounded;
            let (t, upper) = scalar(v);
            let (_, lower) = scalar(a.get(3)?);
            p.data_type = t;
            p.value = serde_json::json!({"upper":upper,"lower":lower,"setPoint":a.get(5).map(scalar).map(|v|v.1).unwrap_or(Value::Null)});
        }
        "IFCPROPERTYTABLEVALUE" => {
            p.kind = Kind::Table;
            let defining = values(v);
            p.data_type = defining
                .first()
                .map(|v| v.0.clone())
                .unwrap_or("IfcLabel".into());
            let defined = values(a.get(3)?);
            p.secondary_data_type = defined
                .first()
                .map(|v| v.0.clone())
                .filter(|t| *t != p.data_type);
            p.expression = a.get(4).map(string).filter(|v| v != "$");
            p.interpolation = a
                .get(7)
                .map(string)
                .map(|v| v.trim_matches('.').to_owned())
                .filter(|v| v != "NOTDEFINED");
            p.value = serde_json::json!({"defining":defining.into_iter().map(|v|v.1).collect::<Vec<_>>(),"defined":defined.into_iter().map(|v|v.1).collect::<Vec<_>>()});
        }
        "IFCPROPERTYREFERENCEVALUE" => {
            p.kind = Kind::Reference;
            p.usage_name = Some(string(v)).filter(|v| v != "$");
            if a.get(3).and_then(|v| v.reference().ok()).is_none() {
                p.value = Value::Null;
                return Some((name, p));
            }
            let (_, r) = e.get(&a.get(3)?.reference().ok()?)?;
            p.value = serde_json::json!({"location":r.first().map(string).unwrap_or_default(),"identification":r.get(1).map(string).filter(|s|s!="$").unwrap_or_default(),"name":r.get(2).map(string).filter(|s|s!="$").unwrap_or_default()});
        }
        "IFCCOMPLEXPROPERTY" => {
            p.kind = Kind::Complex;
            p.usage_name = Some(string(v)).filter(|v| v != "Meshcraft");
            let mut children = BTreeMap::new();
            for r in a.get(3)?.list().ok()? {
                if let Some((name, child)) = read_property(e, r.reference().ok()?, depth + 1) {
                    children.insert(name, child);
                }
            }
            p.value = serde_json::to_value(children).ok()?;
        }
        k if k.starts_with("IFCQUANTITY") => {
            p.kind = Kind::Quantity;
            p.data_type = format!("Q_{}", k.trim_start_matches("IFCQUANTITY"));
            p.value = scalar(a.get(3)?).1;
            if p.data_type == "Q_COUNT"
                && let Some(n) = a.get(3)?.num().ok().filter(|n| n.fract() == 0.)
            {
                p.value = Value::from(n as i64);
            }
        }
        _ => return None,
    }
    let unit_index = match p.kind {
        Kind::Single | Kind::List => Some(3),
        Kind::Bounded => Some(4),
        Kind::Table => Some(5),
        Kind::Quantity => Some(2),
        _ => None,
    };
    p.unit = unit_index
        .and_then(|i| a.get(i))
        .and_then(|v| read_unit(e, v));
    if p.kind == Kind::Table {
        p.secondary_unit = a.get(6).and_then(|v| read_unit(e, v));
    }
    if p.kind == Kind::Enumerated {
        p.unit = a
            .get(3)
            .and_then(|v| v.reference().ok())
            .and_then(|id| e.get(&id))
            .and_then(|(_, a)| a.get(2))
            .and_then(|v| read_unit(e, v));
    }
    Some((name, p))
}
pub(super) fn read(e: &Entities, product: u64) -> Properties {
    let mut result = Properties::new();
    for (kind, a) in e.values() {
        if kind != "IFCRELDEFINESBYPROPERTIES" {
            continue;
        }
        if !a
            .get(4)
            .and_then(|v| v.list().ok())
            .is_some_and(|refs| refs.iter().any(|r| r.reference().ok() == Some(product)))
        {
            continue;
        }
        let Some((kind, pset)) = a
            .get(5)
            .and_then(|v| v.reference().ok())
            .and_then(|id| e.get(&id))
        else {
            continue;
        };
        let index = if kind == "IFCELEMENTQUANTITY" {
            5
        } else if kind == "IFCPROPERTYSET" {
            4
        } else {
            continue;
        };
        let set_name = pset.get(2).map(string).unwrap_or_default();
        if let Some(props) = pset.get(index).and_then(|v| v.list().ok()) {
            for r in props {
                if let Some((name, p)) = r.reference().ok().and_then(|id| read_property(e, id, 0)) {
                    result.entry(set_name.clone()).or_default().insert(name, p);
                }
            }
        }
    }
    result
}
