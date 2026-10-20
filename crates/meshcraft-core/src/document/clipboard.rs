//! Portable selection snapshots. Object placements retain prototype identity within a session.
use super::*;
use crate::operations::objects;
use crate::selection::{EditContext, SelectedEntity, SelectionSession, SelectionTarget};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipboardSelection {
    pub format: String,
    pub scene: Document,
    pub reusable: BTreeSet<DefinitionId>,
}

impl ClipboardSelection {
    pub fn capture(document: &Document, selection: &SelectionSession) -> Result<Self, String> {
        if selection.selected().is_empty() {
            return Err("select elements to copy".into());
        }
        let mut scene = document.clone();
        scene.persist_script_states();
        let reusable = scene.definitions().map(|d| d.id).collect();
        let objects = selection
            .selected()
            .iter()
            .filter(|t| t.entity == SelectedEntity::Occurrence)
            .map(|t| t.occurrence.clone())
            .collect::<BTreeSet<_>>();
        let objects = objects
            .iter()
            .filter(|p| {
                !objects
                    .iter()
                    .any(|a| a.path.len() < p.path.len() && p.path.starts_with(&a.path))
            })
            .cloned()
            .collect::<Vec<_>>();
        if objects.iter().any(|o| o.path.is_empty()) {
            return Err("copy the scene's elements rather than the scene root".into());
        }
        let mut mesh = BTreeMap::<OccurrenceRef, SelectionSession>::new();
        for target in selection.selected() {
            if target.entity == SelectedEntity::Occurrence
                || objects
                    .iter()
                    .any(|o| target.occurrence.path.starts_with(&o.path))
            {
                continue;
            }
            let group = mesh.entry(target.occurrence.clone()).or_insert_with(|| {
                let mut s = SelectionSession::default();
                s.set_context(selection.context.clone());
                s
            });
            group.select(target.clone());
        }
        let mut roots = objects;
        for group in mesh.values() {
            roots.push(objects::group_mesh_selection(
                &mut scene,
                group,
                "Clipboard geometry",
            )?);
        }
        let mut entries = Vec::new();
        let mut overrides = BTreeMap::new();

        for occurrence in roots {
            let resolved = scene
                .resolve_occurrence(&occurrence)
                .ok_or("copied occurrence is missing")?;
            let definition = resolved.definition.id;
            let transform = resolved.world_transform.compose(
                resolved
                    .definition
                    .frame
                    .inverse(crate::geometry::GeometryTolerance::default())
                    .ok_or("singular object frame")?,
            );
            let default_edits = resolved.edits.cloned().unwrap_or_default();
            let id = LayoutEntryId(scene.allocate_id());
            for (path, edits) in &scene.occurrence_edits {
                if path.path.len() > occurrence.path.len()
                    && path.path.starts_with(&occurrence.path)
                {
                    overrides.insert(
                        OccurrenceRef {
                            path: [vec![id], path.path[occurrence.path.len()..].to_vec()].concat(),
                        },
                        edits.clone(),
                    );
                }
            }
            entries.push(LayoutEntry {
                id,
                definition,
                transform,
                default_edits,
                legacy_aliases: vec![],
            });
        }
        let root_layout = ChildLayoutId(scene.allocate_id());
        scene.root_layout = root_layout;
        scene.child_layouts.insert(
            root_layout,
            ChildLayout {
                id: root_layout,
                entries,
                extensions: BTreeMap::new(),
            },
        );
        scene.occurrence_edits = overrides;
        scene
            .extensions
            .retain(|key, _| key == materials::CATALOG_KEY);
        scene
            .definitions
            .remove(&Document::SCENE_ROOT_DEFINITION_ID);
        scene.ensure_scene_root();
        // Copy only reachable content, including layouts owned by instance overrides.
        let mut definitions = BTreeSet::new();
        let mut layouts = BTreeSet::new();
        let mut pending_definitions = vec![Document::SCENE_ROOT_DEFINITION_ID];
        let mut pending_layouts = vec![root_layout];
        pending_layouts.extend(
            scene
                .occurrence_edits
                .values()
                .filter_map(|e| e.child_layout),
        );
        while !pending_definitions.is_empty() || !pending_layouts.is_empty() {
            while let Some(id) = pending_definitions.pop() {
                if !definitions.insert(id) {
                    continue;
                }
                if let Some(layout) = scene.definition(id).and_then(|d| d.default_child_layout) {
                    pending_layouts.push(layout);
                }
            }
            while let Some(id) = pending_layouts.pop() {
                if !layouts.insert(id) {
                    continue;
                }
                for e in &scene
                    .child_layout(id)
                    .ok_or("copied child layout is missing")?
                    .entries
                {
                    pending_definitions.push(e.definition);
                    if let Some(l) = e.default_edits.child_layout {
                        pending_layouts.push(l);
                    }
                }
            }
        }
        scene.definitions.retain(|id, _| definitions.contains(id));
        scene.child_layouts.retain(|id, _| layouts.contains(id));
        scene.hotspot_cache.clear();
        scene.script_cache = Default::default();
        scene.validate()?;
        Ok(Self {
            format: "meshcraft-selection-1".into(),
            scene,
            reusable,
        })
    }

    /// Paste at a world-space translation; original coordinates are preserved at zero offset.
    pub fn paste(
        &self,
        document: &mut Document,
        context: &EditContext,
        delta: Vec3,
        reuse: bool,
    ) -> Result<Vec<SelectionTarget>, String> {
        if self.format != "meshcraft-selection-1" {
            return Err("unsupported clipboard format".into());
        }
        let mut draft = document.clone();
        let no_reuse = BTreeSet::new();
        let mut copied_edits = Vec::new();
        let prototype = draft.import_subscene_reusing(
            &self.scene,
            "Clipboard",
            None,
            None,
            if reuse { &self.reusable } else { &no_reuse },
            Some(&mut copied_edits),
        )?;
        let layout = draft
            .definition(prototype)
            .and_then(|d| d.default_child_layout)
            .and_then(|l| draft.child_layout(l))
            .ok_or("clipboard layout is missing")?
            .clone();
        let mut selected = Vec::new();
        for (index, entry) in layout.entries.into_iter().enumerate() {
            let fragment = !self.reusable.contains(
                &self.scene.child_layouts[&self.scene.root_layout].entries[index].definition,
            );
            if fragment {
                let owner = match context {
                    EditContext::Document => draft
                        .scene_container_occurrence()
                        .unwrap_or_else(OccurrenceRef::scene_root),
                    EditContext::Occurrence(path) => path.clone(),
                    EditContext::Definition(id) => draft
                        .scene_geometry_refs()?
                        .into_iter()
                        .find(|p| {
                            draft
                                .resolve_occurrence(p)
                                .is_some_and(|r| r.definition.id == *id)
                        })
                        .ok_or("paste owner has no placement")?,
                };
                let r = draft
                    .resolve_occurrence(&owner)
                    .ok_or("paste owner is missing")?;
                let destination = r.definition.id;
                let local = r
                    .world_transform
                    .inverse(crate::geometry::GeometryTolerance::default())
                    .ok_or("singular paste owner")?
                    .compose(Affine3::from_translation(delta))
                    .compose(entry.transform);
                let definition = draft
                    .definition(entry.definition)
                    .ok_or("clipboard fragment is missing")?
                    .clone();
                let local = local.compose(definition.frame);
                let mut geometry = definition.geometry.clone();
                for vertex in geometry.vertices.values_mut() {
                    vertex.position = local.transform_point(vertex.position);
                }
                for edge in geometry.segments.values_mut() {
                    for reference in &mut edge.endpoints {
                        reference.position_offset =
                            local.transform_vector(reference.position_offset);
                    }
                }
                for face in geometry.triangles.values_mut() {
                    for reference in &mut face.vertices {
                        reference.position_offset =
                            local.transform_vector(reference.position_offset);
                    }
                }
                selected.extend(geometry.segments.keys().map(|id| SelectionTarget {
                    occurrence: owner.clone(),
                    entity: SelectedEntity::Segment(*id),
                }));
                selected.extend(geometry.triangles.keys().map(|id| SelectionTarget {
                    occurrence: owner.clone(),
                    entity: SelectedEntity::Triangle(*id),
                }));
                if geometry.segments.is_empty() && geometry.triangles.is_empty() {
                    selected.extend(geometry.vertices.keys().map(|id| SelectionTarget {
                        occurrence: owner.clone(),
                        entity: SelectedEntity::Vertex(*id),
                    }));
                }
                if !draft.append_geometry_to_definition(destination, geometry) {
                    return Err("cannot append clipboard geometry".into());
                }
                for (field, kind) in [
                    ("dimensions", crate::selection::SelectionKind::Dimension),
                    ("texts", crate::selection::SelectionKind::Text),
                ] {
                    for record in definition
                        .extensions
                        .get(&format!("k3d.{field}"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let record = crate::operations::objects::transform_annotation_record(
                            field, record, local,
                        )?;
                        let id = draft.add_annotation(destination, field, record)?;
                        selected.push(SelectionTarget {
                            occurrence: owner.clone(),
                            entity: SelectedEntity::SemanticHandle { kind, id },
                        });
                    }
                }
                draft.definitions.remove(&entry.definition);
                continue;
            }
            let occurrence = objects::place_definition(
                &mut draft,
                context,
                entry.definition,
                delta,
                None,
                false,
            )?;
            let parent = OccurrenceRef {
                path: occurrence.path[..occurrence.path.len() - 1].to_vec(),
            };
            let parent_world = if parent.path.is_empty() {
                Affine3::IDENTITY
            } else {
                draft
                    .resolve_occurrence(&parent)
                    .ok_or("paste parent is missing")?
                    .world_transform
            };
            let transform = parent_world
                .inverse(crate::geometry::GeometryTolerance::default())
                .ok_or("singular paste context")?
                .compose(Affine3::from_translation(delta))
                .compose(entry.transform);
            let id = occurrence.path.last().unwrap();
            let placed = draft
                .child_layouts
                .values_mut()
                .flat_map(|l| &mut l.entries)
                .find(|e| e.id == *id)
                .ok_or("paste placement is missing")?;
            placed.transform = transform;
            placed.default_edits = entry.default_edits;
            for (path, edits) in &copied_edits {
                if path.path.first() == Some(&entry.id) {
                    let mapped = OccurrenceRef {
                        path: [occurrence.path.clone(), path.path[1..].to_vec()].concat(),
                    };
                    if draft.resolve_occurrence(&mapped).is_some() {
                        draft.occurrence_edits.insert(mapped, edits.clone());
                    }
                }
            }
            selected.push(SelectionTarget {
                occurrence,
                entity: SelectedEntity::Occurrence,
            });
        }
        // Remove the temporary container; its imported children are now placed directly.
        draft.definitions.remove(&prototype);
        draft.child_layouts.remove(&layout.id);
        draft.validate()?;
        draft.bump_revision();
        *document = draft;
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_reuses_prototypes_and_preserves_instance_state_but_cross_document_imports() {
        let mut doc = Document::new();
        let original = doc
            .add_cuboid("box", Vec3::new(2., 3., 4.), Vec3::ZERO)
            .unwrap();
        let id = doc.resolve_occurrence(&original).unwrap().definition.id;
        let mut edits = InstanceEdits::default();
        edits.values.insert("custom".into(), serde_json::json!(123));
        doc.set_occurrence_edits(original.clone(), edits);
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: original.clone(),
            entity: SelectedEntity::Occurrence,
        });
        let snapshot = ClipboardSelection::capture(&doc, &selection).unwrap();
        let encoded = serde_json::to_string(&snapshot).unwrap();
        let snapshot: ClipboardSelection = serde_json::from_str(&encoded).unwrap();
        let pasted = snapshot
            .paste(
                &mut doc,
                &EditContext::Document,
                Vec3::new(10., 0., 0.),
                true,
            )
            .unwrap();
        let copy = &pasted[0].occurrence;
        assert_ne!(&original, copy);
        assert_eq!(id, doc.resolve_occurrence(copy).unwrap().definition.id);
        assert_eq!(
            Some(&serde_json::json!(123)),
            doc.occurrence_edits(copy).unwrap().values.get("custom")
        );
        let mut other = Document::new();
        other
            .add_cuboid("unrelated", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let pasted = snapshot
            .paste(&mut other, &EditContext::Document, Vec3::ZERO, false)
            .unwrap();
        assert_ne!(
            id,
            other
                .resolve_occurrence(&pasted[0].occurrence)
                .unwrap()
                .definition
                .id
        );
        assert_eq!(
            12,
            other
                .effective_geometry(&pasted[0].occurrence)
                .unwrap()
                .triangles
                .len()
        );
        other.validate().unwrap();
    }
    #[test]
    fn mesh_fragment_copy_is_independent_and_keeps_color() {
        let mut doc = Document::new();
        let (segments, _) = doc
            .append_planar_mesh_to_scene_root(
                vec![Vec3::ZERO, Vec3::X, Vec3::Z],
                vec![[0, 1], [1, 2]],
                vec![],
            )
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: OccurrenceRef::scene_root(),
            entity: SelectedEntity::Segment(segments[0]),
        });
        let snapshot = ClipboardSelection::capture(&doc, &selection).unwrap();
        let pasted = snapshot
            .paste(&mut doc, &EditContext::Document, Vec3::Y, true)
            .unwrap();
        assert_eq!(1, pasted.len());
        let g = doc.effective_geometry(&pasted[0].occurrence).unwrap();
        assert_eq!(3, g.segments.len());
        let SelectedEntity::Segment(new_edge) = pasted[0].entity else {
            panic!("pasted edge");
        };
        let new_segment = &g.segments[&new_edge];
        assert!(
            new_segment
                .endpoints
                .iter()
                .all(|p| (g.vertex_position(*p).unwrap().y - 1.).abs() < 1e-9)
        );
        assert_eq!(
            3,
            doc.scene_root_definition().unwrap().geometry.segments.len()
        );
    }
}

#[cfg(test)]
mod nested_tests {
    use super::*;
    #[test]
    fn nested_overrides_are_copied_and_future_prototype_children_remain_shared() {
        let mut doc = Document::new();
        let child = doc
            .add_cuboid("child", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let parent = objects::group_occurrences(&mut doc, &[child], "parent").unwrap();
        let parent_def = doc.resolve_occurrence(&parent).unwrap().definition.id;
        let layout = doc
            .definition(parent_def)
            .unwrap()
            .default_child_layout
            .unwrap();
        let child_id = doc.child_layout(layout).unwrap().entries[0].id;
        let mut state = InstanceEdits::default();
        state
            .values
            .insert("child-state".into(), serde_json::json!(42));
        doc.set_occurrence_edits(parent.child(child_id), state);
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: parent.clone(),
            entity: SelectedEntity::Occurrence,
        });
        let snapshot = ClipboardSelection::capture(&doc, &selection).unwrap();
        for reuse in [true, false] {
            let mut destination = if reuse { doc.clone() } else { Document::new() };
            let pasted = snapshot
                .paste(
                    &mut destination,
                    &EditContext::Document,
                    Vec3::X * 5.,
                    reuse,
                )
                .unwrap();
            let path = &pasted[0].occurrence;
            let r = destination.resolve_occurrence(path).unwrap();
            assert!(
                r.edits.is_none_or(|e| e.child_layout.is_none()),
                "ordinary copies must keep the shared child layout"
            );
            let children = destination
                .child_layout(r.definition.default_child_layout.unwrap())
                .unwrap();
            let child = path.child(children.entries[0].id);
            assert_eq!(
                Some(&serde_json::json!(42)),
                destination
                    .occurrence_edits(&child)
                    .unwrap()
                    .values
                    .get("child-state")
            );
            destination.validate().unwrap();
        }
    }
}
