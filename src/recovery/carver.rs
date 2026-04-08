// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

use crate::fat32::bpb::Bpb;
use crate::fat32::fat_table::FatTables;
use crate::io::DiskReader;

use super::signatures::FileSignature;
use super::CarvedFile;

/// Scan free (unallocated) clusters for file signatures and carve files.
pub fn carve_files(
    reader: &dyn DiskReader,
    bpb: &Bpb,
    fat: &FatTables,
    signatures: &[FileSignature],
) -> Result<Vec<CarvedFile>> {
    let free_bitmap = fat.primary.free_cluster_bitmap(bpb.total_data_clusters);
    let cluster_size = bpb.cluster_size as usize;

    // Count free clusters for progress bar
    let free_count: u64 = free_bitmap.iter().filter(|&&f| f).count() as u64;
    let pb = ProgressBar::new(free_count);
    pb.set_style(
        ProgressStyle::with_template(
            "  Carving: [{bar:40.cyan/blue}] {pos}/{len} free clusters ({eta})",
        )
        .unwrap()
        .progress_chars("##-"),
    );

    let mut results: Vec<CarvedFile> = Vec::new();
    let mut scanned: u64 = 0;
    let max_cluster = bpb.total_data_clusters + 2;

    let mut cluster = 2u32;
    while cluster < max_cluster {
        if !free_bitmap.get(cluster as usize).copied().unwrap_or(false) {
            cluster += 1;
            continue;
        }

        scanned += 1;
        pb.set_position(scanned);

        // Read this cluster
        let offset = bpb.cluster_offset(cluster);
        let mut buf = vec![0u8; cluster_size];
        if reader.read_at(offset, &mut buf).is_err() {
            cluster += 1;
            continue;
        }

        // Check each signature against the start of this cluster
        for sig in signatures {
            if buf.len() < sig.header.len() {
                continue;
            }
            if &buf[..sig.header.len()] != sig.header {
                continue;
            }

            // Found a header match — carve the file
            log::debug!(
                "found {} header at cluster {} (offset {:#X})",
                sig.name,
                cluster,
                offset
            );

            let carved = carve_single(reader, bpb, &free_bitmap, cluster, sig)?;
            if let Some(cf) = carved {
                // Skip past the carved region
                let skip = cf.clusters.len() as u32;
                results.push(cf);
                cluster += skip.max(1);
                // Don't double-count in progress
                scanned += (skip.saturating_sub(1)) as u64;
                pb.set_position(scanned.min(free_count));
                continue;
            }
        }

        cluster += 1;
    }

    pb.finish_and_clear();
    log::info!("carving found {} file(s)", results.len());
    Ok(results)
}

/// Carve a single file starting at `start_cluster` using the given signature.
fn carve_single(
    reader: &dyn DiskReader,
    bpb: &Bpb,
    free_bitmap: &[bool],
    start_cluster: u32,
    sig: &FileSignature,
) -> Result<Option<CarvedFile>> {
    let cluster_size = bpb.cluster_size as usize;
    let max_clusters = (sig.max_size as usize).div_ceil(cluster_size);
    let max_cluster = bpb.total_data_clusters + 2;

    // Collect contiguous free clusters from start
    let mut clusters = Vec::new();
    let mut total_data = Vec::new();

    for i in 0..max_clusters as u32 {
        let c = start_cluster + i;
        if c >= max_cluster {
            break;
        }
        if !free_bitmap.get(c as usize).copied().unwrap_or(false) {
            break;
        }
        clusters.push(c);

        let offset = bpb.cluster_offset(c);
        let mut buf = vec![0u8; cluster_size];
        reader.read_at(offset, &mut buf)?;

        // Check for footer in this cluster
        if let Some(footer) = sig.footer
            && let Some(pos) = find_last_subsequence(&buf, footer)
        {
            let end = total_data.len() + pos + footer.len();
            total_data.extend_from_slice(&buf);
            let size = end as u64;
            return Ok(Some(CarvedFile {
                signature_name: sig.name.to_string(),
                extension: sig.extension.to_string(),
                offset: bpb.cluster_offset(start_cluster),
                size,
                clusters,
            }));
        }

        total_data.extend_from_slice(&buf);
    }

    // No footer found (or no footer defined) — use all collected data
    if clusters.is_empty() {
        return Ok(None);
    }

    let size = total_data.len() as u64;
    Ok(Some(CarvedFile {
        signature_name: sig.name.to_string(),
        extension: sig.extension.to_string(),
        offset: bpb.cluster_offset(start_cluster),
        size,
        clusters,
    }))
}

/// Find the last occurrence of `needle` in `haystack`.
fn find_last_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    // Search backwards for efficiency (we want the last match)
    (0..=haystack.len() - needle.len())
        .rev()
        .find(|&i| &haystack[i..i + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_footer() {
        let data = b"hello world\xFF\xD9 trailing";
        let pos = find_last_subsequence(data, &[0xFF, 0xD9]);
        assert_eq!(pos, Some(11));
    }

    #[test]
    fn no_footer() {
        let data = b"hello world no footer here";
        let pos = find_last_subsequence(data, &[0xFF, 0xD9]);
        assert!(pos.is_none());
    }
}
