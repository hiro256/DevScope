//! Process-based Build/Test result and state models for v0.3.
//!
//! These types describe the initial Cargo Build/Test Evidence shape. They are not
//! a generic Evidence API or a contract for future Evidence source shapes.

use std::time::Duration;

/// The maximum number of Unicode scalar values retained for diagnostic output.
pub const MAX_DIAGNOSTIC_CHARS: usize = 4096;

/// A verification category in the v0.3 Build/Test model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTestKind {
    Build,
    Test,
}

/// The outcome of a completed verification process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTestOutcome {
    Passed,
    Failed,
}

/// Whether a completed result still corresponds to the observed project state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTestFreshness {
    Fresh,
    Stale,
}

/// A display-oriented status derived from a Build/Test lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTestStatus {
    Unavailable,
    NotRun,
    Running,
    Passed,
    Failed,
    Stale,
    ExecutionError,
}

/// Bounded diagnostic output retained from the tail of a completed process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTestDiagnostic(String);

impl BuildTestDiagnostic {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let char_count = value.chars().count();
        if char_count <= MAX_DIAGNOSTIC_CHARS {
            return Self(value);
        }

        Self(
            value
                .chars()
                .skip(char_count - MAX_DIAGNOSTIC_CHARS)
                .collect(),
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Metadata for a verification process that has started but not completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTestRun {
    kind: BuildTestKind,
    source_label: String,
    command_label: String,
}

impl BuildTestRun {
    pub fn new(
        kind: BuildTestKind,
        source_label: impl Into<String>,
        command_label: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            source_label: source_label.into(),
            command_label: command_label.into(),
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
}

/// A completed process-based Build/Test result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTestResult {
    kind: BuildTestKind,
    outcome: BuildTestOutcome,
    freshness: BuildTestFreshness,
    source_label: String,
    command_label: String,
    exit_code: Option<i32>,
    duration: Duration,
    summary: String,
    diagnostic: Option<BuildTestDiagnostic>,
}

impl BuildTestResult {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: BuildTestKind,
        outcome: BuildTestOutcome,
        freshness: BuildTestFreshness,
        source_label: impl Into<String>,
        command_label: impl Into<String>,
        exit_code: Option<i32>,
        duration: Duration,
        summary: impl Into<String>,
        diagnostic: Option<BuildTestDiagnostic>,
    ) -> Self {
        Self {
            kind,
            outcome,
            freshness,
            source_label: source_label.into(),
            command_label: command_label.into(),
            exit_code,
            duration,
            summary: summary.into(),
            diagnostic,
        }
    }

    pub const fn kind(&self) -> BuildTestKind {
        self.kind
    }

    pub const fn outcome(&self) -> BuildTestOutcome {
        self.outcome
    }

    pub const fn freshness(&self) -> BuildTestFreshness {
        self.freshness
    }

    /// Marks this completed result stale while preserving its observed outcome.
    pub fn mark_stale(&mut self) {
        self.freshness = BuildTestFreshness::Stale;
    }

    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    pub fn command_label(&self) -> &str {
        &self.command_label
    }

    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub const fn duration(&self) -> Duration {
        self.duration
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn diagnostic(&self) -> Option<&BuildTestDiagnostic> {
        self.diagnostic.as_ref()
    }
}

/// A failure to start or observe a Build/Test process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTestExecutionError {
    kind: BuildTestKind,
    source_label: String,
    command_label: String,
    message: String,
}

impl BuildTestExecutionError {
    pub fn new(
        kind: BuildTestKind,
        source_label: impl Into<String>,
        command_label: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            source_label: source_label.into(),
            command_label: command_label.into(),
            message: message.into(),
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

    pub fn message(&self) -> &str {
        &self.message
    }
}

/// The lifecycle state of one process-based Build/Test verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildTestState {
    Unavailable,
    NotRun,
    Running(BuildTestRun),
    Completed(BuildTestResult),
    ExecutionError(BuildTestExecutionError),
}

impl BuildTestState {
    pub fn status(&self) -> BuildTestStatus {
        match self {
            Self::Unavailable => BuildTestStatus::Unavailable,
            Self::NotRun => BuildTestStatus::NotRun,
            Self::Running(_) => BuildTestStatus::Running,
            Self::Completed(result) => match result.freshness() {
                BuildTestFreshness::Stale => BuildTestStatus::Stale,
                BuildTestFreshness::Fresh => match result.outcome() {
                    BuildTestOutcome::Passed => BuildTestStatus::Passed,
                    BuildTestOutcome::Failed => BuildTestStatus::Failed,
                },
            },
            Self::ExecutionError(_) => BuildTestStatus::ExecutionError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(outcome: BuildTestOutcome, freshness: BuildTestFreshness) -> BuildTestResult {
        BuildTestResult::new(
            BuildTestKind::Test,
            outcome,
            freshness,
            "cargo",
            "cargo test",
            Some(0),
            Duration::from_millis(2400),
            "142 tests passed",
            Some(BuildTestDiagnostic::new("diagnostic")),
        )
    }

    #[test]
    fn keeps_build_and_test_kinds_distinct() {
        assert_ne!(BuildTestKind::Build, BuildTestKind::Test);
    }

    #[test]
    fn derives_fresh_completed_statuses() {
        assert_eq!(
            BuildTestState::Completed(result(BuildTestOutcome::Passed, BuildTestFreshness::Fresh))
                .status(),
            BuildTestStatus::Passed
        );
        assert_eq!(
            BuildTestState::Completed(result(BuildTestOutcome::Failed, BuildTestFreshness::Fresh))
                .status(),
            BuildTestStatus::Failed
        );
    }

    #[test]
    fn stale_status_preserves_completed_outcome() {
        for outcome in [BuildTestOutcome::Passed, BuildTestOutcome::Failed] {
            let result = result(outcome, BuildTestFreshness::Stale);
            let state = BuildTestState::Completed(result.clone());
            assert_eq!(state.status(), BuildTestStatus::Stale);
            assert_eq!(result.outcome(), outcome);
        }
    }

    #[test]
    fn derives_running_not_run_and_unavailable_statuses() {
        let running = BuildTestState::Running(BuildTestRun::new(
            BuildTestKind::Build,
            "cargo",
            "cargo check",
        ));
        assert_eq!(running.status(), BuildTestStatus::Running);
        assert_eq!(BuildTestState::NotRun.status(), BuildTestStatus::NotRun);
        assert_eq!(
            BuildTestState::Unavailable.status(),
            BuildTestStatus::Unavailable
        );
    }

    #[test]
    fn distinguishes_execution_error_from_failed_result() {
        let error = BuildTestExecutionError::new(
            BuildTestKind::Test,
            "cargo",
            "cargo test",
            "cargo executable was not found",
        );
        let state = BuildTestState::ExecutionError(error.clone());
        assert_eq!(state.status(), BuildTestStatus::ExecutionError);
        assert_ne!(state.status(), BuildTestStatus::Failed);
        assert_eq!(error.kind(), BuildTestKind::Test);
        assert_eq!(error.source_label(), "cargo");
        assert_eq!(error.command_label(), "cargo test");
        assert_eq!(error.message(), "cargo executable was not found");
    }

    #[test]
    fn retains_result_and_run_fields() {
        let run = BuildTestRun::new(BuildTestKind::Test, "cargo", "cargo test");
        assert_eq!(run.kind(), BuildTestKind::Test);
        assert_eq!(run.source_label(), "cargo");
        assert_eq!(run.command_label(), "cargo test");

        let result = result(BuildTestOutcome::Passed, BuildTestFreshness::Fresh);
        assert_eq!(result.kind(), BuildTestKind::Test);
        assert_eq!(result.source_label(), "cargo");
        assert_eq!(result.command_label(), "cargo test");
        assert_eq!(result.exit_code(), Some(0));
        assert_eq!(result.duration(), Duration::from_millis(2400));
        assert_eq!(result.summary(), "142 tests passed");
        assert_eq!(result.diagnostic().unwrap().as_str(), "diagnostic");
    }

    #[test]
    fn marks_passed_and_failed_results_stale_without_changing_their_outcomes() {
        for outcome in [BuildTestOutcome::Passed, BuildTestOutcome::Failed] {
            let mut completed = result(outcome, BuildTestFreshness::Fresh);
            completed.mark_stale();
            completed.mark_stale();

            assert_eq!(completed.freshness(), BuildTestFreshness::Stale);
            assert_eq!(completed.outcome(), outcome);
        }
    }

    #[test]
    fn retains_a_unicode_safe_diagnostic_tail() {
        let prefix = "あ".repeat(MAX_DIAGNOSTIC_CHARS + 10);
        let diagnostic = BuildTestDiagnostic::new(format!("{prefix}最後の診断"));
        assert_eq!(diagnostic.as_str().chars().count(), MAX_DIAGNOSTIC_CHARS);
        assert!(diagnostic.as_str().ends_with("最後の診断"));
        assert!(std::str::from_utf8(diagnostic.as_str().as_bytes()).is_ok());
    }
}
