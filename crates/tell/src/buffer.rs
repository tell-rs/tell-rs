//! Disk-backed WAL (Write-Ahead Log) for buffering encoded batches on TCP failure.
//!
//! The WAL uses a single append-only file with length-prefixed frames:
//! `[4 bytes LE: frame_len][frame_len bytes: batch]...`
//!
//! A companion cursor file tracks how many bytes have been consumed.
//! On startup, any unconsumed frames from a previous run are drained first.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Default maximum buffer size: 3 GiB.
///
/// Sized to hold hours of metrics during a sustained backend outage.
/// At ~80KB per flush (500 metrics) every 15s, 3 GiB holds ~6.5 days of data.
pub(crate) const DEFAULT_BUFFER_MAX_BYTES: u64 = 3 * 1024 * 1024 * 1024;

/// Frame header size: 4 bytes little-endian length prefix.
const FRAME_HEADER_LEN: usize = 4;

/// Compact when consumed bytes exceed half the file size.
const COMPACT_RATIO: f64 = 0.5;

/// Disk-backed WAL buffer for persisting failed batch sends.
///
/// Only touched on the failure path — successful sends never interact with disk.
pub(crate) struct DiskBuffer {
    wal_path: PathBuf,
    cursor_path: PathBuf,
    max_bytes: u64,
    /// Current write position (end of WAL file).
    write_pos: u64,
    /// Current read/cursor position (bytes already consumed).
    read_pos: u64,
    /// WAL file handle kept open for appending.
    wal_file: File,
}

// Safety: DiskBuffer only contains PathBuf, u64, and File — all Send.
// File is Send in std. This is used from the single-threaded worker task.

impl DiskBuffer {
    /// Open or create a disk buffer at the given directory.
    ///
    /// Creates `buffer.wal` and `buffer.cursor` inside `dir`.
    /// Recovers read/write positions from existing files on restart.
    pub(crate) fn open(dir: &Path, max_bytes: u64) -> io::Result<Self> {
        fs::create_dir_all(dir)?;

        // Set directory permissions to owner-only (0700) on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }

        let wal_path = dir.join("buffer.wal");
        let cursor_path = dir.join("buffer.cursor");

        // Reject symlinks to prevent symlink attacks (agent runs as root).
        if wal_path.exists() && fs::symlink_metadata(&wal_path)?.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "WAL path is a symlink — refusing to open (possible symlink attack)",
            ));
        }

        let wal_file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&wal_path)?;

        // Set WAL file permissions to owner-only (0600) on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            wal_file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }

        let write_pos = wal_file.metadata()?.len();
        let read_pos = read_cursor(&cursor_path)?;

        let mut buf = Self {
            wal_path,
            cursor_path,
            max_bytes,
            write_pos,
            read_pos,
            wal_file,
        };

        // Clamp read_pos if it exceeds write_pos (corrupt/stale cursor)
        if buf.read_pos > buf.write_pos {
            buf.read_pos = buf.write_pos;
            buf.persist_cursor()?;
        }

        Ok(buf)
    }

    /// Append encoded batch bytes as a length-prefixed frame.
    ///
    /// Evicts oldest frames (advances read cursor) if appending would exceed `max_bytes`.
    /// Returns the number of bytes evicted (0 if no eviction was needed).
    pub(crate) fn append(&mut self, batch: &[u8]) -> io::Result<u64> {
        let frame_size = FRAME_HEADER_LEN as u64 + batch.len() as u64;

        // Evict oldest frames to make room if necessary
        let evicted = self.evict_if_needed(frame_size)?;

        let len = batch.len() as u32;
        self.wal_file.write_all(&len.to_le_bytes())?;
        self.wal_file.write_all(batch)?;
        self.wal_file.flush()?;
        self.write_pos += frame_size;

        Ok(evicted)
    }

    /// Read and return the next unconsumed frame, advancing the cursor.
    ///
    /// Returns `None` if the buffer is fully drained.
    pub(crate) fn drain_next(&mut self) -> io::Result<Option<Vec<u8>>> {
        if self.read_pos >= self.write_pos {
            return Ok(None);
        }

        let mut reader = open_reader(&self.wal_path, self.read_pos)?;

        let frame_len = match read_frame_header(&mut reader) {
            Ok(len) => len,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                // Truncated header — reset to end
                self.read_pos = self.write_pos;
                self.persist_cursor()?;
                return Ok(None);
            }
            Err(e) => return Err(e),
        };

        let mut frame = vec![0u8; frame_len as usize];
        match reader.read_exact(&mut frame) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                // Truncated frame — reset to end
                self.read_pos = self.write_pos;
                self.persist_cursor()?;
                return Ok(None);
            }
            Err(e) => return Err(e),
        }

        self.read_pos += FRAME_HEADER_LEN as u64 + frame_len as u64;
        self.persist_cursor()?;

        self.maybe_compact()?;

        Ok(Some(frame))
    }

    /// Whether all frames have been consumed.
    pub(crate) fn is_empty(&self) -> bool {
        self.read_pos >= self.write_pos
    }

    /// Compact the WAL by rewriting only unconsumed data to a new file.
    ///
    /// Called automatically when consumed bytes exceed half the file.
    pub(crate) fn compact(&mut self) -> io::Result<()> {
        if self.read_pos == 0 {
            return Ok(());
        }

        let remaining = self.write_pos - self.read_pos;

        if remaining == 0 {
            return self.truncate_all();
        }

        let tmp_path = self.wal_path.with_extension("wal.tmp");
        rewrite_tail(&self.wal_path, self.read_pos, remaining, &tmp_path)?;

        // Drop current handle before replacing file
        drop(std::mem::replace(
            &mut self.wal_file,
            File::open("/dev/null").map_err(|e| {
                io::Error::new(e.kind(), "failed to open placeholder during compact")
            })?,
        ));

        fs::rename(&tmp_path, &self.wal_path)?;

        self.wal_file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&self.wal_path)?;

        self.write_pos = remaining;
        self.read_pos = 0;
        self.persist_cursor()?;

        Ok(())
    }

    /// Total bytes currently buffered on disk (unconsumed portion).
    #[allow(dead_code)]
    pub(crate) fn buffered_bytes(&self) -> u64 {
        self.write_pos.saturating_sub(self.read_pos)
    }

    /// Write the read cursor to the cursor file atomically.
    fn persist_cursor(&self) -> io::Result<()> {
        write_cursor(&self.cursor_path, self.read_pos)
    }

    /// Trigger compaction when consumed bytes exceed the threshold.
    fn maybe_compact(&mut self) -> io::Result<()> {
        if self.write_pos == 0 {
            return Ok(());
        }
        let consumed_ratio = self.read_pos as f64 / self.write_pos as f64;
        if consumed_ratio > COMPACT_RATIO {
            self.compact()?;
        }
        Ok(())
    }

    /// Evict oldest frames by advancing the read cursor until enough space is free.
    /// Returns the number of bytes evicted (0 if no eviction needed).
    fn evict_if_needed(&mut self, needed: u64) -> io::Result<u64> {
        let current_size = self.write_pos - self.read_pos;
        if current_size + needed <= self.max_bytes {
            return Ok(0);
        }

        let must_free = (current_size + needed) - self.max_bytes;
        let mut freed: u64 = 0;
        let mut reader = open_reader(&self.wal_path, self.read_pos)?;

        while freed < must_free {
            let frame_len = match read_frame_header(&mut reader) {
                Ok(len) => len,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            };
            let frame_total = FRAME_HEADER_LEN as u64 + frame_len as u64;
            // Skip the frame body
            reader.seek(SeekFrom::Current(frame_len as i64))?;
            freed += frame_total;
        }

        self.read_pos += freed;
        self.persist_cursor()?;

        // Compact after eviction since we likely skipped a lot
        self.maybe_compact()?;

        Ok(freed)
    }

    /// Reset both positions and truncate the WAL file to zero.
    fn truncate_all(&mut self) -> io::Result<()> {
        // Drop current handle
        drop(std::mem::replace(
            &mut self.wal_file,
            File::open("/dev/null").map_err(|e| {
                io::Error::new(e.kind(), "failed to open placeholder during truncate")
            })?,
        ));

        // Recreate empty
        self.wal_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&self.wal_path)?;

        self.write_pos = 0;
        self.read_pos = 0;
        self.persist_cursor()?;

        Ok(())
    }
}

/// Read the cursor value from the cursor file. Returns 0 if the file doesn't exist.
fn read_cursor(path: &Path) -> io::Result<u64> {
    match fs::read_to_string(path) {
        Ok(contents) => contents
            .trim()
            .parse::<u64>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    }
}

/// Write the cursor value atomically (write-tmp-then-rename).
fn write_cursor(path: &Path, pos: u64) -> io::Result<()> {
    let tmp = path.with_extension("cursor.tmp");
    fs::write(&tmp, pos.to_string())?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Open a `BufReader` over the WAL file, seeked to `offset`.
fn open_reader(wal_path: &Path, offset: u64) -> io::Result<BufReader<File>> {
    let mut f = File::open(wal_path)?;
    f.seek(SeekFrom::Start(offset))?;
    Ok(BufReader::new(f))
}

/// Read a 4-byte LE frame header, returning the frame body length.
fn read_frame_header(reader: &mut impl Read) -> io::Result<u32> {
    let mut header = [0u8; FRAME_HEADER_LEN];
    reader.read_exact(&mut header)?;
    Ok(u32::from_le_bytes(header))
}

/// Copy the tail of `src` (from `offset`, `len` bytes) into `dst`.
fn rewrite_tail(src: &Path, offset: u64, len: u64, dst: &Path) -> io::Result<()> {
    let mut reader = open_reader(src, offset)?;
    let mut writer = File::create(dst)?;
    let copied = io::copy(&mut reader.by_ref().take(len), &mut writer)?;
    if copied != len {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "WAL file shorter than expected during compaction",
        ));
    }
    writer.flush()?;
    Ok(())
}
