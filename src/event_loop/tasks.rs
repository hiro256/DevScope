//! Session-local addition cues; task domain identity remains unchanged.
use crate::app::{App, TaskEmphasisPhase, TaskPresentationKey, TaskState};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub(super) struct TasksRuntime {
    previous: BTreeMap<TaskPresentationKey, usize>,
    added_at: BTreeMap<TaskPresentationKey, Instant>,
}

impl TasksRuntime {
    pub(super) fn new(state: &TaskState) -> Self {
        Self {
            previous: Self::counts(state),
            added_at: BTreeMap::new(),
        }
    }
    fn counts(state: &TaskState) -> BTreeMap<TaskPresentationKey, usize> {
        let mut counts = BTreeMap::new();
        if let TaskState::Available(summary) = state {
            for task in summary.items() {
                *counts
                    .entry(TaskPresentationKey::from_task(task))
                    .or_default() += 1;
            }
        }
        counts
    }
    pub(super) fn observe(&mut self, app: &mut App, now: Instant) -> bool {
        let current = Self::counts(app.tasks());
        let mut redraw = false;
        self.added_at.retain(|key, _| {
            if current.contains_key(key) {
                return true;
            }
            app.set_task_emphasis(key.clone(), TaskEmphasisPhase::None);
            redraw = true;
            false
        });
        for (key, count) in &current {
            if *count > self.previous.get(key).copied().unwrap_or(0) {
                self.added_at.insert(key.clone(), now);
                redraw |= app.task_emphasis(key) != TaskEmphasisPhase::Hot;
                app.set_task_emphasis(key.clone(), TaskEmphasisPhase::Hot);
            }
        }
        self.previous = current;
        redraw
    }
    pub(super) fn advance(&mut self, app: &mut App, now: Instant) -> bool {
        use TaskEmphasisPhase::*;
        let mut redraw = false;
        self.added_at.retain(|key, start| {
            let elapsed = now.saturating_duration_since(*start);
            let phase = if elapsed < Duration::from_millis(750) {
                Hot
            } else if elapsed < Duration::from_millis(1500) {
                Warm
            } else if elapsed < Duration::from_millis(2250) {
                Settling
            } else if elapsed < Duration::from_millis(3000) {
                Cooling
            } else {
                None
            };
            let previous = app.task_emphasis(key);
            if previous != phase {
                app.set_task_emphasis(key.clone(), phase);
                redraw |= !matches!((previous, phase), (Settling, Cooling));
            }
            phase != None
        });
        redraw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ActivityState, PlanState};
    use devscope::{
        progress::{TaskSummary, TaskSummaryItem},
        project::ProjectSnapshot,
    };

    fn state(names: &[&str], first_line: usize) -> TaskState {
        TaskState::Available(TaskSummary::new(
            names.len(),
            names
                .iter()
                .enumerate()
                .map(|(i, text)| {
                    TaskSummaryItem::new("plan.md".into(), first_line + i, (*text).into())
                })
                .collect(),
        ))
    }
    fn key(text: &str) -> TaskPresentationKey {
        TaskPresentationKey("plan.md".into(), text.into())
    }
    fn apply(app: &mut App, names: &[&str], line: usize) {
        app.apply_markdown_state(PlanState::Unavailable, state(names, line));
    }
    fn setup() -> (App, TasksRuntime, Instant) {
        let app = App::new(ProjectSnapshot::new(
            PlanState::Unavailable,
            ActivityState::Unavailable,
            state(&["A", "B"], 1),
        ));
        let runtime = TasksRuntime::new(app.tasks());
        (app, runtime, Instant::now())
    }
    #[test]
    fn additions_ignore_line_shifts_reorder_and_completion() {
        use TaskEmphasisPhase::*;
        let (mut app, mut runtime, now) = setup();
        assert!(!runtime.observe(&mut app, now));
        apply(&mut app, &["B", "A"], 100);
        assert!(!runtime.observe(&mut app, now));
        apply(&mut app, &["日本語の新しいタスク", "A", "B"], 200);
        assert!(runtime.observe(&mut app, now));
        assert_eq!(app.task_emphasis(&key("A")), None);
        assert_eq!(app.task_emphasis(&key("B")), None);
        assert_eq!(app.task_emphasis(&key("日本語の新しいタスク")), Hot);
        apply(&mut app, &["B"], 1);
        runtime.observe(&mut app, now);
        assert!(runtime.added_at.is_empty());
        assert_eq!(app.task_emphasis(&key("B")), None);
        assert_eq!(app.task_emphasis(&key("日本語の新しいタスク")), None);
    }
    #[test]
    fn boundaries_and_unchanged_observation_do_not_extend_timer() {
        use TaskEmphasisPhase::*;
        let (mut app, mut runtime, now) = setup();
        apply(&mut app, &["A", "B", "C"], 1);
        runtime.observe(&mut app, now);
        for (ms, phase) in [
            (0, Hot),
            (749, Hot),
            (750, Warm),
            (1499, Warm),
            (1500, Settling),
            (2249, Settling),
            (2250, Cooling),
            (2999, Cooling),
            (3000, None),
        ] {
            let redraw = runtime.advance(&mut app, now + Duration::from_millis(ms));
            if ms == 2250 {
                assert!(!redraw);
            }
            assert_eq!(app.task_emphasis(&key("C")), phase);
            assert!(!runtime.observe(&mut app, now + Duration::from_millis(ms)));
        }
        assert!(runtime.added_at.is_empty());
    }
    #[test]
    fn duplicate_increase_restarts_only_matching_key_and_counts_decrease_does_not() {
        use TaskEmphasisPhase::*;
        let (mut app, mut runtime, now) = setup();
        apply(&mut app, &["A", "B", "C"], 1);
        runtime.observe(&mut app, now);
        let later = now + Duration::from_millis(1000);
        runtime.advance(&mut app, later);
        apply(&mut app, &["A", "B", "C", "C", "D"], 1);
        runtime.observe(&mut app, later);
        assert_eq!(app.task_emphasis(&key("C")), Hot);
        assert_eq!(app.task_emphasis(&key("D")), Hot);
        apply(&mut app, &["A", "B", "C", "D"], 500);
        assert!(!runtime.observe(&mut app, later));
        apply(&mut app, &["A", "B", "C", "D", "D"], 500);
        runtime.observe(&mut app, now + Duration::from_millis(2000));
        runtime.advance(&mut app, now + Duration::from_millis(4000));
        assert_eq!(app.task_emphasis(&key("C")), None);
        assert_eq!(app.task_emphasis(&key("D")), Settling);
    }
}
