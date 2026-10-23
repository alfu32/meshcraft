//! Selection-based fill, solid booleans and two-point plane sections.
use super::*;
use meshcraft_core::operations::{
    fill,
    solids::{self, Boolean, Face},
};
#[derive(Default)]
pub(super) struct SectionState {
    pub origin: Option<Vec3>,
    faces: Vec<Face>,
    revision: u64,
}
impl MeshcraftApp {
    fn mesh_faces_for(&self, targets: &BTreeSet<SelectionTarget>) -> Vec<Face> {
        let mut result = vec![];
        for path in self.document.scene_geometry_refs().unwrap_or_default() {
            let (Some(r), Some(g)) = (
                self.document.resolve_occurrence(&path),
                self.document
                    .geometry_in_context(&path, &self.selection.context),
            ) else {
                continue;
            };
            for f in g.triangles.values() {
                let raw = SelectionTarget {
                    occurrence: path.clone(),
                    entity: SelectedEntity::Triangle(f.id),
                };
                let owner = meshcraft_core::document::architecture::semantic_entity(&f.extensions)
                    .map(|entity| SelectionTarget {
                        occurrence: path.clone(),
                        entity,
                    });
                if !targets.contains(&raw)
                    && owner.as_ref().is_none_or(|o| !targets.contains(o))
                    && !targets.iter().any(|t| {
                        t.entity == SelectedEntity::Occurrence
                            && path.path.starts_with(&t.occurrence.path)
                    })
                {
                    continue;
                }
                let [Some(a), Some(b), Some(c)] = f.vertices.map(|v| g.vertex_position(v)) else {
                    continue;
                };
                let mut points = [a, b, c].map(|p| r.world_transform.transform_point(p));
                if r.world_transform.determinant() < 0. {
                    points.swap(1, 2);
                }
                result.push(Face {
                    points,
                    color: f.color,
                    extensions: f.extensions.clone(),
                });
            }
        }
        result
    }
    pub(super) fn fill_selected_loops(&mut self) -> CommandResult {
        let source = self.selected_world_segments();
        let mesh = match fill::fill_segments(&source, POLYLINE_MESH_MERGE_TOLERANCE) {
            Ok(m) => m,
            Err(e) => return CommandResult::failed(e),
        };
        let (owner, path, inverse) = self.line_geometry_context();
        let vertices = mesh
            .vertices
            .into_iter()
            .map(|p| inverse.transform_point(p))
            .collect();
        let triangles = match self.commit_document_edit("Fill Selected Loops", |d| match owner {
            LineOwner::SceneRoot => d.append_triangles_to_scene_root(vertices, mesh.triangles),
            LineOwner::Definition(id) => {
                d.append_triangles_to_definition(id, vertices, mesh.triangles)
            }
        }) {
            Ok(ids) => ids,
            Err(e) => return CommandResult::failed(e),
        };
        let count = triangles.len();
        self.selection.apply(
            triangles.into_iter().map(|id| SelectionTarget {
                occurrence: path.clone(),
                entity: SelectedEntity::Triangle(id),
            }),
            SelectionMode::Replace,
        );
        self.status = format!("Filled selected loops: {count} face(s).");
        CommandResult::completed()
    }
    pub(super) fn solid_boolean(&mut self, operation: Boolean) -> CommandResult {
        let targets = self.selection.ordered_selected().to_vec();
        if targets.len() != 2
            || targets
                .iter()
                .any(|t| t.entity != SelectedEntity::Occurrence)
        {
            return CommandResult::failed(
                "Select exactly two closed mesh object instances in the active graph.",
            );
        }
        let parent = |t: &SelectionTarget| {
            t.occurrence.path[..t.occurrence.path.len().saturating_sub(1)].to_vec()
        };
        if parent(&targets[0]) != parent(&targets[1])
            || targets.iter().any(|t| !self.native_target_editable(t))
        {
            return CommandResult::failed("The two editable solids must share a parent graph.");
        }
        let a = self.mesh_faces_for(&BTreeSet::from([targets[0].clone()]));
        let b = self.mesh_faces_for(&BTreeSet::from([targets[1].clone()]));
        let result = match solids::boolean(&a, &b, operation) {
            Ok(result) => result,
            Err(e) => return CommandResult::failed(e),
        };
        let (owner, path, inverse) = self.line_geometry_context();
        let faces: Vec<_> = result
            .into_iter()
            .map(|mut f| {
                f.points = f.points.map(|p| inverse.transform_point(p));
                f
            })
            .collect();
        let source = self.selection.clone();
        let result = self.commit_document_edit("Solid Boolean", |d| {
            modeling_objects::delete_selection(d, &source)?;
            let definition = match owner {
                LineOwner::SceneRoot => d.ensure_scene_root(),
                LineOwner::Definition(id) => id,
            };
            solids::append_result(d, definition, faces, vec![])
        });
        match result {
            Ok((triangles, edges)) => {
                let count = triangles.len();
                self.selection.apply(
                    triangles
                        .into_iter()
                        .map(|id| SelectionTarget {
                            occurrence: path.clone(),
                            entity: SelectedEntity::Triangle(id),
                        })
                        .chain(edges.into_iter().map(|id| SelectionTarget {
                            occurrence: path.clone(),
                            entity: SelectedEntity::Segment(id),
                        })),
                    SelectionMode::Replace,
                );
                self.status = format!("Solid {operation:?}: {count} result faces.");
                CommandResult::completed()
            }
            Err(e) => CommandResult::failed(e),
        }
    }
    pub(super) fn intersect_mesh_selection(&mut self) -> CommandResult {
        let targets: Vec<_> = self
            .selection
            .selected()
            .iter()
            .filter(|t| t.entity == SelectedEntity::Occurrence)
            .cloned()
            .collect();
        if targets.len() != 2 || self.selection.selected().len() != 2 {
            return CommandResult::failed("Select exactly two mesh objects to intersect.");
        }
        let a = self.mesh_faces_for(&BTreeSet::from([targets[0].clone()]));
        let b = self.mesh_faces_for(&BTreeSet::from([targets[1].clone()]));
        let lines = match solids::intersections(&a, &b) {
            Ok(lines) => lines,
            Err(e) => return CommandResult::failed(e),
        };
        if lines.is_empty() {
            return CommandResult::failed(
                "The selected surfaces have no non-coplanar intersection curves.",
            );
        }
        match self.commit_section_lines(lines, "Mesh Intersection") {
            Ok(count) => {
                self.status = format!("Created {count} intersection line(s).");
                CommandResult::completed()
            }
            Err(e) => CommandResult::failed(e),
        }
    }
    fn commit_section_lines(
        &mut self,
        lines: Vec<[Vec3; 2]>,
        label: &str,
    ) -> Result<usize, String> {
        let (owner, path, inverse) = self.line_geometry_context();
        let lines = lines
            .into_iter()
            .map(|l| l.map(|p| inverse.transform_point(p)))
            .collect();
        let (_, edges) = self.commit_document_edit(label, |d| {
            let definition = match owner {
                LineOwner::SceneRoot => d.ensure_scene_root(),
                LineOwner::Definition(id) => id,
            };
            solids::append_result(d, definition, vec![], lines)
        })?;
        let count = edges.len();
        self.selection.apply(
            edges.into_iter().map(|id| SelectionTarget {
                occurrence: path.clone(),
                entity: SelectedEntity::Segment(id),
            }),
            SelectionMode::Replace,
        );
        Ok(count)
    }
    pub(super) fn begin_section(&mut self) -> CommandResult {
        let faces = self.mesh_faces_for(self.selection.selected());
        if faces.is_empty() {
            return CommandResult::failed("Select faces or objects to section.");
        }
        self.activate_tool("Planar Section");
        self.section = SectionState {
            origin: None,
            faces,
            revision: self.document.revision(),
        };
        self.status = "Pick the section plane origin.".into();
        CommandResult::completed()
    }
    pub(super) fn section_preview(&self, point: Vec3) -> ToolPreview {
        let Some(origin) = self.section.origin else {
            return ToolPreview::Point { position: point };
        };
        let normal = point - origin;
        let mut edges = solids::section(&self.section.faces, origin, normal).unwrap_or_default();
        edges.push([origin, point]);
        if let Some(n) = normal.try_normalized(1e-9) {
            let u = n
                .cross(if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X })
                .try_normalized(1e-9)
                .unwrap();
            let v = n.cross(u);
            let r = self.document.model_settings().grid_size * 3.;
            let corners = [
                origin + (u + v) * r,
                origin + (-u + v) * r,
                origin + (-u - v) * r,
                origin + (u - v) * r,
            ];
            edges.extend((0..4).map(|i| [corners[i], corners[(i + 1) % 4]]));
        }
        ToolPreview::Transform { edges }
    }
    pub(super) fn section_pick(&mut self, point: Vec3) {
        let Some(origin) = self.section.origin else {
            self.section.origin = Some(point);
            self.status = "Pick a point defining the plane normal.".into();
            return;
        };
        if self.section.revision != self.document.revision() {
            self.status =
                "The selection changed during section input. Restart Planar Section.".into();
            return;
        }
        let lines = match solids::section(&self.section.faces, origin, point - origin) {
            Ok(lines) => lines,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if lines.is_empty() {
            self.status = "The plane does not intersect the selected faces.".into();
            return;
        }
        let (owner, path, inverse) = self.line_geometry_context();
        let lines = lines
            .into_iter()
            .map(|s| s.map(|p| inverse.transform_point(p)))
            .collect();
        let result = self.commit_document_edit("Planar Section", |d| {
            let definition = match owner {
                LineOwner::SceneRoot => d.ensure_scene_root(),
                LineOwner::Definition(id) => id,
            };
            solids::append_result(d, definition, vec![], lines)
        });
        match result {
            Ok((_, edges)) => {
                let count = edges.len();
                self.activate_tool("Select");
                self.selection.apply(
                    edges.into_iter().map(|id| SelectionTarget {
                        occurrence: path.clone(),
                        entity: SelectedEntity::Segment(id),
                    }),
                    SelectionMode::Replace,
                );
                self.status = format!("Planar section created {count} line(s).");
            }
            Err(e) => self.status = e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> MeshcraftApp {
        super::super::tests::test_app()
    }
    fn cmd(a: &mut MeshcraftApp, id: &str) {
        let r = a.dispatch_command(id, &CommandPayload::default());
        assert_eq!(
            r.result.completion,
            CommandCompletion::Completed,
            "{id}: {:?}",
            r.result.diagnostics
        );
    }
    #[test]
    fn solids_section_and_undo_are_real_command_workflows() {
        for id in [
            "tool.builtin.solid_union",
            "tool.builtin.solid_intersection",
            "tool.builtin.solid_subtraction",
        ] {
            let mut a = app();
            let p = a
                .document
                .add_cuboid("a", Vec3::new(2., 2., 2.), Vec3::ZERO)
                .unwrap();
            let q = a
                .document
                .add_cuboid("b", Vec3::new(2., 2., 2.), Vec3::X)
                .unwrap();
            a.set_selection([p.clone(), q.clone()]);
            cmd(&mut a, id);
            assert!(a.document.resolve_occurrence(&p).is_none());
            assert!(a.document.resolve_occurrence(&q).is_none());
            assert!(!a.selection.selected().is_empty());
            a.document.validate().unwrap();
            cmd(&mut a, "edit.undo");
            assert!(a.document.resolve_occurrence(&p).is_some());
            assert!(a.document.resolve_occurrence(&q).is_some());
        }
        let mut a = app();
        cmd(&mut a, "geometry.add_cuboid");
        let original = a.selected_occurrences()[0].clone();
        cmd(&mut a, "tool.builtin.plane_section");
        a.handle_native_world_pick(Vec3::X, None, None);
        let revision = a.document.revision();
        let snap = SnapResult {
            point: Vec3::X * 2.,
            normal: Vec3::Y,
            kind: SnapKind::Free,
            screen_distance: 0.,
        };
        assert!(
            a.native_tool_feedback_for_snap(snap, egui::Pos2::ZERO)
                .0
                .is_some()
        );
        assert_eq!(a.document.revision(), revision);
        a.handle_native_world_pick(Vec3::X * 2., None, None);
        assert_eq!(a.active_tool, "Select");
        assert!(a.document.resolve_occurrence(&original).is_some());
        assert!(
            a.selection
                .selected()
                .iter()
                .all(|t| matches!(t.entity, SelectedEntity::Segment(_)))
        );
        cmd(&mut a, "edit.undo");
        a.document.validate().unwrap();
    }

    #[test]
    fn difference_uses_click_order_even_when_it_reverses_placement_ids() {
        for reversed in [false, true] {
            let mut a = app();
            let first = a
                .document
                .add_cuboid("A", Vec3::new(2., 2., 2.), Vec3::ZERO)
                .unwrap();
            let second = a
                .document
                .add_cuboid("B", Vec3::new(2., 2., 2.), Vec3::X)
                .unwrap();
            let mut picks = [first.clone(), second.clone()];
            if reversed {
                picks.reverse();
            }
            for (index, occurrence) in picks.into_iter().enumerate() {
                a.handle_native_selection_click(
                    Some(SelectionTarget {
                        occurrence,
                        entity: SelectedEntity::Occurrence,
                    }),
                    egui::Modifiers {
                        shift: index > 0,
                        ..Default::default()
                    },
                    1000 + index as u64 * 1000,
                    meshcraft_core::geometry::Vec2::ZERO,
                );
            }
            let cursor = a.history.cursor();
            cmd(&mut a, "tool.builtin.solid_subtraction");
            let faces = a.mesh_faces_for(a.selection.selected());
            let xs: Vec<_> = faces.iter().flat_map(|f| f.points.map(|p| p.x)).collect();
            let min = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let expected_min = if reversed { 2. } else { 0. };
            assert!(
                (min - expected_min).abs() < 1e-6 && (max - expected_min - 1.).abs() < 1e-6,
                "reversed={reversed}: {min}..{max}"
            );
            assert_eq!(a.history.cursor(), cursor + 1);
            a.document.validate().unwrap();
            cmd(&mut a, "edit.undo");
            assert!(a.document.resolve_occurrence(&first).is_some());
            assert!(a.document.resolve_occurrence(&second).is_some());
        }
    }
    #[test]
    fn mirror_two_points_and_fill_closed_directed_lines() {
        let mut a = app();
        cmd(&mut a, "geometry.add_cuboid");
        let path = a.selected_occurrences()[0].clone();
        cmd(&mut a, "tool.mirror");
        a.handle_native_world_pick(Vec3::ZERO, None, None);
        a.handle_native_world_pick(Vec3::X, None, None);
        assert!(
            a.document
                .resolve_occurrence(&path)
                .unwrap()
                .world_transform
                .transform_vector(Vec3::X)
                .x
                < 0.
        );
        cmd(&mut a, "edit.undo");
        a.selection.clear();
        let points = [
            Vec3::new(5., 0., 0.),
            Vec3::new(7., 0., 0.),
            Vec3::new(7., 2., 0.),
            Vec3::new(5., 2., 0.),
        ];
        let mut targets = vec![];
        for i in 0..4 {
            targets.extend(
                a.create_line_in_active_context(points[i], points[(i + 1) % 4])
                    .unwrap(),
            );
        }
        a.selection.apply(targets, SelectionMode::Replace);
        cmd(&mut a, "geometry.fill_loops");
        assert!(
            a.selection
                .selected()
                .iter()
                .any(|t| matches!(t.entity, SelectedEntity::Triangle(_)))
        );
        a.document.validate().unwrap();
    }
}
