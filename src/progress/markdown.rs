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
    heading: Option<String>,
    context_start_line: usize,
    context: Vec<String>,
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
    pub fn heading(&self) -> Option<&str> {
        self.heading.as_deref()
    }
    pub const fn context_start_line(&self) -> usize {
        self.context_start_line
    }
    pub fn context(&self) -> &[String] {
        &self.context
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MarkdownProgress {
    root: PathBuf,
    tasks: Vec<MarkdownTask>,
}

impl MarkdownProgress {
    pub fn root(&self) -> &Path {
        &self.root
    }
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
    discover_markdown_files_with_exclusions(root, &[])
}

pub fn discover_markdown_files_with_exclusions(
    root: &Path,
    excludes: &[PathBuf],
) -> Result<Vec<PathBuf>, MarkdownProgressError> {
    discover_markdown_files_with_policy(root, None, excludes)
}

pub fn discover_markdown_files_with_policy(
    root: &Path,
    includes: Option<&[PathBuf]>,
    excludes: &[PathBuf],
) -> Result<Vec<PathBuf>, MarkdownProgressError> {
    if includes.is_some_and(<[PathBuf]>::is_empty) {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    discover(root, root, includes, excludes, &mut files)?;
    files.sort();
    Ok(files)
}

pub fn analyze_markdown_progress(root: &Path) -> Result<MarkdownProgress, MarkdownProgressError> {
    analyze_markdown_progress_with_exclusions(root, &[])
}

pub fn analyze_markdown_progress_with_exclusions(
    root: &Path,
    excludes: &[PathBuf],
) -> Result<MarkdownProgress, MarkdownProgressError> {
    analyze_markdown_progress_with_policy(root, None, excludes)
}

pub fn analyze_markdown_progress_with_policy(
    root: &Path,
    includes: Option<&[PathBuf]>,
    excludes: &[PathBuf],
) -> Result<MarkdownProgress, MarkdownProgressError> {
    let mut tasks = Vec::new();
    for path in discover_markdown_files_with_policy(root, includes, excludes)? {
        let content = fs::read_to_string(&path).map_err(|source| MarkdownProgressError {
            path: path.clone(),
            source,
        })?;
        tasks.extend(parse_tasks(&path, &content));
    }
    Ok(MarkdownProgress {
        root: root.to_path_buf(),
        tasks,
    })
}

fn discover(
    root: &Path,
    directory: &Path,
    includes: Option<&[PathBuf]>,
    excludes: &[PathBuf],
    files: &mut Vec<PathBuf>,
) -> Result<(), MarkdownProgressError> {
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
            if !is_excluded_path(root, &path, excludes)
                && can_contain_included_path(root, &path, includes)
            {
                discover(root, &path, includes, excludes, files)?;
            }
        } else if kind.is_file()
            && !is_excluded_path(root, &path, excludes)
            && is_included_path(root, &path, includes)
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn is_included_path(root: &Path, path: &Path, includes: Option<&[PathBuf]>) -> bool {
    let Some(includes) = includes else {
        return true;
    };
    path.strip_prefix(root).is_ok_and(|relative| {
        includes.iter().any(|include| {
            include == Path::new(".") || relative == include || relative.starts_with(include)
        })
    })
}

fn can_contain_included_path(root: &Path, path: &Path, includes: Option<&[PathBuf]>) -> bool {
    let Some(includes) = includes else {
        return true;
    };
    path.strip_prefix(root).is_ok_and(|relative| {
        includes.iter().any(|include| {
            include == Path::new(".")
                || relative.starts_with(include)
                || include.starts_with(relative)
        })
    })
}
fn is_excluded_path(root: &Path, path: &Path, configured: &[PathBuf]) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "target")
    ) || path.strip_prefix(root).is_ok_and(|relative| {
        relative == Path::new(".devscope").join("work")
            || configured
                .iter()
                .any(|excluded| relative == excluded || relative.starts_with(excluded))
    })
}
fn parse_tasks(path: &Path, content: &str) -> Vec<MarkdownTask> {
    let lines: Vec<_> = content.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            parse_task_line(line).map(|(completed, text)| MarkdownTask {
                path: path.to_path_buf(),
                line: index + 1,
                completed,
                text,
                heading: nearest_heading(&lines, index),
                context_start_line: index.saturating_sub(2) + 1,
                context: lines[index.saturating_sub(2)..(index + 3).min(lines.len())]
                    .iter()
                    .map(|line| (*line).to_owned())
                    .collect(),
            })
        })
        .collect()
}

fn nearest_heading(lines: &[&str], task_index: usize) -> Option<String> {
    lines[..=task_index].iter().rev().find_map(|line| {
        let heading = line.trim_start().strip_prefix('#')?;
        let heading = heading.trim_start_matches('#');
        heading
            .strip_prefix(char::is_whitespace)
            .map(str::trim)
            .filter(|heading| !heading.is_empty())
            .map(ToOwned::to_owned)
    })
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
    use super::{
        analyze_markdown_progress, discover_markdown_files,
        discover_markdown_files_with_exclusions, discover_markdown_files_with_policy, parse_tasks,
    };
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
    fn retains_nearest_heading_and_nearby_source_lines() {
        let tasks = parse_tasks(
            Path::new("tasks.md"),
            "# Plan\n\n## TUI\n- [x] Previous\n- [ ] Selected\n- [ ] Next\nAfter",
        );

        let selected = &tasks[1];
        assert_eq!(selected.heading(), Some("TUI"));
        assert_eq!(selected.context_start_line(), 3);
        assert_eq!(
            selected.context(),
            [
                "## TUI",
                "- [x] Previous",
                "- [ ] Selected",
                "- [ ] Next",
                "After"
            ]
        );
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
        project.write(".devscope/work/current.md", "- [ ] ignored");
        project.write("docs/work/current.md", "- [ ] ordinary");
        assert_eq!(
            discover_markdown_files(project.path()).unwrap(),
            vec![
                project.path().join("docs/nested.md"),
                project.path().join("docs/work/current.md"),
                project.path().join("root.md")
            ]
        );
    }

    #[test]
    fn configured_directory_exclude_keeps_unrelated_markdown() {
        let project = TempProject::new();
        project.write("root.md", "- [ ] root");
        project.write("docs/roadmap.md", "- [ ] roadmap");
        project.write("translations/ja/docs/roadmap.md", "- [ ] translated");

        assert_eq!(
            discover_markdown_files_with_exclusions(
                project.path(),
                &[PathBuf::from("translations")],
            )
            .unwrap(),
            vec![
                project.path().join("docs/roadmap.md"),
                project.path().join("root.md")
            ]
        );
    }

    #[test]
    fn configured_exact_file_exclude_keeps_neighboring_file() {
        let project = TempProject::new();
        project.write("notes/todo.md", "- [ ] todo");
        project.write("notes/keep.md", "- [ ] keep");

        assert_eq!(
            discover_markdown_files_with_exclusions(
                project.path(),
                &[PathBuf::from("notes/todo.md")],
            )
            .unwrap(),
            vec![project.path().join("notes/keep.md")]
        );
    }

    #[test]
    fn configured_exclude_uses_component_boundaries() {
        let project = TempProject::new();
        project.write("generated/tasks.md", "- [ ] generated");
        project.write("generated-old/tasks.md", "- [ ] retained");

        assert_eq!(
            discover_markdown_files_with_exclusions(project.path(), &[PathBuf::from("generated")],)
                .unwrap(),
            vec![project.path().join("generated-old/tasks.md")]
        );
    }

    #[test]
    fn configured_excludes_do_not_remove_mandatory_exclusions() {
        let project = TempProject::new();
        project.write("root.md", "- [ ] root");
        project.write(".git/ignored.md", "- [ ] ignored");
        project.write("target/ignored.md", "- [ ] ignored");
        project.write(".devscope/work/current.md", "- [ ] ignored");

        assert_eq!(
            discover_markdown_files_with_exclusions(project.path(), &[]).unwrap(),
            vec![project.path().join("root.md")]
        );
    }

    #[test]
    fn explicit_file_and_directory_sources_are_sorted_and_deduplicated() {
        let project = TempProject::new();
        project.write("docs/roadmap.md", "- [ ] roadmap");
        project.write("docs/plans/a.md", "- [ ] plan");
        project.write("docs/proposals/example.md", "- [ ] proposal");
        project.write("other.md", "- [ ] other");

        let includes = [
            PathBuf::from("docs/roadmap.md"),
            PathBuf::from("docs/plans"),
            PathBuf::from("docs"),
            PathBuf::from("docs/plans"),
        ];
        assert_eq!(
            discover_markdown_files_with_policy(
                project.path(),
                Some(&includes),
                &[PathBuf::from("docs/proposals")],
            )
            .unwrap(),
            vec![
                project.path().join("docs/plans/a.md"),
                project.path().join("docs/roadmap.md"),
            ]
        );
        assert_eq!(
            discover_markdown_files_with_policy(
                project.path(),
                Some(&[PathBuf::from("docs/roadmap.md")]),
                &[],
            )
            .unwrap(),
            vec![project.path().join("docs/roadmap.md")],
        );
        assert_eq!(
            discover_markdown_files_with_policy(
                project.path(),
                Some(&[PathBuf::from("docs/plans")]),
                &[],
            )
            .unwrap(),
            vec![project.path().join("docs/plans/a.md")],
        );
    }

    #[test]
    fn empty_include_is_empty_but_root_include_keeps_broad_mandatory_exclusions() {
        let project = TempProject::new();
        project.write("root.md", "- [ ] root");
        project.write(".git/ignored.md", "- [ ] git");
        project.write("target/ignored.md", "- [ ] target");
        project.write(".devscope/work/current.md", "- [ ] work");
        assert!(
            discover_markdown_files_with_policy(project.path(), Some(&[]), &[])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            discover_markdown_files_with_policy(project.path(), Some(&[PathBuf::from(".")]), &[],)
                .unwrap(),
            vec![project.path().join("root.md")],
        );
        assert_eq!(
            discover_markdown_files_with_policy(project.path(), None, &[]).unwrap(),
            vec![project.path().join("root.md")],
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
