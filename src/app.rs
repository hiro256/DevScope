use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use devscope::progress::PlanSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanState {
    Available(PlanSummary),
    Unavailable,
}

pub struct App {
    running: bool,
    plan: PlanState,
}

impl App {
    pub const fn new(plan: PlanState) -> Self {
        Self {
            running: true,
            plan,
        }
    }
    pub const fn is_running(&self) -> bool {
        self.running
    }
    pub const fn plan(&self) -> PlanState {
        self.plan
    }
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Press && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            self.running = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{App, PlanState};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use devscope::progress::PlanSummary;
    fn app() -> App {
        App::new(PlanState::Available(PlanSummary::new(1, 2)))
    }
    #[test]
    fn q_exits_the_application() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!app.is_running());
    }
    #[test]
    fn escape_exits_the_application() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.is_running());
    }
    #[test]
    fn retains_the_plan_state() {
        assert_eq!(app().plan(), PlanState::Available(PlanSummary::new(1, 2)));
    }
}
