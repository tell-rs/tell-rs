use std::fs;

use crate::buffer::DiskBuffer;

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tell-buffer-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn empty_buffer() {
    let dir = tmp_dir("empty");
    let buf = DiskBuffer::open(&dir, 1024).unwrap();
    assert!(buf.is_empty());
    assert_eq!(buf.buffered_bytes(), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn append_and_drain_single_frame() {
    let dir = tmp_dir("single");
    let mut buf = DiskBuffer::open(&dir, 1024).unwrap();

    let data = b"hello world";
    buf.append(data).unwrap();
    assert!(!buf.is_empty());
    assert_eq!(buf.buffered_bytes(), 4 + data.len() as u64);

    let frame = buf.drain_next().unwrap().unwrap();
    assert_eq!(frame, data);
    assert!(buf.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn append_and_drain_multiple_frames() {
    let dir = tmp_dir("multi");
    let mut buf = DiskBuffer::open(&dir, 4096).unwrap();

    buf.append(b"one").unwrap();
    buf.append(b"two").unwrap();
    buf.append(b"three").unwrap();

    assert_eq!(buf.drain_next().unwrap().unwrap(), b"one");
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"two");
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"three");
    assert!(buf.drain_next().unwrap().is_none());
    assert!(buf.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn survives_reopen() {
    let dir = tmp_dir("reopen");

    // Write two frames, drain one
    {
        let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
        buf.append(b"first").unwrap();
        buf.append(b"second").unwrap();
        assert_eq!(buf.drain_next().unwrap().unwrap(), b"first");
        // drop — simulates process exit
    }

    // Reopen — should resume from the cursor
    {
        let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
        assert!(!buf.is_empty());
        assert_eq!(buf.drain_next().unwrap().unwrap(), b"second");
        assert!(buf.is_empty());
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn evicts_oldest_when_full() {
    let dir = tmp_dir("evict");
    // Max 50 bytes: each frame is 4 (header) + 10 (body) = 14 bytes, so 3 frames = 42, 4 = 56 > 50
    let mut buf = DiskBuffer::open(&dir, 50).unwrap();

    buf.append(&[0u8; 10]).unwrap(); // frame 1: 14 bytes, total 14
    buf.append(&[1u8; 10]).unwrap(); // frame 2: 14 bytes, total 28
    buf.append(&[2u8; 10]).unwrap(); // frame 3: 14 bytes, total 42

    // This should evict frame 1 to make room
    buf.append(&[3u8; 10]).unwrap(); // frame 4: needs 14, total would be 56 > 50

    // The oldest surviving frame should be frame 2 or 3 (frame 1 evicted)
    let frame = buf.drain_next().unwrap().unwrap();
    // After eviction + compaction, the exact frame depends on how many were evicted.
    // At least frame 1 ([0u8; 10]) should be gone.
    assert_ne!(frame, [0u8; 10]);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn compact_reclaims_space() {
    let dir = tmp_dir("compact");
    let mut buf = DiskBuffer::open(&dir, 4096).unwrap();

    // Append several frames
    for i in 0..10u8 {
        buf.append(&[i; 20]).unwrap();
    }

    let wal_size_before = fs::metadata(dir.join("buffer.wal")).unwrap().len();

    // Drain all — triggers compaction
    for _ in 0..10 {
        buf.drain_next().unwrap().unwrap();
    }
    assert!(buf.is_empty());

    let wal_size_after = fs::metadata(dir.join("buffer.wal")).unwrap().len();
    assert!(
        wal_size_after < wal_size_before,
        "WAL should shrink after compaction"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cursor_file_is_valid() {
    let dir = tmp_dir("cursor-valid");
    let mut buf = DiskBuffer::open(&dir, 4096).unwrap();

    buf.append(b"first").unwrap();
    buf.append(b"second").unwrap();
    // Drain one — cursor advances but compaction may or may not trigger
    buf.drain_next().unwrap();

    // Cursor file should exist and be parseable
    let cursor_content = fs::read_to_string(dir.join("buffer.cursor")).unwrap();
    let _cursor_val: u64 = cursor_content
        .trim()
        .parse()
        .expect("cursor should be valid u64");

    // No .tmp file should be left behind
    assert!(!dir.join("buffer.cursor.tmp").exists());

    // Second frame should still be drainable
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"second");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn handles_corrupt_cursor() {
    let dir = tmp_dir("corrupt-cursor");

    // Write a frame
    {
        let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
        buf.append(b"payload").unwrap();
    }

    // Corrupt the cursor file with a value beyond the WAL
    fs::write(dir.join("buffer.cursor"), "999999").unwrap();

    // Reopen should clamp cursor to write_pos
    let buf = DiskBuffer::open(&dir, 4096).unwrap();
    assert!(buf.is_empty()); // clamped cursor = write_pos → empty

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn handles_missing_cursor_file() {
    let dir = tmp_dir("no-cursor");

    // Write frames
    {
        let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
        buf.append(b"a").unwrap();
        buf.append(b"b").unwrap();
    }

    // Delete cursor file — simulates crash before cursor was written
    let _ = fs::remove_file(dir.join("buffer.cursor"));

    // Reopen — cursor defaults to 0, all frames should be available
    let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
    assert!(!buf.is_empty());
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"a");
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"b");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_append_works() {
    let dir = tmp_dir("empty-frame");
    let mut buf = DiskBuffer::open(&dir, 4096).unwrap();

    buf.append(b"").unwrap();
    let frame = buf.drain_next().unwrap().unwrap();
    assert!(frame.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn large_frame() {
    let dir = tmp_dir("large");
    let data = vec![0xABu8; 100_000];
    let mut buf = DiskBuffer::open(&dir, 200_000).unwrap();

    buf.append(&data).unwrap();
    let frame = buf.drain_next().unwrap().unwrap();
    assert_eq!(frame, data);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn interleaved_append_drain() {
    let dir = tmp_dir("interleave");
    let mut buf = DiskBuffer::open(&dir, 4096).unwrap();

    buf.append(b"a").unwrap();
    buf.append(b"b").unwrap();
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"a");

    buf.append(b"c").unwrap();
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"b");
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"c");
    assert!(buf.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn truncated_header_handled() {
    let dir = tmp_dir("trunc-header");
    let wal_path = dir.join("buffer.wal");
    fs::create_dir_all(&dir).unwrap();

    // Write a valid frame then append 2 garbage bytes (incomplete header)
    {
        let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
        buf.append(b"valid").unwrap();
    }
    // Append 2 bytes directly — a truncated header
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&wal_path).unwrap();
        f.write_all(&[0xFF, 0xFF]).unwrap();
    }
    // Bump write_pos by writing a new cursor that doesn't account for garbage
    // Actually, just reopen — write_pos comes from file length
    let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"valid");
    // Next drain hits truncated header — should return None, not error
    assert!(buf.drain_next().unwrap().is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn truncated_body_handled() {
    let dir = tmp_dir("trunc-body");
    let wal_path = dir.join("buffer.wal");
    fs::create_dir_all(&dir).unwrap();

    {
        let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
        buf.append(b"good").unwrap();
    }
    // Write a header claiming 100 bytes, but only write 3 bytes of body
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&wal_path).unwrap();
        f.write_all(&100u32.to_le_bytes()).unwrap();
        f.write_all(&[1, 2, 3]).unwrap();
    }

    let mut buf = DiskBuffer::open(&dir, 4096).unwrap();
    assert_eq!(buf.drain_next().unwrap().unwrap(), b"good");
    // Next drain hits truncated body — should return None
    assert!(buf.drain_next().unwrap().is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn symlink_wal_rejected() {
    let dir = tmp_dir("symlink");
    fs::create_dir_all(&dir).unwrap();

    // Create a real file to be the symlink target (so .exists() returns true)
    let target = dir.join("real.wal");
    fs::write(&target, b"").unwrap();

    // Create a symlink at buffer.wal pointing to the real file
    let wal_path = dir.join("buffer.wal");
    std::os::unix::fs::symlink(&target, &wal_path).unwrap();

    let result = DiskBuffer::open(&dir, 4096);
    assert!(result.is_err());

    let _ = fs::remove_dir_all(&dir);
}
