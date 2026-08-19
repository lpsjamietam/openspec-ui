mod config;
mod config_manager;
mod github_sync;
mod parser;
mod snapshot;
mod sync_runtime;

use axum::{
    body::Bytes,
    extract::DefaultBodyLimit,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{sse::Event, IntoResponse, Json, Sse},
    routing::{delete, get, post, put},
    Router,
};
use clap::Parser as ClapParser;
use config::{GitContext, GithubProvenance, Source, SourceConfig, SourceMode, StatusProvider};
use config_manager::{AppState, ConfigManager, ConfigResponse};
use futures::stream::{self, Stream};
use notify::{EventKind, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebouncedEvent, FileIdMap};
use parser::{Change, ChangeDetail, Idea, Spec, SpecDetail};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use snapshot::SyncHealth;
use std::{
    convert::Infallible,
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use sync_runtime::{WebhookError, WebhookOutcome};
use tokio::sync::broadcast;
use tower_http::cors::AllowOrigin;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
struct Assets;

#[derive(ClapParser, Debug)]
#[command(name = "openspec-ui")]
#[command(about = "A read-only dashboard for OpenSpec")]
struct Args {
    /// Path to config file
    #[arg(short, long)]
    config: Option<PathBuf>,
}

// AppState is now defined in config_manager module

// === Response Types ===

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceResponse {
    id: String,
    name: String,
    path: String,
    valid: bool,
    track: Option<String>,
    target_branch: Option<String>,
    git: Option<GitContext>,
    github: Option<GithubProvenance>,
    canonical_specs: bool,
}

#[derive(Serialize)]
struct SourcesResponse {
    sources: Vec<SourceResponse>,
}

#[derive(Deserialize)]
struct UpdateSourcesRequest {
    sources: Vec<SourceConfig>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct ChangesResponse {
    changes: Vec<Change>,
}

#[derive(Serialize)]
struct SpecsResponse {
    specs: Vec<Spec>,
}

#[derive(Serialize)]
struct IdeasResponse {
    ideas: Vec<Idea>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateIdeaRequest {
    title: String,
    description: String,
    #[serde(default)]
    source_id: Option<String>,
}

#[derive(Deserialize)]
struct UpdateIdeaRequest {
    title: String,
    description: String,
}

// === Handlers ===

async fn health() -> &'static str {
    "ok"
}

async fn get_sources(State(state): State<AppState>) -> Json<SourcesResponse> {
    let sources = state.get_sources().await;
    let response = sources
        .iter()
        .map(|s| SourceResponse {
            id: s.id.clone(),
            name: s.name.clone(),
            path: s
                .github
                .as_ref()
                .map(|github| format!("github:{}@{}", github.repository, github.ref_name))
                .unwrap_or_else(|| s.path.display().to_string()),
            valid: s.valid,
            track: s.track.clone(),
            target_branch: s.target_branch.clone(),
            git: s.git.clone(),
            github: s.github.clone(),
            canonical_specs: s.canonical_specs,
        })
        .collect();
    Json(SourcesResponse { sources: response })
}

async fn get_changes(State(state): State<AppState>) -> Json<ChangesResponse> {
    let mut all_changes = Vec::new();
    let sources = state.get_sources().await;
    let config = state.config_manager.load_config().ok();

    for source in sources.iter().filter(|s| s.valid && s.include_changes) {
        let mut changes = parser::scan_changes(&source.path, &source.id);
        parser::attach_source_context(&mut changes, source);
        all_changes.extend(changes);
    }

    if config
        .as_ref()
        .is_none_or(|config| config.deduplicate_changes)
    {
        all_changes = parser::deduplicate_changes(all_changes);
    }

    if let Some(config) = config
        .filter(|config| !config.is_github_mode() && config.status_provider == StatusProvider::Auto)
    {
        for change in &mut all_changes {
            let Some(source) = sources.iter().find(|source| source.id == change.source_id) else {
                continue;
            };
            if let Err(error) =
                parser::enrich_change_from_cli(change, &source.path, &config.openspec_command)
            {
                tracing::debug!(
                    change = %change.name,
                    source = %change.source_id,
                    "OpenSpec CLI status unavailable; using filesystem status: {error}"
                );
            }
        }
    }

    Json(ChangesResponse {
        changes: all_changes,
    })
}

fn require_writable(state: &AppState) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if state.config_manager.is_read_only().unwrap_or(true) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "OpenSpec UI is running in read-only mode".to_string(),
            }),
        ));
    }
    Ok(())
}

async fn get_change_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ChangeDetail>, StatusCode> {
    // id format: source_id/change_name
    let parts: Vec<&str> = id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let source_id = parts[0];
    let change_name = parts[1];

    let sources = state.get_sources().await;
    let source = sources
        .iter()
        .find(|s| s.id == source_id && s.valid)
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut detail = parser::get_change_detail(&source.path, source_id, change_name)
        .ok_or(StatusCode::NOT_FOUND)?;
    parser::attach_detail_source_context(&mut detail, source);
    Ok(Json(detail))
}

async fn get_specs(State(state): State<AppState>) -> Json<SpecsResponse> {
    let mut all_specs = Vec::new();
    let sources = state.get_sources().await;
    let specs_source_id = state
        .config_manager
        .load_config()
        .ok()
        .and_then(|config| config.specs_source_id);
    let github_mode = state
        .config_manager
        .load_config()
        .is_ok_and(|config| config.is_github_mode());

    for source in sources.iter().filter(|source| {
        if github_mode {
            source.valid && source.canonical_specs
        } else {
            is_specs_source(source, specs_source_id.as_deref())
        }
    }) {
        let mut specs = parser::scan_specs(&source.path, &source.id);
        for spec in &mut specs {
            spec.github = source.github.clone();
        }
        all_specs.extend(specs);
    }

    Json(SpecsResponse { specs: all_specs })
}

async fn get_spec_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SpecDetail>, StatusCode> {
    // id format: source_id/spec_path (e.g., "brain-gate/chat" or full path)
    let parts: Vec<&str> = id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let source_id = parts[0];
    let spec_name = parts[1];

    let sources = state.get_sources().await;
    let specs_source_id = state
        .config_manager
        .load_config()
        .ok()
        .and_then(|config| config.specs_source_id);
    let github_mode = state
        .config_manager
        .load_config()
        .is_ok_and(|config| config.is_github_mode());
    let source = sources
        .iter()
        .find(|source| {
            source.id == source_id
                && if github_mode {
                    source.valid && source.canonical_specs
                } else {
                    is_specs_source(source, specs_source_id.as_deref())
                }
        })
        .ok_or(StatusCode::NOT_FOUND)?;

    // Try different path formats
    let spec_paths = [
        format!("{spec_name}/spec.md"),
        format!("{spec_name}.md"),
        spec_name.to_string(),
    ];

    for spec_path in &spec_paths {
        if let Some(mut detail) = parser::get_spec_detail(&source.path, source_id, spec_path) {
            detail.github = source.github.clone();
            return Ok(Json(detail));
        }
    }

    Err(StatusCode::NOT_FOUND)
}

fn is_specs_source(source: &Source, specs_source_id: Option<&str>) -> bool {
    source.valid && specs_source_id.is_none_or(|source_id| source.id == source_id)
}

async fn get_sync_health(State(state): State<AppState>) -> Json<SyncHealth> {
    Json(state.sync_health().await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebhookAcknowledgement {
    outcome: &'static str,
}

async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<WebhookAcknowledgement>, (StatusCode, Json<ErrorResponse>)> {
    let runtime = state.github_runtime().await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "GitHub server mode is not active".to_string(),
            }),
        )
    })?;
    let header = |name: &'static str| headers.get(name).and_then(|value| value.to_str().ok());
    match runtime
        .handle_webhook(
            header("x-hub-signature-256"),
            header("x-github-delivery"),
            header("x-github-event"),
            &body,
        )
        .await
    {
        Ok(outcome) => Ok(Json(WebhookAcknowledgement {
            outcome: match outcome {
                WebhookOutcome::Scheduled => "scheduled",
                WebhookOutcome::Duplicate => "duplicate",
                WebhookOutcome::Ignored => "ignored",
            },
        })),
        Err(error) => {
            let status = match error {
                WebhookError::InvalidSignature => StatusCode::UNAUTHORIZED,
                WebhookError::MissingDelivery | WebhookError::InvalidPayload => {
                    StatusCode::BAD_REQUEST
                }
                WebhookError::Persistence => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err((
                status,
                Json(ErrorResponse {
                    error: error.to_string(),
                }),
            ))
        }
    }
}

async fn get_ideas(State(state): State<AppState>) -> Json<IdeasResponse> {
    let mut all_ideas = Vec::new();
    let sources = state.get_sources().await;

    for source in sources.iter().filter(|s| s.valid) {
        let ideas = parser::scan_ideas(&source.path, &source.id);
        all_ideas.extend(ideas);
    }

    Json(IdeasResponse { ideas: all_ideas })
}

async fn create_idea(
    State(state): State<AppState>,
    Json(req): Json<CreateIdeaRequest>,
) -> Result<Json<Idea>, (StatusCode, Json<ErrorResponse>)> {
    require_writable(&state)?;
    // Find target source
    let sources = state.get_sources().await;
    let source = if let Some(source_id) = &req.source_id {
        sources
            .iter()
            .find(|s| s.id == *source_id && s.valid)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("Source '{source_id}' not found"),
                    }),
                )
            })?
    } else {
        // Default to first valid source if none specified
        sources.iter().find(|s| s.valid).ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "No valid source configured".to_string(),
                }),
            )
        })?
    };

    let id = format!("idea-{}", chrono::Utc::now().timestamp_millis());
    let idea = parser::save_idea(
        &source.path,
        &source.id,
        &id,
        &req.title,
        &req.description,
        None,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to save idea: {e}"),
            }),
        )
    })?;

    let _ = state.update_tx.send(());

    Ok(Json(idea))
}

async fn delete_idea(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_writable(&state)?;
    let parts: Vec<&str> = id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid idea ID format".to_string(),
            }),
        ));
    }

    let source_id = parts[0];
    let idea_id = parts[1];

    let sources = state.get_sources().await;
    let source = sources
        .iter()
        .find(|s| s.id == source_id && s.valid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Source not found".to_string(),
                }),
            )
        })?;

    parser::delete_idea(&source.path, idea_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to delete idea: {e}"),
            }),
        )
    })?;

    let _ = state.update_tx.send(());

    Ok(StatusCode::OK)
}

async fn update_idea(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateIdeaRequest>,
) -> Result<Json<Idea>, (StatusCode, Json<ErrorResponse>)> {
    require_writable(&state)?;
    let parts: Vec<&str> = id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid idea ID format".to_string(),
            }),
        ));
    }

    let source_id = parts[0];
    let idea_id = parts[1];

    let sources = state.get_sources().await;
    let source = sources
        .iter()
        .find(|s| s.id == source_id && s.valid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Source not found".to_string(),
                }),
            )
        })?;

    let idea = parser::update_idea(
        &source.path,
        &source.id,
        idea_id,
        &req.title,
        &req.description,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to update idea: {e}"),
            }),
        )
    })?;

    let _ = state.update_tx.send(());

    Ok(Json(idea))
}

async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigResponse>, StatusCode> {
    let config_manager = state.config_manager().await;
    config_manager
        .get_config_response()
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update_sources(
    State(state): State<AppState>,
    Json(req): Json<UpdateSourcesRequest>,
) -> Result<Json<ConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_writable(&state)?;
    let config_manager = state.config_manager().await;

    // Validate sources - invalid ones are filtered out with warnings
    let (valid_sources, warnings) = config_manager.validate_sources(&req.sources);

    // Log warnings for filtered sources
    for warning in &warnings {
        tracing::warn!("{}", warning);
    }

    // Save only valid sources to disk
    if let Err(e) = config_manager.save_sources(&valid_sources) {
        tracing::error!("Failed to save config: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to save configuration: {e}"),
            }),
        ));
    }

    // Reload sources and update state
    let new_sources = config_manager.load_sources().map_err(|e| {
        tracing::error!("Failed to reload sources: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to reload sources: {e}"),
            }),
        )
    })?;

    state.update_sources(new_sources).await;

    // Trigger file watcher restart and SSE update
    let inner = state.inner.read().await;
    let _ = inner.config_update_tx.send(());
    drop(inner);
    let _ = state.update_tx.send(());

    // Return updated config
    config_manager.get_config_response().map(Json).map_err(|e| {
        tracing::error!("Failed to get config response: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to get updated configuration".to_string(),
            }),
        )
    })
}

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.update_tx.subscribe();

    let stream = stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {
                Some((Ok(Event::default().event("update").data("changed")), rx))
            }
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            // Reduce keep-alive interval to 15s to keep connections fresh
            .interval(Duration::from_secs(15))
            .text("keep-alive-text"),
    )
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.is_empty() {
        path = "index.html".to_string();
    }

    match Assets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            if path.contains('.') {
                return StatusCode::NOT_FOUND.into_response();
            }
            // Serve index.html for SPA routing
            match Assets::get("index.html") {
                Some(content) => {
                    ([(header::CONTENT_TYPE, "text/html")], content.data).into_response()
                }
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Determine config path
    let config_path = args
        .config
        .or_else(|| env::var("OPENSPEC_UI_CONFIG").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("openspec-ui.json"));

    if !config_path.exists() {
        tracing::error!("Config file not found: {:?}", config_path);
        std::process::exit(1);
    }

    // Create config manager
    let config_manager = Arc::new(ConfigManager::new(config_path.clone()));
    let runtime_config = match config_manager.load_config() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("Failed to load configuration: {error}");
            std::process::exit(1);
        }
    };

    // Load initial sources
    let sources = match config_manager.load_sources() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to load sources: {}", e);
            std::process::exit(1);
        }
    };

    tracing::info!("Loaded {} sources", sources.len());
    for source in &sources {
        tracing::info!(
            "  {} @ {:?} (valid: {})",
            source.name,
            source.path,
            source.valid
        );
    }

    // Create broadcast channel for SSE updates
    let (update_tx, _) = broadcast::channel::<()>(16);

    // Create a separate channel for watcher restarts (config changes)
    let (config_update_tx, _) = broadcast::channel::<()>(16);
    let config_update_tx_for_watcher = config_update_tx.clone();

    // Create app state
    let state = AppState::new(
        sources.clone(),
        config_manager.clone(),
        update_tx.clone(),
        config_update_tx.clone(),
    );

    if let Some(github) = runtime_config.github.clone() {
        let cache_path = config_manager.resolve_cache_path(&github);
        match sync_runtime::GithubRuntime::start(github, cache_path, state.clone()).await {
            Ok(runtime) => state.set_github_runtime(runtime).await,
            Err(error) => {
                tracing::error!(
                    "Failed to start GitHub server mode: {}",
                    error.safe_summary()
                );
                std::process::exit(1);
            }
        }
    }

    // Filesystem mode retains the existing dynamic watcher. GitHub mode is refreshed by
    // verified webhooks and periodic reconciliation instead.
    if !runtime_config.is_github_mode() {
        let state_for_watcher = state.clone();
        tokio::spawn(async move {
            // Use full debouncer to get event kinds
            let mut current_watcher: Option<
                notify_debouncer_full::Debouncer<notify::RecommendedWatcher, FileIdMap>,
            > = None;
            let mut config_rx = config_update_tx_for_watcher.subscribe();

            // Initial setup
            let mut should_setup = true;

            loop {
                if should_setup {
                    // Get current sources
                    let sources = state_for_watcher.get_sources().await;

                    // Drop old watcher if it exists
                    if let Some(debouncer) = current_watcher.take() {
                        drop(debouncer);
                    }

                    // Create new watcher
                    let update_tx_watcher = state_for_watcher.update_tx.clone();

                    // Using notify-debouncer-full to filter Access events
                    match new_debouncer(
                        Duration::from_millis(500),
                        None, // No cache timeout
                        move |result: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
                            match result {
                                Ok(events) => {
                                    let mut changed = false;
                                    for debounced_event in events {
                                        // Filter out Access events which are causing infinite loops
                                        match debounced_event.event.kind {
                                            EventKind::Access(_) => {
                                                // Ignore access events
                                                continue;
                                            }
                                            _ => {
                                                tracing::info!(
                                                    "File changed: {:?} {:?}",
                                                    debounced_event.event.paths,
                                                    debounced_event.event.kind
                                                );
                                                changed = true;
                                                break;
                                            }
                                        }
                                    }

                                    if changed {
                                        let _ = update_tx_watcher.send(());
                                    }
                                }
                                Err(errors) => {
                                    for e in errors {
                                        tracing::warn!("File watcher error: {}", e);
                                    }
                                }
                            }
                        },
                    ) {
                        Ok(mut debouncer) => {
                            // Watch all valid source directories
                            for source in sources.iter().filter(|s| s.valid) {
                                if let Err(e) = debouncer
                                    .watcher()
                                    .watch(&source.path, RecursiveMode::Recursive)
                                {
                                    tracing::warn!(
                                        "Failed to watch source {:?}: {}",
                                        source.path,
                                        e
                                    );
                                } else {
                                    tracing::info!("Watching source: {:?}", source.path);
                                }
                            }
                            current_watcher = Some(debouncer);
                        }
                        Err(e) => {
                            tracing::error!("Failed to create file watcher: {}", e);
                        }
                    }
                    should_setup = false;
                }

                // Wait for config update signal
                if config_rx.recv().await.is_ok() {
                    tracing::info!("File watcher: configuration updated, restarting watcher...");
                    should_setup = true;
                }
            }
        });
    }

    // ... rest of main ...
    // Determine port
    // ...
    let config_response = config_manager
        .get_config_response()
        .unwrap_or(ConfigResponse {
            source_mode: SourceMode::Filesystem,
            github: None,
            sources: vec![],
            specs_source_id: None,
            port: 3000,
            read_only: true,
            bind_address: "127.0.0.1".to_string(),
            deduplicate_changes: true,
            status_provider: StatusProvider::Auto,
            openspec_command: "openspec".to_string(),
        });

    let port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(config_response.port);

    // ... (rest is same)

    // Configure CORS from environment variable
    let cors = if let Ok(origins_str) = env::var("CORS_ALLOWED_ORIGINS") {
        // Parse comma-separated origins
        let origins: Vec<String> = origins_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if origins.is_empty() {
            tracing::warn!("CORS_ALLOWED_ORIGINS is set but empty, using safe defaults");
            // Safe defaults: localhost only
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact("http://localhost:3000".parse().unwrap()))
                .allow_methods(Any)
                .allow_headers(Any)
        } else {
            tracing::info!("Configured CORS origins: {:?}", origins);
            let allow_origin = AllowOrigin::list(
                origins
                    .iter()
                    .filter_map(|s| s.parse().ok())
                    .collect::<Vec<_>>(),
            );
            CorsLayer::new()
                .allow_origin(allow_origin)
                .allow_methods(Any)
                .allow_headers(Any)
        }
    } else {
        // Default to safe localhost-only origins if env var is not set
        tracing::info!("CORS_ALLOWED_ORIGINS not set, using safe defaults (localhost only)");
        CorsLayer::new()
            .allow_origin(AllowOrigin::exact("http://localhost:3000".parse().unwrap()))
            .allow_methods(Any)
            .allow_headers(Any)
    };

    let mut app = Router::new()
        .route("/api/health", get(health))
        .route("/api/sync-health", get(get_sync_health))
        .route(
            "/api/github/webhook",
            post(github_webhook).layer(DefaultBodyLimit::max(1024 * 1024)),
        )
        .route("/api/config", get(get_config))
        .route("/api/config/sources", put(update_sources))
        .route("/api/sources", get(get_sources))
        .route("/api/changes", get(get_changes))
        .route("/api/changes/{id}", get(get_change_detail))
        .route("/api/specs", get(get_specs))
        .route("/api/specs/{id}", get(get_spec_detail))
        .route("/api/ideas", get(get_ideas).post(create_idea))
        .route("/api/ideas/{id}", delete(delete_idea).put(update_idea))
        .route("/api/events", get(sse_handler))
        .layer(cors)
        .with_state(state);

    // If FRONTEND_DIR is set, use ServeDir (dev mode), otherwise use embedded assets
    if let Ok(frontend_dir) = env::var("FRONTEND_DIR") {
        tracing::info!("Serving frontend from local directory: {}", frontend_dir);
        let serve_dir = ServeDir::new(frontend_dir);
        app = app
            .nest_service("/", serve_dir.clone())
            .fallback_service(serve_dir);
    } else {
        tracing::info!("Serving embedded frontend assets");
        app = app.fallback(static_handler);
    }

    let bind_address = env::var("BIND_ADDRESS").unwrap_or(config_response.bind_address);
    let ip_address: IpAddr = bind_address.parse().unwrap_or_else(|error| {
        tracing::error!("Invalid bind address '{bind_address}': {error}");
        std::process::exit(1);
    });
    let addr = SocketAddr::new(ip_address, port);
    tracing::info!(
        "Starting server on http://{}:{} (read-only: {})",
        ip_address,
        port,
        config_response.read_only
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_state(read_only: bool) -> (AppState, PathBuf) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let config_path = std::env::temp_dir().join(format!("openspec-ui-config-{suffix}.json"));
        std::fs::write(
            &config_path,
            format!(r#"{{"sources":[],"readOnly":{read_only},"bindAddress":"127.0.0.1"}}"#),
        )
        .unwrap();
        let manager = Arc::new(ConfigManager::new(config_path.clone()));
        let (update_tx, _) = broadcast::channel(1);
        let (config_update_tx, _) = broadcast::channel(1);
        (
            AppState::new(vec![], manager, update_tx, config_update_tx),
            config_path,
        )
    }

    fn specs_test_state(specs_source_id: Option<&str>) -> (AppState, PathBuf) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("openspec-ui-specs-{suffix}"));
        let demo_openspec = root.join("demo/openspec");
        let feature_openspec = root.join("feature/openspec");

        std::fs::create_dir_all(demo_openspec.join("specs/demo-capability")).unwrap();
        std::fs::create_dir_all(feature_openspec.join("specs/feature-capability")).unwrap();
        std::fs::write(
            demo_openspec.join("specs/demo-capability/spec.md"),
            "# Demo capability\n",
        )
        .unwrap();
        std::fs::write(
            feature_openspec.join("specs/feature-capability/spec.md"),
            "# Feature capability\n",
        )
        .unwrap();

        let mut config = serde_json::json!({
            "sources": [
                {"name": "demo-base", "path": demo_openspec},
                {"name": "feature-worktree", "path": feature_openspec}
            ]
        });
        if let Some(source_id) = specs_source_id {
            config["specsSourceId"] = serde_json::Value::String(source_id.to_string());
        }

        let config_path = root.join("config.json");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let manager = Arc::new(ConfigManager::new(config_path));
        let sources = manager.load_sources().unwrap();
        let (update_tx, _) = broadcast::channel(1);
        let (config_update_tx, _) = broadcast::channel(1);

        (
            AppState::new(sources, manager, update_tx, config_update_tx),
            root,
        )
    }

    #[test]
    fn read_only_mode_rejects_mutations() {
        let (state, config_path) = test_state(true);
        let (status, response) = require_writable(&state).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(response.0.error, "OpenSpec UI is running in read-only mode");
        std::fs::remove_file(config_path).ok();
    }

    #[test]
    fn writable_mode_allows_mutations() {
        let (state, config_path) = test_state(false);
        assert!(require_writable(&state).is_ok());
        std::fs::remove_file(config_path).ok();
    }

    #[tokio::test]
    async fn configured_specs_source_restricts_list_and_detail() {
        let (state, root) = specs_test_state(Some("demo-base"));

        let Json(response) = get_specs(State(state.clone())).await;
        assert_eq!(response.specs.len(), 1);
        assert_eq!(response.specs[0].source_id, "demo-base");

        let result = get_spec_detail(
            State(state),
            Path("feature-worktree/feature-capability".to_string()),
        )
        .await;
        assert!(matches!(result, Err(StatusCode::NOT_FOUND)));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn omitted_specs_source_preserves_multi_source_behavior() {
        let (state, root) = specs_test_state(None);

        let Json(response) = get_specs(State(state)).await;
        assert_eq!(response.specs.len(), 2);

        std::fs::remove_dir_all(root).ok();
    }

    async fn github_test_state() -> (AppState, tempfile::TempDir) {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("base");
        let pull = root.path().join("pull");
        std::fs::create_dir_all(base.join("specs/accepted")).unwrap();
        std::fs::create_dir_all(base.join("changes/base-change")).unwrap();
        std::fs::create_dir_all(pull.join("specs/unaccepted")).unwrap();
        std::fs::create_dir_all(pull.join("changes/pr-change")).unwrap();
        std::fs::write(base.join("specs/accepted/spec.md"), "# Accepted\n").unwrap();
        std::fs::write(base.join("changes/base-change/proposal.md"), "# Base\n").unwrap();
        std::fs::write(pull.join("specs/unaccepted/spec.md"), "# Unaccepted\n").unwrap();
        std::fs::write(pull.join("changes/pr-change/proposal.md"), "# PR\n").unwrap();

        let config_path = root.path().join("config.json");
        std::fs::write(
            &config_path,
            serde_json::to_vec(&serde_json::json!({
                "sourceMode":"github",
                "github":{
                    "repository":"ToruAI/openspec-ui",
                    "specsRef":"demo/main",
                    "changesBaseRef":"demo/main",
                    "pullRequestTargets":["demo/main"],
                    "cachePath": root.path().join("cache")
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let manager = Arc::new(ConfigManager::new(config_path));
        let (update_tx, _) = broadcast::channel(4);
        let (config_update_tx, _) = broadcast::channel(1);
        let state = AppState::new(Vec::new(), manager, update_tx, config_update_tx);
        let provenance = |ref_name: &str, pull_request| GithubProvenance {
            repository: "ToruAI/openspec-ui".to_string(),
            ref_name: ref_name.to_string(),
            commit: format!("{ref_name}-sha"),
            html_url: "https://github.com/ToruAI/openspec-ui".to_string(),
            pull_request,
        };
        state
            .publish_snapshot(crate::snapshot::ActiveSnapshot {
                sources: vec![
                    Source {
                        id: "github-base".to_string(),
                        name: "base".to_string(),
                        path: base,
                        valid: true,
                        track: Some("github".to_string()),
                        target_branch: Some("demo/main".to_string()),
                        git: None,
                        github: Some(provenance("demo/main", None)),
                        canonical_specs: true,
                        include_changes: true,
                        merged_changes: Vec::new(),
                    },
                    Source {
                        id: "github-pr-7".to_string(),
                        name: "PR 7".to_string(),
                        path: pull,
                        valid: true,
                        track: Some("github".to_string()),
                        target_branch: Some("demo/main".to_string()),
                        git: None,
                        github: Some(provenance(
                            "feature/pr-change",
                            Some(crate::config::PullRequestProvenance {
                                number: 7,
                                head_ref: "feature/pr-change".to_string(),
                                base_ref: "demo/main".to_string(),
                                html_url: "https://github.com/ToruAI/openspec-ui/pull/7"
                                    .to_string(),
                            }),
                        )),
                        canonical_specs: false,
                        include_changes: true,
                        merged_changes: Vec::new(),
                    },
                ],
                revision: Some("revision-1".to_string()),
                health: crate::snapshot::SyncHealth {
                    state: crate::snapshot::SyncState::Healthy,
                    active_revision: Some("revision-1".to_string()),
                    contributing_refs: Vec::new(),
                    last_attempt_at: Some("2026-08-19T00:00:00Z".to_string()),
                    last_success_at: Some("2026-08-19T00:00:00Z".to_string()),
                    last_failure: None,
                    serving_last_known_good: false,
                },
            })
            .await;
        (state, root)
    }

    #[tokio::test]
    async fn github_mode_serves_specs_only_from_the_canonical_ref() {
        let (state, _root) = github_test_state().await;
        let Json(response) = get_specs(State(state.clone())).await;
        assert_eq!(response.specs.len(), 1);
        assert_eq!(response.specs[0].source_id, "github-base");
        assert_eq!(
            response.specs[0].github.as_ref().unwrap().ref_name,
            "demo/main"
        );

        let result =
            get_spec_detail(State(state), Path("github-pr-7/unaccepted".to_string())).await;
        assert!(matches!(result, Err(StatusCode::NOT_FOUND)));
    }

    #[tokio::test]
    async fn github_mode_serves_base_and_pull_request_changes_with_provenance() {
        let (state, _root) = github_test_state().await;
        let Json(response) = get_changes(State(state)).await;
        assert_eq!(response.changes.len(), 2);
        let pull = response
            .changes
            .iter()
            .find(|change| change.name == "pr-change")
            .unwrap();
        assert_eq!(
            pull.github
                .as_ref()
                .unwrap()
                .pull_request
                .as_ref()
                .unwrap()
                .number,
            7
        );
    }

    #[tokio::test]
    async fn snapshot_publication_emits_once_for_changed_content_only() {
        let (state, _root) = github_test_state().await;
        let mut updates = state.update_tx.subscribe();
        let current_health = state.sync_health().await;
        let no_op = crate::snapshot::ActiveSnapshot {
            sources: state.get_sources().await,
            revision: Some("revision-1".to_string()),
            health: current_health.clone(),
        };
        assert!(!state.publish_snapshot(no_op).await);
        assert!(updates.try_recv().is_err());

        let changed = crate::snapshot::ActiveSnapshot {
            sources: state.get_sources().await,
            revision: Some("revision-2".to_string()),
            health: crate::snapshot::SyncHealth {
                active_revision: Some("revision-2".to_string()),
                ..current_health
            },
        };
        assert!(state.publish_snapshot(changed).await);
        assert!(updates.try_recv().is_ok());
    }
}
