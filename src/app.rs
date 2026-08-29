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
    selected_task: Option<usize>,
}
impl App {
    pub fn new(plan: PlanState, activity: ActivityState, tasks: TaskState) -> Self {
        let selected_task = match &tasks {
            TaskState::Available(s) if s.remaining() > 0 => Some(0),
            _ => None,
        };
        Self {
            running: true,
            plan,
            activity,
            tasks,
            selected_task,
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
    pub const fn selected_task(&self) -> Option<usize> {
        self.selected_task
    }
    pub fn handle_key(&mut self, k: KeyEvent) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            _ => {}
        }
    }
    fn move_selection(&mut self, delta: isize) {
        let TaskState::Available(s) = &self.tasks else {
            return;
        };
        let Some(current) = self.selected_task else {
            return;
        };
        let last = s.remaining().saturating_sub(1);
        self.selected_task = Some((current as isize + delta).clamp(0, last as isize) as usize)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use devscope::progress::{PlanSummary, TaskSummaryItem};
    fn app(n: usize) -> App {
        let items = (0..n)
            .map(|i| TaskSummaryItem::new("a.md".into(), i, "x".into()))
            .collect();
        App::new(
            PlanState::Available(PlanSummary::new(0, n)),
            ActivityState::Unavailable,
            TaskState::Available(TaskSummary::new(n, items)),
        )
    }
    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }
    #[test]
    fn navigation_clamps() {
        let mut a = app(3);
        assert_eq!(a.selected_task(), Some(0));
        a.handle_key(key(KeyCode::Up));
        assert_eq!(a.selected_task(), Some(0));
        a.handle_key(key(KeyCode::Down));
        a.handle_key(key(KeyCode::Char('j')));
        a.handle_key(key(KeyCode::Down));
        assert_eq!(a.selected_task(), Some(2));
        a.handle_key(key(KeyCode::Char('k')));
        assert_eq!(a.selected_task(), Some(1));
    }
    #[test]
    fn empty_and_unavailable_unselected() {
        assert_eq!(app(0).selected_task(), None);
        let a = App::new(
            PlanState::Unavailable,
            ActivityState::Unavailable,
            TaskState::Unavailable,
        );
        assert_eq!(a.selected_task(), None);
    }
    #[test]
    fn q_and_escape_exit() {
        let mut a = app(1);
        a.handle_key(key(KeyCode::Char('q')));
        assert!(!a.is_running());
        let mut a = app(1);
        a.handle_key(key(KeyCode::Esc));
        assert!(!a.is_running());
    }
}
