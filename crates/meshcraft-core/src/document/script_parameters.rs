//! Serializable model declarations and authored values, independent of any UI toolkit.
use super::*;

pub const VALUES_KEY: &str = "k3d.scriptParameterValues";
pub const DECLARATIONS_KEY: &str = "k3d.scriptParameterDeclarations";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Prototype,
    Instance,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Number,
    Checkbox,
    List,
    Hotspot,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Declaration {
    pub id: String,
    pub label: String,
    pub scope: Scope,
    pub kind: Kind,
    pub default_value: Value,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}
impl Declaration {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 128
            || matches!(self.id.as_str(), "__proto__" | "prototype" | "constructor")
            || self.label.len() > 4096
            || self.options.len() > 256
            || self
                .options
                .iter()
                .any(|(k, v)| k.is_empty() || k.len() > 128 || v.len() > 4096)
        {
            return Err("Invalid model parameter declaration".into());
        }
        self.validate_value(&self.default_value)
    }

    pub fn same_value(&self, a: &Value, b: &Value) -> bool {
        if self.kind == Kind::Number {
            a.as_f64() == b.as_f64()
        } else {
            a == b
        }
    }
    pub fn validate_value(&self, value: &Value) -> Result<(), String> {
        let valid = match self.kind {
            Kind::Text => value.as_str().is_some_and(|s| s.len() <= 16384),
            Kind::Number => value.as_f64().is_some_and(f64::is_finite),
            Kind::Checkbox => value.is_boolean(),
            Kind::List => value.as_str().is_some_and(|s| self.options.contains_key(s)),
            Kind::Hotspot => {
                serde_json::from_value::<Vec3>(value.clone()).is_ok_and(|p| p.is_finite())
            }
        };
        if valid {
            Ok(())
        } else {
            Err(format!("Invalid value for parameter {}", self.id))
        }
    }
}
impl Document {
    pub fn set_script_parameter_declarations(
        &mut self,
        definition: DefinitionId,
        declarations: Vec<Declaration>,
    ) -> Result<(), String> {
        if declarations.len() > 256 {
            return Err("Too many model parameters".into());
        }
        let mut ids = BTreeSet::new();
        for d in &declarations {
            d.validate()?;
            if !ids.insert(&d.id) {
                return Err("Duplicate model parameter ID".into());
            }
        }
        let value = serde_json::to_value(declarations).map_err(|e| e.to_string())?;
        let target = self
            .definitions
            .get_mut(&definition)
            .ok_or("Prototype missing")?;
        if target.extensions.get(DECLARATIONS_KEY) != Some(&value) {
            target.extensions.insert(DECLARATIONS_KEY.into(), value);
            self.bump_revision();
        }
        Ok(())
    }

    pub fn script_parameter_declarations(
        &self,
        definition: DefinitionId,
        path: Option<&OccurrenceRef>,
    ) -> Vec<Declaration> {
        let state = path
            .and_then(|p| {
                self.script_frame(p).map(|f| f.state.clone()).or_else(|| {
                    self.occurrence_edits(p)
                        .and_then(|e| e.values.get(STATE_KEY))
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                })
            })
            .or_else(|| {
                self.script_cache
                    .frames
                    .iter()
                    .find(|(p, _)| {
                        self.resolve_occurrence(p)
                            .is_some_and(|r| r.definition.id == definition)
                    })
                    .map(|(_, f)| f.state.clone())
            });
        state.map(|s| s.declarations).unwrap_or_else(|| {
            self.definition(definition)
                .and_then(|d| d.extensions.get(DECLARATIONS_KEY))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default()
        })
    }
    pub fn script_parameter_values(
        &self,
        definition: DefinitionId,
        path: Option<&OccurrenceRef>,
    ) -> BTreeMap<String, Value> {
        let raw = match path {
            Some(p) => self
                .occurrence_edits(p)
                .and_then(|e| e.values.get(VALUES_KEY)),
            None => self
                .definition(definition)
                .and_then(|d| d.extensions.get(VALUES_KEY)),
        };
        raw.and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
    pub fn initialized_script_parameter(&self, path: &OccurrenceRef, id: &str) -> Option<Value> {
        self.script_frame(path)
            .and_then(|f| f.state.parameters.get(id))
            .cloned()
            .or_else(|| {
                self.occurrence_edits(path)
                    .and_then(|e| e.values.get(STATE_KEY))
                    .and_then(|v| v.get("parameters"))
                    .and_then(|v| v.get(id))
                    .cloned()
            })
    }
    pub fn set_script_parameter(
        &mut self,
        definition: DefinitionId,
        path: Option<&OccurrenceRef>,
        id: &str,
        value: Value,
    ) -> Result<bool, String> {
        if path.is_some_and(|p| {
            self.resolve_occurrence(p)
                .is_none_or(|r| r.definition.id != definition)
        }) {
            return Err("Parameter instance no longer exists".into());
        }
        let scope = if path.is_some() {
            Scope::Instance
        } else {
            Scope::Prototype
        };
        let declaration = self
            .script_parameter_declarations(definition, path)
            .into_iter()
            .find(|d| d.id == id && d.scope == scope)
            .ok_or("Parameter declaration no longer exists")?;
        if declaration.kind == Kind::Hotspot {
            return Err("Use the hotspot handle to change its position".into());
        }
        declaration.validate_value(&value)?;
        let mut values = self.script_parameter_values(definition, path);
        let initialized = path
            .and_then(|p| self.initialized_script_parameter(p, id))
            .filter(|v| declaration.validate_value(v).is_ok());
        if declaration.same_value(
            values
                .get(id)
                .filter(|v| declaration.validate_value(v).is_ok())
                .or(initialized.as_ref())
                .unwrap_or(&declaration.default_value),
            &value,
        ) {
            return Ok(false);
        }
        values.insert(id.into(), value);
        let values = serde_json::to_value(values).map_err(|e| e.to_string())?;
        if let Some(path) = path {
            let mut edits = self.occurrence_edits(path).cloned().unwrap_or_default();
            edits.values.insert(VALUES_KEY.into(), values);
            self.occurrence_edits.insert(path.clone(), edits);
        } else {
            self.definitions
                .get_mut(&definition)
                .ok_or("Prototype no longer exists")?
                .extensions
                .insert(VALUES_KEY.into(), values);
        }
        self.bump_revision();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn declaration(id: &str, scope: Scope, kind: Kind, value: Value) -> Declaration {
        Declaration {
            id: id.into(),
            label: id.into(),
            scope,
            kind,
            default_value: value,
            options: BTreeMap::new(),
        }
    }
    #[test]
    fn prototype_and_instance_values_copy_and_roundtrip_without_changing_defaults() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("prototype", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let id = document.resolve_occurrence(&first).unwrap().definition.id;
        document
            .set_script_parameter_declarations(
                id,
                vec![
                    declaration("depth", Scope::Prototype, Kind::Number, Value::from(1)),
                    declaration("width", Scope::Instance, Kind::Number, Value::from(3)),
                ],
            )
            .unwrap();
        document
            .set_script_parameter(id, None, "depth", Value::from(2))
            .unwrap();
        document
            .set_script_parameter(id, Some(&first), "width", Value::from(5))
            .unwrap();
        let second = document.copy_root_occurrence(&first, Vec3::X * 4.).unwrap();
        assert_eq!(
            document.script_parameter_values(id, Some(&second))["width"],
            5
        );
        document
            .set_script_parameter(id, Some(&second), "width", Value::from(7))
            .unwrap();
        let restored: Document =
            serde_json::from_value(serde_json::to_value(&document).unwrap()).unwrap();
        assert_eq!(restored.script_parameter_values(id, None)["depth"], 2);
        assert_eq!(
            restored.script_parameter_values(id, Some(&first))["width"],
            5
        );
        assert_eq!(
            restored.script_parameter_values(id, Some(&second))["width"],
            7
        );
        assert_eq!(
            restored.script_parameter_declarations(id, None)[1].default_value,
            3
        );
        let revision = document.revision();
        assert!(
            !document
                .set_script_parameter(id, Some(&second), "width", Value::from(7))
                .unwrap()
        );
        assert_eq!(revision, document.revision());
        assert!(
            document
                .set_script_parameter(id, None, "width", Value::from(1))
                .is_err()
        );
        assert!(
            document
                .set_script_parameter(id, Some(&first), "width", Value::from("bad"))
                .is_err()
        );
        assert_eq!(revision, document.revision());
    }
}
