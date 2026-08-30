use crate::app::{ActivityState, App, PlanState, TaskState};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

const COMPACT_WIDTH: u16 = 20;
const COMPACT_HEIGHT: u16 = 12;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < COMPACT_WIDTH || area.height < COMPACT_HEIGHT {
        render_compact(frame, area);
        return;
    }

    let panels = if area.height >= 23 {
        Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(6),
            Constraint::Length(9),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(6),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area)
    };

    frame.render_widget(
        Paragraph::new("DevScope").style(Style::default().add_modifier(Modifier::BOLD)),
        panels[0],
    );
    frame.render_widget(project_progress(app), panels[1]);

    let task_area = panels[2];
    frame.render_widget(
        Paragraph::new(tasks(
            app.tasks(),
            app.selected_task(),
            inner_height(task_area),
        ))
        .block(Block::default().borders(Borders::ALL).title("Task Summary")),
        task_area,
    );

    if panels.len() == 5 {
        let commit_area = panels[3];
        frame.render_widget(
            Paragraph::new(commits(app.activity(), inner_height(commit_area))).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Recent Commits"),
            ),
            commit_area,
        );
    }

    frame.render_widget(
        Paragraph::new("r: Reload  q / Esc: Quit"),
        panels[panels.len() - 1],
    );
}

fn render_compact(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new("DevScope\nTerminal too small\nq / Esc: Quit"),
        area,
    );
}

fn project_progress(app: &App) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(format!("Plan       {}", plan(app.plan()))),
        Line::from(format!("Activity   {}", activity(app.activity()))),
        Line::from("Evidence   Not available"),
        Line::from("Agent      Not available"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Project Progress"),
    )
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
        ActivitySummary, GitActivity, GitChangedFile, GitCommit, GitFileStatus, PlanSummary,
        TaskSummary, TaskSummaryItem,
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

    #[test]
    fn renders_tasks_and_overflow() {
        let app = app(
            TaskState::Available(TaskSummary::new(8, task_items(8))),
            ActivityState::Unavailable,
        );
        let output = draw(&app, 70, 30);
        assert!(output.contains("Task 0"));
        assert!(output.contains("... and 2 more"));
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
        assert!(draw(&app, 40, 15).contains("> □ Task 7"));
    }
}
