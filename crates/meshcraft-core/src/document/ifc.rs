//! Instance-owned IFC4X3_ADD2 property sets and prototype creation defaults.
use super::*;
use std::sync::LazyLock;
pub const KEY: &str = "k3d.ifcProperties";
pub const SCHEMA: &str = "IFC4X3_ADD2";
pub type Properties = BTreeMap<String, BTreeMap<String, Property>>;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Single,
    Enumerated,
    List,
    Bounded,
    Table,
    Reference,
    Complex,
    Quantity,
}
impl Kind {
    pub const ALL: [Self; 8] = [
        Self::Single,
        Self::Enumerated,
        Self::List,
        Self::Bounded,
        Self::Table,
        Self::Reference,
        Self::Complex,
        Self::Quantity,
    ];
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Property {
    pub kind: Kind,
    pub data_type: String,
    pub value: Value,
    pub unit: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_data_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enumeration: Option<Vec<Value>>,
}
impl Default for Property {
    fn default() -> Self {
        Self {
            kind: Kind::Single,
            data_type: "IfcLabel".into(),
            value: Value::Null,
            unit: None,
            description: String::new(),
            secondary_data_type: None,
            secondary_unit: None,
            expression: None,
            interpolation: None,
            usage_name: None,
            enumeration: None,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub set: String,
    pub name: String,
    pub kind: Kind,
    pub data_type: String,
    pub choices: Vec<String>,
    #[serde(default)]
    pub secondary_data_type: Option<String>,
}
#[derive(Deserialize, Serialize)]
pub struct Catalog {
    pub schema: String,
    pub version: String,
    pub types: Vec<String>,
    #[serde(rename = "typeKinds")]
    pub type_kinds: BTreeMap<String, String>,
    #[serde(rename = "valueTypes")]
    pub value_types: Vec<String>,
    pub properties: Vec<Template>,
}
pub fn catalog() -> &'static Catalog {
    static C: LazyLock<Catalog> = LazyLock::new(|| {
        serde_json::from_str(include_str!("../../../../vendor/ifc4.3.2.0/catalog.json"))
            .expect("pinned official IFC catalog")
    });
    &C
}
fn name(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.contains('\0')
}
fn scalar_value(data_type: &str, v: &Value) -> bool {
    if !data_type.starts_with("Q_") && !catalog().value_types.iter().any(|t| t == data_type) {
        return false;
    }
    match catalog().type_kinds.get(data_type).map(String::as_str) {
        Some("STRING" | "BINARY" | "ENUMERATION") => v.is_string(),
        Some("BOOLEAN") => v.is_boolean(),
        Some("LOGICAL") => v.is_boolean() || v.as_str() == Some("UNKNOWN"),
        Some("INTEGER") => v.as_i64().is_some(),
        Some("REAL" | "NUMBER") => v.as_f64().is_some_and(|n| {
            n.is_finite()
                && (!data_type.contains("Positive") || n > 0.)
                && (!data_type.contains("NonNegative") || n >= 0.)
                && (data_type != "IfcNormalisedRatioMeasure" || (0.0..=1.).contains(&n))
        }),
        Some("ARRAY" | "LIST" | "SET") => v
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(|v| v.is_number() || v.is_string())),
        Some("SELECT") => v.is_string() || v.is_number() || v.is_boolean(),
        _ if data_type.starts_with("Q_") => v.is_number(),
        _ => false,
    }
}
impl Property {
    pub fn validate(&self) -> Result<(), String> {
        self.check(0)
    }
    fn check(&self, depth: usize) -> Result<(), String> {
        if depth > 16
            || self.description.len() > 16384
            || self.unit.as_ref().is_some_and(|u| !name(u))
            || !name(&self.data_type)
        {
            return Err("Invalid IFC property metadata".into());
        }
        let scalar = |v: &Value| scalar_value(&self.data_type, v);
        if self.secondary_unit.as_ref().is_some_and(|v| !name(v))
            || self.secondary_data_type.as_ref().is_some_and(|v| !name(v))
            || self.expression.as_ref().is_some_and(|v| v.len() > 16384)
            || self.usage_name.as_ref().is_some_and(|v| v.len() > 1024)
            || self.interpolation.as_ref().is_some_and(|v| {
                !["LINEAR", "LOG_LINEAR", "LOG_LOG", "NOTDEFINED"].contains(&v.as_str())
            })
        {
            return Err("Invalid IFC property options".into());
        }
        if self
            .enumeration
            .as_ref()
            .is_some_and(|v| v.is_empty() || v.len() > 4096 || !v.iter().all(scalar))
        {
            return Err("Invalid IFC enumeration choices".into());
        }
        if self.value.is_null() {
            return if matches!(self.kind, Kind::Complex | Kind::Quantity) {
                Err("Complex properties and quantities require a value".into())
            } else {
                Ok(())
            };
        }
        let valid = match self.kind {
            Kind::Single => scalar(&self.value),
            Kind::Quantity => self.value.as_f64().is_some_and(f64::is_finite),
            Kind::Enumerated | Kind::List => self.value.as_array().is_some_and(|a| {
                a.len() <= 4096
                    && a.iter().all(scalar)
                    && self
                        .enumeration
                        .as_ref()
                        .is_none_or(|choices| a.iter().all(|v| choices.contains(v)))
            }),
            Kind::Bounded => self.value.as_object().is_some_and(|m| {
                !m.is_empty()
                    && m.iter().all(|(k, v)| {
                        matches!(k.as_str(), "lower" | "upper" | "setPoint")
                            && (v.is_null() || scalar(v))
                    })
                    && match (
                        m.get("lower").and_then(Value::as_f64),
                        m.get("upper").and_then(Value::as_f64),
                    ) {
                        (Some(a), Some(b)) => a <= b,
                        _ => true,
                    }
            }),
            Kind::Table => self.value.as_object().is_some_and(|m| {
                match (
                    m.get("defining").and_then(Value::as_array),
                    m.get("defined").and_then(Value::as_array),
                ) {
                    (Some(a), Some(b)) => {
                        a.len() == b.len()
                            && a.len() <= 4096
                            && a.iter().all(scalar)
                            && b.iter().all(|v| {
                                scalar_value(
                                    self.secondary_data_type
                                        .as_deref()
                                        .unwrap_or(&self.data_type),
                                    v,
                                )
                            })
                    }
                    _ => false,
                }
            }),
            Kind::Reference => {
                self.value.is_string()
                    || self.value.as_object().is_some_and(|m| {
                        m.len() <= 256 && serde_json::to_string(m).is_ok_and(|s| s.len() <= 65536)
                    })
            }
            Kind::Complex => serde_json::from_value::<BTreeMap<String, Property>>(
                self.value.clone(),
            )
            .is_ok_and(|m| {
                m.len() <= 4096 && m.iter().all(|(k, p)| name(k) && p.check(depth + 1).is_ok())
            }),
        };
        if valid {
            Ok(())
        } else {
            Err(format!(
                "Invalid {:?} value for {}",
                self.kind, self.data_type
            ))
        }
    }
}
pub fn validate(properties: &Properties) -> Result<(), String> {
    if properties.len() > 1024 || properties.values().map(BTreeMap::len).sum::<usize>() > 16384 {
        return Err("Too many IFC properties".into());
    }
    for (set, values) in properties {
        if !name(set) {
            return Err("IFC set name is required".into());
        }
        for (key, value) in values {
            if !name(key) {
                return Err("IFC property name is required".into());
            }
            value.validate()?;
        }
    }
    Ok(())
}
fn read(value: Option<&Value>) -> Properties {
    value
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}
impl Document {
    pub fn prototype_ifc_omissions(&self, id: DefinitionId) -> Vec<[String; 2]> {
        self.definition(id)
            .and_then(|d| d.extensions.get("k3d.ifcOmissions"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
    pub fn prototype_ifc(&self, id: DefinitionId) -> Properties {
        read(self.definition(id).and_then(|d| d.extensions.get(KEY)))
    }
    /// An absent legacy instance snapshot is empty; it never live-inherits prototype edits.
    pub fn instance_ifc(&self, path: &OccurrenceRef) -> Properties {
        read(
            self.resolve_occurrence(path)
                .and_then(|r| r.edits)
                .and_then(|e| e.values.get(KEY).or_else(|| e.extensions.get(KEY))),
        )
    }
    pub fn set_prototype_ifc(
        &mut self,
        id: DefinitionId,
        properties: Properties,
    ) -> Result<(), String> {
        validate(&properties)?;
        let mut omissions = self.prototype_ifc_omissions(id);
        for (set, group) in self.prototype_ifc(id) {
            for name in group.keys() {
                if !properties.get(&set).is_some_and(|p| p.contains_key(name)) {
                    let key = [set.clone(), name.clone()];
                    if !omissions.contains(&key) {
                        omissions.push(key);
                    }
                }
            }
        }
        omissions.retain(|[set, name]| !properties.get(set).is_some_and(|p| p.contains_key(name)));
        let value = serde_json::to_value(properties).unwrap();
        let d = self.definitions.get_mut(&id).ok_or("Prototype missing")?;
        if d.extensions.get(KEY) != Some(&value) {
            d.extensions.insert(KEY.into(), value);
            d.extensions.insert(
                "k3d.ifcOmissions".into(),
                serde_json::to_value(omissions).unwrap(),
            );
            self.bump_revision();
        }
        Ok(())
    }
    pub fn set_instance_ifc(
        &mut self,
        path: &OccurrenceRef,
        properties: Properties,
    ) -> Result<(), String> {
        validate(&properties)?;
        self.resolve_occurrence(path).ok_or("Instance missing")?;
        let mut edits = self.occurrence_edits(path).cloned().unwrap_or_default();
        let value = serde_json::to_value(properties).unwrap();
        if edits.values.get(KEY) != Some(&value) || edits.extensions.contains_key(KEY) {
            edits.values.insert(KEY.into(), value);
            edits.extensions.remove(KEY);
            if !self.set_occurrence_edits(path.clone(), edits) {
                return Err("Cannot store instance IFC properties".into());
            }
        }
        Ok(())
    }
    pub fn ifc_creation_edits(&self, id: DefinitionId) -> InstanceEdits {
        let mut edits = InstanceEdits::default();
        let properties = self.prototype_ifc(id);
        if !properties.is_empty() {
            edits
                .values
                .insert(KEY.into(), serde_json::to_value(properties).unwrap());
        }
        edits
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creation_copies_defaults_and_updates_remain_instance_local() {
        let mut d = Document::new();
        let original = d
            .add_cuboid("source", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let id = d.resolve_occurrence(&original).unwrap().definition.id;
        let props = BTreeMap::from([(
            "Pset_WallCommon".into(),
            BTreeMap::from([(
                "Reference".into(),
                Property {
                    value: Value::from("A"),
                    ..Default::default()
                },
            )]),
        )]);
        d.set_prototype_ifc(id, props.clone()).unwrap();
        let placed = crate::operations::objects::place_definition(
            &mut d,
            &crate::selection::EditContext::Document,
            id,
            Vec3::X,
            None,
            false,
        )
        .unwrap();
        assert_eq!(d.instance_ifc(&placed), props);
        assert!(d.instance_ifc(&original).is_empty());
        d.set_prototype_ifc(id, Properties::new()).unwrap();
        assert_eq!(d.instance_ifc(&placed), props);
        let copy = d.copy_root_occurrence(&placed, Vec3::Z).unwrap();
        assert_eq!(d.instance_ifc(&copy), props);
        d.set_instance_ifc(&copy, Properties::new()).unwrap();
        assert_eq!(d.instance_ifc(&placed), props);
        let saved = crate::legacy::export_current_value(&d).unwrap();
        let mut restored = crate::legacy::import_current_value(&saved).unwrap();
        assert!(
            restored
                .occurrence_refs()
                .unwrap()
                .iter()
                .any(|p| restored.instance_ifc(p) == props)
        );
        let path = restored
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .find(|p| restored.instance_ifc(p) == props)
            .unwrap();
        restored.set_instance_ifc(&path, Properties::new()).unwrap();
        let resaved = crate::legacy::export_current_value(&restored).unwrap();
        let reopened = crate::legacy::import_current_value(&resaved).unwrap();
        assert!(
            reopened
                .occurrence_refs()
                .unwrap()
                .iter()
                .all(|p| reopened.instance_ifc(p).is_empty()),
            "editing reopened metadata must replace the original legacy field"
        );
    }
    #[test]
    fn official_catalog_and_all_value_kinds_are_available() {
        assert_eq!(catalog().version, "4.3.2.0");
        assert_eq!(catalog().properties.len(), 3988);
        assert!(
            catalog()
                .properties
                .iter()
                .any(|p| p.set == "Pset_WallCommon" && p.name == "FireRating")
        );
        for (kind, value) in [
            (Kind::Single, serde_json::json!("A")),
            (Kind::List, serde_json::json!(["A", "B"])),
            (Kind::Enumerated, serde_json::json!(["A"])),
            (Kind::Bounded, serde_json::json!({"lower":1,"upper":2})),
            (
                Kind::Table,
                serde_json::json!({"defining":[1,2],"defined":[3,4]}),
            ),
            (
                Kind::Reference,
                serde_json::json!({"location":"https://example.com"}),
            ),
            (
                Kind::Complex,
                serde_json::json!({"Reference":{"value":"A"}}),
            ),
            (Kind::Quantity, serde_json::json!(4)),
        ] {
            Property {
                kind,
                data_type: if matches!(kind, Kind::Bounded | Kind::Quantity | Kind::Table) {
                    "IfcReal".into()
                } else {
                    "IfcLabel".into()
                },
                value,
                ..Default::default()
            }
            .validate()
            .unwrap();
        }
    }
}
