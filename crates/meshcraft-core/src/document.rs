use crate::geometry::{Affine3, ModelUnitScale, Vec3};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

pub mod derived;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}

stable_id!(DefinitionId);
stable_id!(GeometryBundleId);
stable_id!(VertexId);
stable_id!(SegmentId);
stable_id!(TriangleId);
stable_id!(ChildLayoutId);
stable_id!(LayoutEntryId);

/// Identity changes produced by one topology-edit operation.
///
/// A missing key means the source ID is unchanged, an empty target list means it was deleted,
/// and multiple ordered targets represent a split. Consumers apply these mappings in the same
/// transaction as the geometry edit.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TopologyRemap {
    #[serde(default)]
    pub vertices: BTreeMap<VertexId, Vec<VertexId>>,
    #[serde(default)]
    pub segments: BTreeMap<SegmentId, Vec<SegmentId>>,
    #[serde(default)]
    pub triangles: BTreeMap<TriangleId, Vec<TriangleId>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineInsertion {
    /// Segments overlapping any part of the newly drawn line, including reused collinear edges.
    pub affected_segments: Vec<SegmentId>,
    pub remap: TopologyRemap,
}

impl TopologyRemap {
    pub fn vertex_targets(&self, id: VertexId) -> Cow<'_, [VertexId]> {
        match self.vertices.get(&id) {
            Some(targets) => Cow::Borrowed(targets.as_slice()),
            None => Cow::Owned(vec![id]),
        }
    }

    pub fn segment_targets(&self, id: SegmentId) -> Cow<'_, [SegmentId]> {
        match self.segments.get(&id) {
            Some(targets) => Cow::Borrowed(targets.as_slice()),
            None => Cow::Owned(vec![id]),
        }
    }

    pub fn triangle_targets(&self, id: TriangleId) -> Cow<'_, [TriangleId]> {
        match self.triangles.get(&id) {
            Some(targets) => Cow::Borrowed(targets.as_slice()),
            None => Cow::Owned(vec![id]),
        }
    }
}

/// A reference to one realized placement through shared child-layout templates.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OccurrenceRef {
    pub path: Vec<LayoutEntryId>,
}

impl OccurrenceRef {
    /// Refers to authored geometry in the document's scene root.
    pub fn scene_root() -> Self {
        Self { path: Vec::new() }
    }

    pub fn is_scene_root(&self) -> bool {
        self.path.is_empty()
    }

    pub fn root(entry: LayoutEntryId) -> Self {
        Self { path: vec![entry] }
    }

    pub fn child(&self, entry: LayoutEntryId) -> Self {
        let mut path = self.path.clone();
        path.push(entry);
        Self { path }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LegacyAlias {
    pub source: String,
    pub id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Rgba8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Rgba8 {
    pub const WHITE: Self = Self::new(255, 255, 255, 255);

    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct GeometryBundle {
    pub id: GeometryBundleId,
    pub vertices: BTreeMap<VertexId, Vertex>,
    pub segments: BTreeMap<SegmentId, Segment>,
    pub triangles: BTreeMap<TriangleId, Triangle>,
    pub extensions: BTreeMap<String, Value>,
}

impl GeometryBundle {
    /// Checks references and finite coordinates without repairing or discarding raw topology.
    pub fn validate(&self) -> Result<(), String> {
        for (id, vertex) in &self.vertices {
            if *id != vertex.id {
                return Err(format!(
                    "vertex map key {} differs from record id {}",
                    id.0, vertex.id.0
                ));
            }
            if !vertex.position.is_finite() {
                return Err(format!("vertex {} has a non-finite position", id.0));
            }
        }
        for (id, segment) in &self.segments {
            if *id != segment.id {
                return Err(format!(
                    "segment map key {} differs from record id {}",
                    id.0, segment.id.0
                ));
            }
            if segment.endpoints.iter().any(|reference| {
                self.vertex_position(*reference)
                    .is_none_or(|point| !point.is_finite())
            }) {
                return Err(format!("segment {} has invalid endpoints", id.0));
            }
        }
        for (id, triangle) in &self.triangles {
            if *id != triangle.id {
                return Err(format!(
                    "triangle map key {} differs from record id {}",
                    id.0, triangle.id.0
                ));
            }
            if triangle.vertices.iter().any(|reference| {
                self.vertex_position(*reference)
                    .is_none_or(|point| !point.is_finite())
            }) {
                return Err(format!("triangle {} has invalid vertex references", id.0));
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() && self.segments.is_empty() && self.triangles.is_empty()
    }

    /// Resolves a primitive corner to its exact point while retaining its topological vertex ID.
    pub fn vertex_position(&self, reference: VertexRef) -> Option<Vec3> {
        self.vertices
            .get(&reference.id)
            .map(|vertex| vertex.position + reference.position_offset)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Vertex {
    pub id: VertexId,
    pub position: Vec3,
    #[serde(default)]
    pub legacy_aliases: Vec<LegacyAlias>,
}

/// A topological vertex reference with a per-corner offset for source formats that store
/// slightly different coordinates for corners sharing one persisted vertex identity.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct VertexRef {
    pub id: VertexId,
    #[serde(default)]
    pub position_offset: Vec3,
}

impl<'de> Deserialize<'de> for VertexRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Representation {
            Legacy(VertexId),
            Current {
                id: VertexId,
                #[serde(default)]
                position_offset: Vec3,
            },
        }

        Ok(match Representation::deserialize(deserializer)? {
            Representation::Legacy(id) => Self::new(id),
            Representation::Current {
                id,
                position_offset,
            } => Self {
                id,
                position_offset,
            },
        })
    }
}

impl VertexRef {
    pub const fn new(id: VertexId) -> Self {
        Self {
            id,
            position_offset: Vec3::ZERO,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Segment {
    pub id: SegmentId,
    pub endpoints: [VertexRef; 2],
    pub color: Rgba8,
    #[serde(default)]
    pub legacy_aliases: Vec<LegacyAlias>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Triangle {
    pub id: TriangleId,
    pub vertices: [VertexRef; 3],
    pub color: Rgba8,
    #[serde(default)]
    pub legacy_aliases: Vec<LegacyAlias>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

/// One generated face replacing a selected triangle during an atomic topology edit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriangleReplacement {
    pub source: TriangleId,
    pub vertices: [Vec3; 3],
    pub color: Rgba8,
}

/// A raw topological vertex supplied by an importer; positions are never welded implicitly.
#[derive(Clone, Debug)]
pub struct RawGeometryVertex {
    pub position: Vec3,
    pub legacy_aliases: Vec<LegacyAlias>,
}

/// An indexed reference to a raw vertex plus an exact source-corner position offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawVertexRef {
    pub vertex: usize,
    pub position_offset: Vec3,
}

/// A raw segment whose endpoints reference the vertex array passed to the import operation.
#[derive(Clone, Debug)]
pub struct RawGeometrySegment {
    pub endpoints: [RawVertexRef; 2],
    pub color: Rgba8,
    pub legacy_aliases: Vec<LegacyAlias>,
    pub extensions: BTreeMap<String, Value>,
}

/// A raw triangle whose corners reference the vertex array passed to the import operation.
#[derive(Clone, Debug)]
pub struct RawGeometryTriangle {
    pub vertices: [RawVertexRef; 3],
    pub color: Rgba8,
    pub legacy_aliases: Vec<LegacyAlias>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionKind {
    Mesh,
    Voxel,
    Architecture,
    Hybrid,
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Definition {
    pub id: DefinitionId,
    pub name: String,
    /// Editing/workflow classification; domain components remain sparse and composable.
    pub kind: DefinitionKind,
    /// Semantic definition frame `D` in the legacy equation `W = parent_W × I × D`.
    pub frame: Affine3,
    pub geometry: GeometryBundle,
    pub default_child_layout: Option<ChildLayoutId>,
    /// Sparse domain parameters are migrated to typed components by the IO layer.
    pub domain_data: BTreeMap<String, Value>,
    /// Original payload values that this implementation does not yet understand.
    pub extensions: BTreeMap<String, Value>,
    pub legacy_aliases: Vec<LegacyAlias>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChildLayout {
    pub id: ChildLayoutId,
    pub entries: Vec<LayoutEntry>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LayoutEntry {
    pub id: LayoutEntryId,
    pub definition: DefinitionId,
    /// Instance placement `I`; each definition retains its independent frame `D`.
    pub transform: Affine3,
    pub legacy_aliases: Vec<LegacyAlias>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InstanceEdits {
    /// Original legacy placement identity, when the occurrence came from an imported project.
    #[serde(default)]
    pub legacy_instance_id: Option<String>,
    /// `None` inherits the layout transform; geometry replacement may be explicitly empty.
    pub transform: Option<Affine3>,
    pub geometry_replacement: Option<GeometryBundle>,
    pub child_layout: Option<ChildLayoutId>,
    pub hotspot_positions: BTreeMap<String, Vec3>,
    pub values: BTreeMap<String, Value>,
    pub extensions: BTreeMap<String, Value>,
}

impl InstanceEdits {
    fn is_empty(&self) -> bool {
        self.legacy_instance_id.is_none()
            && self.transform.is_none()
            && self.geometry_replacement.is_none()
            && self.child_layout.is_none()
            && self.hotspot_positions.is_empty()
            && self.values.is_empty()
            && self.extensions.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct ModelSettings {
    pub unit_name: String,
    pub unit_size: f64,
    pub grid_size: f64,
    pub circle_segments: u32,
    /// Screen-space snapping radius, matching the legacy `snapEpsilon` setting.
    pub snap_radius: f64,
    pub arc_max_length: f64,
    pub double_line_size: f64,
    pub double_line_offset: f64,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            unit_name: "unit".to_owned(),
            unit_size: 1.0,
            grid_size: 1.0,
            circle_segments: 24,
            snap_radius: 12.0,
            arc_max_length: 0.5,
            double_line_size: 1.0,
            double_line_offset: 0.5,
        }
    }
}

impl ModelSettings {
    pub fn unit_scale(&self) -> Option<ModelUnitScale> {
        ModelUnitScale::new(self.unit_size)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.unit_name.trim().is_empty() {
            return Err("unit name cannot be empty".to_owned());
        }
        if !self.unit_size.is_finite() || self.unit_size <= 0.0 {
            return Err("unit size must be finite and positive".to_owned());
        }
        if !self.grid_size.is_finite() || self.grid_size <= 0.0 {
            return Err("grid size must be finite and positive".to_owned());
        }
        if !(3..=256).contains(&self.circle_segments) {
            return Err("circle segments must be between 3 and 256".to_owned());
        }
        if !self.snap_radius.is_finite() || !(0.0..=128.0).contains(&self.snap_radius) {
            return Err("snap radius must be between 0 and 128 pixels".to_owned());
        }
        if !self.arc_max_length.is_finite() || self.arc_max_length <= 0.0 {
            return Err("arc maximum length must be finite and positive".to_owned());
        }
        if !self.double_line_size.is_finite() || self.double_line_size <= 0.0 {
            return Err("double-line size must be finite and positive".to_owned());
        }
        if !self.double_line_offset.is_finite() {
            return Err("double-line offset must be finite".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Document {
    pub(crate) schema_version: u32,
    pub(crate) revision: u64,
    pub(crate) root_layout: ChildLayoutId,
    pub(crate) definitions: BTreeMap<DefinitionId, Definition>,
    pub(crate) child_layouts: BTreeMap<ChildLayoutId, ChildLayout>,
    /// Overrides belong to realized occurrences, not shared layout entries.
    #[serde(with = "occurrence_edit_map")]
    pub(crate) occurrence_edits: BTreeMap<OccurrenceRef, InstanceEdits>,
    #[serde(default)]
    pub(crate) model_settings: ModelSettings,
    pub(crate) extensions: BTreeMap<String, Value>,
    next_id: u64,
}

impl Document {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;
    pub const SCENE_ROOT_DEFINITION_ID: DefinitionId = DefinitionId(0);

    pub fn new() -> Self {
        let root_layout = ChildLayoutId(1);
        let mut child_layouts = BTreeMap::new();
        child_layouts.insert(
            root_layout,
            ChildLayout {
                id: root_layout,
                entries: Vec::new(),
                extensions: BTreeMap::new(),
            },
        );
        let mut definitions = BTreeMap::new();
        definitions.insert(
            Self::SCENE_ROOT_DEFINITION_ID,
            Definition {
                id: Self::SCENE_ROOT_DEFINITION_ID,
                name: "Root".to_owned(),
                kind: DefinitionKind::Mesh,
                frame: Affine3::IDENTITY,
                geometry: GeometryBundle {
                    id: GeometryBundleId(0),
                    ..GeometryBundle::default()
                },
                default_child_layout: Some(root_layout),
                domain_data: BTreeMap::new(),
                extensions: BTreeMap::new(),
                legacy_aliases: Vec::new(),
            },
        );
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            revision: 0,
            root_layout,
            definitions,
            child_layouts,
            occurrence_edits: BTreeMap::new(),
            model_settings: ModelSettings::default(),
            extensions: BTreeMap::new(),
            next_id: 2,
        }
    }

    pub fn root_layout(&self) -> ChildLayoutId {
        self.root_layout
    }

    pub fn scene_root_definition(&self) -> Option<&Definition> {
        self.definitions.get(&Self::SCENE_ROOT_DEFINITION_ID)
    }

    /// Adds the empty root graph to older normalized snapshots that predate scene-root geometry.
    pub fn ensure_scene_root(&mut self) -> DefinitionId {
        let id = Self::SCENE_ROOT_DEFINITION_ID;
        self.definitions.entry(id).or_insert_with(|| Definition {
            id,
            name: "Root".to_owned(),
            kind: DefinitionKind::Mesh,
            frame: Affine3::IDENTITY,
            geometry: GeometryBundle {
                id: GeometryBundleId(0),
                ..GeometryBundle::default()
            },
            default_child_layout: Some(self.root_layout),
            domain_data: BTreeMap::new(),
            extensions: BTreeMap::new(),
            legacy_aliases: Vec::new(),
        });
        id
    }

    /// Monotonic authored-state revision used to invalidate non-persistent caches.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn model_settings(&self) -> &ModelSettings {
        &self.model_settings
    }

    pub fn definition(&self, id: DefinitionId) -> Option<&Definition> {
        self.definitions.get(&id)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &Definition> {
        self.definitions
            .values()
            .filter(|definition| definition.id != Self::SCENE_ROOT_DEFINITION_ID)
    }

    pub fn child_layout(&self, id: ChildLayoutId) -> Option<&ChildLayout> {
        self.child_layouts.get(&id)
    }

    pub fn occurrence_edits(&self, occurrence: &OccurrenceRef) -> Option<&InstanceEdits> {
        self.occurrence_edits.get(occurrence)
    }

    pub fn update_model_settings(&mut self, settings: ModelSettings) -> Result<bool, String> {
        settings.validate()?;
        if settings == self.model_settings {
            return Ok(false);
        }
        self.model_settings = settings;
        self.revision = self.revision.saturating_add(1);
        Ok(true)
    }

    pub fn rename_definition(
        &mut self,
        definition_id: DefinitionId,
        name: &str,
    ) -> Result<bool, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("object name cannot be empty".to_owned());
        }
        let definition = self
            .definitions
            .get_mut(&definition_id)
            .ok_or_else(|| format!("missing definition {}", definition_id.0))?;
        if definition.name == name {
            return Ok(false);
        }
        definition.name = name.to_owned();
        self.revision = self.revision.saturating_add(1);
        Ok(true)
    }

    pub fn add_definition(
        &mut self,
        name: impl Into<String>,
        kind: DefinitionKind,
    ) -> DefinitionId {
        let id = DefinitionId(self.allocate_id());
        let geometry_id = GeometryBundleId(self.allocate_id());
        self.definitions.insert(
            id,
            Definition {
                id,
                name: name.into(),
                kind,
                frame: Affine3::IDENTITY,
                geometry: GeometryBundle {
                    id: geometry_id,
                    ..GeometryBundle::default()
                },
                default_child_layout: None,
                domain_data: BTreeMap::new(),
                extensions: BTreeMap::new(),
                legacy_aliases: Vec::new(),
            },
        );
        self.revision = self.revision.saturating_add(1);
        id
    }

    pub fn add_child_layout(&mut self) -> ChildLayoutId {
        let id = ChildLayoutId(self.allocate_id());
        self.child_layouts.insert(
            id,
            ChildLayout {
                id,
                entries: Vec::new(),
                extensions: BTreeMap::new(),
            },
        );
        self.revision = self.revision.saturating_add(1);
        id
    }

    /// Replaces a definition's geometry with raw indexed data, without topology repair or welding.
    pub fn replace_definition_geometry_raw(
        &mut self,
        definition_id: DefinitionId,
        vertices: Vec<RawGeometryVertex>,
        segments: Vec<RawGeometrySegment>,
        triangles: Vec<RawGeometryTriangle>,
    ) -> Result<Vec<VertexId>, String> {
        if !self.definitions.contains_key(&definition_id) {
            return Err(format!("missing definition {}", definition_id.0));
        }
        let geometry = self.build_geometry_bundle_raw(vertices, segments, triangles)?;
        let vertex_ids = geometry.vertices.keys().copied().collect();
        self.definitions
            .get_mut(&definition_id)
            .expect("definition checked before geometry allocation")
            .geometry = geometry;
        self.revision = self.revision.saturating_add(1);
        Ok(vertex_ids)
    }

    /// Builds raw indexed geometry for an authored occurrence replacement or definition import.
    pub fn build_geometry_bundle_raw(
        &mut self,
        vertices: Vec<RawGeometryVertex>,
        segments: Vec<RawGeometrySegment>,
        triangles: Vec<RawGeometryTriangle>,
    ) -> Result<GeometryBundle, String> {
        if vertices.iter().any(|vertex| !vertex.position.is_finite()) {
            return Err("raw geometry contains a non-finite vertex".to_owned());
        }
        for segment in &segments {
            if segment.endpoints.iter().any(|reference| {
                reference.vertex >= vertices.len() || !reference.position_offset.is_finite()
            }) {
                return Err("raw segment references a missing vertex".to_owned());
            }
        }
        for triangle in &triangles {
            if triangle.vertices.iter().any(|reference| {
                reference.vertex >= vertices.len() || !reference.position_offset.is_finite()
            }) {
                return Err("raw triangle references a missing vertex".to_owned());
            }
        }

        let mut geometry = GeometryBundle {
            id: GeometryBundleId(self.allocate_id()),
            ..GeometryBundle::default()
        };
        let mut vertex_ids = Vec::with_capacity(vertices.len());
        for vertex in vertices {
            let id = VertexId(self.allocate_id());
            geometry.vertices.insert(
                id,
                Vertex {
                    id,
                    position: vertex.position,
                    legacy_aliases: vertex.legacy_aliases,
                },
            );
            vertex_ids.push(id);
        }
        for segment in segments {
            let id = SegmentId(self.allocate_id());
            geometry.segments.insert(
                id,
                Segment {
                    id,
                    endpoints: [
                        VertexRef {
                            id: vertex_ids[segment.endpoints[0].vertex],
                            position_offset: segment.endpoints[0].position_offset,
                        },
                        VertexRef {
                            id: vertex_ids[segment.endpoints[1].vertex],
                            position_offset: segment.endpoints[1].position_offset,
                        },
                    ],
                    color: segment.color,
                    legacy_aliases: segment.legacy_aliases,
                    extensions: segment.extensions,
                },
            );
        }
        for triangle in triangles {
            let id = TriangleId(self.allocate_id());
            geometry.triangles.insert(
                id,
                Triangle {
                    id,
                    vertices: triangle.vertices.map(|reference| VertexRef {
                        id: vertex_ids[reference.vertex],
                        position_offset: reference.position_offset,
                    }),
                    color: triangle.color,
                    legacy_aliases: triangle.legacy_aliases,
                    extensions: triangle.extensions,
                },
            );
        }
        geometry.validate()?;
        Ok(geometry)
    }

    pub fn add_layout_entry(
        &mut self,
        layout: ChildLayoutId,
        definition: DefinitionId,
        transform: Affine3,
    ) -> Option<LayoutEntryId> {
        if !self.child_layouts.contains_key(&layout) || !self.definitions.contains_key(&definition)
        {
            return None;
        }
        let id = LayoutEntryId(self.allocate_id());
        self.child_layouts
            .get_mut(&layout)?
            .entries
            .push(LayoutEntry {
                id,
                definition,
                transform,
                legacy_aliases: Vec::new(),
            });
        self.revision = self.revision.saturating_add(1);
        Some(id)
    }

    /// Creates a closed, axis-aligned cuboid mesh in the document root.
    pub fn add_cuboid(
        &mut self,
        name: impl Into<String>,
        dimensions: Vec3,
        origin: Vec3,
    ) -> Option<OccurrenceRef> {
        if !dimensions.is_finite()
            || dimensions.x <= 0.0
            || dimensions.y <= 0.0
            || dimensions.z <= 0.0
            || !origin.is_finite()
        {
            return None;
        }
        let mut geometry = GeometryBundle::default();
        let corners = [
            Vec3::ZERO,
            Vec3::new(dimensions.x, 0.0, 0.0),
            Vec3::new(dimensions.x, dimensions.y, 0.0),
            Vec3::new(0.0, dimensions.y, 0.0),
            Vec3::new(0.0, 0.0, dimensions.z),
            Vec3::new(dimensions.x, 0.0, dimensions.z),
            dimensions,
            Vec3::new(0.0, dimensions.y, dimensions.z),
        ];
        let mut vertex_ids = Vec::with_capacity(corners.len());
        for position in corners {
            let id = VertexId(self.allocate_id());
            geometry.vertices.insert(
                id,
                Vertex {
                    id,
                    position,
                    legacy_aliases: Vec::new(),
                },
            );
            vertex_ids.push(id);
        }
        for [a, b] in [
            [0, 1],
            [1, 2],
            [2, 3],
            [3, 0],
            [4, 5],
            [5, 6],
            [6, 7],
            [7, 4],
            [0, 4],
            [1, 5],
            [2, 6],
            [3, 7],
        ] {
            let id = SegmentId(self.allocate_id());
            geometry.segments.insert(
                id,
                Segment {
                    id,
                    endpoints: [VertexRef::new(vertex_ids[a]), VertexRef::new(vertex_ids[b])],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
        }
        for [a, b, c] in [
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 3, 2],
            [0, 2, 1],
            [4, 5, 6],
            [4, 6, 7],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ] {
            let id = TriangleId(self.allocate_id());
            geometry.triangles.insert(
                id,
                Triangle {
                    id,
                    vertices: [
                        VertexRef::new(vertex_ids[a]),
                        VertexRef::new(vertex_ids[b]),
                        VertexRef::new(vertex_ids[c]),
                    ],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
        }
        self.add_root_geometry(name, geometry, Affine3::from_translation(origin))
    }

    /// Adds one open segment as a standalone root object without running topology repair.
    pub fn add_line_segment(
        &mut self,
        name: impl Into<String>,
        start: Vec3,
        end: Vec3,
    ) -> Option<OccurrenceRef> {
        if !start.is_finite() || !end.is_finite() {
            return None;
        }
        let scale = start.length().max(end.length());
        if start.distance(end) <= crate::geometry::GeometryTolerance::default().linear_at(scale) {
            return None;
        }

        let mut geometry = GeometryBundle::default();
        let start_id = VertexId(self.allocate_id());
        let end_id = VertexId(self.allocate_id());
        geometry.vertices.insert(
            start_id,
            Vertex {
                id: start_id,
                position: start,
                legacy_aliases: Vec::new(),
            },
        );
        geometry.vertices.insert(
            end_id,
            Vertex {
                id: end_id,
                position: end,
                legacy_aliases: Vec::new(),
            },
        );
        let segment_id = SegmentId(self.allocate_id());
        geometry.segments.insert(
            segment_id,
            Segment {
                id: segment_id,
                endpoints: [VertexRef::new(start_id), VertexRef::new(end_id)],
                color: Rgba8::WHITE,
                legacy_aliases: Vec::new(),
                extensions: BTreeMap::new(),
            },
        );
        self.add_root_geometry(name, geometry, Affine3::IDENTITY)
    }

    /// Appends a standalone segment to an existing object definition.
    pub fn append_line_segment_to_definition(
        &mut self,
        definition_id: DefinitionId,
        start: Vec3,
        end: Vec3,
    ) -> Option<SegmentId> {
        self.append_line_segment_to_definition_with_remap(definition_id, start, end)?
            .affected_segments
            .first()
            .copied()
    }

    pub fn append_line_segment_to_definition_with_remap(
        &mut self,
        definition_id: DefinitionId,
        start: Vec3,
        end: Vec3,
    ) -> Option<LineInsertion> {
        if !start.is_finite() || !end.is_finite() {
            return None;
        }
        let geometry = self.definitions.get(&definition_id)?.geometry.clone();
        let (replacement, insertion) =
            insert_line_geometry(&geometry, start, end, &mut self.next_id)?;
        let definition = self.definitions.get_mut(&definition_id)?;
        definition.geometry = replacement;
        self.revision = self.revision.saturating_add(1);
        Some(insertion)
    }

    /// Inserts a line directly into the document's scene-root geometry graph.
    pub fn append_line_segment_to_scene_root(
        &mut self,
        start: Vec3,
        end: Vec3,
    ) -> Option<LineInsertion> {
        let definition_id = self.ensure_scene_root();
        let geometry = self.definitions.get(&definition_id)?.geometry.clone();
        let (replacement, insertion) =
            insert_line_geometry(&geometry, start, end, &mut self.next_id)?;
        self.definitions.get_mut(&definition_id)?.geometry = replacement;
        self.revision = self.revision.saturating_add(1);
        Some(insertion)
    }

    /// Appends rectangle edges and faces to an existing object definition.
    pub fn append_planar_rectangle_to_definition(
        &mut self,
        definition_id: DefinitionId,
        corners: [Vec3; 4],
    ) -> Option<(Vec<SegmentId>, Vec<TriangleId>)> {
        if corners.iter().any(|point| !point.is_finite()) {
            return None;
        }
        let edge_u = corners[1] - corners[0];
        let edge_v = corners[3] - corners[0];
        let scale = corners
            .iter()
            .map(|point| point.length())
            .fold(0.0, f64::max);
        let tolerance = crate::geometry::GeometryTolerance::default().linear_at(scale);
        let edge_u_length = edge_u.length();
        let edge_v_length = edge_v.length();
        if edge_u_length <= tolerance
            || edge_v_length <= tolerance
            || edge_u.cross(edge_v).length() <= tolerance * tolerance
            || edge_u.dot(edge_v).abs() > tolerance * edge_u_length.max(edge_v_length)
            || corners[2].distance(corners[0] + edge_u + edge_v) > tolerance * 2.0
        {
            return None;
        }
        let raw_reference = |vertex| RawVertexRef {
            vertex,
            position_offset: Vec3::ZERO,
        };
        let vertices = corners
            .into_iter()
            .map(|position| RawGeometryVertex {
                position,
                legacy_aliases: Vec::new(),
            })
            .collect();
        let segments = [[0, 1], [1, 2], [2, 3], [3, 0]]
            .into_iter()
            .map(|[a, b]| RawGeometrySegment {
                endpoints: [raw_reference(a), raw_reference(b)],
                color: Rgba8::WHITE,
                legacy_aliases: Vec::new(),
                extensions: BTreeMap::new(),
            })
            .collect();
        let triangles = [[0, 1, 2], [0, 2, 3]]
            .into_iter()
            .map(|[a, b, c]| RawGeometryTriangle {
                vertices: [raw_reference(a), raw_reference(b), raw_reference(c)],
                color: Rgba8::WHITE,
                legacy_aliases: Vec::new(),
                extensions: BTreeMap::new(),
            })
            .collect();
        let geometry = self
            .build_geometry_bundle_raw(vertices, segments, triangles)
            .ok()?;
        let segment_ids = geometry.segments.keys().copied().collect();
        let triangle_ids = geometry.triangles.keys().copied().collect();
        self.append_geometry_to_definition(definition_id, geometry)
            .then_some((segment_ids, triangle_ids))
    }

    /// Inserts planar edges through the ordinary line topology path, then appends authored faces.
    /// An empty face list represents a line-only path.
    pub fn append_planar_mesh_to_definition(
        &mut self,
        definition_id: DefinitionId,
        vertices: Vec<Vec3>,
        segments: Vec<[usize; 2]>,
        triangles: Vec<[usize; 3]>,
    ) -> Result<(Vec<SegmentId>, Vec<TriangleId>), String> {
        if !self.definitions.contains_key(&definition_id) {
            return Err(format!("missing definition {}", definition_id.0));
        }
        validate_indexed_geometry_input(&vertices, &segments, &triangles)?;
        if segments.is_empty() {
            return Err("planar geometry needs at least one segment".to_owned());
        }

        let mut geometry = self.definitions[&definition_id].geometry.clone();
        let mut selected_segments = BTreeSet::new();
        let mut next_id = self.next_id;
        for [start, end] in &segments {
            let (replacement, insertion) =
                insert_line_geometry(&geometry, vertices[*start], vertices[*end], &mut next_id)
                    .ok_or_else(|| "planar shape contains an invalid edge".to_owned())?;
            for (source, targets) in insertion.remap.segments {
                if selected_segments.remove(&source) {
                    selected_segments.extend(targets);
                }
            }
            selected_segments.extend(insertion.affected_segments);
            geometry = replacement;
        }

        let model_scale = vertices
            .iter()
            .map(|point| point.length())
            .fold(0.0, f64::max);
        let tolerance = crate::geometry::GeometryTolerance::default().linear_at(model_scale);
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return Err("triangle insertion coordinates exceed the supported range".to_owned());
        }
        let mut vertex_ids = Vec::with_capacity(vertices.len());
        for position in vertices {
            let existing = geometry
                .vertices
                .values()
                .find(|vertex| vertex.position.distance(position) <= tolerance)
                .map(|vertex| vertex.id);
            let id = existing.unwrap_or_else(|| {
                let id = allocate_id(&mut next_id, VertexId);
                geometry.vertices.insert(
                    id,
                    Vertex {
                        id,
                        position,
                        legacy_aliases: Vec::new(),
                    },
                );
                id
            });
            vertex_ids.push(id);
        }

        let mut triangle_ids = Vec::with_capacity(triangles.len());
        for [first, second, third] in triangles {
            let id = allocate_id(&mut next_id, TriangleId);
            geometry.triangles.insert(
                id,
                Triangle {
                    id,
                    vertices: [
                        VertexRef::new(vertex_ids[first]),
                        VertexRef::new(vertex_ids[second]),
                        VertexRef::new(vertex_ids[third]),
                    ],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
            triangle_ids.push(id);
        }
        geometry.validate()?;
        self.definitions
            .get_mut(&definition_id)
            .expect("definition checked before shape insertion")
            .geometry = geometry;
        self.next_id = next_id;
        self.revision = self.revision.saturating_add(1);
        Ok((selected_segments.into_iter().collect(), triangle_ids))
    }

    /// Appends authored faces without creating or changing any line segments.
    pub fn append_triangles_to_definition(
        &mut self,
        definition_id: DefinitionId,
        vertices: Vec<Vec3>,
        triangles: Vec<[usize; 3]>,
    ) -> Result<Vec<TriangleId>, String> {
        if triangles.is_empty() {
            return Err("triangle insertion needs at least one face".to_owned());
        }
        validate_indexed_geometry_input(&vertices, &[], &triangles)?;
        let mut staged = self.clone();
        let Some(definition) = staged.definitions.get(&definition_id) else {
            return Err(format!("missing definition {}", definition_id.0));
        };
        let mut geometry = definition.geometry.clone();
        let model_scale = vertices
            .iter()
            .map(|point| point.length())
            .chain(
                geometry
                    .vertices
                    .values()
                    .map(|vertex| vertex.position.length()),
            )
            .fold(0.0, f64::max);
        let tolerance = crate::geometry::GeometryTolerance::default().linear_at(model_scale);
        let mut vertex_ids = Vec::with_capacity(vertices.len());
        for position in vertices {
            let id = geometry
                .vertices
                .values()
                .find(|vertex| vertex.position.distance(position) <= tolerance)
                .map(|vertex| vertex.id)
                .unwrap_or_else(|| {
                    let id = VertexId(staged.allocate_id());
                    geometry.vertices.insert(
                        id,
                        Vertex {
                            id,
                            position,
                            legacy_aliases: Vec::new(),
                        },
                    );
                    id
                });
            vertex_ids.push(id);
        }
        let mut triangle_ids = Vec::with_capacity(triangles.len());
        for [first, second, third] in triangles {
            let [first_id, second_id, third_id] =
                [vertex_ids[first], vertex_ids[second], vertex_ids[third]];
            let [Some(a), Some(b), Some(c)] = [first_id, second_id, third_id]
                .map(|id| geometry.vertices.get(&id).map(|vertex| vertex.position))
            else {
                return Err("triangle insertion references a missing vertex".to_owned());
            };
            if first_id == second_id
                || second_id == third_id
                || third_id == first_id
                || (b - a).cross(c - a).length() <= tolerance * tolerance
            {
                return Err("triangle insertion contains a degenerate face".to_owned());
            }
            let id = TriangleId(staged.allocate_id());
            geometry.triangles.insert(
                id,
                Triangle {
                    id,
                    vertices: [
                        VertexRef::new(first_id),
                        VertexRef::new(second_id),
                        VertexRef::new(third_id),
                    ],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
            triangle_ids.push(id);
        }
        geometry.validate()?;
        staged
            .definitions
            .get_mut(&definition_id)
            .expect("definition checked before face insertion")
            .geometry = geometry;
        staged.revision = staged.revision.saturating_add(1);
        *self = staged;
        Ok(triangle_ids)
    }

    /// Atomically replaces selected faces and returns the source-to-output identity remap.
    /// Untouched geometry and selected line segments retain their existing IDs.
    pub fn replace_selected_triangles(
        &mut self,
        definition_id: DefinitionId,
        sources: &[TriangleId],
        replacements: Vec<TriangleReplacement>,
    ) -> Result<TopologyRemap, String> {
        let mut staged = self.clone();
        let mut geometry = staged
            .definitions
            .get(&definition_id)
            .ok_or_else(|| format!("missing definition {}", definition_id.0))?
            .geometry
            .clone();
        let remap = staged.replace_triangles_in_bundle(&mut geometry, sources, replacements)?;
        staged
            .definitions
            .get_mut(&definition_id)
            .expect("definition checked before geometry replacement")
            .geometry = geometry;
        staged.revision = staged.revision.saturating_add(1);
        *self = staged;
        Ok(remap)
    }

    /// Atomically replaces selected faces in occurrence-local geometry and remaps their identities.
    pub fn replace_selected_occurrence_triangles(
        &mut self,
        occurrence: &OccurrenceRef,
        sources: &[TriangleId],
        replacements: Vec<TriangleReplacement>,
    ) -> Result<TopologyRemap, String> {
        let mut staged = self.clone();
        let mut edits = staged
            .occurrence_edits(occurrence)
            .cloned()
            .ok_or_else(|| "selected occurrence has no local geometry edits".to_owned())?;
        let mut geometry = edits
            .geometry_replacement
            .take()
            .ok_or_else(|| "selected occurrence has no replacement geometry".to_owned())?;
        let remap = staged.replace_triangles_in_bundle(&mut geometry, sources, replacements)?;
        let bundle_id_conflicts = staged
            .definitions
            .values()
            .any(|definition| definition.geometry.id == geometry.id)
            || staged.occurrence_edits.iter().any(|(other, edits)| {
                other != occurrence
                    && edits
                        .geometry_replacement
                        .as_ref()
                        .is_some_and(|other_geometry| other_geometry.id == geometry.id)
            });
        if bundle_id_conflicts {
            geometry.id = GeometryBundleId(staged.allocate_id());
        }
        edits.geometry_replacement = Some(geometry);
        if !staged.set_occurrence_edits(occurrence.clone(), edits) {
            return Err("could not store the replaced occurrence geometry".to_owned());
        }
        staged.validate()?;
        *self = staged;
        Ok(remap)
    }

    fn replace_triangles_in_bundle(
        &mut self,
        geometry: &mut GeometryBundle,
        sources: &[TriangleId],
        replacements: Vec<TriangleReplacement>,
    ) -> Result<TopologyRemap, String> {
        if sources.is_empty() {
            return Err("face replacement needs at least one selected source face".to_owned());
        }
        let source_set = sources.iter().copied().collect::<BTreeSet<_>>();
        if source_set.len() != sources.len() {
            return Err("face replacement contains duplicate source IDs".to_owned());
        }
        if replacements
            .iter()
            .any(|replacement| !source_set.contains(&replacement.source))
        {
            return Err("replacement face references an unselected source ID".to_owned());
        }
        let mut source_triangles = BTreeMap::new();
        for source in &source_set {
            let triangle = geometry
                .triangles
                .get(source)
                .cloned()
                .ok_or_else(|| format!("missing source triangle {}", source.0))?;
            source_triangles.insert(*source, triangle);
        }

        let scale_origin = geometry
            .vertices
            .values()
            .next()
            .map_or_else(|| replacements[0].vertices[0], |vertex| vertex.position);
        let coordinate_scale = replacements
            .iter()
            .flat_map(|replacement| replacement.vertices)
            .map(|point| (point - scale_origin).length())
            .chain(
                geometry
                    .vertices
                    .values()
                    .map(|vertex| (vertex.position - scale_origin).length()),
            )
            .fold(0.0, f64::max);
        let tolerance = crate::geometry::GeometryTolerance::default().linear_at(coordinate_scale);
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return Err("replacement geometry exceeds the supported coordinate range".to_owned());
        }
        for source in &source_set {
            geometry.triangles.remove(source);
        }

        let mut remap = TopologyRemap::default();
        for source in &source_set {
            remap.triangles.insert(*source, Vec::new());
        }
        let mut inherited_metadata = BTreeSet::new();
        for replacement in replacements {
            let [a, b, c] = replacement.vertices;
            if replacement.vertices.iter().any(|point| !point.is_finite())
                || (b - a).cross(c - a).length() <= tolerance * tolerance
            {
                return Err("replacement geometry contains a degenerate face".to_owned());
            }
            let mut references = [VertexRef::new(VertexId::default()); 3];
            for (index, point) in replacement.vertices.into_iter().enumerate() {
                let matching = geometry
                    .vertices
                    .values()
                    .filter_map(|vertex| {
                        let distance = vertex.position.distance(point);
                        (distance <= tolerance).then_some((distance, vertex.id, vertex.position))
                    })
                    .min_by(|left, right| left.0.total_cmp(&right.0));
                let (id, position) = if let Some((_, id, position)) = matching {
                    (id, position)
                } else {
                    let id = VertexId(self.allocate_id());
                    geometry.vertices.insert(
                        id,
                        Vertex {
                            id,
                            position: point,
                            legacy_aliases: Vec::new(),
                        },
                    );
                    (id, point)
                };
                references[index] = VertexRef {
                    id,
                    position_offset: point - position,
                };
            }
            if references[0].id == references[1].id
                || references[1].id == references[2].id
                || references[2].id == references[0].id
            {
                return Err("replacement face collapses to repeated vertices".to_owned());
            }
            let source_triangle = &source_triangles[&replacement.source];
            let inherit = inherited_metadata.insert(replacement.source);
            let id = TriangleId(self.allocate_id());
            geometry.triangles.insert(
                id,
                Triangle {
                    id,
                    vertices: references,
                    color: replacement.color,
                    legacy_aliases: if inherit {
                        source_triangle.legacy_aliases.clone()
                    } else {
                        Vec::new()
                    },
                    extensions: if inherit {
                        source_triangle.extensions.clone()
                    } else {
                        BTreeMap::new()
                    },
                },
            );
            remap
                .triangles
                .get_mut(&replacement.source)
                .unwrap()
                .push(id);
        }

        geometry.validate()?;
        Ok(remap)
    }

    /// Appends authored faces to the scene-root graph without creating line segments.
    pub fn append_triangles_to_scene_root(
        &mut self,
        vertices: Vec<Vec3>,
        triangles: Vec<[usize; 3]>,
    ) -> Result<Vec<TriangleId>, String> {
        let mut staged = self.clone();
        let definition_id = staged.ensure_scene_root();
        let triangle_ids =
            staged.append_triangles_to_definition(definition_id, vertices, triangles)?;
        *self = staged;
        Ok(triangle_ids)
    }

    /// Appends a colored indexed mesh, reusing graph vertices at matching local points.
    /// Segment entries pair endpoint indices with colors; triangle entries pair corner indices
    /// with colors. Per-corner source offsets are retained when matching existing vertices.
    pub fn append_colored_indexed_mesh_to_definition(
        &mut self,
        definition_id: DefinitionId,
        vertices: Vec<Vec3>,
        segments: Vec<([usize; 2], Rgba8)>,
        triangles: Vec<([usize; 3], Rgba8)>,
    ) -> Result<(Vec<SegmentId>, Vec<TriangleId>), String> {
        if segments.is_empty() && triangles.is_empty() {
            return Err("mesh insertion needs at least one edge or face".to_owned());
        }
        validate_indexed_geometry_input(
            &vertices,
            &segments.iter().map(|(edge, _)| *edge).collect::<Vec<_>>(),
            &triangles.iter().map(|(face, _)| *face).collect::<Vec<_>>(),
        )?;

        let mut staged = self.clone();
        let Some(definition) = staged.definitions.get(&definition_id) else {
            return Err(format!("missing definition {}", definition_id.0));
        };
        let mut geometry = definition.geometry.clone();
        let model_scale = vertices
            .iter()
            .map(|point| point.length())
            .chain(
                geometry
                    .vertices
                    .values()
                    .map(|vertex| vertex.position.length()),
            )
            .fold(0.0, f64::max);
        let tolerance = crate::geometry::GeometryTolerance::default().linear_at(model_scale);
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return Err("mesh coordinates exceed the supported range".to_owned());
        }

        let mut vertex_refs = Vec::with_capacity(vertices.len());
        for position in &vertices {
            let matching_ref = geometry
                .segments
                .values()
                .flat_map(|segment| segment.endpoints)
                .chain(
                    geometry
                        .triangles
                        .values()
                        .flat_map(|triangle| triangle.vertices),
                )
                .filter_map(|reference| {
                    let resolved = geometry.vertex_position(reference)?;
                    (resolved.distance(*position) <= tolerance)
                        .then_some((resolved.distance(*position), reference.id))
                })
                .min_by(|left, right| left.0.total_cmp(&right.0));
            if let Some((_, id)) = matching_ref {
                let base = geometry
                    .vertices
                    .get(&id)
                    .ok_or_else(|| "mesh insertion found a missing source vertex".to_owned())?;
                vertex_refs.push(VertexRef {
                    id,
                    position_offset: *position - base.position,
                });
            } else if let Some(vertex) = geometry
                .vertices
                .values()
                .filter(|vertex| vertex.position.distance(*position) <= tolerance)
                .min_by(|left, right| {
                    left.position
                        .distance(*position)
                        .total_cmp(&right.position.distance(*position))
                })
            {
                vertex_refs.push(VertexRef {
                    id: vertex.id,
                    position_offset: *position - vertex.position,
                });
            } else {
                let id = VertexId(staged.allocate_id());
                geometry.vertices.insert(
                    id,
                    Vertex {
                        id,
                        position: *position,
                        legacy_aliases: Vec::new(),
                    },
                );
                vertex_refs.push(VertexRef::new(id));
            }
        }

        let mut segment_ids = Vec::with_capacity(segments.len());
        for ([start, end], color) in segments {
            let [start_point, end_point] = [vertices[start], vertices[end]];
            if start_point.distance(end_point) <= tolerance {
                return Err("mesh insertion contains a degenerate edge".to_owned());
            }
            let existing = geometry.segments.values().find(|segment| {
                let [Some(a), Some(b)] = segment
                    .endpoints
                    .map(|reference| geometry.vertex_position(reference))
                else {
                    return false;
                };
                (a.distance(start_point) <= tolerance && b.distance(end_point) <= tolerance)
                    || (a.distance(end_point) <= tolerance && b.distance(start_point) <= tolerance)
            });
            if let Some(existing) = existing {
                segment_ids.push(existing.id);
                continue;
            }
            let id = SegmentId(staged.allocate_id());
            geometry.segments.insert(
                id,
                Segment {
                    id,
                    endpoints: [vertex_refs[start], vertex_refs[end]],
                    color,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
            segment_ids.push(id);
        }

        let mut triangle_ids = Vec::with_capacity(triangles.len());
        for ([first, second, third], color) in triangles {
            let [a, b, c] = [vertices[first], vertices[second], vertices[third]];
            if (b - a).cross(c - a).length() <= tolerance * tolerance {
                return Err("mesh insertion contains a degenerate face".to_owned());
            }
            let id = TriangleId(staged.allocate_id());
            geometry.triangles.insert(
                id,
                Triangle {
                    id,
                    vertices: [vertex_refs[first], vertex_refs[second], vertex_refs[third]],
                    color,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
            triangle_ids.push(id);
        }

        geometry.validate()?;
        staged
            .definitions
            .get_mut(&definition_id)
            .expect("definition was checked before mesh insertion")
            .geometry = geometry;
        staged.revision = staged.revision.saturating_add(1);
        *self = staged;
        Ok((segment_ids, triangle_ids))
    }

    /// Appends a colored indexed mesh to the scene-root graph.
    pub fn append_colored_indexed_mesh_to_scene_root(
        &mut self,
        vertices: Vec<Vec3>,
        segments: Vec<([usize; 2], Rgba8)>,
        triangles: Vec<([usize; 3], Rgba8)>,
    ) -> Result<(Vec<SegmentId>, Vec<TriangleId>), String> {
        let mut staged = self.clone();
        let definition_id = staged.ensure_scene_root();
        let inserted = staged.append_colored_indexed_mesh_to_definition(
            definition_id,
            vertices,
            segments,
            triangles,
        )?;
        *self = staged;
        Ok(inserted)
    }

    /// Reverses the winding of triangles in one definition as an atomic edit.
    pub fn reverse_triangle_winding_in_definition(
        &mut self,
        definition_id: DefinitionId,
        triangle_ids: &[TriangleId],
    ) -> Result<(), String> {
        if triangle_ids.is_empty() {
            return Ok(());
        }
        let mut staged = self.clone();
        let Some(definition) = staged.definitions.get(&definition_id) else {
            return Err(format!("missing definition {}", definition_id.0));
        };
        let mut geometry = definition.geometry.clone();
        for triangle_id in triangle_ids {
            let triangle = geometry
                .triangles
                .get_mut(triangle_id)
                .ok_or_else(|| format!("missing triangle {}", triangle_id.0))?;
            triangle.vertices.swap(1, 2);
        }
        geometry.validate()?;
        staged
            .definitions
            .get_mut(&definition_id)
            .expect("definition was checked before winding update")
            .geometry = geometry;
        staged.revision = staged.revision.saturating_add(1);
        *self = staged;
        Ok(())
    }

    /// Reverses triangle winding in the scene-root graph.
    pub fn reverse_triangle_winding_in_scene_root(
        &mut self,
        triangle_ids: &[TriangleId],
    ) -> Result<(), String> {
        let mut staged = self.clone();
        let definition_id = staged.ensure_scene_root();
        staged.reverse_triangle_winding_in_definition(definition_id, triangle_ids)?;
        *self = staged;
        Ok(())
    }

    /// Inserts a planar mesh directly into the scene-root graph.
    pub fn append_planar_mesh_to_scene_root(
        &mut self,
        vertices: Vec<Vec3>,
        segments: Vec<[usize; 2]>,
        triangles: Vec<[usize; 3]>,
    ) -> Result<(Vec<SegmentId>, Vec<TriangleId>), String> {
        validate_indexed_geometry_input(&vertices, &segments, &triangles)?;
        let mut staged = self.clone();
        let definition_id = staged.ensure_scene_root();
        let inserted = staged.append_planar_mesh_to_definition(
            definition_id,
            vertices,
            segments,
            triangles,
        )?;
        *self = staged;
        Ok(inserted)
    }

    fn append_geometry_to_definition(
        &mut self,
        definition_id: DefinitionId,
        mut addition: GeometryBundle,
    ) -> bool {
        let Some(definition) = self.definitions.get(&definition_id) else {
            return false;
        };
        if addition.validate().is_err()
            || addition
                .vertices
                .keys()
                .any(|id| definition.geometry.vertices.contains_key(id))
            || addition
                .segments
                .keys()
                .any(|id| definition.geometry.segments.contains_key(id))
            || addition
                .triangles
                .keys()
                .any(|id| definition.geometry.triangles.contains_key(id))
        {
            return false;
        }
        let Some(definition) = self.definitions.get(&definition_id) else {
            return false;
        };
        let mut merged = definition.geometry.clone();
        merged.vertices.append(&mut addition.vertices);
        merged.segments.append(&mut addition.segments);
        merged.triangles.append(&mut addition.triangles);
        if merged.validate().is_err() {
            return false;
        }
        self.definitions
            .get_mut(&definition_id)
            .expect("definition was checked above")
            .geometry = merged;
        self.revision = self.revision.saturating_add(1);
        true
    }

    /// Adds a filled, two-triangle rectangle on the ground plane using raw topology.
    pub fn add_ground_rectangle(
        &mut self,
        name: impl Into<String>,
        first: Vec3,
        opposite: Vec3,
    ) -> Option<OccurrenceRef> {
        if !first.is_finite() || !opposite.is_finite() {
            return None;
        }
        let min_x = first.x.min(opposite.x);
        let max_x = first.x.max(opposite.x);
        let min_z = first.z.min(opposite.z);
        let max_z = first.z.max(opposite.z);
        let scale = first.length().max(opposite.length());
        let minimum = crate::geometry::GeometryTolerance::default().linear_at(scale);
        if max_x - min_x <= minimum || max_z - min_z <= minimum {
            return None;
        }

        let mut geometry = GeometryBundle::default();
        let corners = [
            Vec3::new(min_x, 0.0, min_z),
            Vec3::new(min_x, 0.0, max_z),
            Vec3::new(max_x, 0.0, max_z),
            Vec3::new(max_x, 0.0, min_z),
        ];
        let mut vertex_ids = Vec::with_capacity(corners.len());
        for position in corners {
            let id = VertexId(self.allocate_id());
            geometry.vertices.insert(
                id,
                Vertex {
                    id,
                    position,
                    legacy_aliases: Vec::new(),
                },
            );
            vertex_ids.push(id);
        }
        for [a, b] in [[0, 1], [1, 2], [2, 3], [3, 0]] {
            let id = SegmentId(self.allocate_id());
            geometry.segments.insert(
                id,
                Segment {
                    id,
                    endpoints: [VertexRef::new(vertex_ids[a]), VertexRef::new(vertex_ids[b])],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
        }
        for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
            let id = TriangleId(self.allocate_id());
            geometry.triangles.insert(
                id,
                Triangle {
                    id,
                    vertices: [
                        VertexRef::new(vertex_ids[a]),
                        VertexRef::new(vertex_ids[b]),
                        VertexRef::new(vertex_ids[c]),
                    ],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
        }
        self.add_root_geometry(name, geometry, Affine3::IDENTITY)
    }

    /// Adds a filled rectangle from four ordered world-space corners.
    ///
    /// Corners must form a non-degenerate rectangle in any plane. This is used by camera-aligned
    /// construction views while `add_ground_rectangle` remains the explicit XZ-plane operation.
    pub fn add_planar_rectangle(
        &mut self,
        name: impl Into<String>,
        corners: [Vec3; 4],
    ) -> Option<OccurrenceRef> {
        if corners.iter().any(|point| !point.is_finite()) {
            return None;
        }
        let edge_u = corners[1] - corners[0];
        let edge_v = corners[3] - corners[0];
        let scale = corners
            .iter()
            .map(|point| point.length())
            .fold(0.0, f64::max);
        let tolerance = crate::geometry::GeometryTolerance::default().linear_at(scale);
        let edge_u_length = edge_u.length();
        let edge_v_length = edge_v.length();
        if edge_u_length <= tolerance
            || edge_v_length <= tolerance
            || edge_u.cross(edge_v).length() <= tolerance * tolerance
            || edge_u.dot(edge_v).abs() > tolerance * edge_u_length.max(edge_v_length)
            || corners[2].distance(corners[0] + edge_u + edge_v) > tolerance * 2.0
        {
            return None;
        }

        let mut geometry = GeometryBundle::default();
        let mut vertex_ids = Vec::with_capacity(corners.len());
        for position in corners {
            let id = VertexId(self.allocate_id());
            geometry.vertices.insert(
                id,
                Vertex {
                    id,
                    position,
                    legacy_aliases: Vec::new(),
                },
            );
            vertex_ids.push(id);
        }
        for [a, b] in [[0, 1], [1, 2], [2, 3], [3, 0]] {
            let id = SegmentId(self.allocate_id());
            geometry.segments.insert(
                id,
                Segment {
                    id,
                    endpoints: [VertexRef::new(vertex_ids[a]), VertexRef::new(vertex_ids[b])],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
        }
        for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
            let id = TriangleId(self.allocate_id());
            geometry.triangles.insert(
                id,
                Triangle {
                    id,
                    vertices: [
                        VertexRef::new(vertex_ids[a]),
                        VertexRef::new(vertex_ids[b]),
                        VertexRef::new(vertex_ids[c]),
                    ],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                    extensions: BTreeMap::new(),
                },
            );
        }
        self.add_root_geometry(name, geometry, Affine3::IDENTITY)
    }

    fn add_root_geometry(
        &mut self,
        name: impl Into<String>,
        mut geometry: GeometryBundle,
        transform: Affine3,
    ) -> Option<OccurrenceRef> {
        let definition_id = DefinitionId(self.allocate_id());
        geometry.id = GeometryBundleId(self.allocate_id());
        self.definitions.insert(
            definition_id,
            Definition {
                id: definition_id,
                name: name.into(),
                kind: DefinitionKind::Mesh,
                frame: Affine3::IDENTITY,
                geometry,
                default_child_layout: None,
                domain_data: BTreeMap::new(),
                extensions: BTreeMap::new(),
                legacy_aliases: Vec::new(),
            },
        );
        let entry_id = LayoutEntryId(self.allocate_id());
        self.child_layouts
            .get_mut(&self.root_layout)?
            .entries
            .push(LayoutEntry {
                id: entry_id,
                definition: definition_id,
                transform,
                legacy_aliases: Vec::new(),
            });
        self.revision = self.revision.saturating_add(1);
        Some(OccurrenceRef::root(entry_id))
    }

    /// Copies a root placement and its occurrence edits, preserving shared geometry.
    pub fn copy_root_occurrence(
        &mut self,
        source: &OccurrenceRef,
        offset: Vec3,
    ) -> Option<OccurrenceRef> {
        self.array_root_occurrence(source, 1, offset)
            .and_then(|copies| copies.into_iter().next())
    }

    /// Removes selected occurrences without changing shared child layouts.
    ///
    /// Root entries are removed from the unique root layout. For nested placements, the active
    /// child layout is copied onto the parent occurrence and its surviving entry IDs are remapped,
    /// so deleting children from one or more groups does not edit other instances of those groups.
    /// A selection cannot contain both a parent occurrence and one of its descendants.
    pub fn remove_selected_occurrences(
        &mut self,
        occurrences: &[OccurrenceRef],
    ) -> Result<usize, String> {
        let mut selected_paths = BTreeSet::new();
        let mut groups: BTreeMap<Vec<LayoutEntryId>, BTreeSet<LayoutEntryId>> = BTreeMap::new();
        for occurrence in occurrences {
            if occurrence.path.is_empty()
                || self.resolve_occurrence(occurrence).is_none()
                || !selected_paths.insert(occurrence.path.clone())
            {
                return Err("delete requires distinct, existing object selections".to_owned());
            }
            let parent_path = occurrence.path[..occurrence.path.len() - 1].to_vec();
            groups
                .entry(parent_path)
                .or_default()
                .insert(*occurrence.path.last().expect("non-empty path"));
        }
        if selected_paths.is_empty() {
            return Err("select one or more objects to delete".to_owned());
        }
        for path in &selected_paths {
            if selected_paths
                .iter()
                .any(|other| path.len() < other.len() && other.starts_with(path))
            {
                return Err("delete a parent or its descendants in separate operations".to_owned());
            }
        }

        let mut groups = groups.into_iter().collect::<Vec<_>>();
        groups.sort_by_key(|(parent, _)| std::cmp::Reverse(parent.len()));

        // Validate every target and child layout before applying any group, keeping failures
        // atomic even when the selection spans several parents.
        for (parent_path, selected_ids) in &groups {
            if parent_path.is_empty() {
                let root = self
                    .child_layouts
                    .get(&self.root_layout)
                    .ok_or_else(|| "document root layout is missing".to_owned())?;
                if selected_ids
                    .iter()
                    .any(|id| !root.entries.iter().any(|entry| entry.id == *id))
                {
                    return Err("a selected root object is no longer present".to_owned());
                }
                continue;
            }

            let parent = OccurrenceRef {
                path: parent_path.clone(),
            };
            let resolved_parent = self
                .resolve_occurrence(&parent)
                .ok_or_else(|| "the selected objects have an invalid parent".to_owned())?;
            let source_layout_id = resolved_parent
                .edits
                .and_then(|edits| edits.child_layout)
                .or(resolved_parent.definition.default_child_layout)
                .ok_or_else(|| "the selected parent has no child layout".to_owned())?;
            let source_layout = self
                .child_layouts
                .get(&source_layout_id)
                .ok_or_else(|| "the selected parent child layout is missing".to_owned())?;
            if selected_ids
                .iter()
                .any(|id| !source_layout.entries.iter().any(|entry| entry.id == *id))
            {
                return Err("a selected object is no longer present under its parent".to_owned());
            }
            for path in self.occurrence_edits.keys() {
                if path.path.starts_with(parent_path) && path.path.len() > parent_path.len() {
                    let child_id = path.path[parent_path.len()];
                    if !source_layout
                        .entries
                        .iter()
                        .any(|entry| entry.id == child_id)
                    {
                        return Err(
                            "an occurrence edit is outside the selected child layout".to_owned()
                        );
                    }
                }
            }
        }

        for (parent_path, selected_ids) in groups {
            if parent_path.is_empty() {
                let root = self
                    .child_layouts
                    .get_mut(&self.root_layout)
                    .expect("root layout was validated above");
                root.entries
                    .retain(|entry| !selected_ids.contains(&entry.id));
                self.occurrence_edits.retain(|path, _| {
                    path.path
                        .first()
                        .is_none_or(|entry| !selected_ids.contains(entry))
                });
                continue;
            }

            let parent = OccurrenceRef {
                path: parent_path.clone(),
            };
            let resolved_parent = self
                .resolve_occurrence(&parent)
                .expect("parent paths were validated above");
            let source_layout_id = resolved_parent
                .edits
                .and_then(|edits| edits.child_layout)
                .or(resolved_parent.definition.default_child_layout)
                .expect("parent child layouts were validated above");
            let source_layout = self.child_layouts[&source_layout_id].clone();
            let mut entry_remap = BTreeMap::new();
            let mut entries = Vec::with_capacity(source_layout.entries.len() - selected_ids.len());
            for mut entry in source_layout.entries {
                if selected_ids.contains(&entry.id) {
                    continue;
                }
                let old_id = entry.id;
                entry.id = LayoutEntryId(self.allocate_id());
                entry_remap.insert(old_id, entry.id);
                entries.push(entry);
            }
            let child_layout_id = ChildLayoutId(self.allocate_id());
            let mut remapped_edits = BTreeMap::new();
            for (path, edits) in &self.occurrence_edits {
                let mut path = path.clone();
                if path.path.starts_with(&parent_path) && path.path.len() > parent_path.len() {
                    let child_index = parent_path.len();
                    let child_id = path.path[child_index];
                    if selected_ids.contains(&child_id) {
                        continue;
                    }
                    path.path[child_index] = entry_remap[&child_id];
                }
                remapped_edits.insert(path, edits.clone());
            }
            let mut parent_edits = remapped_edits.get(&parent).cloned().unwrap_or_default();
            parent_edits.child_layout = Some(child_layout_id);
            remapped_edits.insert(parent, parent_edits);
            self.child_layouts.insert(
                child_layout_id,
                ChildLayout {
                    id: child_layout_id,
                    entries,
                    extensions: source_layout.extensions,
                },
            );
            self.occurrence_edits = remapped_edits;
        }

        self.revision = self.revision.saturating_add(1);
        Ok(selected_paths.len())
    }

    /// Adds `count` root placements using one deterministic translation step.
    pub fn array_root_occurrence(
        &mut self,
        source: &OccurrenceRef,
        count: usize,
        step: Vec3,
    ) -> Option<Vec<OccurrenceRef>> {
        if source.path.len() != 1 || count == 0 || count > 10_000 || !step.is_finite() {
            return None;
        }
        let source_id = source.path[0];
        let root = self.child_layouts.get(&self.root_layout)?;
        let source_entry = root
            .entries
            .iter()
            .find(|entry| entry.id == source_id)?
            .clone();
        let source_edits = self
            .occurrence_edits
            .iter()
            .filter(|(path, _)| path.path.first() == Some(&source_id))
            .map(|(path, edits)| (path.clone(), edits.clone()))
            .collect::<Vec<_>>();

        let mut copies = Vec::with_capacity(count);
        for index in 1..=count {
            let offset = step * index as f64;
            let mut entry = source_entry.clone();
            entry.id = LayoutEntryId(self.allocate_id());
            entry.transform.translation += offset;
            let id = entry.id;
            self.child_layouts
                .get_mut(&self.root_layout)?
                .entries
                .push(entry);
            let occurrence = OccurrenceRef::root(id);
            for (old_path, source_edits) in &source_edits {
                let mut edits = source_edits.clone();
                let mut new_path = old_path.clone();
                new_path.path[0] = id;
                if old_path.path.len() == 1
                    && let Some(transform) = edits.transform.as_mut()
                {
                    transform.translation += offset;
                }
                self.occurrence_edits.insert(new_path, edits);
            }
            copies.push(occurrence);
        }
        self.revision = self.revision.saturating_add(1);
        Some(copies)
    }

    /// Groups selected root occurrences in a shared child layout without moving their geometry.
    pub fn create_object_from_root_occurrences(
        &mut self,
        occurrences: &[OccurrenceRef],
        name: impl Into<String>,
    ) -> Option<OccurrenceRef> {
        if occurrences.is_empty() || occurrences.iter().any(|item| item.path.len() != 1) {
            return None;
        }
        let selected = occurrences
            .iter()
            .map(|item| item.path[0])
            .collect::<BTreeSet<_>>();
        if selected.len() != occurrences.len() {
            return None;
        }
        let root = self.child_layouts.get(&self.root_layout)?;
        let selected_entries = root
            .entries
            .iter()
            .filter(|entry| selected.contains(&entry.id))
            .cloned()
            .collect::<Vec<_>>();
        if selected_entries.len() != selected.len() {
            return None;
        }
        let origin = selected_entries
            .iter()
            .map(|entry| entry.transform.translation)
            .fold(Vec3::ZERO, |sum, position| sum + position)
            * (1.0 / selected_entries.len() as f64);
        if !origin.is_finite() {
            return None;
        }

        let mut remaps = BTreeMap::new();
        let mut layout = ChildLayout {
            id: ChildLayoutId(self.allocate_id()),
            entries: Vec::with_capacity(selected_entries.len()),
            extensions: BTreeMap::new(),
        };
        for entry in &selected_entries {
            let mut child = entry.clone();
            child.id = LayoutEntryId(self.allocate_id());
            child.transform.translation -= origin;
            remaps.insert(entry.id, child.id);
            layout.entries.push(child);
        }

        let layout_id = layout.id;
        self.child_layouts.insert(layout_id, layout);
        let definition_id = DefinitionId(self.allocate_id());
        let geometry_id = GeometryBundleId(self.allocate_id());
        self.definitions.insert(
            definition_id,
            Definition {
                id: definition_id,
                name: name.into(),
                kind: DefinitionKind::Hybrid,
                frame: Affine3::IDENTITY,
                geometry: GeometryBundle {
                    id: geometry_id,
                    ..GeometryBundle::default()
                },
                default_child_layout: Some(layout_id),
                domain_data: BTreeMap::new(),
                extensions: BTreeMap::new(),
                legacy_aliases: Vec::new(),
            },
        );
        let group_entry_id = LayoutEntryId(self.allocate_id());
        let group_entry = LayoutEntry {
            id: group_entry_id,
            definition: definition_id,
            transform: Affine3::from_translation(origin),
            legacy_aliases: Vec::new(),
        };

        let mut remapped_edits = BTreeMap::new();
        for (path, mut edits) in std::mem::take(&mut self.occurrence_edits) {
            let Some(old_root) = path.path.first().copied() else {
                remapped_edits.insert(path, edits);
                continue;
            };
            let Some(child_id) = remaps.get(&old_root).copied() else {
                remapped_edits.insert(path, edits);
                continue;
            };
            let mut new_path = vec![group_entry_id, child_id];
            new_path.extend(path.path.iter().skip(1).copied());
            if path.path.len() == 1
                && let Some(transform) = edits.transform.as_mut()
            {
                transform.translation -= origin;
            }
            remapped_edits.insert(OccurrenceRef { path: new_path }, edits);
        }
        self.occurrence_edits = remapped_edits;

        let root_entries = &mut self.child_layouts.get_mut(&self.root_layout)?.entries;
        root_entries.retain(|entry| !selected.contains(&entry.id));
        root_entries.push(group_entry);
        self.revision = self.revision.saturating_add(1);
        Some(OccurrenceRef::root(group_entry_id))
    }

    pub fn set_default_child_layout(
        &mut self,
        definition: DefinitionId,
        layout: Option<ChildLayoutId>,
    ) -> bool {
        if layout.is_some_and(|layout| !self.child_layouts.contains_key(&layout)) {
            return false;
        }
        let Some(current) = self.definitions.get(&definition) else {
            return false;
        };
        if current.default_child_layout == layout {
            return true;
        }
        let Some(definition) = self.definitions.get_mut(&definition) else {
            return false;
        };
        definition.default_child_layout = layout;
        self.revision = self.revision.saturating_add(1);
        true
    }

    pub fn set_occurrence_edits(
        &mut self,
        occurrence: OccurrenceRef,
        edits: InstanceEdits,
    ) -> bool {
        if self.resolve_occurrence(&occurrence).is_none()
            || edits
                .child_layout
                .is_some_and(|layout| !self.child_layouts.contains_key(&layout))
            || edits
                .transform
                .is_some_and(|transform| !transform.is_finite())
            || edits
                .hotspot_positions
                .values()
                .any(|position| !position.is_finite())
            || edits
                .geometry_replacement
                .as_ref()
                .is_some_and(|geometry| geometry.validate().is_err())
        {
            return false;
        }
        if self.occurrence_edits.get(&occurrence) == Some(&edits)
            || (edits.is_empty() && !self.occurrence_edits.contains_key(&occurrence))
        {
            return true;
        }
        if edits.is_empty() {
            self.occurrence_edits.remove(&occurrence);
        } else {
            self.occurrence_edits.insert(occurrence, edits);
        }
        self.revision = self.revision.saturating_add(1);
        true
    }

    pub fn resolve_occurrence<'a>(
        &'a self,
        occurrence: &OccurrenceRef,
    ) -> Option<ResolvedOccurrence<'a>> {
        if occurrence.path.is_empty() {
            let definition = self.scene_root_definition()?;
            return Some(ResolvedOccurrence {
                occurrence: occurrence.clone(),
                entry: None,
                definition,
                edits: None,
                world_transform: Affine3::IDENTITY,
            });
        }
        let (first, rest) = occurrence.path.split_first()?;
        let mut current_layout = self.root_layout;
        let mut world = Affine3::IDENTITY;
        let mut prefix = Vec::with_capacity(occurrence.path.len());
        let mut resolved = None;

        for entry_id in std::iter::once(first).chain(rest.iter()) {
            let layout = self.child_layouts.get(&current_layout)?;
            let entry = layout.entries.iter().find(|entry| entry.id == *entry_id)?;
            let definition = self.definitions.get(&entry.definition)?;
            prefix.push(*entry_id);
            let path = OccurrenceRef {
                path: prefix.clone(),
            };
            let edits = self.occurrence_edits.get(&path);
            let instance_frame = edits
                .and_then(|edits| edits.transform)
                .unwrap_or(entry.transform);
            world = world.compose(instance_frame).compose(definition.frame);
            resolved = Some(ResolvedOccurrence {
                occurrence: path,
                entry: Some(entry),
                definition,
                edits,
                world_transform: world,
            });
            if prefix.len() < occurrence.path.len() {
                current_layout = edits
                    .and_then(|edits| edits.child_layout)
                    .or(definition.default_child_layout)?;
            }
        }
        resolved
    }

    pub fn effective_geometry(&self, occurrence: &OccurrenceRef) -> Option<&GeometryBundle> {
        let resolved = self.resolve_occurrence(occurrence)?;
        Some(
            resolved
                .edits
                .and_then(|edits| edits.geometry_replacement.as_ref())
                .unwrap_or(&resolved.definition.geometry),
        )
    }

    /// Lists realized occurrences in stable layout order, rejecting recursive expansions.
    pub fn occurrence_refs(&self) -> Result<Vec<OccurrenceRef>, String> {
        fn visit(
            document: &Document,
            layout_id: ChildLayoutId,
            prefix: &[LayoutEntryId],
            active_definitions: &mut BTreeSet<DefinitionId>,
            output: &mut Vec<OccurrenceRef>,
        ) -> Result<(), String> {
            let layout = document
                .child_layouts
                .get(&layout_id)
                .ok_or_else(|| format!("missing child layout {}", layout_id.0))?;
            for entry in &layout.entries {
                if output.len() >= 100_000 {
                    return Err("document expands to more than 100000 placements".to_owned());
                }
                let mut path = prefix.to_vec();
                path.push(entry.id);
                let occurrence = OccurrenceRef { path };
                let resolved = document
                    .resolve_occurrence(&occurrence)
                    .ok_or_else(|| format!("unresolvable occurrence {:?}", occurrence.path))?;
                output.push(occurrence.clone());
                let Some(child_layout) = resolved
                    .edits
                    .and_then(|edits| edits.child_layout)
                    .or(resolved.definition.default_child_layout)
                else {
                    continue;
                };
                if !active_definitions.insert(resolved.definition.id) {
                    return Err(format!(
                        "definition child-layout cycle reaches {}",
                        resolved.definition.id.0
                    ));
                }
                visit(
                    document,
                    child_layout,
                    &occurrence.path,
                    active_definitions,
                    output,
                )?;
                active_definitions.remove(&resolved.definition.id);
            }
            Ok(())
        }

        let mut output = Vec::new();
        visit(
            self,
            self.root_layout,
            &[],
            &mut BTreeSet::new(),
            &mut output,
        )?;
        Ok(output)
    }

    /// Realized geometry owners, including the document root graph before placed objects.
    pub fn scene_geometry_refs(&self) -> Result<Vec<OccurrenceRef>, String> {
        let mut occurrences = Vec::new();
        if self.scene_root_definition().is_some() {
            occurrences.push(OccurrenceRef::scene_root());
        }
        occurrences.extend(self.occurrence_refs()?);
        Ok(occurrences)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.model_settings.validate()?;
        if !self.child_layouts.contains_key(&self.root_layout) {
            return Err("root child layout is missing".to_owned());
        }
        let mut entry_ids = BTreeSet::new();
        for (id, layout) in &self.child_layouts {
            if *id != layout.id {
                return Err(format!(
                    "child-layout map key {} differs from record id {}",
                    id.0, layout.id.0
                ));
            }
            for entry in &layout.entries {
                if !entry_ids.insert(entry.id) {
                    return Err(format!("layout entry id {} is reused", entry.id.0));
                }
                if !entry.transform.is_finite() {
                    return Err(format!(
                        "layout entry {} has a non-finite transform",
                        entry.id.0
                    ));
                }
                if !self.definitions.contains_key(&entry.definition) {
                    return Err(format!(
                        "layout entry {} references missing definition",
                        entry.id.0
                    ));
                }
            }
        }
        for (id, definition) in &self.definitions {
            if *id != definition.id {
                return Err(format!(
                    "definition map key {} differs from record id {}",
                    id.0, definition.id.0
                ));
            }
            if !definition.frame.is_finite() {
                return Err(format!("definition {} has a non-finite frame", id.0));
            }
            definition.geometry.validate()?;
            if definition
                .default_child_layout
                .is_some_and(|layout| !self.child_layouts.contains_key(&layout))
            {
                return Err(format!(
                    "definition {} references a missing child layout",
                    id.0
                ));
            }
        }
        for (occurrence, edits) in &self.occurrence_edits {
            if self.resolve_occurrence(occurrence).is_none() {
                return Err(format!(
                    "occurrence edit path {:?} cannot be resolved",
                    occurrence.path
                ));
            }
            if edits
                .transform
                .is_some_and(|transform| !transform.is_finite())
            {
                return Err(format!(
                    "occurrence edit {:?} has a non-finite transform",
                    occurrence.path
                ));
            }
            if edits
                .hotspot_positions
                .values()
                .any(|position| !position.is_finite())
            {
                return Err(format!(
                    "occurrence edit {:?} has a non-finite hotspot",
                    occurrence.path
                ));
            }
            if edits
                .child_layout
                .is_some_and(|layout| !self.child_layouts.contains_key(&layout))
            {
                return Err(format!(
                    "occurrence edit {:?} references a missing child layout",
                    occurrence.path
                ));
            }
            if let Some(geometry) = &edits.geometry_replacement {
                geometry.validate()?;
            }
        }
        self.validate_definition_acyclicity()?;
        Ok(())
    }

    pub(crate) fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn validate_definition_acyclicity(&self) -> Result<(), String> {
        fn visit(
            document: &Document,
            id: DefinitionId,
            active: &mut BTreeSet<DefinitionId>,
            done: &mut BTreeSet<DefinitionId>,
        ) -> Result<(), String> {
            if done.contains(&id) {
                return Ok(());
            }
            if !active.insert(id) {
                return Err(format!("definition child-layout cycle reaches {}", id.0));
            }
            let definition = document
                .definitions
                .get(&id)
                .ok_or_else(|| format!("missing definition {}", id.0))?;
            if let Some(layout_id) = definition.default_child_layout {
                let layout = document
                    .child_layouts
                    .get(&layout_id)
                    .ok_or_else(|| format!("missing child layout {}", layout_id.0))?;
                for entry in &layout.entries {
                    visit(document, entry.definition, active, done)?;
                }
            }
            active.remove(&id);
            done.insert(id);
            Ok(())
        }

        let mut active = BTreeSet::new();
        let mut done = BTreeSet::new();
        for id in self.definitions.keys().copied() {
            visit(self, id, &mut active, &mut done)?;
        }
        Ok(())
    }
}

fn validate_indexed_geometry_input(
    vertices: &[Vec3],
    segments: &[[usize; 2]],
    triangles: &[[usize; 3]],
) -> Result<(), String> {
    if vertices.is_empty()
        || vertices.iter().any(|point| !point.is_finite())
        || segments
            .iter()
            .flatten()
            .any(|index| *index >= vertices.len())
        || triangles
            .iter()
            .flatten()
            .any(|index| *index >= vertices.len())
    {
        return Err(
            "indexed geometry must contain finite vertices and valid references".to_owned(),
        );
    }
    Ok(())
}

mod occurrence_edit_map {
    use super::{InstanceEdits, OccurrenceRef};
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
    use std::collections::BTreeMap;

    pub fn serialize<S>(
        edits: &BTreeMap<OccurrenceRef, InstanceEdits>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        edits.iter().collect::<Vec<_>>().serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<BTreeMap<OccurrenceRef, InstanceEdits>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries = Vec::<(OccurrenceRef, InstanceEdits)>::deserialize(deserializer)?;
        let mut edits = BTreeMap::new();
        for (occurrence, edit) in entries {
            if edits.insert(occurrence.clone(), edit).is_some() {
                return Err(D::Error::custom(format!(
                    "duplicate occurrence edit for {:?}",
                    occurrence.path
                )));
            }
        }
        Ok(edits)
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct LinePiece {
    start: Vec3,
    end: Vec3,
    source: Option<SegmentId>,
    color: Rgba8,
    legacy_aliases: Vec<LegacyAlias>,
    extensions: BTreeMap<String, Value>,
    from_new_line: bool,
    break_start: bool,
    break_end: bool,
}

struct LineCluster {
    axis: Vec3,
    offset: Vec3,
    pieces: Vec<LinePiece>,
}

/// Inserts a line, splitting same-graph intersections and merging collinear touching intervals.
/// Triangle topology is deliberately unchanged; face cutting is an explicit operation.
fn insert_line_geometry(
    source: &GeometryBundle,
    start: Vec3,
    end: Vec3,
    next_id: &mut u64,
) -> Option<(GeometryBundle, LineInsertion)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let model_scale = start.length().max(end.length()).max(
        source
            .vertices
            .values()
            .map(|vertex| vertex.position.length())
            .fold(0.0, f64::max),
    );
    let epsilon = crate::geometry::LEGACY_LINE_STORE_EPSILON
        .max(crate::geometry::GeometryTolerance::default().linear_at(model_scale));
    if start.distance(end) <= epsilon {
        return None;
    }

    let mut pieces = Vec::with_capacity(source.segments.len() * 2 + 1);
    for segment in source.segments.values() {
        let [Some(a), Some(b)] = segment
            .endpoints
            .map(|reference| source.vertex_position(reference))
        else {
            return None;
        };
        let mut cuts = vec![(0.0, false), (1.0, false)];
        for hit in segment_pair_hits(start, end, a, b, epsilon) {
            if hit.3 {
                cuts.push((hit.1, true));
            }
        }
        sort_dedup_cuts(&mut cuts, epsilon / a.distance(b));
        for pair in cuts.windows(2) {
            if pair[1].0 - pair[0].0 <= epsilon / a.distance(b) {
                continue;
            }
            pieces.push(LinePiece {
                start: a + (b - a) * pair[0].0,
                end: a + (b - a) * pair[1].0,
                source: Some(segment.id),
                color: segment.color,
                legacy_aliases: if pair[0].0 == 0.0 {
                    segment.legacy_aliases.clone()
                } else {
                    Vec::new()
                },
                extensions: segment.extensions.clone(),
                from_new_line: false,
                break_start: pair[0].1,
                break_end: pair[1].1,
            });
        }
    }

    let mut new_cuts = vec![(0.0, false), (1.0, false)];
    for segment in source.segments.values() {
        let [Some(a), Some(b)] = segment
            .endpoints
            .map(|reference| source.vertex_position(reference))
        else {
            return None;
        };
        for hit in segment_pair_hits(start, end, a, b, epsilon) {
            if hit.3 {
                new_cuts.push((hit.0, true));
            }
        }
    }
    let new_length = start.distance(end);
    sort_dedup_cuts(&mut new_cuts, epsilon / new_length);
    for pair in new_cuts.windows(2) {
        if pair[1].0 - pair[0].0 <= epsilon / new_length {
            continue;
        }
        pieces.push(LinePiece {
            start: start + (end - start) * pair[0].0,
            end: start + (end - start) * pair[1].0,
            source: None,
            color: Rgba8::WHITE,
            legacy_aliases: Vec::new(),
            extensions: BTreeMap::new(),
            from_new_line: true,
            break_start: pair[0].1,
            break_end: pair[1].1,
        });
    }

    let mut clusters = Vec::<LineCluster>::new();
    for piece in pieces {
        let direction = piece.end - piece.start;
        let Some(mut axis) = direction.try_normalized(epsilon) else {
            continue;
        };
        if axis.x < -epsilon
            || (axis.x.abs() <= epsilon && axis.y < -epsilon)
            || (axis.x.abs() <= epsilon && axis.y.abs() <= epsilon && axis.z < 0.0)
        {
            axis = -axis;
        }
        let offset = piece.start - axis * piece.start.dot(axis);
        let cluster = clusters.iter_mut().find(|cluster| {
            cluster.axis.cross(axis).length() <= 1.0e-9
                && (cluster.offset - offset).length() <= epsilon
        });
        if let Some(cluster) = cluster {
            cluster.pieces.push(piece);
        } else {
            clusters.push(LineCluster {
                axis,
                offset,
                pieces: vec![piece],
            });
        }
    }

    let mut output = GeometryBundle {
        id: source.id,
        vertices: source.vertices.clone(),
        segments: BTreeMap::new(),
        triangles: source.triangles.clone(),
        extensions: source.extensions.clone(),
    };
    let mut touched = Vec::new();
    let mut insertion_remap = TopologyRemap::default();
    for cluster in clusters {
        let mut intervals = cluster
            .pieces
            .into_iter()
            .map(|piece| {
                let a = piece.start.dot(cluster.axis);
                let b = piece.end.dot(cluster.axis);
                let (low, high, break_low, break_high) = if a <= b {
                    (a, b, piece.break_start, piece.break_end)
                } else {
                    (b, a, piece.break_end, piece.break_start)
                };
                (low, high, break_low, break_high, piece)
            })
            .collect::<Vec<_>>();
        intervals.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        });
        let mut merged = Vec::<(f64, f64, bool, Vec<LinePiece>)>::new();
        for (low, high, break_low, break_high, piece) in intervals {
            if let Some((_, current_high, current_break_high, current_pieces)) = merged.last_mut()
                && (low < *current_high - epsilon
                    || (low <= *current_high + epsilon && !*current_break_high && !break_low))
            {
                if high > *current_high {
                    *current_high = high;
                    *current_break_high = break_high;
                }
                current_pieces.push(piece);
            } else {
                merged.push((low, high, break_high, vec![piece]));
            }
        }
        for (low, high, _break_high, members) in merged {
            let mut existing_ids = members
                .iter()
                .filter_map(|piece| piece.source)
                .collect::<Vec<_>>();
            existing_ids.sort_unstable();
            existing_ids.dedup();
            let retained_id = existing_ids
                .iter()
                .copied()
                .find(|id| !output.segments.contains_key(id));
            let id = retained_id.unwrap_or_else(|| allocate_id(next_id, SegmentId));
            let color_source = members
                .iter()
                .find(|piece| piece.source.is_some())
                .or_else(|| members.first())?;
            let aliases = members
                .iter()
                .find(|piece| !piece.legacy_aliases.is_empty())
                .map_or_else(Vec::new, |piece| piece.legacy_aliases.clone());
            let start_position = cluster.offset + cluster.axis * low;
            let end_position = cluster.offset + cluster.axis * high;
            let start_id = vertex_for_line_point(&mut output, start_position, epsilon, next_id);
            let end_id = vertex_for_line_point(&mut output, end_position, epsilon, next_id);
            output.segments.insert(
                id,
                Segment {
                    id,
                    endpoints: [VertexRef::new(start_id), VertexRef::new(end_id)],
                    color: color_source.color,
                    legacy_aliases: aliases,
                    extensions: color_source.extensions.clone(),
                },
            );
            for old_id in &existing_ids {
                insertion_remap
                    .segments
                    .entry(*old_id)
                    .or_default()
                    .push(id);
            }
            if members.iter().any(|piece| piece.from_new_line) {
                touched.push(id);
            }
        }
    }
    if output.validate().is_err() {
        return None;
    }
    touched.sort_unstable();
    touched.dedup();
    for targets in insertion_remap.segments.values_mut() {
        targets.sort_unstable();
        targets.dedup();
    }
    insertion_remap
        .segments
        .retain(|source, targets| targets.as_slice() != [*source]);
    (!touched.is_empty()).then_some((
        output,
        LineInsertion {
            affected_segments: touched,
            remap: insertion_remap,
        },
    ))
}

fn allocate_id<T>(next_id: &mut u64, wrap: impl FnOnce(u64) -> T) -> T {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);
    wrap(id)
}

fn sort_dedup_cuts(cuts: &mut Vec<(f64, bool)>, parameter_epsilon: f64) {
    cuts.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut deduplicated = Vec::<(f64, bool)>::with_capacity(cuts.len());
    for (parameter, split) in cuts.drain(..) {
        if let Some((previous, previous_split)) = deduplicated.last_mut()
            && (parameter - *previous).abs() <= parameter_epsilon
        {
            *previous = (*previous + parameter) * 0.5;
            *previous_split |= split;
        } else {
            deduplicated.push((parameter, split));
        }
    }
    *cuts = deduplicated;
}

fn vertex_for_line_point(
    geometry: &mut GeometryBundle,
    point: Vec3,
    epsilon: f64,
    next_id: &mut u64,
) -> VertexId {
    if let Some(vertex) = geometry
        .vertices
        .values()
        .filter(|vertex| vertex.position.distance(point) <= epsilon)
        .min_by_key(|vertex| vertex.id)
    {
        return vertex.id;
    }
    let id = allocate_id(next_id, VertexId);
    geometry.vertices.insert(
        id,
        Vertex {
            id,
            position: point,
            legacy_aliases: Vec::new(),
        },
    );
    id
}

/// Returns closest-point parameters and their midpoint when two 3D segments meet within tolerance.
fn segment_pair_hits(
    first_start: Vec3,
    first_end: Vec3,
    second_start: Vec3,
    second_end: Vec3,
    epsilon: f64,
) -> Vec<(f64, f64, Vec3, bool)> {
    let u = first_end - first_start;
    let v = second_end - second_start;
    let w = first_start - second_start;
    let a = u.dot(u);
    let b = u.dot(v);
    let c = v.dot(v);
    let d = u.dot(w);
    let e = v.dot(w);
    if a <= epsilon * epsilon || c <= epsilon * epsilon {
        return Vec::new();
    }
    let denominator = a * c - b * b;
    if denominator.abs() <= f64::EPSILON * a * c * 8.0 {
        if w.cross(u).length() / a.sqrt() > epsilon {
            return Vec::new();
        }
        let axis = u / a.sqrt();
        let second_low = (second_start - first_start).dot(axis);
        let second_high = (second_end - first_start).dot(axis);
        let low = 0.0_f64.max(second_low.min(second_high));
        let high = a.sqrt().min(second_low.max(second_high));
        if high < low - epsilon {
            return Vec::new();
        }
        let mut hits = Vec::<(f64, f64, Vec3, bool)>::new();
        for distance in [low, high] {
            let point = first_start + axis * distance.clamp(0.0, a.sqrt());
            let first_parameter = (distance / a.sqrt()).clamp(0.0, 1.0);
            let second_parameter = ((point - second_start).dot(v) / c).clamp(0.0, 1.0);
            if hits
                .iter()
                .all(|(t, _, _, _): &(f64, f64, Vec3, bool)| (*t - first_parameter).abs() > 1.0e-12)
            {
                hits.push((first_parameter, second_parameter, point, false));
            }
        }
        return hits;
    }
    let first_parameter = ((b * e - c * d) / denominator).clamp(0.0, 1.0);
    let second_parameter = ((a * e - b * d) / denominator).clamp(0.0, 1.0);
    let first_point = first_start + u * first_parameter;
    let second_point = second_start + v * second_parameter;
    (first_point.distance(second_point) <= epsilon)
        .then_some(vec![(
            first_parameter,
            second_parameter,
            (first_point + second_point) * 0.5,
            true,
        )])
        .unwrap_or_default()
}

pub struct ResolvedOccurrence<'a> {
    pub occurrence: OccurrenceRef,
    pub entry: Option<&'a LayoutEntry>,
    pub definition: &'a Definition,
    pub edits: Option<&'a InstanceEdits>,
    pub world_transform: Affine3,
}

#[cfg(test)]
mod tests {
    use super::{
        DefinitionKind, Document, GeometryBundle, InstanceEdits, ModelSettings, OccurrenceRef,
        RawGeometryTriangle, RawGeometryVertex, RawVertexRef, Rgba8, SegmentId, TopologyRemap,
        Triangle, TriangleId, TriangleReplacement, Vertex, VertexId, VertexRef,
    };
    use crate::document::derived::DerivedIndexCache;
    use crate::geometry::{Affine3, Vec3};
    use std::collections::BTreeMap;

    fn triangle_bundle(id: u64, base_x: f64) -> GeometryBundle {
        let mut bundle = GeometryBundle {
            id: super::GeometryBundleId(id),
            ..GeometryBundle::default()
        };
        for (offset, position) in [
            Vec3::new(base_x, 0.0, 0.0),
            Vec3::new(base_x + 1.0, 0.0, 0.0),
            Vec3::new(base_x, 1.0, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            let id = VertexId(id + offset as u64 + 1);
            bundle.vertices.insert(
                id,
                Vertex {
                    id,
                    position,
                    legacy_aliases: Vec::new(),
                },
            );
        }
        let triangle_id = TriangleId(id + 4);
        bundle.triangles.insert(
            triangle_id,
            Triangle {
                id: triangle_id,
                vertices: [
                    VertexRef::new(VertexId(id + 1)),
                    VertexRef::new(VertexId(id + 2)),
                    VertexRef::new(VertexId(id + 3)),
                ],
                color: Rgba8::WHITE,
                legacy_aliases: Vec::new(),
                extensions: BTreeMap::new(),
            },
        );
        bundle
    }

    #[test]
    fn selected_triangle_replacement_is_atomic_and_returns_colored_identity_remap() {
        let mut document = Document::new();
        let definition = document.add_definition("face", DefinitionKind::Mesh);
        document.definitions.get_mut(&definition).unwrap().geometry = triangle_bundle(200, 0.0);
        let source = TriangleId(204);
        let before_revision = document.revision();
        let remap = document
            .replace_selected_triangles(
                definition,
                &[source],
                vec![
                    TriangleReplacement {
                        source,
                        vertices: [
                            Vec3::new(0.0, 0.0, 0.0),
                            Vec3::new(1.0, 0.0, 0.0),
                            Vec3::new(0.0, 0.0, 0.5),
                        ],
                        color: Rgba8 {
                            red: 12,
                            green: 34,
                            blue: 56,
                            alpha: 255,
                        },
                    },
                    TriangleReplacement {
                        source,
                        vertices: [
                            Vec3::new(1.0, 0.0, 0.0),
                            Vec3::new(0.0, 1.0, 0.0),
                            Vec3::new(0.0, 0.0, 0.5),
                        ],
                        color: Rgba8 {
                            red: 12,
                            green: 34,
                            blue: 56,
                            alpha: 255,
                        },
                    },
                ],
            )
            .unwrap();

        let outputs = &remap.triangles[&source];
        assert_eq!(2, outputs.len());
        assert!(!outputs.contains(&source));
        assert_eq!(before_revision + 1, document.revision());
        let geometry = &document.definition(definition).unwrap().geometry;
        assert!(!geometry.triangles.contains_key(&source));
        assert!(
            outputs
                .iter()
                .all(|id| geometry.triangles[id].color.red == 12)
        );
        assert!(document.validate().is_ok());

        let before_failure = document.clone();
        assert!(
            document
                .replace_selected_triangles(
                    definition,
                    outputs,
                    vec![TriangleReplacement {
                        source: outputs[0],
                        vertices: [Vec3::ZERO, Vec3::X, Vec3::X * 2.0],
                        color: Rgba8::WHITE,
                    }],
                )
                .is_err()
        );
        assert_eq!(before_failure, document);
    }

    #[test]
    fn occurrence_triangle_replacement_changes_only_the_local_geometry() {
        let mut document = Document::new();
        let definition = document.add_definition("face", DefinitionKind::Mesh);
        document.definitions.get_mut(&definition).unwrap().geometry = triangle_bundle(200, 0.0);
        let entry = document
            .add_layout_entry(document.root_layout(), definition, Affine3::IDENTITY)
            .unwrap();
        let occurrence = OccurrenceRef::root(entry);
        let source = TriangleId(304);
        assert!(document.set_occurrence_edits(
            occurrence.clone(),
            InstanceEdits {
                geometry_replacement: Some(triangle_bundle(300, 20.0)),
                ..InstanceEdits::default()
            }
        ));
        let before_revision = document.revision();
        let remap = document
            .replace_selected_occurrence_triangles(
                &occurrence,
                &[source],
                vec![
                    TriangleReplacement {
                        source,
                        vertices: [
                            Vec3::new(20.0, 0.0, 0.0),
                            Vec3::new(20.5, 0.0, 0.0),
                            Vec3::new(20.0, 0.5, 0.0),
                        ],
                        color: Rgba8::WHITE,
                    },
                    TriangleReplacement {
                        source,
                        vertices: [
                            Vec3::new(20.5, 0.0, 0.0),
                            Vec3::new(21.0, 0.0, 0.0),
                            Vec3::new(20.0, 1.0, 0.0),
                        ],
                        color: Rgba8::WHITE,
                    },
                ],
            )
            .unwrap();

        assert_eq!(before_revision + 1, document.revision());
        assert_eq!(2, remap.triangles[&source].len());
        assert!(
            document
                .definition(definition)
                .unwrap()
                .geometry
                .triangles
                .contains_key(&TriangleId(204))
        );
        let local = document.effective_geometry(&occurrence).unwrap();
        assert!(!local.triangles.contains_key(&source));
        assert_ne!(
            document.definition(definition).unwrap().geometry.id,
            local.id
        );
        assert!(
            remap.triangles[&source]
                .iter()
                .all(|id| local.triangles.contains_key(id))
        );
        assert!(document.validate().is_ok());
        let mut derived = DerivedIndexCache::default();
        assert!(derived.index(&document).is_ok());

        let before_failure = document.clone();
        assert!(
            document
                .replace_selected_occurrence_triangles(
                    &occurrence,
                    &remap.triangles[&source],
                    vec![TriangleReplacement {
                        source: remap.triangles[&source][0],
                        vertices: [Vec3::ZERO, Vec3::X, Vec3::X * 2.0],
                        color: Rgba8::WHITE,
                    }],
                )
                .is_err()
        );
        assert_eq!(before_failure, document);
    }

    #[test]
    fn placements_share_definition_geometry_until_an_occurrence_is_edited() {
        let mut document = Document::new();
        let definition = document.add_definition("cuboid", DefinitionKind::Mesh);
        document.definitions.get_mut(&definition).unwrap().geometry = triangle_bundle(200, 0.0);
        let first = document
            .add_layout_entry(document.root_layout(), definition, Affine3::IDENTITY)
            .unwrap();
        let second = document
            .add_layout_entry(
                document.root_layout(),
                definition,
                Affine3::from_translation(Vec3::new(4.0, 0.0, 0.0)),
            )
            .unwrap();
        let first_ref = OccurrenceRef::root(first);
        let second_ref = OccurrenceRef::root(second);

        let shared = document.effective_geometry(&first_ref).unwrap();
        assert!(std::ptr::eq(
            shared,
            document.effective_geometry(&second_ref).unwrap()
        ));
        assert_eq!(
            document
                .resolve_occurrence(&second_ref)
                .unwrap()
                .world_transform
                .translation
                .x,
            4.0
        );

        let edits = InstanceEdits {
            geometry_replacement: Some(triangle_bundle(300, 20.0)),
            ..InstanceEdits::default()
        };
        assert!(document.set_occurrence_edits(second_ref.clone(), edits));
        assert!(!std::ptr::eq(
            document.effective_geometry(&first_ref).unwrap(),
            document.effective_geometry(&second_ref).unwrap()
        ));
        assert_eq!(
            document.effective_geometry(&first_ref).unwrap().vertices[&VertexId(201)]
                .position
                .x,
            0.0
        );
        assert_eq!(
            document.effective_geometry(&second_ref).unwrap().vertices[&VertexId(301)]
                .position
                .x,
            20.0
        );

        assert!(document.set_occurrence_edits(second_ref.clone(), InstanceEdits::default()));
        assert!(std::ptr::eq(
            document.effective_geometry(&first_ref).unwrap(),
            document.effective_geometry(&second_ref).unwrap()
        ));
        assert!(document.validate().is_ok());
    }

    #[test]
    fn explicit_empty_replacement_is_distinct_from_inheritance() {
        let mut document = Document::new();
        let definition = document.add_definition("mesh", DefinitionKind::Mesh);
        document.definitions.get_mut(&definition).unwrap().geometry = triangle_bundle(200, 0.0);
        let entry = document
            .add_layout_entry(document.root_layout(), definition, Affine3::IDENTITY)
            .unwrap();
        let occurrence = OccurrenceRef::root(entry);
        let edits = InstanceEdits {
            geometry_replacement: Some(GeometryBundle::default()),
            ..InstanceEdits::default()
        };
        assert!(document.set_occurrence_edits(occurrence.clone(), edits));
        assert!(document.effective_geometry(&occurrence).unwrap().is_empty());
        assert!(
            document.occurrence_edits[&occurrence]
                .geometry_replacement
                .is_some()
        );
        assert!(document.set_occurrence_edits(occurrence.clone(), InstanceEdits::default()));
        assert!(!document.effective_geometry(&occurrence).unwrap().is_empty());
    }

    #[test]
    fn shared_child_layout_keeps_per_occurrence_edits_and_layout_variants_separate() {
        let mut document = Document::new();
        let parent = document.add_definition("parent", DefinitionKind::Mesh);
        let child = document.add_definition("child", DefinitionKind::Mesh);
        document.definitions.get_mut(&child).unwrap().geometry = triangle_bundle(400, 0.0);
        let shared_layout = document.add_child_layout();
        let child_entry = document
            .add_layout_entry(shared_layout, child, Affine3::IDENTITY)
            .unwrap();
        assert!(document.set_default_child_layout(parent, Some(shared_layout)));
        let first_parent = document
            .add_layout_entry(document.root_layout(), parent, Affine3::IDENTITY)
            .unwrap();
        let second_parent = document
            .add_layout_entry(
                document.root_layout(),
                parent,
                Affine3::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            )
            .unwrap();
        let first_child = OccurrenceRef::root(first_parent).child(child_entry);
        let second_child = OccurrenceRef::root(second_parent).child(child_entry);
        let edits = InstanceEdits {
            transform: Some(Affine3::from_translation(Vec3::new(0.0, 2.0, 0.0))),
            ..InstanceEdits::default()
        };
        assert!(document.set_occurrence_edits(first_child.clone(), edits));

        let first_world = document
            .resolve_occurrence(&first_child)
            .unwrap()
            .world_transform;
        let second_world = document
            .resolve_occurrence(&second_child)
            .unwrap()
            .world_transform;
        assert_eq!(first_world.translation, Vec3::new(0.0, 2.0, 0.0));
        assert_eq!(second_world.translation, Vec3::new(10.0, 0.0, 0.0));
        assert!(
            document
                .resolve_occurrence(&second_child)
                .unwrap()
                .edits
                .is_none()
        );

        let alternate_layout = document.add_child_layout();
        let alternate_child = document
            .add_layout_entry(
                alternate_layout,
                child,
                Affine3::from_translation(Vec3::new(0.0, 5.0, 0.0)),
            )
            .unwrap();
        let variant = InstanceEdits {
            child_layout: Some(alternate_layout),
            ..InstanceEdits::default()
        };
        assert!(document.set_occurrence_edits(OccurrenceRef::root(second_parent), variant));
        let variant_ref = OccurrenceRef::root(second_parent).child(alternate_child);
        assert_eq!(
            document
                .resolve_occurrence(&variant_ref)
                .unwrap()
                .world_transform
                .translation,
            Vec3::new(10.0, 5.0, 0.0)
        );
        document.definitions.get_mut(&child).unwrap().geometry = triangle_bundle(500, 2.0);
        let first_child_after_edit = document.effective_geometry(&first_child).unwrap();
        let variant_child_after_edit = document.effective_geometry(&variant_ref).unwrap();
        assert!(std::ptr::eq(
            first_child_after_edit,
            variant_child_after_edit
        ));
        assert_eq!(
            2.0,
            first_child_after_edit.vertices[&VertexId(501)].position.x
        );
        assert_eq!(
            Vec3::new(0.0, 2.0, 0.0),
            document
                .resolve_occurrence(&first_child)
                .unwrap()
                .world_transform
                .translation
        );
        assert_eq!(
            Vec3::new(10.0, 5.0, 0.0),
            document
                .resolve_occurrence(&variant_ref)
                .unwrap()
                .world_transform
                .translation
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn nonidentity_definition_frame_survives_world_transform_resolution() {
        let mut document = Document::new();
        let definition = document.add_definition("offset local frame", DefinitionKind::Mesh);
        document.definitions.get_mut(&definition).unwrap().frame =
            Affine3::from_translation(Vec3::new(0.0, 0.0, 3.0));
        let entry = document
            .add_layout_entry(
                document.root_layout(),
                definition,
                Affine3::from_translation(Vec3::new(2.0, 0.0, 0.0)),
            )
            .unwrap();
        let world = document
            .resolve_occurrence(&OccurrenceRef::root(entry))
            .unwrap()
            .world_transform;
        assert_eq!(world.translation, Vec3::new(2.0, 0.0, 3.0));
    }

    #[test]
    fn serialized_document_round_trips_shared_identity_and_empty_override() {
        let mut document = Document::new();
        let definition = document.add_definition("mesh", DefinitionKind::Hybrid);
        document.definitions.get_mut(&definition).unwrap().geometry = triangle_bundle(200, 0.0);
        let first = document
            .add_layout_entry(document.root_layout(), definition, Affine3::IDENTITY)
            .unwrap();
        let edits = InstanceEdits {
            geometry_replacement: Some(GeometryBundle::default()),
            ..InstanceEdits::default()
        };
        assert!(document.set_occurrence_edits(OccurrenceRef::root(first), edits));
        document
            .definitions
            .get_mut(&definition)
            .unwrap()
            .domain_data
            .insert(
                "future_component".into(),
                serde_json::json!({"unknown": [1, 2, 3]}),
            );
        let encoded = serde_json::to_vec(&document).unwrap();
        let decoded: Document = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, document);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn model_settings_validate_and_increment_document_revision_only_on_change() {
        let mut document = Document::new();
        assert_eq!("unit", document.model_settings.unit_name);
        assert_eq!(24, document.model_settings.circle_segments);
        assert!(document.model_settings.validate().is_ok());
        let revision = document.revision;
        assert!(
            !document
                .update_model_settings(document.model_settings.clone())
                .unwrap()
        );
        let mut settings = ModelSettings {
            unit_name: "meter".to_owned(),
            ..document.model_settings.clone()
        };
        assert!(document.update_model_settings(settings.clone()).unwrap());
        assert_eq!(revision + 1, document.revision);
        assert!(!document.update_model_settings(settings.clone()).unwrap());
        settings.grid_size = 0.0;
        assert!(document.update_model_settings(settings).is_err());
        assert_eq!("meter", document.model_settings.unit_name);
    }

    #[test]
    fn changing_display_units_does_not_rescale_authored_geometry() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("unit probe", Vec3::new(2.0, 3.0, 4.0), Vec3::ZERO)
            .unwrap();
        let original = document
            .effective_geometry(&occurrence)
            .unwrap()
            .vertices
            .clone();

        let mut settings = document.model_settings.clone();
        settings.unit_name = "mm".to_owned();
        settings.unit_size = 0.001;
        document.update_model_settings(settings).unwrap();

        assert_eq!(
            original,
            document.effective_geometry(&occurrence).unwrap().vertices
        );
        assert_eq!(
            Some(0.002),
            document
                .model_settings
                .unit_scale()
                .and_then(|scale| scale.displayed_length(2.0))
        );
    }

    #[test]
    fn older_serialized_documents_receive_default_model_settings() {
        let mut value = serde_json::to_value(Document::new()).unwrap();
        value.as_object_mut().unwrap().remove("model_settings");
        let document: Document = serde_json::from_value(value).unwrap();
        assert_eq!(ModelSettings::default(), document.model_settings);
    }

    #[test]
    fn older_indexed_geometry_refs_default_to_zero_corner_offsets() {
        let mut value = serde_json::to_value(triangle_bundle(200, 0.0)).unwrap();
        value["triangles"]["204"]["vertices"] = serde_json::json!([201, 202, 203]);
        let geometry: GeometryBundle = serde_json::from_value(value).unwrap();
        let triangle = &geometry.triangles[&TriangleId(204)];
        assert_eq!(
            [VertexId(201), VertexId(202), VertexId(203)],
            triangle.vertices.map(|reference| reference.id)
        );
        assert!(
            triangle
                .vertices
                .iter()
                .all(|reference| reference.position_offset == Vec3::ZERO)
        );
    }

    #[test]
    fn topology_remaps_distinguish_unchanged_deleted_and_split_identity() {
        let remap = TopologyRemap {
            vertices: BTreeMap::from([
                (VertexId(10), vec![VertexId(10), VertexId(11)]),
                (VertexId(12), Vec::new()),
            ]),
            segments: BTreeMap::from([(SegmentId(20), Vec::new())]),
            triangles: BTreeMap::from([(TriangleId(30), vec![TriangleId(31)])]),
        };

        assert_eq!(
            &[VertexId(10), VertexId(11)],
            remap.vertex_targets(VertexId(10)).as_ref()
        );
        assert_eq!(
            &[] as &[VertexId],
            remap.vertex_targets(VertexId(12)).as_ref()
        );
        assert_eq!(&[VertexId(13)], remap.vertex_targets(VertexId(13)).as_ref());
        assert_eq!(
            &[] as &[SegmentId],
            remap.segment_targets(SegmentId(20)).as_ref()
        );
        assert_eq!(
            &[TriangleId(31)],
            remap.triangle_targets(TriangleId(30)).as_ref()
        );
        let round_trip: TopologyRemap =
            serde_json::from_value(serde_json::to_value(&remap).unwrap()).unwrap();
        assert_eq!(remap, round_trip);
    }

    #[test]
    fn cuboid_copy_object_and_array_keep_geometry_shared_and_world_positions() {
        let mut document = Document::new();
        let original = document
            .add_cuboid("cuboid", Vec3::new(2.0, 3.0, 4.0), Vec3::ZERO)
            .unwrap();
        let copied = document
            .copy_root_occurrence(&original, Vec3::new(5.0, 0.0, 0.0))
            .unwrap();
        let object = document
            .create_object_from_root_occurrences(&[copied], "cuboid object")
            .unwrap();
        let array = document
            .array_root_occurrence(&object, 12, Vec3::new(4.0, 0.0, 0.0))
            .unwrap();

        let original_geometry = document.effective_geometry(&original).unwrap();
        let object_definition = document.resolve_occurrence(&object).unwrap().definition.id;
        let object_layout = document.definitions[&object_definition]
            .default_child_layout
            .unwrap();
        let object_child = document.child_layouts[&object_layout].entries[0].id;
        let grouped_cuboid = object.child(object_child);
        assert!(std::ptr::eq(
            original_geometry,
            document.effective_geometry(&grouped_cuboid).unwrap()
        ));
        assert_eq!(
            document
                .effective_geometry(&original)
                .unwrap()
                .vertices
                .len(),
            8
        );
        assert_eq!(
            document
                .effective_geometry(&original)
                .unwrap()
                .segments
                .len(),
            12
        );
        assert_eq!(
            document
                .effective_geometry(&original)
                .unwrap()
                .triangles
                .len(),
            12
        );
        assert_eq!(
            document
                .resolve_occurrence(&grouped_cuboid)
                .unwrap()
                .world_transform
                .translation
                .x,
            5.0
        );
        assert_eq!(array.len(), 12);
        assert_eq!(
            document
                .resolve_occurrence(&array[0])
                .unwrap()
                .world_transform
                .translation
                .x,
            9.0
        );
        assert_eq!(
            document
                .resolve_occurrence(&array[11])
                .unwrap()
                .world_transform
                .translation
                .x,
            53.0
        );
        assert_eq!(
            document.child_layouts[&document.root_layout].entries.len(),
            14
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn line_tool_geometry_is_an_open_segment_and_rejects_zero_length() {
        let mut document = Document::new();
        assert!(
            document
                .add_line_segment("line", Vec3::ZERO, Vec3::ZERO)
                .is_none()
        );
        let line = document
            .add_line_segment("line", Vec3::new(-2.0, 1.0, 4.0), Vec3::new(3.0, 2.0, -1.0))
            .unwrap();
        let geometry = document.effective_geometry(&line).unwrap();
        assert_eq!(geometry.vertices.len(), 2);
        assert_eq!(geometry.segments.len(), 1);
        assert!(geometry.triangles.is_empty());
        assert!(document.validate().is_ok());
    }

    #[test]
    fn rectangle_tool_creates_a_ground_plane_face_with_consistent_winding() {
        let mut document = Document::new();
        assert!(
            document
                .add_ground_rectangle("rectangle", Vec3::ZERO, Vec3::new(1.0e-12, 0.0, 2.0),)
                .is_none()
        );
        let rectangle = document
            .add_ground_rectangle(
                "rectangle",
                Vec3::new(3.0, 8.0, -2.0),
                Vec3::new(-1.0, -5.0, 4.0),
            )
            .unwrap();
        let geometry = document.effective_geometry(&rectangle).unwrap();
        assert_eq!(geometry.vertices.len(), 4);
        assert_eq!(geometry.segments.len(), 4);
        assert_eq!(geometry.triangles.len(), 2);
        for triangle in geometry.triangles.values() {
            let [a, b, c] = triangle
                .vertices
                .map(|reference| geometry.vertex_position(reference).unwrap());
            assert!((b - a).cross(c - a).y > 0.0);
        }
        assert!(
            geometry
                .vertices
                .values()
                .all(|vertex| vertex.position.y == 0.0)
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn planar_rectangle_creates_consistent_faces_in_vertical_views() {
        let mut document = Document::new();
        assert!(
            document
                .add_planar_rectangle(
                    "invalid",
                    [
                        Vec3::ZERO,
                        Vec3::new(1.0, 0.0, 0.0),
                        Vec3::new(1.0, 1.0, 0.0),
                        Vec3::new(0.0, 2.0, 0.0),
                    ],
                )
                .is_none()
        );
        let rectangle = document
            .add_planar_rectangle(
                "front rectangle",
                [
                    Vec3::new(1.0, 2.0, 0.0),
                    Vec3::new(4.0, 2.0, 0.0),
                    Vec3::new(4.0, 5.0, 0.0),
                    Vec3::new(1.0, 5.0, 0.0),
                ],
            )
            .unwrap();

        let geometry = document.effective_geometry(&rectangle).unwrap();
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(4, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
        for triangle in geometry.triangles.values() {
            let [a, b, c] = triangle
                .vertices
                .map(|reference| geometry.vertex_position(reference).unwrap());
            assert!((b - a).cross(c - a).z > 0.0);
        }
        assert!(document.validate().is_ok());
    }

    #[test]
    fn appended_planar_geometry_edits_a_definition_without_adding_root_instances() {
        let mut document = Document::new();
        let instance = document
            .add_cuboid("editable", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let unrelated = document
            .add_cuboid(
                "unrelated",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(10.0, 0.0, 0.0),
            )
            .unwrap();
        let definition = document
            .resolve_occurrence(&instance)
            .unwrap()
            .definition
            .id;
        let unrelated_counts = {
            let geometry = document.effective_geometry(&unrelated).unwrap();
            (geometry.segments.len(), geometry.triangles.len())
        };
        let root_count = document.occurrence_refs().unwrap().len();
        let initial_revision = document.revision();

        let line = document
            .append_line_segment_to_definition(
                definition,
                Vec3::new(0.0, 3.0, 0.0),
                Vec3::new(2.0, 3.0, 0.0),
            )
            .unwrap();
        let (segments, triangles) = document
            .append_planar_rectangle_to_definition(
                definition,
                [
                    Vec3::new(0.0, 4.0, 0.0),
                    Vec3::new(2.0, 4.0, 0.0),
                    Vec3::new(2.0, 6.0, 0.0),
                    Vec3::new(0.0, 6.0, 0.0),
                ],
            )
            .unwrap();

        let geometry = document.effective_geometry(&instance).unwrap();
        assert!(geometry.segments.contains_key(&line));
        assert!(segments.iter().all(|id| geometry.segments.contains_key(id)));
        assert!(
            triangles
                .iter()
                .all(|id| geometry.triangles.contains_key(id))
        );
        assert_eq!(17, geometry.segments.len());
        assert_eq!(14, geometry.triangles.len());
        let unrelated_geometry = document.effective_geometry(&unrelated).unwrap();
        assert_eq!(unrelated_counts.0, unrelated_geometry.segments.len());
        assert_eq!(unrelated_counts.1, unrelated_geometry.triangles.len());
        assert_eq!(root_count, document.occurrence_refs().unwrap().len());
        assert_eq!(initial_revision + 2, document.revision());
        assert!(document.validate().is_ok());
    }

    #[test]
    fn scene_root_lines_are_entities_and_split_crossings_without_cutting_faces() {
        let mut document = Document::new();
        let first = document
            .append_line_segment_to_scene_root(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0))
            .unwrap();
        assert_eq!(1, first.affected_segments.len());
        assert!(first.remap.segments.is_empty());
        let original = first.affected_segments[0];

        let crossing = document
            .append_line_segment_to_scene_root(Vec3::new(0.0, -2.0, 0.0), Vec3::new(0.0, 2.0, 0.0))
            .unwrap();
        let root = OccurrenceRef::scene_root();
        let geometry = document.effective_geometry(&root).unwrap();
        assert_eq!(4, geometry.segments.len());
        assert!(geometry.triangles.is_empty());
        assert_eq!(2, crossing.affected_segments.len());
        assert_eq!(2, crossing.remap.segment_targets(original).len());

        let intersection_vertex_ids = geometry
            .vertices
            .values()
            .filter(|vertex| vertex.position == Vec3::ZERO)
            .map(|vertex| vertex.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(1, intersection_vertex_ids.len());
        let intersection_vertex = *intersection_vertex_ids.first().unwrap();
        assert_eq!(
            4,
            geometry
                .segments
                .values()
                .filter(|segment| segment
                    .endpoints
                    .iter()
                    .any(|endpoint| endpoint.id == intersection_vertex))
                .count()
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn planar_mesh_boundary_edges_use_line_splitting_and_overlap_consolidation() {
        let mut document = Document::new();
        document
            .append_line_segment_to_scene_root(Vec3::ZERO, Vec3::X * 4.0)
            .unwrap();
        let (segments, triangles) = document
            .append_planar_mesh_to_scene_root(
                vec![
                    Vec3::X * 2.0,
                    Vec3::X * 4.0,
                    Vec3::new(4.0, 2.0, 0.0),
                    Vec3::new(2.0, 2.0, 0.0),
                ],
                vec![[0, 1], [1, 2], [2, 3], [3, 0]],
                vec![[0, 1, 2], [0, 2, 3]],
            )
            .unwrap();
        let geometry = document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        let bottom_edges = geometry
            .segments
            .values()
            .filter_map(|segment| {
                let points = segment
                    .endpoints
                    .map(|reference| geometry.vertex_position(reference).unwrap());
                (points[0].y == 0.0 && points[1].y == 0.0).then_some(points)
            })
            .collect::<Vec<_>>();
        assert!(
            bottom_edges
                .iter()
                .any(|edge| { edge.contains(&Vec3::ZERO) && edge.contains(&(Vec3::X * 2.0)) })
        );
        assert!(
            bottom_edges
                .iter()
                .any(|edge| { edge.contains(&(Vec3::X * 2.0)) && edge.contains(&(Vec3::X * 4.0)) })
        );
        assert!(segments.iter().all(|id| geometry.segments.contains_key(id)));
        assert_eq!(2, triangles.len());
        assert_eq!(2, geometry.triangles.len());
        assert!(document.validate().is_ok());
    }

    #[test]
    fn triangle_only_insertion_adds_faces_without_changing_selected_lines() {
        let mut document = Document::new();
        document
            .append_line_segment_to_scene_root(Vec3::ZERO, Vec3::X * 2.0)
            .unwrap();
        let triangles = document
            .append_triangles_to_scene_root(
                vec![
                    Vec3::ZERO,
                    Vec3::X * 2.0,
                    Vec3::new(2.0, 0.0, 2.0),
                    Vec3::Z * 2.0,
                ],
                vec![[0, 1, 2], [0, 2, 3]],
            )
            .unwrap();
        let geometry = document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(2, triangles.len());
        assert_eq!(1, geometry.segments.len());
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(2, geometry.triangles.len());

        let revision = document.revision();
        assert!(
            document
                .append_triangles_to_scene_root(vec![Vec3::ZERO, Vec3::X], vec![[0, 1, 2]])
                .is_err()
        );
        assert_eq!(revision, document.revision());
        assert!(document.validate().is_ok());
    }

    #[test]
    fn line_insertion_consolidates_collinear_overlap_and_splits_endpoint_on_edge() {
        let mut document = Document::new();
        document
            .append_line_segment_to_scene_root(Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0))
            .unwrap();
        let overlap = document
            .append_line_segment_to_scene_root(Vec3::new(1.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0))
            .unwrap();
        let root = OccurrenceRef::scene_root();
        let geometry = document.effective_geometry(&root).unwrap();
        assert_eq!(1, geometry.segments.len());
        let segment = geometry.segments.values().next().unwrap();
        let points = segment
            .endpoints
            .map(|reference| geometry.vertex_position(reference).unwrap());
        assert!(points.contains(&Vec3::ZERO));
        assert!(points.contains(&Vec3::new(3.0, 0.0, 0.0)));
        assert_eq!(1, overlap.affected_segments.len());

        let tee = document
            .append_line_segment_to_scene_root(Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 1.0))
            .unwrap();
        assert_eq!(
            3,
            document.effective_geometry(&root).unwrap().segments.len()
        );
        assert_eq!(1, tee.affected_segments.len());
        assert!(document.validate().is_ok());

        let mut touching = Document::new();
        touching
            .append_line_segment_to_scene_root(Vec3::ZERO, Vec3::X)
            .unwrap();
        touching
            .append_line_segment_to_scene_root(Vec3::X, Vec3::X * 2.0)
            .unwrap();
        let geometry = touching
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(1, geometry.segments.len());
        let segment = geometry.segments.values().next().unwrap();
        let points = segment
            .endpoints
            .map(|reference| geometry.vertex_position(reference).unwrap());
        assert!(points.contains(&Vec3::ZERO));
        assert!(points.contains(&(Vec3::X * 2.0)));
    }

    #[test]
    fn ordinary_line_on_face_keeps_triangle_topology_and_near_misses_stay_separate() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("face host", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let definition = document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        let original_triangles = document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .len();
        document
            .append_line_segment_to_definition(
                definition,
                Vec3::new(0.2, 2.0, 0.2),
                Vec3::new(1.8, 2.0, 1.8),
            )
            .unwrap();
        assert_eq!(
            original_triangles,
            document
                .effective_geometry(&occurrence)
                .unwrap()
                .triangles
                .len()
        );

        let mut near_miss = Document::new();
        near_miss
            .append_line_segment_to_scene_root(Vec3::ZERO, Vec3::X)
            .unwrap();
        near_miss
            .append_line_segment_to_scene_root(Vec3::new(1.01, 0.0, 0.0), Vec3::new(1.01, 0.0, 1.0))
            .unwrap();
        let root = OccurrenceRef::scene_root();
        assert_eq!(
            2,
            near_miss.effective_geometry(&root).unwrap().segments.len()
        );
        let revision = near_miss.revision();
        assert!(
            near_miss
                .append_line_segment_to_scene_root(Vec3::ZERO, Vec3::ZERO)
                .is_none()
        );
        assert_eq!(revision, near_miss.revision());
        assert!(near_miss.validate().is_ok());
    }

    #[test]
    fn indexed_geometry_preserves_corner_offsets_coincident_identity_and_face_colors() {
        let mut document = Document::new();
        let bundle = document
            .build_geometry_bundle_raw(
                vec![
                    RawGeometryVertex {
                        position: Vec3::ZERO,
                        legacy_aliases: vec![],
                    },
                    RawGeometryVertex {
                        position: Vec3::ZERO,
                        legacy_aliases: vec![],
                    },
                    RawGeometryVertex {
                        position: Vec3::X,
                        legacy_aliases: vec![],
                    },
                    RawGeometryVertex {
                        position: Vec3::Y,
                        legacy_aliases: vec![],
                    },
                ],
                vec![],
                vec![
                    RawGeometryTriangle {
                        vertices: [
                            RawVertexRef {
                                vertex: 0,
                                position_offset: Vec3::ZERO,
                            },
                            RawVertexRef {
                                vertex: 2,
                                position_offset: Vec3::ZERO,
                            },
                            RawVertexRef {
                                vertex: 3,
                                position_offset: Vec3::ZERO,
                            },
                        ],
                        color: Rgba8::new(255, 0, 0, 255),
                        legacy_aliases: vec![],
                        extensions: BTreeMap::new(),
                    },
                    RawGeometryTriangle {
                        vertices: [
                            RawVertexRef {
                                vertex: 0,
                                position_offset: Vec3::new(0.000_000_5, 0.0, 0.0),
                            },
                            RawVertexRef {
                                vertex: 3,
                                position_offset: Vec3::ZERO,
                            },
                            RawVertexRef {
                                vertex: 2,
                                position_offset: Vec3::ZERO,
                            },
                        ],
                        color: Rgba8::new(0, 0, 255, 128),
                        legacy_aliases: vec![],
                        extensions: BTreeMap::new(),
                    },
                ],
            )
            .unwrap();

        assert_eq!(4, bundle.vertices.len());
        let vertices: Vec<_> = bundle.vertices.values().collect();
        assert_ne!(vertices[0].id, vertices[1].id);
        assert_eq!(vertices[0].position, vertices[1].position);
        assert_eq!(2, bundle.triangles.len());
        let triangles: Vec<_> = bundle.triangles.values().collect();
        assert_eq!(Rgba8::new(255, 0, 0, 255), triangles[0].color);
        assert_eq!(Rgba8::new(0, 0, 255, 128), triangles[1].color);
        assert_eq!(triangles[0].vertices[0].id, triangles[1].vertices[0].id);
        assert_ne!(
            triangles[0].vertices[0].position_offset,
            triangles[1].vertices[0].position_offset
        );
        assert_eq!(
            Vec3::new(0.000_000_5, 0.0, 0.0),
            bundle.vertex_position(triangles[1].vertices[0]).unwrap()
        );
        let round_trip: GeometryBundle =
            serde_json::from_value(serde_json::to_value(&bundle).unwrap()).unwrap();
        assert_eq!(bundle, round_trip);
        assert!(round_trip.validate().is_ok());
    }

    #[test]
    fn grouping_moves_instance_edits_and_array_copies_them_per_occurrence() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("first", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let second = document
            .add_cuboid("second", Vec3::new(1.0, 1.0, 1.0), Vec3::new(4.0, 0.0, 0.0))
            .unwrap();
        let edits = InstanceEdits {
            transform: Some(Affine3::from_translation(Vec3::new(6.0, 0.0, 0.0))),
            values: BTreeMap::from([("material.tint".into(), serde_json::json!("red"))]),
            ..InstanceEdits::default()
        };
        assert!(document.set_occurrence_edits(second.clone(), edits));

        let group = document
            .create_object_from_root_occurrences(&[first, second], "pair")
            .unwrap();
        let group_definition = document.resolve_occurrence(&group).unwrap().definition.id;
        let layout = document.definitions[&group_definition]
            .default_child_layout
            .unwrap();
        let child_for_second = document.child_layouts[&layout].entries[1].id;
        let grouped_second = group.child(child_for_second);
        let grouped = document.resolve_occurrence(&grouped_second).unwrap();
        assert_eq!(grouped.world_transform.translation.x, 6.0);
        assert_eq!(
            grouped.edits.unwrap().values["material.tint"],
            serde_json::json!("red")
        );

        let copies = document
            .array_root_occurrence(&group, 2, Vec3::new(10.0, 0.0, 0.0))
            .unwrap();
        let copied_grouped_second = copies[1].child(child_for_second);
        let copied = document.resolve_occurrence(&copied_grouped_second).unwrap();
        assert_eq!(copied.world_transform.translation.x, 26.0);
        assert_eq!(
            copied.edits.unwrap().values["material.tint"],
            serde_json::json!("red")
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn deleting_nested_siblings_isolated_to_the_selected_instance() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("first", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let second = document
            .add_cuboid("second", Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 0.0, 0.0))
            .unwrap();
        let group = document
            .create_object_from_root_occurrences(&[first, second], "pair")
            .unwrap();
        let group_copy = document
            .copy_root_occurrence(&group, Vec3::new(10.0, 0.0, 0.0))
            .unwrap();
        let mut original_children = document
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .filter(|item| item.path.len() == 2 && item.path[0] == group.path[0])
            .collect::<Vec<_>>();
        let copied_children = document
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .filter(|item| item.path.len() == 2 && item.path[0] == group_copy.path[0])
            .collect::<Vec<_>>();
        assert_eq!(2, original_children.len());
        assert_eq!(2, copied_children.len());

        let removed = document
            .remove_selected_occurrences(&[original_children.remove(0)])
            .unwrap();

        assert_eq!(1, removed);
        assert_eq!(
            1,
            document
                .occurrence_refs()
                .unwrap()
                .iter()
                .filter(|item| { item.path.len() == 2 && item.path[0] == group.path[0] })
                .count()
        );
        assert_eq!(
            2,
            document
                .occurrence_refs()
                .unwrap()
                .iter()
                .filter(|item| { item.path.len() == 2 && item.path[0] == group_copy.path[0] })
                .count()
        );
        assert!(
            copied_children
                .iter()
                .all(|item| document.resolve_occurrence(item).is_some())
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn deleting_root_siblings_removes_their_occurrence_edits() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("first", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let second = document
            .add_cuboid("second", Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 0.0, 0.0))
            .unwrap();
        assert!(document.set_occurrence_edits(
            second.clone(),
            InstanceEdits {
                values: BTreeMap::from([("custom.value".to_owned(), serde_json::json!(7))]),
                ..InstanceEdits::default()
            }
        ));

        assert_eq!(
            1,
            document
                .remove_selected_occurrences(std::slice::from_ref(&first))
                .unwrap()
        );

        assert!(document.resolve_occurrence(&first).is_none());
        assert!(document.resolve_occurrence(&second).is_some());
        assert!(document.occurrence_edits(&second).is_some());
        assert!(document.validate().is_ok());
    }

    #[test]
    fn deleting_selection_across_parent_groups_preserves_each_shared_definition() {
        let mut document = Document::new();
        let create_pair = |document: &mut Document, prefix: &str, offset: f64| {
            let first = document
                .add_cuboid(
                    format!("{prefix}-first"),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(offset, 0.0, 0.0),
                )
                .unwrap();
            let second = document
                .add_cuboid(
                    format!("{prefix}-second"),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(offset + 2.0, 0.0, 0.0),
                )
                .unwrap();
            document
                .create_object_from_root_occurrences(&[first, second], format!("{prefix}-pair"))
                .unwrap()
        };
        let first_group = create_pair(&mut document, "first", 0.0);
        let second_group = create_pair(&mut document, "second", 8.0);
        let targets = [first_group.clone(), second_group.clone()]
            .into_iter()
            .map(|group| {
                document
                    .occurrence_refs()
                    .unwrap()
                    .into_iter()
                    .find(|item| item.path.len() == 2 && item.path[0] == group.path[0])
                    .unwrap()
            })
            .collect::<Vec<_>>();

        assert_eq!(2, document.remove_selected_occurrences(&targets).unwrap());

        let occurrences = document.occurrence_refs().unwrap();
        assert_eq!(
            1,
            occurrences
                .iter()
                .filter(|item| item.path.len() == 2 && item.path[0] == first_group.path[0])
                .count()
        );
        assert_eq!(
            1,
            occurrences
                .iter()
                .filter(|item| item.path.len() == 2 && item.path[0] == second_group.path[0])
                .count()
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn deleting_a_parent_and_its_descendant_is_rejected_without_changes() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("first", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let second = document
            .add_cuboid("second", Vec3::new(1.0, 1.0, 1.0), Vec3::new(2.0, 0.0, 0.0))
            .unwrap();
        let group = document
            .create_object_from_root_occurrences(&[first, second], "pair")
            .unwrap();
        let child = document
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .find(|item| item.path.len() == 2 && item.path[0] == group.path[0])
            .unwrap();
        let revision = document.revision();

        assert!(
            document
                .remove_selected_occurrences(&[group, child])
                .is_err()
        );

        assert_eq!(revision, document.revision());
        assert_eq!(3, document.occurrence_refs().unwrap().len());
        assert!(document.validate().is_ok());
    }
}
