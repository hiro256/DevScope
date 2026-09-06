use std::{
    error::Error,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentWork {
    parent_path: PathBuf,
    parent_task: String,
    items: Vec<CurrentWorkItem>,
}

impl CurrentWork {
    pub fn parent_path(&self) -> &Path {
        &self.parent_path
    }
    pub fn parent_task(&self) -> &str {
        &self.parent_task
    }
    pub fn items(&self) -> &[CurrentWorkItem] {
        &self.items
    }
    pub fn total(&self) -> usize {
        self.items.len()
    }
    pub fn completed(&self) -> usize {
        self.items.iter().filter(|item| item.completed).count()
    }
    pub fn first_incomplete(&self) -> Option<&CurrentWorkItem> {
        self.items.iter().find(|item| !item.completed)
    }
    pub fn remaining(&self) -> usize {
        self.total() - self.completed()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentWorkItem {
    completed: bool,
    text: String,
}
impl CurrentWorkItem {
    pub fn completed(&self) -> bool {
        self.completed
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentWorkDone {
    Completed { number: usize, text: String },
    AlreadyComplete { number: usize, text: String },
}

#[derive(Debug)]
pub enum CurrentWorkError {
    Read { path: PathBuf, source: io::Error },
    Write { path: PathBuf, source: io::Error },
    Format { message: String },
    NotSet,
    ItemDoesNotExist { number: usize },
}
impl fmt::Display for CurrentWorkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(
                f,
                "could not read Current Work file {}: {source}",
                path.display()
            ),
            Self::Write { path, source } => write!(
                f,
                "could not write Current Work file {}: {source}",
                path.display()
            ),
            Self::Format { message } => write!(f, "invalid Current Work file: {message}"),
            Self::NotSet => f.write_str("Current Work is not set"),
            Self::ItemDoesNotExist { number } => {
                write!(f, "Current Work item {number} does not exist")
            }
        }
    }
}
impl Error for CurrentWorkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            _ => None,
        }
    }
}

struct ParsedCurrentWork {
    work: CurrentWork,
    marker_offsets: Vec<usize>,
}

pub fn current_work_path(root: &Path) -> PathBuf {
    root.join(".devscope").join("work").join("current.md")
}

pub fn load_current_work(root: &Path) -> Result<Option<CurrentWork>, CurrentWorkError> {
    let path = current_work_path(root);
    match fs::read_to_string(&path) {
        Ok(text) => parse_current_work(&text).map(|parsed| Some(parsed.work)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CurrentWorkError::Read { path, source }),
    }
}

pub fn mark_current_work_done(
    root: &Path,
    number: usize,
) -> Result<CurrentWorkDone, CurrentWorkError> {
    let path = current_work_path(root);
    let text = fs::read_to_string(&path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            CurrentWorkError::NotSet
        } else {
            CurrentWorkError::Read {
                path: path.clone(),
                source,
            }
        }
    })?;
    let parsed = parse_current_work(&text)?;
    let index = number
        .checked_sub(1)
        .ok_or(CurrentWorkError::ItemDoesNotExist { number })?;
    let item = parsed
        .work
        .items
        .get(index)
        .ok_or(CurrentWorkError::ItemDoesNotExist { number })?;
    if item.completed {
        return Ok(CurrentWorkDone::AlreadyComplete {
            number,
            text: item.text.clone(),
        });
    }
    let offset = *parsed
        .marker_offsets
        .get(index)
        .ok_or(CurrentWorkError::ItemDoesNotExist { number })?;
    let mut updated = text;
    updated.replace_range(offset..offset + 1, "x");
    fs::write(&path, updated).map_err(|source| CurrentWorkError::Write { path, source })?;
    Ok(CurrentWorkDone::Completed {
        number,
        text: item.text.clone(),
    })
}

fn parse_current_work(text: &str) -> Result<ParsedCurrentWork, CurrentWorkError> {
    let mut header = false;
    let mut parent = None;
    let mut task = None;
    let mut items = Vec::new();
    let mut marker_offsets = Vec::new();
    let mut line_start = 0;
    for (index, raw_with_newline) in text.split_inclusive('\n').enumerate() {
        let raw_line = raw_with_newline.trim_end_matches(['\r', '\n']);
        let leading = raw_line.len() - raw_line.trim_start().len();
        let line = raw_line.trim();
        if line.is_empty() {
            line_start += raw_with_newline.len();
            continue;
        }
        if line == "# Current Work" {
            if header {
                return format_error("duplicate # Current Work header");
            }
            header = true;
        } else if let Some(value) = line.strip_prefix("Parent:") {
            if parent.is_some() {
                return format_error("duplicate Parent metadata");
            }
            let value = value.trim();
            if value.is_empty() {
                return format_error("Parent must not be empty");
            }
            let path = PathBuf::from(value);
            if path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return format_error("Parent must be a project-relative path");
            }
            parent = Some(path);
        } else if let Some(value) = line.strip_prefix("Task:") {
            if task.is_some() {
                return format_error("duplicate Task metadata");
            }
            let value = value.trim();
            if value.is_empty() {
                return format_error("Task must not be empty");
            }
            task = Some(value.to_owned());
        } else if line.starts_with('-') {
            let status = line.as_bytes().get(3).copied();
            let rest = line.get(5..);
            let Some(rest) = rest else {
                return format_error(&format!("malformed checkbox on line {}", index + 1));
            };
            if line.as_bytes().get(4) != Some(&b']')
                || !matches!(status, Some(b' ' | b'x' | b'X'))
                || !rest.starts_with(char::is_whitespace)
                || rest.trim().is_empty()
            {
                return format_error(&format!("malformed checkbox on line {}", index + 1));
            }
            items.push(CurrentWorkItem {
                completed: matches!(status, Some(b'x' | b'X')),
                text: rest.trim().to_owned(),
            });
            marker_offsets.push(line_start + leading + 3);
        } else {
            return format_error(&format!("unexpected content on line {}", index + 1));
        }
        line_start += raw_with_newline.len();
    }
    if !header {
        return format_error("missing # Current Work header");
    }
    let Some(parent_path) = parent else {
        return format_error("missing Parent metadata");
    };
    let Some(parent_task) = task else {
        return format_error("missing Task metadata");
    };
    Ok(ParsedCurrentWork {
        work: CurrentWork {
            parent_path,
            parent_task,
            items,
        },
        marker_offsets,
    })
}
fn format_error<T>(message: &str) -> Result<T, CurrentWorkError> {
    Err(CurrentWorkError::Format {
        message: message.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ID: AtomicUsize = AtomicUsize::new(0);
    fn valid() -> &'static str {
        "# Current Work\n\nParent: docs/roadmap.md\nTask: Current Work CLI experiment\n\n- [x] Done\n- [X] Also done\n- [ ] 日本語 item\n"
    }
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "devscope-current-work-{}",
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".devscope/work")).unwrap();
        root
    }
    fn write(root: &Path, text: &str) {
        fs::write(current_work_path(root), text).unwrap();
    }
    #[test]
    fn parses_valid_work_and_derived_counts() {
        let work = parse_current_work(valid()).unwrap().work;
        assert_eq!(
            (work.total(), work.completed(), work.remaining()),
            (3, 2, 1)
        );
        assert_eq!(work.items()[2].text(), "日本語 item");
    }
    #[test]
    fn accepts_zero_items() {
        assert_eq!(
            parse_current_work("# Current Work\nParent: docs/a.md\nTask: Empty\n")
                .unwrap()
                .work
                .total(),
            0
        );
    }
    #[test]
    fn rejects_empty_and_duplicate_metadata() {
        for text in [
            "# Current Work\nParent: \nTask: Task",
            "# Current Work\nParent: a.md\nTask: \n",
            "# Current Work\nParent: a.md\nParent: b.md\nTask: Task",
            "# Current Work\nParent: a.md\nTask: Task\nTask: Again",
        ] {
            assert!(matches!(
                parse_current_work(text),
                Err(CurrentWorkError::Format { .. })
            ));
        }
    }
    #[test]
    fn rejects_unsafe_parent_and_malformed_checkbox() {
        for text in [
            "# Current Work\nParent: ../outside.md\nTask: Task",
            "# Current Work\nParent: a.md\nTask: Task\n- [z] bad",
            "# Current Work\nParent: a.md\nTask: Task\n- bad",
        ] {
            assert!(matches!(
                parse_current_work(text),
                Err(CurrentWorkError::Format { .. })
            ));
        }
    }
    #[test]
    fn missing_file_is_not_set() {
        let root = std::env::temp_dir().join(format!(
            "devscope-current-work-missing-{}",
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        assert_eq!(load_current_work(&root).unwrap(), None);
        assert!(matches!(
            mark_current_work_done(&root, 1),
            Err(CurrentWorkError::NotSet)
        ));
    }
    #[test]
    fn marks_one_item_without_reformatting_other_text() {
        let root = root();
        let before = "# Current Work\r\n\r\nParent: docs/roadmap.md\r\nTask: Test\r\n\r\n- [ ] First  \r\n- [ ] 日本語 Second\r\n";
        write(&root, before);
        assert_eq!(
            mark_current_work_done(&root, 2).unwrap(),
            CurrentWorkDone::Completed {
                number: 2,
                text: "日本語 Second".to_owned()
            }
        );
        assert_eq!(
            fs::read_to_string(current_work_path(&root)).unwrap(),
            before.replacen("- [ ] 日本語", "- [x] 日本語", 1)
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn done_is_idempotent_and_range_and_malformed_do_not_write() {
        let root = root();
        let complete = "# Current Work\nParent: a.md\nTask: Test\n- [x] Done\n";
        write(&root, complete);
        assert!(matches!(
            mark_current_work_done(&root, 1).unwrap(),
            CurrentWorkDone::AlreadyComplete { .. }
        ));
        assert_eq!(
            fs::read_to_string(current_work_path(&root)).unwrap(),
            complete
        );
        assert!(matches!(
            mark_current_work_done(&root, 2),
            Err(CurrentWorkError::ItemDoesNotExist { .. })
        ));
        assert_eq!(
            fs::read_to_string(current_work_path(&root)).unwrap(),
            complete
        );
        let malformed = "# Current Work\nParent: a.md\nTask: Test\n- [z] Broken\n";
        write(&root, malformed);
        assert!(matches!(
            mark_current_work_done(&root, 1),
            Err(CurrentWorkError::Format { .. })
        ));
        assert_eq!(
            fs::read_to_string(current_work_path(&root)).unwrap(),
            malformed
        );
        let _ = fs::remove_dir_all(root);
    }
}
