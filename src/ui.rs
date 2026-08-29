use crate::app::{ActivityState, App, PlanState};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

pub fn render(frame: &mut Frame, app: &App) {
    let areas = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new("DevScope").style(Style::default().add_modifier(Modifier::BOLD)),
        areas[0],
    );
    let overview = Paragraph::new(vec![
        Line::from(format!("Plan       {}", plan_text(app.plan()))),
        Line::from(format!("Activity   {}", activity_text(app.activity()))),
        Line::from("Evidence   Not available"),
        Line::from("Agent      Not available"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Project Progress"),
    );
    frame.render_widget(overview, areas[1]);
    let commits = Paragraph::new(commit_lines(app.activity())).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Recent Commits"),
    );
    frame.render_widget(commits, areas[2]);
    frame.render_widget(Paragraph::new("q / Esc: Quit"), areas[3]);
}
fn plan_text(plan: PlanState) -> String {
    match plan {
        PlanState::Available(s) if s.total() == 0 => "No tasks found".into(),
        PlanState::Available(s) => format!("{} / {} tasks complete", s.completed(), s.total()),
        PlanState::Unavailable => "Unavailable".into(),
    }
}
fn activity_text(state: &ActivityState) -> String {
    match state {
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
fn commit_lines(state: &ActivityState) -> Vec<Line<'static>> {
    match state {
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
mod tests {
    use super::*;
    use crate::app::{ActivityState, PlanState};
    use devscope::progress::{ActivitySummary, GitActivity, GitCommit, PlanSummary};
    use ratatui::{Terminal, backend::TestBackend};
    fn text(app: App) -> String {
        let mut t = Terminal::new(TestBackend::new(60, 15)).unwrap();
        t.draw(|f| render(f, &app)).unwrap();
        t.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }
    #[test]
    fn renders_plan_and_activity() {
        let a = App::new(
            PlanState::Available(PlanSummary::new(3, 5)),
            ActivityState::Available(ActivitySummary::from(&GitActivity {
                changed_files: vec![],
                recent_commits: vec![],
            })),
        );
        let s = text(a);
        assert!(s.contains("3 / 5 tasks complete"));
        assert!(s.contains("Activity   Clean"));
    }
    #[test]
    fn renders_activity_unavailable() {
        assert!(
            text(App::new(PlanState::Unavailable, ActivityState::Unavailable))
                .contains("Activity   Unavailable")
        );
    }
    #[test]
    fn renders_recent_commit() {
        let g = GitActivity {
            changed_files: vec![],
            recent_commits: vec![GitCommit {
                id: "abc1234".into(),
                summary: "newest".into(),
            }],
        };
        assert!(
            text(App::new(
                PlanState::Unavailable,
                ActivityState::Available(ActivitySummary::from(&g))
            ))
            .contains("newest")
        );
    }
}
