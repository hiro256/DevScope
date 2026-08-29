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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivitySummary {
    changed_files: usize,
    recent_commits: Vec<GitCommit>,
}
impl ActivitySummary {
    pub fn changed_files(&self) -> usize {
        self.changed_files
    }
    pub fn recent_commits(&self) -> &[GitCommit] {
        &self.recent_commits
    }
}
impl From<&GitActivity> for ActivitySummary {
    fn from(v: &GitActivity) -> Self {
        Self {
            changed_files: v.changed_file_count(),
            recent_commits: v.recent_commits.clone(),
        }
    }
}
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
    fn from(p: &MarkdownProgress) -> Self {
        Self::new(p.completed_tasks(), p.total_tasks())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummaryItem {
    path: std::path::PathBuf,
    line: usize,
    text: String,
}
impl TaskSummaryItem {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub const fn line(&self) -> usize {
        self.line
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    total: usize,
    items: Vec<TaskSummaryItem>,
}
impl TaskSummary {
    pub fn total(&self) -> usize {
        self.total
    }
    pub fn items(&self) -> &[TaskSummaryItem] {
        &self.items
    }
    pub fn remaining(&self) -> usize {
        self.items.len()
    }
}
impl From<&MarkdownProgress> for TaskSummary {
    fn from(p: &MarkdownProgress) -> Self {
        Self {
            total: p.total_tasks(),
            items: p
                .tasks()
                .iter()
                .filter(|t| !t.completed())
                .map(|t| TaskSummaryItem {
                    path: t.path().into(),
                    line: t.line(),
                    text: t.text().into(),
                })
                .collect(),
        }
    }
}

impl TaskSummaryItem {
    pub fn new(path: std::path::PathBuf, line: usize, text: String) -> Self {
        Self { path, line, text }
    }
}
impl TaskSummary {
    pub fn new(total: usize, items: Vec<TaskSummaryItem>) -> Self {
        Self { total, items }
    }
}
#[cfg(test)]
mod task_summary_tests {
    use super::*;
    #[test]
    fn keeps_incomplete_task_details() {
        let item = TaskSummaryItem::new("docs/roadmap.md".into(), 10, "Task summary".into());
        let summary = TaskSummary::new(2, vec![item]);
        assert_eq!(summary.total(), 2);
        assert_eq!(summary.remaining(), 1);
        assert_eq!(
            summary.items()[0].path(),
            std::path::Path::new("docs/roadmap.md")
        );
        assert_eq!(summary.items()[0].line(), 10);
        assert_eq!(summary.items()[0].text(), "Task summary");
    }
}
