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

#[derive(Debug)]
pub enum CurrentWorkError {
    Read { path: PathBuf, source: io::Error },
    Format { message: String },
}
impl fmt::Display for CurrentWorkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(
                f,
                "could not read Current Work file {}: {source}",
                path.display()
            ),
            Self::Format { message } => write!(f, "invalid Current Work file: {message}"),
        }
    }
}
impl Error for CurrentWorkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Format { .. } => None,
        }
    }
}

pub fn current_work_path(root: &Path) -> PathBuf {
    root.join(".devscope").join("work").join("current.md")
}

pub fn load_current_work(root: &Path) -> Result<Option<CurrentWork>, CurrentWorkError> {
    let path = current_work_path(root);
    match fs::read_to_string(&path) {
        Ok(text) => parse_current_work(&text).map(Some),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CurrentWorkError::Read { path, source }),
    }
}

fn parse_current_work(text: &str) -> Result<CurrentWork, CurrentWorkError> {
    let mut header = false;
    let mut parent = None;
    let mut task = None;
    let mut items = Vec::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
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
            if line.as_bytes().get(4) != Some(&b']') {
                return format_error(&format!("malformed checkbox on line {}", index + 1));
            }
            if !matches!(status, Some(b' ' | b'x' | b'X'))
                || !rest.starts_with(char::is_whitespace)
                || rest.trim().is_empty()
            {
                return format_error(&format!("malformed checkbox on line {}", index + 1));
            }
            items.push(CurrentWorkItem {
                completed: matches!(status, Some(b'x' | b'X')),
                text: rest.trim().to_owned(),
            });
        } else {
            return format_error(&format!("unexpected content on line {}", index + 1));
        }
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
    Ok(CurrentWork {
        parent_path,
        parent_task,
        items,
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
    #[test]
    fn parses_valid_work_and_derived_counts() {
        let work = parse_current_work(valid()).unwrap();
        assert_eq!(work.total(), 3);
        assert_eq!(work.completed(), 2);
        assert_eq!(work.remaining(), 1);
        assert_eq!(work.items()[2].text(), "日本語 item");
    }
    #[test]
    fn accepts_zero_items() {
        let work = parse_current_work("# Current Work\nParent: docs/a.md\nTask: Empty\n").unwrap();
        assert_eq!(work.total(), 0);
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
            "devscope-current-work-{}",
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        assert_eq!(load_current_work(&root).unwrap(), None);
    }
}
