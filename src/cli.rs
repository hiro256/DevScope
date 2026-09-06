use std::{
    ffi::{OsStr, OsString},
    path::Path,
};

use devscope::{
    progress::{ActivitySummary, is_cargo_project},
    project::{ActivityState, PlanState, ProjectSnapshot, TaskState, collect_markdown_state},
};

const CONTEXT_TASK_LIMIT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryMode {
    Tui,
    Context,
    TaskList,
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError {
    message: &'static str,
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message)
    }
}

pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<EntryMode, UsageError> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [] => Ok(EntryMode::Tui),
        [argument] if matches!(argument.as_os_str(), value if value == OsStr::new("context")) => {
            Ok(EntryMode::Context)
        }
        [first, second] if first == OsStr::new("task") && second == OsStr::new("list") => {
            Ok(EntryMode::TaskList)
        }
        [argument] if matches!(argument.as_os_str(), value if value == OsStr::new("-h") || value == OsStr::new("--help")) => {
            Ok(EntryMode::Help)
        }
        [argument] if matches!(argument.as_os_str(), value if value == OsStr::new("-V") || value == OsStr::new("--version")) => {
            Ok(EntryMode::Version)
        }
        [first, ..] if first == OsStr::new("task") => Err(UsageError {
            message: "expected `devscope task list`",
        }),
        _ => Err(UsageError {
            message: "unrecognized command or arguments",
        }),
    }
}

pub const fn usage() -> &'static str {
    "Usage:\n  devscope\n  devscope context\n  devscope task list\n  devscope --help\n  devscope --version\n"
}

pub fn render_context(root: &Path, snapshot: &ProjectSnapshot) -> String {
    let mut output = format!(
        "Project: {}\n{}\n{}\n{}\n{}\n",
        project_name(root),
        render_plan(snapshot),
        render_tasks_summary(snapshot),
        render_activity(snapshot),
        render_evidence(root),
    );

    if let TaskState::Available(tasks) = snapshot.tasks() {
        output.push_str("Next tasks:\n");
        append_tasks(
            &mut output,
            root,
            tasks.items().iter().take(CONTEXT_TASK_LIMIT),
        );
        if tasks.remaining() > CONTEXT_TASK_LIMIT {
            output.push_str(&format!(
                "... {} more\n",
                tasks.remaining() - CONTEXT_TASK_LIMIT
            ));
        }
    }

    output
}

/// Collects only the Markdown-derived task state required by `task list`.
pub fn collect_task_list_state(root: &Path) -> TaskState {
    collect_markdown_state(root)
        .map(|(_, tasks)| tasks)
        .unwrap_or(TaskState::Unavailable)
}

pub fn render_task_list(root: &Path, tasks: &TaskState) -> String {
    let TaskState::Available(tasks) = tasks else {
        return "Tasks: unavailable\n".to_owned();
    };

    let mut output = format!(
        "Tasks: {} remaining / {} total\n",
        tasks.remaining(),
        tasks.total()
    );
    append_tasks(&mut output, root, tasks.items().iter());
    output
}

fn render_plan(snapshot: &ProjectSnapshot) -> String {
    match snapshot.plan() {
        PlanState::Available(plan) => format!("Plan: {}/{}", plan.completed(), plan.total()),
        PlanState::Unavailable => "Plan: unavailable".to_owned(),
    }
}

fn render_tasks_summary(snapshot: &ProjectSnapshot) -> String {
    match snapshot.tasks() {
        TaskState::Available(tasks) => format!("Tasks: {} remaining", tasks.remaining()),
        TaskState::Unavailable => "Tasks: unavailable".to_owned(),
    }
}

fn render_activity(snapshot: &ProjectSnapshot) -> String {
    match snapshot.activity() {
        ActivityState::Available(activity) => format_activity(activity),
        ActivityState::NotRepository => "Activity: not a Git repository".to_owned(),
        ActivityState::Unavailable => "Activity: unavailable".to_owned(),
    }
}

fn format_activity(activity: &ActivitySummary) -> String {
    format!(
        "Activity: {} changed files, {} recent commits",
        activity.changed_files(),
        activity.recent_commits().len()
    )
}

fn render_evidence(root: &Path) -> &'static str {
    if is_cargo_project(root) {
        "Evidence: Cargo Build/Test available; run state not exposed by CLI"
    } else {
        "Evidence: Cargo Build/Test unavailable"
    }
}

fn append_tasks<'a>(
    output: &mut String,
    root: &Path,
    items: impl Iterator<Item = &'a devscope::progress::TaskSummaryItem>,
) {
    for item in items {
        output.push_str(&format!(
            "{}:{}  {}\n",
            display_path(root, item.path()),
            item.line(),
            item.text()
        ));
    }
}

fn project_name(root: &Path) -> String {
    root.file_name()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_owned())
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use devscope::{
        progress::{
            GitActivity, GitChangedFile, GitCommit, GitFileStatus, PlanSummary, TaskSummary,
            TaskSummaryItem,
        },
        project::{ActivityState, PlanState, TaskState},
    };
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static ID: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn parses_supported_entry_modes() {
        assert_eq!(parse_args([]), Ok(EntryMode::Tui));
        assert_eq!(
            parse_args([OsString::from("context")]),
            Ok(EntryMode::Context)
        );
        assert_eq!(
            parse_args([OsString::from("task"), OsString::from("list")]),
            Ok(EntryMode::TaskList)
        );
        for argument in ["-h", "--help"] {
            assert_eq!(parse_args([OsString::from(argument)]), Ok(EntryMode::Help));
        }
        for argument in ["-V", "--version"] {
            assert_eq!(
                parse_args([OsString::from(argument)]),
                Ok(EntryMode::Version)
            );
        }
    }

    #[test]
    fn rejects_unknown_and_incomplete_arguments() {
        for args in [
            vec![OsString::from("foo")],
            vec![OsString::from("task")],
            vec![OsString::from("task"), OsString::from("foo")],
            vec![OsString::from("context"), OsString::from("extra")],
        ] {
            assert!(parse_args(args).is_err());
        }
    }

    #[test]
    fn renders_context_with_relative_tasks_and_truncation() {
        let project = TempProject::new();
        fs::write(
            project.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let items = (1..=6)
            .map(|line| {
                TaskSummaryItem::new(
                    project.path().join("docs/tasks.md"),
                    line,
                    format!("Task {line}"),
                )
            })
            .collect();
        let snapshot = ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(1, 7)),
            ActivityState::NotRepository,
            TaskState::Available(TaskSummary::new(7, items)),
        );

        let output = render_context(project.path(), &snapshot);
        assert!(output.contains("Plan: 1/7"));
        assert!(output.contains("Tasks: 6 remaining"));
        assert!(output.contains("Activity: not a Git repository"));
        assert!(
            output.contains("Evidence: Cargo Build/Test available; run state not exposed by CLI")
        );
        assert!(!output.contains(&project.path().display().to_string()));
        assert!(output.contains("docs"));
        assert!(output.contains(":1  Task 1"));
        assert!(output.contains("... 1 more"));
        assert!(!output.contains("Task 6"));
    }

    #[test]
    fn renders_context_with_activity_and_unavailable_snapshot() {
        let root = Path::new("project");
        let activity = ActivitySummary::from(&GitActivity {
            changed_files: vec![GitChangedFile {
                path: PathBuf::from("a.txt"),
                status: GitFileStatus::Modified,
            }],
            recent_commits: vec![GitCommit {
                id: "abc".to_owned(),
                summary: "One".to_owned(),
            }],
        });
        let available = ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(0, 0)),
            ActivityState::Available(activity),
            TaskState::Available(TaskSummary::new(0, vec![])),
        );
        assert!(
            render_context(root, &available)
                .contains("Activity: 1 changed files, 1 recent commits")
        );

        let unavailable = render_context(root, &ProjectSnapshot::unavailable());
        assert!(unavailable.contains("Plan: unavailable"));
        assert!(unavailable.contains("Tasks: unavailable"));
        assert!(unavailable.contains("Activity: unavailable"));
        assert!(unavailable.contains("Evidence: Cargo Build/Test unavailable"));
    }

    #[test]
    fn task_list_includes_only_remaining_tasks() {
        let root = Path::new("project");
        let summary = TaskSummary::new(
            3,
            vec![
                TaskSummaryItem::new(
                    PathBuf::from("project").join("docs").join("a.md"),
                    2,
                    "First".to_owned(),
                ),
                TaskSummaryItem::new(
                    PathBuf::from("project").join("docs").join("a.md"),
                    4,
                    "Second".to_owned(),
                ),
            ],
        );
        let snapshot = ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(1, 3)),
            ActivityState::NotRepository,
            TaskState::Available(summary),
        );
        let output = render_task_list(root, snapshot.tasks());
        assert!(output.starts_with("Tasks: 2 remaining / 3 total\n"));
        assert!(
            output.contains(&(Path::new("docs").join("a.md").display().to_string() + ":2  First"))
        );
        assert!(
            output.contains(&(Path::new("docs").join("a.md").display().to_string() + ":4  Second"))
        );
    }

    #[test]
    fn task_list_excludes_completed_markdown_tasks() {
        let project = TempProject::new();
        fs::write(
            project.path().join("tasks.md"),
            "- [x] Completed task\n- [ ] Remaining task",
        )
        .unwrap();

        let output = render_task_list(project.path(), &collect_task_list_state(project.path()));
        assert!(output.contains("Tasks: 1 remaining / 2 total"));
        assert!(output.contains("Remaining task"));
        assert!(!output.contains("Completed task"));
    }
    #[test]
    fn task_list_handles_zero_and_unavailable_tasks() {
        let root = Path::new("project");
        let empty = ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(2, 2)),
            ActivityState::NotRepository,
            TaskState::Available(TaskSummary::new(2, vec![])),
        );
        assert_eq!(
            render_task_list(root, empty.tasks()),
            "Tasks: 0 remaining / 2 total\n"
        );
        assert_eq!(
            render_task_list(root, &TaskState::Unavailable),
            "Tasks: unavailable\n"
        );
    }

    struct TempProject {
        path: PathBuf,
    }

    impl TempProject {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "devscope-cli-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
