//! Project-level snapshot collection independent of the TUI.

use std::{error::Error, fmt, path::Path};

use crate::{
    config::{ConfigError, load_project_config},
    progress::{
        ActivitySummary, GitActivityError, MarkdownProgressError, PlanSummary, TaskSummary,
        analyze_markdown_progress_with_exclusions, collect_git_activity,
    },
};

const RECENT_COMMIT_LIMIT: usize = 5;

#[derive(Debug)]
pub enum ProjectCollectionError {
    Config(ConfigError),
    Markdown(MarkdownProgressError),
}

impl fmt::Display for ProjectCollectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(formatter, "Config error: {error}"),
            Self::Markdown(error) => write!(formatter, "Plan collection error: {error}"),
        }
    }
}

impl Error for ProjectCollectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Markdown(error) => Some(error),
        }
    }
}

impl From<ConfigError> for ProjectCollectionError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<MarkdownProgressError> for ProjectCollectionError {
    fn from(error: MarkdownProgressError) -> Self {
        Self::Markdown(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanState {
    Available(PlanSummary),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityState {
    Available(ActivitySummary),
    NotRepository,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    Available(TaskSummary),
    Unavailable,
}

/// The independently collected project state consumed by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSnapshot {
    plan: PlanState,
    activity: ActivityState,
    tasks: TaskState,
}

impl ProjectSnapshot {
    pub const fn new(plan: PlanState, activity: ActivityState, tasks: TaskState) -> Self {
        Self {
            plan,
            activity,
            tasks,
        }
    }

    pub const fn unavailable() -> Self {
        Self::new(
            PlanState::Unavailable,
            ActivityState::Unavailable,
            TaskState::Unavailable,
        )
    }

    pub const fn plan(&self) -> PlanState {
        self.plan
    }

    pub fn activity(&self) -> &ActivityState {
        &self.activity
    }

    pub fn tasks(&self) -> &TaskState {
        &self.tasks
    }

    pub fn into_parts(self) -> (PlanState, ActivityState, TaskState) {
        (self.plan, self.activity, self.tasks)
    }
}

/// Collects the Config-aware Markdown-derived plan and task state in one analysis pass.
pub fn collect_markdown_state(
    root: &Path,
) -> Result<(PlanState, TaskState), ProjectCollectionError> {
    let config = load_project_config(root)?;
    let progress = analyze_markdown_progress_with_exclusions(root, config.plan().excludes())?;
    Ok((
        PlanState::Available(PlanSummary::from(&progress)),
        TaskState::Available(TaskSummary::from(&progress)),
    ))
}

/// Collects the Git-derived activity state. A non-Git directory is a valid result.
pub fn collect_activity_state(root: &Path) -> Result<ActivityState, GitActivityError> {
    match collect_git_activity(root, RECENT_COMMIT_LIMIT) {
        Ok(activity) => Ok(ActivityState::Available(ActivitySummary::from(&activity))),
        Err(GitActivityError::NotRepository) => Ok(ActivityState::NotRepository),
        Err(error) => Err(error),
    }
}

/// Collects the current Config-aware Markdown and Git state for a fixed project root.
pub fn try_collect_project_snapshot(
    root: &Path,
) -> Result<ProjectSnapshot, ProjectCollectionError> {
    let (plan, tasks) = collect_markdown_state(root)?;
    let activity = collect_activity_state(root).unwrap_or(ActivityState::Unavailable);
    Ok(ProjectSnapshot::new(plan, activity, tasks))
}

/// Collects a snapshot for UI refreshes, retaining the existing unavailable fallback
/// for non-Config collection failures.
pub fn collect_project_snapshot(root: &Path) -> ProjectSnapshot {
    try_collect_project_snapshot(root).unwrap_or_else(|_| {
        let activity = collect_activity_state(root).unwrap_or(ActivityState::Unavailable);
        ProjectSnapshot::new(PlanState::Unavailable, activity, TaskState::Unavailable)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    static ID: AtomicUsize = AtomicUsize::new(0);

    struct TempProject {
        path: PathBuf,
    }

    impl TempProject {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "devscope-project-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn collects_plan_and_tasks_from_one_snapshot() {
        let project = TempProject::new();
        project.write("tasks.md", "- [x] Completed\n- [ ] Remaining");

        let snapshot = try_collect_project_snapshot(project.path()).unwrap();
        assert_eq!(
            snapshot.plan(),
            PlanState::Available(PlanSummary::new(1, 2))
        );
        let TaskState::Available(tasks) = snapshot.tasks() else {
            panic!("tasks should be available");
        };
        assert_eq!(tasks.total(), 2);
        assert_eq!(tasks.remaining(), 1);
        assert_eq!(tasks.items()[0].text(), "Remaining");
    }

    #[test]
    fn missing_config_preserves_markdown_collection() {
        let project = TempProject::new();
        project.write("tasks.md", "- [x] Completed\n- [ ] Remaining");

        let (plan, tasks) = collect_markdown_state(project.path()).unwrap();
        assert_eq!(plan, PlanState::Available(PlanSummary::new(1, 2)));
        let TaskState::Available(tasks) = tasks else {
            panic!("tasks should be available");
        };
        assert_eq!(tasks.remaining(), 1);
    }

    #[test]
    fn applies_configured_plan_excludes_without_affecting_other_files() {
        let project = TempProject::new();
        project.write("root.md", "- [ ] root");
        project.write("translations/ja.md", "- [ ] translated");
        project.write(
            ".devscope/config.toml",
            "[plan]\nexclude = [\"translations\"]\n",
        );

        let (plan, tasks) = collect_markdown_state(project.path()).unwrap();
        assert_eq!(plan, PlanState::Available(PlanSummary::new(0, 1)));
        let TaskState::Available(tasks) = tasks else {
            panic!("tasks should be available");
        };
        assert_eq!(tasks.items()[0].text(), "root");
    }
    #[test]
    fn reports_config_errors_explicitly() {
        let project = TempProject::new();
        project.write(".devscope/config.toml", "[plan");

        assert!(matches!(
            collect_markdown_state(project.path()),
            Err(ProjectCollectionError::Config(_))
        ));
    }

    #[test]
    fn reports_non_git_projects_without_losing_markdown() {
        let project = TempProject::new();
        project.write("tasks.md", "- [ ] Remaining");

        let snapshot = try_collect_project_snapshot(project.path()).unwrap();
        assert_eq!(snapshot.activity(), &ActivityState::NotRepository);
        assert_eq!(
            snapshot.plan(),
            PlanState::Available(PlanSummary::new(0, 1))
        );
    }

    #[test]
    fn collects_non_git_activity_as_a_valid_state() {
        let project = TempProject::new();

        assert_eq!(
            collect_activity_state(project.path()).unwrap(),
            ActivityState::NotRepository
        );
    }
}
