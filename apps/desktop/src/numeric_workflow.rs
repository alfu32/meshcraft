//! Buffered tool input; resolution reuses each tool's ordinary point/transaction path.
use super::*;
use meshcraft_core::tools::numeric::{self, Entry, PointInput, RepeatMode};

#[derive(Default)]
pub(super) struct State {
    pub text: String,
    pub pointer: Option<Vec3>,
    axes: Option<[Vec3; 3]>,
}
impl MeshcraftApp {
    pub fn tool_numeric_text(&self) -> &str {
        &self.numeric_entry.text
    }
    pub fn tool_numeric_active(&self) -> bool {
        (self.active_tool != "Select"
            || self.tool_cycle_state.hotspot_placement_active()
            || self.hotspot_reference_pick
            || self.object_place_tool_state.active_definition().is_some())
            && !matches!(
                self.active_tool.as_str(),
                "Paint" | "Orbit" | "Pan" | "Zoom" | "Remove Guide"
            )
    }
    pub fn append_tool_numeric_text(&mut self, text: &str) -> bool {
        if !self.tool_numeric_active() {
            return false;
        }
        if !text.chars().all(numeric::accepted) {
            return false;
        }
        let text = text
            .chars()
            .filter(|c| numeric::accepted(*c))
            .collect::<String>();
        if text.is_empty() {
            return false;
        }
        if self.numeric_entry.text.is_empty()
            && !text.chars().next().is_some_and(numeric::starts_entry)
        {
            return false;
        }
        if self.numeric_entry.text.len() + text.len() > 256 {
            self.status = "Tool input is limited to 256 characters.".into();
            return true;
        }
        if self.numeric_entry.text.is_empty() {
            let normal = self.preferences.grid_normal_axis.vector();
            let fallback = crate::viewport::cursor_axes(normal, None, normal).unwrap_or([
                Vec3::X,
                Vec3::Z,
                Vec3::Y,
            ]);
            #[cfg(not(target_arch = "wasm32"))]
            let axes = self
                .cursor_snap
                .and_then(|s| self.native_cursor_axes(s))
                .unwrap_or(fallback);
            #[cfg(target_arch = "wasm32")]
            let axes = fallback;
            self.numeric_entry.axes = Some(axes);
        }
        self.numeric_entry.text.push_str(&text);
        self.status = format!(
            "Input: {} · Backspace edit · Enter accept",
            self.numeric_entry.text
        );
        true
    }
    pub fn backspace_tool_numeric(&mut self) -> bool {
        if self.numeric_entry.text.pop().is_none() {
            return false;
        }
        self.transform_tool_state.clear_numeric_text();
        self.status = format!("Input: {} · Enter accept", self.numeric_entry.text);
        true
    }
    fn numeric_axes(&self) -> [Vec3; 3] {
        self.numeric_entry.axes.unwrap_or_else(|| {
            let normal = self.preferences.grid_normal_axis.vector();
            crate::viewport::cursor_axes(normal, None, normal).unwrap_or([
                Vec3::X,
                Vec3::Z,
                Vec3::Y,
            ])
        })
    }
    fn numeric_origin(&self) -> Vec3 {
        // Centers remain the origin for every vector in a rotation/scale workflow.
        if matches!(
            self.transform_tool_state.active(),
            Some(TransformTool::Scale | TransformTool::StretchScale)
        ) {
            return self
                .transform_tool_state
                .points()
                .first()
                .copied()
                .unwrap_or(Vec3::ZERO);
        }
        if self.primitive_tool_state.picks().len() == 2 {
            return self.primitive_tool_state.picks()[0];
        }
        self.tool_reference_point().unwrap_or(Vec3::ZERO)
    }
    fn numeric_transform_value(&self, point: PointInput) -> Option<f64> {
        let PointInput::Scalar {
            value,
            explicit_length: false,
            angle,
        } = point
        else {
            return None;
        };
        let tool = self.transform_tool_state.active()?;
        if angle
            && !matches!(
                tool,
                TransformTool::Rotate | TransformTool::Rotate2 | TransformTool::RotateStretch
            )
        {
            return None;
        }
        (self.transform_tool_state.points().len() + 1 == tool.required_picks()
            && matches!(
                tool,
                TransformTool::Rotate
                    | TransformTool::Rotate2
                    | TransformTool::RotateStretch
                    | TransformTool::Scale
                    | TransformTool::Squash
                    | TransformTool::StretchScale
            ))
        .then_some(value)
    }
    fn numeric_point(&self, entry: Entry, pointer: Option<Vec3>) -> Result<Vec3, String> {
        match entry.point {
            Some(point) if self.numeric_transform_value(point).is_some() => pointer
                .or_else(|| self.transform_tool_state.points().last().copied())
                .ok_or_else(|| "Pick the transform reference first".into()),
            Some(point) => point.resolve(self.numeric_origin(), pointer, self.numeric_axes()),
            None => pointer.ok_or_else(|| "Move the pointer to set the copy increment".into()),
        }
    }
    /// A constrained preview point, without changing the raw pointer/direction.
    pub(super) fn numeric_preview_point(&mut self, pointer: Vec3) -> Vec3 {
        self.numeric_entry.pointer = Some(pointer);
        let Ok(entry) = numeric::parse(&self.numeric_entry.text, self.document.model_settings())
        else {
            return pointer;
        };
        if let Some(point) = entry.point.and_then(|p| self.numeric_transform_value(p)) {
            self.transform_tool_state.clear_numeric_text();
            self.transform_tool_state
                .add_numeric_text(&point.to_string());
        } else {
            self.transform_tool_state.clear_numeric_text();
        }
        self.numeric_point(entry, Some(pointer)).unwrap_or(pointer)
    }
    pub(super) fn numeric_copy_transforms(
        &self,
        pointer: Vec3,
        limit: usize,
    ) -> Result<Vec<Affine3>, String> {
        let entry = numeric::parse(&self.numeric_entry.text, self.document.model_settings())?;
        let repetition = entry.repetition.ok_or("No copy repetition entered")?;
        let state = &self.transform_tool_state;
        if !state.copy_mode() {
            return Err("Enable copy mode before entering xN or /N".into());
        }
        let base = state
            .preview(Some(pointer))
            .ok_or("Pick all copy reference points and a valid increment first")?;
        let mut transform = Affine3::IDENTITY;
        let mut result = vec![];
        for i in 1..=repetition.count.min(limit) {
            transform = match repetition.mode {
                RepeatMode::Multiply => base.compose(transform),
                RepeatMode::Divide => {
                    state.divided_preview(pointer, i as f64 / repetition.count as f64)?
                }
            };
            if !transform.is_finite() || transform.inverse(GeometryTolerance::default()).is_none() {
                return Err("Copy transform is singular or out of range".into());
            }
            result.push(transform);
        }
        Ok(result)
    }
    pub(super) fn finish_numeric_entry(&mut self) -> CommandResult {
        match self.accept_numeric_entry() {
            Ok(()) => CommandResult::completed(),
            Err(error) => {
                self.status = error.clone();
                CommandResult::failed(error)
            }
        }
    }
    fn accept_numeric_entry(&mut self) -> Result<(), String> {
        let entry = numeric::parse(&self.numeric_entry.text, self.document.model_settings())?;
        if self.revolve_tool_state.active() && self.revolve_tool_state.axis_points().len() == 2 {
            let Some(PointInput::Scalar {
                value,
                explicit_length: false,
                ..
            }) = entry.point
            else {
                return Err("Enter a Revolve angle in degrees or radians".into());
            };
            if entry.repetition.is_some() {
                return Err("Revolve does not repeat copies".into());
            }
            self.revolve_tool_state.clear_numeric_input();
            self.revolve_tool_state.add_numeric_text(&value.to_string());
            let result = self.finish_revolve();
            if result.completion == CommandCompletion::Failed {
                return Err(result.diagnostics.join("; "));
            }
            self.numeric_entry = Default::default();
            return Ok(());
        }
        let pointer = self
            .numeric_entry
            .pointer
            .or(self.cursor_snap.map(|s| s.point));
        let point = self.numeric_point(entry, pointer)?;
        if let Some(value) = entry.point.and_then(|p| self.numeric_transform_value(p)) {
            self.transform_tool_state.clear_numeric_text();
            self.transform_tool_state
                .add_numeric_text(&value.to_string());
        }
        if entry.repetition.is_some() {
            let transforms =
                self.numeric_copy_transforms(point, modeling_copy::MAX_LINEAR_ARRAY_COPIES)?;
            let mut selection = self.selection.clone();
            let report = self.commit_document_edit("Repeat copy", |doc| {
                modeling_copy::transformed_array(doc, &mut selection, &transforms)
            })?;
            self.selection = selection;
            let tool = self.transform_tool_state.active();
            self.transform_tool_state.activate(tool);
            self.transform_tool_state.set_copy_mode(true);
            self.status = format!(
                "Created {} copies. Type another reference point or pick it.",
                report.copies
            );
        } else if self.transform_tool_state.active().is_some() {
            let mut state = self.transform_tool_state.clone();
            match state.pick(point) {
                TransformPick::NeedPoint { prompt, .. } => {
                    self.transform_tool_state = state;
                    self.status = prompt.into();
                }
                TransformPick::Commit(commit) => {
                    let report = self.commit_transform(commit)?;
                    self.transform_tool_state = state;
                    self.status = transform_status(&report, commit.copy);
                }
                TransformPick::Invalid(error) => return Err(error),
            }
        } else {
            let snap = SnapResult {
                point,
                normal: self.numeric_axes()[2],
                kind: SnapKind::Free,
                screen_distance: 0.,
            };
            self.numeric_entry.text.clear();
            if architecture_workflow::active(&self.active_tool) {
                self.arch_click(point, Some(snap), None);
            } else {
                self.handle_viewport_click_at_point(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE),
                    egui::Pos2::ZERO,
                    Some(snap),
                    Some(point),
                );
            }
        }
        self.numeric_entry.text.clear();
        self.numeric_entry.axes = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> MeshcraftApp {
        super::super::tests::test_app()
    }
    fn enter(app: &mut MeshcraftApp, text: &str, pointer: Vec3) -> CommandResult {
        app.numeric_entry.pointer = Some(pointer);
        assert!(app.append_tool_numeric_text(text));
        app.dispatch_command("tool.finish", &CommandPayload::default())
            .result
    }
    #[test]
    fn line_accepts_units_relative_coordinates_and_repairable_invalid_buffers() {
        let mut app = fixture();
        app.document
            .update_model_settings(meshcraft_core::document::ModelSettings {
                unit_name: "cm".into(),
                unit_size: 30.,
                grid_size: 0.5,
                ..Default::default()
            })
            .unwrap();
        app.activate_tool("Line");
        app.handle_native_world_pick(Vec3::X * 10., None, None);
        assert_eq!(
            CommandCompletion::Completed,
            enter(&mut app, "30cm", Vec3::X * 100.).completion
        );
        assert_eq!(Some(Vec3::X * 11.), app.drawing_tool.anchor());
        assert_eq!(
            CommandCompletion::Completed,
            enter(&mut app, "2,3,4", Vec3::ZERO).completion
        );
        assert_eq!(Some(Vec3::new(13., 3., 4.)), app.drawing_tool.anchor());
        let revision = app.document.revision();
        assert_eq!(
            CommandCompletion::Failed,
            enter(&mut app, "1oops", Vec3::X).completion
        );
        assert_eq!(revision, app.document.revision());
        assert_eq!("1oops", app.tool_numeric_text());
        for _ in 0..5 {
            assert!(app.backspace_tool_numeric());
        }
        assert!(app.tool_numeric_text().is_empty());
        app.handle_native_escape();
        assert_eq!("Select", app.active_tool_label());
    }
    #[test]
    fn generic_circle_radius_and_copy_multiply_divide_use_normal_commit_paths() {
        let mut app = fixture();
        app.activate_tool("Circle");
        app.handle_native_world_pick(Vec3::new(5., 0., 0.), None, None);
        assert_eq!(
            CommandCompletion::Completed,
            enter(&mut app, "2.5", Vec3::new(15., 0., 0.)).completion
        );
        let bounds = app
            .document
            .scene_geometry_refs()
            .unwrap()
            .iter()
            .filter_map(|p| app.document.effective_geometry(p))
            .flat_map(|g| g.vertices.values().map(|v| v.position).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert!(bounds.iter().any(|p| (p.x - 7.5).abs() < 1e-8));
        let mut app = fixture();
        let original = app
            .document
            .add_cuboid("source", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        for (text, span, expected) in [
            ("x3", 2., vec![0., 2., 4., 6.]),
            ("/4", 8., vec![0., 2., 4., 6., 8.]),
        ] {
            app.set_selection([original.clone()]);
            app.activate_tool("Move");
            app.transform_tool_state.set_copy_mode(true);
            app.handle_native_world_pick(Vec3::ZERO, None, None);
            assert_eq!(
                CommandCompletion::Completed,
                enter(&mut app, text, Vec3::X * span).completion
            );
            let mut offsets = app
                .document
                .occurrence_refs()
                .unwrap()
                .iter()
                .map(|p| {
                    app.document
                        .resolve_occurrence(p)
                        .unwrap()
                        .world_transform
                        .translation
                        .x
                })
                .collect::<Vec<_>>();
            offsets.sort_by(f64::total_cmp);
            assert_eq!(expected, offsets);
            app.undo_document();
            assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        }
    }
    #[test]
    fn copy_rotation_keeps_numeric_angles_and_scaling_divides_from_identity() {
        let mut app = fixture();
        let original = app
            .document
            .add_cuboid("source", Vec3::new(1., 1., 1.), Vec3::X)
            .unwrap();
        app.set_selection([original]);
        app.activate_tool("Rotate");
        app.transform_tool_state.set_copy_mode(true);
        for p in [Vec3::ZERO, Vec3::Y, Vec3::X] {
            app.handle_native_world_pick(p, None, None);
        }
        assert_eq!(
            CommandCompletion::Completed,
            enter(&mut app, "90deg /3", -Vec3::Z).completion
        );
        let mut angles = app
            .document
            .occurrence_refs()
            .unwrap()
            .iter()
            .map(|p| {
                let v = app
                    .document
                    .resolve_occurrence(p)
                    .unwrap()
                    .world_transform
                    .translation;
                (-v.z).atan2(v.x).to_degrees()
            })
            .collect::<Vec<_>>();
        angles.sort_by(f64::total_cmp);
        for (a, b) in angles.iter().zip([0., 30., 60., 90.]) {
            assert!((a - b).abs() < 1e-8);
        }
        app.undo_document();
        let original = app.document.occurrence_refs().unwrap()[0].clone();
        app.set_selection([original]);
        app.activate_tool("Scale");
        app.transform_tool_state.set_copy_mode(true);
        app.handle_native_world_pick(Vec3::ZERO, None, None);
        app.handle_native_world_pick(Vec3::X, None, None);
        assert_eq!(
            CommandCompletion::Completed,
            enter(&mut app, "3 /2", Vec3::X * 3.).completion
        );
        let mut scales = app
            .document
            .occurrence_refs()
            .unwrap()
            .iter()
            .map(|p| {
                app.document
                    .resolve_occurrence(p)
                    .unwrap()
                    .world_transform
                    .linear[0][0]
            })
            .collect::<Vec<_>>();
        scales.sort_by(f64::total_cmp);
        assert_eq!(vec![1., 2., 3.], scales);
    }
}
