//! Imports external scenes as reusable prototype subscenes, without flattening their hierarchy.
use super::*;

#[derive(Default)]
struct Ids {
    materials: BTreeMap<String, String>,
    definitions: BTreeMap<DefinitionId, DefinitionId>,
    layouts: BTreeMap<ChildLayoutId, ChildLayoutId>,
    entries: BTreeMap<LayoutEntryId, LayoutEntryId>,
    vertices: BTreeMap<VertexId, VertexId>,
    segments: BTreeMap<SegmentId, SegmentId>,
    triangles: BTreeMap<TriangleId, TriangleId>,
}

impl Document {
    /// Import a snapshot, optionally replacing the cached content of an existing external prototype.
    /// The source scene, its nested instances and root hotspots remain independent of this document.
    pub fn import_subscene(
        &mut self,
        source: &Document,
        name: &str,
        external_path: Option<&str>,
        replace: Option<DefinitionId>,
    ) -> Result<DefinitionId, String> {
        self.import_subscene_reusing(source, name, external_path, replace, &BTreeSet::new(), None)
    }

    pub(super) fn import_subscene_reusing(
        &mut self,
        source: &Document,
        name: &str,
        external_path: Option<&str>,
        replace: Option<DefinitionId>,
        reuse: &BTreeSet<DefinitionId>,
        mut copied_edits: Option<&mut Vec<(OccurrenceRef, InstanceEdits)>>,
    ) -> Result<DefinitionId, String> {
        source.validate()?;
        if name.trim().is_empty() {
            return Err("object prototype name cannot be empty".into());
        }
        if replace == Some(Self::SCENE_ROOT_DEFINITION_ID) {
            return Err("cannot replace the document root with an external object".into());
        }
        if replace.is_some_and(|id| self.definition(id).is_none()) {
            return Err("external prototype no longer exists".into());
        }
        let old_owned = replace
            .map(|root| reachable_definitions(self, [root]))
            .unwrap_or_default();
        let old_layouts = old_owned
            .iter()
            .filter_map(|id| self.definition(*id).and_then(|d| d.default_child_layout))
            .collect::<BTreeSet<_>>();
        let parents = replace
            .map(|id| {
                self.occurrence_refs()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|path| {
                        self.resolve_occurrence(path)
                            .is_some_and(|r| r.definition.id == id)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let nested_overrides = self
            .occurrence_edits
            .iter()
            .filter_map(|(path, edits)| {
                let parent = parents.iter().find(|parent| {
                    path.path.len() > parent.path.len() && path.path.starts_with(&parent.path)
                })?;
                let mut steps = Vec::new();
                for depth in parent.path.len()..path.path.len() {
                    let r = self.resolve_occurrence(&OccurrenceRef {
                        path: path.path[..depth].to_vec(),
                    })?;
                    let layout = r
                        .edits
                        .and_then(|e| e.child_layout)
                        .or(r.definition.default_child_layout)
                        .and_then(|id| self.child_layout(id))?;
                    let index = layout
                        .entries
                        .iter()
                        .position(|e| e.id == path.path[depth])?;
                    let entry = &layout.entries[index];
                    steps.push((
                        index,
                        entry.legacy_aliases.first().map(|a| a.id.clone()),
                        self.definition(entry.definition)?.name.clone(),
                    ));
                }
                Some((path.clone(), parent.clone(), steps, edits.clone()))
            })
            .collect::<Vec<_>>();
        let mut draft = self.clone();
        for (path, _, _, _) in &nested_overrides {
            draft.occurrence_edits.remove(path);
        }
        let mut ids = Ids {
            materials: draft.merge_materials(source)?,
            ..Ids::default()
        };
        let root = replace.unwrap_or_else(|| DefinitionId(draft.allocate_id()));
        ids.definitions.insert(Self::SCENE_ROOT_DEFINITION_ID, root);
        let reused = reuse
            .iter()
            .copied()
            .filter(|id| *id != Self::SCENE_ROOT_DEFINITION_ID && draft.definition(*id).is_some())
            .collect::<BTreeSet<_>>();
        let reused_layouts = reused
            .iter()
            .filter_map(|id| source.definition(*id).and_then(|d| d.default_child_layout))
            .filter(|id| draft.child_layout(*id).is_some())
            .collect::<BTreeSet<_>>();
        for definition in source.definitions() {
            let id = if reused.contains(&definition.id) {
                definition.id
            } else {
                DefinitionId(draft.allocate_id())
            };
            ids.definitions.insert(definition.id, id);
            if reused.contains(&definition.id) {
                ids.vertices
                    .extend(definition.geometry.vertices.keys().map(|id| (*id, *id)));
                ids.segments
                    .extend(definition.geometry.segments.keys().map(|id| (*id, *id)));
                ids.triangles
                    .extend(definition.geometry.triangles.keys().map(|id| (*id, *id)));
            }
        }
        for layout in source.child_layouts.values() {
            ids.layouts.insert(
                layout.id,
                if reused_layouts.contains(&layout.id) {
                    layout.id
                } else {
                    ChildLayoutId(draft.allocate_id())
                },
            );
            for entry in &layout.entries {
                ids.entries.insert(
                    entry.id,
                    if reused_layouts.contains(&layout.id) {
                        entry.id
                    } else {
                        LayoutEntryId(draft.allocate_id())
                    },
                );
            }
        }
        // Persist native IDs as aliases when a component is moved into another document.
        for definition in source.definitions.values() {
            allocate_geometry(&mut draft, &definition.geometry, &mut ids);
        }
        for edits in source.occurrence_edits.values() {
            if let Some(geometry) = edits.geometry_replacement.as_ref() {
                allocate_geometry(&mut draft, geometry, &mut ids);
            }
        }
        for layout in source.child_layouts.values() {
            for entry in &layout.entries {
                if let Some(geometry) = entry.default_edits.geometry_replacement.as_ref() {
                    allocate_geometry(&mut draft, geometry, &mut ids);
                }
            }
        }
        for layout in source.child_layouts.values() {
            if reused_layouts.contains(&layout.id) {
                continue;
            }
            let mut copy = layout.clone();
            copy.id = ids.layouts[&layout.id];
            for entry in &mut copy.entries {
                let original = entry.id;
                entry.id = ids.entries[&original];
                entry.definition = ids.definitions[&entry.definition];
                // Every imported scene placement becomes a default child instance in the subscene.
                let original_path = if copied_edits.is_none() {
                    source
                        .occurrence_refs()?
                        .into_iter()
                        .find(|path| path.path.last() == Some(&original))
                } else {
                    None
                };
                let edits = if copied_edits.is_some() {
                    &entry.default_edits
                } else {
                    original_path
                        .as_ref()
                        .and_then(|p| source.occurrence_edits(p))
                        .unwrap_or(&entry.default_edits)
                };
                entry.default_edits = remap_edits(edits, &ids, &mut draft)?;
                entry.default_edits.legacy_instance_id = None;
            }
            draft.child_layouts.insert(copy.id, copy);
        }
        for definition in source.definitions.values() {
            if reused.contains(&definition.id) {
                continue;
            }
            let mut copy = definition.clone();
            copy.id = ids.definitions[&definition.id];
            copy.geometry = remap_geometry(&definition.geometry, &ids, &mut draft);
            copy.default_child_layout = definition.default_child_layout.map(|id| ids.layouts[&id]);
            fn remap_surfaces(value: &mut Value, ids: &BTreeMap<String, String>) {
                if let Some(map) = value.as_object_mut() {
                    if let Some(overrides) = map
                        .get_mut("proceduralParameters")
                        .and_then(Value::as_object_mut)
                    {
                        *overrides = std::mem::take(overrides)
                            .into_iter()
                            .map(|(id, value)| (ids.get(&id).cloned().unwrap_or(id), value))
                            .collect();
                    }
                    if let Some(materials) = map.get_mut("materials").and_then(Value::as_object_mut)
                    {
                        for value in materials.values_mut() {
                            if let Some(id) = value.as_str().and_then(|id| ids.get(id)) {
                                *value = Value::String(id.clone());
                            }
                        }
                    }
                    for child in map.values_mut() {
                        remap_surfaces(child, ids);
                    }
                } else if let Some(values) = value.as_array_mut() {
                    for value in values {
                        remap_surfaces(value, ids);
                    }
                }
            }
            for field in [
                super::architecture::WALLS,
                super::architecture::SLABS,
                super::architecture::FRAMES,
            ] {
                if let Some(value) = copy.domain_data.get_mut(field) {
                    remap_surfaces(value, &ids.materials);
                }
                if let Some(value) = copy.extensions.get_mut(&format!("k3d.{field}")) {
                    remap_surfaces(value, &ids.materials);
                }
            }
            remap_hotspot_ids(&mut copy, &ids)?;
            draft.definitions.insert(copy.id, copy);
        }
        // A legacy import can omit the normalized root definition. Make its root layout the subscene root.
        if source.scene_root_definition().is_none() {
            let geometry_id = GeometryBundleId(draft.allocate_id());
            draft.definitions.insert(
                root,
                Definition {
                    id: root,
                    name: name.into(),
                    kind: DefinitionKind::Hybrid,
                    frame: Affine3::IDENTITY,
                    geometry: GeometryBundle {
                        id: geometry_id,
                        ..GeometryBundle::default()
                    },
                    default_child_layout: Some(ids.layouts[&source.root_layout]),
                    domain_data: BTreeMap::new(),
                    domain_entity_ids: BTreeMap::new(),
                    extensions: BTreeMap::new(),
                    legacy_aliases: Vec::new(),
                },
            );
        }
        let prototype = draft
            .definitions
            .get_mut(&root)
            .ok_or("imported scene root is missing")?;
        prototype.name = name.trim().into();
        prototype.default_child_layout = Some(ids.layouts[&source.root_layout]);
        prototype.extensions.insert(
            "k3d.externalReferenceEnabled".into(),
            Value::Bool(external_path.is_some()),
        );
        prototype.extensions.insert(
            "k3d.externalReferencePath".into(),
            Value::String(external_path.unwrap_or_default().into()),
        );
        for (old_path, parent, steps, edits) in nested_overrides {
            let mut path = parent;
            let mut valid = true;
            for (index, alias, name) in steps {
                let Some(r) = draft.resolve_occurrence(&path) else {
                    valid = false;
                    break;
                };
                let Some(layout) = r
                    .edits
                    .and_then(|e| e.child_layout)
                    .or(r.definition.default_child_layout)
                    .and_then(|id| draft.child_layout(id))
                else {
                    valid = false;
                    break;
                };
                let entry = alias
                    .as_ref()
                    .and_then(|alias| {
                        layout
                            .entries
                            .iter()
                            .find(|entry| entry.legacy_aliases.iter().any(|a| &a.id == alias))
                    })
                    .or_else(|| {
                        layout.entries.get(index).filter(|e| {
                            draft
                                .definition(e.definition)
                                .is_some_and(|d| d.name == name)
                        })
                    });
                let Some(entry) = entry else {
                    valid = false;
                    break;
                };
                path = path.child(entry.id);
            }
            if valid {
                draft.occurrence_edits.insert(path, edits);
            } else if draft.resolve_occurrence(&old_path).is_some() {
                draft.occurrence_edits.insert(old_path, edits);
            }
        }
        // Release the previous cached subscene while retaining prototypes used elsewhere.
        let starts = draft
            .definitions
            .keys()
            .filter(|id| !old_owned.contains(id) || **id == root)
            .copied()
            .chain(
                draft.child_layouts[&draft.root_layout]
                    .entries
                    .iter()
                    .map(|e| e.definition),
            )
            .collect::<Vec<_>>();
        let mut live = reachable_definitions(&draft, starts);
        for edits in draft.occurrence_edits.values() {
            if let Some(layout) = edits.child_layout.and_then(|id| draft.child_layout(id)) {
                live.extend(reachable_definitions(
                    &draft,
                    layout.entries.iter().map(|e| e.definition),
                ));
            }
        }
        for id in old_owned {
            if !live.contains(&id) {
                draft.definitions.remove(&id);
            }
        }
        let used_layouts = draft
            .definitions
            .values()
            .filter_map(|d| d.default_child_layout)
            .chain(
                draft
                    .occurrence_edits
                    .values()
                    .filter_map(|e| e.child_layout),
            )
            .chain([draft.root_layout])
            .collect::<BTreeSet<_>>();
        for id in old_layouts {
            if !used_layouts.contains(&id) {
                draft.child_layouts.remove(&id);
            }
        }
        if let Some(output) = copied_edits.as_mut() {
            for (path, edits) in &source.occurrence_edits {
                let mapped = OccurrenceRef {
                    path: path.path.iter().map(|id| ids.entries[id]).collect(),
                };
                output.push((mapped, remap_edits(edits, &ids, &mut draft)?));
            }
        }
        draft.bump_revision();
        draft.validate()?;
        *self = draft;
        Ok(root)
    }
}
fn allocate_geometry(document: &mut Document, geometry: &GeometryBundle, ids: &mut Ids) {
    for id in geometry.vertices.keys() {
        ids.vertices
            .entry(*id)
            .or_insert_with(|| VertexId(document.allocate_id()));
    }
    for id in geometry.segments.keys() {
        ids.segments
            .entry(*id)
            .or_insert_with(|| SegmentId(document.allocate_id()));
    }
    for id in geometry.triangles.keys() {
        ids.triangles
            .entry(*id)
            .or_insert_with(|| TriangleId(document.allocate_id()));
    }
}
fn remap_geometry(geometry: &GeometryBundle, ids: &Ids, document: &mut Document) -> GeometryBundle {
    let mut copy = geometry.clone();
    copy.id = GeometryBundleId(document.allocate_id());
    copy.vertices = geometry
        .vertices
        .values()
        .cloned()
        .map(|mut vertex| {
            let old = vertex.id;
            vertex.id = ids.vertices[&old];
            if vertex.legacy_aliases.is_empty() {
                vertex.legacy_aliases.push(LegacyAlias {
                    source: "k3d.vertex".into(),
                    id: format!("meshcraft-vertex-{}", old.0),
                });
            }
            (vertex.id, vertex)
        })
        .collect();
    copy.segments = geometry
        .segments
        .values()
        .cloned()
        .map(|mut segment| {
            remap_material(&mut segment.extensions, &ids.materials);
            let old = segment.id;
            segment.id = ids.segments[&old];
            if segment.legacy_aliases.is_empty() {
                segment.legacy_aliases.push(LegacyAlias {
                    source: "k3d.segment".into(),
                    id: format!("meshcraft-segment-{}", old.0),
                });
            }
            for v in &mut segment.endpoints {
                v.id = ids.vertices[&v.id];
            }
            (segment.id, segment)
        })
        .collect();
    copy.triangles = geometry
        .triangles
        .values()
        .cloned()
        .map(|mut triangle| {
            remap_material(&mut triangle.extensions, &ids.materials);
            let old = triangle.id;
            triangle.id = ids.triangles[&old];
            if triangle.legacy_aliases.is_empty() {
                triangle.legacy_aliases.push(LegacyAlias {
                    source: "k3d.face".into(),
                    id: format!("meshcraft-face-{}", old.0),
                });
            }
            for v in &mut triangle.vertices {
                v.id = ids.vertices[&v.id];
            }
            (triangle.id, triangle)
        })
        .collect();
    copy
}
fn remap_edits(
    edits: &InstanceEdits,
    ids: &Ids,
    document: &mut Document,
) -> Result<InstanceEdits, String> {
    let mut copy = edits.clone();
    if let Some(geometry) = edits.geometry_replacement.as_ref() {
        copy.geometry_replacement = Some(remap_geometry(geometry, ids, document));
    }
    copy.child_layout = edits.child_layout.map(|id| ids.layouts[&id]);
    Ok(copy)
}
fn remap_hotspot_ids(definition: &mut Definition, ids: &Ids) -> Result<(), String> {
    let Some(raw) = definition
        .domain_data
        .get_mut("hotspots")
        .or_else(|| definition.extensions.get_mut("k3d.hotspots"))
    else {
        return Ok(());
    };
    // Coordinate references survive independently. Rewrite native stable-ID strings as well.
    let Some(hotspots) = raw.as_array_mut() else {
        return Err("hotspot definitions must be an array".into());
    };
    for hotspot in hotspots {
        if let Some(vertices) = hotspot
            .get_mut("attachedVertexIds")
            .and_then(Value::as_array_mut)
        {
            for vertex in vertices {
                if let Some(old) = vertex
                    .as_str()
                    .and_then(|s| s.strip_prefix("vertex-"))
                    .and_then(|s| s.parse::<u64>().ok())
                    .and_then(|id| ids.vertices.get(&VertexId(id)))
                {
                    *vertex = Value::String(format!("vertex-{}", old.0));
                }
            }
        }
        for (field, prefix, replacements) in [
            (
                "attachedSegments",
                "segment-",
                ids.segments
                    .iter()
                    .map(|(a, b)| (a.0, b.0))
                    .collect::<BTreeMap<_, _>>(),
            ),
            (
                "attachedTriangles",
                "triangle-",
                ids.triangles.iter().map(|(a, b)| (a.0, b.0)).collect(),
            ),
        ] {
            if let Some(records) = hotspot.get_mut(field).and_then(Value::as_array_mut) {
                for record in records {
                    if let Some(new) = record
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|s| s.strip_prefix(prefix))
                        .and_then(|s| s.parse::<u64>().ok())
                        .and_then(|old| replacements.get(&old))
                    {
                        record["id"] = Value::String(format!("{prefix}{new}"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn reachable_definitions(
    document: &Document,
    roots: impl IntoIterator<Item = DefinitionId>,
) -> BTreeSet<DefinitionId> {
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    let mut result = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !result.insert(id) {
            continue;
        }
        if let Some(layout) = document
            .definition(id)
            .and_then(|d| d.default_child_layout)
            .and_then(|id| document.child_layout(id))
        {
            pending.extend(layout.entries.iter().map(|e| e.definition));
            for entry in &layout.entries {
                if let Some(layout) = entry
                    .default_edits
                    .child_layout
                    .and_then(|id| document.child_layout(id))
                {
                    pending.extend(layout.entries.iter().map(|e| e.definition));
                }
            }
        }
    }
    result
}

impl Document {
    /// Runs a shared-prototype edit against authored content while preserving runtime values.
    /// The temporary edit view never enters undo/save state.
    pub fn edit_prototype<T>(
        &mut self,
        active: &OccurrenceRef,
        edit: impl FnOnce(&mut Document) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut draft = self.prototype_edit_view(active);
        let result = edit(&mut draft)?;
        let saved = self
            .occurrence_edits
            .iter()
            .filter(|(path, _)| path.path.starts_with(&active.path))
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect::<Vec<_>>();
        for (path, mut edits) in saved {
            // Restore the active placement before attempting descendant paths.
            if path != *active && draft.resolve_occurrence(&path).is_none() {
                continue;
            }
            if let Some(old) = self.resolve_occurrence(&path) {
                if edits.geometry_replacement.as_ref() == Some(&old.definition.geometry) {
                    edits.geometry_replacement = None;
                }
                let old_ids = self
                    .hotspots(old.definition.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|h| h.id)
                    .collect::<BTreeSet<_>>();
                if let Some(new) = draft.resolve_occurrence(&path) {
                    let new_ids = draft
                        .hotspots(new.definition.id)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|h| h.id)
                        .collect::<BTreeSet<_>>();
                    let before = edits.hotspot_positions.len();
                    edits
                        .hotspot_positions
                        .retain(|id, _| !old_ids.contains(id) || new_ids.contains(id));
                    if before != edits.hotspot_positions.len() {
                        edits.geometry_replacement = None;
                    }
                }
            }
            draft.occurrence_edits.insert(path, edits);
        }
        if !self.occurrence_edits.contains_key(active) {
            draft.occurrence_edits.remove(active);
        }
        draft.hotspot_cache.clear();
        draft.validate()?;
        *self = draft;
        Ok(result)
    }

    pub fn set_nested_instance_hotspot_default(
        &mut self,
        context: &crate::selection::EditContext,
        occurrence: &OccurrenceRef,
        id: &str,
        target: Vec3,
    ) -> Result<(), String> {
        self.set_hotspot_target(occurrence, id, target)?;
        if !matches!(context, crate::selection::EditContext::Document)
            && let Some(layout) =
                crate::operations::objects::authored_child_layout(self, context, occurrence)
        {
            let edits = self.occurrence_edits.remove(occurrence).unwrap_or_default();
            let entry = self
                .child_layouts
                .get_mut(&layout)
                .and_then(|l| {
                    l.entries
                        .iter_mut()
                        .find(|e| Some(&e.id) == occurrence.path.last())
                })
                .ok_or("nested instance no longer exists")?;
            entry.default_edits = edits;
            self.bump_revision();
        }
        Ok(())
    }
}

impl Document {
    /// The imported scene container is unique: its effective mesh/layout are authored scene content.
    /// Promote legacy overrides on the first edit, preserving the original state in undo history.
    pub fn edit_scene<T>(
        &mut self,
        edit: impl FnOnce(&mut Document) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut draft = self.clone();
        if let Some(path) = draft.scene_container_occurrence() {
            let r = draft
                .resolve_occurrence(&path)
                .ok_or("scene container no longer exists")?;
            let id = r.definition.id;
            let mut edits = r.edits.cloned().unwrap_or_default();
            if let Some(geometry) = edits.geometry_replacement.take() {
                draft.definitions.get_mut(&id).unwrap().geometry = geometry;
            }
            if let Some(layout) = edits.child_layout.take() {
                draft.definitions.get_mut(&id).unwrap().default_child_layout = Some(layout);
            }
            draft.set_occurrence_edits(path, edits);
        }
        let result = edit(&mut draft)?;
        draft.hotspot_cache.clear();
        draft.validate()?;
        *self = draft;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::hotspots::{Hotspot, Operation, SegmentAttachment};
    use crate::legacy::{export_current_value, import_current_value};
    use crate::operations::objects::{duplicate_occurrence, place_definition};
    use crate::selection::EditContext;

    #[test]
    fn external_subscene_retains_root_handles_nested_values_and_unused_prototypes_on_roundtrip() {
        let mut source = Document::new();
        source
            .append_line_segment_to_definition(
                Document::SCENE_ROOT_DEFINITION_ID,
                Vec3::ZERO,
                Vec3::X,
            )
            .unwrap();
        let g = &source.scene_root_definition().unwrap().geometry;
        let h = Hotspot {
            id: "root-driver".into(),
            position: Vec3::ZERO,
            operation: Operation::Stretch,
            attached_segments: g
                .segments
                .values()
                .map(|s| SegmentAttachment::from_segment(g, s))
                .collect(),
            ..Hotspot::default()
        };
        source
            .put_hotspot(Document::SCENE_ROOT_DEFINITION_ID, h)
            .unwrap();
        let child = source
            .add_cuboid("Nested", Vec3::new(1., 2., 3.), Vec3::ZERO)
            .unwrap();
        let child_def = source.resolve_occurrence(&child).unwrap().definition.id;
        source
            .put_hotspot(
                child_def,
                Hotspot {
                    id: "child-driver".into(),
                    position: Vec3::ZERO,
                    ..Hotspot::default()
                },
            )
            .unwrap();
        source
            .set_hotspot_target(&child, "child-driver", Vec3::new(2., 0., 0.))
            .unwrap();
        let mut target = Document::new();
        let prototype = target
            .import_subscene(&source, "External", Some("part.octd"), None)
            .unwrap();
        assert_eq!(1, target.hotspots(prototype).unwrap().len());
        assert!(target.occurrence_refs().unwrap().is_empty());
        // An unused subscene must still retain its hierarchy across the actual project codec.
        let reopened = import_current_value(&export_current_value(&target).unwrap()).unwrap();
        let p = reopened
            .definitions()
            .find(|d| d.name == "External")
            .unwrap();
        assert!(
            reopened
                .child_layout(p.default_child_layout.unwrap())
                .unwrap()
                .entries
                .len()
                == 1
        );
        let parent = place_definition(
            &mut target,
            &EditContext::Document,
            prototype,
            Vec3::new(5., 0., 0.),
            None,
            false,
        )
        .unwrap();
        let nested = target
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .find(|p| p.path.len() == 2)
            .unwrap();
        assert_eq!(
            Vec3::new(2., 0., 0.),
            target
                .resolve_occurrence(&nested)
                .unwrap()
                .edits
                .unwrap()
                .hotspot_positions["child-driver"]
        );
        target
            .set_hotspot_target(&parent, "root-driver", Vec3::new(3., 0., 0.))
            .unwrap();
        let copy = duplicate_occurrence(&mut target, &parent, Vec3::new(6., 0., 0.)).unwrap();
        assert_eq!(
            target.effective_geometry(&parent),
            target.effective_geometry(&copy)
        );
        let roundtrip = import_current_value(&export_current_value(&target).unwrap()).unwrap();
        let instances = roundtrip.occurrence_refs().unwrap();
        assert!(instances.iter().any(|p| {
            roundtrip
                .resolve_occurrence(p)
                .unwrap()
                .edits
                .is_some_and(|e| e.hotspot_positions.contains_key("root-driver"))
        }));
        assert!(instances.iter().any(|p| {
            roundtrip
                .resolve_occurrence(p)
                .unwrap()
                .edits
                .is_some_and(|e| e.hotspot_positions.contains_key("child-driver"))
        }));
    }

    #[test]
    fn refresh_keeps_external_identity_and_independent_values_and_failed_import_is_atomic() {
        let mut source = Document::new();
        source
            .append_line_segment_to_definition(
                Document::SCENE_ROOT_DEFINITION_ID,
                Vec3::ZERO,
                Vec3::X,
            )
            .unwrap();
        source
            .put_hotspot(
                Document::SCENE_ROOT_DEFINITION_ID,
                Hotspot {
                    id: "root".into(),
                    ..Hotspot::default()
                },
            )
            .unwrap();
        let mut target = Document::new();
        let prototype = target
            .import_subscene(&source, "Part", Some("part.octd"), None)
            .unwrap();
        let a = place_definition(
            &mut target,
            &EditContext::Document,
            prototype,
            Vec3::ZERO,
            None,
            false,
        )
        .unwrap();
        let b = place_definition(
            &mut target,
            &EditContext::Document,
            prototype,
            Vec3::X * 5.,
            None,
            false,
        )
        .unwrap();
        target.set_hotspot_target(&a, "root", Vec3::Y).unwrap();
        source
            .append_line_segment_to_definition(
                Document::SCENE_ROOT_DEFINITION_ID,
                Vec3::ZERO,
                Vec3::Z,
            )
            .unwrap();
        assert_eq!(
            prototype,
            target
                .import_subscene(&source, "Part", Some("part.octd"), Some(prototype))
                .unwrap()
        );
        assert!(
            target
                .resolve_occurrence(&a)
                .unwrap()
                .edits
                .unwrap()
                .hotspot_positions
                .contains_key("root")
        );
        assert!(target.resolve_occurrence(&b).unwrap().edits.is_none());
        assert_eq!(
            2,
            target
                .definition(prototype)
                .unwrap()
                .geometry
                .segments
                .len()
        );
        let before = target.clone();
        assert!(
            target
                .import_subscene(&source, "", Some("part.octd"), Some(prototype))
                .is_err()
        );
        assert_eq!(before, target);
    }
}

#[cfg(test)]
mod nested_edit_tests {
    use super::*;
    use crate::document::hotspots::Hotspot;
    use crate::operations::{objects, transforms};
    use crate::selection::{EditContext, SelectedEntity, SelectionSession, SelectionTarget};
    #[test]
    fn nested_copy_move_delete_and_defaults_edit_the_shared_subscene() {
        let mut d = Document::new();
        let child = d
            .add_cuboid("Child", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let child_def = d.resolve_occurrence(&child).unwrap().definition.id;
        d.put_hotspot(
            child_def,
            Hotspot {
                id: "handle".into(),
                ..Hotspot::default()
            },
        )
        .unwrap();
        let parent = objects::group_occurrences(&mut d, &[child], "Parent").unwrap();
        let parent_def = d.resolve_occurrence(&parent).unwrap().definition.id;
        let other = objects::place_definition(
            &mut d,
            &EditContext::Document,
            parent_def,
            Vec3::X * 10.,
            None,
            false,
        )
        .unwrap();
        let entry = d
            .child_layout(
                d.definition(parent_def)
                    .unwrap()
                    .default_child_layout
                    .unwrap(),
            )
            .unwrap()
            .entries[0]
            .id;
        let original = parent.child(entry);
        let other_original = other.child(entry);
        d.set_hotspot_target(&other_original, "handle", Vec3::Y * 3.)
            .unwrap();
        let context = EditContext::Occurrence(parent.clone());
        let copy = d
            .edit_prototype(&parent, |d| {
                objects::duplicate_occurrence_in_context(d, &context, &original, Vec3::X * 2.)
            })
            .unwrap()
            .0;
        assert!(
            d.resolve_occurrence(&other.child(*copy.path.last().unwrap()))
                .is_some()
        );
        d.edit_prototype(&parent, |d| {
            d.set_nested_instance_hotspot_default(&context, &original, "handle", Vec3::Y)
        })
        .unwrap();
        assert_eq!(
            Vec3::Y,
            d.evaluated_hotspot_positions(&original).unwrap()["handle"]
        );
        assert_eq!(
            Vec3::Y * 3.,
            d.evaluated_hotspot_positions(&other_original).unwrap()["handle"]
        );
        // Resetting an occurrence to its authored anchor must mask its parent's default.
        d.set_hotspot_target(&original, "handle", Vec3::ZERO)
            .unwrap();
        assert_eq!(
            Vec3::ZERO,
            d.evaluated_hotspot_positions(&original).unwrap()["handle"]
        );
        d.edit_prototype(&parent, |d| {
            d.set_nested_instance_hotspot_default(&context, &original, "handle", Vec3::ZERO)
        })
        .unwrap();
        assert_eq!(
            Vec3::ZERO,
            d.evaluated_hotspot_positions(&original).unwrap()["handle"]
        );
        assert_eq!(
            Vec3::Y * 3.,
            d.evaluated_hotspot_positions(&other_original).unwrap()["handle"]
        );
        let mut selected = SelectionSession::default();
        selected.set_context(context.clone());
        selected.select(SelectionTarget {
            occurrence: original.clone(),
            entity: SelectedEntity::Occurrence,
        });
        d.edit_prototype(&parent, |d| {
            transforms::transform_selection(
                d,
                &mut selected,
                Affine3::from_translation(Vec3::Z),
                false,
            )
        })
        .unwrap();
        assert_eq!(
            1.,
            d.resolve_occurrence(&other_original)
                .unwrap()
                .world_transform
                .translation
                .z
        );
        selected.clear();
        selected.select(SelectionTarget {
            occurrence: copy.clone(),
            entity: SelectedEntity::Occurrence,
        });
        d.edit_prototype(&parent, |d| objects::delete_selection(d, &selected))
            .unwrap();
        assert!(d.resolve_occurrence(&copy).is_none());
        assert!(
            d.resolve_occurrence(&other.child(*copy.path.last().unwrap()))
                .is_none()
        );
        assert!(d.occurrence_edits(&parent).is_none());
        d.validate().unwrap();
    }
}

fn remap_material(extensions: &mut BTreeMap<String, Value>, ids: &BTreeMap<String, String>) {
    if let Some(id) = materials::material_id(extensions)
        .and_then(|id| ids.get(id))
        .cloned()
    {
        extensions.insert(materials::MATERIAL_KEY.into(), Value::String(id));
    }
}
