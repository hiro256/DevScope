use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

use crate::terminal::AppTerminal;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use devscope::{change::MarkdownChangeDetector, project::collect_project_snapshot};

use crate::{app::App, ui};

const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(250);
const PROJECT_POLL_INTERVAL: Duration = Duration::from_secs(1);

struct PollScheduler {
    interval: Duration,
    next_tick: Instant,
}

impl PollScheduler {
    fn new(start: Instant, interval: Duration) -> Self {
        Self {
            interval,
            next_tick: next_deadline(start, interval),
        }
    }

    fn is_due(&mut self, now: Instant) -> bool {
        if now < self.next_tick {
            return false;
        }
        self.next_tick = next_deadline(now, self.interval);
        true
    }
}

fn next_deadline(now: Instant, interval: Duration) -> Instant {
    now.checked_add(interval).unwrap_or(now)
}

fn check_markdown_changes(
    project_root: Option<&Path>,
    markdown_changes: &mut Option<MarkdownChangeDetector>,
) {
    if let (Some(root), Some(detector)) = (project_root, markdown_changes) {
        let _ = detector.check(root);
    }
}

/// Runs the synchronous TUI event loop without polling in a busy loop.
pub fn run(
    terminal: &mut AppTerminal,
    project_root: Option<&Path>,
    app: &mut App,
) -> io::Result<()> {
    let mut needs_render = true;
    let mut scheduler = PollScheduler::new(Instant::now(), PROJECT_POLL_INTERVAL);
    let mut markdown_changes = project_root.map(MarkdownChangeDetector::new);

    while app.is_running() {
        if needs_render {
            terminal.draw(|frame| ui::render(frame, app))?;
            needs_render = false;
        }

        if event::poll(EVENT_POLL_TIMEOUT)? {
            match event::read()? {
                Event::Key(key)
                    if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('r') =>
                {
                    if let Some(root) = project_root {
                        app.apply_snapshot(collect_project_snapshot(root));
                        if let Some(detector) = &mut markdown_changes {
                            detector.sync(root);
                        }
                    }
                    needs_render = true;
                }
                Event::Key(key) => {
                    app.handle_key(key);
                    needs_render = true;
                }
                Event::Resize(_, _) => needs_render = true,
                _ => {}
            }
        }

        if scheduler.is_due(Instant::now()) {
            check_markdown_changes(project_root, &mut markdown_changes);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static ID: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn is_not_due_before_the_interval() {
        let start = Instant::now();
        let mut scheduler = PollScheduler::new(start, Duration::from_secs(1));
        assert!(!scheduler.is_due(start + Duration::from_millis(999)));
    }

    #[test]
    fn is_due_at_the_interval_and_reschedules() {
        let start = Instant::now();
        let mut scheduler = PollScheduler::new(start, Duration::from_secs(1));
        let due = start + Duration::from_secs(1);
        assert!(scheduler.is_due(due));
        assert!(!scheduler.is_due(due));
        assert!(!scheduler.is_due(due + Duration::from_millis(999)));
        assert!(scheduler.is_due(due + Duration::from_secs(1)));
    }

    #[test]
    fn delayed_checks_emit_only_one_tick() {
        let start = Instant::now();
        let mut scheduler = PollScheduler::new(start, Duration::from_secs(1));
        let delayed = start + Duration::from_millis(3500);
        assert!(scheduler.is_due(delayed));
        assert!(!scheduler.is_due(delayed));
        assert!(!scheduler.is_due(delayed + Duration::from_millis(999)));
        assert!(scheduler.is_due(delayed + Duration::from_secs(1)));
    }

    #[test]
    fn polling_check_updates_the_markdown_detector_baseline() {
        let root = std::env::temp_dir().join(format!(
            "devscope-event-loop-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let markdown = root.join("tasks.md");
        fs::write(&markdown, "- [ ] First").unwrap();
        let mut detector = Some(MarkdownChangeDetector::new(&root));

        fs::write(&markdown, "- [ ] First\n- [ ] Second").unwrap();
        check_markdown_changes(Some(&root), &mut detector);

        assert_eq!(
            detector.as_mut().unwrap().check(&root).unwrap(),
            devscope::change::MarkdownChange::Unchanged
        );
        let _ = fs::remove_dir_all(root);
    }
}
