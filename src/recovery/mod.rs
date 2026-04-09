// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

pub mod carver;
pub mod dir_scan;
pub mod signatures;

use serde::Serialize;

/// Confidence level for a recovered file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Confidence {
    /// FAT chain was intact and could be fully followed.
    High,
    /// Start cluster valid, assumed contiguous allocation (FAT chain broken).
    Medium,
    /// Signature-based carve from unallocated space.
    Carved,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::High => write!(f, "HIGH"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::Carved => write!(f, "CARVED"),
        }
    }
}

/// A file recovered via directory-entry scanning.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveredFile {
    pub name: String,
    pub dir_path: String,
    pub size: u32,
    pub start_cluster: u32,
    pub clusters: Vec<u32>,
    pub confidence: Confidence,
    /// AI-predicted recovery confidence score (0.0–1.0). `None` when AI is disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_score: Option<f32>,
}

/// A file recovered via signature-based carving.
#[derive(Debug, Clone, Serialize)]
pub struct CarvedFile {
    pub signature_name: String,
    pub extension: String,
    pub offset: u64,
    pub size: u64,
    pub clusters: Vec<u32>,
    /// AI-predicted file type (may differ from signature_name). `None` when AI is disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_type: Option<String>,
    /// AI classification confidence (0.0–1.0). `None` when AI is disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_confidence: Option<f32>,
}

impl RecoveredFile {
    pub fn full_path(&self) -> String {
        if self.dir_path.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.dir_path, self.name)
        }
    }
}
