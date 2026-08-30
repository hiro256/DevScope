//! Project-level snapshot collection independent of the TUI.

use std::path::Path;

use crate::progress::{
    ActivitySummary, GitActivityError, PlanSummary, TaskSummary, analyze_markdown_progress,
    collect_git_activity,
};

const RECENT_COMMIT_LIMIT: usize = 5;

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

/// Collects the current Markdown and Git state for a fixed project root.
pub fn collect_project_snapshot(root: &Path) -> ProjectSnapshot {
    let markdown = analyze_markdown_progress(root);
    let (plan, tasks) = match markdown {
        Ok(progress) => (
            PlanState::Available(PlanSummary::from(&progress)),
            TaskState::Available(TaskSummary::from(&progress)),
        ),
        Err(_) => (PlanState::Unavailable, TaskState::Unavailable),
    };

    let activity = match collect_git_activity(root, RECENT_COMMIT_LIMIT) {
        Ok(activity) => ActivityState::Available(ActivitySummary::from(&activity)),
        Err(GitActivityError::NotRepository) => ActivityState::NotRepository,
        Err(_) => ActivityState::Unavailable,
    };

    ProjectSnapshot::new(plan, activity, tasks)
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

        fn write_markdown(&self, contents: &str) {
            fs::write(self.path.join("tasks.md"), contents).unwrap();
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
        project.write_markdown("- [x] Completed\n- [ ] Remaining");

        let snapshot = collect_project_snapshot(project.path());
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
    fn reports_non_git_projects_without_losing_markdown() {
        let project = TempProject::new();
        project.write_markdown("- [ ] Remaining");

        let snapshot = collect_project_snapshot(project.path());
        assert_eq!(snapshot.activity(), &ActivityState::NotRepository);
        assert_eq!(
            snapshot.plan(),
            PlanState::Available(PlanSummary::new(0, 1))
        );
    }
}
