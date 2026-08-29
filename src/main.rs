mod app;
mod event_loop;
mod terminal;
mod ui;
use app::{ActivityState, App, PlanState};
use devscope::progress::{
    ActivitySummary, GitActivityError, PlanSummary, analyze_markdown_progress, collect_git_activity,
};
use std::{env, io};
use terminal::TerminalSession;
const RECENT_COMMIT_LIMIT: usize = 5;
fn main() -> io::Result<()> {
    let root = env::current_dir().ok();
    let plan = root
        .as_deref()
        .and_then(|p| analyze_markdown_progress(p).ok())
        .map(|p| PlanState::Available(PlanSummary::from(&p)))
        .unwrap_or(PlanState::Unavailable);
    let activity = match root
        .as_deref()
        .map(|p| collect_git_activity(p, RECENT_COMMIT_LIMIT))
    {
        Some(Ok(a)) => ActivityState::Available(ActivitySummary::from(&a)),
        Some(Err(GitActivityError::NotRepository)) => ActivityState::NotRepository,
        _ => ActivityState::Unavailable,
    };
    let mut terminal = TerminalSession::enter()?;
    let mut app = App::new(plan, activity);
    let r = event_loop::run(terminal.terminal_mut(), &mut app);
    r.and(terminal.restore())
}
