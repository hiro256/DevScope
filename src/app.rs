use std::{path::PathBuf, time::Duration};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use devscope::{
    current_work::CurrentWork,
    progress::{
        ArtifactObservation, BuildTestKind, BuildTestState, GitChangeCounts, GitFileInspection,
        GitFileStatus,
    },
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
pub enum EvidenceSelection {
    Build,
    Test,
    Artifact,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusedPanel {
    Tasks,
    Evidence,
    ChangedFiles,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DetailTarget {
    ChangedFile {
        path: PathBuf,
        status: GitFileStatus,
        changes: GitChangeCounts,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppView {
    Overview,
    FileBrowser,
}

pub struct App {
    view: AppView,
    pub file_browser: crate::file_browser::FileBrowserState,
    running: bool,
    plan: PlanState,
    activity: ActivityState,
    tasks: TaskState,
    build_test_build: BuildTestState,
    build_test_test: BuildTestState,
    evidence_selection: EvidenceSelection,
    artifact: Option<ArtifactObservation>,
    focused_panel: FocusedPanel,
    selected_task: Option<usize>,
    selected_changed_file: Option<usize>,
    detail_target: Option<DetailTarget>,
    detail_inspection: Option<GitFileInspection>,
    detail_scroll: usize,
    preview_inspection: Option<GitFileInspection>,
    preview_visible: bool,
    preview_scroll: usize,
    current_work: CurrentWorkState,
    refresh_status: RefreshStatus,
    refresh_error: Option<String>,
}

impl App {
    pub fn new(snapshot: ProjectSnapshot) -> Self {
        let mut app = Self {
            view: AppView::Overview,
            file_browser: crate::file_browser::FileBrowserState::default(),
            running: true,
            plan: PlanState::Unavailable,
            activity: ActivityState::Unavailable,
            tasks: TaskState::Unavailable,
            build_test_build: BuildTestState::Unavailable,
            build_test_test: BuildTestState::Unavailable,
            evidence_selection: EvidenceSelection::Build,
            artifact: None,
            focused_panel: FocusedPanel::Tasks,
            selected_task: None,
            selected_changed_file: None,
            detail_target: None,
            detail_inspection: None,
            detail_scroll: 0,
            preview_inspection: None,
            preview_visible: true,
            preview_scroll: 0,
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
        let previous = self.selected_task_identity();
        self.plan = plan;
        self.tasks = tasks;
        self.reconcile_selected_task();
        if previous != self.selected_task_identity() && self.focused_panel == FocusedPanel::Tasks {
            self.preview_scroll = 0;
        }
    }

    fn selected_task_identity(&self) -> Option<(PathBuf, usize, String)> {
        let TaskState::Available(summary) = &self.tasks else {
            return None;
        };
        let task = summary.items().get(self.selected_task?)?;
        Some((
            task.path().to_path_buf(),
            task.line(),
            task.text().to_owned(),
        ))
    }

    pub fn apply_activity_state(&mut self, activity: ActivityState) {
        let previous = self.selected_changed_file_request();
        self.activity = activity;
        self.reconcile_selected_changed_file();
        if previous != self.selected_changed_file_request()
            && self.focused_panel == FocusedPanel::ChangedFiles
        {
            self.preview_scroll = 0;
        }
        self.preview_inspection = None;
        self.reconcile_detail_target();
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

    fn reconcile_detail_target(&mut self) {
        let Some(DetailTarget::ChangedFile { path, .. }) = &self.detail_target else {
            return;
        };
        let path = path.clone();
        self.detail_target = match &self.activity {
            ActivityState::Available(summary) => summary
                .changed_file_items()
                .iter()
                .find(|file| file.path == path)
                .map(|file| DetailTarget::ChangedFile {
                    path: file.path.clone(),
                    status: file.status.clone(),
                    changes: file.changes,
                }),
            ActivityState::NotRepository | ActivityState::Unavailable => None,
        };
        self.detail_inspection = None;
        self.detail_scroll = 0;
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
        match self.evidence_selection {
            EvidenceSelection::Build => Some(BuildTestKind::Build),
            EvidenceSelection::Test => Some(BuildTestKind::Test),
            EvidenceSelection::Artifact => None,
        }
    }
    pub const fn evidence_selection(&self) -> EvidenceSelection {
        self.evidence_selection
    }
    pub fn select_evidence_detail(&mut self, kind: BuildTestKind) {
        let previous = self.evidence_selection;
        self.evidence_selection = match kind {
            BuildTestKind::Build => EvidenceSelection::Build,
            BuildTestKind::Test => EvidenceSelection::Test,
        };
        if previous != self.evidence_selection && self.focused_panel == FocusedPanel::Evidence {
            self.preview_scroll = 0;
        }
    }
    pub fn artifact(&self) -> Option<&ArtifactObservation> {
        self.artifact.as_ref()
    }
    pub fn apply_artifact(&mut self, artifact: Option<ArtifactObservation>) {
        self.artifact = artifact;
        if self.artifact.is_none() && self.evidence_selection == EvidenceSelection::Artifact {
            self.evidence_selection = EvidenceSelection::Build;
            if self.focused_panel == FocusedPanel::Evidence {
                self.preview_scroll = 0;
            }
        }
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

    pub fn detail_inspection(&self) -> Option<&GitFileInspection> {
        self.detail_inspection.as_ref()
    }

    pub fn preview_inspection(&self) -> Option<&GitFileInspection> {
        self.preview_inspection.as_ref()
    }

    pub const fn preview_visible(&self) -> bool {
        self.preview_visible
    }

    pub fn toggle_preview(&mut self) {
        self.preview_visible = !self.preview_visible;
    }

    pub const fn preview_scroll(&self) -> usize {
        self.preview_scroll
    }

    pub fn scroll_preview(&mut self, delta: isize, max_scroll: usize) {
        self.preview_scroll = self
            .preview_scroll
            .saturating_add_signed(delta)
            .min(max_scroll);
    }

    pub fn apply_preview_inspection(&mut self, inspection: GitFileInspection) {
        if self.selected_changed_file_request().is_some() {
            self.preview_inspection = Some(inspection);
        }
    }

    pub const fn detail_scroll(&self) -> usize {
        self.detail_scroll
    }

    pub fn apply_detail_inspection(&mut self, inspection: GitFileInspection) {
        if self.detail_target.is_some() {
            self.detail_inspection = Some(inspection);
            self.detail_scroll = 0;
        }
    }

    pub fn scroll_detail(&mut self, delta: isize, max_scroll: usize) {
        self.detail_scroll =
            (self.detail_scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
    }

    pub fn detail_request(&self) -> Option<(PathBuf, GitFileStatus)> {
        match self.detail_target.as_ref()? {
            DetailTarget::ChangedFile { path, status, .. } => Some((path.clone(), status.clone())),
        }
    }

    pub fn selected_changed_file_request(&self) -> Option<(PathBuf, GitFileStatus)> {
        let (Some(selected), ActivityState::Available(summary)) =
            (self.selected_changed_file, &self.activity)
        else {
            return None;
        };
        let file = summary.changed_file_items().get(selected)?;
        Some((file.path.clone(), file.status.clone()))
    }

    pub fn detail_target(&self) -> Option<&DetailTarget> {
        self.detail_target.as_ref()
    }

    pub fn has_detail_view(&self) -> bool {
        self.detail_target.is_some()
    }

    pub const fn is_file_browser_open(&self) -> bool {
        matches!(self.view, AppView::FileBrowser)
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
        if self.is_file_browser_open() {
            if key.modifiers == KeyModifiers::NONE {
                match key.code {
                    KeyCode::Esc => {
                        self.view = AppView::Overview;
                        self.reconcile_focus(focusable_panels);
                    }
                    KeyCode::Char('q') => self.running = false,
                    _ => {}
                }
            }
            return;
        }
        if self.detail_target.is_some() {
            if key.modifiers != KeyModifiers::NONE {
                return;
            }
            match key.code {
                KeyCode::Char('q') => self.running = false,
                KeyCode::Enter | KeyCode::Esc => {
                    self.detail_target = None;
                    self.detail_inspection = None;
                    self.detail_scroll = 0;
                }
                _ => {}
            }
            return;
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('f') {
            self.view = AppView::FileBrowser;
            return;
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Enter {
            self.open_changed_file_detail();
            return;
        }
        if key.modifiers == KeyModifiers::SHIFT
            && matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
        {
            self.focus_panel(focusable_panels, -1);
            return;
        }
        if key.modifiers != KeyModifiers::NONE {
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Enter | KeyCode::Char('p') => self.toggle_preview(),
            KeyCode::Tab | KeyCode::Right => self.focus_panel(focusable_panels, 1),
            KeyCode::BackTab | KeyCode::Left => self.focus_panel(focusable_panels, -1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused_selection(focusable_panels, 1),
            KeyCode::Up | KeyCode::Char('k') => self.move_focused_selection(focusable_panels, -1),
            _ => {}
        }
    }

    pub fn reconcile_focus(&mut self, focusable_panels: &[FocusedPanel]) {
        if !focusable_panels.is_empty() && !focusable_panels.contains(&self.focused_panel) {
            self.focused_panel = focusable_panels[0];
            self.preview_scroll = 0;
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
        if self.focused_panel != focusable_panels[next] {
            self.focused_panel = focusable_panels[next];
            self.preview_scroll = 0;
        }
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

    fn open_changed_file_detail(&mut self) {
        if self.focused_panel != FocusedPanel::ChangedFiles {
            return;
        }
        let (Some(selected), ActivityState::Available(summary)) =
            (self.selected_changed_file, &self.activity)
        else {
            return;
        };
        let Some(file) = summary.changed_file_items().get(selected) else {
            return;
        };
        self.detail_target = Some(DetailTarget::ChangedFile {
            path: file.path.clone(),
            status: file.status.clone(),
            changes: file.changes,
        });
        self.detail_inspection = None;
        self.detail_scroll = 0;
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
        if self.selected_task != Some(current) {
            self.preview_scroll = 0;
        }
    }

    fn move_evidence_selection(&mut self, delta: isize) {
        let previous = self.evidence_selection;
        self.evidence_selection = match (
            self.evidence_selection,
            delta.is_positive(),
            self.artifact.is_some(),
        ) {
            (EvidenceSelection::Build, true, _) => EvidenceSelection::Test,
            (EvidenceSelection::Test, true, true) => EvidenceSelection::Artifact,
            (EvidenceSelection::Artifact, true, _) => EvidenceSelection::Artifact,
            (EvidenceSelection::Test, false, _) => EvidenceSelection::Build,
            (EvidenceSelection::Artifact, false, _) => EvidenceSelection::Test,
            (selection, _, _) => selection,
        };
        if previous != self.evidence_selection {
            self.preview_scroll = 0;
        }
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
        if self.selected_changed_file != Some(current) {
            self.preview_scroll = 0;
        }
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
            GitChangedFile, GitFileInspectionUnavailable, GitFileStatus, PlanSummary, TaskSummary,
            TaskSummaryItem,
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

    #[test]
    fn browser_keys_preserve_overview_and_never_open_from_detail() {
        let mut app = app(3);
        app.apply_activity_state(activity_with_files(3));
        app.focused_panel = FocusedPanel::ChangedFiles;
        app.selected_task = Some(1);
        app.selected_changed_file = Some(1);
        app.evidence_selection = EvidenceSelection::Test;
        app.preview_visible = false;
        app.preview_scroll = 7;
        let open = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('f')), ALL_PANELS);
        assert!(!app.is_file_browser_open());
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            app.handle_key_with_focusable_panels(KeyEvent { kind, ..open }, ALL_PANELS);
            assert!(!app.is_file_browser_open());
        }
        app.handle_key_with_focusable_panels(open, ALL_PANELS);
        assert!(app.is_file_browser_open());
        for key in [
            open,
            key(KeyCode::Char('b')),
            key(KeyCode::Char('t')),
            key(KeyCode::Char('p')),
            key(KeyCode::Tab),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
        ] {
            app.handle_key_with_focusable_panels(key, ALL_PANELS);
            assert!(app.is_file_browser_open());
            assert!(!app.has_detail_view());
        }
        app.file_browser.scroll(2, 10);
        assert_eq!(app.preview_scroll(), 7);
        app.handle_key_with_focusable_panels(key(KeyCode::Esc), ALL_PANELS);
        assert!(!app.is_file_browser_open());
        assert!(app.is_running());
        assert_eq!(app.focused_panel, FocusedPanel::ChangedFiles);
        assert_eq!(app.selected_task, Some(1));
        assert_eq!(app.selected_changed_file, Some(1));
        assert_eq!(app.evidence_selection, EvidenceSelection::Test);
        assert!(!app.preview_visible);
        assert_eq!(app.preview_scroll, 7);
        app.open_changed_file_detail();
        app.handle_key_with_focusable_panels(open, ALL_PANELS);
        assert!(!app.is_file_browser_open());
        assert!(app.has_detail_view());
        app.handle_key_with_focusable_panels(key(KeyCode::Esc), ALL_PANELS);
        app.handle_key_with_focusable_panels(open, ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('q')), ALL_PANELS);
        assert!(!app.is_running());
    }

    #[test]
    fn arrows_cycle_visible_panels_and_compatibility_keys_match() {
        let mut app = app(3);
        for panels in [ALL_PANELS, &ALL_PANELS[..2]] {
            app.reconcile_focus(&[FocusedPanel::Tasks]);
            for expected in panels.iter().cycle().skip(1).take(panels.len()) {
                app.handle_key_with_focusable_panels(key(KeyCode::Right), panels);
                assert_eq!(app.focused_panel(), *expected);
            }
            app.handle_key_with_focusable_panels(key(KeyCode::Left), panels);
            assert_eq!(app.focused_panel(), *panels.last().unwrap());
            app.handle_key_with_focusable_panels(key(KeyCode::Tab), panels);
            assert_eq!(app.focused_panel(), FocusedPanel::Tasks);
            app.handle_key_with_focusable_panels(
                KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
                panels,
            );
            assert_eq!(app.focused_panel(), *panels.last().unwrap());
        }
    }

    #[test]
    fn preview_keys_are_modifier_aware_and_enter_never_opens_detail() {
        let mut app = app(3);
        app.apply_activity_state(activity_with_files(3));
        for panel in ALL_PANELS {
            app.reconcile_focus(&[*panel]);
            app.scroll_preview(3, 10);
            let selection = (
                app.selected_task(),
                app.selected_changed_file(),
                app.evidence_selection(),
            );
            for code in [
                KeyCode::Up,
                KeyCode::Down,
                KeyCode::Left,
                KeyCode::Right,
                KeyCode::Char('j'),
                KeyCode::Char('k'),
                KeyCode::Char('q'),
                KeyCode::Char('p'),
            ] {
                app.handle_key_with_focusable_panels(
                    KeyEvent::new(code, KeyModifiers::CONTROL),
                    ALL_PANELS,
                );
            }
            assert_eq!(app.focused_panel(), *panel);
            assert_eq!(
                selection,
                (
                    app.selected_task(),
                    app.selected_changed_file(),
                    app.evidence_selection()
                )
            );
            assert!(app.is_running());
            assert!(app.preview_visible());
            app.handle_key_with_focusable_panels(key(KeyCode::Enter), ALL_PANELS);
            assert!(!app.preview_visible());
            assert!(!app.has_detail_view());
            app.handle_key_with_focusable_panels(key(KeyCode::Char('p')), ALL_PANELS);
            assert!(app.preview_visible());
            assert_eq!(app.preview_scroll(), 3);
            for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
                let mut event = key(KeyCode::Enter);
                event.kind = kind;
                app.handle_key_with_focusable_panels(event, ALL_PANELS);
                assert!(app.preview_visible());
            }
            app.handle_key_with_focusable_panels(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
                ALL_PANELS,
            );
            assert_eq!(app.has_detail_view(), *panel == FocusedPanel::ChangedFiles);
        }
    }

    #[test]
    fn preview_scroll_clamps_and_resets_on_target_changes_only() {
        let mut app = app(3);
        app.scroll_preview(-1, 5);
        assert_eq!(app.preview_scroll(), 0);
        app.scroll_preview(isize::MAX, 5);
        assert_eq!(app.preview_scroll(), 5);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.preview_scroll(), 5);
        app.apply_snapshot(snapshot(3));
        assert_eq!(app.preview_scroll(), 5);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.preview_scroll(), 0);
        app.scroll_preview(4, 5);
        app.apply_markdown_state(
            PlanState::Unavailable,
            TaskState::Available(TaskSummary::new(
                1,
                vec![TaskSummaryItem::new("other.md".into(), 1, "changed".into())],
            )),
        );
        assert_eq!(app.preview_scroll(), 0);
        app.scroll_preview(4, 5);
        app.handle_key_with_focusable_panels(key(KeyCode::Right), ALL_PANELS);
        assert_eq!(app.preview_scroll(), 0);
        app.scroll_preview(4, 5);
        app.select_evidence_detail(BuildTestKind::Test);
        assert_eq!(app.preview_scroll(), 0);
        app.scroll_preview(4, 5);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.preview_scroll(), 0);
        app.apply_activity_state(activity_with_files(3));
        app.handle_key_with_focusable_panels(key(KeyCode::Right), ALL_PANELS);
        app.scroll_preview(4, 5);
        app.handle_key_with_focusable_panels(key(KeyCode::Down), ALL_PANELS);
        assert_eq!(app.preview_scroll(), 0);
        app.scroll_preview(4, 5);
        app.apply_activity_state(activity_with_files(1));
        assert_eq!(app.preview_scroll(), 0);
        app.scroll_preview(4, 5);
        app.reconcile_focus(&ALL_PANELS[..2]);
        assert_eq!(app.preview_scroll(), 0);
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
        app.apply_artifact(Some(ArtifactObservation::observation_error(
            PathBuf::from("output.log"),
            "unavailable",
        )));
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Down), ALL_PANELS);
        assert_eq!(app.evidence_selection(), EvidenceSelection::Artifact);
        assert_eq!(app.evidence_detail_kind(), None);
        app.apply_artifact(None);
        assert_eq!(app.evidence_selection(), EvidenceSelection::Build);
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
                    changes: Default::default(),
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
    fn changed_file_detail_opens_closes_and_preserves_selection_and_focus() {
        let mut app = app(1);
        app.apply_activity_state(activity_with_files(2));
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Down), ALL_PANELS);
        assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);
        assert_eq!(app.selected_changed_file(), Some(1));

        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        assert_eq!(
            app.detail_target(),
            Some(&DetailTarget::ChangedFile {
                path: "file-1.rs".into(),
                status: GitFileStatus::Modified,
                changes: Default::default(),
            })
        );
        app.handle_key_with_focusable_panels(key(KeyCode::Char('j')), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('b')), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('t')), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('r')), ALL_PANELS);
        assert_eq!(app.selected_changed_file(), Some(1));

        app.handle_key_with_focusable_panels(key(KeyCode::Esc), ALL_PANELS);
        assert!(!app.has_detail_view());
        assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);
        assert_eq!(app.selected_changed_file(), Some(1));
    }

    #[test]
    fn changed_file_detail_requires_a_selected_changed_file_and_ignores_other_panels() {
        let mut no_selection = app(1);
        no_selection.focused_panel = FocusedPanel::ChangedFiles;
        no_selection.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        assert!(!no_selection.has_detail_view());

        let mut tasks = app(1);
        tasks.apply_activity_state(activity_with_files(1));
        tasks.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        assert!(!tasks.has_detail_view());
        tasks.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        tasks.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        assert!(!tasks.has_detail_view());
    }

    #[test]
    fn preview_toggle_does_not_change_changed_file_detail_behavior() {
        let mut app = app(1);
        app.apply_activity_state(activity_with_files(1));
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('p')), ALL_PANELS);
        assert!(!app.preview_visible());

        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        assert!(app.has_detail_view());
    }

    #[test]
    fn detail_target_refreshes_change_counts_from_activity() {
        let mut app = app(1);
        let activity = |additions, deletions| {
            ActivityState::Available(ActivitySummary::from(&GitActivity {
                changed_files: vec![GitChangedFile {
                    path: "file-0.rs".into(),
                    status: GitFileStatus::Modified,
                    changes: GitChangeCounts {
                        additions: Some(additions),
                        deletions: Some(deletions),
                    },
                }],
                recent_commits: Vec::new(),
            }))
        };
        app.apply_activity_state(activity(12, 4));
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        app.apply_activity_state(activity(20, 6));
        assert_eq!(
            app.detail_target(),
            Some(&DetailTarget::ChangedFile {
                path: "file-0.rs".into(),
                status: GitFileStatus::Modified,
                changes: GitChangeCounts {
                    additions: Some(20),
                    deletions: Some(6),
                },
            })
        );
    }
    #[test]
    fn detail_quits_with_q_normal_escape_quits_and_refresh_disappearance_closes() {
        let mut detail = app(1);
        detail.apply_activity_state(activity_with_files(1));
        detail.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        detail.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        detail.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        assert!(detail.has_detail_view());
        detail.apply_activity_state(activity_with_files(0));
        assert!(!detail.has_detail_view());

        detail.apply_activity_state(activity_with_files(1));
        detail.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        detail.handle_key_with_focusable_panels(key(KeyCode::Char('q')), ALL_PANELS);
        assert!(!detail.is_running());

        let mut normal = app(1);
        normal.handle_key(key(KeyCode::Esc));
        assert!(!normal.is_running());
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
    #[test]
    fn detail_scroll_is_bounded_and_resets_when_the_detail_closes() {
        let mut app = app(1);
        app.apply_activity_state(activity_with_files(1));
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), ALL_PANELS);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            ALL_PANELS,
        );
        app.apply_detail_inspection(GitFileInspection::Unavailable(
            GitFileInspectionUnavailable::Missing,
        ));
        app.scroll_detail(20, 3);
        assert_eq!(app.detail_scroll(), 3);
        app.scroll_detail(-20, 3);
        assert_eq!(app.detail_scroll(), 0);
        app.handle_key_with_focusable_panels(key(KeyCode::Esc), ALL_PANELS);
        assert!(!app.has_detail_view());
        assert_eq!(app.detail_scroll(), 0);
    }

    #[test]
    fn detail_enter_and_escape_share_cleanup_and_preserve_overview_state() {
        for close in [KeyCode::Enter, KeyCode::Esc] {
            for visible in [false, true] {
                let mut app = app(1);
                app.apply_activity_state(activity_with_files(2));
                app.handle_key_with_focusable_panels(key(KeyCode::Left), ALL_PANELS);
                app.handle_key_with_focusable_panels(key(KeyCode::Down), ALL_PANELS);
                if !visible {
                    app.toggle_preview();
                }
                app.scroll_preview(2, 5);
                app.handle_key_with_focusable_panels(
                    KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
                    ALL_PANELS,
                );
                app.apply_detail_inspection(GitFileInspection::Unavailable(
                    GitFileInspectionUnavailable::Missing,
                ));
                app.scroll_detail(3, 5);
                let target = app.detail_target().cloned();
                let diff = app.detail_inspection().cloned();
                for modifier in [
                    KeyModifiers::CONTROL,
                    KeyModifiers::SHIFT,
                    KeyModifiers::ALT,
                ] {
                    app.handle_key_with_focusable_panels(
                        KeyEvent::new(KeyCode::Enter, modifier),
                        ALL_PANELS,
                    );
                    assert_eq!(app.detail_target(), target.as_ref());
                    assert_eq!(app.detail_inspection(), diff.as_ref());
                    assert_eq!(app.detail_scroll(), 3);
                }
                app.handle_key_with_focusable_panels(key(close), ALL_PANELS);
                assert_eq!(app.detail_target(), None);
                assert_eq!(app.detail_inspection(), None);
                assert_eq!(app.detail_scroll(), 0);
                assert!(app.is_running());
                assert_eq!(app.focused_panel(), FocusedPanel::ChangedFiles);
                assert_eq!(app.selected_changed_file(), Some(1));
                assert_eq!(app.preview_visible(), visible);
                assert_eq!(app.preview_scroll(), 2);
            }
        }
    }
}
