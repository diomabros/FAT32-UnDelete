# fat32-undelete

A cross-platform tool to recover deleted files from FAT32 partitions and disk images, written in Rust. Supports both a command-line interface and a native GUI built with [egui](https://github.com/emilk/egui).

## Features

- **Directory-entry scanning** – Parses FAT32 directory structures to find deleted entries and reconstructs files by following the FAT chain (or assuming contiguous allocation when the chain is broken).
- **Signature-based carving** – Scans unallocated clusters for known file headers/footers to recover files even when no directory entry remains.
- **Confidence reporting** – Each recovered file is assigned a confidence level (`HIGH`, `MEDIUM`, `CARVED`) so you can prioritize results.
- **Native GUI** – Built with eframe/egui; launch without arguments or with `--gui`.
- **Raw device access** – Reads directly from physical drives on Windows (`\\.\PhysicalDrive0`) and Linux/macOS (`/dev/sdb1`).
- **Disk image support** – Works with `.img` / `.dd` image files, with optional partition offset.
- **Size & type filters** – Limit recovery by file type, minimum/maximum size.
- **JSON report** – Automatically writes a machine-readable report alongside recovered files.
- **Internationalization** – Auto-detects system locale (English, Italian).

## Supported File Types (Carving)

JPEG, PNG, GIF, PDF, ZIP, BMP, MP3, RAR (v4/v5), 7Z, TIFF and more.

## Requirements

- **Rust** 2024 edition (1.85+)
- **Windows**: no extra dependencies (uses `windows-sys` for raw disk I/O)
- **Linux/macOS**: no extra dependencies (uses `libc` for raw disk I/O)

## Building

```bash
cargo build --release
```

The binary is placed in `target/release/fat32-undelete` (or `.exe` on Windows).

## Usage

### GUI

Launch the graphical interface (default when no source is specified):

```bash
fat32-undelete
fat32-undelete --gui
```

### CLI

```bash
fat32-undelete <SOURCE> [OPTIONS]
```

**Positional argument:**

| Argument | Description |
|----------|-------------|
| `SOURCE` | Path to a disk image (`.img`, `.dd`), device (`\\.\PhysicalDrive0`, `/dev/sdb1`), or drive letter (`E:`) |

**Options:**

| Flag | Description |
|------|-------------|
| `-o, --output <DIR>` | Output directory for recovered files (default: `recovered`) |
| `-m, --mode <MODE>` | Recovery mode: `scan`, `carve`, or `all` (default: `all`) |
| `-l, --list` | List recoverable files without extracting |
| `--types <LIST>` | Filter carved files by type (comma-separated, e.g. `jpeg,png,pdf`) |
| `--min-size <BYTES>` | Minimum file size to recover |
| `--max-size <BYTES>` | Maximum file size to recover |
| `--offset <BYTES>` | Partition offset in bytes (for raw disk images with MBR/GPT) |
| `--dry-run` | Scan and report without writing any files |
| `-v, --verbose` | Increase verbosity (`-v`, `-vv`, `-vvv`) |

### Examples

Recover all deleted files from a disk image:

```bash
fat32-undelete disk.img -o output/
```

List deleted files without extracting:

```bash
fat32-undelete disk.img --list
```

Carve only JPEG and PNG files from a physical drive (Linux):

```bash
sudo fat32-undelete /dev/sdb1 --mode carve --types jpeg,png
```

Recover from a raw disk image with a partition offset:

```bash
fat32-undelete full-disk.dd --offset 1048576
```

Dry run with verbose output:

```bash
fat32-undelete disk.img --dry-run -vv
```

## Project Structure

```
src/
├── main.rs              # CLI entry point and argument parsing
├── gui.rs               # eframe/egui graphical interface
├── i18n.rs              # Internationalization (English, Italian)
├── output.rs            # File writing, summary printing, JSON report
├── fat32/
│   ├── bpb.rs           # BIOS Parameter Block (boot sector) parser
│   ├── dir_entry.rs     # FAT32 directory entry parser
│   └── fat_table.rs     # FAT table loader and chain follower
├── io/
│   ├── file_reader.rs   # Image file reader
│   ├── win_reader.rs    # Windows raw disk reader (\\.\PhysicalDriveN)
│   └── unix_reader.rs   # Linux/macOS raw device reader (/dev/*)
└── recovery/
    ├── carver.rs         # Signature-based file carving engine
    ├── dir_scan.rs       # Deleted directory entry scanner
    └── signatures.rs     # Built-in file signature database
```

## License

Copyright (C) 2026 Francesco Diomaiuta

This program is free software: you can redistribute it and/or modify it under the terms of the **GNU General Public License** as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the [GNU General Public License](LICENSE) for more details.

You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
