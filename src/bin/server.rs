use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use axum::extract::{Multipart, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use gliner::types::{
    ErrorResponse, HealthResponse, InfoResponse, ScanPathRequest, ScanResponse, ScanResult,
    ScanStatus, ScannerConfigSummary,
};
use gliner::{
    profile_from_str, FilterPolicy, LabelProfile, ModelArtifacts, PIIScanner, ScannerOverrides,
    ScannerSettings, EXTENDED_PII_LABELS, LOG_DATA_LABELS, PII_LABELS, PRIVATE_CONVERSATION_LABELS,
};
use tempfile::TempDir;
use tokio::fs;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "gliner-server",
    about = "Axum-based GLiNER PII detection server",
    version
)]
struct ServerArgs {
    #[arg(
        short,
        long,
        default_value = "0.0.0.0",
        help = "Host interface to bind the HTTP server"
    )]
    host: String,

    #[arg(
        short,
        long,
        default_value_t = 8000,
        help = "Port to bind the HTTP server"
    )]
    port: u16,

    #[arg(
        long,
        default_value = "models",
        value_name = "DIR",
        help = "Directory containing ONNX model and tokenizer artefacts"
    )]
    artifacts: PathBuf,

    #[arg(
        long,
        default_value_t = 0.3,
        help = "Base confidence threshold (0.0-1.0)"
    )]
    threshold: f32,

    #[arg(
        long,
        default_value_t = 8_000,
        help = "Base maximum characters per chunk"
    )]
    chunk_size: usize,

    #[arg(
        long,
        default_value_t = 8,
        help = "Base number of chunks to process in parallel"
    )]
    batch_size: usize,

    #[arg(long, help = "Maximum parallel workers for directory scanning")]
    max_workers: Option<usize>,

    #[arg(long, default_value_t = 50.0, help = "Maximum file size (MB)")]
    max_file_size: f32,

    #[arg(
        long,
        default_value_t = true,
        help = "Enable GPU acceleration when available"
    )]
    use_gpu: bool,

    #[arg(long, help = "Disable false positive filtering")]
    disable_filters: bool,

    #[arg(
        long,
        value_name = "PROFILE",
        default_value = "default",
        help = "Default label profile (default|extended|private_conversation|log_intelligence)"
    )]
    label_profile: String,

    #[arg(long, default_value_t = true, help = "Allow all origins for CORS")]
    allow_all_origins: bool,

    #[arg(
        long,
        value_name = "ORIGIN",
        help = "Comma-delimited list of allowed origins for CORS"
    )]
    cors_origins: Option<String>,

    #[arg(
        long,
        default_value = "info",
        help = "Log level (error|warn|info|debug|trace)"
    )]
    log_level: String,
}

struct AppState {
    scanner: Arc<PIIScanner>,
    base_settings: ScannerSettings,
    default_profile: LabelProfile,
    label_sets: HashMap<LabelProfile, Vec<String>>,
    model_name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = ServerArgs::parse();
    init_tracing(&args.log_level)?;

    let artifacts = ModelArtifacts::from_directory(&args.artifacts)
        .context("Unable to load model artefacts")?;

    let base_settings = ScannerSettings {
        threshold: args.threshold,
        max_chunk_size: args.chunk_size,
        batch_size: args.batch_size,
        max_workers: args.max_workers,
        max_file_size_mb: args.max_file_size,
        use_gpu: args.use_gpu,
        filter_false_positives: !args.disable_filters,
    };

    let filter_policy = if args.disable_filters {
        FilterPolicy::Disabled
    } else {
        FilterPolicy::Enabled
    };

    let scanner = PIIScanner::initialise(&artifacts, base_settings.clone(), filter_policy)
        .context("Failed to initialise GLiNER scanner")?;

    let default_profile = profile_from_str(&args.label_profile).unwrap_or(LabelProfile::Default);

    let label_sets = build_label_sets();

    let model_name = scanner
        .model_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let state = Arc::new(AppState {
        scanner: Arc::new(scanner),
        base_settings,
        default_profile,
        label_sets,
        model_name,
    });

    let cors = if args.allow_all_origins {
        CorsLayer::permissive()
    } else if let Some(origins) = &args.cors_origins {
        let origins = origins
            .split(',')
            .filter_map(|origin| {
                let trimmed = origin.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    match trimmed.parse::<HeaderValue>() {
                        Ok(value) => Some(value),
                        Err(err) => {
                            error!("Invalid CORS origin '{}': {}", trimmed, err);
                            None
                        }
                    }
                }
            })
            .collect::<Vec<_>>();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        CorsLayer::new()
            .allow_methods(Any)
            .allow_headers(Any)
            .allow_origin(Any)
    };

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/info", get(info_handler))
        .route("/api/v1/scan/path", post(scan_path_handler))
        .route("/api/v1/scan/upload", post(scan_upload_handler))
        .with_state(state)
        .layer(cors);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    info!("Starting gliner-server on http://{}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn root_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "message": "GLiNER PII Detection API",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "health": "/api/v1/health",
            "info": "/api/v1/info",
            "scan_upload": "/api/v1/scan/upload",
            "scan_path": "/api/v1/scan/path",
            "docs": null
        }
    }))
}

async fn health_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let response = HealthResponse {
        status: "healthy".to_string(),
        model_name: state.model_name.clone(),
        device: if state.scanner.uses_gpu() {
            "CUDA".to_string()
        } else {
            "CPU".to_string()
        },
        gpu_enabled: state.scanner.uses_gpu(),
        labels_count: PII_LABELS.len(),
    };

    Json(response)
}

async fn info_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let response = InfoResponse {
        model_name: state.model_name.clone(),
        device: if state.scanner.uses_gpu() {
            "CUDA".to_string()
        } else {
            "CPU".to_string()
        },
        gpu_enabled: state.scanner.uses_gpu(),
        default_labels: PII_LABELS.iter().map(|s| s.to_string()).collect(),
        extended_labels: EXTENDED_PII_LABELS.iter().map(|s| s.to_string()).collect(),
        private_conversation_labels: PRIVATE_CONVERSATION_LABELS
            .iter()
            .map(|s| s.to_string())
            .collect(),
        log_labels: LOG_DATA_LABELS.iter().map(|s| s.to_string()).collect(),
        default_config: ScannerConfigSummary {
            threshold: state.base_settings.threshold,
            chunk_size: state.base_settings.max_chunk_size,
            batch_size: state.base_settings.batch_size,
            max_file_size_mb: state.base_settings.max_file_size_mb,
        },
    };

    Json(response)
}

async fn scan_path_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ScanPathRequest>,
) -> Result<Json<ScanResponse>, (StatusCode, Json<ErrorResponse>)> {
    let path = PathBuf::from(&request.path);
    if !path.exists() {
        return Err(not_found(format!("Path not found: {}", path.display())));
    }

    let labels = resolve_labels(&state, &request)?;
    if labels.is_empty() {
        return Err(bad_request("No labels selected for scanning"));
    }

    let overrides = build_overrides_from_path_request(&request);
    let settings = state.base_settings.merged(&overrides);

    let results = if path.is_file() {
        match state.scanner.scan_file_with(&path, &labels, &settings) {
            Ok(result) => vec![result],
            Err(err) => {
                return Err(internal_error(format!(
                    "Failed to scan file {}: {}",
                    path.display(),
                    err
                )))
            }
        }
    } else {
        match state
            .scanner
            .scan_directory_with(&path, &labels, request.recursive, &settings)
        {
            Ok(results) => results,
            Err(err) => {
                return Err(internal_error(format!(
                    "Failed to scan directory {}: {}",
                    path.display(),
                    err
                )))
            }
        }
    };

    Ok(Json(ScanResponse {
        summary: summarise(&results),
        results,
    }))
}

async fn scan_upload_handler(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<ScanResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut request = ScanPathRequest {
        path: String::new(),
        recursive: true,
        labels: None,
        label_profile: None,
        use_extended_labels: false,
        use_private_conversation: false,
        use_log_signals: false,
        threshold: None,
        chunk_size: None,
        batch_size: None,
        max_workers: None,
        max_file_size: None,
        filter_false_positives: None,
    };

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| internal_error(format!("Failed to parse multipart form: {}", err)))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let filename = field.file_name().map(|fname| fname.to_string());
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|err| internal_error(format!("Failed to read upload: {}", err)))?
                        .to_vec(),
                );
                file_name = filename;
            }
            "labels" => {
                let value = field
                    .text()
                    .await
                    .map_err(|err| internal_error(format!("Failed to read labels: {}", err)))?;
                request.labels = Some(
                    value
                        .split(',')
                        .map(|label| label.trim().to_string())
                        .filter(|label| !label.is_empty())
                        .collect(),
                );
            }
            "label_profile" => {
                request.label_profile = Some(field.text().await.map_err(|err| {
                    internal_error(format!("Failed to read label_profile: {}", err))
                })?);
            }
            "use_extended_labels" => {
                request.use_extended_labels = parse_bool_field(field)
                    .await
                    .map_err(|err| internal_error(err.to_string()))?;
            }
            "use_private_conversation" => {
                request.use_private_conversation = parse_bool_field(field)
                    .await
                    .map_err(|err| internal_error(err.to_string()))?;
            }
            "use_log_signals" => {
                request.use_log_signals = parse_bool_field(field)
                    .await
                    .map_err(|err| internal_error(err.to_string()))?;
            }
            "threshold" => {
                request.threshold =
                    Some(parse_f32_field(field).await.map_err(|err| {
                        internal_error(format!("Invalid threshold value: {}", err))
                    })?);
            }
            "chunk_size" => {
                request.chunk_size =
                    Some(parse_usize_field(field).await.map_err(|err| {
                        internal_error(format!("Invalid chunk_size value: {}", err))
                    })?);
            }
            "batch_size" => {
                request.batch_size =
                    Some(parse_usize_field(field).await.map_err(|err| {
                        internal_error(format!("Invalid batch_size value: {}", err))
                    })?);
            }
            "max_workers" => {
                request.max_workers = Some(parse_usize_field(field).await.map_err(|err| {
                    internal_error(format!("Invalid max_workers value: {}", err))
                })?);
            }
            "max_file_size" => {
                request.max_file_size = Some(parse_f32_field(field).await.map_err(|err| {
                    internal_error(format!("Invalid max_file_size value: {}", err))
                })?);
            }
            "filter_false_positives" => {
                request.filter_false_positives = Some(
                    parse_bool_field(field)
                        .await
                        .map_err(|err| internal_error(err.to_string()))?,
                );
            }
            _ => {}
        }
    }

    let bytes = file_bytes.ok_or_else(|| bad_request("Missing 'file' field in upload"))?;
    let file_len = bytes.len();

    let labels = resolve_labels(&state, &request)?;
    if labels.is_empty() {
        return Err(bad_request("No labels selected for scanning"));
    }

    let overrides = build_overrides_from_path_request(&request);
    let settings = state.base_settings.merged(&overrides);

    let temp_dir = TempDir::new()
        .map_err(|err| internal_error(format!("Failed to create temp dir: {}", err)))?;
    let filename = file_name.unwrap_or_else(|| "upload.txt".to_string());
    let upload_path = temp_dir.path().join(filename);

    fs::write(&upload_path, &bytes)
        .await
        .map_err(|err| internal_error(format!("Failed to write temp file: {}", err)))?;

    let mut result = state
        .scanner
        .scan_file_with(&upload_path, &labels, &settings)
        .map_err(|err| internal_error(format!("Upload scan failed: {}", err)))?;

    result.file = Some(upload_path.display().to_string());
    result.file_size_bytes = Some(file_len as u64);

    Ok(Json(ScanResponse {
        summary: summarise(&[result.clone()]),
        results: vec![result],
    }))
}

fn build_label_sets() -> HashMap<LabelProfile, Vec<String>> {
    let mut map = HashMap::new();
    map.insert(
        LabelProfile::Default,
        PII_LABELS.iter().map(|s| s.to_string()).collect(),
    );
    map.insert(
        LabelProfile::Extended,
        EXTENDED_PII_LABELS.iter().map(|s| s.to_string()).collect(),
    );
    map.insert(
        LabelProfile::PrivateConversation,
        PRIVATE_CONVERSATION_LABELS
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    map.insert(
        LabelProfile::LogIntelligence,
        LOG_DATA_LABELS.iter().map(|s| s.to_string()).collect(),
    );
    map
}

fn resolve_labels(
    state: &AppState,
    request: &ScanPathRequest,
) -> Result<Vec<String>, (StatusCode, Json<ErrorResponse>)> {
    if let Some(labels) = &request.labels {
        if labels.is_empty() {
            return Err(bad_request("Custom label list is empty"));
        }
        return Ok(deduplicate(labels.clone()));
    }

    let mut selected = Vec::new();
    let profile = request
        .label_profile
        .as_ref()
        .and_then(|name| profile_from_str(name))
        .unwrap_or(state.default_profile);

    if let Some(base) = state.label_sets.get(&profile) {
        selected.extend(base.iter().cloned());
    }

    if request.use_extended_labels {
        if let Some(extended) = state.label_sets.get(&LabelProfile::Extended) {
            selected.extend(extended.iter().cloned());
        }
    }

    if request.use_private_conversation {
        if let Some(private) = state.label_sets.get(&LabelProfile::PrivateConversation) {
            selected.extend(private.iter().cloned());
        }
    }

    if request.use_log_signals {
        if let Some(logs) = state.label_sets.get(&LabelProfile::LogIntelligence) {
            selected.extend(logs.iter().cloned());
        }
    }

    if selected.is_empty() {
        selected.extend(
            state
                .label_sets
                .get(&LabelProfile::Default)
                .cloned()
                .unwrap_or_default(),
        );
    }

    Ok(deduplicate(selected))
}

fn deduplicate(labels: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    labels
        .into_iter()
        .filter(|label| seen.insert(label.to_lowercase()))
        .collect()
}

fn build_overrides_from_path_request(request: &ScanPathRequest) -> ScannerOverrides {
    ScannerOverrides {
        threshold: request.threshold,
        max_chunk_size: request.chunk_size,
        batch_size: request.batch_size,
        max_workers: request.max_workers,
        max_file_size_mb: request.max_file_size,
        use_gpu: None,
        filter_false_positives: request.filter_false_positives,
    }
}

fn summarise(results: &[ScanResult]) -> gliner::types::ScanSummary {
    let total_files = results.len();
    let files_with_pii = results
        .iter()
        .filter(|result| matches!(result.status, ScanStatus::Success) && result.pii_count > 0)
        .count();
    let total_pii = results
        .iter()
        .filter(|result| matches!(result.status, ScanStatus::Success))
        .map(|result| result.pii_count)
        .sum();
    let errors = results
        .iter()
        .filter(|result| matches!(result.status, ScanStatus::Error))
        .count();

    gliner::types::ScanSummary {
        total_files,
        files_with_pii,
        total_pii,
        errors,
    }
}

async fn parse_bool_field(field: axum::extract::multipart::Field<'_>) -> Result<bool> {
    let text = field.text().await?;
    Ok(matches!(
        text.trim().to_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    ))
}

async fn parse_usize_field(field: axum::extract::multipart::Field<'_>) -> Result<usize> {
    let text = field.text().await?;
    text.trim()
        .parse::<usize>()
        .map_err(|err| anyhow!(err.to_string()))
}

async fn parse_f32_field(field: axum::extract::multipart::Field<'_>) -> Result<f32> {
    let text = field.text().await?;
    text.trim()
        .parse::<f32>()
        .map_err(|err| anyhow!(err.to_string()))
}

fn internal_error(message: String) -> (StatusCode, Json<ErrorResponse>) {
    error!("{message}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            message: "Internal server error".to_string(),
            detail: Some(message),
        }),
    )
}

fn bad_request(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            message: message.into(),
            detail: None,
        }),
    )
}

fn not_found(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            message: message.into(),
            detail: None,
        }),
    )
}

fn init_tracing(level: &str) -> Result<()> {
    let filter = EnvFilter::try_new(level)
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received. Stopping server...");
}
