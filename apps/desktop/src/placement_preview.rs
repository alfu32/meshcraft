//! Prepare placement geometry once; pointer motion only changes a small affine transform.
use super::*;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct State {
    key: Option<(u64, u64, DefinitionId, EditContext)>,
    edges: Arc<Vec<[Vec3; 2]>>,
}

impl MeshcraftApp {
    pub(super) fn object_placement_preview(
        &mut self,
        definition: DefinitionId,
        hit: PlacementHit,
    ) -> Result<ToolPreview, String> {
        let key = (
            self.document_generation,
            self.document.visual_revision(),
            definition,
            self.selection.context.clone(),
        );
        if self.placement_preview.key.as_ref() != Some(&key) {
            let frame = self
                .document
                .definition(definition)
                .ok_or("prototype no longer exists")?
                .frame;
            // At the scene root and frame origin, the temporary instance's placement is identity.
            // Its edges include the definition frame, even when that frame is singular.
            let mut draft = self.document.clone();
            let placed = modeling_objects::place_definition(
                &mut draft,
                &EditContext::Definition(Document::SCENE_ROOT_DEFINITION_ID),
                definition,
                frame.translation,
                None,
                false,
            )?;
            let mut edges = BTreeMap::new();
            let point_key =
                |p: Vec3| [p.x, p.y, p.z].map(|v| if v == 0. { 0 } else { v.to_bits() });
            for path in draft
                .scene_geometry_refs()?
                .into_iter()
                .filter(|path| path.path.starts_with(&placed.path))
            {
                let Some(r) = draft.resolve_occurrence(&path) else {
                    continue;
                };
                let Some(g) = draft.effective_geometry(&path) else {
                    continue;
                };
                let world = |v| {
                    g.vertex_position(v)
                        .map(|p| r.world_transform.transform_point(p))
                };
                let mut insert = |[a, b]: [Vec3; 2]| {
                    if a == b {
                        return;
                    }
                    let (ka, kb) = (point_key(a), point_key(b));
                    edges
                        .entry(if ka <= kb { (ka, kb) } else { (kb, ka) })
                        .or_insert([a, b]);
                };
                if let Some(b) = draft.procedural_object_world_bounds(&path) {
                    let p = meshcraft_core::document::procedural::bounds_corners(b);
                    for i in 0..8 {
                        for bit in [1, 2, 4] {
                            if i & bit == 0 {
                                insert([p[i], p[i | bit]]);
                            }
                        }
                    }
                }
                for segment in g.segments.values() {
                    if let [Some(a), Some(b)] = segment.endpoints.map(world) {
                        insert([a, b]);
                    }
                }
                for triangle in g.triangles.values() {
                    if let [Some(a), Some(b), Some(c)] = triangle.vertices.map(world) {
                        for edge in [[a, b], [b, c], [c, a]] {
                            insert(edge);
                        }
                    }
                }
            }
            self.placement_preview.edges = Arc::new(edges.into_values().collect());
            self.placement_preview.key = Some(key);
        }
        let transform = modeling_objects::placement_anchor_transform(
            &self.document,
            &self.selection.context,
            definition,
            hit.position,
            hit.surface_normal,
            self.object_place_tool_state.glue_to_surface(),
        )?;
        Ok(ToolPreview::Placement {
            edges: Arc::clone(&self.placement_preview.edges),
            transform,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;

    fn bounds(
        edges: &[[Vec3; 2]],
        transform: meshcraft_core::geometry::Affine3,
    ) -> meshcraft_core::geometry::Aabb3 {
        meshcraft_core::geometry::Aabb3::from_points(
            edges
                .iter()
                .flatten()
                .map(|p| transform.transform_point(*p)),
        )
        .unwrap()
    }

    #[test]
    fn placement_reuses_edges_moves_exactly_and_matches_glued_commit() {
        let mut app = test_app();
        let source = app
            .document
            .add_cuboid("prototype", Vec3::new(2., 3., 4.), Vec3::ZERO)
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&source)
            .unwrap()
            .definition
            .id;
        // Exercise an offset/nonuniform definition inside a rotated/scaled parent, not just root placement.
        let frame = meshcraft_core::geometry::Affine3::from_linear_translation(
            [[0., -2., 0.], [1., 0., 0.], [0., 0., 3.]],
            Vec3::new(1., 2., 3.),
        );
        let mut raw = serde_json::to_value(&app.document).unwrap();
        raw["definitions"][definition.0.to_string()]["frame"] =
            serde_json::to_value(frame).unwrap();
        app.document = serde_json::from_value(raw).unwrap();
        let parent = app
            .document
            .add_definition("parent", meshcraft_core::document::DefinitionKind::Hybrid);
        let entry = app
            .document
            .add_layout_entry(
                app.document.root_layout(),
                parent,
                meshcraft_core::geometry::Affine3::from_linear_translation(
                    [[0., 0., 2.], [0., 1., 0.], [-1., 0., 0.]],
                    Vec3::new(20., 1., 4.),
                ),
            )
            .unwrap();
        app.selection
            .set_context(EditContext::Occurrence(OccurrenceRef::root(entry)));
        app.begin_placing_definition(definition);
        let revision = app.document.revision();
        let hit = PlacementHit {
            position: Vec3::new(8., 5., 2.),
            surface_normal: Some(Vec3::X),
        };
        let ToolPreview::Placement {
            edges: first,
            transform: original,
        } = app.object_placement_preview(definition, hit).unwrap()
        else {
            panic!("placement preview")
        };
        let moved_hit = PlacementHit {
            position: hit.position + Vec3::Z * 7.,
            ..hit
        };
        let ToolPreview::Placement { edges, transform } =
            app.object_placement_preview(definition, moved_hit).unwrap()
        else {
            panic!("placement preview")
        };
        assert!(Arc::ptr_eq(&first, &edges));
        assert_eq!(
            bounds(&first, original).min + Vec3::Z * 7.,
            bounds(&edges, transform).min
        );
        assert_eq!(revision, app.document.revision());
        app.object_place_tool_state.set_glue_to_surface(true);
        let ToolPreview::Placement { edges, transform } =
            app.object_placement_preview(definition, moved_hit).unwrap()
        else {
            panic!("placement preview")
        };
        assert!(
            Arc::ptr_eq(&first, &edges),
            "glue only changes the transform"
        );
        let expected = bounds(&edges, transform);
        assert_eq!(
            CommandCompletion::Completed,
            app.place_object_at_hit(moved_hit).completion
        );
        let placed = app.selected_occurrences()[0].clone();
        let r = app.document.resolve_occurrence(&placed).unwrap();
        let actual = meshcraft_core::geometry::Aabb3::from_points(
            app.document
                .effective_geometry(&placed)
                .unwrap()
                .vertices
                .values()
                .map(|v| r.world_transform.transform_point(v.position)),
        )
        .unwrap();
        assert!(
            expected.min.distance(actual.min) < 1e-9 && expected.max.distance(actual.max) < 1e-9
        );
        assert!(app.placement_preview.key.is_none());
        app.run_command("edit.undo");
        assert!(app.document.resolve_occurrence(&placed).is_none());
    }

    #[test]
    fn placement_rebuilds_after_visual_or_context_changes_and_cancel_releases_geometry() {
        let mut app = test_app();
        let source = app
            .document
            .add_cuboid("prototype", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&source)
            .unwrap()
            .definition
            .id;
        app.begin_placing_definition(definition);
        let hit = PlacementHit {
            position: Vec3::ZERO,
            surface_normal: None,
        };
        app.object_placement_preview(definition, hit).unwrap();
        let first = Arc::clone(&app.placement_preview.edges);
        app.document
            .append_planar_rectangle_to_definition(
                definition,
                [
                    Vec3::new(0., 2., 0.),
                    Vec3::new(3., 2., 0.),
                    Vec3::new(3., 2., 3.),
                    Vec3::new(0., 2., 3.),
                ],
            )
            .unwrap();
        app.object_placement_preview(definition, hit).unwrap();
        assert!(!Arc::ptr_eq(&first, &app.placement_preview.edges));
        assert_eq!(
            bounds(
                &app.placement_preview.edges,
                meshcraft_core::geometry::Affine3::IDENTITY
            )
            .max,
            Vec3::new(3., 2., 3.)
        );
        let changed = Arc::clone(&app.placement_preview.edges);
        app.document_generation += 1;
        app.object_placement_preview(definition, hit).unwrap();
        assert!(!Arc::ptr_eq(&changed, &app.placement_preview.edges));
        app.cancel_active_tool();
        assert!(app.placement_preview.key.is_none());
        assert!(app.placement_preview.edges.is_empty());
    }
}
