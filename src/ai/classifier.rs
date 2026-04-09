// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

//! AI-based file type classifier.
//!
//! Uses a combination of extended magic byte analysis, Shannon entropy,
//! and byte frequency distribution to classify file content.
//! When an ONNX model is available, it delegates to the model.
//! Otherwise, falls back to a heuristic rule-based classifier.

use super::{ClassificationResult, FileFeatures};

/// Known file type profiles for the heuristic classifier.
struct TypeProfile {
    name: &'static str,
    extension: &'static str,
    /// Expected magic byte prefixes.
    magic_prefixes: &'static [&'static [u8]],
    /// Expected entropy range [min, max].
    entropy_range: (f32, f32),
}

const PROFILES: &[TypeProfile] = &[
    TypeProfile {
        name: "JPEG",
        extension: "jpg",
        magic_prefixes: &[&[0xFF, 0xD8, 0xFF]],
        entropy_range: (7.0, 8.0),
    },
    TypeProfile {
        name: "PNG",
        extension: "png",
        magic_prefixes: &[&[0x89, 0x50, 0x4E, 0x47]],
        entropy_range: (7.0, 8.0),
    },
    TypeProfile {
        name: "GIF",
        extension: "gif",
        magic_prefixes: &[b"GIF87a", b"GIF89a"],
        entropy_range: (5.0, 8.0),
    },
    TypeProfile {
        name: "PDF",
        extension: "pdf",
        magic_prefixes: &[b"%PDF"],
        entropy_range: (4.0, 8.0),
    },
    TypeProfile {
        name: "ZIP",
        extension: "zip",
        magic_prefixes: &[&[0x50, 0x4B, 0x03, 0x04]],
        entropy_range: (7.5, 8.0),
    },
    TypeProfile {
        name: "BMP",
        extension: "bmp",
        magic_prefixes: &[b"BM"],
        entropy_range: (1.0, 7.5),
    },
    TypeProfile {
        name: "MP3",
        extension: "mp3",
        magic_prefixes: &[&[0xFF, 0xFB], b"ID3"],
        entropy_range: (6.0, 8.0),
    },
    TypeProfile {
        name: "RAR",
        extension: "rar",
        magic_prefixes: &[&[0x52, 0x61, 0x72, 0x21]],
        entropy_range: (7.5, 8.0),
    },
    TypeProfile {
        name: "7Z",
        extension: "7z",
        magic_prefixes: &[&[0x37, 0x7A, 0xBC, 0xAF]],
        entropy_range: (7.5, 8.0),
    },
    TypeProfile {
        name: "TIFF",
        extension: "tiff",
        magic_prefixes: &[b"II*\x00", b"MM\x00*"],
        entropy_range: (4.0, 8.0),
    },
    TypeProfile {
        name: "EXE/DLL",
        extension: "exe",
        magic_prefixes: &[b"MZ"],
        entropy_range: (5.0, 8.0),
    },
    TypeProfile {
        name: "SQLite",
        extension: "db",
        magic_prefixes: &[b"SQLite format 3"],
        entropy_range: (3.0, 7.5),
    },
];

/// Classify file content using heuristic rules (no ML model required).
pub fn classify_heuristic(features: &FileFeatures) -> Option<ClassificationResult> {
    let mut best: Option<(f32, &TypeProfile)> = None;

    for profile in PROFILES {
        let mut score = 0.0f32;

        // Check magic bytes match
        let magic_match = profile.magic_prefixes.iter().any(|prefix| {
            features.magic_bytes.len() >= prefix.len()
                && &features.magic_bytes[..prefix.len()] == *prefix
        });

        if magic_match {
            score += 0.7; // Strong signal
        } else {
            continue; // Without magic bytes, skip this profile
        }

        // Check entropy is in expected range
        let (emin, emax) = profile.entropy_range;
        if features.entropy >= emin && features.entropy <= emax {
            score += 0.2;
        } else if features.entropy >= emin - 1.0 && features.entropy <= emax + 0.5 {
            score += 0.1; // Partial match
        }

        // Bonus for having a footer (more reliable carve)
        if features.has_footer {
            score += 0.1;
        }

        if let Some((best_score, _)) = &best {
            if score > *best_score {
                best = Some((score, profile));
            }
        } else {
            best = Some((score, profile));
        }
    }

    best.map(|(score, profile)| ClassificationResult {
        predicted_type: profile.name.to_string(),
        predicted_extension: profile.extension.to_string(),
        confidence: score.min(1.0),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai;

    #[test]
    fn classify_jpeg() {
        let data = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x00\x00\x01";
        let features = ai::extract_features(data, 50_000, true);
        let result = classify_heuristic(&features).expect("should classify JPEG");
        assert_eq!(result.predicted_type, "JPEG");
        assert!(result.confidence >= 0.7);
    }

    #[test]
    fn classify_png() {
        let data = b"\x89PNG\r\n\x1A\n\x00\x00\x00\rIHDR\x00\x00";
        let features = ai::extract_features(data, 30_000, true);
        let result = classify_heuristic(&features).expect("should classify PNG");
        assert_eq!(result.predicted_type, "PNG");
    }

    #[test]
    fn classify_pdf() {
        let data = b"%PDF-1.4 some pdf content here and more stuff";
        let features = ai::extract_features(data, 100_000, true);
        let result = classify_heuristic(&features).expect("should classify PDF");
        assert_eq!(result.predicted_type, "PDF");
    }

    #[test]
    fn unknown_file() {
        let data = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F";
        let features = ai::extract_features(data, 1000, false);
        let result = classify_heuristic(&features);
        assert!(result.is_none(), "unknown magic should not classify");
    }
}
