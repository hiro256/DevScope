//! TUI process verification lifecycle, independent of keyboard and layout routing.
//! App retains Evidence state; this private child owns execution and freshness baselines.

use crate::app::App;
use devscope::{
    config::ProjectConfig,
    progress::{
        BuildTestExecution, BuildTestExecutionCompletion, BuildTestFreshness,
        BuildTestFreshnessBaseline, BuildTestInputChange, BuildTestKind, BuildTestState,
        evaluate_completed_build_test_freshness_with_exclusions, resolve_build_test_command,
        save_build_test_state,
    },
};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct BuildTestRuntime {
    active: Option<BuildTestExecution>,
    build_baseline: Option<BuildTestFreshnessBaseline>,
    release_baseline: Option<BuildTestFreshnessBaseline>,
    test_baseline: Option<BuildTestFreshnessBaseline>,
    active_baseline: Option<BuildTestFreshnessBaseline>,
    active_inputs_changed: bool,
    config: ProjectConfig,
}

impl BuildTestRuntime {
    pub(super) fn new(config: ProjectConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    pub(super) fn initialize(&mut self, project_root: Option<&Path>, app: &mut App) {
        initialize_build_test_availability(project_root, &self.config, app);
        for kind in BuildTestKind::ALL {
            if matches!(app.build_test_state(kind), BuildTestState::Completed(result) if matches!(result.freshness(), BuildTestFreshness::Fresh))
            {
                self.set_baseline(
                    kind,
                    project_root.and_then(|root| {
                        BuildTestFreshnessBaseline::capture_with_exclusions(
                            root,
                            self.config.verify().excludes(),
                        )
                        .ok()
                    }),
                );
            }
        }
    }

    #[cfg(test)]
    pub(super) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    #[cfg(test)]
    pub(super) fn config(&self) -> &ProjectConfig {
        &self.config
    }

    fn clear_baseline(&mut self, kind: BuildTestKind) {
        match kind {
            BuildTestKind::BuildDebug => self.build_baseline = None,
            BuildTestKind::BuildRelease => self.release_baseline = None,
            BuildTestKind::Test => self.test_baseline = None,
        }
    }

    fn set_baseline(&mut self, kind: BuildTestKind, baseline: Option<BuildTestFreshnessBaseline>) {
        match kind {
            BuildTestKind::BuildDebug => self.build_baseline = baseline,
            BuildTestKind::BuildRelease => self.release_baseline = baseline,
            BuildTestKind::Test => self.test_baseline = baseline,
        }
    }
}

fn initialize_build_test_availability(
    project_root: Option<&Path>,
    config: &ProjectConfig,
    app: &mut App,
) {
    for kind in BuildTestKind::ALL {
        if project_root.is_some_and(|root| resolve_build_test_command(root, config, kind).is_some())
        {
            if matches!(app.build_test_state(kind), BuildTestState::Unavailable) {
                app.apply_build_test_state(kind, BuildTestState::NotRun);
            }
        } else {
            app.apply_build_test_state(kind, BuildTestState::Unavailable);
        }
    }
}

pub(super) fn start_manual_build_test(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &mut BuildTestRuntime,
    kind: BuildTestKind,
) -> bool {
    if runtime.active.is_some() {
        return false;
    }

    app.select_evidence_detail(kind);
    runtime.clear_baseline(kind);
    runtime.active_baseline = None;
    runtime.active_inputs_changed = false;
    let Some(root) = project_root else {
        app.apply_build_test_state(kind, BuildTestState::Unavailable);
        return true;
    };
    let Some(spec) = resolve_build_test_command(root, &runtime.config, kind) else {
        app.apply_build_test_state(kind, BuildTestState::Unavailable);
        return true;
    };

    runtime.active_baseline = BuildTestFreshnessBaseline::capture_with_exclusions(
        root,
        runtime.config.verify().excludes(),
    )
    .ok();
    match BuildTestExecution::start(spec) {
        Ok(execution) => {
            app.apply_build_test_state(kind, BuildTestState::Running(execution.run().clone()));
            runtime.active = Some(execution);
        }
        Err(error) => {
            runtime.active_baseline = None;
            app.apply_build_test_state(kind, BuildTestState::ExecutionError(error));
            if let Some(root) = project_root {
                let _ = save_build_test_state(root, kind, app.build_test_state(kind), None);
            }
        }
    }
    true
}

fn apply_build_test_completion(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &mut BuildTestRuntime,
    completion: BuildTestExecutionCompletion,
    inputs_changed: bool,
    started_baseline: Option<BuildTestFreshnessBaseline>,
) {
    match completion {
        BuildTestExecutionCompletion::Completed(result) => {
            let kind = result.kind();
            let (freshness, persisted_baseline) =
                project_root.map_or((BuildTestFreshness::Stale, None), |root| {
                    evaluate_completed_build_test_freshness_with_exclusions(
                        root,
                        runtime.config.verify().excludes(),
                        started_baseline.as_ref(),
                        inputs_changed,
                    )
                });
            let mut result = result;
            if matches!(freshness, BuildTestFreshness::Stale) {
                result.mark_stale();
            }
            app.apply_build_test_state(kind, BuildTestState::Completed(result));
            runtime.set_baseline(kind, persisted_baseline);
            if let (Some(root), Some(baseline)) = (
                project_root,
                match kind {
                    BuildTestKind::BuildDebug => runtime.build_baseline.as_ref(),
                    BuildTestKind::BuildRelease => runtime.release_baseline.as_ref(),
                    BuildTestKind::Test => runtime.test_baseline.as_ref(),
                },
            ) {
                let _ =
                    save_build_test_state(root, kind, app.build_test_state(kind), Some(baseline));
            } else if let Some(root) = project_root {
                let _ = save_build_test_state(root, kind, app.build_test_state(kind), None);
            }
        }
        BuildTestExecutionCompletion::ExecutionError(error) => {
            let kind = error.kind();
            runtime.clear_baseline(kind);
            runtime.active_baseline = None;
            runtime.active_inputs_changed = false;
            app.apply_build_test_state(kind, BuildTestState::ExecutionError(error));
            if let Some(root) = project_root {
                let _ = save_build_test_state(root, kind, app.build_test_state(kind), None);
            }
        }
    }
}

fn check_completed_build_test_freshness(
    project_root: &Path,
    app: &mut App,
    baseline: Option<&BuildTestFreshnessBaseline>,
    kind: BuildTestKind,
    exclusions: &[PathBuf],
) -> bool {
    let BuildTestState::Completed(mut result) = app.build_test_state(kind).clone() else {
        return false;
    };
    if matches!(result.freshness(), BuildTestFreshness::Stale) {
        return false;
    }
    let Some(baseline) = baseline else {
        return false;
    };
    if !matches!(
        baseline.check_with_exclusions(project_root, exclusions),
        Ok(BuildTestInputChange::Changed)
    ) {
        return false;
    }

    result.mark_stale();
    app.apply_build_test_state(kind, BuildTestState::Completed(result));
    true
}

pub(super) fn check_build_test_freshness(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &BuildTestRuntime,
) -> bool {
    let Some(project_root) = project_root else {
        return false;
    };

    check_completed_build_test_freshness(
        project_root,
        app,
        runtime.build_baseline.as_ref(),
        BuildTestKind::BuildDebug,
        runtime.config.verify().excludes(),
    ) | check_completed_build_test_freshness(
        project_root,
        app,
        runtime.release_baseline.as_ref(),
        BuildTestKind::BuildRelease,
        runtime.config.verify().excludes(),
    ) | check_completed_build_test_freshness(
        project_root,
        app,
        runtime.test_baseline.as_ref(),
        BuildTestKind::Test,
        runtime.config.verify().excludes(),
    )
}
pub(super) fn observe_active_build_test_inputs(
    project_root: Option<&Path>,
    runtime: &mut BuildTestRuntime,
) {
    if runtime.active_inputs_changed {
        return;
    }
    if let (Some(root), Some(baseline)) = (project_root, runtime.active_baseline.as_ref()) {
        runtime.active_inputs_changed = matches!(
            baseline.check_with_exclusions(root, runtime.config.verify().excludes()),
            Ok(BuildTestInputChange::Changed)
        );
    }
}

fn finish_build_test_execution(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &mut BuildTestRuntime,
    completion: BuildTestExecutionCompletion,
) {
    observe_active_build_test_inputs(project_root, runtime);
    let inputs_changed = runtime.active_inputs_changed;
    let started_baseline = runtime.active_baseline.take();
    runtime.active = None;
    runtime.active_inputs_changed = false;
    apply_build_test_completion(
        project_root,
        app,
        runtime,
        completion,
        inputs_changed,
        started_baseline,
    );
}

pub(super) fn poll_build_test_execution(
    project_root: Option<&Path>,
    app: &mut App,
    runtime: &mut BuildTestRuntime,
) -> bool {
    let Some(execution) = runtime.active.as_mut() else {
        return false;
    };

    let completion = execution.try_complete();

    match completion {
        Ok(None) => false,
        Ok(Some(completion)) => {
            finish_build_test_execution(project_root, app, runtime, completion);
            true
        }
        Err(error) => {
            let kind = error.kind();
            runtime.active = None;
            runtime.clear_baseline(kind);
            runtime.active_baseline = None;
            runtime.active_inputs_changed = false;
            app.apply_build_test_state(kind, BuildTestState::ExecutionError(error));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devscope::{
        config::load_project_config,
        progress::{
            BuildTestCommandSpec, BuildTestExecutionError, BuildTestOutcome, BuildTestResult,
            BuildTestRun, run_build_test,
        },
        project::ProjectSnapshot,
    };
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    static ID: AtomicUsize = AtomicUsize::new(0);
    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "devscope-verification-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn completed_result(kind: BuildTestKind, outcome: BuildTestOutcome) -> BuildTestResult {
        BuildTestResult::new(
            kind,
            outcome,
            BuildTestFreshness::Fresh,
            "cargo",
            if kind == BuildTestKind::BuildDebug {
                "cargo check"
            } else {
                "cargo test"
            },
            Some(if outcome == BuildTestOutcome::Passed {
                0
            } else {
                1
            }),
            Duration::from_millis(1),
            "completed",
            None,
        )
    }

    #[test]
    fn initializes_build_test_availability_from_the_project_root() {
        let cargo_root = temp_root();
        fs::write(cargo_root.join("Cargo.toml"), "[package]").unwrap();
        let mut cargo_app = App::new(ProjectSnapshot::unavailable());
        initialize_build_test_availability(
            Some(&cargo_root),
            &ProjectConfig::default(),
            &mut cargo_app,
        );
        assert_eq!(
            cargo_app.build_test_state(BuildTestKind::BuildDebug),
            &BuildTestState::NotRun
        );
        assert_eq!(
            cargo_app.build_test_state(BuildTestKind::Test),
            &BuildTestState::NotRun
        );

        let non_cargo_root = temp_root();
        let mut non_cargo_app = App::new(ProjectSnapshot::unavailable());
        initialize_build_test_availability(
            Some(&non_cargo_root),
            &ProjectConfig::default(),
            &mut non_cargo_app,
        );
        assert_eq!(
            non_cargo_app.build_test_state(BuildTestKind::BuildDebug),
            &BuildTestState::Unavailable
        );
        assert_eq!(
            non_cargo_app.build_test_state(BuildTestKind::Test),
            &BuildTestState::Unavailable
        );
        let _ = fs::remove_dir_all(cargo_root);
        let _ = fs::remove_dir_all(non_cargo_root);
    }

    #[test]
    fn initializes_build_test_availability_per_configured_kind() {
        for (contents, build_available, test_available) in [
            (
                "[verify.build]\nprogram = \"configured-build\"\n",
                true,
                false,
            ),
            (
                "[verify.test]\nprogram = \"configured-test\"\n",
                false,
                true,
            ),
            (
                "[verify.build]\nprogram = \"configured-build\"\n\n[verify.test]\nprogram = \"configured-test\"\n",
                true,
                true,
            ),
        ] {
            let root = temp_root();
            fs::create_dir_all(root.join(".devscope")).unwrap();
            fs::write(root.join(".devscope/config.toml"), contents).unwrap();
            let config = load_project_config(&root).unwrap();
            let mut app = App::new(ProjectSnapshot::unavailable());

            initialize_build_test_availability(Some(&root), &config, &mut app);

            assert_eq!(
                matches!(
                    app.build_test_state(BuildTestKind::BuildDebug),
                    BuildTestState::NotRun
                ),
                build_available
            );
            assert_eq!(
                matches!(
                    app.build_test_state(BuildTestKind::Test),
                    BuildTestState::NotRun
                ),
                test_available
            );
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn configured_manual_start_uses_the_resolved_command_spec() {
        let root = temp_root();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify.build]\nprogram = \"configured-build\"\nargs = [\"--focused\"]\n",
        )
        .unwrap();
        let config = load_project_config(&root).unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            config,
            ..Default::default()
        };

        assert!(start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::BuildDebug,
        ));
        let BuildTestState::Running(run) = app.build_test_state(BuildTestKind::BuildDebug) else {
            panic!("configured command should enter the running state");
        };
        assert_eq!(run.source_label(), "configured-build");
        assert_eq!(run.command_label(), "configured-build --focused");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configured_test_overrides_cargo_for_tui_manual_start() {
        let root = temp_root();
        fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify.test]\nprogram = \"configured-test\"\nargs = [\"--override\"]\n",
        )
        .unwrap();
        let config = load_project_config(&root).unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        initialize_build_test_availability(Some(&root), &config, &mut app);
        let mut runtime = BuildTestRuntime {
            config,
            ..Default::default()
        };

        assert!(start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Test,
        ));
        let BuildTestState::Running(run) = app.build_test_state(BuildTestKind::Test) else {
            panic!("configured command should override Cargo for Test");
        };
        assert_eq!(run.source_label(), "configured-test");
        assert_eq!(run.command_label(), "configured-test --override");
        assert_eq!(
            app.build_test_state(BuildTestKind::BuildDebug),
            &BuildTestState::NotRun
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_configured_executable_reaches_execution_error_with_resolved_labels() {
        let root = temp_root();
        fs::create_dir_all(root.join(".devscope")).unwrap();
        fs::write(
            root.join(".devscope/config.toml"),
            "[verify.test]\nprogram = \"definitely-not-installed-command\"\nargs = [\"--focused\"]\n",
        )
        .unwrap();
        let config = load_project_config(&root).unwrap();
        let spec = resolve_build_test_command(&root, &config, BuildTestKind::Test).unwrap();

        let BuildTestExecutionCompletion::ExecutionError(error) = run_build_test(spec) else {
            panic!("missing configured executable should be an execution error");
        };
        assert_eq!(error.kind(), BuildTestKind::Test);
        assert_eq!(error.source_label(), "definitely-not-installed-command");
        assert_eq!(
            error.command_label(),
            "definitely-not-installed-command --focused"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unavailable_manual_start_keeps_no_active_execution() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();

        assert!(start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::BuildDebug,
        ));
        assert!(runtime.active.is_none());
        assert_eq!(
            app.build_test_state(BuildTestKind::BuildDebug),
            &BuildTestState::Unavailable
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_a_second_manual_start_while_an_execution_is_active() {
        let root = temp_root();
        let active = BuildTestExecution::start(BuildTestCommandSpec::new(
            BuildTestKind::BuildDebug,
            "fixture",
            "missing fixture",
            root.join("missing-program"),
            Vec::new(),
            &root,
        ))
        .unwrap();
        let mut runtime = BuildTestRuntime {
            active: Some(active),
            ..Default::default()
        };
        let mut app = App::new(ProjectSnapshot::unavailable());

        assert!(!start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Test,
        ));
        assert!(runtime.active.is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn accepted_manual_starts_select_the_matching_evidence_detail() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();
        assert!(start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::BuildDebug
        ));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::BuildDebug));
        runtime.active = Some(
            BuildTestExecution::start(BuildTestCommandSpec::new(
                BuildTestKind::BuildDebug,
                "fixture",
                "missing",
                root.join("missing"),
                Vec::new(),
                &root,
            ))
            .unwrap(),
        );
        assert!(!start_manual_build_test(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestKind::Test
        ));
        assert_eq!(app.evidence_detail_kind(), Some(BuildTestKind::BuildDebug));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn profiles_stale_independently_and_only_rerun_target_becomes_fresh() {
        let root = temp_root();
        fs::write(root.join("source.rs"), "before").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();
        for kind in BuildTestKind::ALL {
            apply_build_test_completion(
                Some(&root),
                &mut app,
                &mut runtime,
                BuildTestExecutionCompletion::Completed(completed_result(
                    kind,
                    if kind == BuildTestKind::BuildRelease {
                        BuildTestOutcome::Failed
                    } else {
                        BuildTestOutcome::Passed
                    },
                )),
                false,
                Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            );
        }
        fs::write(root.join("source.rs"), "after").unwrap();
        assert!(check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        for kind in BuildTestKind::ALL {
            assert!(
                matches!(app.build_test_state(kind), BuildTestState::Completed(result) if result.kind() == kind && result.freshness() == BuildTestFreshness::Stale)
            );
        }
        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::BuildDebug,
                BuildTestOutcome::Passed,
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        let mut restarted = App::new(ProjectSnapshot::unavailable());
        crate::restore_tui_build_test_states(
            Some(&root),
            &ProjectConfig::default(),
            &mut restarted,
        );
        for kind in BuildTestKind::ALL {
            let BuildTestState::Completed(result) = restarted.build_test_state(kind) else {
                panic!()
            };
            assert_eq!(result.kind(), kind);
            assert_eq!(
                result.freshness(),
                if kind == BuildTestKind::BuildDebug {
                    BuildTestFreshness::Fresh
                } else {
                    BuildTestFreshness::Stale
                }
            );
            assert_eq!(
                result.outcome(),
                if kind == BuildTestKind::BuildRelease {
                    BuildTestOutcome::Failed
                } else {
                    BuildTestOutcome::Passed
                }
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn applies_completions_and_keeps_kind_baselines_independent() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();
        let build = completed_result(BuildTestKind::BuildDebug, BuildTestOutcome::Passed);
        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(build.clone()),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        assert_eq!(
            app.build_test_state(BuildTestKind::BuildDebug),
            &BuildTestState::Completed(build)
        );
        assert!(runtime.build_baseline.is_some());
        assert!(runtime.test_baseline.is_none());

        let test = completed_result(BuildTestKind::Test, BuildTestOutcome::Failed);
        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(test.clone()),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        assert_eq!(
            app.build_test_state(BuildTestKind::Test),
            &BuildTestState::Completed(test)
        );
        assert!(runtime.build_baseline.is_some());
        assert!(runtime.test_baseline.is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_change_marks_completed_evidence_stale_once() {
        let root = temp_root();
        let input = root.join("input.txt");
        fs::write(&input, "before").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();

        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::BuildDebug,
                BuildTestOutcome::Passed,
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));

        fs::write(input, "after").unwrap();
        assert!(check_build_test_freshness(Some(&root), &mut app, &runtime));
        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::BuildDebug)
        else {
            panic!("a completed Build result should remain completed");
        };
        assert_eq!(result.outcome(), BuildTestOutcome::Passed);
        assert_eq!(result.freshness(), BuildTestFreshness::Stale);
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::Test),
            BuildTestState::Unavailable
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn changed_inputs_during_a_run_mark_the_completed_result_stale() {
        let root = temp_root();
        let input = root.join("input.txt");
        fs::write(&input, "before").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            active_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        fs::write(input, "after changed").unwrap();
        finish_build_test_execution(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
            )),
        );

        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::Test) else {
            panic!("a completed Test result should remain completed");
        };
        assert_eq!(result.outcome(), BuildTestOutcome::Failed);
        assert_eq!(result.freshness(), BuildTestFreshness::Stale);
        assert!(runtime.test_baseline.is_none());

        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn final_active_input_check_keeps_unchanged_completion_fresh() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            active_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        finish_build_test_execution(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::BuildDebug,
                BuildTestOutcome::Passed,
            )),
        );

        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::BuildDebug)
        else {
            panic!("a completed Build result should remain completed");
        };
        assert_eq!(result.freshness(), BuildTestFreshness::Fresh);
        assert!(runtime.build_baseline.is_some());
        assert!(runtime.active_baseline.is_none());
        assert!(!runtime.active_inputs_changed);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn final_active_input_check_ignores_target_changes() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            active_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/output"), "generated change").unwrap();
        finish_build_test_execution(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::BuildDebug,
                BuildTestOutcome::Passed,
            )),
        );

        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::BuildDebug)
        else {
            panic!("a completed Build result should remain completed");
        };
        assert_eq!(result.freshness(), BuildTestFreshness::Fresh);

        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn freshness_checks_ignore_git_and_target_but_stale_completed_test_for_project_input() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();

        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::Test,
                BuildTestOutcome::Failed,
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join(".git/metadata"), "internal change").unwrap();
        fs::write(root.join("target/output"), "generated change").unwrap();
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));

        fs::write(root.join("README.md"), "relevant change").unwrap();
        assert!(check_build_test_freshness(Some(&root), &mut app, &runtime));
        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::Test) else {
            panic!("a completed Test result should remain completed");
        };
        assert_eq!(result.outcome(), BuildTestOutcome::Failed);
        assert_eq!(result.freshness(), BuildTestFreshness::Stale);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_start_monitor_ignores_excluded_changes_and_keeps_changed_flag_sticky() {
        let root = temp_root();
        let input = root.join("input.txt");
        fs::write(&input, "before").unwrap();
        let mut runtime = BuildTestRuntime {
            active_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join(".git/metadata"), "internal change").unwrap();
        fs::write(root.join("target/output"), "generated change").unwrap();
        observe_active_build_test_inputs(Some(&root), &mut runtime);
        assert!(!runtime.active_inputs_changed);

        fs::write(input, "after changed").unwrap();
        observe_active_build_test_inputs(Some(&root), &mut runtime);
        assert!(runtime.active_inputs_changed);
        fs::remove_file(root.join("input.txt")).unwrap();
        observe_active_build_test_inputs(Some(&root), &mut runtime);
        assert!(runtime.active_inputs_changed);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn freshness_check_errors_preserve_result_and_baseline() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "input").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime::default();
        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::Completed(completed_result(
                BuildTestKind::BuildDebug,
                BuildTestOutcome::Passed,
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );

        fs::remove_dir_all(&root).unwrap();
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        let BuildTestState::Completed(result) = app.build_test_state(BuildTestKind::BuildDebug)
        else {
            panic!("a completed Build result should remain completed");
        };
        assert_eq!(result.freshness(), BuildTestFreshness::Fresh);
        assert!(runtime.build_baseline.is_some());
    }
    #[test]
    fn freshness_checks_leave_non_completed_states_unchanged() {
        let root = temp_root();
        fs::write(root.join("input.txt"), "before").unwrap();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let runtime = BuildTestRuntime {
            build_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };
        fs::write(root.join("input.txt"), "after changed").unwrap();

        app.apply_build_test_state(BuildTestKind::BuildDebug, BuildTestState::NotRun);
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::BuildDebug),
            BuildTestState::NotRun
        ));

        app.apply_build_test_state(
            BuildTestKind::BuildDebug,
            BuildTestState::Running(BuildTestRun::new(
                BuildTestKind::BuildDebug,
                "cargo",
                "cargo check",
            )),
        );
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::BuildDebug),
            BuildTestState::Running(_)
        ));

        app.apply_build_test_state(
            BuildTestKind::BuildDebug,
            BuildTestState::ExecutionError(BuildTestExecutionError::new(
                BuildTestKind::BuildDebug,
                "cargo",
                "cargo check",
                "could not start",
            )),
        );
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::BuildDebug),
            BuildTestState::ExecutionError(_)
        ));

        app.apply_build_test_state(BuildTestKind::BuildDebug, BuildTestState::Unavailable);
        assert!(!check_build_test_freshness(Some(&root), &mut app, &runtime));
        assert!(matches!(
            app.build_test_state(BuildTestKind::BuildDebug),
            BuildTestState::Unavailable
        ));
        assert!(runtime.build_baseline.is_some());

        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn execution_errors_clear_only_the_matching_baseline() {
        let root = temp_root();
        let mut app = App::new(ProjectSnapshot::unavailable());
        let mut runtime = BuildTestRuntime {
            build_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            test_baseline: Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
            ..Default::default()
        };

        apply_build_test_completion(
            Some(&root),
            &mut app,
            &mut runtime,
            BuildTestExecutionCompletion::ExecutionError(BuildTestExecutionError::new(
                BuildTestKind::BuildDebug,
                "cargo",
                "cargo check",
                "worker disconnected",
            )),
            false,
            Some(BuildTestFreshnessBaseline::capture(&root).unwrap()),
        );
        assert!(matches!(
            app.build_test_state(BuildTestKind::BuildDebug),
            BuildTestState::ExecutionError(_)
        ));
        assert!(runtime.build_baseline.is_none());
        assert!(runtime.test_baseline.is_some());
        let _ = fs::remove_dir_all(root);
    }
}
