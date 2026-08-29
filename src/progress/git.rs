use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    process::{Command, Output},
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
    let changed_files = parse_status(&status)?;
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
fn parse_status(bytes: &[u8]) -> Result<Vec<GitChangedFile>, GitActivityError> {
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
                path: path.into(),
                status,
            },
        );
    }
    Ok(entries.into_values().collect())
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
