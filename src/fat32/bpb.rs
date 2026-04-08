// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use anyhow::{Result, bail, ensure};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;

use crate::io::DiskReader;

/// Parsed FAT32 BIOS Parameter Block + derived geometry.
#[derive(Debug, Clone)]
pub struct Bpb {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub total_sectors: u32,
    pub sectors_per_fat: u32,
    pub root_cluster: u32,
    pub fs_info_sector: u16,
    pub volume_label: String,

    // derived
    pub fat_start_sector: u32,
    pub data_start_sector: u32,
    pub total_data_clusters: u32,
    pub cluster_size: u32,
}

impl Bpb {
    /// Read and parse the BPB from sector 0 of the given reader.
    pub fn parse(reader: &dyn DiskReader) -> Result<Self> {
        let mut sector = vec![0u8; 512];
        let n = reader.read_at(0, &mut sector)?;
        ensure!(n >= 512, "failed to read boot sector (got {n} bytes)");

        // Validate boot signature
        ensure!(
            sector[510] == 0x55 && sector[511] == 0xAA,
            "invalid boot signature (expected 0x55AA)"
        );

        let mut c = Cursor::new(&sector);

        // Skip jump + OEM name (11 bytes)
        c.set_position(0x0B);
        let bytes_per_sector = c.read_u16::<LittleEndian>()?;
        let sectors_per_cluster = sector[0x0D];
        let _reserved_sectors_dup = c.read_u16::<LittleEndian>()?; // 0x0E
        // re-seek because read_u16 at 0x0B consumed 2 bytes then 0x0D was manual
        c.set_position(0x0E);
        let reserved_sectors = c.read_u16::<LittleEndian>()?;
        let num_fats = sector[0x10];

        // 0x11: root_entry_count (must be 0 for FAT32)
        c.set_position(0x13);
        let total_sectors_16 = c.read_u16::<LittleEndian>()?;

        c.set_position(0x20);
        let total_sectors_32 = c.read_u32::<LittleEndian>()?;
        let total_sectors = if total_sectors_16 != 0 {
            total_sectors_16 as u32
        } else {
            total_sectors_32
        };

        c.set_position(0x24);
        let sectors_per_fat = c.read_u32::<LittleEndian>()?;

        c.set_position(0x2C);
        let root_cluster = c.read_u32::<LittleEndian>()?;

        c.set_position(0x30);
        let fs_info_sector = c.read_u16::<LittleEndian>()?;

        // Volume label at 0x47 (11 bytes)
        let volume_label = String::from_utf8_lossy(&sector[0x47..0x52])
            .trim_end()
            .to_string();

        // FS type string at 0x52 (8 bytes) — informational, not authoritative
        let fs_type = String::from_utf8_lossy(&sector[0x52..0x5A]);
        if !fs_type.contains("FAT32") {
            log::warn!("FS type label is '{fs_type}' (expected 'FAT32   ')");
        }

        // Validate field ranges
        ensure!(
            bytes_per_sector.is_power_of_two()
                && (512..=4096).contains(&bytes_per_sector),
            "invalid bytes_per_sector: {bytes_per_sector}"
        );
        ensure!(
            sectors_per_cluster.is_power_of_two() && sectors_per_cluster > 0,
            "invalid sectors_per_cluster: {sectors_per_cluster}"
        );
        ensure!(num_fats >= 1, "num_fats must be >= 1");
        ensure!(sectors_per_fat > 0, "sectors_per_fat must be > 0");
        if root_cluster < 2 {
            bail!("invalid root_cluster: {root_cluster}");
        }

        // Derived values
        let fat_start_sector = reserved_sectors as u32;
        let data_start_sector =
            reserved_sectors as u32 + (num_fats as u32) * sectors_per_fat;
        let data_sectors = total_sectors.saturating_sub(data_start_sector);
        let total_data_clusters = data_sectors / sectors_per_cluster as u32;
        let cluster_size =
            bytes_per_sector as u32 * sectors_per_cluster as u32;

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            total_sectors,
            sectors_per_fat,
            root_cluster,
            fs_info_sector,
            volume_label,
            fat_start_sector,
            data_start_sector,
            total_data_clusters,
            cluster_size,
        })
    }

    /// Convert a cluster number to an absolute byte offset.
    pub fn cluster_offset(&self, cluster: u32) -> u64 {
        let lba = self.data_start_sector as u64
            + (cluster as u64 - 2) * self.sectors_per_cluster as u64;
        lba * self.bytes_per_sector as u64
    }

    /// Number of bytes consumed by one FAT table.
    pub fn fat_size_bytes(&self) -> u64 {
        self.sectors_per_fat as u64 * self.bytes_per_sector as u64
    }

    /// Byte offset of FAT #n (0-indexed).
    pub fn fat_offset(&self, fat_index: u32) -> u64 {
        (self.fat_start_sector as u64 + fat_index as u64 * self.sectors_per_fat as u64)
            * self.bytes_per_sector as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid FAT32 boot sector for testing.
    pub fn make_test_boot_sector() -> Vec<u8> {
        let mut s = vec![0u8; 512];
        // Jump
        s[0] = 0xEB;
        s[1] = 0x58;
        s[2] = 0x90;
        // bytes_per_sector = 512
        s[0x0B] = 0x00;
        s[0x0C] = 0x02;
        // sectors_per_cluster = 8
        s[0x0D] = 8;
        // reserved_sectors = 32
        s[0x0E] = 32;
        s[0x0F] = 0;
        // num_fats = 2
        s[0x10] = 2;
        // total_sectors_16 = 0
        s[0x13] = 0;
        s[0x14] = 0;
        // total_sectors_32 = 32768
        s[0x20] = 0x00;
        s[0x21] = 0x80;
        s[0x22] = 0x00;
        s[0x23] = 0x00;
        // sectors_per_fat = 256
        s[0x24] = 0x00;
        s[0x25] = 0x01;
        s[0x26] = 0x00;
        s[0x27] = 0x00;
        // root_cluster = 2
        s[0x2C] = 2;
        // fs_info_sector = 1
        s[0x30] = 1;
        // volume label "TEST       "
        s[0x47..0x52].copy_from_slice(b"TEST       ");
        // FS type
        s[0x52..0x5A].copy_from_slice(b"FAT32   ");
        // Boot signature
        s[510] = 0x55;
        s[511] = 0xAA;
        s
    }

    struct MockReader(Vec<u8>);
    impl DiskReader for MockReader {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<usize> {
            let start = offset as usize;
            let end = (start + buf.len()).min(self.0.len());
            let len = end.saturating_sub(start);
            buf[..len].copy_from_slice(&self.0[start..end]);
            Ok(len)
        }
        fn sector_size(&self) -> u32 { 512 }
        fn size(&self) -> Option<u64> { Some(self.0.len() as u64) }
    }

    #[test]
    fn parse_valid_bpb() {
        let data = make_test_boot_sector();
        let reader = MockReader(data);
        let bpb = Bpb::parse(&reader).unwrap();
        assert_eq!(bpb.bytes_per_sector, 512);
        assert_eq!(bpb.sectors_per_cluster, 8);
        assert_eq!(bpb.reserved_sectors, 32);
        assert_eq!(bpb.num_fats, 2);
        assert_eq!(bpb.sectors_per_fat, 256);
        assert_eq!(bpb.root_cluster, 2);
        assert_eq!(bpb.fat_start_sector, 32);
        assert_eq!(bpb.data_start_sector, 32 + 2 * 256);
        assert_eq!(bpb.cluster_size, 512 * 8);
    }

    #[test]
    fn bad_signature_rejected() {
        let mut data = make_test_boot_sector();
        data[510] = 0x00;
        let reader = MockReader(data);
        assert!(Bpb::parse(&reader).is_err());
    }
}
