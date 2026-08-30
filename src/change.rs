//! Lightweight project change detection independent of refresh behavior.

use std::{
    ffi::OsString,
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
    children: Option<Vec<OsString>>,
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
    let children = if kind == WorktreeEntryKind::Directory {
        Some(directory_children(path)?)
    } else {
        None
    };
    Ok(WorktreeEntryStamp {
        path: path.to_path_buf(),
        kind,
        len: (kind == WorktreeEntryKind::File).then_some(metadata.len()),
        modified: metadata.modified().ok(),
        children,
    })
}

fn directory_children(directory: &Path) -> Result<Vec<OsString>, GitWorktreeChangeError> {
    let mut children = fs::read_dir(directory)
        .map_err(|source| GitWorktreeChangeError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?
        .map(|entry| {
            entry.map_err(|source| GitWorktreeChangeError::ReadDirectory {
                path: directory.to_path_buf(),
                source,
            })
        })
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_name() == ".git" => None,
            Ok(entry) => Some(Ok(entry.file_name())),
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    children.sort();
    Ok(children)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMetadataChange {
    Changed,
    Unchanged,
}
#[derive(Debug)]
pub enum GitMetadataChangeError {
    Read { path: PathBuf, source: io::Error },
    InvalidGitFile { path: PathBuf },
    UnsafeRef { reference: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum GitMetadataState {
    Absent,
    Present(GitMetadataFingerprint),
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct GitMetadataFingerprint {
    git_dir: PathBuf,
    common_dir: PathBuf,
    head: String,
    current_ref: Option<(String, Option<String>)>,
    index: MetadataStamp,
    packed_refs: MetadataStamp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct MetadataStamp {
    exists: bool,
    len: Option<u64>,
    modified: Option<SystemTime>,
}
pub struct GitMetadataChangeDetector {
    baseline: Option<GitMetadataState>,
}
impl GitMetadataChangeDetector {
    pub fn new(root: &Path) -> Self {
        Self {
            baseline: git_metadata_fingerprint(root).ok(),
        }
    }
    pub fn check(&mut self, root: &Path) -> Result<GitMetadataChange, GitMetadataChangeError> {
        let current = git_metadata_fingerprint(root)?;
        let changed = self
            .baseline
            .as_ref()
            .is_some_and(|baseline| baseline != &current);
        self.baseline = Some(current);
        Ok(if changed {
            GitMetadataChange::Changed
        } else {
            GitMetadataChange::Unchanged
        })
    }
    pub fn sync(&mut self, root: &Path) {
        if let Ok(current) = git_metadata_fingerprint(root) {
            self.baseline = Some(current);
        }
    }
}
fn git_metadata_fingerprint(root: &Path) -> Result<GitMetadataState, GitMetadataChangeError> {
    let locator = root.join(".git");
    let locator_metadata = match fs::symlink_metadata(&locator) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Ok(GitMetadataState::Absent);
        }
        Err(source) => {
            return Err(GitMetadataChangeError::Read {
                path: locator,
                source,
            });
        }
    };
    let git_dir = if locator_metadata.is_dir() {
        locator
    } else if locator_metadata.is_file() {
        let text = read_small(&locator)?;
        let Some(value) = text.trim().strip_prefix("gitdir:") else {
            return Err(GitMetadataChangeError::InvalidGitFile { path: locator });
        };
        let value = PathBuf::from(value.trim());
        if value.is_absolute() {
            value
        } else {
            root.join(value)
        }
    } else {
        return Err(GitMetadataChangeError::InvalidGitFile { path: locator });
    };
    let common_dir = match read_optional(&git_dir.join("commondir"))? {
        Some(value) => {
            let value = PathBuf::from(value.trim());
            if value.is_absolute() {
                value
            } else {
                git_dir.join(value)
            }
        }
        None => git_dir.clone(),
    };
    let head = read_small(&git_dir.join("HEAD"))?;
    let current_ref = head
        .trim()
        .strip_prefix("ref: ")
        .map(|reference| {
            validate_ref(reference)?;
            Ok((
                reference.to_owned(),
                read_optional(&common_dir.join(reference))?,
            ))
        })
        .transpose()?;
    Ok(GitMetadataState::Present(GitMetadataFingerprint {
        index: metadata_stamp(&git_dir.join("index"))?,
        packed_refs: metadata_stamp(&common_dir.join("packed-refs"))?,
        git_dir,
        common_dir,
        head,
        current_ref,
    }))
}
fn read_small(path: &Path) -> Result<String, GitMetadataChangeError> {
    fs::read_to_string(path).map_err(|source| GitMetadataChangeError::Read {
        path: path.to_path_buf(),
        source,
    })
}
fn read_optional(path: &Path) -> Result<Option<String>, GitMetadataChangeError> {
    match read_small(path) {
        Ok(value) => Ok(Some(value)),
        Err(GitMetadataChangeError::Read { source, .. })
            if source.kind() == io::ErrorKind::NotFound =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}
fn metadata_stamp(path: &Path) -> Result<MetadataStamp, GitMetadataChangeError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(MetadataStamp {
            exists: true,
            len: Some(metadata.len()),
            modified: metadata.modified().ok(),
        }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(MetadataStamp {
            exists: false,
            len: None,
            modified: None,
        }),
        Err(source) => Err(GitMetadataChangeError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}
fn validate_ref(reference: &str) -> Result<(), GitMetadataChangeError> {
    let path = Path::new(reference);
    if !reference.starts_with("refs/")
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(GitMetadataChangeError::UnsafeRef {
            reference: reference.to_owned(),
        });
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
    fn worktree_detects_addition_and_updates_the_baseline() {
        let project = TempProject::new();
        project.write("a.txt", "a");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        project.write("b.txt", "b");
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
    fn worktree_detects_deletion() {
        let project = TempProject::new();
        project.write("a.txt", "a");
        project.write("b.txt", "b");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        fs::remove_file(project.0.join("b.txt")).unwrap();
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );
    }

    #[test]
    fn worktree_detects_rename() {
        let project = TempProject::new();
        project.write("a.txt", "a");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        fs::rename(project.0.join("a.txt"), project.0.join("renamed.txt")).unwrap();
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );
    }

    #[test]
    fn worktree_detects_nested_addition() {
        let project = TempProject::new();
        project.write("existing/subdir/keep.txt", "keep");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        project.write("existing/subdir/new.txt", "new");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );
    }

    #[test]
    fn worktree_ignores_git_directory_changes() {
        let project = TempProject::new();
        project.write("a.txt", "a");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        project.write(".git/internal-file", "ignored");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Unchanged
        );
    }
    fn git(root: &Path, args: &[&str]) {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }

    fn git_project() -> TempProject {
        let project = TempProject::new();
        git(&project.0, &["init"]);
        git(&project.0, &["config", "user.name", "DevScope Test"]);
        git(
            &project.0,
            &["config", "user.email", "devscope@test.invalid"],
        );
        project.write("a.txt", "one");
        git(&project.0, &["add", "."]);
        git(&project.0, &["commit", "-m", "initial"]);
        project
    }

    #[test]
    fn git_metadata_is_unchanged_without_git_operations() {
        let project = git_project();
        let mut detector = GitMetadataChangeDetector::new(&project.0);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Unchanged
        );
    }

    #[test]
    fn git_metadata_detects_staging_only() {
        let project = git_project();
        project.write("a.txt", "edited");
        let mut detector = GitMetadataChangeDetector::new(&project.0);
        git(&project.0, &["add", "a.txt"]);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Changed
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Unchanged
        );
    }

    #[test]
    fn git_metadata_detects_commit_and_updates_baseline() {
        let project = git_project();
        project.write("a.txt", "edited");
        git(&project.0, &["add", "a.txt"]);
        let mut detector = GitMetadataChangeDetector::new(&project.0);
        git(&project.0, &["commit", "-m", "second"]);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Changed
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Unchanged
        );
    }

    #[test]
    fn git_metadata_detects_branch_switch_at_same_commit() {
        let project = git_project();
        git(&project.0, &["branch", "other"]);
        let mut detector = GitMetadataChangeDetector::new(&project.0);
        git(&project.0, &["switch", "other"]);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Changed
        );
    }

    #[test]
    fn git_metadata_detects_initialization_and_removal() {
        let project = TempProject::new();
        let mut detector = GitMetadataChangeDetector::new(&project.0);
        git(&project.0, &["init"]);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Changed
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Unchanged
        );
        fs::rename(project.0.join(".git"), project.0.join("git-backup")).unwrap();
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Changed
        );
    }
    #[test]
    fn git_metadata_resolves_a_relative_gitfile_and_commondir() {
        let project = TempProject::new();
        project.write(".git", "gitdir: git-data\n");
        project.write("git-data/commondir", "../common\n");
        project.write("git-data/HEAD", "ref: refs/heads/main\n");
        project.write("git-data/index", "index");
        project.write("common/refs/heads/main", "first\n");
        let mut detector = GitMetadataChangeDetector::new(&project.0);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Unchanged
        );
        project.write("common/refs/heads/main", "second\n");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Changed
        );
    }
}
