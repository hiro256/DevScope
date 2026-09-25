//! Optional, project-local observation policy.
//!
//! Narrow, project-local observation policy.

use std::{
    error::Error,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

pub const CONFIG_PATH: &str = ".devscope/config.toml";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectConfig {
    plan: PlanConfig,
    artifact: ArtifactConfig,
    verify: VerifyConfig,
    activity: ActivityConfig,
}

impl ProjectConfig {
    pub fn artifact(&self) -> &ArtifactConfig {
        &self.artifact
    }

    pub fn plan(&self) -> &PlanConfig {
        &self.plan
    }

    pub fn verify(&self) -> &VerifyConfig {
        &self.verify
    }

    pub fn activity(&self) -> &ActivityConfig {
        &self.activity
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityConfig {
    excludes: Vec<PathBuf>,
}

impl ActivityConfig {
    pub fn excludes(&self) -> &[PathBuf] {
        &self.excludes
    }

    pub fn excludes_path(&self, candidate: &Path) -> bool {
        self.excludes
            .iter()
            .any(|excluded| candidate == excluded || candidate.starts_with(excluded))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtifactConfig {
    path: Option<PathBuf>,
}
impl ArtifactConfig {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanConfig {
    includes: Option<Vec<PathBuf>>,
    excludes: Vec<PathBuf>,
}

impl PlanConfig {
    pub fn includes(&self) -> Option<&[PathBuf]> {
        self.includes.as_deref()
    }

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifyConfig {
    build: Option<VerifyCommandConfig>,
    test: Option<VerifyCommandConfig>,
    excludes: Vec<PathBuf>,
}
impl VerifyConfig {
    pub fn build(&self) -> Option<&VerifyCommandConfig> {
        self.build.as_ref()
    }
    pub fn test(&self) -> Option<&VerifyCommandConfig> {
        self.test.as_ref()
    }
    pub fn excludes(&self) -> &[PathBuf] {
        &self.excludes
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyCommandConfig {
    program: String,
    args: Vec<String>,
}
impl VerifyCommandConfig {
    pub fn program(&self) -> &str {
        &self.program
    }
    pub fn args(&self) -> &[String] {
        &self.args
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
        setting: &'static str,
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
                setting,
                value,
                reason,
            } => {
                write!(
                    formatter,
                    "invalid {setting} path `{value}` in {}: {reason}",
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
    let config = parse_project_config(&path, &contents)?;
    validate_plan_includes(root, &path, config.plan())?;
    Ok(config)
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
        if key != "plan" && key != "artifact" && key != "verify" && key != "activity" {
            return Err(unknown_key(path, key, "top level"));
        }
    }

    let plan_table = match table.get("plan") {
        Some(value) => Some(value.as_table().ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "`plan` must be a table".to_owned(),
        })?),
        None => None,
    };
    for key in plan_table.into_iter().flat_map(|table| table.keys()) {
        if key != "include" && key != "exclude" {
            return Err(unknown_key(path, key, "[plan]"));
        }
    }

    let includes = plan_table
        .and_then(|table| table.get("include"))
        .map(|value| parse_excludes(path, Some(value), "plan.include"))
        .transpose()?;

    let excludes = match plan_table.and_then(|table| table.get("exclude")) {
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
                validate_exclude_path(path, "plan.exclude", value)
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    let artifact = match table.get("artifact") {
        None => ArtifactConfig::default(),
        Some(value) => {
            let table = value.as_table().ok_or_else(|| ConfigError::InvalidSchema {
                path: path.to_path_buf(),
                message: "`artifact` must be a table".into(),
            })?;
            if table.keys().any(|key| key != "path") {
                return Err(ConfigError::InvalidSchema {
                    path: path.to_path_buf(),
                    message: "unknown key in [artifact]".into(),
                });
            }
            let value = table
                .get("path")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| ConfigError::InvalidSchema {
                    path: path.to_path_buf(),
                    message: "`artifact.path` must be a string".into(),
                })?;
            ArtifactConfig {
                path: Some(PathBuf::from(value)),
            }
        }
    };
    let verify = parse_verify_config(path, table.get("verify"))?;
    let activity = parse_activity_config(path, table.get("activity"))?;
    Ok(ProjectConfig {
        plan: PlanConfig { includes, excludes },
        artifact,
        verify,
        activity,
    })
}

fn validate_plan_includes(
    root: &Path,
    config_path: &Path,
    plan: &PlanConfig,
) -> Result<(), ConfigError> {
    let Some(includes) = plan.includes() else {
        return Ok(());
    };
    for include in includes {
        let target = root.join(include);
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Err(ConfigError::InvalidPath {
                    path: config_path.to_path_buf(),
                    setting: "plan.include",
                    value: include.display().to_string(),
                    reason: "path does not exist",
                });
            }
            Err(source) => {
                return Err(ConfigError::Read {
                    path: target,
                    source,
                });
            }
        };
        if metadata.is_dir() {
            continue;
        }
        if !metadata.is_file()
            || !target
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            return Err(ConfigError::InvalidPath {
                path: config_path.to_path_buf(),
                setting: "plan.include",
                value: include.display().to_string(),
                reason: "path must be a Markdown file or directory",
            });
        }
    }
    Ok(())
}

fn parse_activity_config(
    path: &Path,
    value: Option<&toml::Value>,
) -> Result<ActivityConfig, ConfigError> {
    let Some(value) = value else {
        return Ok(ActivityConfig::default());
    };
    let table = value.as_table().ok_or_else(|| ConfigError::InvalidSchema {
        path: path.to_path_buf(),
        message: "`activity` must be a table".into(),
    })?;
    for key in table.keys() {
        if key != "exclude" {
            return Err(unknown_key(path, key, "[activity]"));
        }
    }
    Ok(ActivityConfig {
        excludes: parse_activity_excludes(path, table.get("exclude"))?,
    })
}

fn parse_verify_config(
    path: &Path,
    value: Option<&toml::Value>,
) -> Result<VerifyConfig, ConfigError> {
    let Some(value) = value else {
        return Ok(VerifyConfig::default());
    };
    let table = value.as_table().ok_or_else(|| ConfigError::InvalidSchema {
        path: path.to_path_buf(),
        message: "`verify` must be a table".into(),
    })?;
    for key in table.keys() {
        if key != "exclude" && key != "build" && key != "test" {
            return Err(unknown_key(path, key, "[verify]"));
        }
    }
    Ok(VerifyConfig {
        build: parse_verify_command(path, table.get("build"), "build")?,
        test: parse_verify_command(path, table.get("test"), "test")?,
        excludes: parse_excludes(path, table.get("exclude"), "verify.exclude")?,
    })
}

fn parse_verify_command(
    path: &Path,
    value: Option<&toml::Value>,
    kind: &str,
) -> Result<Option<VerifyCommandConfig>, ConfigError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let table = value.as_table().ok_or_else(|| ConfigError::InvalidSchema {
        path: path.to_path_buf(),
        message: format!("`verify.{kind}` must be a table"),
    })?;
    for key in table.keys() {
        if key != "program" && key != "args" {
            return Err(unknown_key(path, key, &format!("[verify.{kind}]")));
        }
    }
    let program = table
        .get("program")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: format!("`verify.{kind}.program` must be a non-empty string"),
        })?;
    if program.trim().is_empty() {
        return Err(ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: format!("`verify.{kind}.program` must be a non-empty string"),
        });
    }
    let args = match table.get("args") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(|| ConfigError::InvalidSchema {
                path: path.to_path_buf(),
                message: format!("`verify.{kind}.args` must be an array of strings"),
            })?
            .iter()
            .map(|arg| {
                arg.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| ConfigError::InvalidSchema {
                        path: path.to_path_buf(),
                        message: format!("`verify.{kind}.args` must be an array of strings"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(Some(VerifyCommandConfig {
        program: program.into(),
        args,
    }))
}

fn parse_excludes(
    path: &Path,
    value: Option<&toml::Value>,
    setting: &'static str,
) -> Result<Vec<PathBuf>, ConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: format!("`{setting}` must be an array of strings"),
        })?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| ConfigError::InvalidSchema {
                    path: path.to_path_buf(),
                    message: format!("`{setting}` must be an array of strings"),
                })
                .and_then(|value| validate_exclude_path(path, setting, value))
        })
        .collect()
}
fn parse_activity_excludes(
    path: &Path,
    value: Option<&toml::Value>,
) -> Result<Vec<PathBuf>, ConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| ConfigError::InvalidSchema {
            path: path.to_path_buf(),
            message: "`activity.exclude` must be an array of strings".to_owned(),
        })?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| ConfigError::InvalidSchema {
                    path: path.to_path_buf(),
                    message: "`activity.exclude` must be an array of strings".to_owned(),
                })
                .and_then(|value| validate_activity_exclude_path(path, value))
        })
        .collect()
}

fn validate_activity_exclude_path(config_path: &Path, value: &str) -> Result<PathBuf, ConfigError> {
    let normalized = value.replace('\\', "/");
    let path = validate_exclude_path(config_path, "activity.exclude", &normalized)?;
    if path == Path::new(".") {
        return Err(ConfigError::InvalidPath {
            path: config_path.to_path_buf(),
            setting: "activity.exclude",
            value: value.to_owned(),
            reason: "path must not exclude the project root",
        });
    }
    Ok(path)
}

fn unknown_key(path: &Path, key: &str, scope: &str) -> ConfigError {
    ConfigError::InvalidSchema {
        path: path.to_path_buf(),
        message: format!("unknown key `{key}` in {scope}"),
    }
}

fn validate_exclude_path(
    config_path: &Path,
    setting: &'static str,
    value: &str,
) -> Result<PathBuf, ConfigError> {
    let invalid = |reason| ConfigError::InvalidPath {
        path: config_path.to_path_buf(),
        setting,
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

        fn write(&self, relative: &str, contents: &str) {
            let path = self.0.join(relative);
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
    fn parses_activity_excludes_separately_from_verify_and_normalizes_windows_separators() {
        let project = TempProject::new();
        project.write_config(
            "[verify]\nexclude = [\"verify-output\"]\n[activity]\nexclude = ['.devscope\\evidence', \"generated/cache/\"]\n",
        );
        let config = load_project_config(&project.0).unwrap();
        assert_eq!(config.verify().excludes(), [PathBuf::from("verify-output")]);
        assert_eq!(
            config.activity().excludes(),
            [
                PathBuf::from(".devscope/evidence"),
                PathBuf::from("generated/cache")
            ]
        );
        assert!(
            config
                .activity()
                .excludes_path(Path::new(".devscope/evidence/run/result.json"))
        );
        assert!(
            !config
                .activity()
                .excludes_path(Path::new(".devscope/evidence-old/result.json"))
        );
    }

    #[test]
    fn rejects_invalid_activity_exclude_paths() {
        let project = TempProject::new();
        for value in [
            "",
            ".",
            "..",
            "../outside",
            "C:\\temp",
            "/tmp",
            "generated/../src",
            "generated/*",
            "!generated",
        ] {
            project.write_config(&format!("[activity]\nexclude = [{value:?}]"));
            assert!(matches!(
                load_project_config(&project.0),
                Err(ConfigError::InvalidPath { .. })
            ));
        }
    }

    #[test]
    fn plan_include_distinguishes_omitted_empty_and_explicit_paths() {
        let project = TempProject::new();
        assert_eq!(
            load_project_config(&project.0).unwrap().plan().includes(),
            None
        );

        project.write_config("[plan]\ninclude = []");
        assert_eq!(
            load_project_config(&project.0).unwrap().plan().includes(),
            Some([].as_slice())
        );

        project.write("docs/roadmap.md", "- [ ] accepted");
        project.write("docs/plans/next.md", "- [ ] next");
        project.write_config("[plan]\ninclude = [\"docs/roadmap.md\", \"docs/plans\", \".\"]");
        assert_eq!(
            load_project_config(&project.0).unwrap().plan().includes(),
            Some(
                [
                    PathBuf::from("docs/roadmap.md"),
                    PathBuf::from("docs/plans"),
                    PathBuf::from("."),
                ]
                .as_slice()
            )
        );
    }

    #[test]
    fn rejects_invalid_plan_include_syntax_and_schema() {
        let project = TempProject::new();
        for value in [
            "",
            "../outside",
            "/absolute",
            "C:/absolute",
            "notes\\todo.md",
            "docs/*",
            "!docs",
        ] {
            project.write_config(&format!("[plan]\ninclude = [{value:?}]"));
            assert!(matches!(
                load_project_config(&project.0),
                Err(ConfigError::InvalidPath {
                    setting: "plan.include",
                    ..
                })
            ));
        }
        for value in ["true", "[1]", "\"docs\""] {
            project.write_config(&format!("[plan]\ninclude = {value}"));
            assert!(matches!(
                load_project_config(&project.0),
                Err(ConfigError::InvalidSchema { .. })
            ));
        }
    }

    #[test]
    fn rejects_missing_or_non_markdown_plan_include() {
        let project = TempProject::new();
        project.write_config("[plan]\ninclude = [\"missing.md\"]");
        assert!(matches!(
            load_project_config(&project.0),
            Err(ConfigError::InvalidPath {
                reason: "path does not exist",
                ..
            })
        ));

        project.write("notes.txt", "text");
        project.write_config("[plan]\ninclude = [\"notes.txt\"]");
        assert!(matches!(
            load_project_config(&project.0),
            Err(ConfigError::InvalidPath {
                reason: "path must be a Markdown file or directory",
                ..
            })
        ));
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

        project.write_config("[plan]\nunexpected = true");
        assert!(matches!(
            load_project_config(&project.0),
            Err(ConfigError::InvalidSchema { .. })
        ));
    }

    #[test]
    fn parses_optional_single_artifact_target_alongside_plan() {
        let project = TempProject::new();
        project.write_config("[artifact]\npath = \"target/debug/devscope.exe\"");
        assert_eq!(
            load_project_config(&project.0).unwrap().artifact().path(),
            Some(Path::new("target/debug/devscope.exe"))
        );
        project.write_config("[plan]\nexclude = [\"target\"]\n\n[artifact]\npath = \"output.bin\"");
        let config = load_project_config(&project.0).unwrap();
        assert!(config.plan().excludes_path(Path::new("target/file")));
        assert_eq!(config.artifact().path(), Some(Path::new("output.bin")));
    }

    #[test]
    fn rejects_invalid_artifact_schema() {
        let project = TempProject::new();
        for contents in [
            "artifact = \"foo\"",
            "[artifact]\npath = 123",
            "[artifact]\nfoo = \"bar\"",
        ] {
            project.write_config(contents);
            assert!(matches!(
                load_project_config(&project.0),
                Err(ConfigError::InvalidSchema { .. })
            ));
        }
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
    #[test]
    fn parses_verify_commands_excludes_and_existing_sections() {
        let project = TempProject::new();
        project.write_config("[plan]\nexclude = [\"generated\"]\n[artifact]\npath = \"output.bin\"\n[verify]\nexclude = [\"bin\"]\n[verify.build]\nprogram = \"dotnet\"\nargs = [\"build\"]\n[verify.test]\nprogram = \"python\"\nargs = [\"-m\", \"pytest\"]");
        let config = load_project_config(&project.0).unwrap();
        assert_eq!(config.verify().build().unwrap().program(), "dotnet");
        assert_eq!(config.verify().build().unwrap().args(), ["build"]);
        assert_eq!(config.verify().test().unwrap().args(), ["-m", "pytest"]);
        assert_eq!(config.verify().excludes(), [PathBuf::from("bin")]);
        assert_eq!(config.artifact().path(), Some(Path::new("output.bin")));
    }

    #[test]
    fn verify_args_default_to_empty_and_invalid_verify_values_are_rejected() {
        let project = TempProject::new();
        project.write_config("[verify.test]\nprogram = \"pytest\"");
        assert!(
            load_project_config(&project.0)
                .unwrap()
                .verify()
                .test()
                .unwrap()
                .args()
                .is_empty()
        );
        for contents in [
            "verify = \"foo\"",
            "[verify]\nfoo = \"bar\"",
            "[verify.build]\nargs = [\"build\"]",
            "[verify.build]\nprogram = 123",
            "[verify.build]\nprogram = \"dotnet\"\nargs = \"build\"",
            "[verify.build]\nprogram = \"dotnet\"\nargs = [\"build\", 1]",
            "[verify.build]\nprogram = \"\"",
            "[verify.build]\nprogram = \"   \"",
            "[verify]\nexclude = [\"generated/*\"]",
            "[verify]\nexclude = [\"!generated\"]",
            "[verify]\nexclude = [\"../outside\"]",
            "[verify]\nexclude = [\"notes\\\\todo\"]",
        ] {
            project.write_config(contents);
            assert!(matches!(
                load_project_config(&project.0),
                Err(ConfigError::InvalidSchema { .. }) | Err(ConfigError::InvalidPath { .. })
            ));
        }
    }
}
