//! The canonical CHECKSUMS parser must never panic, and anything it accepts
//! must render back to the same bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ww_package::Checksums;

fuzz_target!(|data: &[u8]| {
    if let Ok(parsed) = Checksums::parse(data) {
        assert_eq!(parsed.render().as_bytes(), data);
    }
});
