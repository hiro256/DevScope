//! Resolves the concrete Build/Test process command without starting it.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::config::ProjectConfig;

use super::{BuildTestCommandSpec, BuildTestKind, cargo_build_test_command};

pub fn resolve_build_test_command(
    root: &Path,
    config: &ProjectConfig,
    kind: BuildTestKind,
) -> Option<BuildTestCommandSpec> {
    let command = match kind {
        BuildTestKind::BuildDebug => config.verify().build(),
        BuildTestKind::BuildRelease => config.verify().build_release(),
        BuildTestKind::Test => config.verify().test(),
    };
    command
        .map(|command| {
            let program = command.program();
            let source_label = Path::new(program)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or(program);
            let command_label = std::iter::once(program)
                .chain(command.args().iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            BuildTestCommandSpec::new(
                kind,
                source_label,
                command_label,
                OsString::from(program),
                command.args().iter().map(OsString::from).collect(),
                PathBuf::from(root),
            )
        })
        .or_else(|| cargo_build_test_command(root, kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_project_config;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static ID: AtomicUsize = AtomicUsize::new(0);
    struct TempProject(PathBuf);
    impl TempProject {
        fn new(config: &str, cargo: bool) -> Self {
            let root = std::env::temp_dir().join(format!(
                "devscope-resolver-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join(".devscope")).unwrap();
            fs::write(root.join(".devscope/config.toml"), config).unwrap();
            if cargo {
                fs::write(root.join("Cargo.toml"), "[package]").unwrap();
            }
            Self(root)
        }
    }
    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolves_each_profile_independently_without_mutating_configured_arguments() {
        for (text, debug, release) in [
            (
                "[verify.build]\nprogram = 'dotnet'\nargs = ['build', '-c', 'Debug']",
                Some("dotnet build -c Debug"),
                None,
            ),
            (
                "[verify.build.release]\nprogram = 'dotnet'\nargs = ['build', '-c', 'Release']",
                None,
                Some("dotnet build -c Release"),
            ),
            (
                "[verify.build]\nprogram = 'custom'\nargs = ['debug']\n[verify.build.release]\nprogram = 'custom'\nargs = ['release']",
                Some("custom debug"),
                Some("custom release"),
            ),
        ] {
            for cargo in [false, true] {
                let project = TempProject::new(text, cargo);
                let config = load_project_config(&project.0).unwrap();
                for (kind, expected) in [
                    (
                        BuildTestKind::BuildDebug,
                        debug.or(cargo.then_some("cargo check")),
                    ),
                    (
                        BuildTestKind::BuildRelease,
                        release.or(cargo.then_some("cargo check --release")),
                    ),
                    (BuildTestKind::Test, cargo.then_some("cargo test")),
                ] {
                    let command = resolve_build_test_command(&project.0, &config, kind);
                    assert_eq!(
                        command.as_ref().map(|command| command.command_label()),
                        expected
                    );
                    if let Some(command) = command {
                        assert_eq!(command.kind(), kind);
                    }
                }
            }
        }
    }

    #[test]
    fn resolves_configured_commands_before_cargo_and_preserves_arguments() {
        let project = TempProject::new(
            "[verify.test]\nprogram = \"python\"\nargs = [\"-m\", \"pytest with spaces\"]",
            true,
        );
        let config = load_project_config(&project.0).unwrap();
        let build =
            resolve_build_test_command(&project.0, &config, BuildTestKind::BuildDebug).unwrap();
        assert_eq!(build.command_label(), "cargo check");
        let test = resolve_build_test_command(&project.0, &config, BuildTestKind::Test).unwrap();
        assert_eq!(test.source_label(), "python");
        assert_eq!(test.command_label(), "python -m pytest with spaces");
        assert_eq!(
            test.arguments(),
            [OsString::from("-m"), OsString::from("pytest with spaces")]
        );
        assert_eq!(test.working_directory(), project.0);
    }

    #[test]
    fn resolves_non_cargo_partial_config_without_execution() {
        let project = TempProject::new("[verify.test]\nprogram = \"pytest\"", false);
        let config = load_project_config(&project.0).unwrap();
        assert!(
            resolve_build_test_command(&project.0, &config, BuildTestKind::BuildDebug).is_none()
        );
        let test = resolve_build_test_command(&project.0, &config, BuildTestKind::Test).unwrap();
        assert!(test.arguments().is_empty());
        assert_eq!(test.source_label(), "pytest");
    }
}
