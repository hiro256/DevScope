//! Local-only persistence for observed Cargo Build/Test results.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use super::{
    BuildTestExecutionError, BuildTestFreshness, BuildTestFreshnessBaseline, BuildTestKind,
    BuildTestOutcome, BuildTestResult, BuildTestState,
};

const STATE_PATH: &str = ".devscope/evidence/build-test-v1.tsv";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedBuildTestState {
    pub kind: BuildTestKind,
    pub state: BuildTestState,
    baseline_fingerprint: Option<u64>,
}

impl PersistedBuildTestState {
    pub fn restore_freshness(&mut self, root: &Path) {
        self.restore_freshness_with_exclusions(root, &[]);
    }

    pub fn restore_freshness_with_exclusions(&mut self, root: &Path, exclusions: &[PathBuf]) {
        if let (BuildTestState::Completed(result), Some(expected)) =
            (&mut self.state, self.baseline_fingerprint)
            && BuildTestFreshnessBaseline::fingerprint_with_exclusions(root, exclusions).ok()
                != Some(expected)
        {
            result.mark_stale();
        }
    }
}

pub fn load_build_test_states(root: &Path) -> Vec<PersistedBuildTestState> {
    let path = root.join(STATE_PATH);
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_states(&contents).unwrap_or_default()
}

pub fn save_build_test_state(
    root: &Path,
    kind: BuildTestKind,
    state: &BuildTestState,
    baseline: Option<&BuildTestFreshnessBaseline>,
) -> io::Result<()> {
    let mut states = load_build_test_states(root);
    states.retain(|entry| entry.kind != kind);
    states.push(PersistedBuildTestState {
        kind,
        state: state.clone(),
        baseline_fingerprint: baseline.map(BuildTestFreshnessBaseline::fingerprint_value),
    });
    states.sort_by_key(|entry| match entry.kind {
        BuildTestKind::Build => 0,
        BuildTestKind::Test => 1,
    });
    let path = root.join(STATE_PATH);
    fs::create_dir_all(path.parent().expect("state path has parent"))?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, render_states(&states))?;
    fs::rename(temporary, path)
}

fn parse_states(text: &str) -> Result<Vec<PersistedBuildTestState>, ()> {
    let mut states = Vec::new();
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() < 3 {
            return Err(());
        }
        let kind = match fields[0] {
            "build" => BuildTestKind::Build,
            "test" => BuildTestKind::Test,
            _ => return Err(()),
        };
        let state = match fields[1] {
            "completed" if fields.len() == 9 || fields.len() == 10 => {
                let outcome = match fields[2] {
                    "passed" => BuildTestOutcome::Passed,
                    "failed" => BuildTestOutcome::Failed,
                    _ => return Err(()),
                };
                let (freshness, offset) = if fields.len() == 10 {
                    (
                        match fields[3] {
                            "fresh" => BuildTestFreshness::Fresh,
                            "stale" => BuildTestFreshness::Stale,
                            _ => return Err(()),
                        },
                        1,
                    )
                } else {
                    (BuildTestFreshness::Fresh, 0)
                };
                let exit_code = fields[3 + offset].parse().ok();
                let duration = fields[4 + offset].parse::<u64>().map_err(|_| ())?;
                let source = decode(fields[5 + offset])?;
                let command = decode(fields[6 + offset])?;
                let summary = decode(fields[7 + offset])?;
                let fingerprint = fields[8 + offset].parse().ok();
                states.push(PersistedBuildTestState {
                    kind,
                    state: BuildTestState::Completed(BuildTestResult::new(
                        kind,
                        outcome,
                        freshness,
                        source,
                        command,
                        exit_code,
                        Duration::from_millis(duration),
                        summary,
                        None,
                    )),
                    baseline_fingerprint: fingerprint,
                });
                continue;
            }
            "error" if fields.len() == 5 => {
                BuildTestState::ExecutionError(BuildTestExecutionError::new(
                    kind,
                    decode(fields[2])?,
                    decode(fields[3])?,
                    decode(fields[4])?,
                ))
            }
            _ => return Err(()),
        };
        states.push(PersistedBuildTestState {
            kind,
            state,
            baseline_fingerprint: None,
        });
    }
    Ok(states)
}

fn render_states(states: &[PersistedBuildTestState]) -> String {
    states
        .iter()
        .map(|entry| {
            let kind = match entry.kind {
                BuildTestKind::Build => "build",
                BuildTestKind::Test => "test",
            };
            match &entry.state {
                BuildTestState::Completed(result) => format!(
                    "{kind}\tcompleted\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    match result.outcome() {
                        BuildTestOutcome::Passed => "passed",
                        BuildTestOutcome::Failed => "failed",
                    },
                    match result.freshness() {
                        BuildTestFreshness::Fresh => "fresh",
                        BuildTestFreshness::Stale => "stale",
                    },
                    result.exit_code().unwrap_or(-1),
                    result.duration().as_millis(),
                    encode(result.source_label()),
                    encode(result.command_label()),
                    encode(result.summary()),
                    entry.baseline_fingerprint.unwrap_or_default()
                ),
                BuildTestState::ExecutionError(error) => format!(
                    "{kind}\terror\t{}\t{}\t{}\n",
                    encode(error.source_label()),
                    encode(error.command_label()),
                    encode(error.message())
                ),
                _ => String::new(),
            }
        })
        .collect()
}

fn encode(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn decode(value: &str) -> Result<String, ()> {
    if !value.len().is_multiple_of(2) {
        return Err(());
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| ()))
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|_| ())
}

#[cfg(test)]
pub fn state_path(root: &Path) -> std::path::PathBuf {
    root.join(STATE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static ID: AtomicUsize = AtomicUsize::new(0);
    fn root() -> PathBuf {
        let path = env::temp_dir().join(format!(
            "devscope-state-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("input.rs"), "one").unwrap();
        path
    }
    fn result(kind: BuildTestKind) -> BuildTestState {
        BuildTestState::Completed(BuildTestResult::new(
            kind,
            BuildTestOutcome::Passed,
            super::super::BuildTestFreshness::Fresh,
            "cargo",
            "cargo check",
            Some(0),
            Duration::from_millis(12),
            "passed",
            None,
        ))
    }
    #[test]
    fn saves_and_reloads_build_and_test_results() {
        let root = root();
        let baseline = BuildTestFreshnessBaseline::capture(&root).unwrap();
        save_build_test_state(
            &root,
            BuildTestKind::Build,
            &result(BuildTestKind::Build),
            Some(&baseline),
        )
        .unwrap();
        save_build_test_state(
            &root,
            BuildTestKind::Test,
            &result(BuildTestKind::Test),
            Some(&baseline),
        )
        .unwrap();
        assert_eq!(load_build_test_states(&root).len(), 2);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn missing_and_corrupt_state_are_safe() {
        let root = root();
        assert!(load_build_test_states(&root).is_empty());
        fs::create_dir_all(state_path(&root).parent().unwrap()).unwrap();
        fs::write(state_path(&root), "broken").unwrap();
        assert!(load_build_test_states(&root).is_empty());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn stale_result_remains_stale_after_reload() {
        let root = root();
        let state = BuildTestState::Completed(BuildTestResult::new(
            BuildTestKind::Build,
            BuildTestOutcome::Passed,
            BuildTestFreshness::Stale,
            "cargo",
            "cargo check",
            Some(0),
            Duration::from_millis(12),
            "passed",
            None,
        ));
        save_build_test_state(&root, BuildTestKind::Build, &state, None).unwrap();

        let mut restored = load_build_test_states(&root).pop().unwrap();
        restored.restore_freshness(&root);
        assert_eq!(
            restored.state.status(),
            super::super::BuildTestStatus::Stale
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn restored_result_becomes_stale_after_input_change() {
        let root = root();
        let baseline = BuildTestFreshnessBaseline::capture(&root).unwrap();
        save_build_test_state(
            &root,
            BuildTestKind::Build,
            &result(BuildTestKind::Build),
            Some(&baseline),
        )
        .unwrap();
        let mut restored = load_build_test_states(&root).pop().unwrap();
        restored.restore_freshness(&root);
        assert_eq!(
            restored.state.status(),
            super::super::BuildTestStatus::Passed
        );
        fs::write(root.join("input.rs"), "two").unwrap();
        restored.restore_freshness(&root);
        assert_eq!(
            restored.state.status(),
            super::super::BuildTestStatus::Stale
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restore_with_exclusions_keeps_generated_only_changes_fresh() {
        let root = root();
        fs::create_dir_all(root.join("generated")).unwrap();
        fs::write(root.join("generated/output"), "before").unwrap();
        let exclusions = [PathBuf::from("generated")];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&root, &exclusions).unwrap();
        save_build_test_state(
            &root,
            BuildTestKind::Build,
            &result(BuildTestKind::Build),
            Some(&baseline),
        )
        .unwrap();

        fs::write(root.join("generated/output"), "after").unwrap();
        let mut restored = load_build_test_states(&root).pop().unwrap();
        restored.restore_freshness_with_exclusions(&root, &exclusions);
        assert_eq!(
            restored.state.status(),
            super::super::BuildTestStatus::Passed
        );

        let mut wrong_policy = load_build_test_states(&root).pop().unwrap();
        wrong_policy.restore_freshness(&root);
        assert_eq!(
            wrong_policy.state.status(),
            super::super::BuildTestStatus::Stale
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restore_with_exclusions_stales_source_and_config_changes() {
        let source_root = root();
        fs::create_dir_all(source_root.join("generated")).unwrap();
        fs::create_dir_all(source_root.join(".devscope")).unwrap();
        fs::write(source_root.join("generated/output"), "before").unwrap();
        fs::write(source_root.join(".devscope/config.toml"), "before").unwrap();
        let exclusions = [PathBuf::from("generated")];
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&source_root, &exclusions).unwrap();
        save_build_test_state(
            &source_root,
            BuildTestKind::Build,
            &result(BuildTestKind::Build),
            Some(&baseline),
        )
        .unwrap();

        fs::write(source_root.join("input.rs"), "two").unwrap();
        let mut source_changed = load_build_test_states(&source_root).pop().unwrap();
        source_changed.restore_freshness_with_exclusions(&source_root, &exclusions);
        assert_eq!(
            source_changed.state.status(),
            super::super::BuildTestStatus::Stale
        );

        let root = root();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            r#"exclude = ["generated"]"#,
        )
        .unwrap();
        let baseline =
            BuildTestFreshnessBaseline::capture_with_exclusions(&root, &exclusions).unwrap();
        save_build_test_state(
            &root,
            BuildTestKind::Build,
            &result(BuildTestKind::Build),
            Some(&baseline),
        )
        .unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            r#"exclude = ["generated", "obj"]"#,
        )
        .unwrap();
        let mut config_changed = load_build_test_states(&root).pop().unwrap();
        config_changed.restore_freshness_with_exclusions(&root, &exclusions);
        assert_eq!(
            config_changed.state.status(),
            super::super::BuildTestStatus::Stale
        );
        let _ = fs::remove_dir_all(root);
    }
}
