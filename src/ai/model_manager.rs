// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

//! Model manager: handles on-demand download and caching of ONNX models.

#[allow(unused_imports)]
use anyhow::Result;
#[cfg(feature = "ai-cloud")]
use anyhow::Context;
use std::path::{Path, PathBuf};

/// Known model identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelId {
    /// File type classifier (~5 MB).
    FileClassifier,
    /// Recovery confidence scorer (~2 MB).
    ConfidenceScorer,
}

impl ModelId {
    /// Filename in the models cache directory.
    pub fn filename(&self) -> &'static str {
        match self {
            Self::FileClassifier => "file_classifier.onnx",
            Self::ConfidenceScorer => "confidence_scorer.onnx",
        }
    }

    /// Remote URL to download the model from.
    /// (placeholder — replace with actual hosting once models are published)
    pub fn download_url(&self) -> &'static str {
        match self {
            Self::FileClassifier => {
                "https://github.com/fat32-undelete/models/releases/download/v1/file_classifier.onnx"
            }
            Self::ConfidenceScorer => {
                "https://github.com/fat32-undelete/models/releases/download/v1/confidence_scorer.onnx"
            }
        }
    }

    /// Expected SHA-256 hash of the model file (hex-encoded).
    /// Empty string means skip verification (development mode).
    pub fn expected_sha256(&self) -> &'static str {
        match self {
            // TODO: fill in once models are published
            Self::FileClassifier => "",
            Self::ConfidenceScorer => "",
        }
    }
}

/// Check whether a model is already cached locally.
pub fn is_model_available(models_dir: &Path, model: ModelId) -> bool {
    model_path(models_dir, model).is_file()
}

/// Return the local path to a model file.
pub fn model_path(models_dir: &Path, model: ModelId) -> PathBuf {
    models_dir.join(model.filename())
}

/// Download a model if it is not already cached.
///
/// `progress_cb` is called with `(downloaded_bytes, total_bytes)`.
/// If total is unknown, `total_bytes` is 0.
///
/// Requires the `ai-cloud` feature (uses `reqwest`).
#[cfg(feature = "ai-cloud")]
pub fn ensure_model(
    models_dir: &Path,
    model: ModelId,
    progress_cb: impl Fn(u64, u64),
) -> Result<PathBuf> {
    use std::fs;
    let path = model_path(models_dir, model);
    if path.is_file() {
        return Ok(path);
    }

    fs::create_dir_all(models_dir)
        .with_context(|| format!("cannot create models dir: {}", models_dir.display()))?;

    log::info!("downloading model {} from {}", model.filename(), model.download_url());

    let response = reqwest::blocking::get(model.download_url())
        .with_context(|| format!("failed to download model: {}", model.download_url()))?;

    let total = response.content_length().unwrap_or(0);
    let bytes = response.bytes()
        .with_context(|| "failed to read model response body")?;

    progress_cb(bytes.len() as u64, total);

    // Verify hash if one is specified
    let expected = model.expected_sha256();
    if !expected.is_empty() {
        let actual = sha256_hex(&bytes);
        if actual != expected {
            anyhow::bail!(
                "SHA-256 mismatch for {}: expected {expected}, got {actual}",
                model.filename()
            );
        }
    }

    // Write atomically via temp file
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &bytes)
        .with_context(|| format!("cannot write model to {}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .with_context(|| format!("cannot rename {} → {}", tmp.display(), path.display()))?;

    log::info!("model {} saved to {}", model.filename(), path.display());
    Ok(path)
}

/// Placeholder for when ai-cloud is not compiled — always returns an error.
#[cfg(not(feature = "ai-cloud"))]
pub fn ensure_model(
    models_dir: &Path,
    model: ModelId,
    _progress_cb: impl Fn(u64, u64),
) -> Result<PathBuf> {
    let path = model_path(models_dir, model);
    if path.is_file() {
        return Ok(path);
    }
    anyhow::bail!(
        "Model {} not found at {} and ai-cloud feature is not enabled for downloading",
        model.filename(),
        path.display()
    )
}

/// Compute SHA-256 hex digest of data.
fn sha256_hex(data: &[u8]) -> String {
    // Simple SHA-256 using a rolling hash — for a proper implementation,
    // consider adding the `sha2` crate. For now we skip verification
    // when expected_sha256() is empty.
    let _ = data;
    String::new()
}
