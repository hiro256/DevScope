use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactObservation {
    path: PathBuf,
    status: ArtifactStatus,
}
impl ArtifactObservation {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn status(&self) -> &ArtifactStatus {
        &self.status
    }
    pub fn observation_failed(&self) -> bool {
        matches!(self.status, ArtifactStatus::ObservationError { .. })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactStatus {
    Exists { kind: ArtifactKind, size: u64 },
    Missing,
    ObservationError { message: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    File,
    Directory,
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPathError {
    message: String,
}
impl std::fmt::Display for ArtifactPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ArtifactPathError {}

pub fn observe_artifact(
    root: &Path,
    requested: &Path,
) -> Result<ArtifactObservation, ArtifactPathError> {
    let path = validate_artifact_path(requested)?;
    ensure_artifact_inside_root(root, &path)?;
    let full_path = root.join(&path);
    let status = match fs::metadata(full_path) {
        Ok(metadata) => ArtifactStatus::Exists {
            kind: if metadata.is_file() {
                ArtifactKind::File
            } else if metadata.is_dir() {
                ArtifactKind::Directory
            } else {
                ArtifactKind::Other
            },
            size: metadata.len(),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ArtifactStatus::Missing,
        Err(error) => ArtifactStatus::ObservationError {
            message: error.to_string(),
        },
    };
    Ok(ArtifactObservation { path, status })
}
fn ensure_artifact_inside_root(root: &Path, path: &Path) -> Result<(), ArtifactPathError> {
    let canonical_root = fs::canonicalize(root).map_err(|_| ArtifactPathError {
        message: "could not resolve project root".into(),
    })?;
    let mut existing = root.join(path);
    loop {
        match fs::canonicalize(&existing) {
            Ok(resolved) => {
                if resolved.starts_with(&canonical_root) {
                    return Ok(());
                }
                return Err(ArtifactPathError {
                    message: "artifact path resolves outside project root".into(),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !existing.pop() {
                    return Err(ArtifactPathError {
                        message: "could not resolve artifact path".into(),
                    });
                }
            }
            Err(_) => {
                return Err(ArtifactPathError {
                    message: "could not resolve artifact path".into(),
                });
            }
        }
    }
}
fn validate_artifact_path(requested: &Path) -> Result<PathBuf, ArtifactPathError> {
    let raw = requested.to_string_lossy();
    if raw.is_empty() {
        return Err(ArtifactPathError {
            message: "artifact path must not be empty".into(),
        });
    }
    if raw.starts_with("\\\\") || raw.as_bytes().get(1) == Some(&b':') {
        return Err(ArtifactPathError {
            message: "artifact path must be project-relative".into(),
        });
    }
    let path = PathBuf::from(raw.replace('\\', "/"));
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || !path
            .components()
            .any(|part| matches!(part, Component::Normal(_)))
    {
        return Err(ArtifactPathError {
            message: "artifact path must be project-relative".into(),
        });
    }
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ID: AtomicUsize = AtomicUsize::new(0);
    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "devscope-artifact-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn observes_file_missing_and_directory() {
        let root = root();
        fs::write(root.join("output.bin"), "abc").unwrap();
        assert_eq!(
            observe_artifact(&root, Path::new("output.bin"))
                .unwrap()
                .status(),
            &ArtifactStatus::Exists {
                kind: ArtifactKind::File,
                size: 3
            }
        );
        assert_eq!(
            observe_artifact(&root, Path::new("missing.bin"))
                .unwrap()
                .status(),
            &ArtifactStatus::Missing
        );
        fs::create_dir(root.join("output")).unwrap();
        assert!(matches!(
            observe_artifact(&root, Path::new("output"))
                .unwrap()
                .status(),
            ArtifactStatus::Exists {
                kind: ArtifactKind::Directory,
                ..
            }
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_for_existing_and_missing_paths() {
        use std::os::unix::fs::symlink;
        let root = root();
        let outside = root.parent().unwrap().join(format!(
            "devscope-artifact-outside-{}",
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("file"), "outside").unwrap();
        symlink(&outside, root.join("outside-link")).unwrap();
        for path in ["outside-link/file", "outside-link/missing"] {
            assert!(observe_artifact(&root, Path::new(path)).is_err(), "{path}");
        }
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
    #[test]
    fn rejects_unsafe_paths() {
        for path in ["../outside", "../../outside", "/absolute", "C:\\absolute"] {
            assert!(
                observe_artifact(Path::new("."), Path::new(path)).is_err(),
                "{path}"
            );
        }
    }
}
