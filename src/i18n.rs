// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use std::sync::atomic::{AtomicU8, Ordering};

// ---------------------------------------------------------------------------
// Language enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Language {
    English = 0,
    Italian = 1,
}

impl Language {
    pub fn label(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Italian => "Italiano",
        }
    }

    pub fn all() -> &'static [Language] {
        &[Language::English, Language::Italian]
    }
}

// ---------------------------------------------------------------------------
// Global language state
// ---------------------------------------------------------------------------

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_language(lang: Language) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn language() -> Language {
    match LANG.load(Ordering::Relaxed) {
        1 => Language::Italian,
        _ => Language::English,
    }
}

/// Detect system language from OS locale.
pub fn detect_system_language() -> Language {
    if let Some(locale) = sys_locale::get_locale() {
        if locale.starts_with("it") {
            return Language::Italian;
        }
    }
    Language::English
}

// ---------------------------------------------------------------------------
// Static label translations
// ---------------------------------------------------------------------------

pub struct Labels {
    // GUI top bar
    pub app_title: &'static str,
    pub recovery_tool: &'static str,
    pub log_label: &'static str,

    // Settings panel
    pub settings: &'static str,
    pub source: &'static str,
    pub file_btn: &'static str,
    pub or_select_drive: &'static str,
    pub refresh_drive_list: &'static str,
    pub output_directory: &'static str,
    pub browse_btn: &'static str,
    pub select_disk_image: &'static str,
    pub select_output_dir: &'static str,
    pub partition_offset: &'static str,
    pub recovery_mode: &'static str,
    pub mode_all: &'static str,
    pub mode_scan: &'static str,
    pub mode_carve: &'static str,
    pub file_types_label: &'static str,
    pub min_size_label: &'static str,
    pub max_size_label: &'static str,
    pub scan_btn: &'static str,
    pub recover_btn: &'static str,
    pub scanning: &'static str,
    pub extracting: &'static str,

    // Volume info
    pub volume_info: &'static str,
    pub label_field: &'static str,
    pub sector_size: &'static str,
    pub cluster_size_label: &'static str,
    pub data_clusters: &'static str,
    pub fat_divergences: &'static str,

    // Results panel
    pub no_results_hint: &'static str,
    pub results: &'static str,
    pub select_all: &'static str,
    pub deselect_all: &'static str,
    pub dir_scan_recovered: &'static str,
    pub carved_files: &'static str,

    // Column headers
    pub col_name: &'static str,
    pub col_path: &'static str,
    pub col_size: &'static str,
    pub col_cluster: &'static str,
    pub col_confidence: &'static str,
    pub col_type: &'static str,
    pub col_extension: &'static str,
    pub col_offset: &'static str,

    // Misc
    pub none_label: &'static str,
    pub language_label: &'static str,

    // CLI / output
    pub recovery_summary: &'static str,
    pub dry_run_note: &'static str,

    // Background tasks
    pub parsing_boot_sector: &'static str,
    pub loading_fat_tables: &'static str,
    pub scanning_directories: &'static str,
    pub carving_clusters: &'static str,
}

static EN: Labels = Labels {
    app_title: "FAT32 Undelete",
    recovery_tool: "Recovery Tool",
    log_label: "Log",

    settings: "Settings",
    source: "Source",
    file_btn: "File…",
    or_select_drive: "or select a drive:",
    refresh_drive_list: "Refresh drive list",
    output_directory: "Output directory",
    browse_btn: "Browse…",
    select_disk_image: "Select disk image",
    select_output_dir: "Select output directory",
    partition_offset: "Partition offset (bytes)",
    recovery_mode: "Recovery mode",
    mode_all: "All",
    mode_scan: "Scan",
    mode_carve: "Carve",
    file_types_label: "File types (comma-separated)",
    min_size_label: "Min size (bytes)",
    max_size_label: "Max size (bytes)",
    scan_btn: "🔍 Scan",
    recover_btn: "💾 Recover",
    scanning: "Scanning…",
    extracting: "Extracting…",

    volume_info: "Volume Info",
    label_field: "Label:",
    sector_size: "Sector size:",
    cluster_size_label: "Cluster size:",
    data_clusters: "Data clusters:",
    fat_divergences: "FAT divergences:",

    no_results_hint: "No results yet.\nSelect a source and click Scan.",
    results: "Results",
    select_all: "Select all",
    deselect_all: "Deselect all",
    dir_scan_recovered: "Directory-scan recovered files",
    carved_files: "Carved files",

    col_name: "Name",
    col_path: "Path",
    col_size: "Size",
    col_cluster: "Cluster",
    col_confidence: "Confidence",
    col_type: "Type",
    col_extension: "Extension",
    col_offset: "Offset",

    none_label: "(none)",
    language_label: "Language",

    recovery_summary: "=== Recovery Summary ===",
    dry_run_note: "(dry-run: no files written)",

    parsing_boot_sector: "Parsing FAT32 boot sector...",
    loading_fat_tables: "Loading FAT tables...",
    scanning_directories: "Scanning directories for deleted entries...",
    carving_clusters: "Carving free clusters for file signatures...",
};

static IT: Labels = Labels {
    app_title: "FAT32 Undelete",
    recovery_tool: "Strumento di Recupero",
    log_label: "Registro",

    settings: "Impostazioni",
    source: "Sorgente",
    file_btn: "File…",
    or_select_drive: "oppure seleziona un'unità:",
    refresh_drive_list: "Aggiorna elenco unità",
    output_directory: "Cartella di output",
    browse_btn: "Sfoglia…",
    select_disk_image: "Seleziona immagine disco",
    select_output_dir: "Seleziona cartella di output",
    partition_offset: "Offset partizione (byte)",
    recovery_mode: "Modalità di recupero",
    mode_all: "Tutto",
    mode_scan: "Scansione",
    mode_carve: "Carving",
    file_types_label: "Tipi di file (separati da virgola)",
    min_size_label: "Dim. minima (byte)",
    max_size_label: "Dim. massima (byte)",
    scan_btn: "🔍 Scansiona",
    recover_btn: "💾 Recupera",
    scanning: "Scansione in corso…",
    extracting: "Estrazione in corso…",

    volume_info: "Info Volume",
    label_field: "Etichetta:",
    sector_size: "Dim. settore:",
    cluster_size_label: "Dim. cluster:",
    data_clusters: "Cluster dati:",
    fat_divergences: "Divergenze FAT:",

    no_results_hint: "Nessun risultato.\nSeleziona una sorgente e clicca Scansiona.",
    results: "Risultati",
    select_all: "Seleziona tutto",
    deselect_all: "Deseleziona tutto",
    dir_scan_recovered: "File recuperati da scansione directory",
    carved_files: "File trovati con carving",

    col_name: "Nome",
    col_path: "Percorso",
    col_size: "Dimensione",
    col_cluster: "Cluster",
    col_confidence: "Affidabilità",
    col_type: "Tipo",
    col_extension: "Estensione",
    col_offset: "Offset",

    none_label: "(nessuno)",
    language_label: "Lingua",

    recovery_summary: "=== Riepilogo Recupero ===",
    dry_run_note: "(simulazione: nessun file scritto)",

    parsing_boot_sector: "Analisi settore di boot FAT32...",
    loading_fat_tables: "Caricamento tabelle FAT...",
    scanning_directories: "Scansione directory per voci eliminate...",
    carving_clusters: "Carving cluster liberi per firme file...",
};

/// Get the current translation labels.
pub fn tr() -> &'static Labels {
    match language() {
        Language::Italian => &IT,
        Language::English => &EN,
    }
}

// ---------------------------------------------------------------------------
// Parameterized message functions
// ---------------------------------------------------------------------------

pub fn opening_source(source: &str) -> String {
    match language() {
        Language::Italian => format!("Apertura sorgente: {source}"),
        Language::English => format!("Opening source: {source}"),
    }
}

pub fn fat_divergent_warning(count: usize) -> String {
    match language() {
        Language::Italian => {
            format!("ATTENZIONE: {count} cluster diversi tra FAT1 e FAT2")
        }
        Language::English => {
            format!("WARNING: {count} clusters differ between FAT1 and FAT2")
        }
    }
}

pub fn scan_complete(recovered: usize, carved: usize) -> String {
    match language() {
        Language::Italian => format!(
            "Scansione completata: {recovered} file da directory, {carved} file da carving"
        ),
        Language::English => format!(
            "Scan complete: {recovered} dir-scan file(s), {carved} carved file(s)"
        ),
    }
}

pub fn extracting_n_files(total: usize) -> String {
    match language() {
        Language::Italian => format!("Estrazione di {total} file in corso..."),
        Language::English => format!("Extracting {total} file(s)..."),
    }
}

pub fn extraction_complete(size: &str) -> String {
    match language() {
        Language::Italian => format!("Estrazione completata: {size} recuperati"),
        Language::English => format!("Extraction complete: {size} recovered"),
    }
}

pub fn files_written_to(dir: &str) -> String {
    match language() {
        Language::Italian => format!("✓ File scritti in: {dir}"),
        Language::English => format!("✓ Files written to: {dir}"),
    }
}

pub fn cli_files_written_to(dir: &str) -> String {
    match language() {
        Language::Italian => format!("\nFile scritti in: {dir}"),
        Language::English => format!("\nFiles written to: {dir}"),
    }
}

pub fn dir_scan_carved_summary(recovered: usize, carved: usize) -> String {
    match language() {
        Language::Italian => format!("{recovered} da directory, {carved} da carving"),
        Language::English => format!("{recovered} dir-scan, {carved} carved"),
    }
}

pub fn n_selected(count: usize) -> String {
    match language() {
        Language::Italian => format!("{count} selezionati"),
        Language::English => format!("{count} selected"),
    }
}

pub fn error_msg(err: &str) -> String {
    format!("ERROR: {err}")
}

pub fn dir_scan_files_count(count: usize) -> String {
    match language() {
        Language::Italian => {
            format!("File recuperati da scansione directory ({count}):")
        }
        Language::English => {
            format!("Directory-scan recovered files ({count}):")
        }
    }
}

pub fn carved_files_count(count: usize) -> String {
    match language() {
        Language::Italian => format!("File da carving ({count}):"),
        Language::English => format!("Carved files ({count}):"),
    }
}

pub fn total_summary(files: usize, size: &str) -> String {
    match language() {
        Language::Italian => format!("Totale: {files} file, {size} recuperati"),
        Language::English => format!("Total: {files} file(s), {size} recovered"),
    }
}

pub fn cli_volume_info(
    label: &str,
    sector: u16,
    cluster: u32,
    data_clusters: u32,
) -> String {
    match language() {
        Language::Italian => format!(
            "  Volume: {label} | Settore: {sector} | Cluster: {cluster} | Cluster dati: {data_clusters}"
        ),
        Language::English => format!(
            "  Volume: {label} | Sector: {sector} | Cluster: {cluster} | Data clusters: {data_clusters}"
        ),
    }
}
