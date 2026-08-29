use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChangedFile {
    pub path: PathBuf,
    pub status: GitFileStatus,
}
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
#[derive(Debug)]
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

pub fn collect_git_activity(
    root: &Path,
    commit_limit: usize,
) -> Result<GitActivity, GitActivityError> {
    if !is_git_repository(root)? {
        return Err(GitActivityError::NotRepository);
    }
    let status = git(root, ["status", "--porcelain=v1"])?;
    let mut files = BTreeMap::new();
    for line in status.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let path = PathBuf::from(&line[3..]);
        let kind = if code.contains('D') {
            GitFileStatus::Deleted
        } else if code.contains('R') {
            GitFileStatus::Renamed
        } else if code.contains('A') || code == "??" {
            GitFileStatus::Added
        } else {
            GitFileStatus::Modified
        };
        files.insert(path.clone(), GitChangedFile { path, status: kind });
    }
    let output = git(
        root,
        ["log", &format!("-n{commit_limit}"), "--format=%h%x1f%s"],
    )?;
    let recent_commits = output
        .lines()
        .filter_map(|line| {
            line.split_once('\x1f').map(|(id, summary)| GitCommit {
                id: id.into(),
                summary: summary.into(),
            })
        })
        .collect();
    Ok(GitActivity {
        changed_files: files.into_values().collect(),
        recent_commits,
    })
}
pub fn is_git_repository(root: &Path) -> Result<bool, GitActivityError> {
    Ok(git(root, ["rev-parse", "--is-inside-work-tree"])
        .map(|value| value.trim() == "true")
        .unwrap_or(false))
}
fn git<'a>(
    root: &Path,
    args: impl IntoIterator<Item = &'a str>,
) -> Result<String, GitActivityError> {
    let output = Command::new("git")
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
        })?;
    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|_| GitActivityError::InvalidOutput)
    } else {
        Err(GitActivityError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).into(),
        ))
    }
}
