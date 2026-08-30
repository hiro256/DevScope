//! Lightweight project change detection independent of refresh behavior.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::progress::{MarkdownProgressError, discover_markdown_files};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownChange {
    Changed,
    Unchanged,
}

#[derive(Debug)]
pub enum MarkdownChangeError {
    Discovery(MarkdownProgressError),
    Metadata { path: PathBuf, source: io::Error },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkdownFileStamp {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

pub struct MarkdownChangeDetector {
    baseline: Option<Vec<MarkdownFileStamp>>,
}

impl MarkdownChangeDetector {
    pub fn new(root: &Path) -> Self {
        Self {
            baseline: fingerprint(root).ok(),
        }
    }

    pub fn check(&mut self, root: &Path) -> Result<MarkdownChange, MarkdownChangeError> {
        let current = fingerprint(root)?;
        let changed = self
            .baseline
            .as_ref()
            .is_some_and(|baseline| baseline != &current);
        self.baseline = Some(current);
        Ok(if changed {
            MarkdownChange::Changed
        } else {
            MarkdownChange::Unchanged
        })
    }

    pub fn sync(&mut self, root: &Path) {
        if let Ok(current) = fingerprint(root) {
            self.baseline = Some(current);
        }
    }
}

fn fingerprint(root: &Path) -> Result<Vec<MarkdownFileStamp>, MarkdownChangeError> {
    discover_markdown_files(root)
        .map_err(MarkdownChangeError::Discovery)?
        .into_iter()
        .map(|path| {
            let metadata = fs::metadata(&path).map_err(|source| MarkdownChangeError::Metadata {
                path: path.clone(),
                source,
            })?;
            Ok(MarkdownFileStamp {
                path,
                len: metadata.len(),
                modified: metadata.modified().ok(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static ID: AtomicUsize = AtomicUsize::new(0);
    struct TempProject(PathBuf);
    impl TempProject {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "devscope-change-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn write(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }
    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn detects_markdown_changes_and_updates_baseline() {
        let project = TempProject::new();
        project.write("a.md", "a");
        let mut detector = MarkdownChangeDetector::new(&project.0);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            MarkdownChange::Unchanged
        );
        project.write("a.md", "a longer");
        assert_eq!(detector.check(&project.0).unwrap(), MarkdownChange::Changed);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            MarkdownChange::Unchanged
        );
    }
    #[test]
    fn detects_addition_and_deletion_but_ignores_other_files() {
        let project = TempProject::new();
        project.write("a.md", "a");
        let mut detector = MarkdownChangeDetector::new(&project.0);
        project.write("src/a.rs", "x");
        project.write(".git/ignored.md", "x");
        project.write("target/ignored.md", "x");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            MarkdownChange::Unchanged
        );
        project.write("b.md", "b");
        assert_eq!(detector.check(&project.0).unwrap(), MarkdownChange::Changed);
        fs::remove_file(project.0.join("b.md")).unwrap();
        assert_eq!(detector.check(&project.0).unwrap(), MarkdownChange::Changed);
    }
}
