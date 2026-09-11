use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

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
