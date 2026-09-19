use std::{
    ffi::{OsStr, OsString},
    path::Path,
};

use devscope::{
    config::load_project_config,
    current_work::{CurrentWork, CurrentWorkItem},
    progress::{ActivitySummary, BuildTestKind, resolve_build_test_command},
    project::{
        ActivityState, PlanState, ProjectCollectionError, ProjectSnapshot, TaskState,
        collect_markdown_state,
    },
};

const CONTEXT_TASK_LIMIT: usize = 5;
pub enum CurrentWorkContext<'a> {
    NotSet,
    Available(&'a CurrentWork),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryMode {
    Tui,
    Context,
    TaskList,
    WorkList,
    WorkHistory,
    WorkDone(usize),
    WorkActive(usize),
    WorkActiveClear,
    Verify(devscope::progress::BuildTestKind),
    ArtifactInspect(Option<OsString>),
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
        [first, second, argument] if first == OsStr::new("work") && second == OsStr::new("active") && argument == OsStr::new("clear") => Ok(EntryMode::WorkActiveClear),
        [first, second, number] if first == OsStr::new("work") && second == OsStr::new("active") => number
            .to_string_lossy()
            .parse::<usize>()
            .ok()
            .filter(|number| *number > 0)
            .map(EntryMode::WorkActive)
            .ok_or(UsageError { message: "expected `devscope work active <number>` or `devscope work active clear`" }),
        [first, second, number] if first == OsStr::new("work") && second == OsStr::new("done") => number
            .to_string_lossy()
            .parse::<usize>()
            .ok()
            .filter(|number| *number > 0)
            .map(EntryMode::WorkDone)
            .ok_or(UsageError { message: "expected `devscope work done <number>`" }),
        [first, second] if first == OsStr::new("work") && second == OsStr::new("list") => {
            Ok(EntryMode::WorkList)
        }
        [first, second] if first == OsStr::new("work") && second == OsStr::new("history") => {
            Ok(EntryMode::WorkHistory)
        }
        [first, second] if first == OsStr::new("artifact") && second == OsStr::new("inspect") => {
            Ok(EntryMode::ArtifactInspect(None))
        }
        [first, second, path]
            if first == OsStr::new("artifact") && second == OsStr::new("inspect") =>
        {
            Ok(EntryMode::ArtifactInspect(Some(path.clone())))
        }
        [first, ..] if first == OsStr::new("artifact") => Err(UsageError {
            message: "expected `devscope artifact inspect [path]`",
        }),
        [first, second] if first == OsStr::new("verify") => match second.to_string_lossy().as_ref()
        {
            "build" => Ok(EntryMode::Verify(devscope::progress::BuildTestKind::Build)),
            "test" => Ok(EntryMode::Verify(devscope::progress::BuildTestKind::Test)),
            _ => Err(UsageError {
                message: "expected `devscope verify build` or `devscope verify test`",
            }),
        },
        [first, ..] if first == OsStr::new("verify") => Err(UsageError {
            message: "expected `devscope verify build` or `devscope verify test`",
        }),
        [first, ..] if first == OsStr::new("task") => Err(UsageError {
            message: "expected `devscope task list`",
        }),
        [first, ..] if first == OsStr::new("work") => Err(UsageError {
            message: "expected `devscope work list`, `devscope work history`, `devscope work done <number>`, or `devscope work active <number>|clear`",
        }),
        _ => Err(UsageError {
            message: "unrecognized command or arguments",
        }),
    }
}

pub const fn usage() -> &'static str {
    "Usage:\n  devscope\n  devscope context\n  devscope task list\n  devscope work list\n  devscope work history\n  devscope work done <number>\n  devscope work active <number>\n  devscope work active clear\n  devscope verify build\n  devscope verify test\n  devscope artifact inspect [path]\n  devscope --help\n  devscope --version\n"
}

pub fn render_context(
    root: &Path,
    snapshot: &ProjectSnapshot,
    current_work: CurrentWorkContext<'_>,
) -> String {
    let mut output = format!(
        "Project: {}\n{}\n{}\n{}\n{}\n",
        project_name(root),
        render_plan(snapshot),
        render_tasks_summary(snapshot),
        render_activity(snapshot),
        render_evidence(root),
    );

    append_current_work_summary(&mut output, current_work);

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

fn append_current_work_summary(output: &mut String, current_work: CurrentWorkContext<'_>) {
    match current_work {
        CurrentWorkContext::NotSet => {}
        CurrentWorkContext::Unavailable => output.push_str("Current Work: unavailable\n"),
        CurrentWorkContext::Available(work) => {
            output.push_str(&format!(
                "Current Work: {}/{}\n",
                work.completed(),
                work.total()
            ));
            output.push_str(&format!("Parent: {}\n", work.parent_task()));
            let active = work.active_item().map_or("none", CurrentWorkItem::text);
            output.push_str(&format!("Active: {active}\n"));
            let next = work
                .first_incomplete()
                .map_or("none", CurrentWorkItem::text);
            output.push_str(&format!("Next: {next}\n"));
        }
    }
}
/// Collects only the Markdown-derived task state required by `task list`.
pub fn collect_task_list_state(root: &Path) -> Result<TaskState, ProjectCollectionError> {
    collect_markdown_state(root).map(|(_, tasks)| tasks)
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

pub const fn render_current_work_not_set() -> &'static str {
    "Current Work: not set\n"
}

pub fn render_work_list(work: &CurrentWork) -> String {
    let mut output = format!(
        "Parent: {} :: {}\nWork: {}/{} complete\n",
        work.parent_path().display(),
        work.parent_task(),
        work.completed(),
        work.total()
    );
    if let Some(active) = work.active_index() {
        output.push_str(&format!("Active: {}\n", active + 1));
    }
    for (index, item) in work.items().iter().enumerate() {
        let marker = if item.completed() { "x" } else { " " };
        let active = if work.active_index() == Some(index) {
            "  [active]"
        } else {
            ""
        };
        output.push_str(&format!(
            "{}. [{marker}] {}{active}\n",
            index + 1,
            item.text()
        ));
    }
    output
}
pub fn render_work_active(result: &devscope::current_work::CurrentWorkActiveUpdate) -> String {
    match result {
        devscope::current_work::CurrentWorkActiveUpdate::Set { number, text } => {
            format!("Active work item {number}: {text}\n")
        }
        devscope::current_work::CurrentWorkActiveUpdate::Cleared {
            previous: Some((number, text)),
        } => {
            format!("Cleared active work item {number}: {text}\n")
        }
        devscope::current_work::CurrentWorkActiveUpdate::Cleared { previous: None } => {
            "Active work item is already clear\n".to_owned()
        }
    }
}
pub fn render_work_done(result: &devscope::current_work::CurrentWorkDone) -> String {
    match result {
        devscope::current_work::CurrentWorkDone::Completed { number, text } => {
            format!("Completed work item {number}: {text}\n")
        }
        devscope::current_work::CurrentWorkDone::AlreadyComplete { number, text } => {
            format!("Work item {number} is already complete: {text}\n")
        }
    }
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

fn render_evidence(root: &Path) -> String {
    let config = load_project_config(root).unwrap_or_default();
    let build_available = resolve_build_test_command(root, &config, BuildTestKind::Build).is_some();
    let test_available = resolve_build_test_command(root, &config, BuildTestKind::Test).is_some();
    match (build_available, test_available) {
        (true, true) => "Evidence: Build/Test available; run state not exposed by CLI",
        (true, false) => "Evidence: Build available; Test unavailable",
        (false, true) => "Evidence: Build unavailable; Test available",
        (false, false) => "Evidence: Build/Test unavailable",
    }
    .to_owned()
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

pub fn render_verify(completion: &devscope::progress::BuildTestExecutionCompletion) -> String {
    use devscope::progress::{BuildTestExecutionCompletion, BuildTestOutcome};
    match completion {
        BuildTestExecutionCompletion::Completed(result) => format!(
            "{}\nStatus: {}\nCommand: {}\nDuration: {}\n{}",
            match result.kind() {
                devscope::progress::BuildTestKind::Build => "Build",
                devscope::progress::BuildTestKind::Test => "Test",
            },
            match result.outcome() {
                BuildTestOutcome::Passed => "Passed",
                BuildTestOutcome::Failed => "Failed",
            },
            result.command_label(),
            format_duration(result.duration()),
            if result.outcome() == BuildTestOutcome::Failed {
                format!("Result: {}\n", result.summary())
            } else {
                String::new()
            },
        ),
        BuildTestExecutionCompletion::ExecutionError(error) => format!(
            "{}\nStatus: Execution error\nCommand: {}\nError: {}\n",
            match error.kind() {
                devscope::progress::BuildTestKind::Build => "Build",
                devscope::progress::BuildTestKind::Test => "Test",
            },
            error.command_label(),
            error.message()
        ),
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    if duration.as_secs() > 0 {
        format!(
            "{}.{:01}s",
            duration.as_secs(),
            duration.subsec_millis() / 100
        )
    } else {
        format!("{}ms", duration.as_millis())
    }
}

pub fn render_artifact(observation: &devscope::progress::ArtifactObservation) -> String {
    use devscope::progress::{ArtifactKind, ArtifactStatus};
    let mut output = format!("Artifact\nPath: {}\n", observation.path().display());
    match observation.status() {
        ArtifactStatus::Exists { kind, size } => {
            let kind = match kind {
                ArtifactKind::File => "File",
                ArtifactKind::Directory => "Directory",
                ArtifactKind::Other => "Other",
            };
            output.push_str(&format!(
                "Status: Exists\nKind: {kind}\nSize: {size} bytes\n"
            ));
        }
        ArtifactStatus::Missing => output.push_str("Status: Missing\n"),
        ArtifactStatus::ObservationError { message } => {
            output.push_str(&format!("Status: Observation error\nError: {message}\n"))
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use devscope::{
        current_work::load_current_work,
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
    fn parses_verify_build_test_and_rejects_other_targets() {
        assert_eq!(
            parse_args([OsString::from("verify"), OsString::from("build")]),
            Ok(EntryMode::Verify(devscope::progress::BuildTestKind::Build))
        );
        assert_eq!(
            parse_args([OsString::from("verify"), OsString::from("test")]),
            Ok(EntryMode::Verify(devscope::progress::BuildTestKind::Test))
        );
        assert!(parse_args([OsString::from("verify"), OsString::from("lint")]).is_err());
    }

    #[test]
    fn renders_verify_completion_and_error() {
        use devscope::progress::{
            BuildTestExecutionCompletion, BuildTestExecutionError, BuildTestFreshness,
            BuildTestOutcome, BuildTestResult,
        };
        let passed = BuildTestExecutionCompletion::Completed(BuildTestResult::new(
            devscope::progress::BuildTestKind::Build,
            BuildTestOutcome::Passed,
            BuildTestFreshness::Fresh,
            "cargo",
            "cargo check",
            Some(0),
            std::time::Duration::from_millis(1200),
            "passed",
            None,
        ));
        assert!(render_verify(&passed).contains("Build\nStatus: Passed\nCommand: cargo check"));
        let error = BuildTestExecutionCompletion::ExecutionError(BuildTestExecutionError::new(
            devscope::progress::BuildTestKind::Test,
            "cargo",
            "cargo test",
            "not found",
        ));
        assert!(render_verify(&error).contains("Status: Execution error"));
    }
    #[test]
    fn parses_artifact_inspect_modes_and_rejects_malformed_commands() {
        assert_eq!(
            parse_args([OsString::from("artifact"), OsString::from("inspect")]),
            Ok(EntryMode::ArtifactInspect(None))
        );
        assert_eq!(
            parse_args([
                OsString::from("artifact"),
                OsString::from("inspect"),
                OsString::from("foo")
            ]),
            Ok(EntryMode::ArtifactInspect(Some(OsString::from("foo"))))
        );
        assert!(
            parse_args(
                ["artifact", "inspect", "foo", "bar"]
                    .into_iter()
                    .map(OsString::from)
            )
            .is_err()
        );
        assert!(parse_args(["artifact"].into_iter().map(OsString::from)).is_err());
        assert!(parse_args(["artifact", "foo"].into_iter().map(OsString::from)).is_err());
        assert!(usage().contains("devscope artifact inspect [path]"));
    }
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

        let output = render_context(project.path(), &snapshot, CurrentWorkContext::NotSet);
        assert!(output.contains("Plan: 1/7"));
        assert!(output.contains("Tasks: 6 remaining"));
        assert!(output.contains("Activity: not a Git repository"));
        assert!(output.contains("Evidence: Build/Test available; run state not exposed by CLI"));
        assert!(!output.contains(&project.path().display().to_string()));
        assert!(output.contains("docs"));
        assert!(output.contains(":1  Task 1"));
        assert!(output.contains("... 1 more"));
        assert!(!output.contains("Task 6"));
        assert!(!output.contains("Current Work:"));
    }

    #[test]
    fn renders_context_with_activity_and_unavailable_snapshot() {
        let root = Path::new("project");
        let activity = ActivitySummary::from(&GitActivity {
            changed_files: vec![GitChangedFile {
                path: PathBuf::from("a.txt"),
                status: GitFileStatus::Modified,
                changes: Default::default(),
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
            render_context(root, &available, CurrentWorkContext::NotSet)
                .contains("Activity: 1 changed files, 1 recent commits")
        );

        let unavailable = render_context(
            root,
            &ProjectSnapshot::unavailable(),
            CurrentWorkContext::NotSet,
        );
        assert!(unavailable.contains("Plan: unavailable"));
        assert!(unavailable.contains("Tasks: unavailable"));
        assert!(unavailable.contains("Activity: unavailable"));
        assert!(unavailable.contains("Evidence: Build/Test unavailable"));
    }

    #[test]
    fn renders_configured_evidence_availability_per_kind() {
        let project = TempProject::new();
        fs::create_dir_all(project.path().join(".devscope")).unwrap();
        fs::write(
            project.path().join(".devscope/config.toml"),
            "[verify.test]\nprogram = \"configured-test\"\n",
        )
        .unwrap();

        let output = render_context(
            project.path(),
            &ProjectSnapshot::unavailable(),
            CurrentWorkContext::NotSet,
        );

        assert!(output.contains("Evidence: Build unavailable; Test available"));
        assert!(!output.contains("Cargo Build/Test"));
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

        let output = render_task_list(
            project.path(),
            &collect_task_list_state(project.path()).unwrap(),
        );
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

    #[test]
    fn parses_work_list_and_rejects_other_work_syntax() {
        assert_eq!(
            parse_args([OsString::from("work"), OsString::from("list")]),
            Ok(EntryMode::WorkList)
        );
        assert_eq!(
            parse_args([OsString::from("work"), OsString::from("history")]),
            Ok(EntryMode::WorkHistory)
        );
        for args in [
            vec![OsString::from("work")],
            vec![OsString::from("work"), OsString::from("foo")],
            vec![
                OsString::from("work"),
                OsString::from("list"),
                OsString::from("extra"),
            ],
        ] {
            assert!(parse_args(args).is_err());
        }
    }

    #[test]
    fn renders_work_list_and_not_set() {
        let project = TempProject::new();
        assert_eq!(load_current_work(project.path()).unwrap(), None);
        assert_eq!(render_current_work_not_set(), "Current Work: not set\n");
        let path = project.path().join(".devscope/work/current.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            "# Current Work\nParent: docs/roadmap.md\nTask: Current Work CLI experiment\n- [x] Storage\n- [ ] Dogfood\n",
        )
        .unwrap();
        let work = load_current_work(project.path()).unwrap().unwrap();
        assert_eq!(
            render_work_list(&work),
            "Parent: docs/roadmap.md :: Current Work CLI experiment\nWork: 1/2 complete\n1. [x] Storage\n2. [ ] Dogfood\n"
        );
    }
    #[test]
    fn parses_work_done_and_rejects_invalid_numbers() {
        assert_eq!(
            parse_args([
                OsString::from("work"),
                OsString::from("done"),
                OsString::from("3")
            ]),
            Ok(EntryMode::WorkDone(3))
        );
        for args in [
            vec![OsString::from("work"), OsString::from("done")],
            vec![
                OsString::from("work"),
                OsString::from("done"),
                OsString::from("abc"),
            ],
            vec![
                OsString::from("work"),
                OsString::from("done"),
                OsString::from("0"),
            ],
            vec![
                OsString::from("work"),
                OsString::from("done"),
                OsString::from("3"),
                OsString::from("extra"),
            ],
        ] {
            assert!(parse_args(args).is_err());
        }
    }
    #[test]
    fn parses_active_work_modes_and_renders_active_separately_from_next() {
        assert_eq!(
            parse_args([
                OsString::from("work"),
                OsString::from("active"),
                OsString::from("2")
            ]),
            Ok(EntryMode::WorkActive(2))
        );
        assert_eq!(
            parse_args([
                OsString::from("work"),
                OsString::from("active"),
                OsString::from("clear")
            ]),
            Ok(EntryMode::WorkActiveClear)
        );
        assert!(
            parse_args([
                OsString::from("work"),
                OsString::from("active"),
                OsString::from("0")
            ])
            .is_err()
        );
        assert!(usage().contains("devscope work active <number>"));

        let project = TempProject::new();
        let path = project.path().join(".devscope/work/current.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "# Current Work\nParent: docs/roadmap.md\nTask: Parent\nActive: 2\n- [ ] Next item\n- [ ] Active item\n").unwrap();
        let work = load_current_work(project.path()).unwrap().unwrap();
        let context = render_context(
            project.path(),
            &ProjectSnapshot::unavailable(),
            CurrentWorkContext::Available(&work),
        );
        assert!(context.contains("Active: Active item\nNext: Next item\n"));
        assert!(
            render_work_list(&work)
                .contains("Active: 2\n1. [ ] Next item\n2. [ ] Active item  [active]\n")
        );
        assert!(
            render_work_active(&devscope::current_work::CurrentWorkActiveUpdate::Cleared {
                previous: None
            })
            .contains("already clear")
        );
    }
    #[test]
    fn renders_compact_current_work_context_states() {
        let project = TempProject::new();
        let path = project.path().join(".devscope/work/current.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "# Current Work\nParent: docs/roadmap.md\nTask: 日本語 Parent\n- [x] Done\n- [ ] 次の作業\n",
        )
        .unwrap();
        let work = load_current_work(project.path()).unwrap().unwrap();
        let output = render_context(
            project.path(),
            &ProjectSnapshot::unavailable(),
            CurrentWorkContext::Available(&work),
        );
        assert!(
            output.contains(
                "Current Work: 1/2\nParent: 日本語 Parent\nActive: none\nNext: 次の作業\n"
            )
        );
        assert!(!output.contains("1. [x] Done"));
        assert!(!output.contains(".devscope/work/current.md"));

        fs::write(&path, "# Current Work\nParent: a.md\nTask: Empty\n").unwrap();
        let empty = load_current_work(project.path()).unwrap().unwrap();
        let empty_output = render_context(
            project.path(),
            &ProjectSnapshot::unavailable(),
            CurrentWorkContext::Available(&empty),
        );
        assert!(
            empty_output.contains("Current Work: 0/0\nParent: Empty\nActive: none\nNext: none\n")
        );

        let unavailable = render_context(
            project.path(),
            &ProjectSnapshot::unavailable(),
            CurrentWorkContext::Unavailable,
        );
        assert!(unavailable.contains("Current Work: unavailable\n"));
        assert!(unavailable.contains("Plan: unavailable"));
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
