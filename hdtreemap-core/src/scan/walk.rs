//! The filesystem walk behind `FsScanner` and `scan_blocking`: jwalk on its own rayon pool,
//! turning every entry into a [`Record`] for an `Assembler` (`scan/assemble.rs`).
//!
//! jwalk reads directories in parallel and calls `process_read_dir` on the pool's threads
//! for each batch of entries; that is where the expensive per-entry work happens (the
//! metadata, the allocated size, the device check that keeps the scan on one filesystem,
//! hard-link de-duplication, skipping duplicate mounts). The walk then yields entries in
//! depth-first order on the calling thread, which makes the records and hands them to a
//! sink: the assembler itself (headless) or a batch sent to the UI thread (live).
//!
//! Progress goes to the shared [`ScanProgress`] as records are made; at most every
//! [`NOTIFY_INTERVAL`] the walk publishes the current path and wakes the UI through its
//! `UiQueue`. Symlinks are never followed; unreadable entries are flagged, counted and
//! listed, never fatal.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agg_gui::ui_thread::UiQueue;
use jwalk::{Parallelism, WalkDirGeneric};
use web_time::Instant;

use super::assemble::{Failure, Record};
use super::fs_meta;
use super::ScanProgress;
use crate::model::{NodeFlags, Sizes};

/// Longest gap between progress publications (current path + UI wake-up).
pub const NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

/// What `process_read_dir` learned about one entry, carried to the walking thread.
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

/// Walk `root` (resolved), handing each entry's [`Record`] to `sink` in depth-first order
/// until the walk ends, `progress` is cancelled, or `sink` returns false. `file_root` is
/// the file's name when `root` is a single file.
pub(super) fn walk(
    root: &Path,
    file_root: Option<&str>,
    progress: &Arc<ScanProgress>,
    notify: Option<&UiQueue>,
    sink: &mut dyn FnMut(Record) -> bool,
) {
    let walk = build_walk(root, progress);
    progress.set_current_path(&root.to_string_lossy());
    let mut last_notify = Instant::now();
    let mut since_check = 0u32;
    for item in walk {
        if progress.is_cancelled() {
            break;
        }
        let record = match item {
            Err(err) => {
                let path = err
                    .path()
                    .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
                progress.add_error();
                Record::WalkError {
                    path,
                    message: err.to_string(),
                }
            }
            Ok(entry) => {
                let record = entry_record(&entry, file_root, progress);
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
                record
            }
        };
        if !sink(record) {
            break;
        }
    }
}

/// The [`Record`] of one walked entry, counted in `progress`.
fn entry_record(
    entry: &jwalk::DirEntry<((), EntryInfo)>,
    file_root: Option<&str>,
    progress: &ScanProgress,
) -> Record {
    let info = &entry.client_state;
    let message = info
        .error
        .clone()
        .or_else(|| entry.read_children_error.as_ref().map(|e| e.to_string()));
    let failure = message.map(|message| {
        progress.add_error();
        Box::new(Failure {
            path: entry.path(),
            message,
        })
    });
    let mut flags = info.flags;
    if entry.file_name.to_str().is_none() {
        flags.insert(NodeFlags::LOSSY_NAME);
    }
    if entry.depth == 0 {
        return match file_root {
            // Scanning a single file: it is the only child of its folder.
            Some(name) if info.error.is_none() => {
                progress.add_file(info.sizes.allocated);
                Record::RootFile {
                    name: name.to_string(),
                    sizes: info.sizes,
                    flags,
                    failure,
                }
            }
            _ => Record::Root {
                flags: info.flags,
                failure,
            },
        };
    }
    let name = entry.file_name.to_string_lossy().into_owned();
    if entry.file_type.is_dir() {
        progress.add_dir();
        Record::Dir {
            depth: entry.depth,
            name,
            flags,
            failure,
        }
    } else {
        progress.add_file(info.sizes.allocated);
        Record::File {
            depth: entry.depth,
            name,
            sizes: info.sizes,
            flags,
            failure,
        }
    }
}

/// The configured jwalk walk over `root`: hidden files included, symlinks not followed, its
/// own rayon pool, and the per-entry filter below.
fn build_walk(root: &Path, progress: &Arc<ScanProgress>) -> Walk {
    let (devices, skip) = match fs_meta::mount_plan(root) {
        // Linux: the mount table names every mount point to skip; no device check.
        Some(skip) => (Vec::new(), skip),
        None => {
            let mut devices: Vec<u64> = fs::metadata(root)
                .ok()
                .and_then(|m| fs_meta::device(&m))
                .into_iter()
                .collect();
            if !devices.is_empty() {
                devices.extend(fs_meta::sibling_devices(root));
            }
            (devices, fs_meta::duplicate_mounts(root))
        }
    };
    let filter = Arc::new(Filter {
        devices,
        skip,
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
