use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownTask {
    path: PathBuf,
    line: usize,
    completed: bool,
    text: String,
}

impl MarkdownTask {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub const fn line(&self) -> usize {
        self.line
    }
    pub const fn completed(&self) -> bool {
        self.completed
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MarkdownProgress {
    tasks: Vec<MarkdownTask>,
}

impl MarkdownProgress {
    pub fn tasks(&self) -> &[MarkdownTask] {
        &self.tasks
    }
    pub fn total_tasks(&self) -> usize {
        self.tasks.len()
    }
    pub fn completed_tasks(&self) -> usize {
        self.tasks.iter().filter(|task| task.completed).count()
    }
    pub fn remaining_tasks(&self) -> usize {
        self.total_tasks() - self.completed_tasks()
    }
}

#[derive(Debug)]
pub struct MarkdownProgressError {
    path: PathBuf,
    source: io::Error,
}

impl fmt::Display for MarkdownProgressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "could not read {}", self.path.display())
    }
}
impl Error for MarkdownProgressError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

pub fn discover_markdown_files(root: &Path) -> Result<Vec<PathBuf>, MarkdownProgressError> {
    let mut files = Vec::new();
    discover(root, &mut files)?;
    files.sort();
    Ok(files)
}

pub fn analyze_markdown_progress(root: &Path) -> Result<MarkdownProgress, MarkdownProgressError> {
    let mut tasks = Vec::new();
    for path in discover_markdown_files(root)? {
        let content = fs::read_to_string(&path).map_err(|source| MarkdownProgressError {
            path: path.clone(),
            source,
        })?;
        tasks.extend(parse_tasks(&path, &content));
    }
    Ok(MarkdownProgress { tasks })
}

fn discover(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), MarkdownProgressError> {
    let entries = fs::read_dir(directory).map_err(|source| MarkdownProgressError {
        path: directory.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| MarkdownProgressError {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|source| MarkdownProgressError {
            path: path.clone(),
            source,
        })?;
        if kind.is_dir() {
            if !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".git" | "target")
            ) {
                discover(&path, files)?;
            }
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn parse_tasks(path: &Path, content: &str) -> Vec<MarkdownTask> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            parse_task_line(line).map(|(completed, text)| MarkdownTask {
                path: path.to_path_buf(),
                line: index + 1,
                completed,
                text,
            })
        })
        .collect()
}

fn parse_task_line(line: &str) -> Option<(bool, String)> {
    let line = line.trim_start();
    let marker = line.chars().next()?;
    if !matches!(marker, '-' | '*' | '+') {
        return None;
    }
    let after_marker = line.get(marker.len_utf8()..)?;
    if !after_marker.starts_with(char::is_whitespace) {
        return None;
    }
    let checkbox = after_marker.trim_start();
    let bytes = checkbox.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'[' || bytes[2] != b']' {
        return None;
    }
    let completed = match bytes[1] {
        b' ' => false,
        b'x' | b'X' => true,
        _ => return None,
    };
    Some((completed, checkbox[3..].trim_start().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{analyze_markdown_progress, discover_markdown_files, parse_tasks};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };
    static ID: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn parses_checkbox_markers_and_states() {
        let tasks = parse_tasks(
            Path::new("tasks.md"),
            "- [ ] incomplete\n- [x] complete\n- [X] complete\n* [ ] incomplete\n+ [x] complete",
        );
        assert_eq!(tasks.len(), 5);
        assert_eq!(tasks.iter().filter(|task| task.completed()).count(), 3);
        assert_eq!(tasks[0].text(), "incomplete");
        assert_eq!(tasks[4].line(), 5);
    }

    #[test]
    fn parses_indented_tasks_and_ignores_prose() {
        let tasks = parse_tasks(
            Path::new("tasks.md"),
            "- [ ] parent\n  - [x] child\nThis contains [x] but is not a task.",
        );
        assert_eq!(tasks.len(), 2);
        assert!(!tasks[0].completed());
        assert!(tasks[1].completed());
    }

    #[test]
    fn aggregates_multiple_files() {
        let project = TempProject::new();
        project.write("a.md", "- [ ] first");
        project.write("b.md", "- [x] second\n- [ ] third");
        let progress = analyze_markdown_progress(project.path()).unwrap();
        assert_eq!(
            (
                progress.completed_tasks(),
                progress.total_tasks(),
                progress.remaining_tasks()
            ),
            (1, 3, 2)
        );
        assert_eq!(progress.tasks()[0].path(), project.path().join("a.md"));
    }

    #[test]
    fn skips_git_and_target_directories() {
        let project = TempProject::new();
        project.write("root.md", "- [ ] root");
        project.write("docs/nested.md", "- [ ] nested");
        project.write(".git/ignored.md", "- [ ] ignored");
        project.write("target/ignored.md", "- [ ] ignored");
        assert_eq!(
            discover_markdown_files(project.path()).unwrap(),
            vec![
                project.path().join("docs/nested.md"),
                project.path().join("root.md")
            ]
        );
    }

    struct TempProject {
        path: PathBuf,
    }
    impl TempProject {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "devscope-md-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
        fn path(&self) -> &Path {
            &self.path
        }
        fn write(&self, relative: &str, content: &str) {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }
    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
