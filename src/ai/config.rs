// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use std::path::PathBuf;

/// Which AI backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiBackendChoice {
    /// Disable AI entirely.
    Off,
    /// Local ONNX model inference (requires `ai-local` feature).
    Local,
    /// Cloud API inference (requires `ai-cloud` feature).
    Cloud,
}

impl Default for AiBackendChoice {
    fn default() -> Self {
        Self::Off
    }
}

impl std::fmt::Display for AiBackendChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Local => write!(f, "Local"),
            Self::Cloud => write!(f, "Cloud"),
        }
    }
}

/// Runtime configuration for AI features.
#[derive(Debug, Clone)]
pub struct AiConfig {
    /// Enabled backend.
    pub backend: AiBackendChoice,
    /// Directory where ONNX models are cached after download.
    pub models_dir: PathBuf,
    /// Cloud API endpoint (e.g. `https://api.openai.com/v1`).
    pub cloud_endpoint: String,
    /// Cloud API key (stored in memory only — never serialized).
    pub cloud_api_key: String,
    /// Whether the user has accepted the cloud privacy disclaimer.
    pub cloud_disclaimer_accepted: bool,
    /// Minimum AI confidence score to display (0.0–1.0).
    pub min_confidence_display: f32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            backend: AiBackendChoice::Off,
            models_dir: default_models_dir(),
            cloud_endpoint: "https://api.openai.com/v1".into(),
            cloud_api_key: String::new(),
            cloud_disclaimer_accepted: false,
            min_confidence_display: 0.0,
        }
    }
}

/// Default model cache directory: `~/.fat32-undelete/models/`
fn default_models_dir() -> PathBuf {
    dirs_fallback().join("models")
}

fn dirs_fallback() -> PathBuf {
    if let Some(home) = home_dir() {
        home.join(".fat32-undelete")
    } else {
        PathBuf::from(".fat32-undelete")
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
