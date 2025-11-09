mod chunk;
mod filters;
mod runtime;

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use ndarray::{s, Array2};
use serde::Deserialize;
use tokenizers::PaddingDirection;
use tokenizers::PaddingParams;
use tokenizers::PaddingStrategy;
use tokenizers::Tokenizer;
use tokenizers::TruncationParams;
use walkdir::WalkDir;

use crate::labels::TEXT_EXTENSIONS;
use crate::settings::{ModelArtifacts, ScannerSettings};
use crate::types::{PIIEntity, ScanResult, ScanStatus};

use self::chunk::{chunk_text, TextChunk};
use self::filters::FalsePositiveFilters;
use self::runtime::OnnxEngine;

#[derive(Debug, Clone, Copy)]
pub enum FilterPolicy {
    Enabled,
    Disabled,
}

#[derive(Debug, Deserialize, Default)]
struct TokenizerRuntimeConfig {
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default = "default_max_length")]
    max_length: usize,
    #[serde(default)]
    labels: Vec<String>,
}

fn default_max_length() -> usize {
    128
}

pub struct PIIScanner {
    runtime: Arc<OnnxEngine>,
    tokenizer: Arc<Tokenizer>,
    settings: ScannerSettings,
    filters: FalsePositiveFilters,
    filter_policy: FilterPolicy,
    max_length: usize,
    label_vocab: Vec<String>,
    model_name: Option<String>,
}

impl PIIScanner {
    pub fn initialise(
        artifacts: &ModelArtifacts,
        settings: ScannerSettings,
        filter_policy: FilterPolicy,
    ) -> Result<Self> {
        artifacts.validate()?;

        let onnx = OnnxEngine::new(&artifacts.model, settings.use_gpu)?;
        let tokenizer_path = artifacts.tokenizer.clone();
        let mut tokenizer = Tokenizer::from_file(tokenizer_path.clone()).map_err(|err| {
            anyhow!(
                "Failed to load tokenizer from {}: {}",
                tokenizer_path.display(),
                err
            )
        })?;

        let runtime_config = if let Some(config_path) = &artifacts.tokenizer_config {
            let content = fs::read_to_string(config_path).with_context(|| {
                format!("Failed to read tokenizer config {}", config_path.display())
            })?;
            serde_json::from_str::<TokenizerRuntimeConfig>(&content).with_context(|| {
                format!("Invalid tokenizer config JSON {}", config_path.display())
            })?
        } else {
            TokenizerRuntimeConfig::default()
        };

        let max_length = runtime_config.max_length;

        let _ = tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(max_length),
            direction: PaddingDirection::Right,
            pad_to_multiple_of: None,
            pad_id: 0,
            pad_token: "[PAD]".to_string(),
            pad_type_id: 0,
        }));
        let _ = tokenizer.with_truncation(Some(TruncationParams {
            max_length,
            ..Default::default()
        }));

        let label_vocab = if runtime_config.labels.is_empty() {
            anyhow::bail!(
                "Tokenizer config does not define label vocab; re-export model with labels"
            );
        } else {
            runtime_config.labels
        };
        let model_name = runtime_config.model_name;

        Ok(Self {
            runtime: Arc::new(onnx),
            tokenizer: Arc::new(tokenizer),
            settings,
            filters: FalsePositiveFilters::new(),
            filter_policy,
            max_length,
            label_vocab,
            model_name,
        })
    }

    pub fn model_name(&self) -> Option<&str> {
        self.model_name.as_deref()
    }

    pub fn uses_gpu(&self) -> bool {
        self.runtime.uses_gpu()
    }

    pub fn scan_text(&self, text: &str, labels: &[String]) -> Result<Vec<PIIEntity>> {
        self.scan_text_with(text, labels, &self.settings)
    }

    pub fn scan_text_with(
        &self,
        text: &str,
        labels: &[String],
        settings: &ScannerSettings,
    ) -> Result<Vec<PIIEntity>> {
        let chunks = chunk_text(text, settings.max_chunk_size);

        let encodings = self.encode_chunks(&chunks)?;
        let mut queued = VecDeque::from(encodings);

        let mut entities = Vec::new();
        let mut seen = HashSet::new();

        while !queued.is_empty() {
            let batch = self
                .next_batch(&mut queued, settings.batch_size)
                .collect::<Vec<_>>();

            let batch_results = self.execute_batch(&batch)?;

            for (encoding, logits) in batch.into_iter().zip(batch_results.into_iter()) {
                let chunk = &encoding.chunk;
                let detected =
                    self.decode_entities(chunk, &encoding, &logits, labels, text, settings)?;
                for entity in detected {
                    let key = (entity.start, entity.end, entity.label.clone());
                    if seen.insert(key.clone()) {
                        entities.push(entity);
                    }
                }
            }
        }

        Ok(entities)
    }

    pub fn scan_file(&self, path: impl AsRef<Path>, labels: &[String]) -> Result<ScanResult> {
        self.scan_file_with(path, labels, &self.settings)
    }

    pub fn scan_file_with(
        &self,
        path: impl AsRef<Path>,
        labels: &[String],
        settings: &ScannerSettings,
    ) -> Result<ScanResult> {
        let path = path.as_ref();
        let metadata = path
            .metadata()
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        let file_size_bytes = metadata.len();
        let file_size_mb = (file_size_bytes as f32) / (1024.0 * 1024.0);

        if file_size_mb > settings.max_file_size_mb {
            return Ok(ScanResult {
                file: Some(path.display().to_string()),
                status: ScanStatus::Skipped,
                entities: Vec::new(),
                pii_count: 0,
                file_size_bytes: Some(file_size_bytes),
                chunks_processed: 0,
                error: Some(format!(
                    "File too large ({:.1}MB > {:.1}MB)",
                    file_size_mb, settings.max_file_size_mb
                )),
            });
        }

        let content = read_file_with_fallbacks(path)?;
        if content.trim().is_empty() {
            return Ok(ScanResult {
                file: Some(path.display().to_string()),
                status: ScanStatus::Skipped,
                entities: Vec::new(),
                pii_count: 0,
                file_size_bytes: Some(file_size_bytes),
                chunks_processed: 0,
                error: Some("Empty file".to_string()),
            });
        }

        let chunks_count = chunk_text(&content, settings.max_chunk_size).len();
        let entities = self.scan_text_with(&content, labels, settings)?;
        let pii_count = entities.len();

        Ok(ScanResult {
            file: Some(path.display().to_string()),
            status: ScanStatus::Success,
            entities,
            pii_count,
            file_size_bytes: Some(file_size_bytes),
            chunks_processed: chunks_count,
            error: None,
        })
    }

    pub fn scan_directory(
        &self,
        directory: impl AsRef<Path>,
        labels: &[String],
        recursive: bool,
    ) -> Result<Vec<ScanResult>> {
        self.scan_directory_with(directory, labels, recursive, &self.settings)
    }

    pub fn scan_directory_with(
        &self,
        directory: impl AsRef<Path>,
        labels: &[String],
        recursive: bool,
        settings: &ScannerSettings,
    ) -> Result<Vec<ScanResult>> {
        let directory = directory.as_ref();
        let mut results = Vec::new();

        let walker = if recursive {
            WalkDir::new(directory).into_iter()
        } else {
            WalkDir::new(directory).max_depth(1).into_iter()
        };

        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    results.push(ScanResult {
                        file: err.path().map(|p| p.display().to_string()),
                        status: ScanStatus::Error,
                        entities: Vec::new(),
                        pii_count: 0,
                        file_size_bytes: None,
                        chunks_processed: 0,
                        error: Some(err.to_string()),
                    });
                    continue;
                }
            };

            if entry.file_type().is_dir() {
                continue;
            }
            let path = entry.path();
            if !is_text_extension(path) {
                continue;
            }
            let scan = self
                .scan_file_with(path, labels, settings)
                .with_context(|| format!("Failed scanning {}", path.display()));
            match scan {
                Ok(result) => results.push(result),
                Err(err) => results.push(ScanResult {
                    file: Some(path.display().to_string()),
                    status: ScanStatus::Error,
                    entities: Vec::new(),
                    pii_count: 0,
                    file_size_bytes: None,
                    chunks_processed: 0,
                    error: Some(err.to_string()),
                }),
            }
        }

        Ok(results)
    }

    fn encode_chunks(&self, chunks: &[TextChunk]) -> Result<Vec<EncodedChunk>> {
        let mut encoded = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let encoding = self
                .tokenizer
                .encode(chunk.text.as_str(), true)
                .map_err(|err| anyhow!("Failed to encode text chunk: {}", err))?;
            encoded.push(EncodedChunk {
                chunk: chunk.clone(),
                encoding,
            });
        }
        Ok(encoded)
    }

    fn next_batch<'a>(
        &self,
        queue: &'a mut VecDeque<EncodedChunk>,
        batch_size: usize,
    ) -> impl Iterator<Item = EncodedChunk> + 'a {
        let size = batch_size.min(queue.len()).max(1);
        (0..size).filter_map(move |_| queue.pop_front())
    }

    fn execute_batch(&self, batch: &[EncodedChunk]) -> Result<Vec<BatchLogits>> {
        let batch_size = batch.len();
        let seq_len = batch
            .first()
            .map(|item| item.encoding.len())
            .unwrap_or(self.max_length);

        let mut input_ids = Array2::<i64>::zeros((batch_size, seq_len));
        let mut attention_mask = Array2::<i64>::zeros((batch_size, seq_len));
        let mut token_type_ids = Array2::<i64>::zeros((batch_size, seq_len));
        let mut has_token_types = false;

        for (row, encoded) in batch.iter().enumerate() {
            let ids = encoded.encoding.get_ids();
            let mask = encoded.encoding.get_attention_mask();
            let type_ids = encoded.encoding.get_type_ids();

            for (col, value) in ids.iter().enumerate() {
                input_ids[[row, col]] = i64::from(*value);
            }
            for (col, value) in mask.iter().enumerate() {
                attention_mask[[row, col]] = i64::from(*value);
            }
            if type_ids.iter().any(|id| *id != 0) {
                has_token_types = true;
            }
            for (col, value) in type_ids.iter().enumerate() {
                token_type_ids[[row, col]] = i64::from(*value);
            }
        }

        let logits = self.runtime.run(
            &input_ids,
            Some(&attention_mask),
            if has_token_types {
                Some(&token_type_ids)
            } else {
                None
            },
        )?;

        Ok(logits
            .axis_iter(ndarray::Axis(0))
            .map(|slice| BatchLogits {
                logits: slice.to_owned(),
            })
            .collect())
    }

    fn decode_entities(
        &self,
        chunk: &TextChunk,
        encoded: &EncodedChunk,
        logits: &BatchLogits,
        requested_labels: &[String],
        original_text: &str,
        settings: &ScannerSettings,
    ) -> Result<Vec<PIIEntity>> {
        let mut entities = Vec::new();
        let mut current: Option<PendingEntity> = None;

        let token_offsets = encoded.encoding.get_offsets();
        let attention_mask = encoded.encoding.get_attention_mask();

        for (index, offsets) in token_offsets.iter().enumerate() {
            if index >= logits.logits.shape()[1] {
                break;
            }
            if attention_mask[index] == 0 {
                continue;
            }
            let (start, end) = *offsets;
            if end <= start {
                continue;
            }
            let slice = logits.logits.slice(s![index, ..]).to_owned();
            let (label_idx, score) = argmax(&slice);
            if self.label_vocab.is_empty() || label_idx >= self.label_vocab.len() {
                continue;
            }
            let label = &self.label_vocab[label_idx];
            if label == "O" {
                if let Some(entity) = current.take() {
                    if self.should_emit_entity(&entity, requested_labels, settings) {
                        entities.push(self.finalise_entity(chunk, entity, original_text));
                    }
                }
                continue;
            }
            if !self.label_allowed(label, requested_labels) {
                continue;
            }
            if score < settings.threshold {
                if let Some(entity) = current.take() {
                    if self.should_emit_entity(&entity, requested_labels, settings) {
                        entities.push(self.finalise_entity(chunk, entity, original_text));
                    }
                }
                continue;
            }

            let entity_text = &chunk.text[start..end];
            let absolute_start = chunk.offset + start;
            let absolute_end = chunk.offset + end;

            if let Some(active) = &mut current {
                if active.label == *label && active.end == absolute_start {
                    active.end = absolute_end;
                    active.score = active.score.max(score);
                    active
                        .text
                        .push_str(chunk.text.get(start..end).unwrap_or_default());
                    continue;
                }
                if self.should_emit_entity(active, requested_labels, settings) {
                    entities.push(self.finalise_entity(chunk, active.clone(), original_text));
                }
            }

            current = Some(PendingEntity {
                label: label.clone(),
                start: absolute_start,
                end: absolute_end,
                score,
                text: entity_text.to_string(),
            });
        }

        if let Some(entity) = current {
            if self.should_emit_entity(&entity, requested_labels, settings) {
                entities.push(self.finalise_entity(chunk, entity, original_text));
            }
        }

        Ok(entities)
    }

    fn label_allowed(&self, label: &str, requested_labels: &[String]) -> bool {
        if requested_labels.is_empty() {
            return true;
        }
        requested_labels
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(label))
    }

    fn should_emit_entity(
        &self,
        entity: &PendingEntity,
        requested_labels: &[String],
        settings: &ScannerSettings,
    ) -> bool {
        if entity.score < settings.threshold {
            return false;
        }
        if !self.label_allowed(&entity.label, requested_labels) {
            return false;
        }
        let filters_enabled =
            settings.filter_false_positives && matches!(self.filter_policy, FilterPolicy::Enabled);
        if !filters_enabled {
            return true;
        }
        !self
            .filters
            .is_false_positive(&entity.label, &entity.text, entity.score)
    }

    fn finalise_entity(
        &self,
        chunk: &TextChunk,
        entity: PendingEntity,
        original_text: &str,
    ) -> PIIEntity {
        let text = original_text
            .get(entity.start..entity.end)
            .unwrap_or_else(|| chunk.text.get(0..0).unwrap_or(""));
        PIIEntity {
            text: text.to_string(),
            label: entity.label,
            score: entity.score,
            start: entity.start,
            end: entity.end,
        }
    }
}

struct EncodedChunk {
    chunk: TextChunk,
    encoding: tokenizers::Encoding,
}

struct BatchLogits {
    logits: ndarray::Array2<f32>,
}

#[derive(Clone)]
struct PendingEntity {
    label: String,
    start: usize,
    end: usize,
    score: f32,
    text: String,
}

fn argmax(array: &ndarray::Array1<f32>) -> (usize, f32) {
    let mut best_idx = 0usize;
    let mut best_score = f32::MIN;
    for (idx, value) in array.iter().enumerate() {
        if *value > best_score {
            best_score = *value;
            best_idx = idx;
        }
    }
    (best_idx, best_score)
}

fn is_text_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_lowercase()))
        .map_or(false, |ext| TEXT_EXTENSIONS.contains(&ext.as_str()))
}

fn read_file_with_fallbacks(path: &Path) -> Result<String> {
    if let Ok(content) = fs::read_to_string(path) {
        return Ok(content);
    }
    let bytes = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    for label in ["latin-1", "windows-1252"] {
        if let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            let (decoded, _, had_errors) = encoding.decode(&bytes);
            if !had_errors {
                return Ok(decoded.into_owned());
            }
        }
    }
    anyhow::bail!("Unable to decode file {}", path.display())
}
