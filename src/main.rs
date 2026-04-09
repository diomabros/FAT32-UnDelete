// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

#![allow(dead_code)]

mod ai;
mod fat32;
mod gui;
mod i18n;
mod io;
mod output;
mod recovery;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum)]
enum Mode {
    /// Recover files from deleted directory entries only.
    Scan,
    /// Recover files using signature-based cluster carving only.
    Carve,
    /// Both directory scan and carving (default).
    All,
}

#[derive(Parser, Debug)]
#[command(
    name = "fat32-undelete",
    version,
    about = "Recover deleted files from FAT32 partitions and disk images"
)]
struct Cli {
    /// Path to disk image (.img, .dd), device (\\.\PhysicalDrive0, /dev/sdb1),
    /// or drive letter (E:). Omit to launch the GUI.
    source: Option<String>,

    /// Launch the graphical interface.
    #[arg(long)]
    gui: bool,

    /// Output directory for recovered files.
    #[arg(short, long, default_value = "recovered")]
    output: PathBuf,

    /// Recovery mode.
    #[arg(short, long, value_enum, default_value_t = Mode::All)]
    mode: Mode,

    /// List recoverable files without extracting.
    #[arg(short, long)]
    list: bool,

    /// Filter carved files by type (comma-separated, e.g. "jpeg,png,pdf").
    #[arg(long, value_delimiter = ',')]
    types: Vec<String>,

    /// Minimum file size in bytes to recover.
    #[arg(long)]
    min_size: Option<u64>,

    /// Maximum file size in bytes to recover.
    #[arg(long)]
    max_size: Option<u64>,

    /// Partition offset in bytes (for raw disk images with MBR/GPT).
    #[arg(long, default_value_t = 0)]
    offset: u64,

    /// Increase verbosity (repeat for more: -v, -vv).
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Scan and report without writing any files.
    #[arg(long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    // Detect and set the UI language from OS locale (auto-detected in i18n::init)
    // Language is automatically detected on first use of i18n.

    let cli = Cli::parse();

    // Launch GUI when requested or when no source is provided
    if cli.gui || cli.source.is_none() {
        return gui::run();
    }

    let source = cli.source.unwrap();

    // Initialize logging
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp(None)
        .init();

    // Open source
    println!("{}", i18n::opening_source(&source));
    let reader = io::open_reader(&source, cli.offset)
        .with_context(|| format!("failed to open '{}'", source))?;

    // Parse BPB
    println!("{}", i18n::tr("parsing_boot_sector"));
    let bpb = fat32::bpb::Bpb::parse(reader.as_ref())?;
    let vol_label = if bpb.volume_label.is_empty() {
        i18n::tr("none_label")
    } else {
        bpb.volume_label.clone()
    };
    println!(
        "{}",
        i18n::cli_volume_info(
            &vol_label,
            bpb.bytes_per_sector,
            bpb.cluster_size,
            bpb.total_data_clusters,
        )
    );

    // Load FAT tables
    println!("{}", i18n::tr("loading_fat_tables"));
    let fat = fat32::fat_table::FatTables::load(reader.as_ref(), &bpb)?;
    let divergent = fat.divergent_count();
    if divergent > 0 {
        println!("  {}", i18n::fat_divergent_warning(divergent));
    }

    // Prepare signature list for carving
    let signatures = {
        let all = recovery::signatures::builtin_signatures();
        recovery::signatures::filter_signatures(&all, &cli.types)
    };

    // --- Directory scan ---
    let mut recovered = Vec::new();
    if matches!(cli.mode, Mode::Scan | Mode::All) {
        println!("{}", i18n::tr("scanning_directories"));
        recovered = recovery::dir_scan::scan_deleted(reader.as_ref(), &bpb, &fat, None)?;

        // Apply size filters
        if let Some(min) = cli.min_size {
            recovered.retain(|f| f.size as u64 >= min);
        }
        if let Some(max) = cli.max_size {
            recovered.retain(|f| f.size as u64 <= max);
        }
    }

    // --- Carving ---
    let mut carved = Vec::new();
    if matches!(cli.mode, Mode::Carve | Mode::All) {
        println!("{}", i18n::tr("carving_clusters"));
        carved =
            recovery::carver::carve_files(reader.as_ref(), &bpb, &fat, &signatures, None)?;

        // Apply size filters to carved files too
        if let Some(min) = cli.min_size {
            carved.retain(|f| f.size >= min);
        }
        if let Some(max) = cli.max_size {
            carved.retain(|f| f.size <= max);
        }
    }

    // --- List mode ---
    if cli.list || cli.dry_run {
        output::print_summary(&recovered, &carved, 0, 0);
        if cli.dry_run {
            println!("\n{}", i18n::tr("dry_run_note"));
        }
        return Ok(());
    }

    // --- Extract files ---
    std::fs::create_dir_all(&cli.output)?;

    let recovered_bytes =
        output::write_recovered_files(reader.as_ref(), &bpb, &recovered, &cli.output)?;
    let carved_bytes =
        output::write_carved_files(reader.as_ref(), &bpb, &carved, &cli.output)?;

    output::print_summary(&recovered, &carved, recovered_bytes, carved_bytes);

    // Write JSON report
    output::write_report(&recovered, &carved, &cli.output)?;

    println!("{}", i18n::cli_files_written_to(&cli.output.display().to_string()));
    Ok(())
}
