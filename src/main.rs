mod app;
mod cli;
mod event_loop;
mod terminal;
mod ui;

use std::{env, io, process::ExitCode};

use app::App;
use cli::EntryMode;
use devscope::project::{ProjectSnapshot, collect_project_snapshot};
use terminal::TerminalSession;

fn main() -> ExitCode {
    match cli::parse_args(env::args_os().skip(1)) {
        Ok(EntryMode::Tui) => run_tui().map_or_else(report_runtime_error, |_| ExitCode::SUCCESS),
        Ok(EntryMode::Context) => run_cli(cli::render_context),
        Ok(EntryMode::TaskList) => run_cli(cli::render_task_list),
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

fn run_cli(render: fn(&std::path::Path, &ProjectSnapshot) -> String) -> ExitCode {
    match env::current_dir() {
        Ok(root) => {
            let snapshot = collect_project_snapshot(&root);
            print!("{}", render(&root, &snapshot));
            ExitCode::SUCCESS
        }
        Err(error) => report_runtime_error(error),
    }
}

fn run_tui() -> io::Result<()> {
    let project_root = env::current_dir().ok();
    let snapshot = project_root
        .as_deref()
        .map(collect_project_snapshot)
        .unwrap_or_else(ProjectSnapshot::unavailable);

    let mut terminal = TerminalSession::enter()?;
    let mut app = App::new(snapshot);
    event_loop::run(terminal.terminal_mut(), project_root.as_deref(), &mut app)
        .and(terminal.restore())
}

fn report_runtime_error(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
