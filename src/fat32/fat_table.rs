// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use anyhow::{Result, ensure};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

use crate::fat32::bpb::Bpb;
use crate::io::DiskReader;

/// Mask to extract the 28-bit cluster value from a FAT32 entry.
const FAT_ENTRY_MASK: u32 = 0x0FFF_FFFF;
const EOC_MIN: u32 = 0x0FFF_FFF8;
const BAD_CLUSTER: u32 = 0x0FFF_FFF7;

/// In-memory representation of one FAT table (array of u32 entries).
#[derive(Clone)]
pub struct FatTable {
    entries: Vec<u32>,
}

impl FatTable {
    /// Load FAT #`fat_index` (0-based) from disk.
    pub fn load(reader: &dyn DiskReader, bpb: &Bpb, fat_index: u32) -> Result<Self> {
        let fat_bytes = bpb.fat_size_bytes() as usize;
        let offset = bpb.fat_offset(fat_index);

        let mut raw = vec![0u8; fat_bytes];
        let mut pos = 0usize;
        while pos < fat_bytes {
            let chunk = (fat_bytes - pos).min(1024 * 1024); // 1 MB at a time
            let n = reader.read_at(offset + pos as u64, &mut raw[pos..pos + chunk])?;
            if n == 0 {
                break;
            }
            pos += n;
        }
        ensure!(pos >= 8, "FAT too small ({pos} bytes)");

        let entry_count = fat_bytes / 4;
        let mut entries = Vec::with_capacity(entry_count);
        let mut c = Cursor::new(&raw);
        for _ in 0..entry_count {
            let val = c.read_u32::<LittleEndian>()? & FAT_ENTRY_MASK;
            entries.push(val);
        }
        Ok(Self { entries })
    }

    /// Get the raw 28-bit value for a cluster.
    pub fn get(&self, cluster: u32) -> u32 {
        self.entries.get(cluster as usize).copied().unwrap_or(0)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is cluster marked free?
    pub fn is_free(&self, cluster: u32) -> bool {
        self.get(cluster) == 0
    }

    /// Is cluster marked bad?
    pub fn is_bad(&self, cluster: u32) -> bool {
        self.get(cluster) == BAD_CLUSTER
    }

    /// Is cluster an end-of-chain marker?
    pub fn is_eoc(&self, cluster: u32) -> bool {
        self.get(cluster) >= EOC_MIN
    }

    /// Follow the FAT chain starting at `start`. Returns the list of clusters.
    /// Stops at EOC, free (broken chain), or after `max_len` entries (safety).
    pub fn get_chain(&self, start: u32, max_len: usize) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut current = start;
        let mut seen = std::collections::HashSet::new();

        loop {
            if current < 2 || self.is_bad(current) || self.is_free(current) {
                break;
            }
            if chain.len() >= max_len {
                break;
            }
            if !seen.insert(current) {
                log::warn!("cycle detected in FAT chain at cluster {current}");
                break;
            }
            chain.push(current);
            let next = self.get(current);
            if !(2..0x0FFF_FFF8).contains(&next) {
                // EOC or invalid — this was the last cluster
                break;
            }
            current = next;
        }
        chain
    }

    /// Build a bitmap of free clusters (true = free). Index = cluster number.
    pub fn free_cluster_bitmap(&self, total_data_clusters: u32) -> Vec<bool> {
        let max = (total_data_clusters as usize + 2).min(self.entries.len());
        let mut bitmap = vec![false; max];
        for (i, slot) in bitmap.iter_mut().enumerate().take(max).skip(2) {
            *slot = self.entries[i] == 0;
        }
        bitmap
    }
}

/// Loaded FAT tables (primary + optional secondary) with cross-reference support.
pub struct FatTables {
    pub primary: FatTable,
    pub secondary: Option<FatTable>,
}

impl FatTables {
    pub fn load(reader: &dyn DiskReader, bpb: &Bpb) -> Result<Self> {
        log::info!("loading FAT1...");
        let primary = FatTable::load(reader, bpb, 0)?;

        let secondary = if bpb.num_fats >= 2 {
            log::info!("loading FAT2...");
            Some(FatTable::load(reader, bpb, 1)?)
        } else {
            None
        };

        Ok(Self { primary, secondary })
    }

    /// Try to build a chain from the primary FAT; if it yields nothing useful,
    /// try the secondary FAT.
    pub fn get_chain(&self, start: u32, max_len: usize) -> Vec<u32> {
        let chain = self.primary.get_chain(start, max_len);
        if !chain.is_empty() {
            return chain;
        }
        if let Some(ref sec) = self.secondary {
            return sec.get_chain(start, max_len);
        }
        chain
    }

    /// Check if a cluster is free in the primary FAT.
    pub fn is_free(&self, cluster: u32) -> bool {
        self.primary.is_free(cluster)
    }

    /// Count clusters where FAT1 and FAT2 differ (diagnostic).
    pub fn divergent_count(&self) -> usize {
        let Some(ref sec) = self.secondary else {
            return 0;
        };
        let len = self.primary.len().min(sec.len());
        (0..len)
            .filter(|&i| self.primary.entries[i] != sec.entries[i])
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fat_raw(entries: &[u32]) -> Vec<u8> {
        let mut raw = Vec::with_capacity(entries.len() * 4);
        for &e in entries {
            raw.extend_from_slice(&e.to_le_bytes());
        }
        raw
    }

    #[test]
    fn chain_traversal() {
        // clusters: 0=media, 1=reserved, 2->3->4->EOC
        let entries = vec![0x0FFFFFF8, 0x0FFFFFFF, 3, 4, 0x0FFFFFFF];
        let fat = FatTable {
            entries: entries.iter().map(|e| e & FAT_ENTRY_MASK).collect(),
        };
        let chain = fat.get_chain(2, 100);
        assert_eq!(chain, vec![2, 3, 4]);
    }

    #[test]
    fn chain_broken() {
        // cluster 2 points to 3, but 3 is free (0)
        let entries = vec![0x0FFFFFF8, 0x0FFFFFFF, 3, 0, 0];
        let fat = FatTable {
            entries: entries.iter().map(|e| e & FAT_ENTRY_MASK).collect(),
        };
        let chain = fat.get_chain(2, 100);
        assert_eq!(chain, vec![2]); // stops at 3 because 3 is free
    }

    #[test]
    fn free_bitmap() {
        let entries = vec![0x0FFFFFF8, 0x0FFFFFFF, 3, 0, 0x0FFFFFFF, 0];
        let fat = FatTable {
            entries: entries.iter().map(|e| e & FAT_ENTRY_MASK).collect(),
        };
        let bm = fat.free_cluster_bitmap(4);
        // cluster 0,1 = special; 2=not free(->3); 3=free; 4=not free(EOC); 5=free
        assert!(!bm[2]);
        assert!(bm[3]);
        assert!(!bm[4]);
        assert!(bm[5]);
    }

    #[test]
    fn cycle_detection() {
        // 2->3->2 (cycle)
        let entries = vec![0x0FFFFFF8, 0x0FFFFFFF, 3, 2];
        let fat = FatTable {
            entries: entries.iter().map(|e| e & FAT_ENTRY_MASK).collect(),
        };
        let chain = fat.get_chain(2, 100);
        assert_eq!(chain, vec![2, 3]); // stops when 2 is seen again
    }
}
