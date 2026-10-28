//! Explicit snapshots of evaluated objects. GPU meshes use the retained, shared GL mesh
//! renderer, without re-running scripts, architectural generators or handle evaluation.
use super::*;

pub const GPU_MESH_KEY: &str = "k3d.meshcraftBakedGpuMesh";

impl Document {
    pub fn is_baked_gpu_object(&self, path: &OccurrenceRef) -> bool {
        self.resolve_occurrence(path).is_some_and(|r| {
            r.definition
                .extensions
                .get(GPU_MESH_KEY)
                .and_then(Value::as_bool)
                == Some(true)
        })
    }
}

/// Paths needing a graphics-owner snapshot, including procedural descendants.
pub fn shader_capture_paths(
    document: &Document,
    roots: &[OccurrenceRef],
) -> Result<Vec<OccurrenceRef>, String> {
    let roots = roots.iter().cloned().collect::<BTreeSet<_>>();
    Ok(document
        .scene_geometry_refs()?
        .into_iter()
        .filter(|p| {
            selected_ancestor(p, &roots).is_some() && document.procedural_object(p).is_some()
        })
        .collect())
}

fn selected_ancestor<'a>(
    path: &OccurrenceRef,
    roots: &'a BTreeSet<OccurrenceRef>,
) -> Option<&'a OccurrenceRef> {
    let mut prefix = path.clone();
    loop {
        if let Some(root) = roots.get(&prefix) {
            return Some(root);
        }
        prefix.path.pop()?;
    }
}

/// Freezes complete evaluated trees, preserving placement and materials. The original
/// prototypes remain available and other occurrences retain their parametric behavior.
/// `captured` contains local-space geometry read back from procedural vertex shaders.
pub fn bake_objects(
    document: &mut Document,
    selection: &SelectionSession,
    to_shader: bool,
    captured: &BTreeMap<OccurrenceRef, GeometryBundle>,
) -> Result<Vec<OccurrenceRef>, String> {
    if selection.selected().is_empty()
        || selection
            .selected()
            .iter()
            .any(|t| t.entity != SelectedEntity::Occurrence || t.occurrence.is_scene_root())
    {
        return Err("Select object instances to bake.".into());
    }
    let mut roots: Vec<_> = selection
        .selected()
        .iter()
        .map(|t| t.occurrence.clone())
        .collect();
    roots.sort();
    roots.dedup();
    let mut non_overlapping: Vec<OccurrenceRef> = vec![];
    for path in roots {
        if non_overlapping
            .last()
            .is_none_or(|parent| !path.path.starts_with(&parent.path))
        {
            non_overlapping.push(path);
        }
    }
    let roots = non_overlapping;
    let root_set = roots.iter().cloned().collect::<BTreeSet<_>>();
    let scene = document.scene_geometry_refs()?;
    let mut subtrees = BTreeMap::<OccurrenceRef, Vec<&OccurrenceRef>>::new();
    for path in &scene {
        if let Some(root) = selected_ancestor(path, &root_set) {
            subtrees.entry(root.clone()).or_default().push(path);
        }
    }
    let mut draft = document.clone();
    let mut plans = vec![];
    for root in &roots {
        let r = document
            .resolve_occurrence(root)
            .ok_or("Object no longer exists")?;
        if to_shader && document.is_baked_gpu_object(root) {
            return Err("This object is already a GPU mesh. Use Bake to Mesh to edit it.".into());
        }
        if !to_shader
            && !document.is_baked_gpu_object(root)
            && !subtrees.get(root).is_some_and(|paths| {
                paths
                    .iter()
                    .any(|p| document.procedural_object(p).is_some())
            })
        {
            return Err("Select a GPU or shader-generated object to bake to mesh.".into());
        }
        let inverse = r
            .world_transform
            .inverse(GeometryTolerance::default())
            .ok_or("Cannot bake a singular object transform")?;
        let mut geometry = GeometryBundle {
            id: GeometryBundleId(draft.allocate_id()),
            ..Default::default()
        };
        for path in subtrees.get(root).into_iter().flatten().copied() {
            let child = document
                .resolve_occurrence(path)
                .ok_or("Object descendant no longer exists")?;
            let supported_domain = |field: &str| {
                matches!(
                    field,
                    "hotspots"
                        | "architectureWalls"
                        | "architectureSlabs"
                        | "architectureFrames"
                        | "architectureStairs"
                )
            };
            if child
                .definition
                .domain_data
                .keys()
                .any(|field| !supported_domain(field))
                || child
                    .definition
                    .domain_entity_ids
                    .iter()
                    .any(|(field, ids)| !ids.is_empty() && !supported_domain(field))
            {
                return Err("This object contains semantic content without a mesh bake adapter; convert that content to geometry first".into());
            }
            if ["dimensions", "texts"].iter().any(|f| {
                child
                    .definition
                    .domain_entity_ids
                    .get(*f)
                    .is_some_and(|ids| !ids.is_empty())
                    || child
                        .definition
                        .extensions
                        .get(&format!("k3d.{f}"))
                        .and_then(Value::as_array)
                        .is_some_and(|v| !v.is_empty())
            }) {
                return Err("Bake annotations to geometry before baking this object.".into());
            }
            if document
                .script(child.definition.id)
                .is_some_and(|s| s.enabled)
                && document.script_frame(path).is_none()
            {
                return Err("Wait for the object script to evaluate before baking.".into());
            }
            let source = if document.procedural_object(path).is_some() {
                captured
                    .get(path)
                    .ok_or("This object needs a GPU geometry capture before baking")?
            } else {
                document
                    .effective_geometry(path)
                    .ok_or("Object geometry is unavailable")?
            };
            source.validate()?;
            let mut source = source.clone();
            source.extensions.clear();
            for vertex in source.vertices.values_mut() {
                vertex
                    .legacy_aliases
                    .retain(|a| a.source != crate::document::architecture::GENERATED);
            }
            // Generated architecture becomes ordinary geometry, never stale semantic owners.
            for e in source.segments.values_mut() {
                e.extensions
                    .remove(crate::document::architecture::GENERATED);
            }
            for f in source.triangles.values_mut() {
                f.extensions
                    .remove(crate::document::architecture::GENERATED);
            }
            append_transformed_bundle(
                &mut draft,
                &source,
                inverse.compose(child.world_transform),
                &mut geometry,
            );
        }
        if geometry.is_empty() {
            return Err("The object has no geometry to bake.".into());
        }
        let id = draft.add_definition(
            format!(
                "{} ({})",
                r.definition.name,
                if to_shader { "GPU" } else { "mesh" }
            ),
            DefinitionKind::Mesh,
        );
        let def = draft.definitions.get_mut(&id).unwrap();
        def.geometry = geometry;
        if to_shader {
            def.extensions.insert(GPU_MESH_KEY.into(), true.into());
        }
        if let Some(ifc) = r.definition.extensions.get(crate::document::ifc::KEY) {
            def.extensions
                .insert(crate::document::ifc::KEY.into(), ifc.clone());
        }
        plans.push((root.clone(), id, r.world_transform));
    }
    let mut layouts = BTreeMap::new();
    let mut results = vec![];
    for (root, id, world) in plans {
        let parent = OccurrenceRef {
            path: root.path[..root.path.len() - 1].to_vec(),
        };
        let original_layout = child_layout_for_parent(document, &parent)?;
        let old_index = document
            .child_layout(original_layout)
            .unwrap()
            .entries
            .iter()
            .position(|e| Some(&e.id) == root.path.last())
            .ok_or("Object placement is missing")?;
        let layout = if parent.is_scene_root() {
            draft.root_layout
        } else if let Some(layout) = layouts.get(&parent) {
            *layout
        } else {
            let layout = match authored_child_layout(document, &selection.context, &root) {
                Some(layout) => layout,
                None => fork_occurrence_child_layout(&mut draft, &parent)?,
            };
            layouts.insert(parent.clone(), layout);
            layout
        };
        let parent_world = document
            .resolve_occurrence(&parent)
            .ok_or("Parent is missing")?
            .world_transform;
        let local = parent_world
            .inverse(GeometryTolerance::default())
            .ok_or("Parent transform is singular")?
            .compose(world);
        let entry = &mut draft.child_layouts.get_mut(&layout).unwrap().entries[old_index];
        entry.definition = id;
        entry.transform = local;
        entry.default_edits = InstanceEdits::default();
        let path = parent.child(entry.id);
        let ifc = document.instance_ifc(&root);
        draft
            .occurrence_edits
            .retain(|p, _| !p.path.starts_with(&path.path));
        draft.set_instance_ifc(&path, ifc)?;
        results.push(path);
    }
    draft.bump_revision();
    draft.validate()?;
    *document = draft;
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selection(paths: &[OccurrenceRef]) -> SelectionSession {
        let mut s = SelectionSession::default();
        for p in paths {
            s.select(SelectionTarget {
                occurrence: p.clone(),
                entity: SelectedEntity::Occurrence,
            });
        }
        s
    }
    #[test]
    fn bake_roundtrip_preserves_placement_materials_and_other_instances() {
        let mut d = Document::new();
        let a = d
            .add_cuboid("Box", Vec3::new(2., 3., 4.), Vec3::new(9., 1., 5.))
            .unwrap();
        let b = duplicate_occurrence(&mut d, &a, Vec3::X * 10.).unwrap();
        let original = d.effective_geometry(&a).unwrap().clone();
        let world = d.resolve_occurrence(&a).unwrap().world_transform;
        let old_id = d.resolve_occurrence(&a).unwrap().definition.id;
        let baked = bake_objects(
            &mut d,
            &selection(std::slice::from_ref(&a)),
            true,
            &BTreeMap::new(),
        )
        .unwrap()
        .remove(0);
        assert!(d.is_baked_gpu_object(&baked));
        assert_eq!(world, d.resolve_occurrence(&baked).unwrap().world_transform);
        assert_eq!(old_id, d.resolve_occurrence(&b).unwrap().definition.id);
        assert_eq!(original, d.effective_geometry(&b).unwrap().clone());
        let gpu = d.effective_geometry(&baked).unwrap().clone();
        let restored = bake_objects(&mut d, &selection(&[baked]), false, &BTreeMap::new())
            .unwrap()
            .remove(0);
        assert!(!d.is_baked_gpu_object(&restored));
        assert_eq!(
            world,
            d.resolve_occurrence(&restored).unwrap().world_transform
        );
        let mesh = d.effective_geometry(&restored).unwrap();
        assert_eq!(gpu.segments.len(), mesh.segments.len());
        assert_eq!(gpu.triangles.len(), mesh.triangles.len());
        assert_eq!(
            original
                .triangles
                .values()
                .map(|t| (&t.color, &t.extensions))
                .collect::<Vec<_>>(),
            mesh.triangles
                .values()
                .map(|t| (&t.color, &t.extensions))
                .collect::<Vec<_>>()
        );
        d.validate().unwrap();
    }
    #[test]
    fn nested_occurrence_bake_is_private_and_flattens_children() {
        let mut d = Document::new();
        let a = d
            .add_cuboid("Child", Vec3::new(1., 2., 3.), Vec3::new(4., 0., 0.))
            .unwrap();
        let parent = group_selection(&mut d, &selection(&[a]), "Parent").unwrap();
        let other = duplicate_occurrence(&mut d, &parent, Vec3::Z * 10.).unwrap();
        let child = d
            .scene_geometry_refs()
            .unwrap()
            .into_iter()
            .find(|p| p.path.len() > parent.path.len() && p.path.starts_with(&parent.path))
            .unwrap();
        let other_child = d
            .scene_geometry_refs()
            .unwrap()
            .into_iter()
            .find(|p| p.path.len() > other.path.len() && p.path.starts_with(&other.path))
            .unwrap();
        let old_id = d.resolve_occurrence(&other_child).unwrap().definition.id;
        let world = d.resolve_occurrence(&child).unwrap().world_transform;
        let baked = bake_objects(&mut d, &selection(&[child]), true, &BTreeMap::new())
            .unwrap()
            .remove(0);
        assert_eq!(world, d.resolve_occurrence(&baked).unwrap().world_transform);
        assert!(d.is_baked_gpu_object(&baked));
        assert_eq!(
            old_id,
            d.resolve_occurrence(&other_child).unwrap().definition.id
        );
        let flat = bake_objects(&mut d, &selection(&[parent]), true, &BTreeMap::new())
            .unwrap()
            .remove(0);
        let def = d.resolve_occurrence(&flat).unwrap().definition;
        assert!(def.default_child_layout.is_none());
        assert_eq!(12, def.geometry.triangles.len());
        d.validate().unwrap();
    }
    #[test]
    fn unhandled_semantic_content_is_not_silently_lost() {
        let mut d = Document::new();
        let path = d
            .add_cuboid("Mixed", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let id = d.resolve_occurrence(&path).unwrap().definition.id;
        d.definitions
            .get_mut(&id)
            .unwrap()
            .domain_data
            .insert("voxels".into(), serde_json::json!([{"id":"v"}]));
        let before = d.clone();
        assert!(
            bake_objects(&mut d, &selection(&[path]), true, &BTreeMap::new())
                .unwrap_err()
                .contains("semantic content")
        );
        assert_eq!(before, d);
    }
    #[test]
    fn missing_capture_is_atomic_and_captured_tree_becomes_mesh() {
        let mut d = Document::new();
        let id = d.create_procedural_tree().unwrap();
        let tree =
            place_definition(&mut d, &EditContext::Document, id, Vec3::ZERO, None, false).unwrap();
        let before = d.clone();
        assert!(
            bake_objects(
                &mut d,
                &selection(std::slice::from_ref(&tree)),
                false,
                &BTreeMap::new()
            )
            .is_err()
        );
        assert_eq!(before, d);
        let box_path = d
            .add_cuboid("Capture stand-in", Vec3::new(1., 2., 1.), Vec3::ZERO)
            .unwrap();
        let captured = BTreeMap::from([(
            tree.clone(),
            d.effective_geometry(&box_path).unwrap().clone(),
        )]);
        let paths = bake_objects(&mut d, &selection(&[tree]), false, &captured).unwrap();
        assert!(d.procedural_object(&paths[0]).is_none());
        assert_eq!(12, d.effective_geometry(&paths[0]).unwrap().triangles.len());
    }
}
