//! Initial process-source boundary for v0.3 Build/Test Evidence.
//!
//! `BuildTestCommandSpec` is a provisional Build/Test process command boundary.
//! It is not a generic Evidence source API or extension contract.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use super::{BuildTestKind, BuildTestRun};

/// A process invocation requested by a concrete Build/Test source.
///
/// The command label is for display only. The program and arguments are the
/// machine-readable process representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTestCommandSpec {
    kind: BuildTestKind,
    source_label: String,
    command_label: String,
    program: OsString,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
}

impl BuildTestCommandSpec {
    pub fn new(
        kind: BuildTestKind,
        source_label: impl Into<String>,
        command_label: impl Into<String>,
        program: impl Into<OsString>,
        arguments: Vec<OsString>,
        working_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            kind,
            source_label: source_label.into(),
            command_label: command_label.into(),
            program: program.into(),
            arguments,
            working_directory: working_directory.into(),
        }
    }

    pub const fn kind(&self) -> BuildTestKind {
        self.kind
    }

    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    pub fn command_label(&self) -> &str {
        &self.command_label
    }

    pub fn program(&self) -> &OsStr {
        &self.program
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }
}

impl From<&BuildTestCommandSpec> for BuildTestRun {
    fn from(spec: &BuildTestCommandSpec) -> Self {
        Self::new(spec.kind, &spec.source_label, &spec.command_label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_spec(kind: BuildTestKind) -> BuildTestCommandSpec {
        BuildTestCommandSpec::new(
            kind,
            "cargo",
            "cargo test",
            OsString::from("cargo"),
            vec![OsString::from("test")],
            PathBuf::from(r"C:\work\DevScope"),
        )
    }

    #[test]
    fn retains_basic_command_spec_fields() {
        let spec = command_spec(BuildTestKind::Test);

        assert_eq!(spec.kind(), BuildTestKind::Test);
        assert_eq!(spec.source_label(), "cargo");
        assert_eq!(spec.command_label(), "cargo test");
        assert_eq!(spec.program(), OsStr::new("cargo"));
        assert_eq!(spec.arguments(), [OsString::from("test")]);
        assert_eq!(spec.working_directory(), Path::new(r"C:\work\DevScope"));
    }

    #[test]
    fn retains_build_and_test_kinds() {
        assert_eq!(
            command_spec(BuildTestKind::Build).kind(),
            BuildTestKind::Build
        );
        assert_eq!(
            command_spec(BuildTestKind::Test).kind(),
            BuildTestKind::Test
        );
    }

    #[test]
    fn permits_empty_arguments() {
        let spec = BuildTestCommandSpec::new(
            BuildTestKind::Build,
            "fixture",
            "display only",
            OsString::from("tool"),
            Vec::new(),
            PathBuf::from(r"C:\work"),
        );

        assert!(spec.arguments().is_empty());
    }

    #[test]
    fn retains_arguments_with_spaces_as_single_arguments() {
        let manifest_path = OsString::from(r"C:\Work Space\project\Cargo.toml");
        let spec = BuildTestCommandSpec::new(
            BuildTestKind::Test,
            "cargo",
            "cargo test",
            OsString::from("cargo"),
            vec![
                OsString::from("--manifest-path"),
                manifest_path.clone(),
                OsString::from("test"),
            ],
            PathBuf::from(r"C:\Work Space\project"),
        );

        assert_eq!(spec.arguments()[1], manifest_path);
        assert_eq!(spec.arguments().len(), 3);
    }

    #[test]
    fn retains_unicode_arguments_and_working_directory() {
        let unicode_argument = OsString::from(r"C:\開発\プロジェクト\Cargo.toml");
        let working_directory = PathBuf::from(r"C:\開発\プロジェクト");
        let spec = BuildTestCommandSpec::new(
            BuildTestKind::Test,
            "cargo",
            "cargo test",
            OsString::from("cargo"),
            vec![unicode_argument.clone()],
            working_directory.clone(),
        );

        assert_eq!(spec.arguments(), [unicode_argument]);
        assert_eq!(spec.working_directory(), working_directory);
    }

    #[test]
    fn keeps_display_and_execution_representations_separate() {
        let spec = BuildTestCommandSpec::new(
            BuildTestKind::Test,
            "fixture",
            "This display label is not executable",
            OsString::from("tool"),
            vec![OsString::from("--machine-readable")],
            PathBuf::from(r"C:\work"),
        );

        assert_eq!(spec.command_label(), "This display label is not executable");
        assert_eq!(spec.program(), OsStr::new("tool"));
        assert_eq!(spec.arguments(), [OsString::from("--machine-readable")]);
    }

    #[test]
    fn converts_command_metadata_to_a_build_test_run() {
        let spec = command_spec(BuildTestKind::Test);
        let run = BuildTestRun::from(&spec);

        assert_eq!(run.kind(), BuildTestKind::Test);
        assert_eq!(run.source_label(), "cargo");
        assert_eq!(run.command_label(), "cargo test");
    }
}
