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
            baseline: markdown_fingerprint(root).ok(),
        }
    }

    pub fn check(&mut self, root: &Path) -> Result<MarkdownChange, MarkdownChangeError> {
        let current = markdown_fingerprint(root)?;
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
        if let Ok(current) = markdown_fingerprint(root) {
            self.baseline = Some(current);
        }
    }
}

fn markdown_fingerprint(root: &Path) -> Result<Vec<MarkdownFileStamp>, MarkdownChangeError> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitWorktreeChange {
    Changed,
    Unchanged,
}

#[derive(Debug)]
pub enum GitWorktreeChangeError {
    ReadDirectory { path: PathBuf, source: io::Error },
    Metadata { path: PathBuf, source: io::Error },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorktreeEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorktreeEntryStamp {
    path: PathBuf,
    kind: WorktreeEntryKind,
    len: Option<u64>,
    modified: Option<SystemTime>,
}

pub struct GitWorktreeChangeDetector {
    baseline: Option<Vec<WorktreeEntryStamp>>,
}

impl GitWorktreeChangeDetector {
    pub fn new(root: &Path) -> Self {
        Self {
            baseline: scan_worktree(root).ok(),
        }
    }

    pub fn check(&mut self, root: &Path) -> Result<GitWorktreeChange, GitWorktreeChangeError> {
        let Some(baseline) = &self.baseline else {
            self.baseline = Some(scan_worktree(root)?);
            return Ok(GitWorktreeChange::Unchanged);
        };

        if !known_entries_changed(baseline)? {
            return Ok(GitWorktreeChange::Unchanged);
        }

        let current = scan_worktree(root)?;
        let changed = worktree_entries_differ(baseline, &current);
        self.baseline = Some(current);
        Ok(if changed {
            GitWorktreeChange::Changed
        } else {
            GitWorktreeChange::Unchanged
        })
    }

    pub fn sync(&mut self, root: &Path) {
        if let Ok(current) = scan_worktree(root) {
            self.baseline = Some(current);
        }
    }
}

fn known_entries_changed(baseline: &[WorktreeEntryStamp]) -> Result<bool, GitWorktreeChangeError> {
    for stamp in baseline {
        match worktree_entry_stamp(&stamp.path) {
            Ok(current) if current == *stamp => {}
            Ok(_) => return Ok(true),
            Err(GitWorktreeChangeError::Metadata { source, .. })
                if source.kind() == io::ErrorKind::NotFound =>
            {
                return Ok(true);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

fn worktree_entries_differ(
    baseline: &[WorktreeEntryStamp],
    current: &[WorktreeEntryStamp],
) -> bool {
    baseline.len() != current.len()
        || baseline.iter().zip(current).any(|(left, right)| {
            left.path != right.path
                || left.kind != right.kind
                || left.len != right.len
                || (left.kind != WorktreeEntryKind::Directory && left.modified != right.modified)
        })
}
fn scan_worktree(root: &Path) -> Result<Vec<WorktreeEntryStamp>, GitWorktreeChangeError> {
    let root_stamp = worktree_entry_stamp(root)?;
    let mut entries = vec![root_stamp.clone()];
    if root_stamp.kind == WorktreeEntryKind::Directory {
        scan_directory(root, &mut entries)?;
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn scan_directory(
    directory: &Path,
    entries: &mut Vec<WorktreeEntryStamp>,
) -> Result<(), GitWorktreeChangeError> {
    let read_dir =
        fs::read_dir(directory).map_err(|source| GitWorktreeChangeError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
    for entry in read_dir {
        let entry = entry.map_err(|source| GitWorktreeChangeError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }

        let stamp = worktree_entry_stamp(&path)?;
        let is_directory = stamp.kind == WorktreeEntryKind::Directory;
        entries.push(stamp);
        if is_directory {
            scan_directory(&path, entries)?;
        }
    }
    Ok(())
}

fn worktree_entry_stamp(path: &Path) -> Result<WorktreeEntryStamp, GitWorktreeChangeError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| GitWorktreeChangeError::Metadata {
            path: path.to_path_buf(),
            source,
        })?;
    let kind = if metadata.file_type().is_symlink() {
        WorktreeEntryKind::Symlink
    } else if metadata.is_dir() {
        WorktreeEntryKind::Directory
    } else if metadata.is_file() {
        WorktreeEntryKind::File
    } else {
        WorktreeEntryKind::Other
    };
    Ok(WorktreeEntryStamp {
        path: path.to_path_buf(),
        kind,
        len: (kind == WorktreeEntryKind::File).then_some(metadata.len()),
        modified: metadata.modified().ok(),
    })
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

    #[test]
    fn worktree_baseline_is_unchanged_until_filesystem_changes() {
        let project = TempProject::new();
        project.write("a.txt", "a");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Unchanged
        );

        project.write("a.txt", "a longer value");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Unchanged
        );
    }

    #[test]
    fn worktree_detects_addition_deletion_and_rename() {
        let project = TempProject::new();
        project.write("a.txt", "a");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        project.write("b.txt", "b");
        project.write("a.txt", "a longer value");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );
        fs::remove_file(project.0.join("b.txt")).unwrap();
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );
        fs::rename(project.0.join("a.txt"), project.0.join("renamed.txt")).unwrap();
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );
    }

    #[test]
    fn worktree_detects_nested_addition_and_ignores_git() {
        let project = TempProject::new();
        project.write("existing/subdir/keep.txt", "keep");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        project.write("existing/subdir/new.txt", "new");
        project.write("existing/subdir/keep.txt", "keep longer value");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );

        project.write(".git/internal-file", "ignored");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Unchanged
        );
    }
}
