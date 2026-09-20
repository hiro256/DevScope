use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::terminal::AppTerminal;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use devscope::{
    change::{
        ConfigChange, ConfigChangeDetector, CurrentWorkChange, CurrentWorkChangeDetector,
        GitMetadataChange, GitMetadataChangeDetector, GitWorktreeChange, GitWorktreeChangeDetector,
        MarkdownChange, MarkdownChangeDetector, WorktreeScanDiagnostics, diagnose_worktree_scan,
    },
    config::{ConfigError, ProjectConfig, load_project_config},
    current_work::load_current_work,
    progress::{
        ArtifactObservation, BuildTestExecution, BuildTestExecutionCompletion, BuildTestFreshness,
        BuildTestFreshnessBaseline, BuildTestInputChange, BuildTestKind, BuildTestState,
        GitFileDiff, GitFileDiffUnavailable, collect_git_file_diff,
        evaluate_completed_build_test_freshness_with_exclusions, observe_artifact,
        resolve_build_test_command, save_build_test_state,
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
    fn new(root: PathBuf) -> Self {
        let (command_sender, command_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        let join = thread::spawn(move || {
            let mut detector = GitWorktreeChangeDetector::new(&root);
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
                                    diagnose_worktree_scan(&root).ok(),
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
                    self.last_diagnostics = result.diagnostics;
                    self.last_diagnostic_duration = result.diagnostic_duration;
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

fn new_git_worktree_worker(project_root: Option<&Path>, app: &App) -> Option<GitWorktreeWorker> {
    match (project_root, app.activity()) {
        (Some(root), ActivityState::Available(_)) => {
            Some(GitWorktreeWorker::new(root.to_path_buf()))
        }
        _ => None,
    }
}

fn reconcile_worktree_worker(
    project_root: Option<&Path>,
    app: &App,
    worker: &mut Option<GitWorktreeWorker>,
) {
    if matches!(app.activity(), ActivityState::Available(_)) {
        if worker.is_none() {
            *worker = project_root.map(|root| GitWorktreeWorker::new(root.to_path_buf()));
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
) {
    requests.markdown |= check_markdown_changes(project_root, markdown_changes);
    requests.markdown |= check_config_changes(project_root, config_changes);
    let worktree_changed = check_git_worktree_changes(project_root, worktree_changes);
    let metadata_changed = check_git_metadata_changes(project_root, metadata_changes);
    requests.git |= worktree_changed || metadata_changed;
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
    let diff = collect_git_file_diff(root, &path, &status)
        .unwrap_or(GitFileDiff::Unavailable(GitFileDiffUnavailable::Error));
    app.apply_detail_diff(diff);
    true
}
fn changed_file_preview_active(width: u16, height: u16, app: &App) -> bool {
    ui::has_global_preview(width, height, app.preview_visible())
        && app.focused_panel() == FocusedPanel::ChangedFiles
}

fn refresh_changed_file_preview(root: Option<&Path>, app: &mut App) -> bool {
    let (Some(root), Some((path, status))) = (root, app.selected_changed_file_request()) else {
        return false;
    };
    let diff = collect_git_file_diff(root, &path, &status)
        .unwrap_or(GitFileDiff::Unavailable(GitFileDiffUnavailable::Error));
    app.apply_preview_diff(diff);
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

#[derive(Default)]
struct BuildTestRuntime {
    active: Option<BuildTestExecution>,
    build_baseline: Option<BuildTestFreshnessBaseline>,
    test_baseline: Option<BuildTestFreshnessBaseline>,
    active_baseline: Option<BuildTestFreshnessBaseline>,
    active_inputs_changed: bool,
    config: ProjectConfig,
}

impl BuildTestRuntime {
    fn clear_baseline(&mut self, kind: BuildTestKind) {
        match kind {
            BuildTestKind::Build => self.build_baseline = None,
            BuildTestKind::Test => self.test_baseline = None,
        }
    }

    fn set_baseline(&mut self, kind: BuildTestKind, baseline: Option<BuildTestFreshnessBaseline>) {
        match kind {
            BuildTestKind::Build => self.build_baseline = baseline,
            BuildTestKind::Test => self.test_baseline = baseline,
        }
    }
}

fn initialize_build_test_availability(
    project_root: Option<&Path>,
    config: &ProjectConfig,
    app: &mut App,
) {
    for kind in [BuildTestKind::Build, BuildTestKind::Test] {
        if project_root.is_some_and(|root| resolve_build_test_command(root, config, kind).is_some())
        {
            if matches!(app.build_test_state(kind), BuildTestState::Unavailable) {
                app.apply_build_test_state(kind, BuildTestState::NotRun);
            }
        } else {
            app.apply_build_test_state(kind, BuildTestState::Unavailable);
        }
    }
}

fn manual_build_test_kind(key: KeyEvent) -> Option<BuildTestKind> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    match key.code {
        KeyCode::Char('b') => Some(BuildTestKind::Build),
        KeyCode::Char('t') => Some(BuildTestKind::Test),
        _ => None,
    }
}

fn start_manual_build_test(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &mut BuildTestRuntime,
    kind: BuildTestKind,
) -> bool {
    if runtime.active.is_some() {
        return false;
    }

    app.select_evidence_detail(kind);
    runtime.clear_baseline(kind);
    runtime.active_baseline = None;
    runtime.active_inputs_changed = false;
    let Some(root) = project_root else {
        app.apply_build_test_state(kind, BuildTestState::Unavailable);
        return true;
    };
    let Some(spec) = resolve_build_test_command(root, &runtime.config, kind) else {
        app.apply_build_test_state(kind, BuildTestState::Unavailable);
        return true;
    };

    runtime.active_baseline = BuildTestFreshnessBaseline::capture_with_exclusions(
        root,
        runtime.config.verify().excludes(),
    )
    .ok();
    match BuildTestExecution::start(spec) {
        Ok(execution) => {
            app.apply_build_test_state(kind, BuildTestState::Running(execution.run().clone()));
            runtime.active = Some(execution);
        }
        Err(error) => {
            runtime.active_baseline = None;
            app.apply_build_test_state(kind, BuildTestState::ExecutionError(error));
            if let Some(root) = project_root {
                let _ = save_build_test_state(root, kind, app.build_test_state(kind), None);
            }
        }
    }
    true
}

fn apply_build_test_completion(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &mut BuildTestRuntime,
    completion: BuildTestExecutionCompletion,
    inputs_changed: bool,
    started_baseline: Option<BuildTestFreshnessBaseline>,
) {
    match completion {
        BuildTestExecutionCompletion::Completed(result) => {
            let kind = result.kind();
            let (freshness, persisted_baseline) =
                project_root.map_or((BuildTestFreshness::Stale, None), |root| {
                    evaluate_completed_build_test_freshness_with_exclusions(
                        root,
                        runtime.config.verify().excludes(),
                        started_baseline.as_ref(),
                        inputs_changed,
                    )
                });
            let mut result = result;
            if matches!(freshness, BuildTestFreshness::Stale) {
                result.mark_stale();
            }
            app.apply_build_test_state(kind, BuildTestState::Completed(result));
            runtime.set_baseline(kind, persisted_baseline);
            if let (Some(root), Some(baseline)) = (
                project_root,
                match kind {
                    BuildTestKind::Build => runtime.build_baseline.as_ref(),
                    BuildTestKind::Test => runtime.test_baseline.as_ref(),
                },
            ) {
                let _ =
                    save_build_test_state(root, kind, app.build_test_state(kind), Some(baseline));
            } else if let Some(root) = project_root {
                let _ = save_build_test_state(root, kind, app.build_test_state(kind), None);
            }
        }
        BuildTestExecutionCompletion::ExecutionError(error) => {
            let kind = error.kind();
            runtime.clear_baseline(kind);
            runtime.active_baseline = None;
            runtime.active_inputs_changed = false;
            app.apply_build_test_state(kind, BuildTestState::ExecutionError(error));
            if let Some(root) = project_root {
                let _ = save_build_test_state(root, kind, app.build_test_state(kind), None);
            }
        }
    }
}

fn check_completed_build_test_freshness(
    project_root: &Path,
    app: &mut App,
    baseline: Option<&BuildTestFreshnessBaseline>,
    kind: BuildTestKind,
    exclusions: &[PathBuf],
) -> bool {
    let BuildTestState::Completed(mut result) = app.build_test_state(kind).clone() else {
        return false;
    };
    if matches!(result.freshness(), BuildTestFreshness::Stale) {
        return false;
    }
    let Some(baseline) = baseline else {
        return false;
    };
    if !matches!(
        baseline.check_with_exclusions(project_root, exclusions),
        Ok(BuildTestInputChange::Changed)
    ) {
        return false;
    }

    result.mark_stale();
    app.apply_build_test_state(kind, BuildTestState::Completed(result));
    true
}

fn check_build_test_freshness(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &BuildTestRuntime,
) -> bool {
    let Some(project_root) = project_root else {
        return false;
    };

    check_completed_build_test_freshness(
        project_root,
        app,
        runtime.build_baseline.as_ref(),
        BuildTestKind::Build,
        runtime.config.verify().excludes(),
    ) | check_completed_build_test_freshness(
        project_root,
        app,
        runtime.test_baseline.as_ref(),
        BuildTestKind::Test,
        runtime.config.verify().excludes(),
    )
}
fn observe_active_build_test_inputs(project_root: Option<&Path>, runtime: &mut BuildTestRuntime) {
    if runtime.active_inputs_changed {
        return;
    }
    if let (Some(root), Some(baseline)) = (project_root, runtime.active_baseline.as_ref()) {
        runtime.active_inputs_changed = matches!(
            baseline.check_with_exclusions(root, runtime.config.verify().excludes()),
            Ok(BuildTestInputChange::Changed)
        );
    }
}

fn finish_build_test_execution(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &mut BuildTestRuntime,
    completion: BuildTestExecutionCompletion,
) {
    observe_active_build_test_inputs(project_root, runtime);
    let inputs_changed = runtime.active_inputs_changed;
    let started_baseline = runtime.active_baseline.take();
    runtime.active = None;
    runtime.active_inputs_changed = false;
    apply_build_test_completion(
        project_root,
        app,
        runtime,
        completion,
        inputs_changed,
        started_baseline,
    );
}

fn poll_build_test_execution(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &mut BuildTestRuntime,
) -> bool {
    let Some(execution) = runtime.active.as_mut() else {
        return false;
    };

    let completion = execution.try_complete();

    match completion {
        Ok(None) => false,
        Ok(Some(completion)) => {
            finish_build_test_execution(project_root, app, runtime, completion);
            true
        }
        Err(error) => {
            let kind = error.kind();
            runtime.active = None;
            runtime.clear_baseline(kind);
            runtime.active_baseline = None;
            runtime.active_inputs_changed = false;
            app.apply_build_test_state(kind, BuildTestState::ExecutionError(error));
            true
        }
    }
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
    let mut worktree_worker = new_git_worktree_worker(project_root, app);
    let mut metadata_changes = project_root.map(GitMetadataChangeDetector::new);
    let mut config_changes = project_root.map(ConfigChangeDetector::new);
    let mut current_work_changes = project_root.map(CurrentWorkChangeDetector::new);
    let mut requests = RefreshRequest::default();
    let mut build_test_runtime = BuildTestRuntime {
        config: config.clone(),
        ..Default::default()
    };
    initialize_build_test_availability(project_root, config, app);
    for kind in [BuildTestKind::Build, BuildTestKind::Test] {
        if matches!(app.build_test_state(kind), BuildTestState::Completed(result) if matches!(result.freshness(), BuildTestFreshness::Fresh))
        {
            build_test_runtime.set_baseline(
                kind,
                project_root.and_then(|root| {
                    BuildTestFreshnessBaseline::capture_with_exclusions(
                        root,
                        build_test_runtime.config.verify().excludes(),
                    )
                    .ok()
                }),
            );
        }
    }
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
                Event::Key(key)
                    if key.kind == KeyEventKind::Press
                        && key.code == KeyCode::Char('r')
                        && !app.has_detail_view() =>
                {
                    if let Some(root) = project_root {
                        match try_collect_project_snapshot(root) {
                            Ok(snapshot) => {
                                app.apply_snapshot(snapshot);
                                app.clear_refresh_error();
                            }
                            Err(error) => app.set_refresh_error(error.to_string()),
                        }
                        if let Some(detector) = &mut markdown_changes {
                            detector.sync(root);
                        }
                        reconcile_worktree_worker(project_root, app, &mut worktree_worker);
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
                Event::Key(key)
                    if !app.has_detail_view()
                        && let Some(kind) = manual_build_test_kind(key) =>
                {
                    needs_render |=
                        start_manual_build_test(project_root, app, &mut build_test_runtime, kind);
                }
                Event::Key(key) => {
                    let size = terminal.size()?;
                    let was_detail = app.has_detail_view();
                    let selected_before = app.selected_changed_file();
                    let preview_was_active =
                        changed_file_preview_active(size.width, size.height, app);
                    let focused_before = app.focused_panel();
                    app.handle_key_with_focusable_panels(
                        key,
                        ui::focusable_panels(size.width, size.height),
                    );
                    if app.has_detail_view() {
                        let delta = match key.code {
                            KeyCode::Down | KeyCode::Char('j') => Some(1),
                            KeyCode::Up | KeyCode::Char('k') => Some(-1),
                            _ => None,
                        };
                        if let Some(delta) = delta {
                            app.scroll_detail(delta, ui::detail_scroll_limit(app, size.into()));
                        }
                        if !was_detail {
                            refresh_open_detail(project_root, app);
                        }
                    } else if changed_file_preview_active(size.width, size.height, app)
                        && (selected_before != app.selected_changed_file()
                            || !preview_was_active
                            || focused_before != app.focused_panel())
                    {
                        refresh_changed_file_preview(project_root, app);
                    }
                    needs_render = true;
                }
                Event::Resize(width, height) => {
                    app.reconcile_focus(ui::focusable_panels(width, height));
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
            collect_change_requests(
                project_root,
                &mut markdown_changes,
                &mut None,
                &mut metadata_changes,
                &mut config_changes,
                &mut requests,
            );
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
                reconcile_worktree_worker(project_root, app, &mut worktree_worker);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PlanState, TaskState};
    use devscope::{
        change::{GitWorktreeChange, MarkdownChange},
        progress::{
            ArtifactStatus, BuildTestCommandSpec, BuildTestExecution, BuildTestExecutionCompletion,
            BuildTestExecutionError, BuildTestFreshness, BuildTestFreshnessBaseline, BuildTestKind,
            BuildTestOutcome, BuildTestResult, BuildTestRun, BuildTestState, PlanSummary,
            resolve_build_test_command, run_build_test,
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

    fn completed_result(kind: BuildTestKind, outcome: BuildTestOutcome) -> BuildTestResult {
        BuildTestResult::new(
            kind,
            outcome,
            BuildTestFreshness::Fresh,
            "cargo",
            if kind == BuildTestKind::Build {
                "cargo check"
            } else {
                "cargo test"
            },
            Some(if outcome == BuildTestOutcome::Passed {
                0
            } else {
                1
            }),
            Duration::from_millis(1),
            "completed",
            None,
        )
    }

    #[test]
    fn maps_manual_build_and_test_keys_only_on_press() {
        assert_eq!(
            manual_build_test_kind(key(KeyCode::Char('b'))),
            Some(BuildTestKind::Build)
        );
        assert_eq!(
            manual_build_test_kind(key(KeyCode::Char('t'))),
            Some(BuildTestKind::Test)
        );
        for code in [KeyCode::Char('r'), KeyCode::Char('q'), KeyCode::Char('j')] {
            assert_eq!(manual_build_test_kind(key(code)), None);
        }
        let mut repeat = key(KeyCode::Char('b'));
        repeat.kind = KeyEventKind::Repeat;
        assert_eq!(manual_build_test_kind(repeat), None);
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
    fn initializes_build_test_availability_from_the_project_root() {
        let cargo_root = temp_root();
        fs::write(cargo_root.join("Cargo.toml"), "[package]").unwrap();
        let mut cargo_app = App::new(ProjectSnapshot::unavailable());
        initialize_build_test_availability(
            Some(&cargo_root),
            &ProjectConfig::default(),
            &mut cargo_app,
        );
        assert_eq!(
            cargo_app.build_test_state(BuildTestKind::Build),
            &BuildTestState::NotRun
        );
        assert_eq!(
            cargo_app.build_test_state(BuildTestKind::Test),
            &BuildTestState::NotRun
        );

        let non_cargo_root = temp_root();
        let mut non_cargo_app = App::new(ProjectSnapshot::unavailable());
        initialize_build_test_availability(
            Some(&non_cargo_root),
            &ProjectConfig::default(),
            &mut non_cargo_app,
        );
        assert_eq!(
            non_cargo_app.build_test_state(BuildTestKind::Build),
            &BuildTestState::Unavailable
        );
        assert_eq!(
            non_cargo_app.build_test_state(BuildTestKind::Test),
            &BuildTestState::Unavailable
        );
        let _ = fs::remove_dir_all(cargo_root);
        let _ = fs::remove_dir_all(non_cargo_root);
    }

    #[test]
    fn initializes_build_test_availability_per_configured_kind() {
        for (contents, build_available, test_available) in [
            (
                "[verify.build]\nprogram = \"configured-build\"\n",
                true,
                false,
            ),
            (
                "[verify.test]\nprogram = \"configured-test\"\n",
                false,
                true,
            ),
            (
                "[verify.build]\nprogram = \"configured-build\"\n\n[verify.test]\nprogram = \"configured-test\"\n",
                true,
                true,
            ),
        ] {
            let root = temp_root();
            fs::create_dir_all(root.join(".devscope")).unwrap();
            fs::write(root.join(".devscope/config.toml"), contents).unwrap();
            let config = load_project_config(&root).unwrap();
            let mut app = App::new(ProjectSnapshot::unavailable());

            initialize_build_test_availability(Some(&root), &config, &mut app);

            assert_eq!(
                matches!(
                    app.build_test_state(BuildTestKind::Build),
                    BuildTestState::NotRun
                ),
                build_available
            );
            assert_eq!(
                matches!(
                    app.build_test_state(BuildTestKind::Test),
                    BuildTestState::NotRun
                ),
                test_available
            );
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn configured_manual_start_uses_the_resolved_command_spec() {
        let root = temp_root();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify.build]\nprogram = \"configured-build\"\nargs = [\"--focused\"]\n",
        )
        .unwrap();
        let config = load_project_config(&root).unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            config,
            ..Default::default()
        };

        assert!(start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Build,
        ));
        let BuildTestState::Running(run) = app.build_test_state(BuildTestKind::Build) else {
            panic!("configured command should enter the running state");
        };
        assert_eq!(run.source_label(), "configured-build");
        assert_eq!(run.command_label(), "configured-build --focused");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configured_test_overrides_cargo_for_tui_manual_start() {
        let root = temp_root();
        fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify.test]\nprogram = \"configured-test\"\nargs = [\"--override\"]\n",
        )
        .unwrap();
        let config = load_project_config(&root).unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        initialize_build_test_availability(Some(&root), &config, &mut app);
        let mut runtime = BuildTestRuntime {
            config,
            ..Default::default()
        };

        assert!(start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Test,
        ));
        let BuildTestState::Running(run) = app.build_test_state(BuildTestKind::Test) else {
            panic!("configured command should override Cargo for Test");
        };
        assert_eq!(run.source_label(), "configured-test");
        assert_eq!(run.command_label(), "configured-test --override");
        assert_eq!(
            app.build_test_state(BuildTestKind::Build),
            &BuildTestState::NotRun
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_configured_executable_reaches_execution_error_with_resolved_labels() {
        let root = temp_root();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify.test]\nprogram = \"definitely-not-installed-command\"\nargs = [\"--focused\"]\n",
        )
        .unwrap();
        let config = load_project_config(&root).unwrap();
        let spec = resolve_build_test_command(&root, &config, BuildTestKind::Test).unwrap();

        let BuildTestExecutionCompletion::ExecutionError(error) = run_build_test(spec) else {
            panic!("missing configured executable should be an execution error");
        };
        assert_eq!(error.kind(), BuildTestKind::Test);
        assert_eq!(error.source_label(), "definitely-not-installed-command");
        assert_eq!(
            error.command_label(),
            "definitely-not-installed-command --focused"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unavailable_manual_start_keeps_no_active_execution() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();

        assert!(start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Build,
        ));
        assert!(runtime.active.is_none());
        assert_eq!(
            app.build_test_state(BuildTestKind::Build),
            &BuildTestState::Unavailable
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_a_second_manual_start_while_an_execution_is_active() {
        let root = temp_root();
        let active = BuildTestExecution::start(BuildTestCommandSpec::new(
            BuildTestKind::Build,
            "fixture",
            "missing fixture",
            root.join("missing-program"),
            Vec::new(),
            &root,
        ))
        .unwrap();
        let mut runtime = BuildTestRuntime {
            active: Some(active),
            ..Default::default()
        };
        let mut app = App::new(ProjectSnapshot::unavailable());

        assert!(!start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Test,
        ));
        assert!(runtime.active.is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn accepted_manual_starts_select_the_matching_evidence_detail() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();
        assert!(start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Build
        ));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Build));
        runtime.active = Some(
            BuildTestExecution::start(BuildTestCommandSpec::new(
                BuildTestKind::Build,
                "fixture",
                "missing",
                root.join("missing"),
                Vec::new(),
                &root,
            ))
            .unwrap(),
        );
        assert!(!start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Test
        ));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::Build));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn applies_completions_and_keeps_kind_baselines_independent() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();
        let build = completed_result(BuildTestKind::Build, BuildTestOutcome::Passed);
        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(build.clone()),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        assert_eq!(
            app.build_test_state(BuildTestKind::Build),
            &BuildTestState::Completed(build)
        );
        assert!(runtime.build_baseline.is_some());
        assert!(runtime.test_baseline.is_none());

        let test = completed_result(BuildTestKind::Test, BuildTestOutcome::Failed);
        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(test.clone()),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        assert_eq!(
            app.build_test_state(BuildTestKind::Test),
            &BuildTestState::Completed(test)
        );
        assert!(runtime.build_baseline.is_some());
        assert!(runtime.test_baseline.is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_change_marks_completed_evidence_stale_once() {
        let root = temp_root();
        let input = root.join("input.txt");
        fs::write(&input, "before").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();

        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));

        fs::write(input, "after").unwrap();
        assert!(check_build_test_freshness(Some(&root), &mut app, &runtime));
        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::Build) else {
            panic!("a completed Build result should remain completed");
        };
        assert_eq!(result.outcome(), BuildTestOutcome::Passed);
        assert_eq!(result.freshness(), BuildTestFreshness::Stale);
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::Test),
            BuildTestState::Unavailable
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn changed_inputs_during_a_run_mark_the_completed_result_stale() {
        let root = temp_root();
        let input = root.join("input.txt");
        fs::write(&input, "before").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            active_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        fs::write(input, "after changed").unwrap();
        finish_build_test_execution(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
            )),
        );

        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::Test) else {
            panic!("a completed Test result should remain completed");
        };
        assert_eq!(result.outcome(), BuildTestOutcome::Failed);
        assert_eq!(result.freshness(), BuildTestFreshness::Stale);
        assert!(runtime.test_baseline.is_none());

        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn final_active_input_check_keeps_unchanged_completion_fresh() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            active_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        finish_build_test_execution(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
            )),
        );

        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::Build) else {
            panic!("a completed Build result should remain completed");
        };
        assert_eq!(result.freshness(), BuildTestFreshness::Fresh);
        assert!(runtime.build_baseline.is_some());
        assert!(runtime.active_baseline.is_none());
        assert!(!runtime.active_inputs_changed);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn final_active_input_check_ignores_target_changes() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            active_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/output"), "generated change").unwrap();
        finish_build_test_execution(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
            )),
        );

        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::Build) else {
            panic!("a completed Build result should remain completed");
        };
        assert_eq!(result.freshness(), BuildTestFreshness::Fresh);

        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn freshness_checks_ignore_git_and_target_but_stale_completed_test_for_project_input() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();

        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join(".git/metadata"), "internal change").unwrap();
        fs::write(root.join("target/output"), "generated change").unwrap();
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));

        fs::write(root.join("README.md"), "relevant change").unwrap();
        assert!(check_build_test_freshness(Some(&root), &mut app, &runtime));
        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::Test) else {
            panic!("a completed Test result should remain completed");
        };
        assert_eq!(result.outcome(), BuildTestOutcome::Failed);
        assert_eq!(result.freshness(), BuildTestFreshness::Stale);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_start_monitor_ignores_excluded_changes_and_keeps_changed_flag_sticky() {
        let root = temp_root();
        let input = root.join("input.txt");
        fs::write(&input, "before").unwrap();
        let mut runtime = BuildTestRuntime {
            active_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join(".git/metadata"), "internal change").unwrap();
        fs::write(root.join("target/output"), "generated change").unwrap();
        observe_active_build_test_inputs(Some(&root), &mut runtime);
        assert!(!runtime.active_inputs_changed);

        fs::write(input, "after changed").unwrap();
        observe_active_build_test_inputs(Some(&root), &mut runtime);
        assert!(runtime.active_inputs_changed);
        fs::remove_file(root.join("input.txt")).unwrap();
        observe_active_build_test_inputs(Some(&root), &mut runtime);
        assert!(runtime.active_inputs_changed);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn freshness_check_errors_preserve_result_and_baseline() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();
        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::Build,
                BuildTestOutcome::Passed,
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );

        fs::remove_dir_all(&root).unwrap();
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::Build) else {
            panic!("a completed Build result should remain completed");
        };
        assert_eq!(result.freshness(), BuildTestFreshness::Fresh);
        assert!(runtime.build_baseline.is_some());
    }
    #[test]
    fn freshness_checks_leave_non_completed_states_unchanged() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "before").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let runtime = BuildTestRuntime {
            build_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };
        fs::write(root.join("input.txt"), "after changed").unwrap();

        app.apply_build_test_state(BuildTestKind::Build, BuildTestState::NotRun);
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::Build),
            BuildTestState::NotRun
        ));

        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::Build,
                "cargo",
                "cargo check",
            )),
        );
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::Build),
            BuildTestState::Running(_)
        ));

        app.apply_build_test_state(
            BuildTestKind::Build,
            BuildTestState::ExecutionError(BuildTestExecutionError::new(
                BuildTestKind::Build,
                "cargo",
                "cargo check",
                "could not start",
            )),
        );
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::Build),
            BuildTestState::ExecutionError(_)
        ));

        app.apply_build_test_state(BuildTestKind::Build, BuildTestState::Unavailable);
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::Build),
            BuildTestState::Unavailable
        ));
        assert!(runtime.build_baseline.is_some());

        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn execution_errors_clear_only_the_matching_baseline() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            build_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            test_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::ExecutionError(BuildTestExecutionError::new(
                BuildTestKind::Build,
                "cargo",
                "cargo check",
                "worker disconnected",
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        assert!(matches!(
            app.build_test_state(BuildTestKind::Build),
            BuildTestState::ExecutionError(_)
        ));
        assert!(runtime.build_baseline.is_none());
        assert!(runtime.test_baseline.is_some());
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
        app.handle_key_with_focusable_panels(key(KeyCode::Enter), &panels);
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
        assert!(format!("{:?}", app.detail_diff()).contains("second"));

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
        let first_diff = format!("{:?}", app.preview_diff());
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
        assert_ne!(format!("{:?}", app.preview_diff()), first_diff);

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
        assert!(format!("{:?}", app.preview_diff()).contains("refreshed"));

        fs::write(root.join("tracked.txt"), "tracked").unwrap();
        fs::write(root.join("second.txt"), "original").unwrap();
        requests.git = true;
        let outcome = apply_pending_refreshes(&root, &mut app, &mut worktree, &mut requests);
        assert!(outcome.git);
        assert!(!refresh_detail_after_git_refresh(
            &root, &mut app, &outcome, true
        ));
        assert!(app.preview_diff().is_none());
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
        assert!(app.preview_diff().is_none());
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
        app.handle_key_with_focusable_panels(key(KeyCode::Enter), &panels);
        assert!(refresh_open_detail(Some(&root), &mut app));
        fs::remove_dir_all(root.join(".git")).unwrap();
        assert!(refresh_open_detail(Some(&root), &mut app));
        assert!(matches!(
            app.detail_diff(),
            Some(GitFileDiff::Unavailable(GitFileDiffUnavailable::Error))
        ));
        let _ = fs::remove_dir_all(root);
    }
}
