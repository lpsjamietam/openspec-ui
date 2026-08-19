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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub sources: Vec<SourceConfig>,
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

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusProvider {
    #[default]
    Auto,
    Filesystem,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitContext {
    pub worktree_root: String,
    pub branch: Option<String>,
    pub commit: String,
    pub detached: bool,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub valid: bool,
    pub track: Option<String>,
    pub target_branch: Option<String>,
    pub git: Option<GitContext>,
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
        Ok(config)
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
                    tracing::warn!("Source path does not exist or is not a directory: {:?}", path);
                }
                Source {
                    id: s.name.clone(),
                    name: s.name.clone(),
                    git: valid.then(|| discover_git_context(&path)).flatten(),
                    path,
                    valid,
                    track: s.track.clone(),
                    target_branch: s.target_branch.clone(),
                }
            })
            .collect()
    }
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
        assert_eq!(config.sources[0].track, None);
    }

    #[test]
    fn explicit_safety_overrides_are_preserved() {
        let config: Config = serde_json::from_str(
            r#"{"sources":[],"readOnly":false,"bindAddress":"0.0.0.0","deduplicateChanges":false,"statusProvider":"filesystem","openspecCommand":"openspec-dev"}"#,
        )
        .unwrap();

        assert!(!config.read_only);
        assert_eq!(config.bind_address, "0.0.0.0");
        assert!(!config.deduplicate_changes);
        assert_eq!(config.status_provider, StatusProvider::Filesystem);
        assert_eq!(config.openspec_command, "openspec-dev");
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
