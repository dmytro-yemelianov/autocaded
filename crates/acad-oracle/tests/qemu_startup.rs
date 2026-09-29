#![cfg(unix)]

// Its own test binary: other QEMU tests in the same process would hold the
// session lock and add their run time to the measured startup failure.

use acad_oracle::session::Session;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn qemu_startup_failure_is_reported_at_once_with_its_reason() {
    if !acad_oracle::available() {
        eprintln!("skipping oracle: qemu-system-i386 absent");
        return;
    }
    let missing = Path::new("/tmp/acad-oracle-missing-floppy.img");
    let start = Instant::now();
    let error = match Session::boot_in_place(missing, None) {
        Ok(_) => panic!("booted a missing floppy"),
        Err(error) => error,
    };
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "took {:?}",
        start.elapsed()
    );
    assert!(error.contains("acad-oracle-missing-floppy.img"), "{error}");
}
