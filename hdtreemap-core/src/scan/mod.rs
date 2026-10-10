//! Scanning: how a [`Tree`] is produced (`docs/PLAN.md` §3.2).
//!
//! A scan is a [`ScanSource`]. The UI starts one through `PlatformServices::scan_source`,
//! then polls it once per frame until it hands back a [`ScanResult`]; meanwhile it shows the
//! source's shared [`ScanProgress`] and the tree scanned so far ([`ScanSource::partial`],
//! the live map). Two sources feed the same `TreeBuilder` on the polling (UI) thread, so
//! the UI cannot tell them apart:
//!
//! - [`FsScanner`] (native only): walks the real disk with jwalk on a worker thread
//!   (`walk.rs`) and sends batches of records that the poll assembles (`assemble.rs`).
//! - `demo_fs::DemoScanner` (all targets): streams a synthetic tree, a time-sliced chunk per
//!   poll, so single-threaded wasm stays responsive.

#[cfg(target_os = "macos")]
mod apfs;
#[cfg(not(target_arch = "wasm32"))]
mod assemble;
#[cfg(not(target_arch = "wasm32"))]
mod fs;
#[cfg(not(target_arch = "wasm32"))]
mod fs_meta;
mod progress;
#[cfg(not(target_arch = "wasm32"))]
mod walk;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub use fs::{scan_blocking, FsScanner, BATCH_INTERVAL, BATCH_RECORDS};
pub use progress::{ProgressSnapshot, ScanError, ScanErrors, ScanProgress, MAX_ERROR_PATHS};
#[cfg(not(target_arch = "wasm32"))]
pub use walk::NOTIFY_INTERVAL;

use crate::model::{SizeMode, Tree};

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

/// A running scan's tree so far: correct partial totals, children not yet sorted.
#[derive(Clone, Copy, Debug)]
pub struct PartialScan<'a> {
    /// What is being scanned (the tree's root name).
    pub root: &'a Path,
    pub tree: &'a Tree,
    /// Entries that could not be read so far.
    pub errors: &'a ScanErrors,
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

    /// The tree scanned so far, for the live map; `None` until the source has one (and
    /// always, for a source that shows nothing before it is done). The ids of its nodes
    /// stay valid in the finished tree.
    fn partial(&self) -> Option<PartialScan<'_>> {
        None
    }

    /// The size the partial tree reads and the finished tree is sorted by (the user's size
    /// mode). A source with no partial tree ignores it; its result is re-sorted instead.
    fn set_size_mode(&mut self, _mode: SizeMode) {}

    /// Stop the scan. Dropping a source cancels it too.
    fn cancel(&self) {
        self.progress().cancel();
    }
}
