use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

use crate::terminal::AppTerminal;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use devscope::{
    change::{
        ConfigChange, ConfigChangeDetector, GitMetadataChange, GitMetadataChangeDetector,
        GitWorktreeChange, GitWorktreeChangeDetector, MarkdownChange, MarkdownChangeDetector,
    },
    progress::{
        BuildTestExecution, BuildTestExecutionCompletion, BuildTestFreshness,
        BuildTestFreshnessBaseline, BuildTestInputChange, BuildTestKind, BuildTestState,
        cargo_build_test_command, is_cargo_project,
    },
    project::{collect_activity_state, collect_markdown_state, try_collect_project_snapshot},
};

use crate::{
    app::{ActivityState, App, RefreshSource},
    ui,
};

const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(250);
const PROJECT_POLL_INTERVAL: Duration = Duration::from_secs(1);

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

fn sync_git_worktree_detector(
    project_root: Option<&Path>,
    app: &App,
    worktree_changes: &mut Option<GitWorktreeChangeDetector>,
) {
    match (project_root, app.activity()) {
        (Some(root), ActivityState::Available(_)) => match worktree_changes {
            Some(detector) => detector.sync(root),
            None => *worktree_changes = Some(GitWorktreeChangeDetector::new(root)),
        },
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

fn apply_pending_refreshes(
    root: &Path,
    app: &mut App,
    worktree_changes: &mut Option<GitWorktreeChangeDetector>,
    requests: &mut RefreshRequest,
) -> RefreshOutcome {
    let mut outcome = RefreshOutcome::default();

    if requests.markdown
        && let Ok((plan, tasks)) = collect_markdown_state(root)
    {
        app.apply_markdown_state(plan, tasks);
        requests.markdown = false;
        outcome.markdown = true;
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
}

impl BuildTestRuntime {
    fn clear_baseline(&mut self, kind: BuildTestKind) {
        match kind {
            BuildTestKind::Build => self.build_baseline = None,
            BuildTestKind::Test => self.test_baseline = None,
        }
    }

    fn capture_baseline(&mut self, root: Option<&Path>, kind: BuildTestKind) {
        let baseline = root.and_then(|root| BuildTestFreshnessBaseline::capture(root).ok());
        match kind {
            BuildTestKind::Build => self.build_baseline = baseline,
            BuildTestKind::Test => self.test_baseline = baseline,
        }
    }
}

fn initialize_build_test_availability(project_root: Option<&Path>, app: &mut App) {
    let state = if project_root.is_some_and(is_cargo_project) {
        BuildTestState::NotRun
    } else {
        BuildTestState::Unavailable
    };
    if app.build_test_state(BuildTestKind::Build) == &state
        && app.build_test_state(BuildTestKind::Test) == &state
    {
        return;
    }
    app.apply_build_test_state(BuildTestKind::Build, state.clone());
    app.apply_build_test_state(BuildTestKind::Test, state);
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
    let Some(spec) = cargo_build_test_command(root, kind) else {
        app.apply_build_test_state(kind, BuildTestState::Unavailable);
        return true;
    };

    runtime.active_baseline = BuildTestFreshnessBaseline::capture(root).ok();
    match BuildTestExecution::start(spec) {
        Ok(execution) => {
            app.apply_build_test_state(kind, BuildTestState::Running(execution.run().clone()));
            runtime.active = Some(execution);
        }
        Err(error) => {
            runtime.active_baseline = None;
            app.apply_build_test_state(kind, BuildTestState::ExecutionError(error));
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
) {
    match completion {
        BuildTestExecutionCompletion::Completed(result) => {
            let kind = result.kind();
            let mut result = result;
            if inputs_changed {
                result.mark_stale();
            }
            app.apply_build_test_state(kind, BuildTestState::Completed(result));
            runtime.capture_baseline(project_root, kind);
        }
        BuildTestExecutionCompletion::ExecutionError(error) => {
            let kind = error.kind();
            runtime.clear_baseline(kind);
            runtime.active_baseline = None;
            runtime.active_inputs_changed = false;
            app.apply_build_test_state(kind, BuildTestState::ExecutionError(error));
        }
    }
}

fn check_completed_build_test_freshness(
    project_root: &Path,
    app: &mut App,
    baseline: Option<&BuildTestFreshnessBaseline>,
    kind: BuildTestKind,
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
        baseline.check(project_root),
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
    ) | check_completed_build_test_freshness(
        project_root,
        app,
        runtime.test_baseline.as_ref(),
        BuildTestKind::Test,
    )
}
fn observe_active_build_test_inputs(project_root: Option<&Path>, runtime: &mut BuildTestRuntime) {
    if runtime.active_inputs_changed {
        return;
    }
    if let (Some(root), Some(baseline)) = (project_root, runtime.active_baseline.as_ref()) {
        runtime.active_inputs_changed =
            matches!(baseline.check(root), Ok(BuildTestInputChange::Changed));
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
    runtime.active = None;
    runtime.active_baseline = None;
    runtime.active_inputs_changed = false;
    apply_build_test_completion(project_root, app, runtime, completion, inputs_changed);
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
    app: &mut App,
) -> io::Result<()> {
    let session_start = Instant::now();
    let mut needs_render = true;
    let mut scheduler = PollScheduler::new(session_start, PROJECT_POLL_INTERVAL);
    let mut markdown_changes = project_root.map(MarkdownChangeDetector::new);
    let mut worktree_changes = new_git_worktree_detector(project_root, app);
    let mut metadata_changes = project_root.map(GitMetadataChangeDetector::new);
    let mut config_changes = project_root.map(ConfigChangeDetector::new);
    let mut requests = RefreshRequest::default();
    let mut build_test_runtime = BuildTestRuntime::default();
    initialize_build_test_availability(project_root, app);

    while app.is_running() {
        if needs_render {
            terminal.draw(|frame| ui::render(frame, app))?;
            needs_render = false;
        }

        if event::poll(EVENT_POLL_TIMEOUT)? {
            match event::read()? {
                Event::Key(key)
                    if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('r') =>
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
                        sync_git_worktree_detector(project_root, app, &mut worktree_changes);
                        if let Some(detector) = &mut metadata_changes {
                            detector.sync(root);
                        }
                        if let Some(detector) = &mut config_changes {
                            detector.sync(root);
                        }
                        requests.clear();
                        app.record_refresh(RefreshSource::Manual, session_start.elapsed());
                        app.set_refresh_pending(false);
                    }
                    needs_render = true;
                }
                Event::Key(key) if let Some(kind) = manual_build_test_kind(key) => {
                    needs_render |=
                        start_manual_build_test(project_root, app, &mut build_test_runtime, kind);
                }
                Event::Key(key) => {
                    app.handle_key(key);
                    needs_render = true;
                }
                Event::Resize(_, _) => needs_render = true,
                _ => {}
            }
        }

        needs_render |= poll_build_test_execution(project_root, app, &mut build_test_runtime);

        if scheduler.is_due(Instant::now()) {
            observe_active_build_test_inputs(project_root, &mut build_test_runtime);
            needs_render |= check_build_test_freshness(project_root, app, &build_test_runtime);
            collect_change_requests(
                project_root,
                &mut markdown_changes,
                &mut worktree_changes,
                &mut metadata_changes,
                &mut config_changes,
                &mut requests,
            );
            if let Some(root) = project_root {
                let outcome =
                    apply_pending_refreshes(root, app, &mut worktree_changes, &mut requests);
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
            BuildTestCommandSpec, BuildTestExecution, BuildTestExecutionCompletion,
            BuildTestExecutionError, BuildTestFreshness, BuildTestFreshnessBaseline, BuildTestKind,
            BuildTestOutcome, BuildTestResult, BuildTestRun, BuildTestState, PlanSummary,
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
    fn initializes_build_test_availability_from_the_project_root() {
        let cargo_root = temp_root();
        fs::write(cargo_root.join("Cargo.toml"), "[package]").unwrap();
        let mut cargo_app = App::new(ProjectSnapshot::unavailable());
        initialize_build_test_availability(Some(&cargo_root), &mut cargo_app);
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
        initialize_build_test_availability(Some(&non_cargo_root), &mut non_cargo_app);
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
        assert!(runtime.test_baseline.is_some());

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
        assert!(requests.markdown);
        assert!(!requests.git);
        assert_eq!(app.activity(), &activity);
        assert!(apply_refresh_status(
            &mut app,
            &requests,
            &outcome,
            Duration::from_secs(20)
        ));
        assert_eq!(app.refresh_status().last_source(), status.last_source());
        assert_eq!(app.refresh_status().last_update(), status.last_update());
        assert!(app.refresh_status().retry_pending());
        let _ = fs::remove_file(root);
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
}
