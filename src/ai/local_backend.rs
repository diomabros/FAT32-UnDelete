// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

//! Local AI backend using ONNX Runtime for inference.
//!
//! This module is only compiled when the `ai-local` feature is enabled.

use anyhow::Result;

use super::config::AiConfig;
use super::model_manager::{self, ModelId};
use super::scorer::ScoringFeatures;
use super::{classifier, ClassificationResult, FileFeatures, ScoringResult};

/// Classify a file using the local ONNX model, falling back to heuristics.
pub fn classify(features: &FileFeatures, config: &AiConfig) -> Result<ClassificationResult> {
    // Try ONNX model first if available
    if model_manager::is_model_available(&config.models_dir, ModelId::FileClassifier) {
        if let Some(result) = classify_onnx(features, config) {
            return Ok(result);
        }
    }

    // Fall back to heuristic classifier
    classifier::classify_heuristic(features)
        .ok_or_else(|| anyhow::anyhow!("could not classify file (no matching profile)"))
}

/// Score recovery confidence using the local ONNX model, falling back to heuristics.
pub fn score(features: &ScoringFeatures, config: &AiConfig) -> Result<ScoringResult> {
    // Try ONNX model first if available
    if model_manager::is_model_available(&config.models_dir, ModelId::ConfidenceScorer) {
        if let Some(result) = score_onnx(features, config) {
            return Ok(result);
        }
    }

    // Fall back to heuristic scorer
    Ok(super::scorer::score_heuristic(features))
}

/// ONNX-based file classification.
fn classify_onnx(features: &FileFeatures, config: &AiConfig) -> Option<ClassificationResult> {
    let model_path = model_manager::model_path(&config.models_dir, ModelId::FileClassifier);

    match try_classify_onnx(features, &model_path) {
        Ok(result) => Some(result),
        Err(e) => {
            log::warn!("ONNX classifier failed, falling back to heuristics: {e}");
            None
        }
    }
}

fn try_classify_onnx(
    features: &FileFeatures,
    model_path: &std::path::Path,
) -> Result<ClassificationResult> {
    let mut session = ort::session::Session::builder()
        .map_err(|e| anyhow::anyhow!("ort session builder: {e}"))?
        .with_intra_threads(1)
        .map_err(|e| anyhow::anyhow!("ort intra threads: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow::anyhow!("ort load model: {e}"))?;

    // Build input tensor: [1, 258] — 256 byte dist + entropy + file_size_log
    let mut input_data = Vec::with_capacity(258);
    input_data.extend_from_slice(&features.byte_distribution);
    input_data.push(features.entropy);
    input_data.push((features.file_size as f32 + 1.0).ln());

    let input_tensor = ort::value::Value::from_array(
        ndarray::Array::from_shape_vec((1, 258), input_data)
            .map_err(|e| anyhow::anyhow!("tensor shape error: {e}"))?,
    )
    .map_err(|e| anyhow::anyhow!("ort from_array: {e}"))?;

    let outputs = session
        .run(ort::inputs![input_tensor])
        .map_err(|e| anyhow::anyhow!("ort run: {e}"))?;

    // Parse output: expect a softmax probability vector
    let (_shape, probs) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| anyhow::anyhow!("ort extract: {e}"))?;

    // Map class index to file type
    const CLASSES: &[(&str, &str)] = &[
        ("JPEG", "jpg"),
        ("PNG", "png"),
        ("GIF", "gif"),
        ("PDF", "pdf"),
        ("ZIP", "zip"),
        ("BMP", "bmp"),
        ("MP3", "mp3"),
        ("RAR", "rar"),
        ("7Z", "7z"),
        ("TIFF", "tiff"),
        ("EXE", "exe"),
        ("Unknown", "bin"),
    ];

    let (max_idx, &max_prob) = probs
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((CLASSES.len() - 1, &0.0));

    let idx = max_idx.min(CLASSES.len() - 1);
    Ok(ClassificationResult {
        predicted_type: CLASSES[idx].0.to_string(),
        predicted_extension: CLASSES[idx].1.to_string(),
        confidence: max_prob,
    })
}

/// ONNX-based confidence scoring.
fn score_onnx(features: &ScoringFeatures, config: &AiConfig) -> Option<ScoringResult> {
    let model_path = model_manager::model_path(&config.models_dir, ModelId::ConfidenceScorer);

    match try_score_onnx(features, &model_path) {
        Ok(result) => Some(result),
        Err(e) => {
            log::warn!("ONNX scorer failed, falling back to heuristics: {e}");
            None
        }
    }
}

fn try_score_onnx(
    features: &ScoringFeatures,
    model_path: &std::path::Path,
) -> Result<ScoringResult> {
    let mut session = ort::session::Session::builder()
        .map_err(|e| anyhow::anyhow!("ort session builder: {e}"))?
        .with_intra_threads(1)
        .map_err(|e| anyhow::anyhow!("ort intra threads: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow::anyhow!("ort load model: {e}"))?;

    // Build input tensor: [1, 6]
    let input_data = vec![
        features.fat_chain_integrity,
        if features.clusters_contiguous { 1.0 } else { 0.0 },
        features.size_consistency,
        features.first_cluster_entropy,
        if features.has_valid_header { 1.0 } else { 0.0 },
        (features.file_size as f32 + 1.0).ln(),
    ];

    let input_tensor = ort::value::Value::from_array(
        ndarray::Array::from_shape_vec((1, 6), input_data)
            .map_err(|e| anyhow::anyhow!("tensor shape error: {e}"))?,
    )
    .map_err(|e| anyhow::anyhow!("ort from_array: {e}"))?;

    let outputs = session
        .run(ort::inputs![input_tensor])
        .map_err(|e| anyhow::anyhow!("ort run: {e}"))?;

    let (_shape, score_slice) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| anyhow::anyhow!("ort extract: {e}"))?;
    let score = score_slice
        .first()
        .copied()
        .unwrap_or(0.5);

    Ok(ScoringResult {
        score: score.clamp(0.0, 1.0),
    })
}
