//! `FsScanner`: the native scan source. Walks a real folder with jwalk on its own rayon
//! pool, on a worker thread, and feeds a `TreeBuilder` (`docs/PLAN.md` §3.2).
//!
//! jwalk reads directories in parallel and calls `process_read_dir` on the pool's threads
//! for each batch of entries; that is where the expensive per-entry work happens (`lstat`,
//! the allocated size, the device check that keeps the scan on one filesystem, hard-link
//! de-duplication, skipping duplicate mounts). The walk then yields entries in depth-first
//! order on the worker thread, which builds the tree with a stack of open directories.
//!
//! Progress goes to the shared [`ScanProgress`]; at most every [`NOTIFY_INTERVAL`] the
//! worker publishes the current path and wakes the UI through its `UiQueue`. The finished
//! [`ScanResult`] comes back over a channel that [`ScanSource::poll`] checks. Symlinks are
//! never followed; unreadable entries are flagged, counted and listed, never fatal.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agg_gui::ui_thread::UiQueue;
use jwalk::{Parallelism, WalkDirGeneric};
use web_time::Instant;

use super::fs_meta;
use super::{ScanErrors, ScanPoll, ScanProgress, ScanResult, ScanSource};
use crate::model::{NodeFlags, NodeId, SizeMode, Sizes, TreeBuilder};

/// Longest gap between progress publications (current path + UI wake-up).
pub const NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

/// What `process_read_dir` learned about one entry, carried to the tree-building thread.
#[derive(Debug, Default)]
struct EntryInfo {
    sizes: Sizes,
    flags: NodeFlags,
    /// Why the entry's metadata could not be read.
    error: Option<String>,
    /// A second hard link to a file already counted; dropped from the walk.
    duplicate: bool,
}

type Walk = WalkDirGeneric<((), EntryInfo)>;

/// Shared state of the per-entry filter (runs on jwalk's pool threads).
struct Filter {
    /// Devices that count as the scanned filesystem; empty means "don't check".
    devices: Vec<u64>,
    /// Directories to list but not enter.
    skip: Vec<PathBuf>,
    /// `(device, inode)` of multiply-linked files already counted.
    seen_links: Mutex<HashSet<(u64, u64)>>,
    progress: Arc<ScanProgress>,
}

/// A scan of a real folder running on a worker thread. Dropping it cancels the scan.
pub struct FsScanner {
    progress: Arc<ScanProgress>,
    result: Receiver<ScanResult>,
    root: PathBuf,
    finished: bool,
}

impl FsScanner {
    /// Start scanning `root` on a new thread. `notify` (the UI thread's queue) is woken as
    /// progress is published and when the scan finishes.
    pub fn start(root: PathBuf, notify: Option<UiQueue>) -> Self {
        let progress = Arc::new(ScanProgress::new());
        let (tx, rx) = mpsc::channel();
        let worker_progress = Arc::clone(&progress);
        let worker_root = root.clone();
        let spawned = std::thread::Builder::new()
            .name("hdtreemap-scan".into())
            .spawn(move || {
                let result = scan_blocking(&worker_root, &worker_progress, notify.as_ref());
                // The receiver is gone only when the scan was dropped (cancelled).
                let _ = tx.send(result);
                if let Some(queue) = &notify {
                    queue.signal_async_state_change();
                }
            });
        let result = match spawned {
            Ok(_) => rx,
            Err(err) => {
                // No worker: the result is known now. `tx` died with the closure, so hand
                // the failure over through a fresh, pre-loaded channel.
                let (tx, rx) = mpsc::channel();
                let message = format!("Could not start the scan: {err}");
                let _ = tx.send(failed_result(&root, message, &progress));
                rx
            }
        };
        Self {
            progress,
            result,
            root,
            finished: false,
        }
    }
}

/// A result for a scan that could not run: the root alone, flagged, with `message` as its
/// one error.
fn failed_result(root: &Path, message: String, progress: &ScanProgress) -> ScanResult {
    let mut errors = ScanErrors::default();
    errors.record(root, message);
    progress.add_error();
    progress.mark_done();
    let mut builder = TreeBuilder::new(&root.to_string_lossy());
    builder.mark(builder.root(), NodeFlags::ERROR);
    ScanResult {
        root: root.to_path_buf(),
        tree: builder.finish(SizeMode::Allocated),
        errors,
        elapsed: progress.elapsed(),
    }
}

impl ScanSource for FsScanner {
    fn progress(&self) -> Arc<ScanProgress> {
        Arc::clone(&self.progress)
    }

    fn poll(&mut self, _budget: Duration) -> ScanPoll {
        if self.finished {
            return ScanPoll::Running;
        }
        match self.result.try_recv() {
            Ok(result) => {
                self.finished = true;
                ScanPoll::Finished(Box::new(result))
            }
            Err(TryRecvError::Empty) => ScanPoll::Running,
            Err(TryRecvError::Disconnected) => {
                // The worker died without a result (a panic). Report it as a failed scan.
                self.finished = true;
                let message = "The scan stopped unexpectedly.".to_string();
                ScanPoll::Finished(Box::new(failed_result(&self.root, message, &self.progress)))
            }
        }
    }
}

impl Drop for FsScanner {
    fn drop(&mut self) {
        self.progress.cancel();
    }
}

/// Scan `root` on the calling thread (the worker, or a headless `--dump-top`), publishing to
/// `progress` and waking `notify` at most every [`NOTIFY_INTERVAL`]. Returns early, with the
/// partial tree, once `progress` is cancelled.
pub fn scan_blocking(
    root: &Path,
    progress: &Arc<ScanProgress>,
    notify: Option<&UiQueue>,
) -> ScanResult {
    let mut errors = ScanErrors::default();
    let mut builder = TreeBuilder::new(&root.to_string_lossy());
    let walk = build_walk(root, progress);

    // stack[d] is the open directory at depth d.
    let mut stack: Vec<NodeId> = vec![builder.root()];
    let mut last_notify = Instant::now();
    let mut since_check = 0u32;
    for item in walk {
        if progress.is_cancelled() {
            break;
        }
        match item {
            Err(err) => {
                let path = err
                    .path()
                    .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
                errors.record(&path, err.to_string());
                progress.add_error();
            }
            Ok(entry) => {
                let info = &entry.client_state;
                let node = if entry.depth == 0 {
                    let root_id = builder.root();
                    if !entry.file_type.is_dir() && !entry.file_type.is_symlink() {
                        // Scanning a single file: it is the root's only child.
                        let name = entry.file_name.to_string_lossy();
                        builder.add_file(root_id, &name, info.sizes, info.flags);
                        progress.add_file(info.sizes.allocated);
                    }
                    builder.mark(root_id, info.flags);
                    root_id
                } else {
                    stack.truncate(entry.depth);
                    let parent = stack.last().copied().unwrap_or_else(|| builder.root());
                    let name = entry.file_name.to_string_lossy();
                    if entry.file_type.is_dir() {
                        let id = builder.add_dir(parent, &name);
                        builder.mark(id, info.flags);
                        progress.add_dir();
                        stack.push(id);
                        id
                    } else {
                        progress.add_file(info.sizes.allocated);
                        builder.add_file(parent, &name, info.sizes, info.flags)
                    }
                };
                let failure = info
                    .error
                    .clone()
                    .or_else(|| entry.read_children_error.as_ref().map(|e| e.to_string()));
                if let Some(message) = failure {
                    builder.mark(node, NodeFlags::ERROR);
                    errors.record(&entry.path(), message);
                    progress.add_error();
                }
                since_check += 1;
                if since_check >= 256 {
                    since_check = 0;
                    if last_notify.elapsed() >= NOTIFY_INTERVAL {
                        last_notify = Instant::now();
                        progress.set_current_path(&entry.parent_path.to_string_lossy());
                        if let Some(queue) = notify {
                            queue.signal_async_state_change();
                        }
                    }
                }
            }
        }
    }
    progress.mark_done();
    ScanResult {
        root: root.to_path_buf(),
        tree: builder.finish(SizeMode::Allocated),
        errors,
        elapsed: progress.elapsed(),
    }
}

/// The configured jwalk walk over `root`: hidden files included, symlinks not followed, its
/// own rayon pool, and the per-entry filter below.
fn build_walk(root: &Path, progress: &Arc<ScanProgress>) -> Walk {
    let mut devices: Vec<u64> = fs::metadata(root)
        .ok()
        .and_then(|m| fs_meta::device(&m))
        .into_iter()
        .collect();
    if !devices.is_empty() {
        devices.extend(fs_meta::sibling_devices(root));
    }
    let filter = Arc::new(Filter {
        devices,
        skip: fs_meta::duplicate_mounts(root),
        seen_links: Mutex::new(HashSet::new()),
        progress: Arc::clone(progress),
    });
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    Walk::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .parallelism(Parallelism::RayonNewPool(threads))
        .process_read_dir(move |depth, dir, _state, children| {
            filter.process(depth.is_none(), dir, children);
        })
}

impl Filter {
    /// Fill in each entry's [`EntryInfo`] and decide which directories to enter.
    fn process(
        &self,
        is_root: bool,
        dir: &Path,
        children: &mut Vec<jwalk::Result<jwalk::DirEntry<((), EntryInfo)>>>,
    ) {
        if self.progress.is_cancelled() {
            children.clear();
            return;
        }
        for entry in children.iter_mut().flatten() {
            let path = dir.join(&entry.file_name);
            // The root itself is given by the user and may be a symlink to a folder (a
            // macOS `/Volumes/Macintosh HD`); jwalk follows it, so read what it points to.
            let path = if is_root { entry.path() } else { path };
            let meta = if is_root {
                fs::metadata(&path)
            } else {
                fs::symlink_metadata(&path)
            };
            let meta = match meta {
                Ok(meta) => meta,
                Err(err) => {
                    entry.client_state.error = Some(err.to_string());
                    entry.read_children_path = None;
                    continue;
                }
            };
            let info = &mut entry.client_state;
            if meta.file_type().is_symlink() {
                info.flags.insert(NodeFlags::SYMLINK);
                info.sizes = fs_meta::sizes(&path, &meta);
                continue;
            }
            if meta.is_dir() {
                if is_root {
                    continue;
                }
                let other_device = fs_meta::device(&meta)
                    .is_some_and(|dev| !self.devices.is_empty() && !self.devices.contains(&dev));
                if other_device || self.skip.contains(&path) {
                    info.flags.insert(NodeFlags::SKIPPED);
                    entry.read_children_path = None;
                }
                continue;
            }
            if let Some(key) = fs_meta::hard_link_key(&meta) {
                let mut seen = self
                    .seen_links
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if !seen.insert(key) {
                    info.duplicate = true;
                    continue;
                }
            }
            info.sizes = fs_meta::sizes(&path, &meta);
        }
        children.retain(|c| c.as_ref().map_or(true, |e| !e.client_state.duplicate));
    }
}
