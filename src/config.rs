//! Optional, project-local observation policy.
//!
//! The first Config slice intentionally supports only `[plan].exclude`.

use std::{
    error::Error,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

pub const CONFIG_PATH: &str = ".devscope/config.toml";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectConfig {
    plan: PlanConfig,
}

impl ProjectConfig {
    pub fn plan(&self) -> &PlanConfig {
        &self.plan
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanConfig {
    excludes: Vec<PathBuf>,
}

impl PlanConfig {
    pub fn excludes(&self) -> &[PathBuf] {
        &self.excludes
    }

    /// Returns whether a project-root-relative candidate is excluded by a literal
    /// configured path or by one of that path's ancestor directories.
    pub fn excludes_path(&self, candidate: &Path) -> bool {
        self.excludes
            .iter()
            .any(|excluded| candidate == excluded || candidate.starts_with(excluded))
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    InvalidSchema {
        path: PathBuf,
        message: String,
    },
    InvalidPath {
        path: PathBuf,
        value: String,
        reason: &'static str,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, .. } => {
                write!(formatter, "could not read Config file {}", path.display())
            }
            Self::Parse { path, .. } => {
                write!(formatter, "could not parse Config file {}", path.display())
            }
            Self::InvalidSchema { path, message } => {
                write!(
                    formatter,
                    "invalid Config file {}: {message}",
                    path.display()
                )
            }
            Self::InvalidPath {
                path,
                value,
                reason,
            } => {
                write!(
                    formatter,
                    "invalid plan.exclude path `{value}` in {}: {reason}",
                    path.display()
                )
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::InvalidSchema { .. } | Self::InvalidPath { .. } => None,
        }
    }
}

/// Loads the optional project Config. A missing Config is the default policy.
pub fn load_project_config(root: &Path) -> Result<ProjectConfig, ConfigError> {
    let path = root.join(CONFIG_PATH);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Ok(ProjectConfig::default());
        }
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    parse_project_config(&path, &contents)
}

fn parse_project_config(path: &Path, contents: &str) -> Result<ProjectConfig, ConfigError> {
    let value: toml::Value = toml::from_str(contents).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    let table = value.as_table().ok_or_else(|| ConfigError::InvalidSchema {
        path: path.to_path_buf(),
        message: "root must be a TOML table".to_owned(),
    })?;

    for key in table.keys() {
        if key != "plan" {
            return Err(unknown_key(path, key, "top level"));
        }
    }

    let Some(plan_value) = table.get("plan") else {
        return Ok(ProjectConfig::default());
    };
    let plan_table = plan_value
        .as_table()
        .ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "`plan` must be a table".to_owned(),
        })?;
    for key in plan_table.keys() {
        if key != "exclude" {
            return Err(unknown_key(path, key, "[plan]"));
        }
    }

    let excludes = match plan_table.get("exclude") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(|| ConfigError::InvalidSchema {
                path: path.to_path_buf(),
                message: "`plan.exclude` must be an array of strings".to_owned(),
            })?
            .iter()
            .map(|value| {
                let value = value.as_str().ok_or_else(|| ConfigError::InvalidSchema {
                    path: path.to_path_buf(),
                    message: "`plan.exclude` must be an array of strings".to_owned(),
                })?;
                validate_exclude_path(path, value)
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok(ProjectConfig {
        plan: PlanConfig { excludes },
    })
}

fn unknown_key(path: &Path, key: &str, scope: &str) -> ConfigError {
    ConfigError::InvalidSchema {
        path: path.to_path_buf(),
        message: format!("unknown key `{key}` in {scope}"),
    }
}

fn validate_exclude_path(config_path: &Path, value: &str) -> Result<PathBuf, ConfigError> {
    let invalid = |reason| ConfigError::InvalidPath {
        path: config_path.to_path_buf(),
        value: value.to_owned(),
        reason,
    };
    if value.is_empty() {
        return Err(invalid("path must not be empty"));
    }
    if value.contains('\\') {
        return Err(invalid("use `/` as the path separator"));
    }
    if value.starts_with('!') {
        return Err(invalid("negation is not supported"));
    }
    if value.contains(['*', '?']) {
        return Err(invalid("glob syntax is not supported"));
    }

    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(invalid("path must be relative to the project root"));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ID: AtomicUsize = AtomicUsize::new(0);

    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "devscope-config-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write_config(&self, contents: &str) {
            let path = self.0.join(CONFIG_PATH);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_config_uses_defaults() {
        let project = TempProject::new();
        assert_eq!(
            load_project_config(&project.0).unwrap(),
            ProjectConfig::default()
        );
    }

    #[test]
    fn accepts_empty_and_duplicate_excludes() {
        let project = TempProject::new();
        project.write_config("[plan]\nexclude = [\"generated\", \"generated\"]\n");
        let config = load_project_config(&project.0).unwrap();
        assert_eq!(config.plan().excludes().len(), 2);
        assert!(config.plan().excludes_path(Path::new("generated/tasks.md")));
    }

    #[test]
    fn rejects_malformed_and_unknown_config() {
        let project = TempProject::new();
        project.write_config("[plan\nexclude = []");
        assert!(matches!(
            load_project_config(&project.0),
            Err(ConfigError::Parse { .. })
        ));

        project.write_config("unexpected = true");
        assert!(matches!(
            load_project_config(&project.0),
            Err(ConfigError::InvalidSchema { .. })
        ));

        project.write_config("[plan]\ninclude = [\"docs\"]");
        assert!(matches!(
            load_project_config(&project.0),
            Err(ConfigError::InvalidSchema { .. })
        ));
    }

    #[test]
    fn rejects_glob_and_negation_syntax() {
        let project = TempProject::new();
        for value in ["generated/*", "!translations"] {
            project.write_config(&format!("[plan]\nexclude = [{value:?}]"));
            assert!(matches!(
                load_project_config(&project.0),
                Err(ConfigError::InvalidPath { .. })
            ));
        }
    }
    #[test]
    fn rejects_non_portable_or_unsafe_paths() {
        let project = TempProject::new();
        for value in [
            "",
            "../outside",
            "/absolute",
            "C:/absolute",
            "notes\\todo.md",
        ] {
            project.write_config(&format!("[plan]\nexclude = [{value:?}]"));
            assert!(matches!(
                load_project_config(&project.0),
                Err(ConfigError::InvalidPath { .. })
            ));
        }
    }
}
