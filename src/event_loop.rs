mod verification;
use verification::{
    BuildTestRuntime, check_build_test_freshness, observe_active_build_test_inputs,
    poll_build_test_execution, start_manual_build_test,
};

use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::terminal::AppTerminal;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use devscope::{
    change::{
        ConfigChange, ConfigChangeDetector, CurrentWorkChange, CurrentWorkChangeDetector,
        GitMetadataChange, GitMetadataChangeDetector, GitWorktreeChange, GitWorktreeChangeDetector,
        MarkdownChange, MarkdownChangeDetector, WorktreeScanDiagnostics,
        diagnose_worktree_scan_with_exclusions,
    },
    config::{ConfigError, ProjectConfig, load_project_config},
    current_work::load_current_work,
    progress::{
        ArtifactObservation, BuildTestKind, GitFileInspection, GitFileInspectionUnavailable,
        collect_git_file_inspection, observe_artifact,
    },
    project::{collect_activity_state, collect_markdown_state, try_collect_project_snapshot},
};

use crate::{
    app::{ActivityState, App, CurrentWorkState, FocusedPanel, RefreshSource},
    ui,
};

const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(250);
const PROJECT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SLOW_WORKTREE_SCAN: Duration = Duration::from_millis(200);

struct PollScheduler {
    interval: Duration,
    next_tick: Instant,
}

impl PollScheduler {
    fn new(start: Instant, interval: Duration) -> Self {
        Self {
            interval,
            next_tick: next_deadline(start, interval),
        }
    }

    fn is_due(&mut self, now: Instant) -> bool {
        if now < self.next_tick {
            return false;
        }
        self.next_tick = next_deadline(now, self.interval);
        true
    }
}

#[derive(Default)]
struct RefreshRequest {
    markdown: bool,
    git: bool,
}

impl RefreshRequest {
    fn clear(&mut self) {
        self.markdown = false;
        self.git = false;
    }
}

fn next_deadline(now: Instant, interval: Duration) -> Instant {
    now.checked_add(interval).unwrap_or(now)
}

fn is_slow_worktree_scan(duration: Duration) -> bool {
    duration >= SLOW_WORKTREE_SCAN
}

enum GitWorktreeWorkerCommand {
    Scan { generation: u64 },
    Sync,
    Shutdown,
}

struct WorktreeScanResult {
    generation: u64,
    change: Option<GitWorktreeChange>,
    duration: Duration,
    diagnostics: Option<WorktreeScanDiagnostics>,
    diagnostic_duration: Option<Duration>,
}

struct GitWorktreeWorker {
    commands: Sender<GitWorktreeWorkerCommand>,
    results: Receiver<WorktreeScanResult>,
    join: Option<JoinHandle<()>>,
    scan_in_flight: bool,
    generation: u64,
    last_duration: Option<Duration>,
    last_diagnostics: Option<WorktreeScanDiagnostics>,
    last_diagnostic_duration: Option<Duration>,
}

impl GitWorktreeWorker {
    #[cfg(test)]
    fn new(root: PathBuf) -> Self {
        Self::new_with_exclusions(root, Vec::new())
    }

    fn new_with_exclusions(root: PathBuf, excludes: Vec<PathBuf>) -> Self {
        let (command_sender, command_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        let join = thread::spawn(move || {
            let mut detector =
                GitWorktreeChangeDetector::new_with_exclusions(&root, excludes.clone());
            let mut diagnose_next = false;
            while let Ok(command) = command_receiver.recv() {
                match command {
                    GitWorktreeWorkerCommand::Scan { generation } => {
                        let scan_started = Instant::now();
                        let change = detector.check(&root).ok();
                        let scan_duration = scan_started.elapsed();
                        let (diagnostics, diagnostic_duration) =
                            if diagnose_next && change.is_some() {
                                let diagnostic_started = Instant::now();
                                (
                                    diagnose_worktree_scan_with_exclusions(&root, &excludes).ok(),
                                    Some(diagnostic_started.elapsed()),
                                )
                            } else {
                                (None, None)
                            };
                        diagnose_next = is_slow_worktree_scan(scan_duration);
                        if result_sender
                            .send(WorktreeScanResult {
                                generation,
                                change,
                                duration: scan_duration,
                                diagnostics,
                                diagnostic_duration,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    GitWorktreeWorkerCommand::Sync => detector.sync(&root),
                    GitWorktreeWorkerCommand::Shutdown => break,
                }
            }
        });
        Self {
            commands: command_sender,
            results: result_receiver,
            join: Some(join),
            scan_in_flight: false,
            generation: 0,
            last_duration: None,
            last_diagnostics: None,
            last_diagnostic_duration: None,
        }
    }

    fn request_scan(&mut self) -> bool {
        if self.scan_in_flight {
            return true;
        }
        if self
            .commands
            .send(GitWorktreeWorkerCommand::Scan {
                generation: self.generation,
            })
            .is_ok()
        {
            self.scan_in_flight = true;
            true
        } else {
            false
        }
    }

    fn sync(&mut self) -> bool {
        let generation = self.generation.wrapping_add(1);
        if self.commands.send(GitWorktreeWorkerCommand::Sync).is_ok() {
            self.generation = generation;
            true
        } else {
            false
        }
    }

    fn try_recv_changed(&mut self) -> Result<bool, ()> {
        let mut changed = false;
        loop {
            match self.results.try_recv() {
                Ok(result) => {
                    self.scan_in_flight = false;
                    self.last_duration = Some(result.duration);
                    if let Some(diagnostics) = result.diagnostics {
                        self.last_diagnostics = Some(diagnostics);
                        self.last_diagnostic_duration = result.diagnostic_duration;
                    }
                    changed |= result.generation == self.generation
                        && matches!(result.change, Some(GitWorktreeChange::Changed));
                }
                Err(TryRecvError::Empty) => return Ok(changed),
                Err(TryRecvError::Disconnected) => {
                    self.scan_in_flight = false;
                    return Err(());
                }
            }
        }
    }

    #[cfg(test)]
    fn last_duration(&self) -> Option<Duration> {
        self.last_duration
    }

    fn shutdown(&mut self) {
        let _ = self.commands.send(GitWorktreeWorkerCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for GitWorktreeWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn new_git_worktree_worker(
    project_root: Option<&Path>,
    app: &App,
    activity_excludes: &[PathBuf],
) -> Option<GitWorktreeWorker> {
    match (project_root, app.activity()) {
        (Some(root), ActivityState::Available(_)) => Some(GitWorktreeWorker::new_with_exclusions(
            root.to_path_buf(),
            activity_excludes.to_vec(),
        )),
        _ => None,
    }
}

fn reconcile_worktree_worker(
    project_root: Option<&Path>,
    app: &App,
    activity_excludes: &[PathBuf],
    worker: &mut Option<GitWorktreeWorker>,
) {
    if matches!(app.activity(), ActivityState::Available(_)) {
        if worker.is_none() {
            *worker = project_root.map(|root| {
                GitWorktreeWorker::new_with_exclusions(
                    root.to_path_buf(),
                    activity_excludes.to_vec(),
                )
            });
        }
    } else {
        *worker = None;
    }
}

fn sync_git_worktree_worker(worker: &mut Option<GitWorktreeWorker>) {
    if worker.as_mut().is_some_and(|worker| !worker.sync()) {
        *worker = None;
    }
}

fn check_markdown_changes(
    project_root: Option<&Path>,
    markdown_changes: &mut Option<MarkdownChangeDetector>,
) -> bool {
    let (Some(root), Some(detector)) = (project_root, markdown_changes) else {
        return false;
    };
    matches!(detector.check(root), Ok(MarkdownChange::Changed))
}

fn check_config_changes(
    project_root: Option<&Path>,
    config_changes: &mut Option<ConfigChangeDetector>,
) -> bool {
    let (Some(root), Some(detector)) = (project_root, config_changes) else {
        return false;
    };
    matches!(detector.check(root), Ok(ConfigChange::Changed))
}
fn check_current_work_changes(
    project_root: Option<&Path>,
    current_work_changes: &mut Option<CurrentWorkChangeDetector>,
) -> bool {
    let (Some(root), Some(detector)) = (project_root, current_work_changes) else {
        return false;
    };
    !matches!(detector.check(root), Ok(CurrentWorkChange::Unchanged))
}

fn load_current_work_state(root: &Path) -> CurrentWorkState {
    match load_current_work(root) {
        Ok(Some(work)) => CurrentWorkState::Available(work),
        Ok(None) => CurrentWorkState::NotSet,
        Err(_) => CurrentWorkState::Unavailable,
    }
}

fn refresh_current_work(root: Option<&Path>, app: &mut App) -> bool {
    let Some(root) = root else {
        return false;
    };
    let current_work = load_current_work_state(root);
    if app.current_work() == &current_work {
        return false;
    }
    app.apply_current_work(current_work);
    true
}
fn load_artifact_observation(root: &Path) -> Result<Option<ArtifactObservation>, ConfigError> {
    let config = load_project_config(root)?;
    let Some(path) = config.artifact().path() else {
        return Ok(None);
    };
    Ok(Some(observe_artifact(root, path).unwrap_or_else(|error| {
        ArtifactObservation::observation_error(path.to_path_buf(), error.to_string())
    })))
}

fn refresh_artifact(root: Option<&Path>, app: &mut App) -> Result<bool, ConfigError> {
    let observation = match root {
        Some(root) => load_artifact_observation(root)?,
        None => None,
    };
    if app.artifact() == observation.as_ref() {
        return Ok(false);
    }
    app.apply_artifact(observation);
    Ok(true)
}
fn check_git_worktree_changes(
    project_root: Option<&Path>,
    worktree_changes: &mut Option<GitWorktreeChangeDetector>,
) -> bool {
    let (Some(root), Some(detector)) = (project_root, worktree_changes) else {
        return false;
    };
    matches!(detector.check(root), Ok(GitWorktreeChange::Changed))
}

fn check_git_metadata_changes(
    project_root: Option<&Path>,
    metadata_changes: &mut Option<GitMetadataChangeDetector>,
) -> bool {
    let (Some(root), Some(detector)) = (project_root, metadata_changes) else {
        return false;
    };
    matches!(detector.check(root), Ok(GitMetadataChange::Changed))
}

fn collect_change_requests(
    project_root: Option<&Path>,
    markdown_changes: &mut Option<MarkdownChangeDetector>,
    worktree_changes: &mut Option<GitWorktreeChangeDetector>,
    metadata_changes: &mut Option<GitMetadataChangeDetector>,
    config_changes: &mut Option<ConfigChangeDetector>,
    requests: &mut RefreshRequest,
) -> bool {
    requests.markdown |= check_markdown_changes(project_root, markdown_changes);
    let config_changed = check_config_changes(project_root, config_changes);
    requests.markdown |= config_changed;
    let worktree_changed = check_git_worktree_changes(project_root, worktree_changes);
    let metadata_changed = check_git_metadata_changes(project_root, metadata_changes);
    requests.git |= worktree_changed || metadata_changed;
    config_changed
}

fn reload_activity_excludes(
    root: &Path,
    activity_excludes: &mut Vec<PathBuf>,
    worker: &mut Option<GitWorktreeWorker>,
) -> Result<bool, ConfigError> {
    let new_excludes = load_project_config(root)?.activity().excludes().to_vec();
    if *activity_excludes == new_excludes {
        return Ok(false);
    }
    *activity_excludes = new_excludes;
    *worker = None;
    Ok(true)
}

#[cfg(test)]
fn new_git_worktree_detector(
    project_root: Option<&Path>,
    app: &App,
) -> Option<GitWorktreeChangeDetector> {
    match (project_root, app.activity()) {
        (Some(root), ActivityState::Available(_)) => Some(GitWorktreeChangeDetector::new(root)),
        _ => None,
    }
}

fn reconcile_worktree_detector(
    project_root: Option<&Path>,
    app: &App,
    worktree_changes: &mut Option<GitWorktreeChangeDetector>,
) {
    match (project_root, app.activity(), worktree_changes.is_some()) {
        (Some(root), ActivityState::Available(_), false) => {
            *worktree_changes = Some(GitWorktreeChangeDetector::new(root));
        }
        (_, ActivityState::Available(_), true) => {}
        _ => *worktree_changes = None,
    }
}

#[derive(Default)]
struct RefreshOutcome {
    markdown: bool,
    git: bool,
}

impl RefreshOutcome {
    fn source(&self) -> Option<RefreshSource> {
        match (self.markdown, self.git) {
            (true, true) => Some(RefreshSource::MarkdownAndGit),
            (true, false) => Some(RefreshSource::Markdown),
            (false, true) => Some(RefreshSource::Git),
            (false, false) => None,
        }
    }
}

fn refresh_open_detail(root: Option<&Path>, app: &mut App) -> bool {
    let (Some(root), Some((path, status))) = (root, app.detail_request()) else {
        return false;
    };
    let inspection = collect_git_file_inspection(root, &path, &status).unwrap_or(
        GitFileInspection::Unavailable(GitFileInspectionUnavailable::GitError),
    );
    app.apply_detail_inspection(inspection);
    true
}
fn changed_file_preview_active(width: u16, height: u16, app: &App) -> bool {
    !app.is_file_browser_open()
        && ui::has_global_preview(width, height, app.preview_visible())
        && app.focused_panel() == FocusedPanel::ChangedFiles
}

fn refresh_changed_file_preview(root: Option<&Path>, app: &mut App) -> bool {
    let (Some(root), Some((path, status))) = (root, app.selected_changed_file_request()) else {
        return false;
    };
    let inspection = collect_git_file_inspection(root, &path, &status).unwrap_or(
        GitFileInspection::Unavailable(GitFileInspectionUnavailable::GitError),
    );
    app.apply_preview_inspection(inspection);
    true
}

fn refresh_detail_after_git_refresh(
    root: &Path,
    app: &mut App,
    outcome: &RefreshOutcome,
    preview_active: bool,
) -> bool {
    if !outcome.git {
        return false;
    }
    let detail_changed = refresh_open_detail(Some(root), app);
    let preview_changed = preview_active && refresh_changed_file_preview(Some(root), app);
    detail_changed || preview_changed
}
fn apply_pending_refreshes(
    root: &Path,
    app: &mut App,
    worktree_changes: &mut Option<GitWorktreeChangeDetector>,
    requests: &mut RefreshRequest,
) -> RefreshOutcome {
    let mut outcome = RefreshOutcome::default();

    if requests.markdown {
        match collect_markdown_state(root) {
            Ok((plan, tasks)) => {
                app.apply_markdown_state(plan, tasks);
                app.clear_refresh_error();
                if let Err(error) = refresh_artifact(Some(root), app) {
                    app.set_refresh_error(error.to_string());
                }
                outcome.markdown = true;
            }
            Err(error) => app.set_refresh_error(error.to_string()),
        }
        requests.markdown = false;
    }

    if requests.git
        && let Ok(activity) = collect_activity_state(root)
    {
        app.apply_activity_state(activity);
        requests.git = false;
        reconcile_worktree_detector(Some(root), app, worktree_changes);
        outcome.git = true;
    }

    outcome
}

fn apply_refresh_status(
    app: &mut App,
    requests: &RefreshRequest,
    outcome: &RefreshOutcome,
    elapsed: Duration,
) -> bool {
    let mut changed = false;
    if let Some(source) = outcome.source() {
        app.record_refresh(source, elapsed);
        changed = true;
    }
    app.set_refresh_pending(requests.markdown || requests.git) || changed
}

fn contextual_build_test_kind(
    app: &App,
    area: ratatui::layout::Rect,
    key: KeyEvent,
) -> Option<BuildTestKind> {
    if key.kind != KeyEventKind::Press
        || key.modifiers != KeyModifiers::NONE
        || key.code != KeyCode::Char(' ')
    {
        return None;
    }
    ui::contextual_verification_target(app, area)
}

/// Runs the synchronous TUI event loop without polling in a busy loop.
pub fn run(
    terminal: &mut AppTerminal,
    project_root: Option<&Path>,
    config: &ProjectConfig,
    app: &mut App,
) -> io::Result<()> {
    let session_start = Instant::now();
    let mut needs_render = true;
    let mut scheduler = PollScheduler::new(session_start, PROJECT_POLL_INTERVAL);
    let mut markdown_changes = project_root.map(MarkdownChangeDetector::new);
    let mut activity_excludes = config.activity().excludes().to_vec();
    let mut worktree_worker = new_git_worktree_worker(project_root, app, &activity_excludes);
    let mut metadata_changes = project_root.map(GitMetadataChangeDetector::new);
    let mut config_changes = project_root.map(ConfigChangeDetector::new);
    let mut current_work_changes = project_root.map(CurrentWorkChangeDetector::new);
    let mut requests = RefreshRequest::default();
    let mut build_test_runtime = BuildTestRuntime::new(config.clone());
    build_test_runtime.initialize(project_root, app);
    let initial_size = terminal.size()?;
    if changed_file_preview_active(initial_size.width, initial_size.height, app) {
        refresh_changed_file_preview(project_root, app);
    }

    while app.is_running() {
        if needs_render {
            terminal.draw(|frame| ui::render(frame, app))?;
            needs_render = false;
        }

        if event::poll(EVENT_POLL_TIMEOUT)? {
            match event::read()? {
                Event::Key(key) if app.is_file_browser_open() => {
                    let size = terminal.size()?;
                    handle_navigation_key(project_root, app, key, size.into());
                    needs_render = true;
                }
                Event::Key(key)
                    if key.kind == KeyEventKind::Press
                        && key.modifiers == KeyModifiers::NONE
                        && key.code == KeyCode::Char('r')
                        && !app.has_detail_view() =>
                {
                    if let Some(root) = project_root {
                        match try_collect_project_snapshot(root) {
                            Ok(snapshot) => {
                                app.apply_snapshot(snapshot);
                                app.clear_refresh_error();
                                if let Err(error) = reload_activity_excludes(
                                    root,
                                    &mut activity_excludes,
                                    &mut worktree_worker,
                                ) {
                                    app.set_refresh_error(error.to_string());
                                }
                            }
                            Err(error) => app.set_refresh_error(error.to_string()),
                        }
                        if let Some(detector) = &mut markdown_changes {
                            detector.sync(root);
                        }
                        reconcile_worktree_worker(
                            project_root,
                            app,
                            &activity_excludes,
                            &mut worktree_worker,
                        );
                        sync_git_worktree_worker(&mut worktree_worker);
                        if let Some(detector) = &mut metadata_changes {
                            detector.sync(root);
                        }
                        if let Some(detector) = &mut config_changes {
                            detector.sync(root);
                        }
                        if let Some(detector) = &mut current_work_changes {
                            detector.sync(root);
                        }
                        refresh_open_detail(Some(root), app);
                        let size = terminal.size()?;
                        if changed_file_preview_active(size.width, size.height, app) {
                            refresh_changed_file_preview(Some(root), app);
                        }
                        refresh_current_work(Some(root), app);
                        if let Err(error) = refresh_artifact(Some(root), app) {
                            app.set_refresh_error(error.to_string());
                        }
                        requests.clear();
                        app.record_refresh(RefreshSource::Manual, session_start.elapsed());
                        app.set_refresh_pending(false);
                    }
                    needs_render = true;
                }
                Event::Key(key) => {
                    let size = terminal.size()?;
                    if let Some(kind) = contextual_build_test_kind(app, size.into(), key) {
                        start_manual_build_test(project_root, app, &mut build_test_runtime, kind);
                    } else {
                        handle_navigation_key(project_root, app, key, size.into());
                    }
                    needs_render = true;
                }
                Event::Resize(width, height) => {
                    if !app.is_file_browser_open() {
                        app.reconcile_focus(ui::focusable_panels(width, height));
                    }
                    if changed_file_preview_active(width, height, app) {
                        refresh_changed_file_preview(project_root, app);
                    }
                    needs_render = true;
                }
                _ => {}
            }
        }

        match worktree_worker
            .as_mut()
            .map(GitWorktreeWorker::try_recv_changed)
        {
            Some(Ok(changed)) => requests.git |= changed,
            Some(Err(())) => worktree_worker = None,
            None => {}
        }

        needs_render |= poll_build_test_execution(project_root, app, &mut build_test_runtime);

        if scheduler.is_due(Instant::now()) {
            observe_active_build_test_inputs(project_root, &mut build_test_runtime);
            needs_render |= check_build_test_freshness(project_root, app, &build_test_runtime);
            let config_changed = collect_change_requests(
                project_root,
                &mut markdown_changes,
                &mut None,
                &mut metadata_changes,
                &mut config_changes,
                &mut requests,
            );
            if config_changed
                && let Some(root) = project_root
                && let Err(error) =
                    reload_activity_excludes(root, &mut activity_excludes, &mut worktree_worker)
            {
                app.set_refresh_error(error.to_string());
            }
            if worktree_worker
                .as_mut()
                .is_some_and(|worker| !worker.request_scan())
            {
                worktree_worker = None;
            }
            if check_current_work_changes(project_root, &mut current_work_changes) {
                needs_render |= refresh_current_work(project_root, app);
            }
            if let Some(root) = project_root {
                let outcome = apply_pending_refreshes(root, app, &mut None, &mut requests);
                reconcile_worktree_worker(
                    project_root,
                    app,
                    &activity_excludes,
                    &mut worktree_worker,
                );
                let size = terminal.size()?;
                let preview_active = changed_file_preview_active(size.width, size.height, app);
                needs_render |=
                    refresh_detail_after_git_refresh(root, app, &outcome, preview_active);
                needs_render |=
                    apply_refresh_status(app, &requests, &outcome, session_start.elapsed());
            }
        }
    }

    Ok(())
}

fn handle_navigation_key(
    project_root: Option<&Path>,
    app: &mut App,
    key: KeyEvent,
    area: ratatui::layout::Rect,
) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    // Full View consumes the event before either navigation view can handle it.
    if app.has_detail_view() {
        app.handle_key_with_focusable_panels(key, ui::focusable_panels(area.width, area.height));
        if app.has_detail_view() && key.modifiers == KeyModifiers::NONE {
            let delta = match key.code {
                KeyCode::Down | KeyCode::Char('j') => Some(1),
                KeyCode::Up | KeyCode::Char('k') => Some(-1),
                _ => None,
            };
            if let Some(delta) = delta {
                app.scroll_detail(delta, ui::detail_scroll_limit(app, area));
            }
        }
        return;
    }
    let was_browser = app.is_file_browser_open();
    if was_browser
        || (!app.has_detail_view()
            && key.code == KeyCode::Char('f')
            && key.modifiers == KeyModifiers::CONTROL)
    {
        app.handle_key_with_focusable_panels(key, ui::focusable_panels(area.width, area.height));
        if app.has_detail_view() {
            return;
        }
        if !app.is_file_browser_open() {
            if changed_file_preview_active(area.width, area.height, app) {
                refresh_changed_file_preview(project_root, app);
            }
            return;
        }
        if !was_browser {
            app.file_browser.refresh(project_root, true);
        } else if key.modifiers == KeyModifiers::NONE {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    app.file_browser.move_selection(1, project_root)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    app.file_browser.move_selection(-1, project_root)
                }
                KeyCode::Right | KeyCode::Enter => app.file_browser.enter_selected(project_root),
                KeyCode::Left => app.file_browser.parent(project_root),
                KeyCode::Char('r') => app.file_browser.refresh(project_root, false),
                _ => {}
            }
        } else if key.modifiers == KeyModifiers::CONTROL
            && ui::preview_layout_available(area.width, area.height)
        {
            let delta = match key.code {
                KeyCode::Down => 1,
                KeyCode::Up => -1,
                _ => 0,
            };
            let limit = ui::browser_preview_scroll_limit(app, area);
            app.file_browser.scroll(delta, limit);
        }
        if ui::preview_layout_available(area.width, area.height) {
            app.file_browser
                .scroll(0, ui::browser_preview_scroll_limit(app, area));
        }
        return;
    }
    let selected_before = app.selected_changed_file();
    let preview_was_active = changed_file_preview_active(area.width, area.height, app);
    let focused_before = app.focused_panel();
    app.handle_key_with_focusable_panels(key, ui::focusable_panels(area.width, area.height));
    if app.has_detail_view() {
        refresh_open_detail(project_root, app);
    } else {
        if key.modifiers == KeyModifiers::CONTROL
            && ui::has_global_preview(area.width, area.height, app.preview_visible())
        {
            let delta = match key.code {
                KeyCode::Down => Some(1),
                KeyCode::Up => Some(-1),
                _ => None,
            };
            if let Some(delta) = delta {
                app.scroll_preview(delta, ui::preview_scroll_limit(app, area));
            }
        }
        if changed_file_preview_active(area.width, area.height, app)
            && (selected_before != app.selected_changed_file()
                || !preview_was_active
                || focused_before != app.focused_panel())
        {
            refresh_changed_file_preview(project_root, app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PlanState, TaskState};
    use devscope::{
        change::{GitWorktreeChange, MarkdownChange},
        progress::{
            ArtifactStatus, BuildTestExecutionError, BuildTestKind, BuildTestState, PlanSummary,
        },
        project::{ProjectSnapshot, collect_project_snapshot},
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static ID: AtomicUsize = AtomicUsize::new(0);

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    #[test]
    fn maps_manual_build_and_test_keys_only_on_press() {
        let mut app = App::new(ProjectSnapshot::unavailable());
        app.reconcile_focus(&[crate::app::FocusedPanel::Evidence]);
        app.apply_build_test_state(BuildTestKind::BuildDebug, BuildTestState::NotRun);
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        let area = ratatui::layout::Rect::new(0, 0, 120, 30);
        assert_eq!(
            contextual_build_test_kind(&app, area, key(KeyCode::Char(' '))),
            Some(BuildTestKind::BuildDebug)
        );
        app.select_evidence_detail(BuildTestKind::Test);
        assert_eq!(
            contextual_build_test_kind(&app, area, key(KeyCode::Char(' '))),
            Some(BuildTestKind::Test)
        );
        for code in [
            KeyCode::Char('b'),
            KeyCode::Char('t'),
            KeyCode::Char('r'),
            KeyCode::Char('q'),
            KeyCode::Char('j'),
        ] {
            assert_eq!(contextual_build_test_kind(&app, area, key(code)), None);
        }
        let mut repeat = key(KeyCode::Char(' '));
        repeat.kind = KeyEventKind::Repeat;
        assert_eq!(contextual_build_test_kind(&app, area, repeat), None);
    }

    #[test]
    fn contextual_run_requires_visible_evidence_and_runnable_selected_target() {
        use crate::app::FocusedPanel;
        let mut app = App::new(ProjectSnapshot::unavailable());
        let area = ratatui::layout::Rect::new(0, 0, 120, 30);
        let space = key(KeyCode::Char(' '));
        app.apply_build_test_state(BuildTestKind::BuildDebug, BuildTestState::NotRun);
        app.apply_build_test_state(BuildTestKind::Test, BuildTestState::NotRun);
        for panel in [
            FocusedPanel::Tasks,
            FocusedPanel::ChangedFiles,
            FocusedPanel::Evidence,
        ] {
            app.reconcile_focus(&[panel]);
            for code in ['b', 't'] {
                assert_eq!(
                    contextual_build_test_kind(&app, area, key(KeyCode::Char(code))),
                    None
                );
            }
            assert_eq!(
                contextual_build_test_kind(&app, area, space),
                if panel == FocusedPanel::Evidence {
                    Some(BuildTestKind::BuildDebug)
                } else {
                    None
                }
            );
        }
        app.toggle_preview();
        assert_eq!(contextual_build_test_kind(&app, area, space), None);
        handle_navigation_key(None, &mut app, key(KeyCode::Enter), area);
        assert_eq!(
            contextual_build_test_kind(&app, area, space),
            Some(BuildTestKind::BuildDebug)
        );
        assert_eq!(
            contextual_build_test_kind(&app, ratatui::layout::Rect::new(0, 0, 40, 20), space),
            None
        );
        app.apply_build_test_state(BuildTestKind::BuildDebug, BuildTestState::Unavailable);
        assert_eq!(contextual_build_test_kind(&app, area, space), None);
        app.apply_build_test_state(
            BuildTestKind::BuildDebug,
            BuildTestState::Running(devscope::progress::BuildTestRun::new(
                BuildTestKind::BuildDebug,
                "fixture",
                "build",
            )),
        );
        assert_eq!(contextual_build_test_kind(&app, area, space), None);
        app.select_evidence_detail(BuildTestKind::Test);
        assert_eq!(contextual_build_test_kind(&app, area, space), None);
        app.apply_build_test_state(BuildTestKind::BuildDebug, BuildTestState::NotRun);
        app.apply_build_test_state(
            BuildTestKind::Test,
            BuildTestState::ExecutionError(BuildTestExecutionError::new(
                BuildTestKind::Test,
                "fixture",
                "test",
                "error",
            )),
        );
        assert_eq!(
            contextual_build_test_kind(&app, area, space),
            Some(BuildTestKind::Test)
        );
        for modifier in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SHIFT,
        ] {
            assert_eq!(
                contextual_build_test_kind(&app, area, KeyEvent::new(KeyCode::Char(' '), modifier)),
                None
            );
        }
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(contextual_build_test_kind(&app, area, space), None);
    }

    #[test]
    fn contextual_space_starts_selected_kind_and_does_not_cancel_when_hidden() {
        let root = temp_root();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(root.join(".devscope/config.toml"),
            "[verify.build]\nprogram = \"missing-contextual-fixture\"\n[verify.build.release]\nprogram = \"missing-release-fixture\"\n[verify.test]\nprogram = \"missing-contextual-fixture\"\n").unwrap();
        let area = ratatui::layout::Rect::new(0, 0, 120, 30);
        for kind in BuildTestKind::ALL {
            let mut app = App::new(ProjectSnapshot::unavailable());
            let mut runtime = BuildTestRuntime::new(load_project_config(&root).unwrap());
            runtime.initialize(Some(&root), &mut app);
            app.reconcile_focus(&[crate::app::FocusedPanel::Evidence]);
            app.select_evidence_detail(kind);
            let selected = contextual_build_test_kind(&app, area, key(KeyCode::Char(' '))).unwrap();
            assert_eq!(selected, kind);
            assert!(start_manual_build_test(
                Some(&root),
                &mut app,
                &mut runtime,
                selected
            ));
            assert_eq!(app.evidence_detail_kind(), Some(kind));
            for other in BuildTestKind::ALL
                .into_iter()
                .filter(|other| *other != kind)
            {
                assert_eq!(app.build_test_state(other), &BuildTestState::NotRun);
                app.select_evidence_detail(other);
                assert_eq!(
                    contextual_build_test_kind(&app, area, key(KeyCode::Char(' '))),
                    None
                );
                assert!(!start_manual_build_test(
                    Some(&root),
                    &mut app,
                    &mut runtime,
                    other
                ));
            }
            app.select_evidence_detail(kind);
            let BuildTestState::Running(run) = app.build_test_state(kind) else {
                panic!()
            };
            assert_eq!(run.kind(), kind);
            assert_eq!(
                run.command_label(),
                if kind == BuildTestKind::BuildRelease {
                    "missing-release-fixture"
                } else {
                    "missing-contextual-fixture"
                }
            );
            assert!(matches!(
                app.build_test_state(kind),
                BuildTestState::Running(_)
            ));
            assert_eq!(
                contextual_build_test_kind(&app, area, key(KeyCode::Char(' '))),
                None
            );
            assert!(!start_manual_build_test(
                Some(&root),
                &mut app,
                &mut runtime,
                kind
            ));
            handle_navigation_key(Some(&root), &mut app, key(KeyCode::Enter), area);
            assert!(!app.preview_visible());
            assert!(runtime.is_active());
            let deadline = Instant::now() + Duration::from_secs(5);
            while runtime.is_active() {
                assert!(Instant::now() < deadline, "fixture worker did not finish");
                poll_build_test_execution(Some(&root), &mut app, &mut runtime);
                std::thread::yield_now();
            }
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn current_work_refresh_is_independent_and_recovers_from_malformed_content() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let plan = app.plan();
        let activity = app.activity().clone();
        let path = root.join(".devscope/work/current.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "# Current Work\nParent: docs/roadmap.md\nTask: Work\n- [ ] First\n",
        )
        .unwrap();

        assert!(refresh_current_work(Some(&root), &mut app));
        assert!(matches!(app.current_work(), CurrentWorkState::Available(_)));
        assert_eq!(app.plan(), plan);
        assert_eq!(app.activity(), &activity);

        fs::write(&path, "broken").unwrap();
        assert!(refresh_current_work(Some(&root), &mut app));
        assert_eq!(app.current_work(), &CurrentWorkState::Unavailable);

        fs::write(
            &path,
            "# Current Work\nParent: docs/roadmap.md\nTask: Work\n- [x] First\n",
        )
        .unwrap();
        assert!(refresh_current_work(Some(&root), &mut app));
        assert!(matches!(app.current_work(), CurrentWorkState::Available(_)));

        fs::remove_file(path).unwrap();
        assert!(refresh_current_work(Some(&root), &mut app));
        assert_eq!(app.current_work(), &CurrentWorkState::NotSet);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn current_work_read_error_is_unavailable_and_same_content_recovers() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let plan = app.plan();
        let activity = app.activity().clone();
        let path = root.join(".devscope/work/current.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let contents = "# Current Work\nParent: docs/roadmap.md\nTask: Work\n- [ ] First\n";
        fs::write(&path, contents).unwrap();
        let mut detector = Some(CurrentWorkChangeDetector::new(&root));

        assert!(refresh_current_work(Some(&root), &mut app));
        assert!(matches!(app.current_work(), CurrentWorkState::Available(_)));

        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(check_current_work_changes(Some(&root), &mut detector));
        assert!(refresh_current_work(Some(&root), &mut app));
        assert_eq!(app.current_work(), &CurrentWorkState::Unavailable);
        assert_eq!(app.plan(), plan);
        assert_eq!(app.activity(), &activity);
        assert!(check_current_work_changes(Some(&root), &mut detector));
        assert!(!refresh_current_work(Some(&root), &mut app));

        fs::remove_dir(&path).unwrap();
        fs::write(&path, contents).unwrap();
        assert!(check_current_work_changes(Some(&root), &mut detector));
        assert!(refresh_current_work(Some(&root), &mut app));
        assert!(matches!(app.current_work(), CurrentWorkState::Available(_)));
        assert_eq!(app.plan(), plan);
        assert_eq!(app.activity(), &activity);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn is_not_due_before_the_interval() {
        let start = Instant::now();
        let mut scheduler = PollScheduler::new(start, Duration::from_secs(1));
        assert!(!scheduler.is_due(start + Duration::from_millis(999)));
    }

    fn wait_for_worktree_result(worker: &mut GitWorktreeWorker) -> bool {
        let result = worker
            .results
            .recv_timeout(Duration::from_secs(2))
            .expect("worktree worker did not finish");
        worker.scan_in_flight = false;
        worker.last_duration = Some(result.duration);
        result.generation == worker.generation
            && matches!(result.change, Some(GitWorktreeChange::Changed))
    }

    #[test]
    fn activity_exclude_reload_rebuilds_the_worker_without_changing_build_test_snapshot() {
        let root = git_root();
        fs::create_dir_all(root.join("generated")).unwrap();
        fs::write(root.join("generated/output.txt"), "before").unwrap();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify]\nexclude = [\"old-output\"]\n[verify.build]\nprogram = \"old-build\"\n[activity]\nexclude = []\n",
        )
        .unwrap();
        let startup_config = load_project_config(&root).unwrap();
        let runtime = BuildTestRuntime::new(startup_config.clone());
        let mut activity_excludes = startup_config.activity().excludes().to_vec();
        let mut worker = Some(GitWorktreeWorker::new_with_exclusions(
            root.clone(),
            activity_excludes.clone(),
        ));

        fs::write(
            root.join(".devscope/config.toml"),
            "[verify]\nexclude = [\"new-output\"]\n[verify.build]\nprogram = \"new-build\"\n[activity]\nexclude = [\"generated\"]\n",
        )
        .unwrap();
        assert!(reload_activity_excludes(&root, &mut activity_excludes, &mut worker).unwrap());
        assert!(worker.is_none());
        assert_eq!(activity_excludes, [PathBuf::from("generated")]);
        assert_eq!(runtime.config(), &startup_config);
        assert_eq!(
            runtime.config().verify().build().unwrap().program(),
            "old-build"
        );

        let mut detector =
            GitWorktreeChangeDetector::new_with_exclusions(&root, activity_excludes.clone());
        assert_eq!(detector.check(&root).unwrap(), GitWorktreeChange::Unchanged);

        worker = Some(GitWorktreeWorker::new_with_exclusions(
            root.clone(),
            activity_excludes.clone(),
        ));
        fs::remove_file(root.join(".devscope/config.toml")).unwrap();
        assert!(reload_activity_excludes(&root, &mut activity_excludes, &mut worker).unwrap());
        assert!(worker.is_none());
        assert!(activity_excludes.is_empty());
        assert_eq!(runtime.config(), &startup_config);

        let mut detector =
            GitWorktreeChangeDetector::new_with_exclusions(&root, activity_excludes.clone());
        assert_eq!(detector.check(&root).unwrap(), GitWorktreeChange::Unchanged);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verify_only_reload_keeps_activity_worker_and_build_test_snapshot() {
        let root = git_root();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify]\nexclude = [\"old-output\"]\n[verify.build]\nprogram = \"old-build\"\n[activity]\nexclude = [\"generated\"]\n",
        )
        .unwrap();
        let startup_config = load_project_config(&root).unwrap();
        let runtime = BuildTestRuntime::new(startup_config.clone());
        let mut activity_excludes = startup_config.activity().excludes().to_vec();
        let mut worker = Some(GitWorktreeWorker::new_with_exclusions(
            root.clone(),
            activity_excludes.clone(),
        ));

        fs::write(
            root.join(".devscope/config.toml"),
            "[verify]\nexclude = [\"new-output\"]\n[verify.build]\nprogram = \"new-build\"\n[activity]\nexclude = [\"generated\"]\n",
        )
        .unwrap();
        assert!(!reload_activity_excludes(&root, &mut activity_excludes, &mut worker).unwrap());
        assert!(worker.is_some());
        assert_eq!(activity_excludes, [PathBuf::from("generated")]);
        assert_eq!(runtime.config(), &startup_config);
        assert_eq!(
            runtime.config().verify().excludes(),
            [PathBuf::from("old-output")]
        );
        assert_eq!(
            runtime.config().verify().build().unwrap().program(),
            "old-build"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_config_keeps_activity_excludes_worker_and_build_test_snapshot() {
        let root = git_root();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify]\nexclude = [\"old-output\"]\n[activity]\nexclude = [\"generated\"]\n",
        )
        .unwrap();
        let startup_config = load_project_config(&root).unwrap();
        let runtime = BuildTestRuntime::new(startup_config.clone());
        let mut activity_excludes = startup_config.activity().excludes().to_vec();
        let mut worker = Some(GitWorktreeWorker::new_with_exclusions(
            root.clone(),
            activity_excludes.clone(),
        ));

        fs::write(
            root.join(".devscope/config.toml"),
            "[activity]\nexclude = [\"../outside\"]\n",
        )
        .unwrap();
        assert!(reload_activity_excludes(&root, &mut activity_excludes, &mut worker).is_err());
        assert!(worker.is_some());
        assert_eq!(activity_excludes, [PathBuf::from("generated")]);
        assert_eq!(runtime.config(), &startup_config);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worktree_worker_keeps_detector_state_off_the_event_loop_and_syncs() {
        let root = temp_root();
        fs::write(root.join("tracked.txt"), "before").unwrap();
        let mut worker = GitWorktreeWorker::new(root.clone());

        worker.request_scan();
        worker.request_scan();
        assert!(!wait_for_worktree_result(&mut worker));
        assert!(worker.last_duration().is_some());

        fs::write(root.join("tracked.txt"), "after").unwrap();
        worker.request_scan();
        assert!(wait_for_worktree_result(&mut worker));

        worker.sync();
        worker.request_scan();
        assert!(!wait_for_worktree_result(&mut worker));
        worker.shutdown();
        assert!(worker.join.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worktree_worker_disconnect_clears_in_flight_and_rejects_new_requests() {
        let root = temp_root();
        let mut worker = GitWorktreeWorker::new(root.clone());
        worker.shutdown();
        worker.scan_in_flight = true;

        assert_eq!(worker.try_recv_changed(), Err(()));
        assert!(!worker.scan_in_flight);
        assert!(!worker.request_scan());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worker_retains_last_successful_diagnostics_across_fast_scans_and_sync() {
        let root = temp_root();
        let mut worker = GitWorktreeWorker::new(root.clone());
        let first = WorktreeScanDiagnostics {
            visited_entries: 1,
            subtrees: Vec::new(),
        };
        worker.last_diagnostics = Some(first.clone());
        worker.last_diagnostic_duration = Some(Duration::from_millis(3));
        worker.sync();
        assert_eq!(worker.last_diagnostics, Some(first.clone()));
        assert_eq!(
            worker.last_diagnostic_duration,
            Some(Duration::from_millis(3))
        );

        let replacement = WorktreeScanDiagnostics {
            visited_entries: 2,
            subtrees: Vec::new(),
        };
        worker.last_diagnostics = Some(replacement.clone());
        worker.last_diagnostic_duration = Some(Duration::from_millis(4));
        assert_eq!(worker.last_diagnostics, Some(replacement));
        assert_eq!(
            worker.last_diagnostic_duration,
            Some(Duration::from_millis(4))
        );
        worker.shutdown();
        let fresh = GitWorktreeWorker::new(root.clone());
        assert!(fresh.last_diagnostics.is_none());
        assert!(fresh.last_diagnostic_duration.is_none());
        drop(fresh);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn slow_worktree_scan_classification_uses_scan_duration_only() {
        assert!(!is_slow_worktree_scan(
            SLOW_WORKTREE_SCAN - Duration::from_millis(1)
        ));
        assert!(is_slow_worktree_scan(SLOW_WORKTREE_SCAN));
        let first = is_slow_worktree_scan(Duration::from_millis(250));
        let second = is_slow_worktree_scan(Duration::from_millis(1));
        let third = is_slow_worktree_scan(Duration::from_millis(1));
        assert!(first);
        assert!(!second);
        assert!(!third);
        assert!(is_slow_worktree_scan(Duration::from_millis(250)));
    }

    #[test]
    fn is_due_at_the_interval_and_reschedules() {
        let start = Instant::now();
        let mut scheduler = PollScheduler::new(start, Duration::from_secs(1));
        let due = start + Duration::from_secs(1);
        assert!(scheduler.is_due(due));
        assert!(!scheduler.is_due(due));
        assert!(!scheduler.is_due(due + Duration::from_millis(999)));
        assert!(scheduler.is_due(due + Duration::from_secs(1)));
    }

    #[test]
    fn delayed_checks_emit_only_one_tick() {
        let start = Instant::now();
        let mut scheduler = PollScheduler::new(start, Duration::from_secs(1));
        let delayed = start + Duration::from_millis(3500);
        assert!(scheduler.is_due(delayed));
        assert!(!scheduler.is_due(delayed));
        assert!(!scheduler.is_due(delayed + Duration::from_millis(999)));
        assert!(scheduler.is_due(delayed + Duration::from_secs(1)));
    }

    #[test]
    fn polling_checks_return_changed_and_update_detector_baselines() {
        let root = temp_root();
        let markdown = root.join("tasks.md");
        fs::write(&markdown, "- [ ] First").unwrap();
        let mut markdown_detector = Some(MarkdownChangeDetector::new(&root));
        fs::write(&markdown, "- [ ] First\n- [ ] Second").unwrap();
        assert!(check_markdown_changes(Some(&root), &mut markdown_detector));
        assert_eq!(
            markdown_detector.as_mut().unwrap().check(&root).unwrap(),
            MarkdownChange::Unchanged
        );

        let file = root.join("a.txt");
        fs::write(&file, "a").unwrap();
        let mut worktree_detector = Some(GitWorktreeChangeDetector::new(&root));
        fs::write(&file, "a longer value").unwrap();
        assert!(check_git_worktree_changes(
            Some(&root),
            &mut worktree_detector
        ));
        assert_eq!(
            worktree_detector.as_mut().unwrap().check(&root).unwrap(),
            GitWorktreeChange::Unchanged
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detector_signals_create_source_specific_refresh_requests() {
        let root = temp_root();
        let markdown = root.join("tasks.md");
        fs::write(&markdown, "- [ ] First").unwrap();
        let mut markdown_detector = Some(MarkdownChangeDetector::new(&root));
        let mut metadata_detector = Some(GitMetadataChangeDetector::new(&root));
        let mut worktree_detector = None;
        let mut requests = RefreshRequest::default();

        fs::write(&markdown, "- [x] First\n- [ ] Second").unwrap();
        collect_change_requests(
            Some(&root),
            &mut markdown_detector,
            &mut worktree_detector,
            &mut metadata_detector,
            &mut None,
            &mut requests,
        );
        assert!(requests.markdown);
        assert!(!requests.git);

        let git_root = git_root();
        git(&git_root, &["branch", "other"]);
        let mut markdown_detector = Some(MarkdownChangeDetector::new(&git_root));
        let mut worktree_detector = Some(GitWorktreeChangeDetector::new(&git_root));
        let mut metadata_detector = Some(GitMetadataChangeDetector::new(&git_root));
        let mut requests = RefreshRequest::default();
        git(&git_root, &["switch", "other"]);
        collect_change_requests(
            Some(&git_root),
            &mut markdown_detector,
            &mut worktree_detector,
            &mut metadata_detector,
            &mut None,
            &mut requests,
        );
        assert!(!requests.markdown);
        assert!(requests.git);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(git_root);
    }
    #[test]
    fn markdown_pending_refresh_updates_only_markdown_state() {
        let root = temp_root();
        let markdown = root.join("tasks.md");
        fs::write(&markdown, "- [ ] First").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        fs::write(&markdown, "- [x] First\n- [ ] Second").unwrap();
        let mut requests = RefreshRequest {
            markdown: true,
            git: false,
        };
        let mut worktree = None;

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.markdown);
        assert!(!outcome.git);
        assert!(!requests.markdown);
        assert_eq!(app.plan(), PlanState::Available(PlanSummary::new(1, 2)));
        assert_eq!(app.activity(), &ActivityState::NotRepository);
        let TaskState::Available(tasks) = app.tasks() else {
            panic!("tasks should be available")
        };
        assert_eq!(tasks.items()[0].text(), "Second");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn git_pending_refresh_updates_only_activity_state() {
        let root = git_root();
        fs::write(root.join("tasks.md"), "- [ ] First\n- [ ] Second").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        app.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        let plan = app.plan();
        let tasks = app.tasks().clone();
        let selected = app.selected_task();
        fs::write(root.join("code.rs"), "changed").unwrap();
        let mut requests = RefreshRequest {
            markdown: false,
            git: true,
        };
        let mut worktree = Some(GitWorktreeChangeDetector::new(&root));

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(!outcome.markdown);
        assert!(outcome.git);
        assert!(!requests.git);
        assert_eq!(app.plan(), plan);
        assert_eq!(app.tasks(), &tasks);
        assert_eq!(app.selected_task(), selected);
        let ActivityState::Available(activity) = app.activity() else {
            panic!("Git activity should be available")
        };
        assert_eq!(activity.changed_files(), 2);
        let code_file = activity
            .changed_file_items()
            .iter()
            .find(|file| file.path == Path::new("code.rs"))
            .expect("new Git detail should be retained");
        assert_eq!(code_file.status, devscope::progress::GitFileStatus::Added);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn git_refresh_reconciles_worktree_detector_for_repository_lifecycle() {
        let root = temp_root();
        let mut app = App::new(collect_project_snapshot(&root));
        let mut worktree = None;
        git(&root, &["init"]);
        let mut requests = RefreshRequest {
            markdown: false,
            git: true,
        };

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(matches!(app.activity(), ActivityState::Available(_)));
        assert!(worktree.is_some());
        fs::remove_dir_all(root.join(".git")).unwrap();
        requests.git = true;
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert_eq!(app.activity(), &ActivityState::NotRepository);
        assert!(worktree.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_pending_refresh_is_retained_without_changing_app_state() {
        let root = temp_root();
        let mut app = App::new(collect_project_snapshot(&root));
        app.record_refresh(RefreshSource::Git, Duration::from_secs(10));
        let status = app.refresh_status();
        let activity = app.activity().clone();
        fs::remove_dir_all(&root).unwrap();
        fs::write(&root, "not a directory").unwrap();
        let mut requests = RefreshRequest {
            markdown: true,
            git: false,
        };
        let mut worktree = None;

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(!outcome.markdown);
        assert!(!outcome.git);
        assert!(!requests.markdown);
        assert!(!requests.git);
        assert_eq!(app.activity(), &activity);
        assert!(!apply_refresh_status(
            &mut app,
            &requests,
            &outcome,
            Duration::from_secs(20)
        ));
        assert_eq!(app.refresh_status().last_source(), status.last_source());
        assert_eq!(app.refresh_status().last_update(), status.last_update());
        assert!(!app.refresh_status().retry_pending());
        let _ = fs::remove_file(root);
    }

    #[test]
    fn automatic_config_error_is_consumed_and_recovers_with_exclusion() {
        let root = temp_root();
        fs::write(root.join("tasks.md"), "- [ ] Root").unwrap();
        fs::create_dir_all(root.join("translations")).unwrap();
        fs::write(root.join("translations/ja.md"), "- [ ] Translation").unwrap();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[artifact]\npath = \"output.bin\"\n",
        )
        .unwrap();
        fs::write(root.join("output.bin"), "artifact").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        assert!(refresh_artifact(Some(&root), &mut app).unwrap());
        let previous_plan = app.plan();
        let previous_tasks = app.tasks().clone();
        let previous_artifact = app.artifact().cloned();
        let mut markdown = Some(MarkdownChangeDetector::new(&root));
        let mut worktree = None;
        let mut metadata = Some(GitMetadataChangeDetector::new(&root));
        let mut config = Some(ConfigChangeDetector::new(&root));
        let mut requests = RefreshRequest::default();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut config,
            &mut requests,
        );
        assert!(!requests.markdown);

        fs::write(
            root.join(".devscope/config.toml"),
            "[artifact]\npath = 123\n",
        )
        .unwrap();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut config,
            &mut requests,
        );
        assert!(requests.markdown);
        assert!(!requests.git);
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(!outcome.markdown);
        assert!(!requests.markdown);
        assert!(
            app.refresh_error()
                .is_some_and(|error| error.contains("Config error"))
        );
        assert_eq!(app.plan(), previous_plan);
        assert_eq!(app.tasks(), &previous_tasks);
        assert_eq!(app.artifact(), previous_artifact.as_ref());

        fs::write(root.join("recovered.bin"), "recovered").unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[plan]\nexclude = [\"translations\"]\n\n[artifact]\npath = \"recovered.bin\"\n",
        )
        .unwrap();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut config,
            &mut requests,
        );
        assert!(requests.markdown);
        assert!(!requests.git);
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.markdown);
        assert!(app.refresh_error().is_none());
        assert_eq!(app.plan(), PlanState::Available(PlanSummary::new(0, 1)));
        assert!(matches!(
            app.artifact().map(|artifact| artifact.status()),
            Some(ArtifactStatus::Exists { .. })
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn git_refresh_does_not_clear_a_plan_error() {
        let root = git_root();
        let mut app = App::new(collect_project_snapshot(&root));
        app.set_refresh_error("Config error: invalid Config");
        let mut requests = RefreshRequest {
            markdown: false,
            git: true,
        };
        let mut worktree = new_git_worktree_detector(Some(&root), &app);
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(!outcome.markdown);
        assert!(app.refresh_error().is_some());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn refresh_status_tracks_markdown_and_git_successes() {
        let mut app = App::new(ProjectSnapshot::unavailable());
        let requests = RefreshRequest::default();
        let markdown = RefreshOutcome {
            markdown: true,
            git: false,
        };
        assert!(apply_refresh_status(
            &mut app,
            &requests,
            &markdown,
            Duration::from_secs(12)
        ));
        assert_eq!(app.refresh_status().last_source(), RefreshSource::Markdown);
        assert_eq!(app.refresh_status().last_update(), Duration::from_secs(12));
        assert!(!app.refresh_status().retry_pending());

        let git = RefreshOutcome {
            markdown: false,
            git: true,
        };
        assert!(apply_refresh_status(
            &mut app,
            &requests,
            &git,
            Duration::from_secs(15)
        ));
        assert_eq!(app.refresh_status().last_source(), RefreshSource::Git);
        assert_eq!(app.refresh_status().last_update(), Duration::from_secs(15));

        let both = RefreshOutcome {
            markdown: true,
            git: true,
        };
        assert!(apply_refresh_status(
            &mut app,
            &requests,
            &both,
            Duration::from_secs(19)
        ));
        assert_eq!(
            app.refresh_status().last_source(),
            RefreshSource::MarkdownAndGit
        );
        assert_eq!(app.refresh_status().last_update(), Duration::from_secs(19));
    }

    #[test]
    fn retry_recovery_clears_pending_after_success() {
        let mut app = App::new(ProjectSnapshot::unavailable());
        let failed_requests = RefreshRequest {
            markdown: true,
            git: false,
        };
        assert!(apply_refresh_status(
            &mut app,
            &failed_requests,
            &RefreshOutcome::default(),
            Duration::from_secs(5)
        ));
        assert!(app.refresh_status().retry_pending());
        let recovered_requests = RefreshRequest::default();
        assert!(apply_refresh_status(
            &mut app,
            &recovered_requests,
            &RefreshOutcome {
                markdown: true,
                git: false
            },
            Duration::from_secs(8)
        ));
        assert!(!app.refresh_status().retry_pending());
        assert_eq!(app.refresh_status().last_source(), RefreshSource::Markdown);
        assert_eq!(app.refresh_status().last_update(), Duration::from_secs(8));
    }

    #[test]
    fn empty_request_does_not_refresh_or_change_status() {
        let root = temp_root();
        let mut app = App::new(collect_project_snapshot(&root));
        let status = app.refresh_status();
        let mut requests = RefreshRequest::default();
        let mut worktree = None;

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(!outcome.markdown);
        assert!(!outcome.git);
        assert!(!apply_refresh_status(
            &mut app,
            &requests,
            &outcome,
            Duration::from_secs(1)
        ));
        assert_eq!(app.refresh_status(), status);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unchanged_poll_produces_no_refresh() {
        let root = git_root();
        let mut app = App::new(collect_project_snapshot(&root));
        let status = app.refresh_status();
        let mut markdown = Some(MarkdownChangeDetector::new(&root));
        let mut worktree = new_git_worktree_detector(Some(&root), &app);
        let mut metadata = Some(GitMetadataChangeDetector::new(&root));
        let mut requests = RefreshRequest::default();

        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut None,
            &mut requests,
        );
        assert!(!requests.markdown);
        assert!(!requests.git);

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(!outcome.markdown);
        assert!(!outcome.git);
        assert!(!apply_refresh_status(
            &mut app,
            &requests,
            &outcome,
            Duration::from_secs(1)
        ));
        assert_eq!(app.refresh_status(), status);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn config_changes_request_markdown_without_git() {
        let root = temp_root();
        let mut markdown = Some(MarkdownChangeDetector::new(&root));
        let mut worktree = None;
        let mut metadata = Some(GitMetadataChangeDetector::new(&root));
        let mut config = Some(ConfigChangeDetector::new(&root));
        let mut requests = RefreshRequest::default();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut config,
            &mut requests,
        );
        assert!(!requests.markdown);
        assert!(!requests.git);

        let config_path = root.join(".devscope/config.toml");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, "[plan]\nexclude = [\"alpha\"]\n").unwrap();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut config,
            &mut requests,
        );
        assert!(requests.markdown);
        assert!(!requests.git);

        requests.clear();
        fs::write(&config_path, "[plan]\nexclude = [\"bravo\"]\n").unwrap();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut config,
            &mut requests,
        );
        assert!(requests.markdown);
        assert!(!requests.git);

        requests.clear();
        fs::remove_file(config_path).unwrap();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut config,
            &mut requests,
        );
        assert!(requests.markdown);
        assert!(!requests.git);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn markdown_only_poll_refreshes_only_markdown_state() {
        let root = temp_root();
        let markdown_path = root.join("tasks.md");
        fs::write(&markdown_path, "- [ ] First").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        let mut markdown = Some(MarkdownChangeDetector::new(&root));
        let mut worktree = None;
        let mut metadata = Some(GitMetadataChangeDetector::new(&root));
        let mut requests = RefreshRequest::default();

        fs::write(&markdown_path, "- [x] First\n- [ ] Second").unwrap();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut None,
            &mut requests,
        );
        assert!(requests.markdown);
        assert!(!requests.git);

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.markdown);
        assert!(!outcome.git);
        assert_eq!(app.plan(), PlanState::Available(PlanSummary::new(1, 2)));
        assert_eq!(app.activity(), &ActivityState::NotRepository);
        assert!(apply_refresh_status(
            &mut app,
            &requests,
            &outcome,
            Duration::from_secs(1)
        ));
        assert_eq!(app.refresh_status().last_source(), RefreshSource::Markdown);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worktree_change_poll_refreshes_git_activity() {
        let root = git_root();
        let mut app = App::new(collect_project_snapshot(&root));
        let mut markdown = Some(MarkdownChangeDetector::new(&root));
        let mut worktree = new_git_worktree_detector(Some(&root), &app);
        let mut metadata = Some(GitMetadataChangeDetector::new(&root));
        let mut requests = RefreshRequest::default();

        fs::write(root.join("tracked.txt"), "changed").unwrap();
        collect_change_requests(
            Some(&root),
            &mut markdown,
            &mut worktree,
            &mut metadata,
            &mut None,
            &mut requests,
        );
        assert!(!requests.markdown);
        assert!(requests.git);

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(!outcome.markdown);
        assert!(outcome.git);
        let ActivityState::Available(activity) = app.activity() else {
            panic!("Git activity should be available");
        };
        assert!(activity.changed_file_items().iter().any(|file| {
            file.path == Path::new("tracked.txt")
                && file.status == devscope::progress::GitFileStatus::Modified
        }));
        assert!(apply_refresh_status(
            &mut app,
            &requests,
            &outcome,
            Duration::from_secs(1)
        ));
        assert_eq!(app.refresh_status().last_source(), RefreshSource::Git);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn metadata_only_commit_poll_refreshes_recent_commits() {
        let root = git_root();
        let mut app = App::new(collect_project_snapshot(&root));
        let mut markdown = Some(MarkdownChangeDetector::new(&root));
        let mut worktree = new_git_worktree_detector(Some(&root), &app);
        let mut metadata = Some(GitMetadataChangeDetector::new(&root));
        let mut requests = RefreshRequest::default();

        git(&root, &["commit", "--allow-empty", "-m", "metadata only"]);
        let markdown_changed = check_markdown_changes(Some(&root), &mut markdown);
        let worktree_changed = check_git_worktree_changes(Some(&root), &mut worktree);
        let metadata_changed = check_git_metadata_changes(Some(&root), &mut metadata);
        assert!(!markdown_changed);
        assert!(!worktree_changed);
        assert!(metadata_changed);

        requests.markdown |= markdown_changed;
        requests.git |= worktree_changed || metadata_changed;
        assert!(!requests.markdown);
        assert!(requests.git);

        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(!outcome.markdown);
        assert!(outcome.git);
        let ActivityState::Available(activity) = app.activity() else {
            panic!("Git activity should be available");
        };
        assert_eq!(activity.recent_commits()[0].summary, "metadata only");
        assert!(apply_refresh_status(
            &mut app,
            &requests,
            &outcome,
            Duration::from_secs(1)
        ));
        assert_eq!(app.refresh_status().last_source(), RefreshSource::Git);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_observation_distinguishes_unconfigured_configured_and_invalid_config() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());

        assert!(matches!(load_artifact_observation(&root), Ok(None)));
        assert!(!refresh_artifact(Some(&root), &mut app).unwrap());
        assert!(app.artifact().is_none());

        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[artifact]\npath = \"output.bin\"\n",
        )
        .unwrap();
        fs::write(root.join("output.bin"), "artifact").unwrap();
        assert!(matches!(
            load_artifact_observation(&root),
            Ok(Some(ArtifactObservation { .. }))
        ));
        assert!(refresh_artifact(Some(&root), &mut app).unwrap());
        assert!(matches!(
            app.artifact().map(|artifact| artifact.status()),
            Some(ArtifactStatus::Exists { .. })
        ));
        let previous = app.artifact().cloned();

        fs::write(
            root.join(".devscope/config.toml"),
            "[artifact]\npath = 123\n",
        )
        .unwrap();
        assert!(load_artifact_observation(&root).is_err());
        assert!(refresh_artifact(Some(&root), &mut app).is_err());
        assert_eq!(app.artifact(), previous.as_ref());

        fs::write(
            root.join(".devscope/config.toml"),
            "[artifact]\npath = \"missing.bin\"\n",
        )
        .unwrap();
        assert!(refresh_artifact(Some(&root), &mut app).unwrap());
        assert!(matches!(
            app.artifact().map(|artifact| artifact.status()),
            Some(ArtifactStatus::Missing)
        ));

        fs::remove_file(root.join(".devscope/config.toml")).unwrap();
        assert!(refresh_artifact(Some(&root), &mut app).unwrap());
        assert!(app.artifact().is_none());
        let _ = fs::remove_dir_all(root);
    }
    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "devscope-event-loop-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn git(root: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }

    fn git_root() -> PathBuf {
        let root = temp_root();
        git(&root, &["init"]);
        git(&root, &["config", "user.name", "DevScope Test"]);
        git(&root, &["config", "user.email", "devscope@test.invalid"]);
        fs::write(root.join("tracked.txt"), "tracked").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "initial"]);
        root
    }

    #[test]
    fn detail_enter_returns_without_toggling_or_scrolling_overview_preview() {
        let root = git_root();
        fs::write(root.join("tracked.txt"), "changed\n").unwrap();
        let area = ratatui::layout::Rect::new(0, 0, 80, 25);
        for visible in [false, true] {
            let mut app = App::new(collect_project_snapshot(&root));
            handle_navigation_key(Some(&root), &mut app, key(KeyCode::Left), area);
            if !visible {
                app.toggle_preview();
            }
            app.scroll_preview(2, 5);
            handle_navigation_key(
                Some(&root),
                &mut app,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
                area,
            );
            app.scroll_detail(3, 5);
            handle_navigation_key(
                Some(&root),
                &mut app,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
                area,
            );
            assert!(app.has_detail_view());
            assert_eq!(app.detail_scroll(), 3);
            handle_navigation_key(Some(&root), &mut app, key(KeyCode::Enter), area);
            assert!(!app.has_detail_view());
            assert!(app.detail_inspection().is_none());
            assert_eq!(app.detail_scroll(), 0);
            assert_eq!(app.focused_panel(), crate::app::FocusedPanel::ChangedFiles);
            assert_eq!(app.selected_changed_file(), Some(0));
            assert_eq!(app.preview_visible(), visible);
            assert_eq!(app.preview_scroll(), 2);
            assert!(app.is_running());
            // Only a separate Overview Enter toggles Preview.
            handle_navigation_key(Some(&root), &mut app, key(KeyCode::Enter), area);
            assert_eq!(app.preview_visible(), !visible);
            assert!(!app.has_detail_view());
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn navigation_routes_preview_scroll_and_refresh_without_recollecting_on_scroll() {
        let root = git_root();
        let initial: String = (0..40).map(|i| format!("first {i}\n")).collect();
        fs::write(root.join("tracked.txt"), initial).unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        let area = ratatui::layout::Rect::new(0, 0, 80, 25);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Left), area);
        assert_eq!(app.focused_panel(), crate::app::FocusedPanel::ChangedFiles);
        assert!(format!("{:?}", app.preview_inspection()).contains("first 0"));
        let selected = app.selected_changed_file();
        fs::write(root.join("tracked.txt"), "updated on disk\n").unwrap();
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL),
            area,
        );
        assert_eq!(app.preview_scroll(), 1);
        assert_eq!(app.selected_changed_file(), selected);
        assert!(format!("{:?}", app.preview_inspection()).contains("first 0"));
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL),
            area,
        );
        assert_eq!(app.preview_scroll(), 0);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Enter), area);
        assert!(!app.preview_visible());
        assert!(!app.has_detail_view());
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL),
            area,
        );
        assert_eq!(app.preview_scroll(), 0);
        assert_eq!(app.selected_changed_file(), selected);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Enter), area);
        assert!(format!("{:?}", app.preview_inspection()).contains("updated on disk"));
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Right), area);
        assert_eq!(app.focused_panel(), crate::app::FocusedPanel::Tasks);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Left), area);
        assert!(app.preview_inspection().is_some());
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            area,
        );
        assert!(app.has_detail_view());
        assert!(format!("{:?}", app.detail_inspection()).contains("updated on disk"));
        app.apply_detail_inspection(devscope::progress::GitFileInspection::Diff {
            unstaged: Some(devscope::progress::GitDiffText {
                text: (0..40).map(|i| format!("line {i}\n")).collect(),
                truncated: false,
            }),
            staged: None,
        });
        for code in [KeyCode::Down, KeyCode::Char('j')] {
            handle_navigation_key(Some(&root), &mut app, key(code), area);
        }
        assert_eq!(app.detail_scroll(), 2);
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL),
            area,
        );
        assert_eq!(app.detail_scroll(), 2);
        for code in [KeyCode::Up, KeyCode::Char('k')] {
            handle_navigation_key(Some(&root), &mut app, key(code), area);
        }
        assert_eq!(app.detail_scroll(), 0);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Esc), area);
        assert!(!app.has_detail_view());
        assert!(app.is_running());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hidden_preview_and_modified_shortcuts_do_not_move_selection_or_run_verification() {
        let mut app = App::new(ProjectSnapshot::unavailable());
        app.reconcile_focus(&[crate::app::FocusedPanel::Evidence]);
        for area in [
            ratatui::layout::Rect::new(0, 0, 80, 24),
            ratatui::layout::Rect::new(0, 0, 1, 1),
        ] {
            handle_navigation_key(
                None,
                &mut app,
                KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL),
                area,
            );
            assert_eq!(
                app.evidence_selection(),
                crate::app::EvidenceSelection::BuildDebug
            );
            assert_eq!(app.preview_scroll(), 0);
        }
        for modifier in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SHIFT,
        ] {
            for code in [KeyCode::Char('b'), KeyCode::Char('t')] {
                assert_eq!(
                    contextual_build_test_kind(
                        &app,
                        ratatui::layout::Rect::new(0, 0, 120, 30),
                        KeyEvent::new(code, modifier)
                    ),
                    None
                );
            }
        }
        let mut release = KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL);
        release.kind = KeyEventKind::Release;
        handle_navigation_key(
            None,
            &mut app,
            release,
            ratatui::layout::Rect::new(0, 0, 120, 30),
        );
        assert_eq!(app.preview_scroll(), 0);
    }
    #[test]
    fn automatic_git_refresh_reloads_open_detail_and_resets_scroll() {
        let root = git_root();
        fs::write(root.join("tracked.txt"), "first").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        let panels = [
            crate::app::FocusedPanel::Tasks,
            crate::app::FocusedPanel::Evidence,
            crate::app::FocusedPanel::ChangedFiles,
        ];
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), &panels);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), &panels);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            &panels,
        );
        assert!(refresh_open_detail(Some(&root), &mut app));
        app.scroll_detail(9, 9);

        let markdown_only = RefreshOutcome {
            markdown: true,
            ..RefreshOutcome::default()
        };
        assert!(!refresh_detail_after_git_refresh(
            &root,
            &mut app,
            &markdown_only,
            true
        ));
        assert_eq!(app.detail_scroll(), 9);

        fs::write(root.join("tracked.txt"), "second").unwrap();
        let mut requests = RefreshRequest {
            markdown: false,
            git: true,
        };
        let mut worktree = new_git_worktree_detector(Some(&root), &app);
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(refresh_detail_after_git_refresh(
            &root, &mut app, &outcome, true
        ));
        assert_eq!(app.detail_scroll(), 0);
        assert!(format!("{:?}", app.detail_inspection()).contains("second"));

        fs::write(root.join("tracked.txt"), "tracked").unwrap();
        requests.git = true;
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(!app.has_detail_view());
        assert!(!refresh_detail_after_git_refresh(
            &root, &mut app, &outcome, true
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn browser_navigation_refresh_scroll_and_reopen_are_view_local() {
        let root = git_root();
        fs::create_dir(root.join("browser-dir")).unwrap();
        let original: String = (0..60).map(|i| format!("original {i}\n")).collect();
        fs::write(root.join("browser-dir/a.txt"), &original).unwrap();
        fs::write(root.join("browser-dir/b.txt"), "second file").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        let area = ratatui::layout::Rect::new(0, 0, 80, 30);
        let open = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL);
        handle_navigation_key(Some(&root), &mut app, open, area);
        assert!(app.is_file_browser_open());
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Right), area);
        assert_eq!(app.file_browser.current_dir, Path::new("browser-dir"));
        assert_eq!(
            app.file_browser.preview,
            Some(Ok((original.clone(), false)))
        );
        let selected = app.file_browser.selected;
        fs::write(root.join("browser-dir/a.txt"), "updated").unwrap();
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL);
        handle_navigation_key(Some(&root), &mut app, down, area);
        assert_eq!(app.file_browser.preview_scroll, 1);
        assert_eq!(app.file_browser.selected, selected);
        assert_eq!(
            app.file_browser.preview,
            Some(Ok((original.clone(), false)))
        );
        for k in [
            key(KeyCode::Char('b')),
            key(KeyCode::Char('t')),
            key(KeyCode::Char('p')),
            key(KeyCode::Tab),
            key(KeyCode::Enter),
            open,
        ] {
            handle_navigation_key(Some(&root), &mut app, k, area);
            assert!(app.is_file_browser_open());
            assert!(!app.has_detail_view());
            assert_eq!(
                app.file_browser.preview,
                Some(Ok((original.clone(), false)))
            );
        }
        let browser_before = format!("{:?}", app.file_browser);
        for close in [KeyCode::Enter, KeyCode::Esc] {
            handle_navigation_key(
                Some(&root),
                &mut app,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
                area,
            );
            assert!(
                matches!(app.detail_target(), Some(crate::app::DetailTarget::BrowserFile { text, .. }) if text == &original)
            );
            // Git/background refresh must not replace cached Browser content.
            app.apply_snapshot(collect_project_snapshot(&root));
            assert!(!refresh_open_detail(Some(&root), &mut app));
            for code in [
                KeyCode::Left,
                KeyCode::Right,
                KeyCode::Char('r'),
                KeyCode::Tab,
                KeyCode::Char('p'),
            ] {
                handle_navigation_key(Some(&root), &mut app, key(code), area);
            }
            for code in [KeyCode::Down, KeyCode::Char('j')] {
                handle_navigation_key(Some(&root), &mut app, key(code), area);
            }
            assert_eq!(app.detail_scroll(), 2);
            for code in [KeyCode::Up, KeyCode::Char('k')] {
                handle_navigation_key(Some(&root), &mut app, key(code), area);
            }
            assert_eq!(app.detail_scroll(), 0);
            handle_navigation_key(Some(&root), &mut app, down, area);
            assert_eq!(app.detail_scroll(), 0);
            assert_eq!(format!("{:?}", app.file_browser), browser_before);
            assert!(
                matches!(app.detail_target(), Some(crate::app::DetailTarget::BrowserFile { text, .. }) if text == &original)
            );
            handle_navigation_key(Some(&root), &mut app, key(close), area);
            assert!(!app.has_detail_view());
            assert!(app.is_file_browser_open());
            assert_eq!(format!("{:?}", app.file_browser), browser_before);
        }
        handle_navigation_key(
            Some(&root),
            &mut app,
            down,
            ratatui::layout::Rect::new(0, 0, 40, 20),
        );
        assert_eq!(app.file_browser.preview_scroll, 1);
        let tasks = format!("{:?}", app.tasks());
        fs::write(
            root.join("new-plan.md"),
            "- [ ] browser r must not reload Plan",
        )
        .unwrap();
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Char('r')), area);
        assert_eq!(format!("{:?}", app.tasks()), tasks);
        assert_eq!(
            app.file_browser.preview,
            Some(Ok(("updated".into(), false)))
        );
        assert_eq!(app.file_browser.preview_scroll, 0);
        let before = app.file_browser.entries.clone();
        fs::write(root.join("browser-dir/c.txt"), "new").unwrap();
        app.apply_snapshot(collect_project_snapshot(&root));
        assert_eq!(app.file_browser.entries, before);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Esc), area);
        handle_navigation_key(Some(&root), &mut app, open, area);
        assert_eq!(app.file_browser.current_dir, Path::new("browser-dir"));
        assert!(app.file_browser.entries.iter().any(|e| e.name == "c.txt"));
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Left), area);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Enter), area);
        assert_eq!(app.file_browser.current_dir, Path::new("browser-dir"));
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Esc), area);
        fs::remove_dir_all(root.join("browser-dir")).unwrap();
        handle_navigation_key(Some(&root), &mut app, open, area);
        assert!(app.file_browser.current_dir.as_os_str().is_empty());
        assert!(app.file_browser.notice.is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn browser_return_refreshes_overview_inspection_after_background_update() {
        let root = git_root();
        fs::write(root.join("a-new.txt"), "before").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        let area = ratatui::layout::Rect::new(0, 0, 80, 30);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Left), area);
        let selected = app.selected_changed_file();
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
            area,
        );
        fs::write(root.join("a-new.txt"), "after").unwrap();
        app.apply_snapshot(collect_project_snapshot(&root));
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Esc), area);
        assert!(!app.is_file_browser_open());
        assert_eq!(app.selected_changed_file(), selected);
        assert_eq!(
            app.preview_inspection(),
            Some(&GitFileInspection::FileContent {
                text: "after".into(),
                truncated: false,
            })
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn untracked_inspection_refreshes_preview_and_detail_without_reads_on_scroll() {
        let root = git_root();
        let text: String = (0..40).map(|i| format!("untracked line {i}\n")).collect();
        fs::write(root.join("a-new.txt"), &text).unwrap();
        fs::write(root.join("b-new.txt"), "second untracked file\n").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        let area = ratatui::layout::Rect::new(0, 0, 80, 30);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Left), area);
        let initial = GitFileInspection::FileContent {
            text,
            truncated: false,
        };
        assert_eq!(app.preview_inspection(), Some(&initial));
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            area,
        );
        assert_eq!(app.detail_inspection(), Some(&initial));
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Enter), area);
        fs::write(root.join("a-new.txt"), "updated current content\n").unwrap();
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL),
            area,
        );
        assert_eq!(app.preview_scroll(), 1);
        assert_eq!(app.selected_changed_file(), Some(0));
        assert_eq!(app.preview_inspection(), Some(&initial));
        handle_navigation_key(
            Some(&root),
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            area,
        );
        let mut requests = RefreshRequest {
            markdown: false,
            git: true,
        };
        let mut worktree = new_git_worktree_detector(Some(&root), &app);
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(refresh_detail_after_git_refresh(
            &root, &mut app, &outcome, true
        ));
        let updated = GitFileInspection::FileContent {
            text: "updated current content\n".into(),
            truncated: false,
        };
        assert_eq!(app.preview_inspection(), Some(&updated));
        assert_eq!(app.detail_inspection(), Some(&updated));
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Esc), area);
        handle_navigation_key(Some(&root), &mut app, key(KeyCode::Down), area);
        assert_eq!(app.preview_scroll(), 0);
        assert_eq!(
            app.preview_inspection(),
            Some(&GitFileInspection::FileContent {
                text: "second untracked file\n".into(),
                truncated: false,
            })
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn passive_preview_follows_selection_and_refreshes_after_git_changes() {
        let root = git_root();
        fs::write(root.join("second.txt"), "original").unwrap();
        git(&root, &["add", "second.txt"]);
        git(&root, &["commit", "-m", "add second"]);
        fs::write(root.join("tracked.txt"), "first preview").unwrap();
        fs::write(root.join("second.txt"), "second preview").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));

        assert!(refresh_changed_file_preview(Some(&root), &mut app));
        let first_path = app.selected_changed_file_request().unwrap().0;
        let first_diff = format!("{:?}", app.preview_inspection());
        app.handle_key_with_focusable_panels(
            key(KeyCode::Tab),
            &[
                crate::app::FocusedPanel::Tasks,
                crate::app::FocusedPanel::Evidence,
                crate::app::FocusedPanel::ChangedFiles,
            ],
        );
        app.handle_key_with_focusable_panels(
            key(KeyCode::Tab),
            &[
                crate::app::FocusedPanel::Tasks,
                crate::app::FocusedPanel::Evidence,
                crate::app::FocusedPanel::ChangedFiles,
            ],
        );
        app.handle_key_with_focusable_panels(
            key(KeyCode::Char('j')),
            &[
                crate::app::FocusedPanel::Tasks,
                crate::app::FocusedPanel::Evidence,
                crate::app::FocusedPanel::ChangedFiles,
            ],
        );
        assert!(refresh_changed_file_preview(Some(&root), &mut app));
        let selected_path = app.selected_changed_file_request().unwrap().0;
        assert_ne!(selected_path, first_path);
        assert_ne!(format!("{:?}", app.preview_inspection()), first_diff);

        fs::write(root.join(selected_path), "preview refreshed").unwrap();
        let mut requests = RefreshRequest {
            markdown: false,
            git: true,
        };
        let mut worktree = new_git_worktree_detector(Some(&root), &app);
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(refresh_detail_after_git_refresh(
            &root, &mut app, &outcome, true
        ));
        assert!(format!("{:?}", app.preview_inspection()).contains("refreshed"));

        fs::write(root.join("tracked.txt"), "tracked").unwrap();
        fs::write(root.join("second.txt"), "original").unwrap();
        requests.git = true;
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(!refresh_detail_after_git_refresh(
            &root, &mut app, &outcome, true
        ));
        assert!(app.preview_inspection().is_none());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn hidden_preview_skips_diff_refresh_after_git_activity_update() {
        let root = git_root();
        fs::write(root.join("tracked.txt"), "changed").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        let panels = [
            crate::app::FocusedPanel::Tasks,
            crate::app::FocusedPanel::Evidence,
            crate::app::FocusedPanel::ChangedFiles,
        ];
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), &panels);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), &panels);
        app.handle_key_with_focusable_panels(key(KeyCode::Char('p')), &panels);
        assert!(!app.preview_visible());
        assert!(!changed_file_preview_active(120, 30, &app));
        let mut requests = RefreshRequest {
            markdown: false,
            git: true,
        };
        let mut worktree = new_git_worktree_detector(Some(&root), &app);
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(!refresh_detail_after_git_refresh(
            &root, &mut app, &outcome, false
        ));
        assert!(app.preview_inspection().is_none());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn failed_open_detail_refresh_replaces_cached_diff_with_error() {
        let root = git_root();
        fs::write(root.join("tracked.txt"), "changed").unwrap();
        let mut app = App::new(collect_project_snapshot(&root));
        let panels = [
            crate::app::FocusedPanel::Tasks,
            crate::app::FocusedPanel::Evidence,
            crate::app::FocusedPanel::ChangedFiles,
        ];
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), &panels);
        app.handle_key_with_focusable_panels(key(KeyCode::Tab), &panels);
        app.handle_key_with_focusable_panels(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            &panels,
        );
        assert!(refresh_open_detail(Some(&root), &mut app));
        fs::remove_dir_all(root.join(".git")).unwrap();
        assert!(refresh_open_detail(Some(&root), &mut app));
        assert!(matches!(
            app.detail_inspection(),
            Some(GitFileInspection::Unavailable(
                GitFileInspectionUnavailable::GitError
            ))
        ));
        let _ = fs::remove_dir_all(root);
    }
}
