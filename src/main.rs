mod app;
mod event_loop;
mod terminal;
mod ui;
use app::{ActivityState, App, PlanState, TaskState};
use devscope::progress::{
    ActivitySummary, GitActivityError, PlanSummary, TaskSummary, analyze_markdown_progress,
    collect_git_activity,
};
use std::{env, io};
use terminal::TerminalSession;
const RECENT_COMMIT_LIMIT: usize = 5;
fn main() -> io::Result<()> {
    let root = env::current_dir().ok();
    let markdown = root
        .as_deref()
        .and_then(|p| analyze_markdown_progress(p).ok());
    let plan = markdown
        .as_ref()
        .map(|p| PlanState::Available(PlanSummary::from(p)))
        .unwrap_or(PlanState::Unavailable);
    let tasks = markdown
        .as_ref()
        .map(|p| TaskState::Available(TaskSummary::from(p)))
        .unwrap_or(TaskState::Unavailable);
    let activity = match root
        .as_deref()
        .map(|p| collect_git_activity(p, RECENT_COMMIT_LIMIT))
    {
        Some(Ok(a)) => ActivityState::Available(ActivitySummary::from(&a)),
        Some(Err(GitActivityError::NotRepository)) => ActivityState::NotRepository,
        _ => ActivityState::Unavailable,
    };
    let mut term = TerminalSession::enter()?;
    let mut app = App::new(plan, activity, tasks);
    event_loop::run(term.terminal_mut(), &mut app).and(term.restore())
}
