//! `HDTREEMAP_STATS` for the filesystem walk (`scan/walk.rs`): where a scan spends its
//! time. Counts how each entry's metadata was read (a bulk folder read on macOS, `lstat`,
//! the extra clone lookup) and keeps the folders whose entries took longest to read, then
//! prints them when the walk ends. Off unless the variable is set; then the only cost is
//! a clock read per folder and a few relaxed atomic adds.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::render::stats_enabled;

/// How many of the slowest folders are kept.
const SLOWEST: usize = 15;

/// See the module docs.
#[derive(Default)]
pub(super) struct WalkStats {
    enabled: bool,
    pub folders: AtomicU64,
    pub bulk_entries: AtomicU64,
    pub lstat_entries: AtomicU64,
    pub clone_lookups: AtomicU64,
    /// Total time reading entries' metadata, in microseconds (summed over threads).
    pub micros: AtomicU64,
    slowest: Mutex<Vec<(Duration, PathBuf, usize)>>,
}

impl WalkStats {
    pub fn new() -> Self {
        Self {
            enabled: stats_enabled(),
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }

    /// One folder's `entries` took `took` to read.
    pub fn folder(&self, dir: &Path, entries: usize, took: Duration) {
        Self::add(&self.folders, 1);
        Self::add(&self.micros, took.as_micros() as u64);
        let mut slowest = self
            .slowest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slowest.len() < SLOWEST || slowest.last().is_some_and(|(t, _, _)| took > *t) {
            slowest.push((took, dir.to_path_buf(), entries));
            slowest.sort_by_key(|entry| std::cmp::Reverse(entry.0));
            slowest.truncate(SLOWEST);
        }
    }

    /// Print the report (stderr).
    pub fn report(&self, elapsed: Duration) {
        let get = |c: &AtomicU64| c.load(Ordering::Relaxed);
        crate::render::stats_line(&format!(
            "walk {:.1} s | {} folders | metadata: {} bulk, {} lstat, {} clone \
             lookups | {:.1} s reading (all threads)",
            elapsed.as_secs_f64(),
            get(&self.folders),
            get(&self.bulk_entries),
            get(&self.lstat_entries),
            get(&self.clone_lookups),
            get(&self.micros) as f64 / 1e6
        ));
        let slowest = self
            .slowest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (took, dir, entries) in slowest.iter() {
            crate::render::stats_line(&format!(
                "  {:>8.1} ms  {:>7} entries  {}",
                took.as_secs_f64() * 1000.0,
                entries,
                dir.display()
            ));
        }
    }
}
