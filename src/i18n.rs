// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

// ---------------------------------------------------------------------------
// Translation catalog — loaded from `locales/*.json` at runtime
// ---------------------------------------------------------------------------

/// A single locale: a flat key → value map loaded from a JSON file.
#[derive(Debug, Clone)]
pub struct Locale {
    /// Display name of the language (the `lang_name` key).
    pub name: String,
    /// Short code derived from the filename, e.g. "en", "it".
    pub code: String,
    map: HashMap<String, String>,
}

impl Locale {
    fn load(path: &Path) -> anyhow::Result<Self> {
        let code = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("en")
            .to_string();
        let text = std::fs::read_to_string(path)?;
        let map: HashMap<String, String> = serde_json::from_str(&text)?;
        let name = map
            .get("lang_name")
            .cloned()
            .unwrap_or_else(|| code.clone());
        Ok(Self { name, code, map })
    }

    /// Look up a key, returning the value or the key itself as fallback.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.map
            .get(key)
            .map(|s| s.as_str())
            .unwrap_or(key)
    }

    /// Look up a templated key and replace `{placeholder}` occurrences.
    pub fn fmt(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut s = self.get(key).to_string();
        for &(name, value) in args {
            s = s.replace(&format!("{{{name}}}"), value);
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

struct I18nState {
    locales: Vec<Locale>,
    current: usize, // index into `locales`
}

static STATE: OnceLock<RwLock<I18nState>> = OnceLock::new();

/// Embedded English fallback, used if no locales/ directory is found.
const FALLBACK_EN: &str = include_str!("../locales/en.json");

fn init_state() -> RwLock<I18nState> {
    let mut locales = Vec::new();
    let mut seen_codes = std::collections::HashSet::new();

    for locales_dir in locales_dirs() {
        if !locales_dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&locales_dir) {
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
                .collect();
            paths.sort();
            for path in paths {
                match Locale::load(&path) {
                    Ok(loc) => {
                        if seen_codes.insert(loc.code.clone()) {
                            locales.push(loc);
                        }
                    }
                    Err(e) => eprintln!("i18n: failed to load {}: {e}", path.display()),
                }
            }
        }
    }

    // Ensure we always have at least the English locale.
    if locales.is_empty() {
        let map: HashMap<String, String> =
            serde_json::from_str(FALLBACK_EN).expect("embedded en.json is valid");
        let name = map.get("lang_name").cloned().unwrap_or("English".into());
        locales.push(Locale {
            name,
            code: "en".into(),
            map,
        });
    }

    // Pick the default language from the OS locale.
    let sys_code = detect_system_locale_code();
    let current = locales
        .iter()
        .position(|l| l.code == sys_code)
        .unwrap_or(0);

    RwLock::new(I18nState { locales, current })
}

fn state() -> &'static RwLock<I18nState> {
    STATE.get_or_init(init_state)
}

/// Returns candidate `locales/` directories to search, in priority order.
fn locales_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 1) Next to the executable (for installed / release builds)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("locales"));
        }
    }

    // 2) Current working directory (useful during development)
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_locales = cwd.join("locales");
        if !dirs.iter().any(|d| d == &cwd_locales) {
            dirs.push(cwd_locales);
        }
    }

    // 3) Bare fallback
    dirs.push(PathBuf::from("locales"));
    dirs
}

fn detect_system_locale_code() -> String {
    sys_locale::get_locale()
        .map(|l| {
            // "it-IT" → "it", "en-US" → "en"
            l.split(['-', '_']).next().unwrap_or("en").to_lowercase()
        })
        .unwrap_or_else(|| "en".into())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns the list of available locale codes and display names.
pub fn available_languages() -> Vec<(String, String)> {
    let st = state().read().unwrap();
    st.locales
        .iter()
        .map(|l| (l.code.clone(), l.name.clone()))
        .collect()
}

/// Returns the code of the current language (e.g. "en", "it").
pub fn current_language_code() -> String {
    let st = state().read().unwrap();
    st.locales[st.current].code.clone()
}

/// Switch to a language by code (e.g. "it"). No-op if not found.
pub fn set_language(code: &str) {
    let mut st = state().write().unwrap();
    if let Some(idx) = st.locales.iter().position(|l| l.code == code) {
        st.current = idx;
    }
}

/// Get a simple translated label by key.
pub fn tr(key: &str) -> String {
    let st = state().read().unwrap();
    st.locales[st.current].get(key).to_string()
}

/// Get a translated template and substitute `{placeholder}` values.
pub fn fmt(key: &str, args: &[(&str, &str)]) -> String {
    let st = state().read().unwrap();
    st.locales[st.current].fmt(key, args)
}

// ---------------------------------------------------------------------------
// Convenience wrappers (keep call-sites concise)
// ---------------------------------------------------------------------------

pub fn opening_source(source: &str) -> String {
    fmt("opening_source", &[("source", source)])
}

pub fn fat_divergent_warning(count: usize) -> String {
    fmt("fat_divergent_warning", &[("count", &count.to_string())])
}

pub fn scan_complete(recovered: usize, carved: usize) -> String {
    fmt(
        "scan_complete",
        &[
            ("recovered", &recovered.to_string()),
            ("carved", &carved.to_string()),
        ],
    )
}

pub fn extracting_n_files(total: usize) -> String {
    fmt("extracting_n_files", &[("total", &total.to_string())])
}

pub fn extraction_complete(size: &str) -> String {
    fmt("extraction_complete", &[("size", size)])
}

pub fn files_written_to(dir: &str) -> String {
    fmt("files_written_to", &[("dir", dir)])
}

pub fn cli_files_written_to(dir: &str) -> String {
    fmt("cli_files_written_to", &[("dir", dir)])
}

pub fn dir_scan_carved_summary(recovered: usize, carved: usize) -> String {
    fmt(
        "dir_scan_carved_summary",
        &[
            ("recovered", &recovered.to_string()),
            ("carved", &carved.to_string()),
        ],
    )
}

pub fn n_selected(count: usize) -> String {
    fmt("n_selected", &[("count", &count.to_string())])
}

pub fn error_msg(err: &str) -> String {
    fmt("error_msg", &[("err", err)])
}

pub fn dir_scan_files_count(count: usize) -> String {
    fmt("dir_scan_files_count", &[("count", &count.to_string())])
}

pub fn carved_files_count(count: usize) -> String {
    fmt("carved_files_count", &[("count", &count.to_string())])
}

pub fn total_summary(files: usize, size: &str) -> String {
    fmt(
        "total_summary",
        &[("files", &files.to_string()), ("size", size)],
    )
}

pub fn cli_volume_info(label: &str, sector: u16, cluster: u32, data_clusters: u32) -> String {
    fmt(
        "cli_volume_info",
        &[
            ("label", label),
            ("sector", &sector.to_string()),
            ("cluster", &cluster.to_string()),
            ("data_clusters", &data_clusters.to_string()),
        ],
    )
}
