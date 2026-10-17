use crate::geometry::{Affine3, Vec3};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

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
            if segment
                .endpoints
                .iter()
                .any(|vertex| !self.vertices.contains_key(vertex))
            {
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
            if triangle
                .vertices
                .iter()
                .any(|vertex| !self.vertices.contains_key(vertex))
            {
                return Err(format!("triangle {} has invalid vertex references", id.0));
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() && self.segments.is_empty() && self.triangles.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Vertex {
    pub id: VertexId,
    pub position: Vec3,
    #[serde(default)]
    pub legacy_aliases: Vec<LegacyAlias>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Segment {
    pub id: SegmentId,
    pub endpoints: [VertexId; 2],
    pub color: Rgba8,
    #[serde(default)]
    pub legacy_aliases: Vec<LegacyAlias>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Triangle {
    pub id: TriangleId,
    pub vertices: [VertexId; 3],
    pub color: Rgba8,
    #[serde(default)]
    pub legacy_aliases: Vec<LegacyAlias>,
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
        self.transform.is_none()
            && self.geometry_replacement.is_none()
            && self.child_layout.is_none()
            && self.hotspot_positions.is_empty()
            && self.values.is_empty()
            && self.extensions.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Document {
    pub schema_version: u32,
    pub revision: u64,
    pub root_layout: ChildLayoutId,
    pub definitions: BTreeMap<DefinitionId, Definition>,
    pub child_layouts: BTreeMap<ChildLayoutId, ChildLayout>,
    /// Overrides belong to realized occurrences, not shared layout entries.
    #[serde(with = "occurrence_edit_map")]
    pub occurrence_edits: BTreeMap<OccurrenceRef, InstanceEdits>,
    pub extensions: BTreeMap<String, Value>,
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
            extensions: BTreeMap::new(),
            next_id: 2,
        }
    }

    pub fn root_layout(&self) -> ChildLayoutId {
        self.root_layout
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
        let definition_id = DefinitionId(self.allocate_id());
        let geometry_id = GeometryBundleId(self.allocate_id());
        let mut geometry = GeometryBundle {
            id: geometry_id,
            ..GeometryBundle::default()
        };
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
                    endpoints: [vertex_ids[a], vertex_ids[b]],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
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
                    vertices: [vertex_ids[a], vertex_ids[b], vertex_ids[c]],
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::new(),
                },
            );
        }
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
                transform: Affine3::from_translation(origin),
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
        DefinitionKind, Document, GeometryBundle, InstanceEdits, OccurrenceRef, Rgba8, Triangle,
        TriangleId, Vertex, VertexId,
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
                vertices: [VertexId(id + 1), VertexId(id + 2), VertexId(id + 3)],
                color: Rgba8::WHITE,
                legacy_aliases: Vec::new(),
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
}
