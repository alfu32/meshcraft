//! Explicit document transactions and bounded, snapshot-backed undo history.
//!
//! Transactions own their draft until commit. Derived caches and editor session state are
//! deliberately outside the document snapshots.

use crate::document::{
    ChildLayoutId, Definition, DefinitionId, Document, InstanceEdits, OccurrenceRef,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryLimits {
    pub max_entries: usize,
    /// Approximate bound measured as serialized JSON bytes for retained document states.
    pub max_snapshot_bytes: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_entries: 20,
            max_snapshot_bytes: 64 * 1024 * 1024,
        }
    }
}

impl HistoryLimits {
    pub fn new(max_entries: usize, max_snapshot_bytes: usize) -> Result<Self, String> {
        if max_entries == 0 {
            return Err("history must retain at least one entry".to_owned());
        }
        if max_snapshot_bytes == 0 {
            return Err("history snapshot byte budget must be positive".to_owned());
        }
        Ok(Self {
            max_entries,
            max_snapshot_bytes,
        })
    }
}

pub trait HistoryClock {
    fn now_millis(&self) -> i64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemHistoryClock;

impl HistoryClock for SystemHistoryClock {
    fn now_millis(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                duration.as_millis().min(i64::MAX as u128) as i64
            })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ChangeKind {
    Geometry,
    Appearance,
    Structure,
    ModelSettings,
    Metadata,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DerivedDataKind {
    GeometryIndexes,
    WorldBounds,
    OccurrenceMembership,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeSet {
    pub before_revision: u64,
    pub after_revision: u64,
    pub definitions: BTreeSet<DefinitionId>,
    pub child_layouts: BTreeSet<ChildLayoutId>,
    pub occurrences: BTreeSet<OccurrenceRef>,
    pub kinds: BTreeSet<ChangeKind>,
    pub invalidated: BTreeSet<DerivedDataKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionError {
    Operation(String),
    InvalidDocument(String),
    StaleDocument {
        expected_revision: u64,
        actual_revision: u64,
    },
    HistoryBudgetExceeded {
        required_bytes: usize,
        max_bytes: usize,
    },
    RevisionExhausted,
    SnapshotEncoding(String),
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation(message) => {
                write!(formatter, "transaction operation failed: {message}")
            }
            Self::InvalidDocument(message) => {
                write!(formatter, "invalid transaction document: {message}")
            }
            Self::StaleDocument {
                expected_revision,
                actual_revision,
            } => write!(
                formatter,
                "document changed during transaction (expected revision {expected_revision}, got {actual_revision})"
            ),
            Self::HistoryBudgetExceeded {
                required_bytes,
                max_bytes,
            } => write!(
                formatter,
                "undo history needs {required_bytes} snapshot bytes, over its {max_bytes} byte budget"
            ),
            Self::RevisionExhausted => {
                formatter.write_str("document revision counter is exhausted")
            }
            Self::SnapshotEncoding(message) => {
                write!(formatter, "could not size history snapshot: {message}")
            }
        }
    }
}

impl std::error::Error for TransactionError {}

pub struct PendingTransaction {
    base_revision: u64,
    label: String,
    draft: Document,
}

impl PendingTransaction {
    /// Applies an operation to a candidate draft and publishes it to this preview only if it
    /// succeeds and leaves a valid document. A failed operation leaves the preview untouched.
    pub fn stage<T>(
        &mut self,
        operation: impl FnOnce(&mut Document) -> Result<T, String>,
    ) -> Result<T, TransactionError> {
        let mut candidate = self.draft.clone();
        let result = operation(&mut candidate).map_err(TransactionError::Operation)?;
        candidate
            .validate()
            .map_err(TransactionError::InvalidDocument)?;
        self.draft = candidate;
        Ok(result)
    }

    pub fn preview(&self) -> &Document {
        &self.draft
    }

    pub fn cancel(self) {}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntryInfo {
    pub label: String,
    pub timestamp_millis: i64,
    pub origin: HistoryEntryOrigin,
    pub change_set: ChangeSet,
}

/// A document state and presentation metadata in a saveable history timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct HistorySnapshot {
    pub document: Document,
    pub label: Option<String>,
    pub timestamp_millis: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryEntryOrigin {
    Native,
    Legacy {
        from_entry_index: usize,
        to_entry_index: usize,
    },
}

struct HistoryState {
    document: Document,
    encoded_size: usize,
}

pub struct TransactionHistory<C = SystemHistoryClock> {
    clock: C,
    limits: HistoryLimits,
    states: Vec<HistoryState>,
    entries: Vec<HistoryEntryInfo>,
    cursor: usize,
    snapshot_bytes: usize,
}

impl Default for TransactionHistory<SystemHistoryClock> {
    fn default() -> Self {
        Self::new(SystemHistoryClock, HistoryLimits::default())
    }
}

impl<C: HistoryClock> TransactionHistory<C> {
    pub fn new(clock: C, limits: HistoryLimits) -> Self {
        assert!(
            limits.max_entries > 0,
            "history must retain at least one entry"
        );
        assert!(
            limits.max_snapshot_bytes > 0,
            "history byte budget must be positive"
        );
        Self {
            clock,
            limits,
            states: Vec::new(),
            entries: Vec::new(),
            cursor: 0,
            snapshot_bytes: 0,
        }
    }

    pub fn begin(&self, document: &Document, label: impl Into<String>) -> PendingTransaction {
        PendingTransaction {
            base_revision: document.revision(),
            label: label.into(),
            draft: document.clone(),
        }
    }

    /// Run a single operation in an isolated draft and commit it as one undo entry.
    /// Unlike repeated preview staging, this needs no extra rollback copy: any failed
    /// operation or validation discards the entire draft and leaves the live model intact.
    pub fn transact<T>(
        &mut self,
        document: &mut Document,
        label: impl Into<String>,
        operation: impl FnOnce(&mut Document) -> Result<T, String>,
    ) -> Result<(T, Option<ChangeSet>), TransactionError> {
        let mut pending = self.begin(document, label);
        let result = operation(&mut pending.draft).map_err(TransactionError::Operation)?;
        let changes = self.commit(document, pending)?;
        Ok((result, changes))
    }

    pub fn commit(
        &mut self,
        document: &mut Document,
        mut pending: PendingTransaction,
    ) -> Result<Option<ChangeSet>, TransactionError> {
        if document.revision() != pending.base_revision {
            return Err(TransactionError::StaleDocument {
                expected_revision: pending.base_revision,
                actual_revision: document.revision(),
            });
        }
        if same_authored_state(document, &pending.draft) {
            return Ok(None);
        }
        pending.draft.revision = document
            .revision()
            .checked_add(1)
            .ok_or(TransactionError::RevisionExhausted)?;
        pending
            .draft
            .validate()
            .map_err(TransactionError::InvalidDocument)?;

        let mut change_set = diff_documents(document, &pending.draft);
        let before_size = snapshot_size(document)?;
        let after_size = snapshot_size(&pending.draft)?;
        let (base_index, base_bytes, retained_entries) =
            self.plan_retained_history(document, before_size, after_size)?;

        if self.states.is_empty()
            || !same_authored_state(&self.states[self.cursor].document, document)
        {
            self.states.clear();
            self.entries.clear();
            self.snapshot_bytes = 0;
            self.cursor = 0;
        } else if base_index > 0 {
            self.states.drain(0..base_index);
            self.entries.drain(0..base_index);
            self.snapshot_bytes = self.states.iter().map(|state| state.encoded_size).sum();
            self.cursor -= base_index;
        }

        self.states.truncate(self.cursor + 1);
        self.entries.truncate(self.cursor);
        self.snapshot_bytes = self.states.iter().map(|state| state.encoded_size).sum();
        if self.states.is_empty() {
            self.states.push(HistoryState {
                document: document.clone(),
                encoded_size: before_size,
            });
            self.snapshot_bytes += before_size;
        }
        let new_total = self.snapshot_bytes.saturating_add(after_size);
        if new_total > self.limits.max_snapshot_bytes {
            return Err(TransactionError::HistoryBudgetExceeded {
                required_bytes: new_total,
                max_bytes: self.limits.max_snapshot_bytes,
            });
        }
        change_set.before_revision = document.revision();
        change_set.after_revision = pending.draft.revision;
        self.entries.push(HistoryEntryInfo {
            label: pending.label,
            timestamp_millis: self.clock.now_millis(),
            origin: HistoryEntryOrigin::Native,
            change_set: change_set.clone(),
        });
        self.states.push(HistoryState {
            document: pending.draft.clone(),
            encoded_size: after_size,
        });
        self.snapshot_bytes = new_total;
        self.cursor = self.entries.len();

        // `plan_retained_history` accounts for the new operation and either the retained suffix
        // or the current baseline. The live document changes only after all checks have passed.
        debug_assert!(self.entries.len() <= self.limits.max_entries);
        debug_assert_eq!(self.snapshot_bytes, base_bytes + after_size);
        debug_assert_eq!(self.entries.len(), retained_entries);
        *document = pending.draft;
        Ok(Some(change_set))
    }

    fn plan_retained_history(
        &self,
        document: &Document,
        before_size: usize,
        after_size: usize,
    ) -> Result<(usize, usize, usize), TransactionError> {
        if before_size.saturating_add(after_size) > self.limits.max_snapshot_bytes {
            return Err(TransactionError::HistoryBudgetExceeded {
                required_bytes: before_size.saturating_add(after_size),
                max_bytes: self.limits.max_snapshot_bytes,
            });
        }
        if self.states.is_empty()
            || !same_authored_state(&self.states[self.cursor].document, document)
        {
            return Ok((0, before_size, 1));
        }
        let mut first_state = 0;
        let mut retained_bytes = self.states[..=self.cursor]
            .iter()
            .map(|state| state.encoded_size)
            .sum::<usize>();
        let mut retained_entries = self.cursor + 1;
        while retained_entries > self.limits.max_entries
            || retained_bytes.saturating_add(after_size) > self.limits.max_snapshot_bytes
        {
            if first_state >= self.cursor {
                return Err(TransactionError::HistoryBudgetExceeded {
                    required_bytes: retained_bytes.saturating_add(after_size),
                    max_bytes: self.limits.max_snapshot_bytes,
                });
            }
            retained_bytes = retained_bytes.saturating_sub(self.states[first_state].encoded_size);
            first_state += 1;
            retained_entries -= 1;
        }
        Ok((first_state, retained_bytes, retained_entries))
    }

    pub fn undo(&mut self, document: &mut Document) -> Result<Option<ChangeSet>, TransactionError> {
        if self.cursor == 0 || self.states.is_empty() {
            return Ok(None);
        }
        if !same_authored_state(&self.states[self.cursor].document, document) {
            return Err(TransactionError::StaleDocument {
                expected_revision: self.states[self.cursor].document.revision(),
                actual_revision: document.revision(),
            });
        }
        self.restore_state(document, self.cursor - 1)
    }

    pub fn redo(&mut self, document: &mut Document) -> Result<Option<ChangeSet>, TransactionError> {
        if self.cursor >= self.entries.len() || self.states.is_empty() {
            return Ok(None);
        }
        if !same_authored_state(&self.states[self.cursor].document, document) {
            return Err(TransactionError::StaleDocument {
                expected_revision: self.states[self.cursor].document.revision(),
                actual_revision: document.revision(),
            });
        }
        self.restore_state(document, self.cursor + 1)
    }

    /// Adds an older decoded legacy snapshot immediately before the current timeline state.
    /// The caller can then use ordinary `undo` to traverse it; already loaded redo states remain
    /// ordered after the current state.
    pub fn prepend_legacy_state(
        &mut self,
        document: &Document,
        earlier: Document,
        label: impl Into<String>,
        timestamp_millis: i64,
        earlier_entry_index: usize,
        current_entry_index: usize,
    ) -> Result<(), TransactionError> {
        earlier
            .validate()
            .map_err(TransactionError::InvalidDocument)?;
        document
            .revision()
            .checked_add(1)
            .ok_or(TransactionError::RevisionExhausted)?;
        if !self.states.is_empty()
            && !same_authored_state(&self.states[self.cursor].document, document)
        {
            return Err(TransactionError::StaleDocument {
                expected_revision: self.states[self.cursor].document.revision(),
                actual_revision: document.revision(),
            });
        }
        let current_size = match self.states.get(self.cursor) {
            Some(state) => state.encoded_size,
            None => snapshot_size(document)?,
        };
        let current_additional_size = if self.states.is_empty() {
            current_size
        } else {
            0
        };
        let earlier_size = snapshot_size(&earlier)?;
        self.check_history_capacity(current_size.saturating_add(earlier_size))?;

        let mut change_set = diff_documents(&earlier, document);
        change_set.before_revision = earlier.revision();
        change_set.after_revision = document.revision();
        if self.states.is_empty() {
            self.states.push(HistoryState {
                document: earlier,
                encoded_size: earlier_size,
            });
            self.states.push(HistoryState {
                document: document.clone(),
                encoded_size: current_size,
            });
            self.entries.push(HistoryEntryInfo {
                label: label.into(),
                timestamp_millis,
                origin: HistoryEntryOrigin::Legacy {
                    from_entry_index: earlier_entry_index,
                    to_entry_index: current_entry_index,
                },
                change_set,
            });
            self.cursor = 1;
        } else {
            if self.cursor != 0 {
                return Err(TransactionError::StaleDocument {
                    expected_revision: self.states[self.cursor].document.revision(),
                    actual_revision: document.revision(),
                });
            }
            self.states.insert(
                0,
                HistoryState {
                    document: earlier,
                    encoded_size: earlier_size,
                },
            );
            self.entries.insert(
                0,
                HistoryEntryInfo {
                    label: label.into(),
                    timestamp_millis,
                    origin: HistoryEntryOrigin::Legacy {
                        from_entry_index: earlier_entry_index,
                        to_entry_index: current_entry_index,
                    },
                    change_set,
                },
            );
            self.cursor += 1;
        }
        self.snapshot_bytes = self
            .snapshot_bytes
            .saturating_add(current_additional_size)
            .saturating_add(earlier_size);
        self.trim_history_from_end();
        Ok(())
    }

    /// Adds a decoded legacy snapshot after the current timeline state. The caller can then use
    /// ordinary `redo` to move into it.
    pub fn append_legacy_state(
        &mut self,
        document: &Document,
        later: Document,
        label: impl Into<String>,
        timestamp_millis: i64,
        current_entry_index: usize,
        later_entry_index: usize,
    ) -> Result<(), TransactionError> {
        later
            .validate()
            .map_err(TransactionError::InvalidDocument)?;
        document
            .revision()
            .checked_add(1)
            .ok_or(TransactionError::RevisionExhausted)?;
        if !self.states.is_empty()
            && (self.cursor != self.entries.len()
                || !same_authored_state(&self.states[self.cursor].document, document))
        {
            return Err(TransactionError::StaleDocument {
                expected_revision: self.states[self.cursor].document.revision(),
                actual_revision: document.revision(),
            });
        }
        let current_size = match self.states.get(self.cursor) {
            Some(state) => state.encoded_size,
            None => snapshot_size(document)?,
        };
        let current_additional_size = if self.states.is_empty() {
            current_size
        } else {
            0
        };
        let later_size = snapshot_size(&later)?;
        self.check_history_capacity(current_size.saturating_add(later_size))?;

        let mut change_set = diff_documents(document, &later);
        change_set.before_revision = document.revision();
        change_set.after_revision = later.revision();
        if self.states.is_empty() {
            self.states.push(HistoryState {
                document: document.clone(),
                encoded_size: current_size,
            });
        }
        self.states.push(HistoryState {
            document: later,
            encoded_size: later_size,
        });
        self.entries.push(HistoryEntryInfo {
            label: label.into(),
            timestamp_millis,
            origin: HistoryEntryOrigin::Legacy {
                from_entry_index: current_entry_index,
                to_entry_index: later_entry_index,
            },
            change_set,
        });
        self.snapshot_bytes = self
            .snapshot_bytes
            .saturating_add(current_additional_size)
            .saturating_add(later_size);
        self.trim_history_from_front();
        Ok(())
    }

    fn check_history_capacity(&self, snapshot_bytes: usize) -> Result<(), TransactionError> {
        if snapshot_bytes > self.limits.max_snapshot_bytes {
            return Err(TransactionError::HistoryBudgetExceeded {
                required_bytes: snapshot_bytes,
                max_bytes: self.limits.max_snapshot_bytes,
            });
        }
        Ok(())
    }

    fn trim_history_from_end(&mut self) {
        while self.entries.len() > self.limits.max_entries
            || self.snapshot_bytes > self.limits.max_snapshot_bytes
        {
            debug_assert!(self.cursor < self.entries.len());
            let removed = self.states.pop().expect("history has its final state");
            self.entries.pop().expect("history has its final entry");
            self.snapshot_bytes = self.snapshot_bytes.saturating_sub(removed.encoded_size);
        }
    }

    fn trim_history_from_front(&mut self) {
        while self.entries.len() > self.limits.max_entries
            || self.snapshot_bytes > self.limits.max_snapshot_bytes
        {
            debug_assert!(self.cursor > 0);
            let removed = self.states.remove(0);
            self.entries.remove(0);
            self.cursor -= 1;
            self.snapshot_bytes = self.snapshot_bytes.saturating_sub(removed.encoded_size);
        }
    }

    fn restore_state(
        &mut self,
        document: &mut Document,
        target_index: usize,
    ) -> Result<Option<ChangeSet>, TransactionError> {
        let mut candidate = self.states[target_index].document.clone();
        let next_revision = document
            .revision()
            .checked_add(1)
            .ok_or(TransactionError::RevisionExhausted)?;
        candidate.revision = next_revision;
        candidate
            .validate()
            .map_err(TransactionError::InvalidDocument)?;
        let mut change_set = diff_documents(document, &candidate);
        change_set.before_revision = document.revision();
        change_set.after_revision = next_revision;
        self.cursor = target_index;
        *document = candidate;
        Ok(Some(change_set))
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.entries
            .get(self.cursor.checked_sub(1)?)
            .map(|entry| entry.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.entries
            .get(self.cursor)
            .map(|entry| entry.label.as_str())
    }

    pub fn entries(&self) -> &[HistoryEntryInfo] {
        &self.entries
    }

    /// Clones the bounded document timeline with labels and timestamps for persistence.
    pub fn snapshot_timeline(&self) -> Vec<HistorySnapshot> {
        self.states
            .iter()
            .enumerate()
            .map(|(index, state)| {
                match index
                    .checked_sub(1)
                    .and_then(|entry| self.entries.get(entry))
                {
                    Some(entry) => HistorySnapshot {
                        document: state.document.clone(),
                        label: Some(entry.label.clone()),
                        timestamp_millis: entry.timestamp_millis,
                    },
                    None => HistorySnapshot {
                        document: state.document.clone(),
                        label: Some("Initial".to_owned()),
                        timestamp_millis: 0,
                    },
                }
            })
            .collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn snapshot_bytes(&self) -> usize {
        self.snapshot_bytes
    }

    pub fn limits(&self) -> HistoryLimits {
        self.limits
    }

    pub fn clear(&mut self) {
        self.states.clear();
        self.entries.clear();
        self.cursor = 0;
        self.snapshot_bytes = 0;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueLegacyUndoEntry {
    pub data: String,
    pub label: Option<String>,
    pub timestamp_millis: i64,
    pub diagnostic: Option<String>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueLegacyHistory {
    pub max_entries: i32,
    /// Kept exactly as read; `-1` means no active entry in the legacy manager.
    pub index: i32,
    pub entries: Vec<OpaqueLegacyUndoEntry>,
    pub extensions: BTreeMap<String, Value>,
}

pub trait LegacyHistoryDecoder {
    fn decode_snapshot(&self, encoded_base64_gzip: &str) -> Result<Document, String>;
}

impl OpaqueLegacyHistory {
    fn nonempty_entry_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.data.trim().is_empty())
            .map(|(index, _)| index)
            .collect()
    }

    /// Resolves the legacy manager's active position after it filters empty encoded entries.
    /// The stored `index` and all original entries remain untouched for lossless re-export.
    pub fn active_entry_index(&self) -> Option<usize> {
        let nonempty = self.nonempty_entry_indices();
        if nonempty.is_empty() {
            return None;
        }
        let active = usize::try_from(self.index)
            .unwrap_or_default()
            .min(nonempty.len() - 1);
        nonempty.get(active).copied()
    }

    /// Returns the preceding nonempty source entry, preserving the legacy manager's filtered
    /// ordering while keeping the original entry indices available for diagnostics and export.
    pub fn previous_entry_index(&self, current_index: usize) -> Option<usize> {
        let nonempty = self.nonempty_entry_indices();
        let position = nonempty.iter().position(|index| *index == current_index)?;
        position
            .checked_sub(1)
            .and_then(|previous| nonempty.get(previous).copied())
    }

    /// Returns the next nonempty source entry after the current imported state.
    pub fn next_entry_index(&self, current_index: usize) -> Option<usize> {
        let nonempty = self.nonempty_entry_indices();
        let position = nonempty.iter().position(|index| *index == current_index)?;
        nonempty.get(position + 1).copied()
    }

    /// Lazily decodes one entry. The opaque string is retained unchanged on success and failure.
    /// A bad entry returns a diagnostic and never mutates a live document.
    pub fn decode_entry(
        &mut self,
        index: usize,
        decoder: &impl LegacyHistoryDecoder,
    ) -> Result<Document, String> {
        let Some(entry) = self.entries.get_mut(index) else {
            return Err(format!("legacy undo entry {index} does not exist"));
        };
        let result = decoder
            .decode_snapshot(&entry.data)
            .and_then(|document| document.validate().map(|()| document));
        match result {
            Ok(document) => {
                entry.diagnostic = None;
                Ok(document)
            }
            Err(message) => {
                entry.diagnostic = Some(message.clone());
                Err(message)
            }
        }
    }
}

fn snapshot_size(document: &Document) -> Result<usize, TransactionError> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, document)
        .map_err(|error| TransactionError::SnapshotEncoding(error.to_string()))?;
    Ok(counter.0)
}

fn same_authored_state(left: &Document, right: &Document) -> bool {
    left.same_authored_state(right)
}

fn map_changed<K, V>(left: &BTreeMap<K, V>, right: &BTreeMap<K, V>) -> BTreeSet<K>
where
    K: Clone + Ord,
    V: PartialEq,
{
    left.keys()
        .chain(right.keys())
        .filter(|key| left.get(*key) != right.get(*key))
        .cloned()
        .collect()
}

fn diff_documents(before: &Document, after: &Document) -> ChangeSet {
    let mut change_set = ChangeSet {
        definitions: map_changed(&before.definitions, &after.definitions),
        child_layouts: map_changed(&before.child_layouts, &after.child_layouts),
        occurrences: map_changed(&before.occurrence_edits, &after.occurrence_edits),
        ..ChangeSet::default()
    };

    for id in &change_set.definitions {
        match (before.definitions.get(id), after.definitions.get(id)) {
            (Some(left), Some(right)) => {
                classify_definition_change(left, right, &mut change_set.kinds)
            }
            _ => {
                change_set.kinds.insert(ChangeKind::Geometry);
                change_set.kinds.insert(ChangeKind::Appearance);
                change_set.kinds.insert(ChangeKind::Structure);
            }
        }
    }
    if !change_set.child_layouts.is_empty() {
        change_set.kinds.insert(ChangeKind::Structure);
        change_set.kinds.insert(ChangeKind::Geometry);
    }
    for occurrence in &change_set.occurrences {
        classify_instance_edit_change(
            before.occurrence_edits.get(occurrence),
            after.occurrence_edits.get(occurrence),
            &mut change_set.kinds,
        );
    }
    if before.root_layout != after.root_layout {
        change_set.kinds.insert(ChangeKind::Structure);
    }
    if before.model_settings != after.model_settings {
        change_set.kinds.insert(ChangeKind::ModelSettings);
    }
    if before.extensions != after.extensions || before.schema_version != after.schema_version {
        change_set.kinds.insert(ChangeKind::Metadata);
    }

    if change_set.kinds.contains(&ChangeKind::Geometry)
        || change_set.kinds.contains(&ChangeKind::Appearance)
    {
        change_set
            .invalidated
            .insert(DerivedDataKind::GeometryIndexes);
        change_set.invalidated.insert(DerivedDataKind::WorldBounds);
    }
    if change_set.kinds.contains(&ChangeKind::Structure) {
        change_set
            .invalidated
            .insert(DerivedDataKind::OccurrenceMembership);
        change_set.invalidated.insert(DerivedDataKind::WorldBounds);
    }
    change_set
}

fn classify_definition_change(
    before: &Definition,
    after: &Definition,
    kinds: &mut BTreeSet<ChangeKind>,
) {
    if before.geometry != after.geometry || before.frame != after.frame {
        kinds.insert(ChangeKind::Geometry);
    }
    if before.geometry != after.geometry {
        kinds.insert(ChangeKind::Appearance);
    }
    if before.default_child_layout != after.default_child_layout {
        kinds.insert(ChangeKind::Structure);
    }
    if before.name != after.name
        || before.kind != after.kind
        || before.domain_data != after.domain_data
        || before.domain_entity_ids != after.domain_entity_ids
        || before.extensions != after.extensions
        || before.legacy_aliases != after.legacy_aliases
    {
        kinds.insert(ChangeKind::Metadata);
    }
}

fn classify_instance_edit_change(
    before: Option<&InstanceEdits>,
    after: Option<&InstanceEdits>,
    kinds: &mut BTreeSet<ChangeKind>,
) {
    match (before, after) {
        (Some(left), Some(right)) => {
            if left.transform != right.transform
                || left.geometry_replacement != right.geometry_replacement
            {
                kinds.insert(ChangeKind::Geometry);
                kinds.insert(ChangeKind::Appearance);
            }
            if left.child_layout != right.child_layout {
                kinds.insert(ChangeKind::Structure);
            }
            if left.hotspot_positions != right.hotspot_positions
                || left.values != right.values
                || left.extensions != right.extensions
            {
                kinds.insert(ChangeKind::Metadata);
            }
        }
        _ => {
            kinds.insert(ChangeKind::Geometry);
            kinds.insert(ChangeKind::Appearance);
            kinds.insert(ChangeKind::Structure);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChangeKind, DerivedDataKind, HistoryClock, HistoryEntryOrigin, HistoryLimits,
        LegacyHistoryDecoder, OpaqueLegacyHistory, OpaqueLegacyUndoEntry, TransactionError,
        TransactionHistory,
    };
    use crate::document::{Document, ModelSettings};
    use crate::geometry::Vec3;
    use std::cell::Cell;
    use std::collections::BTreeMap;

    struct FakeClock(Cell<i64>);

    impl HistoryClock for FakeClock {
        fn now_millis(&self) -> i64 {
            self.0.get()
        }
    }

    fn history(max_entries: usize, max_bytes: usize) -> TransactionHistory<FakeClock> {
        TransactionHistory::new(
            FakeClock(Cell::new(42)),
            HistoryLimits::new(max_entries, max_bytes).unwrap(),
        )
    }

    #[test]
    fn one_shot_edit_discards_failures_and_preserves_undo_redo() {
        let mut d = Document::new();
        let mut h = history(10, 1_000_000);
        let before = d.clone();
        let result: Result<((), _), _> = h.transact(&mut d, "failed", |draft| {
            draft
                .add_cuboid("temporary", Vec3::X + Vec3::Y + Vec3::Z, Vec3::ZERO)
                .unwrap();
            Err("deliberate error".into())
        });
        assert!(matches!(result, Err(TransactionError::Operation(_))));
        assert_eq!(before, d);
        assert!(!h.can_undo());
        h.transact(&mut d, "add", |draft| {
            Ok(draft
                .add_cuboid("retained", Vec3::X + Vec3::Y + Vec3::Z, Vec3::ZERO)
                .unwrap())
        })
        .unwrap();
        let after = d.clone();
        assert_eq!(h.entries().len(), 1);
        h.undo(&mut d).unwrap();
        assert!(super::same_authored_state(&before, &d));
        h.redo(&mut d).unwrap();
        assert!(super::same_authored_state(&after, &d));
        let states = h.entries().to_vec();
        let result = h.transact(&mut d, "invalid", |draft| {
            draft
                .definitions
                .values_mut()
                .flat_map(|d| d.geometry.vertices.values_mut())
                .next()
                .unwrap()
                .position
                .x = f64::NAN;
            Ok(())
        });
        assert!(matches!(result, Err(TransactionError::InvalidDocument(_))));
        assert_eq!(after.revision() + 2, d.revision());
        assert!(super::same_authored_state(&after, &d));
        assert_eq!(states, h.entries());
    }

    #[test]
    fn counting_snapshot_writer_matches_json_byte_budget_including_escapes_and_unicode() {
        let mut d = Document::new();
        d.add_cuboid(
            "日本語\n\"quoted\"",
            Vec3::X + Vec3::Y + Vec3::Z,
            Vec3::ZERO,
        )
        .unwrap();
        assert_eq!(
            super::snapshot_size(&d).unwrap(),
            serde_json::to_vec(&d).unwrap().len()
        );
        let mut revision_only = d.clone();
        revision_only.revision += 1;
        assert!(super::same_authored_state(&d, &revision_only));
        revision_only.allocate_id();
        assert!(!super::same_authored_state(&d, &revision_only));
    }

    #[test]
    fn preview_cancel_and_failed_stage_do_not_publish_or_create_history() {
        let mut document = Document::new();
        let mut history = history(10, 1_000_000);
        let original = document.clone();
        let mut pending = history.begin(&document, "Add cuboid");
        pending
            .stage(|draft| {
                draft
                    .add_cuboid("preview", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
                    .ok_or_else(|| "could not add cuboid".to_owned())
            })
            .unwrap();
        assert_eq!(original, document);
        assert!(!pending.preview().occurrence_refs().unwrap().is_empty());
        assert!(
            pending
                .stage::<()>(|draft| {
                    draft.update_model_settings(ModelSettings {
                        unit_name: String::new(),
                        ..draft.model_settings().clone()
                    })?;
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(pending.preview().occurrence_refs().unwrap().len(), 1);
        pending.cancel();
        assert_eq!(original, document);
        assert!(history.entries().is_empty());

        let failed = history.begin(&document, "Invalid edit");
        assert!(history.commit(&mut document, failed).unwrap().is_none());
        assert!(history.entries().is_empty());
    }

    #[test]
    fn each_commit_is_one_entry_with_change_set_and_monotonic_undo_redo_revisions() {
        let mut document = Document::new();
        let mut history = history(10, 1_000_000);
        let mut first = history.begin(&document, "Create cuboid");
        first
            .stage(|draft| {
                draft
                    .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
                    .ok_or_else(|| "could not add cuboid".to_owned())
            })
            .unwrap();
        let created = history.commit(&mut document, first).unwrap().unwrap();
        assert_eq!(1, history.entries().len());
        assert_eq!(0, created.before_revision);
        assert_eq!(1, created.after_revision);
        assert!(created.kinds.contains(&ChangeKind::Geometry));
        assert!(created.invalidated.contains(&DerivedDataKind::WorldBounds));
        assert_eq!("Create cuboid", history.undo_label().unwrap());
        assert_eq!(42, history.entries()[0].timestamp_millis);

        let revision = document.revision();
        assert!(history.undo(&mut document).unwrap().is_some());
        assert!(document.occurrence_refs().unwrap().is_empty());
        assert_eq!(revision + 1, document.revision());
        assert!(history.can_redo());
        assert!(history.redo(&mut document).unwrap().is_some());
        assert_eq!(revision + 2, document.revision());
        assert_eq!(1, document.occurrence_refs().unwrap().len());
    }

    #[test]
    fn redo_branch_and_entry_cap_drop_oldest_history_deterministically() {
        let mut document = Document::new();
        let mut history = history(2, 1_000_000);
        for label in ["One", "Two"] {
            let mut pending = history.begin(&document, label);
            pending
                .stage(|draft| {
                    draft
                        .add_cuboid(label, Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
                        .ok_or_else(|| "could not add cuboid".to_owned())
                })
                .unwrap();
            history.commit(&mut document, pending).unwrap();
        }
        history.undo(&mut document).unwrap();
        assert!(history.can_redo());
        let mut branch = history.begin(&document, "Branch");
        branch
            .stage(|draft| {
                draft
                    .add_cuboid("branch", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
                    .ok_or_else(|| "could not add cuboid".to_owned())
            })
            .unwrap();
        history.commit(&mut document, branch).unwrap();
        assert!(!history.can_redo());
        assert_eq!("Branch", history.entries().last().unwrap().label);
        assert_eq!(2, history.entries().len());

        let mut third = history.begin(&document, "Third");
        third
            .stage(|draft| {
                draft
                    .add_cuboid("third", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
                    .ok_or_else(|| "could not add cuboid".to_owned())
            })
            .unwrap();
        history.commit(&mut document, third).unwrap();
        assert_eq!(2, history.entries().len());
        assert_eq!("Branch", history.entries()[0].label);
        assert_eq!("Third", history.entries()[1].label);
    }

    #[test]
    fn serialized_snapshot_budget_prunes_oldest_states_and_stays_bounded() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("Seed", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let definition = document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        let one_snapshot = serde_json::to_vec(&document).unwrap().len();
        let budget = one_snapshot * 2 + 32;
        let mut history = history(10, budget);

        for name in ["First", "Third"] {
            let mut pending = history.begin(&document, format!("Rename to {name}"));
            pending
                .stage(|draft| draft.rename_definition(definition, name).map(|_| ()))
                .unwrap();
            history.commit(&mut document, pending).unwrap();
            assert!(history.snapshot_bytes() <= budget);
        }

        assert_eq!(1, history.entries().len());
        assert_eq!("Rename to Third", history.undo_label().unwrap());
        assert!(history.undo(&mut document).unwrap().is_some());
        assert_eq!("First", document.definition(definition).unwrap().name);
    }

    #[test]
    fn undo_rejects_untracked_document_changes_instead_of_losing_them() {
        let mut document = Document::new();
        let mut history = history(4, 1_000_000);
        let mut pending = history.begin(&document, "Tracked");
        pending
            .stage(|draft| {
                draft
                    .add_cuboid("tracked", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
                    .ok_or_else(|| "could not add cuboid".to_owned())
            })
            .unwrap();
        history.commit(&mut document, pending).unwrap();
        document
            .add_cuboid("untracked", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let after_untracked_change = document.clone();

        assert!(matches!(
            history.undo(&mut document),
            Err(TransactionError::StaleDocument { .. })
        ));
        assert_eq!(after_untracked_change, document);
    }

    #[test]
    fn history_budget_rejects_before_publishing_document_or_dropping_redo() {
        let mut document = Document::new();
        let mut small = history(4, 1);
        let mut pending = small.begin(&document, "Rejected");
        pending
            .stage(|draft| {
                draft
                    .add_cuboid("large enough", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
                    .ok_or_else(|| "could not add cuboid".to_owned())
            })
            .unwrap();
        let original = document.clone();
        assert!(matches!(
            small.commit(&mut document, pending),
            Err(TransactionError::HistoryBudgetExceeded { .. })
        ));
        assert_eq!(original, document);
        assert!(small.entries().is_empty());
    }

    #[test]
    fn stale_transaction_is_rejected_without_publishing_its_preview() {
        let mut document = Document::new();
        let mut history = history(4, 1_000_000);
        let pending = history.begin(&document, "Stale");
        document
            .add_cuboid("outside", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let changed = document.clone();
        assert!(matches!(
            history.commit(&mut document, pending),
            Err(TransactionError::StaleDocument { .. })
        ));
        assert_eq!(changed, document);
        assert!(history.entries().is_empty());
    }

    struct BadDecoder;

    impl LegacyHistoryDecoder for BadDecoder {
        fn decode_snapshot(&self, _encoded_base64_gzip: &str) -> Result<Document, String> {
            Err("corrupt gzip payload".to_owned())
        }
    }

    #[test]
    fn opaque_legacy_history_preserves_corrupt_payload_and_reports_diagnostic() {
        let original = "not-rewritten-even-when-corrupt".to_owned();
        let mut legacy = OpaqueLegacyHistory {
            max_entries: 20,
            index: 0,
            entries: vec![OpaqueLegacyUndoEntry {
                data: original.clone(),
                label: Some("Auto".to_owned()),
                timestamp_millis: 123,
                diagnostic: None,
                extensions: BTreeMap::new(),
            }],
            extensions: BTreeMap::new(),
        };
        assert!(legacy.decode_entry(0, &BadDecoder).is_err());
        assert_eq!(original, legacy.entries[0].data);
        assert_eq!(
            Some("corrupt gzip payload"),
            legacy.entries[0].diagnostic.as_deref()
        );
        assert_eq!(0, legacy.index);
        assert_eq!(Some(0), legacy.active_entry_index());
        assert_eq!(20, legacy.max_entries);
    }

    #[test]
    fn legacy_history_timeline_bridges_imported_states_and_preserves_filtered_indices() {
        let mut earlier = Document::new();
        earlier
            .add_cuboid("earlier", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let mut current = earlier.clone();
        current
            .add_cuboid("current", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let mut later = current.clone();
        later
            .add_cuboid("later", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();

        let manager = OpaqueLegacyHistory {
            max_entries: 10,
            index: 1,
            entries: vec![
                OpaqueLegacyUndoEntry {
                    data: "old".to_owned(),
                    label: Some("Earlier".to_owned()),
                    timestamp_millis: 10,
                    diagnostic: None,
                    extensions: BTreeMap::new(),
                },
                OpaqueLegacyUndoEntry {
                    data: "  ".to_owned(),
                    label: None,
                    timestamp_millis: 0,
                    diagnostic: None,
                    extensions: BTreeMap::new(),
                },
                OpaqueLegacyUndoEntry {
                    data: "current".to_owned(),
                    label: Some("Current".to_owned()),
                    timestamp_millis: 20,
                    diagnostic: None,
                    extensions: BTreeMap::new(),
                },
                OpaqueLegacyUndoEntry {
                    data: "later".to_owned(),
                    label: Some("Later".to_owned()),
                    timestamp_millis: 30,
                    diagnostic: None,
                    extensions: BTreeMap::new(),
                },
            ],
            extensions: BTreeMap::new(),
        };
        assert_eq!(Some(2), manager.active_entry_index());
        assert_eq!(Some(0), manager.previous_entry_index(2));
        assert_eq!(Some(3), manager.next_entry_index(2));

        let mut live = current.clone();
        let mut history = history(4, 1_000_000);
        history
            .prepend_legacy_state(&live, earlier.clone(), "Earlier", 10, 0, 2)
            .unwrap();
        assert_eq!(
            HistoryEntryOrigin::Legacy {
                from_entry_index: 0,
                to_entry_index: 2
            },
            history.entries()[0].origin
        );
        history.undo(&mut live).unwrap();
        assert_eq!(1, live.occurrence_refs().unwrap().len());
        history.redo(&mut live).unwrap();
        assert_eq!(2, live.occurrence_refs().unwrap().len());
        history
            .append_legacy_state(&live, later, "Later", 30, 2, 3)
            .unwrap();
        history.redo(&mut live).unwrap();
        assert_eq!(3, live.occurrence_refs().unwrap().len());
        assert_eq!(
            HistoryEntryOrigin::Legacy {
                from_entry_index: 2,
                to_entry_index: 3
            },
            history.entries()[1].origin
        );
        assert_eq!("Later", manager.entries[3].label.as_deref().unwrap());
    }

    #[test]
    fn lazy_legacy_history_keeps_nearest_states_when_the_timeline_is_full() {
        let mut current = Document::new();
        current
            .add_cuboid("current", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let earlier = Document::new();
        let mut later = current.clone();
        later
            .add_cuboid("later", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let mut live = current.clone();
        let mut history = history(1, 1_000_000);
        history
            .append_legacy_state(&live, later, "Later", 30, 0, 1)
            .unwrap();
        assert_eq!(1, history.entries().len());
        history.redo(&mut live).unwrap();
        history.undo(&mut live).unwrap();
        history
            .prepend_legacy_state(&live, earlier, "Earlier", 10, 2, 0)
            .unwrap();
        assert_eq!(1, history.entries().len());
        assert!(history.can_undo());
        assert!(!history.can_redo());
        history.undo(&mut live).unwrap();
        assert!(live.occurrence_refs().unwrap().is_empty());
        history.redo(&mut live).unwrap();
        assert_eq!(1, live.occurrence_refs().unwrap().len());
    }

    #[test]
    fn lazy_legacy_history_prunes_farthest_states_to_respect_snapshot_budget() {
        let mut current = Document::new();
        current
            .add_cuboid("one", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let mut later = current.clone();
        later
            .add_cuboid("two", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let mut latest = later.clone();
        latest
            .add_cuboid("three", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let budget =
            serde_json::to_vec(&later).unwrap().len() + serde_json::to_vec(&latest).unwrap().len();
        let mut live = current.clone();
        let mut history = history(10, budget);
        history
            .append_legacy_state(&live, later.clone(), "Two", 2, 0, 1)
            .unwrap();
        history.redo(&mut live).unwrap();
        history
            .append_legacy_state(&live, latest, "Three", 3, 1, 2)
            .unwrap();

        assert_eq!(1, history.entries().len());
        assert_eq!("Three", history.entries()[0].label);
        assert!(!history.can_undo());
        assert!(history.can_redo());
        assert!(history.snapshot_bytes() <= budget);
        assert_eq!(2, live.occurrence_refs().unwrap().len());
        history.redo(&mut live).unwrap();
        assert_eq!(3, live.occurrence_refs().unwrap().len());
    }

    #[test]
    fn legacy_snapshot_budget_failure_does_not_add_a_partial_timeline_entry() {
        let mut current = Document::new();
        current
            .add_cuboid("current", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let mut earlier = Document::new();
        earlier
            .add_cuboid("earlier", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let current_size = serde_json::to_vec(&current).unwrap().len();
        let earlier_size = serde_json::to_vec(&earlier).unwrap().len();
        let live = current.clone();
        let mut history = history(4, current_size + earlier_size - 1);

        assert!(matches!(
            history.prepend_legacy_state(&live, earlier, "Earlier", 1, 0, 1),
            Err(TransactionError::HistoryBudgetExceeded { .. })
        ));
        assert!(history.entries().is_empty());
        assert_eq!(0, history.cursor());
        assert_eq!(current, live);
    }

    #[test]
    fn model_settings_are_document_history_but_session_selection_and_view_are_not() {
        let mut document = Document::new();
        let mut history = history(4, 1_000_000);
        let mut pending = history.begin(&document, "Change units");
        pending
            .stage(|draft| {
                draft.update_model_settings(ModelSettings {
                    unit_name: "meter".to_owned(),
                    ..draft.model_settings().clone()
                })?;
                Ok(())
            })
            .unwrap();
        let change = history.commit(&mut document, pending).unwrap().unwrap();
        assert!(change.kinds.contains(&ChangeKind::ModelSettings));
        history.undo(&mut document).unwrap();
        assert_eq!("unit", document.model_settings().unit_name);
    }
}
