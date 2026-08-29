use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use devscope::progress::{ActivitySummary, PlanSummary, TaskSummary};
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    Available(TaskSummary),
    Unavailable,
}
pub struct App {
    running: bool,
    plan: PlanState,
    activity: ActivityState,
    tasks: TaskState,
}
impl App {
    pub fn new(plan: PlanState, activity: ActivityState, tasks: TaskState) -> Self {
        Self {
            running: true,
            plan,
            activity,
            tasks,
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
    pub fn tasks(&self) -> &TaskState {
        &self.tasks
    }
    pub fn handle_key(&mut self, k: KeyEvent) {
        if k.kind == KeyEventKind::Press && matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
            self.running = false
        }
    }
}
#[cfg(test)]
mod restored_tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use devscope::progress::PlanSummary;
    fn app() -> App {
        App::new(
            PlanState::Available(PlanSummary::new(1, 2)),
            ActivityState::Unavailable,
            TaskState::Unavailable,
        )
    }
    #[test]
    fn q_exits() {
        let mut a = app();
        a.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!a.is_running());
    }
    #[test]
    fn escape_exits() {
        let mut a = app();
        a.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!a.is_running());
    }
    #[test]
    fn retains_states() {
        let a = app();
        assert_eq!(a.plan(), PlanState::Available(PlanSummary::new(1, 2)));
        assert_eq!(a.activity(), &ActivityState::Unavailable);
        assert_eq!(a.tasks(), &TaskState::Unavailable);
    }
}
