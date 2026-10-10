//! Live scan progress shared between a scan source and the UI, plus the scan's error list.
//!
//! [`ScanProgress`] is written by whoever produces the tree (the `FsScanner` worker thread,
//! or the demo generator on the UI thread) and read by the scan screen every frame, so its
//! counters are atomics and the current path sits behind a `Mutex`. The cancel flag lives
//! here too: the UI sets it, the producer checks it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use web_time::Instant;

/// How many error paths a scan keeps; past this only the count grows.
pub const MAX_ERROR_PATHS: usize = 1000;

/// Counters, current path, start time and the done / cancel flags of one scan.
#[derive(Debug)]
pub struct ScanProgress {
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    /// About how many entries (files and folders) the scan will find; 0 when unknown.
    expected_entries: AtomicU64,
    current_path: Mutex<String>,
    started: Instant,
    done: AtomicBool,
    cancel: AtomicBool,
}

/// A consistent-enough copy of [`ScanProgress`] for one frame of the UI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub files: u64,
    pub dirs: u64,
    /// Allocated bytes seen so far.
    pub bytes: u64,
    pub errors: u64,
    /// About how many entries the scan will find (0: unknown), set by sources that know
    /// (the demo's profiles).
    pub expected_entries: u64,
    pub elapsed: Duration,
    pub current_path: String,
    pub done: bool,
}

impl ScanProgress {
    /// Fresh progress; the elapsed clock starts now.
    pub fn new() -> Self {
        Self {
            files: AtomicU64::new(0),
            dirs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            expected_entries: AtomicU64::new(0),
            current_path: Mutex::new(String::new()),
            started: Instant::now(),
            done: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
        }
    }

    /// Count one file of `allocated` bytes.
    pub fn add_file(&self, allocated: u64) {
        self.files.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(allocated, Ordering::Relaxed);
    }

    /// Count `count` files totalling `allocated` bytes at once.
    pub fn add_files(&self, count: u64, allocated: u64) {
        self.files.fetch_add(count, Ordering::Relaxed);
        self.bytes.fetch_add(allocated, Ordering::Relaxed);
    }

    pub fn add_dir(&self) {
        self.dirs.fetch_add(1, Ordering::Relaxed);
    }

    /// Say about how many entries the scan will find, for [`ProgressSnapshot::fraction`].
    pub fn set_expected_entries(&self, entries: u64) {
        self.expected_entries.store(entries, Ordering::Relaxed);
    }

    pub fn add_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_current_path(&self, path: &str) {
        let mut current = lock(&self.current_path);
        current.clear();
        current.push_str(path);
    }

    pub fn mark_done(&self) {
        self.done.store(true, Ordering::Release);
    }

    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    /// Ask the producer to stop as soon as it can.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            expected_entries: self.expected_entries.load(Ordering::Relaxed),
            elapsed: self.elapsed(),
            current_path: lock(&self.current_path).clone(),
            done: self.is_done(),
        }
    }
}

impl ProgressSnapshot {
    /// Estimated share of the scan done, in `0.0..=1.0`, given the bytes it should find
    /// (`expected_bytes`, a volume's used space; 0 when unknown). Bytes alone under-read
    /// while the scan is in folders of many small files and jump on a few huge ones, so
    /// when the source also knows its entry count the two shares are averaged. `None` when
    /// neither is known.
    pub fn fraction(&self, expected_bytes: u64) -> Option<f64> {
        let share = |done: u64, of: u64| (of > 0).then(|| (done as f64 / of as f64).min(1.0));
        let bytes = share(self.bytes, expected_bytes);
        let entries = share(self.files + self.dirs, self.expected_entries);
        match (bytes, entries) {
            (Some(b), Some(e)) => Some((b + e) / 2.0),
            (one, other) => one.or(other),
        }
    }
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// A poisoned lock only means another thread panicked mid-update of a display string; the
/// string is still usable.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One entry that could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanError {
    pub path: PathBuf,
    /// The OS's reason ("Permission denied (os error 13)").
    pub message: String,
}

/// The paths a scan could not read: the first [`MAX_ERROR_PATHS`] of them, and how many
/// there were in all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanErrors {
    entries: Vec<ScanError>,
    total: u64,
}

impl ScanErrors {
    pub fn record(&mut self, path: &Path, message: impl Into<String>) {
        self.total += 1;
        if self.entries.len() < MAX_ERROR_PATHS {
            self.entries.push(ScanError {
                path: path.to_path_buf(),
                message: message.into(),
            });
        }
    }

    /// The kept errors, in the order they happened.
    pub fn entries(&self) -> &[ScanError] {
        &self.entries
    }

    /// Every error, including those past the kept list.
    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// After a rescan of `folder`: drop the kept errors inside it and add `fresh` (the
    /// rescan's errors). Errors past the kept list cannot be told apart by path, so the
    /// total drops only by the kept ones removed.
    pub fn replace_under(&mut self, folder: &Path, fresh: &ScanErrors) {
        let before = self.entries.len();
        self.entries.retain(|e| !e.path.starts_with(folder));
        let removed = (before - self.entries.len()) as u64;
        self.total = self.total.saturating_sub(removed);
        for e in &fresh.entries {
            self.record(&e.path, e.message.clone());
        }
        // Errors the rescan counted but did not keep.
        let unlisted = fresh.total.saturating_sub(fresh.entries.len() as u64);
        self.total = self.total.saturating_add(unlisted);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_fraction_blends_bytes_and_entries() {
        let mut s = super::ProgressSnapshot {
            files: 90,
            dirs: 10,
            bytes: 800,
            ..Default::default()
        };
        assert_eq!(s.fraction(0), None);
        assert_eq!(s.fraction(1000), Some(0.8));
        s.expected_entries = 400;
        assert_eq!(s.fraction(0), Some(0.25));
        assert_eq!(s.fraction(1000), Some((0.8 + 0.25) / 2.0));
        // Never past 1, however far the estimates are off.
        s.bytes = 5000;
        s.expected_entries = 10;
        assert_eq!(s.fraction(1000), Some(1.0));
    }

    use super::*;

    #[test]
    fn counters_and_flags_show_in_snapshots() {
        let p = ScanProgress::new();
        p.add_file(4096);
        p.add_files(2, 100);
        p.add_dir();
        p.add_error();
        p.set_current_path("/a/b");
        let s = p.snapshot();
        assert_eq!((s.files, s.dirs, s.bytes, s.errors), (3, 1, 4196, 1));
        assert_eq!(s.current_path, "/a/b");
        assert!(!s.done && !p.is_cancelled());
        p.cancel();
        p.mark_done();
        assert!(p.is_cancelled() && p.snapshot().done);
    }

    #[test]
    fn a_rescan_replaces_the_errors_inside_its_folder() {
        let mut errors = ScanErrors::default();
        errors.record(Path::new("/r/a/x"), "denied");
        errors.record(Path::new("/r/b/y"), "denied");
        let mut fresh = ScanErrors::default();
        fresh.record(Path::new("/r/a/z"), "gone");
        errors.replace_under(Path::new("/r/a"), &fresh);
        let paths: Vec<_> = errors.entries().iter().map(|e| e.path.clone()).collect();
        assert_eq!(
            paths,
            vec![PathBuf::from("/r/b/y"), PathBuf::from("/r/a/z")]
        );
        assert_eq!(errors.total(), 2);
    }

    #[test]
    fn error_list_is_capped_but_counts_everything() {
        let mut errors = ScanErrors::default();
        assert!(errors.is_empty());
        for i in 0..(MAX_ERROR_PATHS + 5) {
            errors.record(Path::new(&format!("/e{i}")), "denied");
        }
        assert_eq!(errors.entries().len(), MAX_ERROR_PATHS);
        assert_eq!(errors.total(), MAX_ERROR_PATHS as u64 + 5);
        assert_eq!(errors.entries()[0].path, PathBuf::from("/e0"));
        assert_eq!(errors.entries()[0].message, "denied");
    }
}
