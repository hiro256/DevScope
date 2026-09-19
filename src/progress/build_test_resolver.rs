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
        BuildTestKind::Build => config.verify().build(),
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
