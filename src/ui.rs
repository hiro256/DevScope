use std::time::Duration;

use crate::app::{
    ActivityState, App, CurrentWorkState, DetailTarget, EvidenceSelection, FocusedPanel, PlanState,
    RefreshSource, TaskState,
};
use devscope::progress::{
    BuildTestFreshness, BuildTestKind, BuildTestOutcome, BuildTestState, GitChangeCounts,
    GitDiffText, GitFileInspection, GitFileInspectionUnavailable, GitFileStatus,
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
const CHANGE_COUNTS_GAP: usize = 2;
const MAX_CHANGE_COUNTS_COLUMN: usize = 48;
const MIN_NAVIGATION_PANE_WIDTH: u16 = 35;
const MIN_PREVIEW_PANE_WIDTH: u16 = 43;

#[derive(Clone, Copy)]
enum LayoutVariant {
    Large,
    Medium,
    Small,
}

fn preview_pane_widths(width: u16) -> (u16, u16) {
    let panes = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(Rect::new(0, 0, width, 1));
    (panes[0].width, panes[1].width)
}

pub fn preview_layout_available(width: u16, height: u16) -> bool {
    if !matches!(
        layout_variant(width, height),
        Some(LayoutVariant::Large | LayoutVariant::Medium)
    ) {
        return false;
    }
    let (navigation_width, preview_width) = preview_pane_widths(width);
    navigation_width >= MIN_NAVIGATION_PANE_WIDTH && preview_width >= MIN_PREVIEW_PANE_WIDTH
}

pub fn has_global_preview(width: u16, height: u16, enabled: bool) -> bool {
    enabled && preview_layout_available(width, height)
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
    if app.is_file_browser_open() {
        render_file_browser(frame, area, app);
        return;
    }
    if app.detail_target().is_some() {
        render_detail(frame, area, app);
        return;
    }
    let Some(layout) = layout_variant(area.width, area.height) else {
        render_compact(frame, area);
        return;
    };
    let outer = overview_areas(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(header_title(app, usize::from(area.width)))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            Line::from(now_label(app.current_work(), usize::from(area.width))),
        ]),
        outer[0],
    );
    frame.render_widget(project_progress(app, outer[1].width), outer[1]);

    if has_global_preview(area.width, area.height, app.preview_visible()) {
        let panes = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(outer[2]);
        render_navigation_panels(frame, panes[0], layout, app);
        render_preview(frame, panes[1], app);
    } else {
        render_navigation_panels(frame, outer[2], layout, app);
    }

    frame.render_widget(
        Paragraph::new(footer_text(area.width, area.height)),
        outer[3],
    );
}

fn overview_areas(area: Rect) -> [Rect; 4] {
    let areas = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(6),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    [areas[0], areas[1], areas[2], areas[3]]
}

fn safe_display_text(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n");
    let mut output = String::new();
    for ch in normalized.chars() {
        if ch == '\n' || !ch.is_control() {
            output.push(ch);
        } else {
            output.extend(ch.escape_default());
        }
    }
    output
}

fn browser_path(path: &std::path::Path) -> String {
    if path.as_os_str().is_empty() {
        "/".into()
    } else {
        safe_display_text(&path.to_string_lossy()).replace('\n', "\\n")
    }
}

fn file_error_text(error: devscope::progress::SafeTextError) -> &'static str {
    use devscope::progress::SafeTextError::*;
    match error {
        Missing => "Missing file or directory",
        UnsafePath => "Unsafe or excluded path",
        Symlink => "Unsupported link / reparse point",
        NotRegularFile => "Unsupported file type",
        Binary => "Binary or invalid UTF-8 text",
        ReadError => "Read error",
    }
}

fn browser_preview_lines(app: &App) -> Vec<Line<'static>> {
    use devscope::progress::BrowserEntryKind;
    let browser = &app.file_browser;
    let Some(entry) = browser.selected_entry() else {
        return vec![Line::from("No entry selected")];
    };
    let path = browser_path(&entry.path);
    let mut lines = match entry.kind {
        BrowserEntryKind::Directory => vec![Line::from("Directory"), Line::from(path)],
        BrowserEntryKind::Parent => vec![Line::from("Parent"), Line::from(path)],
        BrowserEntryKind::UnsupportedLink => vec![
            Line::from(path),
            Line::from("Unsupported link / reparse point"),
        ],
        BrowserEntryKind::Unsupported => {
            vec![Line::from(path), Line::from("Unsupported file type")]
        }
        BrowserEntryKind::Error => vec![Line::from(path), Line::from("Metadata read error")],
        BrowserEntryKind::File => vec![
            Line::from("File"),
            Line::from(path),
            Line::from(""),
            Line::from("Mode"),
            Line::from("  File content"),
            Line::from(""),
        ],
    };
    if entry.kind == BrowserEntryKind::File {
        match &browser.preview {
            Some(Ok((text, truncated))) => {
                lines.extend(
                    safe_display_text(text)
                        .lines()
                        .map(|line| Line::from(line.to_owned())),
                );
                if *truncated {
                    lines.push(Line::from("... file content truncated ..."));
                }
            }
            Some(Err(error)) => lines.push(Line::from(file_error_text(*error))),
            None => lines.push(Line::from("Unavailable")),
        }
    }
    lines
}

fn browser_areas(area: Rect) -> [Rect; 3] {
    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    [areas[0], areas[1], areas[2]]
}

pub fn browser_preview_scroll_limit(app: &App, area: Rect) -> usize {
    if !preview_layout_available(area.width, area.height) {
        return 0;
    }
    browser_preview_lines(app)
        .len()
        .saturating_sub(inner_height(browser_areas(area)[1]))
}

fn render_file_browser(frame: &mut Frame, area: Rect, app: &App) {
    use devscope::progress::BrowserEntryKind;
    if layout_variant(area.width, area.height).is_none() {
        frame.render_widget(
            Paragraph::new("File Browser\nTerminal too small\nEsc Back / q Quit"),
            area,
        );
        return;
    }
    let browser = &app.file_browser;
    let outer = browser_areas(area);
    let status = [
        browser.notice.map(str::to_owned),
        browser
            .error
            .map(|error| format!("Unavailable: {}", file_error_text(error))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" | ");
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(
                "File Browser  {}",
                browser_path(&browser.current_dir)
            )),
            Line::from(status),
            Line::from(if browser.listing_incomplete {
                "Listing incomplete (bounded subset)"
            } else {
                ""
            }),
        ]),
        outer[0],
    );
    let split = preview_layout_available(area.width, area.height);
    let panes = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(outer[1]);
    let list_area = if split { panes[0] } else { outer[1] };
    let rows = inner_height(list_area);
    let start = browser
        .selected
        .unwrap_or(0)
        .saturating_sub(rows.saturating_sub(1));
    let lines: Vec<Line<'static>> = browser
        .entries
        .iter()
        .enumerate()
        .skip(start)
        .take(rows)
        .map(|(index, entry)| {
            let suffix = match entry.kind {
                BrowserEntryKind::Directory => "/",
                BrowserEntryKind::UnsupportedLink => " [link]",
                BrowserEntryKind::Unsupported => " [unsupported]",
                BrowserEntryKind::Error => " [error]",
                _ => "",
            };
            let name = safe_display_text(&entry.name.to_string_lossy()).replace('\n', "\\n");
            Line::from(format!(
                "{} {name}{suffix}",
                if browser.selected == Some(index) {
                    ">"
                } else {
                    " "
                }
            ))
        })
        .collect();
    let lines = if lines.is_empty() {
        vec![Line::from(if browser.error.is_some() {
            "Unavailable"
        } else {
            "Empty directory"
        })]
    } else {
        lines
    };
    frame.render_widget(
        Paragraph::new(lines).block(panel_block("Files", true)),
        list_area,
    );
    if split {
        let scroll = browser
            .preview_scroll
            .min(browser_preview_scroll_limit(app, area))
            .min(u16::MAX as usize) as u16;
        frame.render_widget(
            Paragraph::new(browser_preview_lines(app))
                .block(panel_block("Preview", false))
                .scroll((scroll, 0)),
            panes[1],
        );
    }
    let footer = if area.width >= 110 {
        "↑/↓ Select  ←/→ Navigate  Enter Open  Ctrl+↑/↓ Scroll  r Reload  Esc Back  q Quit"
    } else if split {
        "↑/↓ Select ←/→ Dir Enter Open Ctrl+↑/↓ Scroll r Reload Esc Back q Quit"
    } else if area.width >= 45 {
        "↑/↓ Select ←/→ Dir Enter Open r Reload Esc Back q"
    } else {
        "←/→ Dir r Reload Esc Back q"
    };
    frame.render_widget(Paragraph::new(footer), outer[2]);
}

fn header_title(app: &App, width: usize) -> String {
    const TITLE: &str = "DevScope";
    const GAP: &str = "  ";
    let refresh = refresh_status(app);
    if Line::from(format!("{TITLE}{GAP}{refresh}")).width() <= width {
        format!("{TITLE}{GAP}{refresh}")
    } else {
        truncate_text(TITLE, width)
    }
}

fn now_label(current_work: &CurrentWorkState, width: usize) -> String {
    const PREFIX: &str = "● NOW  ";
    let value = match current_work {
        CurrentWorkState::Available(work) => {
            work.active_item().map_or("Not set", |item| item.text())
        }
        CurrentWorkState::NotSet => "Not set",
        CurrentWorkState::Unavailable => "Unavailable",
    };
    let available = width.saturating_sub(Line::from(PREFIX).width());
    format!("{PREFIX}{}", truncate_text(value, available))
}
fn footer_text(width: u16, height: u16) -> &'static str {
    if preview_layout_available(width, height) {
        if width >= 120 {
            "←/→:Panel  ↑/↓:Move  Enter:Preview  Ctrl+↑/↓:Scroll  Ctrl+Enter:Detail  b:Build  t:Test  r:Reload  q/Esc:Quit"
        } else if width >= 96 {
            "←/→:Panel  ↑/↓:Move  Enter:Preview  Ctrl+↑/↓:Scroll  b:Build  t:Test  r:Reload  q/Esc:Quit"
        } else {
            "←/→:Panel ↑/↓:Move Enter:Preview Ctrl+↑/↓:Scroll b/t:Verify r:Reload q:Quit"
        }
    } else if width >= 65 {
        "←/→:Panel  ↑/↓:Move  b:Build  t:Test  r:Reload  q/Esc:Quit"
    } else if width >= 40 {
        "←/→:Panel  ↑/↓:Move  b/t:Verify  q:Quit"
    } else {
        "←/→:Panel ↑/↓:Move q"
    }
}
fn render_navigation_panels(frame: &mut Frame, area: Rect, layout: LayoutVariant, app: &App) {
    match layout {
        LayoutVariant::Large => {
            let panels = Layout::vertical([
                Constraint::Length(6),
                Constraint::Length(4),
                Constraint::Length(6),
                Constraint::Min(2),
            ])
            .split(area);
            render_tasks(frame, panels[0], app);
            render_evidence(frame, panels[1], app);
            render_changed_files_list(frame, panels[2], app);
            render_commits(frame, panels[3], app);
        }
        LayoutVariant::Medium => {
            let panels = Layout::vertical([
                Constraint::Length(6),
                Constraint::Length(4),
                Constraint::Min(4),
            ])
            .split(area);
            render_tasks(frame, panels[0], app);
            render_evidence(frame, panels[1], app);
            render_changed_files_list(frame, panels[2], app);
        }
        LayoutVariant::Small => {
            let panels = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);
            render_tasks(frame, panels[0], app);
            render_evidence(frame, panels[1], app);
        }
    }
}

fn render_tasks(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(tasks(
            app.tasks(),
            app.selected_task(),
            inner_height(area),
            inner_width(area),
            app.current_work(),
        ))
        .block(panel_block(
            "Tasks",
            app.focused_panel() == FocusedPanel::Tasks,
        )),
        area,
    );
}

fn render_evidence(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(evidence_selector_lines(app)).block(panel_block(
            "Evidence",
            app.focused_panel() == FocusedPanel::Evidence,
        )),
        area,
    );
}

fn evidence_selector_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = [BuildTestKind::Build, BuildTestKind::Test]
        .into_iter()
        .map(|kind| {
            let selection = match kind {
                BuildTestKind::Build => EvidenceSelection::Build,
                BuildTestKind::Test => EvidenceSelection::Test,
            };
            let marker = if app.evidence_selection() == selection {
                "> "
            } else {
                "  "
            };
            Line::from(format!(
                "{marker}{}  {}",
                detail_kind(kind),
                evidence_selector_status(app.build_test_state(kind))
            ))
        })
        .collect::<Vec<_>>();
    if let Some(artifact) = app.artifact() {
        let marker = if app.evidence_selection() == EvidenceSelection::Artifact {
            "> "
        } else {
            "  "
        };
        let status = artifact_selector_status(artifact.status());
        lines.push(Line::from(format!("{marker}Artifact  {status}")));
    }
    lines
}
fn evidence_selector_status(state: &BuildTestState) -> String {
    match state {
        BuildTestState::Completed(result) => match (result.outcome(), result.freshness()) {
            (BuildTestOutcome::Passed, BuildTestFreshness::Fresh) => "✓ Passed".into(),
            (BuildTestOutcome::Passed, BuildTestFreshness::Stale) => "! Passed (stale)".into(),
            (BuildTestOutcome::Failed, BuildTestFreshness::Fresh) => "✕ Failed".into(),
            (BuildTestOutcome::Failed, BuildTestFreshness::Stale) => "✕ Failed (stale)".into(),
        },
        BuildTestState::Unavailable => "? Unavailable".into(),
        BuildTestState::NotRun => "· Not run".into(),
        BuildTestState::Running(_) => "▶ Running".into(),
        BuildTestState::ExecutionError(_) => "! Error".into(),
    }
}

fn artifact_selector_status(status: &devscope::progress::ArtifactStatus) -> &'static str {
    match status {
        devscope::progress::ArtifactStatus::Exists { .. } => "✓ Exists",
        devscope::progress::ArtifactStatus::Missing => "· Missing",
        devscope::progress::ArtifactStatus::ObservationError { .. } => "! Error",
    }
}

fn render_changed_files_list(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(changed_files(
            app.activity(),
            app.selected_changed_file(),
            inner_height(area),
            inner_width(area),
        ))
        .block(panel_block(
            "Changed Files",
            app.focused_panel() == FocusedPanel::ChangedFiles,
        )),
        area,
    );
}

fn render_commits(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(commits(app.activity(), inner_height(area))).block(
            Block::default()
                .borders(Borders::ALL)
                .title(padded_title("Recent Commits")),
        ),
        area,
    );
}

fn render_preview(frame: &mut Frame, area: Rect, app: &App) {
    let (title, lines) = preview_content(app);
    let limit = lines.len().saturating_sub(inner_height(area));
    let scroll = app.preview_scroll().min(limit).min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel_block(title, false))
            .scroll((scroll, 0)),
        area,
    );
}

pub fn preview_scroll_limit(app: &App, area: Rect) -> usize {
    if app.has_detail_view() || !has_global_preview(area.width, area.height, app.preview_visible())
    {
        return 0;
    }
    preview_content(app)
        .1
        .len()
        .saturating_sub(inner_height(overview_areas(area)[2]))
}

fn preview_content(app: &App) -> (String, Vec<Line<'static>>) {
    match app.focused_panel() {
        FocusedPanel::Tasks => ("Detail: Task".to_owned(), task_preview_lines(app)),
        FocusedPanel::Evidence => evidence_preview(app),

        FocusedPanel::ChangedFiles => {
            let title = selected_changed_file_path(app)
                .map(|path| format!("Detail: {path}"))
                .unwrap_or_else(|| "Detail: Changed Files".into());
            let lines = if app.selected_changed_file().is_some() {
                detail_inspection_lines(app.preview_inspection())
            } else {
                vec![Line::from("No file selected")]
            };
            (title, lines)
        }
    }
}

fn evidence_preview(app: &App) -> (String, Vec<Line<'static>>) {
    if app.evidence_selection() == EvidenceSelection::Artifact {
        return artifact_preview(app.artifact());
    }
    let kind = app.evidence_detail_kind().unwrap_or(BuildTestKind::Build);
    (
        format!("Detail: {}", detail_kind(kind)),
        evidence_preview_lines(kind, app.build_test_state(kind)),
    )
}
fn artifact_preview(
    artifact: Option<&devscope::progress::ArtifactObservation>,
) -> (String, Vec<Line<'static>>) {
    use devscope::progress::{ArtifactKind, ArtifactStatus};
    let Some(artifact) = artifact else {
        return ("Detail: Artifact".into(), vec![Line::from("Unavailable")]);
    };
    let mut lines = preview_field("Path", &artifact.path().display().to_string());
    match artifact.status() {
        ArtifactStatus::Exists { kind, size } => {
            lines.extend(preview_field("Status", "Exists"));
            lines.extend(preview_field(
                "Kind",
                match kind {
                    ArtifactKind::File => "File",
                    ArtifactKind::Directory => "Directory",
                    ArtifactKind::Other => "Other",
                },
            ));
            lines.extend(preview_field("Size", &format!("{size} bytes")));
        }
        ArtifactStatus::Missing => lines.extend(preview_field("Status", "Missing")),
        ArtifactStatus::ObservationError { message } => {
            lines.extend(preview_field("Status", "Observation error"));
            lines.extend(preview_field("Error", message));
        }
    }
    ("Detail: Artifact".into(), lines)
}
fn evidence_preview_lines(kind: BuildTestKind, state: &BuildTestState) -> Vec<Line<'static>> {
    match state {
        BuildTestState::Unavailable => preview_field("Status", "Unavailable"),
        BuildTestState::NotRun => {
            let mut lines = preview_field("Status", "Not run");
            lines.extend(preview_field(
                "Action",
                &format!("Press {} to run {}.", detail_key(kind), detail_kind(kind)),
            ));
            lines
        }
        BuildTestState::Running(run) => {
            let mut lines = preview_field("Status", "Running");
            lines.extend(preview_field("Command", run.command_label()));
            lines
        }
        BuildTestState::Completed(result) => {
            let outcome = match result.outcome() {
                BuildTestOutcome::Passed => "Passed",
                BuildTestOutcome::Failed => "Failed",
            };
            let freshness = match result.freshness() {
                BuildTestFreshness::Fresh => "Fresh",
                BuildTestFreshness::Stale => "Stale",
            };
            let mut lines = preview_field("Status", outcome);
            lines.extend(preview_field("Freshness", freshness));
            lines.extend(preview_field("Command", result.command_label()));
            lines.extend(preview_field(
                "Duration",
                &format_duration(result.duration()),
            ));
            if !result.summary().is_empty() {
                lines.extend(preview_field("Result", result.summary()));
            }
            lines
        }
        BuildTestState::ExecutionError(error) => {
            let mut lines = preview_field("Status", "Execution error");
            lines.extend(preview_field("Command", error.command_label()));
            lines.extend(preview_field("Error", error.message()));
            lines
        }
    }
}

fn preview_field(label: &str, value: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(label.to_owned()),
        Line::from(format!("  {value}")),
    ]
}
fn task_preview_lines(app: &App) -> Vec<Line<'static>> {
    let (Some(selected), TaskState::Available(summary)) = (app.selected_task(), app.tasks()) else {
        return vec![Line::from("No task selected")];
    };
    let Some(task) = summary.items().get(selected) else {
        return vec![Line::from("No task selected")];
    };

    let mut lines = vec![
        Line::from(task.text().to_owned()),
        Line::from(""),
        Line::from("Source"),
        Line::from(format!("  {}", task.source_path().display())),
    ];
    if let Some(heading) = task.heading() {
        lines.extend([
            Line::from(""),
            Line::from("Section"),
            Line::from(format!("  {heading}")),
        ]);
    }
    if let CurrentWorkState::Available(work) = app.current_work()
        && task_matches_current_work(task, app.current_work())
    {
        lines.extend([Line::from(""), Line::from("Current Work")]);
        lines.extend(work.items().iter().map(|item| {
            let marker = if item.completed() { "✓" } else { "□" };
            Line::from(format!("  {marker} {}", item.text()))
        }));
    }
    if !task.context().is_empty() {
        lines.extend([Line::from(""), Line::from("Context")]);
        lines.extend(task.context().iter().enumerate().map(|(index, line)| {
            let line_number = task.context_start_line() + index;
            let marker = if line_number == task.line() {
                "│ "
            } else {
                "  "
            };
            Line::from(format!("{marker}{line}"))
        }));
    }
    lines
}
fn selected_changed_file_path(app: &App) -> Option<String> {
    let (Some(selected), ActivityState::Available(summary)) =
        (app.selected_changed_file(), app.activity())
    else {
        return None;
    };
    summary
        .changed_file_items()
        .get(selected)
        .map(|file| file.path.display().to_string())
}
fn render_detail(frame: &mut Frame, area: Rect, app: &App) {
    let header_height = 10;
    let areas = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    let Some(DetailTarget::ChangedFile {
        path,
        status,
        changes,
    }) = app.detail_target()
    else {
        return;
    };
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
    frame.render_widget(
        Paragraph::new(detail_inspection_lines(app.detail_inspection()))
            .block(panel_block(
                match app.detail_inspection() {
                    Some(GitFileInspection::Diff { .. }) => "Diff",
                    Some(GitFileInspection::FileContent { .. }) => "File content",
                    _ => "Inspection",
                },
                false,
            ))
            .scroll((app.detail_scroll().min(u16::MAX as usize) as u16, 0)),
        areas[1],
    );
    frame.render_widget(
        Paragraph::new("j/k: Scroll  Enter/Esc: Back  q: Quit"),
        areas[2],
    );
}

pub fn detail_scroll_limit(app: &App, area: Rect) -> usize {
    let body_height = usize::from(area.height.saturating_sub(11).saturating_sub(2));
    detail_inspection_lines(app.detail_inspection())
        .len()
        .saturating_sub(body_height)
}

fn detail_inspection_lines(diff: Option<&GitFileInspection>) -> Vec<Line<'static>> {
    match diff {
        None => vec![Line::from("Loading inspection...")],
        Some(GitFileInspection::Unavailable(reason)) => vec![
            Line::from("Inspection unavailable"),
            Line::from(match reason {
                GitFileInspectionUnavailable::Missing => "File is missing",
                GitFileInspectionUnavailable::UnsafePath => "Unsafe project-relative path",
                GitFileInspectionUnavailable::Symlink => "Symlink / reparse point is not followed",
                GitFileInspectionUnavailable::NotRegularFile => "Not a regular file",
                GitFileInspectionUnavailable::Binary => "Binary or unsupported UTF-8 text",
                GitFileInspectionUnavailable::ReadError => "File could not be read",
                GitFileInspectionUnavailable::GitError => "Git diff collection failed",
            }),
        ],
        Some(GitFileInspection::FileContent { text, truncated }) => {
            let mut lines = vec![
                Line::from("Mode"),
                Line::from("  File content"),
                Line::from(""),
            ];
            lines.extend(
                safe_display_text(text)
                    .lines()
                    .map(|line| Line::from(line.to_owned())),
            );
            if *truncated {
                lines.push(Line::from("... file content truncated ..."));
            }
            lines
        }
        Some(GitFileInspection::Diff { unstaged, staged }) => {
            let mut lines = vec![Line::from("Mode"), Line::from("  Git diff")];
            append_diff_section(&mut lines, "Unstaged", unstaged.as_ref());
            append_diff_section(&mut lines, "Staged", staged.as_ref());
            lines
        }
    }
}

fn append_diff_section(lines: &mut Vec<Line<'static>>, title: &str, diff: Option<&GitDiffText>) {
    let Some(diff) = diff else {
        return;
    };
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(title.to_owned()));
    lines.extend(diff.text.lines().map(|line| Line::from(line.to_owned())));
    if diff.truncated {
        lines.push(Line::from("... diff truncated ..."));
    }
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
        "Build {} | Test {}",
        evidence_selector_status(build),
        evidence_selector_status(test)
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

fn task_matches_current_work(
    task: &devscope::progress::TaskSummaryItem,
    current_work: &CurrentWorkState,
) -> bool {
    let CurrentWorkState::Available(work) = current_work else {
        return false;
    };
    task.source_path()
        .components()
        .eq(work.parent_path().components())
        && task.text() == work.parent_task()
}

fn tasks(
    task_state: &TaskState,
    selected: Option<usize>,
    rows: usize,
    width: usize,
    current_work: &CurrentWorkState,
) -> Vec<Line<'static>> {
    if rows == 0 {
        return vec![];
    }

    match task_state {
        TaskState::Unavailable => vec![Line::from("Unavailable")],
        TaskState::Available(summary) if summary.total() == 0 => vec![Line::from("No tasks found")],
        TaskState::Available(summary) if summary.remaining() == 0 => {
            vec![Line::from("All tasks completed")]
        }
        TaskState::Available(summary) => task_lines(summary, selected, rows, width, current_work),
    }
}

fn task_lines(
    summary: &devscope::progress::TaskSummary,
    selected: Option<usize>,
    rows: usize,
    width: usize,
    current_work: &CurrentWorkState,
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
            task_line(
                item,
                index == selected,
                width,
                task_matches_current_work(item, current_work),
            )
        })
        .collect::<Vec<_>>();

    if end < total && lines.len() < rows {
        lines.push(Line::from(format!("... and {} more", total - end)));
    }
    lines
}

fn task_line(
    task: &devscope::progress::TaskSummaryItem,
    selected: bool,
    width: usize,
    has_current_work: bool,
) -> Line<'static> {
    let prefix = format!("{} □ ", if selected { ">" } else { " " });
    if !has_current_work {
        return Line::from(format!("{prefix}{}", task.text()));
    }

    const INDICATOR: &str = "  [Work]";
    let reserved = Line::from(prefix.clone()).width() + Line::from(INDICATOR).width();
    if reserved > width {
        return Line::from(format!("{prefix}{}", task.text()));
    }
    let text = truncate_text(task.text(), width.saturating_sub(reserved));
    Line::from(format!("{prefix}{text}{INDICATOR}"))
}

fn truncate_text(text: &str, width: usize) -> String {
    if Line::from(text.to_owned()).width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }

    let ellipsis = "…";
    let text_width = width.saturating_sub(Line::from(ellipsis).width());
    let mut result = String::new();
    let mut result_width: usize = 0;
    for character in text.chars() {
        let character_width = Line::from(character.to_string()).width();
        if result_width.saturating_add(character_width) > text_width {
            break;
        }
        result.push(character);
        result_width += character_width;
    }
    result.push_str(ellipsis);
    result
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
            let visible_files = &files[start..end];
            let counts_column = change_counts_column(visible_files, width);
            let mut lines = visible_files
                .iter()
                .enumerate()
                .map(|(offset, file)| {
                    changed_file_line(file, start + offset == selected, width, counts_column)
                })
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

fn change_counts_column(files: &[devscope::progress::GitChangedFile], width: usize) -> usize {
    files
        .iter()
        .map(|file| Line::from(changed_file_prefix(file, false)).width())
        .max()
        .map(|prefix_width| {
            prefix_width
                .saturating_add(CHANGE_COUNTS_GAP)
                .min(MAX_CHANGE_COUNTS_COLUMN)
                .min(width)
        })
        .unwrap_or_default()
}

fn changed_file_line(
    file: &devscope::progress::GitChangedFile,
    selected: bool,
    width: usize,
    counts_column: usize,
) -> Line<'static> {
    let path = changed_file_prefix(file, selected);
    let Some(counts) = change_counts_summary(file.changes) else {
        return Line::from(path);
    };
    let path_width = Line::from(path.clone()).width();
    let counts_width = Line::from(counts.clone()).width();
    if path_width.saturating_add(CHANGE_COUNTS_GAP) > counts_column
        || counts_column.saturating_add(counts_width) > width
    {
        return Line::from(path);
    }
    Line::from(format!(
        "{path}{}{counts}",
        " ".repeat(counts_column - path_width)
    ))
}

fn changed_file_prefix(file: &devscope::progress::GitChangedFile, selected: bool) -> String {
    format!(
        "{} {}  {}",
        if selected { ">" } else { " " },
        git_file_status(&file.status),
        file.path.display()
    )
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
        matching_work_state("docs/roadmap.md", "Work", items)
    }

    fn matching_work_state(parent: &str, task: &str, items: &str) -> CurrentWorkState {
        let root = std::env::temp_dir().join(format!(
            "devscope-ui-work-{}-{}",
            std::process::id(),
            WORK_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join(".devscope/work/current.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!("# Current Work\nParent: {parent}\nTask: {task}\n{items}"),
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

    #[test]
    fn browser_preview_escapes_controls_without_changing_observation() {
        use devscope::progress::{BrowserEntry, BrowserEntryKind};
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
            focusable_panels(80, 30),
        );
        let text = "hello\r\n\x1b[2J\tcontrol\x07\x08\r\u{009b}";
        app.file_browser.entries = vec![BrowserEntry {
            path: "test.txt".into(),
            name: "test.txt".into(),
            kind: BrowserEntryKind::File,
        }];
        app.file_browser.selected = Some(0);
        app.file_browser.preview = Some(Ok((text.into(), false)));
        let lines = browser_preview_lines(&app)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(lines.contains("File content"));
        assert!(lines.contains("hello\n\\u{1b}[2J\\tcontrol"));
        assert!(!lines.chars().any(|c| c.is_control() && c != '\n'));
        assert_eq!(app.file_browser.preview, Some(Ok((text.into(), false))));
        assert!(draw(&app, 80, 30).contains("File content"));
    }

    #[test]
    fn browser_render_scroll_resize_and_incomplete_listing_are_safe() {
        use devscope::progress::{BrowserEntry, BrowserEntryKind, SafeTextError};
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.toggle_preview(); // Overview preference must not hide Browser Preview.
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
            focusable_panels(80, 30),
        );
        app.file_browser.entries = vec![BrowserEntry {
            path: "long.txt".into(),
            name: "long.txt".into(),
            kind: BrowserEntryKind::File,
        }];
        app.file_browser.selected = Some(0);
        app.file_browser.preview = Some(Ok((
            (0..60).map(|i| format!("browser line {i:02}\n")).collect(),
            true,
        )));
        app.file_browser.listing_incomplete = true;
        app.file_browser.error = Some(SafeTextError::ReadError);
        let first = draw(&app, 80, 30);
        assert!(first.contains("File Browser"));
        assert!(first.contains("Listing incomplete"));
        assert!(first.contains("Read error"));
        assert!(first.contains("browser line 00"));
        let limit = browser_preview_scroll_limit(&app, Rect::new(0, 0, 80, 30));
        app.file_browser.scroll(isize::MAX, limit);
        let last = draw(&app, 80, 30);
        assert!(last.contains("browser line 59"));
        assert!(last.contains("file content truncated"));
        let offset = app.file_browser.preview_scroll;
        for (width, height) in [(160, 40), (80, 25), (77, 30), (80, 24), (40, 18), (1, 1)] {
            let output = draw(&app, width, height);
            if !preview_layout_available(width, height) {
                assert!(!output.contains("browser line"));
            }
        }
        assert_eq!(app.file_browser.preview_scroll, offset);
        assert!(draw(&app, 80, 30).contains("browser line 59"));
    }

    #[test]
    fn browser_non_file_selection_and_text_errors_have_honest_previews() {
        use devscope::progress::{BrowserEntry, BrowserEntryKind, SafeTextError};
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.file_browser.selected = Some(0);
        app.file_browser.preview = Some(Ok(("stale file text".into(), false)));
        for (kind, expected) in [
            (BrowserEntryKind::Directory, "Directory"),
            (BrowserEntryKind::Parent, "Parent"),
            (BrowserEntryKind::UnsupportedLink, "Unsupported link"),
            (BrowserEntryKind::Unsupported, "Unsupported file type"),
            (BrowserEntryKind::Error, "Metadata read error"),
        ] {
            app.file_browser.entries = vec![BrowserEntry {
                path: "entry".into(),
                name: "entry".into(),
                kind,
            }];
            let output = browser_preview_lines(&app)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(output.contains(expected));
            assert!(!output.contains("stale file text"));
        }
        app.file_browser.entries[0].kind = BrowserEntryKind::File;
        for (reason, text) in [
            (SafeTextError::Binary, "Binary"),
            (SafeTextError::ReadError, "Read error"),
            (SafeTextError::Missing, "Missing"),
        ] {
            app.file_browser.preview = Some(Err(reason));
            assert!(
                browser_preview_lines(&app)
                    .iter()
                    .any(|line| line.to_string().contains(text))
            );
        }
    }

    #[test]
    fn preview_scrolling_renders_later_lines_for_each_source() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(
                1,
                vec![preview_task(
                    "roadmap.md",
                    3,
                    "Task title",
                    "Section name",
                    &[
                        "context zero",
                        "context one",
                        "context two",
                        "context three",
                    ],
                )],
            )),
            activity_with_files(vec![GitChangedFile {
                path: "file.rs".into(),
                status: GitFileStatus::Modified,
                changes: Default::default(),
            }]),
        );
        app.apply_build_test_state(
            BuildTestKind::Build,
            completed_state(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
                BuildTestFreshness::Fresh,
            ),
        );
        app.apply_preview_inspection(GitFileInspection::Diff {
            unstaged: Some(GitDiffText {
                text: (0..25).map(|i| format!("+diff {i}\n")).collect(),
                truncated: false,
            }),
            staged: None,
        });
        for panel in [
            FocusedPanel::Tasks,
            FocusedPanel::Evidence,
            FocusedPanel::ChangedFiles,
        ] {
            app.reconcile_focus(&[panel]);
            let area = Rect::new(0, 0, 60, 6);
            let limit = preview_content(&app)
                .1
                .len()
                .saturating_sub(inner_height(area));
            assert!(limit > 0);
            let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
            terminal
                .draw(|frame| render_preview(frame, area, &app))
                .unwrap();
            let before = text(&terminal);
            app.scroll_preview(isize::MAX, limit);
            terminal
                .draw(|frame| render_preview(frame, area, &app))
                .unwrap();
            assert_ne!(before, text(&terminal), "{panel:?}");
            assert_eq!(app.preview_scroll(), limit);
        }
    }

    #[test]
    fn preview_limit_matches_real_layout_and_footer_fits() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        for (width, height) in [(120, 30), (80, 25), (80, 24), (40, 20), (1, 1)] {
            assert_eq!(
                preview_scroll_limit(&app, Rect::new(0, 0, width, height)),
                0
            );
            app.scroll_preview(
                1,
                preview_scroll_limit(&app, Rect::new(0, 0, width, height)),
            );
            assert_eq!(app.preview_scroll(), 0);
            draw(&app, width, height);
        }
        for width in 20..160 {
            assert!(
                Line::from(footer_text(width, 30)).width() <= usize::from(width),
                "width {width}"
            );
        }
        let context: Vec<String> = (0..40).map(|i| format!("context {i}")).collect();
        let refs: Vec<&str> = context.iter().map(String::as_str).collect();
        app.apply_markdown_state(
            PlanState::Unavailable,
            TaskState::Available(TaskSummary::new(
                1,
                vec![preview_task("roadmap.md", 3, "Task", "TUI", &refs)],
            )),
        );
        let area = Rect::new(0, 0, 80, 25);
        let limit = preview_scroll_limit(&app, area);
        assert_eq!(
            limit,
            task_preview_lines(&app).len() - inner_height(overview_areas(area)[2])
        );
        let before = draw(&app, 80, 25);
        app.scroll_preview(1, limit);
        assert_ne!(before, draw(&app, 80, 25));
        app.toggle_preview();
        assert_eq!(preview_scroll_limit(&app, area), 0);
        assert_eq!(app.preview_scroll(), 1);
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
    fn evidence_preview_uses_selected_live_build_test_state() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(BuildTestKind::Build, BuildTestState::NotRun);
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        let panels = focusable_panels(80, 30);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        let not_run = draw(&app, 80, 30);
        assert!(not_run.contains("Detail: Build"));
        assert!(not_run.contains("Status"));
        assert!(not_run.contains("Not run"));
        assert!(not_run.contains("Press b to run Build."));
        assert!(not_run.contains("> Build  · Not run"));
        assert!(not_run.contains("  Test  · Not run"));

        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Build,
                "cargo",
                "cargo check",
            )),
        );
        let running = draw(&app, 80, 30);
        assert!(running.contains("Build  ▶ Running"));
        assert!(running.contains("cargo check"));

        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Completed(BuildTestResult::new(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
                BuildTestFreshness::Fresh,
                "cargo",
                "cargo check",
                Some(0),
                Duration::from_millis(850),
                "Passed",
                None,
            )),
        );
        let passed = draw(&app, 80, 30);
        assert!(passed.contains("Status"));
        assert!(passed.contains("Freshness"));
        assert!(passed.contains("Fresh"));
        assert!(passed.contains("Duration"));
        assert!(passed.contains("850ms"));

        app.apply_build_test_state(
            BuildTestKind::Build,
            completed_state(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
                BuildTestFreshness::Stale,
            ),
        );
        let stale = draw(&app, 80, 30);
        assert!(stale.contains("Passed"));
        assert!(stale.contains("Stale"));
        assert!(stale.contains("> Build  ! Passed (stale)"));

        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
            panels,
        );
        app.apply_build_test_state(
            BuildTestKind::Test,
            BuildTestState::Completed(BuildTestResult::new(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
                BuildTestFreshness::Fresh,
                "cargo",
                "cargo test",
                Some(101),
                Duration::from_secs(2),
                "1 failed",
                None,
            )),
        );
        let failed = draw(&app, 80, 30);
        assert!(failed.contains("Detail: Test"));
        assert!(failed.contains("Failed"));
        assert!(failed.contains("1 failed"));
        assert!(failed.contains("  Build  ! Passed (stale)"));
        assert!(failed.contains("> Test  ✕ Failed"));

        app.apply_build_test_state(
            BuildTestKind::Test,
            execution_error_state(BuildTestKind::Test),
        );
        let error = draw(&app, 80, 30);
        assert!(error.contains("Execution error"));
        assert!(error.contains("a detailed execution error"));
    }

    #[test]
    fn evidence_status_markers_preserve_source_specific_meaning() {
        assert_eq!(
            evidence_selector_status(&completed_state(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
                BuildTestFreshness::Fresh,
            )),
            "✓ Passed"
        );
        assert_eq!(
            evidence_selector_status(&completed_state(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
                BuildTestFreshness::Stale,
            )),
            "! Passed (stale)"
        );
        assert_eq!(
            evidence_selector_status(&completed_state(
                BuildTestKind::Build,
                BuildTestOutcome::Failed,
                BuildTestFreshness::Fresh,
            )),
            "✕ Failed"
        );
        assert_eq!(
            evidence_selector_status(&BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Build,
                "cargo",
                "cargo check",
            ))),
            "▶ Running"
        );
        assert!(
            !evidence_selector_status(&BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Build,
                "cargo",
                "cargo check",
            )))
            .contains('●')
        );
        assert_eq!(
            evidence_selector_status(&execution_error_state(BuildTestKind::Build)),
            "! Error"
        );
        assert_eq!(
            evidence_selector_status(&BuildTestState::NotRun),
            "· Not run"
        );
        assert_eq!(
            evidence_selector_status(&BuildTestState::Unavailable),
            "? Unavailable"
        );
        assert_eq!(
            artifact_selector_status(&devscope::progress::ArtifactStatus::Exists {
                kind: devscope::progress::ArtifactKind::File,
                size: 0,
            }),
            "✓ Exists"
        );
        assert_eq!(
            artifact_selector_status(&devscope::progress::ArtifactStatus::Missing),
            "· Missing"
        );
        assert_eq!(
            artifact_selector_status(&devscope::progress::ArtifactStatus::ObservationError {
                message: "denied".into(),
            }),
            "! Error"
        );
    }

    #[test]
    fn evidence_selector_and_preview_render_configured_artifact() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_artifact(Some(
            devscope::progress::ArtifactObservation::observation_error(
                "target/output.bin".into(),
                "permission denied",
            ),
        ));
        let panels = focusable_panels(100, 30);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            focusable_panels(100, 30),
        );

        assert_eq!(
            evidence_selector_lines(&app)[2],
            Line::from("> Artifact  ! Error")
        );
        let (title, detail) = artifact_preview(app.artifact());
        assert_eq!(title, "Detail: Artifact");
        let detail = format!("{detail:?}");
        assert!(detail.contains("target/output.bin"));
        assert!(detail.contains("permission denied"));
        assert!(!detail.contains("Freshness"));
    }
    #[test]
    fn evidence_preview_handles_unavailable_state() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        let panels = focusable_panels(80, 30);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        let output = draw(&app, 80, 30);
        assert!(output.contains("Detail: Build"));
        assert!(output.contains("Unavailable"));
    }
    #[test]
    fn renders_evidence_details_for_initial_running_and_completed_states() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(BuildTestKind::Build, BuildTestState::NotRun);
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        assert!(draw(&app, 120, 30).contains("Evidence"));
        app.select_evidence_detail(BuildTestKind::Build);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            focusable_panels(120, 30),
        );
        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Build,
                "hidden source",
                "cargo check",
            )),
        );
        let running = draw(&app, 120, 30);
        assert!(running.contains("Evidence"));
        assert!(running.contains("Build  ▶ Running"));
        assert!(running.contains("cargo check"));
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
        let passed = draw(&app, 120, 30);
        assert!(passed.contains("Build  ✓ Passed"));
        assert!(passed.contains("cargo check passed"));
    }

    #[test]
    fn renders_failed_stale_and_error_evidence_details() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.select_evidence_detail(BuildTestKind::Test);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            focusable_panels(120, 30),
        );
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
        let failed = draw(&app, 120, 30);
        assert!(failed.contains("Test  ✕ Failed"));
        assert!(failed.contains("Evidence"));
        assert!(failed.contains("cargo test failed"));
        assert!(failed.contains("Result"));
        app.apply_build_test_state(
            BuildTestKind::Test,
            completed_state(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
                BuildTestFreshness::Stale,
            ),
        );
        assert!(draw(&app, 120, 30).contains("Test  ✕ Failed (stale)"));
        app.apply_build_test_state(
            BuildTestKind::Test,
            execution_error_state(BuildTestKind::Test),
        );
        assert!(draw(&app, 120, 30).contains("Test  ! Error"));
        assert!(draw(&app, 120, 30).contains("a detailed execution error"));
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
        assert_eq!(evidence(&app), "Build · Not run | Test · Not run");
        assert!(draw(&app, 80, 30).contains("Evidence   Build · Not run | Test · Not run"));
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
        assert!(draw(&app, 80, 30).contains("Evidence   Build ▶ Running | Test · Not run"));
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
        assert!(draw(&app, 80, 30).contains("Evidence   Build ✓ Passed | Test ✕ Failed"));
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
        assert!(
            draw(&app, 80, 30).contains("Evidence   Build ! Passed (stale) | Test ? Unavailable")
        );
    }

    #[test]
    fn renders_evidence_execution_errors_without_details() {
        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_build_test_state(
            BuildTestKind::Build,
            execution_error_state(BuildTestKind::Build),
        );
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            focusable_panels(120, 30),
        );
        let output = draw(&app, 120, 30);
        assert!(output.contains("Evidence   Build ! Error | Test · Not run"));
        assert!(output.contains("Build  ! Error"));
        assert!(output.contains("a detailed execution error"));
        assert!(!output.contains("detailed result summary"));
    }

    #[test]
    fn renders_manual_evidence_controls_in_the_footer() {
        let app = app(TaskState::Unavailable, ActivityState::Unavailable);
        let output = draw(&app, 120, 30);
        assert!(output.contains("b:Build"));
        assert!(output.contains("t:Test"));
        assert!(output.contains("r:Reload"));
        assert!(output.contains("Enter:Preview"));
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

    fn preview_task(
        path: &str,
        line: usize,
        text: &str,
        heading: &str,
        context: &[&str],
    ) -> TaskSummaryItem {
        TaskSummaryItem::with_source_context(
            path.into(),
            line,
            text.into(),
            path.into(),
            Some(heading.into()),
            line.saturating_sub(2),
            context.iter().map(|line| (*line).into()).collect(),
        )
    }

    #[test]
    fn task_preview_shows_source_grounded_context_and_follows_selection() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(
                3,
                vec![
                    preview_task(
                        "docs/roadmap.md",
                        4,
                        "Detail View experiment",
                        "TUI",
                        &[
                            "## TUI",
                            "- [x] Previous",
                            "- [ ] Detail View experiment",
                            "- [ ] Artifact Evidence experiment",
                        ],
                    ),
                    preview_task(
                        "docs/roadmap.md",
                        4,
                        "Artifact Evidence experiment",
                        "Core observation",
                        &[
                            "## Core observation",
                            "- [ ] Detail View experiment",
                            "- [ ] Artifact Evidence experiment",
                            "- [ ] Progress history experiment",
                        ],
                    ),
                ],
            )),
            ActivityState::Unavailable,
        );
        let panels = focusable_panels(80, 30);
        let first = draw(&app, 80, 30);
        assert!(first.contains("Detail View experiment"));
        assert!(first.contains("Source"));
        assert!(first.contains("docs/roadmap.md"));
        assert!(first.contains("Section"));
        assert!(first.contains("TUI"));
        assert!(first.contains("Context"));
        assert!(first.contains("│ - [ ] Detail View experiment"));
        assert!(!first.contains("> - [ ] Detail View experiment"));

        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
            panels,
        );
        let second = draw(&app, 80, 30);
        assert!(second.contains("Artifact Evidence experiment"));
        assert!(second.contains("Core observation"));
        assert!(second.contains("│ - [ ] Artifact Evidence experiment"));
        assert!(!second.contains("> - [ ] Artifact Evidence experiment"));
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn task_list_marks_only_the_matching_current_work_parent() {
        let parent_task = "Explore verification integration for Build/Test Evidence";
        let summary = TaskSummary::new(
            3,
            vec![
                preview_task("docs/roadmap.md", 1, "Other task", "Core", &[]),
                preview_task("docs/roadmap.md", 2, parent_task, "Core", &[]),
                preview_task("docs/other.md", 3, parent_task, "Core", &[]),
            ],
        );
        let current_work = matching_work_state("docs\\roadmap.md", parent_task, "- [ ] Work item");

        let lines = task_lines(&summary, Some(0), 3, 100, &current_work);
        assert!(!line_text(&lines[0]).contains("[Work]"));
        assert!(line_text(&lines[1]).contains("[Work]"));
        assert!(!line_text(&lines[2]).contains("[Work]"));

        let lines = task_lines(&summary, Some(2), 3, 100, &current_work);
        assert!(line_text(&lines[1]).contains("[Work]"));
    }

    #[test]
    fn task_list_reserves_width_for_current_work_indicator() {
        let parent_task = "A long Current Work parent task that must retain its indicator";
        let summary = TaskSummary::new(
            1,
            vec![preview_task("docs/roadmap.md", 1, parent_task, "Core", &[])],
        );
        let current_work = matching_work_state("docs/roadmap.md", parent_task, "- [ ] Work item");

        let line = line_text(&task_lines(&summary, Some(0), 1, 30, &current_work)[0]);
        assert!(line.contains("[Work]"));
        assert!(line.contains('…'));
        assert!(Line::from(line).width() <= 30);

        let mut app = app(TaskState::Available(summary), ActivityState::Unavailable);
        app.apply_current_work(current_work);
        assert!(draw(&app, 70, 30).contains("[Work]"));

        app.apply_current_work(CurrentWorkState::Unavailable);
        assert!(!draw(&app, 70, 30).contains("[Work]"));
    }

    #[test]
    fn task_preview_shows_matching_current_work_without_affecting_unrelated_tasks() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(
                2,
                vec![
                    preview_task(
                        "docs/roadmap.md",
                        4,
                        "Explore verification integration for Build/Test Evidence",
                        "Core observation",
                        &["- [ ] Explore verification integration for Build/Test Evidence"],
                    ),
                    preview_task(
                        "docs/roadmap.md",
                        5,
                        "Artifact Evidence experiment",
                        "Core observation",
                        &["- [ ] Artifact Evidence experiment"],
                    ),
                ],
            )),
            ActivityState::Unavailable,
        );
        app.apply_current_work(matching_work_state(
            "docs\\roadmap.md",
            "Explore verification integration for Build/Test Evidence",
            "- [x] Show matching Work\n- [ ] Preserve source context",
        ));

        let panels = focusable_panels(120, 30);
        let matching = draw(&app, 120, 30);
        assert!(matching.contains("Current Work"));
        assert!(matching.contains("✓ Show matching Work"));
        assert!(matching.contains("□ Preserve source context"));
        assert!(matching.contains("Source"));
        assert!(matching.contains("Context"));

        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
            panels,
        );
        assert!(!draw(&app, 120, 30).contains("Current Work"));
    }

    #[test]
    fn task_preview_keeps_source_context_when_current_work_is_unavailable() {
        let mut app = app(
            TaskState::Available(TaskSummary::new(
                1,
                vec![preview_task(
                    "docs/roadmap.md",
                    4,
                    "Explore verification integration for Build/Test Evidence",
                    "Core observation",
                    &["- [ ] Explore verification integration for Build/Test Evidence"],
                )],
            )),
            ActivityState::Unavailable,
        );
        app.apply_current_work(CurrentWorkState::Unavailable);

        let output = draw(&app, 120, 30);
        assert!(!output.contains("Current Work"));
        assert!(output.contains("Source"));
        assert!(output.contains("Context"));
    }
    #[test]
    fn task_preview_handles_no_selection_and_unavailable_tasks() {
        let no_selection = app(
            TaskState::Available(TaskSummary::new(0, vec![])),
            ActivityState::Unavailable,
        );
        assert!(draw(&no_selection, 80, 30).contains("No task selected"));

        let unavailable = app(TaskState::Unavailable, ActivityState::Unavailable);
        assert!(draw(&unavailable, 80, 30).contains("No task selected"));
    }

    #[test]
    fn task_preview_keeps_primary_information_at_half_screen_width() {
        let app = app(
            TaskState::Available(TaskSummary::new(
                1,
                vec![preview_task(
                    "docs/roadmap.md",
                    4,
                    "Detail View experiment",
                    "TUI",
                    &["- [ ] Detail View experiment"],
                )],
            )),
            ActivityState::Unavailable,
        );
        let output = draw(&app, 80, 30);
        assert!(output.contains("Detail View experiment"));
        assert!(output.contains("docs/roadmap.md"));
        assert!(output.contains("TUI"));
    }
    #[test]
    fn renders_global_preview_for_each_focused_panel_and_preserves_toggle_state() {
        let mut app = app(
            TaskState::Unavailable,
            activity_with_files(vec![
                GitChangedFile {
                    path: "src/a.rs".into(),
                    status: GitFileStatus::Modified,
                    changes: Default::default(),
                },
                GitChangedFile {
                    path: "src/b.rs".into(),
                    status: GitFileStatus::Modified,
                    changes: Default::default(),
                },
            ]),
        );
        let panels = focusable_panels(120, 30);
        let task_preview = draw(&app, 120, 30);
        assert!(task_preview.contains("Project Progress"));
        assert!(task_preview.contains("Detail: Task"));
        assert!(task_preview.contains("No task selected"));
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        assert!(draw(&app, 120, 30).contains("Detail: Build"));
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        app.apply_preview_inspection(GitFileInspection::Diff {
            unstaged: Some(GitDiffText {
                text: "+preview-a".into(),
                truncated: false,
            }),
            staged: None,
        });
        let changed = draw(&app, 120, 30);
        assert!(changed.contains("Detail: src/a.rs"));
        assert!(changed.contains("+preview-a"));
        assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
            panels,
        );
        assert!(!app.preview_visible());
        assert!(!draw(&app, 120, 30).contains("Detail: src/a.rs"));
        assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);
        assert_eq!(app.selected_changed_file(), Some(0));
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
            panels,
        );
        assert!(app.preview_visible());
        assert!(draw(&app, 120, 30).contains("Detail: src/a.rs"));
    }

    #[test]
    fn global_preview_is_passive_follows_changed_file_selection_and_preserves_detail() {
        let mut app = app(
            TaskState::Unavailable,
            activity_with_files(vec![
                GitChangedFile {
                    path: "src/a.rs".into(),
                    status: GitFileStatus::Modified,
                    changes: Default::default(),
                },
                GitChangedFile {
                    path: "src/b.rs".into(),
                    status: GitFileStatus::Modified,
                    changes: Default::default(),
                },
            ]),
        );
        let panels = focusable_panels(120, 30);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            panels,
        );
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
            panels,
        );
        app.apply_preview_inspection(GitFileInspection::Diff {
            unstaged: Some(GitDiffText {
                text: "+preview-b".into(),
                truncated: false,
            }),
            staged: None,
        });
        assert!(draw(&app, 120, 30).contains("Detail: src/b.rs"));
        assert!(draw(&app, 120, 30).contains("+preview-b"));
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            panels,
        );
        assert!(app.has_detail_view());
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            panels,
        );
        assert!(!app.has_detail_view());
        assert_eq!(app.selected_changed_file(), Some(1));
    }

    #[test]
    fn global_preview_uses_pane_minima_and_respects_responsive_fallbacks() {
        let (navigation, preview) = preview_pane_widths(78);
        assert_eq!(navigation, MIN_NAVIGATION_PANE_WIDTH);
        assert_eq!(preview, MIN_PREVIEW_PANE_WIDTH);
        assert!(has_global_preview(78, 30, true));
        assert!(!has_global_preview(77, 30, true));
        assert!(has_global_preview(80, 25, true));
        assert!(!has_global_preview(78, 24, true));
        assert!(!has_global_preview(78, 30, false));
        let mut preview_app = app(TaskState::Unavailable, ActivityState::Unavailable);
        assert!(has_global_preview(80, 30, preview_app.preview_visible()));
        assert!(!has_global_preview(80, 24, preview_app.preview_visible()));
        assert!(has_global_preview(80, 30, preview_app.preview_visible()));
        preview_app.toggle_preview();
        assert!(!has_global_preview(80, 30, preview_app.preview_visible()));
        let fresh_app = app(TaskState::Unavailable, ActivityState::Unavailable);
        assert!(draw(&fresh_app, 80, 30).contains("Detail: Task"));
        assert!(!draw(&fresh_app, 77, 30).contains("Detail:"));
        assert!(!draw(&fresh_app, 78, 24).contains("Detail:"));
    }

    #[test]
    fn footer_advertises_preview_only_when_the_layout_can_show_it() {
        assert!(footer_text(80, 30).contains("Enter:Preview"));
        assert!(!footer_text(77, 30).contains("Enter:Preview"));
        assert!(!footer_text(80, 24).contains("Enter:Preview"));
    }
    #[test]
    fn maps_git_file_statuses_to_short_prefixes() {
        assert_eq!(git_file_status(&GitFileStatus::Modified), "M");
        assert_eq!(git_file_status(&GitFileStatus::Added), "A");
        assert_eq!(git_file_status(&GitFileStatus::Deleted), "D");
        assert_eq!(git_file_status(&GitFileStatus::Renamed), "R");
    }

    #[test]
    fn file_content_inspection_renders_mode_and_scrolls_in_preview_and_detail() {
        let mut app = app(
            TaskState::Unavailable,
            activity_with_files(vec![GitChangedFile {
                path: "new.txt".into(),
                status: GitFileStatus::Added,
                changes: GitChangeCounts::unavailable(),
            }]),
        );
        let panels = focusable_panels(80, 30);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            panels,
        );
        let inspection = GitFileInspection::FileContent {
            text: (0..40).map(|i| format!("current line {i:02}\n")).collect(),
            truncated: true,
        };
        app.apply_preview_inspection(inspection.clone());
        let first = draw(&app, 80, 30);
        assert!(first.contains("Detail: new.txt"));
        assert!(first.contains("File content"));
        assert!(first.contains("current line 00"));
        assert!(!first.contains("Git diff"));
        let limit = preview_scroll_limit(&app, Rect::new(0, 0, 80, 30));
        assert!(limit > 0);
        app.scroll_preview(isize::MAX, limit);
        let scrolled = draw(&app, 80, 30);
        assert!(!scrolled.contains("current line 00"));
        assert!(scrolled.contains("current line 39"));
        assert!(scrolled.contains("file content truncated"));
        for (width, height) in [(160, 40), (80, 25), (60, 20), (1, 1)] {
            let _ = draw(&app, width, height);
        }
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            panels,
        );
        app.apply_detail_inspection(inspection);
        let detail = draw(&app, 80, 30);
        assert!(detail.contains("Changed File Detail"));
        assert!(detail.contains("Added"));
        assert!(detail.contains("File content"));
        assert!(detail.contains("current line 00"));
        let limit = detail_scroll_limit(&app, Rect::new(0, 0, 80, 30));
        app.scroll_detail(limit as isize, limit);
        assert!(draw(&app, 80, 30).contains("... file content truncated ..."));
    }

    #[test]
    fn inspection_unavailable_reasons_are_explicit() {
        for (reason, expected) in [
            (GitFileInspectionUnavailable::Missing, "File is missing"),
            (
                GitFileInspectionUnavailable::UnsafePath,
                "Unsafe project-relative path",
            ),
            (
                GitFileInspectionUnavailable::Symlink,
                "Symlink / reparse point is not followed",
            ),
            (
                GitFileInspectionUnavailable::NotRegularFile,
                "Not a regular file",
            ),
            (
                GitFileInspectionUnavailable::Binary,
                "Binary or unsupported UTF-8 text",
            ),
            (
                GitFileInspectionUnavailable::ReadError,
                "File could not be read",
            ),
            (
                GitFileInspectionUnavailable::GitError,
                "Git diff collection failed",
            ),
        ] {
            let lines = detail_inspection_lines(Some(&GitFileInspection::Unavailable(reason)));
            assert_eq!(lines[0].to_string(), "Inspection unavailable");
            assert_eq!(lines[1].to_string(), expected);
        }
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
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            panels,
        );

        app.apply_detail_inspection(GitFileInspection::Diff {
            unstaged: Some(GitDiffText {
                text: "@@ -1 +1 @@\n-old\n+new\n".into(),
                truncated: false,
            }),
            staged: Some(GitDiffText {
                text: "@@ -1 +1 @@\n-staged-old\n+staged-new\n".into(),
                truncated: true,
            }),
        });
        let output = draw(&app, 80, 30);
        assert!(output.contains("Changed File Detail"));
        assert!(output.contains("src/ui.rs"));
        assert!(output.contains("Modified"));
        assert!(output.contains("+12 -4"));
        assert!(output.contains("Git diff"));
        assert!(output.contains("Unstaged"));
        assert!(output.contains("-old"));
        assert!(output.contains("+new"));
        assert!(output.contains("Staged"));
        assert!(output.contains("... diff truncated ..."));
        assert!(output.contains("j/k: Scroll  Enter/Esc: Back  q: Quit"));
        assert_eq!(change_summary(Default::default()), "unavailable");
        for (width, height) in [(40, 18), (20, 5), (1, 1)] {
            let _ = draw(&app, width, height);
        }
    }
    #[test]
    fn aligns_change_counts_to_the_visible_path_column() {
        let file = |path: &str, additions: Option<u64>, deletions: Option<u64>| GitChangedFile {
            path: path.into(),
            status: GitFileStatus::Modified,
            changes: GitChangeCounts {
                additions,
                deletions,
            },
        };
        let files = vec![
            file("short.rs", Some(1), Some(1)),
            file("longer.rs", Some(2), Some(3)),
            file("unknown.bin", None, None),
        ];
        let activity = activity_with_files(files.clone());
        let wide = changed_files(&activity, Some(1), 3, 80);
        let first_counts = wide[0].to_string().find("+1 -1").unwrap();
        let second_counts = wide[1].to_string().find("+2 -3").unwrap();
        assert_eq!(first_counts, second_counts);
        assert!(wide[1].to_string().starts_with("> M  longer.rs"));
        assert!(wide[2].to_string().contains("M  unknown.bin"));
        assert!(!wide[2].to_string().contains('+'));

        let unicode = file("src/日本語.rs", Some(4), Some(2));
        let unicode_prefix = Line::from(changed_file_prefix(&unicode, false)).width();
        assert_eq!(
            change_counts_column(std::slice::from_ref(&unicode), 80),
            unicode_prefix + CHANGE_COUNTS_GAP
        );

        let narrow = changed_files(&activity, Some(1), 3, 18);
        assert!(narrow[0].to_string().contains("M  short.rs"));
        assert!(!narrow[0].to_string().contains("+1 -1"));
        assert!(narrow[1].to_string().starts_with("> M  longer.rs"));
        assert!(!narrow[1].to_string().contains("+2 -3"));
    }

    #[test]
    fn limits_change_counts_to_visible_paths_and_the_column_cap() {
        let file = |path: &str| GitChangedFile {
            path: path.into(),
            status: GitFileStatus::Modified,
            changes: GitChangeCounts {
                additions: Some(1),
                deletions: Some(1),
            },
        };
        let visible = vec![file("a.rs"), file("longer-visible.rs"), file("b.rs")];
        let mut all_files = visible.clone();
        all_files.push(file(
            "offscreen-path-that-must-not-move-the-counts-column.rs",
        ));
        all_files.push(file("another-offscreen-path.rs"));
        let visible_lines = changed_files(&activity_with_files(all_files), Some(0), 4, 80);
        let expected_column =
            Line::from(changed_file_prefix(&visible[1], false)).width() + CHANGE_COUNTS_GAP;
        assert_eq!(
            visible_lines[0].to_string().find("+1 -1"),
            Some(expected_column)
        );
        assert!(visible_lines[3].to_string().contains("... and 2 more"));

        let capped = vec![file("short.rs"), file(&"very-long-path-".repeat(8))];
        let capped_lines = changed_files(&activity_with_files(capped.clone()), Some(0), 2, 80);
        assert_eq!(change_counts_column(&capped, 80), MAX_CHANGE_COUNTS_COLUMN);
        assert_eq!(
            capped_lines[0].to_string().find("+1 -1"),
            Some(MAX_CHANGE_COUNTS_COLUMN)
        );
        assert!(!capped_lines[1].to_string().contains("+1 -1"));
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
        assert!(small.contains("Tasks"));
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
        assert!(tasks_focused.contains("Tasks"));
        assert!(tasks_focused.contains("Evidence"));

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let evidence_focused = draw(&app, 80, 30);
        assert!(evidence_focused.contains("Tasks"));
        assert!(evidence_focused.contains("Evidence"));
        assert_ne!(tasks_focused, evidence_focused);
    }
    #[test]
    fn now_uses_only_the_explicit_active_item() {
        let no_active = work_state("- [ ] Next candidate\n- [ ] Other item\n");
        let no_active_label = now_label(&no_active, 80);
        assert!(no_active_label.contains("Not set"));
        assert!(!no_active_label.contains("Next candidate"));

        let active = work_state("Active: 2\n- [ ] Next candidate\n- [ ] Explicit active item\n");
        let active_label = now_label(&active, 80);
        assert!(active_label.contains("Explicit active item"));
        assert!(!active_label.contains("Next candidate"));
        assert!(now_label(&CurrentWorkState::NotSet, 80).contains("Not set"));
        assert!(now_label(&CurrentWorkState::Unavailable, 80).contains("Unavailable"));
    }

    #[test]
    fn now_truncates_and_refreshes_with_current_work_state() {
        let long = "A deliberately long explicit Active Current Work item that must remain on one header line";
        let active = work_state(&format!("Active: 1\n- [ ] {long}\n"));
        let label = now_label(&active, 40);
        assert!(label.contains('…'));
        assert!(Line::from(label).width() <= 40);

        let mut app = app(TaskState::Unavailable, ActivityState::Unavailable);
        app.apply_current_work(active);
        assert!(draw(&app, 80, 30).contains("● NOW  A deliberately long"));
        app.apply_current_work(work_state("- [ ] Next candidate\n"));
        let refreshed = draw(&app, 40, 18);
        assert!(refreshed.contains("NOW"));
        assert!(refreshed.contains("Not set"));
        assert!(!refreshed.contains("Next candidate"));
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
    fn project_progress_keeps_overview_states_visible_across_layouts_and_preview() {
        let mut app = App::new(ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(2, 4)),
            activity_with_files(vec![GitChangedFile {
                path: "src/ui.rs".into(),
                status: GitFileStatus::Modified,
                changes: Default::default(),
            }]),
            TaskState::Unavailable,
        ));
        app.apply_current_work(work_state("- [x] Done\n- [ ] Next\n"));
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
            BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Test,
                "cargo",
                "cargo test",
            )),
        );

        for (width, height) in [(80, 30), (80, 25), (40, 18)] {
            let output = draw(&app, width, height);
            assert!(output.contains("Plan"));
            assert!(output.contains("50% 2/4"));
            assert!(output.contains("Work"));
            assert!(output.contains("50% 1/2"));
            assert!(output.contains("Activity   1 changed file"));
            assert!(output.contains("Evidence"));
            assert!(output.contains("Build ✓ Passed"));
            if width >= 80 {
                assert!(output.contains("Evidence   Build ✓ Passed | Test ▶ Running"));
            }
        }

        app.toggle_preview();
        let without_preview = draw(&app, 80, 30);
        assert!(without_preview.contains("50% 2/4"));
        assert!(without_preview.contains("50% 1/2"));
        assert!(without_preview.contains("Evidence   Build ✓ Passed | Test ▶ Running"));
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
            " Tasks ",
            " Evidence ",
            " Changed Files ",
            " Recent Commits ",
        ] {
            assert!(output.contains(title));
        }
    }
}
