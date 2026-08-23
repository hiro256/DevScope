use std::{io, time::Duration};

use crate::terminal::AppTerminal;
use crossterm::event::{self, Event};

use crate::{app::App, ui};

const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(250);

/// Runs the synchronous TUI event loop without polling in a busy loop.
pub fn run(terminal: &mut AppTerminal, app: &mut App) -> io::Result<()> {
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
            Event::Key(key) => app.handle_key(key),
            Event::Resize(_, _) => needs_render = true,
            _ => {}
        }
    }

    Ok(())
}
