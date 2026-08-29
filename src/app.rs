use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use devscope::progress::{ActivitySummary, PlanSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanState {
    Available(PlanSummary),
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityState {
    Available(ActivitySummary),
    NotRepository,
    Unavailable,
}
pub struct App {
    running: bool,
    plan: PlanState,
    activity: ActivityState,
}
impl App {
    pub fn new(plan: PlanState, activity: ActivityState) -> Self {
        Self {
            running: true,
            plan,
            activity,
        }
    }
    pub const fn is_running(&self) -> bool {
        self.running
    }
    pub const fn plan(&self) -> PlanState {
        self.plan
    }
    pub fn activity(&self) -> &ActivityState {
        &self.activity
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
    use super::*;
    use crossterm::event::KeyModifiers;
    fn app() -> App {
        App::new(
            PlanState::Available(PlanSummary::new(1, 2)),
            ActivityState::Unavailable,
        )
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
    fn retains_plan_and_activity_state() {
        let app = app();
        assert_eq!(app.plan(), PlanState::Available(PlanSummary::new(1, 2)));
        assert_eq!(app.activity(), &ActivityState::Unavailable);
    }
}
