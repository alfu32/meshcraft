//! Read-only adapters from the current Octodraw prototype/instance JSON format.

use crate::document::{
    ChildLayoutId, DefinitionId, DefinitionKind, Document, InstanceEdits, LayoutEntryId,
    LegacyAlias, ModelSettings, OccurrenceRef, RawGeometrySegment, RawGeometryTriangle,
    RawGeometryVertex, Rgba8,
};
use crate::geometry::{Affine3, GeometryTolerance, Vec3};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const LEGACY_VERTEX_KEY_EPSILON: f64 = 1.0e-3;
const MAX_INSTANCE_COUNT: usize = 100_000;
const MAX_INSTANCE_DEPTH: usize = 256;

type ImportResult<T> = Result<T, String>;
type ParsedOverrideFace = ([Vec3; 3], Rgba8, Vec<LegacyAlias>);

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
    ) -> usize {
        if let Some(key) = quantized_alias {
            if let Some(index) = reusable.get(&key) {
                return *index;
            }
            let index = self.vertices.len();
            self.vertices.push(RawGeometryVertex {
                position,
                legacy_aliases: aliases,
            });
            reusable.insert(key, index);
            return index;
        }

        let index = self.vertices.len();
        self.vertices.push(RawGeometryVertex {
            position,
            legacy_aliases: aliases,
        });
        index
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
    let segment_values = read_array(object.get("segments"), &format!("{path}/segments"))?;
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
        });
    }

    let face_values = read_array(object.get("faces"), &format!("{path}/faces"))?;
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
        let mut indices = [0; 3];
        for (corner, point) in points.into_iter().enumerate() {
            indices[corner] = geometry.add_vertex(
                point,
                vertex_alias(point, &aliases),
                &mut reusable,
                quantized_alias_key(point, &aliases)?,
            );
        }
        geometry.triangles.push(RawGeometryTriangle {
            vertices: indices,
            color: parse_color(face.get("color"), &format!("{face_path}/color"))?,
            legacy_aliases: entity_alias(face, "k3d.face")?,
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
            });
        }
        for triangle in override_triangles {
            let points = triangle.0;
            let indices =
                points.map(|point| geometry.add_vertex(point, Vec::new(), &mut reusable, None));
            geometry.triangles.push(RawGeometryTriangle {
                vertices: indices,
                color: triangle.1,
                legacy_aliases: triangle.2,
            });
        }
        geometry
    });

    let mut preserved_fields = BTreeMap::new();
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
) -> ImportResult<Vec<(Vec3, Vec3, Vec<LegacyAlias>)>> {
    let mut output = Vec::new();
    for (index, value) in read_array(
        object.get("overrideSegments"),
        &format!("{path}/overrideSegments"),
    )?
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
        ));
    }
    Ok(output)
}

fn parse_override_faces(
    object: &Map<String, Value>,
    path: &str,
) -> ImportResult<Vec<ParsedOverrideFace>> {
    let mut output = Vec::new();
    for (index, value) in read_array(
        object.get("overrideFaces"),
        &format!("{path}/overrideFaces"),
    )?
    .iter()
    .enumerate()
    {
        let face_path = format!("{path}/overrideFaces/{index}");
        let face = value
            .as_object()
            .ok_or_else(|| format!("{face_path}: face must be an object"))?;
        output.push((
            [
                read_vec3(face.get("a"), &format!("{face_path}/a"))?,
                read_vec3(face.get("b"), &format!("{face_path}/b"))?,
                read_vec3(face.get("c"), &format!("{face_path}/c"))?,
            ],
            parse_color(face.get("color"), &format!("{face_path}/color"))?,
            entity_alias(face, "k3d.face")?,
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
    use super::{GeometryDraft, QuantizedVertex, import_current_json, import_current_value};
    use crate::document::{DefinitionKind, LegacyAlias, OccurrenceRef};
    use crate::geometry::Vec3;
    use serde_json::Value;
    use std::collections::HashMap;

    const ARCH_FRAME: &str =
        include_str!("../../../fixtures/legacy/samples/examples/arch_frame.octd");
    const HOTSPOT_SCENE: &str =
        include_str!("../../../fixtures/legacy/samples/examples/hotspots.demo.2.k3d");

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
        assert!(document.occurrence_edits.values().any(|edits| {
            edits
                .hotspot_positions
                .values()
                .any(|position| *position != Vec3::ZERO)
        }));
        assert!(document.extensions.contains_key("k3d.undoHistory"));
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
        assert!(
            document
                .resolve_occurrence(&OccurrenceRef {
                    path: first_child.path.clone(),
                })
                .is_some()
        );
    }

    #[test]
    fn preserves_prototype_geometry_when_only_domain_overrides_are_present() {
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
            alias,
            &mut reusable,
            Some(key),
        );
        let unaliased_first =
            geometry.add_vertex(Vec3::new(0.01, 0.02, 0.03), Vec::new(), &mut reusable, None);
        let unaliased_second =
            geometry.add_vertex(Vec3::new(0.01, 0.02, 0.03), Vec::new(), &mut reusable, None);
        assert_eq!(first, same_legacy_vertex);
        assert_ne!(unaliased_first, unaliased_second);
        assert_eq!(3, geometry.vertices.len());
    }
}
