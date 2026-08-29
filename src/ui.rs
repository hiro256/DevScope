use crate::app::{App, PlanState};
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
        Constraint::Length(1),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new("DevScope").style(Style::default().add_modifier(Modifier::BOLD)),
        areas[0],
    );
    let progress = Paragraph::new(vec![
        Line::from(format!("Plan       {}", plan_status_text(app.plan()))),
        Line::from("Activity   Not available"),
        Line::from("Evidence   Not available"),
        Line::from("Agent      Not available"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Project Progress"),
    );
    frame.render_widget(progress, areas[1]);
    frame.render_widget(Paragraph::new("q / Esc: Quit"), areas[2]);
}

fn plan_status_text(plan: PlanState) -> String {
    match plan {
        PlanState::Available(summary) if summary.total() == 0 => "No tasks found".to_owned(),
        PlanState::Available(summary) => format!(
            "{} / {} tasks complete",
            summary.completed(),
            summary.total()
        ),
        PlanState::Unavailable => "Unavailable".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::plan_status_text;
    use crate::app::PlanState;
    use devscope::progress::PlanSummary;
    #[test]
    fn formats_completed_plan_progress() {
        assert_eq!(
            plan_status_text(PlanState::Available(PlanSummary::new(3, 5))),
            "3 / 5 tasks complete"
        );
    }
    #[test]
    fn formats_empty_plan_progress() {
        assert_eq!(
            plan_status_text(PlanState::Available(PlanSummary::new(0, 0))),
            "No tasks found"
        );
    }
}
