//! Shared scaffolding for tests: unique temporary directories without a
//! tempfile dependency (process id + an atomic counter make collisions
//! impossible within a test run, and unlikely enough across them).

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Distinguishes directories created by concurrently running tests.
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Creates and returns a fresh temporary directory for one test, tagged
/// for identifiability. The caller removes it when done.
pub fn unique_temp_dir(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mathdoku-explorer-{tag}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).expect("the temporary directory should be creatable");
    path
}
