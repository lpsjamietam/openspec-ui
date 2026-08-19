use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("Invalid configuration: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfig {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    #[default]
    Filesystem,
    Github,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GithubConfig {
    pub repository: String,
    pub specs_ref: String,
    pub changes_base_ref: String,
    pub pull_request_targets: Vec<String>,
    pub cache_path: String,
    #[serde(default = "default_reconciliation_interval_seconds")]
    pub reconciliation_interval_seconds: u64,
    #[serde(default = "default_max_pull_requests")]
    pub max_pull_requests: usize,
    #[serde(default = "default_github_api_base_url")]
    pub api_base_url: String,
    #[serde(default = "default_max_file_bytes")]
    pub max_file_bytes: usize,
    #[serde(default = "default_max_snapshot_bytes")]
    pub max_snapshot_bytes: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub source_mode: SourceMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubConfig>,
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specs_source_id: Option<String>,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_read_only")]
    pub read_only: bool,
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    #[serde(default = "default_deduplicate_changes")]
    pub deduplicate_changes: bool,
    #[serde(default)]
    pub status_provider: StatusProvider,
    #[serde(default = "default_openspec_command")]
    pub openspec_command: String,
}

fn default_port() -> u16 {
    3000
}

fn default_read_only() -> bool {
    true
}

fn default_bind_address() -> String {
    "127.0.0.1".to_string()
}

fn default_deduplicate_changes() -> bool {
    true
}

fn default_openspec_command() -> String {
    "openspec".to_string()
}

fn default_reconciliation_interval_seconds() -> u64 {
    15 * 60
}

fn default_max_pull_requests() -> usize {
    50
}

fn default_github_api_base_url() -> String {
    "https://api.github.com".to_string()
}

fn default_max_file_bytes() -> usize {
    1024 * 1024
}

fn default_max_snapshot_bytes() -> usize {
    25 * 1024 * 1024
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusProvider {
    #[default]
    Auto,
    Filesystem,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitContext {
    pub worktree_root: String,
    pub branch: Option<String>,
    pub commit: String,
    pub detached: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestProvenance {
    pub number: u64,
    pub head_ref: String,
    pub base_ref: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GithubProvenance {
    pub repository: String,
    pub ref_name: String,
    pub commit: String,
    pub html_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequestProvenance>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MergedChange {
    pub change_name: String,
    pub pull_request_number: u64,
    pub merged_at: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub valid: bool,
    pub track: Option<String>,
    pub target_branch: Option<String>,
    pub git: Option<GitContext>,
    pub github: Option<GithubProvenance>,
    pub canonical_specs: bool,
    pub include_changes: bool,
    #[serde(default)]
    pub merged_changes: Vec<MergedChange>,
}

fn git_output(path: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub fn discover_git_context(path: &Path) -> Option<GitContext> {
    let worktree_root = git_output(path, &["rev-parse", "--show-toplevel"])?;
    let commit = git_output(path, &["rev-parse", "--short=12", "HEAD"])?;
    let branch = git_output(path, &["symbolic-ref", "--quiet", "--short", "HEAD"]);

    Some(GitContext {
        worktree_root,
        detached: branch.is_none(),
        branch,
        commit,
    })
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        match self.source_mode {
            SourceMode::Filesystem => {
                if self.github.is_some() {
                    return Err(ConfigError::Validation(
                        "github settings require sourceMode=github".to_string(),
                    ));
                }
            }
            SourceMode::Github => {
                if !self.read_only {
                    return Err(ConfigError::Validation(
                        "GitHub mode must run with readOnly=true".to_string(),
                    ));
                }
                if !self.sources.is_empty() || self.specs_source_id.is_some() {
                    return Err(ConfigError::Validation(
                        "GitHub mode cannot be combined with filesystem sources or specsSourceId"
                            .to_string(),
                    ));
                }
                let github = self.github.as_ref().ok_or_else(|| {
                    ConfigError::Validation(
                        "sourceMode=github requires a github configuration".to_string(),
                    )
                })?;
                validate_repository(&github.repository)?;
                validate_ref("specsRef", &github.specs_ref)?;
                validate_ref("changesBaseRef", &github.changes_base_ref)?;
                if github.pull_request_targets.is_empty() {
                    return Err(ConfigError::Validation(
                        "github.pullRequestTargets must contain at least one branch".to_string(),
                    ));
                }
                for target in &github.pull_request_targets {
                    validate_ref("pullRequestTargets", target)?;
                }
                if github.cache_path.trim().is_empty() {
                    return Err(ConfigError::Validation(
                        "github.cachePath must not be empty".to_string(),
                    ));
                }
                if github.reconciliation_interval_seconds == 0 {
                    return Err(ConfigError::Validation(
                        "github.reconciliationIntervalSeconds must be greater than zero"
                            .to_string(),
                    ));
                }
                if github.max_pull_requests == 0 {
                    return Err(ConfigError::Validation(
                        "github.maxPullRequests must be greater than zero".to_string(),
                    ));
                }
                if github.max_file_bytes == 0 || github.max_snapshot_bytes < github.max_file_bytes {
                    return Err(ConfigError::Validation(
                        "GitHub snapshot size limits are invalid".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn is_github_mode(&self) -> bool {
        self.source_mode == SourceMode::Github
    }

    pub fn resolve_sources(&self, base_path: &Path) -> Vec<Source> {
        self.sources
            .iter()
            .map(|s| {
                let path = if s.path.starts_with("./") || s.path.starts_with("../") {
                    base_path.join(&s.path)
                } else {
                    PathBuf::from(&s.path)
                };
                let valid = path.exists() && path.is_dir();
                if !valid {
                    tracing::warn!(
                        "Source path does not exist or is not a directory: {:?}",
                        path
                    );
                }
                Source {
                    id: s.name.clone(),
                    name: s.name.clone(),
                    git: valid.then(|| discover_git_context(&path)).flatten(),
                    github: None,
                    canonical_specs: self
                        .specs_source_id
                        .as_deref()
                        .is_some_and(|id| id == s.name),
                    include_changes: true,
                    merged_changes: Vec::new(),
                    path,
                    valid,
                    track: s.track.clone(),
                    target_branch: s.target_branch.clone(),
                }
            })
            .collect()
    }
}

fn validate_repository(repository: &str) -> Result<(), ConfigError> {
    let mut parts = repository.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty()
        || name.is_empty()
        || parts.next().is_some()
        || !owner.chars().all(valid_repo_character)
        || !name.chars().all(valid_repo_character)
    {
        return Err(ConfigError::Validation(
            "github.repository must use owner/name syntax".to_string(),
        ));
    }
    Ok(())
}

fn valid_repo_character(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.')
}

fn validate_ref(field: &str, value: &str) -> Result<(), ConfigError> {
    let invalid = value.trim().is_empty()
        || value.starts_with('-')
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains("..")
        || value.contains("@{")
        || value.contains('\0')
        || value.chars().any(char::is_whitespace)
        || value
            .chars()
            .any(|c| matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'));
    if invalid {
        return Err(ConfigError::Validation(format!(
            "github.{field} contains an invalid Git ref"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn existing_config_uses_safe_defaults() {
        let config: Config = serde_json::from_str(
            r#"{"sources":[{"name":"reach","path":"/tmp/reach/openspec"}],"port":4000}"#,
        )
        .unwrap();

        assert!(config.read_only);
        assert_eq!(config.bind_address, "127.0.0.1");
        assert!(config.deduplicate_changes);
        assert_eq!(config.status_provider, StatusProvider::Auto);
        assert_eq!(config.openspec_command, "openspec");
        assert_eq!(config.specs_source_id, None);
        assert_eq!(config.sources[0].track, None);
        assert_eq!(config.source_mode, SourceMode::Filesystem);
        assert!(config.github.is_none());
    }

    #[test]
    fn valid_github_mode_uses_hosted_defaults() {
        let config: Config = serde_json::from_str(
            r#"{
                "sourceMode":"github",
                "github":{
                    "repository":"ToruAI/openspec-ui",
                    "specsRef":"demo/main",
                    "changesBaseRef":"demo/main",
                    "pullRequestTargets":["demo/main"],
                    "cachePath":"/data/openspec-ui"
                }
            }"#,
        )
        .unwrap();

        config.validate().unwrap();
        let github = config.github.unwrap();
        assert_eq!(github.reconciliation_interval_seconds, 900);
        assert_eq!(github.max_pull_requests, 50);
        assert!(config.sources.is_empty());
    }

    #[test]
    fn github_mode_rejects_filesystem_authority() {
        let config: Config = serde_json::from_str(
            r#"{
                "sourceMode":"github",
                "sources":[{"name":"local","path":"/tmp/openspec"}],
                "github":{
                    "repository":"ToruAI/openspec-ui",
                    "specsRef":"demo/main",
                    "changesBaseRef":"demo/main",
                    "pullRequestTargets":["demo/main"],
                    "cachePath":"/data/openspec-ui"
                }
            }"#,
        )
        .unwrap();

        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("cannot be combined"));
    }

    #[test]
    fn github_mode_rejects_invalid_repository_and_refs() {
        let mut config: Config = serde_json::from_str(
            r#"{
                "sourceMode":"github",
                "github":{
                    "repository":"https://github.com/ToruAI/openspec-ui",
                    "specsRef":"demo/../main",
                    "changesBaseRef":"demo/main",
                    "pullRequestTargets":["demo/main"],
                    "cachePath":"/data/openspec-ui"
                }
            }"#,
        )
        .unwrap();
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("owner/name"));

        config.github.as_mut().unwrap().repository = "ToruAI/openspec-ui".to_string();
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("invalid Git ref"));
    }

    #[test]
    fn explicit_safety_overrides_are_preserved() {
        let config: Config = serde_json::from_str(
            r#"{"sources":[],"specsSourceId":"demo-base","readOnly":false,"bindAddress":"0.0.0.0","deduplicateChanges":false,"statusProvider":"filesystem","openspecCommand":"openspec-dev"}"#,
        )
        .unwrap();

        assert!(!config.read_only);
        assert_eq!(config.bind_address, "0.0.0.0");
        assert!(!config.deduplicate_changes);
        assert_eq!(config.status_provider, StatusProvider::Filesystem);
        assert_eq!(config.openspec_command, "openspec-dev");
        assert_eq!(config.specs_source_id.as_deref(), Some("demo-base"));
    }

    #[test]
    fn discovers_branch_and_commit_for_a_git_worktree() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ospec-ui-git-{suffix}"));
        let openspec = root.join("openspec");
        std::fs::create_dir_all(&openspec).unwrap();
        std::fs::write(root.join("README.md"), "test\n").unwrap();

        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .status()
                .unwrap()
        };
        assert!(git(&["init", "-b", "codex/test-worktree"]).success());
        assert!(git(&["add", "."]).success());
        assert!(git(&[
            "-c",
            "user.name=OpenSpec UI Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "fixture",
        ])
        .success());

        let context = discover_git_context(&openspec).expect("Git context should be discovered");
        assert_eq!(context.branch.as_deref(), Some("codex/test-worktree"));
        assert!(!context.detached);
        assert!(!context.commit.is_empty());
        assert_eq!(Path::new(&context.worktree_root), root);

        std::fs::remove_dir_all(&root).ok();
    }
}
