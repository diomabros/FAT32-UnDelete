// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

//! Cloud AI backend — sends only feature vectors (never raw file bytes) to remote APIs.
//!
//! This module is only compiled when the `ai-cloud` feature is enabled.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::config::AiConfig;
use super::scorer::ScoringFeatures;
use super::{ClassificationResult, FileFeatures, ScoringResult};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ClassifyRequest {
    /// Shannon entropy (0.0–8.0).
    entropy: f32,
    /// Normalized byte frequency distribution (256 values).
    byte_distribution: Vec<f32>,
    /// File size in bytes.
    file_size: u64,
    /// First 16 bytes as hex string (the only raw bytes sent).
    magic_hex: String,
    /// Whether a footer was detected.
    has_footer: bool,
}

#[derive(Deserialize)]
struct ClassifyResponse {
    predicted_type: String,
    predicted_extension: String,
    confidence: f32,
}

#[derive(Serialize)]
struct ScoreRequest {
    fat_chain_integrity: f32,
    clusters_contiguous: bool,
    size_consistency: f32,
    first_cluster_entropy: f32,
    has_valid_header: bool,
    file_size: u64,
}

#[derive(Deserialize)]
struct ScoreResponse {
    score: f32,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Classify a file by sending its feature vector to the cloud API.
///
/// **Privacy**: only the feature vector is sent — never the raw file content.
/// The only bytes sent are the first 16 magic bytes (as hex), which are
/// typically part of a public file format specification.
pub fn classify(features: &FileFeatures, config: &AiConfig) -> Result<ClassificationResult> {
    let request = ClassifyRequest {
        entropy: features.entropy,
        byte_distribution: features.byte_distribution.to_vec(),
        file_size: features.file_size,
        magic_hex: hex_encode(&features.magic_bytes),
        has_footer: features.has_footer,
    };

    let url = format!("{}/fat32-undelete/classify", config.cloud_endpoint.trim_end_matches('/'));

    let response: ClassifyResponse = reqwest::blocking::Client::new()
        .post(&url)
        .bearer_auth(&config.cloud_api_key)
        .json(&request)
        .send()
        .with_context(|| format!("cloud classify request to {url} failed"))?
        .error_for_status()
        .with_context(|| "cloud classify returned error status")?
        .json()
        .with_context(|| "failed to parse cloud classify response")?;

    Ok(ClassificationResult {
        predicted_type: response.predicted_type,
        predicted_extension: response.predicted_extension,
        confidence: response.confidence.clamp(0.0, 1.0),
    })
}

/// Score recovery confidence by sending the scoring feature vector to the cloud API.
///
/// **Privacy**: only numeric features are sent — no file content.
pub fn score(features: &ScoringFeatures, config: &AiConfig) -> Result<ScoringResult> {
    let request = ScoreRequest {
        fat_chain_integrity: features.fat_chain_integrity,
        clusters_contiguous: features.clusters_contiguous,
        size_consistency: features.size_consistency,
        first_cluster_entropy: features.first_cluster_entropy,
        has_valid_header: features.has_valid_header,
        file_size: features.file_size,
    };

    let url = format!("{}/fat32-undelete/score", config.cloud_endpoint.trim_end_matches('/'));

    let response: ScoreResponse = reqwest::blocking::Client::new()
        .post(&url)
        .bearer_auth(&config.cloud_api_key)
        .json(&request)
        .send()
        .with_context(|| format!("cloud score request to {url} failed"))?
        .error_for_status()
        .with_context(|| "cloud score returned error status")?
        .json()
        .with_context(|| "failed to parse cloud score response")?;

    Ok(ScoringResult {
        score: response.score.clamp(0.0, 1.0),
    })
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}
