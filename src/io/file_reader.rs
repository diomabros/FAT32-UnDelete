// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

use super::DiskReader;

/// Reads from a regular file (disk image, `.img`, `.dd`, etc.).
/// Also used as fallback for devices on platforms without a specialized reader.
pub struct FileReader {
    file: Mutex<File>,
    offset: u64,
    size: u64,
}

impl FileReader {
    pub fn open(path: &str, partition_offset: u64) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("cannot open '{path}' (read-only)"))?;
        let meta = file.metadata()?;
        let size = meta.len().saturating_sub(partition_offset);
        Ok(Self {
            file: Mutex::new(file),
            offset: partition_offset,
            size,
        })
    }
}

impl DiskReader for FileReader {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let abs = self.offset.checked_add(offset)
            .context("offset overflow")?;
        let mut f = self.file.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        f.seek(SeekFrom::Start(abs))?;
        let n = f.read(buf)?;
        Ok(n)
    }

    fn sector_size(&self) -> u32 {
        512
    }

    fn size(&self) -> Option<u64> {
        Some(self.size)
    }
}
