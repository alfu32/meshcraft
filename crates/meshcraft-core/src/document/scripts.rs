//! Authored JavaScript hooks and bounded, nonpersistent evaluated instance geometry.
use super::*;
use std::sync::Arc;

pub const SCRIPT_KEY: &str = "k3d.meshcraftScript";
pub const STATE_KEY: &str = "k3d.meshcraftScriptState";
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScriptDefinition {
    pub enabled: bool,
    /// Unified module source; empty values use the older separate hook bodies.
    pub source: String,
    pub on_create: String,
    pub on_animation_frame: String,
}
impl Default for ScriptDefinition {
    fn default() -> Self {
        Self {
            enabled: true,
            source: String::new(),
            on_create: String::new(),
            on_animation_frame: "return this.geometry;".into(),
        }
    }
}
impl ScriptDefinition {
    pub fn editor_source(&self) -> String {
        if !self.source.trim().is_empty() {
            return self.source.clone();
        }
        let hook = |name: &str, body: &str| {
            if body.trim_start().starts_with("function") {
                format!("const {name} = ({body});")
            } else {
                format!("function {name}(instance, model) {{\n{body}\n}}")
            }
        };
        format!(
            "{}\n\n{}\n",
            hook("onCreate", &self.on_create),
            hook("onAnimationFrame", &self.on_animation_frame)
        )
    }
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ScriptState {
    pub initializer: String,
    pub properties: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScriptFrame {
    pub geometry: GeometryBundle,
    pub hotspots: Vec<hotspots::Hotspot>,
    pub state: ScriptState,
}
#[derive(Clone, Debug, Default)]
pub struct FrameCache {
    pub frames: Arc<BTreeMap<OccurrenceRef, ScriptFrame>>,
    pub generation: u64,
}
impl PartialEq for FrameCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScriptPoint {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    #[serde(default)]
    pub id: String,
}
impl ScriptPoint {
    fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Drawable {
    Authored,
    Segment {
        id: String,
        points: [ScriptPoint; 2],
        color: Rgba8,
        #[serde(default)]
        material: Option<materials::Material>,
    },
    Face {
        id: String,
        points: [ScriptPoint; 3],
        color: Rgba8,
        #[serde(default)]
        material: Option<materials::Material>,
    },
    Hotspot {
        id: String,
        position: Vec3,
    },
}
fn stable_id(kind: &str, id: &str) -> u64 {
    kind.bytes()
        .chain(id.bytes())
        .fold(0xcbf29ce484222325, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100000001b3)
        })
        | (1 << 63)
}
impl ScriptFrame {
    pub fn build(
        authored: &GeometryBundle,
        items: Vec<Drawable>,
        state: ScriptState,
    ) -> Result<Self, String> {
        if items.len() > 100_000 {
            return Err("script output exceeds 100,000 drawables".into());
        }
        let mut result = Self {
            geometry: GeometryBundle {
                id: authored.id,
                ..Default::default()
            },
            hotspots: Vec::new(),
            state,
        };
        let mut ids = BTreeSet::new();
        for item in items {
            match item {
                Drawable::Authored => {
                    result.geometry.vertices.extend(authored.vertices.clone());
                    result.geometry.segments.extend(authored.segments.clone());
                    result.geometry.triangles.extend(authored.triangles.clone());
                }
                Drawable::Hotspot { id, position } => {
                    if id.is_empty()
                        || !position.is_finite()
                        || !ids.insert(format!("hotspot:{id}"))
                    {
                        return Err("hotspot IDs must be unique and positions finite".into());
                    }
                    result.hotspots.push(hotspots::Hotspot {
                        name: id.clone(),
                        id,
                        position,
                        ..Default::default()
                    });
                }
                Drawable::Segment {
                    id,
                    points,
                    color,
                    material,
                } => {
                    if id.is_empty() || !ids.insert(format!("segment:{id}")) {
                        return Err("segment IDs must be nonempty and unique".into());
                    }
                    let refs = insert_points(&mut result.geometry, points)?;
                    let authored_id = id
                        .strip_prefix("authored:segment:")
                        .and_then(|id| id.parse().ok())
                        .map(SegmentId);
                    let original = authored_id.and_then(|id| authored.segments.get(&id));
                    let extensions = drawable_extensions(
                        original.map(|s| (&s.extensions, s.color)),
                        &id,
                        color,
                        material,
                    )?;
                    let segment_id = authored_id.unwrap_or(SegmentId(stable_id("segment", &id)));
                    result.geometry.segments.insert(
                        segment_id,
                        Segment {
                            id: segment_id,
                            endpoints: refs,
                            color,
                            extensions,
                            legacy_aliases: Vec::new(),
                        },
                    );
                }
                Drawable::Face {
                    id,
                    points,
                    color,
                    material,
                } => {
                    if id.is_empty() || !ids.insert(format!("face:{id}")) {
                        return Err("face IDs must be nonempty and unique".into());
                    }
                    let refs = insert_points(&mut result.geometry, points)?;
                    let authored_id = id
                        .strip_prefix("authored:face:")
                        .and_then(|id| id.parse().ok())
                        .map(TriangleId);
                    let original = authored_id.and_then(|id| authored.triangles.get(&id));
                    let extensions = drawable_extensions(
                        original.map(|s| (&s.extensions, s.color)),
                        &id,
                        color,
                        material,
                    )?;
                    let triangle_id = authored_id.unwrap_or(TriangleId(stable_id("face", &id)));
                    result.geometry.triangles.insert(
                        triangle_id,
                        Triangle {
                            id: triangle_id,
                            vertices: refs,
                            color,
                            extensions,
                            legacy_aliases: Vec::new(),
                        },
                    );
                }
            }
        }
        result.geometry.validate()?;
        Ok(result)
    }
}
fn drawable_extensions(
    original: Option<(&BTreeMap<String, Value>, Rgba8)>,
    id: &str,
    color: Rgba8,
    material: Option<materials::Material>,
) -> Result<BTreeMap<String, Value>, String> {
    let mut extensions = original.map(|(e, _)| e.clone()).unwrap_or_default();
    if original.is_some_and(|(_, c)| c != color) || material.is_some() {
        extensions.remove(materials::MATERIAL_KEY);
    }
    extensions.insert("k3d.meshcraftScriptId".into(), Value::String(id.into()));
    if let Some(material) = material {
        material.validate()?;
        extensions.insert(
            "k3d.meshcraftScriptMaterial".into(),
            serde_json::to_value(material).map_err(|e| e.to_string())?,
        );
    }
    Ok(extensions)
}
fn insert_points<const N: usize>(
    g: &mut GeometryBundle,
    points: [ScriptPoint; N],
) -> Result<[VertexRef; N], String> {
    let mut refs = [VertexRef::new(VertexId(0)); N];
    for (i, point) in points.into_iter().enumerate() {
        let position = point.position();
        let key = if point.id.is_empty() {
            format!(
                "{:016x}:{:016x}:{:016x}",
                position.x.to_bits(),
                position.y.to_bits(),
                position.z.to_bits()
            )
        } else {
            point.id
        };
        let id = VertexId(stable_id("point", &key));
        if g.vertices.get(&id).is_some_and(|v| v.position != position) {
            return Err("one Point ID has conflicting coordinates".into());
        }
        g.vertices.insert(
            id,
            Vertex {
                id,
                position,
                legacy_aliases: Vec::new(),
            },
        );
        refs[i] = VertexRef::new(id);
    }
    Ok(refs)
}
/// Script-visible local geometry with stable point, edge and face identities.
pub fn geometry_drawables(geometry: &GeometryBundle) -> Vec<Value> {
    let point = |reference: VertexRef| {
        let p = geometry.vertex_position(reference).unwrap_or(Vec3::ZERO);
        serde_json::json!({"x":p.x,"y":p.y,"z":p.z,"id":format!("authored:point:{}:{:?}",reference.id.0,reference.position_offset)})
    };
    geometry.segments.values().map(|s|serde_json::json!({"kind":"segment","id":format!("authored:segment:{}",s.id.0),"points":s.endpoints.map(&point),"color":s.color})).chain(geometry.triangles.values().map(|f|serde_json::json!({"kind":"face","id":format!("authored:face:{}",f.id.0),"points":f.vertices.map(&point),"color":f.color}))).collect()
}
impl Document {
    pub fn script(&self, id: DefinitionId) -> Option<ScriptDefinition> {
        self.definition(id)?
            .extensions
            .get(SCRIPT_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
    pub fn set_script(&mut self, id: DefinitionId, source: ScriptDefinition) -> Result<(), String> {
        if source.source.len() + source.on_create.len() + source.on_animation_frame.len()
            > 1_000_000
        {
            return Err("script source exceeds 1 MB".into());
        }
        let definition = self
            .definitions
            .get_mut(&id)
            .ok_or("prototype no longer exists")?;
        definition.extensions.insert(
            SCRIPT_KEY.into(),
            serde_json::to_value(source).map_err(|e| e.to_string())?,
        );
        self.bump_revision();
        Ok(())
    }
    pub fn script_frame(&self, path: &OccurrenceRef) -> Option<&ScriptFrame> {
        self.script_cache.frames.get(path)
    }
    pub fn visual_revision(&self) -> u64 {
        self.revision
            .wrapping_add(self.script_cache.generation.wrapping_mul(1_000_000_007))
    }
    pub fn install_script_frames(&mut self, frames: BTreeMap<OccurrenceRef, ScriptFrame>) {
        if frames.is_empty() {
            return;
        }
        let mut next = (*self.script_cache.frames).clone();
        next.retain(|path, _| {
            self.resolve_occurrence(path)
                .is_some_and(|r| self.script(r.definition.id).is_some_and(|s| s.enabled))
        });
        let visual_change = frames.iter().any(|(path, frame)| {
            next.get(path)
                .is_none_or(|old| old.geometry != frame.geometry || old.hotspots != frame.hotspots)
        });
        next.extend(frames);
        self.script_cache.frames = Arc::new(next);
        if visual_change {
            self.script_cache.generation = self.script_cache.generation.wrapping_add(1);
        }
    }
    pub fn persist_script_states(&mut self) {
        for (path, frame) in self.script_cache.frames.iter() {
            if !self
                .resolve_occurrence(path)
                .is_some_and(|r| self.script(r.definition.id).is_some_and(|s| s.enabled))
            {
                continue;
            }
            let mut edits = self.occurrence_edits(path).cloned().unwrap_or_default();
            edits.values.insert(
                STATE_KEY.into(),
                serde_json::to_value(&frame.state).unwrap_or(Value::Null),
            );
            self.occurrence_edits.insert(path.clone(), edits);
        }
    }
    pub fn instance_hotspots(
        &self,
        path: &OccurrenceRef,
    ) -> Result<Vec<hotspots::Hotspot>, String> {
        let r = self
            .resolve_occurrence(path)
            .ok_or("instance no longer exists")?;
        let mut result = self.hotspots(r.definition.id)?;
        if self.script(r.definition.id).is_some_and(|s| s.enabled)
            && let Some(frame) = self.script_frame(path)
        {
            for h in &frame.hotspots {
                result.retain(|old| old.id != h.id);
                result.push(h.clone());
            }
        }
        Ok(result)
    }
    pub fn scripted_occurrences(&self) -> Vec<OccurrenceRef> {
        self.scene_geometry_refs()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| {
                self.resolve_occurrence(p)
                    .is_some_and(|r| self.script(r.definition.id).is_some_and(|s| s.enabled))
            })
            .collect()
    }
    /// Isolated rendering scene for the IDE; source catalog and nested prototype hierarchy survive.
    pub fn script_preview(&self, id: DefinitionId) -> Result<(Self, OccurrenceRef), String> {
        if id == Self::SCENE_ROOT_DEFINITION_ID {
            return Err("select an object prototype".into());
        }
        let mut preview = self.clone();
        preview.ensure_scene_root();
        let entry = LayoutEntryId(preview.allocate_id());
        preview
            .child_layouts
            .get_mut(&preview.root_layout)
            .ok_or("missing root")?
            .entries = vec![LayoutEntry {
            id: entry,
            definition: id,
            transform: Affine3::IDENTITY,
            default_edits: InstanceEdits::default(),
            legacy_aliases: Vec::new(),
        }];
        preview
            .definitions
            .get_mut(&Self::SCENE_ROOT_DEFINITION_ID)
            .unwrap()
            .geometry = GeometryBundle::default();
        preview.occurrence_edits.clear();
        preview.script_cache = FrameCache::default();
        preview.bump_revision();
        Ok((preview, OccurrenceRef::root(entry)))
    }
    pub fn add_scripted_object(
        &mut self,
        name: &str,
        script: ScriptDefinition,
    ) -> Result<OccurrenceRef, String> {
        let path = self
            .add_cuboid(name, Vec3::new(1., 1., 1.), Vec3::ZERO)
            .ok_or("could not create prototype")?;
        let id = self.resolve_occurrence(&path).unwrap().definition.id;
        self.definitions.get_mut(&id).unwrap().geometry = GeometryBundle::default();
        self.set_script(id, script)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_geometry_hooks_keep_material_metadata_and_stable_point_ids() {
        let mut document = Document::new();
        let path = document
            .add_cuboid("authored", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        let original = document.effective_geometry(&path).unwrap().clone();
        let items = geometry_drawables(&original)
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<Drawable>, _>>()
            .unwrap();
        let frame = ScriptFrame::build(&original, items, ScriptState::default()).unwrap();
        assert_eq!(original.triangles.len(), frame.geometry.triangles.len());
        assert_eq!(original.segments.len(), frame.geometry.segments.len());
        assert_eq!(original.vertices.len(), frame.geometry.vertices.len());
        assert!(
            frame
                .geometry
                .triangles
                .values()
                .all(|t| materials::material_id(&t.extensions) == Some("default"))
        );
        let definition = document.resolve_occurrence(&path).unwrap().definition.id;
        document
            .set_script(definition, ScriptDefinition::default())
            .unwrap();
        let mut state = ScriptState {
            initializer: "this.width=3".into(),
            ..Default::default()
        };
        state.properties.insert("width".into(), Value::from(7));
        let mut frame = frame;
        frame.state = state.clone();
        document.install_script_frames(BTreeMap::from([(path.clone(), frame)]));
        let copy = document.copy_root_occurrence(&path, Vec3::X * 4.).unwrap();
        assert_eq!(
            Value::from(7),
            document.occurrence_edits(&copy).unwrap().values[STATE_KEY]["properties"]["width"]
        );
        let saved = crate::legacy::export_current_value(&document).unwrap();
        let reopened = crate::legacy::import_current_value(&saved).unwrap();
        assert!(reopened.occurrence_refs().unwrap().iter().any(|path| {
            reopened.occurrence_edits(path).is_some_and(|e| {
                e.values
                    .get(STATE_KEY)
                    .is_some_and(|v| v["properties"]["width"] == 7)
            })
        }));
        let preview = document.script_preview(definition).unwrap().0;
        preview.validate().unwrap();
    }
    #[test]
    fn invalid_generated_ids_and_points_do_not_replace_the_last_good_frame() {
        let authored = GeometryBundle::default();
        let duplicate =
            serde_json::json!({"kind":"hotspot","id":"h","position":{"x":0,"y":0,"z":0}});
        let items = vec![
            serde_json::from_value(duplicate.clone()).unwrap(),
            serde_json::from_value(duplicate).unwrap(),
        ];
        assert!(ScriptFrame::build(&authored, items, ScriptState::default()).is_err());
        let point = ScriptPoint {
            x: f64::INFINITY,
            y: 0.,
            z: 0.,
            id: "p".into(),
        };
        let item = Drawable::Segment {
            id: "edge".into(),
            points: [
                point,
                ScriptPoint {
                    x: 1.,
                    y: 0.,
                    z: 0.,
                    id: "q".into(),
                },
            ],
            color: Rgba8::WHITE,
            material: None,
        };
        assert!(ScriptFrame::build(&authored, vec![item], ScriptState::default()).is_err());
    }
}
