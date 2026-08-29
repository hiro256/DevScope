use crate::app::{ActivityState, App, PlanState, TaskState};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};
const TASK_LIMIT: usize = 5;
pub fn render(f: &mut Frame, a: &App) {
    let x = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(9),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .split(f.area());
    f.render_widget(
        Paragraph::new("DevScope").style(Style::default().add_modifier(Modifier::BOLD)),
        x[0],
    );
    f.render_widget(
        Paragraph::new(vec![
            Line::from(format!("Plan       {}", plan(a.plan()))),
            Line::from(format!("Activity   {}", activity(a.activity()))),
            Line::from("Evidence   Not available"),
            Line::from("Agent      Not available"),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Project Progress"),
        ),
        x[1],
    );
    f.render_widget(
        Paragraph::new(tasks(a.tasks()))
            .block(Block::default().borders(Borders::ALL).title("Task Summary")),
        x[2],
    );
    f.render_widget(
        Paragraph::new(commits(a.activity())).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Recent Commits"),
        ),
        x[3],
    );
    f.render_widget(Paragraph::new("q / Esc: Quit"), x[4]);
}
fn plan(p: PlanState) -> String {
    match p {
        PlanState::Available(s) if s.total() == 0 => "No tasks found".into(),
        PlanState::Available(s) => format!("{} / {} tasks complete", s.completed(), s.total()),
        PlanState::Unavailable => "Unavailable".into(),
    }
}
fn activity(a: &ActivityState) -> String {
    match a {
        ActivityState::Available(s) if s.changed_files() == 0 => "Clean".into(),
        ActivityState::Available(s) => format!(
            "{} changed file{}",
            s.changed_files(),
            if s.changed_files() == 1 { "" } else { "s" }
        ),
        ActivityState::NotRepository => "Not a Git repository".into(),
        ActivityState::Unavailable => "Unavailable".into(),
    }
}
fn tasks(t: &TaskState) -> Vec<Line<'static>> {
    match t {
        TaskState::Unavailable => vec![Line::from("Unavailable")],
        TaskState::Available(s) if s.total() == 0 => vec![Line::from("No tasks found")],
        TaskState::Available(s) if s.remaining() == 0 => vec![Line::from("All tasks completed")],
        TaskState::Available(s) => {
            let mut v: Vec<_> = s
                .items()
                .iter()
                .take(TASK_LIMIT)
                .map(|i| Line::from(format!("□ {}", i.text())))
                .collect();
            if s.remaining() > TASK_LIMIT {
                v.push(Line::from(format!(
                    "... and {} more",
                    s.remaining() - TASK_LIMIT
                )))
            }
            v
        }
    }
}
fn commits(a: &ActivityState) -> Vec<Line<'static>> {
    match a {
        ActivityState::Available(s) if s.recent_commits().is_empty() => {
            vec![Line::from("No commits yet")]
        }
        ActivityState::Available(s) => s
            .recent_commits()
            .iter()
            .take(3)
            .map(|c| Line::from(format!("{}  {}", c.id, c.summary)))
            .collect(),
        _ => vec![Line::from("Unavailable")],
    }
}
#[cfg(test)]
mod task_ui_tests {
    use super::*;
    use crate::app::{ActivityState, PlanState, TaskState};
    use devscope::progress::{PlanSummary, TaskSummary, TaskSummaryItem};
    use ratatui::{Terminal, backend::TestBackend};
    fn draw(tasks: TaskState) -> String {
        let a = App::new(
            PlanState::Available(PlanSummary::new(0, 0)),
            ActivityState::Unavailable,
            tasks,
        );
        let mut t = Terminal::new(TestBackend::new(70, 30)).unwrap();
        t.draw(|f| render(f, &a)).unwrap();
        t.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }
    #[test]
    fn renders_tasks_and_overflow() {
        let items = (0..6)
            .map(|i| TaskSummaryItem::new("a.md".into(), i + 1, format!("Task {i}")))
            .collect();
        let s = draw(TaskState::Available(TaskSummary::new(6, items)));
        assert!(s.contains("Task 0"));
        assert!(s.contains("... and 1 more"));
    }
    #[test]
    fn renders_empty_and_completed() {
        assert!(draw(TaskState::Available(TaskSummary::new(0, vec![]))).contains("No tasks found"));
        assert!(
            draw(TaskState::Available(TaskSummary::new(1, vec![]))).contains("All tasks completed")
        );
    }
}

#[cfg(test)]
mod overview_regression_tests {
    use super::*;
    use crate::app::{ActivityState, PlanState, TaskState};
    use devscope::progress::{ActivitySummary, GitActivity, GitCommit, PlanSummary, TaskSummary};
    use ratatui::{Terminal, backend::TestBackend};
    fn draw_all(activity: ActivityState) -> String {
        let a = App::new(
            PlanState::Available(PlanSummary::new(3, 5)),
            activity,
            TaskState::Available(TaskSummary::new(0, vec![])),
        );
        let mut t = Terminal::new(TestBackend::new(80, 30)).unwrap();
        t.draw(|f| render(f, &a)).unwrap();
        t.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }
    #[test]
    fn renders_plan_clean_and_recent_commit() {
        let g = GitActivity {
            changed_files: vec![],
            recent_commits: vec![GitCommit {
                id: "abc".into(),
                summary: "newest".into(),
            }],
        };
        let s = draw_all(ActivityState::Available(ActivitySummary::from(&g)));
        assert!(s.contains("3 / 5 tasks complete"));
        assert!(s.contains("Activity   Clean"));
        assert!(s.contains("newest"));
    }
    #[test]
    fn renders_activity_unavailable_and_pluralization() {
        assert!(draw_all(ActivityState::Unavailable).contains("Activity   Unavailable"));
        let g = GitActivity {
            changed_files: vec![devscope::progress::GitChangedFile {
                path: "a".into(),
                status: devscope::progress::GitFileStatus::Modified,
            }],
            recent_commits: vec![],
        };
        assert!(
            draw_all(ActivityState::Available(ActivitySummary::from(&g)))
                .contains("1 changed file")
        );
        let g = GitActivity {
            changed_files: vec![
                devscope::progress::GitChangedFile {
                    path: "a".into(),
                    status: devscope::progress::GitFileStatus::Modified,
                },
                devscope::progress::GitChangedFile {
                    path: "b".into(),
                    status: devscope::progress::GitFileStatus::Modified,
                },
            ],
            recent_commits: vec![],
        };
        assert!(
            draw_all(ActivityState::Available(ActivitySummary::from(&g)))
                .contains("2 changed files")
        );
    }
}
