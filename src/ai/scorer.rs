// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

//! Smart confidence scorer for recovered files.
//!
//! Computes a fine-grained 0.0–1.0 recovery confidence score based on
//! multiple signals: FAT chain integrity, cluster contiguity, file header
//! validation, and content entropy analysis.

use super::ScoringResult;

/// Input features for confidence scoring.
#[derive(Debug, Clone)]
pub struct ScoringFeatures {
    /// Fraction of requested clusters that were found in the FAT chain (0.0–1.0).
    /// 1.0 means the full chain is intact. 0.0 means completely broken.
    pub fat_chain_integrity: f32,
    /// Whether clusters are contiguous on disk (no fragmentation).
    pub clusters_contiguous: bool,
    /// Ratio: (cluster count × cluster size) / declared file size.
    /// Close to 1.0 is ideal. Much larger means possible over-allocation.
    pub size_consistency: f32,
    /// Shannon entropy of the first cluster content (0.0–8.0).
    pub first_cluster_entropy: f32,
    /// Whether the first cluster starts with a recognized file magic.
    pub has_valid_header: bool,
    /// File size in bytes.
    pub file_size: u64,
}

/// Compute recovery confidence score using heuristic rules.
///
/// Returns a score between 0.0 and 1.0 where:
/// - `>= 0.8` → high confidence (likely recoverable intact)
/// - `0.5–0.8` → medium confidence (partially recoverable)
/// - `< 0.5` → low confidence (may be corrupted)
pub fn score_heuristic(features: &ScoringFeatures) -> ScoringResult {
    let mut score = 0.0f32;

    // FAT chain integrity is the strongest signal (weight: 0.35)
    score += features.fat_chain_integrity * 0.35;

    // Contiguous clusters reduce fragmentation risk (weight: 0.15)
    if features.clusters_contiguous {
        score += 0.15;
    }

    // Size consistency: how well cluster allocation matches declared size (weight: 0.20)
    // Perfect = 1.0, over/under-allocated reduces score
    let size_score = if features.size_consistency > 0.0 {
        let deviation = (features.size_consistency - 1.0).abs();
        (1.0 - deviation).max(0.0)
    } else {
        0.0
    };
    score += size_score * 0.20;

    // Valid file header (weight: 0.15)
    if features.has_valid_header {
        score += 0.15;
    }

    // Entropy analysis (weight: 0.15)
    // Very low entropy (< 0.5) on non-empty files is suspicious (zeroed out)
    // Very specific entropy doesn't help much, but non-zero is a positive signal
    let entropy_score = if features.file_size == 0 {
        0.0
    } else if features.first_cluster_entropy < 0.5 {
        0.3 // Suspicious — possibly overwritten with zeros
    } else {
        1.0 // Reasonable content present
    };
    score += entropy_score * 0.15;

    ScoringResult {
        score: score.clamp(0.0, 1.0),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_confidence_intact_chain() {
        let features = ScoringFeatures {
            fat_chain_integrity: 1.0,
            clusters_contiguous: true,
            size_consistency: 1.0,
            first_cluster_entropy: 7.5,
            has_valid_header: true,
            file_size: 50_000,
        };
        let result = score_heuristic(&features);
        assert!(
            result.score >= 0.8,
            "intact chain should score >= 0.8, got {}",
            result.score
        );
    }

    #[test]
    fn medium_confidence_broken_chain() {
        let features = ScoringFeatures {
            fat_chain_integrity: 0.0,
            clusters_contiguous: true,
            size_consistency: 1.0,
            first_cluster_entropy: 6.0,
            has_valid_header: true,
            file_size: 50_000,
        };
        let result = score_heuristic(&features);
        assert!(
            result.score >= 0.3 && result.score < 0.8,
            "broken chain with valid header should be medium, got {}",
            result.score
        );
    }

    #[test]
    fn low_confidence_zeroed_content() {
        let features = ScoringFeatures {
            fat_chain_integrity: 0.0,
            clusters_contiguous: false,
            size_consistency: 2.0, // Over-allocated
            first_cluster_entropy: 0.1, // Nearly zeroed
            has_valid_header: false,
            file_size: 50_000,
        };
        let result = score_heuristic(&features);
        assert!(
            result.score < 0.5,
            "zeroed content should score < 0.5, got {}",
            result.score
        );
    }
}
