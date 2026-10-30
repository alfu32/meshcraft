//! Make Object uses the active parent's axes: Y up and +Z front, matching Paste.
use super::*;
use crate::geometry::Aabb3;

#[derive(Default)]
struct Bounds(Option<Aabb3>);
impl Bounds {
    fn point(&mut self, p: Vec3) -> Result<(), String> {
        if !p.is_finite() {
            return Err("object bounds contain a non-finite point".into());
        }
        if let Some(b) = &mut self.0 {
            b.include(p);
        } else {
            self.0 = Some(Aabb3 { min: p, max: p });
        }
        Ok(())
    }
    fn geometry(&mut self, geometry: &GeometryBundle, transform: Affine3) -> Result<(), String> {
        let mut referenced = BTreeSet::new();
        for reference in geometry
            .segments
            .values()
            .flat_map(|s| s.endpoints)
            .chain(geometry.triangles.values().flat_map(|f| f.vertices))
        {
            let p = geometry
                .vertex_position(reference)
                .ok_or("object has a missing vertex")?;
            self.point(transform.transform_point(p))?;
            referenced.insert(reference.id);
        }
        for vertex in geometry
            .vertices
            .values()
            .filter(|v| !referenced.contains(&v.id))
        {
            self.point(transform.transform_point(vertex.position))?;
        }
        Ok(())
    }
    fn annotations(
        &mut self,
        extensions: &BTreeMap<String, Value>,
        transform: Affine3,
    ) -> Result<(), String> {
        for field in ["dimensions", "texts"] {
            for record in extensions
                .get(&format!("k3d.{field}"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                for key in if field == "dimensions" {
                    &["start", "end", "offset"][..]
                } else {
                    &["position"][..]
                } {
                    if let Some(p) = record
                        .get(key)
                        .and_then(|v| serde_json::from_value::<Vec3>(v.clone()).ok())
                    {
                        self.point(transform.transform_point(p))?;
                    }
                }
                for p in record
                    .get("leaderPoints")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|v| serde_json::from_value::<Vec3>(v.clone()).ok())
                {
                    self.point(transform.transform_point(p))?;
                }
            }
        }
        Ok(())
    }
    fn anchor(&self) -> Option<Vec3> {
        self.0.map(|b| Vec3::new(b.min.x, b.min.y, b.max.z))
    }
}

pub(super) fn mesh_anchor(
    geometry: &GeometryBundle,
    extensions: &BTreeMap<String, Value>,
) -> Result<Vec3, String> {
    let mut bounds = Bounds::default();
    bounds.geometry(geometry, Affine3::IDENTITY)?;
    bounds.annotations(extensions, Affine3::IDENTITY)?;
    Ok(bounds.anchor().unwrap_or(Vec3::ZERO))
}

/// Visit only the selected subtrees; do not serialize or flatten the drawing.
pub(super) fn instances_anchor(
    document: &Document,
    roots: &[OccurrenceRef],
    parent: &OccurrenceRef,
) -> Result<Vec3, String> {
    let parent_world = if parent.is_scene_root() {
        Affine3::IDENTITY
    } else {
        document
            .resolve_occurrence(parent)
            .ok_or("object parent is missing")?
            .world_transform
    };
    let inverse = parent_world
        .inverse(GeometryTolerance::default())
        .ok_or("object parent transform is singular")?;
    let mut bounds = Bounds::default();
    let mut pending = roots.to_vec();
    let mut visited = 0usize;
    while let Some(path) = pending.pop() {
        visited += 1;
        if visited > 100_000 {
            return Err("object grouping expands to more than 100000 placements".into());
        }
        let r = document
            .resolve_occurrence(&path)
            .ok_or("selected object no longer exists")?;
        let local = inverse.compose(r.world_transform);
        // Finishes do not affect bounds. Avoid initializing the whole-scene
        // material/hotspot caches just to inspect these selected placements.
        let frame = document
            .script(r.definition.id)
            .filter(|s| s.enabled)
            .and_then(|_| document.script_frame(&path));
        if let Some(frame) = frame {
            bounds.geometry(&frame.geometry, local)?;
        } else if r.edits.is_some_and(|e| !e.hotspot_positions.is_empty()) {
            bounds.geometry(&document.evaluate_hotspots(&path)?.geometry, local)?;
        } else {
            bounds.geometry(
                r.edits
                    .and_then(|e| e.geometry_replacement.as_ref())
                    .unwrap_or(&r.definition.geometry),
                local,
            )?;
        }
        if let Some(b) = document.procedural_object_local_bounds(&path) {
            for p in crate::document::procedural::bounds_corners(b) {
                bounds.point(local.transform_point(p))?;
            }
        }
        bounds.annotations(&r.definition.extensions, local)?;
        if let Some(layout) = r
            .edits
            .and_then(|e| e.child_layout)
            .or(r.definition.default_child_layout)
            .and_then(|id| document.child_layout(id))
        {
            pending.extend(layout.entries.iter().map(|entry| path.child(entry.id)));
        }
    }
    if let Some(anchor) = bounds.anchor() {
        return Ok(anchor);
    }
    // An empty placement has no drawable bounds; retain a useful insertion point.
    for path in roots {
        if let Some(r) = document.resolve_occurrence(path) {
            bounds.point(inverse.transform_point(r.world_transform.transform_point(Vec3::ZERO)))?;
        }
    }
    Ok(bounds.anchor().unwrap_or(Vec3::ZERO))
}

pub(super) fn rebase_mesh(
    geometry: &mut GeometryBundle,
    extensions: &mut BTreeMap<String, Value>,
    origin: Vec3,
) -> Result<(), String> {
    if origin == Vec3::ZERO {
        return Ok(());
    }
    for vertex in geometry.vertices.values_mut() {
        vertex.position -= origin;
    }
    let translation = Affine3::from_translation(-origin);
    for field in [
        "dimensions",
        "texts",
        "architectureWalls",
        "architectureSlabs",
        "architectureFrames",
        "architectureStairs",
    ] {
        if let Some(records) = extensions
            .get_mut(&format!("k3d.{field}"))
            .and_then(Value::as_array_mut)
        {
            for record in records {
                *record = transform_annotation_record(field, record, translation)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::SelectionSession;

    fn close(a: Vec3, b: Vec3) {
        assert!(a.distance(b) < 1e-9, "{a:?} != {b:?}");
    }
    fn mesh_points(document: &Document, path: &OccurrenceRef) -> Vec<Vec3> {
        let r = document.resolve_occurrence(path).unwrap();
        let g = document.effective_geometry(path).unwrap();
        g.segments
            .values()
            .flat_map(|s| s.endpoints)
            .chain(g.triangles.values().flat_map(|f| f.vertices))
            .map(|v| {
                r.world_transform
                    .transform_point(g.vertex_position(v).unwrap())
            })
            .collect()
    }
    fn same_points(a: &[Vec3], b: &[Vec3]) {
        assert_eq!(a.len(), b.len());
        for p in a {
            assert!(b.iter().any(|q| p.distance(*q) < 1e-9), "missing {p:?}");
        }
    }
    #[test]
    fn raw_mesh_anchor_uses_actual_reference_offsets_and_insertion_uses_the_new_origin() {
        let mut d = Document::new();
        let insertion = d
            .append_line_segment_to_scene_root(
                Vec3::new(100., 100., 100.),
                Vec3::new(101., 100., 100.),
            )
            .unwrap();
        let edge = insertion.affected_segments[0];
        let root = d
            .definitions
            .get_mut(&Document::SCENE_ROOT_DEFINITION_ID)
            .unwrap();
        root.geometry.segments.get_mut(&edge).unwrap().endpoints[0].position_offset =
            Vec3::new(-95., -98., -90.);
        root.geometry.segments.get_mut(&edge).unwrap().endpoints[1].position_offset =
            Vec3::new(-91., -96., -88.);
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: OccurrenceRef::scene_root(),
            entity: SelectedEntity::Segment(edge),
        });
        let before = mesh_points(&d, &OccurrenceRef::scene_root());
        let grouped = group_selection(&mut d, &selection, "Anchored line").unwrap();
        let r = d.resolve_occurrence(&grouped).unwrap();
        close(r.world_transform.translation, Vec3::new(5., 2., 12.));
        same_points(&before, &mesh_points(&d, &grouped));
        let def = r.definition.id;
        let placed = place_definition(
            &mut d,
            &EditContext::Document,
            def,
            Vec3::new(20., 3., 4.),
            None,
            false,
        )
        .unwrap();
        let b = Aabb3::from_points(mesh_points(&d, &placed)).unwrap();
        close(Vec3::new(b.min.x, b.min.y, b.max.z), Vec3::new(20., 3., 4.));
    }
    #[test]
    fn grouping_instances_uses_geometry_frames_and_transform_overrides_not_their_origins() {
        let mut d = Document::new();
        let a = d
            .add_cuboid("box", Vec3::new(2., 3., 4.), Vec3::new(-8., 2., 9.))
            .unwrap();
        let def = d.resolve_occurrence(&a).unwrap().definition.id;
        d.definitions.get_mut(&def).unwrap().frame = Affine3::from_linear_translation(
            [[0., -2., 0.], [1., 0., 0.], [0., 0., 3.]],
            Vec3::new(2., 1., -3.),
        );
        let b = d.copy_root_occurrence(&a, Vec3::X * 14.).unwrap();
        let mut edits = d.occurrence_edits(&b).cloned().unwrap_or_default();
        edits.transform = Some(Affine3::from_translation(Vec3::new(11., -3., 5.)));
        d.set_occurrence_edits(b.clone(), edits);
        let before = mesh_points(&d, &a)
            .into_iter()
            .chain(mesh_points(&d, &b))
            .collect::<Vec<_>>();
        let bb = Aabb3::from_points(before.clone()).unwrap();
        let group = group_occurrences(&mut d, &[a, b], "Pair").unwrap();
        let r = d.resolve_occurrence(&group).unwrap();
        close(
            r.world_transform.translation,
            Vec3::new(bb.min.x, bb.min.y, bb.max.z),
        );
        let children = d
            .child_layout(r.definition.default_child_layout.unwrap())
            .unwrap()
            .entries
            .iter()
            .map(|e| group.child(e.id))
            .collect::<Vec<_>>();
        let after = children
            .iter()
            .flat_map(|p| mesh_points(&d, p))
            .collect::<Vec<_>>();
        same_points(&before, &after);
        let inverse = r
            .world_transform
            .inverse(GeometryTolerance::default())
            .unwrap();
        let bb = Aabb3::from_points(after.iter().map(|p| inverse.transform_point(*p))).unwrap();
        close(Vec3::new(bb.min.x, bb.min.y, bb.max.z), Vec3::ZERO);
    }
    #[test]
    fn mesh_grouping_in_rotated_scaled_prototype_keeps_world_shape_and_local_corner_anchor() {
        let mut d = Document::new();
        let parent = d
            .add_cuboid("parent", Vec3::new(1., 1., 1.), Vec3::new(10., 5., -8.))
            .unwrap();
        let def = d.resolve_occurrence(&parent).unwrap().definition.id;
        d.definitions.get_mut(&def).unwrap().frame = Affine3::from_linear_translation(
            [[0., 0., 2.], [0., 3., 0.], [-1., 0., 0.]],
            Vec3::ZERO,
        );
        let id = d
            .append_line_segment_to_definition(def, Vec3::new(4., 2., 8.), Vec3::new(9., 6., 15.))
            .unwrap();
        let parent_world = d.resolve_occurrence(&parent).unwrap().world_transform;
        let mut selection = SelectionSession::default();
        selection.context = EditContext::Occurrence(parent.clone());
        selection.select(SelectionTarget {
            occurrence: parent.clone(),
            entity: SelectedEntity::Segment(id),
        });
        let group = group_selection(&mut d, &selection, "Nested").unwrap();
        close(
            d.resolve_occurrence(&group)
                .unwrap()
                .world_transform
                .translation,
            parent_world.transform_point(Vec3::new(4., 2., 15.)),
        );
        let actual = mesh_points(&d, &group);
        same_points(
            &[
                parent_world.transform_point(Vec3::new(4., 2., 8.)),
                parent_world.transform_point(Vec3::new(9., 6., 15.)),
            ],
            &actual,
        );
    }
    #[test]
    fn architecture_geometry_and_parameters_are_rebased_together_and_roundtrip_in_place() {
        use crate::document::architecture::{Element, Wall};
        let mut d = Document::new();
        let root = d.ensure_scene_root();
        let id = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    start: Vec3::new(7., 2., 10.),
                    end: Vec3::new(13., 2., 10.),
                    height: 4.,
                    ..Default::default()
                }),
            )
            .unwrap();
        let before = mesh_points(&d, &OccurrenceRef::scene_root());
        let bounds = Aabb3::from_points(before.clone()).unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: OccurrenceRef::scene_root(),
            entity: SelectedEntity::SemanticHandle {
                kind: SelectionKind::Wall,
                id,
            },
        });
        let group = group_selection(&mut d, &selection, "Wall object").unwrap();
        close(
            d.resolve_occurrence(&group)
                .unwrap()
                .world_transform
                .translation,
            Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
        );
        same_points(&before, &mesh_points(&d, &group));
        d.regenerate_architecture();
        same_points(&before, &mesh_points(&d, &group));
        let saved = crate::legacy::export_current_value(&d).unwrap();
        let reopened = crate::legacy::import_current_value(&saved).unwrap();
        let all = reopened
            .scene_geometry_refs()
            .unwrap()
            .into_iter()
            .flat_map(|p| mesh_points(&reopened, &p))
            .collect::<Vec<_>>();
        same_points(&before, &all);
    }
}
