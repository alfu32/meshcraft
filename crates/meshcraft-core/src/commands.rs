//! Stable command metadata and deterministic command dispatch contracts.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const COMMAND_SCHEMA_VERSION: u16 = 1;

/// A stable, validated identifier such as `edit.undo` or `tool.rectangle`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommandId(String);

impl CommandId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.split('.').all(|part| {
                !part.is_empty()
                    && part.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || byte == b'_'
                            || byte == b'-'
                    })
                    && part.as_bytes()[0].is_ascii_alphanumeric()
                    && part.as_bytes()[part.len() - 1].is_ascii_alphanumeric()
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(format!("invalid command id `{value}`"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for CommandId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CommandId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    DocumentMutation,
    SessionMutation,
    ToolActivation,
    HostRequest,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCapability {
    Core,
    Desktop,
    Rendering,
    Import,
    Export,
    Plugin,
    Deferred,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CommandStateQuery {
    Always,
    UndoAvailable,
    RedoAvailable,
    SelectionAvailable,
    RootSelectionAvailable,
    SingleRootSelection,
    ActiveTool(String),
    ActiveProfile(String),
    PreferenceEnabled(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandParameter {
    pub name: String,
    pub value_type: CommandValueType,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandValueType {
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
    Any,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandPresentation {
    pub palette_path: String,
    pub menu_path: Vec<String>,
    pub toolbar_group: Option<String>,
    pub icon_key: Option<String>,
    pub tutorial_action_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandDescriptor {
    pub schema_version: u16,
    pub id: CommandId,
    pub label: String,
    pub category: String,
    pub keywords: Vec<String>,
    pub shortcut: Option<String>,
    pub capability: CommandCapability,
    pub kind: CommandKind,
    pub state_query: CommandStateQuery,
    pub parameters: Vec<CommandParameter>,
    pub presentation: CommandPresentation,
}

impl CommandDescriptor {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        category: impl Into<String>,
        keywords: impl IntoIterator<Item = impl Into<String>>,
        kind: CommandKind,
    ) -> Result<Self, String> {
        let label = label.into();
        let category = category.into();
        let palette_path = format!("{category}> {label}");
        Ok(Self {
            schema_version: COMMAND_SCHEMA_VERSION,
            id: CommandId::new(id)?,
            label,
            category,
            keywords: keywords.into_iter().map(Into::into).collect(),
            shortcut: None,
            capability: CommandCapability::Core,
            kind,
            state_query: CommandStateQuery::Always,
            parameters: Vec::new(),
            presentation: CommandPresentation {
                palette_path,
                ..CommandPresentation::default()
            },
        })
    }

    pub fn with_state_query(mut self, query: CommandStateQuery) -> Self {
        self.state_query = query;
        self
    }

    pub fn with_presentation(mut self, presentation: CommandPresentation) -> Self {
        self.presentation = presentation;
        self
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn with_parameters(mut self, parameters: Vec<CommandParameter>) -> Self {
        self.parameters = parameters;
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandPayload {
    pub schema_version: u16,
    pub values: BTreeMap<String, Value>,
}

impl Default for CommandPayload {
    fn default() -> Self {
        Self {
            schema_version: COMMAND_SCHEMA_VERSION,
            values: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandState {
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub checked: Option<bool>,
}

impl Default for CommandState {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_reason: None,
            checked: None,
        }
    }
}

impl CommandState {
    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            disabled_reason: Some(reason.into()),
            checked: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CommandCompletion {
    Completed,
    Pending { job_id: String },
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum HostEffect {
    OpenFileDialog {
        request_id: String,
        extensions: Vec<String>,
    },
    SaveFileDialog {
        request_id: String,
        extensions: Vec<String>,
    },
    ReadStorage {
        request_id: String,
        key: String,
    },
    WriteStorage {
        key: String,
        value: Value,
    },
    CaptureViewport {
        request_id: String,
    },
    StartJob {
        job_id: String,
        kind: String,
        input_revision: Option<u64>,
    },
    CancelJob {
        job_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandResult {
    pub completion: CommandCompletion,
    pub document_revision: Option<u64>,
    pub outputs: BTreeMap<String, Value>,
    pub diagnostics: Vec<String>,
    pub host_effects: Vec<HostEffect>,
}

impl CommandResult {
    pub fn completed() -> Self {
        Self {
            completion: CommandCompletion::Completed,
            document_revision: None,
            outputs: BTreeMap::new(),
            diagnostics: Vec::new(),
            host_effects: Vec::new(),
        }
    }

    pub fn pending(job_id: impl Into<String>, effect: HostEffect) -> Self {
        Self {
            completion: CommandCompletion::Pending {
                job_id: job_id.into(),
            },
            host_effects: vec![effect],
            ..Self::completed()
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            completion: CommandCompletion::Failed,
            diagnostics: vec![message.into()],
            ..Self::completed()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandDispatch {
    pub command_id: Option<CommandId>,
    pub accepted: bool,
    pub result: CommandResult,
}

pub struct CommandRegistration<H> {
    pub descriptor: CommandDescriptor,
    pub handler: H,
}

/// Registration order is stable and is retained for user-facing presentation order.
pub struct CommandRegistry<H> {
    entries: Vec<CommandRegistration<H>>,
    ids: BTreeSet<CommandId>,
}

impl<H> Default for CommandRegistry<H> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            ids: BTreeSet::new(),
        }
    }
}

impl<H> CommandRegistry<H> {
    pub fn register(&mut self, descriptor: CommandDescriptor, handler: H) -> Result<(), String> {
        if descriptor.schema_version != COMMAND_SCHEMA_VERSION {
            return Err(format!(
                "unsupported command schema version {} for {}",
                descriptor.schema_version, descriptor.id
            ));
        }
        let mut parameter_names = BTreeSet::new();
        for parameter in &descriptor.parameters {
            if parameter.name.is_empty() || !parameter_names.insert(parameter.name.as_str()) {
                return Err(format!(
                    "invalid or duplicate parameter on {}",
                    descriptor.id
                ));
            }
        }
        if !self.ids.insert(descriptor.id.clone()) {
            return Err(format!("duplicate command id {}", descriptor.id));
        }
        self.entries.push(CommandRegistration {
            descriptor,
            handler,
        });
        Ok(())
    }

    pub fn entries(&self) -> impl Iterator<Item = &CommandRegistration<H>> {
        self.entries.iter()
    }

    pub fn get(&self, id: &str) -> Option<&CommandRegistration<H>> {
        self.entries
            .iter()
            .find(|entry| entry.descriptor.id.as_str() == id)
    }

    pub fn dispatch(
        &self,
        id: &str,
        payload: &CommandPayload,
        state: &CommandState,
        execute: impl FnOnce(&H, &CommandPayload) -> CommandResult,
    ) -> CommandDispatch {
        let Some(entry) = self.get(id) else {
            return rejected(None, format!("unknown command `{id}`"));
        };
        if !state.enabled {
            let reason = state
                .disabled_reason
                .as_deref()
                .unwrap_or("command is disabled in the current context");
            return rejected(Some(entry.descriptor.id.clone()), reason.to_owned());
        }
        if let Err(error) = validate_payload(&entry.descriptor, payload) {
            return rejected(Some(entry.descriptor.id.clone()), error);
        }
        CommandDispatch {
            command_id: Some(entry.descriptor.id.clone()),
            accepted: true,
            result: execute(&entry.handler, payload),
        }
    }
}

fn rejected(command_id: Option<CommandId>, message: String) -> CommandDispatch {
    CommandDispatch {
        command_id,
        accepted: false,
        result: CommandResult::failed(message),
    }
}

fn validate_payload(
    descriptor: &CommandDescriptor,
    payload: &CommandPayload,
) -> Result<(), String> {
    if payload.schema_version != descriptor.schema_version {
        return Err(format!(
            "payload schema version {} does not match command schema version {}",
            payload.schema_version, descriptor.schema_version
        ));
    }
    let parameters = descriptor
        .parameters
        .iter()
        .map(|parameter| (parameter.name.as_str(), parameter))
        .collect::<BTreeMap<_, _>>();
    for (name, value) in &payload.values {
        let Some(parameter) = parameters.get(name.as_str()) else {
            return Err(format!(
                "unexpected parameter `{name}` for {}",
                descriptor.id
            ));
        };
        if !value_matches_type(value, parameter.value_type) {
            return Err(format!("parameter `{name}` has the wrong value type"));
        }
    }
    for parameter in &descriptor.parameters {
        if parameter.required && !payload.values.contains_key(&parameter.name) {
            return Err(format!("missing required parameter `{}`", parameter.name));
        }
    }
    Ok(())
}

fn value_matches_type(value: &Value, value_type: CommandValueType) -> bool {
    match value_type {
        CommandValueType::Boolean => value.is_boolean(),
        CommandValueType::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
        CommandValueType::Number => value.is_number(),
        CommandValueType::String => value.is_string(),
        CommandValueType::Array => value.is_array(),
        CommandValueType::Object => value.is_object(),
        CommandValueType::Any => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandCapability, CommandCompletion, CommandDescriptor, CommandKind, CommandParameter,
        CommandPayload, CommandPresentation, CommandRegistry, CommandState, CommandValueType,
        HostEffect,
    };
    use serde_json::json;

    fn descriptor(id: &str, kind: CommandKind) -> CommandDescriptor {
        CommandDescriptor::new(id, "Save", "File", ["write"], kind).unwrap()
    }

    #[test]
    fn command_ids_are_validated_and_schema_is_serializable() {
        assert!(super::CommandId::new("edit.undo").is_ok());
        assert!(super::CommandId::new("Edit.Undo").is_err());
        assert!(super::CommandId::new("edit..undo").is_err());

        let encoded =
            serde_json::to_value(descriptor("file.save", CommandKind::HostRequest)).unwrap();
        assert_eq!(encoded["schema_version"], json!(1));
        assert_eq!(encoded["capability"], json!("core"));
    }

    #[test]
    fn registry_rejects_duplicates_and_keeps_stable_registration_order() {
        let mut registry = CommandRegistry::default();
        registry
            .register(descriptor("edit.undo", CommandKind::DocumentMutation), 1)
            .unwrap();
        assert!(
            registry
                .register(descriptor("edit.undo", CommandKind::DocumentMutation), 2)
                .is_err()
        );
        registry
            .register(descriptor("edit.redo", CommandKind::DocumentMutation), 3)
            .unwrap();
        let ids = registry
            .entries()
            .map(|entry| entry.descriptor.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["edit.undo", "edit.redo"]);
    }

    #[test]
    fn dispatch_checks_state_and_typed_payload_before_handler() {
        let mut command = descriptor("export.write", CommandKind::HostRequest);
        command.parameters = vec![CommandParameter {
            name: "path".to_owned(),
            value_type: CommandValueType::String,
            required: true,
        }];
        let mut registry = CommandRegistry::default();
        registry.register(command, "write").unwrap();
        let called = std::cell::Cell::new(false);
        let disabled = registry.dispatch(
            "export.write",
            &CommandPayload::default(),
            &CommandState::disabled("choose an export path"),
            |_, _| {
                called.set(true);
                super::CommandResult::completed()
            },
        );
        assert!(!disabled.accepted);
        assert!(!called.get());

        let missing = registry.dispatch(
            "export.write",
            &CommandPayload::default(),
            &CommandState::default(),
            |_, _| {
                called.set(true);
                super::CommandResult::completed()
            },
        );
        assert!(!missing.accepted);
        assert!(!called.get());

        let mut payload = CommandPayload::default();
        payload.values.insert("path".to_owned(), json!("scene.obj"));
        let accepted = registry.dispatch(
            "export.write",
            &payload,
            &CommandState::default(),
            |handler, payload| {
                assert_eq!(*handler, "write");
                assert_eq!(payload.values["path"], json!("scene.obj"));
                super::CommandResult::pending(
                    "job-1",
                    HostEffect::StartJob {
                        job_id: "job-1".to_owned(),
                        kind: "export".to_owned(),
                        input_revision: Some(3),
                    },
                )
            },
        );
        assert!(accepted.accepted);
        assert_eq!(
            accepted.result.completion,
            CommandCompletion::Pending {
                job_id: "job-1".to_owned()
            }
        );
    }

    #[test]
    fn tool_activation_is_not_a_document_mutation() {
        let mut registry = CommandRegistry::default();
        registry
            .register(
                descriptor("tool.select", CommandKind::ToolActivation),
                "select",
            )
            .unwrap();
        let dispatch = registry.dispatch(
            "tool.select",
            &CommandPayload::default(),
            &CommandState::default(),
            |_, _| super::CommandResult::completed(),
        );
        assert!(dispatch.accepted);
        assert_eq!(
            registry.get("tool.select").unwrap().descriptor.kind,
            CommandKind::ToolActivation
        );
        assert_eq!(dispatch.result.document_revision, None);
    }

    #[test]
    fn descriptors_include_capabilities_and_action_tree_metadata() {
        let mut command = descriptor("tool.rectangle", CommandKind::ToolActivation);
        command.capability = CommandCapability::Desktop;
        command.presentation = CommandPresentation {
            palette_path: "Tool> Rectangle".to_owned(),
            menu_path: vec!["Tools".to_owned(), "Draw".to_owned()],
            toolbar_group: Some("Point Construction".to_owned()),
            icon_key: Some("rectangle".to_owned()),
            tutorial_action_id: Some("basic.rectangle".to_owned()),
        };
        let serialized = serde_json::to_value(command).unwrap();
        assert_eq!(serialized["capability"], "desktop");
        assert_eq!(serialized["presentation"]["menu_path"][0], "Tools");
    }
}
