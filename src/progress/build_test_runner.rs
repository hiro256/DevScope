//! Non-blocking process execution for v0.3 Build/Test Evidence.
//!
//! Each execution owns one background worker and one completion channel. The caller
//! polls completion without waiting for the child process.

use std::{
    process::Command,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Instant,
};

use super::{
    BuildTestCommandSpec, BuildTestDiagnostic, BuildTestExecutionError, BuildTestFreshness,
    BuildTestOutcome, BuildTestResult, BuildTestRun,
};

/// A terminal result reported by a non-blocking Build/Test execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildTestExecutionCompletion {
    Completed(BuildTestResult),
    ExecutionError(BuildTestExecutionError),
}

/// A handle for one Build/Test process executing in a background worker.
pub struct BuildTestExecution {
    run: BuildTestRun,
    receiver: Option<Receiver<BuildTestExecutionCompletion>>,
}

impl BuildTestExecution {
    /// Starts a Build/Test process on a background worker thread.
    pub fn start(spec: BuildTestCommandSpec) -> Result<Self, BuildTestExecutionError> {
        let run = BuildTestRun::from(&spec);
        let worker_run = run.clone();
        let (sender, receiver) = mpsc::channel();

        thread::Builder::new()
            .name("devscope-build-test".into())
            .spawn(move || {
                let completion = execute(spec);
                let _ = sender.send(completion);
            })
            .map_err(|source| {
                execution_error(
                    &worker_run,
                    format!("failed to start Build/Test worker: {source}"),
                )
            })?;

        Ok(Self {
            run,
            receiver: Some(receiver),
        })
    }

    /// Returns metadata for the process currently represented by this handle.
    pub fn run(&self) -> &BuildTestRun {
        &self.run
    }

    /// Polls process completion without blocking the caller thread.
    ///
    /// A completion is returned at most once. Further polls after completion return
    /// `Ok(None)` rather than a channel-disconnect error.
    pub fn try_complete(
        &mut self,
    ) -> Result<Option<BuildTestExecutionCompletion>, BuildTestExecutionError> {
        let Some(receiver) = &self.receiver else {
            return Ok(None);
        };

        match receiver.try_recv() {
            Ok(completion) => {
                self.receiver = None;
                Ok(Some(completion))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                Err(execution_error(
                    &self.run,
                    "Build/Test worker exited without reporting a completion",
                ))
            }
        }
    }
}

fn execute(spec: BuildTestCommandSpec) -> BuildTestExecutionCompletion {
    let started = Instant::now();
    let output = Command::new(spec.program())
        .args(spec.arguments())
        .current_dir(spec.working_directory())
        .output();

    match output {
        Ok(output) => {
            let outcome = if output.status.success() {
                BuildTestOutcome::Passed
            } else {
                BuildTestOutcome::Failed
            };
            let summary = format!(
                "{} {}",
                spec.command_label(),
                match outcome {
                    BuildTestOutcome::Passed => "passed",
                    BuildTestOutcome::Failed => "failed",
                }
            );
            BuildTestExecutionCompletion::Completed(BuildTestResult::new(
                spec.kind(),
                outcome,
                BuildTestFreshness::Fresh,
                spec.source_label(),
                spec.command_label(),
                output.status.code(),
                started.elapsed(),
                summary,
                diagnostic_from_output(&output.stdout, &output.stderr),
            ))
        }
        Err(source) => BuildTestExecutionCompletion::ExecutionError(BuildTestExecutionError::new(
            spec.kind(),
            spec.source_label(),
            spec.command_label(),
            format!("failed to execute Build/Test process: {source}"),
        )),
    }
}

fn diagnostic_from_output(stdout: &[u8], stderr: &[u8]) -> Option<BuildTestDiagnostic> {
    if stdout.is_empty() && stderr.is_empty() {
        return None;
    }

    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    Some(BuildTestDiagnostic::new(format!("{stdout}{stderr}")))
}

fn execution_error(run: &BuildTestRun, message: impl Into<String>) -> BuildTestExecutionError {
    BuildTestExecutionError::new(run.kind(), run.source_label(), run.command_label(), message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::BuildTestKind;
    use std::{
        env,
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    static ID: AtomicUsize = AtomicUsize::new(0);

    const CHILD_PREFIX: &str = "progress::build_test_runner::tests::child_";

    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "devscope-build-test-runner-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn child_spec(root: &Path, child: &str) -> BuildTestCommandSpec {
        BuildTestCommandSpec::new(
            BuildTestKind::Test,
            "fixture",
            format!("fixture {child}"),
            env::current_exe().unwrap().into_os_string(),
            vec![
                OsString::from("--exact"),
                OsString::from(format!("{CHILD_PREFIX}{child}")),
                OsString::from("--ignored"),
                OsString::from("--nocapture"),
            ],
            root,
        )
    }

    fn wait_for_completion(execution: &mut BuildTestExecution) -> BuildTestExecutionCompletion {
        for _ in 0..200 {
            if let Some(completion) = execution.try_complete().unwrap() {
                return completion;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("fixture process did not complete");
    }

    #[test]
    fn starts_non_blocking_and_exposes_running_metadata() {
        let project = TempProject::new();
        let mut execution = BuildTestExecution::start(child_spec(&project.0, "delayed")).unwrap();

        assert_eq!(execution.run().kind(), BuildTestKind::Test);
        assert_eq!(execution.run().source_label(), "fixture");
        assert_eq!(execution.run().command_label(), "fixture delayed");
        assert_eq!(execution.try_complete().unwrap(), None);

        let _ = wait_for_completion(&mut execution);
        assert_eq!(execution.try_complete().unwrap(), None);
    }

    #[test]
    fn normalizes_successful_completion_as_a_fresh_result() {
        let project = TempProject::new();
        let mut execution = BuildTestExecution::start(child_spec(&project.0, "success")).unwrap();

        let BuildTestExecutionCompletion::Completed(result) = wait_for_completion(&mut execution)
        else {
            panic!("successful fixture should complete");
        };
        assert_eq!(result.outcome(), BuildTestOutcome::Passed);
        assert_eq!(result.freshness(), BuildTestFreshness::Fresh);
        assert_eq!(result.exit_code(), Some(0));
        assert!(result.duration() > Duration::ZERO);
        assert_eq!(result.summary(), "fixture success passed");
    }

    #[test]
    fn normalizes_nonzero_exit_as_failed_completion() {
        let project = TempProject::new();
        let mut execution = BuildTestExecution::start(child_spec(&project.0, "failure")).unwrap();

        let BuildTestExecutionCompletion::Completed(result) = wait_for_completion(&mut execution)
        else {
            panic!("non-zero fixture should be a completed result");
        };
        assert_eq!(result.outcome(), BuildTestOutcome::Failed);
        assert_eq!(result.freshness(), BuildTestFreshness::Fresh);
        assert_eq!(result.exit_code(), Some(7));
        assert_eq!(result.summary(), "fixture failure failed");
    }

    #[test]
    fn reports_spawn_failure_as_an_execution_error() {
        let project = TempProject::new();
        let spec = BuildTestCommandSpec::new(
            BuildTestKind::Build,
            "fixture",
            "missing fixture",
            project.0.join("definitely-missing-program"),
            Vec::new(),
            &project.0,
        );
        let mut execution = BuildTestExecution::start(spec).unwrap();

        let BuildTestExecutionCompletion::ExecutionError(error) =
            wait_for_completion(&mut execution)
        else {
            panic!("missing executable should be an execution error");
        };
        assert_eq!(error.kind(), BuildTestKind::Build);
        assert_eq!(error.source_label(), "fixture");
        assert_eq!(error.command_label(), "missing fixture");
        assert!(
            error
                .message()
                .contains("failed to execute Build/Test process")
        );
    }

    #[test]
    fn captures_stdout_stderr_and_a_bounded_diagnostic_tail() {
        let project = TempProject::new();
        let mut execution =
            BuildTestExecution::start(child_spec(&project.0, "large_output")).unwrap();

        let BuildTestExecutionCompletion::Completed(result) = wait_for_completion(&mut execution)
        else {
            panic!("large-output fixture should complete");
        };
        let diagnostic = result.diagnostic().expect("fixture should write output");
        assert!(diagnostic.as_str().contains("fixture stderr tail"));
        assert!(diagnostic.as_str().chars().count() <= super::super::MAX_DIAGNOSTIC_CHARS);
    }

    #[test]
    fn captures_stdout_and_stderr_in_the_diagnostic() {
        let project = TempProject::new();
        let mut execution = BuildTestExecution::start(child_spec(&project.0, "output")).unwrap();

        let BuildTestExecutionCompletion::Completed(result) = wait_for_completion(&mut execution)
        else {
            panic!("output fixture should complete");
        };
        let diagnostic = result.diagnostic().expect("fixture should write output");
        assert!(diagnostic.as_str().contains("fixture stdout"));
        assert!(diagnostic.as_str().contains("fixture stderr"));
    }

    #[test]
    fn applies_the_spec_working_directory_to_the_child() {
        let project = TempProject::new();
        let mut execution =
            BuildTestExecution::start(child_spec(&project.0, "working_directory")).unwrap();

        let BuildTestExecutionCompletion::Completed(result) = wait_for_completion(&mut execution)
        else {
            panic!("working-directory fixture should complete");
        };
        let diagnostic = result
            .diagnostic()
            .expect("fixture should print its directory");
        assert!(
            diagnostic
                .as_str()
                .contains(&project.0.display().to_string())
        );
    }

    #[test]
    fn executes_program_and_arguments_without_parsing_display_metadata() {
        let project = TempProject::new();
        let spec = BuildTestCommandSpec::new(
            BuildTestKind::Build,
            "display-only source",
            "this is not an executable command",
            env::current_exe().unwrap().into_os_string(),
            vec![
                OsString::from("--exact"),
                OsString::from(format!("{CHILD_PREFIX}child_success")),
                OsString::from("--ignored"),
                OsString::from("--nocapture"),
            ],
            &project.0,
        );
        let mut execution = BuildTestExecution::start(spec).unwrap();

        let BuildTestExecutionCompletion::Completed(result) = wait_for_completion(&mut execution)
        else {
            panic!("fixture program should complete successfully");
        };
        assert_eq!(result.kind(), BuildTestKind::Build);
        assert_eq!(result.outcome(), BuildTestOutcome::Passed);
        assert_eq!(result.source_label(), "display-only source");
        assert_eq!(result.command_label(), "this is not an executable command");
        assert_eq!(result.summary(), "this is not an executable command passed");
    }
    #[test]
    #[ignore]
    fn child_delayed() {
        thread::sleep(Duration::from_millis(300));
        println!("fixture delayed");
    }

    #[test]
    #[ignore]
    fn child_success() {
        println!("fixture success");
    }

    #[test]
    #[ignore]
    fn child_failure() {
        eprintln!("fixture failure");
        std::process::exit(7);
    }

    #[test]
    #[ignore]
    fn child_output() {
        println!("fixture stdout");
        eprintln!("fixture stderr");
    }

    #[test]
    #[ignore]
    fn child_large_output() {
        println!(
            "fixture stdout {}",
            "x".repeat(super::super::MAX_DIAGNOSTIC_CHARS + 100)
        );
        eprintln!("fixture stderr tail");
    }

    #[test]
    #[ignore]
    fn child_working_directory() {
        println!("{}", env::current_dir().unwrap().display());
    }
}
