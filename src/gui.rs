// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::fat32;
use crate::i18n;
use crate::io;
use crate::output;
use crate::recovery;
use crate::recovery::{CarvedFile, RecoveredFile};

// ---------------------------------------------------------------------------
// Messages from background thread → GUI
// ---------------------------------------------------------------------------

enum BgMessage {
    Log(String),
    VolumeInfo {
        label: String,
        bytes_per_sector: u16,
        cluster_size: u32,
        total_data_clusters: u32,
        fat_divergent: usize,
    },
    ScanResults {
        recovered: Vec<RecoveredFile>,
        carved: Vec<CarvedFile>,
    },
    ExtractProgress {
        current: usize,
        total: usize,
    },
    ExtractDone {
        recovered_bytes: u64,
        carved_bytes: u64,
    },
    Error(String),
    ScanFinished,
    ExtractFinished,
}

// ---------------------------------------------------------------------------
// Recovery mode (mirrors CLI enum)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Scan,
    Carve,
    All,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scan => write!(f, "Scan"),
            Self::Carve => write!(f, "Carve"),
            Self::All => write!(f, "All"),
        }
    }
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

struct App {
    // --- Input fields ---
    source: String,
    output_dir: String,
    offset: String,
    mode: Mode,
    file_types: String,
    min_size: String,
    max_size: String,

    // --- Runtime state ---
    scanning: bool,
    extracting: bool,

    // --- Results ---
    volume_label: String,
    bytes_per_sector: u16,
    cluster_size: u32,
    total_data_clusters: u32,
    fat_divergent: usize,
    has_volume_info: bool,

    recovered: Vec<RecoveredFile>,
    carved: Vec<CarvedFile>,
    recovered_selected: Vec<bool>,
    carved_selected: Vec<bool>,

    // --- Extraction results ---
    last_recovered_bytes: u64,
    last_carved_bytes: u64,
    extract_done: bool,

    // --- Progress ---
    progress_step: String,
    progress_current: usize,
    progress_total: usize,

    // --- Logs ---
    logs: Vec<String>,

    // --- Channel ---
    rx: Option<mpsc::Receiver<BgMessage>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            source: String::new(),
            output_dir: "recovered".into(),
            offset: "0".into(),
            mode: Mode::All,
            file_types: String::new(),
            min_size: String::new(),
            max_size: String::new(),

            scanning: false,
            extracting: false,

            volume_label: String::new(),
            bytes_per_sector: 0,
            cluster_size: 0,
            total_data_clusters: 0,
            fat_divergent: 0,
            has_volume_info: false,

            recovered: Vec::new(),
            carved: Vec::new(),
            recovered_selected: Vec::new(),
            carved_selected: Vec::new(),

            last_recovered_bytes: 0,
            last_carved_bytes: 0,
            extract_done: false,

            progress_step: String::new(),
            progress_current: 0,
            progress_total: 0,

            logs: Vec::new(),
            rx: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Background task: scan
// ---------------------------------------------------------------------------

fn spawn_scan(
    tx: mpsc::Sender<BgMessage>,
    source: String,
    offset: u64,
    mode: Mode,
    file_types: Vec<String>,
    min_size: Option<u64>,
    max_size: Option<u64>,
) {
    thread::spawn(move || {
        let run = || -> anyhow::Result<()> {
            tx.send(BgMessage::Log(i18n::opening_source(&source)))
                .ok();

            let reader = io::open_reader(&source, offset)?;

            tx.send(BgMessage::Log(i18n::tr().parsing_boot_sector.into()))
                .ok();
            let bpb = fat32::bpb::Bpb::parse(reader.as_ref())?;

            tx.send(BgMessage::Log(i18n::tr().loading_fat_tables.into())).ok();
            let fat = fat32::fat_table::FatTables::load(reader.as_ref(), &bpb)?;
            let divergent = fat.divergent_count();

            tx.send(BgMessage::VolumeInfo {
                label: bpb.volume_label.clone(),
                bytes_per_sector: bpb.bytes_per_sector,
                cluster_size: bpb.cluster_size,
                total_data_clusters: bpb.total_data_clusters,
                fat_divergent: divergent,
            })
            .ok();

            if divergent > 0 {
                tx.send(BgMessage::Log(i18n::fat_divergent_warning(divergent)))
                    .ok();
            }

            let signatures = {
                let all = recovery::signatures::builtin_signatures();
                recovery::signatures::filter_signatures(&all, &file_types)
            };

            // --- Directory scan ---
            let mut recovered = Vec::new();
            if matches!(mode, Mode::Scan | Mode::All) {
                tx.send(BgMessage::Log(
                    i18n::tr().scanning_directories.into(),
                ))
                .ok();
                recovered =
                    recovery::dir_scan::scan_deleted(reader.as_ref(), &bpb, &fat)?;

                if let Some(min) = min_size {
                    recovered.retain(|f| f.size as u64 >= min);
                }
                if let Some(max) = max_size {
                    recovered.retain(|f| f.size as u64 <= max);
                }
            }

            // --- Carving ---
            let mut carved = Vec::new();
            if matches!(mode, Mode::Carve | Mode::All) {
                tx.send(BgMessage::Log(
                    i18n::tr().carving_clusters.into(),
                ))
                .ok();
                carved = recovery::carver::carve_files(
                    reader.as_ref(),
                    &bpb,
                    &fat,
                    &signatures,
                )?;

                if let Some(min) = min_size {
                    carved.retain(|f| f.size >= min);
                }
                if let Some(max) = max_size {
                    carved.retain(|f| f.size <= max);
                }
            }

            tx.send(BgMessage::Log(i18n::scan_complete(
                recovered.len(),
                carved.len(),
            )))
            .ok();

            tx.send(BgMessage::ScanResults { recovered, carved }).ok();
            Ok(())
        };

        if let Err(e) = run() {
            tx.send(BgMessage::Error(format!("{e:#}"))).ok();
        }
        tx.send(BgMessage::ScanFinished).ok();
    });
}

// ---------------------------------------------------------------------------
// Background task: extract
// ---------------------------------------------------------------------------

fn spawn_extract(
    tx: mpsc::Sender<BgMessage>,
    source: String,
    offset: u64,
    output_dir: PathBuf,
    recovered: Vec<RecoveredFile>,
    carved: Vec<CarvedFile>,
) {
    thread::spawn(move || {
        let run = || -> anyhow::Result<()> {
            let reader = io::open_reader(&source, offset)?;
            let bpb = fat32::bpb::Bpb::parse(reader.as_ref())?;

            std::fs::create_dir_all(&output_dir)?;

            let total = recovered.len() + carved.len();
            tx.send(BgMessage::Log(i18n::extracting_n_files(total)))
                .ok();

            let recovered_bytes = output::write_recovered_files(
                reader.as_ref(),
                &bpb,
                &recovered,
                &output_dir,
            )?;
            tx.send(BgMessage::ExtractProgress {
                current: recovered.len(),
                total,
            })
            .ok();

            let carved_bytes = output::write_carved_files(
                reader.as_ref(),
                &bpb,
                &carved,
                &output_dir,
            )?;
            tx.send(BgMessage::ExtractProgress {
                current: total,
                total,
            })
            .ok();

            output::write_report(&recovered, &carved, &output_dir)?;

            tx.send(BgMessage::ExtractDone {
                recovered_bytes,
                carved_bytes,
            })
            .ok();
            Ok(())
        };

        if let Err(e) = run() {
            tx.send(BgMessage::Error(format!("{e:#}"))).ok();
        }
        tx.send(BgMessage::ExtractFinished).ok();
    });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    for &unit in UNITS {
        if size < 1024.0 {
            return format!("{size:.1} {unit}");
        }
        size /= 1024.0;
    }
    format!("{size:.1} TB")
}

fn parse_u64_opt(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        s.parse().ok()
    }
}

/// List available drives / block devices for the current platform.
#[cfg(windows)]
fn list_drives() -> Vec<String> {
    // Query the Win32 API for logical drive bitmask
    let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
    let mut drives = Vec::new();
    for i in 0u32..26 {
        if mask & (1 << i) != 0 {
            let letter = (b'A' + i as u8) as char;
            // Use the \\.\X: form so the tool can open the raw volume
            drives.push(format!("\\\\.\\{letter}:"));
        }
    }
    drives
}

#[cfg(unix)]
fn list_drives() -> Vec<String> {
    let mut drives = Vec::new();
    // Common block device paths on Linux/macOS
    let candidates: &[&str] = &["/dev/sda", "/dev/sdb", "/dev/sdc", "/dev/sdd",
        "/dev/nvme0n1", "/dev/nvme1n1",
        "/dev/mmcblk0", "/dev/mmcblk1",
        "/dev/disk0", "/dev/disk1", "/dev/disk2", "/dev/disk3"];

    for &dev in candidates {
        if std::path::Path::new(dev).exists() {
            drives.push(dev.to_string());
        }
    }

    // Also list numbered partitions for found disks (sd*N, nvme*pN)
    if let Ok(entries) = std::fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if (name.starts_with("sd") && name.len() == 4 && name.as_bytes()[3].is_ascii_digit())
                || name.starts_with("mmcblk")
                || name.starts_with("nvme")
            {
                let path = format!("/dev/{name}");
                if !drives.contains(&path) {
                    drives.push(path);
                }
            }
        }
    }

    drives.sort();
    drives
}

#[cfg(not(any(windows, unix)))]
fn list_drives() -> Vec<String> {
    Vec::new()
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("…{}", &s[s.len() - max + 1..])
    }
}

// ---------------------------------------------------------------------------
// eframe App implementation
// ---------------------------------------------------------------------------

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain messages from background thread
        self.poll_messages();

        // Request repaint while background work is running
        if self.scanning || self.extracting {
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(i18n::tr().app_title);
                ui.separator();
                ui.label(i18n::tr().recovery_tool);
            });
        });

        egui::TopBottomPanel::bottom("log_panel")
            .resizable(true)
            .min_height(80.0)
            .default_height(140.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(i18n::tr().log_label).strong());
                ui.separator();
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.logs {
                            ui.label(egui::RichText::new(line).monospace().size(12.0));
                        }
                    });
            });

        egui::SidePanel::left("settings_panel")
            .resizable(true)
            .default_width(340.0)
            .min_width(280.0)
            .show(ctx, |ui| {
                self.draw_settings(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_results(ui);
        });
    }
}

impl App {
    // -----------------------------------------------------------------------
    // Message polling
    // -----------------------------------------------------------------------

    fn poll_messages(&mut self) {
        let Some(rx) = self.rx.as_ref() else {
            return;
        };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                BgMessage::Log(text) => self.logs.push(text),
                BgMessage::VolumeInfo {
                    label,
                    bytes_per_sector,
                    cluster_size,
                    total_data_clusters,
                    fat_divergent,
                } => {
                    self.volume_label = label;
                    self.bytes_per_sector = bytes_per_sector;
                    self.cluster_size = cluster_size;
                    self.total_data_clusters = total_data_clusters;
                    self.fat_divergent = fat_divergent;
                    self.has_volume_info = true;
                }
                BgMessage::ScanResults { recovered, carved } => {
                    self.recovered_selected = vec![true; recovered.len()];
                    self.carved_selected = vec![true; carved.len()];
                    self.recovered = recovered;
                    self.carved = carved;
                }
                BgMessage::ExtractProgress { current, total } => {
                    self.progress_current = current;
                    self.progress_total = total;
                }
                BgMessage::ExtractDone {
                    recovered_bytes,
                    carved_bytes,
                } => {
                    self.last_recovered_bytes = recovered_bytes;
                    self.last_carved_bytes = carved_bytes;
                    self.extract_done = true;
                    self.logs.push(i18n::extraction_complete(
                        &human_size(recovered_bytes + carved_bytes),
                    ));
                }
                BgMessage::Error(e) => {
                    self.logs.push(i18n::error_msg(&e));
                }
                BgMessage::ScanFinished => {
                    self.scanning = false;
                }
                BgMessage::ExtractFinished => {
                    self.extracting = false;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Settings panel (left)
    // -----------------------------------------------------------------------

    fn draw_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(i18n::tr().settings);
        ui.separator();

        // --- Language selector ---
        ui.label(egui::RichText::new(i18n::tr().language_label).strong());
        ui.horizontal(|ui| {
            for &lang in i18n::Language::all() {
                if ui
                    .selectable_label(i18n::language() == lang, lang.label())
                    .clicked()
                {
                    i18n::set_language(lang);
                }
            }
        });
        ui.add_space(4.0);

        // --- Source ---
        ui.label(egui::RichText::new(i18n::tr().source).strong());
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.source);
            if ui.button(i18n::tr().file_btn).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(i18n::tr().select_disk_image)
                    .pick_file()
                {
                    self.source = path.display().to_string();
                }
            }
        });
        // Drive picker
        ui.horizontal(|ui| {
            ui.label(i18n::tr().or_select_drive);
            egui::ComboBox::from_id_salt("drive_picker")
                .selected_text(if self.source.is_empty() {
                    "—".to_string()
                } else {
                    self.source.clone()
                })
                .show_ui(ui, |ui| {
                    for drive in &list_drives() {
                        if ui.selectable_label(self.source == *drive, drive).clicked() {
                            self.source = drive.clone();
                        }
                    }
                });
            if ui.button("↻").on_hover_text(i18n::tr().refresh_drive_list).clicked() {
                // ComboBox re-queries list_drives() on next frame automatically
            }
        });
        ui.add_space(4.0);

        // --- Output directory ---
        ui.label(egui::RichText::new(i18n::tr().output_directory).strong());
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.output_dir);
            if ui.button(i18n::tr().browse_btn).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(i18n::tr().select_output_dir)
                    .pick_folder()
                {
                    self.output_dir = path.display().to_string();
                }
            }
        });
        ui.add_space(4.0);

        // --- Offset ---
        ui.label(egui::RichText::new(i18n::tr().partition_offset).strong());
        ui.text_edit_singleline(&mut self.offset);
        ui.add_space(4.0);

        // --- Mode ---
        ui.label(egui::RichText::new(i18n::tr().recovery_mode).strong());
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.mode, Mode::All, i18n::tr().mode_all);
            ui.selectable_value(&mut self.mode, Mode::Scan, i18n::tr().mode_scan);
            ui.selectable_value(&mut self.mode, Mode::Carve, i18n::tr().mode_carve);
        });
        ui.add_space(4.0);

        // --- File types ---
        ui.label(egui::RichText::new(i18n::tr().file_types_label).strong());
        ui.text_edit_singleline(&mut self.file_types);
        ui.add_space(4.0);

        // --- Size filters ---
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(i18n::tr().min_size_label);
                ui.text_edit_singleline(&mut self.min_size);
            });
            ui.vertical(|ui| {
                ui.label(i18n::tr().max_size_label);
                ui.text_edit_singleline(&mut self.max_size);
            });
        });
        ui.add_space(12.0);

        // --- Action buttons ---
        let busy = self.scanning || self.extracting;

        ui.horizontal(|ui| {
            let scan_btn = ui.add_enabled(!busy && !self.source.is_empty(), egui::Button::new(i18n::tr().scan_btn));
            if scan_btn.clicked() {
                self.start_scan();
            }

            let extract_btn = ui.add_enabled(
                !busy && self.has_selected_files(),
                egui::Button::new(i18n::tr().recover_btn),
            );
            if extract_btn.clicked() {
                self.start_extract();
            }
        });

        // --- Progress ---
        if self.scanning {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(i18n::tr().scanning);
            });
        }

        if self.extracting {
            ui.add_space(8.0);
            if self.progress_total > 0 {
                let frac = self.progress_current as f32 / self.progress_total as f32;
                ui.add(
                    egui::ProgressBar::new(frac)
                        .text(format!(
                            "{}/{}",
                            self.progress_current, self.progress_total
                        ))
                        .animate(true),
                );
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(i18n::tr().extracting);
                });
            }
        }

        if self.extract_done {
            ui.add_space(8.0);
            ui.colored_label(
                egui::Color32::from_rgb(80, 200, 80),
                i18n::files_written_to(&self.output_dir),
            );
        }

        // --- Volume info ---
        if self.has_volume_info {
            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new(i18n::tr().volume_info).strong());
            egui::Grid::new("vol_info").show(ui, |ui| {
                ui.label(i18n::tr().label_field);
                ui.label(if self.volume_label.is_empty() {
                    i18n::tr().none_label
                } else {
                    &self.volume_label
                });
                ui.end_row();

                ui.label(i18n::tr().sector_size);
                ui.label(format!("{}", self.bytes_per_sector));
                ui.end_row();

                ui.label(i18n::tr().cluster_size_label);
                ui.label(format!("{}", self.cluster_size));
                ui.end_row();

                ui.label(i18n::tr().data_clusters);
                ui.label(format!("{}", self.total_data_clusters));
                ui.end_row();

                if self.fat_divergent > 0 {
                    ui.label(i18n::tr().fat_divergences);
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 180, 50),
                        format!("{}", self.fat_divergent),
                    );
                    ui.end_row();
                }
            });
        }
    }

    // -----------------------------------------------------------------------
    // Results panel (center)
    // -----------------------------------------------------------------------

    fn draw_results(&mut self, ui: &mut egui::Ui) {
        if self.recovered.is_empty() && self.carved.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(i18n::tr().no_results_hint)
                        .size(16.0)
                        .color(egui::Color32::GRAY),
                );
            });
            return;
        }

        // Summary
        ui.horizontal(|ui| {
            ui.heading(i18n::tr().results);
            ui.separator();
            ui.label(i18n::dir_scan_carved_summary(
                self.recovered.len(),
                self.carved.len(),
            ));
        });
        ui.add_space(4.0);

        // Selection controls
        ui.horizontal(|ui| {
            if ui.button(i18n::tr().select_all).clicked() {
                for s in &mut self.recovered_selected {
                    *s = true;
                }
                for s in &mut self.carved_selected {
                    *s = true;
                }
            }
            if ui.button(i18n::tr().deselect_all).clicked() {
                for s in &mut self.recovered_selected {
                    *s = false;
                }
                for s in &mut self.carved_selected {
                    *s = false;
                }
            }
            let sel_count = self.selected_count();
            ui.label(i18n::n_selected(sel_count));
        });
        ui.separator();

        // Table with scroll
        egui::ScrollArea::both().show(ui, |ui| {
            // --- Recovered files table ---
            if !self.recovered.is_empty() {
                ui.label(
                    egui::RichText::new(i18n::tr().dir_scan_recovered)
                        .strong()
                        .size(14.0),
                );
                ui.add_space(2.0);

                egui::Grid::new("recovered_table")
                    .striped(true)
                    .min_col_width(40.0)
                    .show(ui, |ui| {
                        // Header
                        ui.label(egui::RichText::new("").strong());
                        ui.label(egui::RichText::new(i18n::tr().col_name).strong());
                        ui.label(egui::RichText::new(i18n::tr().col_path).strong());
                        ui.label(egui::RichText::new(i18n::tr().col_size).strong());
                        ui.label(egui::RichText::new(i18n::tr().col_cluster).strong());
                        ui.label(egui::RichText::new(i18n::tr().col_confidence).strong());
                        ui.end_row();

                        for (i, file) in self.recovered.iter().enumerate() {
                            if let Some(sel) = self.recovered_selected.get_mut(i) {
                                ui.checkbox(sel, "");
                            }
                            ui.label(&file.name);
                            ui.label(truncate_str(&file.dir_path, 30));
                            ui.label(human_size(file.size as u64));
                            ui.label(format!("{}", file.start_cluster));
                            let conf_color = match file.confidence {
                                recovery::Confidence::High => {
                                    egui::Color32::from_rgb(80, 200, 80)
                                }
                                recovery::Confidence::Medium => {
                                    egui::Color32::from_rgb(255, 200, 50)
                                }
                                recovery::Confidence::Carved => {
                                    egui::Color32::from_rgb(150, 150, 255)
                                }
                            };
                            ui.colored_label(conf_color, format!("{}", file.confidence));
                            ui.end_row();
                        }
                    });
                ui.add_space(12.0);
            }

            // --- Carved files table ---
            if !self.carved.is_empty() {
                ui.label(
                    egui::RichText::new(i18n::tr().carved_files)
                        .strong()
                        .size(14.0),
                );
                ui.add_space(2.0);

                egui::Grid::new("carved_table")
                    .striped(true)
                    .min_col_width(40.0)
                    .show(ui, |ui| {
                        // Header
                        ui.label(egui::RichText::new("").strong());
                        ui.label(egui::RichText::new(i18n::tr().col_type).strong());
                        ui.label(egui::RichText::new(i18n::tr().col_extension).strong());
                        ui.label(egui::RichText::new(i18n::tr().col_size).strong());
                        ui.label(egui::RichText::new(i18n::tr().col_offset).strong());
                        ui.end_row();

                        for (i, file) in self.carved.iter().enumerate() {
                            if let Some(sel) = self.carved_selected.get_mut(i) {
                                ui.checkbox(sel, "");
                            }
                            ui.label(&file.signature_name);
                            ui.label(&file.extension);
                            ui.label(human_size(file.size));
                            ui.label(format!("{:#X}", file.offset));
                            ui.end_row();
                        }
                    });
            }
        });
    }

    // -----------------------------------------------------------------------
    // Actions
    // -----------------------------------------------------------------------

    fn start_scan(&mut self) {
        self.scanning = true;
        self.extract_done = false;
        self.has_volume_info = false;
        self.recovered.clear();
        self.carved.clear();
        self.recovered_selected.clear();
        self.carved_selected.clear();
        self.logs.clear();

        let offset = self.offset.trim().parse::<u64>().unwrap_or(0);
        let file_types: Vec<String> = self
            .file_types
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);

        spawn_scan(
            tx,
            self.source.clone(),
            offset,
            self.mode,
            file_types,
            parse_u64_opt(&self.min_size),
            parse_u64_opt(&self.max_size),
        );
    }

    fn start_extract(&mut self) {
        self.extracting = true;
        self.extract_done = false;
        self.progress_current = 0;
        self.progress_total = 0;

        // Build filtered file lists based on selection
        let recovered: Vec<RecoveredFile> = self
            .recovered
            .iter()
            .enumerate()
            .filter(|(i, _)| self.recovered_selected.get(*i).copied().unwrap_or(false))
            .map(|(_, f)| f.clone())
            .collect();

        let carved: Vec<CarvedFile> = self
            .carved
            .iter()
            .enumerate()
            .filter(|(i, _)| self.carved_selected.get(*i).copied().unwrap_or(false))
            .map(|(_, f)| f.clone())
            .collect();

        let offset = self.offset.trim().parse::<u64>().unwrap_or(0);

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);

        spawn_extract(
            tx,
            self.source.clone(),
            offset,
            PathBuf::from(&self.output_dir),
            recovered,
            carved,
        );
    }

    fn has_selected_files(&self) -> bool {
        self.recovered_selected.iter().any(|&s| s)
            || self.carved_selected.iter().any(|&s| s)
    }

    fn selected_count(&self) -> usize {
        self.recovered_selected.iter().filter(|&&s| s).count()
            + self.carved_selected.iter().filter(|&&s| s).count()
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn run() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FAT32 Undelete",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}
