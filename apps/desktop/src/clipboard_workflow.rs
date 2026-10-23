use super::*;
use meshcraft_core::document::clipboard::ClipboardSelection;
use serde_json::Value;

#[derive(Default)]
pub(super) struct ClipboardState {
    pub text: String,
    pending: Option<String>,
    local: Option<(u64, String)>,
    pub(super) placement: Option<PastePlacement>,
}
pub(super) struct PastePlacement {
    snapshot: ClipboardSelection,
    edges: Vec<[Vec3; 2]>,
    pub anchor: Vec3,
    pointer: Vec3,
    reuse: bool,
    generation: u64,
    context: EditContext,
}

impl MeshcraftApp {
    pub(crate) fn native_set_clipboard(&mut self, text: String) {
        self.clipboard.text = text;
    }
    pub(crate) fn native_take_clipboard_write(&mut self) -> Option<String> {
        self.clipboard.pending.take()
    }

    pub(super) fn clipboard_action(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> CommandResult {
        if matches!(action, "paste" | "paste_in_place") {
            let text = payload
                .values
                .get("clipboard")
                .and_then(Value::as_str)
                .unwrap_or(&self.clipboard.text)
                .to_owned();
            let snapshot = match clipboard_snapshot(&text) {
                Ok(snapshot) => snapshot,
                Err(error) => return CommandResult::failed(error),
            };
            let reuse = !text.starts_with("// Meshcraft Object Script")
                && self
                    .clipboard
                    .local
                    .as_ref()
                    .is_some_and(|(generation, original)| {
                        *generation == self.document_generation && original == &text
                    });
            let context = self.selection.context.clone();
            if action == "paste_in_place" {
                let delta = match payload_vec3(payload, "offset", Vec3::ZERO) {
                    Ok(v) => v,
                    Err(e) => return CommandResult::failed(e),
                };
                return match self.commit_document_edit("Paste in Place", |doc| {
                    snapshot.paste(doc, &context, delta, reuse)
                }) {
                    Ok(targets) => {
                        self.cancel_active_tool();
                        self.selection.apply(targets, SelectionMode::Replace);
                        self.status = "Pasted selection in place.".into();
                        CommandResult::completed()
                    }
                    Err(error) => CommandResult::failed(error),
                };
            }
            let edges = snapshot.placement_wireframe();
            let Some(bounds) =
                meshcraft_core::geometry::Aabb3::from_points(edges.iter().flatten().copied())
            else {
                return CommandResult::failed("clipboard has no finite geometry to place");
            };
            // Y is up; +Z faces the Front view. Keep this anchor fixed for the entire bundle.
            let anchor = Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z);
            let pointer = self.cursor_snap.map_or(Vec3::ZERO, |snap| snap.point);
            self.activate_tool("Paste");
            self.clipboard.placement = Some(PastePlacement {
                snapshot,
                edges,
                anchor,
                pointer,
                reuse,
                generation: self.document_generation,
                context,
            });
            self.status =
                "Paste: pick insertion point · Esc cancels · Ctrl+Shift+V pastes in place.".into();
            return CommandResult::completed();
        }
        let snapshot = match ClipboardSelection::capture(&self.document, &self.selection) {
            Ok(s) => s,
            Err(e) => return CommandResult::failed(e),
        };
        let text = if action == "script" {
            match selection_script(&self.document, &self.selection) {
                Ok(t) => t,
                Err(e) => return CommandResult::failed(e),
            }
        } else {
            match serde_json::to_string(&snapshot) {
                Ok(t) => t,
                Err(e) => return CommandResult::failed(e.to_string()),
            }
        };
        if action == "cut" {
            let result =
                self.execute_command(ShellAction::DeleteSelection, &CommandPayload::default());
            if result.completion != CommandCompletion::Completed {
                return result;
            }
        }
        self.clipboard.local = Some((self.document_generation, text.clone()));
        self.clipboard.pending = Some(text.clone());
        self.clipboard.text = text;
        self.status = if action == "script" {
            "Copied selection as an object script."
        } else if action == "cut" {
            "Cut selection to clipboard."
        } else {
            "Copied selection to clipboard."
        }
        .into();
        CommandResult::completed()
    }

    pub(crate) fn native_paste_preview(&self) -> Option<ToolPreview> {
        let placement = self.clipboard.placement.as_ref()?;
        if self.active_tool != "Paste"
            || placement.generation != self.document_generation
            || placement.context != self.selection.context
        {
            return None;
        }
        let delta = placement.pointer - placement.anchor;
        let edges = placement
            .edges
            .iter()
            .map(|edge| edge.map(|point| point + delta))
            .collect();
        Some(ToolPreview::Transform { edges })
    }

    pub(crate) fn paste_preview_at(&mut self, point: Option<Vec3>) -> Option<ToolPreview> {
        if let Some(placement) = self.clipboard.placement.as_mut()
            && let Some(point) = point.filter(|p| p.is_finite())
        {
            placement.pointer = point;
        }
        self.native_paste_preview()
    }

    pub(super) fn paste_at(&mut self, point: Vec3) {
        let Some(placement) = self.clipboard.placement.take() else {
            return;
        };
        if placement.generation != self.document_generation
            || placement.context != self.selection.context
        {
            self.cancel_active_tool();
            self.status = "Paste canceled because the editing context changed.".into();
            return;
        }
        let delta = point - placement.anchor;
        match self.commit_document_edit("Paste", |document| {
            placement
                .snapshot
                .paste(document, &placement.context, delta, placement.reuse)
        }) {
            Ok(targets) => {
                self.cancel_active_tool();
                self.selection.apply(targets, SelectionMode::Replace);
                self.status = "Pasted selection at insertion point.".into();
            }
            Err(error) => {
                self.clipboard.placement = Some(placement);
                self.status = error;
            }
        }
    }
}

fn clipboard_snapshot(text: &str) -> Result<ClipboardSelection, String> {
    if text.starts_with("// Meshcraft Object Script") {
        // Copy as Script emits a literal drawable array. Decode that array for feedback,
        // without running JavaScript on the UI thread. The original hooks remain executable.
        let items = text
            .split_once("  return ")
            .and_then(|(_, body)| body.strip_suffix(";\n}\n"))
            .ok_or("script has no geometry snapshot; open it in the object script editor")?;
        let items = serde_json::from_str(items)
            .map_err(|e| format!("invalid script geometry snapshot: {e}"))?;
        let mut scene = Document::new();
        let object = scene.add_scripted_object(
            "Scripted Object",
            meshcraft_core::document::scripts::ScriptDefinition {
                source: text.to_owned(),
                ..Default::default()
            },
        )?;
        let frame = meshcraft_core::document::scripts::ScriptFrame::build(
            &Default::default(),
            items,
            Default::default(),
        )?;
        scene.install_script_frames(BTreeMap::from([(object.clone(), frame)]));
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: object,
            entity: SelectedEntity::Occurrence,
        });
        return ClipboardSelection::capture(&scene, &selection);
    }
    let snapshot: ClipboardSelection = serde_json::from_str(text)
        .map_err(|_| "clipboard does not contain a Meshcraft selection".to_owned())?;
    if snapshot.format != "meshcraft-selection-1" {
        return Err("unsupported clipboard format".into());
    }
    snapshot.scene.validate()?;
    Ok(snapshot)
}

fn selection_script(document: &Document, selection: &SelectionSession) -> Result<String, String> {
    let normalized = modeling_objects::selection_with_architecture_instances(document, selection);
    let selection = &normalized;
    let mut items = Vec::new();
    for occurrence in document.scene_geometry_refs()? {
        let whole = selection.selected().iter().any(|t| {
            t.entity == SelectedEntity::Occurrence
                && occurrence.path.starts_with(&t.occurrence.path)
        });
        let Some(r) = document.resolve_occurrence(&occurrence) else {
            continue;
        };
        let Some(geometry) = document.effective_geometry(&occurrence) else {
            continue;
        };
        let prefix = format!("copy:{:?}:", occurrence.path);
        let material_colors = document.material_colors();
        for mut drawable in meshcraft_core::document::scripts::geometry_drawables(geometry) {
            let id = drawable["id"].as_str().unwrap_or_default();
            let extensions = id
                .strip_prefix("authored:segment:")
                .and_then(|id| id.parse::<u64>().ok())
                .and_then(|id| {
                    geometry
                        .segments
                        .get(&meshcraft_core::document::SegmentId(id))
                })
                .map(|s| &s.extensions)
                .or_else(|| {
                    id.strip_prefix("authored:face:")
                        .and_then(|id| id.parse::<u64>().ok())
                        .and_then(|id| {
                            geometry
                                .triangles
                                .get(&meshcraft_core::document::TriangleId(id))
                        })
                        .map(|f| &f.extensions)
                });
            let owner =
                extensions.and_then(meshcraft_core::document::architecture::semantic_entity);
            let selected = whole
                || owner.is_some_and(|entity| {
                    selection.selected().contains(&SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity,
                    })
                })
                || selection.selected().iter().any(|t| {
                    t.occurrence == occurrence
                        && match t.entity {
                            SelectedEntity::Segment(s) => id == format!("authored:segment:{}", s.0),
                            SelectedEntity::Triangle(f) => id == format!("authored:face:{}", f.0),
                            _ => false,
                        }
                });
            if !selected {
                continue;
            }
            // Use the currently evaluated/deformed geometry, with world placement baked in.
            if let Some(points) = drawable["points"].as_array_mut() {
                for point in points {
                    let p: Vec3 =
                        serde_json::from_value(point.clone()).map_err(|e| e.to_string())?;
                    let position = r.world_transform.transform_point(p);
                    let pid = format!("{prefix}{}", point["id"].as_str().unwrap_or_default());
                    *point = json!({"x": position.x, "y": position.y, "z": position.z, "id": pid});
                }
            }
            let material = extensions.and_then(meshcraft_core::document::materials::material_id);
            if let Some(color) = material.and_then(|id| material_colors.get(id)) {
                drawable["color"] = json!(color);
            }
            drawable["id"] = json!(format!(
                "{prefix}{}",
                drawable["id"].as_str().unwrap_or_default()
            ));
            items.push(drawable);
        }
    }
    if items.is_empty() {
        return Err("Copy as Script requires selected segments, faces or object geometry".into());
    }
    let geometry = serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?;
    Ok(format!(
        "// Meshcraft Object Script — selection snapshot\nfunction onCreate(instance, model) {{}}\n\nfunction onAnimationFrame(instance, model) {{\n  return {geometry};\n}}\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn preview_bounds(app: &MeshcraftApp) -> meshcraft_core::geometry::Aabb3 {
        let Some(ToolPreview::Transform { edges }) = app.native_paste_preview() else {
            panic!("Paste must show wireframe before the first pointer move");
        };
        meshcraft_core::geometry::Aabb3::from_points(edges.into_iter().flatten()).unwrap()
    }

    fn architecture_app(reopened: bool) -> (MeshcraftApp, Vec<SelectionTarget>) {
        use meshcraft_core::document::{
            DefinitionKind,
            architecture::{Element, Frame, FrameKind, Hole, Slab, Wall},
        };
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        d.put_architecture(
            root,
            None,
            Element::Wall(Wall {
                name: "Wall".into(),
                end: Vec3::X * 5.,
                holes: vec![Hole {
                    u0: 1.,
                    u1: 2.,
                    v0: 1.,
                    v1: 2.,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        )
        .unwrap();
        d.put_architecture(
            root,
            None,
            Element::Slab(Slab {
                name: "Slab".into(),
                profile: PolylinePath {
                    points: vec![
                        Vec3::new(0., 3., 0.),
                        Vec3::new(4., 3., 0.),
                        Vec3::new(4., 3., 4.),
                        Vec3::new(0., 3., 4.),
                    ],
                    closed: true,
                },
                holes: vec![Hole {
                    u0: 1.,
                    u1: 2.,
                    v0: 1.,
                    v1: 2.,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        )
        .unwrap();
        for (name, kind, x) in [
            ("Window", FrameKind::Window, 8.),
            ("Door", FrameKind::Door, 16.),
        ] {
            let definition = d.add_definition(name, DefinitionKind::Architecture);
            d.put_architecture(
                definition,
                None,
                Element::Frame(Frame {
                    name: name.into(),
                    kind,
                    glazing_enabled: kind == FrameKind::Window,
                    contour: vec![
                        Vec3::ZERO,
                        Vec3::X * 2.,
                        Vec3::new(2., 3., 0.),
                        Vec3::Y * 3.,
                    ],
                    ..Default::default()
                }),
            )
            .unwrap();
            for x in if kind == FrameKind::Window {
                vec![x, 12.]
            } else {
                vec![x]
            } {
                modeling_objects::place_definition(
                    &mut d,
                    &EditContext::Document,
                    definition,
                    Vec3::X * x,
                    None,
                    false,
                )
                .unwrap();
            }
        }
        if reopened {
            d = meshcraft_core::legacy::import_current_value(
                &meshcraft_core::legacy::export_current_value(&d).unwrap(),
            )
            .unwrap();
        }
        let selected = d
            .architecture_elements()
            .into_iter()
            .filter(|e| (e.transform.translation.x - 12.).abs() > 1e-6)
            .map(|e| e.target)
            .collect::<Vec<_>>();
        assert_eq!(4, selected.len());
        (
            MeshcraftApp::new_native_session(d, "Ready".into(), None),
            selected,
        )
    }

    fn complete(app: &mut MeshcraftApp, id: &str) -> CommandResult {
        let r = app.dispatch_command(id, &CommandPayload::default()).result;
        assert_eq!(
            CommandCompletion::Completed,
            r.completion,
            "{id}: {:?}",
            r.diagnostics
        );
        r
    }

    #[test]
    fn mixed_architecture_transform_tools_accept_final_pick_and_match_their_preview() {
        for reopened in [false, true] {
            for (command, picks, copies) in [
                ("tool.builtin.move", vec![Vec3::ZERO, Vec3::Y * 9.], false),
                ("tool.move_copy", vec![Vec3::ZERO, Vec3::Y * 9.], true),
                (
                    "tool.rotate_copy", // Keep the copy away from the source wall's autojoin endpoint.
                    vec![
                        Vec3::new(0., 0., -10.),
                        Vec3::new(1., 0., -10.),
                        Vec3::new(0., 0., -9.),
                    ],
                    true,
                ),
            ] {
                let (mut app, targets) = architecture_app(reopened);
                app.apply_native_selection_region(targets, false, SelectionMode::Replace);
                let original = app.document.architecture_elements();
                let before = meshcraft_core::legacy::export_current_value(&app.document).unwrap();
                let source = ClipboardSelection::capture(&app.document, &app.selection)
                    .unwrap()
                    .placement_wireframe();
                complete(&mut app, command);
                for p in &picks[..picks.len() - 1] {
                    app.handle_native_world_pick(*p, None, None);
                }
                let last = *picks.last().unwrap();
                let affine = app.transform_tool_state.preview(Some(last)).unwrap();
                let expected = meshcraft_core::geometry::Aabb3::from_points(
                    source
                        .into_iter()
                        .flatten()
                        .map(|p| affine.transform_point(p)),
                )
                .unwrap();
                let preview = modeling_transforms::preview_selection_transform(
                    &app.document,
                    &app.selection,
                    affine,
                )
                .unwrap();
                let preview =
                    meshcraft_core::geometry::Aabb3::from_points(preview.into_iter().flatten())
                        .unwrap();
                assert!(
                    expected.min.distance(preview.min) < 1e-6
                        && expected.max.distance(preview.max) < 1e-6,
                    "{command}: frame geometry missing from preview"
                );
                app.handle_native_world_pick(last, None, None);
                assert_eq!(1, app.history.entries().len(), "{command}: {}", app.status);
                assert!(
                    app.transform_tool_state.points().is_empty(),
                    "final pick was rejected: {}",
                    app.status
                );
                let selected = app.native_selected_architectures();
                assert_eq!(4, selected.len());
                let actual = ClipboardSelection::capture(&app.document, &app.selection)
                    .unwrap()
                    .placement_wireframe();
                let actual =
                    meshcraft_core::geometry::Aabb3::from_points(actual.into_iter().flatten())
                        .unwrap();
                assert!(
                    expected.min.distance(actual.min) < 1e-6
                        && expected.max.distance(actual.max) < 1e-6,
                    "{command} (reopened={reopened}): committed geometry {actual:?} differs from preview {expected:?}"
                );
                let all = app.document.architecture_elements();
                assert_eq!(if copies { 9 } else { 5 }, all.len());
                for e in &original {
                    if copies || (e.transform.translation.x - 12.).abs() < 1e-6 {
                        assert!(
                            all.iter()
                                .any(|a| a.element == e.element && a.transform == e.transform),
                            "{command}: changed source/sibling instance"
                        );
                    }
                }
                for e in selected {
                    let source = original
                        .iter()
                        .find(|a| a.element.name() == e.element.name())
                        .unwrap();
                    assert_eq!(source.element.holes().len(), e.element.holes().len());
                    assert_eq!(
                        source.element.surface_materials(),
                        e.element.surface_materials()
                    );
                }
                complete(&mut app, "edit.undo");
                assert_eq!(
                    before,
                    meshcraft_core::legacy::export_current_value(&app.document).unwrap()
                );
                complete(&mut app, "edit.redo");
                assert_eq!(
                    if copies { 9 } else { 5 },
                    app.document.architecture_elements().len()
                );
            }
        }
    }

    #[test]
    fn architecture_frame_surface_picks_support_array_queries_and_multiple_copies() {
        for reopened in [false, true] {
            let (mut app, targets) = architecture_app(reopened);
            app.selection.apply(targets, SelectionMode::Replace);
            assert!(modeling_copy::can_linear_array_selection(
                &app.document,
                &app.selection
            ));
            assert!(
                (modeling_copy::projected_selection_span(&app.document, &app.selection, Vec3::X)
                    .unwrap()
                    - 18.)
                    .abs()
                    < 1e-6
            );
            let original = app.document.architecture_elements();
            let mut selected = app.selection.clone();
            let report = modeling_copy::translated_array(
                &mut app.document,
                &mut selected,
                &[Vec3::Y * 9., Vec3::Y * 18.],
            )
            .unwrap();
            assert_eq!(2, report.copies);
            assert_eq!(8, selected.selected().len());
            assert_eq!(13, app.document.architecture_elements().len());
            for source in original {
                assert!(
                    app.document
                        .architecture_elements()
                        .iter()
                        .any(|e| e.element == source.element && e.transform == source.transform)
                );
            }
            app.document.validate().unwrap();
        }
    }

    #[test]
    fn moving_a_frame_inside_prototype_editing_changes_its_geometry_not_its_placements() {
        let (mut app, targets) = architecture_app(true);
        let frame = targets
            .into_iter()
            .find(|t| app.document.architecture_element(t).unwrap().element.name() == "Window")
            .unwrap();
        let original = app
            .document
            .resolve_occurrence(&frame.occurrence)
            .unwrap()
            .world_transform;
        app.selection
            .set_context(EditContext::Occurrence(frame.occurrence.clone()));
        app.selection.select(frame.clone());
        complete(&mut app, "tool.builtin.move");
        app.handle_native_world_pick(Vec3::ZERO, None, None);
        app.handle_native_world_pick(Vec3::Y * 9., None, None);
        assert_eq!(1, app.history.entries().len(), "{}", app.status);
        assert_eq!(
            original,
            app.document
                .resolve_occurrence(&frame.occurrence)
                .unwrap()
                .world_transform
        );
        let meshcraft_core::document::architecture::Element::Frame(changed) =
            app.document.architecture_element(&frame).unwrap().element
        else {
            panic!()
        };
        assert!(changed.contour.iter().all(|p| p.y >= 9. - 1e-6));
        assert_eq!(
            EditContext::Occurrence(frame.occurrence),
            app.selection.context
        );
    }

    #[test]
    fn architecture_copy_paste_and_script_capture_preserve_owners_in_new_and_reopened_drawings() {
        for reopened in [false, true] {
            let (mut app, targets) = architecture_app(reopened);
            app.selection.apply(targets.clone(), SelectionMode::Replace);
            let original = app.document.architecture_elements();
            complete(&mut app, "edit.copy_selection");
            let snapshot = clipboard_snapshot(&app.clipboard.text).unwrap();
            assert!(!snapshot.placement_wireframe().is_empty());
            complete(&mut app, "edit.paste_in_place");
            let pasted = app.native_selected_architectures();
            assert_eq!(4, pasted.len());
            assert_eq!(9, app.document.architecture_elements().len());
            for copy in &pasted {
                let source = original
                    .iter()
                    .find(|e| e.element.name() == copy.element.name())
                    .unwrap();
                assert_eq!(source.element.holes(), copy.element.holes());
                assert_eq!(
                    source.element.surface_materials(),
                    copy.element.surface_materials()
                );
                assert!(!targets.contains(&copy.target));
            }
            complete(&mut app, "edit.undo");
            assert_eq!(5, app.document.architecture_elements().len());
            app.selection.apply(targets.clone(), SelectionMode::Replace);
            complete(&mut app, "edit.paste");
            let insertion = Vec3::new(30., 4., -12.);
            app.paste_preview_at(Some(insertion));
            let expected = preview_bounds(&app);
            app.handle_native_world_pick(insertion, None, None);
            assert_eq!("Select", app.active_tool);
            assert_eq!(9, app.document.architecture_elements().len());
            let pasted = ClipboardSelection::capture(&app.document, &app.selection).unwrap();
            let actual = meshcraft_core::geometry::Aabb3::from_points(
                pasted.placement_wireframe().into_iter().flatten(),
            )
            .unwrap();
            assert!(
                expected.min.distance(actual.min) < 1e-8
                    && expected.max.distance(actual.max) < 1e-8
            );
            complete(&mut app, "edit.undo");
            app.selection.apply(targets, SelectionMode::Replace);
            complete(&mut app, "edit.copy_as_script");
            let scripted = clipboard_snapshot(&app.clipboard.text).unwrap();
            let bounds = meshcraft_core::geometry::Aabb3::from_points(
                scripted.placement_wireframe().into_iter().flatten(),
            )
            .unwrap();
            let original_bounds = meshcraft_core::geometry::Aabb3::from_points(
                snapshot.placement_wireframe().into_iter().flatten(),
            )
            .unwrap();
            assert_eq!(original_bounds, bounds);
            assert_eq!(5, app.document.architecture_elements().len());
            app.document.validate().unwrap();
        }
    }

    #[test]
    fn mixed_architecture_explode_preserves_sibling_instances_materials_and_undo() {
        for reopened in [false, true] {
            let (mut app, targets) = architecture_app(reopened);
            let sibling = app
                .document
                .architecture_elements()
                .into_iter()
                .find(|e| (e.transform.translation.x - 12.).abs() < 1e-6)
                .unwrap();
            app.selection.apply(targets.clone(), SelectionMode::Replace);
            let before = app.document.clone();
            let r = complete(&mut app, "object.explode_selection");
            assert_eq!(4, r.outputs["exploded_count"]);
            assert!(
                r.outputs["edge_count"].as_u64().unwrap() > 0
                    && r.outputs["face_count"].as_u64().unwrap() > 0
            );
            let remaining = app.document.architecture_elements();
            assert_eq!(1, remaining.len());
            assert_eq!(sibling.element, remaining[0].element);
            assert_eq!(sibling.transform, remaining[0].transform);
            assert_eq!(sibling.definition, remaining[0].definition);
            assert!(app.selection.selected().iter().all(|t| matches!(
                t.entity,
                SelectedEntity::Segment(_) | SelectedEntity::Triangle(_)
            )));
            let colors = app.document.material_colors();
            for t in app.selection.selected() {
                if let SelectedEntity::Triangle(id) = t.entity {
                    let g = app.document.effective_geometry(&t.occurrence).unwrap();
                    let face = &g.triangles[&id];
                    let id =
                        meshcraft_core::document::materials::material_id(&face.extensions).unwrap();
                    assert_eq!(face.color, colors[id]);
                    assert!(!meshcraft_core::document::architecture::generated(
                        &face.extensions
                    ));
                }
            }
            let reopened = meshcraft_core::legacy::import_current_value(
                &meshcraft_core::legacy::export_current_value(&app.document).unwrap(),
            )
            .unwrap();
            assert_eq!(1, reopened.architecture_elements().len());
            complete(&mut app, "edit.undo");
            assert_eq!(
                meshcraft_core::legacy::export_current_value(&before).unwrap(),
                meshcraft_core::legacy::export_current_value(&app.document).unwrap()
            );
            let before_rejection = app.document.clone();
            app.selection.apply(targets, SelectionMode::Replace);
            app.selection_display_policy
                .set_unlocked(SelectionKind::Frame, false);
            let r = app.explode_selection();
            assert_eq!(CommandCompletion::Failed, r.completion);
            assert_eq!(before_rejection, app.document);
        }
    }

    #[test]
    fn exploding_a_frame_inside_its_prototype_keeps_the_instance_placements() {
        let (mut app, targets) = architecture_app(true);
        let frame = targets
            .into_iter()
            .find(|t| {
                matches!(
                    t.entity,
                    SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Frame,
                        ..
                    }
                )
            })
            .unwrap();
        let original = app
            .document
            .resolve_occurrence(&frame.occurrence)
            .unwrap()
            .definition
            .id;
        app.selection
            .set_context(EditContext::Occurrence(frame.occurrence.clone()));
        app.selection.select(frame.clone());
        complete(&mut app, "object.explode_selection");
        assert_eq!(
            original,
            app.document
                .resolve_occurrence(&frame.occurrence)
                .unwrap()
                .definition
                .id
        );
        assert!(app.document.architecture_element(&frame).is_none());
        assert!(
            !app.document
                .effective_geometry(&frame.occurrence)
                .unwrap()
                .triangles
                .is_empty()
        );
        assert_eq!(
            EditContext::Occurrence(frame.occurrence),
            app.selection.context
        );
    }

    #[test]
    fn normal_paste_floats_a_frozen_bundle_until_one_undoable_pick() {
        let mut app = super::super::tests::test_app();
        let first = app
            .document
            .add_cuboid("first", Vec3::new(2., 3., 4.), Vec3::new(-5., 2., 7.))
            .unwrap();
        let second = app
            .document
            .add_cuboid("second", Vec3::new(1., 2., 3.), Vec3::new(3., 4., -2.))
            .unwrap();
        let old_origins = [&first, &second].map(|o| {
            app.document
                .resolve_occurrence(o)
                .unwrap()
                .world_transform
                .transform_point(Vec3::ZERO)
        });
        app.set_selection([first.clone(), second.clone()]);
        app.run_command("edit.copy_selection");
        let revision = app.document.revision();
        let entries = app.history.entries().len();
        app.run_command("edit.paste");
        assert_eq!("Paste", app.active_tool);
        assert_eq!(revision, app.document.revision());
        assert_eq!(entries, app.history.entries().len());
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        let initial = preview_bounds(&app);
        assert_eq!(
            Vec3::ZERO,
            Vec3::new(initial.min.x, initial.min.y, initial.max.z)
        );
        let insertion = Vec3::new(12., 6., -9.);
        app.paste_preview_at(Some(insertion));
        let preview = preview_bounds(&app);
        assert_eq!(
            insertion,
            Vec3::new(preview.min.x, preview.min.y, preview.max.z)
        );
        // Changes to the OS clipboard after invoking Paste cannot change the floating bundle.
        app.native_set_clipboard("unrelated text".into());
        app.handle_native_world_pick(insertion, None, None);
        assert_eq!("Select", app.active_tool);
        assert!(app.native_paste_preview().is_none());
        assert_eq!(entries + 1, app.history.entries().len());
        assert_eq!(4, app.document.occurrence_refs().unwrap().len());
        let copies = app.selected_occurrences();
        assert_eq!(2, copies.len());
        let origins = copies
            .iter()
            .map(|o| {
                app.document
                    .resolve_occurrence(o)
                    .unwrap()
                    .world_transform
                    .transform_point(Vec3::ZERO)
            })
            .collect::<Vec<_>>();
        assert_eq!(old_origins[1] - old_origins[0], origins[1] - origins[0]);
        let pasted = ClipboardSelection::capture(&app.document, &app.selection).unwrap();
        let actual = meshcraft_core::geometry::Aabb3::from_points(
            pasted.placement_wireframe().into_iter().flatten(),
        )
        .unwrap();
        assert_eq!(
            preview, actual,
            "the committed geometry must match the last preview"
        );
        app.run_command("edit.undo");
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.redo");
        assert_eq!(4, app.document.occurrence_refs().unwrap().len());
    }

    #[test]
    fn paste_cancel_keeps_selection_and_edit_context_and_in_place_needs_no_pick() {
        let mut app = super::super::tests::test_app();
        app.run_command("geometry.add_cuboid");
        let original = app.selected_occurrences()[0].clone();
        app.run_command("edit.copy_selection");
        let entries = app.history.entries().len();
        app.run_command("edit.paste");
        app.handle_native_escape();
        assert!(app.native_paste_preview().is_none());
        assert_eq!(vec![original.clone()], app.selected_occurrences());
        assert_eq!(entries, app.history.entries().len());
        app.run_command("edit.paste");
        app.run_command("tool.builtin.line");
        assert!(app.clipboard.placement.is_none());
        app.run_command("edit.paste_in_place");
        let copy = app.selected_occurrences()[0].clone();
        assert_ne!(original, copy);
        assert_eq!(
            app.document
                .resolve_occurrence(&original)
                .unwrap()
                .world_transform,
            app.document
                .resolve_occurrence(&copy)
                .unwrap()
                .world_transform
        );
        app.selection
            .set_context(EditContext::Occurrence(original.clone()));
        app.run_command("edit.paste");
        app.handle_native_escape();
        assert_eq!(
            EditContext::Occurrence(original),
            app.selection.context,
            "canceling floating paste must not also exit prototype editing"
        );
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
    }

    #[test]
    fn placement_in_a_transformed_prototype_uses_world_anchor_and_keeps_context() {
        let mut app = super::super::tests::test_app();
        let source = app
            .document
            .add_cuboid("source", Vec3::new(2., 3., 4.), Vec3::ZERO)
            .unwrap();
        let parent = app
            .document
            .add_cuboid("parent", Vec3::new(1., 1., 1.), Vec3::new(10., 4., -2.))
            .unwrap();
        app.set_selection([source]);
        app.run_command("edit.copy_selection");
        app.selection
            .set_context(EditContext::Occurrence(parent.clone()));
        app.run_command("edit.paste");
        let insertion = Vec3::new(17., 8., 3.);
        app.handle_native_world_pick(insertion, None, None);
        assert_eq!(
            EditContext::Occurrence(parent.clone()),
            app.selection.context
        );
        let copy = app.selected_occurrences()[0].clone();
        assert!(copy.path.starts_with(&parent.path));
        let snapshot = ClipboardSelection::capture(&app.document, &app.selection).unwrap();
        let bounds = meshcraft_core::geometry::Aabb3::from_points(
            snapshot.placement_wireframe().into_iter().flatten(),
        )
        .unwrap();
        assert_eq!(
            insertion,
            Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z)
        );
        app.document.validate().unwrap();
    }

    #[test]
    fn paste_after_new_document_imports_prototypes_despite_reused_numeric_ids() {
        let mut app = super::super::tests::test_app();
        app.run_command("geometry.add_cuboid");
        app.run_command("edit.copy_selection");
        let source: ClipboardSelection = serde_json::from_str(&app.clipboard.text).unwrap();
        let expected = source.placement_wireframe();
        app.start_new_document();
        app.document
            .add_cuboid("different prototype", Vec3::new(20., 20., 20.), Vec3::ZERO)
            .unwrap();
        app.run_command("edit.paste_in_place");
        let pasted = ClipboardSelection::capture(&app.document, &app.selection).unwrap();
        assert_eq!(expected, pasted.placement_wireframe());
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
    }

    #[test]
    fn script_snapshot_paste_has_wireframe_without_evaluating_js_and_keeps_script() {
        let mut app = super::super::tests::test_app();
        app.run_command("geometry.add_cuboid");
        app.run_command("edit.copy_as_script");
        app.run_command("edit.paste");
        assert_eq!("Paste", app.active_tool);
        assert!(preview_bounds(&app).max.y > 0.);
        app.handle_native_world_pick(Vec3::X * 4., None, None);
        let copy = app.selected_occurrences()[0].clone();
        let definition = app
            .document
            .resolve_occurrence(&copy)
            .unwrap()
            .definition
            .id;
        assert!(app.document.script(definition).is_some());
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        app.document.validate().unwrap();
    }

    #[test]
    fn copy_cut_paste_are_independent_undoable_and_export_executable_script() {
        let mut app = super::super::tests::test_app();
        app.run_command("geometry.add_cuboid");
        let original = app.selected_occurrences()[0].clone();
        let prototype = app
            .document
            .resolve_occurrence(&original)
            .unwrap()
            .definition
            .id;
        app.run_command("edit.copy_selection");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.paste_in_place");
        let copy = app.selected_occurrences()[0].clone();
        assert_ne!(original, copy);
        assert_eq!(
            prototype,
            app.document
                .resolve_occurrence(&copy)
                .unwrap()
                .definition
                .id
        );
        app.run_command("edit.cut_selection");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.paste_in_place");
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.undo");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        app.set_selection([original]);
        app.run_command("edit.copy_as_script");
        let source = app.clipboard.text.clone();
        assert!(source.starts_with("// Meshcraft Object Script"));
        let engine = crate::script_engine::Session::new(&json!(null)).unwrap();
        let evaluated = engine.evaluate(&json!({"id":"copied","source":{"source":source}, "geometry":[],"hotspots":{},"time":0.,"deltaTime":0.016})).unwrap();
        let items: Vec<meshcraft_core::document::scripts::Drawable> =
            serde_json::from_value(evaluated["items"].clone()).unwrap();
        let frame = meshcraft_core::document::scripts::ScriptFrame::build(
            &meshcraft_core::document::GeometryBundle::default(),
            items,
            Default::default(),
        )
        .unwrap();
        assert_eq!(12, frame.geometry.triangles.len());
    }
}
