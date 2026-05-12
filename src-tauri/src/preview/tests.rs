//! W4a preview tests.

use super::resolver::read_capped;
use super::types::MAX_PREVIEW_BYTES;
use std::fs::write;
use tempfile::TempDir;

// Path traversal + boundary tests (don't require AppState — resolver tests
// that need real mounts live in the manual smoke section of the PR plan).

#[test]
fn read_capped_returns_full_file_under_cap() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("small.txt");
    write(&path, b"hello world").unwrap();

    let result = read_capped(&path).unwrap();
    assert_eq!(result.bytes, b"hello world");
    assert_eq!(result.size, 11);
    assert!(!result.truncated);
    assert!(result.mtime_ms > 0);
}

#[test]
fn read_capped_truncates_oversized_file() {
    // Synthesize a file slightly larger than the cap by writing a known
    // pattern. Use the smallest possible size so the test is fast.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("big.bin");
    // 1 KB above the cap — enough to verify truncation behavior without
    // allocating 50 MB in the test.
    let oversize = (MAX_PREVIEW_BYTES + 1024) as usize;
    let payload = vec![b'x'; oversize];
    write(&path, &payload).unwrap();

    let result = read_capped(&path).unwrap();
    assert_eq!(result.size as usize, oversize);
    assert!(result.truncated, "should mark as truncated");
    assert_eq!(
        result.bytes.len() as u64,
        MAX_PREVIEW_BYTES,
        "bytes must equal exactly MAX_PREVIEW_BYTES"
    );
    assert!(result.bytes.iter().all(|&b| b == b'x'));
}

#[test]
fn read_capped_rejects_nonexistent_path() {
    let result = read_capped(std::path::Path::new("/definitely/does/not/exist/xyz"));
    assert!(result.is_err());
}

#[test]
fn read_capped_rejects_directory() {
    let tmp = TempDir::new().unwrap();
    let result = read_capped(tmp.path());
    assert!(result.is_err(), "directories should be rejected");
}

// Note: resolve_path requires AppState which has heavy construction
// requirements (Tauri AppHandle, etc.). The traversal guards are exercised
// up-front in the function itself and are covered by the manual smoke test
// in the PR plan. The pure-Rust read_capped tests above cover the boundary
// behavior unit-tests should pin.
