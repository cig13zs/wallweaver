//! Feed arbitrary bytes to the package reader. It must never panic, never
//! write outside the destination, and never leave a staging folder behind.
#![no_main]

use std::io::Cursor;

use libfuzzer_sys::fuzz_target;
use ww_package::{VerifyOptions, unpack, verify};

fuzz_target!(|data: &[u8]| {
    let _ = verify(Cursor::new(data), &VerifyOptions::developer());

    let Ok(dir) = tempfile::tempdir() else { return };
    let dest = dir.path().join("pkg");
    let result = unpack(Cursor::new(data), &dest, &VerifyOptions::developer());
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .map(|it| it.filter_map(Result::ok).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    match result {
        Ok(_) => assert_eq!(entries, vec![std::ffi::OsString::from("pkg")]),
        Err(_) => assert!(entries.is_empty(), "left behind: {entries:?}"),
    }
});
