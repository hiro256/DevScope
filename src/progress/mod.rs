//! Project progress analysis independent of the TUI.

mod git;
mod markdown;

pub use git::{
    GitActivity, GitActivityError, GitChangedFile, GitCommit, GitFileStatus, collect_git_activity,
    is_git_repository,
};
pub use markdown::{
    MarkdownProgress, MarkdownProgressError, MarkdownTask, analyze_markdown_progress,
    discover_markdown_files,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanSummary {
    completed: usize,
    total: usize,
}
impl PlanSummary {
    pub const fn new(completed: usize, total: usize) -> Self {
        Self { completed, total }
    }
    pub const fn completed(&self) -> usize {
        self.completed
    }
    pub const fn total(&self) -> usize {
        self.total
    }
}
impl From<&MarkdownProgress> for PlanSummary {
    fn from(progress: &MarkdownProgress) -> Self {
        Self::new(progress.completed_tasks(), progress.total_tasks())
    }
}
