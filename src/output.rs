// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::fat32::bpb::Bpb;
use crate::i18n;
use crate::io::DiskReader;
use crate::recovery::{CarvedFile, RecoveredFile};

/// Write recovered (dir-scan) files to the output directory.
pub fn write_recovered_files(
    reader: &dyn DiskReader,
    bpb: &Bpb,
    files: &[RecoveredFile],
    output_dir: &Path,
) -> Result<u64> {
    if files.is_empty() {
        return Ok(0);
    }

    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "  Extracting: [{bar:40.green/black}] {pos}/{len} files",
        )
        .unwrap()
        .progress_chars("##-"),
    );

    let mut total_bytes = 0u64;

    for file in files {
        let dest = resolve_path(output_dir, &file.dir_path, &file.name);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let dest = unique_path(&dest);

        let mut out = fs::File::create(&dest)
            .with_context(|| format!("cannot create '{}'", dest.display()))?;

        let mut remaining = file.size as u64;
        let cluster_size = bpb.cluster_size as u64;

        for &cluster in &file.clusters {
            let to_read = remaining.min(cluster_size) as usize;
            if to_read == 0 {
                break;
            }
            let offset = bpb.cluster_offset(cluster);
            let mut buf = vec![0u8; to_read];
            reader.read_at(offset, &mut buf)?;
            out.write_all(&buf)?;
            remaining = remaining.saturating_sub(to_read as u64);
        }

        total_bytes += file.size as u64;
        pb.inc(1);
    }

    pb.finish_and_clear();
    Ok(total_bytes)
}

/// Write carved files to the output directory.
pub fn write_carved_files(
    reader: &dyn DiskReader,
    bpb: &Bpb,
    files: &[CarvedFile],
    output_dir: &Path,
) -> Result<u64> {
    if files.is_empty() {
        return Ok(0);
    }

    let carved_dir = output_dir.join("carved");
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "  Extracting carved: [{bar:40.yellow/black}] {pos}/{len} files",
        )
        .unwrap()
        .progress_chars("##-"),
    );

    let mut total_bytes = 0u64;

    for (idx, file) in files.iter().enumerate() {
        let type_dir = carved_dir.join(&file.extension);
        fs::create_dir_all(&type_dir)?;

        let name = format!("carved_{:04}.{}", idx, file.extension);
        let dest = unique_path(&type_dir.join(&name));

        let mut out = fs::File::create(&dest)
            .with_context(|| format!("cannot create '{}'", dest.display()))?;

        let cluster_size = bpb.cluster_size as u64;
        let mut remaining = file.size;

        for &cluster in &file.clusters {
            let to_read = remaining.min(cluster_size) as usize;
            if to_read == 0 {
                break;
            }
            let offset = bpb.cluster_offset(cluster);
            let mut buf = vec![0u8; to_read];
            reader.read_at(offset, &mut buf)?;
            out.write_all(&buf)?;
            remaining = remaining.saturating_sub(to_read as u64);
        }

        total_bytes += file.size;
        pb.inc(1);
    }

    pb.finish_and_clear();
    Ok(total_bytes)
}

/// Build a destination path preserving the directory tree.
fn resolve_path(output_dir: &Path, dir_path: &str, file_name: &str) -> PathBuf {
    let mut dest = output_dir.to_path_buf();
    if !dir_path.is_empty() {
        for component in dir_path.split('/') {
            dest.push(component);
        }
    }
    dest.push(file_name);
    dest
}

/// If `path` already exists, append _1, _2, etc. until unique.
fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let parent = path.parent().unwrap_or(Path::new("."));

    for i in 1..10000 {
        let name = if ext.is_empty() {
            format!("{stem}_{i}")
        } else {
            format!("{stem}_{i}.{ext}")
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    // Extremely unlikely fallback
    path.to_path_buf()
}

/// Print a human-readable summary table.
pub fn print_summary(
    recovered: &[RecoveredFile],
    carved: &[CarvedFile],
    recovered_bytes: u64,
    carved_bytes: u64,
) {
    println!();
    println!("{}", i18n::tr().recovery_summary);
    println!();

    if !recovered.is_empty() {
        println!("{}", i18n::dir_scan_files_count(recovered.len()));
        println!(
            "  {:<40} {:>10} {:>10}",
            i18n::tr().col_name, i18n::tr().col_size, i18n::tr().col_confidence
        );
        println!("  {}", "-".repeat(62));
        for f in recovered {
            println!(
                "  {:<40} {:>10} {:>10}",
                truncate(&f.full_path(), 40),
                human_size(f.size as u64),
                f.confidence,
            );
        }
        println!();
    }

    if !carved.is_empty() {
        println!("{}", i18n::carved_files_count(carved.len()));
        println!(
            "  {:<20} {:>10} {:>14}",
            i18n::tr().col_type, i18n::tr().col_size, i18n::tr().col_offset
        );
        println!("  {}", "-".repeat(46));
        for f in carved {
            println!(
                "  {:<20} {:>10} {:#14X}",
                f.signature_name,
                human_size(f.size),
                f.offset,
            );
        }
        println!();
    }

    let total_files = recovered.len() + carved.len();
    let total_bytes = recovered_bytes + carved_bytes;
    println!(
        "{}",
        i18n::total_summary(total_files, &human_size(total_bytes))
    );
}

/// Format bytes as human-readable.
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("...{}", &s[s.len() - max + 3..])
    }
}

/// Write a JSON report of all recovered files.
pub fn write_report(
    recovered: &[RecoveredFile],
    carved: &[CarvedFile],
    output_dir: &Path,
) -> Result<()> {
    #[derive(serde::Serialize)]
    struct Report<'a> {
        recovered: &'a [RecoveredFile],
        carved: &'a [CarvedFile],
    }
    let report = Report { recovered, carved };
    let path = output_dir.join("recovery_report.json");
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&path, json)?;
    log::info!("report written to {}", path.display());
    Ok(())
}
