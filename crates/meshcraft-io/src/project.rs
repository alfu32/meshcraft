//! Read and write the legacy project byte container and its encoded history snapshots.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use meshcraft_core::document::Document;
use meshcraft_core::legacy::{export_current_value, import_current_value};
use meshcraft_core::transactions::{
    HistorySnapshot, LegacyHistoryDecoder, OpaqueLegacyHistory, OpaqueLegacyUndoEntry,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
const COMPRESS_THRESHOLD_BYTES: usize = 10 * 1024 * 1024;
const DEFAULT_MAX_DECODED_BYTES: usize = 512 * 1024 * 1024;
const DECODE_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodeProgress {
    pub decoded_bytes: usize,
    pub encoded_bytes: usize,
    pub compressed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectError {
    Cancelled,
    EmptyInput,
    PayloadTooLarge { limit_bytes: usize },
    InvalidGzip(String),
    InvalidUtf8(String),
    InvalidJson(String),
    InvalidHistory(String),
    InvalidModel(String),
    InvalidSnapshotEncoding(String),
    InvalidCompression(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("project decoding was cancelled"),
            Self::EmptyInput => formatter.write_str("project file is empty"),
            Self::PayloadTooLarge { limit_bytes } => {
                write!(
                    formatter,
                    "decoded project exceeds the {limit_bytes} byte limit"
                )
            }
            Self::InvalidGzip(error) => write!(formatter, "invalid gzip project payload: {error}"),
            Self::InvalidUtf8(error) => write!(formatter, "project payload is not UTF-8: {error}"),
            Self::InvalidJson(error) => write!(formatter, "invalid project JSON: {error}"),
            Self::InvalidHistory(error) => {
                write!(formatter, "invalid legacy undo history: {error}")
            }
            Self::InvalidModel(error) => {
                write!(formatter, "unsupported or invalid project model: {error}")
            }
            Self::InvalidSnapshotEncoding(error) => {
                write!(formatter, "invalid encoded history snapshot: {error}")
            }
            Self::InvalidCompression(error) => {
                write!(formatter, "could not compress project JSON: {error}")
            }
        }
    }
}

impl std::error::Error for ProjectError {}

#[derive(Clone, Debug)]
pub struct ImportedProject {
    pub document: Document,
    pub history: Option<OpaqueLegacyHistory>,
    pub warnings: Vec<String>,
    pub source_schema_version: u32,
    pub source_schema_version_present: bool,
}

/// Decodes JSON or gzip by magic bytes, normalizes supported legacy shapes, then validates the model.
pub fn read_project(bytes: &[u8]) -> Result<ImportedProject, ProjectError> {
    read_project_with(bytes, DEFAULT_MAX_DECODED_BYTES, |_| {}, || false)
}

/// Decodes in bounded chunks. Cancellation is checked before and during decompression; progress
/// reports decoded output and the original input size without assuming a compression ratio.
pub fn read_project_with(
    bytes: &[u8],
    max_decoded_bytes: usize,
    mut progress: impl FnMut(DecodeProgress),
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<ImportedProject, ProjectError> {
    if bytes.is_empty() {
        return Err(ProjectError::EmptyInput);
    }
    let payload = decode_payload_with(bytes, max_decoded_bytes, &mut progress, &mut is_cancelled)?;
    let text = std::str::from_utf8(&payload)
        .map_err(|error| ProjectError::InvalidUtf8(error.to_string()))?;
    let root: Value =
        serde_json::from_str(text).map_err(|error| ProjectError::InvalidJson(error.to_string()))?;
    drop(payload);
    let root_object = root
        .as_object()
        .ok_or_else(|| ProjectError::InvalidJson("snapshot root must be an object".to_owned()))?;
    let history = parse_legacy_history(root_object.get("undoHistory"))?;
    let (source_schema_version, source_schema_version_present) = match root_object.get("version") {
        None => (17, false),
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|version| u32::try_from(version).ok())
            .map(|version| (version, true))
            .ok_or_else(|| {
                ProjectError::InvalidModel(
                    "version must be a nonnegative 32-bit integer".to_owned(),
                )
            })?,
        Some(_) => {
            return Err(ProjectError::InvalidModel(
                "version must be a nonnegative 32-bit integer".to_owned(),
            ));
        }
    };
    let mut normalized =
        crate::legacy::normalize_scene_shape(root).map_err(ProjectError::InvalidModel)?;
    if let Some(root) = normalized.snapshot.as_object_mut() {
        root.remove("undoHistory");
    }
    let document =
        import_current_value(&normalized.snapshot).map_err(ProjectError::InvalidModel)?;
    let warnings = normalized.warnings;
    drop(normalized.snapshot);
    document.validate().map_err(ProjectError::InvalidModel)?;
    Ok(ImportedProject {
        document,
        history,
        warnings,
        source_schema_version,
        source_schema_version_present,
    })
}

/// Returns the UTF-8 JSON payload after applying the project container's magic-byte rule.
pub fn decode_project_payload(
    bytes: &[u8],
    max_decoded_bytes: usize,
) -> Result<Vec<u8>, ProjectError> {
    decode_payload_with(bytes, max_decoded_bytes, &mut |_| {}, &mut || false)
}

fn decode_payload_with(
    bytes: &[u8],
    max_decoded_bytes: usize,
    progress: &mut impl FnMut(DecodeProgress),
    is_cancelled: &mut impl FnMut() -> bool,
) -> Result<Vec<u8>, ProjectError> {
    let compressed = bytes.starts_with(&GZIP_MAGIC);
    if !compressed && bytes.len() > max_decoded_bytes {
        return Err(ProjectError::PayloadTooLarge {
            limit_bytes: max_decoded_bytes,
        });
    }
    let mut decoded = Vec::with_capacity(bytes.len().min(max_decoded_bytes));
    if compressed {
        let mut reader = MultiGzDecoder::new(bytes);
        let mut buffer = [0; DECODE_CHUNK_BYTES];
        loop {
            if is_cancelled() {
                return Err(ProjectError::Cancelled);
            }
            let read = reader
                .read(&mut buffer)
                .map_err(|error| ProjectError::InvalidGzip(error.to_string()))?;
            if read == 0 {
                break;
            }
            if read > max_decoded_bytes.saturating_sub(decoded.len()) {
                return Err(ProjectError::PayloadTooLarge {
                    limit_bytes: max_decoded_bytes,
                });
            }
            decoded.extend_from_slice(&buffer[..read]);
            progress(DecodeProgress {
                decoded_bytes: decoded.len(),
                encoded_bytes: bytes.len(),
                compressed: true,
            });
        }
    } else {
        for chunk in bytes.chunks(DECODE_CHUNK_BYTES) {
            if is_cancelled() {
                return Err(ProjectError::Cancelled);
            }
            decoded.extend_from_slice(chunk);
            progress(DecodeProgress {
                decoded_bytes: decoded.len(),
                encoded_bytes: bytes.len(),
                compressed: false,
            });
        }
    }
    Ok(decoded)
}

/// Matches the legacy writer: plain UTF-8 JSON through 10 MiB, gzip above that threshold.
pub fn encode_project_json(json: &str) -> Result<Vec<u8>, ProjectError> {
    serde_json::from_str::<Value>(json)
        .map_err(|error| ProjectError::InvalidJson(error.to_string()))?;
    encode_valid_project_json(json)
}

fn encode_valid_project_json(json: &str) -> Result<Vec<u8>, ProjectError> {
    let bytes = json.as_bytes();
    if bytes.len() <= COMPRESS_THRESHOLD_BYTES {
        return Ok(bytes.to_vec());
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .map_err(|error| ProjectError::InvalidCompression(error.to_string()))?;
    encoder
        .finish()
        .map_err(|error| ProjectError::InvalidCompression(error.to_string()))
}

/// Serializes a normalized document as schema-17 JSON and applies the legacy project container
/// compression threshold. Legacy history is stored separately from the normalized document and is
/// included only when a live opaque history value is supplied.
pub fn write_project(
    document: &Document,
    history: Option<&OpaqueLegacyHistory>,
) -> Result<Vec<u8>, ProjectError> {
    let mut snapshot = export_current_value(document).map_err(ProjectError::InvalidModel)?;
    let root = snapshot.as_object_mut().ok_or_else(|| {
        ProjectError::InvalidModel("exported snapshot root is not an object".to_owned())
    })?;
    match history {
        Some(history) => {
            root.insert("undoHistory".to_owned(), legacy_history_value(history));
        }
        None => {
            // Without a live history object, an imported raw payload could point to stale states.
            root.remove("undoHistory");
        }
    }
    let json = serde_json::to_string(&snapshot)
        .map_err(|error| ProjectError::InvalidJson(error.to_string()))?;
    if json.len() > DEFAULT_MAX_DECODED_BYTES {
        return Err(ProjectError::PayloadTooLarge {
            limit_bytes: DEFAULT_MAX_DECODED_BYTES,
        });
    }
    encode_valid_project_json(&json)
}

/// Encodes the current transaction timeline in the schema-17 undo-history format.
///
/// Camera state is session state rather than authored document history, so the active camera pose
/// is merged into every snapshot before it is exported.
pub fn encode_native_history(
    timeline: &[HistorySnapshot],
    cursor: usize,
    camera_state: &Value,
) -> Result<OpaqueLegacyHistory, ProjectError> {
    if timeline.is_empty() || cursor >= timeline.len() {
        return Err(ProjectError::InvalidHistory(
            "native history must have snapshots and a cursor within the timeline".to_owned(),
        ));
    }
    let max_entries = i32::try_from(timeline.len()).map_err(|_| {
        ProjectError::InvalidHistory("native history contains too many snapshots".to_owned())
    })?;

    let mut entries = Vec::with_capacity(timeline.len());
    for snapshot in timeline {
        let document = meshcraft_core::legacy::snapshot_with_legacy_camera_state(
            &snapshot.document,
            camera_state.clone(),
        )
        .map_err(ProjectError::InvalidModel)?;
        let value = export_current_value(&document).map_err(ProjectError::InvalidModel)?;
        let json = serde_json::to_vec(&value)
            .map_err(|error| ProjectError::InvalidJson(error.to_string()))?;
        if json.len() > DEFAULT_MAX_DECODED_BYTES {
            return Err(ProjectError::PayloadTooLarge {
                limit_bytes: DEFAULT_MAX_DECODED_BYTES,
            });
        }
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&json)
            .map_err(|error| ProjectError::InvalidCompression(error.to_string()))?;
        let compressed = encoder
            .finish()
            .map_err(|error| ProjectError::InvalidCompression(error.to_string()))?;
        entries.push(OpaqueLegacyUndoEntry {
            data: BASE64_STANDARD.encode(compressed),
            label: snapshot.label.clone(),
            timestamp_millis: snapshot.timestamp_millis,
            diagnostic: None,
            extensions: BTreeMap::new(),
        });
    }

    Ok(OpaqueLegacyHistory {
        max_entries,
        index: i32::try_from(cursor).map_err(|_| {
            ProjectError::InvalidHistory("native history cursor is out of range".to_owned())
        })?,
        entries,
        extensions: BTreeMap::new(),
    })
}

fn legacy_history_value(history: &OpaqueLegacyHistory) -> Value {
    let mut root = history
        .extensions
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<_, _>>();
    root.insert("maxEntries".to_owned(), Value::from(history.max_entries));
    root.insert("index".to_owned(), Value::from(history.index));
    root.insert(
        "entries".to_owned(),
        Value::Array(
            history
                .entries
                .iter()
                .map(|entry| {
                    let mut object = entry
                        .extensions
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect::<Map<_, _>>();
                    object.insert("data".to_owned(), Value::String(entry.data.clone()));
                    if let Some(label) = &entry.label {
                        object.insert("label".to_owned(), Value::String(label.clone()));
                    }
                    object.insert("timestamp".to_owned(), Value::from(entry.timestamp_millis));
                    Value::Object(object)
                })
                .collect(),
        ),
    );
    Value::Object(root)
}

/// Lazily adapts a T04 history entry to the current-format document importer.
#[derive(Clone, Copy, Debug, Default)]
pub struct LegacySnapshotAdapter {
    pub max_decoded_bytes: usize,
}

impl LegacySnapshotAdapter {
    pub fn with_limit(max_decoded_bytes: usize) -> Self {
        Self { max_decoded_bytes }
    }
}

impl LegacyHistoryDecoder for LegacySnapshotAdapter {
    fn decode_snapshot(&self, encoded_base64_gzip: &str) -> Result<Document, String> {
        let bytes = BASE64_STANDARD
            .decode(encoded_base64_gzip)
            .map_err(|error| {
                ProjectError::InvalidSnapshotEncoding(error.to_string()).to_string()
            })?;
        let limit = if self.max_decoded_bytes == 0 {
            DEFAULT_MAX_DECODED_BYTES
        } else {
            self.max_decoded_bytes
        };
        read_project_with(&bytes, limit, |_| {}, || false)
            .map(|project| project.document)
            .map_err(|error| error.to_string())
    }
}

fn parse_legacy_history(
    value: Option<&Value>,
) -> Result<Option<OpaqueLegacyHistory>, ProjectError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let object = value.as_object().ok_or_else(|| {
        ProjectError::InvalidHistory("undoHistory must be an object or null".to_owned())
    })?;
    let max_entries = read_i32(object, "maxEntries", 0, "/undoHistory")?;
    let index = read_i32(object, "index", -1, "/undoHistory")?;
    let empty_entries = Value::Array(Vec::new());
    let raw_entries = match object.get("entries") {
        None => &empty_entries,
        Some(value) if value.is_array() => value,
        Some(_) => {
            return Err(ProjectError::InvalidHistory(
                "/undoHistory/entries must be an array".to_owned(),
            ));
        }
    };
    let mut entries = Vec::new();
    for (entry_index, value) in raw_entries
        .as_array()
        .expect("array checked above")
        .iter()
        .enumerate()
    {
        let path = format!("/undoHistory/entries/{entry_index}");
        let object = value
            .as_object()
            .ok_or_else(|| ProjectError::InvalidHistory(format!("{path} must be an object")))?;
        let data = read_string(object, "data", &path)?.unwrap_or_default();
        let label = read_optional_string(object, "label", &path)?;
        let timestamp_millis = read_i64(object, "timestamp", 0, &path)?;
        let extensions = unknown_fields(object, &["data", "label", "timestamp"]);
        entries.push(OpaqueLegacyUndoEntry {
            data,
            label,
            timestamp_millis,
            diagnostic: None,
            extensions,
        });
    }
    Ok(Some(OpaqueLegacyHistory {
        max_entries,
        index,
        entries,
        extensions: unknown_fields(object, &["maxEntries", "index", "entries"]),
    }))
}

fn read_i32(
    object: &Map<String, Value>,
    key: &str,
    default: i32,
    path: &str,
) -> Result<i32, ProjectError> {
    match object.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_i64()
            .and_then(|number| i32::try_from(number).ok())
            .ok_or_else(|| {
                ProjectError::InvalidHistory(format!("{path}/{key} must be a 32-bit integer"))
            }),
    }
}

fn read_i64(
    object: &Map<String, Value>,
    key: &str,
    default: i64,
    path: &str,
) -> Result<i64, ProjectError> {
    match object.get(key) {
        None => Ok(default),
        Some(value) => value.as_i64().ok_or_else(|| {
            ProjectError::InvalidHistory(format!("{path}/{key} must be an integer"))
        }),
    }
}

fn read_string(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<String>, ProjectError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(ProjectError::InvalidHistory(format!(
            "{path}/{key} must be a string"
        ))),
    }
}

fn read_optional_string(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<String>, ProjectError> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(ProjectError::InvalidHistory(format!(
            "{path}/{key} must be a string or null"
        ))),
    }
}

fn unknown_fields(object: &Map<String, Value>, known: &[&str]) -> BTreeMap<String, Value> {
    object
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        LegacySnapshotAdapter, ProjectError, decode_project_payload, encode_native_history,
        encode_project_json, export_current_value, read_project, read_project_with, write_project,
    };
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use meshcraft_core::document::Document;
    use meshcraft_core::geometry::Vec3;
    use meshcraft_core::transactions::{
        HistoryLimits, LegacyHistoryDecoder, OpaqueLegacyHistory, OpaqueLegacyUndoEntry,
        SystemHistoryClock, TransactionHistory,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::io::Write;

    const CURRENT_MODEL: &str =
        include_str!("../../../fixtures/legacy/samples/examples/arch_frame.octd");
    const ROOT_GROUP_MODEL: &str =
        include_str!("../../../fixtures/legacy/historical/root-group-test-2.k3d");

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn reads_json_and_magic_detected_gzip_and_keeps_missing_version_distinct() {
        let plain = read_project(CURRENT_MODEL.as_bytes()).unwrap();
        let compressed = read_project(&gzip(CURRENT_MODEL.as_bytes())).unwrap();
        assert_eq!(plain.document, compressed.document);
        assert!(!plain.source_schema_version_present);
        assert_eq!(17, plain.source_schema_version);
        assert_eq!(2, plain.document.occurrence_refs().unwrap().len());

        let mut malformed: Value = serde_json::from_str(CURRENT_MODEL).unwrap();
        malformed["version"] = Value::String("17".to_owned());
        assert!(matches!(
            read_project(malformed.to_string().as_bytes()),
            Err(ProjectError::InvalidModel(_))
        ));
    }

    #[test]
    fn writes_schema17_projects_and_retains_opaque_legacy_history() {
        let imported = read_project(CURRENT_MODEL.as_bytes()).unwrap();
        let history = OpaqueLegacyHistory {
            max_entries: 7,
            index: 1,
            entries: vec![
                OpaqueLegacyUndoEntry {
                    data: "opaque-invalid-entry".to_owned(),
                    label: Some("Before edit".to_owned()),
                    timestamp_millis: 1234,
                    diagnostic: Some("must not be discarded".to_owned()),
                    extensions: BTreeMap::from([("futureEntry".to_owned(), json!(true))]),
                },
                OpaqueLegacyUndoEntry {
                    data: "encoded-current-snapshot".to_owned(),
                    label: None,
                    timestamp_millis: 2345,
                    diagnostic: None,
                    extensions: BTreeMap::new(),
                },
            ],
            extensions: BTreeMap::from([("futureHistory".to_owned(), json!([1, 2, 3]))]),
        };

        let encoded = write_project(&imported.document, Some(&history)).unwrap();
        let reopened = read_project(&encoded).unwrap();
        let reopened_history = reopened.history.unwrap();
        assert_eq!(history.max_entries, reopened_history.max_entries);
        assert_eq!(history.index, reopened_history.index);
        assert_eq!(history.entries.len(), reopened_history.entries.len());
        assert_eq!(history.entries[0].data, reopened_history.entries[0].data);
        assert_eq!(history.entries[0].label, reopened_history.entries[0].label);
        assert_eq!(
            history.entries[0].timestamp_millis,
            reopened_history.entries[0].timestamp_millis
        );
        assert_eq!(
            history.entries[0].extensions,
            reopened_history.entries[0].extensions
        );
        assert_eq!(history.extensions, reopened_history.extensions);
        assert_eq!(17, reopened.source_schema_version);
        assert!(reopened.source_schema_version_present);
        assert_eq!(
            imported.document.occurrence_refs().unwrap().len(),
            reopened.document.occurrence_refs().unwrap().len()
        );
    }

    #[test]
    fn encodes_native_timeline_as_reopenable_legacy_history() {
        let mut document = Document::new();
        let mut timeline = TransactionHistory::new(SystemHistoryClock, HistoryLimits::default());
        let mut pending = timeline.begin(&document, "Create cuboid");
        pending
            .stage(|draft| {
                draft
                    .add_cuboid("box", Vec3::new(2.0, 3.0, 4.0), Vec3::ZERO)
                    .map(|_| ())
                    .ok_or_else(|| "could not create cuboid".to_owned())
            })
            .unwrap();
        timeline.commit(&mut document, pending).unwrap();
        let snapshots = timeline.snapshot_timeline();
        let camera_state = json!({
            "position": {"x": 9.0, "y": 4.0, "z": -6.0},
            "target": {"x": 1.0, "y": 2.0, "z": 3.0},
            "near": 0.2,
            "far": 320.0,
            "fieldOfView": 54.0
        });

        let encoded_history =
            encode_native_history(&snapshots, timeline.cursor(), &camera_state).unwrap();
        assert_eq!(2, encoded_history.max_entries);
        assert_eq!(1, encoded_history.index);
        assert_eq!(Some("Initial"), encoded_history.entries[0].label.as_deref());
        assert_eq!(
            Some("Create cuboid"),
            encoded_history.entries[1].label.as_deref()
        );

        let encoded_project = write_project(&document, Some(&encoded_history)).unwrap();
        let reopened = read_project(&encoded_project).unwrap();
        let reopened_history = reopened.history.unwrap();
        assert_eq!(2, reopened_history.entries.len());
        assert_eq!(1, reopened_history.index);
        for (snapshot, entry) in snapshots.iter().zip(&reopened_history.entries) {
            let decoded = LegacySnapshotAdapter::default()
                .decode_snapshot(&entry.data)
                .unwrap();
            let expected = meshcraft_core::legacy::snapshot_with_legacy_camera_state(
                &snapshot.document,
                camera_state.clone(),
            )
            .unwrap();
            assert_eq!(
                export_current_value(&expected).unwrap(),
                export_current_value(&decoded).unwrap()
            );
        }
    }

    #[test]
    fn rejects_native_history_with_an_invalid_cursor() {
        let snapshots = [meshcraft_core::transactions::HistorySnapshot {
            document: Document::new(),
            label: Some("Initial".to_owned()),
            timestamp_millis: 0,
        }];
        assert!(matches!(
            encode_native_history(&snapshots, 1, &json!({})),
            Err(ProjectError::InvalidHistory(_))
        ));
    }

    #[test]
    fn writes_a_reopenable_empty_document() {
        let encoded = write_project(&Document::new(), None).unwrap();
        let reopened = read_project(&encoded).unwrap();
        assert_eq!(1, reopened.document.definitions().count());
        assert_eq!(1, reopened.document.occurrence_refs().unwrap().len());
        let root = reopened
            .document
            .resolve_occurrence(&reopened.document.occurrence_refs().unwrap()[0])
            .unwrap();
        assert!(root.definition.geometry.is_empty());
    }

    #[test]
    fn imports_historical_root_group_tree_and_preserves_unrecognized_group_payload() {
        let mut source: Value = serde_json::from_str(ROOT_GROUP_MODEL).unwrap();
        source["rootGroup"]["futureDomainPayload"] = json!({"enabled": true, "values": [1, 2]});
        source["rootGroup"]["segments"][0]["futureSegmentPayload"] = json!({"tag": "line"});
        source["rootGroup"]["faces"][0]["futureFacePayload"] = json!(["surface"]);
        source["rootGroup"]["children"] = json!([{
            "id": "child-instance",
            "name": "Child",
            "instanceOrigin": {"x": 10},
            "instanceAxisU": {"x": 1},
            "instanceAxisV": {"y": 1},
            "instanceAxisW": {"z": 1},
            "segments": [{"start": {}, "end": {"x": 1}}]
        }]);
        let imported = read_project(source.to_string().as_bytes()).unwrap();
        let occurrences = imported.document.occurrence_refs().unwrap();
        assert_eq!(2, occurrences.len());
        let root = imported
            .document
            .resolve_occurrence(&occurrences[0])
            .unwrap();
        assert_eq!("Root", root.definition.name);
        assert_eq!(3, root.definition.geometry.segments.len());
        assert_eq!(1, root.definition.geometry.triangles.len());
        assert_eq!(
            Some(&json!({"enabled": true, "values": [1, 2]})),
            root.definition.extensions.get("k3d.futureDomainPayload")
        );
        assert!(
            root.definition
                .geometry
                .vertices
                .values()
                .any(|vertex| { vertex.position == Vec3::new(-3.0, 0.0, 4.0) })
        );
        assert!(root.definition.geometry.segments.values().any(|segment| {
            segment.extensions.get("k3d.futureSegmentPayload") == Some(&json!({"tag": "line"}))
        }));
        assert!(root.definition.geometry.triangles.values().any(|triangle| {
            triangle.extensions.get("k3d.futureFacePayload") == Some(&json!(["surface"]))
        }));
        let child = occurrences
            .iter()
            .find(|occurrence| occurrence.path.len() == 2)
            .unwrap();
        let child = imported.document.resolve_occurrence(child).unwrap();
        assert_eq!("Child", child.definition.name);
        assert_eq!(10.0, child.world_transform.translation.x);
    }

    #[test]
    fn imports_flat_geometry_and_prototype_only_recovery_shapes() {
        let root_group: Value = serde_json::from_str(ROOT_GROUP_MODEL).unwrap();
        let group = &root_group["rootGroup"];
        let flat = json!({
            "segments": group["segments"].clone(),
            "faces": group["faces"].clone()
        });
        let flat_document = read_project(flat.to_string().as_bytes()).unwrap().document;
        let flat_occurrences = flat_document.occurrence_refs().unwrap();
        assert_eq!(1, flat_occurrences.len());
        let flat_root = flat_document
            .resolve_occurrence(&flat_occurrences[0])
            .unwrap();
        assert_eq!(3, flat_root.definition.geometry.segments.len());
        assert_eq!(1, flat_root.definition.geometry.triangles.len());

        let mut prototype_only: Value = serde_json::from_str(CURRENT_MODEL).unwrap();
        prototype_only
            .as_object_mut()
            .unwrap()
            .remove("rootInstance");
        let recovered = read_project(prototype_only.to_string().as_bytes()).unwrap();
        let occurrences = recovered.document.occurrence_refs().unwrap();
        assert_eq!(2, occurrences.len());
        let root_path = occurrences
            .iter()
            .find(|occurrence| occurrence.path.len() == 1)
            .unwrap();
        assert_eq!(
            "Root",
            recovered
                .document
                .resolve_occurrence(root_path)
                .unwrap()
                .definition
                .name
        );
    }

    #[test]
    fn rejects_malformed_prototype_container_instead_of_treating_it_as_flat_geometry() {
        let error = read_project(br#"{"prototypes":{},"segments":[]}"#).unwrap_err();
        assert!(
            matches!(error, ProjectError::InvalidModel(message) if message.contains("/prototypes must be an array"))
        );
    }

    #[test]
    fn repairs_only_an_empty_zero_basis_root_and_reports_the_original_axes() {
        let empty = json!({
            "prototypes": [{
                "id": "root", "name": "Root",
                "definitionAxisU": {"x": 1},
                "definitionAxisV": {"y": 1},
                "definitionAxisW": {"z": 1}
            }],
            "rootInstance": {
                "id": "root", "prototypeId": "root",
                "instanceOrigin": {},
                "instanceAxisU": {}, "instanceAxisV": {}, "instanceAxisW": {}
            }
        });
        let repaired = read_project(empty.to_string().as_bytes()).unwrap();
        assert_eq!(1, repaired.warnings.len());
        let occurrence = repaired.document.occurrence_refs().unwrap().remove(0);
        let resolved = repaired.document.resolve_occurrence(&occurrence).unwrap();
        assert_eq!(Vec3::X, resolved.world_transform.transform_point(Vec3::X));
        let serialized = serde_json::to_value(&repaired.document).unwrap();
        assert_eq!(
            json!({
                "kind": "empty-root-zero-basis-to-identity",
                "originalInstanceAxes": {
                    "instanceAxisU": {}, "instanceAxisV": {}, "instanceAxisW": {}
                }
            }),
            serialized["extensions"]["k3d.__meshcraft_migration_repair"]
        );

        let nonempty = json!({
            "prototypes": [{
                "id": "root", "name": "Root",
                "definitionAxisU": {"x": 1},
                "definitionAxisV": {"y": 1},
                "definitionAxisW": {"z": 1},
                "segments": [{"start": {}, "end": {"x": 1}}]
            }],
            "rootInstance": {
                "id": "root", "prototypeId": "root",
                "instanceOrigin": {},
                "instanceAxisU": {}, "instanceAxisV": {}, "instanceAxisW": {}
            }
        });
        assert!(matches!(
            read_project(nonempty.to_string().as_bytes()),
            Err(ProjectError::InvalidModel(message)) if message.contains("singular")
        ));
    }

    #[test]
    fn parses_sparse_history_defaults_and_preserves_unknown_nested_fields() {
        let mut snapshot: Value = serde_json::from_str(CURRENT_MODEL).unwrap();
        snapshot["undoHistory"] = json!({
            "index": 4,
            "futurePolicy": {"keep": true},
            "entries": [{"data": "opaque", "timestamp": 9, "futureEntry": [1, 2]}]
        });
        let imported = read_project(snapshot.to_string().as_bytes()).unwrap();
        let history = imported.history.unwrap();
        assert_eq!(0, history.max_entries);
        assert_eq!(4, history.index);
        assert_eq!(Some(0), history.active_entry_index());
        assert_eq!("opaque", history.entries[0].data);
        assert_eq!(
            Some(&json!({"keep": true})),
            history.extensions.get("futurePolicy")
        );
        assert_eq!(
            Some(&json!([1, 2])),
            history.entries[0].extensions.get("futureEntry")
        );
    }

    #[test]
    fn lazily_decodes_standard_base64_gzip_history_entries() {
        let encoded = BASE64_STANDARD.encode(gzip(CURRENT_MODEL.as_bytes()));
        let mut history = OpaqueLegacyHistory {
            max_entries: 10,
            index: 0,
            entries: vec![meshcraft_core::transactions::OpaqueLegacyUndoEntry {
                data: encoded.clone(),
                label: Some("Before edit".to_owned()),
                timestamp_millis: 5,
                diagnostic: None,
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        };
        let decoded = history
            .decode_entry(0, &LegacySnapshotAdapter::default())
            .unwrap();
        assert_eq!(2, decoded.occurrence_refs().unwrap().len());
        assert_eq!(encoded, history.entries[0].data);
    }

    #[test]
    fn corrupt_history_encoding_is_diagnostic_and_does_not_discard_the_entry() {
        let encoded = "%%% invalid base64 %%%".to_owned();
        let mut history = OpaqueLegacyHistory {
            max_entries: 1,
            index: 0,
            entries: vec![meshcraft_core::transactions::OpaqueLegacyUndoEntry {
                data: encoded.clone(),
                label: Some("Bad entry".to_owned()),
                timestamp_millis: 7,
                diagnostic: None,
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        };
        assert!(
            history
                .decode_entry(0, &LegacySnapshotAdapter::default())
                .is_err()
        );
        assert_eq!(encoded, history.entries[0].data);
        assert!(history.entries[0].diagnostic.is_some());
        assert_eq!(Some(0), history.active_entry_index());
    }

    #[test]
    fn gzip_reader_reports_cancellation_size_limit_and_bad_streams() {
        let compressed = gzip(CURRENT_MODEL.as_bytes());
        assert!(matches!(
            decode_project_payload(&compressed, 16),
            Err(ProjectError::PayloadTooLarge { .. })
        ));
        assert_eq!(
            Err(ProjectError::Cancelled),
            read_project_with(&compressed, 1024 * 1024, |_| {}, || true).map(|_| ())
        );
        assert!(matches!(
            decode_project_payload(&[0x1f, 0x8b, 0x00], 1024),
            Err(ProjectError::InvalidGzip(_))
        ));
    }

    #[test]
    fn project_writer_uses_plain_json_at_threshold_and_gzip_above_it() {
        let at_limit = format!("{{\"payload\":\"{}\"}}", "x".repeat(10 * 1024 * 1024 - 14));
        let plain = encode_project_json(&at_limit).unwrap();
        assert!(!plain.starts_with(&[0x1f, 0x8b]));
        let over_limit = format!("{{\"payload\":\"{}\"}}", "x".repeat(10 * 1024 * 1024));
        let compressed = encode_project_json(&over_limit).unwrap();
        assert!(compressed.starts_with(&[0x1f, 0x8b]));
        assert_eq!(
            over_limit.as_bytes(),
            decode_project_payload(&compressed, 11 * 1024 * 1024).unwrap()
        );
    }
}
