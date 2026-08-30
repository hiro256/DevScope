use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use devscope::project::ProjectSnapshot;

pub use devscope::project::{ActivityState, PlanState, TaskState};

pub struct App {
    running: bool,
    plan: PlanState,
    activity: ActivityState,
    tasks: TaskState,
    selected_task: Option<usize>,
}

impl App {
    pub fn new(snapshot: ProjectSnapshot) -> Self {
        let (plan, activity, tasks) = snapshot.into_parts();
        let selected_task = match &tasks {
            TaskState::Available(summary) if summary.remaining() > 0 => Some(0),
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

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let TaskState::Available(summary) = &self.tasks else {
            return;
        };
        let Some(current) = self.selected_task else {
            return;
        };
        let last = summary.remaining().saturating_sub(1);
        self.selected_task = Some((current as isize + delta).clamp(0, last as isize) as usize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use devscope::{
        progress::{PlanSummary, TaskSummary, TaskSummaryItem},
        project::ProjectSnapshot,
    };

    fn app(count: usize) -> App {
        let items = (0..count)
            .map(|index| TaskSummaryItem::new("a.md".into(), index, "x".into()))
            .collect();
        App::new(ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(0, count)),
            ActivityState::Unavailable,
            TaskState::Available(TaskSummary::new(count, items)),
        ))
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn navigation_clamps() {
        let mut app = app(3);
        assert_eq!(app.selected_task(), Some(0));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected_task(), Some(0));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_task(), Some(2));
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.selected_task(), Some(1));
    }

    #[test]
    fn empty_and_unavailable_are_unselected() {
        assert_eq!(app(0).selected_task(), None);
        let app = App::new(ProjectSnapshot::unavailable());
        assert_eq!(app.selected_task(), None);
    }

    #[test]
    fn q_and_escape_exit() {
        let mut first_app = app(1);
        first_app.handle_key(key(KeyCode::Char('q')));
        assert!(!first_app.is_running());
        let mut second_app = app(1);
        second_app.handle_key(key(KeyCode::Esc));
        assert!(!second_app.is_running());
    }
}
