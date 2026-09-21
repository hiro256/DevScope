use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Duration,
};

use crate::change::{WorktreeScanDiagnostics, diagnose_worktree_subtree_with_exclusions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GitChangeCounts {
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
}

impl GitChangeCounts {
    pub const fn unavailable() -> Self {
        Self {
            additions: None,
            deletions: None,
        }
    }

    fn combined_with(self, other: Self) -> Self {
        match (
            self.additions,
            self.deletions,
            other.additions,
            other.deletions,
        ) {
            (Some(additions), Some(deletions), Some(other_additions), Some(other_deletions)) => {
                Self {
                    additions: additions.checked_add(other_additions),
                    deletions: deletions.checked_add(other_deletions),
                }
            }
            _ => Self::unavailable(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChangedFile {
    pub path: PathBuf,
    pub status: GitFileStatus,
    pub changes: GitChangeCounts,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiffText {
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitFileDiff {
    Available {
        unstaged: Option<GitDiffText>,
        staged: Option<GitDiffText>,
    },
    Unavailable(GitFileDiffUnavailable),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileDiffUnavailable {
    Untracked,
    Renamed,
    NoContent,
    Error,
}

const MAX_FILE_DIFF_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub id: String,
    pub summary: String,
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GitActivity {
    pub changed_files: Vec<GitChangedFile>,
    pub recent_commits: Vec<GitCommit>,
}
impl GitActivity {
    pub fn changed_file_count(&self) -> usize {
        self.changed_files.len()
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum GitActivityError {
    NotRepository,
    GitUnavailable,
    CommandFailed(String),
    InvalidOutput,
}
impl fmt::Display for GitActivityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for GitActivityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityExcludeCandidateStatus {
    SafeCandidate,
    ReviewRequired,
    NotCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityExcludeCandidateReason {
    GitIgnored,
    NotGitIgnored,
    ContainsTrackedFiles,
    DevScopeLocalState,
    GeneratedOutputNameHint,
    GitObservationUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityExcludeCandidateAssessment {
    pub path: PathBuf,
    pub visited_entries: usize,
    pub duration: Duration,
    pub git_ignored: Option<bool>,
    pub contains_tracked_files: Option<bool>,
    pub status: ActivityExcludeCandidateStatus,
    pub reasons: Vec<ActivityExcludeCandidateReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityExcludeProposalSource {
    RootDiagnostic,
    DrillDown { parent: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityExcludeProposal {
    pub path: PathBuf,
    pub source: ActivityExcludeProposalSource,
    pub visited_entries: usize,
    pub duration: Duration,
    pub reasons: Vec<ActivityExcludeCandidateReason>,
}

/// Converts assessed, safe worktree subtrees into read-only Activity exclusion proposals.
///
/// This on-demand helper does not change Config or participate in polling. It can inspect one
/// level below a reviewed root directory, while retaining Git assessment as the safety boundary.
pub fn propose_activity_exclusions(
    root: &Path,
    diagnostics: &WorktreeScanDiagnostics,
    current_excludes: &[PathBuf],
    limit: usize,
) -> Vec<ActivityExcludeProposal> {
    let root_assessments = assess_activity_exclude_candidates(root, diagnostics, limit);
    let mut proposals = BTreeMap::new();

    for assessment in &root_assessments {
        add_activity_exclude_proposal(
            &mut proposals,
            root,
            assessment,
            ActivityExcludeProposalSource::RootDiagnostic,
            current_excludes,
        );
    }

    for assessment in root_assessments.iter().filter(|assessment| {
        assessment.status == ActivityExcludeCandidateStatus::ReviewRequired
            && assessment.path.is_dir()
    }) {
        let Some(parent) = project_relative_activity_path(root, &assessment.path) else {
            continue;
        };
        if activity_path_is_excluded(current_excludes, &parent) {
            continue;
        }
        let Ok(children) =
            diagnose_worktree_subtree_with_exclusions(root, &assessment.path, current_excludes)
        else {
            continue;
        };
        for child in assess_activity_exclude_candidates(root, &children, limit) {
            add_activity_exclude_proposal(
                &mut proposals,
                root,
                &child,
                ActivityExcludeProposalSource::DrillDown {
                    parent: parent.clone(),
                },
                current_excludes,
            );
        }
    }

    let mut proposals = proposals.into_values().collect::<Vec<_>>();
    proposals.sort_by(|left, right| {
        right
            .duration
            .cmp(&left.duration)
            .then_with(|| right.visited_entries.cmp(&left.visited_entries))
            .then_with(|| left.path.cmp(&right.path))
    });
    proposals
}

fn add_activity_exclude_proposal(
    proposals: &mut BTreeMap<PathBuf, ActivityExcludeProposal>,
    root: &Path,
    assessment: &ActivityExcludeCandidateAssessment,
    source: ActivityExcludeProposalSource,
    current_excludes: &[PathBuf],
) {
    if assessment.status != ActivityExcludeCandidateStatus::SafeCandidate {
        return;
    }
    let Some(path) = project_relative_activity_path(root, &assessment.path) else {
        return;
    };
    if activity_path_is_excluded(current_excludes, &path) {
        return;
    }
    proposals
        .entry(path.clone())
        .or_insert(ActivityExcludeProposal {
            path,
            source,
            visited_entries: assessment.visited_entries,
            duration: assessment.duration,
            reasons: assessment.reasons.clone(),
        });
}

fn project_relative_activity_path(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(root).ok()?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return None;
    }
    Some(relative.to_path_buf())
}

fn activity_path_is_excluded(excludes: &[PathBuf], candidate: &Path) -> bool {
    excludes
        .iter()
        .any(|excluded| candidate == excluded || candidate.starts_with(excluded))
}
/// Assesses already-collected slow-scan subtrees for possible Activity exclusions.
///
/// This is a read-only, on-demand assessment. It does not modify configuration and is
/// intentionally not part of the normal worktree scan path.
pub fn assess_activity_exclude_candidates(
    root: &Path,
    diagnostics: &WorktreeScanDiagnostics,
    limit: usize,
) -> Vec<ActivityExcludeCandidateAssessment> {
    let mut subtrees = diagnostics.subtrees.clone();
    subtrees.sort_by(|left, right| {
        right
            .duration
            .cmp(&left.duration)
            .then_with(|| right.visited_entries.cmp(&left.visited_entries))
            .then_with(|| left.path.cmp(&right.path))
    });
    subtrees
        .into_iter()
        .take(limit)
        .map(|subtree| {
            assess_activity_exclude_candidate(
                root,
                subtree.path,
                subtree.visited_entries,
                subtree.duration,
            )
        })
        .collect()
}

fn assess_activity_exclude_candidate(
    root: &Path,
    path: PathBuf,
    visited_entries: usize,
    duration: Duration,
) -> ActivityExcludeCandidateAssessment {
    let relative = path.strip_prefix(root).ok();
    let git_path = relative.and_then(|path| git_path_for_subtree(root, path));
    let git_ignored = git_path
        .as_deref()
        .and_then(|path| git_path_is_ignored(root, path));
    let contains_tracked_files = git_path
        .as_deref()
        .and_then(|path| git_path_contains_tracked_files(root, path));
    let is_devscope_local_state = relative.is_some_and(|path| path == Path::new(".devscope"));
    let has_generated_output_name_hint = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_generated_output_name_hint);

    let mut reasons = Vec::new();
    match git_ignored {
        Some(true) => reasons.push(ActivityExcludeCandidateReason::GitIgnored),
        Some(false) => reasons.push(ActivityExcludeCandidateReason::NotGitIgnored),
        None => reasons.push(ActivityExcludeCandidateReason::GitObservationUnavailable),
    }
    match contains_tracked_files {
        Some(true) => reasons.push(ActivityExcludeCandidateReason::ContainsTrackedFiles),
        Some(false) => {}
        None if !reasons.contains(&ActivityExcludeCandidateReason::GitObservationUnavailable) => {
            reasons.push(ActivityExcludeCandidateReason::GitObservationUnavailable)
        }
        None => {}
    }
    if is_devscope_local_state {
        reasons.push(ActivityExcludeCandidateReason::DevScopeLocalState);
    }
    if has_generated_output_name_hint {
        reasons.push(ActivityExcludeCandidateReason::GeneratedOutputNameHint);
    }

    let status = if is_devscope_local_state
        || git_ignored.is_none()
        || contains_tracked_files.is_none()
        || (git_ignored == Some(true) && contains_tracked_files == Some(true))
    {
        ActivityExcludeCandidateStatus::ReviewRequired
    } else if contains_tracked_files == Some(true) {
        ActivityExcludeCandidateStatus::NotCandidate
    } else if git_ignored == Some(true) {
        ActivityExcludeCandidateStatus::SafeCandidate
    } else {
        ActivityExcludeCandidateStatus::ReviewRequired
    };

    ActivityExcludeCandidateAssessment {
        path,
        visited_entries,
        duration,
        git_ignored,
        contains_tracked_files,
        status,
        reasons,
    }
}

fn git_path_for_subtree(root: &Path, relative: &Path) -> Option<String> {
    let mut path = relative.to_str()?.replace('\\', "/");
    if root.join(relative).is_dir() {
        path.push('/');
    }
    Some(path)
}

fn git_path_is_ignored(root: &Path, path: &str) -> Option<bool> {
    let output = run_git(root, ["check-ignore", "--no-index", "-q", "--", path]).ok()?;
    if output.status.success() {
        Some(true)
    } else if output.status.code() == Some(1) {
        Some(false)
    } else {
        None
    }
}

fn git_path_contains_tracked_files(root: &Path, path: &str) -> Option<bool> {
    let output = run_git(root, ["ls-files", "-c", "-z", "--", path]).ok()?;
    output.status.success().then_some(!output.stdout.is_empty())
}

fn is_generated_output_name_hint(name: &str) -> bool {
    [
        "target",
        "bin",
        "obj",
        "node_modules",
        "dist",
        "build",
        ".generated",
        ".venv",
        "venv",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

pub fn is_git_repository(root: &Path) -> Result<bool, GitActivityError> {
    let output = run_git(root, ["rev-parse", "--is-inside-work-tree"])?;
    if output.status.success() {
        return Ok(String::from_utf8(output.stdout)
            .map_err(|_| GitActivityError::InvalidOutput)?
            .trim()
            == "true");
    }
    if output.status.code() == Some(128) {
        return Ok(false);
    }
    Err(command_error(&output))
}
pub fn collect_git_activity(root: &Path, limit: usize) -> Result<GitActivity, GitActivityError> {
    if !is_git_repository(root)? {
        return Err(GitActivityError::NotRepository);
    }
    let status = run_success(root, ["status", "--porcelain=v1", "-z"])?;
    let change_counts = collect_change_counts(root)?;
    let changed_files = parse_status(&status, &change_counts)?;
    let head = run_git(root, ["rev-parse", "--verify", "HEAD"])?;
    let recent_commits = if head.status.success() {
        parse_commits(&run_success(
            root,
            ["log", &format!("-n{limit}"), "--format=%h%x1f%s"],
        )?)?
    } else if head.status.code() == Some(128) {
        Vec::new()
    } else {
        return Err(command_error(&head));
    };
    Ok(GitActivity {
        changed_files,
        recent_commits,
    })
}
pub fn collect_git_file_diff(
    root: &Path,
    path: &Path,
    status: &GitFileStatus,
) -> Result<GitFileDiff, GitActivityError> {
    if matches!(status, GitFileStatus::Renamed) {
        return Ok(GitFileDiff::Unavailable(GitFileDiffUnavailable::Renamed));
    }
    let path = path.to_str().ok_or(GitActivityError::InvalidOutput)?;
    let unstaged = run_limited_diff(root, ["diff", "--no-ext-diff", "--no-color", "--", path])?;
    let staged = run_limited_diff(
        root,
        [
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-color",
            "--",
            path,
        ],
    )?;
    if unstaged.is_none() && staged.is_none() {
        let unavailable = if matches!(status, GitFileStatus::Added) {
            GitFileDiffUnavailable::Untracked
        } else {
            GitFileDiffUnavailable::NoContent
        };
        return Ok(GitFileDiff::Unavailable(unavailable));
    }
    Ok(GitFileDiff::Available { unstaged, staged })
}

fn run_limited_diff<'a>(
    root: &Path,
    args: impl IntoIterator<Item = &'a str>,
) -> Result<Option<GitDiffText>, GitActivityError> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                GitActivityError::GitUnavailable
            } else {
                GitActivityError::CommandFailed(error.to_string())
            }
        })?;
    let mut stdout = child.stdout.take().ok_or(GitActivityError::InvalidOutput)?;
    let mut bytes = Vec::new();
    stdout
        .by_ref()
        .take((MAX_FILE_DIFF_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| GitActivityError::CommandFailed(error.to_string()))?;
    let truncated = bytes.len() > MAX_FILE_DIFF_BYTES;
    if truncated {
        bytes.truncate(MAX_FILE_DIFF_BYTES);
        let _ = child.kill();
    }
    let status = child
        .wait()
        .map_err(|error| GitActivityError::CommandFailed(error.to_string()))?;
    if !truncated && !status.success() {
        return Err(GitActivityError::CommandFailed("git diff failed".into()));
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(GitDiffText {
        text: String::from_utf8(bytes).map_err(|_| GitActivityError::InvalidOutput)?,
        truncated,
    }))
}
fn parse_status(
    bytes: &[u8],
    change_counts: &BTreeMap<PathBuf, GitChangeCounts>,
) -> Result<Vec<GitChangedFile>, GitActivityError> {
    let mut entries = BTreeMap::new();
    let mut parts = bytes.split(|b| *b == 0);
    while let Some(record) = parts.next() {
        if record.is_empty() {
            continue;
        }
        if record.len() < 4 {
            return Err(GitActivityError::InvalidOutput);
        }
        let code = &record[..2];
        let path =
            String::from_utf8(record[3..].to_vec()).map_err(|_| GitActivityError::InvalidOutput)?;
        let status = if code.contains(&b'D') {
            GitFileStatus::Deleted
        } else if code.contains(&b'R') {
            let _ = parts.next();
            GitFileStatus::Renamed
        } else if code.contains(&b'A') || code == b"??" {
            GitFileStatus::Added
        } else {
            GitFileStatus::Modified
        };
        entries.insert(
            PathBuf::from(&path),
            GitChangedFile {
                changes: change_counts
                    .get(Path::new(&path))
                    .copied()
                    .unwrap_or_default(),
                path: path.into(),
                status,
            },
        );
    }
    Ok(entries.into_values().collect())
}
fn collect_change_counts(
    root: &Path,
) -> Result<BTreeMap<PathBuf, GitChangeCounts>, GitActivityError> {
    let mut counts = parse_numstat(&run_success(root, ["diff", "--numstat", "-z"])?)?;
    for (path, staged) in
        parse_numstat(&run_success(root, ["diff", "--cached", "--numstat", "-z"])?)?
    {
        counts
            .entry(path)
            .and_modify(|unstaged| *unstaged = unstaged.combined_with(staged))
            .or_insert(staged);
    }
    Ok(counts)
}

fn parse_numstat(bytes: &[u8]) -> Result<BTreeMap<PathBuf, GitChangeCounts>, GitActivityError> {
    let mut counts = BTreeMap::new();
    let mut records = bytes.split(|byte| *byte == 0);
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let (Some(additions), Some(deletions), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(GitActivityError::InvalidOutput);
        };
        if path.is_empty() {
            let _ = records.next();
            let _ = records.next();
            continue;
        }
        let path = String::from_utf8(path.to_vec()).map_err(|_| GitActivityError::InvalidOutput)?;
        let changes = match (
            std::str::from_utf8(additions)
                .ok()
                .and_then(|value| value.parse().ok()),
            std::str::from_utf8(deletions)
                .ok()
                .and_then(|value| value.parse().ok()),
        ) {
            (Some(additions), Some(deletions)) => GitChangeCounts {
                additions: Some(additions),
                deletions: Some(deletions),
            },
            _ => GitChangeCounts::unavailable(),
        };
        counts.insert(path.into(), changes);
    }
    Ok(counts)
}

fn parse_commits(bytes: &[u8]) -> Result<Vec<GitCommit>, GitActivityError> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| GitActivityError::InvalidOutput)?
        .lines()
        .map(|line| {
            line.split_once('\x1f')
                .map(|(id, summary)| GitCommit {
                    id: id.into(),
                    summary: summary.into(),
                })
                .ok_or(GitActivityError::InvalidOutput)
        })
        .collect()
}
fn run_success<'a>(
    root: &Path,
    args: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<u8>, GitActivityError> {
    let output = run_git(root, args)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(command_error(&output))
    }
}
fn run_git<'a>(
    root: &Path,
    args: impl IntoIterator<Item = &'a str>,
) -> Result<Output, GitActivityError> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GitActivityError::GitUnavailable
            } else {
                GitActivityError::CommandFailed(e.to_string())
            }
        })
}
fn command_error(output: &Output) -> GitActivityError {
    GitActivityError::CommandFailed(String::from_utf8_lossy(&output.stderr).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::{WorktreeSubtreeStat, diagnose_worktree_subtree};
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static ID: AtomicUsize = AtomicUsize::new(0);
    struct Repo(PathBuf);
    impl Repo {
        fn new(git: bool) -> Self {
            let p = std::env::temp_dir().join(format!(
                "devscope-git-{}",
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            if git {
                cmd(&p, &["init"]);
                cmd(&p, &["config", "user.name", "DevScope Test"]);
                cmd(&p, &["config", "user.email", "devscope@test.invalid"]);
            }
            Self(p)
        }
        fn commit(&self, s: &str) {
            fs::write(self.0.join("a.txt"), s).unwrap();
            cmd(&self.0, &["add", "."]);
            cmd(&self.0, &["commit", "-m", s]);
        }
    }
    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn cmd(p: &Path, a: &[&str]) {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(p)
                .args(a)
                .status()
                .unwrap()
                .success()
        );
    }
    #[test]
    fn parses_numstat_for_multiple_files_and_keeps_unknown_counts_safe() {
        let counts =
            parse_numstat(b"12\t4\tsrc/ui.rs\x003\t0\tdocs/design.md\x00-\t-\tassets/logo.png\x00")
                .unwrap();
        assert_eq!(
            counts.get(Path::new("src/ui.rs")),
            Some(&GitChangeCounts {
                additions: Some(12),
                deletions: Some(4),
            })
        );
        assert_eq!(
            counts.get(Path::new("docs/design.md")),
            Some(&GitChangeCounts {
                additions: Some(3),
                deletions: Some(0),
            })
        );
        assert_eq!(
            counts.get(Path::new("assets/logo.png")),
            Some(&GitChangeCounts::unavailable())
        );
    }

    #[test]
    fn keeps_renamed_status_and_treats_rename_numstat_paths_as_unavailable() {
        let counts = parse_numstat(b"0\t0\t\0old.txt\0new.txt\0").unwrap();
        assert!(counts.is_empty());
        let changed = parse_status(b"R  new.txt\0old.txt\0", &counts).unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].path, Path::new("new.txt"));
        assert_eq!(changed[0].status, GitFileStatus::Renamed);
        assert_eq!(changed[0].changes, GitChangeCounts::unavailable());
    }
    #[test]
    fn collects_staged_and_unstaged_counts_and_leaves_untracked_unknown() {
        let repo = Repo::new(true);
        repo.commit("one");
        fs::write(repo.0.join("a.txt"), "staged\n").unwrap();
        cmd(&repo.0, &["add", "a.txt"]);
        fs::write(repo.0.join("a.txt"), "staged\nunstaged\n").unwrap();
        fs::write(repo.0.join("new.txt"), "untracked\n").unwrap();

        let activity = collect_git_activity(&repo.0, 5).unwrap();
        let staged_and_unstaged = activity
            .changed_files
            .iter()
            .find(|file| file.path == Path::new("a.txt"))
            .unwrap();
        assert_eq!(
            staged_and_unstaged.changes,
            GitChangeCounts {
                additions: Some(2),
                deletions: Some(1),
            }
        );
        let untracked = activity
            .changed_files
            .iter()
            .find(|file| file.path == Path::new("new.txt"))
            .unwrap();
        assert_eq!(untracked.status, GitFileStatus::Added);
        assert_eq!(untracked.changes, GitChangeCounts::unavailable());
    }
    #[test]
    fn collects_modified_staged_and_untracked_file_diffs() {
        let repo = Repo::new(true);
        repo.commit("old\n");
        fs::write(repo.0.join("a.txt"), "unstaged\n").unwrap();
        let unstaged =
            collect_git_file_diff(&repo.0, Path::new("a.txt"), &GitFileStatus::Modified).unwrap();
        let GitFileDiff::Available { unstaged, staged } = unstaged else {
            panic!("modified file should have a diff")
        };
        assert!(unstaged.unwrap().text.contains("-old"));
        assert!(staged.is_none());

        fs::write(repo.0.join("a.txt"), "staged\n").unwrap();
        cmd(&repo.0, &["add", "a.txt"]);
        fs::write(repo.0.join("a.txt"), "staged\nunstaged\n").unwrap();
        let combined =
            collect_git_file_diff(&repo.0, Path::new("a.txt"), &GitFileStatus::Modified).unwrap();
        let GitFileDiff::Available { unstaged, staged } = combined else {
            panic!("staged and unstaged changes should both be available")
        };
        assert!(unstaged.unwrap().text.contains("+unstaged"));
        assert!(staged.unwrap().text.contains("+staged"));

        fs::write(repo.0.join("new.txt"), "untracked\n").unwrap();
        assert_eq!(
            collect_git_file_diff(&repo.0, Path::new("new.txt"), &GitFileStatus::Added).unwrap(),
            GitFileDiff::Unavailable(GitFileDiffUnavailable::Untracked)
        );
    }

    #[test]
    fn handles_binary_and_renamed_diffs_without_panicking() {
        let repo = Repo::new(true);
        repo.commit("text");
        fs::write(repo.0.join("a.txt"), [0, 1, 2]).unwrap();
        assert!(
            collect_git_file_diff(&repo.0, Path::new("a.txt"), &GitFileStatus::Modified).is_ok()
        );
        assert_eq!(
            collect_git_file_diff(&repo.0, Path::new("a.txt"), &GitFileStatus::Renamed).unwrap(),
            GitFileDiff::Unavailable(GitFileDiffUnavailable::Renamed)
        );
    }
    #[test]
    fn detection_and_zero_commits() {
        let n = Repo::new(false);
        assert!(!is_git_repository(&n.0).unwrap());
        let r = Repo::new(true);
        assert!(is_git_repository(&r.0).unwrap());
        assert!(
            collect_git_activity(&r.0, 5)
                .unwrap()
                .recent_commits
                .is_empty()
        );
    }
    #[test]
    fn status_count_and_commits() {
        let r = Repo::new(true);
        r.commit("one");
        assert_eq!(
            collect_git_activity(&r.0, 5).unwrap().changed_file_count(),
            0
        );
        fs::write(r.0.join("a.txt"), "two").unwrap();
        fs::write(r.0.join("new file.txt"), "x").unwrap();
        let a = collect_git_activity(&r.0, 5).unwrap();
        assert_eq!(a.changed_file_count(), 2);
        r.commit("two");
        let c = collect_git_activity(&r.0, 1).unwrap().recent_commits;
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].summary, "two");
        assert!(!c[0].id.is_empty());
    }

    fn diagnostics(root: &Path, entries: &[(&str, usize, u64)]) -> WorktreeScanDiagnostics {
        WorktreeScanDiagnostics {
            visited_entries: entries.iter().map(|(_, entries, _)| entries).sum(),
            subtrees: entries
                .iter()
                .map(|(path, visited_entries, duration)| WorktreeSubtreeStat {
                    path: root.join(path),
                    visited_entries: *visited_entries,
                    duration: std::time::Duration::from_millis(*duration),
                })
                .collect(),
        }
    }

    #[test]
    fn assesses_ignored_untracked_subtree_as_safe_candidate() {
        let repo = Repo::new(true);
        fs::write(repo.0.join(".gitignore"), "dist/\n").unwrap();
        fs::create_dir_all(repo.0.join("dist")).unwrap();
        fs::write(repo.0.join("dist/output.txt"), "generated").unwrap();

        let assessment = assess_activity_exclude_candidates(
            &repo.0,
            &diagnostics(&repo.0, &[("dist", 20, 10)]),
            3,
        )
        .remove(0);

        assert_eq!(
            assessment.status,
            ActivityExcludeCandidateStatus::SafeCandidate
        );
        assert_eq!(assessment.git_ignored, Some(true));
        assert_eq!(assessment.contains_tracked_files, Some(false));
        assert!(
            assessment
                .reasons
                .contains(&ActivityExcludeCandidateReason::GitIgnored)
        );
        assert!(
            assessment
                .reasons
                .contains(&ActivityExcludeCandidateReason::GeneratedOutputNameHint)
        );
    }

    #[test]
    fn assesses_force_added_ignored_subtree_as_review_required() {
        let repo = Repo::new(true);
        fs::write(repo.0.join(".gitignore"), "dist/\n").unwrap();
        fs::create_dir_all(repo.0.join("dist")).unwrap();
        fs::write(repo.0.join("dist/forced.txt"), "tracked").unwrap();
        cmd(&repo.0, &["add", ".gitignore"]);
        cmd(&repo.0, &["add", "-f", "dist/forced.txt"]);
        cmd(&repo.0, &["commit", "-m", "force add generated output"]);

        let assessment = assess_activity_exclude_candidates(
            &repo.0,
            &diagnostics(&repo.0, &[("dist", 20, 10)]),
            3,
        )
        .remove(0);

        assert_eq!(
            assessment.status,
            ActivityExcludeCandidateStatus::ReviewRequired
        );
        assert_eq!(assessment.git_ignored, Some(true));
        assert_eq!(assessment.contains_tracked_files, Some(true));
        assert!(
            assessment
                .reasons
                .contains(&ActivityExcludeCandidateReason::ContainsTrackedFiles)
        );
    }

    #[test]
    fn assesses_tracked_source_subtree_as_not_candidate() {
        let repo = Repo::new(true);
        fs::create_dir_all(repo.0.join("src")).unwrap();
        fs::write(repo.0.join("src/lib.rs"), "pub fn example() {}\n").unwrap();
        cmd(&repo.0, &["add", "src/lib.rs"]);
        cmd(&repo.0, &["commit", "-m", "track source"]);

        let assessment = assess_activity_exclude_candidates(
            &repo.0,
            &diagnostics(&repo.0, &[("src", 8, 10)]),
            3,
        )
        .remove(0);

        assert_eq!(
            assessment.status,
            ActivityExcludeCandidateStatus::NotCandidate
        );
        assert_eq!(assessment.git_ignored, Some(false));
        assert_eq!(assessment.contains_tracked_files, Some(true));
    }

    #[test]
    fn respects_nested_ignore_negation_and_info_exclude_rules() {
        let repo = Repo::new(true);
        fs::write(
            repo.0.join(".gitignore"),
            "generated/*\n!generated/keep.txt\n",
        )
        .unwrap();
        fs::create_dir_all(repo.0.join("generated")).unwrap();
        fs::write(repo.0.join("generated/keep.txt"), "keep").unwrap();
        fs::write(repo.0.join(".git/info/exclude"), "private-output/\n").unwrap();
        fs::create_dir_all(repo.0.join("private-output")).unwrap();
        fs::write(repo.0.join("private-output/cache.txt"), "cache").unwrap();

        let assessments = assess_activity_exclude_candidates(
            &repo.0,
            &diagnostics(
                &repo.0,
                &[("generated/keep.txt", 1, 20), ("private-output", 5, 10)],
            ),
            3,
        );

        assert_eq!(assessments[0].git_ignored, Some(false));
        assert_eq!(
            assessments[0].status,
            ActivityExcludeCandidateStatus::ReviewRequired
        );
        assert_eq!(assessments[1].git_ignored, Some(true));
        assert_eq!(
            assessments[1].status,
            ActivityExcludeCandidateStatus::SafeCandidate
        );
    }

    #[test]
    fn keeps_devscope_local_state_out_of_safe_candidates() {
        let repo = Repo::new(true);
        fs::create_dir_all(repo.0.join(".devscope/work")).unwrap();
        fs::write(repo.0.join(".devscope/work/current.md"), "# Current Work\n").unwrap();
        fs::write(repo.0.join(".devscope/config.toml"), "[devscope]\n").unwrap();
        cmd(&repo.0, &["add", ".devscope/config.toml"]);
        cmd(&repo.0, &["commit", "-m", "track DevScope config"]);

        let assessment = assess_activity_exclude_candidates(
            &repo.0,
            &diagnostics(&repo.0, &[(".devscope", 5, 10)]),
            3,
        )
        .remove(0);

        assert_eq!(
            assessment.status,
            ActivityExcludeCandidateStatus::ReviewRequired
        );
        assert_eq!(assessment.contains_tracked_files, Some(true));
        assert!(
            assessment
                .reasons
                .contains(&ActivityExcludeCandidateReason::DevScopeLocalState)
        );
    }

    #[test]
    fn drills_down_devscope_children_with_existing_git_assessment_semantics() {
        let repo = Repo::new(true);
        fs::write(
            repo.0.join(".gitignore"),
            ".devscope/evidence/\n.devscope/history/\n.devscope/forced/\n",
        )
        .unwrap();
        fs::create_dir_all(repo.0.join(".devscope/evidence")).unwrap();
        fs::write(repo.0.join(".devscope/evidence/latest.json"), "evidence").unwrap();
        fs::create_dir_all(repo.0.join(".devscope/history")).unwrap();
        fs::write(repo.0.join(".devscope/history/events.jsonl"), "event").unwrap();
        fs::create_dir_all(repo.0.join(".devscope/work")).unwrap();
        fs::write(repo.0.join(".devscope/work/current.md"), "# Current Work\n").unwrap();
        fs::write(repo.0.join(".devscope/config.toml"), "[devscope]\n").unwrap();
        fs::create_dir_all(repo.0.join(".devscope/forced")).unwrap();
        fs::write(repo.0.join(".devscope/forced/value.txt"), "tracked").unwrap();
        fs::write(repo.0.join(".devscope/review.txt"), "review").unwrap();
        fs::write(repo.0.join(".git/info/exclude"), ".devscope/info-cache/\n").unwrap();
        fs::create_dir_all(repo.0.join(".devscope/info-cache")).unwrap();
        fs::write(repo.0.join(".devscope/info-cache/cache.txt"), "cache").unwrap();
        cmd(
            &repo.0,
            &[
                "add",
                ".gitignore",
                ".devscope/work/current.md",
                ".devscope/config.toml",
            ],
        );
        cmd(&repo.0, &["add", "-f", ".devscope/forced/value.txt"]);
        cmd(
            &repo.0,
            &["commit", "-m", "set up DevScope diagnostic fixture"],
        );

        let root_assessment = assess_activity_exclude_candidates(
            &repo.0,
            &diagnostics(&repo.0, &[(".devscope", 20, 100)]),
            3,
        )
        .remove(0);
        assert_eq!(
            root_assessment.status,
            ActivityExcludeCandidateStatus::ReviewRequired
        );
        assert!(
            root_assessment
                .reasons
                .contains(&ActivityExcludeCandidateReason::DevScopeLocalState)
        );

        let child_diagnostics =
            diagnose_worktree_subtree(&repo.0, &repo.0.join(".devscope")).unwrap();
        let assessments = assess_activity_exclude_candidates(&repo.0, &child_diagnostics, 10);
        let by_name = |name| {
            assessments
                .iter()
                .find(|assessment| {
                    assessment
                        .path
                        .file_name()
                        .is_some_and(|value| value == name)
                })
                .unwrap()
        };

        for name in ["evidence", "history", "info-cache"] {
            let assessment = by_name(name);
            assert_eq!(
                assessment.status,
                ActivityExcludeCandidateStatus::SafeCandidate
            );
            assert_eq!(assessment.git_ignored, Some(true));
            assert_eq!(assessment.contains_tracked_files, Some(false));
        }
        for name in ["work", "config.toml"] {
            let assessment = by_name(name);
            assert_ne!(
                assessment.status,
                ActivityExcludeCandidateStatus::SafeCandidate
            );
            assert_eq!(assessment.contains_tracked_files, Some(true));
        }
        let forced = by_name("forced");
        assert_eq!(
            forced.status,
            ActivityExcludeCandidateStatus::ReviewRequired
        );
        assert_eq!(forced.git_ignored, Some(true));
        assert_eq!(forced.contains_tracked_files, Some(true));
        assert_eq!(
            by_name("review.txt").status,
            ActivityExcludeCandidateStatus::ReviewRequired
        );
    }

    #[test]
    fn respects_nested_gitignore_and_negation_when_assessing_drill_down_children() {
        let repo = Repo::new(true);
        fs::create_dir_all(repo.0.join(".devscope/probes/ignored")).unwrap();
        fs::write(
            repo.0.join(".devscope/probes/.gitignore"),
            "ignored/\n*.tmp\n!keep.tmp\n",
        )
        .unwrap();
        fs::write(repo.0.join(".devscope/probes/ignored/value.txt"), "ignored").unwrap();
        fs::write(repo.0.join(".devscope/probes/drop.tmp"), "ignored").unwrap();
        fs::write(repo.0.join(".devscope/probes/keep.tmp"), "keep").unwrap();

        let assessments = assess_activity_exclude_candidates(
            &repo.0,
            &diagnostics(
                &repo.0,
                &[
                    (".devscope/probes/ignored", 4, 30),
                    (".devscope/probes/drop.tmp", 1, 20),
                    (".devscope/probes/keep.tmp", 1, 10),
                ],
            ),
            3,
        );

        assert_eq!(
            assessments[0].status,
            ActivityExcludeCandidateStatus::SafeCandidate
        );
        assert_eq!(
            assessments[1].status,
            ActivityExcludeCandidateStatus::SafeCandidate
        );
        assert_eq!(
            assessments[2].status,
            ActivityExcludeCandidateStatus::ReviewRequired
        );
        assert_eq!(assessments[2].git_ignored, Some(false));
    }

    #[test]
    fn proposes_safe_root_candidates_in_stable_order_without_duplicates_or_excluded_paths() {
        let repo = Repo::new(true);
        fs::write(repo.0.join(".gitignore"), "cache/\ndist/\n").unwrap();
        for path in ["cache/value.txt", "dist/value.txt", "dist/cache/value.txt"] {
            let path = repo.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "generated").unwrap();
        }
        let root_diagnostics = diagnostics(
            &repo.0,
            &[("dist", 10, 10), ("cache", 20, 20), ("dist", 10, 10)],
        );

        let proposals = propose_activity_exclusions(&repo.0, &root_diagnostics, &[], 3);
        assert_eq!(
            proposals
                .iter()
                .map(|proposal| proposal.path.as_path())
                .collect::<Vec<_>>(),
            vec![Path::new("cache"), Path::new("dist")]
        );
        assert!(proposals.iter().all(|proposal| {
            proposal.source == ActivityExcludeProposalSource::RootDiagnostic
                && proposal
                    .reasons
                    .contains(&ActivityExcludeCandidateReason::GitIgnored)
                && proposal
                    .path
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_)))
        }));

        let already_excluded =
            propose_activity_exclusions(&repo.0, &root_diagnostics, &[PathBuf::from("dist")], 3);
        assert_eq!(already_excluded.len(), 1);
        assert_eq!(already_excluded[0].path, Path::new("cache"));

        let descendant = propose_activity_exclusions(
            &repo.0,
            &diagnostics(&repo.0, &[("dist/cache", 5, 5)]),
            &[PathBuf::from("dist")],
            3,
        );
        assert!(descendant.is_empty());
    }

    #[test]
    fn drills_down_reviewed_devscope_once_and_proposes_only_safe_children() {
        let repo = Repo::new(true);
        fs::write(
            repo.0.join(".gitignore"),
            ".devscope/evidence/\n.devscope/history/\n.devscope/forced/\n",
        )
        .unwrap();
        for directory in [
            ".devscope/evidence",
            ".devscope/history",
            ".devscope/work",
            ".devscope/forced",
        ] {
            fs::create_dir_all(repo.0.join(directory)).unwrap();
        }
        fs::write(repo.0.join(".devscope/evidence/latest.json"), "evidence").unwrap();
        fs::write(repo.0.join(".devscope/history/events.jsonl"), "event").unwrap();
        fs::write(repo.0.join(".devscope/work/current.md"), "# Current Work\n").unwrap();
        fs::write(repo.0.join(".devscope/config.toml"), "[activity]\n").unwrap();
        fs::write(repo.0.join(".devscope/forced/value.txt"), "tracked").unwrap();
        cmd(
            &repo.0,
            &[
                "add",
                ".gitignore",
                ".devscope/work/current.md",
                ".devscope/config.toml",
            ],
        );
        cmd(&repo.0, &["add", "-f", ".devscope/forced/value.txt"]);
        cmd(&repo.0, &["commit", "-m", "set up proposal fixture"]);

        let proposals = propose_activity_exclusions(
            &repo.0,
            &diagnostics(&repo.0, &[(".devscope", 20, 100)]),
            &[],
            10,
        );
        let paths = proposals
            .iter()
            .map(|proposal| proposal.path.as_path())
            .collect::<Vec<_>>();
        assert!(paths.contains(&Path::new(".devscope/evidence")));
        assert!(paths.contains(&Path::new(".devscope/history")));
        assert!(!paths.contains(&Path::new(".devscope/work")));
        assert!(!paths.contains(&Path::new(".devscope/config.toml")));
        assert!(!paths.contains(&Path::new(".devscope/forced")));
        assert!(proposals.iter().all(|proposal| {
            proposal.source
                == ActivityExcludeProposalSource::DrillDown {
                    parent: PathBuf::from(".devscope"),
                }
        }));
    }

    #[test]
    fn refuses_force_added_generated_and_git_unknown_candidates() {
        let force_added = Repo::new(true);
        fs::write(force_added.0.join(".gitignore"), "dist/\n").unwrap();
        fs::create_dir_all(force_added.0.join("dist")).unwrap();
        fs::write(force_added.0.join("dist/forced.txt"), "tracked").unwrap();
        cmd(&force_added.0, &["add", ".gitignore"]);
        cmd(&force_added.0, &["add", "-f", "dist/forced.txt"]);
        cmd(
            &force_added.0,
            &["commit", "-m", "force add generated output"],
        );
        assert!(
            propose_activity_exclusions(
                &force_added.0,
                &diagnostics(&force_added.0, &[("dist", 20, 10)]),
                &[],
                3,
            )
            .is_empty()
        );

        let generated_name_only = Repo::new(true);
        fs::create_dir_all(generated_name_only.0.join("dist")).unwrap();
        fs::write(generated_name_only.0.join("dist/output.txt"), "generated").unwrap();
        assert!(
            propose_activity_exclusions(
                &generated_name_only.0,
                &diagnostics(&generated_name_only.0, &[("dist", 20, 10)]),
                &[],
                3,
            )
            .is_empty()
        );

        let not_a_repo = Repo::new(false);
        assert!(
            propose_activity_exclusions(
                &not_a_repo.0,
                &diagnostics(&not_a_repo.0, &[("dist", 20, 10)]),
                &[],
                3,
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_recursively_drill_into_reviewed_children() {
        let repo = Repo::new(true);
        fs::create_dir_all(repo.0.join(".devscope/probes/ignored")).unwrap();
        fs::write(repo.0.join(".devscope/probes/.gitignore"), "ignored/\n").unwrap();
        fs::write(
            repo.0.join(".devscope/probes/ignored/value.txt"),
            "generated",
        )
        .unwrap();

        let proposals = propose_activity_exclusions(
            &repo.0,
            &diagnostics(&repo.0, &[(".devscope", 20, 100)]),
            &[],
            10,
        );
        assert!(proposals.is_empty());
    }
    #[test]
    fn keeps_git_observation_errors_reviewable_and_orders_candidates_stably() {
        let not_a_repo = Repo::new(false);
        let assessments = assess_activity_exclude_candidates(
            &not_a_repo.0,
            &diagnostics(&not_a_repo.0, &[("src", 2, 5), ("b", 1, 10), ("a", 2, 10)]),
            3,
        );

        assert_eq!(
            assessments
                .iter()
                .map(|assessment| assessment.path.file_name().unwrap().to_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["a", "b", "src"]
        );
        assert!(assessments.iter().all(|assessment| {
            assessment.status == ActivityExcludeCandidateStatus::ReviewRequired
                && assessment.git_ignored.is_none()
                && assessment.contains_tracked_files.is_none()
                && assessment
                    .reasons
                    .contains(&ActivityExcludeCandidateReason::GitObservationUnavailable)
        }));
    }
}
#[cfg(test)]
mod extra_tests {
    use super::*;
    use std::{fs, process::Command};
    fn run(p: &Path, a: &[&str]) {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(p)
                .args(a)
                .status()
                .unwrap()
                .success()
        );
    }
    #[test]
    fn staged_unstaged_and_commit_order() {
        let p = std::env::temp_dir().join(format!("devscope-git-extra-{}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        run(&p, &["init"]);
        run(&p, &["config", "user.name", "Test"]);
        run(&p, &["config", "user.email", "t@x.invalid"]);
        for s in ["oldest", "middle", "newest"] {
            fs::write(p.join("f"), s).unwrap();
            run(&p, &["add", "."]);
            run(&p, &["commit", "-m", s]);
        }
        fs::write(p.join("f"), "staged").unwrap();
        run(&p, &["add", "f"]);
        fs::write(p.join("f"), "unstaged").unwrap();
        let a = collect_git_activity(&p, 2).unwrap();
        assert_eq!(a.changed_file_count(), 1);
        assert_eq!(a.changed_files[0].status, GitFileStatus::Modified);
        assert_eq!(a.recent_commits.len(), 2);
        assert_eq!(a.recent_commits[0].summary, "newest");
        assert_eq!(a.recent_commits[1].summary, "middle");
        let _ = fs::remove_dir_all(p);
    }
}
