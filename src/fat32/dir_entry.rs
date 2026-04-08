// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use anyhow::Result;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

use crate::fat32::bpb::Bpb;
use crate::io::DiskReader;

/// Marker for a deleted directory entry.
pub const DELETED_MARKER: u8 = 0xE5;
/// Attribute flag for LFN entry.
pub const ATTR_LFN: u8 = 0x0F;
/// Attribute flag for directory.
pub const ATTR_DIRECTORY: u8 = 0x10;
/// Attribute flag for volume label.
pub const ATTR_VOLUME_ID: u8 = 0x08;
/// LFN sequence "last logical" bit.
const LFN_LAST_ENTRY: u8 = 0x40;

/// Represents a parsed 8.3 directory entry.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub raw: [u8; 32],
    pub is_deleted: bool,
    pub is_directory: bool,
    pub is_volume_label: bool,
    pub short_name: String,
    pub extension: String,
    pub attributes: u8,
    pub start_cluster: u32,
    pub file_size: u32,
    pub create_date: u16,
    pub create_time: u16,
    pub modify_date: u16,
    pub modify_time: u16,
    pub access_date: u16,
}

/// A reconstructed directory entry (may have LFN).
#[derive(Debug, Clone)]
pub struct FullDirEntry {
    pub entry: DirEntry,
    pub long_name: Option<String>,
    /// Directory path relative to root (e.g. "photos/vacation").
    pub dir_path: String,
}

impl FullDirEntry {
    /// Best available file name: LFN if present, otherwise 8.3.
    pub fn file_name(&self) -> String {
        if let Some(ref lfn) = self.long_name {
            return lfn.clone();
        }
        let name = self.entry.short_name.trim_end();
        let ext = self.entry.extension.trim_end();
        if ext.is_empty() {
            name.to_string()
        } else {
            format!("{name}.{ext}")
        }
    }
}

impl DirEntry {
    /// Parse a 32-byte raw directory entry.
    pub fn parse(raw: &[u8; 32]) -> Self {
        let first = raw[0];
        let attributes = raw[0x0B];

        let short_name = String::from_utf8_lossy(&raw[0..8]).to_string();
        let extension = String::from_utf8_lossy(&raw[8..11]).to_string();

        let mut c = Cursor::new(raw as &[u8]);

        c.set_position(0x0D);
        let _create_time_tenths = raw[0x0D];
        c.set_position(0x0E);
        let create_time = c.read_u16::<LittleEndian>().unwrap_or(0);
        let create_date = c.read_u16::<LittleEndian>().unwrap_or(0);
        let access_date = c.read_u16::<LittleEndian>().unwrap_or(0);
        let cluster_hi = c.read_u16::<LittleEndian>().unwrap_or(0);
        let modify_time = c.read_u16::<LittleEndian>().unwrap_or(0);
        let modify_date = c.read_u16::<LittleEndian>().unwrap_or(0);
        let cluster_lo = c.read_u16::<LittleEndian>().unwrap_or(0);
        let file_size = c.read_u32::<LittleEndian>().unwrap_or(0);

        let start_cluster = ((cluster_hi as u32) << 16) | cluster_lo as u32;

        Self {
            raw: *raw,
            is_deleted: first == DELETED_MARKER,
            is_directory: (attributes & ATTR_DIRECTORY) != 0,
            is_volume_label: (attributes & ATTR_VOLUME_ID) != 0,
            short_name,
            extension,
            attributes,
            start_cluster,
            file_size,
            create_date,
            create_time,
            modify_date,
            modify_time,
            access_date,
        }
    }

    /// Is this an LFN entry? (attribute byte == 0x0F)
    pub fn is_lfn(raw: &[u8; 32]) -> bool {
        raw[0x0B] == ATTR_LFN
    }

    /// Is this the end-of-directory sentinel? (first byte == 0x00)
    pub fn is_end(raw: &[u8; 32]) -> bool {
        raw[0] == 0x00
    }
}

/// Extract UCS-2 name characters from a single LFN entry (up to 13 chars).
fn lfn_chars(raw: &[u8; 32]) -> Vec<u16> {
    let mut chars = Vec::with_capacity(13);
    // Characters at offsets: 1-10 (5 chars), 14-25 (6 chars), 28-31 (2 chars)
    let ranges: &[(usize, usize)] = &[(1, 11), (0x0E, 0x1A), (0x1C, 0x20)];
    for &(start, end) in ranges {
        let mut i = start;
        while i + 1 < end && i + 1 < 32 {
            let ch = u16::from_le_bytes([raw[i], raw[i + 1]]);
            if ch == 0x0000 || ch == 0xFFFF {
                return chars;
            }
            chars.push(ch);
            i += 2;
        }
    }
    chars
}

/// Reconstruct a long file name from a sequence of LFN entries
/// (ordered last-to-first as they appear on disk: highest sequence first).
pub fn reconstruct_lfn(lfn_entries: &[[u8; 32]]) -> Option<String> {
    if lfn_entries.is_empty() {
        return None;
    }
    let mut all_chars: Vec<u16> = Vec::new();
    // LFN entries on disk are in reverse order (highest sequence number first).
    // We iterate them as stored (reverse), so entry 0 = last logical part.
    for raw in lfn_entries.iter().rev() {
        let chars = lfn_chars(raw);
        all_chars.extend_from_slice(&chars);
    }
    Some(String::from_utf16_lossy(&all_chars))
}

/// Compute the 8.3 checksum used to validate LFN entries.
pub fn short_name_checksum(name_ext: &[u8; 11]) -> u8 {
    let mut sum: u8 = 0;
    for &b in name_ext {
        sum = sum.wrapping_shr(1).wrapping_add(sum.wrapping_shl(7)).wrapping_add(b);
    }
    sum
}

/// Read all entries from a single directory (given its cluster chain).
/// Returns a list of `FullDirEntry` items (both active and deleted).
pub fn read_directory(
    reader: &dyn DiskReader,
    bpb: &Bpb,
    clusters: &[u32],
    dir_path: &str,
) -> Result<Vec<FullDirEntry>> {
    let mut results = Vec::new();
    let mut pending_lfn: Vec<[u8; 32]> = Vec::new();

    for &cluster in clusters {
        let offset = bpb.cluster_offset(cluster);
        let mut cluster_data = vec![0u8; bpb.cluster_size as usize];
        reader.read_at(offset, &mut cluster_data)?;

        let mut pos = 0;
        while pos + 32 <= cluster_data.len() {
            let raw: [u8; 32] = cluster_data[pos..pos + 32].try_into().unwrap();
            pos += 32;

            if DirEntry::is_end(&raw) {
                // End of directory — but keep scanning other clusters
                // (some implementations don't zero-fill remaining clusters)
                break;
            }

            if raw[0] == 0x00 {
                break;
            }

            if DirEntry::is_lfn(&raw) {
                // Check if this is the start of a new LFN chain
                if raw[0] & LFN_LAST_ENTRY != 0 {
                    pending_lfn.clear();
                }
                pending_lfn.push(raw);
                continue;
            }

            let entry = DirEntry::parse(&raw);

            // Skip volume label entries
            if entry.is_volume_label && !entry.is_directory {
                pending_lfn.clear();
                continue;
            }

            // Skip . and .. entries
            let first_two = &raw[0..2];
            if first_two == b". " || (first_two[0] == b'.' && first_two[1] == b'.') {
                pending_lfn.clear();
                continue;
            }

            // Try to reconstruct LFN
            let long_name = if !pending_lfn.is_empty() {
                // Validate checksum
                let name_ext: [u8; 11] = raw[0..11].try_into().unwrap();
                let expected_cksum = short_name_checksum(&name_ext);
                let lfn_cksum = pending_lfn.first().map(|e| e[0x0D]).unwrap_or(0);
                if lfn_cksum == expected_cksum || entry.is_deleted {
                    // For deleted entries, checksum may not match (first byte changed)
                    reconstruct_lfn(&pending_lfn)
                } else {
                    None
                }
            } else {
                None
            };
            pending_lfn.clear();

            results.push(FullDirEntry {
                entry,
                long_name,
                dir_path: dir_path.to_string(),
            });
        }
    }
    Ok(results)
}

/// Recursively scan all directories starting from root.
/// Returns all entries (active and deleted) with their full paths.
pub fn scan_all_directories(
    reader: &dyn DiskReader,
    bpb: &Bpb,
    fat: &crate::fat32::fat_table::FatTables,
) -> Result<Vec<FullDirEntry>> {
    let mut all_entries = Vec::new();
    let mut stack: Vec<(u32, String)> = vec![(bpb.root_cluster, String::new())];

    while let Some((cluster, path)) = stack.pop() {
        let clusters = fat.get_chain(cluster, 1024);
        if clusters.is_empty() {
            // For root, if FAT chain is empty, try reading the root cluster directly
            let single = vec![cluster];
            let entries = read_directory(reader, bpb, &single, &path)?;
            for entry in &entries {
                if entry.entry.is_directory
                    && !entry.entry.is_deleted
                    && entry.entry.start_cluster >= 2
                {
                    let sub_path = if path.is_empty() {
                        entry.file_name()
                    } else {
                        format!("{}/{}", path, entry.file_name())
                    };
                    stack.push((entry.entry.start_cluster, sub_path));
                }
            }
            all_entries.extend(entries);
        } else {
            let entries = read_directory(reader, bpb, &clusters, &path)?;
            for entry in &entries {
                if entry.entry.is_directory
                    && !entry.entry.is_deleted
                    && entry.entry.start_cluster >= 2
                {
                    let sub_path = if path.is_empty() {
                        entry.file_name()
                    } else {
                        format!("{}/{}", path, entry.file_name())
                    };
                    stack.push((entry.entry.start_cluster, sub_path));
                }
            }
            all_entries.extend(entries);
        }
    }
    Ok(all_entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_known() {
        // "HELLO   TXT" in 8.3 format
        let name: [u8; 11] = *b"HELLO   TXT";
        let cksum = short_name_checksum(&name);
        // Just verify it's deterministic and non-zero
        assert_ne!(cksum, 0);
        assert_eq!(cksum, short_name_checksum(&name));
    }

    #[test]
    fn parse_deleted_entry() {
        let mut raw = [0u8; 32];
        raw[0] = 0xE5; // deleted
        raw[1..8].copy_from_slice(b"ELLO   ");
        raw[8..11].copy_from_slice(b"TXT");
        raw[0x0B] = 0x20; // archive attribute
        // file size = 1024
        raw[0x1C] = 0x00;
        raw[0x1D] = 0x04;
        raw[0x1E] = 0x00;
        raw[0x1F] = 0x00;
        // start cluster = 5 (lo=5, hi=0)
        raw[0x1A] = 5;
        raw[0x1B] = 0;
        raw[0x14] = 0;
        raw[0x15] = 0;

        let entry = DirEntry::parse(&raw);
        assert!(entry.is_deleted);
        assert!(!entry.is_directory);
        assert_eq!(entry.start_cluster, 5);
        assert_eq!(entry.file_size, 1024);
    }

    #[test]
    fn lfn_char_extraction() {
        let mut raw = [0xFFu8; 32];
        raw[0] = 0x41; // sequence 1, last
        raw[0x0B] = ATTR_LFN;
        raw[0x0D] = 0; // checksum
        // Put "Hi" in first two char slots (offsets 1-4)
        raw[1] = b'H';
        raw[2] = 0;
        raw[3] = b'i';
        raw[4] = 0;
        // null terminator
        raw[5] = 0;
        raw[6] = 0;

        let chars = lfn_chars(&raw);
        assert_eq!(chars.len(), 2);
        assert_eq!(String::from_utf16_lossy(&chars), "Hi");
    }
}
