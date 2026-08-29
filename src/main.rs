//! DevScope command-line entry point.

mod app;
mod event_loop;
mod terminal;
mod ui;

use app::{App, PlanState};
use devscope::progress::{PlanSummary, analyze_markdown_progress};
use std::{env, io};
use terminal::TerminalSession;

fn main() -> io::Result<()> {
    let mut terminal = TerminalSession::enter()?;
    let mut app = App::new(load_plan_state());
    let run_result = event_loop::run(terminal.terminal_mut(), &mut app);
    let restore_result = terminal.restore();
    run_result.and(restore_result)
}

fn load_plan_state() -> PlanState {
    env::current_dir()
        .ok()
        .and_then(|root| analyze_markdown_progress(&root).ok())
        .map(|progress| PlanState::Available(PlanSummary::from(&progress)))
        .unwrap_or(PlanState::Unavailable)
}
