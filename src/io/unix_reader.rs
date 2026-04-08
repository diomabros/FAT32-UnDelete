// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

#![cfg(unix)]

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

use super::DiskReader;

pub struct UnixDiskReader {
    file: Mutex<File>,
    sector_size: u32,
}

impl UnixDiskReader {
    pub fn open(path: &str) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("cannot open '{path}' — try running as root"))?;

        let sector_size = Self::detect_sector_size(&file);

        Ok(Self {
            file: Mutex::new(file),
            sector_size,
        })
    }

    #[cfg(target_os = "linux")]
    fn detect_sector_size(file: &File) -> u32 {
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        let mut ss: libc::c_int = 0;
        // BLKSSZGET = 0x1268
        let ret = unsafe { libc::ioctl(fd, 0x1268, &mut ss) };
        if ret == 0 && ss > 0 {
            ss as u32
        } else {
            512
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn detect_sector_size(_file: &File) -> u32 {
        512
    }
}

impl DiskReader for UnixDiskReader {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let mut f = self.file.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        f.seek(SeekFrom::Start(offset))?;
        let n = f.read(buf)?;
        Ok(n)
    }

    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn size(&self) -> Option<u64> {
        None
    }
}
