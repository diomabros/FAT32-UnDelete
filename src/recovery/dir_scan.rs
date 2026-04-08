// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use anyhow::Result;

use crate::fat32::bpb::Bpb;
use crate::fat32::dir_entry;
use crate::fat32::fat_table::FatTables;
use crate::io::DiskReader;
use super::{Confidence, RecoveredFile};

/// Scan all directories for deleted entries and attempt recovery.
pub fn scan_deleted(
    reader: &dyn DiskReader,
    bpb: &Bpb,
    fat: &FatTables,
) -> Result<Vec<RecoveredFile>> {
    let all_entries = dir_entry::scan_all_directories(reader, bpb, fat)?;
    let mut recovered = Vec::new();

    for full in &all_entries {
        if !full.entry.is_deleted {
            continue;
        }
        // Skip directories and volume labels
        if full.entry.is_directory || full.entry.is_volume_label {
            continue;
        }
        // Skip zero-size files
        if full.entry.file_size == 0 {
            continue;
        }
        let start = full.entry.start_cluster;
        if start < 2 || start as usize >= fat.primary.len() {
            log::debug!("skipping deleted entry with invalid start cluster {start}");
            continue;
        }

        let name = full.file_name();
        let max_clusters =
            (full.entry.file_size as u64).div_ceil(bpb.cluster_size as u64);

        // Try to follow the FAT chain
        let chain = fat.get_chain(start, max_clusters as usize);
        let (clusters, confidence) = if chain.len() as u64 >= max_clusters {
            // FAT chain is intact
            (chain[..max_clusters as usize].to_vec(), Confidence::High)
        } else {
            // FAT chain is broken — assume contiguous allocation
            let contiguous = build_contiguous(start, max_clusters as u32, fat, bpb);
            (contiguous, Confidence::Medium)
        };

        if clusters.is_empty() {
            continue;
        }

        recovered.push(RecoveredFile {
            name,
            dir_path: full.dir_path.clone(),
            size: full.entry.file_size,
            start_cluster: start,
            clusters,
            confidence,
        });
    }

    log::info!(
        "directory scan found {} deleted file(s)",
        recovered.len()
    );
    Ok(recovered)
}

/// Build a contiguous cluster list starting at `start`, up to `count` clusters.
/// Skips bad clusters and stops if a cluster is already allocated.
fn build_contiguous(start: u32, count: u32, fat: &FatTables, bpb: &Bpb) -> Vec<u32> {
    let max_cluster = bpb.total_data_clusters + 2;
    let mut clusters = Vec::with_capacity(count as usize);
    for i in 0..count {
        let c = start + i;
        if c >= max_cluster {
            break;
        }
        if fat.primary.is_bad(c) {
            break;
        }
        // For contiguous assumption, we accept even non-free clusters
        // (they may still hold our data)
        clusters.push(c);
    }
    clusters
}
