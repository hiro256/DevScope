use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::app::App;

/// Draws the initial project overview without depending on progress data sources.
pub fn render(frame: &mut Frame, _app: &App) {
    let areas = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(frame.area());

    let title = Paragraph::new("DevScope").style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(title, areas[0]);

    let progress = Paragraph::new(vec![
        Line::from("Plan       Not available"),
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
