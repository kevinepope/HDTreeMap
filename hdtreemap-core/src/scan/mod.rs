//! Scanning: how a [`Tree`] is produced (`docs/PLAN.md` §3.2).
//!
//! A scan is a [`ScanSource`]. The UI starts one through `PlatformServices::scan_source`,
//! then polls it once per frame until it hands back a [`ScanResult`]; meanwhile it shows the
//! source's shared [`ScanProgress`]. Two sources feed the same `TreeBuilder`, so the UI
//! cannot tell them apart:
//!
//! - [`FsScanner`] (native only): walks the real disk with jwalk on a worker thread.
//! - `demo_fs::DemoScanner` (all targets): streams a synthetic tree, a time-sliced chunk per
//!   poll, so single-threaded wasm stays responsive.

#[cfg(not(target_arch = "wasm32"))]
mod fs;
#[cfg(not(target_arch = "wasm32"))]
mod fs_meta;
mod progress;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub use fs::{scan_blocking, FsScanner, NOTIFY_INTERVAL};
pub use progress::{ProgressSnapshot, ScanError, ScanErrors, ScanProgress, MAX_ERROR_PATHS};

use crate::model::Tree;

/// A finished scan.
#[derive(Clone, Debug)]
pub struct ScanResult {
    /// What was scanned (the tree's root name).
    pub root: PathBuf,
    /// The tree, children sorted by allocated size.
    pub tree: Tree,
    /// Entries that could not be read.
    pub errors: ScanErrors,
    /// Wall time from start to finish.
    pub elapsed: Duration,
}

/// What a poll found.
#[derive(Debug)]
pub enum ScanPoll {
    /// Still working; read [`ScanSource::progress`] for how far it got.
    Running,
    /// Done. Each source returns its result exactly once.
    Finished(Box<ScanResult>),
}

/// A running scan. See the module docs.
pub trait ScanSource {
    /// The live counters, current path and cancel flag.
    fn progress(&self) -> Arc<ScanProgress>;

    /// Advance the scan, spending at most about `budget` of the calling (UI) thread's time,
    /// and report whether it has finished. A source that works on its own thread only
    /// checks for its result.
    fn poll(&mut self, budget: Duration) -> ScanPoll;

    /// Stop the scan. Dropping a source cancels it too.
    fn cancel(&self) {
        self.progress().cancel();
    }
}
