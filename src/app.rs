use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

/// UI state only. Project progress data will be supplied by Progress Core later.
pub struct App {
    running: bool,
}

impl App {
    pub const fn new() -> Self {
        Self { running: true }
    }

    pub const fn is_running(&self) -> bool {
        self.running
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            self.running = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::App;

    #[test]
    fn q_exits_the_application() {
        let mut app = App::new();

        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

        assert!(!app.is_running());
    }

    #[test]
    fn escape_exits_the_application() {
        let mut app = App::new();

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!app.is_running());
    }
}
