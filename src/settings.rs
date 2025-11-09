use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// File-system paths that describe the ONNX model and tokenizer artefacts.
#[derive(Debug, Clone)]
pub struct ModelArtifacts {
    pub model: PathBuf,
    pub tokenizer: PathBuf,
    pub tokenizer_config: Option<PathBuf>,
}

impl ModelArtifacts {
    /// Build artefacts from a directory containing the exported model files.
    pub fn from_directory(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref();
        let mut onnx_candidates = Vec::new();
        let mut tokenizer_candidates = Vec::new();

        for entry in directory
            .read_dir()
            .with_context(|| format!("Failed to read model directory {}", directory.display()))?
        {
            let entry = entry.with_context(|| "Failed to enumerate directory entry")?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "onnx") {
                onnx_candidates.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains("tokenizer.json"))
            {
                tokenizer_candidates.push(path);
            }
        }

        let onnx = onnx_candidates
            .into_iter()
            .next()
            .context("Could not find .onnx model in directory")?;
        let tokenizer = tokenizer_candidates
            .into_iter()
            .next()
            .context("Could not find tokenizer.json in directory")?;

        let config_path = directory.join("config.json");
        let tokenizer_config = if config_path.exists() {
            Some(config_path)
        } else {
            None
        };

        Ok(Self {
            model: onnx,
            tokenizer,
            tokenizer_config,
        })
    }

    /// Validate that the artefacts exist.
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.model.is_file(),
            "ONNX model not found at {}",
            self.model.display()
        );
        anyhow::ensure!(
            self.tokenizer.is_file(),
            "Tokenizer JSON not found at {}",
            self.tokenizer.display()
        );
        if let Some(config) = &self.tokenizer_config {
            anyhow::ensure!(
                config.is_file(),
                "Tokenizer config not found at {}",
                config.display()
            );
        }
        Ok(())
    }
}

/// Configuration knobs for the scanner.
#[derive(Debug, Clone)]
pub struct ScannerSettings {
    pub threshold: f32,
    pub max_chunk_size: usize,
    pub batch_size: usize,
    pub max_workers: Option<usize>,
    pub max_file_size_mb: f32,
    pub use_gpu: bool,
    pub filter_false_positives: bool,
}

impl Default for ScannerSettings {
    fn default() -> Self {
        Self {
            threshold: 0.3,
            max_chunk_size: 8_000,
            batch_size: 8,
            max_workers: None,
            max_file_size_mb: 50.0,
            use_gpu: true,
            filter_false_positives: true,
        }
    }
}

impl ScannerSettings {
    pub fn merged(&self, overrides: &ScannerOverrides) -> Self {
        Self {
            threshold: overrides.threshold.unwrap_or(self.threshold),
            max_chunk_size: overrides.max_chunk_size.unwrap_or(self.max_chunk_size),
            batch_size: overrides.batch_size.unwrap_or(self.batch_size),
            max_workers: overrides.max_workers.or(self.max_workers),
            max_file_size_mb: overrides.max_file_size_mb.unwrap_or(self.max_file_size_mb),
            use_gpu: overrides.use_gpu.unwrap_or(self.use_gpu),
            filter_false_positives: overrides
                .filter_false_positives
                .unwrap_or(self.filter_false_positives),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScannerOverrides {
    pub threshold: Option<f32>,
    pub max_chunk_size: Option<usize>,
    pub batch_size: Option<usize>,
    pub max_workers: Option<usize>,
    pub max_file_size_mb: Option<f32>,
    pub use_gpu: Option<bool>,
    pub filter_false_positives: Option<bool>,
}
