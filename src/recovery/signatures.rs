// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Francesco PC Desktop <francesco@diomabros.it>

/// Built-in file signature database for carving.

#[derive(Debug, Clone)]
pub struct FileSignature {
    pub name: &'static str,
    pub extension: &'static str,
    pub header: &'static [u8],
    /// Optional footer/trailer bytes (scan forward to find end of file).
    pub footer: Option<&'static [u8]>,
    /// Maximum file size to carve if no footer is found (bytes).
    pub max_size: u64,
}

/// Returns all built-in file signatures.
pub fn builtin_signatures() -> Vec<FileSignature> {
    vec![
        FileSignature {
            name: "JPEG",
            extension: "jpg",
            header: &[0xFF, 0xD8, 0xFF],
            footer: Some(&[0xFF, 0xD9]),
            max_size: 25 * 1024 * 1024, // 25 MB
        },
        FileSignature {
            name: "PNG",
            extension: "png",
            header: &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            footer: Some(&[0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]),
            max_size: 25 * 1024 * 1024,
        },
        FileSignature {
            name: "GIF87a",
            extension: "gif",
            header: &[0x47, 0x49, 0x46, 0x38, 0x37, 0x61],
            footer: Some(&[0x3B]),
            max_size: 10 * 1024 * 1024,
        },
        FileSignature {
            name: "GIF89a",
            extension: "gif",
            header: &[0x47, 0x49, 0x46, 0x38, 0x39, 0x61],
            footer: Some(&[0x3B]),
            max_size: 10 * 1024 * 1024,
        },
        FileSignature {
            name: "PDF",
            extension: "pdf",
            header: &[0x25, 0x50, 0x44, 0x46], // %PDF
            footer: Some(b"%%EOF"),
            max_size: 100 * 1024 * 1024,
        },
        FileSignature {
            name: "ZIP",
            extension: "zip",
            header: &[0x50, 0x4B, 0x03, 0x04],
            footer: None, // ZIP EOCD detection is complex; use max_size
            max_size: 100 * 1024 * 1024,
        },
        FileSignature {
            name: "BMP",
            extension: "bmp",
            header: &[0x42, 0x4D], // BM
            footer: None,
            max_size: 50 * 1024 * 1024,
        },
        FileSignature {
            name: "MP3 (sync)",
            extension: "mp3",
            header: &[0xFF, 0xFB],
            footer: None,
            max_size: 20 * 1024 * 1024,
        },
        FileSignature {
            name: "MP3 (ID3)",
            extension: "mp3",
            header: &[0x49, 0x44, 0x33], // ID3
            footer: None,
            max_size: 20 * 1024 * 1024,
        },
        FileSignature {
            name: "RAR v4",
            extension: "rar",
            header: &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00],
            footer: None,
            max_size: 100 * 1024 * 1024,
        },
        FileSignature {
            name: "RAR v5",
            extension: "rar",
            header: &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01],
            footer: None,
            max_size: 100 * 1024 * 1024,
        },
        FileSignature {
            name: "7Z",
            extension: "7z",
            header: &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C],
            footer: None,
            max_size: 100 * 1024 * 1024,
        },
        FileSignature {
            name: "TIFF (LE)",
            extension: "tiff",
            header: &[0x49, 0x49, 0x2A, 0x00],
            footer: None,
            max_size: 50 * 1024 * 1024,
        },
        FileSignature {
            name: "TIFF (BE)",
            extension: "tiff",
            header: &[0x4D, 0x4D, 0x00, 0x2A],
            footer: None,
            max_size: 50 * 1024 * 1024,
        },
    ]
}

/// Filter signatures by a set of desired type names / extensions.
pub fn filter_signatures(sigs: &[FileSignature], types: &[String]) -> Vec<FileSignature> {
    if types.is_empty() {
        return sigs.to_vec();
    }
    let lower: Vec<String> = types.iter().map(|s| s.to_lowercase()).collect();
    sigs.iter()
        .filter(|s| {
            lower.iter().any(|t| {
                s.extension.eq_ignore_ascii_case(t) || s.name.to_lowercase().contains(t.as_str())
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_count() {
        let sigs = builtin_signatures();
        assert!(sigs.len() >= 10);
    }

    #[test]
    fn filter_jpeg() {
        let sigs = builtin_signatures();
        let filtered = filter_signatures(&sigs, &["jpg".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "JPEG");
    }
}
