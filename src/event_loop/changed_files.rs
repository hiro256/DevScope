//! Session-local Changed Files presentation; Git observations remain unchanged.
use crate::app::{ActivityState, App, ChangedFileEmphasisPhase};
use devscope::progress::GitChangedFile;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) struct ChangedFilesRuntime {
    previous: BTreeMap<PathBuf, GitChangedFile>,
    changed_at: BTreeMap<PathBuf, Instant>,
}

impl ChangedFilesRuntime {
    pub(super) fn new(activity: &ActivityState) -> Self {
        Self {
            previous: Self::entries(activity),
            changed_at: BTreeMap::new(),
        }
    }

    fn entries(activity: &ActivityState) -> BTreeMap<PathBuf, GitChangedFile> {
        match activity {
            ActivityState::Available(summary) => summary
                .changed_file_items()
                .iter()
                .map(|file| (file.path.clone(), file.clone()))
                .collect(),
            ActivityState::Unavailable | ActivityState::NotRepository => BTreeMap::new(),
        }
    }

    // Called after the existing Activity/snapshot application, never from rendering.
    pub(super) fn observe(&mut self, app: &mut App, now: Instant) -> bool {
        let current = Self::entries(app.activity());
        let mut changed = false;
        self.changed_at.retain(|path, _| {
            if current.contains_key(path) {
                return true;
            }
            app.set_changed_file_emphasis(path.clone(), ChangedFileEmphasisPhase::None);
            changed = true;
            false
        });
        for (path, file) in &current {
            if self.previous.get(path) != Some(file) {
                self.changed_at.insert(path.clone(), now);
                app.set_changed_file_emphasis(path.clone(), ChangedFileEmphasisPhase::Hot);
                changed = true;
            }
        }
        self.previous = current;
        changed
    }

    pub(super) fn advance(&mut self, app: &mut App, now: Instant) -> bool {
        let mut redraw = false;
        self.changed_at.retain(|path, started| {
            let elapsed = now.saturating_duration_since(*started);
            let phase = if elapsed < Duration::from_millis(750) {
                ChangedFileEmphasisPhase::Hot
            } else if elapsed < Duration::from_millis(1500) {
                ChangedFileEmphasisPhase::Warm
            } else if elapsed < Duration::from_millis(2250) {
                ChangedFileEmphasisPhase::Settling
            } else if elapsed < Duration::from_secs(3) {
                ChangedFileEmphasisPhase::Cooling
            } else {
                ChangedFileEmphasisPhase::None
            };
            let previous = app.changed_file_emphasis(path);
            if previous != phase {
                app.set_changed_file_emphasis(path.clone(), phase);
                // Warm and Settling have the same visible styling.
                redraw |= !matches!(
                    (previous, phase),
                    (
                        ChangedFileEmphasisPhase::Warm,
                        ChangedFileEmphasisPhase::Settling
                    )
                );
            }
            phase != ChangedFileEmphasisPhase::None
        });
        redraw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PlanState, TaskState};
    use devscope::{
        progress::{ActivitySummary, GitActivity, GitChangeCounts, GitFileStatus},
        project::ProjectSnapshot,
    };

    fn file(path: &str, additions: u64) -> GitChangedFile {
        GitChangedFile {
            path: path.into(),
            status: GitFileStatus::Modified,
            changes: GitChangeCounts {
                additions: Some(additions),
                deletions: Some(0),
            },
        }
    }
    fn state(files: Vec<GitChangedFile>) -> ActivityState {
        ActivityState::Available(ActivitySummary::from(&GitActivity {
            changed_files: files,
            recent_commits: vec![],
        }))
    }
    fn setup(files: Vec<GitChangedFile>) -> (App, ChangedFilesRuntime) {
        let app = App::new(ProjectSnapshot::new(
            PlanState::Unavailable,
            state(files),
            TaskState::Unavailable,
        ));
        let runtime = ChangedFilesRuntime::new(app.activity());
        (app, runtime)
    }
    #[test]
    fn baseline_reorder_removal_and_unavailable_are_quiet_or_clear() {
        let (mut app, mut runtime) = setup(vec![file("a", 1), file("b", 2)]);
        let now = Instant::now();
        assert!(!runtime.observe(&mut app, now));
        app.apply_activity_state(state(vec![file("b", 2), file("a", 1)]));
        assert!(!runtime.observe(&mut app, now));
        app.apply_activity_state(state(vec![file("b", 3), file("new", 1)]));
        assert!(runtime.observe(&mut app, now));
        assert_eq!(
            app.changed_file_emphasis(std::path::Path::new("b")),
            ChangedFileEmphasisPhase::Hot
        );
        app.apply_activity_state(state(vec![file("b", 3)]));
        runtime.observe(&mut app, now);
        assert_eq!(
            app.changed_file_emphasis(std::path::Path::new("new")),
            ChangedFileEmphasisPhase::None
        );
        for unavailable in [ActivityState::Unavailable, ActivityState::NotRepository] {
            app.apply_activity_state(unavailable);
            runtime.observe(&mut app, now);
            assert!(runtime.changed_at.is_empty());
            assert!(runtime.previous.is_empty());
            assert_eq!(
                app.changed_file_emphasis(std::path::Path::new("b")),
                ChangedFileEmphasisPhase::None
            );
        }
    }
    #[test]
    fn independent_boundaries_unchanged_refresh_and_restart() {
        use ChangedFileEmphasisPhase::*;
        let (mut app, mut runtime) = setup(vec![file("a", 1)]);
        let now = Instant::now();
        app.apply_activity_state(state(vec![file("a", 2), file("b", 1)]));
        runtime.observe(&mut app, now);
        for (ms, phase, redraw) in [
            (749, Hot, false),
            (750, Warm, true),
            (1499, Warm, false),
            (1500, Settling, false),
            (2250, Cooling, true),
        ] {
            assert_eq!(
                runtime.advance(&mut app, now + Duration::from_millis(ms)),
                redraw
            );
            assert_eq!(app.changed_file_emphasis(std::path::Path::new("a")), phase);
        }
        assert!(!runtime.observe(&mut app, now + Duration::from_millis(2300)));
        let mut a = file("a", 2);
        a.status = GitFileStatus::Added;
        app.apply_activity_state(state(vec![a, file("b", 1)]));
        runtime.observe(&mut app, now + Duration::from_millis(2500));
        runtime.advance(&mut app, now + Duration::from_millis(3000));
        assert_eq!(app.changed_file_emphasis(std::path::Path::new("a")), Hot);
        assert_eq!(app.changed_file_emphasis(std::path::Path::new("b")), None);
        runtime.advance(&mut app, now + Duration::from_millis(5500));
        assert!(runtime.changed_at.is_empty());
        assert!(!runtime.observe(&mut app, now + Duration::from_secs(6)));
    }
}
