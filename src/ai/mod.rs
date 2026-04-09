// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

//! AI module — optional intelligence layer for FAT32 recovery.
//!
//! All AI functionality is gated behind Cargo feature flags (`ai-local`, `ai-cloud`)
//! and can be disabled at runtime via [`config::AiConfig`].

pub mod config;
pub mod classifier;
pub mod scorer;
pub mod model_manager;

#[cfg(feature = "ai-local")]
pub mod local_backend;

#[cfg(feature = "ai-cloud")]
pub mod cloud_backend;

#[allow(unused_imports)]
use anyhow::Result;
use config::{AiBackendChoice, AiConfig};

// ---------------------------------------------------------------------------
// Feature vector — shared representation sent to both local and cloud backends
// ---------------------------------------------------------------------------

/// Extracted features from raw file bytes. This is what gets sent to cloud APIs
/// (never raw bytes) and fed into local ONNX models.
#[derive(Debug, Clone)]
pub struct FileFeatures {
    /// Shannon entropy of the sample (0.0–8.0).
    pub entropy: f32,
    /// Byte frequency distribution (256 buckets, normalized to 0.0–1.0).
    pub byte_distribution: [f32; 256],
    /// File size in bytes.
    pub file_size: u64,
    /// First 16 bytes of the file (magic bytes).
    pub magic_bytes: Vec<u8>,
    /// Whether a known footer was detected.
    pub has_footer: bool,
}

/// Result from the AI file classifier.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// Predicted file type name (e.g. "JPEG", "PDF").
    pub predicted_type: String,
    /// Predicted file extension.
    pub predicted_extension: String,
    /// Confidence score 0.0–1.0.
    pub confidence: f32,
}

/// Result from the AI confidence scorer.
#[derive(Debug, Clone)]
pub struct ScoringResult {
    /// Numeric recovery confidence score 0.0–1.0.
    pub score: f32,
}

// ---------------------------------------------------------------------------
// Feature extraction (pure computation — no ML dependency)
// ---------------------------------------------------------------------------

/// Compute Shannon entropy of a byte buffer.
pub fn shannon_entropy(data: &[u8]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let len = data.len() as f64;
    let mut entropy = 0.0f64;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy as f32
}

/// Compute normalized byte frequency distribution.
pub fn byte_distribution(data: &[u8]) -> [f32; 256] {
    let mut dist = [0f32; 256];
    if data.is_empty() {
        return dist;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let len = data.len() as f32;
    for (i, &c) in counts.iter().enumerate() {
        dist[i] = c as f32 / len;
    }
    dist
}

/// Extract features from a raw data buffer.
pub fn extract_features(data: &[u8], file_size: u64, has_footer: bool) -> FileFeatures {
    let magic_len = data.len().min(16);
    FileFeatures {
        entropy: shannon_entropy(data),
        byte_distribution: byte_distribution(data),
        file_size,
        magic_bytes: data[..magic_len].to_vec(),
        has_footer,
    }
}

// ---------------------------------------------------------------------------
// AI Engine — unified entry point
// ---------------------------------------------------------------------------

/// Unified AI engine that delegates to the configured backend.
pub struct AiEngine {
    config: AiConfig,
}

impl AiEngine {
    pub fn new(config: AiConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &AiConfig {
        &self.config
    }

    pub fn is_enabled(&self) -> bool {
        self.config.backend != AiBackendChoice::Off
    }

    /// Classify a file based on its content features.
    #[allow(unused_variables)]
    pub fn classify(&self, features: &FileFeatures) -> Option<ClassificationResult> {
        if !self.is_enabled() {
            return None;
        }
        match self.config.backend {
            AiBackendChoice::Off => None,
            AiBackendChoice::Local => {
                #[cfg(feature = "ai-local")]
                {
                    local_backend::classify(features, &self.config).ok()
                }
                #[cfg(not(feature = "ai-local"))]
                {
                    log::warn!("AI local backend requested but ai-local feature not compiled");
                    None
                }
            }
            AiBackendChoice::Cloud => {
                if !self.config.cloud_disclaimer_accepted {
                    log::warn!("Cloud AI backend requires privacy disclaimer acceptance");
                    return None;
                }
                #[cfg(feature = "ai-cloud")]
                {
                    cloud_backend::classify(features, &self.config).ok()
                }
                #[cfg(not(feature = "ai-cloud"))]
                {
                    log::warn!("AI cloud backend requested but ai-cloud feature not compiled");
                    None
                }
            }
        }
    }

    /// Score recovery confidence for a file based on FAT chain and content features.
    #[allow(unused_variables)]
    pub fn score(&self, features: &scorer::ScoringFeatures) -> Option<ScoringResult> {
        if !self.is_enabled() {
            return None;
        }
        match self.config.backend {
            AiBackendChoice::Off => None,
            AiBackendChoice::Local => {
                #[cfg(feature = "ai-local")]
                {
                    local_backend::score(features, &self.config).ok()
                }
                #[cfg(not(feature = "ai-local"))]
                {
                    log::warn!("AI local backend requested but ai-local feature not compiled");
                    None
                }
            }
            AiBackendChoice::Cloud => {
                if !self.config.cloud_disclaimer_accepted {
                    return None;
                }
                #[cfg(feature = "ai-cloud")]
                {
                    cloud_backend::score(features, &self.config).ok()
                }
                #[cfg(not(feature = "ai-cloud"))]
                {
                    log::warn!("AI cloud backend requested but ai-cloud feature not compiled");
                    None
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_uniform() {
        // All zeros should have entropy 0
        let data = vec![0u8; 1024];
        let e = shannon_entropy(&data);
        assert!(e < 0.01, "uniform data should have near-zero entropy, got {e}");
    }

    #[test]
    fn entropy_random_like() {
        // Data with all 256 byte values equally represented
        let mut data = Vec::with_capacity(256 * 4);
        for _ in 0..4 {
            for b in 0u8..=255 {
                data.push(b);
            }
        }
        let e = shannon_entropy(&data);
        assert!(e > 7.9, "uniform-random data should have ~8.0 entropy, got {e}");
    }

    #[test]
    fn byte_dist_sums_to_one() {
        let data = b"hello world, this is a test of the byte distribution function!!";
        let dist = byte_distribution(data);
        let sum: f32 = dist.iter().sum();
        assert!((sum - 1.0).abs() < 0.01, "distribution should sum to ~1.0, got {sum}");
    }

    #[test]
    fn extract_features_smoke() {
        let data = b"\xFF\xD8\xFF\xE0some jpeg-like content here";
        let feats = extract_features(data, 12345, true);
        assert_eq!(feats.magic_bytes, &data[..16]);
        assert_eq!(feats.file_size, 12345);
        assert!(feats.has_footer);
    }
}
