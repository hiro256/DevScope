//! Session-local cues for a newly prepended prefix, not history-change notifications.
use super::transient::transient_emphasis_phase;
use crate::app::{ActivityState, App, TransientEmphasisPhase};
use std::{collections::BTreeMap, time::Instant};

enum CommitBaseline {
    Unavailable,
    Available { head: Option<String> },
}

impl CommitBaseline {
    fn from_activity(activity: &ActivityState) -> Self {
        match activity {
            ActivityState::Available(summary) => Self::Available {
                head: summary
                    .recent_commits()
                    .first()
                    .map(|commit| commit.id.clone()),
            },
            ActivityState::Unavailable | ActivityState::NotRepository => Self::Unavailable,
        }
    }
}

pub(super) struct CommitsRuntime {
    baseline: CommitBaseline,
    added_at: BTreeMap<String, Instant>,
}

impl CommitsRuntime {
    pub(super) fn new(activity: &ActivityState) -> Self {
        Self {
            baseline: CommitBaseline::from_activity(activity),
            added_at: BTreeMap::new(),
        }
    }
    fn ids(activity: &ActivityState) -> Vec<String> {
        match activity {
            ActivityState::Available(summary) => summary
                .recent_commits()
                .iter()
                .map(|commit| commit.id.clone())
                .collect(),
            _ => vec![],
        }
    }
    pub(super) fn observe(&mut self, app: &mut App, now: Instant) -> bool {
        let ids = Self::ids(app.activity());
        let current_baseline = CommitBaseline::from_activity(app.activity());
        let prefix = match (&self.baseline, &current_baseline) {
            (CommitBaseline::Available { head: Some(head) }, CommitBaseline::Available { .. }) => {
                ids.iter().position(|id| id == head)
            }
            (CommitBaseline::Available { head: None }, CommitBaseline::Available { .. }) => {
                Some(ids.len())
            }
            // A recovered observation is not evidence that these commits are new.
            _ => None,
        };
        let mut redraw = false;
        self.added_at.retain(|id, _| {
            if prefix.is_some() && ids.contains(id) {
                return true;
            }
            app.set_commit_emphasis(id.clone(), TransientEmphasisPhase::None);
            redraw = true;
            false
        });
        if let Some(prefix) = prefix {
            for id in ids.iter().take(prefix) {
                self.added_at.insert(id.clone(), now);
                app.set_commit_emphasis(id.clone(), TransientEmphasisPhase::Hot);
                redraw = true;
            }
        }
        self.baseline = current_baseline;
        redraw
    }
    pub(super) fn advance(&mut self, app: &mut App, now: Instant) -> bool {
        use TransientEmphasisPhase::*;
        let mut redraw = false;
        self.added_at.retain(|id, start| {
            let elapsed = now.saturating_duration_since(*start);
            let phase = transient_emphasis_phase(elapsed);
            let previous = app.commit_emphasis(id);
            if previous != phase {
                app.set_commit_emphasis(id.clone(), phase);
                let appearance = |phase| match phase {
                    None => 0,
                    Hot => 2,
                    _ => 1,
                };
                redraw |= appearance(previous) != appearance(phase);
            }
            phase != None
        });
        redraw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{PlanState, TaskState};
    use devscope::{
        progress::{ActivitySummary, GitActivity, GitCommit},
        project::ProjectSnapshot,
    };
    use std::time::Duration;

    fn state(ids: &[&str]) -> ActivityState {
        ActivityState::Available(ActivitySummary::from(&GitActivity {
            changed_files: vec![],
            recent_commits: ids
                .iter()
                .map(|id| GitCommit {
                    id: (*id).into(),
                    summary: "日本語のコミット".into(),
                })
                .collect(),
        }))
    }
    fn setup(ids: &[&str]) -> (App, CommitsRuntime, Instant) {
        let app = App::new(ProjectSnapshot::new(
            PlanState::Unavailable,
            state(ids),
            TaskState::Unavailable,
        ));
        let runtime = CommitsRuntime::new(app.activity());
        (app, runtime, Instant::now())
    }
    #[test]
    fn available_history_recovers_from_unavailable_quietly() {
        let (mut app, mut runtime, now) = setup(&["C", "B", "A"]);
        app.apply_activity_state(ActivityState::Unavailable);
        runtime.observe(&mut app, now);
        app.apply_activity_state(state(&["C", "B", "A"]));
        assert!(!runtime.observe(&mut app, now));
        assert!(runtime.added_at.is_empty());
        for id in ["C", "B", "A"] {
            assert_eq!(app.commit_emphasis(id), TransientEmphasisPhase::None);
        }
    }
    #[test]
    fn available_history_recovers_from_not_repository_quietly() {
        let (mut app, mut runtime, now) = setup(&["C", "B", "A"]);
        app.apply_activity_state(state(&["D", "C", "B", "A"]));
        runtime.observe(&mut app, now);
        app.apply_activity_state(ActivityState::NotRepository);
        runtime.observe(&mut app, now);
        assert!(runtime.added_at.is_empty());
        assert_eq!(app.commit_emphasis("D"), TransientEmphasisPhase::None);
        app.apply_activity_state(state(&["X", "Y", "Z"]));
        assert!(!runtime.observe(&mut app, now));
        assert!(runtime.added_at.is_empty());
        app.apply_activity_state(state(&["N", "X", "Y", "Z"]));
        assert!(runtime.observe(&mut app, now));
        assert_eq!(app.commit_emphasis("N"), TransientEmphasisPhase::Hot);
        for id in ["X", "Y", "Z"] {
            assert_eq!(app.commit_emphasis(id), TransientEmphasisPhase::None);
        }
    }
    #[test]
    fn empty_available_history_still_detects_first_commit() {
        let (mut app, mut runtime, now) = setup(&[]);
        app.apply_activity_state(state(&["A"]));
        assert!(runtime.observe(&mut app, now));
        assert_eq!(app.commit_emphasis("A"), TransientEmphasisPhase::Hot);
    }
    #[test]
    fn initially_unavailable_observation_establishes_quiet_baseline() {
        for unavailable in [ActivityState::Unavailable, ActivityState::NotRepository] {
            let mut app = App::new(ProjectSnapshot::new(
                PlanState::Unavailable,
                unavailable,
                TaskState::Unavailable,
            ));
            let mut runtime = CommitsRuntime::new(app.activity());
            app.apply_activity_state(state(&["C", "B", "A"]));
            assert!(!runtime.observe(&mut app, Instant::now()));
            assert!(runtime.added_at.is_empty());
        }
    }
    #[test]
    fn quiet_baseline_single_and_multiple_prepends() {
        use TransientEmphasisPhase::*;
        let (mut app, mut runtime, now) = setup(&["C", "B", "A"]);
        assert!(!runtime.observe(&mut app, now));
        assert_eq!(app.commit_emphasis("C"), None);
        app.apply_activity_state(state(&["D", "C", "B", "A"]));
        assert!(runtime.observe(&mut app, now));
        assert_eq!(app.commit_emphasis("D"), Hot);
        assert_eq!(app.commit_emphasis("C"), None);
        runtime.advance(&mut app, now + Duration::from_secs(3));
        app.apply_activity_state(state(&["G", "F", "E", "D", "C"]));
        runtime.observe(&mut app, now + Duration::from_secs(4));
        for id in ["G", "F", "E"] {
            assert_eq!(app.commit_emphasis(id), Hot);
        }
        for id in ["D", "C"] {
            assert_eq!(app.commit_emphasis(id), None);
        }
    }
    #[test]
    fn discontinuity_clears_active_cues_then_accepts_next_prepend() {
        use TransientEmphasisPhase::*;
        let (mut app, mut runtime, now) = setup(&["C", "B", "A"]);
        app.apply_activity_state(state(&["D", "C", "B"]));
        runtime.observe(&mut app, now);
        app.apply_activity_state(state(&["X", "C", "Y"]));
        runtime.observe(&mut app, now);
        assert!(runtime.added_at.is_empty());
        for id in ["D", "X", "C", "Y"] {
            assert_eq!(app.commit_emphasis(id), None);
        }
        app.apply_activity_state(state(&["N", "X", "C", "Y"]));
        runtime.observe(&mut app, now);
        assert_eq!(app.commit_emphasis("N"), Hot);
        app.apply_activity_state(ActivityState::Unavailable);
        runtime.observe(&mut app, now);
        assert_eq!(app.commit_emphasis("N"), None);
        assert!(runtime.added_at.is_empty());
    }
    #[test]
    fn first_commit_boundaries_and_unchanged_refresh() {
        use TransientEmphasisPhase::*;
        let (mut app, mut runtime, now) = setup(&[]);
        app.apply_activity_state(state(&["A"]));
        runtime.observe(&mut app, now);
        for (ms, phase, redraw) in [
            (0, Hot, false),
            (749, Hot, false),
            (750, Warm, true),
            (1499, Warm, false),
            (1500, Settling, false),
            (2249, Settling, false),
            (2250, Cooling, false),
            (2999, Cooling, false),
            (3000, None, true),
        ] {
            assert_eq!(
                runtime.advance(&mut app, now + Duration::from_millis(ms)),
                redraw
            );
            assert_eq!(app.commit_emphasis("A"), phase);
            assert!(!runtime.observe(&mut app, now + Duration::from_millis(ms)));
        }
        assert!(runtime.added_at.is_empty());
    }
    #[test]
    fn timers_are_independent_and_removed_ids_are_pruned() {
        use TransientEmphasisPhase::*;
        let (mut app, mut runtime, now) = setup(&["C"]);
        app.apply_activity_state(state(&["D", "C"]));
        runtime.observe(&mut app, now);
        let later = now + Duration::from_millis(1000);
        runtime.advance(&mut app, later);
        app.apply_activity_state(state(&["E", "D", "C"]));
        runtime.observe(&mut app, later);
        assert_eq!(app.commit_emphasis("D"), Warm);
        assert_eq!(app.commit_emphasis("E"), Hot);
        runtime.advance(&mut app, now + Duration::from_millis(3000));
        assert_eq!(app.commit_emphasis("D"), None);
        assert_eq!(app.commit_emphasis("E"), Settling);
        app.apply_activity_state(state(&["F", "E"]));
        runtime.observe(&mut app, now + Duration::from_millis(3100));
        app.apply_activity_state(state(&["F"]));
        runtime.observe(&mut app, now + Duration::from_millis(3200));
        assert_eq!(app.commit_emphasis("E"), None);
        assert_eq!(runtime.added_at.len(), 1);
    }
}
