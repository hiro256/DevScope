//! DevScope command-line entry point.

mod app;
mod event_loop;
mod terminal;
mod ui;

use std::io;

use app::App;
use terminal::TerminalSession;

fn main() -> io::Result<()> {
    let mut terminal = TerminalSession::enter()?;
    let mut app = App::new();

    let run_result = event_loop::run(terminal.terminal_mut(), &mut app);
    let restore_result = terminal.restore();
    run_result.and(restore_result)
}
