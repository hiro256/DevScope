use std::time::Duration;

use crate::app::{ActivityState, App, PlanState, RefreshSource, TaskState};
use devscope::progress::{
    BuildTestFreshness, BuildTestKind, BuildTestOutcome, BuildTestResult, BuildTestState,
    BuildTestStatus, GitFileStatus,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

const COMPACT_WIDTH: u16 = 20;
const COMPACT_HEIGHT: u16 = 18;

#[derive(Clone, Copy)]
enum LayoutVariant {
    Large,
    Medium,
    Small,
}

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < COMPACT_WIDTH || area.height < COMPACT_HEIGHT {
        render_compact(frame, area);
        return;
    }

    let layout = if area.height >= 30 {
        LayoutVariant::Large
    } else if area.height >= 25 {
        LayoutVariant::Medium
    } else {
        LayoutVariant::Small
    };
    let (
        title_area,
        progress_area,
        task_area,
        details_area,
        changed_files_area,
        commits_area,
        footer_area,
    ) = match layout {
        LayoutVariant::Large => {
            let panels = Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Length(6),
                Constraint::Length(6),
                Constraint::Length(6),
                Constraint::Min(2),
                Constraint::Length(1),
            ])
            .split(area);
            (
                panels[0],
                panels[1],
                panels[2],
                panels[3],
                Some(panels[4]),
                Some(panels[5]),
                panels[6],
            )
        }
        LayoutVariant::Medium => {
            let panels = Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Length(6),
                Constraint::Length(6),
                Constraint::Min(4),
                Constraint::Length(1),
            ])
            .split(area);
            (
                panels[0],
                panels[1],
                panels[2],
                panels[3],
                Some(panels[4]),
                None,
                panels[5],
            )
        }
        LayoutVariant::Small => {
            let panels = Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Length(3),
                Constraint::Length(6),
                Constraint::Length(1),
            ])
            .split(area);
            (
                panels[0], panels[1], panels[2], panels[3], None, None, panels[4],
            )
        }
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from("DevScope").style(Style::default().add_modifier(Modifier::BOLD)),
            Line::from(refresh_status(app)),
        ]),
        title_area,
    );
    frame.render_widget(project_progress(app), progress_area);
    frame.render_widget(
        Paragraph::new(tasks(
            app.tasks(),
            app.selected_task(),
            inner_height(task_area),
        ))
        .block(Block::default().borders(Borders::ALL).title("Task Summary")),
        task_area,
    );
    frame.render_widget(
        Paragraph::new(evidence_details(app, inner_height(details_area))).block(
            Block::default()
                .borders(Borders::ALL)
                .title(evidence_detail_title(app)),
        ),
        details_area,
    );

    if let Some(changed_files_area) = changed_files_area {
        frame.render_widget(
            Paragraph::new(changed_files(
                app.activity(),
                inner_height(changed_files_area),
            ))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Changed Files"),
            ),
            changed_files_area,
        );
    }

    if let Some(commits_area) = commits_area {
        frame.render_widget(
            Paragraph::new(commits(app.activity(), inner_height(commits_area))).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Recent Commits"),
            ),
            commits_area,
        );
    }

    frame.render_widget(
        Paragraph::new("b:Build  t:Test  r:Reload  q/Esc:Quit"),
        footer_area,
    );
}
fn render_compact(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new("DevScope\nTerminal too small\nq / Esc: Quit"),
        area,
    );
}

fn refresh_status(app: &App) -> String {
    if let Some(error) = app.refresh_error() {
        return format!("Config error: {error}");
    }
    let status = app.refresh_status();
    let watching = if status.retry_pending() {
        "Retry pending"
    } else {
        "Watching"
    };
    format!(
        "{watching} · Last refresh: {} {}",
        refresh_source(status.last_source()),
        format_timestamp(status.last_update())
    )
}

fn refresh_source(source: RefreshSource) -> &'static str {
    match source {
        RefreshSource::Initial => "Initial",
        RefreshSource::Manual => "Manual",
        RefreshSource::Markdown => "Markdown",
        RefreshSource::Git => "Git",
        RefreshSource::MarkdownAndGit => "Markdown+Git",
    }
}

fn format_timestamp(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours == 0 {
        format!("+{minutes:02}:{seconds:02}")
    } else {
        format!("+{hours}:{minutes:02}:{seconds:02}")
    }
}
fn project_progress(app: &App) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(format!("Plan       {}", plan(app.plan()))),
        Line::from(format!("Activity   {}", activity(app.activity()))),
        Line::from(format!("Evidence   {}", evidence(app))),
        Line::from("Agent      Not available"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Project Progress"),
    )
}

fn evidence(app: &App) -> String {
    let build = app.build_test_state(BuildTestKind::Build);
    let test = app.build_test_state(BuildTestKind::Test);
    if matches!(build, BuildTestState::Unavailable) && matches!(test, BuildTestState::Unavailable) {
        return "Not available".into();
    }

    format!(
        "Build {} · Test {}",
        build_test_status(build),
        build_test_status(test)
    )
}

fn build_test_status(state: &BuildTestState) -> &'static str {
    match state.status() {
        BuildTestStatus::Unavailable => "Unavailable",
        BuildTestStatus::NotRun => "Not run",
        BuildTestStatus::Running => "Running",
        BuildTestStatus::Passed => "Passed",
        BuildTestStatus::Failed => "Failed",
        BuildTestStatus::Stale => "Stale",
        BuildTestStatus::ExecutionError => "Error",
    }
}

fn evidence_detail_title(app: &App) -> String {
    match app.evidence_detail_kind() {
        Some(BuildTestKind::Build) => "Details: Build".into(),
        Some(BuildTestKind::Test) => "Details: Test".into(),
        None => "Details: Evidence".into(),
    }
}

fn evidence_details(app: &App, rows: usize) -> Vec<Line<'static>> {
    if rows == 0 {
        return Vec::new();
    }

    let Some(kind) = app.evidence_detail_kind() else {
        let unavailable = matches!(
            app.build_test_state(BuildTestKind::Build),
            BuildTestState::Unavailable
        ) && matches!(
            app.build_test_state(BuildTestKind::Test),
            BuildTestState::Unavailable
        );
        return if unavailable {
            vec![Line::from("Build/Test Evidence is not available.")]
        } else {
            vec![
                Line::from("Build and Test have not been run yet."),
                Line::from("Press b to run Build or t to run Test."),
            ]
        };
    };

    evidence_detail_lines(kind, app.build_test_state(kind), rows)
}

fn evidence_detail_lines(
    kind: BuildTestKind,
    state: &BuildTestState,
    rows: usize,
) -> Vec<Line<'static>> {
    let lines = match state {
        BuildTestState::Unavailable => vec![Line::from("Unavailable")],
        BuildTestState::NotRun => vec![
            Line::from("Not run"),
            Line::from(format!(
                "Press {} to run {}.",
                detail_key(kind),
                detail_kind(kind)
            )),
        ],
        BuildTestState::Running(run) => vec![
            Line::from(run.command_label().to_owned()),
            Line::from("Running"),
        ],
        BuildTestState::Completed(result) => completed_detail_lines(result),
        BuildTestState::ExecutionError(error) => {
            let mut lines = vec![
                Line::from(error.command_label().to_owned()),
                Line::from("Error"),
            ];
            lines.extend(
                error
                    .message()
                    .lines()
                    .map(|line| Line::from(line.to_owned())),
            );
            lines
        }
    };
    lines.into_iter().take(rows).collect()
}

fn completed_detail_lines(result: &BuildTestResult) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(result.command_label().to_owned()),
        Line::from(completed_status(result)),
    ];
    if !result.summary().is_empty() {
        lines.push(Line::from(result.summary().to_owned()));
    }
    if let Some(diagnostic) = result.diagnostic() {
        lines.extend(
            diagnostic
                .as_str()
                .lines()
                .map(|line| Line::from(line.to_owned())),
        );
    }
    lines
}

fn completed_status(result: &BuildTestResult) -> String {
    let outcome = match result.outcome() {
        BuildTestOutcome::Passed => "Passed",
        BuildTestOutcome::Failed => "Failed",
    };
    let freshness = match result.freshness() {
        BuildTestFreshness::Fresh => "",
        BuildTestFreshness::Stale => " · Stale",
    };
    let exit = result
        .exit_code()
        .map(|code| format!(" · exit {code}"))
        .unwrap_or_default();
    format!(
        "{outcome}{freshness} · {}{exit}",
        format_duration(result.duration())
    )
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() >= 60 {
        return format!("{}m {}s", duration.as_secs() / 60, duration.as_secs() % 60);
    }
    if duration.as_secs() > 0 {
        return format!(
            "{}.{:01}s",
            duration.as_secs(),
            duration.subsec_millis() / 100
        );
    }
    format!("{}ms", duration.subsec_millis())
}

fn detail_kind(kind: BuildTestKind) -> &'static str {
    match kind {
        BuildTestKind::Build => "Build",
        BuildTestKind::Test => "Test",
    }
}

fn detail_key(kind: BuildTestKind) -> char {
    match kind {
        BuildTestKind::Build => 'b',
        BuildTestKind::Test => 't',
    }
}
fn inner_height(area: Rect) -> usize {
    usize::from(area.height.saturating_sub(2))
}

fn plan(plan: PlanState) -> String {
    match plan {
        PlanState::Available(summary) if summary.total() == 0 => "No tasks found".into(),
        PlanState::Available(summary) => {
            format!(
                "{} / {} tasks complete",
                summary.completed(),
                summary.total()
            )
        }
        PlanState::Unavailable => "Unavailable".into(),
    }
}

fn activity(activity: &ActivityState) -> String {
    match activity {
        ActivityState::Available(summary) if summary.changed_files() == 0 => "Clean".into(),
        ActivityState::Available(summary) => format!(
            "{} changed file{}",
            summary.changed_files(),
            if summary.changed_files() == 1 {
                ""
            } else {
                "s"
            }
        ),
        ActivityState::NotRepository => "Not a Git repository".into(),
        ActivityState::Unavailable => "Unavailable".into(),
    }
}

fn tasks(task_state: &TaskState, selected: Option<usize>, rows: usize) -> Vec<Line<'static>> {
    if rows == 0 {
        return vec![];
    }

    match task_state {
        TaskState::Unavailable => vec![Line::from("Unavailable")],
        TaskState::Available(summary) if summary.total() == 0 => vec![Line::from("No tasks found")],
        TaskState::Available(summary) if summary.remaining() == 0 => {
            vec![Line::from("All tasks completed")]
        }
        TaskState::Available(summary) => task_lines(summary, selected, rows),
    }
}

fn task_lines(
    summary: &devscope::progress::TaskSummary,
    selected: Option<usize>,
    rows: usize,
) -> Vec<Line<'static>> {
    let total = summary.remaining();
    let selected = selected.unwrap_or(0).min(total - 1);
    let item_rows = if total > rows {
        rows.saturating_sub(1).max(1)
    } else {
        rows
    };
    let start = selected
        .saturating_sub(item_rows - 1)
        .min(total.saturating_sub(item_rows));
    let end = (start + item_rows).min(total);
    let mut lines = summary.items()[start..end]
        .iter()
        .enumerate()
        .map(|(offset, item)| {
            let index = start + offset;
            Line::from(format!(
                "{} □ {}",
                if index == selected { ">" } else { " " },
                item.text()
            ))
        })
        .collect::<Vec<_>>();

    if end < total && lines.len() < rows {
        lines.push(Line::from(format!("... and {} more", total - end)));
    }
    lines
}

fn changed_files(activity: &ActivityState, rows: usize) -> Vec<Line<'static>> {
    if rows == 0 {
        return vec![];
    }

    match activity {
        ActivityState::Available(summary) if summary.changed_files() == 0 => {
            vec![Line::from("No changed files")]
        }
        ActivityState::Available(summary) => {
            let files = summary.changed_file_items();
            let file_rows = if files.len() > rows && rows > 1 {
                rows - 1
            } else {
                rows
            };
            let mut lines = files
                .iter()
                .take(file_rows)
                .map(|file| {
                    Line::from(format!(
                        "{}  {}",
                        git_file_status(&file.status),
                        file.path.display()
                    ))
                })
                .collect::<Vec<_>>();
            if files.len() > file_rows && rows > 1 {
                lines.push(Line::from(format!(
                    "... and {} more",
                    files.len() - file_rows
                )));
            }
            lines
        }
        ActivityState::NotRepository | ActivityState::Unavailable => {
            vec![Line::from("Unavailable")]
        }
    }
}

fn git_file_status(status: &GitFileStatus) -> &'static str {
    match status {
        GitFileStatus::Modified => "M",
        GitFileStatus::Added => "A",
        GitFileStatus::Deleted => "D",
        GitFileStatus::Renamed => "R",
    }
}
fn commits(activity: &ActivityState, rows: usize) -> Vec<Line<'static>> {
    if rows == 0 {
        return vec![];
    }

    match activity {
        ActivityState::Available(summary) if summary.recent_commits().is_empty() => {
            vec![Line::from("No commits yet")]
        }
        ActivityState::Available(summary) => summary
            .recent_commits()
            .iter()
            .take(rows)
            .map(|commit| Line::from(format!("{}  {}", commit.id, commit.summary)))
            .collect(),
        _ => vec![Line::from("Unavailable")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ActivityState, PlanState, TaskState};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use devscope::progress::{
        ActivitySummary, BuildTestDiagnostic, BuildTestExecutionError, BuildTestFreshness,
        BuildTestKind, BuildTestOutcome, BuildTestResult, BuildTestRun, BuildTestState,
        GitActivity, GitChangedFile, GitCommit, GitFileStatus, PlanSummary, TaskSummary,
        TaskSummaryItem,
    };
    use devscope::project::ProjectSnapshot;
    use ratatui::{Terminal, backend::TestBackend};

    fn app(tasks: TaskState, activity: ActivityState) -> App {
        App::new(ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(3, 5)),
            activity,
            tasks,
        ))
    }

    fn task_items(count: usize) -> Vec<TaskSummaryItem> {
        (0..count)
            .map(|index| TaskSummaryItem::new("a.md".into(), index + 1, format!("Task {index}")))
            .collect()
    }

    fn text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    fn draw(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        text(&terminal)
    }

    fn completed_state(
        kind: BuildTestKind,
        outcome: BuildTestOutcome,
        freshness: BuildTestFreshness,
    ) -> BuildTestState {
        BuildTestState::Completed(BuildTestResult::new(
            kind,
            outcome,
            freshness,
            "cargo",
            "cargo command",
            Some(1),
            Duration::from_secs(42),
            "detailed result summary",
            None,
        ))
    }

    fn execution_error_state(kind: BuildTestKind) -> BuildTestState {
        BuildTestState::ExecutionError(BuildTestExecutionError::new(
            kind,
            "cargo",
            "cargo command",
            "a detailed execution error",
        ))
    }

    #[test]
    fn renders_evidence_details_for_initial_running_and_completed_states() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(BuildTestKind::Build, BuildTestState::NotRun);
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        assert!(draw(&app, 80, 30).contains("Build and Test have not been run yet."));
        app.select_evidence_detail(BuildTestKind::Build);
        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Build,
                "hidden source",
                "cargo check",
            )),
        );
        let running = draw(&app, 80, 30);
        assert!(running.contains("Details: Build"));
        assert!(running.contains("cargo check"));
        assert!(running.contains("Running"));
        assert!(!running.contains("hidden source"));
        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Completed(BuildTestResult::new(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
                BuildTestFreshness::Fresh,
                "hidden source",
                "cargo check",
                Some(0),
                Duration::from_millis(850),
                "cargo check passed",
                None,
            )),
        );
        let passed = draw(&app, 80, 30);
        assert!(passed.contains("Passed · 850ms · exit 0"));
        assert!(passed.contains("cargo check passed"));
    }

    #[test]
    fn renders_failed_stale_and_error_evidence_details() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.select_evidence_detail(BuildTestKind::Test);
        app.apply_build_test_state(
            BuildTestKind::Test,
            BuildTestState::Completed(BuildTestResult::new(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
                BuildTestFreshness::Fresh,
                "hidden source",
                "cargo test",
                Some(101),
                Duration::from_millis(3400),
                "cargo test failed",
                Some(BuildTestDiagnostic::new(
                    "first diagnostic\nlast diagnostic",
                )),
            )),
        );
        let failed = draw(&app, 80, 30);
        assert!(failed.contains("Failed · 3.4s · exit 101"));
        assert!(failed.contains("cargo test failed"));
        assert!(failed.contains("first diagnostic"));
        app.apply_build_test_state(
            BuildTestKind::Test,
            completed_state(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
                BuildTestFreshness::Stale,
            ),
        );
        assert!(draw(&app, 80, 30).contains("Failed · Stale"));
        app.apply_build_test_state(
            BuildTestKind::Test,
            execution_error_state(BuildTestKind::Test),
        );
        assert!(draw(&app, 80, 30).contains("a detailed execution error"));
    }

    #[test]
    fn formats_evidence_durations() {
        assert_eq!(format_duration(Duration::from_millis(850)), "850ms");
        assert_eq!(format_duration(Duration::from_millis(1800)), "1.8s");
        assert_eq!(format_duration(Duration::from_secs(72)), "1m 12s");
    }
    #[test]
    fn renders_evidence_unavailable_when_both_states_are_unavailable() {
        let app = app(TaskState::Unavailable, ActivityState::Unavailable);
        let output = draw(&app, 80, 30);
        assert!(output.contains("Evidence   Not available"));
        assert!(!output.contains("Build Unavailable"));
    }

    #[test]
    fn renders_evidence_not_run_states() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(BuildTestKind::Build, BuildTestState::NotRun);
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        assert!(draw(&app, 80, 30).contains("Evidence   Build Not run · Test Not run"));
    }

    #[test]
    fn renders_evidence_running_state() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Build,
                "cargo",
                "cargo check",
            )),
        );
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        assert!(draw(&app, 80, 30).contains("Evidence   Build Running · Test Not run"));
    }

    #[test]
    fn renders_evidence_passed_and_failed_states() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(
            BuildTestKind::Build,
            completed_state(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
                BuildTestFreshness::Fresh,
            ),
        );
        app.apply_build_test_state(
            BuildTestKind::Test,
            completed_state(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
                BuildTestFreshness::Fresh,
            ),
        );
        assert!(draw(&app, 80, 30).contains("Evidence   Build Passed · Test Failed"));
    }

    #[test]
    fn renders_evidence_stale_and_mixed_states() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(
            BuildTestKind::Build,
            completed_state(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
                BuildTestFreshness::Stale,
            ),
        );
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::Unavailable);
        assert!(draw(&app, 80, 30).contains("Evidence   Build Stale · Test Unavailable"));
    }

    #[test]
    fn renders_evidence_execution_errors_without_details() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(
            BuildTestKind::Build,
            execution_error_state(BuildTestKind::Build),
        );
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        let output = draw(&app, 80, 30);
        assert!(output.contains("Evidence   Build Error · Test Not run"));
        assert!(!output.contains("a detailed execution error"));
        assert!(!output.contains("detailed result summary"));
    }

    #[test]
    fn renders_manual_evidence_controls_in_the_footer() {
        let app = app(TaskState::Unavailable, ActivityState::Unavailable);
        let output = draw(&app, 80, 30);
        assert!(output.contains("b:Build"));
        assert!(output.contains("t:Test"));
        assert!(output.contains("r:Reload"));
        assert!(output.contains("q/Esc:Quit"));
    }

    #[test]
    fn renders_tasks_and_overflow() {
        let app = app(
            TaskState::Available(TaskSummary::new(8, task_items(8))),
            ActivityState::Unavailable,
        );
        let output = draw(&app, 70, 30);
        assert!(output.contains("Task 0"));
        assert!(output.contains("... and 5 more"));
    }

    #[test]
    fn renders_empty_and_completed_tasks() {
        let empty = app(
            TaskState::Available(TaskSummary::new(0, vec![])),
            ActivityState::Unavailable,
        );
        assert!(draw(&empty, 70, 30).contains("No tasks found"));

        let completed = app(
            TaskState::Available(TaskSummary::new(1, vec![])),
            ActivityState::Unavailable,
        );
        assert!(draw(&completed, 70, 30).contains("All tasks completed"));
    }

    #[test]
    fn renders_plan_clean_and_recent_commit() {
        let activity = GitActivity {
            changed_files: vec![],
            recent_commits: vec![GitCommit {
                id: "abc".into(),
                summary: "newest".into(),
            }],
        };
        let app = app(
            TaskState::Available(TaskSummary::new(0, vec![])),
            ActivityState::Available(ActivitySummary::from(&activity)),
        );
        let output = draw(&app, 80, 30);
        assert!(output.contains("3 / 5 tasks complete"));
        assert!(output.contains("Activity   Clean"));
        assert!(output.contains("newest"));
    }

    #[test]
    fn renders_activity_unavailable_and_pluralization() {
        let unavailable = app(
            TaskState::Available(TaskSummary::new(0, vec![])),
            ActivityState::Unavailable,
        );
        assert!(draw(&unavailable, 80, 30).contains("Activity   Unavailable"));

        let one_file = GitActivity {
            changed_files: vec![GitChangedFile {
                path: "a".into(),
                status: GitFileStatus::Modified,
            }],
            recent_commits: vec![],
        };
        let one = app(
            TaskState::Available(TaskSummary::new(0, vec![])),
            ActivityState::Available(ActivitySummary::from(&one_file)),
        );
        assert!(draw(&one, 80, 30).contains("1 changed file"));

        let many_files = GitActivity {
            changed_files: vec![
                GitChangedFile {
                    path: "a".into(),
                    status: GitFileStatus::Modified,
                },
                GitChangedFile {
                    path: "b".into(),
                    status: GitFileStatus::Modified,
                },
            ],
            recent_commits: vec![],
        };
        let many = app(
            TaskState::Available(TaskSummary::new(0, vec![])),
            ActivityState::Available(ActivitySummary::from(&many_files)),
        );
        assert!(draw(&many, 80, 30).contains("2 changed files"));
    }

    #[test]
    fn renders_selection_and_moves_it() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(2, task_items(2))),
            ActivityState::Unavailable,
        );
        assert!(draw(&app, 80, 30).contains("> □ Task 0"));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let output = draw(&app, 80, 30);
        assert!(output.contains("  □ Task 0"));
        assert!(output.contains("> □ Task 1"));
    }

    #[test]
    fn renders_initial_refresh_status() {
        let app = app(
            TaskState::Available(TaskSummary::new(0, vec![])),
            ActivityState::Unavailable,
        );
        let output = draw(&app, 80, 30);
        assert!(output.contains("Watching"));
        assert!(output.contains("Last refresh"));
        assert!(output.contains("Initial"));
        assert!(output.contains("+00:00"));
    }

    #[test]
    fn renders_retry_pending_refresh_status() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(0, vec![])),
            ActivityState::Unavailable,
        );
        app.record_refresh(RefreshSource::Git, Duration::from_secs(65));
        app.set_refresh_pending(true);
        let output = draw(&app, 80, 30);
        assert!(output.contains("Retry pending"));
        assert!(output.contains("Git"));
        assert!(output.contains("+01:05"));
    }

    #[test]
    fn formats_session_relative_timestamps() {
        assert_eq!(format_timestamp(Duration::from_secs(0)), "+00:00");
        assert_eq!(format_timestamp(Duration::from_secs(7)), "+00:07");
        assert_eq!(format_timestamp(Duration::from_secs(65)), "+01:05");
        assert_eq!(format_timestamp(Duration::from_secs(3661)), "+1:01:01");
    }
    #[test]
    fn renders_without_panicking_at_small_sizes() {
        let app = app(
            TaskState::Available(TaskSummary::new(
                1,
                vec![TaskSummaryItem::new(
                    "a.md".into(),
                    1,
                    "狭い端末でも表示する".into(),
                )],
            )),
            ActivityState::Unavailable,
        );
        for (width, height) in [(80, 30), (40, 15), (30, 10), (20, 5), (10, 3), (1, 1)] {
            let _ = draw(&app, width, height);
        }
    }

    #[test]
    fn redraws_after_backend_resize() {
        let app = app(
            TaskState::Available(TaskSummary::new(8, task_items(8))),
            ActivityState::Unavailable,
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        terminal.backend_mut().resize(40, 15);
        terminal.draw(|frame| render(frame, &app)).unwrap();
        terminal.backend_mut().resize(20, 5);
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn keeps_selected_task_visible_after_resize() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(8, task_items(8))),
            ActivityState::Unavailable,
        );
        for _ in 0..7 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert!(draw(&app, 40, 18).contains("> □ Task 7"));
    }
    fn activity_with_files(files: Vec<GitChangedFile>) -> ActivityState {
        ActivityState::Available(ActivitySummary::from(&GitActivity {
            changed_files: files,
            recent_commits: vec![GitCommit {
                id: "abc".into(),
                summary: "recent".into(),
            }],
        }))
    }

    #[test]
    fn maps_git_file_statuses_to_short_prefixes() {
        assert_eq!(git_file_status(&GitFileStatus::Modified), "M");
        assert_eq!(git_file_status(&GitFileStatus::Added), "A");
        assert_eq!(git_file_status(&GitFileStatus::Deleted), "D");
        assert_eq!(git_file_status(&GitFileStatus::Renamed), "R");
    }

    #[test]
    fn renders_changed_files_and_clean_state() {
        let activity = activity_with_files(vec![
            GitChangedFile {
                path: "src/a.rs".into(),
                status: GitFileStatus::Modified,
            },
            GitChangedFile {
                path: "src/b.rs".into(),
                status: GitFileStatus::Added,
            },
            GitChangedFile {
                path: "docs/old.md".into(),
                status: GitFileStatus::Deleted,
            },
            GitChangedFile {
                path: "docs/new.md".into(),
                status: GitFileStatus::Renamed,
            },
        ]);
        let changed_app = app(TaskState::Unavailable, activity);
        let output = draw(&changed_app, 80, 30);
        assert!(output.contains("Changed Files"));
        assert!(output.contains("M  src/a.rs"));
        assert!(output.contains("A  src/b.rs"));
        assert!(output.contains("D  docs/old.md"));
        assert!(output.contains("R  docs/new.md"));

        let clean = app(TaskState::Unavailable, activity_with_files(vec![]));
        assert!(draw(&clean, 80, 30).contains("No changed files"));
    }

    #[test]
    fn renders_changed_file_overflow() {
        let files = (0..8)
            .map(|index| GitChangedFile {
                path: format!("src/file-{index}.rs").into(),
                status: GitFileStatus::Modified,
            })
            .collect();
        let app = app(TaskState::Unavailable, activity_with_files(files));
        let output = draw(&app, 80, 30);
        assert!(output.contains("M  src/file-0.rs"));
        assert!(output.contains("M  src/file-2.rs"));
        assert!(output.contains("... and 5 more"));
    }

    #[test]
    fn prioritizes_changed_files_across_responsive_layouts() {
        let app = app(
            TaskState::Available(TaskSummary::new(1, task_items(1))),
            activity_with_files(vec![GitChangedFile {
                path: "src/a.rs".into(),
                status: GitFileStatus::Modified,
            }]),
        );
        let large = draw(&app, 80, 30);
        assert!(large.contains("Changed Files"));
        assert!(large.contains("Recent Commits"));

        let medium = draw(&app, 80, 25);
        assert!(medium.contains("Changed Files"));
        assert!(!medium.contains("Recent Commits"));

        let small = draw(&app, 40, 18);
        assert!(small.contains("Task Summary"));
        assert!(!small.contains("Changed Files"));
        assert!(!small.contains("Recent Commits"));

        assert!(draw(&app, 19, 18).contains("Terminal too small"));
    }

    #[test]
    fn renders_changed_files_unavailable_when_activity_is_unavailable() {
        let app = app(TaskState::Unavailable, ActivityState::Unavailable);
        assert!(draw(&app, 80, 30).contains("Changed Files"));
        assert!(draw(&app, 80, 30).contains("Unavailable"));
    }
}
