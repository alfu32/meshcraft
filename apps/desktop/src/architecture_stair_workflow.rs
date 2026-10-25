//! Stair inputs accept either complete occurrences or selected connected segments.
use super::*;
impl MeshcraftApp {
    pub(super) fn stair_source(
        &self,
        targets: &[SelectionTarget],
        seed: Option<&SelectionTarget>,
        closed: bool,
    ) -> Result<Vec<Vec3>, String> {
        type Key = [i64; 3];
        let key = |p: Vec3| [p.x, p.y, p.z].map(|v| (v / 1e-6).round() as i64);
        let mut edges = Vec::<[Vec3; 2]>::new();
        let mut seen = BTreeSet::new();
        let paths = self.document.scene_geometry_refs()?;
        for target in targets {
            let occurrences = if target.entity == SelectedEntity::Occurrence {
                paths
                    .iter()
                    .filter(|p| p.path.starts_with(&target.occurrence.path))
                    .cloned()
                    .collect()
            } else {
                vec![target.occurrence.clone()]
            };
            for occurrence in occurrences {
                let r = self
                    .document
                    .resolve_occurrence(&occurrence)
                    .ok_or("Input object no longer exists")?;
                let g = self
                    .document
                    .effective_geometry(&occurrence)
                    .ok_or("Input object has no geometry")?;
                for edge in g.segments.values() {
                    if let SelectedEntity::Segment(id) = target.entity {
                        if seed.is_none() && edge.id != id {
                            continue;
                        }
                    } else if target.entity != SelectedEntity::Occurrence {
                        return Err(
                            "Select contour/path segments or their object instances.".into()
                        );
                    }
                    let p = edge.endpoints.map(|v| {
                        g.vertex_position(v)
                            .map(|p| r.world_transform.transform_point(p))
                    });
                    let [Some(a), Some(b)] = p else {
                        continue;
                    };
                    let mut k = [key(a), key(b)];
                    k.sort();
                    if k[0] == k[1] {
                        continue;
                    }
                    if seen.insert(k) {
                        edges.push([a, b]);
                    }
                }
            }
        }
        if edges.is_empty() || edges.len() > 8192 {
            return Err("Choose 1–8192 connected source segments.".into());
        }
        let mut adjacency = BTreeMap::<Key, Vec<usize>>::new();
        for (i, e) in edges.iter().enumerate() {
            for p in e {
                adjacency.entry(key(*p)).or_default().push(i);
            }
        }
        if let Some(SelectionTarget {
            occurrence,
            entity: SelectedEntity::Segment(id),
        }) = seed
        {
            let r = self
                .document
                .resolve_occurrence(occurrence)
                .ok_or("Picked source is missing")?;
            let g = self
                .document
                .effective_geometry(occurrence)
                .ok_or("Picked source is empty")?;
            let edge = g.segments.get(id).ok_or("Picked edge is missing")?;
            let point = g
                .vertex_position(edge.endpoints[0])
                .map(|p| r.world_transform.transform_point(p))
                .ok_or("Picked endpoint is missing")?;
            let mut stack = vec![key(point)];
            let mut connected = BTreeSet::new();
            let mut nodes = BTreeSet::new();
            while let Some(p) = stack.pop() {
                if !nodes.insert(p) {
                    continue;
                }
                for &i in adjacency.get(&p).into_iter().flatten() {
                    connected.insert(i);
                    for end in edges[i] {
                        stack.push(key(end));
                    }
                }
            }
            edges = connected.into_iter().map(|i| edges[i]).collect();
            adjacency.clear();
            for (i, e) in edges.iter().enumerate() {
                for p in e {
                    adjacency.entry(key(*p)).or_default().push(i);
                }
            }
        }
        let ends = adjacency
            .iter()
            .filter(|(_, e)| e.len() == 1)
            .map(|(k, _)| *k)
            .collect::<Vec<_>>();
        if adjacency.values().any(|e| e.len() > 2)
            || (closed && !ends.is_empty())
            || (!closed && ends.len() != 2)
        {
            return Err(if closed {
                "Contour C must be one closed, unbranched polyline."
            } else {
                "Walking path P must be one open, unbranched polyline."
            }
            .into());
        }
        let mut current = if closed { key(edges[0][0]) } else { ends[0] };
        let mut used = BTreeSet::new();
        let mut points = vec![];
        loop {
            let next = adjacency
                .get(&current)
                .and_then(|indices| indices.iter().find(|i| !used.contains(*i)))
                .copied();
            let Some(i) = next else {
                break;
            };
            used.insert(i);
            let e = edges[i];
            let forward = key(e[0]) == current;
            if points.is_empty() {
                points.push(if forward { e[0] } else { e[1] });
            }
            let p = if forward { e[1] } else { e[0] };
            points.push(p);
            current = key(p);
        }
        if used.len() != edges.len() {
            return Err(
                "Input contains disconnected contours/paths; select one connected set.".into(),
            );
        }
        if closed {
            points.pop();
            if points.len() < 3 {
                return Err("Contour C needs at least three corners.".into());
            }
        } else {
            // Keep the source segment direction, so reversing P reverses ascent and swaps left/right.
            if let Some(i) = points.windows(2).position(|p| {
                (key(p[0]) == key(edges[0][0]) && key(p[1]) == key(edges[0][1]))
                    || (key(p[1]) == key(edges[0][0]) && key(p[0]) == key(edges[0][1]))
            }) && key(points[i]) != key(edges[0][0])
            {
                points.reverse();
            }
        }
        Ok(points)
    }
    pub(super) fn stair_element(
        &self,
        contour: Vec<Vec3>,
        path: Vec<Vec3>,
        payload: &CommandPayload,
    ) -> Result<Element, String> {
        let (_, _, inverse) = self.arch_owner();
        let mut stair = self.arch_defaults().stair;
        let preferred = self.preferences.grid_normal_axis.vector();
        let mut derived = Vec3::ZERO;
        if let Some(origin) = contour.first() {
            for i in 0..contour.len() {
                derived += (contour[i] - *origin).cross(contour[(i + 1) % contour.len()] - *origin);
            }
        }
        let mut normal = derived.try_normalized(1e-9).unwrap_or(preferred);
        if normal.dot(preferred) < 0. {
            normal = -normal;
        }
        let normal = payload_vec3(payload, "normal", normal)?;
        stair.up = inverse
            .transform_normal(normal, GeometryTolerance::default())
            .ok_or("Stair normal is invalid")?;
        stair.contour = contour
            .into_iter()
            .map(|p| inverse.transform_point(p))
            .collect();
        stair.walking_path = path
            .into_iter()
            .map(|p| inverse.transform_point(p))
            .collect();
        let mut element = Element::Stair(stair);
        Self::arch_patch(&mut element, payload)?;
        element.mesh()?;
        Ok(element)
    }
    pub(super) fn commit_stair_input(
        &mut self,
        path: Vec<Vec3>,
        payload: &CommandPayload,
    ) -> Result<SelectionTarget, String> {
        let contour = self
            .architecture
            .stair_contour
            .clone()
            .ok_or("Set the closed contour C first.")?;
        let element = self.stair_element(contour, path, payload)?;
        self.commit_arch_element(element)
    }
    pub fn native_stair_contour_ready(&self) -> bool {
        self.architecture.stair_contour.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> (MeshcraftApp, Vec<SelectionTarget>, SelectionTarget) {
        let mut doc = Document::new();
        let (segments, _) = doc
            .append_planar_mesh_to_scene_root(
                vec![
                    Vec3::ZERO,
                    Vec3::X * 4.,
                    Vec3::new(4., 0., 12.),
                    Vec3::Z * 12.,
                    Vec3::new(2., 0., 1.),
                    Vec3::new(2., 0., 11.),
                ],
                vec![[0, 1], [1, 2], [2, 3], [3, 0], [4, 5]],
                vec![],
            )
            .unwrap();
        let path = doc
            .scene_geometry_refs()
            .unwrap()
            .into_iter()
            .find(|p| {
                doc.effective_geometry(p)
                    .is_some_and(|g| g.segments.contains_key(&segments[0]))
            })
            .unwrap();
        let target = |id| SelectionTarget {
            occurrence: path.clone(),
            entity: SelectedEntity::Segment(id),
        };
        let c = segments[..4].iter().copied().map(target).collect();
        let p = target(segments[4]);
        (
            MeshcraftApp::new_native_session(doc, "Stair test".into(), None),
            c,
            p,
        )
    }
    fn command(app: &mut MeshcraftApp, id: &str, args: Value) -> CommandResult {
        let payload = CommandPayload {
            values: args
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            ..Default::default()
        };
        let result = app.dispatch_command(id, &payload).result;
        assert_eq!(
            result.completion,
            CommandCompletion::Completed,
            "{id}: {:?}",
            result.diagnostics
        );
        result
    }
    #[test]
    fn stair_selection_sets_and_two_click_picking_create_the_same_owner() {
        let (mut app, c, p) = source();
        app.selection.apply(c.clone(), SelectionMode::Replace);
        command(&mut app, "architecture.stair_contour", json!({}));
        assert!(app.native_stair_contour_ready());
        app.activate_tool("Select");
        assert!(app.native_stair_contour_ready());
        app.selection.apply([p.clone()], SelectionMode::Replace);
        command(
            &mut app,
            "architecture.stair_path",
            json!({"stair_spacing":2.,"stair_levels":5}),
        );
        let owner = app.native_selected_architecture().unwrap();
        assert_eq!(owner.element.kind(), SelectionKind::Stair);
        assert_eq!(owner.element.value()["height"], 10.);
        assert_eq!(owner.element.value()["railSpacing"], 2.);
        assert!(!app.native_stair_contour_ready());
        let record = owner.element.value();
        command(&mut app, "edit.undo", json!({}));
        assert!(app.document.architecture_elements().is_empty());
        command(&mut app, "edit.redo", json!({}));
        assert_eq!(
            app.document.architecture_elements()[0].element.value(),
            record
        );
        let (mut app, c, p) = source();
        command(&mut app, "tool.builtin.arch_stair", json!({}));
        app.handle_native_world_pick(Vec3::ZERO, None, Some(c[0].clone()));
        assert!(app.native_stair_contour_ready());
        app.handle_native_world_pick(Vec3::ZERO, None, Some(p));
        assert_eq!(app.active_tool, "Select");
        assert_eq!(app.document.architecture_elements().len(), 1);
    }
    #[test]
    fn stair_object_inputs_respect_instance_placement_and_path_direction() {
        let mut doc = Document::new();
        doc.ensure_scene_root();
        let c = doc.add_definition("C", meshcraft_core::document::DefinitionKind::Mesh);
        doc.append_planar_mesh_to_definition(
            c,
            vec![
                Vec3::ZERO,
                Vec3::X * 4.,
                Vec3::new(4., 0., 12.),
                Vec3::Z * 12.,
            ],
            vec![[0, 1], [1, 2], [2, 3], [3, 0]],
            vec![],
        )
        .unwrap();
        let p = doc.add_definition("P", meshcraft_core::document::DefinitionKind::Mesh);
        doc.append_planar_mesh_to_definition(
            p,
            vec![Vec3::new(2., 0., 12.), Vec3::new(2., 0., 0.)],
            vec![[0, 1]],
            vec![],
        )
        .unwrap();
        let offset = Vec3::new(10., 0., 5.);
        let c = meshcraft_core::operations::objects::place_definition(
            &mut doc,
            &EditContext::Document,
            c,
            offset,
            None,
            false,
        )
        .unwrap();
        let p = meshcraft_core::operations::objects::place_definition(
            &mut doc,
            &EditContext::Document,
            p,
            offset,
            None,
            false,
        )
        .unwrap();
        let mut app = MeshcraftApp::new_native_session(doc, "Stair objects".into(), None);
        app.selection.apply(
            [SelectionTarget {
                occurrence: c,
                entity: SelectedEntity::Occurrence,
            }],
            SelectionMode::Replace,
        );
        command(&mut app, "tool.builtin.arch_stair", json!({}));
        assert!(app.native_stair_contour_ready());
        app.handle_native_world_pick(
            Vec3::ZERO,
            None,
            Some(SelectionTarget {
                occurrence: p,
                entity: SelectedEntity::Occurrence,
            }),
        );
        let Element::Stair(s) = app.native_selected_architecture().unwrap().element else {
            panic!()
        };
        assert_eq!(s.walking_path[0], offset + Vec3::new(2., 0., 12.));
        assert_eq!(s.contour.len(), 4);
    }
    #[test]
    fn stair_properties_height_handle_copy_paste_and_explode_remain_semantic() {
        let (mut app, c, p) = source();
        app.selection.apply(c, SelectionMode::Replace);
        command(&mut app, "tool.builtin.arch_stair", json!({}));
        app.handle_native_world_pick(Vec3::ZERO, None, Some(p));
        let owner = app.native_selected_architecture().unwrap();
        command(
            &mut app,
            "architecture.update",
            json!({"stair_height":12.,"stair_left":false,"stair_steps":8}),
        );
        assert_eq!(
            app.native_selected_architecture().unwrap().element.value()["height"],
            12.
        );
        let original = app.document.clone();
        let original_history = (app.history.entries().len(), app.history.cursor());
        assert!(
            app.commit_native_architecture_control(
                &owner.target,
                &Control::StairHeight,
                Vec3::new(2., -2., 12.)
            )
            .is_err()
        );
        assert_eq!(original, app.document);
        assert_eq!(
            original_history,
            (app.history.entries().len(), app.history.cursor())
        );
        app.commit_native_architecture_control(
            &owner.target,
            &Control::StairHeight,
            Vec3::new(2., 15., 12.),
        )
        .unwrap();
        assert_eq!(
            app.native_selected_architecture().unwrap().element.value()["height"],
            15.
        );
        let material = meshcraft_core::document::materials::Material {
            id: "stair-procedural".into(),
            name: "Stair procedural".into(),
            procedural: Some(Default::default()),
            ..Default::default()
        };
        app.save_native_material(material).unwrap();
        app.assign_native_material("stair-procedural").unwrap();
        assert!(
            app.native_selected_architecture()
                .unwrap()
                .element
                .surface_materials()
                .values()
                .all(|id| id == "stair-procedural")
        );
        command(&mut app, "edit.copy_selection", json!({}));
        command(&mut app, "edit.paste_in_place", json!({}));
        assert_eq!(app.document.architecture_elements().len(), 2);
        command(&mut app, "edit.undo", json!({}));
        assert_eq!(app.document.architecture_elements().len(), 1);
        app.selection.apply([owner.target], SelectionMode::Replace);
        command(&mut app, "object.explode_selection", json!({}));
        assert!(app.document.architecture_elements().is_empty());
        assert!(
            !app.document
                .definition(Document::SCENE_ROOT_DEFINITION_ID)
                .unwrap()
                .geometry
                .triangles
                .is_empty()
        );
    }
}
