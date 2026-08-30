mod app;
mod event_loop;
mod terminal;
mod ui;

use std::{env, io};

use app::App;
use devscope::project::{ProjectSnapshot, collect_project_snapshot};
use terminal::TerminalSession;

fn main() -> io::Result<()> {
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
