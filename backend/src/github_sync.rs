use crate::{
    config::{GithubConfig, GithubProvenance, MergedChange, PullRequestProvenance, Source},
    parser,
    snapshot::{ActiveSnapshot, ContributingRef, SyncFailure, SyncHealth, SyncState},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

const CURRENT_SNAPSHOT_FILE: &str = "current-snapshot.json";

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("missing deployment secret: {0}")]
    MissingSecret(&'static str),
    #[error("invalid deployment secret: {0}")]
    InvalidSecret(&'static str),
    #[error("GitHub authentication failed")]
    Authentication,
    #[error("GitHub request failed with status {0}")]
    Github(StatusCode),
    #[error("GitHub response was invalid")]
    GithubResponse,
    #[error("snapshot content failed validation: {0}")]
    Snapshot(&'static str),
    #[error("snapshot cache operation failed")]
    Cache,
}

impl SyncError {
    pub fn category(&self) -> &'static str {
        match self {
            Self::MissingSecret(_) | Self::InvalidSecret(_) | Self::Authentication => {
                "authentication"
            }
            Self::Github(_) | Self::GithubResponse => "github",
            Self::Snapshot(_) => "validation",
            Self::Cache => "cache",
        }
    }

    pub fn safe_summary(&self) -> String {
        match self {
            Self::MissingSecret(name) => format!("Required secret {name} is unavailable"),
            Self::InvalidSecret(name) => format!("Required secret {name} is invalid"),
            Self::Authentication => "GitHub App authentication failed".to_string(),
            Self::Github(status) => format!("GitHub returned HTTP {}", status.as_u16()),
            Self::GithubResponse => "GitHub returned an invalid response".to_string(),
            Self::Snapshot(reason) => format!("Remote OpenSpec content was rejected: {reason}"),
            Self::Cache => "The snapshot cache could not be updated".to_string(),
        }
    }
}

pub struct GithubSecrets {
    app_id: u64,
    installation_id: u64,
    private_key: String,
    webhook_secret: String,
}

impl GithubSecrets {
    pub fn load() -> Result<Self, SyncError> {
        let app_id = required_secret("GITHUB_APP_ID", "GITHUB_APP_ID_FILE")?
            .parse()
            .map_err(|_| SyncError::InvalidSecret("GITHUB_APP_ID"))?;
        let installation_id = required_secret(
            "GITHUB_APP_INSTALLATION_ID",
            "GITHUB_APP_INSTALLATION_ID_FILE",
        )?
        .parse()
        .map_err(|_| SyncError::InvalidSecret("GITHUB_APP_INSTALLATION_ID"))?;
        let private_key = required_secret("GITHUB_APP_PRIVATE_KEY", "GITHUB_APP_PRIVATE_KEY_FILE")?
            .replace("\\n", "\n");
        if !private_key.contains("BEGIN") {
            return Err(SyncError::InvalidSecret("GITHUB_APP_PRIVATE_KEY"));
        }
        let webhook_secret =
            required_secret("GITHUB_WEBHOOK_SECRET", "GITHUB_WEBHOOK_SECRET_FILE")?;
        if webhook_secret.is_empty() {
            return Err(SyncError::InvalidSecret("GITHUB_WEBHOOK_SECRET"));
        }
        Ok(Self {
            app_id,
            installation_id,
            private_key,
            webhook_secret,
        })
    }

    pub fn webhook_secret(&self) -> &[u8] {
        self.webhook_secret.as_bytes()
    }

    #[cfg(test)]
    pub(crate) fn fixture(webhook_secret: &str) -> Self {
        Self {
            app_id: 1,
            installation_id: 2,
            private_key: "unused fixture key".to_string(),
            webhook_secret: webhook_secret.to_string(),
        }
    }
}

fn required_secret(
    environment_name: &'static str,
    file_environment_name: &'static str,
) -> Result<String, SyncError> {
    if let Ok(value) = env::var(environment_name) {
        let value = value.trim().to_string();
        if !value.is_empty() {
            return Ok(value);
        }
    }
    if let Ok(path) = env::var(file_environment_name) {
        let value =
            fs::read_to_string(path).map_err(|_| SyncError::MissingSecret(environment_name))?;
        let value = value.trim().to_string();
        if !value.is_empty() {
            return Ok(value);
        }
    }
    Err(SyncError::MissingSecret(environment_name))
}

#[derive(Clone)]
pub struct GithubAppClient {
    http: reqwest::Client,
    config: GithubConfig,
    secrets: Arc<GithubSecrets>,
    token: Arc<Mutex<Option<CachedToken>>>,
}

#[derive(Clone)]
struct CachedToken {
    value: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Serialize)]
struct AppClaims {
    iat: i64,
    exp: i64,
    iss: String,
}

#[derive(Deserialize)]
struct InstallationTokenResponse {
    token: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Serialize)]
struct InstallationTokenRequest {
    repositories: Vec<String>,
    permissions: HashMap<&'static str, &'static str>,
}

impl GithubAppClient {
    pub fn new(config: GithubConfig, secrets: Arc<GithubSecrets>) -> Result<Self, SyncError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("openspec-ui")
            .build()
            .map_err(|_| SyncError::Authentication)?;
        Ok(Self {
            http,
            config,
            secrets,
            token: Arc::new(Mutex::new(None)),
        })
    }

    async fn installation_token(&self) -> Result<String, SyncError> {
        let mut cached = self.token.lock().await;
        if let Some(token) = cached.as_ref() {
            if token.expires_at > Utc::now() + Duration::minutes(2) {
                return Ok(token.value.clone());
            }
        }

        let now = Utc::now().timestamp();
        let claims = AppClaims {
            iat: now - 60,
            exp: now + 9 * 60,
            iss: self.secrets.app_id.to_string(),
        };
        let key = EncodingKey::from_rsa_pem(self.secrets.private_key.as_bytes())
            .map_err(|_| SyncError::InvalidSecret("GITHUB_APP_PRIVATE_KEY"))?;
        let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key)
            .map_err(|_| SyncError::Authentication)?;
        let url = format!(
            "{}/app/installations/{}/access_tokens",
            self.config.api_base_url.trim_end_matches('/'),
            self.secrets.installation_id
        );
        let response = self
            .http
            .post(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .bearer_auth(jwt)
            .json(&InstallationTokenRequest {
                repositories: vec![self
                    .config
                    .repository
                    .split_once('/')
                    .map(|(_, name)| name)
                    .unwrap_or(&self.config.repository)
                    .to_string()],
                permissions: HashMap::from([("contents", "read"), ("pull_requests", "read")]),
            })
            .send()
            .await
            .map_err(|_| SyncError::Authentication)?;
        if !response.status().is_success() {
            return Err(SyncError::Authentication);
        }
        let response: InstallationTokenResponse = response
            .json()
            .await
            .map_err(|_| SyncError::GithubResponse)?;
        let value = response.token.clone();
        *cached = Some(CachedToken {
            value: response.token,
            expires_at: response.expires_at,
        });
        Ok(value)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, SyncError> {
        let token = self.installation_token().await?;
        let url = format!(
            "{}/{}",
            self.config.api_base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let response = self
            .http
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| SyncError::GithubResponse)?;
        if !response.status().is_success() {
            return Err(SyncError::Github(response.status()));
        }
        response.json().await.map_err(|_| SyncError::GithubResponse)
    }

    async fn resolve_ref(&self, ref_name: &str) -> Result<String, SyncError> {
        let response: RefResponse = self
            .get(&format!(
                "repos/{}/git/ref/heads/{}",
                self.config.repository,
                urlencoding::encode(ref_name)
            ))
            .await?;
        Ok(response.object.sha)
    }

    async fn open_pull_requests(&self) -> Result<Vec<PullRequest>, SyncError> {
        let mut pulls = Vec::new();
        for target in &self.config.pull_request_targets {
            let response: Vec<PullRequest> = self
                .get(&format!(
                    "repos/{}/pulls?state=open&base={}&per_page=100",
                    self.config.repository,
                    urlencoding::encode(target)
                ))
                .await?;
            pulls.extend(response);
            if pulls.len() > self.config.max_pull_requests {
                return Err(SyncError::Snapshot("eligible pull-request limit exceeded"));
            }
        }
        pulls.sort_by_key(|pull| pull.number);
        pulls.dedup_by_key(|pull| pull.number);
        Ok(pulls)
    }

    async fn merged_changes(&self) -> Result<Vec<MergedChange>, SyncError> {
        let pulls: Vec<PullRequest> = self
            .get(&format!(
                "repos/{}/pulls?state=closed&base={}&sort=updated&direction=desc&per_page=100",
                self.config.repository,
                urlencoding::encode(&self.config.changes_base_ref)
            ))
            .await?;
        let mut by_change = HashMap::<String, MergedChange>::new();
        for pull in pulls.into_iter().filter(|pull| pull.merged_at.is_some()) {
            let files: Vec<PullFile> = self
                .get(&format!(
                    "repos/{}/pulls/{}/files?per_page=100",
                    self.config.repository, pull.number
                ))
                .await?;
            for change_name in extract_change_names(files.iter().map(|file| file.filename.as_str()))
            {
                by_change
                    .entry(change_name.clone())
                    .or_insert_with(|| MergedChange {
                        change_name,
                        pull_request_number: pull.number,
                        merged_at: pull.merged_at.clone().unwrap_or_default(),
                        html_url: pull.html_url.clone(),
                    });
            }
        }
        Ok(by_change.into_values().collect())
    }

    async fn materialize_ref(&self, commit: &str, destination: &Path) -> Result<(), SyncError> {
        let tree: TreeResponse = self
            .get(&format!(
                "repos/{}/git/trees/{}?recursive=1",
                self.config.repository, commit
            ))
            .await?;
        if tree.truncated {
            return Err(SyncError::Snapshot("Git tree response was truncated"));
        }
        fs::create_dir_all(destination).map_err(|_| SyncError::Cache)?;
        let mut total_bytes = 0usize;
        for entry in tree.tree {
            let Some(relative) = entry.path.strip_prefix("openspec/") else {
                continue;
            };
            if entry.kind != "blob" {
                continue;
            }
            if entry.mode == "120000" {
                return Err(SyncError::Snapshot("symlinks are not allowed"));
            }
            let relative = safe_relative_path(relative)?;
            let declared_size = entry.size.unwrap_or_default();
            if declared_size > self.config.max_file_bytes {
                return Err(SyncError::Snapshot("file size limit exceeded"));
            }
            let blob: BlobResponse = self
                .get(&format!(
                    "repos/{}/git/blobs/{}",
                    self.config.repository, entry.sha
                ))
                .await?;
            if blob.encoding != "base64" || blob.size > self.config.max_file_bytes {
                return Err(SyncError::Snapshot("invalid or oversized blob"));
            }
            let content = STANDARD
                .decode(blob.content.replace('\n', ""))
                .map_err(|_| SyncError::GithubResponse)?;
            if content.len() > self.config.max_file_bytes {
                return Err(SyncError::Snapshot("file size limit exceeded"));
            }
            total_bytes = total_bytes
                .checked_add(content.len())
                .ok_or(SyncError::Snapshot("snapshot size limit exceeded"))?;
            if total_bytes > self.config.max_snapshot_bytes {
                return Err(SyncError::Snapshot("snapshot size limit exceeded"));
            }
            let output = destination.join(relative);
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).map_err(|_| SyncError::Cache)?;
            }
            fs::write(output, content).map_err(|_| SyncError::Cache)?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct RefResponse {
    object: RefObject,
}

#[derive(Deserialize)]
struct RefObject {
    sha: String,
}

#[derive(Clone, Deserialize)]
struct PullRequest {
    number: u64,
    html_url: String,
    head: PullRef,
    base: PullRef,
    #[serde(default)]
    merged_at: Option<String>,
}

#[derive(Clone, Deserialize)]
struct PullRef {
    #[serde(rename = "ref")]
    ref_name: String,
    sha: String,
}

#[derive(Deserialize)]
struct PullFile {
    filename: String,
}

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    #[serde(default)]
    size: Option<usize>,
}

#[derive(Deserialize)]
struct BlobResponse {
    content: String,
    encoding: String,
    size: usize,
}

pub struct GithubSynchronizer {
    client: GithubAppClient,
    config: GithubConfig,
    cache_path: PathBuf,
}

impl GithubSynchronizer {
    pub fn new(client: GithubAppClient, config: GithubConfig, cache_path: PathBuf) -> Self {
        Self {
            client,
            config,
            cache_path,
        }
    }

    pub async fn reconcile(&self) -> Result<ActiveSnapshot, SyncError> {
        fs::create_dir_all(self.cache_path.join("generations")).map_err(|_| SyncError::Cache)?;
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let staging = self
            .cache_path
            .join("generations")
            .join(format!(".staging-{suffix}"));
        fs::create_dir_all(&staging).map_err(|_| SyncError::Cache)?;

        let result = self.build_generation(&staging).await;
        let (mut sources, revision, refs) = match result {
            Ok(result) => result,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
        };

        let generation = self.cache_path.join("generations").join(&revision);
        if generation.exists() {
            fs::remove_dir_all(&staging).map_err(|_| SyncError::Cache)?;
        } else {
            fs::rename(&staging, &generation).map_err(|_| SyncError::Cache)?;
        }
        for source in &mut sources {
            let directory = source.path.file_name().ok_or(SyncError::Cache)?.to_owned();
            source.path = generation.join(directory);
            source.valid = source.path.is_dir();
        }
        if sources.iter().any(|source| !source.valid) {
            return Err(SyncError::Cache);
        }

        let now = Utc::now().to_rfc3339();
        let snapshot = ActiveSnapshot {
            sources,
            revision: Some(revision.clone()),
            health: SyncHealth {
                state: SyncState::Healthy,
                active_revision: Some(revision),
                contributing_refs: refs,
                last_attempt_at: Some(now.clone()),
                last_success_at: Some(now),
                last_failure: None,
                serving_last_known_good: false,
            },
        };
        persist_snapshot(&self.cache_path, &snapshot)?;
        prune_generations(
            &self.cache_path,
            snapshot.revision.as_deref().unwrap_or_default(),
        )?;
        Ok(snapshot)
    }

    async fn build_generation(
        &self,
        staging: &Path,
    ) -> Result<(Vec<Source>, String, Vec<ContributingRef>), SyncError> {
        let specs_commit = self.client.resolve_ref(&self.config.specs_ref).await?;
        let changes_commit = if self.config.changes_base_ref == self.config.specs_ref {
            specs_commit.clone()
        } else {
            self.client
                .resolve_ref(&self.config.changes_base_ref)
                .await?
        };
        let pulls = self.client.open_pull_requests().await?;
        let merged_changes = self.client.merged_changes().await?;
        let mut source_descriptors = Vec::new();

        source_descriptors.push(SourceDescriptor {
            id: "github-base".to_string(),
            name: self.config.repository.clone(),
            directory: "base".to_string(),
            ref_name: self.config.changes_base_ref.clone(),
            commit: changes_commit.clone(),
            pull_request: None,
            canonical_specs: self.config.specs_ref == self.config.changes_base_ref,
            include_changes: true,
            merged_changes,
        });
        if self.config.specs_ref != self.config.changes_base_ref {
            source_descriptors.push(SourceDescriptor {
                id: "github-specs".to_string(),
                name: format!("{} specs", self.config.repository),
                directory: "specs".to_string(),
                ref_name: self.config.specs_ref.clone(),
                commit: specs_commit,
                pull_request: None,
                canonical_specs: true,
                include_changes: false,
                merged_changes: Vec::new(),
            });
        }
        for pull in pulls {
            source_descriptors.push(SourceDescriptor {
                id: format!("github-pr-{}", pull.number),
                name: format!("{} PR #{}", self.config.repository, pull.number),
                directory: format!("pr-{}", pull.number),
                ref_name: pull.head.ref_name.clone(),
                commit: pull.head.sha.clone(),
                pull_request: Some(PullRequestProvenance {
                    number: pull.number,
                    head_ref: pull.head.ref_name,
                    base_ref: pull.base.ref_name,
                    html_url: pull.html_url,
                }),
                canonical_specs: false,
                include_changes: true,
                merged_changes: Vec::new(),
            });
        }

        for descriptor in &source_descriptors {
            self.client
                .materialize_ref(&descriptor.commit, &staging.join(&descriptor.directory))
                .await?;
            validate_materialized_source(&staging.join(&descriptor.directory))?;
        }
        let revision = snapshot_revision(&source_descriptors);
        let refs = source_descriptors
            .iter()
            .map(|source| ContributingRef {
                source_id: source.id.clone(),
                ref_name: source.ref_name.clone(),
                commit: source.commit.clone(),
                pull_request_number: source.pull_request.as_ref().map(|pull| pull.number),
            })
            .collect();
        let sources = source_descriptors
            .into_iter()
            .map(|source| source.into_source(staging, &self.config.repository))
            .collect();
        Ok((sources, revision, refs))
    }
}

struct SourceDescriptor {
    id: String,
    name: String,
    directory: String,
    ref_name: String,
    commit: String,
    pull_request: Option<PullRequestProvenance>,
    canonical_specs: bool,
    include_changes: bool,
    merged_changes: Vec<MergedChange>,
}

impl SourceDescriptor {
    fn into_source(self, root: &Path, repository: &str) -> Source {
        let github_url = if let Some(pull) = &self.pull_request {
            pull.html_url.clone()
        } else {
            format!("https://github.com/{repository}/tree/{}", self.commit)
        };
        Source {
            id: self.id,
            name: self.name,
            path: root.join(self.directory),
            valid: true,
            track: Some("github".to_string()),
            target_branch: self
                .pull_request
                .as_ref()
                .map(|pull| pull.base_ref.clone())
                .or_else(|| Some(self.ref_name.clone())),
            git: None,
            github: Some(GithubProvenance {
                repository: repository.to_string(),
                ref_name: self.ref_name,
                commit: self.commit,
                html_url: github_url,
                pull_request: self.pull_request,
            }),
            canonical_specs: self.canonical_specs,
            include_changes: self.include_changes,
            merged_changes: self.merged_changes,
        }
    }
}

fn snapshot_revision(sources: &[SourceDescriptor]) -> String {
    let mut values = sources
        .iter()
        .map(|source| format!("{}:{}", source.id, source.commit))
        .collect::<Vec<_>>();
    values.sort();
    let mut hasher = Sha256::new();
    for value in values {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())[..20].to_string()
}

fn safe_relative_path(value: &str) -> Result<PathBuf, SyncError> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || value
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(SyncError::Snapshot("unsafe repository path"));
    }
    Ok(path.to_path_buf())
}

fn validate_materialized_source(path: &Path) -> Result<(), SyncError> {
    if !path.is_dir() {
        return Err(SyncError::Snapshot("OpenSpec source is missing"));
    }
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|_| SyncError::Snapshot("OpenSpec source is unreadable"))?;
        if entry.path_is_symlink() {
            return Err(SyncError::Snapshot("symlinks are not allowed"));
        }
    }
    let _ = parser::scan_changes(path, "validation");
    let _ = parser::scan_specs(path, "validation");
    Ok(())
}

fn extract_change_names<'a>(files: impl Iterator<Item = &'a str>) -> HashSet<String> {
    files
        .filter_map(|filename| {
            let mut parts = filename.split('/');
            if parts.next()? != "openspec" || parts.next()? != "changes" {
                return None;
            }
            let change = parts.next()?;
            if change == "archive" || change.is_empty() {
                return None;
            }
            Some(change.to_string())
        })
        .collect()
}

fn persist_snapshot(cache_path: &Path, snapshot: &ActiveSnapshot) -> Result<(), SyncError> {
    fs::create_dir_all(cache_path).map_err(|_| SyncError::Cache)?;
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|_| SyncError::Cache)?;
    let temporary = cache_path.join(format!("{CURRENT_SNAPSHOT_FILE}.tmp"));
    fs::write(&temporary, bytes).map_err(|_| SyncError::Cache)?;
    fs::rename(temporary, cache_path.join(CURRENT_SNAPSHOT_FILE)).map_err(|_| SyncError::Cache)
}

pub fn restore_cached_snapshot(cache_path: &Path) -> Result<Option<ActiveSnapshot>, SyncError> {
    let current = cache_path.join(CURRENT_SNAPSHOT_FILE);
    if !current.exists() {
        return Ok(None);
    }
    let bytes = fs::read(current).map_err(|_| SyncError::Cache)?;
    let snapshot: ActiveSnapshot = serde_json::from_slice(&bytes).map_err(|_| SyncError::Cache)?;
    let canonical_cache = cache_path.canonicalize().map_err(|_| SyncError::Cache)?;
    for source in &snapshot.sources {
        let path = source.path.canonicalize().map_err(|_| SyncError::Cache)?;
        if !path.starts_with(&canonical_cache) || !path.is_dir() {
            return Err(SyncError::Cache);
        }
    }
    Ok(Some(snapshot))
}

fn prune_generations(cache_path: &Path, current_revision: &str) -> Result<(), SyncError> {
    let generations = cache_path.join("generations");
    let mut entries = fs::read_dir(&generations)
        .map_err(|_| SyncError::Cache)?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    entries.reverse();
    let mut retained_previous = false;
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == current_revision {
            continue;
        }
        if !name.starts_with(".staging-") && !retained_previous {
            retained_previous = true;
            continue;
        }
        fs::remove_dir_all(entry.path()).map_err(|_| SyncError::Cache)?;
    }
    Ok(())
}

pub fn degraded_health(previous: &SyncHealth, error: &SyncError) -> SyncHealth {
    let now = Utc::now().to_rfc3339();
    SyncHealth {
        state: SyncState::Degraded,
        active_revision: previous.active_revision.clone(),
        contributing_refs: previous.contributing_refs.clone(),
        last_attempt_at: Some(now.clone()),
        last_success_at: previous.last_success_at.clone(),
        last_failure: Some(SyncFailure {
            category: error.category().to_string(),
            summary: error.safe_summary(),
            occurred_at: now,
        }),
        serving_last_known_good: previous.active_revision.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::Uri, response::Json, Router};
    use serde_json::{json, Value};

    #[test]
    fn path_validation_rejects_traversal_and_absolute_paths() {
        assert!(safe_relative_path("specs/api/spec.md").is_ok());
        assert!(safe_relative_path("../secret").is_err());
        assert!(safe_relative_path("/etc/passwd").is_err());
        assert!(safe_relative_path("specs/./api.md").is_err());
    }

    #[test]
    fn merged_change_association_uses_changed_openspec_paths_only() {
        let names = extract_change_names(
            [
                "openspec/changes/add-sync/proposal.md",
                "openspec/changes/add-sync/tasks.md",
                "openspec/changes/archive/2026-add-old/proposal.md",
                "backend/src/main.rs",
            ]
            .into_iter(),
        );
        assert_eq!(names, HashSet::from(["add-sync".to_string()]));
    }

    #[test]
    fn cached_snapshot_rejects_sources_outside_cache() {
        let cache = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let snapshot = ActiveSnapshot {
            sources: vec![Source {
                id: "outside".to_string(),
                name: "outside".to_string(),
                path: outside.path().to_path_buf(),
                valid: true,
                track: None,
                target_branch: None,
                git: None,
                github: None,
                canonical_specs: true,
                include_changes: true,
                merged_changes: Vec::new(),
            }],
            revision: Some("revision".to_string()),
            health: SyncHealth::initializing(),
        };
        fs::write(
            cache.path().join(CURRENT_SNAPSHOT_FILE),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();

        assert!(restore_cached_snapshot(cache.path()).is_err());
    }

    #[test]
    fn safe_errors_do_not_include_secret_values() {
        let error = SyncError::MissingSecret("GITHUB_WEBHOOK_SECRET");
        assert_eq!(
            error.safe_summary(),
            "Required secret GITHUB_WEBHOOK_SECRET is unavailable"
        );
    }

    #[tokio::test]
    async fn invalid_private_key_prevents_token_refresh_without_exposing_material() {
        let config = GithubConfig {
            repository: "ToruAI/openspec-ui".to_string(),
            specs_ref: "demo/main".to_string(),
            changes_base_ref: "demo/main".to_string(),
            pull_request_targets: vec!["demo/main".to_string()],
            cache_path: "/tmp/cache".to_string(),
            reconciliation_interval_seconds: 900,
            max_pull_requests: 50,
            api_base_url: "http://127.0.0.1:1".to_string(),
            max_file_bytes: 1024,
            max_snapshot_bytes: 4096,
        };
        let client = GithubAppClient::new(
            config,
            Arc::new(GithubSecrets::fixture("secret-value-that-must-not-appear")),
        )
        .unwrap();
        let error = client.installation_token().await.unwrap_err();
        assert!(matches!(
            error,
            SyncError::InvalidSecret("GITHUB_APP_PRIVATE_KEY")
        ));
        assert!(!error.safe_summary().contains("secret-value"));
    }

    async fn fixture_api(uri: Uri) -> Json<Value> {
        let path = uri.path();
        let query = uri.query().unwrap_or_default();
        let value = if path.contains("/git/ref/heads/") {
            json!({"object":{"sha":"base-sha"}})
        } else if path.ends_with("/pulls") && query.contains("state=open") {
            json!([{
                "number": 42,
                "html_url": "https://github.com/ToruAI/openspec-ui/pull/42",
                "head": {"ref":"feature/github-sync","sha":"pr-sha"},
                "base": {"ref":"demo/main","sha":"base-sha"},
                "merged_at": null
            }])
        } else if path.ends_with("/pulls") && query.contains("state=closed") {
            json!([{
                "number": 12,
                "html_url": "https://github.com/ToruAI/openspec-ui/pull/12",
                "head": {"ref":"feature/old-change","sha":"old-sha"},
                "base": {"ref":"demo/main","sha":"base-sha"},
                "merged_at": "2026-01-01T00:00:00Z"
            }])
        } else if path.ends_with("/pulls/12/files") {
            json!([{"filename":"openspec/changes/old-change/proposal.md"}])
        } else if path.ends_with("/git/trees/base-sha") {
            json!({"truncated":false,"tree":[
                {"path":"openspec/changes/old-change/proposal.md","mode":"100644","type":"blob","sha":"base-proposal","size":11},
                {"path":"openspec/specs/api/spec.md","mode":"100644","type":"blob","sha":"base-spec","size":11},
                {"path":"backend/src/main.rs","mode":"100644","type":"blob","sha":"ignored","size":999999}
            ]})
        } else if path.ends_with("/git/trees/pr-sha") {
            json!({"truncated":false,"tree":[
                {"path":"openspec/changes/github-sync/proposal.md","mode":"100644","type":"blob","sha":"pr-proposal","size":13}
            ]})
        } else if path.ends_with("/git/blobs/base-proposal") {
            blob("# Proposal\n")
        } else if path.ends_with("/git/blobs/base-spec") {
            blob("# API spec\n")
        } else if path.ends_with("/git/blobs/pr-proposal") {
            blob("# PR proposal\n")
        } else {
            json!({"message":"fixture route not found","path":path,"query":query})
        };
        Json(value)
    }

    fn blob(content: &str) -> Value {
        json!({
            "content": STANDARD.encode(content.as_bytes()),
            "encoding": "base64",
            "size": content.len()
        })
    }

    async fn fixture_client(config: GithubConfig) -> GithubAppClient {
        let secrets = Arc::new(GithubSecrets {
            app_id: 1,
            installation_id: 2,
            private_key: "unused because the fixture token is cached".to_string(),
            webhook_secret: "secret".to_string(),
        });
        let client = GithubAppClient::new(config, secrets).unwrap();
        *client.token.lock().await = Some(CachedToken {
            value: "fixture-token".to_string(),
            expires_at: Utc::now() + Duration::hours(1),
        });
        client
    }

    async fn fixture_config() -> (GithubConfig, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().fallback(fixture_api))
                .await
                .unwrap();
        });
        (
            GithubConfig {
                repository: "ToruAI/openspec-ui".to_string(),
                specs_ref: "demo/main".to_string(),
                changes_base_ref: "demo/main".to_string(),
                pull_request_targets: vec!["demo/main".to_string()],
                cache_path: "unused".to_string(),
                reconciliation_interval_seconds: 900,
                max_pull_requests: 50,
                api_base_url: format!("http://{address}"),
                max_file_bytes: 1024,
                max_snapshot_bytes: 4096,
            },
            server,
        )
    }

    #[tokio::test]
    async fn fixture_reconciliation_filters_refs_publishes_and_restores_atomically() {
        let (config, server) = fixture_config().await;
        let cache = tempfile::tempdir().unwrap();
        let client = fixture_client(config.clone()).await;
        let synchronizer = GithubSynchronizer::new(client, config, cache.path().to_path_buf());

        let snapshot = synchronizer.reconcile().await.unwrap();
        assert_eq!(snapshot.health.state, SyncState::Healthy);
        assert_eq!(snapshot.sources.len(), 2);
        let base = snapshot
            .sources
            .iter()
            .find(|source| source.id == "github-base")
            .unwrap();
        let pull = snapshot
            .sources
            .iter()
            .find(|source| source.id == "github-pr-42")
            .unwrap();
        assert!(base.canonical_specs);
        assert!(!pull.canonical_specs);
        assert_eq!(
            pull.github
                .as_ref()
                .unwrap()
                .pull_request
                .as_ref()
                .unwrap()
                .number,
            42
        );
        assert!(base.path.join("specs/api/spec.md").is_file());
        assert!(!base.path.join("backend/src/main.rs").exists());
        assert_eq!(base.merged_changes[0].change_name, "old-change");

        let restored = restore_cached_snapshot(cache.path()).unwrap().unwrap();
        assert_eq!(restored.revision, snapshot.revision);
        assert!(restored.sources.iter().all(|source| source.path.is_dir()));

        let unchanged = synchronizer.reconcile().await.unwrap();
        assert_eq!(unchanged.revision, snapshot.revision);
        server.abort();
    }

    #[tokio::test]
    async fn rejected_generation_keeps_the_last_known_good_pointer() {
        let (config, server) = fixture_config().await;
        let cache = tempfile::tempdir().unwrap();
        let good_client = fixture_client(config.clone()).await;
        let good = GithubSynchronizer::new(good_client, config.clone(), cache.path().to_path_buf())
            .reconcile()
            .await
            .unwrap();

        let mut rejected_config = config;
        rejected_config.max_file_bytes = 5;
        let rejected_client = fixture_client(rejected_config.clone()).await;
        let result =
            GithubSynchronizer::new(rejected_client, rejected_config, cache.path().to_path_buf())
                .reconcile()
                .await;
        assert!(matches!(result, Err(SyncError::Snapshot(_))));
        assert_eq!(
            restore_cached_snapshot(cache.path())
                .unwrap()
                .unwrap()
                .revision,
            good.revision
        );
        server.abort();
    }
}
