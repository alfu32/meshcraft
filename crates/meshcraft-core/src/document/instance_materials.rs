//! Whole-instance finishes stay authored independently of generated/shared topology.
use super::*;

pub const KEY: &str = "k3d.meshcraftInstanceMaterial";
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Binding {
    pub id: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
}
impl Document {
    /// The closest instance override also covers that instance's nested geometry.
    pub fn instance_material(&self, path: &OccurrenceRef) -> Option<Binding> {
        (1..=path.path.len()).rev().find_map(|n| {
            let prefix = OccurrenceRef {
                path: path.path[..n].to_vec(),
            };
            let edits = self.resolve_occurrence(&prefix)?.edits?;
            serde_json::from_value(edits.values.get(KEY)?.clone()).ok()
        })
    }
    pub fn set_instance_material(
        &mut self,
        path: &OccurrenceRef,
        id: &str,
        parameters: BTreeMap<String, Value>,
    ) -> Result<(), String> {
        if path.is_scene_root() || !self.is_object_occurrence(path) {
            return Err("Material override requires an object instance".into());
        }
        if self.procedural_object(path).is_some() {
            return Err("Shader-generated objects use Shader Object Settings; bake to mesh to assign a surface material".into());
        }
        let material = self
            .materials()
            .into_iter()
            .find(|m| m.id == id)
            .ok_or("Material missing")?;
        if !parameters.is_empty() {
            let p = material.procedural.ok_or("Material is conventional")?;
            self.procedural_programs()
                .into_iter()
                .find(|v| v.id == p.program)
                .ok_or("Program unavailable")?
                .resolve(&p.parameters, &parameters)?;
        }
        let mut edits = self.occurrence_edits(path).cloned().unwrap_or_default();
        edits.values.insert(
            KEY.into(),
            serde_json::to_value(Binding {
                id: id.into(),
                parameters,
            })
            .unwrap(),
        );
        if !self.set_occurrence_edits(path.clone(), edits) {
            return Err("Instance unavailable".into());
        }
        // Painting a group replaces all of its finishes, including older child
        // overrides. Mask shared child defaults on this realized parent only.
        let has_children = self
            .resolve_occurrence(path)
            .and_then(|r| {
                r.edits
                    .and_then(|e| e.child_layout)
                    .or(r.definition.default_child_layout)
            })
            .and_then(|id| self.child_layout(id))
            .is_some_and(|l| !l.entries.is_empty());
        if has_children {
            let children = self
                .occurrence_refs()?
                .into_iter()
                .filter(|p| p != path && p.path.starts_with(&path.path))
                .collect::<Vec<_>>();
            for child in children {
                let Some(mut edits) = self
                    .occurrence_edits(&child)
                    .filter(|e| e.values.contains_key(KEY))
                    .cloned()
                else {
                    continue;
                };
                edits.values.insert(KEY.into(), Value::Null);
                self.set_occurrence_edits(child, edits);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Vec3;
    #[test]
    fn gpu_generators_require_their_shader_settings_instead_of_silently_ignoring_paint() {
        let mut d = Document::new();
        let definition = d.create_procedural_tree().unwrap();
        let path = crate::operations::objects::place_definition(
            &mut d,
            &crate::selection::EditContext::Document,
            definition,
            Vec3::ZERO,
            None,
            false,
        )
        .unwrap();
        let before = d.clone();
        assert!(
            d.set_instance_material(&path, "ansi-red", BTreeMap::new())
                .unwrap_err()
                .contains("Shader Object Settings")
        );
        assert_eq!(d, before);
    }
    #[test]
    fn instance_finish_is_cached_copied_persistent_and_never_replaces_topology() {
        let mut d = Document::new();
        let a = d
            .add_cuboid("box", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        let b = d.copy_root_occurrence(&a, Vec3::X * 3.).unwrap();
        let original = d.effective_geometry(&a).unwrap().clone();
        d.set_instance_material(&a, "ansi-red", BTreeMap::new())
            .unwrap();
        let mesh = d.effective_geometry(&a).unwrap();
        assert!(
            mesh.triangles
                .values()
                .all(|f| materials::material_id(&f.extensions) == Some("ansi-red"))
        );
        assert!(std::ptr::eq(mesh, d.effective_geometry(&a).unwrap()));
        assert_eq!(original.vertices, mesh.vertices);
        assert_eq!(original, *d.effective_geometry(&b).unwrap());
        assert!(
            d.occurrence_edits(&a)
                .unwrap()
                .geometry_replacement
                .is_none()
        );
        let c = d.copy_root_occurrence(&a, Vec3::Z * 3.).unwrap();
        assert_eq!(d.instance_material(&c).unwrap().id, "ansi-red");
        assert!(d.remove_material("ansi-red").is_err());
        let value = crate::legacy::export_current_value(&d).unwrap();
        let reopened = crate::legacy::import_current_value(&value).unwrap();
        assert_eq!(
            2,
            reopened
                .occurrence_refs()
                .unwrap()
                .iter()
                .filter(|p| reopened
                    .instance_material(p)
                    .is_some_and(|m| m.id == "ansi-red"))
                .count()
        );
    }
    #[test]
    fn group_finish_masks_child_defaults_only_in_the_painted_parent() {
        let mut d = Document::new();
        let cube = d
            .add_cuboid("child", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        d.set_instance_material(&cube, "ansi-red", BTreeMap::new())
            .unwrap();
        let group =
            crate::operations::objects::group_occurrences(&mut d, &[cube], "group").unwrap();
        let twin = d.copy_root_occurrence(&group, Vec3::X * 4.).unwrap();
        let layout = d
            .resolve_occurrence(&group)
            .unwrap()
            .definition
            .default_child_layout
            .unwrap();
        let entry = d.child_layout(layout).unwrap().entries[0].id;
        let child = group.child(entry);
        let other = twin.child(entry);
        d.set_instance_material(&group, "ansi-green", BTreeMap::new())
            .unwrap();
        assert_eq!(d.instance_material(&child).unwrap().id, "ansi-green");
        assert_eq!(d.instance_material(&other).unwrap().id, "ansi-red");
        d.set_instance_material(&child, "ansi-blue", BTreeMap::new())
            .unwrap();
        assert_eq!(d.instance_material(&child).unwrap().id, "ansi-blue");
        assert_eq!(d.instance_material(&other).unwrap().id, "ansi-red");
    }
    #[test]
    fn import_remaps_instance_material_ids_on_catalog_collision() {
        let mut source = Document::new();
        let cube = source
            .add_cuboid("painted", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let mut red = source.next_material();
        red.color = Rgba8::new(230, 10, 20, 255);
        source.put_material(red.clone()).unwrap();
        source
            .set_instance_material(&cube, &red.id, BTreeMap::new())
            .unwrap();
        let mut host = Document::new();
        let mut blue = host.next_material();
        blue.color = Rgba8::new(10, 20, 230, 255);
        host.put_material(blue.clone()).unwrap();
        let imported = host
            .import_subscene(&source, "Imported", None, None)
            .unwrap();
        let path = crate::operations::objects::place_definition(
            &mut host,
            &crate::selection::EditContext::Document,
            imported,
            Vec3::ZERO,
            None,
            false,
        )
        .unwrap();
        let painted = host
            .scene_geometry_refs()
            .unwrap()
            .into_iter()
            .find(|p| p.path.starts_with(&path.path) && host.instance_material(p).is_some())
            .unwrap();
        let binding = host.instance_material(&painted).unwrap();
        assert_ne!(binding.id, blue.id);
        assert_eq!(host.material_colors()[&binding.id], red.color);
        assert!(
            host.effective_geometry(&painted)
                .unwrap()
                .triangles
                .values()
                .all(|f| f.color == red.color)
        );
    }
    #[test]
    fn instance_finish_survives_script_updates_and_catalog_edits() {
        let mut d = Document::new();
        let a = d
            .add_cuboid("script", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        let def = d.resolve_occurrence(&a).unwrap().definition.id;
        d.set_script(def, scripts::ScriptDefinition::default())
            .unwrap();
        d.set_instance_material(&a, "ansi-red", BTreeMap::new())
            .unwrap();
        let frame = scripts::ScriptFrame {
            geometry: d.unpainted_geometry(&a).unwrap().clone(),
            hotspots: vec![],
            state: Default::default(),
        };
        d.install_script_frames(BTreeMap::from([(a.clone(), frame.clone())]));
        assert!(
            d.effective_geometry(&a)
                .unwrap()
                .triangles
                .values()
                .all(|f| f.color == Rgba8::new(128, 0, 0, 255))
        );
        let mut frame = frame;
        frame
            .geometry
            .vertices
            .values_mut()
            .for_each(|v| v.position.y += 4.);
        d.install_script_frames(BTreeMap::from([(a.clone(), frame)]));
        assert!(
            d.effective_geometry(&a)
                .unwrap()
                .vertices
                .values()
                .all(|v| v.position.y >= 4.)
        );
        let mut red = d
            .materials()
            .into_iter()
            .find(|m| m.id == "ansi-red")
            .unwrap();
        red.color = Rgba8::new(123, 12, 13, 255);
        d.put_material(red.clone()).unwrap();
        assert!(
            d.effective_geometry(&a)
                .unwrap()
                .triangles
                .values()
                .all(|f| f.color == red.color)
        );
    }
}
