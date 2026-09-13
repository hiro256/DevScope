mod app;
mod cli;
mod event_loop;
mod terminal;
mod ui;

use std::{env, io, process::ExitCode};

use app::{App, CurrentWorkState};
use cli::{CurrentWorkContext, EntryMode};
use devscope::{
    config::load_project_config,
    current_work::{load_current_work, mark_current_work_done},
    progress::{
        BuildTestExecutionCompletion, BuildTestKind, BuildTestState, cargo_build_test_command,
        evaluate_completed_build_test_freshness, load_build_test_states, run_build_test,
        save_build_test_state,
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
        Ok(EntryMode::WorkDone(number)) => run_work_done(number),
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
            print!("{}", cli::render_context(&root, &snapshot, current_work));
            ExitCode::SUCCESS
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
fn run_work_done(number: usize) -> ExitCode {
    match env::current_dir() {
        Ok(root) => match mark_current_work_done(&root, number) {
            Ok(result) => {
                print!("{}", cli::render_work_done(&result));
                ExitCode::SUCCESS
            }
            Err(error) => report_runtime_error(error),
        },
        Err(error) => report_runtime_error(error),
    }
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
    let Some(spec) = cargo_build_test_command(&root, kind) else {
        eprintln!("error: Cargo Build/Test is unavailable for this project");
        return ExitCode::FAILURE;
    };
    let baseline = devscope::progress::BuildTestFreshnessBaseline::capture(&root).ok();
    let mut completion = run_build_test(spec);
    let persisted_baseline = match &mut completion {
        BuildTestExecutionCompletion::Completed(result) => {
            let (freshness, baseline) =
                evaluate_completed_build_test_freshness(&root, baseline.as_ref(), false);
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

    let mut terminal = TerminalSession::enter()?;
    let mut app = App::new(snapshot);
    restore_tui_build_test_states(project_root.as_deref(), &mut app);
    app.apply_current_work(load_tui_current_work(project_root.as_deref()));
    event_loop::run(terminal.terminal_mut(), project_root.as_deref(), &mut app)
        .and(terminal.restore())
}

fn restore_tui_build_test_states(root: Option<&std::path::Path>, app: &mut App) {
    let Some(root) = root else { return };
    for mut stored in load_build_test_states(root) {
        stored.restore_freshness(root);
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
fn report_runtime_error(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
