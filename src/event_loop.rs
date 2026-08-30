use std::{io, path::Path, time::Duration};

use crate::terminal::AppTerminal;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use devscope::project::collect_project_snapshot;

use crate::{app::App, ui};

const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(250);

/// Runs the synchronous TUI event loop without polling in a busy loop.
pub fn run(
    terminal: &mut AppTerminal,
    project_root: Option<&Path>,
    app: &mut App,
) -> io::Result<()> {
    let mut needs_render = true;

    while app.is_running() {
        if needs_render {
            terminal.draw(|frame| ui::render(frame, app))?;
            needs_render = false;
        }

        if !event::poll(EVENT_POLL_TIMEOUT)? {
            continue;
        }

        match event::read()? {
            Event::Key(key)
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('r') =>
            {
                if let Some(root) = project_root {
                    app.apply_snapshot(collect_project_snapshot(root));
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

    Ok(())
}
