//! Persisted dimensions and billboard text/leader annotations using compatible domain records.
use super::*;
use crate::selection::{SelectedEntity, SelectionKind, SelectionTarget};

#[derive(Clone, Debug)]
pub struct Annotation {
    pub target: SelectionTarget,
    pub record: Value,
    pub edges: Vec<[Vec3; 2]>,
    pub label: String,
    pub position: Vec3,
    pub size: f64,
    pub arrow: Option<[Vec3; 2]>,
}

pub fn dimension_points(start: Vec3, end: Vec3, offset: Vec3) -> Option<[Vec3; 2]> {
    let direction = (end - start).try_normalized(1e-12)?;
    let displacement = offset - start;
    let perpendicular = displacement - direction * displacement.dot(direction);
    Some([start + perpendicular, end + perpendicular])
}
impl Document {
    pub fn add_annotation(
        &mut self,
        definition: DefinitionId,
        field: &str,
        record: Value,
    ) -> Result<String, String> {
        if !matches!(field, "texts" | "dimensions") {
            return Err("unsupported annotation type".into());
        }
        let id = format!("meshcraft:annotation:{}", self.allocate_id());
        let d = self
            .definitions
            .get_mut(&definition)
            .ok_or("annotation owner is missing")?;
        let key = format!("k3d.{field}");
        let mut records = d
            .extensions
            .get(&key)
            .or_else(|| d.domain_data.get(field))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        records.push(record);
        d.domain_entity_ids
            .entry(field.into())
            .or_default()
            .push(id.clone());
        d.extensions.insert(key, Value::Array(records.clone()));

        self.bump_revision();
        Ok(id)
    }
    pub fn update_annotation(
        &mut self,
        target: &SelectionTarget,
        record: Value,
    ) -> Result<(), String> {
        let SelectedEntity::SemanticHandle { kind, id } = &target.entity else {
            return Err("select an annotation".into());
        };
        let field = match kind {
            SelectionKind::Dimension => "dimensions",
            SelectionKind::Text => "texts",
            _ => return Err("select a text, leader or dimension".into()),
        };
        let definition = self
            .resolve_occurrence(&target.occurrence)
            .ok_or("annotation owner is missing")?
            .definition
            .id;
        let d = self
            .definitions
            .get_mut(&definition)
            .ok_or("annotation owner is missing")?;
        let index = d
            .domain_entity_ids
            .get(field)
            .and_then(|ids| ids.iter().position(|v| v == id))
            .ok_or("annotation is missing")?;
        let key = format!("k3d.{field}");
        let mut records = d
            .extensions
            .get(&key)
            .or_else(|| d.domain_data.get(field))
            .and_then(Value::as_array)
            .cloned()
            .ok_or("annotation data is missing")?;
        *records.get_mut(index).ok_or("annotation is missing")? = record;
        d.extensions.insert(key, Value::Array(records.clone()));

        self.bump_revision();
        Ok(())
    }
    pub fn annotations(&self) -> Vec<Annotation> {
        let mut result = Vec::new();
        for occurrence in self.scene_geometry_refs().unwrap_or_default() {
            let Some(r) = self.resolve_occurrence(&occurrence) else {
                continue;
            };
            for (field, kind) in [
                ("dimensions", SelectionKind::Dimension),
                ("texts", SelectionKind::Text),
            ] {
                let Some(records) = r
                    .definition
                    .extensions
                    .get(&format!("k3d.{field}"))
                    .or_else(|| r.definition.domain_data.get(field))
                    .and_then(Value::as_array)
                else {
                    continue;
                };
                for (index, record) in records.iter().enumerate() {
                    let Some(id) = r
                        .definition
                        .domain_entity_ids
                        .get(field)
                        .and_then(|ids| ids.get(index))
                    else {
                        continue;
                    };
                    let point = |key: &str| -> Option<Vec3> {
                        let p: Vec3 = serde_json::from_value(record.get(key)?.clone()).ok()?;
                        p.is_finite().then(|| r.world_transform.transform_point(p))
                    };
                    let mut edges = Vec::new();
                    let mut arrow = None;
                    let (position, label) = if field == "dimensions" {
                        let (Some(a), Some(b), Some(offset)) =
                            (point("start"), point("end"), point("offset"))
                        else {
                            continue;
                        };
                        let Some([c, d]) = dimension_points(a, b, offset) else {
                            continue;
                        };
                        edges.extend([[a, c], [b, d], [c, d]]);
                        arrow = Some([d, c]);
                        let label = record
                            .get("text")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .unwrap_or_else(|| {
                                format!(
                                    "{:.3} {}",
                                    self.model_settings.display_length(a.distance(b)),
                                    self.model_settings.unit_name
                                )
                            });
                        ((c + d) * 0.5, label)
                    } else {
                        let Some(position) = point("position") else {
                            continue;
                        };
                        if let Some(points) = record.get("leaderPoints").and_then(Value::as_array) {
                            let mut path = points
                                .iter()
                                .filter_map(|p| serde_json::from_value::<Vec3>(p.clone()).ok())
                                .filter(|p| p.is_finite())
                                .map(|p| r.world_transform.transform_point(p))
                                .collect::<Vec<_>>();
                            path.push(position);
                            edges.extend(path.windows(2).map(|p| [p[0], p[1]]));
                            arrow = edges.first().map(|[a, b]| [*b, *a]);
                        }
                        (
                            position,
                            record
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or("Text")
                                .to_owned(),
                        )
                    };
                    let size = record
                        .get("size")
                        .and_then(Value::as_f64)
                        .filter(|s| s.is_finite() && *s > 0.)
                        .unwrap_or(0.25)
                        * r.world_transform.transform_vector(Vec3::Y).length();
                    result.push(Annotation {
                        target: SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::SemanticHandle {
                                kind,
                                id: id.clone(),
                            },
                        },
                        record: record.clone(),
                        edges,
                        label,
                        position,
                        size,
                        arrow,
                    });
                }
            }
        }
        result
    }
}
