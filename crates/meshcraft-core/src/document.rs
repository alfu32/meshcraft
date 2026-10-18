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
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            unit_name: "unit".to_owned(),
            unit_size: 1.0,
            grid_size: 1.0,
            circle_segments: 24,
            snap_radius: 12.0,
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
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            revision: 0,
            root_layout,
            definitions: BTreeMap::new(),
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
        self.definitions.values()
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
                entry,
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

    fn allocate_id(&mut self) -> u64 {
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

pub struct ResolvedOccurrence<'a> {
    pub occurrence: OccurrenceRef,
    pub entry: &'a LayoutEntry,
    pub definition: &'a Definition,
    pub edits: Option<&'a InstanceEdits>,
    pub world_transform: Affine3,
}

#[cfg(test)]
mod tests {
    use super::{
        DefinitionKind, Document, GeometryBundle, InstanceEdits, ModelSettings, OccurrenceRef,
        RawGeometryTriangle, RawGeometryVertex, RawVertexRef, Rgba8, SegmentId, TopologyRemap,
        Triangle, TriangleId, Vertex, VertexId, VertexRef,
    };
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
