//! Session-local TUI browser state; observations happen only at input/refresh boundaries.
use devscope::progress::{
    BrowserEntry, BrowserEntryKind, SafeTextError, read_project_directory, read_safe_text_file,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct FileBrowserState {
    pub current_dir: PathBuf,
    pub entries: Vec<BrowserEntry>,
    pub selected: Option<usize>,
    pub preview: Option<Result<(String, bool), SafeTextError>>,
    pub preview_scroll: usize,
    pub listing_incomplete: bool,
    pub error: Option<SafeTextError>,
    pub notice: Option<&'static str>,
}

impl FileBrowserState {
    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.selected.and_then(|index| self.entries.get(index))
    }
    fn identity(&self) -> Option<(PathBuf, BrowserEntryKind)> {
        self.selected_entry()
            .map(|entry| (entry.path.clone(), entry.kind))
    }
    pub fn refresh(&mut self, root: Option<&Path>, reopening: bool) {
        let before = self.identity();
        self.notice = None;
        let Some(root) = root else {
            self.entries.clear();
            self.selected = None;
            self.preview = None;
            self.preview_scroll = 0;
            self.listing_incomplete = false;
            self.error = Some(SafeTextError::Missing);
            return;
        };
        let mut listing = read_project_directory(root, &self.current_dir);
        if reopening
            && !self.current_dir.as_os_str().is_empty()
            && matches!(
                listing.error,
                Some(
                    SafeTextError::Missing
                        | SafeTextError::UnsafePath
                        | SafeTextError::Symlink
                        | SafeTextError::NotRegularFile
                )
            )
        {
            self.current_dir.clear();
            self.preview_scroll = 0;
            self.notice = Some("Previous directory unavailable; returned to root");
            listing = read_project_directory(root, &self.current_dir);
        }
        self.entries = listing.entries;
        self.error = listing.error;
        self.listing_incomplete = listing.incomplete;
        self.selected = before
            .as_ref()
            .and_then(|(path, kind)| {
                self.entries
                    .iter()
                    .position(|e| &e.path == path && &e.kind == kind)
            })
            .or_else(|| {
                self.entries
                    .iter()
                    .position(|e| e.kind != BrowserEntryKind::Parent)
            })
            .or_else(|| (!self.entries.is_empty()).then_some(0));
        if self.identity() != before {
            self.preview_scroll = 0;
        }
        self.observe_selected(Some(root));
    }
    pub fn move_selection(&mut self, delta: isize, root: Option<&Path>) {
        let before = self.selected;
        if let Some(index) = before {
            self.selected = Some(
                index
                    .saturating_add_signed(delta)
                    .min(self.entries.len().saturating_sub(1)),
            );
        }
        if before != self.selected {
            self.preview_scroll = 0;
            self.observe_selected(root);
        }
    }
    pub fn enter_selected(&mut self, root: Option<&Path>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if matches!(
            entry.kind,
            BrowserEntryKind::Directory | BrowserEntryKind::Parent
        ) {
            self.change_directory(entry.path.clone(), root);
        }
    }
    pub fn parent(&mut self, root: Option<&Path>) {
        if let Some(parent) = self.current_dir.parent() {
            self.change_directory(parent.to_path_buf(), root);
        }
    }
    fn change_directory(&mut self, path: PathBuf, root: Option<&Path>) {
        self.current_dir = path;
        self.entries.clear();
        self.selected = None;
        self.preview = None;
        self.preview_scroll = 0;
        self.refresh(root, false);
    }
    fn observe_selected(&mut self, root: Option<&Path>) {
        self.preview = match (root, self.selected_entry()) {
            (Some(root), Some(entry)) if entry.kind == BrowserEntryKind::File => {
                Some(read_safe_text_file(root, &entry.path))
            }
            _ => None,
        };
    }
    pub fn scroll(&mut self, delta: isize, limit: usize) {
        self.preview_scroll = self.preview_scroll.saturating_add_signed(delta).min(limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static ID: AtomicUsize = AtomicUsize::new(0);
    struct Project(PathBuf);
    impl Project {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "devscope-browser-state-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("dir")).unwrap();
            fs::write(root.join("dir/a"), "first").unwrap();
            fs::write(root.join("dir/b"), "second").unwrap();
            Self(root)
        }
    }
    impl Drop for Project {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn browser_state_selects_first_real_retains_identity_and_resets_on_change() {
        let p = Project::new();
        let root = Some(p.0.as_path());
        let mut browser = FileBrowserState::default();
        browser.refresh(root, true);
        browser.parent(root);
        assert!(browser.current_dir.as_os_str().is_empty());
        browser.enter_selected(root);
        assert_eq!(browser.current_dir, Path::new("dir"));
        assert_eq!(browser.selected, Some(1));
        assert_eq!(browser.preview, Some(Ok(("first".into(), false))));
        browser.scroll(7, 20);
        browser.refresh(root, false);
        assert_eq!(browser.preview_scroll, 7);
        assert_eq!(browser.selected, Some(1));
        browser.move_selection(1, root);
        assert_eq!(browser.preview_scroll, 0);
        assert_eq!(browser.preview, Some(Ok(("second".into(), false))));
        browser.scroll(100, 9);
        assert_eq!(browser.preview_scroll, 9);
        browser.scroll(-100, 9);
        assert_eq!(browser.preview_scroll, 0);
        browser.enter_selected(root);
        assert_eq!(browser.current_dir, Path::new("dir"));
        fs::remove_file(p.0.join("dir/b")).unwrap();
        browser.refresh(root, false);
        assert_eq!(browser.selected, Some(1));
        assert_eq!(browser.preview, Some(Ok(("first".into(), false))));
        browser.move_selection(-10, root);
        assert_eq!(browser.selected, Some(0));
        assert!(browser.preview.is_none());
        browser.enter_selected(root);
        assert!(browser.current_dir.as_os_str().is_empty());
    }

    #[test]
    fn browser_state_reopen_refreshes_and_recovers_invalid_directory() {
        let p = Project::new();
        let root = Some(p.0.as_path());
        let mut browser = FileBrowserState::default();
        browser.refresh(root, true);
        browser.enter_selected(root);
        fs::write(p.0.join("dir/a"), "updated").unwrap();
        browser.refresh(root, true);
        assert_eq!(browser.current_dir, Path::new("dir"));
        assert_eq!(browser.preview, Some(Ok(("updated".into(), false))));
        fs::remove_dir_all(p.0.join("dir")).unwrap();
        browser.refresh(root, false);
        assert!(browser.error.is_some());
        assert!(browser.preview.is_none());
        assert_eq!(
            browser.selected_entry().unwrap().kind,
            BrowserEntryKind::Parent
        );
        browser.refresh(root, true);
        assert!(browser.current_dir.as_os_str().is_empty());
        assert!(browser.notice.is_some());
        assert!(browser.selected.is_none());
        for path in ["target", "../escape"] {
            browser.current_dir = path.into();
            browser.refresh(root, true);
            assert!(browser.current_dir.as_os_str().is_empty());
            assert!(browser.notice.is_some());
        }
        browser.refresh(None, true);
        assert!(browser.error.is_some());
        assert!(browser.entries.is_empty());
    }
}
