// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

use anyhow::{Context, Result, bail};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_NORMAL, FILE_FLAG_NO_BUFFERING, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};

use super::DiskReader;

const GENERIC_READ: u32 = 0x80000000;

// Declare Win32 FFI functions directly — works across all windows-sys versions.
unsafe extern "system" {
    fn CreateFileW(
        lpfilename: *const u16,
        dwdesiredaccess: u32,
        dwsharemode: u32,
        lpsecurityattributes: *const std::ffi::c_void,
        dwcreationdisposition: u32,
        dwflagsandattributes: u32,
        htemplatefile: HANDLE,
    ) -> HANDLE;

    fn ReadFile(
        hfile: HANDLE,
        lpbuffer: *mut u8,
        nnumberofbytestoread: u32,
        lpnumberofbytesread: *mut u32,
        lpoverlapped: *mut std::ffi::c_void,
    ) -> i32;

    fn SetFilePointerEx(
        hfile: HANDLE,
        lidistancetomove: i64,
        lpnewfilepointer: *mut i64,
        dwmovemethod: u32,
    ) -> i32;
}

pub struct WindowsDiskReader {
    handle: Mutex<HANDLE>,
    sector_size: u32,
}

// SAFETY: HANDLE is a raw pointer but we protect access with a Mutex
// and only perform read operations.
unsafe impl Send for WindowsDiskReader {}

impl WindowsDiskReader {
    pub fn open(path: &str) -> Result<Self> {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_NO_BUFFERING,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            bail!(
                "cannot open '{}': {}",
                path,
                std::io::Error::last_os_error()
            );
        }
        Ok(Self {
            handle: Mutex::new(handle),
            sector_size: 512,
        })
    }
}

impl Drop for WindowsDiskReader {
    fn drop(&mut self) {
        let mut null_handle: HANDLE = std::ptr::null_mut();
        let h = self.handle.get_mut().unwrap_or(&mut null_handle);
        if *h != INVALID_HANDLE_VALUE && !(*h).is_null() {
            unsafe { CloseHandle(*h) };
        }
    }
}

impl DiskReader for WindowsDiskReader {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let handle = *self.handle.lock().map_err(|e| anyhow::anyhow!("{e}"))?;

        // Sector-align the read for FILE_FLAG_NO_BUFFERING
        let ss = self.sector_size as u64;
        let aligned_offset = offset / ss * ss;
        let skip = (offset - aligned_offset) as usize;
        let aligned_len = (buf.len() + skip).div_ceil(ss as usize) * ss as usize;

        let mut aligned_buf = vec![0u8; aligned_len];

        // Seek
        let mut new_pos: i64 = 0;
        let ok = unsafe {
            SetFilePointerEx(handle, aligned_offset as i64, &mut new_pos, 0)
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error())
                .context("SetFilePointerEx failed");
        }

        // Read
        let mut bytes_read: u32 = 0;
        let ok = unsafe {
            ReadFile(
                handle,
                aligned_buf.as_mut_ptr().cast(),
                aligned_len as u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error()).context("ReadFile failed");
        }

        let available = (bytes_read as usize).saturating_sub(skip);
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&aligned_buf[skip..skip + to_copy]);
        Ok(to_copy)
    }

    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn size(&self) -> Option<u64> {
        None // device size unknown without extra ioctl
    }
}
