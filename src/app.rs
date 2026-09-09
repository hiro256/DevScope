use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use devscope::{
    current_work::CurrentWork,
    progress::{BuildTestKind, BuildTestState},
    project::ProjectSnapshot,
};

pub use devscope::project::{ActivityState, PlanState, TaskState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshSource {
    Initial,
    Manual,
    Markdown,
    Git,
    MarkdownAndGit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefreshStatus {
    last_update: Duration,
    last_source: RefreshSource,
    retry_pending: bool,
}

impl RefreshStatus {
    const fn initial() -> Self {
        Self {
            last_update: Duration::ZERO,
            last_source: RefreshSource::Initial,
            retry_pending: false,
        }
    }

    pub const fn last_update(&self) -> Duration {
        self.last_update
    }

    pub const fn last_source(&self) -> RefreshSource {
        self.last_source
    }

    pub const fn retry_pending(&self) -> bool {
        self.retry_pending
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CurrentWorkState {
    NotSet,
    Available(CurrentWork),
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusedPanel {
    Tasks,
    Evidence,
    ChangedFiles,
}

pub struct App {
    running: bool,
    plan: PlanState,
    activity: ActivityState,
    tasks: TaskState,
    build_test_build: BuildTestState,
    build_test_test: BuildTestState,
    evidence_detail_kind: Option<BuildTestKind>,
    focused_panel: FocusedPanel,
    selected_task: Option<usize>,
    selected_changed_file: Option<usize>,
    current_work: CurrentWorkState,
    refresh_status: RefreshStatus,
    refresh_error: Option<String>,
}

impl App {
    pub fn new(snapshot: ProjectSnapshot) -> Self {
        let mut app = Self {
            running: true,
            plan: PlanState::Unavailable,
            activity: ActivityState::Unavailable,
            tasks: TaskState::Unavailable,
            build_test_build: BuildTestState::Unavailable,
            build_test_test: BuildTestState::Unavailable,
            evidence_detail_kind: Some(BuildTestKind::Build),
            focused_panel: FocusedPanel::Tasks,
            selected_task: None,
            selected_changed_file: None,
            current_work: CurrentWorkState::NotSet,
            refresh_status: RefreshStatus::initial(),
            refresh_error: None,
        };
        app.apply_snapshot(snapshot);
        app
    }

    pub fn apply_snapshot(&mut self, snapshot: ProjectSnapshot) {
        let (plan, activity, tasks) = snapshot.into_parts();
        self.apply_markdown_state(plan, tasks);
        self.apply_activity_state(activity);
    }

    pub fn apply_markdown_state(&mut self, plan: PlanState, tasks: TaskState) {
        self.plan = plan;
        self.tasks = tasks;
        self.reconcile_selected_task();
    }

    pub fn apply_activity_state(&mut self, activity: ActivityState) {
        self.activity = activity;
        self.reconcile_selected_changed_file();
    }
    pub fn apply_current_work(&mut self, current_work: CurrentWorkState) {
        self.current_work = current_work;
    }

    pub fn set_refresh_error(&mut self, error: impl Into<String>) {
        self.refresh_error = Some(error.into());
    }
    pub fn clear_refresh_error(&mut self) {
        self.refresh_error = None;
    }
    pub fn refresh_error(&self) -> Option<&str> {
        self.refresh_error.as_deref()
    }

    pub fn record_refresh(&mut self, source: RefreshSource, elapsed: Duration) {
        self.refresh_status.last_source = source;
        self.refresh_status.last_update = elapsed;
    }

    pub fn set_refresh_pending(&mut self, pending: bool) -> bool {
        if self.refresh_status.retry_pending == pending {
            return false;
        }
        self.refresh_status.retry_pending = pending;
        true
    }

    fn reconcile_selected_task(&mut self) {
        self.selected_task = match &self.tasks {
            TaskState::Available(summary) if summary.remaining() > 0 => {
                Some(self.selected_task.unwrap_or(0).min(summary.remaining() - 1))
            }
            _ => None,
        };
    }

    fn reconcile_selected_changed_file(&mut self) {
        self.selected_changed_file = match &self.activity {
            ActivityState::Available(summary) if summary.changed_files() > 0 => Some(
                self.selected_changed_file
                    .unwrap_or(0)
                    .min(summary.changed_files() - 1),
            ),
            _ => None,
        };
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
    pub fn current_work(&self) -> &CurrentWorkState {
        &self.current_work
    }

    pub fn build_test_state(&self, kind: BuildTestKind) -> &BuildTestState {
        match kind {
            BuildTestKind::Build => &self.build_test_build,
            BuildTestKind::Test => &self.build_test_test,
        }
    }

    pub fn apply_build_test_state(&mut self, kind: BuildTestKind, state: BuildTestState) {
        match kind {
            BuildTestKind::Build => self.build_test_build = state,
            BuildTestKind::Test => self.build_test_test = state,
        }
    }

    pub const fn evidence_detail_kind(&self) -> Option<BuildTestKind> {
        self.evidence_detail_kind
    }

    pub fn select_evidence_detail(&mut self, kind: BuildTestKind) {
        self.evidence_detail_kind = Some(kind);
    }

    pub const fn focused_panel(&self) -> FocusedPanel {
        self.focused_panel
    }

    pub const fn selected_task(&self) -> Option<usize> {
        self.selected_task
    }

    pub const fn selected_changed_file(&self) -> Option<usize> {
        self.selected_changed_file
    }

    pub const fn refresh_status(&self) -> RefreshStatus {
        self.refresh_status
    }

    #[cfg(test)]
    pub fn handle_key(&mut self, key: KeyEvent) {
        self.handle_key_with_focusable_panels(key, &[FocusedPanel::Tasks, FocusedPanel::Evidence]);
    }

    pub fn handle_key_with_focusable_panels(
        &mut self,
        key: KeyEvent,
        focusable_panels: &[FocusedPanel],
    ) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Tab => self.focus_panel(focusable_panels, 1),
            KeyCode::BackTab => self.focus_panel(focusable_panels, -1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused_selection(focusable_panels, 1),
            KeyCode::Up | KeyCode::Char('k') => self.move_focused_selection(focusable_panels, -1),
            _ => {}
        }
    }

    pub fn reconcile_focus(&mut self, focusable_panels: &[FocusedPanel]) {
        if !focusable_panels.is_empty() && !focusable_panels.contains(&self.focused_panel) {
            self.focused_panel = focusable_panels[0];
        }
    }

    fn focus_panel(&mut self, focusable_panels: &[FocusedPanel], delta: isize) {
        let Some(current) = focusable_panels
            .iter()
            .position(|panel| *panel == self.focused_panel)
        else {
            self.reconcile_focus(focusable_panels);
            return;
        };
        let next = (current as isize + delta).rem_euclid(focusable_panels.len() as isize) as usize;
        self.focused_panel = focusable_panels[next];
    }

    fn move_focused_selection(&mut self, focusable_panels: &[FocusedPanel], delta: isize) {
        if !focusable_panels.contains(&self.focused_panel) {
            return;
        }
        match self.focused_panel {
            FocusedPanel::Tasks => self.move_task_selection(delta),
            FocusedPanel::Evidence => self.move_evidence_selection(delta),
            FocusedPanel::ChangedFiles => self.move_changed_file_selection(delta),
        }
    }

    fn move_task_selection(&mut self, delta: isize) {
        let TaskState::Available(summary) = &self.tasks else {
            return;
        };
        let Some(current) = self.selected_task else {
            return;
        };
        let last = summary.remaining().saturating_sub(1);
        self.selected_task = Some((current as isize + delta).clamp(0, last as isize) as usize);
    }

    fn move_evidence_selection(&mut self, delta: isize) {
        let current = self.evidence_detail_kind.unwrap_or(BuildTestKind::Build);
        self.evidence_detail_kind = Some(match (current, delta.is_positive()) {
            (BuildTestKind::Build, true) => BuildTestKind::Test,
            (BuildTestKind::Test, false) => BuildTestKind::Build,
            (kind, _) => kind,
        });
    }

    fn move_changed_file_selection(&mut self, delta: isize) {
        let ActivityState::Available(summary) = &self.activity else {
            return;
        };
        let Some(current) = self.selected_changed_file else {
            return;
        };
        let last = summary.changed_files().saturating_sub(1);
        self.selected_changed_file =
            Some((current as isize + delta).clamp(0, last as isize) as usize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use devscope::{
        progress::{
            ActivitySummary, BuildTestExecutionError, BuildTestFreshness, BuildTestKind,
            BuildTestOutcome, BuildTestResult, BuildTestRun, BuildTestState, GitActivity,
            GitChangedFile, GitFileStatus, PlanSummary, TaskSummary, TaskSummaryItem,
        },
        project::{ProjectSnapshot, collect_project_snapshot},
    };
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static ID: AtomicUsize = AtomicUsize::new(0);

    fn app(count: usize) -> App {
        App::new(snapshot(count))
    }

    fn snapshot(count: usize) -> ProjectSnapshot {
        let items = (0..count)
            .map(|index| TaskSummaryItem::new("a.md".into(), index, "x".into()))
            .collect();
        ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(0, count)),
            ActivityState::Unavailable,
            TaskState::Available(TaskSummary::new(count, items)),
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn move_to(app: &mut App, index: usize) {
        for _ in 0..index {
            app.handle_key(key(KeyCode::Down));
        }
    }

    fn completed_build_result() -> BuildTestResult {
        BuildTestResult::new(
            BuildTestKind::Build,
            BuildTestOutcome::Passed,
            BuildTestFreshness::Fresh,
            "cargo",
            "cargo check",
            Some(0),
            Duration::from_millis(1),
            "cargo check passed",
            None,
        )
    }

    #[test]
    fn build_test_states_start_unavailable_and_remain_independent() {
        let mut app = app(1);
        assert_eq!(
            app.build_test_state(BuildTestKind::Build),
            &BuildTestState::Unavailable
        );
        assert_eq!(
            app.build_test_state(BuildTestKind::Test),
            &BuildTestState::Unavailable
        );

        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Build,
                "cargo",
                "cargo check",
            )),
        );
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);

        assert!(matches!(
            app.build_test_state(BuildTestKind::Build),
            BuildTestState::Running(_)
        ));
        assert_eq!(
            app.build_test_state(BuildTestKind::Test),
            &BuildTestState::NotRun
        );
    }

    #[test]
    fn snapshot_and_partial_refreshes_preserve_build_test_states() {
        let mut app = app(2);
        let completed = completed_build_result();
        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Completed(completed.clone()),
        );
        app.apply_build_test_state(
            BuildTestKind::Test,
            BuildTestState::ExecutionError(BuildTestExecutionError::new(
                BuildTestKind::Test,
                "cargo",
                "cargo test",
                "spawn failed",
            )),
        );

        app.apply_snapshot(snapshot(1));
        app.apply_markdown_state(
            PlanState::Unavailable,
            TaskState::Available(TaskSummary::new(0, Vec::new())),
        );
        app.apply_activity_state(ActivityState::NotRepository);

        assert_eq!(
            app.build_test_state(BuildTestKind::Build),
            &BuildTestState::Completed(completed)
        );
        assert!(matches!(
            app.build_test_state(BuildTestKind::Test),
            BuildTestState::ExecutionError(_)
        ));
    }

    #[test]
    fn evidence_selection_starts_at_build_and_survives_state_and_snapshot_updates() {
        let mut app = app(2);
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Build));
        app.select_evidence_detail(BuildTestKind::Build);
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        app.apply_snapshot(snapshot(1));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Build));
        app.select_evidence_detail(BuildTestKind::Test);
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Test));
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
    fn applies_snapshot_and_preserves_or_clamps_selection() {
        let mut app = app(5);
        move_to(&mut app, 2);
        app.apply_snapshot(snapshot(4));
        assert_eq!(app.selected_task(), Some(2));

        move_to(&mut app, 1);
        assert_eq!(app.selected_task(), Some(3));
        app.apply_snapshot(snapshot(2));
        assert_eq!(app.selected_task(), Some(1));

        app.apply_snapshot(snapshot(0));
        assert_eq!(app.selected_task(), None);
        app.apply_snapshot(snapshot(2));
        assert_eq!(app.selected_task(), Some(0));
    }

    #[test]
    fn applies_a_recollected_snapshot_from_the_same_root() {
        let root = std::env::temp_dir().join(format!(
            "devscope-manual-reload-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let markdown = root.join("tasks.md");
        fs::write(&markdown, "- [ ] First").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));

        fs::write(&markdown, "- [x] First\n- [ ] Second").unwrap();
        app.apply_snapshot(collect_project_snapshot(&root));

        assert_eq!(app.plan(), PlanState::Available(PlanSummary::new(1, 2)));
        let TaskState::Available(tasks) = app.tasks() else {
            panic!("tasks should be available");
        };
        assert_eq!(tasks.remaining(), 1);
        assert_eq!(tasks.items()[0].text(), "Second");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn markdown_apply_preserves_activity_and_reconciles_selection() {
        let mut app = App::new(ProjectSnapshot::new(
            PlanState::Available(PlanSummary::new(0, 3)),
            ActivityState::NotRepository,
            TaskState::Available(TaskSummary::new(
                3,
                (0..3)
                    .map(|index| TaskSummaryItem::new("a.md".into(), index, "x".into()))
                    .collect(),
            )),
        ));
        move_to(&mut app, 2);

        app.apply_markdown_state(
            PlanState::Available(PlanSummary::new(1, 2)),
            TaskState::Available(TaskSummary::new(
                2,
                (0..2)
                    .map(|index| TaskSummaryItem::new("b.md".into(), index, "y".into()))
                    .collect(),
            )),
        );
        assert_eq!(app.activity(), &ActivityState::NotRepository);
        assert_eq!(app.selected_task(), Some(1));

        app.apply_markdown_state(
            PlanState::Available(PlanSummary::new(2, 2)),
            TaskState::Available(TaskSummary::new(2, Vec::new())),
        );
        assert_eq!(app.selected_task(), None);

        app.apply_markdown_state(
            PlanState::Available(PlanSummary::new(2, 3)),
            TaskState::Available(TaskSummary::new(
                3,
                vec![TaskSummaryItem::new("c.md".into(), 0, "z".into())],
            )),
        );
        assert_eq!(app.selected_task(), Some(0));
    }

    #[test]
    fn activity_apply_preserves_markdown_and_selection() {
        let mut app = app(3);
        move_to(&mut app, 1);
        let plan = app.plan();
        let tasks = app.tasks().clone();

        app.apply_activity_state(ActivityState::NotRepository);

        assert_eq!(app.plan(), plan);
        assert_eq!(app.tasks(), &tasks);
        assert_eq!(app.selected_task(), Some(1));
        assert_eq!(app.activity(), &ActivityState::NotRepository);
    }
    #[test]
    fn empty_and_unavailable_are_unselected() {
        assert_eq!(app(0).selected_task(), None);
        let app = App::new(ProjectSnapshot::unavailable());
        assert_eq!(app.selected_task(), None);
    }

    #[test]
    fn refresh_status_starts_at_initial_snapshot() {
        let status = app(1).refresh_status();
        assert_eq!(status.last_source(), RefreshSource::Initial);
        assert_eq!(status.last_update(), Duration::ZERO);
        assert!(!status.retry_pending());
    }

    #[test]
    fn records_successful_refreshes() {
        let mut app = app(1);
        app.record_refresh(RefreshSource::Markdown, Duration::from_secs(12));
        let status = app.refresh_status();
        assert_eq!(status.last_source(), RefreshSource::Markdown);
        assert_eq!(status.last_update(), Duration::from_secs(12));

        app.record_refresh(RefreshSource::Manual, Duration::from_secs(25));
        assert_eq!(app.refresh_status().last_source(), RefreshSource::Manual);
        assert_eq!(app.refresh_status().last_update(), Duration::from_secs(25));
    }

    #[test]
    fn refresh_pending_reports_only_state_changes() {
        let mut app = app(1);
        assert!(app.set_refresh_pending(true));
        assert!(!app.set_refresh_pending(true));
        assert!(app.refresh_status().retry_pending());
        assert!(app.set_refresh_pending(false));
        assert!(!app.refresh_status().retry_pending());
    }
    fn activity_with_files(count: usize) -> ActivityState {
        ActivityState::Available(ActivitySummary::from(&GitActivity {
            changed_files: (0..count)
                .map(|index| GitChangedFile {
                    path: format!("file-{index}.rs").into(),
                    status: GitFileStatus::Modified,
                })
                .collect(),
            recent_commits: Vec::new(),
        }))
    }

    const ALL_PANELS: &[FocusedPanel] = &[
        FocusedPanel::Tasks,
        FocusedPanel::Evidence,
        FocusedPanel::ChangedFiles,
    ];
    const TASKS_AND_EVIDENCE: &[FocusedPanel] = &[FocusedPanel::Tasks, FocusedPanel::Evidence];

    #[test]
    fn changed_file_selection_starts_clamps_and_reconciles_after_activity_refresh() {
        let mut app = app(2);
        app.apply_activity_state(activity_with_files(3));
        assert_eq!(app.selected_changed_file(), Some(0));

        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);
        app.handle_key_with_focusable_panels(key(KeyCode::Up), ALL_PANELS);
        assert_eq!(app.selected_changed_file(), Some(0));
        app.handle_key_with_focusable_panels(key(KeyCode::Down), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('j')), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Down), ALL_PANELS);
        assert_eq!(app.selected_changed_file(), Some(2));

        app.apply_activity_state(activity_with_files(1));
        assert_eq!(app.selected_changed_file(), Some(0));
        app.apply_activity_state(ActivityState::NotRepository);
        assert_eq!(app.selected_changed_file(), None);
    }

    #[test]
    fn three_panel_focus_wraps_and_changed_file_navigation_is_local() {
        let mut app = app(3);
        app.apply_activity_state(activity_with_files(3));
        app.handle_key_with_focusable_panels(key(KeyCode::Down), ALL_PANELS);
        let selected_task = app.selected_task();
        let selected_evidence = app.evidence_detail_kind();

        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('j')), ALL_PANELS);
        assert_eq!(app.selected_changed_file(), Some(1));
        assert_eq!(app.selected_task(), selected_task);
        assert_eq!(app.evidence_detail_kind(), selected_evidence);

        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        assert_eq!(app.focused_panel(), FocusedPanel::Tasks);
        app.handle_key_with_focusable_panels(key(KeyCode::BackTab), ALL_PANELS);
        assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);
    }

    #[test]
    fn focus_reconciles_when_changed_files_becomes_hidden() {
        let mut app = app(1);
        app.apply_activity_state(activity_with_files(1));
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);

        app.reconcile_focus(TASKS_AND_EVIDENCE);
        assert_eq!(app.focused_panel(), FocusedPanel::Tasks);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), TASKS_AND_EVIDENCE);
        assert_eq!(app.focused_panel(), FocusedPanel::Evidence);
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
    #[test]
    fn panel_focus_wraps_and_routes_selection_locally() {
        let mut app = app(3);
        assert_eq!(app.focused_panel(), FocusedPanel::Tasks);
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Build));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.selected_task(), Some(1));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Build));

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_panel(), FocusedPanel::Evidence);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Test));
        assert_eq!(app.selected_task(), Some(1));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Test));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Build));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Build));

        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.focused_panel(), FocusedPanel::Tasks);
        assert_eq!(app.selected_task(), Some(1));
        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.focused_panel(), FocusedPanel::Evidence);
    }

    #[test]
    fn snapshot_preserves_focus_and_evidence_selection() {
        let mut app = app(3);
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('j')));
        app.apply_snapshot(snapshot(1));
        assert_eq!(app.focused_panel(), FocusedPanel::Evidence);
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Test));
    }
}
