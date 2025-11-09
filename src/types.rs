use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    Success,
    Error,
    Skipped,
}

impl Default for ScanStatus {
    fn default() -> Self {
        ScanStatus::Success
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PIIEntity {
    pub text: String,
    pub label: String,
    pub score: f32,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanResult {
    pub file: Option<String>,
    pub status: ScanStatus,
    pub entities: Vec<PIIEntity>,
    pub pii_count: usize,
    pub file_size_bytes: Option<u64>,
    pub chunks_processed: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanSummary {
    pub total_files: usize,
    pub files_with_pii: usize,
    pub total_pii: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanResponse {
    pub results: Vec<ScanResult>,
    pub summary: ScanSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub model_name: String,
    pub device: String,
    pub gpu_enabled: bool,
    pub labels_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResponse {
    pub model_name: String,
    pub device: String,
    pub gpu_enabled: bool,
    pub default_labels: Vec<String>,
    pub extended_labels: Vec<String>,
    pub private_conversation_labels: Vec<String>,
    pub log_labels: Vec<String>,
    pub default_config: ScannerConfigSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfigSummary {
    pub threshold: f32,
    pub chunk_size: usize,
    pub batch_size: usize,
    pub max_file_size_mb: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanPathRequest {
    pub path: String,
    #[serde(default = "true_bool")]
    pub recursive: bool,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub label_profile: Option<String>,
    #[serde(default)]
    pub use_extended_labels: bool,
    #[serde(default)]
    pub use_private_conversation: bool,
    #[serde(default)]
    pub use_log_signals: bool,
    #[serde(default)]
    pub threshold: Option<f32>,
    #[serde(default)]
    pub chunk_size: Option<usize>,
    #[serde(default)]
    pub batch_size: Option<usize>,
    #[serde(default)]
    pub max_workers: Option<usize>,
    #[serde(default)]
    pub max_file_size: Option<f32>,
    #[serde(default)]
    pub filter_false_positives: Option<bool>,
}

fn true_bool() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
