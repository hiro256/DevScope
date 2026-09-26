mod app;
mod cli;
mod event_loop;
mod file_browser;
mod terminal;
mod ui;

use std::{env, io, process::ExitCode};

use app::{App, CurrentWorkState};
use cli::{CurrentWorkContext, EntryMode};
use devscope::{
    config::{ConfigError, ProjectConfig, load_project_config},
    current_work::{
        CurrentWorkError, clear_current_work_active, load_current_work, mark_current_work_done,
        set_current_work_active,
    },
    current_work_history::{
        read_current_work_history, record_active_update, record_completion,
        render_current_work_history,
    },
    progress::{
        ArtifactObservation, BuildTestExecutionCompletion, BuildTestKind, BuildTestState,
        evaluate_completed_build_test_freshness_with_exclusions, is_git_repository,
        load_build_test_states, observe_artifact, propose_activity_exclusions,
        resolve_build_test_command, run_build_test, save_build_test_state,
    },
    project::{ProjectSnapshot, try_collect_project_snapshot},
};
use terminal::TerminalSession;

fn main() -> ExitCode {
    match cli::parse_args(env::args_os().skip(1)) {
        Ok(EntryMode::Tui) => run_tui().map_or_else(report_runtime_error, |_| ExitCode::SUCCESS),
        Ok(EntryMode::Context) => run_context(),
        Ok(EntryMode::TaskList) => run_task_list(),
        Ok(EntryMode::WorkList) => run_work_list(),
        Ok(EntryMode::WorkHistory) => run_work_history(),
        Ok(EntryMode::WorkDone(number)) => run_work_done(number),
        Ok(EntryMode::WorkActive(number)) => run_work_active(number),
        Ok(EntryMode::WorkActiveClear) => run_work_active_clear(),
        Ok(EntryMode::ActivitySuggestExcludes) => run_activity_suggest_excludes(),
        Ok(EntryMode::Verify(kind)) => run_verify(kind),
        Ok(EntryMode::ArtifactInspect(path)) => run_artifact_inspect(path),
        Ok(EntryMode::Help) => {
            print!("{}", cli::usage());
            ExitCode::SUCCESS
        }
        Ok(EntryMode::Version) => {
            println!("devscope {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}\n\n{}", cli::usage());
            ExitCode::from(2)
        }
    }
}

fn run_context() -> ExitCode {
    match env::current_dir() {
        Ok(root) => {
            let snapshot = match try_collect_project_snapshot(&root) {
                Ok(snapshot) => snapshot,
                Err(error) => return report_runtime_error(error),
            };
            let loaded_current_work = load_current_work(&root);
            let current_work = match &loaded_current_work {
                Ok(Some(work)) => CurrentWorkContext::Available(work),
                Ok(None) => CurrentWorkContext::NotSet,
                Err(_) => CurrentWorkContext::Unavailable,
            };
            match cli::render_context(&root, &snapshot, current_work) {
                Ok(output) => {
                    print!("{output}");
                    ExitCode::SUCCESS
                }
                Err(error) => report_runtime_error(error),
            }
        }
        Err(error) => report_runtime_error(error),
    }
}

fn run_task_list() -> ExitCode {
    match env::current_dir() {
        Ok(root) => match cli::collect_task_list_state(&root) {
            Ok(tasks) => {
                print!("{}", cli::render_task_list(&root, &tasks));
                ExitCode::SUCCESS
            }
            Err(error) => report_runtime_error(error),
        },
        Err(error) => report_runtime_error(error),
    }
}

fn run_work_list() -> ExitCode {
    match env::current_dir() {
        Ok(root) => match load_current_work(&root) {
            Ok(Some(work)) => {
                print!("{}", cli::render_work_list(&work));
                ExitCode::SUCCESS
            }
            Ok(None) => {
                print!("{}", cli::render_current_work_not_set());
                ExitCode::SUCCESS
            }
            Err(error) => report_runtime_error(error),
        },
        Err(error) => report_runtime_error(error),
    }
}
fn run_work_history() -> ExitCode {
    match env::current_dir() {
        Ok(root) => match read_current_work_history(&root) {
            Ok(events) => {
                print!("{}", render_current_work_history(&events));
                ExitCode::SUCCESS
            }
            Err(error) => report_runtime_error(error),
        },
        Err(error) => report_runtime_error(error),
    }
}

fn run_work_done(number: usize) -> ExitCode {
    match env::current_dir() {
        Ok(root) => match load_current_work(&root) {
            Ok(Some(before)) => match mark_current_work_done(&root, number) {
                Ok(result) => {
                    print!("{}", cli::render_work_done(&result));
                    if let Err(error) = record_completion(&root, &before, &result) {
                        eprintln!("warning: could not append Current Work history: {error}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => report_runtime_error(error),
            },
            Ok(None) => report_runtime_error(CurrentWorkError::NotSet),
            Err(error) => report_runtime_error(error),
        },
        Err(error) => report_runtime_error(error),
    }
}

fn run_work_active(number: usize) -> ExitCode {
    match env::current_dir() {
        Ok(root) => match load_current_work(&root) {
            Ok(Some(before)) => match set_current_work_active(&root, number) {
                Ok(result) => {
                    print!("{}", cli::render_work_active(&result));
                    if let Err(error) = record_active_update(&root, &before, &result) {
                        eprintln!("warning: could not append Current Work history: {error}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => report_runtime_error(error),
            },
            Ok(None) => report_runtime_error(CurrentWorkError::NotSet),
            Err(error) => report_runtime_error(error),
        },
        Err(error) => report_runtime_error(error),
    }
}

fn run_work_active_clear() -> ExitCode {
    match env::current_dir() {
        Ok(root) => match load_current_work(&root) {
            Ok(Some(before)) => match clear_current_work_active(&root) {
                Ok(result) => {
                    print!("{}", cli::render_work_active(&result));
                    if let Err(error) = record_active_update(&root, &before, &result) {
                        eprintln!("warning: could not append Current Work history: {error}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => report_runtime_error(error),
            },
            Ok(None) => report_runtime_error(CurrentWorkError::NotSet),
            Err(error) => report_runtime_error(error),
        },
        Err(error) => report_runtime_error(error),
    }
}
fn run_activity_suggest_excludes() -> ExitCode {
    const PROPOSAL_LIMIT: usize = 3;

    let Ok(root) = env::current_dir() else {
        return ExitCode::FAILURE;
    };
    let config = match load_project_config(&root) {
        Ok(config) => config,
        Err(error) => return report_runtime_error(error),
    };
    match is_git_repository(&root) {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("error: Activity exclusion proposals unavailable: not a Git repository");
            return ExitCode::FAILURE;
        }
        Err(error) => return report_runtime_error(error),
    }
    let diagnostics = match devscope::change::diagnose_worktree_scan_with_exclusions(
        &root,
        config.activity().excludes(),
    ) {
        Ok(diagnostics) => diagnostics,
        Err(error) => return report_runtime_error(format!("{error:?}")),
    };
    let proposals = propose_activity_exclusions(
        &root,
        &diagnostics,
        config.activity().excludes(),
        PROPOSAL_LIMIT,
    )
    .into_iter()
    .take(PROPOSAL_LIMIT)
    .collect::<Vec<_>>();
    print!("{}", cli::render_activity_exclusion_proposals(&proposals));
    ExitCode::SUCCESS
}

fn run_artifact_inspect(path: Option<std::ffi::OsString>) -> ExitCode {
    let Ok(root) = env::current_dir() else {
        return ExitCode::FAILURE;
    };
    let path = match path {
        Some(path) => std::path::PathBuf::from(path),
        None => match load_project_config(&root) {
            Ok(config) => match config.artifact().path() {
                Some(path) => path.to_path_buf(),
                None => {
                    eprintln!("error: no Artifact target is configured");
                    return ExitCode::FAILURE;
                }
            },
            Err(error) => return report_runtime_error(error),
        },
    };
    match devscope::progress::observe_artifact(&root, &path) {
        Ok(observation) => {
            print!("{}", cli::render_artifact(&observation));
            if observation.observation_failed() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => report_runtime_error(error),
    }
}
fn run_verify(kind: BuildTestKind) -> ExitCode {
    let Ok(root) = env::current_dir() else {
        return ExitCode::FAILURE;
    };
    let config = match load_project_config(&root) {
        Ok(config) => config,
        Err(error) => return report_runtime_error(error),
    };
    let exclusions = config.verify().excludes();
    let Some(spec) = resolve_build_test_command(&root, &config, kind) else {
        eprintln!("error: Build/Test verification is unavailable for this project");
        return ExitCode::FAILURE;
    };
    let baseline =
        devscope::progress::BuildTestFreshnessBaseline::capture_with_exclusions(&root, exclusions)
            .ok();
    let mut completion = run_build_test(spec);
    let persisted_baseline = match &mut completion {
        BuildTestExecutionCompletion::Completed(result) => {
            let (freshness, baseline) = evaluate_completed_build_test_freshness_with_exclusions(
                &root,
                exclusions,
                baseline.as_ref(),
                false,
            );
            if matches!(freshness, devscope::progress::BuildTestFreshness::Stale) {
                result.mark_stale();
            }
            baseline
        }
        BuildTestExecutionCompletion::ExecutionError(_) => None,
    };
    let state = match &completion {
        BuildTestExecutionCompletion::Completed(result) => {
            BuildTestState::Completed(result.clone())
        }
        BuildTestExecutionCompletion::ExecutionError(error) => {
            BuildTestState::ExecutionError(error.clone())
        }
    };
    if let Err(error) = save_build_test_state(&root, kind, &state, persisted_baseline.as_ref()) {
        eprintln!("error: could not save observed Evidence: {error}");
        return ExitCode::FAILURE;
    }
    print!("{}", cli::render_verify(&completion));
    match completion {
        BuildTestExecutionCompletion::Completed(result)
            if result.outcome() == devscope::progress::BuildTestOutcome::Passed =>
        {
            ExitCode::SUCCESS
        }
        _ => ExitCode::FAILURE,
    }
}
fn run_tui() -> io::Result<()> {
    let project_root = env::current_dir().ok();
    let snapshot = match project_root.as_deref() {
        Some(root) => try_collect_project_snapshot(root).map_err(io::Error::other)?,
        None => ProjectSnapshot::unavailable(),
    };

    let config = match project_root.as_deref() {
        Some(root) => load_project_config(root).map_err(io::Error::other)?,
        None => ProjectConfig::default(),
    };
    let mut terminal = TerminalSession::enter()?;
    let mut app = App::new(snapshot);
    restore_tui_build_test_states(project_root.as_deref(), &config, &mut app);
    app.apply_artifact(load_tui_artifact(project_root.as_deref()).map_err(io::Error::other)?);
    app.apply_current_work(load_tui_current_work(project_root.as_deref()));
    event_loop::run(
        terminal.terminal_mut(),
        project_root.as_deref(),
        &config,
        &mut app,
    )
    .and(terminal.restore())
}

fn restore_tui_build_test_states(
    root: Option<&std::path::Path>,
    config: &ProjectConfig,
    app: &mut App,
) {
    let Some(root) = root else { return };
    for mut stored in load_build_test_states(root) {
        stored.restore_freshness_with_exclusions(root, config.verify().excludes());
        app.apply_build_test_state(stored.kind, stored.state);
    }
}
fn load_tui_current_work(root: Option<&std::path::Path>) -> CurrentWorkState {
    let Some(root) = root else {
        return CurrentWorkState::Unavailable;
    };
    match load_current_work(root) {
        Ok(Some(work)) => CurrentWorkState::Available(work),
        Ok(None) => CurrentWorkState::NotSet,
        Err(_) => CurrentWorkState::Unavailable,
    }
}
fn load_tui_artifact(
    root: Option<&std::path::Path>,
) -> Result<Option<ArtifactObservation>, ConfigError> {
    let Some(root) = root else {
        return Ok(None);
    };
    let config = load_project_config(root)?;
    let Some(path) = config.artifact().path() else {
        return Ok(None);
    };
    Ok(Some(observe_artifact(root, path).unwrap_or_else(|error| {
        ArtifactObservation::observation_error(path.to_path_buf(), error.to_string())
    })))
}
fn report_runtime_error(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
