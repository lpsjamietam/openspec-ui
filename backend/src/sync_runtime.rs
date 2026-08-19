use crate::{
    config::GithubConfig,
    config_manager::AppState,
    github_sync::{
        degraded_health, restore_cached_snapshot, GithubAppClient, GithubSecrets,
        GithubSynchronizer, SyncError,
    },
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, Mutex, RwLock};

const DELIVERY_LEDGER_FILE: &str = "webhook-deliveries.json";
const PENDING_REFRESH_FILE: &str = "refresh-pending";
const MAX_DELIVERIES: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookOutcome {
    Scheduled,
    Duplicate,
    Ignored,
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("webhook signature is missing or invalid")]
    InvalidSignature,
    #[error("webhook delivery identifier is missing")]
    MissingDelivery,
    #[error("webhook payload is invalid")]
    InvalidPayload,
    #[error("webhook state could not be persisted")]
    Persistence,
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedLedger {
    deliveries: VecDeque<String>,
}

struct DeliveryLedger {
    path: PathBuf,
    entries: VecDeque<String>,
    lookup: HashSet<String>,
}

impl DeliveryLedger {
    fn load(cache_path: &Path) -> Result<Self, SyncError> {
        fs::create_dir_all(cache_path).map_err(|_| SyncError::Cache)?;
        let path = cache_path.join(DELIVERY_LEDGER_FILE);
        let persisted = if path.exists() {
            let bytes = fs::read(&path).map_err(|_| SyncError::Cache)?;
            serde_json::from_slice::<PersistedLedger>(&bytes).map_err(|_| SyncError::Cache)?
        } else {
            PersistedLedger::default()
        };
        let lookup = persisted.deliveries.iter().cloned().collect();
        Ok(Self {
            path,
            entries: persisted.deliveries,
            lookup,
        })
    }

    fn record(&mut self, delivery: &str) -> Result<bool, WebhookError> {
        if self.lookup.contains(delivery) {
            return Ok(false);
        }
        self.entries.push_back(delivery.to_string());
        self.lookup.insert(delivery.to_string());
        while self.entries.len() > MAX_DELIVERIES {
            if let Some(expired) = self.entries.pop_front() {
                self.lookup.remove(&expired);
            }
        }
        atomic_write_json(
            &self.path,
            &PersistedLedger {
                deliveries: self.entries.clone(),
            },
        )
        .map_err(|_| WebhookError::Persistence)?;
        Ok(true)
    }
}

#[derive(Clone)]
struct RefreshHandle {
    sender: mpsc::Sender<()>,
    pending_path: PathBuf,
}

impl RefreshHandle {
    fn schedule(&self) -> Result<(), WebhookError> {
        atomic_write(&self.pending_path, b"pending\n").map_err(|_| WebhookError::Persistence)?;
        match self.sender.try_send(()) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(())) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(())) => Err(WebhookError::Persistence),
        }
    }
}

pub struct GithubRuntime {
    config: GithubConfig,
    secrets: Arc<GithubSecrets>,
    ledger: Mutex<DeliveryLedger>,
    refresh: RefreshHandle,
    eligible_pull_request_refs: Arc<RwLock<HashSet<String>>>,
}

impl GithubRuntime {
    pub async fn start(
        config: GithubConfig,
        cache_path: PathBuf,
        state: AppState,
    ) -> Result<Arc<Self>, SyncError> {
        let secrets = Arc::new(GithubSecrets::load()?);
        let eligible_pull_request_refs = Arc::new(RwLock::new(HashSet::new()));
        if let Some(snapshot) = restore_cached_snapshot(&cache_path)? {
            *eligible_pull_request_refs.write().await = pull_request_refs(&snapshot.health);
            state.publish_snapshot(snapshot).await;
        }
        let client = GithubAppClient::new(config.clone(), secrets.clone())?;
        let synchronizer = Arc::new(GithubSynchronizer::new(
            client,
            config.clone(),
            cache_path.clone(),
        ));
        let ledger = DeliveryLedger::load(&cache_path)?;
        let (sender, mut receiver) = mpsc::channel::<()>(1);
        let refresh = RefreshHandle {
            sender,
            pending_path: cache_path.join(PENDING_REFRESH_FILE),
        };
        let runtime = Arc::new(Self {
            config: config.clone(),
            secrets,
            ledger: Mutex::new(ledger),
            refresh: refresh.clone(),
            eligible_pull_request_refs: eligible_pull_request_refs.clone(),
        });

        let worker_state = state.clone();
        let worker_refresh = refresh.clone();
        let worker_pull_request_refs = eligible_pull_request_refs;
        tokio::spawn(async move {
            let mut failures = 0u32;
            while receiver.recv().await.is_some() {
                match synchronizer.reconcile().await {
                    Ok(snapshot) => {
                        failures = 0;
                        *worker_pull_request_refs.write().await =
                            pull_request_refs(&snapshot.health);
                        worker_state.publish_snapshot(snapshot).await;
                        let _ = fs::remove_file(&worker_refresh.pending_path);
                    }
                    Err(error) => {
                        failures = failures.saturating_add(1);
                        tracing::warn!(
                            category = error.category(),
                            "GitHub reconciliation failed: {}",
                            error.safe_summary()
                        );
                        let previous = worker_state.sync_health().await;
                        worker_state
                            .update_sync_health(degraded_health(&previous, &error))
                            .await;
                        let backoff = 2u64.saturating_pow(failures.min(5)).min(60);
                        tokio::time::sleep(Duration::from_secs(backoff)).await;
                        let _ = worker_refresh.schedule();
                    }
                }
            }
        });

        let periodic_refresh = refresh.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(config.reconciliation_interval_seconds));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                interval.tick().await;
                let _ = periodic_refresh.schedule();
            }
        });

        refresh.schedule().map_err(|_| SyncError::Cache)?;
        Ok(runtime)
    }

    pub async fn handle_webhook(
        &self,
        signature: Option<&str>,
        delivery: Option<&str>,
        event: Option<&str>,
        body: &[u8],
    ) -> Result<WebhookOutcome, WebhookError> {
        verify_signature(self.secrets.webhook_secret(), signature, body)?;
        let delivery = delivery
            .filter(|value| !value.trim().is_empty())
            .ok_or(WebhookError::MissingDelivery)?;
        let payload: Value =
            serde_json::from_slice(body).map_err(|_| WebhookError::InvalidPayload)?;

        let mut ledger = self.ledger.lock().await;
        if !ledger.record(delivery)? {
            return Ok(WebhookOutcome::Duplicate);
        }
        drop(ledger);

        if !self.is_relevant(event.unwrap_or_default(), &payload).await {
            return Ok(WebhookOutcome::Ignored);
        }
        self.refresh.schedule()?;
        Ok(WebhookOutcome::Scheduled)
    }

    async fn is_relevant(&self, event: &str, payload: &Value) -> bool {
        let pull_request_refs = self.eligible_pull_request_refs.read().await;
        webhook_is_relevant(&self.config, &pull_request_refs, event, payload)
    }
}

fn pull_request_refs(health: &crate::snapshot::SyncHealth) -> HashSet<String> {
    health
        .contributing_refs
        .iter()
        .filter(|contributor| contributor.pull_request_number.is_some())
        .map(|contributor| contributor.ref_name.clone())
        .collect()
}

fn webhook_is_relevant(
    config: &GithubConfig,
    eligible_pull_request_refs: &HashSet<String>,
    event: &str,
    payload: &Value,
) -> bool {
    let configured_repo = config.repository.as_str();
    let repository_matches = payload
        .pointer("/repository/full_name")
        .and_then(Value::as_str)
        .is_some_and(|repository| repository == configured_repo);

    match event {
        "installation_repositories" => payload
            .get("repositories_added")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .chain(
                payload
                    .get("repositories_removed")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            )
            .any(|repository| {
                repository
                    .get("full_name")
                    .and_then(Value::as_str)
                    .is_some_and(|repository| repository == configured_repo)
            }),
        _ if !repository_matches => false,
        _ => match event {
            "push" => payload
                .get("ref")
                .and_then(Value::as_str)
                .and_then(|value| value.strip_prefix("refs/heads/"))
                .is_some_and(|branch| {
                    branch == config.specs_ref
                        || branch == config.changes_base_ref
                        || eligible_pull_request_refs.contains(branch)
                }),
            "pull_request" => {
                let supported_action =
                    payload
                        .get("action")
                        .and_then(Value::as_str)
                        .is_some_and(|action| {
                            matches!(
                                action,
                                "opened"
                                    | "reopened"
                                    | "synchronize"
                                    | "closed"
                                    | "edited"
                                    | "ready_for_review"
                                    | "converted_to_draft"
                            )
                        });
                let base = payload
                    .pointer("/pull_request/base/ref")
                    .and_then(Value::as_str);
                let previous_base = payload
                    .pointer("/changes/base/ref/from")
                    .and_then(Value::as_str);
                supported_action
                    && [base, previous_base].into_iter().flatten().any(|base| {
                        config
                            .pull_request_targets
                            .iter()
                            .any(|target| target == base)
                    })
            }
            _ => false,
        },
    }
}

fn verify_signature(
    secret: &[u8],
    signature: Option<&str>,
    body: &[u8],
) -> Result<(), WebhookError> {
    let signature = signature
        .and_then(|value| value.strip_prefix("sha256="))
        .ok_or(WebhookError::InvalidSignature)?;
    let bytes = hex::decode(signature).map_err(|_| WebhookError::InvalidSignature)?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret).map_err(|_| WebhookError::InvalidSignature)?;
    mac.update(body);
    mac.verify_slice(&bytes)
        .map_err(|_| WebhookError::InvalidSignature)
}

fn atomic_write_json(path: &Path, value: &impl Serialize) -> Result<(), std::io::Error> {
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    atomic_write(path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> GithubConfig {
        GithubConfig {
            repository: "ToruAI/openspec-ui".to_string(),
            specs_ref: "demo/main".to_string(),
            changes_base_ref: "demo/main".to_string(),
            pull_request_targets: vec!["demo/main".to_string()],
            cache_path: "/tmp/cache".to_string(),
            reconciliation_interval_seconds: 900,
            max_pull_requests: 50,
            api_base_url: "https://api.github.com".to_string(),
            max_file_bytes: 1024,
            max_snapshot_bytes: 4096,
        }
    }

    #[test]
    fn signature_validation_is_constant_time_via_hmac_verification() {
        let body = br#"{"action":"opened"}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(body);
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_signature(b"secret", Some(&signature), body).is_ok());
        assert!(verify_signature(b"secret", Some("sha256=00"), body).is_err());
        assert!(verify_signature(b"secret", None, body).is_err());
    }

    #[test]
    fn relevance_filters_repository_branch_action_and_event() {
        let payload: Value = serde_json::from_str(
            r#"{
                "action":"synchronize",
                "repository":{"full_name":"ToruAI/openspec-ui"},
                "pull_request":{"base":{"ref":"demo/main"}}
            }"#,
        )
        .unwrap();
        assert!(webhook_is_relevant(
            &config(),
            &HashSet::new(),
            "pull_request",
            &payload
        ));

        let mut wrong = payload.clone();
        wrong["repository"]["full_name"] = Value::String("other/repo".to_string());
        assert!(!webhook_is_relevant(
            &config(),
            &HashSet::new(),
            "pull_request",
            &wrong
        ));
        assert!(!webhook_is_relevant(
            &config(),
            &HashSet::new(),
            "issues",
            &payload
        ));

        let retargeted_away: Value = serde_json::from_str(
            r#"{
                "action":"edited",
                "repository":{"full_name":"ToruAI/openspec-ui"},
                "pull_request":{"base":{"ref":"untracked"}},
                "changes":{"base":{"ref":{"from":"demo/main"}}}
            }"#,
        )
        .unwrap();
        assert!(webhook_is_relevant(
            &config(),
            &HashSet::new(),
            "pull_request",
            &retargeted_away
        ));

        let installation: Value = serde_json::from_str(
            r#"{
                "repositories_added":[{"full_name":"ToruAI/openspec-ui"}],
                "repositories_removed":[]
            }"#,
        )
        .unwrap();
        assert!(webhook_is_relevant(
            &config(),
            &HashSet::new(),
            "installation_repositories",
            &installation
        ));

        let pull_request_push: Value = serde_json::from_str(
            r#"{
                "repository":{"full_name":"ToruAI/openspec-ui"},
                "ref":"refs/heads/describe-pr-change"
            }"#,
        )
        .unwrap();
        assert!(webhook_is_relevant(
            &config(),
            &HashSet::from(["describe-pr-change".to_string()]),
            "push",
            &pull_request_push
        ));
    }

    #[test]
    fn delivery_ledger_is_bounded_and_persistent() {
        let cache = tempfile::tempdir().unwrap();
        let mut ledger = DeliveryLedger::load(cache.path()).unwrap();
        assert!(ledger.record("delivery-1").unwrap());
        assert!(!ledger.record("delivery-1").unwrap());
        let restored = DeliveryLedger::load(cache.path()).unwrap();
        assert!(restored.lookup.contains("delivery-1"));
    }

    fn signed(secret: &[u8], body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    fn runtime(cache: &Path) -> (GithubRuntime, mpsc::Receiver<()>) {
        let (sender, receiver) = mpsc::channel(1);
        (
            GithubRuntime {
                config: config(),
                secrets: Arc::new(GithubSecrets::fixture("secret")),
                ledger: Mutex::new(DeliveryLedger::load(cache).unwrap()),
                refresh: RefreshHandle {
                    sender,
                    pending_path: cache.join(PENDING_REFRESH_FILE),
                },
                eligible_pull_request_refs: Arc::new(RwLock::new(HashSet::new())),
            },
            receiver,
        )
    }

    #[tokio::test]
    async fn verified_delivery_schedules_once_and_returns_promptly() {
        let cache = tempfile::tempdir().unwrap();
        let (runtime, mut receiver) = runtime(cache.path());
        let body = br#"{
            "action":"synchronize",
            "repository":{"full_name":"ToruAI/openspec-ui"},
            "pull_request":{"base":{"ref":"demo/main"}}
        }"#;
        let signature = signed(b"secret", body);
        let outcome = tokio::time::timeout(
            Duration::from_millis(100),
            runtime.handle_webhook(
                Some(&signature),
                Some("delivery-1"),
                Some("pull_request"),
                body,
            ),
        )
        .await
        .expect("webhook acknowledgement must not wait for synchronization")
        .unwrap();
        assert_eq!(outcome, WebhookOutcome::Scheduled);
        assert!(receiver.try_recv().is_ok());
        assert!(cache.path().join(PENDING_REFRESH_FILE).is_file());

        let duplicate = runtime
            .handle_webhook(
                Some(&signature),
                Some("delivery-1"),
                Some("pull_request"),
                body,
            )
            .await
            .unwrap();
        assert_eq!(duplicate, WebhookOutcome::Duplicate);
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn invalid_malformed_and_irrelevant_deliveries_do_not_schedule() {
        let cache = tempfile::tempdir().unwrap();
        let (runtime, mut receiver) = runtime(cache.path());
        let relevant = br#"{
            "repository":{"full_name":"ToruAI/openspec-ui"},
            "ref":"refs/heads/demo/main"
        }"#;
        assert!(matches!(
            runtime
                .handle_webhook(
                    Some("sha256=00"),
                    Some("invalid-signature"),
                    Some("push"),
                    relevant,
                )
                .await,
            Err(WebhookError::InvalidSignature)
        ));

        let malformed = b"not-json";
        assert!(matches!(
            runtime
                .handle_webhook(
                    Some(&signed(b"secret", malformed)),
                    Some("malformed"),
                    Some("push"),
                    malformed,
                )
                .await,
            Err(WebhookError::InvalidPayload)
        ));

        let irrelevant = br#"{
            "repository":{"full_name":"ToruAI/openspec-ui"},
            "ref":"refs/heads/unrelated"
        }"#;
        let ignored = runtime
            .handle_webhook(
                Some(&signed(b"secret", irrelevant)),
                Some("irrelevant"),
                Some("push"),
                irrelevant,
            )
            .await
            .unwrap();
        assert_eq!(ignored, WebhookOutcome::Ignored);
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn installation_event_and_bursts_are_coalesced_by_the_bounded_queue() {
        let cache = tempfile::tempdir().unwrap();
        let (runtime, mut receiver) = runtime(cache.path());
        let body = br#"{
            "repositories_added":[{"full_name":"ToruAI/openspec-ui"}],
            "repositories_removed":[]
        }"#;
        for delivery in ["install-1", "install-2"] {
            assert_eq!(
                runtime
                    .handle_webhook(
                        Some(&signed(b"secret", body)),
                        Some(delivery),
                        Some("installation_repositories"),
                        body,
                    )
                    .await
                    .unwrap(),
                WebhookOutcome::Scheduled
            );
        }
        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_err());
    }
}
