// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

pub mod file_reader;
#[cfg(windows)]
pub mod win_reader;
#[cfg(unix)]
pub mod unix_reader;

use anyhow::Result;

/// Read-only abstraction over a block device or image file.
pub trait DiskReader: Send {
    /// Read `buf.len()` bytes starting at absolute byte `offset`.
    /// Returns the number of bytes actually read.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;

    /// Sector size of the underlying medium (typically 512).
    fn sector_size(&self) -> u32;

    /// Total size in bytes (if known).
    fn size(&self) -> Option<u64>;
}

/// Open the appropriate reader based on the path.
/// Raw device paths (`\\.\`, `/dev/`) get platform-specific readers;
/// everything else is treated as an image file.
pub fn open_reader(path: &str, offset: u64) -> Result<Box<dyn DiskReader>> {
    #[cfg(windows)]
    if path.starts_with("\\\\.\\") {
        return Ok(Box::new(win_reader::WindowsDiskReader::open(path)?));
    }

    #[cfg(unix)]
    if path.starts_with("/dev/") {
        return Ok(Box::new(unix_reader::UnixDiskReader::open(path)?));
    }

    let reader = file_reader::FileReader::open(path, offset)?;
    Ok(Box::new(reader))
}
