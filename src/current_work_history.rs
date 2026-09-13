//! Source-specific, supplemental history for explicit Current Work mutations.

use std::{
    error::Error,
    fmt, fs,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

use crate::current_work::{CurrentWork, CurrentWorkActiveUpdate, CurrentWorkDone, CurrentWorkItem};

const HISTORY_VERSION: u8 = 1;

#[derive(Debug)]
pub enum CurrentWorkHistoryError {
    Read { path: PathBuf, source: io::Error },
    Write { path: PathBuf, source: io::Error },
    Serialize(serde_json::Error),
    Timestamp(time::error::Format),
    LocalOffset(time::error::IndeterminateOffset),
    Invalid { line: usize, message: String },
}

impl fmt::Display for CurrentWorkHistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(
                formatter,
                "could not read Current Work history {}: {source}",
                path.display()
            ),
            Self::Write { path, source } => write!(
                formatter,
                "could not write Current Work history {}: {source}",
                path.display()
            ),
            Self::Serialize(source) => write!(
                formatter,
                "could not serialize Current Work history: {source}"
            ),
            Self::Timestamp(source) => write!(
                formatter,
                "could not format Current Work history timestamp: {source}"
            ),
            Self::LocalOffset(source) => write!(
                formatter,
                "could not determine local offset for Current Work history: {source}"
            ),
            Self::Invalid { line, message } => write!(
                formatter,
                "invalid Current Work history at line {line}: {message}"
            ),
        }
    }
}

impl Error for CurrentWorkHistoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Serialize(source) => Some(source),
            Self::Timestamp(source) => Some(source),
            Self::LocalOffset(source) => Some(source),
            Self::Invalid { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemReference {
    number: usize,
    text: String,
}

impl ItemReference {
    fn from_item(number: usize, item: &CurrentWorkItem) -> Self {
        Self {
            number,
            text: item.text().to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurrentWorkHistoryEvent {
    ActiveSet {
        version: u8,
        timestamp: String,
        parent: String,
        task: String,
        item: ItemReference,
    },
    ActiveChanged {
        version: u8,
        timestamp: String,
        parent: String,
        task: String,
        from: ItemReference,
        to: ItemReference,
    },
    ActiveCleared {
        version: u8,
        timestamp: String,
        parent: String,
        task: String,
        item: ItemReference,
    },
    WorkCompleted {
        version: u8,
        timestamp: String,
        parent: String,
        task: String,
        item: ItemReference,
        cleared_active: bool,
    },
}

impl CurrentWorkHistoryEvent {
    fn active_set(
        work: &CurrentWork,
        item: ItemReference,
    ) -> Result<Self, CurrentWorkHistoryError> {
        Ok(Self::ActiveSet {
            version: HISTORY_VERSION,
            timestamp: current_timestamp()?,
            parent: work.parent_path().display().to_string(),
            task: work.parent_task().to_owned(),
            item,
        })
    }

    fn active_changed(
        work: &CurrentWork,
        from: ItemReference,
        to: ItemReference,
    ) -> Result<Self, CurrentWorkHistoryError> {
        Ok(Self::ActiveChanged {
            version: HISTORY_VERSION,
            timestamp: current_timestamp()?,
            parent: work.parent_path().display().to_string(),
            task: work.parent_task().to_owned(),
            from,
            to,
        })
    }

    fn active_cleared(
        work: &CurrentWork,
        item: ItemReference,
    ) -> Result<Self, CurrentWorkHistoryError> {
        Ok(Self::ActiveCleared {
            version: HISTORY_VERSION,
            timestamp: current_timestamp()?,
            parent: work.parent_path().display().to_string(),
            task: work.parent_task().to_owned(),
            item,
        })
    }

    fn work_completed(
        work: &CurrentWork,
        item: ItemReference,
        cleared_active: bool,
    ) -> Result<Self, CurrentWorkHistoryError> {
        Ok(Self::WorkCompleted {
            version: HISTORY_VERSION,
            timestamp: current_timestamp()?,
            parent: work.parent_path().display().to_string(),
            task: work.parent_task().to_owned(),
            item,
            cleared_active,
        })
    }

    pub fn timestamp(&self) -> &str {
        match self {
            Self::ActiveSet { timestamp, .. }
            | Self::ActiveChanged { timestamp, .. }
            | Self::ActiveCleared { timestamp, .. }
            | Self::WorkCompleted { timestamp, .. } => timestamp,
        }
    }

    fn context(&self) -> (&str, &str) {
        match self {
            Self::ActiveSet { parent, task, .. }
            | Self::ActiveChanged { parent, task, .. }
            | Self::ActiveCleared { parent, task, .. }
            | Self::WorkCompleted { parent, task, .. } => (parent, task),
        }
    }

    fn line(&self) -> String {
        let time = OffsetDateTime::parse(self.timestamp(), &Rfc3339)
            .expect("Current Work history timestamps are validated before rendering")
            .format(time::macros::format_description!("[hour]:[minute]"))
            .expect("hour and minute formatting is infallible");
        match self {
            Self::ActiveSet { item, .. } => {
                format!("{time} Active set      #{} {}", item.number, item.text)
            }
            Self::ActiveChanged { from, to, .. } => format!(
                "{time} Active changed  #{} {} -> #{} {}",
                from.number, from.text, to.number, to.text
            ),
            Self::ActiveCleared { item, .. } => {
                format!("{time} Active cleared  #{} {}", item.number, item.text)
            }
            Self::WorkCompleted {
                item,
                cleared_active,
                ..
            } => {
                let suffix = if *cleared_active {
                    "; Active cleared"
                } else {
                    ""
                };
                format!(
                    "{time} Completed       #{} {}{suffix}",
                    item.number, item.text
                )
            }
        }
    }
}

#[derive(Deserialize)]
struct EventHeader {
    version: u8,
    kind: String,
}

pub fn current_work_history_path(root: &Path) -> PathBuf {
    root.join(".devscope")
        .join("history")
        .join("current-work.jsonl")
}

/// Appends an event only after the Current Work mutation already succeeded.
pub fn record_active_update(
    root: &Path,
    before: &CurrentWork,
    result: &CurrentWorkActiveUpdate,
) -> Result<(), CurrentWorkHistoryError> {
    let event = match result {
        CurrentWorkActiveUpdate::Set { number, .. }
            if before.active_index() == Some(number - 1) =>
        {
            return Ok(());
        }
        CurrentWorkActiveUpdate::Set { number, .. } => {
            let index = number - 1;
            let to = ItemReference::from_item(*number, &before.items()[index]);
            match before.active_index() {
                None => CurrentWorkHistoryEvent::active_set(before, to)?,
                Some(previous) => CurrentWorkHistoryEvent::active_changed(
                    before,
                    ItemReference::from_item(previous + 1, &before.items()[previous]),
                    to,
                )?,
            }
        }
        CurrentWorkActiveUpdate::Cleared {
            previous: Some((number, _)),
        } => CurrentWorkHistoryEvent::active_cleared(
            before,
            ItemReference::from_item(*number, &before.items()[number - 1]),
        )?,
        CurrentWorkActiveUpdate::Cleared { previous: None } => return Ok(()),
    };
    append_current_work_history(root, &event)
}

/// Appends a completion event only for an actual unchecked-to-checked mutation.
pub fn record_completion(
    root: &Path,
    before: &CurrentWork,
    result: &CurrentWorkDone,
) -> Result<(), CurrentWorkHistoryError> {
    let CurrentWorkDone::Completed { number, .. } = result else {
        return Ok(());
    };
    let index = number - 1;
    let event = CurrentWorkHistoryEvent::work_completed(
        before,
        ItemReference::from_item(*number, &before.items()[index]),
        before.active_index() == Some(index),
    )?;
    append_current_work_history(root, &event)
}

fn append_current_work_history(
    root: &Path,
    event: &CurrentWorkHistoryEvent,
) -> Result<(), CurrentWorkHistoryError> {
    let path = current_work_history_path(root);
    let parent = path
        .parent()
        .expect("Current Work history path has a parent");
    fs::create_dir_all(parent).map_err(|source| CurrentWorkHistoryError::Write {
        path: parent.to_path_buf(),
        source,
    })?;
    let serialized = serde_json::to_string(event).map_err(CurrentWorkHistoryError::Serialize)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|source| CurrentWorkHistoryError::Write {
            path: path.clone(),
            source,
        })?;
    writeln!(file, "{serialized}").map_err(|source| CurrentWorkHistoryError::Write { path, source })
}

/// Reads local Current Work history newest first for session-resumption use.
pub fn read_current_work_history(
    root: &Path,
) -> Result<Vec<CurrentWorkHistoryEvent>, CurrentWorkHistoryError> {
    let path = current_work_history_path(root);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(CurrentWorkHistoryError::Read { path, source }),
    };
    let mut events = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let header: EventHeader =
            serde_json::from_str(line).map_err(|source| CurrentWorkHistoryError::Invalid {
                line: line_number,
                message: source.to_string(),
            })?;
        if header.version != HISTORY_VERSION {
            return Err(CurrentWorkHistoryError::Invalid {
                line: line_number,
                message: format!("unsupported version {}", header.version),
            });
        }
        if !matches!(
            header.kind.as_str(),
            "active_set" | "active_changed" | "active_cleared" | "work_completed"
        ) {
            return Err(CurrentWorkHistoryError::Invalid {
                line: line_number,
                message: format!("unknown event kind {}", header.kind),
            });
        }
        let event: CurrentWorkHistoryEvent =
            serde_json::from_str(line).map_err(|source| CurrentWorkHistoryError::Invalid {
                line: line_number,
                message: source.to_string(),
            })?;
        OffsetDateTime::parse(event.timestamp(), &Rfc3339).map_err(|source| {
            CurrentWorkHistoryError::Invalid {
                line: line_number,
                message: format!("invalid RFC 3339 timestamp: {source}"),
            }
        })?;
        events.push(event);
    }
    events.reverse();
    Ok(events)
}

pub fn render_current_work_history(events: &[CurrentWorkHistoryEvent]) -> String {
    if events.is_empty() {
        return "Current Work history: empty\n".to_owned();
    }
    let mut output = String::from("Current Work history:\n");
    let mut previous_context = None;
    for event in events {
        let context = event.context();
        if previous_context != Some(context) {
            if previous_context.is_some() {
                output.push('\n');
            }
            output.push_str(context.0);
            output.push_str(" > ");
            output.push_str(context.1);
            output.push('\n');
            previous_context = Some(context);
        }
        output.push_str("  ");
        output.push_str(&event.line());
        output.push('\n');
    }
    output
}

fn current_timestamp() -> Result<String, CurrentWorkHistoryError> {
    let offset = UtcOffset::current_local_offset().map_err(CurrentWorkHistoryError::LocalOffset)?;
    OffsetDateTime::now_utc()
        .to_offset(offset)
        .format(time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"
        ))
        .map_err(CurrentWorkHistoryError::Timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::current_work::{
        clear_current_work_active, load_current_work, mark_current_work_done,
        set_current_work_active,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ID: AtomicUsize = AtomicUsize::new(0);

    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "devscope-current-work-history-{}",
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".devscope/work")).unwrap();
        root
    }

    fn write_work(root: &Path, active: Option<usize>) {
        let active = active.map_or_else(String::new, |number| format!("Active: {number}\n"));
        fs::write(
            root.join(".devscope/work/current.md"),
            format!(
                "# Current Work\nParent: docs/roadmap.md\nTask: Progress history experiment\n{active}- [ ] First\n- [ ] Second\n- [x] Done\n"
            ),
        )
        .unwrap();
    }

    fn before(root: &Path) -> CurrentWork {
        load_current_work(root).unwrap().unwrap()
    }
    fn rendered_event(
        parent: &str,
        task: &str,
        number: usize,
        text: &str,
    ) -> CurrentWorkHistoryEvent {
        CurrentWorkHistoryEvent::ActiveSet {
            version: HISTORY_VERSION,
            timestamp: "2026-09-13T16:00:00+09:00".to_owned(),
            parent: parent.to_owned(),
            task: task.to_owned(),
            item: ItemReference {
                number,
                text: text.to_owned(),
            },
        }
    }

    #[test]
    fn records_active_set_with_parent_task_number_text_and_offset_timestamp() {
        let root = root();
        write_work(&root, None);
        let work = before(&root);
        let result = set_current_work_active(&root, 2).unwrap();
        record_active_update(&root, &work, &result).unwrap();

        let events = read_current_work_history(&root).unwrap();
        assert!(
            matches!(events.as_slice(), [CurrentWorkHistoryEvent::ActiveSet { parent, task, item, .. }] if parent == "docs/roadmap.md" && task == "Progress history experiment" && item.number == 2 && item.text == "Second")
        );
        assert!(matches!(events[0].timestamp().as_bytes()[19], b'+' | b'-'));
        OffsetDateTime::parse(events[0].timestamp(), &Rfc3339).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn records_active_changed_with_from_and_to() {
        let root = root();
        write_work(&root, Some(1));
        let work = before(&root);
        let result = set_current_work_active(&root, 2).unwrap();
        record_active_update(&root, &work, &result).unwrap();
        assert!(
            matches!(read_current_work_history(&root).unwrap().as_slice(), [CurrentWorkHistoryEvent::ActiveChanged { from, to, .. }] if from.number == 1 && from.text == "First" && to.number == 2 && to.text == "Second")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn suppresses_same_active_and_clear_no_op_history() {
        let root = root();
        write_work(&root, Some(2));
        let work = before(&root);
        let result = set_current_work_active(&root, 2).unwrap();
        record_active_update(&root, &work, &result).unwrap();
        assert!(!current_work_history_path(&root).exists());
        let result = clear_current_work_active(&root).unwrap();
        let work = before(&root);
        record_active_update(&root, &work, &result).unwrap();
        let result = clear_current_work_active(&root).unwrap();
        record_active_update(&root, &before(&root), &result).unwrap();
        assert_eq!(read_current_work_history(&root).unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn records_active_clear_only_when_state_changed() {
        let root = root();
        write_work(&root, Some(2));
        let work = before(&root);
        let result = clear_current_work_active(&root).unwrap();
        record_active_update(&root, &work, &result).unwrap();
        assert!(
            matches!(read_current_work_history(&root).unwrap().as_slice(), [CurrentWorkHistoryEvent::ActiveCleared { item, .. }] if item.number == 2)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn records_active_completion_as_one_event_and_preserves_non_active_state() {
        let active_root = root();
        write_work(&active_root, Some(2));
        let work = before(&active_root);
        let result = mark_current_work_done(&active_root, 2).unwrap();
        record_completion(&active_root, &work, &result).unwrap();
        assert!(matches!(
            read_current_work_history(&active_root).unwrap().as_slice(),
            [CurrentWorkHistoryEvent::WorkCompleted {
                cleared_active: true,
                ..
            }]
        ));
        assert!(before(&active_root).active_index().is_none());

        let non_active_root = root();
        write_work(&non_active_root, Some(2));
        let work = before(&non_active_root);
        let result = mark_current_work_done(&non_active_root, 1).unwrap();
        record_completion(&non_active_root, &work, &result).unwrap();
        assert!(matches!(
            read_current_work_history(&non_active_root)
                .unwrap()
                .as_slice(),
            [CurrentWorkHistoryEvent::WorkCompleted {
                cleared_active: false,
                ..
            }]
        ));
        assert_eq!(before(&non_active_root).active_index(), Some(1));
        let _ = fs::remove_dir_all(non_active_root);
        let _ = fs::remove_dir_all(active_root);
    }

    #[test]
    fn suppresses_already_complete_and_failed_operation_history() {
        let root = root();
        write_work(&root, None);
        let work = before(&root);
        let done = mark_current_work_done(&root, 3).unwrap();
        record_completion(&root, &work, &done).unwrap();
        assert!(!current_work_history_path(&root).exists());
        assert!(set_current_work_active(&root, 3).is_err());
        assert!(!current_work_history_path(&root).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn append_failure_does_not_roll_back_current_work_mutation() {
        let root = root();
        write_work(&root, None);
        let work = before(&root);
        let result = set_current_work_active(&root, 1).unwrap();
        fs::create_dir_all(current_work_history_path(&root)).unwrap();
        assert!(record_active_update(&root, &work, &result).is_err());
        assert_eq!(before(&root).active_index(), Some(0));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_newest_first_and_renders_compact_entries() {
        let root = root();
        write_work(&root, None);
        let work = before(&root);
        let first = set_current_work_active(&root, 1).unwrap();
        record_active_update(&root, &work, &first).unwrap();
        let work = before(&root);
        let second = set_current_work_active(&root, 2).unwrap();
        record_active_update(&root, &work, &second).unwrap();
        let events = read_current_work_history(&root).unwrap();
        assert!(matches!(
            events[0],
            CurrentWorkHistoryEvent::ActiveChanged { .. }
        ));
        let output = render_current_work_history(&events);
        assert!(output.contains("Active changed  #1 First -> #2 Second"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn renders_one_context_header_for_adjacent_events_with_the_same_parent_and_task() {
        let output = render_current_work_history(&[
            rendered_event("docs/a.md", "Task A", 2, "Second"),
            rendered_event("docs/a.md", "Task A", 1, "First"),
        ]);
        assert_eq!(output.matches("docs/a.md > Task A").count(), 1);
        assert!(output.contains("  16:00 Active set      #2 Second\n"));
        assert!(output.contains("  16:00 Active set      #1 First\n"));
    }

    #[test]
    fn renders_context_switches_with_blank_lines_in_newest_first_order() {
        let output = render_current_work_history(&[
            rendered_event("docs/a.md", "Task A", 4, "Newest A"),
            rendered_event("docs/a.md", "Task A", 3, "Older A"),
            rendered_event("docs/b.md", "Task B", 2, "Newer B"),
            rendered_event("docs/b.md", "Task B", 1, "Older B"),
        ]);
        let newest = output.find("#4 Newest A").unwrap();
        let older = output.find("#3 Older A").unwrap();
        let newer_b = output.find("#2 Newer B").unwrap();
        let older_b = output.find("#1 Older B").unwrap();
        assert!(newest < older && older < newer_b && newer_b < older_b);
        assert_eq!(output.matches("docs/a.md > Task A").count(), 1);
        assert_eq!(output.matches("docs/b.md > Task B").count(), 1);
        assert!(output.contains("#3 Older A\n\ndocs/b.md > Task B"));
    }

    #[test]
    fn renders_a_reappearing_context_as_a_new_timeline_group() {
        let output = render_current_work_history(&[
            rendered_event("docs/a.md", "Task A", 4, "Newest A"),
            rendered_event("docs/a.md", "Task A", 3, "Older A"),
            rendered_event("docs/b.md", "Task B", 2, "B"),
            rendered_event("docs/a.md", "Task A", 1, "Oldest A"),
        ]);
        assert_eq!(output.matches("docs/a.md > Task A").count(), 2);
        assert_eq!(output.matches("docs/b.md > Task B").count(), 1);
        assert!(output.contains("#2 B\n\ndocs/a.md > Task A"));
    }
    #[test]
    fn treats_a_parent_or_task_change_as_a_distinct_context_group() {
        let output = render_current_work_history(&[
            rendered_event("docs/a.md", "Shared task", 1, "First"),
            rendered_event("docs/b.md", "Shared task", 2, "Second"),
            rendered_event("docs/b.md", "Other task", 3, "Third"),
        ]);
        assert_eq!(output.matches("docs/a.md > Shared task").count(), 1);
        assert_eq!(output.matches("docs/b.md > Shared task").count(), 1);
        assert_eq!(output.matches("docs/b.md > Other task").count(), 1);
    }

    #[test]
    fn preserves_existing_event_text_inside_context_groups() {
        let event = CurrentWorkHistoryEvent::WorkCompleted {
            version: HISTORY_VERSION,
            timestamp: "2026-09-13T16:00:00+09:00".to_owned(),
            parent: "docs/a.md".to_owned(),
            task: "Task A".to_owned(),
            item: ItemReference {
                number: 3,
                text: "Completed item".to_owned(),
            },
            cleared_active: true,
        };
        let output = render_current_work_history(&[event]);
        assert!(output.contains("  16:00 Completed       #3 Completed item; Active cleared"));
    }
    #[test]
    fn rejects_malformed_unknown_version_and_unknown_kind_history_lines() {
        for line in [
            "not json",
            r#"{"version":2,"kind":"active_set"}"#,
            r#"{"version":1,"kind":"future"}"#,
        ] {
            let root = root();
            fs::create_dir_all(root.join(".devscope/history")).unwrap();
            fs::write(current_work_history_path(&root), format!("{line}\n")).unwrap();
            assert!(matches!(
                read_current_work_history(&root),
                Err(CurrentWorkHistoryError::Invalid { line: 1, .. })
            ));
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn empty_history_is_successful() {
        let root = root();
        assert!(read_current_work_history(&root).unwrap().is_empty());
        assert_eq!(
            render_current_work_history(&[]),
            "Current Work history: empty\n"
        );
        let _ = fs::remove_dir_all(root);
    }
}
