use std::time::Duration;

use crate::app::{
    ActivityState, App, CurrentWorkState, DetailTarget, FocusedPanel, PlanState, RefreshSource,
    TaskState,
};
use devscope::progress::{
    BuildTestFreshness, BuildTestKind, BuildTestOutcome, BuildTestResult, BuildTestState,
    BuildTestStatus, GitChangeCounts, GitFileStatus,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph},
};

const COMPACT_WIDTH: u16 = 20;
const COMPACT_HEIGHT: u16 = 18;

#[derive(Clone, Copy)]
enum LayoutVariant {
    Large,
    Medium,
    Small,
}

pub fn focusable_panels(width: u16, height: u16) -> &'static [FocusedPanel] {
    match layout_variant(width, height) {
        Some(LayoutVariant::Large | LayoutVariant::Medium) => &[
            FocusedPanel::Tasks,
            FocusedPanel::Evidence,
            FocusedPanel::ChangedFiles,
        ],
        Some(LayoutVariant::Small) => &[FocusedPanel::Tasks, FocusedPanel::Evidence],
        None => &[],
    }
}

fn layout_variant(width: u16, height: u16) -> Option<LayoutVariant> {
    if width < COMPACT_WIDTH || height < COMPACT_HEIGHT {
        None
    } else if height >= 30 {
        Some(LayoutVariant::Large)
    } else if height >= 25 {
        Some(LayoutVariant::Medium)
    } else {
        Some(LayoutVariant::Small)
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if let Some(target) = app.detail_target() {
        render_detail(frame, area, target);
        return;
    }
    let Some(layout) = layout_variant(area.width, area.height) else {
        render_compact(frame, area);
        return;
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
    frame.render_widget(project_progress(app, progress_area.width), progress_area);
    frame.render_widget(
        Paragraph::new(tasks(
            app.tasks(),
            app.selected_task(),
            inner_height(task_area),
        ))
        .block(panel_block(
            "Task Summary",
            app.focused_panel() == FocusedPanel::Tasks,
        )),
        task_area,
    );
    frame.render_widget(
        Paragraph::new(evidence_details(app, inner_height(details_area))).block(panel_block(
            evidence_detail_title(app),
            app.focused_panel() == FocusedPanel::Evidence,
        )),
        details_area,
    );

    if let Some(changed_files_area) = changed_files_area {
        frame.render_widget(
            Paragraph::new(changed_files(
                app.activity(),
                app.selected_changed_file(),
                inner_height(changed_files_area),
                inner_width(changed_files_area),
            ))
            .block(panel_block(
                "Changed Files",
                app.focused_panel() == FocusedPanel::ChangedFiles,
            )),
            changed_files_area,
        );
    }

    if let Some(commits_area) = commits_area {
        frame.render_widget(
            Paragraph::new(commits(app.activity(), inner_height(commits_area))).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(padded_title("Recent Commits")),
            ),
            commits_area,
        );
    }

    frame.render_widget(
        Paragraph::new("Tab:Panel  j/k:Move  b:Build  t:Test  r:Reload  q/Esc:Quit"),
        footer_area,
    );
}
fn render_detail(frame: &mut Frame, area: Rect, target: &DetailTarget) {
    let areas = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let DetailTarget::ChangedFile {
        path,
        status,
        changes,
    } = target;
    frame.render_widget(
        Paragraph::new(format!(
            "File\n  {}\n\nStatus\n  {}\n\nChanges\n  {}",
            path.display(),
            git_file_status_name(status),
            change_summary(*changes),
        ))
        .block(panel_block("Changed File Detail", false)),
        areas[0],
    );
    frame.render_widget(Paragraph::new("Esc: Back  q: Quit"), areas[1]);
}

fn panel_block(title: impl AsRef<str>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .title(padded_title(title))
}
fn render_compact(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new("DevScope\nTerminal too small\nq / Esc: Quit"),
        area,
    );
}

fn refresh_status(app: &App) -> String {
    if let Some(error) = app.refresh_error() {
        return error.to_owned();
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

fn inner_width(area: Rect) -> usize {
    usize::from(area.width.saturating_sub(2))
}

fn padded_title(title: impl AsRef<str>) -> String {
    format!(" {} ", title.as_ref())
}

fn project_progress(app: &App, width: u16) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(plan_line(app.plan(), usize::from(width.saturating_sub(2)))),
        Line::from(work_line(
            app.current_work(),
            usize::from(width.saturating_sub(2)),
        )),
        Line::from(format!("Activity   {}", activity(app.activity()))),
        Line::from(format!("Evidence   {}", evidence(app))),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(padded_title("Project Progress")),
    )
}

fn plan_line(plan_state: PlanState, width: usize) -> String {
    const LABEL: &str = "Plan       ";
    format!(
        "{LABEL}{}",
        plan(plan_state, width.saturating_sub(LABEL.len()))
    )
}

fn work_line(current_work: &CurrentWorkState, width: usize) -> String {
    const LABEL: &str = "Work       ";
    let value_width = width.saturating_sub(LABEL.len());
    let value = match current_work {
        CurrentWorkState::NotSet => "Not set".into(),
        CurrentWorkState::Unavailable => "Unavailable".into(),
        CurrentWorkState::Available(work) if work.total() == 0 => "No items".into(),
        CurrentWorkState::Available(work) => {
            progress_value(work.completed(), work.total(), value_width)
        }
    };
    format!("{LABEL}{value}")
}
fn plan(plan: PlanState, width: usize) -> String {
    match plan {
        PlanState::Available(summary) if summary.total() == 0 => "No tasks found".into(),
        PlanState::Available(summary) => {
            progress_value(summary.completed(), summary.total(), width)
        }
        PlanState::Unavailable => "Unavailable".into(),
    }
}

fn progress_value(completed: usize, total: usize, width: usize) -> String {
    if total == 0 {
        return "No tasks found".into();
    }

    let completed = completed.min(total);
    let percent = ((completed as u128 * 100) / total as u128) as usize;
    let count = format!("{completed}/{total}");
    if width < count.len() {
        return String::new();
    }
    if width < count.len() + percent.to_string().len() + 2 {
        return count;
    }

    let percentage = format!("{percent}%");
    let reserved = percentage.len() + count.len() + 2;
    let bar_width = width.saturating_sub(reserved).min(14);
    if bar_width == 0 {
        return format!("{percentage} {count}");
    }

    let filled = ((completed as u128 * bar_width as u128) / total as u128) as usize;
    let bar = format!("{}{}", "━".repeat(filled), "─".repeat(bar_width - filled));
    format!("{bar} {percentage} {count}")
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

fn changed_files(
    activity: &ActivityState,
    selected: Option<usize>,
    rows: usize,
    width: usize,
) -> Vec<Line<'static>> {
    if rows == 0 {
        return vec![];
    }

    match activity {
        ActivityState::Available(summary) if summary.changed_files() == 0 => {
            vec![Line::from("No changed files")]
        }
        ActivityState::Available(summary) => {
            let files = summary.changed_file_items();
            let selected = selected.unwrap_or(0).min(files.len() - 1);
            let file_rows = if files.len() > rows && rows > 1 {
                rows - 1
            } else {
                rows
            };
            let start = selected
                .saturating_sub(file_rows.saturating_sub(1))
                .min(files.len().saturating_sub(file_rows));
            let end = (start + file_rows).min(files.len());
            let mut lines = files[start..end]
                .iter()
                .enumerate()
                .map(|(offset, file)| changed_file_line(file, start + offset == selected, width))
                .collect::<Vec<_>>();
            if end < files.len() && lines.len() < rows {
                lines.push(Line::from(format!("... and {} more", files.len() - end)));
            }
            lines
        }
        ActivityState::NotRepository | ActivityState::Unavailable => {
            vec![Line::from("Unavailable")]
        }
    }
}

fn changed_file_line(
    file: &devscope::progress::GitChangedFile,
    selected: bool,
    width: usize,
) -> Line<'static> {
    let path = format!(
        "{} {}  {}",
        if selected { ">" } else { " " },
        git_file_status(&file.status),
        file.path.display()
    );
    let Some(counts) = change_counts_summary(file.changes) else {
        return Line::from(path);
    };
    let path_width = Line::from(path.clone()).width();
    let counts_width = Line::from(counts.clone()).width();
    if path_width + 2 + counts_width > width {
        return Line::from(path);
    }
    Line::from(format!(
        "{path}{}{counts}",
        " ".repeat(width - path_width - counts_width)
    ))
}

fn change_counts_summary(changes: GitChangeCounts) -> Option<String> {
    match (changes.additions, changes.deletions) {
        (Some(additions), Some(deletions)) => Some(format!("+{additions} -{deletions}")),
        _ => None,
    }
}
fn change_summary(changes: GitChangeCounts) -> String {
    match (changes.additions, changes.deletions) {
        (Some(additions), Some(deletions)) => format!("+{additions} -{deletions}"),
        _ => "unavailable".into(),
    }
}

fn git_file_status_name(status: &GitFileStatus) -> &'static str {
    match status {
        GitFileStatus::Modified => "Modified",
        GitFileStatus::Added => "Added",
        GitFileStatus::Deleted => "Deleted",
        GitFileStatus::Renamed => "Renamed",
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
    use devscope::current_work::load_current_work;
    use devscope::progress::{
        ActivitySummary, BuildTestDiagnostic, BuildTestExecutionError, BuildTestFreshness,
        BuildTestKind, BuildTestOutcome, BuildTestResult, BuildTestRun, BuildTestState,
        GitActivity, GitChangedFile, GitCommit, GitFileStatus, PlanSummary, TaskSummary,
        TaskSummaryItem,
    };
    use devscope::project::ProjectSnapshot;
    use ratatui::{Terminal, backend::TestBackend};
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static WORK_ID: AtomicUsize = AtomicUsize::new(0);

    fn work_state(items: &str) -> CurrentWorkState {
        let root = std::env::temp_dir().join(format!(
            "devscope-ui-work-{}-{}",
            std::process::id(),
            WORK_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join(".devscope/work/current.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!("# Current Work\nParent: docs/roadmap.md\nTask: Work\n{items}"),
        )
        .unwrap();
        let work = load_current_work(&root).unwrap().unwrap();
        let _ = fs::remove_dir_all(root);
        CurrentWorkState::Available(work)
    }
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
        assert!(draw(&app, 80, 30).contains("Details: Build"));
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
        assert!(output.contains("a detailed execution error"));
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
        assert!(output.contains("60% 3/5"));
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
                changes: Default::default(),
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
                    changes: Default::default(),
                },
                GitChangedFile {
                    path: "b".into(),
                    status: GitFileStatus::Modified,
                    changes: Default::default(),
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
    fn renders_changed_file_detail_with_path_status_and_small_terminal_safety() {
        let mut app = app(
            TaskState::Unavailable,
            activity_with_files(vec![GitChangedFile {
                path: "src/ui.rs".into(),
                status: GitFileStatus::Modified,
                changes: GitChangeCounts {
                    additions: Some(12),
                    deletions: Some(4),
                },
            }]),
        );
        let panels = focusable_panels(80, 30);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            panels,
        );

        let output = draw(&app, 80, 30);
        assert!(output.contains("Changed File Detail"));
        assert!(output.contains("src/ui.rs"));
        assert!(output.contains("Modified"));
        assert!(output.contains("+12 -4"));
        assert!(output.contains("Esc: Back  q: Quit"));
        assert_eq!(change_summary(Default::default()), "unavailable");
        for (width, height) in [(40, 18), (20, 5), (1, 1)] {
            let _ = draw(&app, width, height);
        }
    }
    #[test]
    fn shows_change_counts_in_changed_files_without_sacrificing_path_or_selection() {
        let files = vec![
            GitChangedFile {
                path: "src/ui.rs".into(),
                status: GitFileStatus::Modified,
                changes: GitChangeCounts {
                    additions: Some(24),
                    deletions: Some(8),
                },
            },
            GitChangedFile {
                path: "src/app.rs".into(),
                status: GitFileStatus::Modified,
                changes: GitChangeCounts {
                    additions: Some(12),
                    deletions: Some(3),
                },
            },
            GitChangedFile {
                path: "assets/new.bin".into(),
                status: GitFileStatus::Added,
                changes: GitChangeCounts::unavailable(),
            },
        ];
        let activity = activity_with_files(files.clone());
        let wide = changed_files(&activity, Some(1), 3, 60);
        assert!(wide[0].to_string().contains("M  src/ui.rs"));
        assert!(wide[0].to_string().ends_with("+24 -8"));
        assert!(wide[1].to_string().starts_with("> M  src/app.rs"));
        assert!(wide[1].to_string().ends_with("+12 -3"));
        assert!(wide[2].to_string().contains("A  assets/new.bin"));
        assert!(!wide[2].to_string().contains('+'));

        let narrow = changed_files(&activity, Some(1), 3, 16);
        assert!(narrow[0].to_string().contains("M  src/ui.rs"));
        assert!(!narrow[0].to_string().contains("+24 -8"));
        assert!(narrow[1].to_string().starts_with("> M  src/app.rs"));
        assert!(!narrow[1].to_string().contains("+12 -3"));

        let exact_file = &files[0];
        let path_width = changed_file_line(exact_file, true, 0).width();
        let counts_width = Line::from("+24 -8").width();
        let exact_width = path_width + 2 + counts_width;
        assert!(
            changed_file_line(exact_file, true, exact_width)
                .to_string()
                .ends_with("+24 -8")
        );
        assert!(
            !changed_file_line(exact_file, true, exact_width - 1)
                .to_string()
                .contains("+24 -8")
        );

        let output = draw(
            &app(TaskState::Unavailable, activity_with_files(files)),
            80,
            30,
        );
        assert!(output.contains("M  src/ui.rs"));
        assert!(output.contains("+24 -8"));
        assert!(output.contains("+12 -3"));
    }
    #[test]
    fn renders_changed_files_and_clean_state() {
        let activity = activity_with_files(vec![
            GitChangedFile {
                path: "src/a.rs".into(),
                status: GitFileStatus::Modified,
                changes: Default::default(),
            },
            GitChangedFile {
                path: "src/b.rs".into(),
                status: GitFileStatus::Added,
                changes: Default::default(),
            },
            GitChangedFile {
                path: "docs/old.md".into(),
                status: GitFileStatus::Deleted,
                changes: Default::default(),
            },
            GitChangedFile {
                path: "docs/new.md".into(),
                status: GitFileStatus::Renamed,
                changes: Default::default(),
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
                changes: Default::default(),
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
                changes: Default::default(),
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
    #[test]
    fn renders_focus_border_for_tasks_and_evidence() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(1, task_items(1))),
            ActivityState::Unavailable,
        );
        let tasks_focused = draw(&app, 80, 30);
        assert!(tasks_focused.contains("Task Summary"));
        assert!(tasks_focused.contains("Details: Build"));

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let evidence_focused = draw(&app, 80, 30);
        assert!(evidence_focused.contains("Task Summary"));
        assert!(evidence_focused.contains("Details: Build"));
        assert_ne!(tasks_focused, evidence_focused);
    }
    #[test]
    fn renders_current_work_states_with_the_shared_progress_visual() {
        assert_eq!(
            work_line(&CurrentWorkState::NotSet, 80),
            "Work       Not set"
        );
        assert_eq!(
            work_line(&CurrentWorkState::Unavailable, 80),
            "Work       Unavailable"
        );
        assert_eq!(work_line(&work_state(""), 80), "Work       No items");
        let partial = work_line(&work_state("- [x] Done\n- [ ] Next\n"), 80);
        assert!(partial.contains("50% 1/2"));
        assert!(partial.contains('━'));
        assert!(work_line(&work_state("- [x] Done\n"), 80).contains("100% 1/1"));
        assert_eq!(
            work_line(&work_state("- [x] Done\n- [ ] Next\n"), 17),
            "Work       1/2"
        );
    }
    #[test]
    fn plan_progress_degrades_without_exceeding_the_available_line_width() {
        for width in [80, 32, 18] {
            let line = plan_line(PlanState::Available(PlanSummary::new(53, 59)), width);
            assert!(line.chars().count() <= width, "{line:?} exceeds {width}");
        }

        let wide = plan_line(PlanState::Available(PlanSummary::new(53, 59)), 80);
        assert!(wide.contains("━━━━━━━━"));
        assert!(wide.contains("89% 53/59"));
        let medium = plan_line(PlanState::Available(PlanSummary::new(53, 59)), 32);
        assert!(medium.contains("89% 53/59"));
        assert!(medium.contains('━'));
        assert_eq!(
            plan_line(PlanState::Available(PlanSummary::new(53, 59)), 18),
            "Plan       53/59"
        );
        assert_eq!(progress_value(53, 59, "53/59".len()), "53/59");
        assert_eq!(progress_value(53, 59, "53/59".len() - 1), "");
        assert_eq!(
            plan(PlanState::Available(PlanSummary::new(0, 0)), 40),
            "No tasks found"
        );
        assert!(plan(PlanState::Available(PlanSummary::new(0, 5)), 40).contains("0% 0/5"));
        assert!(plan(PlanState::Available(PlanSummary::new(5, 5)), 40).contains("100% 5/5"));
        assert_eq!(plan(PlanState::Unavailable, 40), "Unavailable");
    }

    #[test]
    fn focusable_panels_follow_the_responsive_layout() {
        assert_eq!(
            focusable_panels(80, 30),
            &[
                FocusedPanel::Tasks,
                FocusedPanel::Evidence,
                FocusedPanel::ChangedFiles
            ]
        );
        assert_eq!(
            focusable_panels(80, 25),
            &[
                FocusedPanel::Tasks,
                FocusedPanel::Evidence,
                FocusedPanel::ChangedFiles
            ]
        );
        assert_eq!(
            focusable_panels(40, 18),
            &[FocusedPanel::Tasks, FocusedPanel::Evidence]
        );
        assert!(focusable_panels(19, 18).is_empty());
    }

    #[test]
    fn renders_changed_file_selection_and_hides_its_focus_on_small_layouts() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(2, task_items(2))),
            activity_with_files(vec![
                GitChangedFile {
                    path: "src/a.rs".into(),
                    status: GitFileStatus::Modified,
                    changes: Default::default(),
                },
                GitChangedFile {
                    path: "src/b.rs".into(),
                    status: GitFileStatus::Added,
                    changes: Default::default(),
                },
            ]),
        );
        let panels = focusable_panels(80, 30);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            panels,
        );
        let large = draw(&app, 80, 30);
        assert!(large.contains("  M  src/a.rs"));
        assert!(large.contains("> A  src/b.rs"));

        app.reconcile_focus(focusable_panels(40, 18));
        assert_eq!(app.focused_panel(), FocusedPanel::Tasks);
        let small = draw(&app, 40, 18);
        assert!(!small.contains("Changed Files"));
        assert!(!small.contains("src/a.rs"));
    }

    #[test]
    fn renders_padded_titles_for_all_visible_panels() {
        let app = app(
            TaskState::Available(TaskSummary::new(1, task_items(1))),
            activity_with_files(vec![GitChangedFile {
                path: "src/a.rs".into(),
                status: GitFileStatus::Modified,
                changes: Default::default(),
            }]),
        );
        let output = draw(&app, 80, 30);
        for title in [
            " Project Progress ",
            " Task Summary ",
            " Details: Build ",
            " Changed Files ",
            " Recent Commits ",
        ] {
            assert!(output.contains(title));
        }
    }
}
