use crate::config::Source;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    Disabled,
    Initializing,
    Healthy,
    Degraded,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContributingRef {
    pub source_id: String,
    pub ref_name: String,
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request_number: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncFailure {
    pub category: String,
    pub summary: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncHealth {
    pub state: SyncState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_revision: Option<String>,
    #[serde(default)]
    pub contributing_refs: Vec<ContributingRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<SyncFailure>,
    pub serving_last_known_good: bool,
}

impl SyncHealth {
    pub fn filesystem() -> Self {
        Self {
            state: SyncState::Disabled,
            active_revision: None,
            contributing_refs: Vec::new(),
            last_attempt_at: None,
            last_success_at: None,
            last_failure: None,
            serving_last_known_good: false,
        }
    }

    pub fn initializing() -> Self {
        Self {
            state: SyncState::Initializing,
            active_revision: None,
            contributing_refs: Vec::new(),
            last_attempt_at: None,
            last_success_at: None,
            last_failure: None,
            serving_last_known_good: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSnapshot {
    pub sources: Vec<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub health: SyncHealth,
}

impl ActiveSnapshot {
    pub fn filesystem(sources: Vec<Source>) -> Self {
        Self {
            sources,
            revision: None,
            health: SyncHealth::filesystem(),
        }
    }

    pub fn initializing() -> Self {
        Self {
            sources: Vec::new(),
            revision: None,
            health: SyncHealth::initializing(),
        }
    }
}
