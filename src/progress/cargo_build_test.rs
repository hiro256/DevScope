//! Cargo command specifications for v0.3 Build/Test Evidence.
//!
//! This module determines whether a project root is structurally applicable to
//! Cargo and creates command specifications. It does not detect or start Cargo.

use std::{ffi::OsString, path::Path};

use super::{BuildTestCommandSpec, BuildTestKind};

const CARGO_SOURCE_LABEL: &str = "cargo";
const CARGO_PROGRAM: &str = "cargo";

/// Returns whether the project root contains a regular `Cargo.toml` file.
pub fn is_cargo_project(root: &Path) -> bool {
    root.join("Cargo.toml").is_file()
}

/// Creates the initial Cargo Build/Test command specification for a project root.
///
/// Returns `None` when the root does not directly contain a regular `Cargo.toml`.
pub fn cargo_build_test_command(root: &Path, kind: BuildTestKind) -> Option<BuildTestCommandSpec> {
    is_cargo_project(root).then(|| {
        let (command_label, argument) = match kind {
            BuildTestKind::Build => ("cargo check", "check"),
            BuildTestKind::Test => ("cargo test", "test"),
        };

        BuildTestCommandSpec::new(
            kind,
            CARGO_SOURCE_LABEL,
            command_label,
            OsString::from(CARGO_PROGRAM),
            vec![OsString::from(argument)],
            root,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::OsStr,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static ID: AtomicUsize = AtomicUsize::new(0);

    struct TempProject(PathBuf);

    impl TempProject {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "devscope-cargo-build-test-{name}-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn manifest(&self) {
            fs::write(self.0.join("Cargo.toml"), "[package]").unwrap();
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn recognizes_a_manifest_at_the_project_root() {
        let project = TempProject::new("manifest");
        project.manifest();

        assert!(is_cargo_project(&project.0));
    }

    #[test]
    fn rejects_missing_and_nested_only_manifests() {
        let missing = TempProject::new("missing");
        assert!(!is_cargo_project(&missing.0));

        let nested = TempProject::new("nested");
        fs::create_dir_all(nested.0.join("nested")).unwrap();
        fs::write(nested.0.join("nested/Cargo.toml"), "[package]").unwrap();
        assert!(!is_cargo_project(&nested.0));
    }

    #[test]
    fn creates_the_build_command_spec() {
        let project = TempProject::new("build");
        project.manifest();
        let spec = cargo_build_test_command(&project.0, BuildTestKind::Build).unwrap();

        assert_eq!(spec.kind(), BuildTestKind::Build);
        assert_eq!(spec.source_label(), "cargo");
        assert_eq!(spec.command_label(), "cargo check");
        assert_eq!(spec.program(), OsStr::new("cargo"));
        assert_eq!(spec.arguments(), [OsString::from("check")]);
        assert_eq!(spec.working_directory(), project.0);
    }

    #[test]
    fn creates_the_test_command_spec() {
        let project = TempProject::new("test");
        project.manifest();
        let spec = cargo_build_test_command(&project.0, BuildTestKind::Test).unwrap();

        assert_eq!(spec.kind(), BuildTestKind::Test);
        assert_eq!(spec.source_label(), "cargo");
        assert_eq!(spec.command_label(), "cargo test");
        assert_eq!(spec.program(), OsStr::new("cargo"));
        assert_eq!(spec.arguments(), [OsString::from("test")]);
        assert_eq!(spec.working_directory(), project.0);
    }

    #[test]
    fn keeps_build_and_test_as_separate_commands() {
        let project = TempProject::new("separate");
        project.manifest();
        let build = cargo_build_test_command(&project.0, BuildTestKind::Build).unwrap();
        let test = cargo_build_test_command(&project.0, BuildTestKind::Test).unwrap();

        assert_ne!(build.command_label(), test.command_label());
        assert_ne!(build.arguments(), test.arguments());
    }

    #[test]
    fn preserves_project_roots_with_spaces_and_unicode() {
        for name in ["Work Space", "開発"] {
            let project = TempProject::new(name);
            project.manifest();
            let spec = cargo_build_test_command(&project.0, BuildTestKind::Test).unwrap();

            assert_eq!(spec.working_directory(), project.0);
        }
    }
    #[test]
    fn rejects_a_manifest_directory_and_returns_no_command_for_non_cargo_roots() {
        let directory_manifest = TempProject::new("directory-manifest");
        fs::create_dir_all(directory_manifest.0.join("Cargo.toml")).unwrap();
        assert!(!is_cargo_project(&directory_manifest.0));

        let missing = TempProject::new("non-cargo-command");
        assert!(cargo_build_test_command(&missing.0, BuildTestKind::Build).is_none());
        assert!(cargo_build_test_command(&missing.0, BuildTestKind::Test).is_none());

        let parent = TempProject::new("parent-manifest");
        parent.manifest();
        let child = parent.0.join("child");
        fs::create_dir_all(&child).unwrap();
        assert!(!is_cargo_project(&child));
    }
}
