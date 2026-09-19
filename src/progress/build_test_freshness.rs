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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BuildTestInputEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BuildTestInputEntry {
    path: PathBuf,
    kind: BuildTestInputEntryKind,
    content_fingerprint: Option<u64>,
    symlink_target: Option<PathBuf>,
}

/// The relevant project filesystem state captured when verification starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTestFreshnessBaseline {
    entries: Vec<BuildTestInputEntry>,
}

impl BuildTestFreshnessBaseline {
    /// Captures the project state at the start of a Build/Test process.
    pub fn capture(root: &Path) -> Result<Self, BuildTestFreshnessError> {
        Self::capture_with_exclusions(root, &[])
    }

    pub fn capture_with_exclusions(
        root: &Path,
        exclusions: &[PathBuf],
    ) -> Result<Self, BuildTestFreshnessError> {
        Ok(Self {
            entries: scan_build_test_inputs(root, exclusions)?,
        })
    }

    /// Compares current inputs with the captured state without updating the baseline.
    pub fn fingerprint(root: &Path) -> Result<u64, BuildTestFreshnessError> {
        Self::fingerprint_with_exclusions(root, &[])
    }

    pub fn fingerprint_with_exclusions(
        root: &Path,
        exclusions: &[PathBuf],
    ) -> Result<u64, BuildTestFreshnessError> {
        let baseline = Self::capture_with_exclusions(root, exclusions)?;
        Ok(baseline.fingerprint_value())
    }

    pub fn fingerprint_value(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.entries.hash(&mut hasher);
        hasher.finish()
    }

    pub fn check(&self, root: &Path) -> Result<BuildTestInputChange, BuildTestFreshnessError> {
        self.check_with_exclusions(root, &[])
    }

    pub fn check_with_exclusions(
        &self,
        root: &Path,
        exclusions: &[PathBuf],
    ) -> Result<BuildTestInputChange, BuildTestFreshnessError> {
        let current = scan_build_test_inputs(root, exclusions)?;
        Ok(if self.entries == current {
            BuildTestInputChange::Unchanged
        } else {
            BuildTestInputChange::Changed
        })
    }
}

/// Evaluates a completed Build/Test run against its inputs at start.
///
/// An unavailable comparison is conservative: the result is stale rather than
/// claiming that its verification inputs remained unchanged.
pub fn evaluate_completed_build_test_freshness(
    root: &Path,
    started_baseline: Option<&BuildTestFreshnessBaseline>,
    inputs_changed_while_running: bool,
) -> (
    super::BuildTestFreshness,
    Option<BuildTestFreshnessBaseline>,
) {
    evaluate_completed_build_test_freshness_with_exclusions(
        root,
        &[],
        started_baseline,
        inputs_changed_while_running,
    )
}

pub fn evaluate_completed_build_test_freshness_with_exclusions(
    root: &Path,
    exclusions: &[PathBuf],
    started_baseline: Option<&BuildTestFreshnessBaseline>,
    inputs_changed_while_running: bool,
) -> (
    super::BuildTestFreshness,
    Option<BuildTestFreshnessBaseline>,
) {
    let Some(started_baseline) = started_baseline else {
        return (super::BuildTestFreshness::Stale, None);
    };

    let inputs_changed_after_start = matches!(
        started_baseline.check_with_exclusions(root, exclusions),
        Ok(BuildTestInputChange::Changed) | Err(_)
    );
    if inputs_changed_while_running || inputs_changed_after_start {
        (super::BuildTestFreshness::Stale, None)
    } else {
        (
            super::BuildTestFreshness::Fresh,
            Some(started_baseline.clone()),
        )
    }
}

fn scan_build_test_inputs(
    root: &Path,
    exclusions: &[PathBuf],
) -> Result<Vec<BuildTestInputEntry>, BuildTestFreshnessError> {
    let mut entries = Vec::new();
    scan_directory(root, root, exclusions, &mut entries)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn scan_directory(
    root: &Path,
    directory: &Path,
    exclusions: &[PathBuf],
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

        if is_excluded(root, &path, kind, exclusions) {
            continue;
        }
        if is_transparent_container(root, &path, kind) {
            scan_directory(root, &path, exclusions, entries)?;
            continue;
        }

        let relative_path = path
            .strip_prefix(root)
            .map_or_else(|_| path.clone(), Path::to_path_buf);
        let input = build_test_input_entry(path.clone(), relative_path, kind)?;
        let is_directory = input.kind == BuildTestInputEntryKind::Directory;
        entries.push(input);
        if is_directory {
            scan_directory(root, &path, exclusions, entries)?;
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

fn is_transparent_container(root: &Path, path: &Path, kind: BuildTestInputEntryKind) -> bool {
    kind == BuildTestInputEntryKind::Directory
        && path
            .strip_prefix(root)
            .is_ok_and(|relative| relative == Path::new(".devscope"))
}
fn is_excluded(
    root: &Path,
    path: &Path,
    kind: BuildTestInputEntryKind,
    exclusions: &[PathBuf],
) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };

    path.strip_prefix(root).is_ok_and(|relative| {
        exclusions
            .iter()
            .any(|excluded| relative == excluded || relative.starts_with(excluded))
    }) || name == ".git"
        || (name == "target" && kind == BuildTestInputEntryKind::Directory)
        || (kind == BuildTestInputEntryKind::Directory
            && path.strip_prefix(root).is_ok_and(|relative| {
                relative == Path::new(".devscope").join("work")
                    || relative == Path::new(".devscope").join("evidence")
            }))
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
    fn completion_freshness_uses_the_start_baseline() {
        let project = TempProject::new();
        project.write("src/lib.rs", "before");
        let baseline = project.capture();

        assert!(matches!(
            evaluate_completed_build_test_freshness(&project.0, Some(&baseline), false),
            (super::super::BuildTestFreshness::Fresh, Some(_))
        ));

        project.write("src/lib.rs", "after");
        assert_eq!(
            evaluate_completed_build_test_freshness(&project.0, Some(&baseline), false),
            (super::super::BuildTestFreshness::Stale, None)
        );

        let unchanged_project = TempProject::new();
        unchanged_project.write("src/lib.rs", "unchanged");
        let unchanged_baseline = unchanged_project.capture();
        assert_eq!(
            evaluate_completed_build_test_freshness(
                &unchanged_project.0,
                Some(&unchanged_baseline),
                true,
            ),
            (super::super::BuildTestFreshness::Stale, None)
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
    fn ignores_initial_current_work_creation_but_detects_initial_devscope_config() {
        let project = TempProject::new();
        project.write("src/lib.rs", "unchanged");
        let baseline = project.capture();
        project.write(".devscope/work/current.md", "# Current Work");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );

        let project = TempProject::new();
        project.write("src/lib.rs", "unchanged");
        let baseline = project.capture();
        project.write(".devscope/config.toml", "relevant");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
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
        project.write(".devscope/config.toml", "relevant");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn ignores_current_work_deletion_and_keeps_devscope_file_relevant() {
        let project = TempProject::new();
        project.write(".devscope/work/current.md", "before");
        let baseline = project.capture();
        fs::remove_dir_all(project.0.join(".devscope/work")).unwrap();
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Unchanged
        );

        let project = TempProject::new();
        project.write(".devscope", "before");
        let baseline = project.capture();
        project.write(".devscope", "after");
        assert_eq!(
            baseline.check(&project.0).unwrap(),
            BuildTestInputChange::Changed
        );
    }
    #[test]
    fn configured_exclusions_ignore_a_directory_subtree_but_not_relevant_inputs() {
        let project = TempProject::new();
        project.write("src/lib.rs", "before");
        project.write("generated/output.bin", "before");
        let exclusions = [PathBuf::from("generated")];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&project.0, &exclusions).unwrap();
        let initial_fingerprint =
            BuildTestFreshnessBaseline::fingerprint_with_exclusions(&project.0, &exclusions)
                .unwrap();

        project.write("generated/nested/output.bin", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Unchanged
        );
        assert_eq!(
            BuildTestFreshnessBaseline::fingerprint_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            initial_fingerprint
        );

        project.write("src/lib.rs", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn configured_exclusions_support_exact_file_paths() {
        let project = TempProject::new();
        project.write("generated/version.txt", "before");
        project.write("generated/other.txt", "before");
        let exclusions = [PathBuf::from("generated/version.txt")];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&project.0, &exclusions).unwrap();

        project.write("generated/version.txt", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Unchanged
        );

        project.write("generated/other.txt", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn configured_directory_exclusion_keeps_siblings_relevant() {
        let project = TempProject::new();
        project.write("src/App/bin/Debug/net10.0/output.dll", "before");
        project.write("src/App/obj/project.assets.json", "before");
        let exclusions = [PathBuf::from("src/App/bin")];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&project.0, &exclusions).unwrap();

        project.write("src/App/bin/Debug/net10.0/nested/output.dll", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Unchanged
        );

        project.write("src/App/obj/project.assets.json", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn configured_multiple_exclusions_ignore_dotnet_generated_outputs() {
        let project = TempProject::new();
        for path in [
            "src/DogfoodApp/bin/Debug/net10.0/DogfoodApp.exe",
            "src/DogfoodApp/obj/project.assets.json",
            "tests/DogfoodApp.Tests/bin/Debug/net10.0/tests.dll",
            "tests/DogfoodApp.Tests/obj/project.assets.json",
        ] {
            project.write(path, "before");
        }
        let exclusions = [
            PathBuf::from("src/DogfoodApp/bin"),
            PathBuf::from("src/DogfoodApp/obj"),
            PathBuf::from("tests/DogfoodApp.Tests/bin"),
            PathBuf::from("tests/DogfoodApp.Tests/obj"),
        ];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&project.0, &exclusions).unwrap();

        for path in [
            "src/DogfoodApp/bin/Debug/net10.0/DogfoodApp.exe",
            "src/DogfoodApp/obj/project.assets.json",
            "tests/DogfoodApp.Tests/bin/Debug/net10.0/tests.dll",
            "tests/DogfoodApp.Tests/obj/project.assets.json",
        ] {
            project.write(path, "after");
        }
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Unchanged
        );
    }

    #[test]
    fn dotnet_generated_outputs_are_ignored_but_source_edits_are_relevant() {
        let project = TempProject::new();
        project.write("src/DogfoodApp/Program.cs", "before");
        project.write("src/DogfoodApp/bin/Debug/net10.0/DogfoodApp.exe", "before");
        project.write("src/DogfoodApp/obj/project.assets.json", "before");
        let exclusions = [
            PathBuf::from("src/DogfoodApp/bin"),
            PathBuf::from("src/DogfoodApp/obj"),
        ];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&project.0, &exclusions).unwrap();

        project.write("src/DogfoodApp/bin/Debug/net10.0/DogfoodApp.exe", "after");
        project.write("src/DogfoodApp/obj/project.assets.json", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Unchanged
        );

        project.write("src/DogfoodApp/Program.cs", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn configured_exclusions_keep_config_and_policy_edits_relevant() {
        let project = TempProject::new();
        project.write("generated/output.bin", "before");
        project.write(
            ".devscope/config.toml",
            r#"[verify]
exclude = ["bin"]
"#,
        );
        let exclusions = [PathBuf::from("generated")];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&project.0, &exclusions).unwrap();

        project.write("generated/output.bin", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Unchanged
        );

        project.write(
            ".devscope/config.toml",
            r#"[verify]
exclude = ["bin", "obj"]
"#,
        );
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Changed
        );
    }

    #[test]
    fn configured_exclusions_preserve_builtin_work_evidence_and_target_ignores() {
        let project = TempProject::new();
        project.write(".devscope/work/current.md", "before");
        project.write(".devscope/evidence/build-test-v1.tsv", "before");
        project.write("target/debug/app.exe", "before");
        project.write("nested/target/file", "before");
        let exclusions = [PathBuf::from("generated")];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&project.0, &exclusions).unwrap();

        project.write(".devscope/work/current.md", "after");
        project.write(".devscope/evidence/build-test-v1.tsv", "after");
        project.write("target/debug/app.exe", "after");
        project.write("nested/target/file", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Unchanged
        );
    }

    #[test]
    fn configured_exclusions_support_unicode_paths() {
        let project = TempProject::new();
        project.write("生成物/output.txt", "before");
        let exclusions = [PathBuf::from("生成物")];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&project.0, &exclusions).unwrap();

        project.write("生成物/output.txt", "after");
        assert_eq!(
            baseline
                .check_with_exclusions(&project.0, &exclusions)
                .unwrap(),
            BuildTestInputChange::Unchanged
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
