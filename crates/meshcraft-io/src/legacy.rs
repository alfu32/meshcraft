//! Shape adapters for legacy project snapshots that predate prototype/instance persistence.

use serde_json::{Map, Value, json};

pub(crate) struct NormalizedScene {
    pub snapshot: Value,
    pub warnings: Vec<String>,
}

/// Converts the supported schema-17 scene shapes into the current prototype/instance envelope.
/// Unknown root and definition fields are left in the JSON tree for the core importer to retain.
pub(crate) fn normalize_scene_shape(mut snapshot: Value) -> Result<NormalizedScene, String> {
    let (prototype_count, has_instances) = {
        let root = snapshot
            .as_object()
            .ok_or_else(|| "Octodraw snapshot root must be a JSON object".to_owned())?;
        let prototype_count = match root.get("prototypes") {
            None | Some(Value::Null) => None,
            Some(Value::Array(prototypes)) => Some(prototypes.len()),
            Some(_) => return Err("Octodraw snapshot /prototypes must be an array".to_owned()),
        };
        let has_instances = root
            .get("rootInstance")
            .is_some_and(|value| !value.is_null());
        (prototype_count, has_instances)
    };

    let mut normalized = if has_instances && prototype_count.is_some_and(|count| count > 0) {
        snapshot
            .as_object_mut()
            .expect("snapshot root was validated above")
            .remove("rootGroup");
        snapshot
    } else if prototype_count.is_some_and(|count| count > 0) {
        let root = snapshot
            .as_object()
            .expect("snapshot root was validated above");
        let prototypes = root
            .get("prototypes")
            .and_then(Value::as_array)
            .expect("prototype array was validated above");
        normalize_prototype_only(root, prototypes)?
    } else if let Some(root_group) = snapshot
        .as_object()
        .and_then(|root| root.get("rootGroup"))
        .filter(|value| !value.is_null())
    {
        let root = snapshot
            .as_object()
            .expect("snapshot root was validated above");
        normalize_root_group(root, root_group)?
    } else {
        let root = snapshot
            .as_object()
            .expect("snapshot root was validated above");
        normalize_flat_geometry(root)?
    };

    let mut warnings = Vec::new();
    if repair_empty_root_basis(&mut normalized) {
        warnings.push(
            "The empty root instance had a zero basis; it was repaired to identity. Original axes were retained in project metadata.".to_owned(),
        );
    }
    Ok(NormalizedScene {
        snapshot: normalized,
        warnings,
    })
}

fn repair_empty_root_basis(snapshot: &mut Value) -> bool {
    let Some(root) = snapshot.as_object_mut() else {
        return false;
    };
    if root.contains_key("__meshcraft_migration_repair")
        || !empty_collections(root, &["segments", "faces"])
    {
        return false;
    }
    let Some(instance_value) = root.get("rootInstance") else {
        return false;
    };
    let Some(instance) = instance_value.as_object() else {
        return false;
    };
    let Some(prototype_id) = instance.get("prototypeId").and_then(Value::as_str) else {
        return false;
    };
    if !known_fields_only(
        instance,
        &[
            "id",
            "prototypeId",
            "instanceOrigin",
            "instanceAxisU",
            "instanceAxisV",
            "instanceAxisW",
            "children",
            "hotspotPositions",
            "hotspotSegmentAttachments",
            "hotspotTriangleAttachments",
            "overrideSegments",
            "overrideFaces",
            "overrideDimensions",
            "overrideTexts",
        ],
    ) || !empty_collections(
        instance,
        &[
            "children",
            "hotspotPositions",
            "hotspotSegmentAttachments",
            "hotspotTriangleAttachments",
            "overrideSegments",
            "overrideFaces",
            "overrideDimensions",
            "overrideTexts",
        ],
    ) || !["instanceAxisU", "instanceAxisV", "instanceAxisW"]
        .iter()
        .all(|key| zero_vector(instance.get(*key)))
    {
        return false;
    }
    let Some(prototype) = root
        .get("prototypes")
        .and_then(Value::as_array)
        .and_then(|prototypes| {
            prototypes.iter().find(|prototype| {
                prototype
                    .as_object()
                    .and_then(|object| object.get("id"))
                    .and_then(Value::as_str)
                    == Some(prototype_id)
            })
        })
        .and_then(Value::as_object)
    else {
        return false;
    };
    if !known_fields_only(
        prototype,
        &[
            "id",
            "name",
            "definitionOrigin",
            "definitionAxisU",
            "definitionAxisV",
            "definitionAxisW",
            "gluedToSurface",
            "kind",
            "externalReferenceEnabled",
            "externalReferencePath",
            "voxelColor",
            "voxels",
            "architectureWalls",
            "architectureSlabs",
            "architectureStairs",
            "architectureFrames",
            "hvacPlumbingRuns",
            "hvacVentilationDucts",
            "hotspots",
            "prototypeVertices",
            "segments",
            "faces",
            "dimensions",
            "texts",
        ],
    ) || !empty_collections(
        prototype,
        &[
            "segments",
            "faces",
            "voxels",
            "architectureWalls",
            "architectureSlabs",
            "architectureStairs",
            "architectureFrames",
            "hvacPlumbingRuns",
            "hvacVentilationDucts",
            "hotspots",
            "prototypeVertices",
            "dimensions",
            "texts",
        ],
    ) || !is_disabled(prototype.get("externalReferenceEnabled"))
    {
        return false;
    }

    let instance = root
        .get_mut("rootInstance")
        .and_then(Value::as_object_mut)
        .expect("root instance was checked above");
    let original_axes = ["instanceAxisU", "instanceAxisV", "instanceAxisW"]
        .into_iter()
        .map(|key| {
            (
                key.to_owned(),
                instance.get(key).cloned().unwrap_or(Value::Null),
            )
        })
        .collect::<Map<_, _>>();
    instance.insert("instanceAxisU".to_owned(), identity_u());
    instance.insert("instanceAxisV".to_owned(), identity_v());
    instance.insert("instanceAxisW".to_owned(), identity_w());
    root.insert(
        "__meshcraft_migration_repair".to_owned(),
        json!({
            "kind": "empty-root-zero-basis-to-identity",
            "originalInstanceAxes": original_axes
        }),
    );
    true
}

fn empty_collections(object: &Map<String, Value>, keys: &[&str]) -> bool {
    keys.iter().all(|key| match object.get(*key) {
        None => true,
        Some(Value::Array(values)) => values.is_empty(),
        _ => false,
    })
}

fn known_fields_only(object: &Map<String, Value>, keys: &[&str]) -> bool {
    object.keys().all(|key| keys.contains(&key.as_str()))
}

fn zero_vector(value: Option<&Value>) -> bool {
    match value {
        None => true,
        Some(Value::Object(vector)) => ["x", "y", "z"].iter().all(|axis| match vector.get(*axis) {
            None => true,
            Some(Value::Number(number)) => number.as_f64() == Some(0.0),
            _ => false,
        }),
        _ => false,
    }
}

fn is_disabled(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Bool(false)))
}

fn normalize_prototype_only(
    root: &Map<String, Value>,
    source_prototypes: &[Value],
) -> Result<Value, String> {
    let root_exists = source_prototypes.iter().any(|prototype| {
        prototype
            .as_object()
            .and_then(|object| object.get("id"))
            .and_then(Value::as_str)
            == Some("root")
    });
    let first_object = source_prototypes.iter().find_map(|prototype| {
        let object = prototype.as_object()?;
        let id = object.get("id")?.as_str()?;
        (id != "root").then_some(id.to_owned())
    });

    let mut output = root.clone();
    let mut prototypes = source_prototypes.to_vec();
    if !root_exists {
        prototypes.insert(0, root_prototype());
    }
    output.insert("prototypes".to_owned(), Value::Array(prototypes));
    output.insert(
        "rootInstance".to_owned(),
        root_instance(first_object.as_deref()),
    );
    output.remove("rootGroup");
    output.remove("segments");
    output.remove("faces");
    Ok(Value::Object(output))
}

fn normalize_root_group(root: &Map<String, Value>, root_group: &Value) -> Result<Value, String> {
    let mut prototypes = Vec::new();
    let mut next_id = 0usize;
    let root_instance = convert_group(root_group, "/rootGroup", &mut prototypes, &mut next_id)?;

    let mut output = root.clone();
    output.remove("rootGroup");
    output.remove("rootInstance");
    output.remove("prototypes");
    output.insert("prototypes".to_owned(), Value::Array(prototypes));
    output.insert("rootInstance".to_owned(), root_instance);
    Ok(Value::Object(output))
}

fn normalize_flat_geometry(root: &Map<String, Value>) -> Result<Value, String> {
    let mut prototype = root_prototype();
    let prototype_object = prototype
        .as_object_mut()
        .expect("synthetic root prototype is an object");
    prototype_object.insert(
        "segments".to_owned(),
        root.get("segments")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );
    prototype_object.insert(
        "faces".to_owned(),
        root.get("faces")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );

    let mut output = root.clone();
    output.remove("rootGroup");
    output.remove("rootInstance");
    output.remove("prototypes");
    output.remove("segments");
    output.remove("faces");
    output.insert("prototypes".to_owned(), Value::Array(vec![prototype]));
    output.insert("rootInstance".to_owned(), root_instance(None));
    Ok(Value::Object(output))
}

fn convert_group(
    value: &Value,
    path: &str,
    prototypes: &mut Vec<Value>,
    next_id: &mut usize,
) -> Result<Value, String> {
    let group = value
        .as_object()
        .ok_or_else(|| format!("{path}: group must be an object"))?;
    let source_id = optional_string(group, "id", path)?.unwrap_or_default();
    let name = optional_string(group, "name", path)?.unwrap_or_default();
    let prototype_id = format!("legacy:rootGroup:prototype:{}", *next_id);
    *next_id += 1;

    let mut prototype = Map::new();
    prototype.insert("id".to_owned(), Value::String(prototype_id.clone()));
    prototype.insert("name".to_owned(), Value::String(name));
    prototype.insert(
        "definitionOrigin".to_owned(),
        optional_non_null(group, "definitionOrigin").unwrap_or_else(|| json!({})),
    );
    prototype.insert(
        "definitionAxisU".to_owned(),
        optional_non_null(group, "definitionAxisU").unwrap_or_else(identity_u),
    );
    prototype.insert(
        "definitionAxisV".to_owned(),
        optional_non_null(group, "definitionAxisV").unwrap_or_else(identity_v),
    );
    prototype.insert(
        "definitionAxisW".to_owned(),
        optional_non_null(group, "definitionAxisW").unwrap_or_else(identity_w),
    );
    prototype.insert(
        "segments".to_owned(),
        group
            .get("segments")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );
    prototype.insert(
        "faces".to_owned(),
        group
            .get("faces")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );
    prototype.insert(
        "gluedToSurface".to_owned(),
        group
            .get("gluedToSurface")
            .cloned()
            .unwrap_or(Value::Bool(false)),
    );
    for (key, field) in group {
        if !group_dto_fields().contains(&key.as_str()) {
            prototype.insert(key.clone(), field.clone());
        }
    }
    prototypes.push(Value::Object(prototype));

    let children = match group.get("children") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(children)) => children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                convert_group(
                    child,
                    &format!("{path}/children/{index}"),
                    prototypes,
                    next_id,
                )
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(format!("{path}/children: expected an array")),
    };

    let mut instance = Map::new();
    let instance_id = if source_id.is_empty() {
        format!("legacy:rootGroup:instance:{prototype_id}")
    } else {
        source_id
    };
    instance.insert("id".to_owned(), Value::String(instance_id));
    instance.insert("prototypeId".to_owned(), Value::String(prototype_id));
    instance.insert(
        "instanceOrigin".to_owned(),
        instance_field(group, "instanceOrigin", "origin").unwrap_or_else(|| json!({})),
    );
    instance.insert(
        "instanceAxisU".to_owned(),
        instance_field(group, "instanceAxisU", "axisU").unwrap_or_else(|| json!({})),
    );
    instance.insert(
        "instanceAxisV".to_owned(),
        instance_field(group, "instanceAxisV", "axisV").unwrap_or_else(|| json!({})),
    );
    instance.insert(
        "instanceAxisW".to_owned(),
        instance_field(group, "instanceAxisW", "axisW").unwrap_or_else(|| json!({})),
    );
    instance.insert("children".to_owned(), Value::Array(children));
    for field in [
        "hotspotPositions",
        "hotspotSegmentAttachments",
        "hotspotTriangleAttachments",
        "overrideSegments",
        "overrideFaces",
        "overrideDimensions",
        "overrideTexts",
    ] {
        if let Some(value) = group.get(field) {
            instance.insert(field.to_owned(), value.clone());
        }
    }
    Ok(Value::Object(instance))
}

fn root_prototype() -> Value {
    json!({
        "id": "root",
        "name": "Root",
        "definitionOrigin": {},
        "definitionAxisU": identity_u(),
        "definitionAxisV": identity_v(),
        "definitionAxisW": identity_w(),
        "segments": [],
        "faces": []
    })
}

fn root_instance(first_child_prototype: Option<&str>) -> Value {
    let mut children = Vec::new();
    if let Some(prototype_id) = first_child_prototype {
        children.push(json!({
            "id": format!("legacy:prototype-recovery:{prototype_id}"),
            "prototypeId": prototype_id,
            "instanceOrigin": {},
            "instanceAxisU": identity_u(),
            "instanceAxisV": identity_v(),
            "instanceAxisW": identity_w()
        }));
    }
    json!({
        "id": "root",
        "prototypeId": "root",
        "instanceOrigin": {},
        "instanceAxisU": identity_u(),
        "instanceAxisV": identity_v(),
        "instanceAxisW": identity_w(),
        "children": children
    })
}

fn group_dto_fields() -> &'static [&'static str] {
    &[
        "id",
        "name",
        "origin",
        "axisU",
        "axisV",
        "axisW",
        "definitionOrigin",
        "definitionAxisU",
        "definitionAxisV",
        "definitionAxisW",
        "instanceOrigin",
        "instanceAxisU",
        "instanceAxisV",
        "instanceAxisW",
        "gluedToSurface",
        "segments",
        "faces",
        "children",
        "hotspotPositions",
        "hotspotSegmentAttachments",
        "hotspotTriangleAttachments",
        "overrideSegments",
        "overrideFaces",
        "overrideDimensions",
        "overrideTexts",
    ]
}

fn optional_string(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{path}/{key}: expected a string")),
    }
}

fn optional_non_null(object: &Map<String, Value>, key: &str) -> Option<Value> {
    object.get(key).filter(|value| !value.is_null()).cloned()
}

fn identity_u() -> Value {
    json!({"x": 1})
}

fn identity_v() -> Value {
    json!({"y": 1})
}

fn identity_w() -> Value {
    json!({"z": 1})
}

fn instance_field(group: &Map<String, Value>, explicit: &str, fallback: &str) -> Option<Value> {
    optional_non_null(group, explicit).or_else(|| optional_non_null(group, fallback))
}
