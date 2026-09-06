//! Freshness baseline for v0.3 process-based Build/Test Evidence.
//!
//! This module compares project filesystem inputs for Build/Test verification. It
//! is intentionally Build/Test-specific and is not a generic Evidence freshness API.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
};

/// Whether the inputs to a completed Build/Test verification have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTestInputChange {
    Unchanged,
    Changed,
}

/// A filesystem error encountered while capturing or comparing Build/Test inputs.
#[derive(Debug)]
pub enum BuildTestFreshnessError {
    ReadDirectory { path: PathBuf, source: io::Error },
    Metadata { path: PathBuf, source: io::Error },
    ReadFile { path: PathBuf, source: io::Error },
    ReadLink { path: PathBuf, source: io::Error },
}

impl BuildTestFreshnessError {
    pub fn path(&self) -> &Path {
        match self {
            Self::ReadDirectory { path, .. }
            | Self::Metadata { path, .. }
            | Self::ReadFile { path, .. }
            | Self::ReadLink { path, .. } => path,
        }
    }

    pub fn source(&self) -> &io::Error {
        match self {
            Self::ReadDirectory { source, .. }
            | Self::Metadata { source, .. }
            | Self::ReadFile { source, .. }
            | Self::ReadLink { source, .. } => source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildTestInputEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildTestInputEntry {
    path: PathBuf,
    kind: BuildTestInputEntryKind,
    content_fingerprint: Option<u64>,
    symlink_target: Option<PathBuf>,
}

/// The relevant project filesystem state captured when verification completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTestFreshnessBaseline {
    entries: Vec<BuildTestInputEntry>,
}

impl BuildTestFreshnessBaseline {
    /// Captures the project state after a Build/Test process has completed.
    pub fn capture(root: &Path) -> Result<Self, BuildTestFreshnessError> {
        Ok(Self {
            entries: scan_build_test_inputs(root)?,
        })
    }

    /// Compares current inputs with the captured state without updating the baseline.
    pub fn check(&self, root: &Path) -> Result<BuildTestInputChange, BuildTestFreshnessError> {
        let current = scan_build_test_inputs(root)?;
        Ok(if self.entries == current {
            BuildTestInputChange::Unchanged
        } else {
            BuildTestInputChange::Changed
        })
    }
}

fn scan_build_test_inputs(
    root: &Path,
) -> Result<Vec<BuildTestInputEntry>, BuildTestFreshnessError> {
    let mut entries = Vec::new();
    scan_directory(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn scan_directory(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<BuildTestInputEntry>,
) -> Result<(), BuildTestFreshnessError> {
    let read_dir =
        fs::read_dir(directory).map_err(|source| BuildTestFreshnessError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| BuildTestFreshnessError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|source| BuildTestFreshnessError::Metadata {
                path: path.clone(),
                source,
            })?;
        let kind = entry_kind(&metadata);

        if is_excluded(root, &path, kind) {
            continue;
        }

        let relative_path = path
            .strip_prefix(root)
            .map_or_else(|_| path.clone(), Path::to_path_buf);
        let input = build_test_input_entry(path.clone(), relative_path, kind)?;
        let is_directory = input.kind == BuildTestInputEntryKind::Directory;
        entries.push(input);
        if is_directory {
            scan_directory(root, &path, entries)?;
        }
    }

    Ok(())
}

fn entry_kind(metadata: &fs::Metadata) -> BuildTestInputEntryKind {
    if metadata.file_type().is_symlink() {
        BuildTestInputEntryKind::Symlink
    } else if metadata.is_file() {
        BuildTestInputEntryKind::File
    } else if metadata.is_dir() {
        BuildTestInputEntryKind::Directory
    } else {
        BuildTestInputEntryKind::Other
    }
}

fn is_excluded(root: &Path, path: &Path, kind: BuildTestInputEntryKind) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };

    name == ".git"
        || (name == "target" && kind == BuildTestInputEntryKind::Directory)
        || (kind == BuildTestInputEntryKind::Directory
            && path
                .strip_prefix(root)
                .is_ok_and(|relative| relative == Path::new(".devscope").join("work")))
}

fn build_test_input_entry(
    path: PathBuf,
    relative_path: PathBuf,
    kind: BuildTestInputEntryKind,
) -> Result<BuildTestInputEntry, BuildTestFreshnessError> {
    let content_fingerprint = (kind == BuildTestInputEntryKind::File)
        .then(|| file_content_fingerprint(&path))
        .transpose()?;
    let symlink_target = (kind == BuildTestInputEntryKind::Symlink)
        .then(|| {
            fs::read_link(&path).map_err(|source| BuildTestFreshnessError::ReadLink {
                path: path.clone(),
                source,
            })
        })
        .transpose()?;

    Ok(BuildTestInputEntry {
        path: relative_path,
        kind,
        content_fingerprint,
        symlink_target,
    })
}

fn file_content_fingerprint(path: &Path) -> Result<u64, BuildTestFreshnessError> {
    let contents = fs::read(path).map_err(|source| BuildTestFreshnessError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    Ok(hasher.finish())
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
                "devscope-build-test-freshness-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, relative_path: &str, contents: &str) {
            let path = self.0.join(relative_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn capture(&self) -> BuildTestFreshnessBaseline {
            BuildTestFreshnessBaseline::capture(&self.0).unwrap()
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn retains_error_path_and_source() {
        let error = BuildTestFreshnessError::ReadFile {
            path: PathBuf::from("broken.txt"),
            source: io::Error::other("read failed"),
        };

        assert_eq!(error.path(), Path::new("broken.txt"));
        assert_eq!(error.source().kind(), io::ErrorKind::Other);
    }

    #[test]
    fn is_unchanged_immediately_after_capture() {
        let project = TempProject::new();
        project.write("src/lib.rs", "pub fn value() -> u8 { 1 }");
        let baseline = project.capture();

        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );
    }

    #[test]
    fn detects_source_and_same_length_content_changes() {
        let project = TempProject::new();
        project.write("src/lib.rs", "abcd");
        let baseline = project.capture();

        project.write("src/lib.rs", "wxyz");

        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn detects_cargo_manifest_and_lock_changes() {
        for (path, before, after) in [
            ("Cargo.toml", "[package]", "[workspace]"),
            ("Cargo.lock", "version = 1", "version = 2"),
            (
                "build.rs",
                "fn main() {}",
                "fn main() { println!(\"changed\"); }",
            ),
        ] {
            let project = TempProject::new();
            project.write(path, before);
            let baseline = project.capture();
            project.write(path, after);

            assert_eq!(
                baseline.check(&project.0).unwrap(),
                BuildTestInputChange::Changed
            );
        }
    }

    #[test]
    fn detects_markdown_changes_conservatively() {
        let project = TempProject::new();
        project.write("docs/design.md", "before");
        let baseline = project.capture();
        project.write("docs/design.md", "after");

        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn detects_file_addition_and_deletion() {
        let project = TempProject::new();
        project.write("keep.txt", "keep");
        let baseline = project.capture();
        project.write("added.txt", "added");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );

        let project = TempProject::new();
        project.write("removed.txt", "remove");
        let baseline = project.capture();
        fs::remove_file(project.0.join("removed.txt")).unwrap();
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn detects_directory_addition_and_deletion() {
        let project = TempProject::new();
        project.write("keep.txt", "keep");
        let baseline = project.capture();
        project.write("generated/nested/file.txt", "new");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );

        let project = TempProject::new();
        project.write("removed/nested/file.txt", "old");
        let baseline = project.capture();
        fs::remove_dir_all(project.0.join("removed")).unwrap();
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn ignores_target_addition_modification_and_deletion() {
        let project = TempProject::new();
        let baseline = project.capture();
        project.write("target/debug/output", "first");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );
        project.write("target/debug/output", "second");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );
        fs::remove_file(project.0.join("target/debug/output")).unwrap();
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );
    }

    #[test]
    fn ignores_root_current_work_but_not_other_devscope_files() {
        let project = TempProject::new();
        project.write(".devscope/work/current.md", "before");
        let baseline = project.capture();
        project.write(".devscope/work/current.md", "after");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );
        project.write(".devscope/config", "relevant");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn ignores_git_metadata_changes() {
        let project = TempProject::new();
        let baseline = project.capture();
        project.write(".git/HEAD", "ref: refs/heads/main");

        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );
    }

    #[test]
    fn supports_unicode_paths() {
        let project = TempProject::new();
        project.write("src/日本語.rs", "before");
        let baseline = project.capture();
        project.write("src/日本語.rs", "after!");

        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn check_keeps_the_original_baseline_until_recapture() {
        let project = TempProject::new();
        project.write("src/lib.rs", "first");
        let baseline = project.capture();
        project.write("src/lib.rs", "second");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
        project.write("src/lib.rs", "third");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );

        let recaptured = project.capture();
        assert_eq!(
            recaptured.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );
    }
    #[test]
    fn treats_a_regular_file_named_target_as_a_relevant_input() {
        let project = TempProject::new();
        project.write("target", "before");
        let baseline = project.capture();
        project.write("target", "after changed");

        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn ignores_nested_target_directories_recursively() {
        let project = TempProject::new();
        project.write("src/generated/target/output", "before");
        let baseline = project.capture();
        project.write("src/generated/target/output", "after changed");
        project.write("src/generated/target/nested/another", "added");

        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );
    }
}
