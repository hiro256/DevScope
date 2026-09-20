//! Lightweight project change detection independent of refresh behavior.

use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    config::CONFIG_PATH,
    current_work::current_work_path,
    progress::{MarkdownProgressError, discover_markdown_files},
};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSubtreeStat {
    pub path: PathBuf,
    pub visited_entries: usize,
    pub duration: std::time::Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeScanDiagnostics {
    pub visited_entries: usize,
    pub subtrees: Vec<WorktreeSubtreeStat>,
}

pub fn diagnose_worktree_scan(
    root: &Path,
) -> Result<WorktreeScanDiagnostics, GitWorktreeChangeError> {
    let mut visited_entries = 1;
    let mut subtrees = Vec::new();
    for entry in fs::read_dir(root).map_err(|source| GitWorktreeChangeError::ReadDirectory {
        path: root.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| GitWorktreeChangeError::ReadDirectory {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        let stamp = worktree_entry_stamp(&path)?;
        if is_generated_worktree_directory(&stamp) {
            continue;
        }
        let started = std::time::Instant::now();
        let mut entries = vec![stamp.clone()];
        if stamp.kind == WorktreeEntryKind::Directory {
            scan_directory(&path, &mut entries)?;
        }
        visited_entries += entries.len();
        subtrees.push(WorktreeSubtreeStat {
            path,
            visited_entries: entries.len(),
            duration: started.elapsed(),
        });
    }
    subtrees.sort_by(|left, right| {
        right
            .duration
            .cmp(&left.duration)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(WorktreeScanDiagnostics {
        visited_entries,
        subtrees,
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
        if is_generated_worktree_directory(&stamp) {
            continue;
        }
        let is_directory = stamp.kind == WorktreeEntryKind::Directory;
        entries.push(stamp);
        if is_directory {
            scan_directory(&path, entries)?;
        }
    }
    Ok(())
}

fn is_generated_worktree_directory(stamp: &WorktreeEntryStamp) -> bool {
    stamp.kind == WorktreeEntryKind::Directory
        && stamp.path.file_name().is_some_and(|name| name == "target")
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigChange {
    Changed,
    Unchanged,
}
#[derive(Debug)]
pub enum ConfigChangeError {
    Read { path: PathBuf, source: io::Error },
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigStamp(Option<u64>);
pub struct ConfigChangeDetector {
    baseline: Option<ConfigStamp>,
}
impl ConfigChangeDetector {
    pub fn new(root: &Path) -> Self {
        Self {
            baseline: config_fingerprint(root).ok(),
        }
    }
    pub fn check(&mut self, root: &Path) -> Result<ConfigChange, ConfigChangeError> {
        let current = config_fingerprint(root)?;
        let changed = self
            .baseline
            .as_ref()
            .is_some_and(|baseline| baseline != &current);
        self.baseline = Some(current);
        Ok(if changed {
            ConfigChange::Changed
        } else {
            ConfigChange::Unchanged
        })
    }
    pub fn sync(&mut self, root: &Path) {
        if let Ok(current) = config_fingerprint(root) {
            self.baseline = Some(current);
        }
    }
}
fn config_fingerprint(root: &Path) -> Result<ConfigStamp, ConfigChangeError> {
    let path = root.join(CONFIG_PATH);
    match fs::read(&path) {
        Ok(bytes) => {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hasher);
            Ok(ConfigStamp(Some(hasher.finish())))
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(ConfigStamp(None)),
        Err(source) => Err(ConfigChangeError::Read { path, source }),
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentWorkChange {
    Changed,
    Unchanged,
}

#[derive(Debug)]
pub enum CurrentWorkChangeError {
    Read { path: PathBuf, source: io::Error },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CurrentWorkStamp(Option<u64>);

pub struct CurrentWorkChangeDetector {
    baseline: Option<CurrentWorkStamp>,
}

impl CurrentWorkChangeDetector {
    pub fn new(root: &Path) -> Self {
        Self {
            baseline: current_work_fingerprint(root).ok(),
        }
    }

    pub fn check(&mut self, root: &Path) -> Result<CurrentWorkChange, CurrentWorkChangeError> {
        let current = match current_work_fingerprint(root) {
            Ok(current) => current,
            Err(error) => {
                self.baseline = None;
                return Err(error);
            }
        };
        let changed = match &self.baseline {
            Some(baseline) => baseline != &current,
            None => true,
        };
        self.baseline = Some(current);
        Ok(if changed {
            CurrentWorkChange::Changed
        } else {
            CurrentWorkChange::Unchanged
        })
    }

    pub fn sync(&mut self, root: &Path) {
        self.baseline = current_work_fingerprint(root).ok();
    }
}

fn current_work_fingerprint(root: &Path) -> Result<CurrentWorkStamp, CurrentWorkChangeError> {
    let path = current_work_path(root);
    match fs::read(&path) {
        Ok(bytes) => {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hasher);
            Ok(CurrentWorkStamp(Some(hasher.finish())))
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(CurrentWorkStamp(None)),
        Err(source) => Err(CurrentWorkChangeError::Read { path, source }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
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
    fn current_work_detector_tracks_creation_same_length_edit_and_deletion() {
        let project = TempProject::new();
        let mut detector = CurrentWorkChangeDetector::new(&project.0);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Unchanged
        );
        project.write(
            ".devscope/work/current.md",
            "# Current Work\nParent: a.md\nTask: One\n- [ ] Alpha\n",
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Changed
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Unchanged
        );
        project.write(
            ".devscope/work/current.md",
            "# Current Work\nParent: a.md\nTask: One\n- [ ] Bravo\n",
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Changed
        );
        fs::remove_file(project.0.join(".devscope/work/current.md")).unwrap();
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Changed
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Unchanged
        );
    }
    #[test]
    fn current_work_detector_invalidates_the_baseline_after_a_read_error() {
        let project = TempProject::new();
        let path = project.0.join(".devscope/work/current.md");
        project.write(
            ".devscope/work/current.md",
            "# Current Work\nParent: a.md\nTask: One\n- [ ] Alpha\n",
        );
        let mut detector = CurrentWorkChangeDetector::new(&project.0);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Unchanged
        );

        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(matches!(
            detector.check(&project.0),
            Err(CurrentWorkChangeError::Read { .. })
        ));

        fs::remove_dir(&path).unwrap();
        project.write(
            ".devscope/work/current.md",
            "# Current Work\nParent: a.md\nTask: One\n- [ ] Alpha\n",
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Changed
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Unchanged
        );

        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        detector.sync(&project.0);
        fs::remove_dir(&path).unwrap();
        project.write(
            ".devscope/work/current.md",
            "# Current Work\nParent: a.md\nTask: One\n- [ ] Alpha\n",
        );
        assert_eq!(
            detector.check(&project.0).unwrap(),
            CurrentWorkChange::Changed
        );
    }
    #[test]
    fn config_detector_tracks_creation_same_length_edit_and_deletion() {
        let project = TempProject::new();
        let mut detector = ConfigChangeDetector::new(&project.0);
        assert_eq!(detector.check(&project.0).unwrap(), ConfigChange::Unchanged);
        assert_eq!(detector.check(&project.0).unwrap(), ConfigChange::Unchanged);
        project.write(CONFIG_PATH, "[plan]\nexclude = [\"alpha\"]\n");
        assert_eq!(detector.check(&project.0).unwrap(), ConfigChange::Changed);
        assert_eq!(detector.check(&project.0).unwrap(), ConfigChange::Unchanged);
        project.write(CONFIG_PATH, "[plan]\nexclude = [\"bravo\"]\n");
        assert_eq!(detector.check(&project.0).unwrap(), ConfigChange::Changed);
        assert_eq!(detector.check(&project.0).unwrap(), ConfigChange::Unchanged);
        fs::remove_file(project.0.join(CONFIG_PATH)).unwrap();
        assert_eq!(detector.check(&project.0).unwrap(), ConfigChange::Changed);
        assert_eq!(detector.check(&project.0).unwrap(), ConfigChange::Unchanged);
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
    #[ignore = "manual Windows timing experiment"]
    fn worktree_diagnostics_timing_experiment() {
        let project = TempProject::new();
        for index in 0..5_000 {
            project.write(&format!("generated/{index}.txt"), "fixture");
        }
        let mut baseline = Vec::new();
        let mut diagnostics = Vec::new();
        for _ in 0..5 {
            let started = std::time::Instant::now();
            let _ = scan_worktree(&project.0).unwrap();
            baseline.push(started.elapsed());
            let started = std::time::Instant::now();
            let _ = diagnose_worktree_scan(&project.0).unwrap();
            diagnostics.push(started.elapsed());
        }
        baseline.sort();
        diagnostics.sort();
        println!(
            "worktree diagnostic timing: baseline={:?}/{:?}; diagnostics={:?}/{:?}",
            baseline[2], baseline[4], diagnostics[2], diagnostics[4]
        );
    }

    #[test]
    fn worktree_diagnostics_group_root_subtrees_and_skip_target() {
        let project = TempProject::new();
        project.write("src/main.rs", "fn main() {}");
        project.write("src/lib.rs", "pub fn lib() {}");
        project.write("docs/guide.md", "# Guide");
        project.write("README.md", "# Root");
        project.write("target/generated.txt", "ignored");

        let diagnostics = diagnose_worktree_scan(&project.0).unwrap();
        assert_eq!(diagnostics.visited_entries, 7);
        assert_eq!(diagnostics.subtrees.len(), 3);
        let src = diagnostics
            .subtrees
            .iter()
            .find(|stat| stat.path.file_name().is_some_and(|name| name == "src"))
            .unwrap();
        assert_eq!(src.visited_entries, 3);
        assert!(
            diagnostics
                .subtrees
                .iter()
                .all(|stat| stat.path.file_name().is_none_or(|name| name != "target"))
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

    #[test]
    fn worktree_ignores_generated_target_directory_changes() {
        let project = TempProject::new();
        project.write("a.txt", "a");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        project.write("target/debug/output", "first");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Unchanged
        );

        project.write("target/debug/output", "second");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Unchanged
        );

        fs::remove_dir_all(project.0.join("target")).unwrap();
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Unchanged
        );
    }

    #[test]
    fn worktree_keeps_a_regular_file_named_target_relevant() {
        let project = TempProject::new();
        project.write("target", "before");
        let mut detector = GitWorktreeChangeDetector::new(&project.0);

        project.write("target", "after changed");
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitWorktreeChange::Changed
        );
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CandidateStamp {
        exists: bool,
        len: Option<u64>,
        modified: Option<SystemTime>,
    }

    #[derive(Debug)]
    struct TrackedCandidateCache {
        paths: Vec<PathBuf>,
        stamps: Vec<CandidateStamp>,
    }

    impl TrackedCandidateCache {
        fn new(root: &Path) -> Self {
            let paths = tracked_candidate_paths(root);
            let stamps = paths
                .iter()
                .map(|path| candidate_stamp(&root.join(path)))
                .collect();
            Self { paths, stamps }
        }

        fn refresh(&mut self, root: &Path) {
            *self = Self::new(root);
        }

        fn changed(&self, root: &Path) -> bool {
            self.paths
                .iter()
                .zip(&self.stamps)
                .any(|(path, stamp)| candidate_stamp(&root.join(path)) != *stamp)
        }
    }

    fn candidate_stamp(path: &Path) -> CandidateStamp {
        match fs::metadata(path) {
            Ok(metadata) => CandidateStamp {
                exists: true,
                len: Some(metadata.len()),
                modified: metadata.modified().ok(),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => CandidateStamp {
                exists: false,
                len: None,
                modified: None,
            },
            Err(error) => panic!("candidate metadata failed for {}: {error}", path.display()),
        }
    }

    fn git_candidate_output(root: &Path, args: &[&str]) -> Vec<u8> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("ls-files")
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    }

    fn parse_candidate_paths(output: &[u8]) -> Vec<PathBuf> {
        output
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| PathBuf::from(std::str::from_utf8(path).unwrap()))
            .collect()
    }

    fn git_candidate_paths(root: &Path, args: &[&str]) -> Vec<PathBuf> {
        parse_candidate_paths(&git_candidate_output(root, args))
    }

    fn tracked_candidate_paths(root: &Path) -> Vec<PathBuf> {
        git_candidate_paths(root, &["-c", "-z"])
    }

    fn untracked_candidate_paths(root: &Path) -> Vec<PathBuf> {
        git_candidate_paths(root, &["-o", "--exclude-standard", "-z"])
    }

    #[test]
    fn candidate_cache_detects_tracked_edits_and_deletions() {
        let project = git_project();
        let cache = TrackedCandidateCache::new(&project.0);
        assert_eq!(cache.paths, vec![PathBuf::from("a.txt")]);
        assert!(!cache.changed(&project.0));

        project.write("a.txt", "a longer tracked edit");
        assert!(cache.changed(&project.0));
        fs::remove_file(project.0.join("a.txt")).unwrap();
        assert!(cache.changed(&project.0));
    }

    #[test]
    fn candidate_cache_refreshes_after_index_changes_and_keeps_tracked_ignored_paths() {
        let project = git_project();
        project.write(".gitignore", "target/\n");
        git(&project.0, &["add", ".gitignore"]);
        git(&project.0, &["commit", "-m", "ignore target"]);
        project.write("target/tracked.txt", "tracked despite ignore");
        git(&project.0, &["add", "-f", "target/tracked.txt"]);
        let mut cache = TrackedCandidateCache::new(&project.0);
        assert!(cache.paths.contains(&PathBuf::from("target/tracked.txt")));

        let mut metadata = GitMetadataChangeDetector::new(&project.0);
        git(&project.0, &["mv", "a.txt", "renamed.txt"]);
        assert_eq!(
            metadata.check(&project.0).unwrap(),
            GitMetadataChange::Changed
        );
        cache.refresh(&project.0);
        assert!(cache.paths.contains(&PathBuf::from("renamed.txt")));
        assert!(!cache.paths.contains(&PathBuf::from("a.txt")));

        git(&project.0, &["commit", "-m", "rename"]);
        let index_before = metadata_stamp(&project.0.join(".git/index")).unwrap();
        git(&project.0, &["rm", "renamed.txt"]);
        assert_ne!(
            metadata_stamp(&project.0.join(".git/index")).unwrap(),
            index_before
        );
        cache.refresh(&project.0);
        assert!(!cache.paths.contains(&PathBuf::from("renamed.txt")));
    }

    #[test]
    fn candidate_cache_keeps_branch_switches_and_staged_adds_separate() {
        let project = git_project();
        git(&project.0, &["branch", "same-commit"]);
        let mut metadata = GitMetadataChangeDetector::new(&project.0);
        let before = tracked_candidate_paths(&project.0);
        git(&project.0, &["switch", "same-commit"]);
        assert_eq!(
            metadata.check(&project.0).unwrap(),
            GitMetadataChange::Changed
        );
        assert_eq!(tracked_candidate_paths(&project.0), before);

        let index_before = metadata_stamp(&project.0.join(".git/index")).unwrap();
        project.write("staged.txt", "staged");
        git(&project.0, &["add", "staged.txt"]);
        assert_ne!(
            metadata_stamp(&project.0.join(".git/index")).unwrap(),
            index_before
        );
        assert!(tracked_candidate_paths(&project.0).contains(&PathBuf::from("staged.txt")));
    }

    #[test]
    fn candidate_cache_requires_untracked_discovery_for_new_paths_and_honors_ignores() {
        let project = git_project();
        project.write(".gitignore", "ignored/\n*.log\n!important.log\n");
        project.write("subdir/.gitignore", "cache/\n");
        git(&project.0, &["add", ".gitignore", "subdir/.gitignore"]);
        git(&project.0, &["commit", "-m", "ignore fixture"]);
        let cache = TrackedCandidateCache::new(&project.0);

        project.write("new.txt", "root");
        project.write("src/new.rs", "nested");
        project.write("new-top-level/file.txt", "directory");
        project.write("ignored/file.txt", "ignored");
        project.write("subdir/cache/file.txt", "ignored nested");
        project.write("ordinary.log", "ignored");
        project.write("important.log", "not ignored");
        let untracked = untracked_candidate_paths(&project.0);
        for path in [
            "new.txt",
            "src/new.rs",
            "new-top-level/file.txt",
            "important.log",
        ] {
            assert!(untracked.contains(&PathBuf::from(path)), "{path}");
        }
        for path in ["ignored/file.txt", "subdir/cache/file.txt", "ordinary.log"] {
            assert!(!untracked.contains(&PathBuf::from(path)), "{path}");
        }
        assert!(!cache.changed(&project.0));
    }

    #[test]
    fn candidate_cache_keeps_tracked_devscope_work_and_config_visible() {
        let project = git_project();
        project.write(".gitignore", ".devscope/work/\n");
        project.write(".devscope/work/current.md", "tracked work");
        project.write(".devscope/config.toml", "[plan]\n");
        git(
            &project.0,
            &[
                "add",
                ".gitignore",
                "-f",
                ".devscope/work/current.md",
                ".devscope/config.toml",
            ],
        );
        let tracked = tracked_candidate_paths(&project.0);
        assert!(tracked.contains(&PathBuf::from(".devscope/work/current.md")));
        assert!(tracked.contains(&PathBuf::from(".devscope/config.toml")));
    }

    #[test]
    #[ignore = "manual Windows timing experiment"]
    fn candidate_cache_timing_experiment() {
        let project = git_project();
        project.write(".gitignore", "ignored/\n");
        for index in 0..5_000 {
            project.write(&format!("tracked/{index}.txt"), "tracked");
            project.write(&format!("ignored/{index}.txt"), "ignored");
        }
        git(&project.0, &["add", "."]);
        git(&project.0, &["commit", "-m", "candidate timing fixture"]);
        for index in 0..25 {
            project.write(&format!("untracked/{index}.txt"), "untracked");
        }

        let cache = TrackedCandidateCache::new(&project.0);
        let mut full_process = Vec::new();
        let mut full_parse = Vec::new();
        let mut full_metadata = Vec::new();
        let mut full_total = Vec::new();
        let mut cached_tracked_metadata = Vec::new();
        let mut cached_untracked_process = Vec::new();
        let mut cached_untracked_parse = Vec::new();
        let mut cached_untracked_metadata = Vec::new();
        let mut cached_total = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            let full_output =
                git_candidate_output(&project.0, &["-c", "-o", "--exclude-standard", "-z"]);
            full_process.push(started.elapsed());
            let started = Instant::now();
            let full = parse_candidate_paths(&full_output);
            full_parse.push(started.elapsed());
            let started = Instant::now();
            let _stamps: Vec<_> = full
                .iter()
                .map(|path| candidate_stamp(&project.0.join(path)))
                .collect();
            full_metadata.push(started.elapsed());
            full_total.push(
                *full_process.last().unwrap()
                    + *full_parse.last().unwrap()
                    + *full_metadata.last().unwrap(),
            );

            let started = Instant::now();
            let _tracked_changed = cache.changed(&project.0);
            cached_tracked_metadata.push(started.elapsed());
            let started = Instant::now();
            let untracked_output =
                git_candidate_output(&project.0, &["-o", "--exclude-standard", "-z"]);
            cached_untracked_process.push(started.elapsed());
            let started = Instant::now();
            let untracked = parse_candidate_paths(&untracked_output);
            cached_untracked_parse.push(started.elapsed());
            let started = Instant::now();
            let _stamps: Vec<_> = untracked
                .iter()
                .map(|path| candidate_stamp(&project.0.join(path)))
                .collect();
            cached_untracked_metadata.push(started.elapsed());
            cached_total.push(
                *cached_tracked_metadata.last().unwrap()
                    + *cached_untracked_process.last().unwrap()
                    + *cached_untracked_parse.last().unwrap()
                    + *cached_untracked_metadata.last().unwrap(),
            );
        }
        println!(
            "candidate timing full: process={:?}; parse={:?}; metadata={:?}; total={:?}",
            timing_summary(&mut full_process),
            timing_summary(&mut full_parse),
            timing_summary(&mut full_metadata),
            timing_summary(&mut full_total),
        );
        println!(
            "candidate timing cached: tracked metadata={:?}; untracked process={:?}; untracked parse={:?}; untracked metadata={:?}; total={:?}",
            timing_summary(&mut cached_tracked_metadata),
            timing_summary(&mut cached_untracked_process),
            timing_summary(&mut cached_untracked_parse),
            timing_summary(&mut cached_untracked_metadata),
            timing_summary(&mut cached_total),
        );
    }

    fn timing_summary(samples: &mut [Duration]) -> (Duration, Duration) {
        samples.sort();
        (samples[samples.len() / 2], samples[samples.len() - 1])
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
    #[test]
    fn git_metadata_detects_detached_head_movement() {
        let project = git_project();
        project.write("a.txt", "two");
        git(&project.0, &["add", "a.txt"]);
        git(&project.0, &["commit", "-m", "second"]);
        let commit_b = String::from_utf8(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&project.0)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let commit_a = String::from_utf8(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&project.0)
                .args(["rev-parse", "HEAD~1"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        git(&project.0, &["checkout", "--detach", commit_a.trim()]);
        let mut detector = GitMetadataChangeDetector::new(&project.0);
        assert_eq!(
            detector.check(&project.0).unwrap(),
            GitMetadataChange::Unchanged
        );
        git(
            &project.0,
            &["update-ref", "--no-deref", "HEAD", commit_b.trim()],
        );
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
    fn rejects_unsafe_git_ref_paths() {
        for reference in ["../../outside", "refs/heads/../../outside", "heads/main"] {
            assert!(matches!(
                validate_ref(reference),
                Err(GitMetadataChangeError::UnsafeRef { .. })
            ));
        }
    }
}
