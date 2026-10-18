//! Import and export adapters for Octodraw's schema-17 prototype/instance JSON format.

use crate::document::{
    ChildLayoutId, DefinitionId, DefinitionKind, Document, InstanceEdits, LayoutEntryId,
    LegacyAlias, ModelSettings, OccurrenceRef, RawGeometrySegment, RawGeometryTriangle,
    RawGeometryVertex, RawVertexRef, Rgba8,
};
use crate::geometry::{Affine3, GeometryTolerance, Vec3};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const LEGACY_VERTEX_KEY_EPSILON: f64 = 1.0e-3;
const MAX_INSTANCE_COUNT: usize = 100_000;
const MAX_INSTANCE_DEPTH: usize = 256;
const SCHEMA17_DOMAIN_ARRAY_FIELDS: &[&str] = &[
    "voxels",
    "architectureWalls",
    "architectureSlabs",
    "architectureStairs",
    "architectureFrames",
    "hvacPlumbingRuns",
    "hvacVentilationDucts",
    "hotspots",
];

type ImportResult<T> = Result<T, String>;
type ParsedOverrideSegment = (Vec3, Vec3, Vec<LegacyAlias>, BTreeMap<String, Value>);
type ParsedOverrideFace = (
    [Vec3; 3],
    Rgba8,
    Vec<LegacyAlias>,
    BTreeMap<String, Value>,
    Option<Value>,
);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct QuantizedVertex(i64, i64, i64);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct LayoutKey {
    definition_id: String,
    children: Vec<(String, [u64; 12])>,
}

#[derive(Clone, Debug)]
struct GeometryDraft {
    vertices: Vec<RawGeometryVertex>,
    segments: Vec<RawGeometrySegment>,
    triangles: Vec<RawGeometryTriangle>,
}

impl GeometryDraft {
    fn new() -> Self {
        Self {
            vertices: Vec::new(),
            segments: Vec::new(),
            triangles: Vec::new(),
        }
    }

    fn add_vertex(
        &mut self,
        position: Vec3,
        aliases: Vec<LegacyAlias>,
        reusable: &mut HashMap<QuantizedVertex, usize>,
        quantized_alias: Option<QuantizedVertex>,
    ) -> RawVertexRef {
        if let Some(key) = quantized_alias {
            if let Some(index) = reusable.get(&key) {
                return RawVertexRef {
                    vertex: *index,
                    position_offset: position - self.vertices[*index].position,
                };
            }
            let index = self.vertices.len();
            self.vertices.push(RawGeometryVertex {
                position,
                legacy_aliases: aliases,
            });
            reusable.insert(key, index);
            return RawVertexRef {
                vertex: index,
                position_offset: Vec3::ZERO,
            };
        }

        let index = self.vertices.len();
        self.vertices.push(RawGeometryVertex {
            position,
            legacy_aliases: aliases,
        });
        RawVertexRef {
            vertex: index,
            position_offset: Vec3::ZERO,
        }
    }

    fn into_parts(
        self,
    ) -> (
        Vec<RawGeometryVertex>,
        Vec<RawGeometrySegment>,
        Vec<RawGeometryTriangle>,
    ) {
        (self.vertices, self.segments, self.triangles)
    }
}

#[derive(Clone, Debug)]
struct LegacyInstance {
    id: String,
    prototype_id: String,
    transform: Affine3,
    children: Vec<LegacyInstance>,
    hotspot_positions: BTreeMap<String, Vec3>,
    geometry_override: Option<GeometryDraft>,
    preserved_fields: BTreeMap<String, Value>,
    layout_key: LayoutKey,
}

#[derive(Clone, Debug)]
struct LayoutVariant {
    id: ChildLayoutId,
    entries: Vec<LayoutEntryId>,
}

/// Imports prototype/instance Octodraw JSON into a normalized document.
///
/// This path preserves unsupported DTO fields as opaque extension values. It does not perform
/// topology repair, coordinate welding, domain regeneration, or schema-17 export.
pub fn import_current_json(source: &str) -> ImportResult<Document> {
    let snapshot: Value =
        serde_json::from_str(source).map_err(|error| format!("invalid Octodraw JSON: {error}"))?;
    import_current_value(&snapshot)
}

/// Imports a decoded current-format snapshot value.
pub fn import_current_value(snapshot: &Value) -> ImportResult<Document> {
    let root = snapshot
        .as_object()
        .ok_or_else(|| "Octodraw snapshot root must be a JSON object".to_owned())?;
    if root.get("rootGroup").is_some() {
        return Err("legacy rootGroup documents are not supported by this importer yet".to_owned());
    }
    if root.get("prototypes").is_none()
        && (root.contains_key("segments") || root.contains_key("faces"))
    {
        return Err(
            "flat legacy geometry documents are not supported by this importer yet".to_owned(),
        );
    }

    let prototype_values = root
        .get("prototypes")
        .and_then(Value::as_array)
        .ok_or_else(|| "Octodraw snapshot is missing its prototypes array".to_owned())?;
    if prototype_values.is_empty() {
        return Err("Octodraw snapshot has no prototypes".to_owned());
    }

    let mut document = Document::new();
    document.model_settings = parse_model_settings(root)?;
    preserve_root_fields(root, &mut document);
    let mut definitions = HashMap::new();
    for (index, prototype) in prototype_values.iter().enumerate() {
        let path = format!("/prototypes/{index}");
        let object = prototype
            .as_object()
            .ok_or_else(|| format!("{path}: prototype must be an object"))?;
        let legacy_id = read_string(object, "id", &path)?.unwrap_or_default();
        if legacy_id.is_empty() {
            return Err(format!("{path}/id: prototype ID is required"));
        }
        if definitions.contains_key(&legacy_id) {
            return Err(format!("{path}/id: duplicate prototype ID {legacy_id:?}"));
        }
        let name = read_string(object, "name", &path)?
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Object".to_owned());
        let kind = parse_definition_kind(object, &path)?;
        let frame = parse_transform(
            object,
            "definitionOrigin",
            ["definitionAxisU", "definitionAxisV", "definitionAxisW"],
            &path,
        )?;
        validate_transform(frame, &format!("{path}/definitionOrigin"))?;

        let definition_id = document.add_definition(name, kind);
        let definition = document
            .definitions
            .get_mut(&definition_id)
            .expect("new definition is present");
        definition.frame = frame;
        definition.legacy_aliases.push(LegacyAlias {
            source: "k3d.prototype".to_owned(),
            id: legacy_id.clone(),
        });
        preserve_unknown_fields(
            object,
            &[
                "id",
                "name",
                "kind",
                "definitionOrigin",
                "definitionAxisU",
                "definitionAxisV",
                "definitionAxisW",
                "prototypeVertices",
                "segments",
                "faces",
            ],
            &mut definition.extensions,
            "k3d.",
        );
        for field in SCHEMA17_DOMAIN_ARRAY_FIELDS.iter().copied().chain([
            "dimensions",
            "texts",
            "voxelColor",
        ]) {
            if let Some(value) = object.get(field) {
                definition.extensions.insert(
                    format!("k3d.{field}"),
                    normalize_sparse_component_dtos(field, value),
                );
            }
        }
        if let Some(vertices) = object.get("prototypeVertices") {
            let vertices = normalize_sparse_dto_array(
                vertices,
                &serde_json::json!({"id": "", "x": 0, "y": 0, "z": 0}),
            );
            definition
                .extensions
                .insert("k3d.__raw_prototype_vertices".to_owned(), vertices.clone());
            if let Some(vertices) = vertices.as_array() {
                for (vertex_index, vertex) in vertices.iter().enumerate() {
                    if let Some(vertex) = vertex.as_object() {
                        preserve_unknown_fields(
                            vertex,
                            &["id", "x", "y", "z"],
                            &mut definition.extensions,
                            &format!("k3d.prototypeVertices.{vertex_index}."),
                        );
                    }
                }
            }
        }
        if let Some(kind) = object.get("kind")
            && parse_definition_kind(object, &path)? == DefinitionKind::Other
        {
            definition
                .extensions
                .insert("k3d.__raw_kind".to_owned(), kind.clone());
        }

        let geometry = parse_prototype_geometry(object, &path)?;
        let (vertices, segments, triangles) = geometry.into_parts();
        document.replace_definition_geometry_raw(definition_id, vertices, segments, triangles)?;
        definitions.insert(legacy_id, definition_id);
    }

    let root_instance = root
        .get("rootInstance")
        .ok_or_else(|| "Octodraw snapshot has no rootInstance".to_owned())?;
    let mut instance_count = 0;
    let mut root_node = parse_instance(
        root_instance,
        "/rootInstance",
        &definitions,
        0,
        &mut instance_count,
    )?;

    let mut instance_groups: BTreeMap<LayoutKey, Vec<&LegacyInstance>> = BTreeMap::new();
    collect_layout_groups(&root_node, &mut instance_groups);
    let mut variants = BTreeMap::new();
    for (key, instances) in &instance_groups {
        let layout_id = document.add_child_layout();
        let representative = instances
            .first()
            .ok_or_else(|| "empty child-layout variant".to_owned())?;
        let mut entry_ids = Vec::with_capacity(representative.children.len());
        for (child_index, child) in representative.children.iter().enumerate() {
            let definition_id = definitions
                .get(&child.prototype_id)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "instance {} references missing prototype {}",
                        child.id, child.prototype_id
                    )
                })?;
            let entry_id = document
                .add_layout_entry(layout_id, definition_id, child.transform)
                .ok_or_else(|| format!("could not create child entry for instance {}", child.id))?;
            let mut aliases = BTreeSet::new();
            for instance in instances {
                if let Some(child) = instance.children.get(child_index)
                    && !child.id.is_empty()
                {
                    aliases.insert(child.id.clone());
                }
            }
            let entry = document
                .child_layouts
                .get_mut(&layout_id)
                .and_then(|layout| layout.entries.iter_mut().find(|entry| entry.id == entry_id))
                .expect("layout entry was inserted");
            entry.legacy_aliases = aliases
                .into_iter()
                .map(|id| LegacyAlias {
                    source: "k3d.instance".to_owned(),
                    id,
                })
                .collect();
            entry_ids.push(entry_id);
        }
        variants.insert(
            key.clone(),
            LayoutVariant {
                id: layout_id,
                entries: entry_ids,
            },
        );
    }

    let mut default_layouts = BTreeMap::new();
    collect_default_layouts(&root_node, &variants, &mut default_layouts)?;
    for (legacy_id, layout) in &default_layouts {
        let definition_id = definitions.get(legacy_id).copied().ok_or_else(|| {
            format!("default child layout references missing prototype {legacy_id}")
        })?;
        if !document.set_default_child_layout(definition_id, Some(*layout)) {
            return Err(format!(
                "could not set default children for prototype {legacy_id}"
            ));
        }
    }

    let root_definition = definitions
        .get(&root_node.prototype_id)
        .copied()
        .ok_or_else(|| {
            format!(
                "root instance references missing prototype {}",
                root_node.prototype_id
            )
        })?;
    let root_entry = document
        .add_layout_entry(document.root_layout(), root_definition, root_node.transform)
        .ok_or_else(|| "could not create root placement".to_owned())?;
    if !root_node.id.is_empty() {
        document
            .child_layouts
            .get_mut(&document.root_layout())
            .and_then(|layout| {
                layout
                    .entries
                    .iter_mut()
                    .find(|entry| entry.id == root_entry)
            })
            .expect("root placement was inserted")
            .legacy_aliases
            .push(LegacyAlias {
                source: "k3d.instance".to_owned(),
                id: root_node.id.clone(),
            });
    }
    let root_path = OccurrenceRef::root(root_entry);
    apply_instance_edits(
        &mut document,
        &root_node,
        &root_path,
        &definitions,
        &default_layouts,
        &variants,
    )?;
    attach_child_edits(
        &mut document,
        &root_node,
        &root_path,
        &definitions,
        &default_layouts,
        &variants,
    )?;

    document.validate()?;
    // Keep a stable signature for the imported root in document metadata.
    document.extensions.insert(
        "k3d.root_instance_id".to_owned(),
        Value::String(std::mem::take(&mut root_node.id)),
    );
    Ok(document)
}

/// Returns the schema-17 perspective camera block retained by the normalized document.
pub fn legacy_camera_state(document: &Document) -> Option<&Value> {
    document.extensions.get("k3d.cameraState")
}

/// Builds a persistence snapshot with the active perspective camera pose.
///
/// Unknown keys in an imported camera block are retained. The source document is not changed,
/// and camera motion therefore remains outside geometry revision and undo history.
pub fn snapshot_with_legacy_camera_state(
    document: &Document,
    camera_state: Value,
) -> ImportResult<Document> {
    let replacement = camera_state
        .as_object()
        .ok_or_else(|| "legacy camera state must be a JSON object".to_owned())?;
    let mut snapshot = document.clone();
    let mut merged = snapshot
        .extensions
        .get("k3d.cameraState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    merged.extend(replacement.clone());
    snapshot
        .extensions
        .insert("k3d.cameraState".to_owned(), Value::Object(merged));
    Ok(snapshot)
}

fn preserve_root_fields(root: &Map<String, Value>, document: &mut Document) {
    let source_version = root.get("version").cloned().unwrap_or(Value::from(17));
    document
        .extensions
        .insert("k3d.source_schema_version".to_owned(), source_version);
    document.extensions.insert(
        "k3d.source_schema_version_present".to_owned(),
        Value::Bool(root.contains_key("version")),
    );
    preserve_unknown_fields(
        root,
        &[
            "version",
            "prototypes",
            "rootInstance",
            "rootGroup",
            "segments",
            "faces",
            "modelUnit",
            "gridSpacing",
            "circleSegments",
            "snapEpsilon",
            "undoHistory",
        ],
        &mut document.extensions,
        "k3d.",
    );
    if let Some(unit) = root.get("modelUnit").and_then(Value::as_object) {
        preserve_unknown_fields(
            unit,
            &["name", "size"],
            &mut document.extensions,
            "k3d.modelUnit.",
        );
    }
}

/// Expands the normalized document into a schema-17 Octodraw prototype/instance snapshot.
///
/// The writer deliberately errors when a typed value has no legacy mapping. Opaque values read
/// from legacy JSON remain under their `k3d.*` extension keys and are restored verbatim.
pub fn export_current_value(document: &Document) -> ImportResult<Value> {
    document.validate()?;
    let mut prototype_ids = BTreeMap::new();
    let mut used_prototype_ids = BTreeSet::new();
    for definition in document.definitions.values() {
        let candidate = first_alias(&definition.legacy_aliases, "k3d.prototype");
        let fallback = format!("meshcraft-prototype-{}", definition.id.0);
        prototype_ids.insert(
            definition.id,
            unique_legacy_id(candidate, &fallback, &mut used_prototype_ids),
        );
    }

    let mut prototypes = document
        .definitions
        .values()
        .map(|definition| export_definition(definition, &prototype_ids))
        .collect::<ImportResult<Vec<_>>>()?;
    let root_layout = document
        .child_layouts
        .get(&document.root_layout)
        .ok_or_else(|| "document root layout is missing".to_owned())?;
    let mut used_instance_ids = BTreeSet::new();
    let root_instance = match root_layout.entries.as_slice() {
        [entry] => export_occurrence(
            document,
            &OccurrenceRef::root(entry.id),
            &prototype_ids,
            &mut used_instance_ids,
        )?,
        entries => {
            let wrapper_id =
                unique_legacy_id(None, "meshcraft-scene-root", &mut used_prototype_ids);
            prototypes.push(export_empty_definition(&wrapper_id, "Root"));
            let mut children = Vec::with_capacity(entries.len());
            for entry in entries {
                children.push(export_occurrence(
                    document,
                    &OccurrenceRef::root(entry.id),
                    &prototype_ids,
                    &mut used_instance_ids,
                )?);
            }
            let root_instance_id =
                unique_legacy_id(None, "meshcraft-scene-instance", &mut used_instance_ids);
            let mut root = Map::new();
            root.insert("id".to_owned(), Value::String(root_instance_id));
            root.insert("prototypeId".to_owned(), Value::String(wrapper_id));
            write_transform(
                &mut root,
                Affine3::IDENTITY,
                "instanceOrigin",
                ["instanceAxisU", "instanceAxisV", "instanceAxisW"],
            );
            root.insert("children".to_owned(), Value::Array(children));
            Value::Object(root)
        }
    };

    export_root(document, prototypes, root_instance)
}

fn export_root(
    document: &Document,
    prototypes: Vec<Value>,
    root_instance: Value,
) -> ImportResult<Value> {
    let settings = &document.model_settings;
    let mut root = Map::new();
    root.insert("version".to_owned(), Value::from(17));
    root.insert("prototypes".to_owned(), Value::Array(prototypes));
    root.insert("rootInstance".to_owned(), root_instance);
    root.insert(
        "modelUnit".to_owned(),
        serde_json::json!({ "name": settings.unit_name, "size": settings.unit_size }),
    );
    root.insert("gridSpacing".to_owned(), Value::from(settings.grid_size));
    root.insert(
        "circleSegments".to_owned(),
        Value::from(settings.circle_segments),
    );
    root.insert("snapEpsilon".to_owned(), Value::from(settings.snap_radius));

    let mut model_unit = Map::new();
    for (key, value) in &document.extensions {
        if let Some(field) = key.strip_prefix("k3d.modelUnit.") {
            model_unit.entry(field.to_owned()).or_insert(value.clone());
        } else if let Some(field) = key.strip_prefix("k3d.")
            && !matches!(
                field,
                "source_schema_version"
                    | "source_schema_version_present"
                    | "root_instance_id"
                    | "modelUnit"
            )
        {
            root.entry(field.to_owned()).or_insert(value.clone());
        } else if !key.starts_with("k3d.") {
            return Err(format!(
                "document extension {key:?} has no schema-17 field mapping"
            ));
        }
    }
    let mut model_unit_value = serde_json::json!({
        "name": settings.unit_name,
        "size": settings.unit_size,
    });
    if let Some(object) = model_unit_value.as_object_mut() {
        for (field, value) in model_unit {
            object.entry(field).or_insert(value);
        }
    }
    root.insert("modelUnit".to_owned(), model_unit_value);
    Ok(Value::Object(root))
}

fn export_empty_definition(id: &str, name: &str) -> Value {
    let mut object = Map::new();
    object.insert("id".to_owned(), Value::String(id.to_owned()));
    object.insert("name".to_owned(), Value::String(name.to_owned()));
    object.insert("kind".to_owned(), Value::String("MESH".to_owned()));
    write_transform(
        &mut object,
        Affine3::IDENTITY,
        "definitionOrigin",
        ["definitionAxisU", "definitionAxisV", "definitionAxisW"],
    );
    object.insert("prototypeVertices".to_owned(), Value::Array(Vec::new()));
    object.insert("segments".to_owned(), Value::Array(Vec::new()));
    object.insert("faces".to_owned(), Value::Array(Vec::new()));
    Value::Object(object)
}

fn export_definition(
    definition: &crate::document::Definition,
    prototype_ids: &BTreeMap<DefinitionId, String>,
) -> ImportResult<Value> {
    let mut object = Map::new();
    let prototype_id = prototype_ids
        .get(&definition.id)
        .ok_or_else(|| format!("prototype {} has no legacy ID", definition.id.0))?;
    object.insert("id".to_owned(), Value::String(prototype_id.clone()));
    object.insert("name".to_owned(), Value::String(definition.name.clone()));
    let kind = match definition.kind {
        DefinitionKind::Mesh => "MESH",
        DefinitionKind::Voxel => "VOXEL",
        DefinitionKind::Architecture => "ARCHITECTURE",
        DefinitionKind::Hybrid => "HYBRID",
        DefinitionKind::Other => definition
            .extensions
            .get("k3d.__raw_kind")
            .and_then(Value::as_str)
            .unwrap_or("OTHER"),
    };
    object.insert("kind".to_owned(), Value::String(kind.to_owned()));
    write_transform(
        &mut object,
        definition.frame,
        "definitionOrigin",
        ["definitionAxisU", "definitionAxisV", "definitionAxisW"],
    );
    export_domain_data(definition, &mut object)?;
    let mut used_vertices = BTreeSet::new();
    let mut used_segments = BTreeSet::new();
    let mut used_faces = BTreeSet::new();
    let mut segments = Vec::with_capacity(definition.geometry.segments.len());
    for segment in definition.geometry.segments.values() {
        let start = definition
            .geometry
            .vertex_position(segment.endpoints[0])
            .ok_or_else(|| format!("segment {} has an unresolved start vertex", segment.id.0))?;
        let end = definition
            .geometry
            .vertex_position(segment.endpoints[1])
            .ok_or_else(|| format!("segment {} has an unresolved end vertex", segment.id.0))?;
        let mut item = Map::new();
        let candidate = first_alias(&segment.legacy_aliases, "k3d.segment");
        item.insert(
            "id".to_owned(),
            Value::String(unique_legacy_id(
                candidate,
                &format!("meshcraft-segment-{}", segment.id.0),
                &mut used_segments,
            )),
        );
        item.insert("start".to_owned(), vec3_value(start));
        item.insert("end".to_owned(), vec3_value(end));
        merge_entity_extensions(&mut item, &segment.extensions)?;
        segments.push(Value::Object(item));
    }
    let mut faces = Vec::with_capacity(definition.geometry.triangles.len());
    for face in definition.geometry.triangles.values() {
        let points = face
            .vertices
            .map(|vertex| definition.geometry.vertex_position(vertex))
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("face {} has an unresolved vertex", face.id.0))?;
        let mut item = Map::new();
        let candidate = first_alias(&face.legacy_aliases, "k3d.face");
        item.insert(
            "id".to_owned(),
            Value::String(unique_legacy_id(
                candidate,
                &format!("meshcraft-face-{}", face.id.0),
                &mut used_faces,
            )),
        );
        item.insert("a".to_owned(), vec3_value(points[0]));
        item.insert("b".to_owned(), vec3_value(points[1]));
        item.insert("c".to_owned(), vec3_value(points[2]));
        item.insert(
            "color".to_owned(),
            legacy_color(face.color, &face.extensions),
        );
        merge_entity_extensions(&mut item, &face.extensions)?;
        faces.push(Value::Object(item));
    }
    object.insert("segments".to_owned(), Value::Array(segments));
    object.insert("faces".to_owned(), Value::Array(faces));
    let prototype_vertices = export_prototype_vertices(definition, &mut used_vertices)?;
    if !prototype_vertices.as_array().is_some_and(Vec::is_empty)
        || definition
            .extensions
            .contains_key("k3d.__raw_prototype_vertices")
    {
        object.insert("prototypeVertices".to_owned(), prototype_vertices);
    }
    merge_prefixed_fields(
        &mut object,
        &definition.extensions,
        &["__raw_kind", "__raw_prototype_vertices"],
    )?;
    Ok(Value::Object(object))
}

fn export_domain_data(
    definition: &crate::document::Definition,
    object: &mut Map<String, Value>,
) -> ImportResult<()> {
    for (field, value) in &definition.domain_data {
        let valid_shape = if *field == "voxelColor" {
            value.is_object() || value.is_null()
        } else if SCHEMA17_DOMAIN_ARRAY_FIELDS.contains(&field.as_str()) {
            value.is_array()
        } else {
            return Err(format!(
                "prototype {} domain field {field:?} has no schema-17 export adapter",
                definition.id.0
            ));
        };
        if !valid_shape {
            return Err(format!(
                "prototype {} domain field {field:?} has an invalid schema-17 JSON shape",
                definition.id.0
            ));
        }
        object.insert(field.clone(), value.clone());
    }
    Ok(())
}

fn export_prototype_vertices(
    definition: &crate::document::Definition,
    used: &mut BTreeSet<String>,
) -> ImportResult<Value> {
    let raw = definition
        .extensions
        .get("k3d.__raw_prototype_vertices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut objects = Vec::new();
    let mut existing = BTreeMap::<String, usize>::new();
    for item in raw {
        if let Some(id) = item
            .as_object()
            .and_then(|object| object.get("id"))
            .and_then(Value::as_str)
        {
            existing.insert(id.to_owned(), objects.len());
        }
        objects.push(item);
    }
    for vertex in definition.geometry.vertices.values() {
        let Some(candidate) = first_alias(&vertex.legacy_aliases, "k3d.vertex") else {
            continue;
        };
        let id = unique_legacy_id(
            Some(candidate),
            &format!("meshcraft-vertex-{}", vertex.id.0),
            used,
        );
        let QuantizedVertex(x, y, z) = quantize_vertex(vertex.position)?;
        let item = if let Some(index) = existing.get(&id).copied() {
            objects[index].as_object_mut().cloned().unwrap_or_default()
        } else {
            Map::new()
        };
        let mut item = item;
        item.insert("id".to_owned(), Value::String(id.clone()));
        item.insert("x".to_owned(), Value::from(x));
        item.insert("y".to_owned(), Value::from(y));
        item.insert("z".to_owned(), Value::from(z));
        if let Some(index) = existing.get(&id).copied() {
            objects[index] = Value::Object(item);
        } else {
            existing.insert(id, objects.len());
            objects.push(Value::Object(item));
        }
    }
    Ok(Value::Array(objects))
}

fn export_occurrence(
    document: &Document,
    occurrence: &OccurrenceRef,
    prototype_ids: &BTreeMap<DefinitionId, String>,
    used_instance_ids: &mut BTreeSet<String>,
) -> ImportResult<Value> {
    let resolved = document
        .resolve_occurrence(occurrence)
        .ok_or_else(|| format!("cannot export unresolved occurrence {:?}", occurrence.path))?;
    let edits = resolved.edits;
    let candidate = edits
        .and_then(|edits| edits.legacy_instance_id.as_deref())
        .or_else(|| first_alias(&resolved.entry.legacy_aliases, "k3d.instance"));
    let fallback = format!("meshcraft-instance-{}", resolved.entry.id.0);
    let id = unique_legacy_id(candidate, &fallback, used_instance_ids);
    let prototype_id = prototype_ids.get(&resolved.definition.id).ok_or_else(|| {
        format!(
            "definition {} has no exported prototype",
            resolved.definition.id.0
        )
    })?;
    let transform = edits
        .and_then(|edits| edits.transform)
        .unwrap_or(resolved.entry.transform);
    let mut object = Map::new();
    object.insert("id".to_owned(), Value::String(id));
    object.insert(
        "prototypeId".to_owned(),
        Value::String(prototype_id.clone()),
    );
    write_transform(
        &mut object,
        transform,
        "instanceOrigin",
        ["instanceAxisU", "instanceAxisV", "instanceAxisW"],
    );

    let layout_id = edits
        .and_then(|edits| edits.child_layout)
        .or(resolved.definition.default_child_layout);
    let mut children = Vec::new();
    if let Some(layout_id) = layout_id {
        let layout = document
            .child_layouts
            .get(&layout_id)
            .ok_or_else(|| format!("occurrence references missing layout {}", layout_id.0))?;
        if !layout.extensions.is_empty() {
            return Err(format!(
                "child layout {} has unmapped legacy extensions",
                layout_id.0
            ));
        }
        for entry in &layout.entries {
            children.push(export_occurrence(
                document,
                &occurrence.child(entry.id),
                prototype_ids,
                used_instance_ids,
            )?);
        }
    }
    object.insert("children".to_owned(), Value::Array(children));
    if let Some(edits) = edits {
        if !edits.hotspot_positions.is_empty()
            || edits.values.contains_key("k3d.__raw_hotspot_positions")
        {
            object.insert(
                "hotspotPositions".to_owned(),
                export_hotspot_positions(edits)?,
            );
        }
        if let Some(replacement) = &edits.geometry_replacement {
            let (segments, faces) = export_override_geometry(replacement)?;
            object.insert("overrideSegments".to_owned(), Value::Array(segments));
            object.insert("overrideFaces".to_owned(), Value::Array(faces));
        }
        merge_prefixed_fields(&mut object, &edits.values, &["__raw_hotspot_positions"])?;
        merge_prefixed_fields(&mut object, &edits.extensions, &[])?;
    }
    Ok(Value::Object(object))
}

fn export_hotspot_positions(edits: &InstanceEdits) -> ImportResult<Value> {
    let raw = edits
        .values
        .get("k3d.__raw_hotspot_positions")
        .and_then(Value::as_array);
    let mut output = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(raw) = raw {
        for item in raw {
            let Some(object) = item.as_object() else {
                return Err("preserved hotspot position is not an object".to_owned());
            };
            let Some(id) = object.get("id").and_then(Value::as_str) else {
                output.push(item.clone());
                continue;
            };
            let Some(position) = edits.hotspot_positions.get(id) else {
                continue;
            };
            let mut item = object.clone();
            item.insert("position".to_owned(), vec3_value(*position));
            output.push(Value::Object(item));
            seen.insert(id.to_owned());
        }
    }
    for (id, position) in &edits.hotspot_positions {
        if seen.insert(id.clone()) {
            output.push(serde_json::json!({"id": id, "position": vec3_value(*position)}));
        }
    }
    Ok(Value::Array(output))
}

fn export_override_geometry(
    geometry: &crate::document::GeometryBundle,
) -> ImportResult<(Vec<Value>, Vec<Value>)> {
    let mut used_segments = BTreeSet::new();
    let mut used_faces = BTreeSet::new();
    let mut segments = Vec::with_capacity(geometry.segments.len());
    for segment in geometry.segments.values() {
        let start = geometry
            .vertex_position(segment.endpoints[0])
            .ok_or_else(|| "override segment has an unresolved start vertex".to_owned())?;
        let end = geometry
            .vertex_position(segment.endpoints[1])
            .ok_or_else(|| "override segment has an unresolved end vertex".to_owned())?;
        let mut item = Map::new();
        let candidate = first_alias(&segment.legacy_aliases, "k3d.segment");
        item.insert(
            "id".to_owned(),
            Value::String(unique_legacy_id(
                candidate,
                &format!("meshcraft-override-segment-{}", segment.id.0),
                &mut used_segments,
            )),
        );
        item.insert("start".to_owned(), vec3_value(start));
        item.insert("end".to_owned(), vec3_value(end));
        merge_entity_extensions(&mut item, &segment.extensions)?;
        segments.push(Value::Object(item));
    }
    let mut faces = Vec::with_capacity(geometry.triangles.len());
    for face in geometry.triangles.values() {
        let points = face
            .vertices
            .map(|vertex| geometry.vertex_position(vertex))
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| "override face has an unresolved vertex".to_owned())?;
        let mut item = Map::new();
        let candidate = first_alias(&face.legacy_aliases, "k3d.face");
        item.insert(
            "id".to_owned(),
            Value::String(unique_legacy_id(
                candidate,
                &format!("meshcraft-override-face-{}", face.id.0),
                &mut used_faces,
            )),
        );
        item.insert("a".to_owned(), vec3_value(points[0]));
        item.insert("b".to_owned(), vec3_value(points[1]));
        item.insert("c".to_owned(), vec3_value(points[2]));
        item.insert(
            "color".to_owned(),
            legacy_color(face.color, &face.extensions),
        );
        merge_entity_extensions(&mut item, &face.extensions)?;
        faces.push(Value::Object(item));
    }
    Ok((segments, faces))
}

fn first_alias<'a>(aliases: &'a [LegacyAlias], source: &str) -> Option<&'a str> {
    aliases
        .iter()
        .find(|alias| alias.source == source && !alias.id.is_empty())
        .map(|alias| alias.id.as_str())
}

fn unique_legacy_id(
    candidate: Option<&str>,
    fallback: &str,
    used: &mut BTreeSet<String>,
) -> String {
    if let Some(candidate) = candidate
        && !candidate.is_empty()
        && used.insert(candidate.to_owned())
    {
        return candidate.to_owned();
    }
    if used.insert(fallback.to_owned()) {
        return fallback.to_owned();
    }
    let mut suffix = 2_u64;
    loop {
        let candidate = format!("{fallback}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

fn vec3_value(value: Vec3) -> Value {
    serde_json::json!({ "x": value.x, "y": value.y, "z": value.z })
}

fn write_transform(
    object: &mut Map<String, Value>,
    transform: Affine3,
    origin_field: &str,
    axes_fields: [&str; 3],
) {
    object.insert(origin_field.to_owned(), vec3_value(transform.translation));
    let axes = [
        Vec3::new(
            transform.linear[0][0],
            transform.linear[1][0],
            transform.linear[2][0],
        ),
        Vec3::new(
            transform.linear[0][1],
            transform.linear[1][1],
            transform.linear[2][1],
        ),
        Vec3::new(
            transform.linear[0][2],
            transform.linear[1][2],
            transform.linear[2][2],
        ),
    ];
    for (field, axis) in axes_fields.into_iter().zip(axes) {
        object.insert(field.to_owned(), vec3_value(axis));
    }
}

fn merge_prefixed_fields(
    object: &mut Map<String, Value>,
    extensions: &BTreeMap<String, Value>,
    internal_suffixes: &[&str],
) -> ImportResult<()> {
    for (key, value) in extensions {
        let Some(field) = key.strip_prefix("k3d.") else {
            return Err(format!("extension {key:?} has no schema-17 field mapping"));
        };
        if internal_suffixes.contains(&field)
            || field.starts_with("prototypeVertices.")
            || field.starts_with("hotspotPositions/")
        {
            continue;
        }
        object.entry(field.to_owned()).or_insert(value.clone());
    }
    Ok(())
}

fn merge_entity_extensions(
    object: &mut Map<String, Value>,
    extensions: &BTreeMap<String, Value>,
) -> ImportResult<()> {
    merge_prefixed_fields(object, extensions, &["__raw_color"])
}

fn legacy_color(color: Rgba8, extensions: &BTreeMap<String, Value>) -> Value {
    if let Some(raw) = extensions.get("k3d.__raw_color")
        && parse_color(Some(raw), "color").ok() == Some(color)
    {
        return raw.clone();
    }
    serde_json::json!({
        "r": f64::from(color.red) / 255.0,
        "g": f64::from(color.green) / 255.0,
        "b": f64::from(color.blue) / 255.0,
        "a": f64::from(color.alpha) / 255.0,
    })
}

fn preserve_raw_color(
    raw: Option<&Value>,
    _parsed: Rgba8,
    extensions: &mut BTreeMap<String, Value>,
) {
    if let Some(raw) = raw {
        extensions.insert("k3d.__raw_color".to_owned(), raw.clone());
    }
}

fn parse_model_settings(root: &Map<String, Value>) -> ImportResult<ModelSettings> {
    let mut settings = ModelSettings::default();
    if let Some(unit) = root.get("modelUnit") {
        let unit = unit
            .as_object()
            .ok_or_else(|| "modelUnit: expected an object".to_owned())?;
        if let Some(name) = read_string(unit, "name", "/modelUnit")? {
            settings.unit_name = name;
        }
        if let Some(size) = read_optional_number(unit, "size", "/modelUnit")? {
            settings.unit_size = size;
        }
    }
    if let Some(grid_size) = read_optional_number(root, "gridSpacing", "")? {
        settings.grid_size = grid_size;
    }
    if let Some(segments) = root.get("circleSegments") {
        let segments = segments
            .as_u64()
            .ok_or_else(|| "circleSegments: expected a positive integer".to_owned())?;
        settings.circle_segments = u32::try_from(segments)
            .map_err(|_| "circleSegments: integer is outside the supported range".to_owned())?;
    }
    if let Some(snap_radius) = read_optional_number(root, "snapEpsilon", "")? {
        settings.snap_radius = snap_radius;
    }
    settings
        .validate()
        .map_err(|error| format!("model settings: {error}"))?;
    Ok(settings)
}

fn read_optional_number(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> ImportResult<Option<f64>> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_f64()
        .ok_or_else(|| format!("{path}/{key}: expected a number"))?;
    if !value.is_finite() {
        return Err(format!("{path}/{key}: expected a finite number"));
    }
    Ok(Some(value))
}

fn preserve_unknown_fields(
    object: &Map<String, Value>,
    consumed: &[&str],
    destination: &mut BTreeMap<String, Value>,
    prefix: &str,
) {
    for (key, value) in object {
        if !consumed.contains(&key.as_str()) {
            destination.insert(format!("{prefix}{key}"), value.clone());
        }
    }
}

fn normalize_sparse_component_dtos(field: &str, value: &Value) -> Value {
    if field == "voxelColor" {
        return normalize_sparse_dto_object(
            value,
            &serde_json::json!({"r": 1, "g": 1, "b": 1, "a": 1}),
        );
    }
    let defaults = match field {
        "segments" | "overrideSegments" => serde_json::json!({
            "id": "",
            "start": {"x": 0, "y": 0, "z": 0},
            "end": {"x": 0, "y": 0, "z": 0}
        }),
        "faces" | "overrideFaces" => serde_json::json!({
            "id": "",
            "a": {"x": 0, "y": 0, "z": 0},
            "b": {"x": 0, "y": 0, "z": 0},
            "c": {"x": 0, "y": 0, "z": 0},
            "color": {"r": 1, "g": 1, "b": 1, "a": 1}
        }),
        "voxels" => serde_json::json!({
            "x": 0,
            "y": 0,
            "z": 0,
            "color": {"r": 1, "g": 1, "b": 1, "a": 1}
        }),
        "dimensions" | "overrideDimensions" => serde_json::json!({
            "start": {"x": 0, "y": 0, "z": 0},
            "end": {"x": 0, "y": 0, "z": 0},
            "offset": {"x": 0, "y": 0, "z": 0}
        }),
        "texts" | "overrideTexts" => serde_json::json!({
            "position": {"x": 0, "y": 0, "z": 0},
            "text": "",
            "size": 0.1,
            "normal": {"x": 0, "y": 0, "z": 0},
            "axisU": {"x": 0, "y": 0, "z": 0},
            "screenText": true,
            "kind": "BITMAP",
            "tracking": 0.1,
            "lineSpacing": 1.25,
            "glyphSourcePath": "embedded:alphabet"
        }),
        "hotspotPositions" => serde_json::json!({
            "id": "",
            "position": {"x": 0, "y": 0, "z": 0}
        }),
        "hotspotSegmentAttachments" | "hotspotTriangleAttachments" => serde_json::json!({
            "id": "",
            "refs": []
        }),
        "architectureWalls" => serde_json::json!({
            "id": "",
            "name": "",
            "start": {"x": 0, "y": 0, "z": 0},
            "end": {"x": 0, "y": 0, "z": 0},
            "thickness": 0.2,
            "height": 2.7,
            "inclinationDeg": 0,
            "exteriorColor": {"r": 0.93, "g": 0.93, "b": 0.93, "a": 1},
            "interiorColor": {"r": 0.84, "g": 0.84, "b": 0.84, "a": 1},
            "holes": []
        }),
        "architectureSlabs" => serde_json::json!({
            "id": "",
            "name": "",
            "min": {"x": 0, "y": 0, "z": 0},
            "max": {"x": 0, "y": 0, "z": 0},
            "axisU": {"x": 1, "y": 0, "z": 0},
            "axisV": {"x": 0, "y": 0, "z": 1},
            "normal": {"x": 0, "y": 1, "z": 0},
            "thickness": 0.2,
            "topColor": {"r": 0.93, "g": 0.93, "b": 0.93, "a": 1},
            "bottomColor": {"r": 0.84, "g": 0.84, "b": 0.84, "a": 1},
            "sideColor": {"r": 0.88, "g": 0.88, "b": 0.88, "a": 1},
            "holes": []
        }),
        "architectureStairs" => serde_json::json!({
            "id": "",
            "name": "",
            "min": {"x": 0, "y": 0, "z": 0},
            "max": {"x": 0, "y": 0, "z": 0},
            "contour": [],
            "walkingPath": [],
            "walkingStart": {"x": 0, "y": 0, "z": 0},
            "walkingEnd": {"x": 0, "y": 0, "z": 0},
            "height": 2.7,
            "stepCount": 14,
            "supportThickness": 0.2,
            "railLeft": true,
            "railRight": true,
            "treadColor": {"r": 0.93, "g": 0.93, "b": 0.93, "a": 1},
            "supportColor": {"r": 0.82, "g": 0.82, "b": 0.82, "a": 1}
        }),
        "architectureFrames" => serde_json::json!({
            "id": "",
            "name": "",
            "cornerA": {"x": 0, "y": 0, "z": 0},
            "cornerB": {"x": 0, "y": 0, "z": 0},
            "contour": [],
            "normal": {"x": 0, "y": 1, "z": 0},
            "depth": 0.12,
            "frameWidth": 0.06,
            "kind": "WINDOW",
            "color": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1},
            "glazingEnabled": false,
            "glazingColor": {"r": 0.72, "g": 0.84, "b": 0.95, "a": 0.4}
        }),
        "hvacPlumbingRuns" => serde_json::json!({
            "id": "",
            "name": "",
            "path": [],
            "diameter": 0.2,
            "sides": 16,
            "color": {"r": 0.7, "g": 0.82, "b": 0.95, "a": 1}
        }),
        "hvacVentilationDucts" => serde_json::json!({
            "id": "",
            "name": "",
            "start": {"x": 0, "y": 0, "z": 0},
            "end": {"x": 0, "y": 0, "z": 0},
            "binormalRef": {"x": 0, "y": 0, "z": 0},
            "autoJoinEnabled": false,
            "width": 0.5,
            "height": 0.25,
            "humpHalfSpan": 0.625,
            "humpClearance": 0.05,
            "color": {"r": 0.82, "g": 0.82, "b": 0.82, "a": 1}
        }),
        "hotspots" => serde_json::json!({
            "id": "",
            "name": "",
            "position": {"x": 0, "y": 0, "z": 0},
            "operation": "MOVE",
            "shape": "CIRCLE",
            "color": {"r": 0.2, "g": 0.55, "b": 0.95, "a": 1},
            "referencePosition": null,
            "attachedHotspotIds": [],
            "attachedVertexIds": [],
            "attachedSegments": [],
            "attachedTriangles": []
        }),
        _ => return value.clone(),
    };
    let mut dtos = normalize_sparse_dto_array(value, &defaults);
    if matches!(field, "architectureWalls" | "architectureSlabs")
        && let Some(dtos) = dtos.as_array_mut()
    {
        let hole_defaults = serde_json::json!({
            "id": "",
            "name": "",
            "u0": 0,
            "u1": 0,
            "v0": 0,
            "v1": 0
        });
        for dto in dtos {
            if let Some(holes) = dto.as_object_mut().and_then(|dto| dto.get_mut("holes")) {
                *holes = normalize_sparse_dto_array(holes, &hole_defaults);
            }
        }
    }
    if field == "hotspots"
        && let Some(dtos) = dtos.as_array_mut()
    {
        let segment_defaults = serde_json::json!({
            "id": "",
            "a": {"x": 0, "y": 0, "z": 0},
            "b": {"x": 0, "y": 0, "z": 0}
        });
        let triangle_defaults = serde_json::json!({
            "id": "",
            "a": {"x": 0, "y": 0, "z": 0},
            "b": {"x": 0, "y": 0, "z": 0},
            "c": {"x": 0, "y": 0, "z": 0}
        });
        for dto in dtos {
            if let Some(hotspot) = dto.as_object_mut() {
                if let Some(segments) = hotspot.get_mut("attachedSegments") {
                    *segments = normalize_sparse_dto_array(segments, &segment_defaults);
                }
                if let Some(triangles) = hotspot.get_mut("attachedTriangles") {
                    *triangles = normalize_sparse_dto_array(triangles, &triangle_defaults);
                }
            }
        }
    }
    let attachment_kind = match field {
        "hotspotSegmentAttachments" => Some("segment"),
        "hotspotTriangleAttachments" => Some("triangle"),
        _ => None,
    };
    if let Some(attachment_kind) = attachment_kind
        && let Some(dtos) = dtos.as_array_mut()
    {
        let ref_defaults = if attachment_kind == "segment" {
            serde_json::json!({
                "id": "",
                "a": {"x": 0, "y": 0, "z": 0},
                "b": {"x": 0, "y": 0, "z": 0}
            })
        } else {
            serde_json::json!({
                "id": "",
                "a": {"x": 0, "y": 0, "z": 0},
                "b": {"x": 0, "y": 0, "z": 0},
                "c": {"x": 0, "y": 0, "z": 0}
            })
        };
        for dto in dtos {
            if let Some(refs) = dto.as_object_mut().and_then(|dto| dto.get_mut("refs")) {
                *refs = normalize_sparse_dto_array(refs, &ref_defaults);
            }
        }
    }
    let vector_array_fields: &[&str] = match field {
        "architectureFrames" => &["contour"],
        "architectureStairs" => &["contour", "walkingPath"],
        "hvacPlumbingRuns" => &["path"],
        _ => &[],
    };
    if !vector_array_fields.is_empty()
        && let Some(dtos) = dtos.as_array_mut()
    {
        let vector_defaults = serde_json::json!({"x": 0, "y": 0, "z": 0});
        for dto in dtos {
            if let Some(dto) = dto.as_object_mut() {
                for field in vector_array_fields {
                    if let Some(values) = dto.get_mut(*field) {
                        *values = normalize_sparse_dto_array(values, &vector_defaults);
                    }
                }
            }
        }
    }
    dtos
}

fn normalize_sparse_dto_array(value: &Value, defaults: &Value) -> Value {
    let Some(dtos) = value.as_array() else {
        return value.clone();
    };

    Value::Array(
        dtos.iter()
            .map(|dto| {
                let Some(dto) = dto.as_object() else {
                    return dto.clone();
                };
                let mut dto = dto.clone();
                apply_sparse_dto_defaults(&mut dto, defaults);
                Value::Object(dto)
            })
            .collect(),
    )
}

fn normalize_sparse_dto_object(value: &Value, defaults: &Value) -> Value {
    let Some(mut object) = value.as_object().cloned() else {
        return value.clone();
    };
    apply_sparse_dto_defaults(&mut object, defaults);
    Value::Object(object)
}

fn apply_sparse_dto_defaults(object: &mut Map<String, Value>, defaults: &Value) {
    apply_sparse_dto_map_defaults(object, defaults.as_object().expect("DTO defaults"));
}

fn apply_sparse_dto_map_defaults(object: &mut Map<String, Value>, defaults: &Map<String, Value>) {
    for (field, default) in defaults {
        match object.entry(field.clone()) {
            serde_json::map::Entry::Vacant(entry) => {
                entry.insert(default.clone());
            }
            serde_json::map::Entry::Occupied(mut entry) => {
                if let (Some(value), Some(defaults)) =
                    (entry.get_mut().as_object_mut(), default.as_object())
                {
                    apply_sparse_dto_map_defaults(value, defaults);
                }
            }
        }
    }
}

fn parse_definition_kind(object: &Map<String, Value>, path: &str) -> ImportResult<DefinitionKind> {
    let kind = read_string(object, "kind", path)?.unwrap_or_else(|| "MESH".to_owned());
    Ok(match kind.to_ascii_uppercase().as_str() {
        "MESH" => DefinitionKind::Mesh,
        "VOXEL" => DefinitionKind::Voxel,
        "ARCHITECTURE" => DefinitionKind::Architecture,
        "HYBRID" => DefinitionKind::Hybrid,
        _ => DefinitionKind::Other,
    })
}

fn parse_prototype_geometry(
    object: &Map<String, Value>,
    path: &str,
) -> ImportResult<GeometryDraft> {
    let aliases = parse_vertex_aliases(
        object.get("prototypeVertices"),
        &format!("{path}/prototypeVertices"),
    )?;
    let mut geometry = GeometryDraft::new();
    let mut reusable = HashMap::new();
    let normalized_segments = object
        .get("segments")
        .map(|value| normalize_sparse_component_dtos("segments", value));
    let segment_values = read_array(normalized_segments.as_ref(), &format!("{path}/segments"))?;
    for (index, value) in segment_values.iter().enumerate() {
        let segment_path = format!("{path}/segments/{index}");
        let segment = value
            .as_object()
            .ok_or_else(|| format!("{segment_path}: segment must be an object"))?;
        let start = read_vec3(segment.get("start"), &format!("{segment_path}/start"))?;
        let end = read_vec3(segment.get("end"), &format!("{segment_path}/end"))?;
        let start_index = geometry.add_vertex(
            start,
            vertex_alias(start, &aliases),
            &mut reusable,
            quantized_alias_key(start, &aliases)?,
        );
        let end_index = geometry.add_vertex(
            end,
            vertex_alias(end, &aliases),
            &mut reusable,
            quantized_alias_key(end, &aliases)?,
        );
        geometry.segments.push(RawGeometrySegment {
            endpoints: [start_index, end_index],
            color: Rgba8::WHITE,
            legacy_aliases: entity_alias(segment, "k3d.segment")?,
            extensions: entity_extensions(segment, &["id", "start", "end"]),
        });
    }

    let normalized_faces = object
        .get("faces")
        .map(|value| normalize_sparse_component_dtos("faces", value));
    let face_values = read_array(normalized_faces.as_ref(), &format!("{path}/faces"))?;
    for (index, value) in face_values.iter().enumerate() {
        let face_path = format!("{path}/faces/{index}");
        let face = value
            .as_object()
            .ok_or_else(|| format!("{face_path}: face must be an object"))?;
        let points = [
            read_vec3(face.get("a"), &format!("{face_path}/a"))?,
            read_vec3(face.get("b"), &format!("{face_path}/b"))?,
            read_vec3(face.get("c"), &format!("{face_path}/c"))?,
        ];
        let mut indices = [RawVertexRef {
            vertex: 0,
            position_offset: Vec3::ZERO,
        }; 3];
        for (corner, point) in points.into_iter().enumerate() {
            indices[corner] = geometry.add_vertex(
                point,
                vertex_alias(point, &aliases),
                &mut reusable,
                quantized_alias_key(point, &aliases)?,
            );
        }
        let color = parse_color(face.get("color"), &format!("{face_path}/color"))?;
        let mut extensions = entity_extensions(face, &["id", "a", "b", "c", "color"]);
        preserve_raw_color(face.get("color"), color, &mut extensions);
        geometry.triangles.push(RawGeometryTriangle {
            vertices: indices,
            color,
            legacy_aliases: entity_alias(face, "k3d.face")?,
            extensions,
        });
    }
    Ok(geometry)
}

fn parse_instance(
    value: &Value,
    path: &str,
    definitions: &HashMap<String, DefinitionId>,
    depth: usize,
    count: &mut usize,
) -> ImportResult<LegacyInstance> {
    if depth > MAX_INSTANCE_DEPTH {
        return Err(format!(
            "{path}: hierarchy exceeds {MAX_INSTANCE_DEPTH} levels"
        ));
    }
    *count += 1;
    if *count > MAX_INSTANCE_COUNT {
        return Err(format!("hierarchy exceeds {MAX_INSTANCE_COUNT} instances"));
    }
    let object = value
        .as_object()
        .ok_or_else(|| format!("{path}: instance must be an object"))?;
    let id = read_string(object, "id", path)?.unwrap_or_default();
    let prototype_id = read_string(object, "prototypeId", path)?.unwrap_or_default();
    if !definitions.contains_key(&prototype_id) {
        return Err(format!(
            "{path}/prototypeId: unknown prototype {prototype_id:?}"
        ));
    }
    let transform = parse_transform(
        object,
        "instanceOrigin",
        ["instanceAxisU", "instanceAxisV", "instanceAxisW"],
        path,
    )?;
    validate_transform(transform, &format!("{path}/instanceOrigin"))?;

    let mut children = Vec::new();
    for (index, child) in read_array(object.get("children"), &format!("{path}/children"))?
        .iter()
        .enumerate()
    {
        children.push(parse_instance(
            child,
            &format!("{path}/children/{index}"),
            definitions,
            depth + 1,
            count,
        )?);
    }
    let mut hotspot_positions = BTreeMap::new();
    let mut preserved_fields = BTreeMap::new();
    for (index, position_value) in read_array(
        object.get("hotspotPositions"),
        &format!("{path}/hotspotPositions"),
    )?
    .iter()
    .enumerate()
    {
        let position_path = format!("{path}/hotspotPositions/{index}");
        let position_object = position_value
            .as_object()
            .ok_or_else(|| format!("{position_path}: hotspot position must be an object"))?;
        preserve_unknown_fields(
            position_object,
            &["id", "position"],
            &mut preserved_fields,
            &format!("k3d.hotspotPositions/{index}/"),
        );
        let hotspot_id = read_string(position_object, "id", &position_path)?.unwrap_or_default();
        if !hotspot_id.is_empty() {
            hotspot_positions.insert(
                hotspot_id,
                read_vec3(
                    position_object.get("position"),
                    &format!("{position_path}/position"),
                )?,
            );
        }
    }
    if let Some(raw_positions) = object.get("hotspotPositions") {
        preserved_fields.insert(
            "k3d.__raw_hotspot_positions".to_owned(),
            normalize_sparse_component_dtos("hotspotPositions", raw_positions),
        );
    }

    let override_segments = parse_override_segments(object, path)?;
    let override_triangles = parse_override_faces(object, path)?;
    let has_geometry_overrides = ["overrideSegments", "overrideFaces"].iter().any(|key| {
        object
            .get(*key)
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    });
    let geometry_override = has_geometry_overrides.then(|| {
        let mut geometry = GeometryDraft::new();
        let mut reusable = HashMap::new();
        for segment in override_segments {
            let start = segment.0;
            let end = segment.1;
            let start_index = geometry.add_vertex(start, Vec::new(), &mut reusable, None);
            let end_index = geometry.add_vertex(end, Vec::new(), &mut reusable, None);
            geometry.segments.push(RawGeometrySegment {
                endpoints: [start_index, end_index],
                color: Rgba8::WHITE,
                legacy_aliases: segment.2,
                extensions: segment.3,
            });
        }
        for triangle in override_triangles {
            let points = triangle.0;
            let indices =
                points.map(|point| geometry.add_vertex(point, Vec::new(), &mut reusable, None));
            let mut extensions = triangle.3;
            preserve_raw_color(triangle.4.as_ref(), triangle.1, &mut extensions);
            geometry.triangles.push(RawGeometryTriangle {
                vertices: indices,
                color: triangle.1,
                legacy_aliases: triangle.2,
                extensions,
            });
        }
        geometry
    });

    preserve_unknown_fields(
        object,
        &[
            "id",
            "prototypeId",
            "instanceOrigin",
            "instanceAxisU",
            "instanceAxisV",
            "instanceAxisW",
            "children",
            "hotspotPositions",
            "overrideSegments",
            "overrideFaces",
        ],
        &mut preserved_fields,
        "k3d.",
    );
    for field in [
        "hotspotSegmentAttachments",
        "hotspotTriangleAttachments",
        "overrideDimensions",
        "overrideTexts",
    ] {
        if let Some(value) = object.get(field) {
            preserved_fields.insert(
                format!("k3d.{field}"),
                normalize_sparse_component_dtos(field, value),
            );
        }
    }
    let mut instance = LegacyInstance {
        id,
        prototype_id,
        transform,
        children,
        hotspot_positions,
        geometry_override,
        preserved_fields,
        layout_key: LayoutKey {
            definition_id: String::new(),
            children: Vec::new(),
        },
    };
    instance.layout_key = layout_key(&instance);
    Ok(instance)
}

fn parse_override_segments(
    object: &Map<String, Value>,
    path: &str,
) -> ImportResult<Vec<ParsedOverrideSegment>> {
    let mut output = Vec::new();
    let normalized = object
        .get("overrideSegments")
        .map(|value| normalize_sparse_component_dtos("overrideSegments", value));
    for (index, value) in read_array(normalized.as_ref(), &format!("{path}/overrideSegments"))?
        .iter()
        .enumerate()
    {
        let segment_path = format!("{path}/overrideSegments/{index}");
        let segment = value
            .as_object()
            .ok_or_else(|| format!("{segment_path}: segment must be an object"))?;
        output.push((
            read_vec3(segment.get("start"), &format!("{segment_path}/start"))?,
            read_vec3(segment.get("end"), &format!("{segment_path}/end"))?,
            entity_alias(segment, "k3d.segment")?,
            entity_extensions(segment, &["id", "start", "end"]),
        ));
    }
    Ok(output)
}

fn parse_override_faces(
    object: &Map<String, Value>,
    path: &str,
) -> ImportResult<Vec<ParsedOverrideFace>> {
    let mut output = Vec::new();
    let normalized = object
        .get("overrideFaces")
        .map(|value| normalize_sparse_component_dtos("overrideFaces", value));
    for (index, value) in read_array(normalized.as_ref(), &format!("{path}/overrideFaces"))?
        .iter()
        .enumerate()
    {
        let face_path = format!("{path}/overrideFaces/{index}");
        let face = value
            .as_object()
            .ok_or_else(|| format!("{face_path}: face must be an object"))?;
        let color = parse_color(face.get("color"), &format!("{face_path}/color"))?;
        output.push((
            [
                read_vec3(face.get("a"), &format!("{face_path}/a"))?,
                read_vec3(face.get("b"), &format!("{face_path}/b"))?,
                read_vec3(face.get("c"), &format!("{face_path}/c"))?,
            ],
            color,
            entity_alias(face, "k3d.face")?,
            entity_extensions(face, &["id", "a", "b", "c", "color"]),
            face.get("color").cloned(),
        ));
    }
    Ok(output)
}

fn layout_key(instance: &LegacyInstance) -> LayoutKey {
    LayoutKey {
        definition_id: instance.prototype_id.clone(),
        children: instance
            .children
            .iter()
            .map(|child| {
                (
                    child.prototype_id.clone(),
                    transform_signature(child.transform),
                )
            })
            .collect(),
    }
}

fn transform_signature(transform: Affine3) -> [u64; 12] {
    let mut signature = [0; 12];
    let mut index = 0;
    for value in transform.linear.into_iter().flatten() {
        signature[index] = canonical_float_bits(value);
        index += 1;
    }
    for value in [
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
    ] {
        signature[index] = canonical_float_bits(value);
        index += 1;
    }
    signature
}

fn canonical_float_bits(value: f64) -> u64 {
    if value == 0.0 { 0 } else { value.to_bits() }
}

fn collect_layout_groups<'a>(
    instance: &'a LegacyInstance,
    groups: &mut BTreeMap<LayoutKey, Vec<&'a LegacyInstance>>,
) {
    groups
        .entry(instance.layout_key.clone())
        .or_default()
        .push(instance);
    for child in &instance.children {
        collect_layout_groups(child, groups);
    }
}

fn collect_default_layouts(
    instance: &LegacyInstance,
    variants: &BTreeMap<LayoutKey, LayoutVariant>,
    defaults: &mut BTreeMap<String, ChildLayoutId>,
) -> ImportResult<()> {
    if !defaults.contains_key(&instance.prototype_id) {
        let variant = variants
            .get(&instance.layout_key)
            .ok_or_else(|| format!("no child-layout variant for instance {}", instance.id))?;
        defaults.insert(instance.prototype_id.clone(), variant.id);
    }
    for child in &instance.children {
        collect_default_layouts(child, variants, defaults)?;
    }
    Ok(())
}

fn apply_instance_edits(
    document: &mut Document,
    instance: &LegacyInstance,
    occurrence: &OccurrenceRef,
    definitions: &HashMap<String, DefinitionId>,
    default_layouts: &BTreeMap<String, ChildLayoutId>,
    variants: &BTreeMap<LayoutKey, LayoutVariant>,
) -> ImportResult<()> {
    let variant = variants
        .get(&instance.layout_key)
        .ok_or_else(|| format!("no child-layout variant for instance {}", instance.id))?;
    let default_layout = default_layouts
        .get(&instance.prototype_id)
        .copied()
        .ok_or_else(|| {
            format!(
                "no default child layout for prototype {}",
                instance.prototype_id
            )
        })?;
    let mut edits = InstanceEdits::default();
    if !instance.id.is_empty() {
        edits.legacy_instance_id = Some(instance.id.clone());
    }
    if variant.id != default_layout {
        edits.child_layout = Some(variant.id);
    }
    edits.hotspot_positions = instance.hotspot_positions.clone();
    edits.values = instance.preserved_fields.clone();
    if let Some(geometry) = &instance.geometry_override {
        let (vertices, segments, triangles) = geometry.clone().into_parts();
        edits.geometry_replacement =
            Some(document.build_geometry_bundle_raw(vertices, segments, triangles)?);
    }
    if !document.set_occurrence_edits(occurrence.clone(), edits) {
        return Err(format!(
            "could not apply authored overrides to instance {}",
            instance.id
        ));
    }

    // Validate the definition reference here so an incomplete map cannot reach a later traversal.
    if !definitions.contains_key(&instance.prototype_id) {
        return Err(format!("unknown prototype {}", instance.prototype_id));
    }
    Ok(())
}

fn attach_child_edits(
    document: &mut Document,
    instance: &LegacyInstance,
    occurrence: &OccurrenceRef,
    definitions: &HashMap<String, DefinitionId>,
    default_layouts: &BTreeMap<String, ChildLayoutId>,
    variants: &BTreeMap<LayoutKey, LayoutVariant>,
) -> ImportResult<()> {
    let variant = variants
        .get(&instance.layout_key)
        .ok_or_else(|| format!("no child-layout variant for instance {}", instance.id))?;
    for (index, child) in instance.children.iter().enumerate() {
        let entry = *variant
            .entries
            .get(index)
            .ok_or_else(|| format!("missing shared child entry for instance {}", child.id))?;
        let child_occurrence = occurrence.child(entry);
        apply_instance_edits(
            document,
            child,
            &child_occurrence,
            definitions,
            default_layouts,
            variants,
        )?;
        attach_child_edits(
            document,
            child,
            &child_occurrence,
            definitions,
            default_layouts,
            variants,
        )?;
    }
    Ok(())
}

fn parse_transform(
    object: &Map<String, Value>,
    origin_field: &str,
    axes_fields: [&str; 3],
    path: &str,
) -> ImportResult<Affine3> {
    let origin = read_vec3(object.get(origin_field), &format!("{path}/{origin_field}"))?;
    let u = read_vec3(
        object.get(axes_fields[0]),
        &format!("{path}/{}", axes_fields[0]),
    )?;
    let v = read_vec3(
        object.get(axes_fields[1]),
        &format!("{path}/{}", axes_fields[1]),
    )?;
    let w = read_vec3(
        object.get(axes_fields[2]),
        &format!("{path}/{}", axes_fields[2]),
    )?;
    Ok(Affine3::from_linear_translation(
        [[u.x, v.x, w.x], [u.y, v.y, w.y], [u.z, v.z, w.z]],
        origin,
    ))
}

fn validate_transform(transform: Affine3, path: &str) -> ImportResult<()> {
    if !transform.is_finite() {
        return Err(format!("{path}: transform contains a non-finite value"));
    }
    if transform.inverse(GeometryTolerance::default()).is_none() {
        return Err(format!(
            "{path}: transform is singular or numerically unstable"
        ));
    }
    Ok(())
}

fn parse_vertex_aliases(
    values: Option<&Value>,
    path: &str,
) -> ImportResult<HashMap<QuantizedVertex, String>> {
    let mut aliases = HashMap::new();
    for (index, value) in read_array(values, path)?.iter().enumerate() {
        let vertex_path = format!("{path}/{index}");
        let object = value
            .as_object()
            .ok_or_else(|| format!("{vertex_path}: prototype vertex must be an object"))?;
        let id = read_string(object, "id", &vertex_path)?.unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let key = QuantizedVertex(
            read_integer(object, "x", &vertex_path)?,
            read_integer(object, "y", &vertex_path)?,
            read_integer(object, "z", &vertex_path)?,
        );
        aliases.insert(key, id);
    }
    Ok(aliases)
}

fn quantize_vertex(point: Vec3) -> ImportResult<QuantizedVertex> {
    fn component(value: f64) -> ImportResult<i64> {
        let scaled = (value / LEGACY_VERTEX_KEY_EPSILON + 0.5).floor();
        if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
            return Err("vertex coordinate is outside legacy key range".to_owned());
        }
        Ok(scaled as i64)
    }
    Ok(QuantizedVertex(
        component(point.x)?,
        component(point.y)?,
        component(point.z)?,
    ))
}

fn quantized_alias_key(
    point: Vec3,
    aliases: &HashMap<QuantizedVertex, String>,
) -> ImportResult<Option<QuantizedVertex>> {
    let key = quantize_vertex(point)?;
    Ok(aliases.contains_key(&key).then_some(key))
}

fn vertex_alias(point: Vec3, aliases: &HashMap<QuantizedVertex, String>) -> Vec<LegacyAlias> {
    quantize_vertex(point)
        .ok()
        .and_then(|key| aliases.get(&key))
        .map(|id| {
            vec![LegacyAlias {
                source: "k3d.vertex".to_owned(),
                id: id.clone(),
            }]
        })
        .unwrap_or_default()
}

fn entity_alias(object: &Map<String, Value>, source: &str) -> ImportResult<Vec<LegacyAlias>> {
    Ok(read_string(object, "id", "entity")?
        .filter(|id| !id.is_empty())
        .map(|id| {
            vec![LegacyAlias {
                source: source.to_owned(),
                id,
            }]
        })
        .unwrap_or_default())
}

fn entity_extensions(object: &Map<String, Value>, consumed: &[&str]) -> BTreeMap<String, Value> {
    let mut extensions = BTreeMap::new();
    preserve_unknown_fields(object, consumed, &mut extensions, "k3d.");
    extensions
}

fn parse_color(value: Option<&Value>, path: &str) -> ImportResult<Rgba8> {
    let Some(value) = value else {
        return Ok(Rgba8::WHITE);
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("{path}: color must be an object"))?;
    fn channel(object: &Map<String, Value>, key: &str, path: &str) -> ImportResult<u8> {
        let value = object.get(key).map_or(Ok(1.0), |value| {
            value
                .as_f64()
                .ok_or_else(|| format!("{path}/{key}: color channel must be numeric"))
        })?;
        if !value.is_finite() {
            return Err(format!("{path}/{key}: color channel must be finite"));
        }
        Ok((value.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
    Ok(Rgba8::new(
        channel(object, "r", path)?,
        channel(object, "g", path)?,
        channel(object, "b", path)?,
        channel(object, "a", path)?,
    ))
}

fn read_array<'a>(value: Option<&'a Value>, path: &str) -> ImportResult<&'a [Value]> {
    match value {
        None => Ok(&[]),
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(format!("{path}: expected an array")),
    }
}

fn read_vec3(value: Option<&Value>, path: &str) -> ImportResult<Vec3> {
    let Some(value) = value else {
        // Kotlin DTO vector fields are initialized to the zero vector when omitted.
        return Ok(Vec3::ZERO);
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("{path}: vector must be an object"))?;
    fn component(object: &Map<String, Value>, key: &str, path: &str) -> ImportResult<f64> {
        match object.get(key) {
            None => Ok(0.0),
            Some(value) => value
                .as_f64()
                .ok_or_else(|| format!("{path}/{key}: vector coordinate must be numeric")),
        }
    }
    let result = Vec3::new(
        component(object, "x", path)?,
        component(object, "y", path)?,
        component(object, "z", path)?,
    );
    if !result.is_finite() {
        return Err(format!("{path}: vector contains a non-finite coordinate"));
    }
    Ok(result)
}

fn read_integer(object: &Map<String, Value>, key: &str, path: &str) -> ImportResult<i64> {
    match object.get(key) {
        None => Ok(0),
        Some(value) => value
            .as_i64()
            .ok_or_else(|| format!("{path}/{key}: expected an integer")),
    }
}

fn read_string(object: &Map<String, Value>, key: &str, path: &str) -> ImportResult<Option<String>> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{path}/{key}: expected a string")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GeometryDraft, QuantizedVertex, export_current_value, import_current_json,
        import_current_value, legacy_camera_state, snapshot_with_legacy_camera_state,
    };
    use crate::document::{DefinitionKind, Document, LegacyAlias, OccurrenceRef};
    use crate::geometry::Vec3;
    use serde_json::Value;
    use std::collections::{BTreeSet, HashMap};

    const ARCH_FRAME: &str =
        include_str!("../../../fixtures/legacy/samples/examples/arch_frame.octd");
    const HOTSPOT_SCENE: &str =
        include_str!("../../../fixtures/legacy/samples/examples/hotspots.demo.2.k3d");
    const ARCHITECTURE_WITH_MESH: &str =
        include_str!("../../../fixtures/legacy/samples/examples/test-shortcuts.0.octd");
    const ALPHABET_GLYPHS: &str =
        include_str!("../../../fixtures/legacy/samples/assets/glyphs/alphabet.octd");
    const DIFFERING_INSTANCE_OVERRIDE: &str = include_str!(
        "../../../fixtures/scenarios/object-model/differing-instance-override-input.octd"
    );

    #[test]
    fn camera_persistence_snapshot_updates_pose_and_retains_unknown_camera_fields() {
        let mut source: Value = serde_json::from_str(ARCH_FRAME).unwrap();
        source["cameraState"] = serde_json::json!({
            "position": {"x": 2.0, "y": 3.0, "z": 4.0},
            "target": {"x": 0.0, "y": 0.0, "z": 0.0},
            "vendorExtension": {"lens": "wide"}
        });
        let document = import_current_value(&source).unwrap();
        let original_revision = document.revision();
        let replacement = serde_json::json!({
            "position": {"x": 8.0, "y": 5.0, "z": -3.0},
            "target": {"x": 1.0, "y": 2.0, "z": 3.0},
            "fieldOfView": 51.0
        });

        let snapshot = snapshot_with_legacy_camera_state(&document, replacement).unwrap();
        let exported = export_current_value(&snapshot).unwrap();

        assert_eq!(original_revision, document.revision());
        assert_eq!(
            serde_json::json!({"lens": "wide"}),
            exported["cameraState"]["vendorExtension"]
        );
        assert_eq!(8.0, exported["cameraState"]["position"]["x"]);
        assert_eq!(51.0, exported["cameraState"]["fieldOfView"]);
        assert_eq!(
            3.0,
            legacy_camera_state(&document).unwrap()["position"]["y"]
        );
    }

    #[test]
    fn exports_schema17_snapshot_and_round_trips_frames_geometry_and_instance_ids() {
        let original = import_current_json(ARCH_FRAME).unwrap();
        let exported = export_current_value(&original).unwrap();
        assert_eq!(17, exported["version"]);
        assert_eq!(
            original.extensions["k3d.root_instance_id"],
            exported["rootInstance"]["id"]
        );
        let reopened = import_current_value(&exported).unwrap();
        let original_occurrences = original.occurrence_refs().unwrap();
        let reopened_occurrences = reopened.occurrence_refs().unwrap();
        assert_eq!(original_occurrences.len(), reopened_occurrences.len());
        assert_eq!(original.definitions.len(), reopened.definitions.len());
        for (before, after) in original_occurrences.iter().zip(&reopened_occurrences) {
            let before_resolved = original.resolve_occurrence(before).unwrap();
            let after_resolved = reopened.resolve_occurrence(after).unwrap();
            assert_eq!(
                before_resolved.definition.name,
                after_resolved.definition.name
            );
            assert_eq!(
                before_resolved.definition.kind,
                after_resolved.definition.kind
            );
            assert_eq!(
                before_resolved.definition.frame,
                after_resolved.definition.frame
            );
            assert_eq!(
                before_resolved.world_transform,
                after_resolved.world_transform
            );
            assert_eq!(
                before_resolved
                    .edits
                    .and_then(|edits| edits.legacy_instance_id.as_deref()),
                after_resolved
                    .edits
                    .and_then(|edits| edits.legacy_instance_id.as_deref())
            );
            let before_geometry = original.effective_geometry(before).unwrap();
            let after_geometry = reopened.effective_geometry(after).unwrap();
            assert_eq!(
                before_geometry.segments.len(),
                after_geometry.segments.len()
            );
            assert_eq!(
                before_geometry.triangles.len(),
                after_geometry.triangles.len()
            );
            assert_eq!(
                before_geometry
                    .segments
                    .values()
                    .map(|segment| {
                        segment
                            .endpoints
                            .map(|endpoint| before_geometry.vertex_position(endpoint).unwrap())
                    })
                    .collect::<Vec<_>>(),
                after_geometry
                    .segments
                    .values()
                    .map(|segment| {
                        segment
                            .endpoints
                            .map(|endpoint| after_geometry.vertex_position(endpoint).unwrap())
                    })
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn sparse_schema17_component_dtos_receive_legacy_defaults() {
        let mut source: Value = serde_json::from_str(ARCH_FRAME).unwrap();
        source["prototypes"][0]["architectureFrames"] = serde_json::json!([
            {
                "id": "sparse-frame",
                "contour": [{"x": 2}],
                "vendorData": {"kept": true}
            },
            {"id": "explicit-frame", "depth": 0.75, "glazingEnabled": true}
        ]);
        source["prototypes"][0]["segments"] = serde_json::json!([{
            "id": "sparse-segment",
            "start": {"x": 4},
            "end": {"x": 5},
            "vendorSegment": true
        }]);
        source["prototypes"][0]["faces"] = serde_json::json!([{
            "id": "sparse-face",
            "a": {},
            "b": {"x": 1},
            "c": {"y": 1},
            "color": {"r": 0.4},
            "vendorFace": true
        }]);
        source["prototypes"][0]["prototypeVertices"] =
            serde_json::json!([{"id": "sparse-vertex", "vendorVertex": true}]);
        source["prototypes"][0]["architectureWalls"] = serde_json::json!([{
            "id": "wall",
            "exteriorColor": {"r": 0.25},
            "holes": [{"id": "hole"}]
        }]);
        source["prototypes"][0]["architectureSlabs"] = serde_json::json!([{"id": "slab"}]);
        source["prototypes"][0]["architectureStairs"] = serde_json::json!([{
            "id": "stair",
            "contour": [{"y": 2}],
            "walkingPath": [{"z": 3}]
        }]);
        source["prototypes"][0]["voxels"] = serde_json::json!([{"x": 3}]);
        source["prototypes"][0]["voxelColor"] = serde_json::json!({"r": 0.4});
        source["prototypes"][0]["hvacPlumbingRuns"] = serde_json::json!([{
            "id": "pipe",
            "path": [{"x": 4}]
        }]);
        source["prototypes"][0]["hvacVentilationDucts"] = serde_json::json!([{"id": "duct"}]);
        source["prototypes"][0]["hotspots"] = serde_json::json!([{
            "id": "hotspot",
            "attachedSegments": [{"id": "attached-segment", "a": {"x": 9}}],
            "attachedTriangles": [{"id": "attached-triangle", "c": {"z": 3}}]
        }]);
        source["prototypes"][0]["dimensions"] = serde_json::json!([{"start": {"x": 4}}]);
        source["prototypes"][0]["texts"] = serde_json::json!([{"text": "label"}]);
        source["rootInstance"]["hotspotPositions"] = serde_json::json!([{"id": "position"}]);
        source["rootInstance"]["hotspotSegmentAttachments"] = serde_json::json!([{
            "id": "segment-attachments",
            "refs": [{"id": "segment-ref"}]
        }]);
        source["rootInstance"]["hotspotTriangleAttachments"] = serde_json::json!([{
            "id": "triangle-attachments",
            "refs": [{"id": "triangle-ref"}]
        }]);
        source["rootInstance"]["overrideDimensions"] = serde_json::json!([{"start": {"x": 4}}]);
        source["rootInstance"]["overrideTexts"] = serde_json::json!([{"text": "instance label"}]);
        source["rootInstance"]["overrideSegments"] = serde_json::json!([{
            "id": "sparse-override-segment",
            "start": {"z": 6},
            "end": {"z": 7},
            "vendorOverrideSegment": true
        }]);
        source["rootInstance"]["overrideFaces"] = serde_json::json!([{
            "id": "sparse-override-face",
            "a": {"z": 1},
            "b": {"x": 1, "z": 1},
            "c": {"y": 1, "z": 1},
            "color": {"b": 0.4},
            "vendorOverrideFace": true
        }]);

        let imported = import_current_value(&source).unwrap();
        let exported = export_current_value(&imported).unwrap();
        let prototype = exported["prototypes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|prototype| {
                prototype["architectureFrames"]
                    .as_array()
                    .is_some_and(|frames| frames.iter().any(|frame| frame["id"] == "sparse-frame"))
            })
            .unwrap();
        let frames = prototype["architectureFrames"].as_array().unwrap();

        let segment = &prototype["segments"][0];
        assert_eq!(4.0, segment["start"]["x"].as_f64().unwrap());
        assert_eq!(0.0, segment["start"]["y"].as_f64().unwrap());
        assert_eq!(0.0, segment["end"]["z"].as_f64().unwrap());
        assert_eq!(true, segment["vendorSegment"]);

        let face = &prototype["faces"][0];
        assert_eq!(0.0, face["a"]["z"].as_f64().unwrap());
        assert_eq!(1.0, face["b"]["x"].as_f64().unwrap());
        assert_eq!(1.0, face["c"]["y"].as_f64().unwrap());
        assert_eq!(1.0, face["color"]["g"].as_f64().unwrap());
        assert_eq!(1.0, face["color"]["a"].as_f64().unwrap());
        assert_eq!(true, face["vendorFace"]);

        let root_instance = &exported["rootInstance"];
        let override_segment = &root_instance["overrideSegments"][0];
        assert_eq!(0.0, override_segment["start"]["x"].as_f64().unwrap());
        assert_eq!(6.0, override_segment["start"]["z"].as_f64().unwrap());
        assert_eq!(0.0, override_segment["end"]["x"].as_f64().unwrap());
        assert_eq!(true, override_segment["vendorOverrideSegment"]);

        let override_face = &root_instance["overrideFaces"][0];
        assert_eq!(0.0, override_face["a"]["x"].as_f64().unwrap());
        assert_eq!(1.0, override_face["b"]["x"].as_f64().unwrap());
        assert_eq!(1.0, override_face["c"]["y"].as_f64().unwrap());
        assert_eq!(1.0, override_face["color"]["r"].as_f64().unwrap());
        assert_eq!(1.0, override_face["color"]["a"].as_f64().unwrap());
        assert_eq!(true, override_face["vendorOverrideFace"]);

        assert_eq!(
            serde_json::json!({
                "id": "sparse-frame",
                "name": "",
                "cornerA": {"x": 0, "y": 0, "z": 0},
                "cornerB": {"x": 0, "y": 0, "z": 0},
                "contour": [{"x": 2, "y": 0, "z": 0}],
                "normal": {"x": 0, "y": 1, "z": 0},
                "depth": 0.12,
                "frameWidth": 0.06,
                "kind": "WINDOW",
                "color": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1},
                "glazingEnabled": false,
                "glazingColor": {"r": 0.72, "g": 0.84, "b": 0.95, "a": 0.4},
                "vendorData": {"kept": true}
            }),
            frames[0]
        );
        assert_eq!(0.75, frames[1]["depth"]);
        assert_eq!(true, frames[1]["glazingEnabled"]);

        let wall = &prototype["architectureWalls"][0];
        assert_eq!(0.2, wall["thickness"]);
        assert_eq!(2.7, wall["height"]);
        assert_eq!(0.25, wall["exteriorColor"]["r"]);
        assert_eq!(0.93, wall["exteriorColor"]["g"]);
        assert_eq!(1, wall["exteriorColor"]["a"]);
        assert_eq!(0, wall["holes"][0]["u0"]);
        assert_eq!("", wall["holes"][0]["name"]);

        let slab = &prototype["architectureSlabs"][0];
        assert_eq!(0.2, slab["thickness"]);
        assert_eq!(1, slab["axisU"]["x"]);
        assert_eq!(1, slab["axisV"]["z"]);
        assert_eq!(0.93, slab["topColor"]["r"]);

        let stair = &prototype["architectureStairs"][0];
        assert_eq!(2.7, stair["height"]);
        assert_eq!(14, stair["stepCount"]);
        assert_eq!(0.2, stair["supportThickness"]);
        assert_eq!(true, stair["railLeft"]);
        assert_eq!(true, stair["railRight"]);
        assert_eq!(2, stair["contour"][0]["y"]);
        assert_eq!(0, stair["contour"][0]["x"]);
        assert_eq!(3, stair["walkingPath"][0]["z"]);
        assert_eq!(0, stair["walkingPath"][0]["y"]);

        assert_eq!(0.4, prototype["voxelColor"]["r"]);
        assert_eq!(1, prototype["voxelColor"]["g"]);
        assert_eq!(0, prototype["prototypeVertices"][0]["x"]);
        assert_eq!(0, prototype["prototypeVertices"][0]["y"]);
        assert_eq!(0, prototype["prototypeVertices"][0]["z"]);
        assert_eq!(true, prototype["prototypeVertices"][0]["vendorVertex"]);
        assert_eq!(3, prototype["voxels"][0]["x"]);
        assert_eq!(0, prototype["voxels"][0]["z"]);
        assert_eq!(1, prototype["voxels"][0]["color"]["a"]);

        let plumbing = &prototype["hvacPlumbingRuns"][0];
        assert_eq!(0.2, plumbing["diameter"]);
        assert_eq!(16, plumbing["sides"]);
        assert_eq!(0.7, plumbing["color"]["r"]);
        assert_eq!(4, plumbing["path"][0]["x"]);
        assert_eq!(0, plumbing["path"][0]["y"]);

        let ventilation = &prototype["hvacVentilationDucts"][0];
        assert_eq!(false, ventilation["autoJoinEnabled"]);
        assert_eq!(0.5, ventilation["width"]);
        assert_eq!(0.25, ventilation["height"]);
        assert_eq!(0.625, ventilation["humpHalfSpan"]);

        let hotspot = &prototype["hotspots"][0];
        assert_eq!("MOVE", hotspot["operation"]);
        assert_eq!("CIRCLE", hotspot["shape"]);
        assert_eq!(0.2, hotspot["color"]["r"]);
        assert_eq!(Value::Null, hotspot["referencePosition"]);
        assert_eq!(9, hotspot["attachedSegments"][0]["a"]["x"]);
        assert_eq!(0, hotspot["attachedSegments"][0]["a"]["y"]);
        assert_eq!(0, hotspot["attachedSegments"][0]["a"]["z"]);
        assert_eq!(3, hotspot["attachedTriangles"][0]["c"]["z"]);
        assert_eq!(0, hotspot["attachedTriangles"][0]["c"]["x"]);

        let dimension = &prototype["dimensions"][0];
        assert_eq!(4, dimension["start"]["x"]);
        assert_eq!(0, dimension["start"]["y"]);
        assert_eq!(0, dimension["start"]["z"]);
        assert_eq!(0, dimension["end"]["y"]);
        assert_eq!(0, dimension["offset"]["z"]);

        let text = &prototype["texts"][0];
        assert_eq!("label", text["text"]);
        assert_eq!(0.1, text["size"]);
        assert_eq!(true, text["screenText"]);
        assert_eq!(0.1, text["tracking"]);
        assert_eq!(1.25, text["lineSpacing"]);
        assert_eq!("embedded:alphabet", text["glyphSourcePath"]);

        assert_eq!(0.0, root_instance["hotspotPositions"][0]["position"]["z"]);
        assert_eq!(
            0,
            root_instance["hotspotSegmentAttachments"][0]["refs"][0]["a"]["x"]
        );
        assert_eq!(
            0,
            root_instance["hotspotTriangleAttachments"][0]["refs"][0]["c"]["z"]
        );
        assert_eq!(0, root_instance["overrideDimensions"][0]["offset"]["x"]);
        assert_eq!(0.1, root_instance["overrideTexts"][0]["size"]);
    }

    #[test]
    fn exports_typed_domain_components_through_schema17_fields() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("domain", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let definition_id = document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        let expected = serde_json::json!({
            "voxelColor": {"r": 0.2, "g": 0.4, "b": 0.6, "a": 1.0},
            "voxels": [{
                "x": 1,
                "y": 2,
                "z": 3,
                "color": {"r": 1, "g": 1, "b": 1, "a": 1},
                "position": {"x": 1, "y": 2, "z": 3},
                "futureVoxel": true
            }],
            "architectureWalls": [{
                "id": "wall-1",
                "name": "",
                "start": {"x": 0, "y": 0, "z": 0},
                "end": {"x": 0, "y": 0, "z": 0},
                "thickness": 0.2,
                "height": 2.7,
                "inclinationDeg": 0,
                "exteriorColor": {"r": 0.93, "g": 0.93, "b": 0.93, "a": 1},
                "interiorColor": {"r": 0.84, "g": 0.84, "b": 0.84, "a": 1},
                "holes": [],
                "futureWall": "stone"
            }],
            "architectureSlabs": [{
                "id": "slab-1",
                "name": "",
                "min": {"x": 0, "y": 0, "z": 0},
                "max": {"x": 0, "y": 0, "z": 0},
                "axisU": {"x": 1, "y": 0, "z": 0},
                "axisV": {"x": 0, "y": 0, "z": 1},
                "normal": {"x": 0, "y": 1, "z": 0},
                "thickness": 0.3,
                "topColor": {"r": 0.93, "g": 0.93, "b": 0.93, "a": 1},
                "bottomColor": {"r": 0.84, "g": 0.84, "b": 0.84, "a": 1},
                "sideColor": {"r": 0.88, "g": 0.88, "b": 0.88, "a": 1},
                "holes": []
            }],
            "architectureStairs": [{
                "id": "stair-1",
                "name": "",
                "min": {"x": 0, "y": 0, "z": 0},
                "max": {"x": 0, "y": 0, "z": 0},
                "contour": [],
                "walkingPath": [],
                "walkingStart": {"x": 0, "y": 0, "z": 0},
                "walkingEnd": {"x": 0, "y": 0, "z": 0},
                "height": 2.7,
                "stepCount": 14,
                "supportThickness": 0.2,
                "railLeft": true,
                "railRight": true,
                "treadColor": {"r": 0.93, "g": 0.93, "b": 0.93, "a": 1},
                "supportColor": {"r": 0.82, "g": 0.82, "b": 0.82, "a": 1}
            }],
            "architectureFrames": [{
                "id": "frame-1",
                "name": "",
                "cornerA": {"x": 0, "y": 0, "z": 0},
                "cornerB": {"x": 0, "y": 0, "z": 0},
                "contour": [],
                "normal": {"x": 0, "y": 1, "z": 0},
                "depth": 0.12,
                "frameWidth": 0.06,
                "kind": "WINDOW",
                "color": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1},
                "glazingEnabled": false,
                "glazingColor": {"r": 0.72, "g": 0.84, "b": 0.95, "a": 0.4}
            }],
            "hvacPlumbingRuns": [{
                "id": "pipe-1",
                "name": "",
                "path": [],
                "diameter": 0.2,
                "sides": 16,
                "color": {"r": 0.7, "g": 0.82, "b": 0.95, "a": 1}
            }],
            "hvacVentilationDucts": [{
                "id": "duct-1",
                "name": "",
                "start": {"x": 0, "y": 0, "z": 0},
                "end": {"x": 0, "y": 0, "z": 0},
                "binormalRef": {"x": 0, "y": 0, "z": 0},
                "autoJoinEnabled": false,
                "width": 0.5,
                "height": 0.25,
                "humpHalfSpan": 0.625,
                "humpClearance": 0.05,
                "color": {"r": 0.82, "g": 0.82, "b": 0.82, "a": 1}
            }],
            "hotspots": [{
                "id": "hotspot-1",
                "name": "",
                "position": {"x": 0, "y": 0, "z": 0},
                "operation": "MOVE",
                "shape": "CIRCLE",
                "color": {"r": 0.2, "g": 0.55, "b": 0.95, "a": 1},
                "referencePosition": null,
                "attachedHotspotIds": [],
                "attachedVertexIds": [],
                "attachedSegments": [],
                "attachedTriangles": [],
                "futureHotspot": {"mode": "switch"}
            }]
        });
        let definition = document.definitions.get_mut(&definition_id).unwrap();
        for (field, value) in expected.as_object().unwrap() {
            definition.domain_data.insert(field.clone(), value.clone());
        }
        definition.extensions.insert(
            "k3d.architectureWalls".to_owned(),
            serde_json::json!([{"id": "stale-imported-wall"}]),
        );

        let exported = export_current_value(&document).unwrap();
        let prototype = exported["prototypes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|prototype| prototype["name"] == "domain")
            .unwrap();
        for (field, value) in expected.as_object().unwrap() {
            assert_eq!(value, &prototype[field], "{field}");
        }

        let reopened = import_current_value(&exported).unwrap();
        let reexported = export_current_value(&reopened).unwrap();
        let reopened_prototype = reexported["prototypes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|prototype| prototype["name"] == "domain")
            .unwrap();
        for (field, value) in expected.as_object().unwrap() {
            assert_eq!(value, &reopened_prototype[field], "{field}");
        }
    }

    #[test]
    fn rejects_unknown_or_malformed_typed_domain_fields() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("domain", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let definition_id = document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        document
            .definitions
            .get_mut(&definition_id)
            .unwrap()
            .domain_data
            .insert(
                "future_component".to_owned(),
                serde_json::json!({"value": 1}),
            );

        assert!(
            export_current_value(&document)
                .unwrap_err()
                .contains("no schema-17 export adapter")
        );

        let definition = document.definitions.get_mut(&definition_id).unwrap();
        definition.domain_data.clear();
        definition.domain_data.insert(
            "architectureWalls".to_owned(),
            serde_json::json!({"id": "wall-1"}),
        );

        assert!(
            export_current_value(&document)
                .unwrap_err()
                .contains("invalid schema-17 JSON shape")
        );
    }

    #[test]
    fn exports_multiple_roots_with_a_wrapper_and_remaps_copied_instance_ids() {
        let mut document = import_current_json(ARCH_FRAME).unwrap();
        let root = document.occurrence_refs().unwrap().remove(0);
        assert!(
            document
                .copy_root_occurrence(&root, Vec3::new(5.0, 0.0, 0.0))
                .is_some()
        );
        let exported = export_current_value(&document).unwrap();
        let children = exported["rootInstance"]["children"].as_array().unwrap();
        assert_eq!(2, children.len());
        assert_eq!(
            document.extensions["k3d.root_instance_id"],
            children[0]["id"]
        );

        let mut ids = BTreeSet::new();
        let mut pending = vec![&exported["rootInstance"]];
        while let Some(instance) = pending.pop() {
            let id = instance["id"].as_str().unwrap();
            assert!(
                ids.insert(id.to_owned()),
                "duplicate exported instance ID {id}"
            );
            pending.extend(instance["children"].as_array().unwrap());
        }
        let reopened = import_current_value(&exported).unwrap();
        assert_eq!(5, reopened.occurrence_refs().unwrap().len());
    }

    #[test]
    fn exports_hotspot_overrides_unknown_payloads_and_differing_geometry_overrides() {
        let mut source: Value = serde_json::from_str(HOTSPOT_SCENE).unwrap();
        source["futureRootPayload"] = serde_json::json!({"enabled": true, "revision": 3});
        source["prototypes"][0]["prototypeVertices"] = serde_json::json!([
            {"id":"retained-vertex", "x":0, "y":0, "z":0, "futureVertex":"kept"}
        ]);
        let original = import_current_value(&source).unwrap();
        let exported = export_current_value(&original).unwrap();
        assert_eq!(source["futureRootPayload"], exported["futureRootPayload"]);
        assert_eq!(
            source["prototypes"][0]["prototypeVertices"][0]["futureVertex"],
            exported["prototypes"][0]["prototypeVertices"][0]["futureVertex"]
        );
        let reopened = import_current_value(&exported).unwrap();
        let original_occurrences = original.occurrence_refs().unwrap();
        let reopened_occurrences = reopened.occurrence_refs().unwrap();
        assert_eq!(original_occurrences.len(), reopened_occurrences.len());
        for (before, after) in original_occurrences.iter().zip(&reopened_occurrences) {
            assert_eq!(
                original
                    .occurrence_edits(before)
                    .and_then(|edits| edits.legacy_instance_id.as_deref()),
                reopened
                    .occurrence_edits(after)
                    .and_then(|edits| edits.legacy_instance_id.as_deref())
            );
            assert_eq!(
                original
                    .occurrence_edits(before)
                    .map(|edits| &edits.hotspot_positions),
                reopened
                    .occurrence_edits(after)
                    .map(|edits| &edits.hotspot_positions)
            );
        }

        let source_with_override: Value =
            serde_json::from_str(DIFFERING_INSTANCE_OVERRIDE).unwrap();
        let overridden = import_current_value(&source_with_override).unwrap();
        let roundtrip = export_current_value(&overridden).unwrap();
        let reopened_override = import_current_value(&roundtrip).unwrap();
        let override_occurrence = overridden
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .find(|occurrence| {
                overridden
                    .occurrence_edits(occurrence)
                    .is_some_and(|edits| edits.geometry_replacement.is_some())
            })
            .unwrap();
        let reopened_occurrence = reopened_override
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .find(|occurrence| {
                reopened_override
                    .occurrence_edits(occurrence)
                    .is_some_and(|edits| edits.geometry_replacement.is_some())
            })
            .unwrap();
        assert_eq!(
            overridden
                .effective_geometry(&override_occurrence)
                .unwrap()
                .segments
                .len(),
            reopened_override
                .effective_geometry(&reopened_occurrence)
                .unwrap()
                .segments
                .len()
        );
        assert_eq!(17, roundtrip["version"]);
    }

    #[test]
    fn imports_prototype_geometry_and_child_instances_without_welding_unaliased_points() {
        let document = import_current_json(ARCH_FRAME).unwrap();
        document.validate().unwrap();
        let occurrences = document.occurrence_refs().unwrap();
        assert_eq!(2, occurrences.len());
        let root = document.resolve_occurrence(&occurrences[0]).unwrap();
        assert_eq!("Root", root.definition.name);
        assert_eq!(DefinitionKind::Mesh, root.definition.kind);
        let child = document.resolve_occurrence(&occurrences[1]).unwrap();
        assert_eq!("arch_frame", child.definition.name);
        assert_eq!(12, child.definition.geometry.segments.len());
        assert_eq!(8, child.definition.geometry.vertices.len());
        assert!(child.definition.geometry.vertices.values().all(|vertex| {
            vertex
                .legacy_aliases
                .iter()
                .any(|alias| alias.source == "k3d.vertex")
        }));
    }

    #[test]
    fn shares_equivalent_child_layouts_and_preserves_per_instance_inputs() {
        let document = import_current_json(HOTSPOT_SCENE).unwrap();
        document.validate().unwrap();
        let occurrences = document.occurrence_refs().unwrap();
        assert_eq!(6, occurrences.len());
        assert_eq!(3, document.definitions.len());
        let root = document.resolve_occurrence(&occurrences[0]).unwrap();
        let first_child = document.resolve_occurrence(&occurrences[1]).unwrap();
        let root_layout = root.definition.default_child_layout.unwrap();
        let first_child_layout = first_child.definition.default_child_layout.unwrap();
        assert_ne!(root_layout, first_child_layout);
        let shared_prototype = first_child.definition.id;
        assert!(occurrences[1..4].iter().all(|occurrence| {
            document
                .resolve_occurrence(occurrence)
                .unwrap()
                .definition
                .id
                == shared_prototype
        }));
        let hotspot_positions: Vec<_> = occurrences[1..4]
            .iter()
            .map(|occurrence| {
                *document.occurrence_edits[occurrence]
                    .hotspot_positions
                    .values()
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(
            [
                Vec3::new(6.0, 0.0, 2.0),
                Vec3::new(3.0, 0.0, 2.0),
                Vec3::new(7.0, 0.0, 2.0),
            ],
            hotspot_positions.as_slice()
        );
        assert!(!document.extensions.contains_key("k3d.undoHistory"));
    }

    #[test]
    fn imports_model_settings_and_preserves_unknown_unit_fields() {
        let mut snapshot: Value = serde_json::from_str(ARCH_FRAME).unwrap();
        snapshot["modelUnit"] = serde_json::json!({
            "name": "meter",
            "size": 0.01,
            "futureUnitHint": "kept"
        });
        snapshot["gridSpacing"] = serde_json::json!(2.5);
        snapshot["circleSegments"] = serde_json::json!(48);
        snapshot["snapEpsilon"] = serde_json::json!(8.0);
        let document = import_current_value(&snapshot).unwrap();
        assert_eq!("meter", document.model_settings.unit_name);
        assert_eq!(0.01, document.model_settings.unit_size);
        assert_eq!(2.5, document.model_settings.grid_size);
        assert_eq!(48, document.model_settings.circle_segments);
        assert_eq!(8.0, document.model_settings.snap_radius);
        assert_eq!(
            &serde_json::json!("kept"),
            &document.extensions["k3d.modelUnit.futureUnitHint"]
        );
    }

    #[test]
    fn rejects_unresolved_or_singular_legacy_documents_without_partial_results() {
        let unresolved = r#"{"prototypes":[{"id":"root","name":"Root","definitionAxisU":{"x":1},"definitionAxisV":{"y":1},"definitionAxisW":{"z":1}}],"rootInstance":{"id":"root","prototypeId":"missing","instanceAxisU":{"x":1},"instanceAxisV":{"y":1},"instanceAxisW":{"z":1}}}"#;
        assert!(
            import_current_json(unresolved)
                .unwrap_err()
                .contains("unknown prototype")
        );

        let singular = r#"{"prototypes":[{"id":"root","name":"Root","definitionAxisU":{},"definitionAxisV":{"y":1},"definitionAxisW":{"z":1}}],"rootInstance":{"id":"root","prototypeId":"root","instanceAxisU":{"x":1},"instanceAxisV":{"y":1},"instanceAxisW":{"z":1}}}"#;
        assert!(
            import_current_json(singular)
                .unwrap_err()
                .contains("singular")
        );
    }

    #[test]
    fn occurrence_paths_keep_shared_children_distinct() {
        let document = import_current_json(HOTSPOT_SCENE).unwrap();
        let occurrences = document.occurrence_refs().unwrap();
        let first_child = &occurrences[1];
        let second_child = &occurrences[2];
        assert_eq!(2, first_child.path.len());
        assert_eq!(2, second_child.path.len());
        assert_ne!(first_child, second_child);
        let first_resolved = document
            .resolve_occurrence(&OccurrenceRef {
                path: first_child.path.clone(),
            })
            .unwrap();
        assert!(first_resolved.entry.legacy_aliases.iter().any(|alias| {
            alias.source == "k3d.instance" && alias.id == "d4c79371-673f-4818-9abf-8a708edf684c"
        }));
    }

    #[test]
    fn preserves_prototype_geometry_when_only_annotation_overrides_are_present() {
        let source = r#"{
            "prototypes": [
                {"id":"root","name":"Root","definitionAxisU":{"x":1},"definitionAxisV":{"y":1},"definitionAxisW":{"z":1}},
                {"id":"box","name":"box","definitionAxisU":{"x":1},"definitionAxisV":{"y":1},"definitionAxisW":{"z":1},"segments":[{"start":{},"end":{"x":1}}]},
                {"id":"unused","name":"unused","definitionAxisU":{"x":1},"definitionAxisV":{"y":1},"definitionAxisW":{"z":1}}
            ],
            "rootInstance": {"prototypeId":"root","instanceAxisU":{"x":1},"instanceAxisV":{"y":1},"instanceAxisW":{"z":1},"children":[
                {"prototypeId":"box","instanceAxisU":{"x":1},"instanceAxisV":{"y":1},"instanceAxisW":{"z":1},"overrideDimensions":[{"id":"dimension-1"}]}
            ]}
        }"#;
        let document = import_current_json(source).unwrap();
        let occurrences = document.occurrence_refs().unwrap();
        let child = occurrences
            .iter()
            .find(|item| item.path.len() == 2)
            .unwrap();
        let geometry = document.effective_geometry(child).unwrap();
        assert_eq!(1, geometry.segments.len());
        let edits = document.occurrence_edits.get(child).unwrap();
        assert!(edits.geometry_replacement.is_none());
        assert!(edits.values.contains_key("k3d.overrideDimensions"));
        assert!(
            document
                .definitions
                .values()
                .find(|definition| definition.name == "unused")
                .unwrap()
                .default_child_layout
                .is_none()
        );
    }

    #[test]
    fn preserves_unknown_fields_on_prototype_and_instance_geometry_entities() {
        let source = r#"{
            "prototypes": [{
                "id":"root", "name":"Root",
                "definitionAxisU":{"x":1}, "definitionAxisV":{"y":1}, "definitionAxisW":{"z":1},
                "prototypeVertices":[{"id":"vertex-1","x":0,"y":0,"z":0,"futureVertex":"point"}],
                "segments":[{"id":"segment-1","start":{},"end":{"x":1},"futureSegment":{"tag":"kept"}}],
                "faces":[{"id":"face-1","a":{},"b":{"x":1},"c":{"y":1},"futureFace":[1,2]}]
            }],
            "rootInstance": {
                "id":"root-instance", "prototypeId":"root",
                "instanceAxisU":{"x":1}, "instanceAxisV":{"y":1}, "instanceAxisW":{"z":1},
                "hotspotPositions":[{"id":"hotspot-1","position":{},"futureHotspot":"location"}],
                "overrideSegments":[{"id":"override-segment","start":{},"end":{"z":1},"futureOverride":"segment"}],
                "overrideFaces":[{"id":"override-face","a":{},"b":{"x":1},"c":{"z":1},"futureOverride":"face"}]
            }
        }"#;
        let document = import_current_json(source).unwrap();
        let definition = document
            .definitions
            .values()
            .find(|definition| definition.name == "Root")
            .unwrap();
        assert_eq!(
            Some(&serde_json::json!({"tag":"kept"})),
            definition
                .geometry
                .segments
                .values()
                .next()
                .unwrap()
                .extensions
                .get("k3d.futureSegment")
        );
        assert_eq!(
            Some(&serde_json::json!([1, 2])),
            definition
                .geometry
                .triangles
                .values()
                .next()
                .unwrap()
                .extensions
                .get("k3d.futureFace")
        );
        assert_eq!(
            Some(&serde_json::json!("point")),
            definition
                .extensions
                .get("k3d.prototypeVertices.0.futureVertex")
        );

        let occurrence = document.occurrence_refs().unwrap().remove(0);
        let effective = document.effective_geometry(&occurrence).unwrap();
        assert_eq!(
            Some(&serde_json::json!("location")),
            document
                .occurrence_edits
                .get(&occurrence)
                .unwrap()
                .values
                .get("k3d.hotspotPositions/0/futureHotspot")
        );
        assert_eq!(
            Some(&serde_json::json!("segment")),
            effective
                .segments
                .values()
                .next()
                .unwrap()
                .extensions
                .get("k3d.futureOverride")
        );
        assert_eq!(
            Some(&serde_json::json!("face")),
            effective
                .triangles
                .values()
                .next()
                .unwrap()
                .extensions
                .get("k3d.futureOverride")
        );
    }

    #[test]
    fn keeps_a_differing_instance_mesh_override_local_to_its_occurrence() {
        let document = import_current_json(DIFFERING_INSTANCE_OVERRIDE).unwrap();
        let occurrences = document.occurrence_refs().unwrap();
        let changed = &occurrences[1];
        let inherited = &occurrences[2];
        let definition = document.resolve_occurrence(changed).unwrap().definition;
        let replacement = document.effective_geometry(changed).unwrap();

        assert_eq!(
            definition.id,
            document
                .resolve_occurrence(inherited)
                .unwrap()
                .definition
                .id
        );
        assert_ne!(
            replacement.segments.values().next().unwrap().legacy_aliases,
            definition
                .geometry
                .segments
                .values()
                .next()
                .unwrap()
                .legacy_aliases
        );
        assert_eq!(
            Vec3::new(2.0, 0.0, 0.0),
            replacement
                .vertex_position(replacement.segments.values().next().unwrap().endpoints[0])
                .unwrap()
        );
        assert!(
            document.occurrence_edits[changed]
                .geometry_replacement
                .is_some()
        );
        assert!(
            document
                .occurrence_edits
                .get(inherited)
                .and_then(|edits| edits.geometry_replacement.as_ref())
                .is_none()
        );
        assert!(std::ptr::eq(
            &definition.geometry,
            document.effective_geometry(inherited).unwrap()
        ));
        assert!(document.validate().is_ok());
    }

    #[test]
    fn reuses_vertices_only_when_a_legacy_vertex_alias_proves_identity() {
        let mut geometry = GeometryDraft::new();
        let mut reusable = HashMap::new();
        let key = QuantizedVertex(10, 20, 30);
        let alias = vec![LegacyAlias {
            source: "k3d.vertex".to_owned(),
            id: "vertex-1".to_owned(),
        }];
        let first = geometry.add_vertex(
            Vec3::new(0.01, 0.02, 0.03),
            alias.clone(),
            &mut reusable,
            Some(key),
        );
        let same_legacy_vertex = geometry.add_vertex(
            Vec3::new(0.010_000_5, 0.02, 0.03),
            alias.clone(),
            &mut reusable,
            Some(key),
        );
        let exact_legacy_vertex =
            geometry.add_vertex(Vec3::new(0.01, 0.02, 0.03), alias, &mut reusable, Some(key));
        let unaliased_first =
            geometry.add_vertex(Vec3::new(0.01, 0.02, 0.03), Vec::new(), &mut reusable, None);
        let unaliased_second =
            geometry.add_vertex(Vec3::new(0.01, 0.02, 0.03), Vec::new(), &mut reusable, None);
        assert_eq!(first.vertex, same_legacy_vertex.vertex);
        assert_eq!(first.vertex, exact_legacy_vertex.vertex);
        assert_eq!(Vec3::ZERO, exact_legacy_vertex.position_offset);
        assert_ne!(Vec3::ZERO, same_legacy_vertex.position_offset);
        let canonical = geometry.vertices[first.vertex].position;
        assert_eq!(Vec3::new(0.01, 0.02, 0.03), canonical);
        assert_eq!(
            Vec3::new(0.010_000_5, 0.02, 0.03),
            canonical + same_legacy_vertex.position_offset
        );
        assert_ne!(unaliased_first.vertex, unaliased_second.vertex);
        assert_eq!(3, geometry.vertices.len());
    }

    #[test]
    fn imports_shared_legacy_vertex_aliases_without_losing_corner_coordinates() {
        let source = r#"{
            "prototypes": [{
                "id": "root",
                "name": "Root",
                "definitionAxisU":{"x":1}, "definitionAxisV":{"y":1}, "definitionAxisW":{"z":1},
                "prototypeVertices": [{"id":"vertex-1","x":10,"y":20,"z":30}],
                "segments": [
                    {"start":{"x":0.01,"y":0.02,"z":0.03},"end":{"x":1,"y":0,"z":0}},
                    {"start":{"x":0.0100005,"y":0.02,"z":0.03},"end":{"x":1,"y":0,"z":0}}
                ]
            }],
            "rootInstance": {
                "id":"root-instance", "prototypeId":"root",
                "instanceAxisU":{"x":1}, "instanceAxisV":{"y":1}, "instanceAxisW":{"z":1}
            }
        }"#;
        let document = import_current_json(source).unwrap();
        let occurrence = document
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let geometry = document.effective_geometry(&occurrence).unwrap();
        let segments: Vec<_> = geometry.segments.values().collect();
        let first_corner = segments[0].endpoints[0];
        let second_corner = segments[1].endpoints[0];

        assert_eq!(first_corner.id, second_corner.id);
        assert_ne!(first_corner.position_offset, second_corner.position_offset);
        assert_eq!(
            Vec3::new(0.01, 0.02, 0.03),
            geometry.vertex_position(first_corner).unwrap()
        );
        assert_eq!(
            Vec3::new(0.010_000_5, 0.02, 0.03),
            geometry.vertex_position(second_corner).unwrap()
        );
        assert_ne!(segments[0].endpoints[1].id, segments[1].endpoints[1].id);
        assert!(
            geometry.vertices[&first_corner.id]
                .legacy_aliases
                .iter()
                .any(|alias| alias.source == "k3d.vertex" && alias.id == "vertex-1")
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn imports_real_glyph_fixture_alias_collisions_with_exact_corner_positions() {
        let document = import_current_json(ALPHABET_GLYPHS).unwrap();
        let prototype = document
            .definitions
            .values()
            .find(|definition| {
                definition.legacy_aliases.iter().any(|alias| {
                    alias.source == "k3d.prototype"
                        && alias.id == "5764ebfb-723d-48fe-9572-d6ab917a2b3a"
                })
            })
            .unwrap();
        let vertex = prototype
            .geometry
            .vertices
            .values()
            .find(|vertex| {
                vertex.legacy_aliases.iter().any(|alias| {
                    alias.source == "k3d.vertex"
                        && alias.id == "cc94ef9a-a22c-4491-a0bb-73be59c5e507"
                })
            })
            .unwrap();
        let segment_points = prototype
            .geometry
            .segments
            .values()
            .flat_map(|segment| segment.endpoints)
            .filter(|reference| reference.id == vertex.id)
            .filter_map(|reference| prototype.geometry.vertex_position(reference));
        let triangle_points = prototype
            .geometry
            .triangles
            .values()
            .flat_map(|triangle| triangle.vertices)
            .filter(|reference| reference.id == vertex.id)
            .filter_map(|reference| prototype.geometry.vertex_position(reference));
        let points: Vec<_> = segment_points.chain(triangle_points).collect();
        let first = Vec3::new(4.0, 0.0, 11.000_025);
        let second = Vec3::new(4.0, 0.0, 11.000_027);
        assert!(points.contains(&first));
        assert!(points.contains(&second));
        assert!(
            points
                .iter()
                .all(|point| *point == first || *point == second)
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn preserves_loose_mesh_and_architecture_payload_on_the_same_definition() {
        let document = import_current_json(ARCHITECTURE_WITH_MESH).unwrap();
        let root = document
            .definitions
            .values()
            .find(|definition| definition.name == "Root")
            .unwrap();

        assert_eq!(48, root.geometry.segments.len());
        assert_eq!(50, root.geometry.triangles.len());
        for (field, expected) in [
            ("k3d.architectureWalls", 5),
            ("k3d.architectureSlabs", 6),
            ("k3d.architectureFrames", 25),
        ] {
            assert_eq!(
                Some(expected),
                root.extensions
                    .get(field)
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len)
            );
        }
        assert!(document.validate().is_ok());
    }
}
