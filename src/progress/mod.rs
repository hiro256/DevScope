//! Project progress analysis independent of the TUI.
mod build_test;
mod build_test_source;
pub use build_test::{
    BuildTestDiagnostic, BuildTestExecutionError, BuildTestFreshness, BuildTestKind,
    BuildTestOutcome, BuildTestResult, BuildTestRun, BuildTestState, BuildTestStatus,
    MAX_DIAGNOSTIC_CHARS,
};
pub use build_test_source::BuildTestCommandSpec;
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
    changed_files: Vec<GitChangedFile>,
    recent_commits: Vec<GitCommit>,
}
impl ActivitySummary {
    pub fn changed_files(&self) -> usize {
        self.changed_files.len()
    }
    pub fn changed_file_items(&self) -> &[GitChangedFile] {
        &self.changed_files
    }
    pub fn recent_commits(&self) -> &[GitCommit] {
        &self.recent_commits
    }
}
impl From<&GitActivity> for ActivitySummary {
    fn from(v: &GitActivity) -> Self {
        Self {
            changed_files: v.changed_files.clone(),
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
#[cfg(test)]
mod markdown_conversion_tests {
    use super::*;
    use std::fs;
    #[test]
    fn converts_markdown_progress_to_incomplete_task_summary() {
        let root =
            std::env::temp_dir().join(format!("devscope-task-summary-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("tasks.md");
        fs::write(&path, "- [x] completed task\n- [ ] incomplete task").unwrap();
        let progress = analyze_markdown_progress(&root).unwrap();
        let summary = TaskSummary::from(&progress);
        assert_eq!(summary.total(), 2);
        assert_eq!(summary.remaining(), 1);
        assert_eq!(summary.items()[0].text(), "incomplete task");
        assert_eq!(summary.items()[0].line(), 2);
        assert_eq!(summary.items()[0].path(), path);
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod activity_summary_tests {
    use super::*;

    #[test]
    fn keeps_changed_file_details_and_recent_commits() {
        let activity = GitActivity {
            changed_files: vec![
                GitChangedFile {
                    path: "src/a.rs".into(),
                    status: GitFileStatus::Modified,
                },
                GitChangedFile {
                    path: "src/b.rs".into(),
                    status: GitFileStatus::Added,
                },
            ],
            recent_commits: vec![GitCommit {
                id: "abc123".into(),
                summary: "Keep activity details".into(),
            }],
        };

        let summary = ActivitySummary::from(&activity);

        assert_eq!(summary.changed_files(), 2);
        assert_eq!(
            summary.changed_file_items(),
            activity.changed_files.as_slice()
        );
        assert_eq!(
            summary.changed_file_items()[0].path,
            std::path::Path::new("src/a.rs")
        );
        assert_eq!(
            summary.changed_file_items()[0].status,
            GitFileStatus::Modified
        );
        assert_eq!(
            summary.changed_file_items()[1].path,
            std::path::Path::new("src/b.rs")
        );
        assert_eq!(summary.changed_file_items()[1].status, GitFileStatus::Added);
        assert_eq!(summary.recent_commits(), activity.recent_commits.as_slice());
    }
}
